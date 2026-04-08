use crate::jobs::{
    self, QcIssueRecord, TtsPreviewManifestSegment, VoiceCloneRunOutcome, VoiceQcReportSection,
};
use crate::paths::AppPaths;
use crate::subtitle_tracks;
use crate::subtitles::SubtitleDocument;
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VoiceBenchmarkScoreTerm {
    pub key: String,
    pub label: String,
    pub weight: f32,
    pub value: f32,
    pub points: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VoiceBenchmarkCandidate {
    pub candidate_id: String,
    pub display_name: String,
    pub backend_id: String,
    pub variant_label: Option<String>,
    pub manifest_path: String,
    pub expected_segments: usize,
    pub rendered_segments: usize,
    pub coverage_ratio: f32,
    pub timing_fit_ratio: f32,
    pub timing_overrun_segments: usize,
    pub timing_short_segments: usize,
    pub warn_count: usize,
    pub fail_count: usize,
    pub reference_warn_count: usize,
    pub reference_fail_count: usize,
    pub output_warn_count: usize,
    pub output_fail_count: usize,
    pub similarity_proxy: Option<f32>,
    pub converted_ratio: Option<f32>,
    pub voice_clone_outcome: Option<VoiceCloneRunOutcome>,
    pub voice_clone_requested_segments: usize,
    pub voice_clone_converted_segments: usize,
    pub voice_clone_fallback_segments: usize,
    pub voice_clone_standard_tts_segments: usize,
    pub final_mix_ready: bool,
    pub export_pack_ready: bool,
    pub score: f32,
    pub score_breakdown: Vec<VoiceBenchmarkScoreTerm>,
    pub strengths: Vec<String>,
    pub concerns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VoiceBenchmarkReport {
    pub schema_version: u32,
    pub generated_at_ms: i64,
    pub item_id: String,
    pub track_id: String,
    pub goal: String,
    pub recommended_candidate_id: Option<String>,
    pub candidate_count: usize,
    pub summary: Vec<String>,
    pub json_path: String,
    pub markdown_path: String,
    pub candidates: Vec<VoiceBenchmarkCandidate>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VoiceBenchmarkHistoryEntry {
    pub generated_at_ms: i64,
    pub goal: String,
    pub json_path: String,
    pub markdown_path: String,
    pub recommended_candidate_id: Option<String>,
    pub candidate_count: usize,
    pub summary: Vec<String>,
    pub top_candidate_display_name: Option<String>,
    pub top_candidate_backend_id: Option<String>,
    pub top_candidate_variant_label: Option<String>,
    pub top_candidate_score: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VoiceBenchmarkLeaderboardRow {
    pub aggregate_id: String,
    pub display_name: String,
    pub backend_id: String,
    pub variant_label: Option<String>,
    pub appearance_count: usize,
    pub win_count: usize,
    pub latest_generated_at_ms: i64,
    pub latest_score: f32,
    pub best_score: f32,
    pub average_score: f32,
    pub average_coverage_ratio: f32,
    pub average_timing_fit_ratio: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VoiceBenchmarkLeaderboardExport {
    pub schema_version: u32,
    pub generated_at_ms: i64,
    pub item_id: String,
    pub track_id: String,
    pub goal: String,
    pub source_report_count: usize,
    pub latest_report_json_path: Option<String>,
    pub json_path: String,
    pub markdown_path: String,
    pub csv_path: String,
    pub history: Vec<VoiceBenchmarkHistoryEntry>,
    pub rows: Vec<VoiceBenchmarkLeaderboardRow>,
}

#[derive(Debug, Clone, Deserialize)]
struct TtsManifestMeta {
    #[serde(default)]
    backend: Option<String>,
    #[serde(default)]
    track_id: Option<String>,
    #[serde(default)]
    voice_clone_outcome: Option<VoiceCloneRunOutcome>,
    #[serde(default)]
    voice_clone_requested_segments: usize,
    #[serde(default)]
    voice_clone_converted_segments: usize,
    #[serde(default)]
    voice_clone_fallback_segments: usize,
    #[serde(default)]
    voice_clone_standard_tts_segments: usize,
    #[serde(default)]
    segments: Vec<TtsPreviewManifestSegment>,
}

#[derive(Debug, Clone, Deserialize)]
struct VoicePreservingReport {
    #[serde(default)]
    segments_total: usize,
    #[serde(default)]
    segments_base_ok: usize,
    #[serde(default)]
    segments_converted_ok: usize,
}

#[derive(Debug, Clone)]
struct ManifestCandidateSpec {
    candidate_id: String,
    display_name: String,
    backend_id: String,
    variant_label: Option<String>,
    manifest_path: PathBuf,
    segments: Vec<TtsPreviewManifestSegment>,
    durations_by_index: HashMap<u32, i64>,
    converted_ratio: Option<f32>,
    voice_clone_outcome: Option<VoiceCloneRunOutcome>,
    voice_clone_requested_segments: usize,
    voice_clone_converted_segments: usize,
    voice_clone_fallback_segments: usize,
    voice_clone_standard_tts_segments: usize,
    final_mix_ready: bool,
    export_pack_ready: bool,
}

pub fn generate_voice_benchmark_report(
    paths: &AppPaths,
    item_id: &str,
    track_id: &str,
    goal: Option<&str>,
) -> Result<VoiceBenchmarkReport> {
    let track = subtitle_tracks::get_track(paths, track_id)?;
    if track.item_id != item_id {
        return Err(crate::EngineError::InstallFailed(format!(
            "voice benchmark item_id mismatch: params.item_id={item_id} track.item_id={}",
            track.item_id
        )));
    }
    let doc = subtitle_tracks::load_document(paths, track_id)?;
    let goal = normalize_goal(goal);
    let mut candidates =
        build_candidate_reports(paths, item_id, &paths.derived_item_dir(item_id), &doc, track_id)?;
    rank_candidates(&mut candidates, &goal);

    let (json_path, markdown_path) = benchmark_report_paths(paths, item_id, track_id, &goal);
    if let Some(parent) = json_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let report = VoiceBenchmarkReport {
        schema_version: 1,
        generated_at_ms: now_ms(),
        item_id: item_id.to_string(),
        track_id: track_id.to_string(),
        goal,
        recommended_candidate_id: candidates.first().map(|value| value.candidate_id.clone()),
        candidate_count: candidates.len(),
        summary: build_report_summary(&candidates),
        json_path: json_path.to_string_lossy().to_string(),
        markdown_path: markdown_path.to_string_lossy().to_string(),
        candidates,
    };
    let json = serde_json::to_string_pretty(&report)?;
    let markdown = render_markdown(&report);
    std::fs::write(&json_path, format!("{json}\n"))?;
    std::fs::write(&markdown_path, &markdown)?;
    archive_benchmark_snapshot(paths, &report, &json, &markdown)?;
    Ok(report)
}

pub fn load_voice_benchmark_report(
    paths: &AppPaths,
    item_id: &str,
    track_id: &str,
    goal: Option<&str>,
) -> Result<Option<VoiceBenchmarkReport>> {
    let goal = normalize_goal(goal);
    let (json_path, _) = benchmark_report_paths(paths, item_id, track_id, &goal);
    if !json_path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(json_path)?;
    Ok(Some(serde_json::from_slice::<VoiceBenchmarkReport>(&bytes)?))
}

pub fn list_voice_benchmark_history(
    paths: &AppPaths,
    item_id: &str,
    track_id: &str,
    goal: Option<&str>,
) -> Result<Vec<VoiceBenchmarkHistoryEntry>> {
    let goal = normalize_goal(goal);
    let reports = load_benchmark_history_reports(paths, item_id, track_id, &goal)?;
    Ok(reports
        .into_iter()
        .map(history_entry_from_report)
        .collect::<Vec<_>>())
}

pub fn export_voice_benchmark_leaderboard(
    paths: &AppPaths,
    item_id: &str,
    track_id: &str,
    goal: Option<&str>,
) -> Result<VoiceBenchmarkLeaderboardExport> {
    let goal = normalize_goal(goal);
    let reports = load_benchmark_history_reports(paths, item_id, track_id, &goal)?;
    if reports.is_empty() {
        return Err(crate::EngineError::InstallFailed(
            "no voice benchmark history found; generate a benchmark report first".to_string(),
        ));
    }
    let history = reports
        .iter()
        .cloned()
        .map(|report| history_entry_from_report(report))
        .collect::<Vec<_>>();
    let rows = build_leaderboard_rows(&reports);
    let (json_path, markdown_path, csv_path) =
        leaderboard_export_paths(paths, item_id, track_id, &goal);
    if let Some(parent) = json_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let latest_report_json_path = reports
        .first()
        .map(|report| report.json_path.clone())
        .filter(|value| !value.trim().is_empty());
    let export = VoiceBenchmarkLeaderboardExport {
        schema_version: 1,
        generated_at_ms: now_ms(),
        item_id: item_id.to_string(),
        track_id: track_id.to_string(),
        goal,
        source_report_count: history.len(),
        latest_report_json_path,
        json_path: json_path.to_string_lossy().to_string(),
        markdown_path: markdown_path.to_string_lossy().to_string(),
        csv_path: csv_path.to_string_lossy().to_string(),
        history,
        rows,
    };
    let json = serde_json::to_string_pretty(&export)?;
    std::fs::write(&json_path, format!("{json}\n"))?;
    std::fs::write(&markdown_path, render_leaderboard_markdown(&export))?;
    std::fs::write(&csv_path, render_leaderboard_csv(&export))?;
    Ok(export)
}

fn build_candidate_reports(
    paths: &AppPaths,
    item_id: &str,
    item_dir: &Path,
    doc: &SubtitleDocument,
    track_id: &str,
) -> Result<Vec<VoiceBenchmarkCandidate>> {
    let specs = discover_manifest_candidates(paths, item_dir, track_id)?;
    if specs.is_empty() {
        return Err(crate::EngineError::InstallFailed(
            "no voice benchmark candidates found; render at least one TTS or voice-preserving output first"
                .to_string(),
        ));
    }

    let benchmark_dir = item_dir.join("voice_benchmark");
    let temp_root = benchmark_dir.join(format!("tmp_{}", now_ms()));
    std::fs::create_dir_all(&temp_root)?;

    let mut out = Vec::new();
    for spec in specs {
        let candidate_dir = temp_root.join(&spec.candidate_id);
        std::fs::create_dir_all(&candidate_dir)?;
        let (voice_report, voice_issues) =
            jobs::collect_voice_qc(paths, item_id, &spec.segments, &candidate_dir)?;
        out.push(summarize_candidate(doc, spec, voice_report, voice_issues));
    }

    let _ = std::fs::remove_dir_all(&temp_root);
    Ok(out)
}

fn discover_manifest_candidates(
    paths: &AppPaths,
    item_dir: &Path,
    track_id: &str,
) -> Result<Vec<ManifestCandidateSpec>> {
    let tts_root = item_dir.join("tts_preview");
    let mut out = Vec::new();
    if !tts_root.exists() {
        return Ok(out);
    }

    for entry in std::fs::read_dir(&tts_root)?.flatten() {
        let backend_dir = entry.path();
        if !backend_dir.is_dir() {
            continue;
        }
        let Some(backend_name) = backend_dir.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if let Some(spec) = load_manifest_candidate(paths, item_dir, track_id, backend_name, None)? {
            out.push(spec);
        }
        let variants_dir = backend_dir.join("variants");
        if !variants_dir.exists() {
            continue;
        }
        let Ok(variant_entries) = std::fs::read_dir(&variants_dir) else {
            continue;
        };
        for variant_entry in variant_entries.flatten() {
            let variant_path = variant_entry.path();
            if !variant_path.is_dir() {
                continue;
            }
            let Some(label) = variant_path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if let Some(spec) =
                load_manifest_candidate(paths, item_dir, track_id, backend_name, Some(label))?
            {
                out.push(spec);
            }
        }
    }

    out.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    Ok(out)
}

fn load_manifest_candidate(
    paths: &AppPaths,
    item_dir: &Path,
    track_id: &str,
    backend_dir: &str,
    variant_label: Option<&str>,
) -> Result<Option<ManifestCandidateSpec>> {
    let manifest_path = manifest_path(item_dir, backend_dir, variant_label);
    if !manifest_path.exists() {
        return Ok(None);
    }
    let bytes = match std::fs::read(&manifest_path) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    let meta = match serde_json::from_slice::<TtsManifestMeta>(&bytes) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    let Some(meta_track_id) = meta.track_id.as_deref().map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if meta_track_id != track_id {
        return Ok(None);
    }

    let backend_id = meta
        .backend
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(backend_dir)
        .to_string();
    let normalized_variant = normalize_variant_label(variant_label);
    let mut durations_by_index = HashMap::new();
    for segment in &meta.segments {
        if !segment.audio_exists {
            continue;
        }
        let Some(audio_path) = segment
            .audio_path
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
        else {
            continue;
        };
        if !audio_path.exists() {
            continue;
        }
        if let Some(ms) = wav_duration_ms_best_effort(&audio_path) {
            durations_by_index.insert(segment.index, ms);
        } else if let Ok(probe) = crate::ffmpeg::probe(paths, &audio_path) {
            if let Some(ms) = probe.duration_ms {
                durations_by_index.insert(segment.index, ms);
            }
        }
    }

    let display_name = match normalized_variant.as_deref() {
        Some(label) => format!("{} ({label})", backend_display_name(&backend_id)),
        None => backend_display_name(&backend_id).to_string(),
    };
    let variant_dir = tts_variant_dir(item_dir, backend_dir, normalized_variant.as_deref());
    let dub_dir = dub_variant_dir(item_dir, normalized_variant.as_deref());
    let final_mix_ready = dub_dir.join("mix_dub_preview_v1.wav").exists()
        || dub_dir.join("mux_dub_preview_v1.mp4").exists()
        || dub_dir.join("mux_dub_preview_v1.mkv").exists();
    let export_pack_ready = match normalized_variant.as_deref() {
        Some(label) => item_dir
            .join("exports")
            .join(format!("export_pack_v1_{label}.zip"))
            .exists(),
        None => item_dir.join("exports").join("export_pack_v1.zip").exists(),
    };

    Ok(Some(ManifestCandidateSpec {
        candidate_id: candidate_id(&backend_id, normalized_variant.as_deref()),
        display_name,
        backend_id: backend_id.clone(),
        variant_label: normalized_variant.clone(),
        manifest_path,
        segments: meta.segments,
        durations_by_index,
        converted_ratio: load_voice_preserving_ratio(&variant_dir, normalized_variant.as_deref()),
        voice_clone_outcome: meta.voice_clone_outcome,
        voice_clone_requested_segments: meta.voice_clone_requested_segments,
        voice_clone_converted_segments: meta.voice_clone_converted_segments,
        voice_clone_fallback_segments: meta.voice_clone_fallback_segments,
        voice_clone_standard_tts_segments: meta.voice_clone_standard_tts_segments,
        final_mix_ready,
        export_pack_ready,
    }))
}

fn summarize_candidate(
    doc: &SubtitleDocument,
    spec: ManifestCandidateSpec,
    voice_report: VoiceQcReportSection,
    voice_issues: Vec<QcIssueRecord>,
) -> VoiceBenchmarkCandidate {
    let expected_segments = doc
        .segments
        .iter()
        .filter(|segment| !segment.text.trim().is_empty())
        .count();
    let doc_by_index = doc
        .segments
        .iter()
        .map(|segment| (segment.index, segment))
        .collect::<HashMap<_, _>>();

    let rendered_segments = spec
        .segments
        .iter()
        .filter(|segment| {
            segment.audio_exists
                && segment
                    .audio_path
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(PathBuf::from)
                    .map(|path| path.exists())
                    .unwrap_or(false)
        })
        .count();

    let mut timing_fit_segments = 0usize;
    let mut timing_overrun_segments = 0usize;
    let mut timing_short_segments = 0usize;
    for segment in &spec.segments {
        let Some(doc_segment) = doc_by_index.get(&segment.index) else {
            continue;
        };
        let window_ms = (doc_segment.end_ms - doc_segment.start_ms).max(0);
        let Some(tts_ms) = spec.durations_by_index.get(&segment.index).copied() else {
            continue;
        };
        if window_ms > 0 && tts_ms > window_ms + 120 {
            timing_overrun_segments += 1;
        } else if window_ms > 0 && tts_ms < (window_ms / 2).saturating_sub(200) {
            timing_short_segments += 1;
        } else {
            timing_fit_segments += 1;
        }
    }

    let mut warn_count = 0usize;
    let mut fail_count = 0usize;
    let mut reference_warn_count = 0usize;
    let mut reference_fail_count = 0usize;
    let mut output_warn_count = 0usize;
    let mut output_fail_count = 0usize;
    for issue in &voice_issues {
        match issue.severity.as_str() {
            "fail" => fail_count += 1,
            "warn" => warn_count += 1,
            _ => {}
        }
        if issue.kind.starts_with("voice_reference") {
            match issue.severity.as_str() {
                "fail" => reference_fail_count += 1,
                "warn" => reference_warn_count += 1,
                _ => {}
            }
        }
        if issue.kind.starts_with("voice_output")
            || issue.kind == "voice_similarity_weak"
            || issue.kind == "voice_impression_mismatch"
        {
            match issue.severity.as_str() {
                "fail" => output_fail_count += 1,
                "warn" => output_warn_count += 1,
                _ => {}
            }
        }
    }

    let coverage_ratio = ratio(rendered_segments, expected_segments);
    let timing_fit_ratio = ratio(timing_fit_segments, expected_segments);
    let similarity_proxy = similarity_proxy(&voice_report);
    let mut strengths = Vec::new();
    let mut concerns = Vec::new();

    if coverage_ratio >= 0.98 {
        strengths.push("Nearly full segment coverage.".to_string());
    } else if coverage_ratio < 0.85 {
        concerns.push("Coverage is incomplete; some subtitle windows do not have audio.".to_string());
    }
    if timing_fit_ratio >= 0.9 {
        strengths.push("Timing fit stays inside most subtitle windows.".to_string());
    } else {
        if timing_overrun_segments > 0 {
            concerns.push(format!(
                "{timing_overrun_segments} segment(s) overrun their subtitle window."
            ));
        }
        if timing_short_segments > 0 {
            concerns.push(format!(
                "{timing_short_segments} segment(s) are much shorter than their window."
            ));
        }
    }
    if output_fail_count == 0 && output_warn_count <= 2 {
        strengths.push("Output audio QC is clean.".to_string());
    } else if output_fail_count > 0 {
        concerns.push(format!(
            "Output QC found {output_fail_count} fail issue(s) and {output_warn_count} warn issue(s)."
        ));
    }
    if reference_fail_count > 0 {
        concerns.push(format!(
            "Reference QC found {reference_fail_count} fail issue(s); clone stability may be weak."
        ));
    } else if reference_warn_count == 0 {
        strengths.push("Reference set is healthy.".to_string());
    }
    if let Some(value) = similarity_proxy {
        if value >= 0.8 {
            strengths.push("Pitch-based similarity proxy is strong.".to_string());
        } else if value < 0.55 {
            concerns.push("Similarity proxy is weak versus the current references.".to_string());
        }
    } else {
        concerns.push("Similarity proxy is unavailable; not enough voiced material was detected.".to_string());
    }
    if let Some(converted_ratio) = spec.converted_ratio {
        if converted_ratio >= 0.95 {
            strengths.push("Voice-preserving conversion covered nearly all rendered segments.".to_string());
        } else {
            concerns.push(format!(
                "Voice-preserving conversion only covered {:.0}% of rendered segments.",
                converted_ratio * 100.0
            ));
        }
    }
    match spec.voice_clone_outcome {
        Some(VoiceCloneRunOutcome::ClonePreserved) => {
            strengths.push("Clone truth confirmed: all clone-intended segments were converted.".to_string());
        }
        Some(VoiceCloneRunOutcome::PartialFallback) => {
            concerns.push(format!(
                "Clone truth: {} segment(s) converted and {} fell back to plain TTS.",
                spec.voice_clone_converted_segments, spec.voice_clone_fallback_segments
            ));
        }
        Some(VoiceCloneRunOutcome::FallbackOnly) => {
            concerns.push("Clone truth: no clone-intended segments converted; output is plain TTS fallback.".to_string());
        }
        Some(VoiceCloneRunOutcome::StandardTtsOnly) => {
            strengths.push("Current run stayed on standard TTS routing only.".to_string());
        }
        None => {}
    }

    VoiceBenchmarkCandidate {
        candidate_id: spec.candidate_id,
        display_name: spec.display_name,
        backend_id: spec.backend_id,
        variant_label: spec.variant_label,
        manifest_path: spec.manifest_path.to_string_lossy().to_string(),
        expected_segments,
        rendered_segments,
        coverage_ratio,
        timing_fit_ratio,
        timing_overrun_segments,
        timing_short_segments,
        warn_count,
        fail_count,
        reference_warn_count,
        reference_fail_count,
        output_warn_count,
        output_fail_count,
        similarity_proxy,
        converted_ratio: spec.converted_ratio,
        voice_clone_outcome: spec.voice_clone_outcome,
        voice_clone_requested_segments: spec.voice_clone_requested_segments,
        voice_clone_converted_segments: spec.voice_clone_converted_segments,
        voice_clone_fallback_segments: spec.voice_clone_fallback_segments,
        voice_clone_standard_tts_segments: spec.voice_clone_standard_tts_segments,
        final_mix_ready: spec.final_mix_ready,
        export_pack_ready: spec.export_pack_ready,
        score: 0.0,
        score_breakdown: Vec::new(),
        strengths,
        concerns,
    }
}

fn rank_candidates(candidates: &mut [VoiceBenchmarkCandidate], goal: &str) {
    for candidate in candidates.iter_mut() {
        let output_health =
            issue_health(candidate.output_warn_count, candidate.output_fail_count, candidate.rendered_segments.max(1));
        let reference_health =
            issue_health(candidate.reference_warn_count, candidate.reference_fail_count, candidate.rendered_segments.max(1));
        let similarity = candidate.similarity_proxy.unwrap_or(0.35).clamp(0.0, 1.0);
        let availability = if candidate.final_mix_ready {
            1.0
        } else if candidate.rendered_segments > 0 {
            0.75
        } else {
            0.0
        };
        let export_readiness = if candidate.export_pack_ready { 1.0 } else { 0.75 };
        let conversion = if candidate.backend_id == "dub_voice_preserving_v1" {
            Some(candidate.converted_ratio.unwrap_or(0.35).clamp(0.0, 1.0))
        } else {
            None
        };

        let terms = vec![
            ScoreTermInput::new("coverage", "Coverage", weight_for_goal(goal, "coverage"), Some(candidate.coverage_ratio)),
            ScoreTermInput::new("timing_fit", "Timing fit", weight_for_goal(goal, "timing_fit"), Some(candidate.timing_fit_ratio)),
            ScoreTermInput::new("output_health", "Output health", weight_for_goal(goal, "output_health"), Some(output_health)),
            ScoreTermInput::new("reference_health", "Reference health", weight_for_goal(goal, "reference_health"), Some(reference_health)),
            ScoreTermInput::new("similarity", "Similarity proxy", weight_for_goal(goal, "similarity"), Some(similarity)),
            ScoreTermInput::new("conversion", "Voice-preserving coverage", weight_for_goal(goal, "conversion"), conversion),
            ScoreTermInput::new("availability", "Ready preview", weight_for_goal(goal, "availability"), Some(availability)),
            ScoreTermInput::new("export_readiness", "Export readiness", weight_for_goal(goal, "export_readiness"), Some(export_readiness)),
        ];
        let total_weight = terms
            .iter()
            .filter_map(|term| term.value.map(|_| term.weight))
            .sum::<f32>()
            .max(0.0001);

        candidate.score_breakdown = terms
            .into_iter()
            .filter_map(|term| {
                let value = term.value?;
                let weight = term.weight / total_weight;
                let points = weight * value.clamp(0.0, 1.0);
                Some(VoiceBenchmarkScoreTerm {
                    key: term.key.to_string(),
                    label: term.label.to_string(),
                    weight,
                    value,
                    points,
                })
            })
            .collect();
        candidate.score = candidate
            .score_breakdown
            .iter()
            .map(|term| term.points)
            .sum::<f32>()
            * 100.0;
    }

    candidates.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                b.coverage_ratio
                    .partial_cmp(&a.coverage_ratio)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                b.timing_fit_ratio
                    .partial_cmp(&a.timing_fit_ratio)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.display_name.cmp(&b.display_name))
    });
}

fn build_report_summary(candidates: &[VoiceBenchmarkCandidate]) -> Vec<String> {
    let Some(best) = candidates.first() else {
        return vec!["No benchmark candidates were found.".to_string()];
    };
    let mut summary = vec![format!(
        "Top candidate is {} with a {:.1} score.",
        best.display_name, best.score
    )];
    summary.push(format!(
        "Coverage {:.0}%, timing fit {:.0}%, output QC fails {}.",
        best.coverage_ratio * 100.0,
        best.timing_fit_ratio * 100.0,
        best.output_fail_count
    ));
    if let Some(outcome) = &best.voice_clone_outcome {
        summary.push(format!(
            "Clone truth state is {} (converted {}, fallback {}, standard TTS {}).",
            match outcome {
                VoiceCloneRunOutcome::ClonePreserved => "clone preserved",
                VoiceCloneRunOutcome::PartialFallback => "partial fallback",
                VoiceCloneRunOutcome::FallbackOnly => "fallback only",
                VoiceCloneRunOutcome::StandardTtsOnly => "standard TTS only",
            },
            best.voice_clone_converted_segments,
            best.voice_clone_fallback_segments,
            best.voice_clone_standard_tts_segments
        ));
    }
    if candidates.len() > 1 {
        let second = &candidates[1];
        summary.push(format!(
            "Next best is {} at {:.1}, a {:.1}-point gap.",
            second.display_name,
            second.score,
            (best.score - second.score).max(0.0)
        ));
    }
    summary
}

fn render_markdown(report: &VoiceBenchmarkReport) -> String {
    let mut out = String::new();
    out.push_str("# Voice Benchmark Lab\n\n");
    out.push_str(&format!(
        "- Item: `{}`\n- Track: `{}`\n- Goal: `{}`\n- Recommended candidate: `{}`\n- Generated: `{}`\n\n",
        report.item_id,
        report.track_id,
        report.goal,
        report.recommended_candidate_id.as_deref().unwrap_or("-"),
        report.generated_at_ms
    ));
    for line in &report.summary {
        out.push_str(&format!("- {line}\n"));
    }
    out.push_str(
        "\n| Rank | Candidate | Score | Coverage | Timing | Output fails | Similarity | Conversion | Clone truth |\n| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |\n",
    );
    for (index, candidate) in report.candidates.iter().enumerate() {
        let similarity = candidate
            .similarity_proxy
            .map(|value| format!("{:.0}%", value * 100.0))
            .unwrap_or_else(|| "-".to_string());
        let conversion = candidate
            .converted_ratio
            .map(|value| format!("{:.0}%", value * 100.0))
            .unwrap_or_else(|| "-".to_string());
        let clone_truth = candidate
            .voice_clone_outcome
            .as_ref()
            .map(|value| match value {
                VoiceCloneRunOutcome::ClonePreserved => "clone preserved".to_string(),
                VoiceCloneRunOutcome::PartialFallback => format!(
                    "partial fallback ({} converted / {} fallback)",
                    candidate.voice_clone_converted_segments, candidate.voice_clone_fallback_segments
                ),
                VoiceCloneRunOutcome::FallbackOnly => "fallback only".to_string(),
                VoiceCloneRunOutcome::StandardTtsOnly => "standard TTS only".to_string(),
            })
            .unwrap_or_else(|| "-".to_string());
        out.push_str(&format!(
            "| {} | {} | {:.1} | {:.0}% | {:.0}% | {} | {} | {} | {} |\n",
            index + 1,
            candidate.display_name,
            candidate.score,
            candidate.coverage_ratio * 100.0,
            candidate.timing_fit_ratio * 100.0,
            candidate.output_fail_count,
            similarity,
            conversion,
            clone_truth
        ));
    }
    out
}

fn benchmark_report_paths(
    paths: &AppPaths,
    item_id: &str,
    track_id: &str,
    goal: &str,
) -> (PathBuf, PathBuf) {
    let dir = paths.derived_item_dir(item_id).join("voice_benchmark");
    let stem = format!("voice_benchmark_v1_{track_id}_{goal}");
    (dir.join(format!("{stem}.json")), dir.join(format!("{stem}.md")))
}

fn benchmark_history_dir(paths: &AppPaths, item_id: &str, track_id: &str, goal: &str) -> PathBuf {
    paths
        .derived_item_dir(item_id)
        .join("voice_benchmark")
        .join("history")
        .join(format!("{track_id}_{goal}"))
}

fn benchmark_snapshot_paths(
    paths: &AppPaths,
    item_id: &str,
    track_id: &str,
    goal: &str,
    generated_at_ms: i64,
) -> (PathBuf, PathBuf) {
    let dir = benchmark_history_dir(paths, item_id, track_id, goal);
    let stem = format!("voice_benchmark_snapshot_v1_{track_id}_{goal}_{generated_at_ms}");
    (dir.join(format!("{stem}.json")), dir.join(format!("{stem}.md")))
}

fn leaderboard_export_paths(
    paths: &AppPaths,
    item_id: &str,
    track_id: &str,
    goal: &str,
) -> (PathBuf, PathBuf, PathBuf) {
    let dir = paths.derived_item_dir(item_id).join("voice_benchmark");
    let stem = format!("voice_benchmark_leaderboard_v1_{track_id}_{goal}");
    (
        dir.join(format!("{stem}.json")),
        dir.join(format!("{stem}.md")),
        dir.join(format!("{stem}.csv")),
    )
}

fn archive_benchmark_snapshot(
    paths: &AppPaths,
    report: &VoiceBenchmarkReport,
    json: &str,
    markdown: &str,
) -> Result<()> {
    let (json_path, markdown_path) = benchmark_snapshot_paths(
        paths,
        &report.item_id,
        &report.track_id,
        &report.goal,
        report.generated_at_ms,
    );
    if let Some(parent) = json_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&json_path, format!("{json}\n"))?;
    std::fs::write(&markdown_path, markdown)?;
    Ok(())
}

fn load_benchmark_history_reports(
    paths: &AppPaths,
    item_id: &str,
    track_id: &str,
    goal: &str,
) -> Result<Vec<VoiceBenchmarkReport>> {
    let history_dir = benchmark_history_dir(paths, item_id, track_id, goal);
    let mut reports: Vec<VoiceBenchmarkReport> = Vec::new();
    if history_dir.exists() {
        for entry in std::fs::read_dir(&history_dir)?.flatten() {
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let bytes = match std::fs::read(&path) {
                Ok(value) => value,
                Err(_) => continue,
            };
            let report = match serde_json::from_slice::<VoiceBenchmarkReport>(&bytes) {
                Ok(value) => value,
                Err(_) => continue,
            };
            if report.item_id == item_id && report.track_id == track_id && report.goal == goal {
                reports.push(report);
            }
        }
    }
    if reports.is_empty() {
        if let Some(report) = load_voice_benchmark_report(paths, item_id, track_id, Some(goal))? {
            reports.push(report);
        }
    }
    reports.sort_by(|a, b| b.generated_at_ms.cmp(&a.generated_at_ms));
    Ok(reports)
}

fn history_entry_from_report(report: VoiceBenchmarkReport) -> VoiceBenchmarkHistoryEntry {
    let top = report.candidates.first();
    VoiceBenchmarkHistoryEntry {
        generated_at_ms: report.generated_at_ms,
        goal: report.goal,
        json_path: report.json_path,
        markdown_path: report.markdown_path,
        recommended_candidate_id: report.recommended_candidate_id,
        candidate_count: report.candidate_count,
        summary: report.summary,
        top_candidate_display_name: top.map(|value| value.display_name.clone()),
        top_candidate_backend_id: top.map(|value| value.backend_id.clone()),
        top_candidate_variant_label: top.and_then(|value| value.variant_label.clone()),
        top_candidate_score: top.map(|value| value.score),
    }
}

#[derive(Debug, Clone)]
struct LeaderboardAccumulator {
    aggregate_id: String,
    display_name: String,
    backend_id: String,
    variant_label: Option<String>,
    appearance_count: usize,
    win_count: usize,
    latest_generated_at_ms: i64,
    latest_score: f32,
    best_score: f32,
    total_score: f32,
    total_coverage_ratio: f32,
    total_timing_fit_ratio: f32,
}

fn build_leaderboard_rows(reports: &[VoiceBenchmarkReport]) -> Vec<VoiceBenchmarkLeaderboardRow> {
    let mut by_candidate: HashMap<String, LeaderboardAccumulator> = HashMap::new();
    for report in reports {
        let winner_key = report
            .candidates
            .first()
            .map(|candidate| aggregate_candidate_key(&candidate.backend_id, candidate.variant_label.as_deref()));
        for candidate in &report.candidates {
            let key = aggregate_candidate_key(&candidate.backend_id, candidate.variant_label.as_deref());
            let entry = by_candidate.entry(key.clone()).or_insert_with(|| LeaderboardAccumulator {
                aggregate_id: key.clone(),
                display_name: candidate.display_name.clone(),
                backend_id: candidate.backend_id.clone(),
                variant_label: candidate.variant_label.clone(),
                appearance_count: 0,
                win_count: 0,
                latest_generated_at_ms: report.generated_at_ms,
                latest_score: candidate.score,
                best_score: candidate.score,
                total_score: 0.0,
                total_coverage_ratio: 0.0,
                total_timing_fit_ratio: 0.0,
            });
            entry.appearance_count += 1;
            if winner_key.as_deref() == Some(key.as_str()) {
                entry.win_count += 1;
            }
            if report.generated_at_ms >= entry.latest_generated_at_ms {
                entry.latest_generated_at_ms = report.generated_at_ms;
                entry.latest_score = candidate.score;
                entry.display_name = candidate.display_name.clone();
                entry.backend_id = candidate.backend_id.clone();
                entry.variant_label = candidate.variant_label.clone();
            }
            entry.best_score = entry.best_score.max(candidate.score);
            entry.total_score += candidate.score;
            entry.total_coverage_ratio += candidate.coverage_ratio;
            entry.total_timing_fit_ratio += candidate.timing_fit_ratio;
        }
    }

    let mut rows = by_candidate
        .into_values()
        .map(|value| VoiceBenchmarkLeaderboardRow {
            aggregate_id: value.aggregate_id,
            display_name: value.display_name,
            backend_id: value.backend_id,
            variant_label: value.variant_label,
            appearance_count: value.appearance_count,
            win_count: value.win_count,
            latest_generated_at_ms: value.latest_generated_at_ms,
            latest_score: value.latest_score,
            best_score: value.best_score,
            average_score: value.total_score / value.appearance_count.max(1) as f32,
            average_coverage_ratio: value.total_coverage_ratio / value.appearance_count.max(1) as f32,
            average_timing_fit_ratio: value.total_timing_fit_ratio / value.appearance_count.max(1) as f32,
        })
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| {
        b.win_count
            .cmp(&a.win_count)
            .then_with(|| {
                b.latest_score
                    .partial_cmp(&a.latest_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                b.average_score
                    .partial_cmp(&a.average_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.display_name.cmp(&b.display_name))
    });
    rows
}

fn aggregate_candidate_key(backend_id: &str, variant_label: Option<&str>) -> String {
    candidate_id(backend_id, variant_label)
}

fn render_leaderboard_markdown(export: &VoiceBenchmarkLeaderboardExport) -> String {
    let mut out = String::new();
    out.push_str("# Voice Benchmark Leaderboard\n\n");
    out.push_str(&format!(
        "- Item: `{}`\n- Track: `{}`\n- Goal: `{}`\n- Source reports: `{}`\n- Generated: `{}`\n\n",
        export.item_id, export.track_id, export.goal, export.source_report_count, export.generated_at_ms
    ));
    out.push_str(
        "| Rank | Candidate | Wins | Appearances | Latest | Best | Avg | Coverage | Timing |\n| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n",
    );
    for (index, row) in export.rows.iter().enumerate() {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {:.1} | {:.1} | {:.1} | {:.0}% | {:.0}% |\n",
            index + 1,
            row.display_name,
            row.win_count,
            row.appearance_count,
            row.latest_score,
            row.best_score,
            row.average_score,
            row.average_coverage_ratio * 100.0,
            row.average_timing_fit_ratio * 100.0
        ));
    }
    if !export.history.is_empty() {
        out.push_str("\n## Compare History\n\n");
        for entry in &export.history {
            out.push_str(&format!(
                "- `{}` winner: `{}` score `{}` ({})\n",
                entry.generated_at_ms,
                entry.top_candidate_display_name.as_deref().unwrap_or("-"),
                entry
                    .top_candidate_score
                    .map(|value| format!("{value:.1}"))
                    .unwrap_or_else(|| "-".to_string()),
                entry.top_candidate_backend_id.as_deref().unwrap_or("-")
            ));
        }
    }
    out
}

fn render_leaderboard_csv(export: &VoiceBenchmarkLeaderboardExport) -> String {
    let mut out = String::from(
        "rank,aggregate_id,display_name,backend_id,variant_label,wins,appearances,latest_score,best_score,average_score,average_coverage_ratio,average_timing_fit_ratio,latest_generated_at_ms\n",
    );
    for (index, row) in export.rows.iter().enumerate() {
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{:.3},{:.3},{:.3},{:.6},{:.6},{}\n",
            index + 1,
            csv_escape(&row.aggregate_id),
            csv_escape(&row.display_name),
            csv_escape(&row.backend_id),
            csv_escape(row.variant_label.as_deref().unwrap_or("")),
            row.win_count,
            row.appearance_count,
            row.latest_score,
            row.best_score,
            row.average_score,
            row.average_coverage_ratio,
            row.average_timing_fit_ratio,
            row.latest_generated_at_ms
        ));
    }
    out
}

fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn manifest_path(item_dir: &Path, backend_dir: &str, variant_label: Option<&str>) -> PathBuf {
    tts_variant_dir(item_dir, backend_dir, variant_label).join("manifest.json")
}

fn tts_variant_dir(item_dir: &Path, backend_dir: &str, variant_label: Option<&str>) -> PathBuf {
    let mut dir = item_dir.join("tts_preview").join(backend_dir);
    if let Some(label) = normalize_variant_label(variant_label) {
        dir = dir.join("variants").join(label);
    }
    dir
}

fn dub_variant_dir(item_dir: &Path, variant_label: Option<&str>) -> PathBuf {
    let mut dir = item_dir.join("dub_preview");
    if let Some(label) = normalize_variant_label(variant_label) {
        dir = dir.join("alternates").join(label);
    }
    dir
}

fn normalize_variant_label(raw: Option<&str>) -> Option<String> {
    let raw = raw?.trim();
    if raw.is_empty() {
        return None;
    }
    let mut out = String::new();
    let mut prev_underscore = false;
    for ch in raw.chars() {
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
    let out = out.trim_matches('_');
    if out.is_empty() {
        None
    } else {
        Some(out.to_string())
    }
}

fn normalize_goal(raw: Option<&str>) -> String {
    match raw
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .as_deref()
    {
        Some("identity") => "identity".to_string(),
        Some("expressive") => "expressive".to_string(),
        Some("timing") => "timing".to_string(),
        Some("speed") => "speed".to_string(),
        _ => "balanced".to_string(),
    }
}

fn backend_display_name(backend_id: &str) -> &'static str {
    match backend_id {
        "dub_voice_preserving_v1" => "OpenVoice V2 + Kokoro",
        "tts_neural_local_v1" => "Kokoro local",
        "pyttsx3_v1" => "pyttsx3 preview",
        _ => "Voice backend",
    }
}

fn candidate_id(backend_id: &str, variant_label: Option<&str>) -> String {
    match variant_label {
        Some(label) => format!("{backend_id}:{label}"),
        None => backend_id.to_string(),
    }
}

fn load_voice_preserving_ratio(dir: &Path, variant_label: Option<&str>) -> Option<f32> {
    let name = match variant_label {
        Some(label) => format!("tts_voice_preserving_report_{label}.json"),
        None => "tts_voice_preserving_report.json".to_string(),
    };
    let bytes = std::fs::read(dir.join(name)).ok()?;
    let report = serde_json::from_slice::<VoicePreservingReport>(&bytes).ok()?;
    let total = report
        .segments_total
        .max(report.segments_base_ok)
        .max(report.segments_converted_ok);
    if total == 0 {
        return None;
    }
    Some((report.segments_converted_ok as f32 / total as f32).clamp(0.0, 1.0))
}

fn similarity_proxy(report: &VoiceQcReportSection) -> Option<f32> {
    let mut reference_pitch_by_speaker: HashMap<&str, Vec<f32>> = HashMap::new();
    for reference in &report.references {
        let Some(pitch_hz) = reference.stats.pitch_hz else {
            continue;
        };
        reference_pitch_by_speaker
            .entry(reference.speaker_key.as_str())
            .or_default()
            .push(pitch_hz);
    }

    let mut scores = Vec::new();
    for output in &report.outputs {
        let Some(speaker_key) = output.speaker_key.as_deref() else {
            continue;
        };
        let Some(reference_values) = reference_pitch_by_speaker.get(speaker_key) else {
            continue;
        };
        let Some(output_pitch) = output.stats.pitch_hz else {
            continue;
        };
        let reference_pitch = median(reference_values);
        let ratio = if output_pitch > reference_pitch {
            output_pitch / reference_pitch.max(1.0)
        } else {
            reference_pitch / output_pitch.max(1.0)
        };
        scores.push((2.0 - ratio).clamp(0.0, 1.0));
    }

    if scores.is_empty() {
        None
    } else {
        Some(scores.iter().copied().sum::<f32>() / scores.len() as f32)
    }
}

fn issue_health(warn_count: usize, fail_count: usize, denominator: usize) -> f32 {
    let denom = denominator.max(1) as f32;
    let penalty = ((warn_count as f32) * 0.35 + fail_count as f32) / denom;
    (1.0 - penalty.clamp(0.0, 1.0)).clamp(0.0, 1.0)
}

fn weight_for_goal(goal: &str, key: &str) -> f32 {
    match (goal, key) {
        ("identity", "coverage") => 0.14,
        ("identity", "timing_fit") => 0.10,
        ("identity", "output_health") => 0.14,
        ("identity", "reference_health") => 0.18,
        ("identity", "similarity") => 0.32,
        ("identity", "conversion") => 0.08,
        ("identity", "availability") => 0.02,
        ("identity", "export_readiness") => 0.02,
        ("expressive", "coverage") => 0.16,
        ("expressive", "timing_fit") => 0.14,
        ("expressive", "output_health") => 0.24,
        ("expressive", "reference_health") => 0.12,
        ("expressive", "similarity") => 0.18,
        ("expressive", "conversion") => 0.08,
        ("expressive", "availability") => 0.04,
        ("expressive", "export_readiness") => 0.04,
        ("timing", "coverage") => 0.20,
        ("timing", "timing_fit") => 0.34,
        ("timing", "output_health") => 0.14,
        ("timing", "reference_health") => 0.08,
        ("timing", "similarity") => 0.08,
        ("timing", "conversion") => 0.04,
        ("timing", "availability") => 0.06,
        ("timing", "export_readiness") => 0.06,
        ("speed", "coverage") => 0.24,
        ("speed", "timing_fit") => 0.24,
        ("speed", "output_health") => 0.12,
        ("speed", "reference_health") => 0.06,
        ("speed", "similarity") => 0.08,
        ("speed", "conversion") => 0.04,
        ("speed", "availability") => 0.12,
        ("speed", "export_readiness") => 0.10,
        (_, "coverage") => 0.24,
        (_, "timing_fit") => 0.22,
        (_, "output_health") => 0.18,
        (_, "reference_health") => 0.10,
        (_, "similarity") => 0.12,
        (_, "conversion") => 0.06,
        (_, "availability") => 0.04,
        (_, "export_readiness") => 0.04,
        _ => 0.0,
    }
}

fn wav_duration_ms_best_effort(path: &Path) -> Option<i64> {
    let reader = hound::WavReader::open(path).ok()?;
    let spec = reader.spec();
    if spec.sample_rate == 0 {
        return None;
    }
    Some(((reader.duration() as f64 / spec.sample_rate as f64) * 1000.0).round() as i64)
}

fn ratio(numerator: usize, denominator: usize) -> f32 {
    if denominator == 0 {
        return 0.0;
    }
    (numerator as f32 / denominator as f32).clamp(0.0, 1.0)
}

fn median(values: &[f32]) -> f32 {
    let mut ordered = values.to_vec();
    ordered.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    ordered[ordered.len() / 2]
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

struct ScoreTermInput {
    key: &'static str,
    label: &'static str,
    weight: f32,
    value: Option<f32>,
}

impl ScoreTermInput {
    fn new(key: &'static str, label: &'static str, weight: f32, value: Option<f32>) -> Self {
        Self {
            key,
            label,
            weight,
            value,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::subtitles::{SubtitleDocument, SubtitleSegment, SUBTITLE_JSON_SCHEMA_VERSION};
    use rusqlite::params;
    use std::time::Duration;

    #[test]
    fn discover_manifest_candidates_reads_base_and_variant_manifests() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        let item_dir = paths.derived_item_dir("item-1");
        let base_dir = item_dir.join("tts_preview").join("dub_voice_preserving_v1");
        let variant_dir = base_dir.join("variants").join("alt_a");
        std::fs::create_dir_all(&variant_dir).expect("variant dir");

        let audio_path = base_dir.join("seg_0001.wav");
        write_sine_wav(&audio_path, 16_000, 700);
        let manifest = serde_json::json!({
            "backend": "dub_voice_preserving_v1",
            "track_id": "track-1",
            "segments": [{
                "index": 1,
                "start_ms": 0,
                "end_ms": 1000,
                "speaker": "S1",
                "audio_path": audio_path.to_string_lossy().to_string(),
                "audio_exists": true
            }]
        });
        std::fs::write(
            base_dir.join("manifest.json"),
            format!("{}\n", serde_json::to_string_pretty(&manifest).expect("manifest")),
        )
        .expect("write manifest");
        std::fs::write(
            variant_dir.join("manifest.json"),
            format!("{}\n", serde_json::to_string_pretty(&manifest).expect("variant")),
        )
        .expect("write variant manifest");

        let candidates =
            discover_manifest_candidates(&paths, &item_dir, "track-1").expect("discover");
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].backend_id, "dub_voice_preserving_v1");
        assert_eq!(candidates[1].variant_label.as_deref(), Some("alt_a"));
    }

    #[test]
    fn rank_candidates_prefers_identity_similarity() {
        let mut candidates = vec![
            sample_candidate("seed", "dub_voice_preserving_v1", Some(0.92), Some(0.95), 0.80),
            sample_candidate("timing", "tts_neural_local_v1", Some(0.55), None, 0.96),
        ];
        rank_candidates(&mut candidates, "identity");
        assert_eq!(candidates[0].candidate_id, "seed");
        assert!(candidates[0].score > candidates[1].score);
    }

    #[test]
    fn generate_voice_benchmark_report_writes_json_and_markdown() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        if std::process::Command::new(paths.ffmpeg_cmd())
            .arg("-version")
            .output()
            .is_err()
        {
            return;
        }
        db::ensure_schema(&paths).expect("schema");
        seed_item_and_track(&paths);

        let item_dir = paths.derived_item_dir("item-1");
        let base_dir = item_dir.join("tts_preview").join("dub_voice_preserving_v1");
        std::fs::create_dir_all(&base_dir).expect("base dir");
        let audio_path = base_dir.join("seg_0001.wav");
        write_sine_wav(&audio_path, 16_000, 900);
        let manifest = serde_json::json!({
            "backend": "dub_voice_preserving_v1",
            "track_id": "track-1",
            "segments": [{
                "index": 1,
                "start_ms": 0,
                "end_ms": 1200,
                "speaker": "S1",
                "audio_path": audio_path.to_string_lossy().to_string(),
                "audio_exists": true
            }]
        });
        std::fs::write(
            base_dir.join("manifest.json"),
            format!("{}\n", serde_json::to_string_pretty(&manifest).expect("manifest")),
        )
        .expect("write manifest");
        std::fs::write(
            base_dir.join("tts_voice_preserving_report.json"),
            "{\n  \"segments_total\": 1,\n  \"segments_base_ok\": 1,\n  \"segments_converted_ok\": 1\n}\n",
        )
        .expect("write report");

        let reference_path = paths.base_dir.join("refs").join("speaker.wav");
        if let Some(parent) = reference_path.parent() {
            std::fs::create_dir_all(parent).expect("refs dir");
        }
        write_sine_wav(&reference_path, 16_000, 1300);
        crate::speakers::upsert_item_speaker_setting(
            &paths,
            "item-1",
            "S1",
            Some("Speaker 1".to_string()),
            None,
            None,
            Some(reference_path.to_string_lossy().to_string()),
            Some(vec![reference_path.to_string_lossy().to_string()]),
            None,
            None,
            None,
            None,
            None,
        )
        .expect("speaker");

        let report =
            generate_voice_benchmark_report(&paths, "item-1", "track-1", Some("balanced"))
                .expect("report");
        assert_eq!(report.candidate_count, 1);
        assert!(Path::new(&report.json_path).exists());
        assert!(Path::new(&report.markdown_path).exists());
        let history =
            list_voice_benchmark_history(&paths, "item-1", "track-1", Some("balanced"))
                .expect("history");
        assert_eq!(history.len(), 1);
        assert!(Path::new(&history[0].json_path).exists());
        assert!(Path::new(&history[0].markdown_path).exists());
    }

    #[test]
    fn export_voice_benchmark_leaderboard_writes_json_markdown_and_csv() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        if std::process::Command::new(paths.ffmpeg_cmd())
            .arg("-version")
            .output()
            .is_err()
        {
            return;
        }
        db::ensure_schema(&paths).expect("schema");
        seed_item_and_track(&paths);

        let item_dir = paths.derived_item_dir("item-1");
        let base_dir = item_dir.join("tts_preview").join("dub_voice_preserving_v1");
        let variant_dir = base_dir.join("variants").join("alt_a");
        std::fs::create_dir_all(&variant_dir).expect("variant dir");
        let base_audio = base_dir.join("seg_0001.wav");
        let variant_audio = variant_dir.join("seg_0001.wav");
        write_sine_wav(&base_audio, 16_000, 900);
        write_sine_wav(&variant_audio, 16_000, 1100);
        let base_manifest = serde_json::json!({
            "backend": "dub_voice_preserving_v1",
            "track_id": "track-1",
            "segments": [{
                "index": 1,
                "start_ms": 0,
                "end_ms": 1200,
                "speaker": "S1",
                "audio_path": base_audio.to_string_lossy().to_string(),
                "audio_exists": true
            }]
        });
        let variant_manifest = serde_json::json!({
            "backend": "dub_voice_preserving_v1",
            "track_id": "track-1",
            "segments": [{
                "index": 1,
                "start_ms": 0,
                "end_ms": 1200,
                "speaker": "S1",
                "audio_path": variant_audio.to_string_lossy().to_string(),
                "audio_exists": true
            }]
        });
        std::fs::write(
            base_dir.join("manifest.json"),
            format!("{}\n", serde_json::to_string_pretty(&base_manifest).expect("base manifest")),
        )
        .expect("write base manifest");
        std::fs::write(
            variant_dir.join("manifest.json"),
            format!(
                "{}\n",
                serde_json::to_string_pretty(&variant_manifest).expect("variant manifest")
            ),
        )
        .expect("write variant manifest");
        std::fs::write(
            base_dir.join("tts_voice_preserving_report.json"),
            "{\n  \"segments_total\": 1,\n  \"segments_base_ok\": 1,\n  \"segments_converted_ok\": 1\n}\n",
        )
        .expect("write base report");
        std::fs::write(
            variant_dir.join("tts_voice_preserving_report_alt_a.json"),
            "{\n  \"segments_total\": 1,\n  \"segments_base_ok\": 1,\n  \"segments_converted_ok\": 1\n}\n",
        )
        .expect("write variant report");

        let reference_path = paths.base_dir.join("refs").join("speaker.wav");
        if let Some(parent) = reference_path.parent() {
            std::fs::create_dir_all(parent).expect("refs dir");
        }
        write_sine_wav(&reference_path, 16_000, 1300);
        crate::speakers::upsert_item_speaker_setting(
            &paths,
            "item-1",
            "S1",
            Some("Speaker 1".to_string()),
            None,
            None,
            Some(reference_path.to_string_lossy().to_string()),
            Some(vec![reference_path.to_string_lossy().to_string()]),
            None,
            None,
            None,
            None,
            None,
        )
        .expect("speaker");

        let first =
            generate_voice_benchmark_report(&paths, "item-1", "track-1", Some("balanced"))
                .expect("first report");
        std::thread::sleep(Duration::from_millis(2));
        let second =
            generate_voice_benchmark_report(&paths, "item-1", "track-1", Some("balanced"))
                .expect("second report");

        let export =
            export_voice_benchmark_leaderboard(&paths, "item-1", "track-1", Some("balanced"))
                .expect("export");
        assert!(Path::new(&export.json_path).exists());
        assert!(Path::new(&export.markdown_path).exists());
        assert!(Path::new(&export.csv_path).exists());
        assert!(export.source_report_count >= 2);
        assert!(export.rows.len() >= 2);
        assert_eq!(
            export.latest_report_json_path.as_deref(),
            Some(second.json_path.as_str())
        );
        assert!(export
            .history
            .iter()
            .any(|entry| entry.generated_at_ms == first.generated_at_ms));
    }

    fn sample_candidate(
        id: &str,
        backend_id: &str,
        similarity_proxy: Option<f32>,
        converted_ratio: Option<f32>,
        timing_fit_ratio: f32,
    ) -> VoiceBenchmarkCandidate {
        VoiceBenchmarkCandidate {
            candidate_id: id.to_string(),
            display_name: id.to_string(),
            backend_id: backend_id.to_string(),
            variant_label: None,
            manifest_path: id.to_string(),
            expected_segments: 10,
            rendered_segments: 10,
            coverage_ratio: 1.0,
            timing_fit_ratio,
            timing_overrun_segments: 0,
            timing_short_segments: 0,
            warn_count: 2,
            fail_count: 0,
            reference_warn_count: 0,
            reference_fail_count: 0,
            output_warn_count: 2,
            output_fail_count: 0,
            similarity_proxy,
            converted_ratio,
            voice_clone_outcome: None,
            voice_clone_requested_segments: 0,
            voice_clone_converted_segments: 0,
            voice_clone_fallback_segments: 0,
            voice_clone_standard_tts_segments: 0,
            final_mix_ready: true,
            export_pack_ready: true,
            score: 0.0,
            score_breakdown: Vec::new(),
            strengths: Vec::new(),
            concerns: Vec::new(),
        }
    }

    fn seed_item_and_track(paths: &AppPaths) {
        let conn = db::open(paths).expect("open db");
        db::migrate(&conn).expect("migrate");
        conn.execute(
            "INSERT INTO library_item (id, created_at_ms, source_type, source_uri, title, media_path) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params!["item-1", 1_i64, "file", "file://item-1", "Item 1", "D:/media/item1.mp4"],
        )
        .expect("insert item");

        let doc = SubtitleDocument {
            schema_version: SUBTITLE_JSON_SCHEMA_VERSION,
            kind: "translated".to_string(),
            lang: "eng".to_string(),
            segments: vec![SubtitleSegment {
                index: 1,
                start_ms: 0,
                end_ms: 1200,
                text: "Hello world".to_string(),
                speaker: Some("S1".to_string()),
            }],
        };
        let track_path = paths
            .derived_item_dir("item-1")
            .join("translate")
            .join("track.json");
        if let Some(parent) = track_path.parent() {
            std::fs::create_dir_all(parent).expect("track dir");
        }
        std::fs::write(
            &track_path,
            format!("{}\n", serde_json::to_string_pretty(&doc).expect("doc json")),
        )
        .expect("write track");
        conn.execute(
            "INSERT INTO subtitle_track (id, item_id, kind, lang, format, path, created_by, version) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                "track-1",
                "item-1",
                "translated",
                "eng",
                "ytfetch_subtitle_json_v1",
                track_path.to_string_lossy().to_string(),
                "test",
                1_i64
            ],
        )
        .expect("insert track");
    }

    fn write_sine_wav(path: &Path, sample_rate: u32, duration_ms: u32) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("wav dir");
        }
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).expect("wav create");
        let total_samples = ((sample_rate as u64) * (duration_ms as u64) / 1000) as usize;
        for index in 0..total_samples {
            let t = index as f32 / sample_rate as f32;
            let sample = (0.25 * (2.0 * std::f32::consts::PI * 220.0 * t).sin()
                * i16::MAX as f32) as i16;
            writer.write_sample(sample).expect("sample");
        }
        writer.finalize().expect("finalize");
    }
}
