use crate::jobs::{self, VoiceAudioStats};
use crate::paths::AppPaths;
use crate::speakers;
use crate::{EngineError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VoiceReferenceCurationScoreTerm {
    pub key: String,
    pub label: String,
    pub weight: f32,
    pub value: f32,
    pub points: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VoiceReferenceCurationStats {
    pub duration_ms: i64,
    pub sample_rate: u32,
    pub peak_abs: f32,
    pub rms: f32,
    pub clipped_ratio: f32,
    pub silence_ratio: f32,
    pub zero_cross_ratio: f32,
    pub pitch_hz: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VoiceReferenceCurationEntry {
    pub rank: usize,
    pub path: String,
    pub label: String,
    pub score: f32,
    pub warn_count: usize,
    pub fail_count: usize,
    pub recommended_primary: bool,
    pub recommended_compact: bool,
    pub stats: VoiceReferenceCurationStats,
    pub warnings: Vec<String>,
    pub strengths: Vec<String>,
    pub concerns: Vec<String>,
    pub score_breakdown: Vec<VoiceReferenceCurationScoreTerm>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VoiceReferenceCurationReport {
    pub schema_version: u32,
    pub generated_at_ms: i64,
    pub item_id: String,
    pub speaker_key: String,
    pub reference_count: usize,
    pub recommended_primary_path: Option<String>,
    pub recommended_ranked_paths: Vec<String>,
    pub recommended_compact_paths: Vec<String>,
    pub summary: Vec<String>,
    pub json_path: String,
    pub markdown_path: String,
    pub references: Vec<VoiceReferenceCurationEntry>,
}

pub fn generate_reference_curation_report(
    paths: &AppPaths,
    item_id: &str,
    speaker_key: &str,
) -> Result<VoiceReferenceCurationReport> {
    let setting = get_item_speaker_setting(paths, item_id, speaker_key)?;
    let reference_paths = setting.tts_voice_profile_paths.clone();
    if reference_paths.is_empty() {
        return Err(EngineError::InstallFailed(format!(
            "speaker {speaker_key} has no reference clips"
        )));
    }

    let curation_dir = report_dir(paths, item_id);
    std::fs::create_dir_all(&curation_dir)?;
    let temp_dir = curation_dir.join(format!("tmp_{}_{}", sanitize_key(speaker_key), now_ms()));
    std::fs::create_dir_all(&temp_dir)?;

    let mut analyzed: Vec<AnalyzedReference> = Vec::new();
    for (index, raw_path) in reference_paths.iter().enumerate() {
        let path = PathBuf::from(raw_path);
        if !path.exists() {
            analyzed.push(AnalyzedReference {
                path: raw_path.clone(),
                label: file_label(&path),
                stats: VoiceAudioStats::default(),
                warnings: vec![format!(
                    "Reference clip is missing: {}",
                    path.to_string_lossy()
                )],
                warn_count: 0,
                fail_count: 1,
            });
            continue;
        }
        let stats = analyze_reference_audio(paths, &path, &temp_dir, &format!("ref_{index:02}"))?;
        let warnings = jobs::voice_qc_messages(&stats, true, None, Some(speaker_key));
        let warn_count = warnings
            .iter()
            .filter(|(_, severity, _, _)| severity == "warn")
            .count();
        let fail_count = warnings
            .iter()
            .filter(|(_, severity, _, _)| severity == "fail")
            .count();
        analyzed.push(AnalyzedReference {
            path: path.to_string_lossy().to_string(),
            label: file_label(&path),
            stats,
            warnings: warnings
                .into_iter()
                .map(|(_, _, message, _)| message)
                .collect(),
            warn_count,
            fail_count,
        });
    }

    let median_pitch = median_pitch(
        &analyzed
            .iter()
            .filter_map(|entry| entry.stats.pitch_hz)
            .collect::<Vec<_>>(),
    );

    let mut ranked: Vec<VoiceReferenceCurationEntry> = analyzed
        .into_iter()
        .map(|entry| score_reference(entry, median_pitch))
        .collect();
    ranked.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.label.cmp(&b.label))
    });

    let recommended_ranked_paths = ranked
        .iter()
        .map(|entry| entry.path.clone())
        .collect::<Vec<_>>();
    let recommended_compact_paths = recommended_compact_paths(&ranked);
    let recommended_primary_path = ranked.first().map(|entry| entry.path.clone());
    for (index, entry) in ranked.iter_mut().enumerate() {
        entry.rank = index + 1;
        entry.recommended_primary = index == 0;
        entry.recommended_compact = recommended_compact_paths
            .iter()
            .any(|path| path == &entry.path);
    }

    let (json_path, markdown_path) = report_paths(paths, item_id, speaker_key);
    let report = VoiceReferenceCurationReport {
        schema_version: 1,
        generated_at_ms: now_ms(),
        item_id: item_id.to_string(),
        speaker_key: speaker_key.to_string(),
        reference_count: ranked.len(),
        recommended_primary_path,
        recommended_ranked_paths,
        recommended_compact_paths,
        summary: build_summary(&ranked),
        json_path: json_path.to_string_lossy().to_string(),
        markdown_path: markdown_path.to_string_lossy().to_string(),
        references: ranked,
    };
    std::fs::write(
        &json_path,
        format!("{}\n", serde_json::to_string_pretty(&report)?),
    )?;
    std::fs::write(&markdown_path, render_markdown(&report))?;
    let _ = std::fs::remove_dir_all(&temp_dir);
    Ok(report)
}

pub fn load_reference_curation_report(
    paths: &AppPaths,
    item_id: &str,
    speaker_key: &str,
) -> Result<Option<VoiceReferenceCurationReport>> {
    let (json_path, _) = report_paths(paths, item_id, speaker_key);
    if !json_path.exists() {
        return Ok(None);
    }
    Ok(Some(
        serde_json::from_slice::<VoiceReferenceCurationReport>(&std::fs::read(json_path)?)?,
    ))
}

pub fn apply_reference_curation(
    paths: &AppPaths,
    item_id: &str,
    speaker_key: &str,
    mode: &str,
) -> Result<speakers::ItemSpeakerSetting> {
    let normalized_mode = normalize_mode(mode)?;
    let report = match load_reference_curation_report(paths, item_id, speaker_key)? {
        Some(value) => value,
        None => generate_reference_curation_report(paths, item_id, speaker_key)?,
    };
    let next_paths = match normalized_mode {
        "compact" => report.recommended_compact_paths.clone(),
        _ => report.recommended_ranked_paths.clone(),
    };
    if next_paths.is_empty() {
        return Err(EngineError::InstallFailed(format!(
            "reference curation produced no paths for speaker {speaker_key}"
        )));
    }

    let current = get_item_speaker_setting(paths, item_id, speaker_key)?;
    speakers::upsert_item_speaker_setting(
        paths,
        item_id,
        speaker_key,
        current.display_name.clone(),
        current.voice_profile_id.clone(),
        current.tts_voice_id.clone(),
        next_paths.first().cloned(),
        Some(next_paths),
        current.style_preset.clone(),
        current.prosody_preset.clone(),
        current.pronunciation_overrides.clone(),
        current.render_mode.clone(),
        current.subtitle_prosody_mode.clone(),
    )
}

fn get_item_speaker_setting(
    paths: &AppPaths,
    item_id: &str,
    speaker_key: &str,
) -> Result<speakers::ItemSpeakerSetting> {
    speakers::list_item_speaker_settings(paths, item_id)?
        .into_iter()
        .find(|setting| setting.speaker_key == speaker_key)
        .ok_or_else(|| {
            EngineError::InstallFailed(format!(
                "speaker setting not found for item={item_id} speaker={speaker_key}"
            ))
        })
}

fn report_dir(paths: &AppPaths, item_id: &str) -> PathBuf {
    paths
        .derived_item_dir(item_id)
        .join("voice_reference_curation")
}

fn report_paths(paths: &AppPaths, item_id: &str, speaker_key: &str) -> (PathBuf, PathBuf) {
    let stem = format!("voice_reference_curation_v1_{}", sanitize_key(speaker_key));
    let dir = report_dir(paths, item_id);
    (
        dir.join(format!("{stem}.json")),
        dir.join(format!("{stem}.md")),
    )
}

fn sanitize_key(raw: &str) -> String {
    let mut out = String::new();
    let mut prev_underscore = false;
    for ch in raw.trim().chars() {
        let mapped = if ch.is_ascii_alphanumeric() {
            ch.to_ascii_lowercase()
        } else {
            '_'
        };
        if mapped == '_' {
            if prev_underscore {
                continue;
            }
            prev_underscore = true;
        } else {
            prev_underscore = false;
        }
        out.push(mapped);
    }
    let trimmed = out.trim_matches('_').to_string();
    if trimmed.is_empty() {
        "speaker".to_string()
    } else {
        trimmed
    }
}

fn analyze_reference_audio(
    paths: &AppPaths,
    input_path: &Path,
    temp_dir: &Path,
    slug: &str,
) -> Result<VoiceAudioStats> {
    let ext = input_path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase());
    if ext.as_deref() == Some("wav") {
        match jobs::analyze_wav_stats(input_path) {
            Ok(value) => return Ok(value),
            Err(_) => {}
        }
    }
    jobs::analyze_audio_for_qc(paths, input_path, temp_dir, slug)
}

fn score_reference(
    entry: AnalyzedReference,
    median_pitch_hz: Option<f32>,
) -> VoiceReferenceCurationEntry {
    let duration_health = duration_health(entry.stats.duration_ms);
    let level_health = level_health(entry.stats.rms);
    let silence_health = silence_health(entry.stats.silence_ratio);
    let clipping_health = clipping_health(entry.stats.clipped_ratio);
    let noise_health = noise_health(entry.stats.zero_cross_ratio, entry.stats.rms);
    let issue_health = issue_health(entry.warn_count, entry.fail_count);
    let pitch_consistency = pitch_consistency(entry.stats.pitch_hz, median_pitch_hz);
    let terms = vec![
        ScoreTermInput::new("duration", "Duration", 0.20, duration_health),
        ScoreTermInput::new("level", "Level", 0.16, level_health),
        ScoreTermInput::new("silence", "Silence", 0.16, silence_health),
        ScoreTermInput::new("clipping", "Clipping", 0.16, clipping_health),
        ScoreTermInput::new("noise", "Noise", 0.10, noise_health),
        ScoreTermInput::new("issues", "Issue health", 0.12, issue_health),
        ScoreTermInput::new("pitch", "Pitch consistency", 0.10, pitch_consistency),
    ];
    let score_breakdown = terms
        .iter()
        .map(|term| VoiceReferenceCurationScoreTerm {
            key: term.key.to_string(),
            label: term.label.to_string(),
            weight: term.weight,
            value: term.value,
            points: term.weight * term.value * 100.0,
        })
        .collect::<Vec<_>>();
    let score = score_breakdown.iter().map(|term| term.points).sum::<f32>();

    let mut strengths = Vec::new();
    let mut concerns = Vec::new();
    if entry.stats.duration_ms >= 3000 && entry.stats.duration_ms <= 15000 {
        strengths.push("Reference duration sits in the safer cloning range.".to_string());
    }
    if entry.stats.rms >= 0.02 && entry.stats.silence_ratio < 0.65 {
        strengths.push("Reference has usable level and speech density.".to_string());
    }
    if entry.warn_count == 0 && entry.fail_count == 0 {
        strengths.push("No QC warnings were raised for this reference.".to_string());
    }
    if entry.fail_count > 0 {
        concerns.push(format!(
            "{} fail issue(s) make this a risky primary reference.",
            entry.fail_count
        ));
    } else if entry.warn_count > 0 {
        concerns.push(format!(
            "{} warning(s) suggest this reference should not be first choice.",
            entry.warn_count
        ));
    }
    if pitch_consistency < 0.7 {
        concerns.push("Pitch differs from the speaker's median reference profile.".to_string());
    }
    if entry.stats.duration_ms < 2500 {
        concerns.push("Reference is short; 3-10 seconds is safer.".to_string());
    }

    VoiceReferenceCurationEntry {
        rank: 0,
        path: entry.path,
        label: entry.label,
        score,
        warn_count: entry.warn_count,
        fail_count: entry.fail_count,
        recommended_primary: false,
        recommended_compact: false,
        stats: stats_from_audio(entry.stats),
        warnings: entry.warnings,
        strengths,
        concerns,
        score_breakdown,
    }
}

fn recommended_compact_paths(entries: &[VoiceReferenceCurationEntry]) -> Vec<String> {
    let mut selected = Vec::new();
    let mut total_duration = 0i64;
    for entry in entries.iter().filter(|entry| entry.fail_count == 0) {
        selected.push(entry.path.clone());
        total_duration += entry.stats.duration_ms;
        if selected.len() >= 3 && total_duration >= 8_000 {
            break;
        }
        if selected.len() >= 5 {
            break;
        }
    }
    if selected.is_empty() {
        selected.extend(
            entries
                .iter()
                .take(entries.len().min(3))
                .map(|entry| entry.path.clone()),
        );
    } else if selected.len() == 1 && entries.len() > 1 {
        if let Some(next) = entries
            .iter()
            .skip(1)
            .find(|entry| entry.path != selected[0])
        {
            selected.push(next.path.clone());
        }
    }
    selected
}

fn build_summary(entries: &[VoiceReferenceCurationEntry]) -> Vec<String> {
    if entries.is_empty() {
        return vec!["No references were available to curate.".to_string()];
    }
    let top = &entries[0];
    let fail_count = entries.iter().filter(|entry| entry.fail_count > 0).count();
    let warn_count = entries.iter().filter(|entry| entry.warn_count > 0).count();
    let compact_count = recommended_compact_paths(entries).len();
    let mut out = vec![format!(
        "Top reference is {} with a score of {:.1}.",
        top.label, top.score
    )];
    out.push(format!(
        "Recommended compact bundle keeps {compact_count} reference(s) out of {}.",
        entries.len()
    ));
    if fail_count > 0 {
        out.push(format!(
            "{fail_count} reference(s) have fail-level QC issues and should not be primary."
        ));
    } else if warn_count > 0 {
        out.push(format!(
            "{warn_count} reference(s) carry warnings, but none hit fail level."
        ));
    } else {
        out.push("All current references passed QC without warnings.".to_string());
    }
    out
}

fn render_markdown(report: &VoiceReferenceCurationReport) -> String {
    let mut out = String::new();
    out.push_str("# Voice Reference Curation Report\n\n");
    out.push_str(&format!(
        "- Item: `{}`\n- Speaker: `{}`\n- Generated: `{}`\n- References: `{}`\n\n",
        report.item_id, report.speaker_key, report.generated_at_ms, report.reference_count
    ));
    if let Some(primary) = report.recommended_primary_path.as_deref() {
        out.push_str(&format!(
            "- Recommended primary: `{}`\n",
            file_label(Path::new(primary))
        ));
    }
    out.push_str(&format!(
        "- Recommended compact bundle: `{}`\n\n",
        report
            .recommended_compact_paths
            .iter()
            .map(|path| file_label(Path::new(path)))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    if !report.summary.is_empty() {
        out.push_str("## Summary\n\n");
        for line in &report.summary {
            out.push_str(&format!("- {line}\n"));
        }
        out.push('\n');
    }
    out.push_str("## References\n\n");
    for entry in &report.references {
        out.push_str(&format!(
            "### {}. {} ({:.1})\n\n",
            entry.rank, entry.label, entry.score
        ));
        out.push_str(&format!(
            "- Duration: {} ms\n- RMS: {:.3}\n- Silence: {:.1}%\n- Clipping: {:.2}%\n- Noise proxy: {:.3}\n- Warnings: {}\n- Failures: {}\n\n",
            entry.stats.duration_ms,
            entry.stats.rms,
            entry.stats.silence_ratio * 100.0,
            entry.stats.clipped_ratio * 100.0,
            entry.stats.zero_cross_ratio,
            entry.warn_count,
            entry.fail_count
        ));
        if !entry.warnings.is_empty() {
            out.push_str("Warnings:\n");
            for warning in &entry.warnings {
                out.push_str(&format!("- {warning}\n"));
            }
            out.push('\n');
        }
    }
    out
}

fn stats_from_audio(stats: VoiceAudioStats) -> VoiceReferenceCurationStats {
    VoiceReferenceCurationStats {
        duration_ms: stats.duration_ms,
        sample_rate: stats.sample_rate,
        peak_abs: stats.peak_abs,
        rms: stats.rms,
        clipped_ratio: stats.clipped_ratio,
        silence_ratio: stats.silence_ratio,
        zero_cross_ratio: stats.zero_cross_ratio,
        pitch_hz: stats.pitch_hz,
    }
}

fn duration_health(duration_ms: i64) -> f32 {
    if duration_ms <= 0 {
        0.0
    } else if duration_ms < 1_000 {
        0.15
    } else if duration_ms < 2_500 {
        0.45
    } else if duration_ms <= 12_000 {
        1.0
    } else if duration_ms <= 24_000 {
        0.82
    } else {
        0.62
    }
}

fn level_health(rms: f32) -> f32 {
    if rms < 0.008 {
        0.0
    } else if rms < 0.02 {
        0.5
    } else if rms <= 0.16 {
        1.0
    } else if rms <= 0.30 {
        0.82
    } else {
        0.6
    }
}

fn silence_health(silence_ratio: f32) -> f32 {
    clamp01(1.0 - silence_ratio / 0.90)
}

fn clipping_health(clipped_ratio: f32) -> f32 {
    clamp01(1.0 - clipped_ratio / 0.02)
}

fn noise_health(zero_cross_ratio: f32, rms: f32) -> f32 {
    if rms < 0.015 {
        return 0.75;
    }
    if zero_cross_ratio <= 0.10 {
        1.0
    } else if zero_cross_ratio >= 0.25 {
        0.25
    } else {
        clamp01(1.0 - (zero_cross_ratio - 0.10) / 0.15)
    }
}

fn issue_health(warn_count: usize, fail_count: usize) -> f32 {
    clamp01(1.0 - warn_count as f32 * 0.10 - fail_count as f32 * 0.28)
}

fn pitch_consistency(pitch_hz: Option<f32>, median_pitch_hz: Option<f32>) -> f32 {
    let (Some(pitch_hz), Some(median_pitch_hz)) = (pitch_hz, median_pitch_hz) else {
        return 0.75;
    };
    if pitch_hz <= 0.0 || median_pitch_hz <= 0.0 {
        return 0.75;
    }
    let ratio = if pitch_hz > median_pitch_hz {
        pitch_hz / median_pitch_hz
    } else {
        median_pitch_hz / pitch_hz
    };
    if ratio <= 1.12 {
        1.0
    } else if ratio >= 1.70 {
        0.25
    } else {
        clamp01(1.0 - (ratio - 1.12) / 0.58)
    }
}

fn median_pitch(values: &[f32]) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    let mut ordered = values.to_vec();
    ordered.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    Some(ordered[ordered.len() / 2])
}

fn clamp01(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

fn file_label(path: &Path) -> String {
    if let Some(value) = path.file_name().and_then(|value| value.to_str()) {
        value.to_string()
    } else {
        path.to_string_lossy().to_string()
    }
}

fn normalize_mode(raw: &str) -> Result<&'static str> {
    match raw.trim() {
        "" | "ranked" => Ok("ranked"),
        "compact" => Ok("compact"),
        other => Err(EngineError::InstallFailed(format!(
            "unsupported reference curation mode: {other}"
        ))),
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[derive(Debug, Clone)]
struct ScoreTermInput<'a> {
    key: &'a str,
    label: &'a str,
    weight: f32,
    value: f32,
}

impl<'a> ScoreTermInput<'a> {
    fn new(key: &'a str, label: &'a str, weight: f32, value: f32) -> Self {
        Self {
            key,
            label,
            weight,
            value: clamp01(value),
        }
    }
}

#[derive(Debug, Clone)]
struct AnalyzedReference {
    path: String,
    label: String,
    stats: VoiceAudioStats,
    warnings: Vec<String>,
    warn_count: usize,
    fail_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::speakers;
    use rusqlite::params;
    use tempfile::tempdir;

    #[test]
    fn generate_reference_curation_report_ranks_clean_reference_first() {
        let dir = tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        seed_item(&paths, "item-1");
        let good = dir.path().join("good.wav");
        let weak = dir.path().join("weak.wav");
        write_sine_wav(&good, 16_000, 7_000, 0.14);
        write_sine_wav(&weak, 16_000, 1_000, 0.01);

        speakers::upsert_item_speaker_setting(
            &paths,
            "item-1",
            "spk-1",
            Some("Speaker".to_string()),
            None,
            None,
            Some(good.to_string_lossy().to_string()),
            Some(vec![
                good.to_string_lossy().to_string(),
                weak.to_string_lossy().to_string(),
            ]),
            None,
            None,
            None,
            None,
            None,
        )
        .expect("upsert");

        let report = generate_reference_curation_report(&paths, "item-1", "spk-1").expect("report");
        assert_eq!(report.references.len(), 2);
        assert_eq!(
            report.references[0].path,
            good.to_string_lossy().to_string()
        );
        assert_eq!(
            report.recommended_primary_path.as_deref(),
            Some(good.to_string_lossy().as_ref())
        );
        assert!(report
            .recommended_compact_paths
            .contains(&good.to_string_lossy().to_string()));
    }

    #[test]
    fn apply_reference_curation_reorders_speaker_paths() {
        let dir = tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        seed_item(&paths, "item-1");
        let first = dir.path().join("first.wav");
        let second = dir.path().join("second.wav");
        write_sine_wav(&first, 16_000, 900, 0.01);
        write_sine_wav(&second, 16_000, 6_000, 0.12);

        speakers::upsert_item_speaker_setting(
            &paths,
            "item-1",
            "spk-1",
            None,
            Some("profile-1".to_string()),
            None,
            Some(first.to_string_lossy().to_string()),
            Some(vec![
                first.to_string_lossy().to_string(),
                second.to_string_lossy().to_string(),
            ]),
            Some("neutral".to_string()),
            None,
            None,
            None,
            None,
        )
        .expect("upsert");

        let updated =
            apply_reference_curation(&paths, "item-1", "spk-1", "ranked").expect("apply ranked");
        assert_eq!(
            updated.tts_voice_profile_paths[0],
            second.to_string_lossy().to_string()
        );
        assert_eq!(updated.voice_profile_id.as_deref(), Some("profile-1"));
        assert_eq!(updated.style_preset.as_deref(), Some("neutral"));
    }

    fn write_sine_wav(path: &Path, sample_rate: u32, duration_ms: i64, amplitude: f32) {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).expect("create wav");
        let total_samples = ((sample_rate as f64) * (duration_ms as f64) / 1000.0).round() as usize;
        for index in 0..total_samples {
            let t = index as f32 / sample_rate as f32;
            let value = (t * 2.0 * std::f32::consts::PI * 220.0).sin() * amplitude;
            let sample = (value * i16::MAX as f32) as i16;
            writer.write_sample(sample).expect("sample");
        }
        writer.finalize().expect("finalize");
    }

    fn seed_item(paths: &AppPaths, item_id: &str) {
        let conn = db::open(paths).expect("db open");
        db::migrate(&conn).expect("migrate");
        conn.execute(
            "INSERT INTO library_item (
                id, created_at_ms, source_type, source_uri, title, media_path
            ) VALUES (?1, 0, 'local', ?2, ?3, ?4)",
            params![
                item_id,
                format!("file:///{item_id}"),
                item_id,
                format!("{item_id}.mp4")
            ],
        )
        .expect("seed item");
    }
}
