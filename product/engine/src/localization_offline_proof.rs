use crate::models::ModelStore;
use crate::paths::AppPaths;
use crate::{
    jobs, library, subtitle_tracks, tools, voice_reference_candidates, EngineError, Result,
};
use hound::{SampleFormat, WavReader};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const JOB_TIMEOUT: Duration = Duration::from_secs(90 * 60);
const RUNNER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Deserialize)]
pub struct OfflineLocalizationProofRequest {
    pub media_path: String,
    #[serde(default)]
    pub asr_lang: Option<String>,
    pub proof_run_id: String,
    pub proof_started_at_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct OfflineLocalizationProofJob {
    pub id: String,
    pub job_type: String,
    pub status: String,
    pub batch_id: Option<String>,
    pub created_at_ms: i64,
    pub started_at_ms: Option<i64>,
    pub finished_at_ms: Option<i64>,
    pub input_media_path: Option<String>,
    pub input_media_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OfflineLocalizationProofArtifact {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub producer_job_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OfflineLocalizationProofSummary {
    pub schema_version: u32,
    pub outcome: String,
    pub generated_at_ms: i64,
    pub proof_run_id: String,
    pub proof_started_at_ms: i64,
    pub proof_batch_ids: Vec<String>,
    pub engine_version: String,
    pub app_base_dir: String,
    pub media_path: String,
    pub media_sha256: String,
    pub item_id: String,
    pub asr_model_id: String,
    pub asr_lang: Option<String>,
    pub first_stage: String,
    pub final_stage: String,
    pub source_track_id: String,
    pub translated_track_id: String,
    pub speaker_keys: Vec<String>,
    pub network_policy: Vec<String>,
    pub required_job_types: Vec<String>,
    pub jobs: Vec<OfflineLocalizationProofJob>,
    pub mux: OfflineLocalizationProofArtifact,
    pub mix: OfflineLocalizationProofArtifact,
    pub export_pack: OfflineLocalizationProofArtifact,
    pub voice_report: OfflineLocalizationProofArtifact,
    pub proof_summary_path: String,
}

struct OfflineEnvironmentGuard {
    previous: Vec<(&'static str, Option<std::ffi::OsString>)>,
}

impl OfflineEnvironmentGuard {
    fn apply() -> Self {
        let values = [
            ("HF_HUB_OFFLINE", "1"),
            ("TRANSFORMERS_OFFLINE", "1"),
            ("HF_DATASETS_OFFLINE", "1"),
            ("PIP_NO_INDEX", "1"),
            ("HTTP_PROXY", "http://127.0.0.1:1"),
            ("HTTPS_PROXY", "http://127.0.0.1:1"),
            ("ALL_PROXY", "http://127.0.0.1:1"),
            ("NO_PROXY", "127.0.0.1,localhost"),
        ];
        let previous = values
            .iter()
            .map(|(name, value)| {
                let old = std::env::var_os(name);
                std::env::set_var(name, value);
                (*name, old)
            })
            .collect();
        Self { previous }
    }
}

impl Drop for OfflineEnvironmentGuard {
    fn drop(&mut self) {
        for (name, value) in self.previous.drain(..) {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis() as i64)
        .unwrap_or(0)
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn artifact(path: &Path, producer_job_id: &str) -> Result<OfflineLocalizationProofArtifact> {
    let metadata = std::fs::metadata(path)?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err(EngineError::InstallFailed(format!(
            "proof artifact is missing or empty: {}",
            path.display()
        )));
    }
    Ok(OfflineLocalizationProofArtifact {
        path: path.to_string_lossy().to_string(),
        bytes: metadata.len(),
        sha256: sha256_file(path)?,
        producer_job_id: producer_job_id.to_string(),
    })
}

fn publish_proof_artifact_atomic<F>(
    source: &Path,
    final_path: &Path,
    proof_run_id: &str,
    producer_job_id: &str,
    validate: F,
) -> Result<OfflineLocalizationProofArtifact>
where
    F: Fn(&Path) -> Result<()>,
{
    if final_path.exists() {
        return Err(EngineError::InstallFailed(format!(
            "proof artifact final already exists: {}",
            final_path.display()
        )));
    }
    let file_name = final_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            EngineError::InstallFailed("proof artifact name is not Unicode".to_string())
        })?;
    let temp_path = final_path.with_file_name(format!(".{proof_run_id}.{file_name}.tmp"));
    let mut source_file = File::open(source)?;
    let mut temp_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)?;
    std::io::copy(&mut source_file, &mut temp_file)?;
    temp_file.flush()?;
    temp_file.sync_all()?;
    drop(temp_file);
    validate(&temp_path)?;
    let temp_artifact = artifact(&temp_path, producer_job_id)?;
    std::fs::rename(&temp_path, final_path)?;
    OpenOptions::new()
        .write(true)
        .open(final_path)?
        .sync_all()?;
    let final_artifact = artifact(final_path, producer_job_id)?;
    if final_artifact.bytes != temp_artifact.bytes || final_artifact.sha256 != temp_artifact.sha256
    {
        return Err(EngineError::InstallFailed(format!(
            "proof artifact identity changed during atomic publication: {}",
            final_path.display()
        )));
    }
    Ok(final_artifact)
}

fn publish_proof_json_atomic(
    value: &serde_json::Value,
    final_path: &Path,
    proof_run_id: &str,
    producer_job_id: &str,
) -> Result<OfflineLocalizationProofArtifact> {
    if final_path.exists() {
        return Err(EngineError::InstallFailed(format!(
            "proof JSON final already exists: {}",
            final_path.display()
        )));
    }
    let file_name = final_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| EngineError::InstallFailed("proof JSON name is not Unicode".to_string()))?;
    let temp_path = final_path.with_file_name(format!(".{proof_run_id}.{file_name}.tmp"));
    let mut temp_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)?;
    serde_json::to_writer_pretty(&mut temp_file, value)?;
    temp_file.write_all(b"\n")?;
    temp_file.flush()?;
    temp_file.sync_all()?;
    drop(temp_file);
    validate_proof_json(&temp_path)?;
    let temp_artifact = artifact(&temp_path, producer_job_id)?;
    std::fs::rename(&temp_path, final_path)?;
    OpenOptions::new()
        .write(true)
        .open(final_path)?
        .sync_all()?;
    let final_artifact = artifact(final_path, producer_job_id)?;
    if final_artifact.bytes != temp_artifact.bytes || final_artifact.sha256 != temp_artifact.sha256
    {
        return Err(EngineError::InstallFailed(format!(
            "proof JSON identity changed during atomic publication: {}",
            final_path.display()
        )));
    }
    Ok(final_artifact)
}

fn validate_proof_json(path: &Path) -> Result<()> {
    let value: serde_json::Value = serde_json::from_reader(File::open(path)?)?;
    if !value.is_object() {
        return Err(EngineError::InstallFailed(format!(
            "proof JSON artifact is not an object: {}",
            path.display()
        )));
    }
    Ok(())
}

fn validate_proof_export_zip(path: &Path) -> Result<()> {
    let mut archive = zip::ZipArchive::new(File::open(path)?).map_err(|error| {
        EngineError::InstallFailed(format!("proof export ZIP is invalid: {error}"))
    })?;
    let mut names = BTreeSet::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| {
            EngineError::InstallFailed(format!("proof export ZIP entry is invalid: {error}"))
        })?;
        let name = entry.name().replace('\\', "/");
        if name.starts_with('/') || name.split('/').any(|part| part == "..") {
            return Err(EngineError::InstallFailed(format!(
                "proof export ZIP contains an unsafe path: {name}"
            )));
        }
        if !entry.is_dir() {
            std::io::copy(&mut entry, &mut std::io::sink())?;
            names.insert(name);
        }
    }
    let required_exact = [
        "dub_preview/mix_dub_preview_v1.wav",
        "dub_preview/mux_dub_preview_v1.mkv",
        "provenance/manifest.json",
    ];
    if !required_exact.iter().all(|name| names.contains(*name))
        || !names
            .iter()
            .any(|name| name.starts_with("subtitles/source."))
        || !names
            .iter()
            .any(|name| name.starts_with("subtitles/translated."))
    {
        return Err(EngineError::InstallFailed(format!(
            "proof export ZIP is missing required dub, captions, or provenance members: {}",
            path.display()
        )));
    }
    Ok(())
}

fn validate_proof_mkv(paths: &AppPaths, path: &Path) -> Result<()> {
    let ffprobe = [
        paths.tools_dir().join("ffmpeg").join("ffprobe.exe"),
        paths
            .tools_dir()
            .join("ffmpeg")
            .join("bin")
            .join("ffprobe.exe"),
    ]
    .into_iter()
    .find(|candidate| candidate.is_file())
    .ok_or_else(|| {
        EngineError::InstallFailed("proof MKV validator ffprobe is missing".to_string())
    })?;
    let output = std::process::Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_type:format=format_name",
            "-of",
            "json",
        ])
        .arg(path)
        .output()?;
    if !output.status.success() {
        return Err(EngineError::InstallFailed(format!(
            "proof MKV ffprobe failed with status {}",
            output.status
        )));
    }
    let value: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let types = value
        .get("streams")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|stream| stream.get("codec_type").and_then(serde_json::Value::as_str))
        .collect::<BTreeSet<_>>();
    let format_name = value
        .pointer("/format/format_name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if !format_name.split(',').any(|value| value == "matroska")
        || !["video", "audio", "subtitle"]
            .iter()
            .all(|required| types.contains(required))
    {
        return Err(EngineError::InstallFailed(
            "proof MKV lacks Matroska, video, dubbed audio, or subtitle semantics".to_string(),
        ));
    }
    Ok(())
}

fn wait_for_job(paths: &AppPaths, job_id: &str, timeout: Duration) -> Result<jobs::JobRow> {
    let start = Instant::now();
    loop {
        if let Some(job) = jobs::list_jobs(paths, 1_000, 0)?
            .into_iter()
            .find(|row| row.id == job_id)
        {
            match job.status {
                jobs::JobStatus::Succeeded => return Ok(job),
                jobs::JobStatus::Failed => {
                    return Err(EngineError::InstallFailed(format!(
                        "job {job_id} failed: {}",
                        job.error.unwrap_or_else(|| "(no error)".to_string())
                    )))
                }
                jobs::JobStatus::Canceled => {
                    return Err(EngineError::InstallFailed(format!(
                        "job {job_id} was canceled"
                    )))
                }
                jobs::JobStatus::Queued | jobs::JobStatus::Running => {}
            }
        }
        if start.elapsed() >= timeout {
            return Err(EngineError::InstallFailed(format!(
                "timeout waiting for job {job_id}"
            )));
        }
        thread::sleep(Duration::from_millis(500));
    }
}

fn item_jobs(paths: &AppPaths, item_id: &str) -> Result<Vec<jobs::JobRow>> {
    let mut rows = jobs::list_jobs(paths, 2_000, 0)?
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
    let mut stable_since: Option<Instant> = None;
    let mut last_signature: Option<(usize, i64, usize)> = None;
    loop {
        let rows = item_jobs(paths, item_id)?;
        if let Some(failed) = rows
            .iter()
            .find(|job| matches!(job.status, jobs::JobStatus::Failed))
        {
            return Err(EngineError::InstallFailed(format!(
                "localization job {} failed at {}: {}",
                failed.id,
                failed.job_type,
                failed.error.as_deref().unwrap_or("(no error)")
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
                .filter(|job| matches!(job.status, jobs::JobStatus::Succeeded))
                .count(),
        );
        if !active {
            if last_signature == Some(signature) {
                if stable_since
                    .map(|since| since.elapsed() >= Duration::from_secs(2))
                    .unwrap_or(false)
                {
                    return Ok(rows);
                }
            } else {
                stable_since = Some(Instant::now());
                last_signature = Some(signature);
            }
        } else {
            stable_since = None;
            last_signature = Some(signature);
        }
        if start.elapsed() >= timeout {
            return Err(EngineError::InstallFailed(format!(
                "timeout waiting for localization jobs for item {item_id}"
            )));
        }
        thread::sleep(Duration::from_millis(700));
    }
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
            EngineError::InstallFailed(format!("required subtitle track is missing: {kind}/{lang}"))
        })
}

fn speaker_keys(paths: &AppPaths, track_id: &str) -> Result<Vec<String>> {
    let document = subtitle_tracks::load_document(paths, track_id)?;
    Ok(document
        .segments
        .iter()
        .filter_map(|segment| segment.speaker.as_deref())
        .map(str::trim)
        .filter(|speaker| !speaker.is_empty())
        .map(str::to_string)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect())
}

fn wav_is_non_silent(path: &Path) -> Result<bool> {
    let mut reader = WavReader::open(path).map_err(|error| {
        EngineError::InstallFailed(format!("failed to read {}: {error}", path.display()))
    })?;
    match reader.spec().sample_format {
        SampleFormat::Int => {
            for sample in reader.samples::<i32>() {
                if sample.map_err(|error| {
                    EngineError::InstallFailed(format!("failed to read WAV sample: {error}"))
                })? != 0
                {
                    return Ok(true);
                }
            }
        }
        SampleFormat::Float => {
            for sample in reader.samples::<f32>() {
                if sample
                    .map_err(|error| {
                        EngineError::InstallFailed(format!("failed to read WAV sample: {error}"))
                    })?
                    .abs()
                    > 0.000_001
                {
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}

fn newest_mkv(dir: &Path) -> Result<PathBuf> {
    let mut candidates = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(current) = pending.pop() {
        for entry in std::fs::read_dir(&current)? {
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path);
            } else if path
                .extension()
                .and_then(|value| value.to_str())
                .map(|value| value.eq_ignore_ascii_case("mkv"))
                .unwrap_or(false)
            {
                let modified = std::fs::metadata(&path)
                    .and_then(|metadata| metadata.modified())
                    .unwrap_or(UNIX_EPOCH);
                candidates.push((modified, path));
            }
        }
    }
    candidates
        .into_iter()
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, path)| path)
        .ok_or_else(|| EngineError::InstallFailed("muxed MKV output is missing".to_string()))
}

fn preflight(paths: &AppPaths) -> Result<String> {
    let ffmpeg = tools::ffmpeg_tools_status(paths);
    if !ffmpeg.installed || ffmpeg.ffmpeg_version.is_none() || ffmpeg.ffprobe_version.is_none() {
        return Err(EngineError::InstallFailed(
            "offline proof preflight failed: bundled ffmpeg/ffprobe is not ready".to_string(),
        ));
    }
    let python = tools::python_toolchain_status(paths);
    if !python.base_available
        || !python.venv_exists
        || python.venv_python_version.is_none()
        || python.venv_pip_version.is_none()
    {
        return Err(EngineError::InstallFailed(
            "offline proof preflight failed: bundled Python environment is not ready".to_string(),
        ));
    }
    if !tools::demucs_pack_status(paths).installed {
        return Err(EngineError::InstallFailed(
            "offline proof preflight failed: bundled separation pack is not ready".to_string(),
        ));
    }
    let diarization = tools::diarization_pack_status(paths);
    if !diarization.installed {
        return Err(EngineError::InstallFailed(format!(
            "offline proof preflight failed: bundled diarization pack is not ready ({})",
            diarization.status_detail
        )));
    }
    let neural = tools::tts_neural_local_v1_pack_status(paths);
    if !neural.installed {
        return Err(EngineError::InstallFailed(format!(
            "offline proof preflight failed: bundled neural TTS pack is not ready ({})",
            neural.status_detail
        )));
    }
    let voice = tools::tts_voice_preserving_local_v1_pack_status(paths);
    if !voice.installed {
        return Err(EngineError::InstallFailed(format!(
            "offline proof preflight failed: bundled voice-preserving pack is not ready ({})",
            voice.status_detail
        )));
    }
    let cosyvoice = tools::cosyvoice_pack_status(paths);
    if !cosyvoice.installed {
        return Err(EngineError::InstallFailed(format!(
            "offline proof preflight failed: bundled CosyVoice pack is not ready ({})",
            cosyvoice.status_detail
        )));
    }
    let model_id = paths.effective_asr_model_id();
    ModelStore::new(paths.clone()).verify_model_by_id(&model_id)?;
    Ok(model_id)
}

fn run_offline_localization_proof_internal(
    paths: &AppPaths,
    request: OfflineLocalizationProofRequest,
    requested_proof_dir: Option<PathBuf>,
) -> Result<OfflineLocalizationProofSummary> {
    let media_path = std::fs::canonicalize(request.media_path.trim())?;
    if !media_path.is_file() {
        return Err(EngineError::InstallFailed(format!(
            "proof media is not a local file: {}",
            media_path.display()
        )));
    }
    let proof_run_id = request.proof_run_id.trim().to_string();
    if proof_run_id.len() != 32 || !proof_run_id.bytes().all(|value| value.is_ascii_hexdigit()) {
        return Err(EngineError::InstallFailed(
            "offline proof run id must be exactly 32 hexadecimal characters".to_string(),
        ));
    }
    let proof_started_at_ms = request.proof_started_at_ms;
    if proof_started_at_ms <= 0 || proof_started_at_ms > now_ms() {
        return Err(EngineError::InstallFailed(
            "offline proof start timestamp is invalid".to_string(),
        ));
    }
    let canonical_media = media_path.to_string_lossy().to_string();
    let media_sha256 = sha256_file(&media_path)?;
    let asr_lang = request
        .asr_lang
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && value != "auto");
    let _offline_environment = OfflineEnvironmentGuard::apply();
    let asr_model_id = preflight(paths)?;
    jobs::set_runtime_max_concurrency(paths, 1)?;
    let runner = jobs::start_runner(paths.clone())?;

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let import_job = jobs::enqueue_import_local(
            paths,
            media_path.to_string_lossy().to_string(),
            true,
            false,
        )?;
        let import_row = wait_for_job(paths, &import_job.id, Duration::from_secs(20 * 60))?;
        if import_row.created_at_ms < proof_started_at_ms
            || import_row.started_at_ms.is_none()
            || import_row.finished_at_ms.is_none()
        {
            return Err(EngineError::InstallFailed(
                "import job is not bound to the current proof flight".to_string(),
            ));
        }

        let item = library::list_items(paths, 500, 0)?
            .into_iter()
            .find(|item| item.media_path == canonical_media)
            .ok_or_else(|| {
                EngineError::InstallFailed("imported media is missing from the library".to_string())
            })?;

        let first = jobs::enqueue_localization_run_v1(
            paths,
            jobs::LocalizationRunRequest {
                item_id: item.id.clone(),
                asr_lang: asr_lang.clone(),
                separation_backend: Some("demucs".to_string()),
                output_mode: Some("dub".to_string()),
                queue_export_pack: true,
                queue_qc: true,
                speaker_count: jobs::DiarizationSpeakerCountRequest::default(),
            },
        )?;
        let mut proof_batch_ids = vec![first.batch_id.clone()];
        let _ = wait_for_item_jobs_to_settle(paths, &item.id, JOB_TIMEOUT)?;

        let source_track = subtitle_tracks::list_tracks(paths, &item.id)?
            .into_iter()
            .filter(|track| track.kind == "source")
            .max_by_key(|track| track.version)
            .ok_or_else(|| EngineError::InstallFailed("source captions are missing".to_string()))?;
        let translated_track = latest_track(paths, &item.id, "translated", "en")?;
        let speakers = speaker_keys(paths, &translated_track.id)?;
        if speakers.is_empty() {
            return Err(EngineError::InstallFailed(
                "translated captions do not contain diarized speaker labels".to_string(),
            ));
        }

        let candidates = voice_reference_candidates::generate_reference_candidates(
            paths,
            voice_reference_candidates::VoiceReferenceCandidateGenerationRequest {
                item_id: item.id.clone(),
                track_id: Some(translated_track.id.clone()),
                speaker_key: None,
                missing_only: false,
            },
        )?;
        if candidates.bundles.len() != speakers.len() {
            return Err(EngineError::InstallFailed(format!(
                "expected {} voice-reference bundle(s), generated {}",
                speakers.len(),
                candidates.bundles.len()
            )));
        }
        for speaker in &speakers {
            voice_reference_candidates::apply_reference_candidate(
                paths, &item.id, speaker, "replace",
            )?;
        }

        let mut rows = wait_for_item_jobs_to_settle(paths, &item.id, JOB_TIMEOUT)?;
        let required = [
            "asr_local",
            "translate_local",
            "diarize_local_v1",
            "separate_audio_demucs_v1",
            "dub_voice_preserving_v1",
            "mix_dub_preview_v1",
            "mux_dub_preview_v1",
            "export_pack_v1",
        ];
        let complete = |rows: &[jobs::JobRow]| {
            required.iter().all(|required_type| {
                rows.iter().any(|row| {
                    row.job_type == *required_type
                        && matches!(row.status, jobs::JobStatus::Succeeded)
                })
            })
        };
        let mut final_stage = "auto_resume".to_string();
        if !complete(&rows) {
            let final_run = jobs::enqueue_localization_run_v1(
                paths,
                jobs::LocalizationRunRequest {
                    item_id: item.id.clone(),
                    asr_lang: asr_lang.clone(),
                    separation_backend: Some("demucs".to_string()),
                    output_mode: Some("dub".to_string()),
                    queue_export_pack: true,
                    queue_qc: true,
                    speaker_count: jobs::DiarizationSpeakerCountRequest::default(),
                },
            )?;
            if !proof_batch_ids.contains(&final_run.batch_id) {
                proof_batch_ids.push(final_run.batch_id.clone());
            }
            final_stage = final_run.stage;
            rows = wait_for_item_jobs_to_settle(paths, &item.id, JOB_TIMEOUT)?;
        }
        if !complete(&rows) {
            let missing = required
                .iter()
                .filter(|required_type| {
                    !rows.iter().any(|row| {
                        row.job_type == **required_type
                            && matches!(row.status, jobs::JobStatus::Succeeded)
                    })
                })
                .copied()
                .collect::<Vec<_>>();
            return Err(EngineError::InstallFailed(format!(
                "offline localization proof is missing successful stages: {}",
                missing.join(", ")
            )));
        }
        rows.retain(|row| {
            row.created_at_ms >= proof_started_at_ms
                && row
                    .batch_id
                    .as_ref()
                    .is_some_and(|batch_id| proof_batch_ids.contains(batch_id))
        });
        if !complete(&rows) {
            return Err(EngineError::InstallFailed(
                "offline localization proof stages are not bound to current proof-owned batches"
                    .to_string(),
            ));
        }

        let item_dir = paths.derived_item_dir(&item.id);
        let mix_path = item_dir.join("dub_preview").join("mix_dub_preview_v1.wav");
        if !wav_is_non_silent(&mix_path)? {
            return Err(EngineError::InstallFailed(
                "mixed dub output is silent".to_string(),
            ));
        }
        let mux_path = newest_mkv(&item_dir.join("dub_preview"))?;
        let export_path = item_dir.join("exports").join("export_pack_v1.zip");
        let mut selected_pipeline_jobs = Vec::new();
        for required_type in required {
            let row = rows
                .iter()
                .filter(|row| {
                    row.job_type == required_type
                        && matches!(row.status, jobs::JobStatus::Succeeded)
                })
                .max_by_key(|row| row.finished_at_ms.unwrap_or(row.created_at_ms))
                .cloned()
                .ok_or_else(|| {
                    EngineError::InstallFailed(format!(
                        "proof-owned successful job is missing: {required_type}"
                    ))
                })?;
            if row.created_at_ms < proof_started_at_ms
                || row.started_at_ms.is_none()
                || row.finished_at_ms.is_none()
            {
                return Err(EngineError::InstallFailed(format!(
                    "proof job lacks current-flight timestamps: {}",
                    row.id
                )));
            }
            selected_pipeline_jobs.push(row);
        }
        let selected_job = |job_type: &str| -> Result<&jobs::JobRow> {
            selected_pipeline_jobs
                .iter()
                .find(|row| row.job_type == job_type)
                .ok_or_else(|| {
                    EngineError::InstallFailed(format!("selected proof job is missing: {job_type}"))
                })
        };
        let voice_job = selected_job("dub_voice_preserving_v1")?;
        let voice_report_path = paths
            .job_artifacts_dir(&voice_job.id)
            .join("tts_voice_preserving_report.json");

        let proof_dir = requested_proof_dir.clone().unwrap_or_else(|| {
            paths
                .base_dir
                .join("diagnostics")
                .join("offline_localization_proof")
                .join(format!("run_{}", now_ms()))
        });
        if !proof_dir.is_absolute() || !proof_dir.starts_with(&paths.base_dir) {
            return Err(EngineError::InstallFailed(format!(
                "offline proof output must stay inside the isolated app-data root: {}",
                proof_dir.display()
            )));
        }
        if !proof_dir.is_dir() {
            return Err(EngineError::InstallFailed(format!(
                "owned proof output directory was not prepared: {}",
                proof_dir.display()
            )));
        }
        let copied_mix = proof_dir.join("localized_dub.wav");
        let copied_mux = proof_dir.join("localized_dub.mkv");
        let copied_export = proof_dir.join("localization_export.zip");
        let copied_voice_report = proof_dir.join("voice_report.json");
        let mix_artifact = publish_proof_artifact_atomic(
            &mix_path,
            &copied_mix,
            &proof_run_id,
            &selected_job("mix_dub_preview_v1")?.id,
            |path| {
                if wav_is_non_silent(path)? {
                    Ok(())
                } else {
                    Err(EngineError::InstallFailed(
                        "published proof mix is silent".to_string(),
                    ))
                }
            },
        )?;
        let mux_artifact = publish_proof_artifact_atomic(
            &mux_path,
            &copied_mux,
            &proof_run_id,
            &selected_job("mux_dub_preview_v1")?.id,
            |path| validate_proof_mkv(paths, path),
        )?;
        let export_artifact = publish_proof_artifact_atomic(
            &export_path,
            &copied_export,
            &proof_run_id,
            &selected_job("export_pack_v1")?.id,
            validate_proof_export_zip,
        )?;
        let source_voice_report: serde_json::Value =
            serde_json::from_reader(File::open(&voice_report_path)?)?;
        let voice_params: serde_json::Value = serde_json::from_str(&voice_job.params_json)?;
        let backend_id = voice_params
            .pointer("/pipeline/tts_backend_id")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                EngineError::InstallFailed(
                    "voice proof job lacks its governed backend identity".to_string(),
                )
            })?;
        let voice_clone_outcome = source_voice_report
            .get("voice_clone_outcome")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                EngineError::InstallFailed(
                    "voice proof report lacks its governed outcome".to_string(),
                )
            })?;
        let voice_pack = tools::tts_voice_preserving_local_v1_pack_status(paths);
        if !voice_pack.installed
            || voice_pack
                .expected_lockfile_sha
                .as_deref()
                .unwrap_or_default()
                .is_empty()
            || voice_pack
                .installed_lockfile_sha
                .as_deref()
                .unwrap_or_default()
                .is_empty()
        {
            return Err(EngineError::InstallFailed(
                "voice proof pack identity is incomplete".to_string(),
            ));
        }
        let governed_voice_report = serde_json::json!({
            "schema_version": 2,
            "outcome": "succeeded",
            "proof_run_id": proof_run_id,
            "producer_job_id": voice_job.id,
            "backend_id": backend_id,
            "voice_clone_outcome": voice_clone_outcome,
            "model_identity": {
                "kokoro_version": voice_pack.kokoro_version,
                "openvoice_version": voice_pack.openvoice_version,
                "cosyvoice_version": voice_pack.cosyvoice_version,
                "openvoice_models_dir": voice_pack.openvoice_models_dir,
                "openvoice_models_installed": voice_pack.openvoice_models_installed
            },
            "cache_identity": {
                "expected_lockfile_sha256": voice_pack.expected_lockfile_sha,
                "installed_lockfile_sha256": voice_pack.installed_lockfile_sha,
                "huggingface_cache_dir": paths.huggingface_cache_dir().to_string_lossy()
            },
            "source_report_sha256": sha256_file(&voice_report_path)?,
            "source_report": source_voice_report
        });
        let voice_artifact = publish_proof_json_atomic(
            &governed_voice_report,
            &copied_voice_report,
            &proof_run_id,
            &voice_job.id,
        )?;
        let summary_path = proof_dir.join("proof_summary.json");

        let jobs = std::iter::once((&import_row, true))
            .chain(selected_pipeline_jobs.iter().map(|row| (row, false)))
            .map(|(row, is_import)| OfflineLocalizationProofJob {
                id: row.id.clone(),
                job_type: row.job_type.clone(),
                status: format!("{:?}", row.status).to_ascii_lowercase(),
                batch_id: row.batch_id.clone(),
                created_at_ms: row.created_at_ms,
                started_at_ms: row.started_at_ms,
                finished_at_ms: row.finished_at_ms,
                input_media_path: is_import.then(|| canonical_media.clone()),
                input_media_sha256: is_import.then(|| media_sha256.clone()),
            })
            .collect::<Vec<_>>();
        let required_job_types = std::iter::once("import_local".to_string())
            .chain(required.iter().map(|value| (*value).to_string()))
            .collect::<Vec<_>>();
        proof_batch_ids.sort();
        proof_batch_ids.dedup();
        let summary = OfflineLocalizationProofSummary {
            schema_version: 2,
            outcome: "succeeded".to_string(),
            generated_at_ms: now_ms(),
            proof_run_id: proof_run_id.clone(),
            proof_started_at_ms,
            proof_batch_ids,
            engine_version: crate::diagnostics::engine_version().to_string(),
            app_base_dir: paths.base_dir.to_string_lossy().to_string(),
            media_path: media_path.to_string_lossy().to_string(),
            media_sha256: media_sha256.clone(),
            item_id: item.id,
            asr_model_id,
            asr_lang,
            first_stage: first.stage,
            final_stage,
            source_track_id: source_track.id,
            translated_track_id: translated_track.id,
            speaker_keys: speakers,
            network_policy: vec![
                "HF_HUB_OFFLINE=1".to_string(),
                "TRANSFORMERS_OFFLINE=1".to_string(),
                "HF_DATASETS_OFFLINE=1".to_string(),
                "PIP_NO_INDEX=1".to_string(),
                "remote HTTP/HTTPS/ALL proxy forced to closed loopback port".to_string(),
            ],
            required_job_types,
            jobs,
            mux: mux_artifact,
            mix: mix_artifact,
            export_pack: export_artifact,
            voice_report: voice_artifact,
            proof_summary_path: summary_path.to_string_lossy().to_string(),
        };
        Ok(summary)
    }));

    let shutdown = runner.stop_and_join(RUNNER_SHUTDOWN_TIMEOUT);
    match (result, shutdown) {
        (Ok(Ok(summary)), Ok(report)) if report.panic_count() == 0 => {
            let summary_value = serde_json::to_value(&summary)?;
            publish_proof_json_atomic(
                &summary_value,
                std::path::Path::new(&summary.proof_summary_path),
                &summary.proof_run_id,
                &summary.proof_run_id,
            )?;
            Ok(summary)
        }
        (Ok(Ok(_)), Ok(report)) => Err(EngineError::InstallFailed(format!(
            "offline proof runner shut down with {} panic(s)",
            report.panic_count()
        ))),
        (Ok(Ok(_)), Err(error)) => Err(error),
        (Ok(Err(error)), _) => Err(error),
        (Err(_), _) => Err(EngineError::InstallFailed(
            "offline localization proof panicked; its owned runner was stopped".to_string(),
        )),
    }
}

pub fn run_offline_localization_proof(
    paths: &AppPaths,
    request: OfflineLocalizationProofRequest,
) -> Result<OfflineLocalizationProofSummary> {
    run_offline_localization_proof_internal(paths, request, None)
}

pub fn run_offline_localization_proof_at(
    paths: &AppPaths,
    request: OfflineLocalizationProofRequest,
    proof_dir: PathBuf,
) -> Result<OfflineLocalizationProofSummary> {
    run_offline_localization_proof_internal(paths, request, Some(proof_dir))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifact_rejects_empty_file_and_hashes_nonempty_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let empty = dir.path().join("empty.bin");
        std::fs::write(&empty, []).expect("write empty");
        assert!(artifact(&empty, "job-empty").is_err());
        let full = dir.path().join("full.bin");
        std::fs::write(&full, b"voxvulgi").expect("write full");
        let observed = artifact(&full, "job-full").expect("artifact");
        assert_eq!(observed.bytes, 8);
        assert_eq!(observed.sha256.len(), 64);
    }

    #[test]
    fn newest_mkv_never_accepts_mp4() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("preview.mp4"), b"mp4").expect("write mp4");
        assert!(newest_mkv(dir.path()).is_err());
        let nested = dir.path().join("nested");
        std::fs::create_dir_all(&nested).expect("nested");
        let mkv = nested.join("preview.mkv");
        std::fs::write(&mkv, b"mkv").expect("write mkv");
        assert_eq!(newest_mkv(dir.path()).expect("mkv"), mkv);
    }

    #[test]
    fn atomic_artifact_validation_failure_never_publishes_final_or_success_receipts() {
        let dir = tempfile::tempdir().expect("tempdir");
        let source = dir.path().join("source.bin");
        let final_path = dir.path().join("localized_dub.mkv");
        std::fs::write(&source, b"corrupt-nonempty-mux").expect("source");
        let run_id = "a".repeat(32);
        let result =
            publish_proof_artifact_atomic(&source, &final_path, &run_id, "mux-job", |_| {
                Err(EngineError::InstallFailed(
                    "deterministic semantic rejection".to_string(),
                ))
            });
        assert!(result.is_err());
        assert!(!final_path.exists());
        assert!(!dir.path().join("proof_summary.json").exists());
        assert!(!dir.path().join("terminal_status.json").exists());
        assert!(dir
            .path()
            .join(format!(".{run_id}.localized_dub.mkv.tmp"))
            .is_file());
    }

    #[test]
    fn atomic_json_publication_is_create_new_and_producer_bound() {
        let dir = tempfile::tempdir().expect("tempdir");
        let final_path = dir.path().join("proof_summary.json");
        let run_id = "b".repeat(32);
        let artifact = publish_proof_json_atomic(
            &serde_json::json!({"schema_version": 2, "outcome": "succeeded"}),
            &final_path,
            &run_id,
            &run_id,
        )
        .expect("publish JSON");
        assert_eq!(artifact.producer_job_id, run_id);
        let before = std::fs::read(&final_path).expect("summary bytes");
        assert!(publish_proof_json_atomic(
            &serde_json::json!({"schema_version": 2, "outcome": "forged"}),
            &final_path,
            &run_id,
            &run_id,
        )
        .is_err());
        assert_eq!(std::fs::read(&final_path).expect("summary after"), before);
    }
}
