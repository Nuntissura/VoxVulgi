use std::path::PathBuf;
use std::thread::sleep;
use std::time::{Duration, Instant};

use voxvulgi_engine::{jobs, library, models, paths::AppPaths, subtitle_tracks, tools};

const ASR_JOB_TIMEOUT_SECS: u64 = 1_200;
const JOB_TIMEOUT_SECS: u64 = 300;
const SAMPLE_ENV_VAR: &str = "VOXVULGI_SMOKE_SAMPLE";

#[derive(Debug)]
struct SmokeFailure {
    message: String,
}

impl std::fmt::Display for SmokeFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for SmokeFailure {}

type SmokeResult<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
fn default_sample_path() -> PathBuf {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    let test_material_dir = repo_root.join("Test material");
    if let Ok(entries) = std::fs::read_dir(&test_material_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase());
            if matches!(ext.as_deref(), Some("mp4" | "mkv" | "mov" | "m4v")) {
                return path;
            }
        }
    }
    test_material_dir.join("sample.mp4")
}

fn resolve_sample_path() -> PathBuf {
    if let Ok(p) = std::env::var(SAMPLE_ENV_VAR) {
        if !p.trim().is_empty() {
            return PathBuf::from(p);
        }
    }
    default_sample_path()
}

fn wait_for_job_done(
    paths: &AppPaths,
    job_id: &str,
    timeout: Duration,
) -> SmokeResult<jobs::JobRow> {
    let started = Instant::now();
    loop {
        if started.elapsed() > timeout {
            return Err(Box::new(SmokeFailure {
                message: format!("timeout waiting for job {job_id}"),
            }));
        }

        let jobs = jobs::list_jobs(paths, 200, 0).map_err(|e| format!("list_jobs failed: {e}"))?;
        let target = jobs.into_iter().find(|j| j.id == job_id);
        if let Some(job) = target {
            if matches!(job.status, jobs::JobStatus::Succeeded) {
                return Ok(job);
            }
            if matches!(job.status, jobs::JobStatus::Failed | jobs::JobStatus::Canceled) {
                return Err(Box::new(SmokeFailure {
                    message: format!(
                        "job {job_id} failed (status={:?}, error={:?})",
                        job.status, job.error
                    ),
                }));
            }
        }

        sleep(Duration::from_millis(300));
    }
}

fn pick_track_id(
    item_id: &str,
    tracks: Vec<subtitle_tracks::SubtitleTrackRow>,
    kind: &str,
) -> Option<String> {
    tracks
        .into_iter()
        .filter(|t| t.kind == kind)
        .find(|t| t.item_id == item_id)
        .map(|t| t.id)
}

#[test]
#[ignore = "manual WP-0027 smoke chain"]
fn wp_0027_phase2_smoke_chain_on_sample() -> SmokeResult<()> {
    let sample = resolve_sample_path();
    assert!(
        sample.exists(),
        "sample must exist: {} (set {} to override)",
        sample.display(),
        SAMPLE_ENV_VAR
    );

    let base_dir = std::env::temp_dir()
        .join("voxvulgi_wp0027_smoke")
        .join(format!("run_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base_dir);
    std::fs::create_dir_all(&base_dir)?;

    let paths = AppPaths::new(base_dir.clone());
    paths.ensure_dirs().map_err(|e| format!("ensure_dirs failed: {e}"))?;

    let ffmpeg = tools::install_ffmpeg_tools(&paths)
        .map_err(|e| format!("install ffmpeg tools failed: {e}"))?;
    assert!(ffmpeg.installed, "ffmpeg tools should be installed");

    let store = models::ModelStore::new(paths.clone());
    store
        .install_bundled_model("whispercpp-tiny")
        .map_err(|e| format!("install whisper model failed: {e}"))?;

    let item = library::import_local_file(&paths, &sample)
        .map_err(|e| format!("import local failed: {e}"))?;

    let runner = jobs::start_runner(paths.clone())?;

    let install_py = tools::install_python_toolchain(&paths)
        .map_err(|e| format!("install python toolchain failed: {e}"))?;
    assert!(install_py.venv_exists, "python venv should exist");

    let sep_status = tools::install_spleeter_pack(&paths)
        .map_err(|e| format!("install spleeter failed: {e}"))?;
    assert!(sep_status.installed, "spleeter should be installed");

    let diar_status = tools::install_diarization_pack(&paths)
        .map_err(|e| format!("install diarization failed: {e}"))?;
    assert!(diar_status.installed, "diarization pack should be installed");

    let tts_status = tools::install_tts_preview_pack(&paths)
        .map_err(|e| format!("install tts preview failed: {e}"))?;
    assert!(tts_status.installed, "tts preview pack should be installed");

    let asr_job = jobs::enqueue_asr_local(&paths, item.id.clone(), Some("ja".to_string()))
        .map_err(|e| format!("enqueue asr failed: {e}"))?;
    let _ = wait_for_job_done(&paths, &asr_job.id, Duration::from_secs(ASR_JOB_TIMEOUT_SECS))
        .map_err(|e| format!("ASR job failed: {e}"))?;

    let source_track_id = {
        let tracks = subtitle_tracks::list_tracks(&paths, &item.id)
            .map_err(|e| format!("list tracks failed: {e}"))?;
        pick_track_id(&item.id, tracks, "source")
            .ok_or_else(|| "missing source track after ASR".to_string())?
    };

    let separate_job = jobs::enqueue_separate_audio_spleeter(&paths, item.id.clone())
        .map_err(|e| format!("enqueue separate failed: {e}"))?;
    let _ = wait_for_job_done(&paths, &separate_job.id, Duration::from_secs(JOB_TIMEOUT_SECS))
        .map_err(|e| format!("Separate job failed: {e}"))?;

    let sep_vocals = paths
        .derived_item_dir(&item.id)
        .join("separation")
        .join("spleeter_2stems")
        .join("vocals.wav");
    let sep_background = paths
        .derived_item_dir(&item.id)
        .join("separation")
        .join("spleeter_2stems")
        .join("background.wav");
    assert!(sep_vocals.exists(), "vocals.wav expected: {}", sep_vocals.display());
    assert!(
        sep_background.exists(),
        "background.wav expected: {}",
        sep_background.display()
    );

    let diarize_job = jobs::enqueue_diarize_local_v1(
        &paths,
        item.id.clone(),
        source_track_id.clone(),
    )
    .map_err(|e| format!("enqueue diarize failed: {e}"))?;
    let _ = wait_for_job_done(&paths, &diarize_job.id, Duration::from_secs(JOB_TIMEOUT_SECS))
        .map_err(|e| format!("Diarize job failed: {e}"))?;

    let diar_track_id = {
        let tracks = subtitle_tracks::list_tracks(&paths, &item.id)
            .map_err(|e| format!("list tracks after diarize failed: {e}"))?;
        tracks
            .into_iter()
            .filter(|t| t.kind == "source")
            .find(|t| t.item_id == item.id && t.created_by.starts_with("diarize:"))
            .map(|t| t.id)
            .ok_or_else(|| "missing diarized source track".to_string())?
    };

    let diar_json = paths
        .derived_item_dir(&item.id)
        .join("diarize")
        .join("diarization.json");
    assert!(diar_json.exists(), "diarization.json expected: {}", diar_json.display());

    let tts_job = jobs::enqueue_tts_preview_pyttsx3_v1(
        &paths,
        item.id.clone(),
        diar_track_id.clone(),
    )
    .map_err(|e| format!("enqueue tts preview failed: {e}"))?;
    let _ = wait_for_job_done(&paths, &tts_job.id, Duration::from_secs(JOB_TIMEOUT_SECS))
        .map_err(|e| format!("TTS preview job failed: {e}"))?;

    let tts_manifest = paths
        .derived_item_dir(&item.id)
        .join("tts_preview")
        .join("pyttsx3_v1")
        .join("manifest.json");
    assert!(tts_manifest.exists(), "tts manifest expected: {}", tts_manifest.display());

    let mix_job = jobs::enqueue_mix_dub_preview_v1(&paths, item.id.clone())
        .map_err(|e| format!("enqueue mix failed: {e}"))?;
    let _ = wait_for_job_done(&paths, &mix_job.id, Duration::from_secs(JOB_TIMEOUT_SECS))
        .map_err(|e| format!("Mix job failed: {e}"))?;

    let mixed = paths
        .derived_item_dir(&item.id)
        .join("dub_preview")
        .join("mix_dub_preview_v1.wav");
    assert!(mixed.exists(), "mixed preview expected: {}", mixed.display());

    let mux_job = jobs::enqueue_mux_dub_preview_v1(&paths, item.id.clone())
        .map_err(|e| format!("enqueue mux failed: {e}"))?;
    let _ = wait_for_job_done(&paths, &mux_job.id, Duration::from_secs(JOB_TIMEOUT_SECS))
        .map_err(|e| format!("Mux job failed: {e}"))?;

    let muxed = paths
        .derived_item_dir(&item.id)
        .join("dub_preview")
        .join("mux_dub_preview_v1.mp4");
    assert!(muxed.exists(), "muxed preview expected: {}", muxed.display());

    runner.stop();
    Ok(())
}

