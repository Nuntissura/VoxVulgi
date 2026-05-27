use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use hound::{SampleFormat, WavReader};
use serde::Serialize;
use voxvulgi_engine::models::ModelStore;
use voxvulgi_engine::paths::AppPaths;
use voxvulgi_engine::{
    db, jobs, library, subtitle_tracks, tools, voice_reference_candidates, EngineError, Result,
};

fn repo_root_from_engine_dir(engine_dir: &Path) -> PathBuf {
    engine_dir.join("..").join("..")
}

fn wait_for_job(paths: &AppPaths, job_id: &str, timeout: Duration) -> Result<jobs::JobRow> {
    let start = Instant::now();
    loop {
        let rows = jobs::list_jobs(paths, 500, 0)?;
        if let Some(job) = rows.into_iter().find(|value| value.id == job_id) {
            match job.status {
                jobs::JobStatus::Succeeded => return Ok(job),
                jobs::JobStatus::Failed => {
                    return Err(EngineError::InstallFailed(format!(
                        "job {job_id} failed: {}",
                        job.error.unwrap_or_else(|| "(no error)".to_string())
                    )))
                }
                jobs::JobStatus::Canceled => {
                    return Err(EngineError::InstallFailed(format!("job {job_id} canceled")))
                }
                jobs::JobStatus::Queued | jobs::JobStatus::Running => {}
            }
        }

        if start.elapsed() > timeout {
            return Err(EngineError::InstallFailed(format!(
                "timeout waiting for job {job_id}"
            )));
        }
        thread::sleep(Duration::from_millis(500));
    }
}

fn wait_for_batch_to_idle(
    paths: &AppPaths,
    batch_id: &str,
    timeout: Duration,
) -> Result<Vec<jobs::JobRow>> {
    let start = Instant::now();
    loop {
        let rows = jobs::list_jobs(paths, 1000, 0)?
            .into_iter()
            .filter(|job| job.batch_id.as_deref() == Some(batch_id))
            .collect::<Vec<_>>();
        if rows
            .iter()
            .any(|job| matches!(job.status, jobs::JobStatus::Failed))
        {
            let failed = rows
                .iter()
                .find(|job| matches!(job.status, jobs::JobStatus::Failed))
                .expect("failed job");
            return Err(EngineError::InstallFailed(format!(
                "batch {batch_id} failed at {}: {}",
                failed.job_type,
                failed
                    .error
                    .clone()
                    .unwrap_or_else(|| "(no error)".to_string())
            )));
        }
        if !rows.is_empty()
            && rows.iter().all(|job| {
                matches!(
                    job.status,
                    jobs::JobStatus::Succeeded | jobs::JobStatus::Canceled
                )
            })
        {
            return Ok(rows);
        }
        if start.elapsed() > timeout {
            return Err(EngineError::InstallFailed(format!(
                "timeout waiting for batch {batch_id}"
            )));
        }
        thread::sleep(Duration::from_millis(700));
    }
}

fn item_jobs(paths: &AppPaths, item_id: &str) -> Result<Vec<jobs::JobRow>> {
    let mut rows = jobs::list_jobs(paths, 1000, 0)?
        .into_iter()
        .filter(|job| job.item_id.as_deref() == Some(item_id))
        .collect::<Vec<_>>();
    rows.sort_by_key(|job| job.created_at_ms);
    Ok(rows)
}

fn wait_for_item_jobs_to_settle(
    paths: &AppPaths,
    item_id: &str,
    timeout: Duration,
) -> Result<Vec<jobs::JobRow>> {
    let start = Instant::now();
    let mut idle_since: Option<Instant> = None;
    let mut last_signature: Option<(usize, i64, usize)> = None;

    loop {
        let rows = item_jobs(paths, item_id)?;
        if let Some(failed) = rows
            .iter()
            .find(|job| matches!(job.status, jobs::JobStatus::Failed))
        {
            return Err(EngineError::InstallFailed(format!(
                "item job {} failed at {}: {}",
                failed.id,
                failed.job_type,
                failed
                    .error
                    .clone()
                    .unwrap_or_else(|| "(no error)".to_string())
            )));
        }

        let active = rows.iter().any(|job| {
            matches!(
                job.status,
                jobs::JobStatus::Queued | jobs::JobStatus::Running
            )
        });
        let signature = (
            rows.len(),
            rows.iter().map(|job| job.created_at_ms).max().unwrap_or(0),
            rows.iter()
                .filter(|job| {
                    matches!(
                        job.status,
                        jobs::JobStatus::Succeeded | jobs::JobStatus::Canceled
                    )
                })
                .count(),
        );

        if !active {
            if last_signature.as_ref() == Some(&signature) {
                if idle_since
                    .map(|since| since.elapsed() >= Duration::from_millis(1500))
                    .unwrap_or(false)
                {
                    return Ok(rows);
                }
            } else {
                idle_since = Some(Instant::now());
                last_signature = Some(signature);
            }
        } else {
            idle_since = None;
            last_signature = Some(signature);
        }

        if start.elapsed() > timeout {
            return Err(EngineError::InstallFailed(format!(
                "timeout waiting for item jobs to settle for {item_id}"
            )));
        }
        thread::sleep(Duration::from_millis(700));
    }
}

fn latest_succeeded_item_job<'a>(
    rows: &'a [jobs::JobRow],
    job_type: &str,
) -> Option<&'a jobs::JobRow> {
    rows.iter()
        .filter(|job| job.job_type == job_type && matches!(job.status, jobs::JobStatus::Succeeded))
        .max_by_key(|job| job.finished_at_ms.unwrap_or(job.created_at_ms))
}

fn ensure_model_installed(store: &ModelStore, model_id: &str) -> Result<()> {
    let inv = store.inventory()?;
    let model = inv
        .models
        .iter()
        .find(|m| m.id == model_id)
        .ok_or_else(|| EngineError::UnknownModel(model_id.to_string()))?;
    if model.installed {
        return Ok(());
    }
    store.install_model(model_id)?;
    Ok(())
}

fn wav_peak_is_non_silent(path: &Path) -> Result<bool> {
    let mut reader = WavReader::open(path).map_err(|e| {
        EngineError::InstallFailed(format!("open wav failed ({}): {e}", path.display()))
    })?;
    let spec = reader.spec();
    let is_non_silent = match spec.sample_format {
        SampleFormat::Int => {
            let mut max_peak = 0i64;
            for sample in reader.samples::<i32>() {
                let value = sample.map_err(|e| {
                    EngineError::InstallFailed(format!(
                        "read wav sample failed ({}): {e}",
                        path.display()
                    ))
                })?;
                max_peak = max_peak.max((value as i64).abs());
                if max_peak > 0 {
                    return Ok(true);
                }
            }
            false
        }
        SampleFormat::Float => {
            let mut max_peak = 0.0f32;
            for sample in reader.samples::<f32>() {
                let value = sample.map_err(|e| {
                    EngineError::InstallFailed(format!(
                        "read wav sample failed ({}): {e}",
                        path.display()
                    ))
                })?;
                max_peak = max_peak.max(value.abs());
                if max_peak > 0.000_001 {
                    return Ok(true);
                }
            }
            false
        }
    };
    Ok(is_non_silent)
}

fn latest_track(
    paths: &AppPaths,
    item_id: &str,
    kind: &str,
    lang: &str,
) -> Result<subtitle_tracks::SubtitleTrackRow> {
    subtitle_tracks::list_tracks(paths, item_id)?
        .into_iter()
        .filter(|track| track.kind == kind && track.lang == lang)
        .max_by_key(|track| track.version)
        .ok_or_else(|| {
            EngineError::InstallFailed(format!("track not found for {item_id}: {kind}/{lang}"))
        })
}

fn speaker_keys_for_track(paths: &AppPaths, track_id: &str) -> Result<Vec<String>> {
    let doc = subtitle_tracks::load_document(paths, track_id)?;
    let mut speakers = doc
        .segments
        .iter()
        .filter_map(|segment| segment.speaker.as_ref())
        .map(|speaker| speaker.trim().to_string())
        .filter(|speaker| !speaker.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    speakers.sort();
    Ok(speakers)
}

fn validate_observed_speaker_count(
    speaker_keys: &[String],
    requested: &jobs::DiarizationSpeakerCountRequest,
) -> Result<()> {
    let observed = speaker_keys.len() as u32;
    if let Some(exact) = requested.exact_speakers {
        if observed != exact {
            return Err(EngineError::InstallFailed(format!(
                "expected exactly {exact} speaker label(s), observed {observed}: {}",
                speaker_keys.join(", ")
            )));
        }
    }
    if let Some(min) = requested.min_speakers {
        if observed < min {
            return Err(EngineError::InstallFailed(format!(
                "expected at least {min} speaker label(s), observed {observed}: {}",
                speaker_keys.join(", ")
            )));
        }
    }
    if let Some(max) = requested.max_speakers {
        if observed > max {
            return Err(EngineError::InstallFailed(format!(
                "expected at most {max} speaker label(s), observed {observed}: {}",
                speaker_keys.join(", ")
            )));
        }
    }
    Ok(())
}

#[derive(Serialize)]
struct ProofSummary {
    media_path: String,
    proof_label: String,
    item_id: String,
    first_stage: String,
    second_stage: String,
    source_track_id: String,
    translated_track_id: String,
    speaker_keys: Vec<String>,
    speaker_count: jobs::DiarizationSpeakerCountRequest,
    voice_preserving_report: String,
    mix_wav: String,
    mux_mp4: String,
    app_base_dir: String,
    deliverables_dir: String,
}

fn env_u32(name: &str) -> Option<u32> {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<u32>().ok())
        .filter(|value| *value > 0)
}

fn env_trimmed(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn diarization_speaker_count_from_env() -> jobs::DiarizationSpeakerCountRequest {
    let exact_speakers = env_u32("VOXVULGI_SMOKE_EXACT_SPEAKERS");
    let min_speakers = env_u32("VOXVULGI_SMOKE_MIN_SPEAKERS");
    let max_speakers = env_u32("VOXVULGI_SMOKE_MAX_SPEAKERS");
    let mode = env_trimmed("VOXVULGI_SMOKE_SPEAKER_COUNT_MODE").or_else(|| {
        if exact_speakers.is_some() {
            Some("exact".to_string())
        } else if min_speakers.is_some() || max_speakers.is_some() {
            Some("range".to_string())
        } else {
            None
        }
    });

    jobs::DiarizationSpeakerCountRequest {
        mode,
        exact_speakers,
        min_speakers,
        max_speakers,
    }
}

fn safe_label(raw: &str) -> String {
    let mut label = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    while label.contains("__") {
        label = label.replace("__", "_");
    }
    label = label.trim_matches('_').to_string();
    if label.is_empty() {
        "sample".to_string()
    } else {
        label
    }
}

fn proof_label_for_media(media_path: &Path) -> String {
    env_trimmed("VOXVULGI_SMOKE_LABEL")
        .map(|value| safe_label(&value))
        .unwrap_or_else(|| {
            let stem = media_path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("sample");
            safe_label(stem)
        })
}

fn main() -> Result<()> {
    let engine_dir = std::env::current_dir()?;
    let repo_root = repo_root_from_engine_dir(&engine_dir);
    let base_dir = std::env::var("VOXVULGI_SMOKE_BASE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| repo_root.join("tmp_smoke_wp0150"));
    let media_path = std::env::var("VOXVULGI_SMOKE_MEDIA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            repo_root
                .join("Test material")
                .join("[4K] Queen is here ðŸ˜ Miyeon so cute ðŸ’• (ENG SUB).mp4")
        });
    let proof_dir = std::env::var("VOXVULGI_PROOF_DIR").ok().map(PathBuf::from);
    let proof_label = proof_label_for_media(&media_path);
    let speaker_count = diarization_speaker_count_from_env();

    let paths = AppPaths::new(base_dir.clone());
    paths.ensure_dirs()?;
    db::ensure_schema(&paths)?;
    jobs::set_runtime_max_concurrency(&paths, 1)?;

    let ffmpeg = tools::ffmpeg_tools_status(&paths);
    if ffmpeg.ffmpeg_version.is_none() || ffmpeg.ffprobe_version.is_none() {
        let _ = tools::install_ffmpeg_tools(&paths)?;
    }
    let python = tools::python_toolchain_status(&paths);
    if !python.venv_exists {
        let _ = tools::install_python_toolchain(&paths)?;
    }
    let vp = tools::tts_voice_preserving_local_v1_pack_status(&paths);
    if !vp.installed {
        let _ = tools::install_tts_voice_preserving_local_v1_pack(&paths)?;
    }
    let neural = tools::tts_neural_local_v1_pack_status(&paths);
    if !neural.installed {
        let _ = tools::install_tts_neural_local_v1_pack(&paths)?;
    }
    let diar = tools::diarization_pack_status(&paths);
    if !diar.installed {
        let _ = tools::install_diarization_pack(&paths)?;
    }

    let store = ModelStore::new(paths.clone());
    ensure_model_installed(&store, "whispercpp-tiny")?;

    let runner = jobs::start_runner(paths.clone())?;

    let import_job =
        jobs::enqueue_import_local(&paths, media_path.to_string_lossy().to_string(), true, true)?;
    wait_for_job(&paths, &import_job.id, Duration::from_secs(20 * 60))?;

    let canonical_media = std::fs::canonicalize(&media_path)?;
    let canonical_media_str = canonical_media.to_string_lossy().to_string();
    let item = library::list_items(&paths, 100, 0)?
        .into_iter()
        .find(|it| it.media_path == canonical_media_str)
        .ok_or_else(|| {
            EngineError::InstallFailed("imported item not found in library".to_string())
        })?;

    let first_run = jobs::enqueue_localization_run_v1(
        &paths,
        jobs::LocalizationRunRequest {
            item_id: item.id.clone(),
            asr_lang: Some("ko".to_string()),
            separation_backend: None,
            output_mode: None,
            queue_export_pack: false,
            queue_qc: false,
            speaker_count: speaker_count.clone(),
        },
    )?;
    wait_for_batch_to_idle(&paths, &first_run.batch_id, Duration::from_secs(90 * 60))?;

    let source_track = latest_track(&paths, &item.id, "source", "ko")?;
    let translated_track = latest_track(&paths, &item.id, "translated", "en")?;
    let speaker_keys = speaker_keys_for_track(&paths, &translated_track.id)?;
    if speaker_keys.is_empty() {
        return Err(EngineError::InstallFailed(
            "translated English track is missing speaker labels after the first staged run"
                .to_string(),
        ));
    }
    validate_observed_speaker_count(&speaker_keys, &speaker_count)?;

    let generated_refs = voice_reference_candidates::generate_reference_candidates(
        &paths,
        voice_reference_candidates::VoiceReferenceCandidateGenerationRequest {
            item_id: item.id.clone(),
            track_id: Some(translated_track.id.clone()),
            speaker_key: None,
            missing_only: false,
        },
    )?;
    if generated_refs.bundles.len() != speaker_keys.len() {
        return Err(EngineError::InstallFailed(format!(
            "expected {} generated speaker reference bundle(s), got {}",
            speaker_keys.len(),
            generated_refs.bundles.len()
        )));
    }
    for speaker_key in &speaker_keys {
        let _ = voice_reference_candidates::apply_reference_candidate(
            &paths,
            &item.id,
            speaker_key,
            "replace",
        )?;
    }

    let mix_wav = paths
        .derived_item_dir(&item.id)
        .join("dub_preview")
        .join("mix_dub_preview_v1.wav");
    let mux_mp4 = paths
        .derived_item_dir(&item.id)
        .join("dub_preview")
        .join("mux_dub_preview_v1.mp4");
    let mut second_stage = "auto_resume".to_string();
    let mut final_jobs =
        wait_for_item_jobs_to_settle(&paths, &item.id, Duration::from_secs(90 * 60))?;

    if !mix_wav.exists() || !mux_mp4.exists() {
        let second_run = jobs::enqueue_localization_run_v1(
            &paths,
            jobs::LocalizationRunRequest {
                item_id: item.id.clone(),
                asr_lang: Some("ko".to_string()),
                separation_backend: None,
                output_mode: None,
                queue_export_pack: false,
                queue_qc: true,
                speaker_count: speaker_count.clone(),
            },
        )?;
        second_stage = second_run.stage;
        final_jobs = wait_for_item_jobs_to_settle(&paths, &item.id, Duration::from_secs(90 * 60))?;
    }

    let voice_job =
        latest_succeeded_item_job(&final_jobs, "dub_voice_preserving_v1").ok_or_else(|| {
            EngineError::InstallFailed("dub job missing from settled item jobs".to_string())
        })?;
    let _mix_job =
        latest_succeeded_item_job(&final_jobs, "mix_dub_preview_v1").ok_or_else(|| {
            EngineError::InstallFailed("mix job missing from settled item jobs".to_string())
        })?;
    let _mux_job =
        latest_succeeded_item_job(&final_jobs, "mux_dub_preview_v1").ok_or_else(|| {
            EngineError::InstallFailed("mux job missing from settled item jobs".to_string())
        })?;
    let voice_report = paths
        .job_artifacts_dir(&voice_job.id)
        .join("tts_voice_preserving_report.json");

    if !mix_wav.exists() || !wav_peak_is_non_silent(&mix_wav)? {
        return Err(EngineError::InstallFailed(format!(
            "mixed dub output missing or silent ({})",
            mix_wav.display()
        )));
    }
    if !mux_mp4.exists() {
        return Err(EngineError::InstallFailed(format!(
            "muxed MP4 missing ({})",
            mux_mp4.display()
        )));
    }
    if !voice_report.exists() {
        return Err(EngineError::InstallFailed(format!(
            "voice report missing ({})",
            voice_report.display()
        )));
    }

    if let Some(proof_dir) = proof_dir {
        std::fs::create_dir_all(&proof_dir)?;
        let deliverables_dir = proof_dir.join("deliverables");
        std::fs::create_dir_all(&deliverables_dir)?;

        let copied_mp4 =
            deliverables_dir.join(format!("{proof_label}_localization_run_preview.mp4"));
        let copied_wav =
            deliverables_dir.join(format!("{proof_label}_localization_run_preview.wav"));
        let copied_report =
            deliverables_dir.join(format!("{proof_label}_tts_voice_preserving_report.json"));
        std::fs::copy(&mux_mp4, &copied_mp4)?;
        std::fs::copy(&mix_wav, &copied_wav)?;
        std::fs::copy(&voice_report, &copied_report)?;

        let summary = ProofSummary {
            media_path: media_path.to_string_lossy().to_string(),
            proof_label,
            item_id: item.id.clone(),
            first_stage: first_run.stage,
            second_stage,
            source_track_id: source_track.id.clone(),
            translated_track_id: translated_track.id.clone(),
            speaker_keys,
            speaker_count,
            voice_preserving_report: copied_report.to_string_lossy().to_string(),
            mix_wav: copied_wav.to_string_lossy().to_string(),
            mux_mp4: copied_mp4.to_string_lossy().to_string(),
            app_base_dir: base_dir.to_string_lossy().to_string(),
            deliverables_dir: deliverables_dir.to_string_lossy().to_string(),
        };
        std::fs::write(
            proof_dir.join("proof_summary.json"),
            format!("{}\n", serde_json::to_string_pretty(&summary)?),
        )?;
    }

    runner.stop();
    Ok(())
}
