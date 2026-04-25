#[cfg(test)]
use crate::db;
use crate::ffmpeg;
use crate::library;
use crate::paths::AppPaths;
use crate::speakers;
use crate::subtitle_tracks;
use crate::subtitles::SubtitleSegment;
use crate::{jobs, EngineError, Result};
#[cfg(test)]
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const CANDIDATE_SCHEMA_VERSION: u32 = 1;
const TARGET_DURATION_MS: i64 = 8_000;
const MIN_CLIP_DURATION_MS: i64 = 900;
const MAX_CLIP_DURATION_MS: i64 = 6_500;
const MIN_TOTAL_DURATION_MS: i64 = 2_500;
const MAX_CLIPS_PER_SPEAKER: usize = 4;
const SEGMENT_PAD_MS: i64 = 120;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VoiceReferenceCandidateClip {
    pub segment_index: u32,
    pub start_ms: i64,
    pub end_ms: i64,
    pub duration_ms: i64,
    pub text_preview: String,
    pub clip_path: String,
    pub clip_exists: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VoiceReferenceCandidateBundle {
    pub speaker_key: String,
    pub candidate_path: String,
    pub candidate_exists: bool,
    pub json_path: String,
    pub clip_count: usize,
    pub total_duration_ms: i64,
    pub warnings: Vec<String>,
    pub notes: Vec<String>,
    pub clips: Vec<VoiceReferenceCandidateClip>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VoiceReferenceCandidateReport {
    pub schema_version: u32,
    pub generated_at_ms: i64,
    pub item_id: String,
    pub track_id: String,
    pub source_media_path: String,
    pub bundles: Vec<VoiceReferenceCandidateBundle>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VoiceReferenceCandidateGenerationRequest {
    pub item_id: String,
    #[serde(default)]
    pub track_id: Option<String>,
    #[serde(default)]
    pub speaker_key: Option<String>,
    #[serde(default)]
    pub missing_only: bool,
}

pub fn generate_reference_candidates(
    paths: &AppPaths,
    request: VoiceReferenceCandidateGenerationRequest,
) -> Result<VoiceReferenceCandidateReport> {
    let item_id = request.item_id.trim();
    if item_id.is_empty() {
        return Err(EngineError::InstallFailed("item_id is empty".to_string()));
    }
    let item = library::get_item_by_id(paths, item_id)?;
    let media_path = PathBuf::from(item.media_path.trim());
    if !media_path.exists() {
        return Err(EngineError::InstallFailed(format!(
            "source media path does not exist: {}",
            media_path.to_string_lossy()
        )));
    }

    let track = select_track_with_speakers(paths, item_id, request.track_id.as_deref())?;
    let doc = subtitle_tracks::load_document(paths, &track.id)?;
    let speaker_settings = speakers::list_item_speaker_settings(paths, item_id)?;

    let mut target_speakers = doc
        .segments
        .iter()
        .filter_map(|segment| segment.speaker.as_deref())
        .map(|speaker| speaker.trim().to_string())
        .filter(|speaker| !speaker.is_empty())
        .collect::<Vec<_>>();
    target_speakers.sort();
    target_speakers.dedup();

    if let Some(speaker_key) = request
        .speaker_key
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        target_speakers.retain(|speaker| speaker == &speaker_key);
    }

    if request.missing_only {
        target_speakers.retain(|speaker| {
            speaker_settings
                .iter()
                .find(|setting| setting.speaker_key == *speaker)
                .map(|setting| setting.tts_voice_profile_paths.is_empty())
                .unwrap_or(true)
        });
    }

    let mut bundles = Vec::new();
    for speaker_key in target_speakers {
        let bundle = generate_bundle_for_speaker(
            paths,
            item_id,
            &track.id,
            &media_path,
            &doc.segments,
            &speaker_key,
        )?;
        bundles.push(bundle);
    }
    bundles.sort_by(|a, b| a.speaker_key.cmp(&b.speaker_key));

    Ok(VoiceReferenceCandidateReport {
        schema_version: CANDIDATE_SCHEMA_VERSION,
        generated_at_ms: now_ms(),
        item_id: item_id.to_string(),
        track_id: track.id,
        source_media_path: media_path.to_string_lossy().to_string(),
        bundles,
    })
}

pub fn load_reference_candidates(
    paths: &AppPaths,
    item_id: &str,
    speaker_key: Option<&str>,
) -> Result<Option<VoiceReferenceCandidateReport>> {
    let item_id = item_id.trim();
    if item_id.is_empty() {
        return Err(EngineError::InstallFailed("item_id is empty".to_string()));
    }
    let root = candidates_root(paths, item_id);
    if !root.exists() {
        return Ok(None);
    }

    let filter_speaker = speaker_key
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let mut bundles: Vec<VoiceReferenceCandidateBundle> = Vec::new();
    let mut generated_at_ms = 0_i64;
    let mut track_id = String::new();
    let mut source_media_path = String::new();
    for entry in std::fs::read_dir(&root)? {
        let entry = entry?;
        let json_path = entry.path().join("candidate_bundle_v1.json");
        if !json_path.exists() {
            continue;
        }
        let stored: StoredVoiceReferenceCandidateBundle =
            serde_json::from_slice(&std::fs::read(&json_path)?)?;
        if let Some(filter_speaker) = filter_speaker.as_deref() {
            if stored.speaker_key != filter_speaker {
                continue;
            }
        }
        generated_at_ms = generated_at_ms.max(stored.generated_at_ms);
        if track_id.is_empty() {
            track_id = stored.track_id.clone();
        }
        if source_media_path.is_empty() {
            source_media_path = stored.source_media_path.clone();
        }
        bundles.push(stored.bundle);
    }

    if bundles.is_empty() {
        return Ok(None);
    }
    bundles.sort_by(|a, b| a.speaker_key.cmp(&b.speaker_key));
    Ok(Some(VoiceReferenceCandidateReport {
        schema_version: CANDIDATE_SCHEMA_VERSION,
        generated_at_ms,
        item_id: item_id.to_string(),
        track_id,
        source_media_path,
        bundles,
    }))
}

pub fn apply_reference_candidate(
    paths: &AppPaths,
    item_id: &str,
    speaker_key: &str,
    mode: &str,
) -> Result<speakers::ItemSpeakerSetting> {
    let item_id = item_id.trim();
    let speaker_key = speaker_key.trim();
    if item_id.is_empty() || speaker_key.is_empty() {
        return Err(EngineError::InstallFailed(
            "item_id or speaker_key is empty".to_string(),
        ));
    }
    let mode = normalize_apply_mode(mode)?;
    let report = load_reference_candidates(paths, item_id, Some(speaker_key))?
        .and_then(|value| {
            value
                .bundles
                .into_iter()
                .find(|bundle| bundle.speaker_key == speaker_key)
        })
        .ok_or_else(|| {
            EngineError::InstallFailed(format!(
                "no generated reference candidate found for speaker {speaker_key}"
            ))
        })?;
    if !report.candidate_exists {
        return Err(EngineError::InstallFailed(format!(
            "generated reference candidate is missing for speaker {speaker_key}"
        )));
    }
    let current = speakers::list_item_speaker_settings(paths, item_id)?
        .into_iter()
        .find(|setting| setting.speaker_key == speaker_key);
    let next_paths = match mode {
        "replace" => vec![report.candidate_path.clone()],
        _ => unique_paths(
            std::iter::once(report.candidate_path.clone()).chain(
                current
                    .iter()
                    .flat_map(|setting| setting.tts_voice_profile_paths.clone()),
            ),
        ),
    };
    speakers::upsert_item_speaker_setting(
        paths,
        item_id,
        speaker_key,
        current
            .as_ref()
            .and_then(|setting| setting.display_name.clone()),
        None,
        current
            .as_ref()
            .and_then(|setting| setting.tts_voice_id.clone()),
        next_paths.first().cloned(),
        Some(next_paths),
        current
            .as_ref()
            .and_then(|setting| setting.style_preset.clone()),
        current
            .as_ref()
            .and_then(|setting| setting.prosody_preset.clone()),
        current
            .as_ref()
            .and_then(|setting| setting.pronunciation_overrides.clone()),
        current
            .as_ref()
            .and_then(|setting| setting.render_mode.clone())
            .or_else(|| Some("clone".to_string())),
        current
            .as_ref()
            .and_then(|setting| setting.subtitle_prosody_mode.clone()),
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredVoiceReferenceCandidateBundle {
    generated_at_ms: i64,
    item_id: String,
    track_id: String,
    speaker_key: String,
    source_media_path: String,
    bundle: VoiceReferenceCandidateBundle,
}

fn generate_bundle_for_speaker(
    paths: &AppPaths,
    item_id: &str,
    track_id: &str,
    media_path: &Path,
    segments: &[SubtitleSegment],
    speaker_key: &str,
) -> Result<VoiceReferenceCandidateBundle> {
    let selected_segments = choose_segments_for_speaker(segments, speaker_key);
    if selected_segments.is_empty() {
        return Err(EngineError::InstallFailed(format!(
            "no usable subtitle segments found for speaker {speaker_key}"
        )));
    }

    let speaker_root = candidate_speaker_root(paths, item_id, speaker_key);
    let clips_dir = speaker_root.join("clips");
    std::fs::create_dir_all(&clips_dir)?;
    let temp_dir = speaker_root.join(format!("tmp_{}", now_ms()));
    std::fs::create_dir_all(&temp_dir)?;

    let mut clips: Vec<VoiceReferenceCandidateClip> = Vec::new();
    let mut clip_paths: Vec<PathBuf> = Vec::new();
    for (position, segment) in selected_segments.iter().enumerate() {
        let clip_path = clips_dir.join(format!(
            "seg_{:02}_{:06}_{:06}.wav",
            position + 1,
            segment.start_ms.max(0),
            segment.end_ms.max(0)
        ));
        ffmpeg::extract_audio_clip_wav_16k_mono(
            paths,
            media_path,
            &clip_path,
            segment.start_ms,
            segment.end_ms,
        )?;
        clip_paths.push(clip_path.clone());
        clips.push(VoiceReferenceCandidateClip {
            segment_index: segment.index,
            start_ms: segment.start_ms,
            end_ms: segment.end_ms,
            duration_ms: (segment.end_ms - segment.start_ms).max(0),
            text_preview: trim_text_preview(&segment.text),
            clip_path: clip_path.to_string_lossy().to_string(),
            clip_exists: clip_path.exists(),
        });
    }

    let candidate_path = speaker_root.join("candidate_bundle_v1.wav");
    ffmpeg::concat_wav_files_16k_mono(paths, &clip_paths, &candidate_path)?;
    let candidate_stats =
        jobs::analyze_audio_for_qc(paths, &candidate_path, &temp_dir, "candidate")?;
    let warnings = jobs::voice_qc_messages(&candidate_stats, true, None, Some(speaker_key))
        .into_iter()
        .map(|(_, _, message, _)| message)
        .collect::<Vec<_>>();

    let total_duration_ms = clips.iter().map(|clip| clip.duration_ms).sum::<i64>();
    let notes = vec![
        format!(
            "Built from {} subtitle-aligned source segment(s) for {}.",
            clips.len(),
            speaker_key
        ),
        format!(
            "Total bundled duration: {} ms. Target range is {}-{} ms.",
            total_duration_ms, MIN_TOTAL_DURATION_MS, TARGET_DURATION_MS
        ),
    ];

    let bundle = VoiceReferenceCandidateBundle {
        speaker_key: speaker_key.to_string(),
        candidate_path: candidate_path.to_string_lossy().to_string(),
        candidate_exists: candidate_path.exists(),
        json_path: speaker_root
            .join("candidate_bundle_v1.json")
            .to_string_lossy()
            .to_string(),
        clip_count: clips.len(),
        total_duration_ms,
        warnings,
        notes,
        clips,
    };
    let stored = StoredVoiceReferenceCandidateBundle {
        generated_at_ms: now_ms(),
        item_id: item_id.to_string(),
        track_id: track_id.to_string(),
        speaker_key: speaker_key.to_string(),
        source_media_path: media_path.to_string_lossy().to_string(),
        bundle: bundle.clone(),
    };
    std::fs::write(
        speaker_root.join("candidate_bundle_v1.json"),
        format!("{}\n", serde_json::to_string_pretty(&stored)?),
    )?;
    let _ = std::fs::remove_dir_all(&temp_dir);
    Ok(bundle)
}

fn select_track_with_speakers(
    paths: &AppPaths,
    item_id: &str,
    requested_track_id: Option<&str>,
) -> Result<subtitle_tracks::SubtitleTrackRow> {
    let tracks = subtitle_tracks::list_tracks(paths, item_id)?;
    if let Some(track_id) = requested_track_id
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        if let Some(track) = tracks.iter().find(|track| track.id == track_id) {
            let doc = subtitle_tracks::load_document(paths, &track.id)?;
            if doc.segments.iter().any(|segment| {
                segment
                    .speaker
                    .as_deref()
                    .map(|value| !value.trim().is_empty())
                    .unwrap_or(false)
            }) {
                return Ok(track.clone());
            }
        }
    }
    tracks
        .into_iter()
        .filter(|track| track.kind == "translated" && track.lang == "en")
        .filter_map(|track| {
            subtitle_tracks::load_document(paths, &track.id)
                .ok()
                .filter(|doc| {
                    doc.segments.iter().any(|segment| {
                        segment
                            .speaker
                            .as_deref()
                            .map(|value| !value.trim().is_empty())
                            .unwrap_or(false)
                    })
                })
                .map(|_| track)
        })
        .max_by_key(|track| track.version)
        .ok_or_else(|| {
            EngineError::InstallFailed(
                "no translated English track with speaker labels is available".to_string(),
            )
        })
}

fn choose_segments_for_speaker<'a>(
    segments: &'a [SubtitleSegment],
    speaker_key: &str,
) -> Vec<SubtitleSegment> {
    let mut candidates = segments
        .iter()
        .filter(|segment| segment.speaker.as_deref().map(|value| value.trim()) == Some(speaker_key))
        .filter_map(|segment| prepare_candidate_segment(segment))
        .collect::<Vec<_>>();
    candidates.sort_by(|a, b| {
        let a_duration = a.end_ms - a.start_ms;
        let b_duration = b.end_ms - b.start_ms;
        b_duration
            .cmp(&a_duration)
            .then_with(|| a.index.cmp(&b.index))
    });

    let mut selected: Vec<SubtitleSegment> = Vec::new();
    let mut total_duration_ms = 0_i64;
    for segment in candidates {
        if selected.len() >= MAX_CLIPS_PER_SPEAKER {
            break;
        }
        total_duration_ms += (segment.end_ms - segment.start_ms).max(0);
        selected.push(segment);
        if total_duration_ms >= TARGET_DURATION_MS {
            break;
        }
    }
    selected.sort_by_key(|segment| segment.start_ms);
    selected
}

fn prepare_candidate_segment(segment: &SubtitleSegment) -> Option<SubtitleSegment> {
    let text = segment.text.trim();
    if text.is_empty() {
        return None;
    }
    let start_ms = (segment.start_ms - SEGMENT_PAD_MS).max(0);
    let end_ms = (segment.end_ms + SEGMENT_PAD_MS).max(start_ms + 1);
    let duration_ms = end_ms - start_ms;
    if duration_ms < MIN_CLIP_DURATION_MS {
        return None;
    }
    let capped_end_ms = if duration_ms > MAX_CLIP_DURATION_MS {
        start_ms + MAX_CLIP_DURATION_MS
    } else {
        end_ms
    };
    Some(SubtitleSegment {
        index: segment.index,
        start_ms,
        end_ms: capped_end_ms,
        text: text.to_string(),
        speaker: segment.speaker.clone(),
    })
}

fn candidates_root(paths: &AppPaths, item_id: &str) -> PathBuf {
    paths
        .derived_item_voice_dir(item_id)
        .join("reference_candidates")
}

fn candidate_speaker_root(paths: &AppPaths, item_id: &str, speaker_key: &str) -> PathBuf {
    let slug = sanitize_segment(speaker_key);
    let hash = stable_key_hash(speaker_key);
    candidates_root(paths, item_id).join(format!("{slug}__{hash}"))
}

fn sanitize_segment(raw: &str) -> String {
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
    let out = out.trim_matches('_');
    if out.is_empty() {
        "speaker".to_string()
    } else {
        out.to_string()
    }
}

fn stable_key_hash(raw: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in raw.trim().as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn trim_text_preview(raw: &str) -> String {
    let text = raw.replace('\n', " ").replace('\r', " ").trim().to_string();
    if text.len() <= 90 {
        text
    } else {
        format!("{}...", &text[..87])
    }
}

fn unique_paths<I>(values: I) -> Vec<String>
where
    I: IntoIterator<Item = String>,
{
    let mut out = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() || out.iter().any(|existing| existing == trimmed) {
            continue;
        }
        out.push(trimmed.to_string());
    }
    out
}

fn normalize_apply_mode(raw: &str) -> Result<&str> {
    let mode = raw.trim().to_ascii_lowercase();
    match mode.as_str() {
        "replace" => Ok("replace"),
        "" | "append" => Ok("append"),
        _ => Err(EngineError::InstallFailed(format!(
            "unsupported candidate apply mode: {raw}"
        ))),
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::AppPaths;
    use crate::subtitles::{SubtitleDocument, SubtitleSegment, SUBTITLE_JSON_SCHEMA_VERSION};
    use hound::{SampleFormat, WavSpec, WavWriter};
    use std::f32::consts::PI;

    fn seed_item(paths: &AppPaths, item_id: &str, media_path: &Path) {
        let conn = db::open(paths).expect("open db");
        db::migrate(&conn).expect("migrate");
        conn.execute(
            "INSERT INTO library_item (id, created_at_ms, source_type, source_uri, title, media_path) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                item_id,
                1_i64,
                "file",
                format!("file://{item_id}"),
                "Item 1",
                media_path.to_string_lossy().to_string()
            ],
        )
        .expect("insert item");
    }

    fn seed_track(paths: &AppPaths, item_id: &str, track_id: &str) {
        let doc = SubtitleDocument {
            schema_version: SUBTITLE_JSON_SCHEMA_VERSION,
            kind: "translated".to_string(),
            lang: "en".to_string(),
            segments: vec![
                SubtitleSegment {
                    index: 1,
                    start_ms: 0,
                    end_ms: 2200,
                    text: "First speaker sentence".to_string(),
                    speaker: Some("S1".to_string()),
                },
                SubtitleSegment {
                    index: 2,
                    start_ms: 2400,
                    end_ms: 4300,
                    text: "Second speaker sentence".to_string(),
                    speaker: Some("S2".to_string()),
                },
                SubtitleSegment {
                    index: 3,
                    start_ms: 4500,
                    end_ms: 6700,
                    text: "First speaker follow up".to_string(),
                    speaker: Some("S1".to_string()),
                },
            ],
        };
        let track_path = paths
            .derived_item_dir(item_id)
            .join("translated")
            .join(format!("{track_id}.json"));
        std::fs::create_dir_all(track_path.parent().expect("track dir")).expect("track dir");
        std::fs::write(
            &track_path,
            format!(
                "{}\n",
                serde_json::to_string_pretty(&doc).expect("doc json")
            ),
        )
        .expect("write track");
        let conn = db::open(paths).expect("open db");
        db::migrate(&conn).expect("migrate");
        conn.execute(
            "INSERT INTO subtitle_track (id, item_id, kind, lang, format, path, created_by, version) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                track_id,
                item_id,
                "translated",
                "en",
                "ytfetch_subtitle_json_v1",
                track_path.to_string_lossy().to_string(),
                "test",
                1_i64
            ],
        )
        .expect("insert track");
    }

    fn write_test_wav(path: &Path, duration_ms: u32) {
        std::fs::create_dir_all(path.parent().expect("wav dir")).expect("wav dir");
        let spec = WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };
        let mut writer = WavWriter::create(path, spec).expect("wav create");
        let total_samples = ((spec.sample_rate as u64) * (duration_ms as u64) / 1000) as usize;
        for index in 0..total_samples {
            let t = index as f32 / spec.sample_rate as f32;
            let sample = (0.20 * (2.0 * PI * 220.0 * t).sin() * i16::MAX as f32) as i16;
            writer.write_sample(sample).expect("sample");
        }
        writer.finalize().expect("finalize");
    }

    fn ffmpeg_available() -> bool {
        std::process::Command::new("ffmpeg")
            .arg("-version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    #[test]
    fn generate_reference_candidates_creates_bundle_files() {
        if !ffmpeg_available() {
            return;
        }
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        let media_path = dir.path().join("media").join("source.wav");
        write_test_wav(&media_path, 8_000);
        seed_item(&paths, "item-1", &media_path);
        seed_track(&paths, "item-1", "track-en");

        let report = generate_reference_candidates(
            &paths,
            VoiceReferenceCandidateGenerationRequest {
                item_id: "item-1".to_string(),
                track_id: Some("track-en".to_string()),
                speaker_key: None,
                missing_only: false,
            },
        )
        .expect("generate");

        assert_eq!(report.bundles.len(), 2);
        assert!(report.bundles.iter().all(|bundle| bundle.candidate_exists));
        assert!(report
            .bundles
            .iter()
            .all(|bundle| PathBuf::from(&bundle.candidate_path).exists()));
    }

    #[test]
    fn apply_reference_candidate_append_keeps_existing_paths() {
        if !ffmpeg_available() {
            return;
        }
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        let media_path = dir.path().join("media").join("source.wav");
        let existing_ref = dir.path().join("refs").join("manual.wav");
        write_test_wav(&media_path, 8_000);
        write_test_wav(&existing_ref, 3_000);
        seed_item(&paths, "item-1", &media_path);
        seed_track(&paths, "item-1", "track-en");
        speakers::upsert_item_speaker_setting(
            &paths,
            "item-1",
            "S1",
            None,
            None,
            None,
            Some(existing_ref.to_string_lossy().to_string()),
            Some(vec![existing_ref.to_string_lossy().to_string()]),
            None,
            None,
            None,
            Some("clone".to_string()),
            None,
        )
        .expect("speaker");
        let _ = generate_reference_candidates(
            &paths,
            VoiceReferenceCandidateGenerationRequest {
                item_id: "item-1".to_string(),
                track_id: Some("track-en".to_string()),
                speaker_key: Some("S1".to_string()),
                missing_only: false,
            },
        )
        .expect("generate");

        let updated = apply_reference_candidate(&paths, "item-1", "S1", "append").expect("apply");
        assert!(updated.tts_voice_profile_paths.len() >= 2);
        assert_eq!(
            updated.tts_voice_profile_paths[1],
            existing_ref.to_string_lossy().to_string()
        );
        assert!(PathBuf::from(&updated.tts_voice_profile_paths[0]).exists());
        assert_eq!(updated.voice_profile_id, None);
    }
}
