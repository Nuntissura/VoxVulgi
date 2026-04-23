use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use hound::{SampleFormat, WavReader};
use serde_json::Value;
use voxvulgi_engine::models::ModelStore;
use voxvulgi_engine::paths::AppPaths;
use voxvulgi_engine::{db, jobs, library, speakers, subtitle_tracks, tools, EngineError, Result};

fn repo_root_from_engine_dir(engine_dir: &Path) -> PathBuf {
    engine_dir.join("..").join("..")
}

fn wait_for_job(paths: &AppPaths, job_id: &str, timeout: Duration) -> Result<jobs::JobRow> {
    let start = Instant::now();
    let mut last_status = String::new();

    loop {
        let rows = jobs::list_jobs(paths, 500, 0)?;
        if let Some(job) = rows.into_iter().find(|j| j.id == job_id) {
            let status = format!("{:?}", job.status);
            if status != last_status {
                last_status = status.clone();
                eprintln!(
                    "job {} status={} progress={}%",
                    job_id,
                    status,
                    (job.progress * 100.0).round()
                );
            }

            match job.status {
                jobs::JobStatus::Succeeded => return Ok(job),
                jobs::JobStatus::Failed => {
                    return Err(EngineError::InstallFailed(format!(
                        "job {job_id} failed: {}",
                        job.error
                            .clone()
                            .unwrap_or_else(|| "(no error)".to_string())
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
    eprintln!(
        "installing model {} ({} bytes)...",
        model.id, model.expected_bytes
    );
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

fn main() -> Result<()> {
    let engine_dir = std::env::current_dir()?;
    let repo_root = repo_root_from_engine_dir(&engine_dir);
    let base_dir = std::env::var("VOXVULGI_SMOKE_BASE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| repo_root.join("tmp_smoke_wp0029"));
    let media_path = std::env::var("VOXVULGI_SMOKE_MEDIA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            repo_root
                .join("Test material")
                .join("[4K] Queen is here 😍 Miyeon so cute 💕 (ENG SUB).mp4")
        });

    eprintln!("base_dir: {}", base_dir.to_string_lossy());
    eprintln!("media_path: {}", media_path.to_string_lossy());

    let paths = AppPaths::new(base_dir);
    paths.ensure_dirs()?;
    db::ensure_schema(&paths)?;

    jobs::set_runtime_max_concurrency(&paths, 1)?;

    // Installs (explicit / local-first).
    let ffmpeg = tools::ffmpeg_tools_status(&paths);
    if ffmpeg.ffmpeg_version.is_none() || ffmpeg.ffprobe_version.is_none() {
        eprintln!("installing FFmpeg tools...");
        let _ = tools::install_ffmpeg_tools(&paths)?;
    }

    let python = tools::python_toolchain_status(&paths);
    if !python.venv_exists {
        eprintln!("setting up Python toolchain...");
        let _ = tools::install_python_toolchain(&paths)?;
    }

    let diar = tools::diarization_pack_status(&paths);
    if !diar.installed {
        eprintln!("installing diarization pack...");
        let _ = tools::install_diarization_pack(&paths)?;
    }

    let spleeter = tools::spleeter_pack_status(&paths);
    if !spleeter.installed {
        eprintln!("installing Spleeter pack...");
        let _ = tools::install_spleeter_pack(&paths)?;
    }

    let vp = tools::tts_voice_preserving_local_v1_pack_status(&paths);
    eprintln!(
        "voice-preserving status: installed={} openvoice_version={:?} models_installed={} patch_applied={} models_dir={}",
        vp.installed,
        vp.openvoice_version,
        vp.openvoice_models_installed,
        vp.openvoice_patch_applied,
        vp.openvoice_models_dir
    );
    if !vp.installed {
        eprintln!("installing voice-preserving pack (Kokoro + OpenVoice V2 models)...");
        let _ = tools::install_tts_voice_preserving_local_v1_pack(&paths)?;
    }

    // Models (ASR/translate).
    let store = ModelStore::new(paths.clone());
    ensure_model_installed(&store, "whispercpp-tiny")?;

    // Start runner (executes queued jobs).
    let runner = jobs::start_runner(paths.clone())?;

    // 1) Import.
    let import_job = jobs::enqueue_import_local(
        &paths,
        media_path.to_string_lossy().to_string(),
        true,
    )?;
    wait_for_job(&paths, &import_job.id, Duration::from_secs(20 * 60))?;

    let canonical_media = std::fs::canonicalize(&media_path)?;
    let canonical_media_str = canonical_media.to_string_lossy().to_string();
    let item = library::list_items(&paths, 50, 0)?
        .into_iter()
        .find(|it| it.media_path == canonical_media_str)
        .ok_or_else(|| {
            EngineError::InstallFailed("imported item not found in library".to_string())
        })?;
    eprintln!("imported item_id={}", item.id);

    // 2) ASR (KO/JA auto works; KO is the common case for our sample).
    let asr_job = jobs::enqueue_asr_local(&paths, item.id.clone(), Some("ko".to_string()))?;
    wait_for_job(&paths, &asr_job.id, Duration::from_secs(45 * 60))?;

    // 3) Translate to EN from the latest source track.
    let tracks = subtitle_tracks::list_tracks(&paths, &item.id)?;
    let source_track = tracks
        .iter()
        .find(|t| t.kind == "source")
        .ok_or_else(|| EngineError::InstallFailed("source subtitle track not found".to_string()))?;
    let translate_job =
        jobs::enqueue_translate_local(&paths, item.id.clone(), source_track.id.clone())?;
    wait_for_job(&paths, &translate_job.id, Duration::from_secs(45 * 60))?;

    // 4) Diarize the translated EN track so speakers are available for voice-preserving dubbing.
    let tracks = subtitle_tracks::list_tracks(&paths, &item.id)?;
    let translated_track = tracks
        .iter()
        .find(|t| t.kind == "translated" && t.lang == "en")
        .ok_or_else(|| {
            EngineError::InstallFailed("translated EN subtitle track not found".to_string())
        })?;
    let diarize_job =
        jobs::enqueue_diarize_local_v1(&paths, item.id.clone(), translated_track.id.clone())?;
    wait_for_job(&paths, &diarize_job.id, Duration::from_secs(45 * 60))?;

    let tracks = subtitle_tracks::list_tracks(&paths, &item.id)?;
    let diarized_en_track = tracks
        .iter()
        .filter(|t| t.kind == "translated" && t.lang == "en")
        .max_by_key(|t| t.version)
        .ok_or_else(|| EngineError::InstallFailed("diarized EN track not found".to_string()))?;
    eprintln!(
        "using diarized_en_track={} v{}",
        diarized_en_track.id, diarized_en_track.version
    );

    // Build a single reference audio clip from the source media (10s) and map all speakers to it.
    let ref_dir = paths.derived_item_dir(&item.id).join("voice_profiles");
    std::fs::create_dir_all(&ref_dir)?;
    let ref_wav = ref_dir.join("ref_10s.wav");
    let ffmpeg_cmd = paths.ffmpeg_cmd();
    let output = voxvulgi_engine::cmd::command(ffmpeg_cmd)
        .args(["-nostdin", "-y"])
        .arg("-i")
        .arg(&canonical_media)
        .args(["-vn", "-ac", "1", "-ar", "16000", "-t", "10"])
        .arg(&ref_wav)
        .output()
        .map_err(|e| EngineError::InstallFailed(format!("ffmpeg extract ref failed: {e}")))?;
    if !output.status.success() {
        return Err(EngineError::InstallFailed(format!(
            "ffmpeg extract ref failed (code={:?}): {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }

    let diarized_doc = subtitle_tracks::load_document(&paths, &diarized_en_track.id)?;
    let mut speakers_set: BTreeSet<String> = BTreeSet::new();
    for seg in diarized_doc.segments.iter() {
        if let Some(s) = seg.speaker.as_ref() {
            let t = s.trim();
            if !t.is_empty() {
                speakers_set.insert(t.to_string());
            }
        }
    }
    if speakers_set.is_empty() {
        return Err(EngineError::InstallFailed(
            "no speakers found in diarized track (cannot run voice-preserving dub)".to_string(),
        ));
    }

    for speaker_key in speakers_set.iter() {
        let _ = speakers::upsert_item_speaker_setting(
            &paths,
            &item.id,
            speaker_key,
            None,
            None,
            None,
            Some(ref_wav.to_string_lossy().to_string()),
            Some(vec![ref_wav.to_string_lossy().to_string()]),
            None,
            None,
            None,
            Some("clone".to_string()),
            None,
        )?;
    }
    eprintln!(
        "mapped {} speakers to {}",
        speakers_set.len(),
        ref_wav.display()
    );

    // 5) Voice-preserving dub (segments + manifest).
    let dub_job = jobs::enqueue_dub_voice_preserving_v1(
        &paths,
        item.id.clone(),
        diarized_en_track.id.clone(),
    )?;
    wait_for_job(&paths, &dub_job.id, Duration::from_secs(60 * 60))?;

    let vp_report = paths
        .job_artifacts_dir(&dub_job.id)
        .join("tts_voice_preserving_report.json");

    let report_json = std::fs::read_to_string(&vp_report).map_err(|e| {
        EngineError::InstallFailed(format!(
            "voice-preserving report missing ({}): {e}",
            vp_report.display()
        ))
    })?;
    let report_value: Value = serde_json::from_str(&report_json)?;
    let segments_base_ok = report_value
        .get("segments_base_ok")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let segments_converted_ok = report_value
        .get("segments_converted_ok")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if segments_base_ok == 0 {
        return Err(EngineError::InstallFailed(format!(
            "voice-preserving smoke failed: no base speech segments were synthesized (report={})",
            vp_report.display()
        )));
    }

    // 6) Separation -> mix -> mux.
    let sep_job = jobs::enqueue_separate_audio_spleeter(&paths, item.id.clone())?;
    wait_for_job(&paths, &sep_job.id, Duration::from_secs(60 * 60))?;

    let mix_job = jobs::enqueue_mix_dub_preview_v1(&paths, item.id.clone())?;
    wait_for_job(&paths, &mix_job.id, Duration::from_secs(60 * 60))?;

    let mux_job = jobs::enqueue_mux_dub_preview_v1(&paths, item.id.clone())?;
    wait_for_job(&paths, &mux_job.id, Duration::from_secs(60 * 60))?;

    let item_dir = paths.derived_item_dir(&item.id);
    let voice_manifest = item_dir
        .join("tts_preview")
        .join("dub_voice_preserving_v1")
        .join("manifest.json");
    let voice_segments_dir = item_dir
        .join("tts_preview")
        .join("dub_voice_preserving_v1")
        .join("segments");
    let mix_out = item_dir.join("dub_preview").join("mix_dub_preview_v1.wav");
    let mux_out = item_dir.join("dub_preview").join("mux_dub_preview_v1.mp4");
    let first_segment = std::fs::read_dir(&voice_segments_dir)?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .find(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("wav"))
                .unwrap_or(false)
        })
        .ok_or_else(|| {
            EngineError::InstallFailed(format!(
                "voice-preserving smoke failed: no segment WAVs found in {}",
                voice_segments_dir.display()
            ))
        })?;
    if !wav_peak_is_non_silent(&first_segment)? {
        return Err(EngineError::InstallFailed(format!(
            "voice-preserving smoke failed: first segment is silent ({})",
            first_segment.display()
        )));
    }
    eprintln!(
        "voice-preserving report: {} (base_ok={}, converted_ok={})",
        vp_report.display(),
        segments_base_ok,
        segments_converted_ok
    );
    eprintln!("voice manifest: {}", voice_manifest.display());
    eprintln!("mixed dub wav: {}", mix_out.display());
    eprintln!("muxed preview mp4: {}", mux_out.display());

    runner.stop();
    Ok(())
}
