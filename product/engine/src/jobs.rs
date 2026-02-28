use crate::paths::AppPaths;
use crate::{
    asr, cmd, config, db, ffmpeg, image_batch, library, subtitle_tracks, subtitles, tools,
    translate,
    speakers, EngineError, Result,
    subscriptions,
};
use regex::Regex;
use rusqlite::params;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use url::Url;
use uuid::Uuid;

const DEFAULT_MAX_CONCURRENT_JOBS: usize = 4;
const MAX_MAX_CONCURRENT_JOBS: usize = 16;
const JOB_LOG_ROTATE_BYTES: u64 = 50 * 1024 * 1024;
const JOB_LOG_MAX_BACKUPS: usize = 3;
const JOB_LOG_MAX_AGE_DAYS: u64 = 30;
const JOB_LOG_TOTAL_CAP_BYTES: u64 = 1 * 1024 * 1024 * 1024;
const MAX_DOWNLOAD_BATCH_URLS: usize = 1500;
const DOWNLOAD_PROVIDER_DIRECT_HTTP: &str = "direct_http_v1";
const DOWNLOAD_PROVIDER_YOUTUBE_YT_DLP: &str = "youtube_yt_dlp_v1";
const DOWNLOAD_RIGHTS_NOTE_UNSPECIFIED: &str = "not_collected";
const DEFAULT_VIDEO_OUTPUT_SUBDIR: &str = "video";
const DEFAULT_INSTAGRAM_OUTPUT_SUBDIR: &str = "instagram";
const DEFAULT_IMAGES_OUTPUT_SUBDIR: &str = "images";
const EMBED_CRAWL_MAX_PAGES: usize = 8;
const EMBED_CRAWL_MAX_CANDIDATES: usize = 40;
const EMBED_FETCH_MAX_BODY_BYTES: u64 = 2 * 1024 * 1024;
const DIRECT_DOWNLOAD_SNIFF_BYTES: usize = 8192;
const INSTAGRAM_API_APP_ID: &str = "936619743392459";
const DEFAULT_HTTP_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36";
const META_KEY_JOBS_QUEUE_PAUSED: &str = "jobs_queue_paused";
const META_KEY_JOBS_MAX_CONCURRENCY: &str = "jobs_max_concurrency";
const YT_DLP_EXPAND_TIMEOUT_SECS: u64 = 900;
const YT_DLP_DOWNLOAD_TIMEOUT_SECS: u64 = 7200;
const EXTERNAL_CMD_POLL_INTERVAL_MS: u64 = 200;
const YT_DLP_BOOTSTRAP_TIMEOUT_SECS: u64 = 180;
#[cfg(windows)]
const YT_DLP_WINDOWS_DOWNLOAD_URL: &str =
    "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe";

static YT_DLP_BOOTSTRAP_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Debug, Clone, Serialize)]
pub struct JobLogRetentionPolicy {
    pub rotate_bytes: u64,
    pub max_backups: usize,
    pub max_age_days: u64,
    pub total_cap_bytes: u64,
}

pub fn job_log_retention_policy() -> JobLogRetentionPolicy {
    JobLogRetentionPolicy {
        rotate_bytes: JOB_LOG_ROTATE_BYTES,
        max_backups: JOB_LOG_MAX_BACKUPS,
        max_age_days: JOB_LOG_MAX_AGE_DAYS,
        total_cap_bytes: JOB_LOG_TOTAL_CAP_BYTES,
    }
}

pub fn prune_job_logs_now(paths: &AppPaths) -> Result<()> {
    prune_job_logs(paths)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Canceled,
}

impl JobStatus {
    fn as_str(&self) -> &'static str {
        match self {
            JobStatus::Queued => "queued",
            JobStatus::Running => "running",
            JobStatus::Succeeded => "succeeded",
            JobStatus::Failed => "failed",
            JobStatus::Canceled => "canceled",
        }
    }

    fn from_str(value: &str) -> Option<Self> {
        match value {
            "queued" => Some(JobStatus::Queued),
            "running" => Some(JobStatus::Running),
            "succeeded" => Some(JobStatus::Succeeded),
            "failed" => Some(JobStatus::Failed),
            "canceled" => Some(JobStatus::Canceled),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobType {
    ImportLocal,
    DownloadDirectUrl,
    YoutubeSubscriptionRefreshV1,
    DownloadImageBatch,
    AsrLocal,
    TranslateLocal,
    DiarizeLocalV1,
    DubVoicePreservingV1,
    TtsPreviewPyttsx3V1,
    TtsNeuralLocalV1,
    MixDubPreviewV1,
    MuxDubPreviewV1,
    SeparateAudioSpleeter,
    SeparateAudioDemucsV1,
    CleanVocalsV1,
    QcReportV1,
    ExportPackV1,
    InstallPhase2PacksV1,
    DummySleep,
}

impl JobType {
    fn as_str(&self) -> &'static str {
        match self {
            JobType::ImportLocal => "import_local",
            JobType::DownloadDirectUrl => "download_direct_url",
            JobType::YoutubeSubscriptionRefreshV1 => "youtube_subscription_refresh_v1",
            JobType::DownloadImageBatch => "download_image_batch",
            JobType::AsrLocal => "asr_local",
            JobType::TranslateLocal => "translate_local",
            JobType::DiarizeLocalV1 => "diarize_local_v1",
            JobType::DubVoicePreservingV1 => "dub_voice_preserving_v1",
            JobType::TtsPreviewPyttsx3V1 => "tts_preview_pyttsx3_v1",
            JobType::TtsNeuralLocalV1 => "tts_neural_local_v1",
            JobType::MixDubPreviewV1 => "mix_dub_preview_v1",
            JobType::MuxDubPreviewV1 => "mux_dub_preview_v1",
            JobType::SeparateAudioSpleeter => "separate_audio_spleeter",
            JobType::SeparateAudioDemucsV1 => "separate_audio_demucs_v1",
            JobType::CleanVocalsV1 => "clean_vocals_v1",
            JobType::QcReportV1 => "qc_report_v1",
            JobType::ExportPackV1 => "export_pack_v1",
            JobType::InstallPhase2PacksV1 => "install_phase2_packs_v1",
            JobType::DummySleep => "dummy_sleep",
        }
    }

    fn from_str(value: &str) -> Option<Self> {
        match value {
            "import_local" => Some(JobType::ImportLocal),
            "download_direct_url" => Some(JobType::DownloadDirectUrl),
            "youtube_subscription_refresh_v1" => Some(JobType::YoutubeSubscriptionRefreshV1),
            "download_image_batch" => Some(JobType::DownloadImageBatch),
            "asr_local" => Some(JobType::AsrLocal),
            "translate_local" => Some(JobType::TranslateLocal),
            "diarize_local_v1" => Some(JobType::DiarizeLocalV1),
            "dub_voice_preserving_v1" => Some(JobType::DubVoicePreservingV1),
            "tts_preview_pyttsx3_v1" => Some(JobType::TtsPreviewPyttsx3V1),
            "tts_neural_local_v1" => Some(JobType::TtsNeuralLocalV1),
            "mix_dub_preview_v1" => Some(JobType::MixDubPreviewV1),
            "mux_dub_preview_v1" => Some(JobType::MuxDubPreviewV1),
            "separate_audio_spleeter" => Some(JobType::SeparateAudioSpleeter),
            "separate_audio_demucs_v1" => Some(JobType::SeparateAudioDemucsV1),
            "clean_vocals_v1" => Some(JobType::CleanVocalsV1),
            "qc_report_v1" => Some(JobType::QcReportV1),
            "export_pack_v1" => Some(JobType::ExportPackV1),
            "install_phase2_packs_v1" => Some(JobType::InstallPhase2PacksV1),
            "dummy_sleep" => Some(JobType::DummySleep),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRow {
    pub id: String,
    pub item_id: Option<String>,
    pub batch_id: Option<String>,
    pub job_type: String,
    pub status: JobStatus,
    pub progress: f32,
    pub error: Option<String>,
    pub created_at_ms: i64,
    pub started_at_ms: Option<i64>,
    pub finished_at_ms: Option<i64>,
    pub logs_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobQueueControlState {
    pub paused: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobFlushSummary {
    pub removed_jobs: usize,
    pub removed_log_files: usize,
    pub removed_artifact_dirs: usize,
    pub removed_output_dirs: usize,
    pub removed_cache_entries: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ImportLocalParams {
    path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct InstallPhase2PacksV1Params {}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AsrLocalParams {
    item_id: String,
    lang: Option<String>,
    model_id: String,
    #[serde(default)]
    batch_on_import: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TranslateLocalParams {
    item_id: String,
    source_track_id: String,
    model_id: String,
    #[serde(default)]
    batch_on_import: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DiarizeLocalV1Params {
    item_id: String,
    source_track_id: String,
    #[serde(default)]
    backend: Option<String>,
    #[serde(default)]
    batch_on_import: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TtsPreviewPyttsx3V1Params {
    item_id: String,
    source_track_id: String,
    #[serde(default)]
    batch_on_import: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TtsNeuralLocalV1Params {
    item_id: String,
    source_track_id: String,
    #[serde(default)]
    batch_on_import: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DubVoicePreservingV1Params {
    item_id: String,
    source_track_id: String,
    #[serde(default)]
    batch_on_import: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MixDubPreviewV1Params {
    item_id: String,
    #[serde(default)]
    ducking_strength: Option<f32>,
    #[serde(default)]
    loudness_target_lufs: Option<f32>,
    #[serde(default)]
    timing_fit_enabled: Option<bool>,
    #[serde(default)]
    timing_fit_min_factor: Option<f32>,
    #[serde(default)]
    timing_fit_max_factor: Option<f32>,
    #[serde(default)]
    batch_on_import: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MuxDubPreviewV1Params {
    item_id: String,
    #[serde(default)]
    output_container: Option<String>,
    #[serde(default)]
    keep_original_audio: Option<bool>,
    #[serde(default)]
    dubbed_audio_lang: Option<String>,
    #[serde(default)]
    original_audio_lang: Option<String>,
    #[serde(default)]
    batch_on_import: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SeparateAudioSpleeterParams {
    item_id: String,
    #[serde(default)]
    batch_on_import: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SeparateAudioDemucsV1Params {
    item_id: String,
    #[serde(default)]
    batch_on_import: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CleanVocalsV1Params {
    item_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct QcReportV1Params {
    item_id: String,
    track_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExportPackV1Params {
    item_id: String,
}

#[derive(Debug, Clone, Deserialize)]
struct DiarizeLocalV1Output {
    schema_version: Option<u32>,
    segments: Vec<DiarizeLocalV1Segment>,
}

#[derive(Debug, Clone, Deserialize)]
struct DiarizeLocalV1Segment {
    start_ms: i64,
    end_ms: i64,
    speaker: String,
}

#[derive(Debug, Clone, Deserialize)]
struct TtsPreviewManifest {
    segments: Vec<TtsPreviewManifestSegment>,
}

#[derive(Debug, Clone, Deserialize)]
struct TtsPreviewManifestSegment {
    index: u32,
    start_ms: i64,
    end_ms: i64,
    #[serde(default)]
    audio_path: Option<String>,
    #[serde(default)]
    audio_exists: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DummySleepParams {
    seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DownloadDirectUrlParams {
    url: String,
    #[serde(default)]
    provider: String,
    #[serde(default, skip_serializing)]
    auth_cookie: Option<String>,
    #[serde(default)]
    output_subdir: Option<String>,
    #[serde(default)]
    output_dir: Option<String>,
    #[serde(default)]
    use_browser_cookies: bool,
    #[serde(default)]
    subscription_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct YoutubeSubscriptionRefreshV1Params {
    subscription_id: String,
    #[serde(default)]
    max_items: Option<usize>,
    #[serde(default)]
    output_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DownloadImageBatchParams {
    start_urls: Vec<String>,
    max_pages: usize,
    delay_ms: u64,
    allow_cross_domain: bool,
    follow_content_links: bool,
    skip_url_keywords: Vec<String>,
    output_subdir: String,
    #[serde(default)]
    output_dir: Option<String>,
    #[serde(default, skip_serializing)]
    auth_cookie: Option<String>,
}

#[derive(Debug, Clone)]
struct DownloadTarget {
    url: String,
    provider: &'static str,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRuntimeSettings {
    pub max_concurrency: usize,
}

pub fn enqueue_import_local(paths: &AppPaths, path: String) -> Result<JobRow> {
    let params_json = serde_json::to_string(&ImportLocalParams { path })?;
    enqueue(paths, JobType::ImportLocal, params_json)
}

pub fn enqueue_install_phase2_packs_v1(paths: &AppPaths) -> Result<JobRow> {
    let params_json = serde_json::to_string(&InstallPhase2PacksV1Params::default())?;
    enqueue(paths, JobType::InstallPhase2PacksV1, params_json)
}

pub fn enqueue_dummy_sleep(paths: &AppPaths, seconds: u64) -> Result<JobRow> {
    let seconds = seconds.clamp(1, 600);
    let params_json = serde_json::to_string(&DummySleepParams { seconds })?;
    enqueue(paths, JobType::DummySleep, params_json)
}

pub fn enqueue_asr_local(
    paths: &AppPaths,
    item_id: String,
    lang: Option<String>,
) -> Result<JobRow> {
    let lang = match lang {
        Some(v) => {
            let v = v.trim().to_string();
            if v.is_empty() || v == "auto" {
                None
            } else {
                Some(v)
            }
        }
        None => None,
    };

    let model_id = "whispercpp-tiny".to_string();
    let params_json = serde_json::to_string(&AsrLocalParams {
        item_id: item_id.clone(),
        lang,
        model_id,
        batch_on_import: false,
    })?;

    enqueue_with_type_and_item_id(paths, JobType::AsrLocal, params_json, Some(item_id))
}

pub fn enqueue_translate_local(
    paths: &AppPaths,
    item_id: String,
    source_track_id: String,
) -> Result<JobRow> {
    let model_id = "whispercpp-tiny".to_string();
    let params_json = serde_json::to_string(&TranslateLocalParams {
        item_id: item_id.clone(),
        source_track_id,
        model_id,
        batch_on_import: false,
    })?;

    enqueue_with_type_and_item_id(paths, JobType::TranslateLocal, params_json, Some(item_id))
}

pub fn enqueue_diarize_local_v1(
    paths: &AppPaths,
    item_id: String,
    source_track_id: String,
) -> Result<JobRow> {
    let params_json = serde_json::to_string(&DiarizeLocalV1Params {
        item_id: item_id.clone(),
        source_track_id,
        backend: None,
        batch_on_import: false,
    })?;

    enqueue_with_type_and_item_id(paths, JobType::DiarizeLocalV1, params_json, Some(item_id))
}

pub fn enqueue_diarize_local_v1_with_backend(
    paths: &AppPaths,
    item_id: String,
    source_track_id: String,
    backend: Option<String>,
) -> Result<JobRow> {
    let backend = backend
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let params_json = serde_json::to_string(&DiarizeLocalV1Params {
        item_id: item_id.clone(),
        source_track_id,
        backend,
        batch_on_import: false,
    })?;

    enqueue_with_type_and_item_id(paths, JobType::DiarizeLocalV1, params_json, Some(item_id))
}

pub fn enqueue_tts_preview_pyttsx3_v1(
    paths: &AppPaths,
    item_id: String,
    source_track_id: String,
) -> Result<JobRow> {
    let params_json = serde_json::to_string(&TtsPreviewPyttsx3V1Params {
        item_id: item_id.clone(),
        source_track_id,
        batch_on_import: false,
    })?;
    enqueue_with_type_and_item_id(
        paths,
        JobType::TtsPreviewPyttsx3V1,
        params_json,
        Some(item_id),
    )
}

pub fn enqueue_tts_neural_local_v1(
    paths: &AppPaths,
    item_id: String,
    source_track_id: String,
) -> Result<JobRow> {
    let params_json = serde_json::to_string(&TtsNeuralLocalV1Params {
        item_id: item_id.clone(),
        source_track_id,
        batch_on_import: false,
    })?;
    enqueue_with_type_and_item_id(
        paths,
        JobType::TtsNeuralLocalV1,
        params_json,
        Some(item_id),
    )
}

pub fn enqueue_dub_voice_preserving_v1(
    paths: &AppPaths,
    item_id: String,
    source_track_id: String,
) -> Result<JobRow> {
    let params_json = serde_json::to_string(&DubVoicePreservingV1Params {
        item_id: item_id.clone(),
        source_track_id,
        batch_on_import: false,
    })?;
    enqueue_with_type_and_item_id(
        paths,
        JobType::DubVoicePreservingV1,
        params_json,
        Some(item_id),
    )
}

pub fn enqueue_mix_dub_preview_v1(paths: &AppPaths, item_id: String) -> Result<JobRow> {
    let params_json =
        serde_json::to_string(&MixDubPreviewV1Params {
            item_id: item_id.clone(),
            ducking_strength: None,
            loudness_target_lufs: None,
            timing_fit_enabled: None,
            timing_fit_min_factor: None,
            timing_fit_max_factor: None,
            batch_on_import: false,
        })?;
    enqueue_with_type_and_item_id(paths, JobType::MixDubPreviewV1, params_json, Some(item_id))
}

pub fn enqueue_mix_dub_preview_v1_with_options(
    paths: &AppPaths,
    item_id: String,
    ducking_strength: Option<f32>,
    loudness_target_lufs: Option<f32>,
    timing_fit_enabled: Option<bool>,
    timing_fit_min_factor: Option<f32>,
    timing_fit_max_factor: Option<f32>,
) -> Result<JobRow> {
    let params_json = serde_json::to_string(&MixDubPreviewV1Params {
        item_id: item_id.clone(),
        ducking_strength,
        loudness_target_lufs,
        timing_fit_enabled,
        timing_fit_min_factor,
        timing_fit_max_factor,
        batch_on_import: false,
    })?;
    enqueue_with_type_and_item_id(paths, JobType::MixDubPreviewV1, params_json, Some(item_id))
}

pub fn enqueue_mux_dub_preview_v1(paths: &AppPaths, item_id: String) -> Result<JobRow> {
    let params_json =
        serde_json::to_string(&MuxDubPreviewV1Params {
            item_id: item_id.clone(),
            output_container: None,
            keep_original_audio: None,
            dubbed_audio_lang: None,
            original_audio_lang: None,
            batch_on_import: false,
        })?;
    enqueue_with_type_and_item_id(paths, JobType::MuxDubPreviewV1, params_json, Some(item_id))
}

pub fn enqueue_mux_dub_preview_v1_with_options(
    paths: &AppPaths,
    item_id: String,
    output_container: Option<String>,
    keep_original_audio: Option<bool>,
    dubbed_audio_lang: Option<String>,
    original_audio_lang: Option<String>,
) -> Result<JobRow> {
    let params_json = serde_json::to_string(&MuxDubPreviewV1Params {
        item_id: item_id.clone(),
        output_container,
        keep_original_audio,
        dubbed_audio_lang,
        original_audio_lang,
        batch_on_import: false,
    })?;
    enqueue_with_type_and_item_id(paths, JobType::MuxDubPreviewV1, params_json, Some(item_id))
}

pub fn enqueue_separate_audio_spleeter(paths: &AppPaths, item_id: String) -> Result<JobRow> {
    let params_json =
        serde_json::to_string(&SeparateAudioSpleeterParams {
            item_id: item_id.clone(),
            batch_on_import: false,
        })?;
    enqueue_with_type_and_item_id(
        paths,
        JobType::SeparateAudioSpleeter,
        params_json,
        Some(item_id),
    )
}

pub fn enqueue_separate_audio_demucs_v1(paths: &AppPaths, item_id: String) -> Result<JobRow> {
    let params_json = serde_json::to_string(&SeparateAudioDemucsV1Params {
        item_id: item_id.clone(),
        batch_on_import: false,
    })?;
    enqueue_with_type_and_item_id(
        paths,
        JobType::SeparateAudioDemucsV1,
        params_json,
        Some(item_id),
    )
}

pub fn enqueue_clean_vocals_v1(paths: &AppPaths, item_id: String) -> Result<JobRow> {
    let params_json = serde_json::to_string(&CleanVocalsV1Params {
        item_id: item_id.clone(),
    })?;
    enqueue_with_type_and_item_id(paths, JobType::CleanVocalsV1, params_json, Some(item_id))
}

pub fn enqueue_qc_report_v1(
    paths: &AppPaths,
    item_id: String,
    track_id: String,
) -> Result<JobRow> {
    let params_json = serde_json::to_string(&QcReportV1Params {
        item_id: item_id.clone(),
        track_id,
    })?;
    enqueue_with_type_and_item_id(paths, JobType::QcReportV1, params_json, Some(item_id))
}

pub fn enqueue_export_pack_v1(paths: &AppPaths, item_id: String) -> Result<JobRow> {
    let params_json = serde_json::to_string(&ExportPackV1Params {
        item_id: item_id.clone(),
    })?;
    enqueue_with_type_and_item_id(paths, JobType::ExportPackV1, params_json, Some(item_id))
}

pub fn enqueue_download_direct_url_batch(
    paths: &AppPaths,
    urls: Vec<String>,
    auth_cookie: Option<String>,
    output_dir: Option<String>,
    use_browser_cookies: Option<bool>,
) -> Result<Vec<JobRow>> {
    enqueue_download_direct_url_batch_raw(
        paths,
        urls,
        Some(DOWNLOAD_PROVIDER_DIRECT_HTTP.to_string()),
        auth_cookie,
        output_dir,
        use_browser_cookies,
        None,
    )
}

pub fn enqueue_download_direct_url_batch_raw(
    paths: &AppPaths,
    urls: Vec<String>,
    provider_hint: Option<String>,
    auth_cookie: Option<String>,
    output_dir: Option<String>,
    use_browser_cookies: Option<bool>,
    batch_id: Option<String>,
) -> Result<Vec<JobRow>> {
    enqueue_download_direct_url_batch_raw_with_subscription(
        paths,
        urls,
        provider_hint,
        auth_cookie,
        output_dir,
        use_browser_cookies,
        batch_id,
        None,
    )
}

pub fn enqueue_youtube_subscription_refresh_v1(
    paths: &AppPaths,
    subscription_id: String,
    output_dir: Option<String>,
    batch_id: Option<String>,
) -> Result<JobRow> {
    let trimmed = subscription_id.trim();
    if trimmed.is_empty() {
        return Err(EngineError::InstallFailed(
            "subscription id is empty".to_string(),
        ));
    }
    let output_dir = normalize_output_dir(output_dir);
    let params_json = serde_json::to_string(&YoutubeSubscriptionRefreshV1Params {
        subscription_id: trimmed.to_string(),
        max_items: None,
        output_dir,
    })?;
    enqueue_with_type_item_and_batch_id(
        paths,
        JobType::YoutubeSubscriptionRefreshV1,
        params_json,
        None,
        batch_id.or_else(|| Some(Uuid::new_v4().to_string())),
    )
}

fn enqueue_download_direct_url_batch_raw_with_subscription(
    paths: &AppPaths,
    urls: Vec<String>,
    provider_hint: Option<String>,
    auth_cookie: Option<String>,
    output_dir: Option<String>,
    use_browser_cookies: Option<bool>,
    batch_id: Option<String>,
    subscription_id: Option<String>,
) -> Result<Vec<JobRow>> {
    let auth_cookie = normalize_auth_cookie(auth_cookie);
    let output_dir = normalize_output_dir(output_dir);
    let use_browser_cookies = use_browser_cookies.unwrap_or(false);
    let urls = normalize_direct_urls(urls)?;
    if urls.is_empty() {
        return Err(EngineError::InstallFailed(
            "provide at least one valid http(s) URL".to_string(),
        ));
    }
    if urls.len() > MAX_DOWNLOAD_BATCH_URLS {
        return Err(EngineError::InstallFailed(format!(
            "too many URLs in one batch: {} (max {})",
            urls.len(),
            MAX_DOWNLOAD_BATCH_URLS
        )));
    }

    let provider_hint = provider_hint
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| DOWNLOAD_PROVIDER_DIRECT_HTTP.to_string());
    let batch_id = batch_id.or_else(|| Some(Uuid::new_v4().to_string()));
    let mut jobs: Vec<JobRow> = Vec::with_capacity(urls.len());
    for url in urls {
        let provider = effective_download_provider(&provider_hint, &url);
        let params_json = serde_json::to_string(&DownloadDirectUrlParams {
            url,
            provider: provider.to_string(),
            auth_cookie: None,
            output_subdir: None,
            output_dir: output_dir.clone(),
            use_browser_cookies,
            subscription_id: subscription_id.clone(),
        })?;
        let job = enqueue_with_type_item_and_batch_id(
            paths,
            JobType::DownloadDirectUrl,
            params_json,
            None,
            batch_id.clone(),
        )?;

        if let Some(cookie) = auth_cookie.as_deref() {
            if let Err(err) = write_job_cookie_secret(paths, &job.id, cookie) {
                let _ = delete_job_by_id(paths, &job.id);
                for queued in &jobs {
                    let _ = delete_job_by_id(paths, &queued.id);
                    let _ = remove_job_cookie_secret(paths, &queued.id);
                }
                return Err(err);
            }
        }
        jobs.push(job);
    }

    Ok(jobs)
}

pub fn enqueue_download_instagram_batch(
    paths: &AppPaths,
    urls: Vec<String>,
    auth_cookie: Option<String>,
    output_dir: Option<String>,
    use_browser_cookies: Option<bool>,
) -> Result<Vec<JobRow>> {
    let auth_cookie = normalize_auth_cookie(auth_cookie);
    let output_dir = normalize_output_dir(output_dir);
    let use_browser_cookies = use_browser_cookies.unwrap_or(false);
    let normalized_urls = normalize_direct_urls(urls)?;
    if normalized_urls.is_empty() {
        return Err(EngineError::InstallFailed(
            "provide at least one valid instagram URL".to_string(),
        ));
    }

    if let Some(non_instagram) = normalized_urls
        .iter()
        .find(|url| !is_instagram_url(url.as_str()))
    {
        return Err(EngineError::InstallFailed(format!(
            "instagram batch accepts only instagram.com URLs (got {})",
            redact_url_for_log(non_instagram)
        )));
    }

    enqueue_download_direct_url_batch_raw(
        paths,
        normalized_urls,
        Some(DOWNLOAD_PROVIDER_YOUTUBE_YT_DLP.to_string()),
        auth_cookie,
        output_dir,
        Some(use_browser_cookies),
        None,
    )
}

pub fn enqueue_download_image_batch(
    paths: &AppPaths,
    start_urls: Vec<String>,
    max_pages: Option<usize>,
    delay_ms: Option<u64>,
    allow_cross_domain: Option<bool>,
    follow_content_links: Option<bool>,
    skip_url_keywords: Vec<String>,
    output_subdir: Option<String>,
    output_dir: Option<String>,
    auth_cookie: Option<String>,
) -> Result<JobRow> {
    let had_explicit_subdir = output_subdir
        .as_ref()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    let req = image_batch::build_image_batch_request(
        start_urls,
        max_pages,
        delay_ms,
        allow_cross_domain,
        follow_content_links,
        skip_url_keywords,
        output_subdir,
        auth_cookie,
    )?;
    let output_subdir = if had_explicit_subdir {
        req.output_subdir
    } else {
        String::new()
    };
    let output_dir = normalize_output_dir(output_dir);

    let params_json = serde_json::to_string(&DownloadImageBatchParams {
        start_urls: req.start_urls,
        max_pages: req.max_pages,
        delay_ms: req.delay_ms,
        allow_cross_domain: req.allow_cross_domain,
        follow_content_links: req.follow_content_links,
        skip_url_keywords: req.skip_url_keywords,
        output_subdir,
        output_dir,
        auth_cookie: None,
    })?;
    let job = enqueue_with_type_item_and_batch_id(
        paths,
        JobType::DownloadImageBatch,
        params_json,
        None,
        Some(Uuid::new_v4().to_string()),
    )?;

    if let Some(cookie) = req.auth_cookie.as_deref() {
        if let Err(err) = write_job_cookie_secret(paths, &job.id, cookie) {
            let _ = delete_job_by_id(paths, &job.id);
            remove_job_cookie_secret(paths, &job.id);
            return Err(err);
        }
    }

    Ok(job)
}

pub fn list_jobs(paths: &AppPaths, limit: usize, offset: usize) -> Result<Vec<JobRow>> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;

    let mut stmt = conn.prepare(
        r#"
SELECT
  id,
  item_id,
  batch_id,
  type,
  status,
  progress,
  error,
  created_at_ms,
  started_at_ms,
  finished_at_ms,
  logs_path
FROM job
ORDER BY created_at_ms DESC
LIMIT ?1 OFFSET ?2
"#,
    )?;

    let rows = stmt
        .query_map(params![limit as i64, offset as i64], |row| {
            let status_str: String = row.get(4)?;
            let status = JobStatus::from_str(&status_str).unwrap_or(JobStatus::Failed);
            Ok(JobRow {
                id: row.get(0)?,
                item_id: row.get(1)?,
                batch_id: row.get(2)?,
                job_type: row.get(3)?,
                status,
                progress: row.get(5)?,
                error: row.get(6)?,
                created_at_ms: row.get(7)?,
                started_at_ms: row.get(8)?,
                finished_at_ms: row.get(9)?,
                logs_path: row.get(10)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(rows)
}

pub fn get_queue_control(paths: &AppPaths) -> Result<JobQueueControlState> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    Ok(JobQueueControlState {
        paused: is_queue_paused_conn(&conn)?,
    })
}

pub fn get_runtime_settings(paths: &AppPaths) -> Result<JobRuntimeSettings> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    Ok(JobRuntimeSettings {
        max_concurrency: get_max_concurrency_conn(&conn)?,
    })
}

pub fn set_runtime_max_concurrency(
    paths: &AppPaths,
    max_concurrency: usize,
) -> Result<JobRuntimeSettings> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    let max_concurrency = max_concurrency.clamp(1, MAX_MAX_CONCURRENT_JOBS);
    conn.execute(
        "INSERT INTO meta(key, value) VALUES(?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![META_KEY_JOBS_MAX_CONCURRENCY, max_concurrency.to_string()],
    )?;
    Ok(JobRuntimeSettings { max_concurrency })
}

pub fn set_queue_paused(paths: &AppPaths, paused: bool) -> Result<JobQueueControlState> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    conn.execute(
        "INSERT INTO meta(key, value) VALUES(?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![META_KEY_JOBS_QUEUE_PAUSED, if paused { "1" } else { "0" }],
    )?;
    Ok(JobQueueControlState { paused })
}

pub fn cancel_job(paths: &AppPaths, job_id: &str) -> Result<()> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;

    let updated = conn.execute(
        "UPDATE job SET status=?1, finished_at_ms=?2 WHERE id=?3 AND status IN (?4, ?5)",
        params![
            JobStatus::Canceled.as_str(),
            now_ms(),
            job_id,
            JobStatus::Queued.as_str(),
            JobStatus::Running.as_str()
        ],
    )?;

    if updated == 0 {
        return Ok(());
    }

    remove_job_cookie_secret(paths, job_id);
    Ok(())
}

pub fn cancel_all_jobs(paths: &AppPaths) -> Result<usize> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    let updated = conn.execute(
        "UPDATE job SET status=?1, finished_at_ms=?2 WHERE status IN (?3, ?4)",
        params![
            JobStatus::Canceled.as_str(),
            now_ms(),
            JobStatus::Queued.as_str(),
            JobStatus::Running.as_str()
        ],
    )?;

    if updated > 0 {
        let _ = clear_dir_entries(&paths.job_secrets_dir());
    }
    Ok(updated)
}

pub fn flush_jobs_cache(paths: &AppPaths) -> Result<JobFlushSummary> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;

    let terminal_statuses = [
        JobStatus::Succeeded.as_str(),
        JobStatus::Failed.as_str(),
        JobStatus::Canceled.as_str(),
    ];

    let mut stmt = conn.prepare(
        "SELECT id, type, params_json, logs_path FROM job WHERE status IN (?1, ?2, ?3) ORDER BY created_at_ms ASC",
    )?;
    let terminal_jobs = stmt
        .query_map(
            params![
                terminal_statuses[0],
                terminal_statuses[1],
                terminal_statuses[2]
            ],
            |row| {
                let id: String = row.get(0)?;
                let job_type: String = row.get(1)?;
                let params_json: String = row.get(2)?;
                let logs_path: String = row.get(3)?;
                Ok((id, job_type, params_json, logs_path))
            },
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let removed_jobs = conn.execute(
        "DELETE FROM job WHERE status IN (?1, ?2, ?3)",
        params![
            terminal_statuses[0],
            terminal_statuses[1],
            terminal_statuses[2]
        ],
    )?;
    drop(stmt);
    drop(conn);

    let mut removed_log_files = 0_usize;
    let mut removed_artifact_dirs = 0_usize;
    let download_root = match paths.effective_download_dir() {
        Ok(v) => v,
        Err(_) => paths.default_download_dir(),
    };
    let mut output_dirs: HashSet<PathBuf> = HashSet::new();

    for (job_id, job_type, params_json, logs_path) in &terminal_jobs {
        let log_path = PathBuf::from(logs_path);
        removed_log_files += remove_job_log_files(&log_path);
        remove_job_cookie_secret(paths, job_id);

        let artifacts_dir = paths.job_artifacts_dir(job_id);
        if artifacts_dir.exists() && std::fs::remove_dir_all(&artifacts_dir).is_ok() {
            removed_artifact_dirs += 1;
        }

        if job_type == JobType::DownloadImageBatch.as_str() {
            if let Ok(params) = serde_json::from_str::<DownloadImageBatchParams>(params_json) {
                if let Some(raw_dir) = normalize_output_dir(params.output_dir) {
                    let mut custom_dir = PathBuf::from(raw_dir);
                    if !custom_dir.is_absolute() {
                        if let Ok(cwd) = std::env::current_dir() {
                            custom_dir = cwd.join(custom_dir);
                        }
                    }
                    output_dirs.insert(custom_dir);
                } else {
                    let subdir = params.output_subdir.trim();
                    if !subdir.is_empty() {
                        // Current layout stores image jobs under downloads/images/<subdir>.
                        output_dirs.insert(download_root.join(DEFAULT_IMAGES_OUTPUT_SUBDIR).join(subdir));
                        // Backward compatibility for older jobs written at downloads/<subdir>.
                        output_dirs.insert(download_root.join(subdir));
                    }
                }
            }
        }
    }

    let mut removed_output_dirs = 0_usize;
    for output_dir in output_dirs {
        if output_dir.exists() && std::fs::remove_dir_all(&output_dir).is_ok() {
            removed_output_dirs += 1;
        }
    }

    let removed_cache_entries = clear_dir_entries(&paths.cache_dir())?;

    Ok(JobFlushSummary {
        removed_jobs,
        removed_log_files,
        removed_artifact_dirs,
        removed_output_dirs,
        removed_cache_entries,
    })
}

pub fn retry_job(paths: &AppPaths, job_id: &str) -> Result<JobRow> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;

    let (type_str, params_json, batch_id): (String, String, Option<String>) = conn.query_row(
        "SELECT type, params_json, batch_id FROM job WHERE id=?1",
        [job_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;

    let job_type = JobType::from_str(&type_str)
        .ok_or_else(|| EngineError::InstallFailed(format!("unknown job type in db: {type_str}")))?;

    let item_id = match job_type {
        JobType::AsrLocal => serde_json::from_str::<AsrLocalParams>(&params_json)
            .ok()
            .map(|p| p.item_id),
        JobType::TranslateLocal => serde_json::from_str::<TranslateLocalParams>(&params_json)
            .ok()
            .map(|p| p.item_id),
        JobType::DiarizeLocalV1 => serde_json::from_str::<DiarizeLocalV1Params>(&params_json)
            .ok()
            .map(|p| p.item_id),
        JobType::TtsPreviewPyttsx3V1 => serde_json::from_str::<TtsPreviewPyttsx3V1Params>(&params_json)
            .ok()
            .map(|p| p.item_id),
        JobType::TtsNeuralLocalV1 => serde_json::from_str::<TtsNeuralLocalV1Params>(&params_json)
            .ok()
            .map(|p| p.item_id),
        JobType::DubVoicePreservingV1 => serde_json::from_str::<DubVoicePreservingV1Params>(&params_json)
            .ok()
            .map(|p| p.item_id),
        JobType::MixDubPreviewV1 => serde_json::from_str::<MixDubPreviewV1Params>(&params_json)
            .ok()
            .map(|p| p.item_id),
        JobType::MuxDubPreviewV1 => serde_json::from_str::<MuxDubPreviewV1Params>(&params_json)
            .ok()
            .map(|p| p.item_id),
        JobType::SeparateAudioSpleeter => serde_json::from_str::<SeparateAudioSpleeterParams>(&params_json)
            .ok()
            .map(|p| p.item_id),
        JobType::SeparateAudioDemucsV1 => serde_json::from_str::<SeparateAudioDemucsV1Params>(&params_json)
            .ok()
            .map(|p| p.item_id),
        JobType::CleanVocalsV1 => serde_json::from_str::<CleanVocalsV1Params>(&params_json)
            .ok()
            .map(|p| p.item_id),
        JobType::QcReportV1 => serde_json::from_str::<QcReportV1Params>(&params_json)
            .ok()
            .map(|p| p.item_id),
        JobType::ExportPackV1 => serde_json::from_str::<ExportPackV1Params>(&params_json)
            .ok()
            .map(|p| p.item_id),
        _ => None,
    };

    // Re-enqueue with identical params.
    enqueue_with_type_item_and_batch_id(paths, job_type, params_json, item_id, batch_id)
}

#[derive(Debug, Clone)]
pub struct JobRunnerHandle {
    stop: Arc<AtomicBool>,
}

impl JobRunnerHandle {
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

pub fn start_runner(paths: AppPaths) -> Result<JobRunnerHandle> {
    paths.ensure_dirs()?;
    let conn = db::open(&paths)?;
    db::migrate(&conn)?;

    // If the app crashed, requeue any running jobs.
    requeue_orphaned_running_jobs(&conn)?;

    let stop = Arc::new(AtomicBool::new(false));
    let running = Arc::new(AtomicUsize::new(0));

    let prune_paths = paths.clone();
    thread::spawn(move || {
        let _ = prune_job_logs(&prune_paths);
    });

    let stop_thread = stop.clone();
    let running_thread = running.clone();
    thread::spawn(move || runner_loop(paths, stop_thread, running_thread));

    Ok(JobRunnerHandle { stop })
}

fn requeue_orphaned_running_jobs(conn: &rusqlite::Connection) -> Result<usize> {
    let updated = conn.execute(
        "UPDATE job
         SET status=?1, started_at_ms=NULL, finished_at_ms=?2, error=?3
         WHERE status=?4",
        params![
            JobStatus::Failed.as_str(),
            now_ms(),
            "interrupted by app shutdown",
            JobStatus::Running.as_str()
        ],
    )?;
    Ok(updated)
}

fn enqueue(paths: &AppPaths, job_type: JobType, params_json: String) -> Result<JobRow> {
    enqueue_with_type_and_item_id(paths, job_type, params_json, None)
}

fn enqueue_with_type_and_item_id(
    paths: &AppPaths,
    job_type: JobType,
    params_json: String,
    item_id: Option<String>,
) -> Result<JobRow> {
    enqueue_with_type_item_and_batch_id(paths, job_type, params_json, item_id, None)
}

fn enqueue_with_type_item_and_batch_id(
    paths: &AppPaths,
    job_type: JobType,
    params_json: String,
    item_id: Option<String>,
    batch_id: Option<String>,
) -> Result<JobRow> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;

    let id = Uuid::new_v4().to_string();
    let created_at_ms = now_ms();
    let logs_path = paths
        .job_logs_dir()
        .join(format!("{id}.jsonl"))
        .to_string_lossy()
        .to_string();

    conn.execute(
        r#"
INSERT INTO job (
  id,
  item_id,
  batch_id,
  type,
  status,
  progress,
  error,
  params_json,
  created_at_ms,
  started_at_ms,
  finished_at_ms,
  logs_path
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
"#,
        params![
            &id,
            &item_id,
            &batch_id,
            job_type.as_str(),
            JobStatus::Queued.as_str(),
            0.0_f32,
            Option::<String>::None,
            &params_json,
            created_at_ms,
            Option::<i64>::None,
            Option::<i64>::None,
            &logs_path
        ],
    )?;

    Ok(JobRow {
        id,
        item_id,
        batch_id,
        job_type: job_type.as_str().to_string(),
        status: JobStatus::Queued,
        progress: 0.0,
        error: None,
        created_at_ms,
        started_at_ms: None,
        finished_at_ms: None,
        logs_path,
    })
}

fn job_batch_id(paths: &AppPaths, job_id: &str) -> Result<Option<String>> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    let batch_id: Option<String> = conn.query_row(
        "SELECT batch_id FROM job WHERE id=?1",
        params![job_id],
        |row| row.get(0),
    )?;
    Ok(batch_id.and_then(|v| {
        let t = v.trim().to_string();
        if t.is_empty() { None } else { Some(t) }
    }))
}

fn item_has_active_job(paths: &AppPaths, item_id: &str, job_type: &str) -> Result<bool> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    let count: i64 = conn.query_row(
        r#"
SELECT COUNT(*)
FROM job
WHERE item_id=?1 AND type=?2 AND status IN (?3, ?4)
"#,
        params![
            item_id,
            job_type,
            JobStatus::Queued.as_str(),
            JobStatus::Running.as_str()
        ],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn separation_background_path_best_effort(paths: &AppPaths, item_id: &str) -> Option<PathBuf> {
    let item_dir = paths.derived_item_dir(item_id);
    let demucs = item_dir
        .join("separation")
        .join("demucs_two_stems_v1")
        .join("background.wav");
    if demucs.exists() {
        return Some(demucs);
    }

    let spleeter = item_dir
        .join("separation")
        .join("spleeter_2stems")
        .join("background.wav");
    if spleeter.exists() {
        return Some(spleeter);
    }

    None
}

fn separation_vocals_path_best_effort(paths: &AppPaths, item_id: &str) -> Option<PathBuf> {
    let item_dir = paths.derived_item_dir(item_id);
    let demucs = item_dir
        .join("separation")
        .join("demucs_two_stems_v1")
        .join("vocals.wav");
    if demucs.exists() {
        return Some(demucs);
    }

    let spleeter = item_dir
        .join("separation")
        .join("spleeter_2stems")
        .join("vocals.wav");
    if spleeter.exists() {
        return Some(spleeter);
    }

    None
}

fn separation_background_exists(paths: &AppPaths, item_id: &str) -> bool {
    separation_background_path_best_effort(paths, item_id).is_some()
}

fn tts_manifest_exists(paths: &AppPaths, item_id: &str) -> bool {
    let item_dir = paths.derived_item_dir(item_id);
    for manifest in [
        item_dir
            .join("tts_preview")
            .join("dub_voice_preserving_v1")
            .join("manifest.json"),
        item_dir
            .join("tts_preview")
            .join("tts_neural_local_v1")
            .join("manifest.json"),
        item_dir
            .join("tts_preview")
            .join("pyttsx3_v1")
            .join("manifest.json"),
    ] {
        if manifest.exists() {
            return true;
        }
    }
    false
}

fn mix_output_exists(paths: &AppPaths, item_id: &str) -> bool {
    paths.derived_item_dir(item_id)
        .join("dub_preview")
        .join("mix_dub_preview_v1.wav")
        .exists()
}

fn mux_output_exists(paths: &AppPaths, item_id: &str) -> bool {
    let dir = paths.derived_item_dir(item_id).join("dub_preview");
    dir.join("mux_dub_preview_v1.mp4").exists() || dir.join("mux_dub_preview_v1.mkv").exists()
}

fn runner_loop(paths: AppPaths, stop: Arc<AtomicBool>, running: Arc<AtomicUsize>) {
    while !stop.load(Ordering::SeqCst) {
        let paused = match is_queue_paused(&paths) {
            Ok(v) => v,
            Err(_) => false,
        };
        if paused {
            thread::sleep(Duration::from_millis(250));
            continue;
        }

        let max_concurrency = match get_max_concurrency(&paths) {
            Ok(v) => v,
            Err(_) => DEFAULT_MAX_CONCURRENT_JOBS,
        };
        let available = max_concurrency.saturating_sub(running.load(Ordering::SeqCst));
        if available == 0 {
            thread::sleep(Duration::from_millis(200));
            continue;
        }

        let queued = match fetch_queued_jobs(&paths, available) {
            Ok(v) => v,
            Err(_) => {
                thread::sleep(Duration::from_millis(400));
                continue;
            }
        };

        if queued.is_empty() {
            thread::sleep(Duration::from_millis(300));
            continue;
        }

        for (job_id, type_str, params_json) in queued {
            if stop.load(Ordering::SeqCst) {
                break;
            }

            let claimed = match claim_job(&paths, &job_id) {
                Ok(v) => v,
                Err(_) => false,
            };
            if !claimed {
                continue;
            }

            running.fetch_add(1, Ordering::SeqCst);
            let paths_worker = paths.clone();
            let running_worker = running.clone();
            thread::spawn(move || {
                let result = execute_job(&paths_worker, &job_id, &type_str, &params_json);
                if let Err(e) = result {
                    let _ = set_failed(&paths_worker, &job_id, &e.to_string());
                }
                running_worker.fetch_sub(1, Ordering::SeqCst);
            });
        }
    }
}

fn fetch_queued_jobs(paths: &AppPaths, limit: usize) -> Result<Vec<(String, String, String)>> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;

    let mut stmt = conn.prepare(
        "SELECT id, type, params_json FROM job WHERE status=?1 ORDER BY created_at_ms ASC LIMIT ?2",
    )?;

    let rows = stmt
        .query_map(params![JobStatus::Queued.as_str(), limit as i64], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(rows)
}

fn claim_job(paths: &AppPaths, job_id: &str) -> Result<bool> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;

    let updated = conn.execute(
        "UPDATE job SET status=?1, started_at_ms=?2 WHERE id=?3 AND status=?4",
        params![
            JobStatus::Running.as_str(),
            now_ms(),
            job_id,
            JobStatus::Queued.as_str()
        ],
    )?;
    Ok(updated == 1)
}

fn execute_job(paths: &AppPaths, job_id: &str, type_str: &str, params_json: &str) -> Result<()> {
    let artifacts_dir = paths.job_artifacts_dir(job_id);
    std::fs::create_dir_all(&artifacts_dir)?;

    if is_canceled(paths, job_id)? {
        log_line(paths, job_id, "info", "job_canceled", serde_json::json!({}))?;
        return Ok(());
    }

    log_line(
        paths,
        job_id,
        "info",
        "job_started",
        serde_json::json!({ "type": type_str }),
    )?;

    let job_type = JobType::from_str(type_str)
        .ok_or_else(|| EngineError::InstallFailed(format!("unknown job type in db: {type_str}")))?;

    match job_type {
        JobType::ImportLocal => {
            set_progress(paths, job_id, 0.05)?;
            let p: ImportLocalParams = serde_json::from_str(params_json)?;

            if is_canceled(paths, job_id)? {
                log_line(paths, job_id, "info", "job_canceled", serde_json::json!({}))?;
                return Ok(());
            }

            log_line(
                paths,
                job_id,
                "info",
                "import_local_begin",
                serde_json::json!({ "path": p.path }),
            )?;

            let item = library::import_local_file(paths, Path::new(&p.path))?;
            set_progress(paths, job_id, 1.0)?;

            // Associate created item id.
            let conn = db::open(paths)?;
            db::migrate(&conn)?;
            conn.execute(
                "UPDATE job SET item_id=?1 WHERE id=?2",
                params![item.id, job_id],
            )?;

            log_line(
                paths,
                job_id,
                "info",
                "import_local_done",
                serde_json::json!({ "item_id": item.id }),
            )?;

            // Optional: batch-on-import automation (local-only; off by default).
            let rules = config::load_batch_on_import_rules(paths).unwrap_or_default();
            let any_enabled = rules.auto_asr
                || rules.auto_translate
                || rules.auto_separate
                || rules.auto_diarize
                || rules.auto_dub_preview;
            if any_enabled {
                let batch_id = Some(Uuid::new_v4().to_string());
                log_line(
                    paths,
                    job_id,
                    "info",
                    "batch_on_import_rules_applied",
                    serde_json::json!({
                        "batch_id": batch_id,
                        "rules": {
                            "auto_asr": rules.auto_asr,
                            "auto_translate": rules.auto_translate,
                            "auto_separate": rules.auto_separate,
                            "auto_diarize": rules.auto_diarize,
                            "auto_dub_preview": rules.auto_dub_preview,
                        }
                    }),
                )?;

                let needs_asr =
                    rules.auto_asr || rules.auto_translate || rules.auto_diarize || rules.auto_dub_preview;
                let needs_separate = rules.auto_separate || rules.auto_dub_preview;

                if needs_separate {
                    let params_json = serde_json::to_string(&SeparateAudioSpleeterParams {
                        item_id: item.id.clone(),
                        batch_on_import: true,
                    })?;
                    let _ = enqueue_with_type_item_and_batch_id(
                        paths,
                        JobType::SeparateAudioSpleeter,
                        params_json,
                        Some(item.id.clone()),
                        batch_id.clone(),
                    )?;
                }

                if needs_asr {
                    let params_json = serde_json::to_string(&AsrLocalParams {
                        item_id: item.id.clone(),
                        lang: None,
                        model_id: "whispercpp-tiny".to_string(),
                        batch_on_import: true,
                    })?;
                    let _ = enqueue_with_type_item_and_batch_id(
                        paths,
                        JobType::AsrLocal,
                        params_json,
                        Some(item.id.clone()),
                        batch_id.clone(),
                    )?;
                }
            }
        }
        JobType::DownloadDirectUrl => {
            set_progress(paths, job_id, 0.05)?;
            let p: DownloadDirectUrlParams = serde_json::from_str(params_json)?;
            let subscription_id = p.subscription_id.clone();
            let url = normalize_direct_url(&p.url)?;
            let provider = effective_download_provider(&p.provider, &url);
            let auth_cookie = read_job_cookie_secret(paths, job_id).or_else(|| normalize_auth_cookie(p.auth_cookie));
            remove_job_cookie_secret(paths, job_id);
            let mut output_dir = normalize_output_dir(p.output_dir);
            let output_subdir = normalize_output_subdir(p.output_subdir);
            let use_browser_cookies = p.use_browser_cookies;
            if output_dir.is_none() && output_subdir.is_none() {
                output_dir = Some(default_direct_job_output_dir(paths, provider, &url, job_id)?);
            }

            if is_canceled(paths, job_id)? {
                log_line(paths, job_id, "info", "job_canceled", serde_json::json!({}))?;
                return Ok(());
            }

            log_line(
                paths,
                job_id,
                "info",
                "download_direct_url_begin",
                serde_json::json!({
                    "url": redact_url_for_log(&url),
                    "provider": provider
                }),
            )?;

            let downloaded_path =
                download_url_to_library(
                    paths,
                    &url,
                    job_id,
                    provider,
                    auth_cookie.as_deref(),
                    output_dir.as_deref(),
                    output_subdir.as_deref(),
                    use_browser_cookies,
                )?;
            set_progress(paths, job_id, 0.70)?;

            if is_canceled(paths, job_id)? {
                log_line(paths, job_id, "info", "job_canceled", serde_json::json!({}))?;
                return Ok(());
            }

            let item = library::import_downloaded_file(
                paths,
                &downloaded_path,
                &url,
                DOWNLOAD_RIGHTS_NOTE_UNSPECIFIED,
                provider,
                now_ms(),
            )?;
            set_progress(paths, job_id, 1.0)?;

            if let Some(sub_id) = subscription_id.as_deref() {
                if let Err(err) = append_youtube_archive_on_success(paths, sub_id, &url) {
                    let _ = log_line(
                        paths,
                        job_id,
                        "warning",
                        "youtube_archive_append_failed",
                        serde_json::json!({
                            "subscription_id": sub_id,
                            "error": err.to_string(),
                        }),
                    );
                }
            }

            let conn = db::open(paths)?;
            db::migrate(&conn)?;
            conn.execute(
                "UPDATE job SET item_id=?1 WHERE id=?2",
                params![item.id, job_id],
            )?;

            log_line(
                paths,
                job_id,
                "info",
                "download_direct_url_done",
                serde_json::json!({
                    "item_id": item.id,
                    "path": downloaded_path.to_string_lossy().to_string()
                }),
            )?;
        }
        JobType::YoutubeSubscriptionRefreshV1 => {
            set_progress(paths, job_id, 0.05)?;
            let p: YoutubeSubscriptionRefreshV1Params = serde_json::from_str(params_json)?;

            if is_canceled(paths, job_id)? {
                log_line(paths, job_id, "info", "job_canceled", serde_json::json!({}))?;
                return Ok(());
            }

            let sub = subscriptions::get_youtube_subscription_by_id(paths, &p.subscription_id)?
                .ok_or_else(|| EngineError::InstallFailed(format!("subscription not found: {}", p.subscription_id)))?;

            let max_items = p.max_items.unwrap_or(200).clamp(1, MAX_DOWNLOAD_BATCH_URLS);
            let output_dir = subscriptions::youtube_subscription_output_dir(paths, &sub)?;
            std::fs::create_dir_all(&output_dir)?;

            let archive_path = subscriptions::youtube_subscription_archive_path(paths, &sub)?;
            let archived = read_ytdlp_archive_ids(&archive_path).unwrap_or_default();

            log_line(
                paths,
                job_id,
                "info",
                "youtube_subscription_refresh_begin",
                serde_json::json!({
                    "subscription_id": sub.id,
                    "source_url": redact_url_for_log(&sub.source_url),
                    "max_items": max_items,
                }),
            )?;

            let expanded = expand_yt_dlp_urls(
                paths,
                &sub.source_url,
                max_items,
                None,
                use_browser_cookies_for_url(&sub.source_url, sub.use_browser_cookies),
            )?;
            set_progress(paths, job_id, 0.40)?;

            if is_canceled(paths, job_id)? {
                log_line(paths, job_id, "info", "job_canceled", serde_json::json!({}))?;
                return Ok(());
            }

            let mut new_urls: Vec<String> = Vec::new();
            let mut skipped_archived = 0_usize;
            for candidate in expanded {
                let Some(video_id) = subscriptions::youtube_video_id_from_url(candidate.as_str()) else {
                    continue;
                };
                if archived.contains(video_id.as_str()) {
                    skipped_archived += 1;
                    continue;
                }
                new_urls.push(candidate);
            }

            if new_urls.is_empty() {
                set_progress(paths, job_id, 1.0)?;
                log_line(
                    paths,
                    job_id,
                    "info",
                    "youtube_subscription_refresh_done",
                    serde_json::json!({
                        "queued": 0,
                        "skipped_archived": skipped_archived,
                    }),
                )?;
                return Ok(());
            }

            let queued = enqueue_download_direct_url_batch_raw_with_subscription(
                paths,
                new_urls,
                Some(DOWNLOAD_PROVIDER_YOUTUBE_YT_DLP.to_string()),
                None,
                Some(output_dir.to_string_lossy().to_string()),
                Some(sub.use_browser_cookies),
                Some(job_id.to_string()),
                Some(sub.id.clone()),
            )?;
            set_progress(paths, job_id, 1.0)?;

            log_line(
                paths,
                job_id,
                "info",
                "youtube_subscription_refresh_done",
                serde_json::json!({
                    "queued": queued.len(),
                    "skipped_archived": skipped_archived,
                    "archive_path": archive_path.to_string_lossy().to_string(),
                }),
            )?;
        }
        JobType::DownloadImageBatch => {
            set_progress(paths, job_id, 0.05)?;
            let p: DownloadImageBatchParams = serde_json::from_str(params_json)?;
            let auth_cookie = read_job_cookie_secret(paths, job_id).or(p.auth_cookie);
            remove_job_cookie_secret(paths, job_id);

            if is_canceled(paths, job_id)? {
                log_line(paths, job_id, "info", "job_canceled", serde_json::json!({}))?;
                return Ok(());
            }

            let output_dir_override = normalize_output_dir(p.output_dir.clone());
            let selected_subdir = normalize_output_subdir(Some(p.output_subdir.clone()));
            let auto_job_subdir = default_job_folder_name(job_id);
            let effective_subdir = selected_subdir
                .clone()
                .unwrap_or_else(|| auto_job_subdir.clone());
            let output_root = if let Some(custom_dir) = output_dir_override.as_deref() {
                resolve_downloads_dir_with_override(paths, Some(custom_dir), None)?
            } else {
                let images_root = resolve_downloads_dir(paths, Some(DEFAULT_IMAGES_OUTPUT_SUBDIR))?;
                images_root.join(&effective_subdir)
            };

            let start_urls_redacted: Vec<String> = p
                .start_urls
                .iter()
                .map(|url| redact_url_for_log(url))
                .collect();
            log_line(
                paths,
                job_id,
                "info",
                "download_image_batch_begin",
                serde_json::json!({
                    "start_urls": start_urls_redacted,
                    "max_pages": p.max_pages,
                    "delay_ms": p.delay_ms,
                    "allow_cross_domain": p.allow_cross_domain,
                    "follow_content_links": p.follow_content_links,
                    "output_subdir": if output_dir_override.is_some() { serde_json::Value::Null } else { serde_json::Value::String(effective_subdir.clone()) },
                    "output_dir": output_root.to_string_lossy().to_string(),
                }),
            )?;

            let manifest_path = artifacts_dir.join("image_manifest.csv");
            let request = image_batch::ImageBatchRequest {
                start_urls: p.start_urls,
                max_pages: p.max_pages,
                delay_ms: p.delay_ms,
                allow_cross_domain: p.allow_cross_domain,
                follow_content_links: p.follow_content_links,
                skip_url_keywords: p.skip_url_keywords,
                output_subdir: if output_dir_override.is_some() {
                    output_root
                        .file_name()
                        .and_then(|v| v.to_str())
                        .unwrap_or(DEFAULT_IMAGES_OUTPUT_SUBDIR)
                        .to_string()
                } else {
                    effective_subdir
                },
                auth_cookie,
            };

            let summary = image_batch::run_image_batch_download(
                &request,
                &output_root,
                &manifest_path,
                || is_canceled(paths, job_id),
                |progress| set_progress(paths, job_id, progress),
                |level, event, data| log_line(paths, job_id, level, event, data),
            )?;

            if is_canceled(paths, job_id)? {
                log_line(paths, job_id, "info", "job_canceled", serde_json::json!({}))?;
                return Ok(());
            }

            let summary_path = artifacts_dir.join("image_summary.json");
            std::fs::write(
                &summary_path,
                format!("{}\n", serde_json::to_string_pretty(&summary)?),
            )?;
            set_progress(paths, job_id, 1.0)?;

            log_line(
                paths,
                job_id,
                "info",
                "download_image_batch_done",
                serde_json::json!({
                    "pages_crawled": summary.pages_crawled,
                    "images_downloaded": summary.images_downloaded,
                    "duplicates": summary.duplicate_images,
                    "skipped_profile_images": summary.skipped_profile_images,
                    "failed_images": summary.failed_images,
                    "manifest_path": summary.manifest_path,
                    "output_dir": summary.output_dir,
                    "summary_path": summary_path,
                }),
            )?;
        }
        JobType::AsrLocal => {
            set_progress(paths, job_id, 0.05)?;
            let p: AsrLocalParams = serde_json::from_str(params_json)?;

            if is_canceled(paths, job_id)? {
                log_line(paths, job_id, "info", "job_canceled", serde_json::json!({}))?;
                return Ok(());
            }

            log_line(
                paths,
                job_id,
                "info",
                "asr_begin",
                serde_json::json!({ "item_id": &p.item_id, "lang": &p.lang, "model_id": &p.model_id }),
            )?;

            let item = library::get_item_by_id(paths, &p.item_id)?;
            let media_path = Path::new(&item.media_path);

            let asr_dir = paths.derived_item_dir(&item.id).join("asr");
            std::fs::create_dir_all(&asr_dir)?;

            let audio_path = asr_dir.join("audio_16k.wav");
            log_line(
                paths,
                job_id,
                "info",
                "asr_extract_audio_begin",
                serde_json::json!({ "path": &item.media_path, "audio_path": &audio_path }),
            )?;
            if audio_path.exists() && std::fs::metadata(&audio_path).map(|m| m.len()).unwrap_or(0) > 0 {
                log_line(
                    paths,
                    job_id,
                    "info",
                    "asr_extract_audio_resume_skip_existing",
                    serde_json::json!({ "audio_path": &audio_path }),
                )?;
            } else {
                ffmpeg::extract_audio_wav_16k_mono(paths, media_path, &audio_path)?;
            }
            set_progress(paths, job_id, 0.25)?;

            if is_canceled(paths, job_id)? {
                log_line(paths, job_id, "info", "job_canceled", serde_json::json!({}))?;
                return Ok(());
            }

            log_line(
                paths,
                job_id,
                "info",
                "asr_transcribe_begin",
                serde_json::json!({ "model_id": &p.model_id, "lang": &p.lang, "audio_path": &audio_path }),
            )?;
            let doc = asr::transcribe_whisper_wav_16k_mono(
                paths,
                &p.model_id,
                &audio_path,
                p.lang.as_deref(),
            )?;
            set_progress(paths, job_id, 0.85)?;

            if is_canceled(paths, job_id)? {
                log_line(paths, job_id, "info", "job_canceled", serde_json::json!({}))?;
                return Ok(());
            }

            let json_path = asr_dir.join("source.json");
            let srt_path = asr_dir.join("source.srt");
            let vtt_path = asr_dir.join("source.vtt");
            subtitles::write_artifacts(&doc, &json_path, &srt_path, &vtt_path)?;
            set_progress(paths, job_id, 0.95)?;

            let track_id = Uuid::new_v4().to_string();
            let conn = db::open(paths)?;
            db::migrate(&conn)?;
            conn.execute(
                r#"
INSERT INTO subtitle_track (
  id,
  item_id,
  kind,
  lang,
  format,
  path,
  created_by,
  version
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
"#,
                params![
                    &track_id,
                    &item.id,
                    "source",
                    &doc.lang,
                    "ytfetch_subtitle_json_v1",
                    json_path.to_string_lossy().to_string(),
                    format!("asr:{}", p.model_id),
                    1_i64
                ],
            )?;

            log_line(
                paths,
                job_id,
                "info",
                "asr_done",
                serde_json::json!({ "track_id": track_id, "json_path": json_path }),
            )?;

            if p.batch_on_import {
                let rules = config::load_batch_on_import_rules(paths).unwrap_or_default();
                let batch_id = job_batch_id(paths, job_id).ok().flatten();

                if rules.auto_diarize {
                    if !item_has_active_job(paths, &item.id, JobType::DiarizeLocalV1.as_str())
                        .unwrap_or(false)
                    {
                        let params_json = serde_json::to_string(&DiarizeLocalV1Params {
                            item_id: item.id.clone(),
                            source_track_id: track_id.clone(),
                            backend: None,
                            batch_on_import: true,
                        })?;
                        let _ = enqueue_with_type_item_and_batch_id(
                            paths,
                            JobType::DiarizeLocalV1,
                            params_json,
                            Some(item.id.clone()),
                            batch_id.clone(),
                        )?;
                    }
                }

                if rules.auto_translate || rules.auto_dub_preview {
                    if !item_has_active_job(paths, &item.id, JobType::TranslateLocal.as_str())
                        .unwrap_or(false)
                    {
                        let params_json = serde_json::to_string(&TranslateLocalParams {
                            item_id: item.id.clone(),
                            source_track_id: track_id.clone(),
                            model_id: "whispercpp-tiny".to_string(),
                            batch_on_import: true,
                        })?;
                        let _ = enqueue_with_type_item_and_batch_id(
                            paths,
                            JobType::TranslateLocal,
                            params_json,
                            Some(item.id.clone()),
                            batch_id.clone(),
                        )?;
                    }
                }
            }
        }
        JobType::TranslateLocal => {
            set_progress(paths, job_id, 0.05)?;
            let p: TranslateLocalParams = serde_json::from_str(params_json)?;

            if is_canceled(paths, job_id)? {
                log_line(paths, job_id, "info", "job_canceled", serde_json::json!({}))?;
                return Ok(());
            }

            log_line(
                paths,
                job_id,
                "info",
                "translate_begin",
                serde_json::json!({
                    "item_id": &p.item_id,
                    "source_track_id": &p.source_track_id,
                    "model_id": &p.model_id
                }),
            )?;

            let source_track = subtitle_tracks::get_track(paths, &p.source_track_id)?;
            if source_track.item_id != p.item_id {
                return Err(EngineError::InstallFailed(format!(
                    "translate job item_id mismatch: params.item_id={} track.item_id={}",
                    p.item_id, source_track.item_id
                )));
            }
            let source_doc = subtitle_tracks::load_document(paths, &p.source_track_id)?;

            let item = library::get_item_by_id(paths, &p.item_id)?;
            let media_path = Path::new(&item.media_path);

            let translate_dir = paths.derived_item_dir(&item.id).join("translate");
            std::fs::create_dir_all(&translate_dir)?;

            let audio_path = translate_dir.join("audio_16k.wav");
            log_line(
                paths,
                job_id,
                "info",
                "translate_extract_audio_begin",
                serde_json::json!({ "path": &item.media_path, "audio_path": &audio_path }),
            )?;
            if audio_path.exists() && std::fs::metadata(&audio_path).map(|m| m.len()).unwrap_or(0) > 0 {
                log_line(
                    paths,
                    job_id,
                    "info",
                    "translate_extract_audio_resume_skip_existing",
                    serde_json::json!({ "audio_path": &audio_path }),
                )?;
            } else {
                ffmpeg::extract_audio_wav_16k_mono(paths, media_path, &audio_path)?;
            }
            set_progress(paths, job_id, 0.25)?;

            if is_canceled(paths, job_id)? {
                log_line(paths, job_id, "info", "job_canceled", serde_json::json!({}))?;
                return Ok(());
            }

            log_line(
                paths,
                job_id,
                "info",
                "translate_whisper_begin",
                serde_json::json!({ "model_id": &p.model_id, "audio_path": &audio_path }),
            )?;
            let result = translate::translate_doc_whisper_to_en(
                paths,
                &source_doc,
                &audio_path,
                &p.model_id,
                translate::TranslateOptions::default(),
            )?;
            set_progress(paths, job_id, 0.85)?;

            if is_canceled(paths, job_id)? {
                log_line(paths, job_id, "info", "job_canceled", serde_json::json!({}))?;
                return Ok(());
            }

            let conn = db::open(paths)?;
            db::migrate(&conn)?;
            let max_version: Option<i64> = conn.query_row(
                r#"
SELECT MAX(version)
FROM subtitle_track
WHERE item_id=?1 AND kind=?2 AND lang=?3 AND format=?4
"#,
                params![&item.id, "translated", "en", "ytfetch_subtitle_json_v1"],
                |row| row.get(0),
            )?;
            let next_version = max_version.unwrap_or(0) + 1;

            let stem = "en";
            let json_path = if next_version <= 1 {
                translate_dir.join(format!("{stem}.json"))
            } else {
                translate_dir.join(format!("{stem}.v{next_version}.json"))
            };
            let srt_path = if next_version <= 1 {
                translate_dir.join(format!("{stem}.srt"))
            } else {
                translate_dir.join(format!("{stem}.v{next_version}.srt"))
            };
            let vtt_path = if next_version <= 1 {
                translate_dir.join(format!("{stem}.vtt"))
            } else {
                translate_dir.join(format!("{stem}.v{next_version}.vtt"))
            };

            subtitles::write_artifacts(&result.doc, &json_path, &srt_path, &vtt_path)?;
            set_progress(paths, job_id, 0.95)?;

            let track_id = Uuid::new_v4().to_string();
            conn.execute(
                r#"
INSERT INTO subtitle_track (
  id,
  item_id,
  kind,
  lang,
  format,
  path,
  created_by,
  version
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
"#,
                params![
                    &track_id,
                    &item.id,
                    "translated",
                    "en",
                    "ytfetch_subtitle_json_v1",
                    json_path.to_string_lossy().to_string(),
                    format!("translate:whispercpp:{}", p.model_id),
                    next_version,
                ],
            )?;

            let report_path = artifacts_dir.join("translate_report.json");
            std::fs::write(
                &report_path,
                format!("{}\n", serde_json::to_string_pretty(&result.report)?),
            )?;

            log_line(
                paths,
                job_id,
                "info",
                "translate_done",
                serde_json::json!({
                    "track_id": track_id,
                    "json_path": json_path,
                    "warnings": result.report.warnings.len(),
                    "report_path": report_path
                }),
            )?;

            if p.batch_on_import {
                let rules = config::load_batch_on_import_rules(paths).unwrap_or_default();
                if rules.auto_dub_preview {
                    let batch_id = job_batch_id(paths, job_id).ok().flatten();

                    let prefer_neural = tools::tts_neural_local_v1_pack_status(paths).installed;
                    let tts_job_type = if prefer_neural {
                        JobType::TtsNeuralLocalV1
                    } else {
                        JobType::TtsPreviewPyttsx3V1
                    };

                    if !item_has_active_job(paths, &item.id, tts_job_type.as_str()).unwrap_or(false) {
                        let params_json = if prefer_neural {
                            serde_json::to_string(&TtsNeuralLocalV1Params {
                                item_id: item.id.clone(),
                                source_track_id: track_id.clone(),
                                batch_on_import: true,
                            })?
                        } else {
                            serde_json::to_string(&TtsPreviewPyttsx3V1Params {
                                item_id: item.id.clone(),
                                source_track_id: track_id.clone(),
                                batch_on_import: true,
                            })?
                        };

                        let _ = enqueue_with_type_item_and_batch_id(
                            paths,
                            tts_job_type,
                            params_json,
                            Some(item.id.clone()),
                            batch_id.clone(),
                        )?;
                    }
                }
            }
        }
        JobType::DiarizeLocalV1 => {
            set_progress(paths, job_id, 0.05)?;
            let p: DiarizeLocalV1Params = serde_json::from_str(params_json)?;

            if is_canceled(paths, job_id)? {
                log_line(paths, job_id, "info", "job_canceled", serde_json::json!({}))?;
                return Ok(());
            }

            log_line(
                paths,
                job_id,
                "info",
                "diarize_begin",
                serde_json::json!({
                    "item_id": &p.item_id,
                    "source_track_id": &p.source_track_id,
                    "backend": p.backend
                }),
            )?;

            let requested_backend = p
                .backend
                .as_deref()
                .map(|v| v.trim().to_lowercase())
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| "baseline".to_string());
            let use_pyannote = requested_backend == "pyannote_byo_v1" || requested_backend == "pyannote";
            let backend_for_log = if use_pyannote {
                "pyannote_byo_v1"
            } else {
                "resemblyzer_partials_cluster_v1"
            };

            log_line(
                paths,
                job_id,
                "info",
                "diarize_backend_selected",
                serde_json::json!({ "backend": backend_for_log }),
            )?;

            if !use_pyannote {
                let pack = tools::diarization_pack_status(paths);
                if !pack.installed {
                    return Err(EngineError::InstallFailed(
                        "Diarization pack is not installed. Open Diagnostics -> Tools -> Install diarization pack."
                            .to_string(),
                    ));
                }
            }

            let source_track = subtitle_tracks::get_track(paths, &p.source_track_id)?;
            if source_track.item_id != p.item_id {
                return Err(EngineError::InstallFailed(format!(
                    "diarize job item_id mismatch: params.item_id={} track.item_id={}",
                    p.item_id, source_track.item_id
                )));
            }
            let source_doc = subtitle_tracks::load_document(paths, &p.source_track_id)?;

            let item = library::get_item_by_id(paths, &p.item_id)?;
            let media_path = Path::new(&item.media_path);

            let diarize_dir = paths.derived_item_dir(&item.id).join("diarize");
            std::fs::create_dir_all(&diarize_dir)?;

            let audio_path = diarize_dir.join("audio_16k.wav");
            log_line(
                paths,
                job_id,
                "info",
                "diarize_extract_audio_begin",
                serde_json::json!({ "path": &item.media_path, "audio_path": &audio_path }),
            )?;
            if audio_path.exists() && std::fs::metadata(&audio_path).map(|m| m.len()).unwrap_or(0) > 0 {
                log_line(
                    paths,
                    job_id,
                    "info",
                    "diarize_extract_audio_resume_skip_existing",
                    serde_json::json!({ "audio_path": &audio_path }),
                )?;
            } else {
                ffmpeg::extract_audio_wav_16k_mono(paths, media_path, &audio_path)?;
            }
            set_progress(paths, job_id, 0.25)?;

            if is_canceled(paths, job_id)? {
                log_line(paths, job_id, "info", "job_canceled", serde_json::json!({}))?;
                return Ok(());
            }

            let diarization_json_path = if use_pyannote {
                diarize_dir.join("diarization_pyannote_byo_v1.json")
            } else {
                diarize_dir.join("diarization.json")
            };
            let created_by = if use_pyannote {
                "diarize:pyannote_byo_v1".to_string()
            } else {
                "diarize:resemblyzer_partials_cluster_v1".to_string()
            };

            if diarization_json_path.exists()
                && std::fs::metadata(&diarization_json_path).map(|m| m.len()).unwrap_or(0) > 0
            {
                log_line(
                    paths,
                    job_id,
                    "info",
                    "diarize_resume_skip_existing",
                    serde_json::json!({ "diarization_json_path": &diarization_json_path }),
                )?;
            } else if use_pyannote {
                let status = config::load_optional_diarization_backend_status(paths)?;
                if !status.config.enabled
                    || status.config.backend.trim().to_lowercase() != "pyannote_byo_v1"
                {
                    return Err(EngineError::InstallFailed(
                        "Optional diarization backend is not enabled/configured. Open Diagnostics -> Settings -> Optional diarization backend."
                            .to_string(),
                    ));
                }

                let python_exe = status
                    .config
                    .python_exe
                    .as_deref()
                    .map(|v| v.trim())
                    .filter(|v| !v.is_empty())
                    .ok_or_else(|| {
                        EngineError::InstallFailed(
                            "Optional diarization backend requires python_exe. Configure it in Diagnostics -> Settings -> Optional diarization backend."
                                .to_string(),
                        )
                    })?;
                let python_exe = PathBuf::from(python_exe);
                if !python_exe.exists() {
                    return Err(EngineError::InstallFailed(format!(
                        "optional diarization python_exe not found: {}",
                        python_exe.to_string_lossy()
                    )));
                }

                let pipeline = status
                    .config
                    .local_model_path
                    .as_deref()
                    .map(|v| v.trim().to_string())
                    .filter(|v| !v.is_empty())
                    .or_else(|| {
                        status
                            .config
                            .model_id
                            .as_deref()
                            .map(|v| v.trim().to_string())
                            .filter(|v| !v.is_empty())
                    })
                    .unwrap_or_else(|| "pyannote/speaker-diarization-3.1".to_string());

                let token = config::read_optional_diarization_backend_token(paths)?;
                let needs_token = status
                    .config
                    .local_model_path
                    .as_deref()
                    .map(|v| v.trim())
                    .filter(|v| !v.is_empty())
                    .is_none();
                if needs_token && token.is_none() {
                    return Err(EngineError::InstallFailed(
                        "optional diarization backend token missing; set it in Diagnostics -> Settings -> Optional diarization backend."
                            .to_string(),
                    ));
                }

                log_line(
                    paths,
                    job_id,
                    "info",
                    "diarize_python_begin",
                    serde_json::json!({
                        "audio_path": &audio_path,
                        "diarization_json_path": &diarization_json_path,
                        "backend": "pyannote_byo_v1",
                        "pipeline": &pipeline,
                        "note": "This backend may download gated models during explicit runs, depending on your configuration."
                    }),
                )?;

                let script_path = artifacts_dir.join("diarize_pyannote_byo_v1.py");
                let script = r#"
import argparse
import json
import os

try:
    from pyannote.audio import Pipeline
except Exception as e:
    raise RuntimeError("pyannote.audio is required for pyannote_byo_v1") from e


def load_pipeline(pipeline_id, token):
    # API changed across versions; try both call signatures.
    try:
        return Pipeline.from_pretrained(pipeline_id, use_auth_token=token)
    except TypeError:
        return Pipeline.from_pretrained(pipeline_id, token=token)


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--audio", required=True)
    ap.add_argument("--output", required=True)
    ap.add_argument("--pipeline", required=True)
    args = ap.parse_args()

    token = os.environ.get("HF_TOKEN") or os.environ.get("HUGGINGFACE_HUB_TOKEN") or os.environ.get("PYANNOTE_TOKEN")
    pipeline = load_pipeline(args.pipeline, token)

    diar = pipeline(args.audio)
    segments = []
    for turn, _, speaker in diar.itertracks(yield_label=True):
        segments.append(
            {
                "start_ms": int(round(float(turn.start) * 1000.0)),
                "end_ms": int(round(float(turn.end) * 1000.0)),
                "speaker": str(speaker),
            }
        )

    out = {"schema_version": 1, "algorithm": "pyannote_byo_v1", "segments": segments}
    with open(args.output, "w", encoding="utf-8") as f:
        json.dump(out, f, ensure_ascii=False, indent=2)
        f.write("\n")


if __name__ == "__main__":
    main()
"#;
                std::fs::write(&script_path, script)?;

                let mut py_cmd = cmd::command(&python_exe);
                py_cmd.arg(&script_path);
                py_cmd.arg("--audio").arg(&audio_path);
                py_cmd.arg("--output").arg(&diarization_json_path);
                py_cmd.arg("--pipeline").arg(&pipeline);
                py_cmd.env("PYTHONNOUSERSITE", "1");
                py_cmd.env(
                    "XDG_CACHE_HOME",
                    paths.cache_dir().join("python").to_string_lossy().to_string(),
                );
                py_cmd.env(
                    "HF_HOME",
                    paths.python_models_dir().join("hf").to_string_lossy().to_string(),
                );
                py_cmd.env("HF_HUB_DISABLE_TELEMETRY", "1");
                if let Some(token) = token.as_deref() {
                    py_cmd.env("HF_TOKEN", token);
                    py_cmd.env("HUGGINGFACE_HUB_TOKEN", token);
                    py_cmd.env("PYANNOTE_TOKEN", token);
                }

                let output = py_cmd.output().map_err(|e| {
                    EngineError::InstallFailed(format!("failed to run pyannote diarization script: {e}"))
                })?;
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    return Err(EngineError::InstallFailed(format!(
                        "pyannote diarization script failed (code={:?}): {}",
                        output.status.code(),
                        stderr.trim()
                    )));
                }
            } else {
                let venv_python = tools::python_venv_python_path(paths).map_err(|_| {
                    EngineError::InstallFailed(
                        "Python toolchain is not set up. Open Diagnostics -> Tools -> Setup Python toolchain."
                            .to_string(),
                    )
                })?;

                let script_path = artifacts_dir.join("diarize_local_v1.py");

                let script = r#"
import argparse
import json
import math

import numpy as np
import soundfile as sf
from resemblyzer import VoiceEncoder

try:
    from sklearn.cluster import AgglomerativeClustering
    from sklearn.metrics import silhouette_score
except Exception as e:
    raise RuntimeError("scikit-learn is required for clustering; install diarization pack") from e


def choose_k(X, k_min=2, k_max=4):
    n = X.shape[0]
    if n < 2:
        return 1, np.zeros((n,), dtype=np.int64)

    best_k = 1
    best_score = -1.0
    best_labels = np.zeros((n,), dtype=np.int64)

    upper = min(k_max, n)
    for k in range(k_min, upper + 1):
        labels = AgglomerativeClustering(n_clusters=k).fit_predict(X)
        uniq = np.unique(labels)
        if uniq.shape[0] < 2:
            continue
        try:
            score = float(silhouette_score(X, labels))
        except Exception:
            score = -1.0
        if score > best_score:
            best_score = score
            best_k = k
            best_labels = labels.astype(np.int64)

    if best_k == 1:
        return 1, np.zeros((n,), dtype=np.int64)
    return best_k, best_labels


def slices_to_segments(slices, labels, sr):
    segments = []
    if not slices:
        return segments

    cur_label = int(labels[0])
    cur_start = int(slices[0].start)
    cur_end = int(slices[0].stop)

    def emit(start_samp, end_samp, label):
        start_ms = int(round((start_samp / sr) * 1000.0))
        end_ms = int(round((end_samp / sr) * 1000.0))
        if end_ms < start_ms:
            end_ms = start_ms
        segments.append({
            "start_ms": start_ms,
            "end_ms": end_ms,
            "speaker": f"S{label + 1}",
        })

    for i in range(1, len(slices)):
        sl = slices[i]
        label = int(labels[i])
        start = int(sl.start)
        end = int(sl.stop)
        if label == cur_label and start <= cur_end:
            cur_end = max(cur_end, end)
        else:
            emit(cur_start, cur_end, cur_label)
            cur_label = label
            cur_start = start
            cur_end = end

    emit(cur_start, cur_end, cur_label)
    return segments


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--input", required=True)
    ap.add_argument("--output", required=True)
    args = ap.parse_args()

    wav, sr = sf.read(args.input)
    if wav.ndim > 1:
        wav = wav[:, 0]
    wav = wav.astype(np.float32, copy=False)

    if int(sr) != 16000:
        raise RuntimeError(f"expected 16kHz wav; got sr={sr}")

    encoder = VoiceEncoder()
    _, partial_embeds, partial_slices = encoder.embed_utterance(wav, return_partials=True)

    X = np.array(partial_embeds, dtype=np.float32)
    _, labels = choose_k(X, k_min=2, k_max=4)
    segments = slices_to_segments(list(partial_slices), labels, int(sr))

    out = {
        "schema_version": 1,
        "algorithm": "resemblyzer_partials_cluster_v1",
        "segments": segments,
    }

    with open(args.output, "w", encoding="utf-8") as f:
        json.dump(out, f, ensure_ascii=True, indent=2)
        f.write("\n")


if __name__ == "__main__":
    main()
"#;
                std::fs::write(&script_path, script)?;

                log_line(
                    paths,
                    job_id,
                    "info",
                    "diarize_python_begin",
                    serde_json::json!( {
                        "audio_path": &audio_path,
                        "diarization_json_path": &diarization_json_path,
                        "backend": "resemblyzer_partials_cluster_v1"
                    } ),
                )?;

                let mut py_cmd = cmd::command(&venv_python);
                py_cmd.arg(&script_path);
                py_cmd.arg("--input").arg(&audio_path);
                py_cmd.arg("--output").arg(&diarization_json_path);
                py_cmd.env("PYTHONNOUSERSITE", "1");
                py_cmd.env(
                    "XDG_CACHE_HOME",
                    paths.cache_dir().join("python").to_string_lossy().to_string(),
                );
                let output = py_cmd.output().map_err(|e| {
                    EngineError::InstallFailed(format!("failed to run diarize script: {e}"))
                })?;
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    return Err(EngineError::InstallFailed(format!(
                        "diarize script failed (code={:?}): {}",
                        output.status.code(),
                        stderr.trim()
                    )));
                }
            }

            set_progress(paths, job_id, 0.80)?;

            let diar_bytes = std::fs::read(&diarization_json_path)?;
            let diar: DiarizeLocalV1Output = serde_json::from_slice(&diar_bytes)?;
            let _ = diar.schema_version;

            let mut labeled = source_doc.clone();
            for seg in &mut labeled.segments {
                let mut best_speaker: Option<&str> = None;
                let mut best_overlap = 0_i64;
                for d in &diar.segments {
                    let overlap = std::cmp::min(seg.end_ms, d.end_ms)
                        - std::cmp::max(seg.start_ms, d.start_ms);
                    if overlap > best_overlap {
                        best_overlap = overlap;
                        best_speaker = Some(d.speaker.as_str());
                    }
                }
                seg.speaker = best_speaker.map(|s| s.to_string());
            }
            set_progress(paths, job_id, 0.90)?;

            let conn = db::open(paths)?;
            db::migrate(&conn)?;
            let max_version: Option<i64> = conn.query_row(
                r#"
SELECT MAX(version)
FROM subtitle_track
WHERE item_id=?1 AND kind=?2 AND lang=?3 AND format=?4
"#,
                params![
                    &item.id,
                    &source_track.kind,
                    &source_track.lang,
                    &source_track.format
                ],
                |row| row.get(0),
            )?;
            let next_version = max_version.unwrap_or(0) + 1;

            let stem = "source.speakers";
            let json_path = if next_version <= 1 {
                diarize_dir.join(format!("{stem}.json"))
            } else {
                diarize_dir.join(format!("{stem}.v{next_version}.json"))
            };
            let srt_path = if next_version <= 1 {
                diarize_dir.join(format!("{stem}.srt"))
            } else {
                diarize_dir.join(format!("{stem}.v{next_version}.srt"))
            };
            let vtt_path = if next_version <= 1 {
                diarize_dir.join(format!("{stem}.vtt"))
            } else {
                diarize_dir.join(format!("{stem}.v{next_version}.vtt"))
            };

            subtitles::write_artifacts(&labeled, &json_path, &srt_path, &vtt_path)?;
            set_progress(paths, job_id, 0.95)?;

            let track_id = Uuid::new_v4().to_string();
            conn.execute(
                r#"
INSERT INTO subtitle_track (
  id,
  item_id,
  kind,
  lang,
  format,
  path,
  created_by,
  version
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
"#,
                params![
                    &track_id,
                    &item.id,
                    &source_track.kind,
                    &source_track.lang,
                    &source_track.format,
                    json_path.to_string_lossy().to_string(),
                    &created_by,
                    next_version
                ],
            )?;

            log_line(
                paths,
                job_id,
                "info",
                "diarize_done",
                serde_json::json!({
                    "track_id": track_id,
                    "json_path": json_path,
                    "diarization_json_path": diarization_json_path,
                    "segments": diar.segments.len(),
                }),
            )?;
        }
        JobType::TtsPreviewPyttsx3V1 => {
            set_progress(paths, job_id, 0.05)?;
            let p: TtsPreviewPyttsx3V1Params = serde_json::from_str(params_json)?;

            if is_canceled(paths, job_id)? {
                log_line(paths, job_id, "info", "job_canceled", serde_json::json!({}))?;
                return Ok(());
            }

            log_line(
                paths,
                job_id,
                "info",
                "tts_preview_begin",
                serde_json::json!({
                    "item_id": &p.item_id,
                    "source_track_id": &p.source_track_id,
                    "backend": "pyttsx3_v1"
                }),
            )?;

            let pack = tools::tts_preview_pack_status(paths);
            if !pack.installed {
                return Err(EngineError::InstallFailed(
                    "TTS preview pack is not installed. Open Diagnostics -> Tools -> Install TTS preview pack."
                        .to_string(),
                ));
            }

            let source_track = subtitle_tracks::get_track(paths, &p.source_track_id)?;
            if source_track.item_id != p.item_id {
                return Err(EngineError::InstallFailed(format!(
                    "tts preview job item_id mismatch: params.item_id={} track.item_id={}",
                    p.item_id, source_track.item_id
                )));
            }

            let doc = subtitle_tracks::load_document(paths, &p.source_track_id)?;

            let item = library::get_item_by_id(paths, &p.item_id)?;

            let mut voice_id_by_speaker: HashMap<String, String> = HashMap::new();
            let speaker_settings = speakers::list_item_speaker_settings(paths, &item.id)?;
            for s in speaker_settings {
                if let Some(voice_id) = s.tts_voice_id {
                    voice_id_by_speaker.insert(s.speaker_key, voice_id);
                }
            }

            let out_dir = paths
                .derived_item_dir(&item.id)
                .join("tts_preview")
                .join("pyttsx3_v1");
            let segments_dir = out_dir.join("segments");
            std::fs::create_dir_all(&segments_dir)?;
            let manifest_path = out_dir.join("manifest.json");
            if manifest_path.exists() {
                set_progress(paths, job_id, 1.0)?;
                log_line(
                    paths,
                    job_id,
                    "info",
                    "tts_preview_resume_skip_existing",
                    serde_json::json!({ "manifest_path": &manifest_path }),
                )?;

                if p.batch_on_import {
                    let rules = config::load_batch_on_import_rules(paths).unwrap_or_default();
                    if rules.auto_dub_preview
                        && separation_background_exists(paths, &item.id)
                        && !mix_output_exists(paths, &item.id)
                        && !item_has_active_job(paths, &item.id, JobType::MixDubPreviewV1.as_str())
                            .unwrap_or(false)
                    {
                        let batch_id = job_batch_id(paths, job_id).ok().flatten();
                        let params_json = serde_json::to_string(&MixDubPreviewV1Params {
                            item_id: item.id.clone(),
                            ducking_strength: None,
                            loudness_target_lufs: None,
                            timing_fit_enabled: None,
                            timing_fit_min_factor: None,
                            timing_fit_max_factor: None,
                            batch_on_import: true,
                        })?;
                        let _ = enqueue_with_type_item_and_batch_id(
                            paths,
                            JobType::MixDubPreviewV1,
                            params_json,
                            Some(item.id.clone()),
                            batch_id,
                        )?;
                    }
                }

                return Ok(());
            }

            #[derive(Serialize)]
            struct TtsRequestSegment {
                index: u32,
                #[serde(default)]
                speaker: Option<String>,
                #[serde(default)]
                voice_id: Option<String>,
                text: String,
                out_path: String,
            }

            let mut request: Vec<TtsRequestSegment> = Vec::new();
            for seg in &doc.segments {
                let text = seg.text.trim();
                if text.is_empty() {
                    continue;
                }
                let speaker = seg
                    .speaker
                    .as_ref()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
                let voice_id = speaker
                    .as_ref()
                    .and_then(|k| voice_id_by_speaker.get(k).cloned());
                let out_path = segments_dir.join(format!("seg_{:04}.wav", seg.index));
                request.push(TtsRequestSegment {
                    index: seg.index,
                    speaker,
                    voice_id,
                    text: text.to_string(),
                    out_path: out_path.to_string_lossy().to_string(),
                });
            }

            let request_path = artifacts_dir.join("tts_request.json");
            std::fs::write(
                &request_path,
                format!("{}\n", serde_json::to_string_pretty(&request)?),
            )?;

            if is_canceled(paths, job_id)? {
                log_line(paths, job_id, "info", "job_canceled", serde_json::json!({}))?;
                return Ok(());
            }

            let venv_python = tools::python_venv_python_path(paths).map_err(|_| {
                EngineError::InstallFailed(
                    "Python toolchain is not set up. Open Diagnostics -> Tools -> Setup Python toolchain."
                        .to_string(),
                )
            })?;

            let script_path = artifacts_dir.join("tts_pyttsx3_v1.py");
            let script = r#"
import argparse
import json
import os

import pyttsx3


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--request", required=True)
    args = ap.parse_args()

    with open(args.request, "r", encoding="utf-8") as f:
        items = json.load(f)

    engine = pyttsx3.init()
    default_voice = None
    try:
        default_voice = engine.getProperty("voice")
    except Exception:
        default_voice = None
    if default_voice is not None:
        default_voice = (str(default_voice).strip() or None)

    current_voice = default_voice or ""

    def flush_queue():
        try:
            engine.runAndWait()
        except Exception:
            pass

    for it in items:
        text = (it.get("text") or "").strip()
        out_path = (it.get("out_path") or "").strip()
        voice_id = (it.get("voice_id") or "").strip()
        if not text or not out_path:
            continue

        desired_voice = voice_id if voice_id else (default_voice or "")
        if desired_voice != current_voice:
            flush_queue()
            if desired_voice:
                try:
                    engine.setProperty("voice", desired_voice)
                    current_voice = desired_voice
                except Exception:
                    current_voice = desired_voice
            else:
                # If we can't restore a known default voice id, re-init the engine to reset state.
                try:
                    engine = pyttsx3.init()
                except Exception:
                    pass
                try:
                    default_voice = engine.getProperty("voice")
                except Exception:
                    default_voice = None
                if default_voice is not None:
                    default_voice = (str(default_voice).strip() or None)
                current_voice = default_voice or ""

        out_dir = os.path.dirname(out_path)
        if out_dir:
            os.makedirs(out_dir, exist_ok=True)
        engine.save_to_file(text, out_path)

    flush_queue()


if __name__ == "__main__":
    main()
"#;
            std::fs::write(&script_path, script)?;

            log_line(
                paths,
                job_id,
                "info",
                "tts_preview_python_begin",
                serde_json::json!({ "request_path": &request_path, "segments": request.len() }),
            )?;

            let mut py_cmd = cmd::command(&venv_python);
            py_cmd.arg(&script_path);
            py_cmd.arg("--request").arg(&request_path);
            py_cmd.env("PYTHONNOUSERSITE", "1");
            py_cmd.env(
                "XDG_CACHE_HOME",
                paths.cache_dir().join("python").to_string_lossy().to_string(),
            );
            let output = py_cmd.output().map_err(|e| {
                EngineError::InstallFailed(format!("failed to run pyttsx3 script: {e}"))
            })?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(EngineError::InstallFailed(format!(
                    "pyttsx3 script failed (code={:?}): {}",
                    output.status.code(),
                    stderr.trim()
                )));
            }
            set_progress(paths, job_id, 0.80)?;

            #[derive(Serialize)]
            struct TtsManifestSegment {
                index: u32,
                start_ms: i64,
                end_ms: i64,
                speaker: Option<String>,
                #[serde(default)]
                tts_voice_id: Option<String>,
                text: String,
                audio_path: Option<String>,
                audio_exists: bool,
            }

            #[derive(Serialize)]
            struct TtsManifest {
                schema_version: u32,
                backend: String,
                item_id: String,
                track_id: String,
                segments: Vec<TtsManifestSegment>,
            }

            let mut manifest_segments: Vec<TtsManifestSegment> = Vec::new();
            for seg in &doc.segments {
                let audio_path = segments_dir.join(format!("seg_{:04}.wav", seg.index));
                let exists = audio_path.exists();
                let speaker = seg
                    .speaker
                    .as_ref()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
                let tts_voice_id = speaker
                    .as_ref()
                    .and_then(|k| voice_id_by_speaker.get(k).cloned());
                manifest_segments.push(TtsManifestSegment {
                    index: seg.index,
                    start_ms: seg.start_ms,
                    end_ms: seg.end_ms,
                    speaker,
                    tts_voice_id,
                    text: seg.text.clone(),
                    audio_path: if exists {
                        Some(audio_path.to_string_lossy().to_string())
                    } else {
                        None
                    },
                    audio_exists: exists,
                });
            }

            let manifest = TtsManifest {
                schema_version: 1,
                backend: "pyttsx3_v1".to_string(),
                item_id: item.id.clone(),
                track_id: source_track.id.clone(),
                segments: manifest_segments,
            };

            std::fs::write(
                &manifest_path,
                format!("{}\n", serde_json::to_string_pretty(&manifest)?),
            )?;
            set_progress(paths, job_id, 0.95)?;

            log_line(
                paths,
                job_id,
                "info",
                "tts_preview_done",
                serde_json::json!({
                    "manifest_path": &manifest_path,
                    "segments_dir": &segments_dir
                }),
            )?;

            if p.batch_on_import {
                let rules = config::load_batch_on_import_rules(paths).unwrap_or_default();
                if rules.auto_dub_preview
                    && separation_background_exists(paths, &item.id)
                    && !mix_output_exists(paths, &item.id)
                    && !item_has_active_job(paths, &item.id, JobType::MixDubPreviewV1.as_str())
                        .unwrap_or(false)
                {
                    let batch_id = job_batch_id(paths, job_id).ok().flatten();
                    let params_json = serde_json::to_string(&MixDubPreviewV1Params {
                        item_id: item.id.clone(),
                        ducking_strength: None,
                        loudness_target_lufs: None,
                        timing_fit_enabled: None,
                        timing_fit_min_factor: None,
                        timing_fit_max_factor: None,
                        batch_on_import: true,
                    })?;
                    let _ = enqueue_with_type_item_and_batch_id(
                        paths,
                        JobType::MixDubPreviewV1,
                        params_json,
                        Some(item.id.clone()),
                        batch_id,
                    )?;
                }
            }
        }
        JobType::TtsNeuralLocalV1 => {
            set_progress(paths, job_id, 0.05)?;
            let p: TtsNeuralLocalV1Params = serde_json::from_str(params_json)?;

            if is_canceled(paths, job_id)? {
                log_line(paths, job_id, "info", "job_canceled", serde_json::json!({}))?;
                return Ok(());
            }

            log_line(
                paths,
                job_id,
                "info",
                "tts_preview_begin",
                serde_json::json!({
                    "item_id": &p.item_id,
                    "source_track_id": &p.source_track_id,
                    "backend": "neural_local_v1"
                }),
            )?;

            let pack = tools::tts_neural_local_v1_pack_status(paths);
            if !pack.installed {
                return Err(EngineError::InstallFailed(
                    "Neural TTS local pack is not installed. Open Diagnostics -> Tools -> Install Neural TTS local pack."
                        .to_string(),
                ));
            }

            let source_track = subtitle_tracks::get_track(paths, &p.source_track_id)?;
            if source_track.item_id != p.item_id {
                return Err(EngineError::InstallFailed(format!(
                    "tts preview job item_id mismatch: params.item_id={} track.item_id={}",
                    p.item_id, source_track.item_id
                )));
            }

            let doc = subtitle_tracks::load_document(paths, &p.source_track_id)?;
            let item = library::get_item_by_id(paths, &p.item_id)?;

            let mut voice_id_by_speaker: HashMap<String, String> = HashMap::new();
            let speaker_settings = speakers::list_item_speaker_settings(paths, &item.id)?;
            for s in speaker_settings {
                if let Some(voice_id) = s.tts_voice_id {
                    voice_id_by_speaker.insert(s.speaker_key, voice_id);
                }
            }

            let out_dir = paths
                .derived_item_dir(&item.id)
                .join("tts_preview")
                .join("tts_neural_local_v1");
            let segments_dir = out_dir.join("segments");
            std::fs::create_dir_all(&segments_dir)?;
            let manifest_path = out_dir.join("manifest.json");
            if manifest_path.exists() {
                set_progress(paths, job_id, 1.0)?;
                log_line(
                    paths,
                    job_id,
                    "info",
                    "tts_preview_resume_skip_existing",
                    serde_json::json!({ "manifest_path": &manifest_path }),
                )?;

                if p.batch_on_import {
                    let rules = config::load_batch_on_import_rules(paths).unwrap_or_default();
                    if rules.auto_dub_preview
                        && separation_background_exists(paths, &item.id)
                        && !mix_output_exists(paths, &item.id)
                        && !item_has_active_job(paths, &item.id, JobType::MixDubPreviewV1.as_str())
                            .unwrap_or(false)
                    {
                        let batch_id = job_batch_id(paths, job_id).ok().flatten();
                        let params_json = serde_json::to_string(&MixDubPreviewV1Params {
                            item_id: item.id.clone(),
                            ducking_strength: None,
                            loudness_target_lufs: None,
                            timing_fit_enabled: None,
                            timing_fit_min_factor: None,
                            timing_fit_max_factor: None,
                            batch_on_import: true,
                        })?;
                        let _ = enqueue_with_type_item_and_batch_id(
                            paths,
                            JobType::MixDubPreviewV1,
                            params_json,
                            Some(item.id.clone()),
                            batch_id,
                        )?;
                    }
                }

                return Ok(());
            }

            #[derive(Serialize)]
            struct TtsRequestSegment {
                index: u32,
                #[serde(default)]
                speaker: Option<String>,
                #[serde(default)]
                voice_id: Option<String>,
                text: String,
                out_path: String,
            }

            let mut request: Vec<TtsRequestSegment> = Vec::new();
            for seg in &doc.segments {
                let text = seg.text.trim();
                if text.is_empty() {
                    continue;
                }
                let speaker = seg
                    .speaker
                    .as_ref()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
                let voice_id = speaker
                    .as_ref()
                    .and_then(|k| voice_id_by_speaker.get(k).cloned());
                let out_path = segments_dir.join(format!("seg_{:04}.wav", seg.index));
                request.push(TtsRequestSegment {
                    index: seg.index,
                    speaker,
                    voice_id,
                    text: text.to_string(),
                    out_path: out_path.to_string_lossy().to_string(),
                });
            }

            let request_path = artifacts_dir.join("tts_request_neural_v1.json");
            std::fs::write(
                &request_path,
                format!("{}\n", serde_json::to_string_pretty(&request)?),
            )?;

            if is_canceled(paths, job_id)? {
                log_line(paths, job_id, "info", "job_canceled", serde_json::json!({}))?;
                return Ok(());
            }

            let venv_python = tools::python_venv_python_path(paths).map_err(|_| {
                EngineError::InstallFailed(
                    "Python toolchain is not set up. Open Diagnostics -> Tools -> Setup Python toolchain."
                        .to_string(),
                )
            })?;

            let script_path = artifacts_dir.join("tts_neural_local_v1.py");
            let script = r##"
import argparse
import json
import os
from typing import Any, Iterable, Optional, Tuple

import numpy as np
import soundfile as sf

try:
    from kokoro import KPipeline
except Exception as e:
    raise RuntimeError("kokoro package is required for neural TTS") from e


def chunks_from_output(output: Any) -> Iterable[Tuple[np.ndarray, Optional[int]]]:
    if output is None:
        return []

    if isinstance(output, tuple) and len(output) > 0:
        chunks = [output]
    elif isinstance(output, list):
        chunks = output
    else:
        try:
            chunks = list(output)
        except TypeError:
            chunks = [output]

    for chunk in chunks:
        if chunk is None:
            continue
        if isinstance(chunk, dict):
            audio = chunk.get("audio") or chunk.get("waveform")
            sr = chunk.get("sample_rate") or chunk.get("sample_rate_hz") or chunk.get("sr")
            if audio is not None:
                yield np.asarray(audio, dtype=np.float32), int(sr) if sr is not None else None
            continue

        if isinstance(chunk, tuple) or isinstance(chunk, list):
            if len(chunk) == 2 and isinstance(chunk[1], (int, float, np.integer)):
                yield np.asarray(chunk[0], dtype=np.float32), int(chunk[1])
                continue
            if len(chunk) >= 3:
                audio = chunk[1]
                sr = chunk[2]
                if isinstance(sr, (int, float, np.integer)) and audio is not None:
                    yield np.asarray(audio, dtype=np.float32), int(sr)
                continue

        if isinstance(chunk, np.ndarray):
            yield chunk.astype(np.float32), None


def synthesize(
    pipeline: Any,
    text: str,
    out_path: str,
    voice_id: str,
) -> None:
    args = {}
    if voice_id:
        args["voice"] = voice_id
    tries = [dict(args), dict(args)]
    if "voice" in args:
        tries.append({"speaker": voice_id})
    tries.append({})

    out_dir = os.path.dirname(out_path)
    if out_dir:
        os.makedirs(out_dir, exist_ok=True)

    last_error = None
    for call_kwargs in tries:
        try:
            output = pipeline(text, **call_kwargs)
            pieces = []
            sample_rate = None

            for piece in chunks_from_output(output):
                arr, sr = piece
                if arr.size == 0:
                    continue
                pieces.append(arr)
                if sample_rate is None and sr is not None:
                    sample_rate = sr

            if not pieces:
                raise RuntimeError("pipeline produced no chunks")

            audio = np.concatenate(pieces, axis=0).astype(np.float32)
            sf.write(out_path, audio, sample_rate if sample_rate is not None else 24000)
            return
        except Exception as e:
            last_error = e

    raise RuntimeError(f"synthesis failed for '{text[:40]}': {last_error}")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--request", required=True)
    args = parser.parse_args()

    with open(args.request, "r", encoding="utf-8") as f:
        items = json.load(f)

    try:
        try:
            pipeline = KPipeline(lang_code="a")
        except TypeError:
            pipeline = KPipeline("a")
    except TypeError:
        pipeline = KPipeline()

    for item in items:
        text = (item.get("text") or "").strip()
        out_path = (item.get("out_path") or "").strip()
        voice_id = (item.get("voice_id") or "").strip()
        if not text or not out_path:
            continue
        synthesize(pipeline, text, out_path, voice_id)


if __name__ == "__main__":
    main()
"##;
            std::fs::write(&script_path, script)?;

            log_line(
                paths,
                job_id,
                "info",
                "tts_preview_neural_python_begin",
                serde_json::json!({ "request_path": &request_path, "segments": request.len() }),
            )?;

            let mut py_cmd = cmd::command(&venv_python);
            py_cmd.arg(&script_path);
            py_cmd.arg("--request").arg(&request_path);
            py_cmd.env("PYTHONNOUSERSITE", "1");
            py_cmd.env(
                "XDG_CACHE_HOME",
                paths.cache_dir().join("python").to_string_lossy().to_string(),
            );
            py_cmd.env(
                "HF_HOME",
                paths.cache_dir().join("huggingface").to_string_lossy().to_string(),
            );
            py_cmd.env(
                "HUGGINGFACE_HUB_CACHE",
                paths.cache_dir()
                    .join("huggingface")
                    .join("hub")
                    .to_string_lossy()
                    .to_string(),
            );
            py_cmd.env("HF_HUB_OFFLINE", "1");
            py_cmd.env("TRANSFORMERS_OFFLINE", "1");
            let output = py_cmd.output().map_err(|e| {
                EngineError::InstallFailed(format!("failed to run neural TTS script: {e}"))
            })?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(EngineError::InstallFailed(format!(
                    "neural TTS script failed (code={:?}): {}",
                    output.status.code(),
                    stderr.trim()
                )));
            }
            set_progress(paths, job_id, 0.80)?;

            #[derive(Serialize)]
            struct TtsManifestSegment {
                index: u32,
                start_ms: i64,
                end_ms: i64,
                speaker: Option<String>,
                #[serde(default)]
                tts_voice_id: Option<String>,
                text: String,
                audio_path: Option<String>,
                audio_exists: bool,
            }

            #[derive(Serialize)]
            struct TtsManifest {
                schema_version: u32,
                backend: String,
                item_id: String,
                track_id: String,
                segments: Vec<TtsManifestSegment>,
            }

            let mut manifest_segments: Vec<TtsManifestSegment> = Vec::new();
            for seg in &doc.segments {
                let audio_path = segments_dir.join(format!("seg_{:04}.wav", seg.index));
                let exists = audio_path.exists();
                let speaker = seg
                    .speaker
                    .as_ref()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
                let tts_voice_id = speaker
                    .as_ref()
                    .and_then(|k| voice_id_by_speaker.get(k).cloned());
                manifest_segments.push(TtsManifestSegment {
                    index: seg.index,
                    start_ms: seg.start_ms,
                    end_ms: seg.end_ms,
                    speaker,
                    tts_voice_id,
                    text: seg.text.clone(),
                    audio_path: if exists {
                        Some(audio_path.to_string_lossy().to_string())
                    } else {
                        None
                    },
                    audio_exists: exists,
                });
            }

            let manifest = TtsManifest {
                schema_version: 1,
                backend: "neural_local_v1".to_string(),
                item_id: item.id.clone(),
                track_id: source_track.id.clone(),
                segments: manifest_segments,
            };

            std::fs::write(
                &manifest_path,
                format!("{}\n", serde_json::to_string_pretty(&manifest)?),
            )?;
            set_progress(paths, job_id, 0.95)?;

            log_line(
                paths,
                job_id,
                "info",
                "tts_preview_done",
                serde_json::json!({
                    "manifest_path": &manifest_path,
                    "segments_dir": &segments_dir
                }),
            )?;

            if p.batch_on_import {
                let rules = config::load_batch_on_import_rules(paths).unwrap_or_default();
                if rules.auto_dub_preview
                    && separation_background_exists(paths, &item.id)
                    && !mix_output_exists(paths, &item.id)
                    && !item_has_active_job(paths, &item.id, JobType::MixDubPreviewV1.as_str())
                        .unwrap_or(false)
                {
                    let batch_id = job_batch_id(paths, job_id).ok().flatten();
                    let params_json = serde_json::to_string(&MixDubPreviewV1Params {
                        item_id: item.id.clone(),
                        ducking_strength: None,
                        loudness_target_lufs: None,
                        timing_fit_enabled: None,
                        timing_fit_min_factor: None,
                        timing_fit_max_factor: None,
                        batch_on_import: true,
                    })?;
                    let _ = enqueue_with_type_item_and_batch_id(
                        paths,
                        JobType::MixDubPreviewV1,
                        params_json,
                        Some(item.id.clone()),
                        batch_id,
                    )?;
                }
            }
        }
        JobType::DubVoicePreservingV1 => {
            set_progress(paths, job_id, 0.05)?;
            let p: DubVoicePreservingV1Params = serde_json::from_str(params_json)?;

            if is_canceled(paths, job_id)? {
                log_line(paths, job_id, "info", "job_canceled", serde_json::json!({}))?;
                return Ok(());
            }

            log_line(
                paths,
                job_id,
                "info",
                "tts_preview_begin",
                serde_json::json!({
                    "item_id": &p.item_id,
                    "source_track_id": &p.source_track_id,
                    "backend": "voice_preserving_local_v1"
                }),
            )?;

            let pack = tools::tts_voice_preserving_local_v1_pack_status(paths);
            if !pack.installed {
                return Err(EngineError::InstallFailed(
                    "Voice-preserving TTS pack is not installed. Open Diagnostics -> Tools -> Install voice-preserving TTS pack."
                        .to_string(),
                ));
            }

            let neural_pack = tools::tts_neural_local_v1_pack_status(paths);
            if !neural_pack.installed {
                return Err(EngineError::InstallFailed(
                    "Neural TTS pack is not installed (Kokoro is required as the base TTS stage). Open Diagnostics -> Tools -> Install neural TTS pack."
                        .to_string(),
                ));
            }

            let ffmpeg = tools::ffmpeg_tools_status(paths);
            if ffmpeg.ffmpeg_version.is_none() {
                return Err(EngineError::InstallFailed(
                    "FFmpeg tools are not available. Open Diagnostics -> Tools -> Install FFmpeg tools."
                        .to_string(),
                ));
            }

            let source_track = subtitle_tracks::get_track(paths, &p.source_track_id)?;
            if source_track.item_id != p.item_id {
                return Err(EngineError::InstallFailed(format!(
                    "tts preview job item_id mismatch: params.item_id={} track.item_id={}",
                    p.item_id, source_track.item_id
                )));
            }

            let doc = subtitle_tracks::load_document(paths, &p.source_track_id)?;
            let item = library::get_item_by_id(paths, &p.item_id)?;

            let mut tts_voice_profile_path_by_speaker: HashMap<String, String> = HashMap::new();
            let speaker_settings = speakers::list_item_speaker_settings(paths, &item.id)?;
            for s in speaker_settings {
                if let Some(voice_profile_path) = s.tts_voice_profile_path {
                    tts_voice_profile_path_by_speaker.insert(s.speaker_key, voice_profile_path);
                }
            }

            let out_dir = paths
                .derived_item_dir(&item.id)
                .join("tts_preview")
                .join("dub_voice_preserving_v1");
            let segments_dir = out_dir.join("segments");
            let base_segments_dir = out_dir.join("base_segments");
            std::fs::create_dir_all(&segments_dir)?;
            std::fs::create_dir_all(&base_segments_dir)?;

            #[derive(Serialize)]
            struct TtsRequestSegment {
                index: u32,
                #[serde(default)]
                speaker: Option<String>,
                #[serde(default)]
                tts_voice_profile_path: Option<String>,
                start_ms: i64,
                end_ms: i64,
                text: String,
                base_out_path: String,
                out_path: String,
            }

            let mut request: Vec<TtsRequestSegment> = Vec::new();
            for seg in &doc.segments {
                let text = seg.text.trim();
                if text.is_empty() {
                    continue;
                }
                let speaker = seg
                    .speaker
                    .as_ref()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
                let tts_voice_profile_path = speaker
                    .as_ref()
                    .and_then(|k| tts_voice_profile_path_by_speaker.get(k).cloned());
                let base_out_path = base_segments_dir.join(format!("seg_{:04}.wav", seg.index));
                let out_path = segments_dir.join(format!("seg_{:04}.wav", seg.index));
                request.push(TtsRequestSegment {
                    index: seg.index,
                    speaker,
                    tts_voice_profile_path,
                    start_ms: seg.start_ms,
                    end_ms: seg.end_ms,
                    text: text.to_string(),
                    base_out_path: base_out_path.to_string_lossy().to_string(),
                    out_path: out_path.to_string_lossy().to_string(),
                });
            }

            let request_path = artifacts_dir.join("tts_voice_preserving_request.json");
            std::fs::write(
                &request_path,
                format!("{}\n", serde_json::to_string_pretty(&request)?),
            )?;

            if is_canceled(paths, job_id)? {
                log_line(paths, job_id, "info", "job_canceled", serde_json::json!({}))?;
                return Ok(());
            }

            let venv_python = tools::python_venv_python_path(paths).map_err(|_| {
                EngineError::InstallFailed(
                    "Python toolchain is not set up. Open Diagnostics -> Tools -> Setup Python toolchain."
                        .to_string(),
                )
            })?;

            let script_path = artifacts_dir.join("tts_voice_preserving_v1.py");
            let script = r###"
import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import time
from typing import Any, Iterable, Optional, Tuple

import numpy as np
import soundfile as sf

try:
    import torch
except Exception as e:
    raise RuntimeError("torch is required for voice-preserving dubbing") from e

try:
    from kokoro import KPipeline
except Exception as e:
    raise RuntimeError("kokoro package is required for voice-preserving dubbing") from e

try:
    from openvoice.api import ToneColorConverter
except Exception as e:
    raise RuntimeError("openvoice package is required for voice-preserving dubbing") from e


def file_exists(path: str) -> bool:
    try:
        return os.path.isfile(path) and os.path.getsize(path) > 0
    except Exception:
        return False


def safe_slug(value: str) -> str:
    value = (value or "").strip()
    if not value:
        return "speaker"
    return re.sub(r"[^a-zA-Z0-9_-]+", "_", value)[:64] or "speaker"


def run_ffmpeg_convert(ffmpeg_cmd: str, in_path: str, out_path: str) -> str:
    if not ffmpeg_cmd:
        return in_path
    out_dir = os.path.dirname(out_path)
    if out_dir:
        os.makedirs(out_dir, exist_ok=True)
    cmd = [
        ffmpeg_cmd,
        "-y",
        "-hide_banner",
        "-loglevel",
        "error",
        "-i",
        in_path,
        "-vn",
        "-ac",
        "1",
        "-ar",
        "16000",
        out_path,
    ]
    subprocess.run(cmd, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    return out_path if file_exists(out_path) else in_path


def chunks_from_output(output: Any) -> Iterable[Tuple[np.ndarray, Optional[int]]]:
    if output is None:
        return []

    if isinstance(output, tuple) and len(output) > 0:
        chunks = [output]
    elif isinstance(output, list):
        chunks = output
    else:
        try:
            chunks = list(output)
        except TypeError:
            chunks = [output]

    for chunk in chunks:
        if chunk is None:
            continue
        if isinstance(chunk, dict):
            audio = chunk.get("audio") or chunk.get("waveform")
            sr = chunk.get("sample_rate") or chunk.get("sample_rate_hz") or chunk.get("sr")
            if audio is not None:
                yield np.asarray(audio, dtype=np.float32), int(sr) if sr is not None else None
            continue

        if isinstance(chunk, tuple) or isinstance(chunk, list):
            if len(chunk) == 2 and isinstance(chunk[1], (int, float, np.integer)):
                yield np.asarray(chunk[0], dtype=np.float32), int(chunk[1])
                continue
            if len(chunk) >= 3:
                audio = chunk[1]
                sr = chunk[2]
                if isinstance(sr, (int, float, np.integer)) and audio is not None:
                    yield np.asarray(audio, dtype=np.float32), int(sr)
                continue

        if isinstance(chunk, np.ndarray):
            yield chunk.astype(np.float32), None


def kokoro_synthesize(pipeline: Any, text: str, out_path: str, voice_id: str = "") -> None:
    out_dir = os.path.dirname(out_path)
    if out_dir:
        os.makedirs(out_dir, exist_ok=True)

    args: dict[str, Any] = {}
    if voice_id:
        args["voice"] = voice_id
    tries = [dict(args), dict(args)]
    if "voice" in args:
        tries.append({"speaker": voice_id})
    tries.append({})

    last_error: Optional[BaseException] = None
    for call_kwargs in tries:
        try:
            output = pipeline(text, **call_kwargs)
            pieces = []
            sample_rate = None
            for arr, sr in chunks_from_output(output):
                if arr.size == 0:
                    continue
                pieces.append(arr)
                if sample_rate is None and sr is not None:
                    sample_rate = sr
            if not pieces:
                raise RuntimeError("kokoro produced no chunks")
            audio = np.concatenate(pieces, axis=0).astype(np.float32)
            sf.write(out_path, audio, sample_rate if sample_rate is not None else 24000)
            return
        except Exception as e:
            last_error = e

    raise RuntimeError(f"kokoro synthesis failed for '{text[:40]}': {last_error}")


def load_converter(models_dir: str, device: str) -> Any:
    config_path = os.path.join(models_dir, "converter", "config.json")
    ckpt_path = os.path.join(models_dir, "converter", "checkpoint.pth")
    if not os.path.isfile(config_path):
        raise RuntimeError(f"OpenVoice config not found: {config_path}")
    if not os.path.isfile(ckpt_path):
        raise RuntimeError(f"OpenVoice checkpoint not found: {ckpt_path}")

    try:
        converter = ToneColorConverter(config_path, device=device, enable_watermark=False)
    except TypeError as e:
        raise RuntimeError("ToneColorConverter must support enable_watermark=False") from e

    for attr in ("watermark_model", "watermark_detector"):
        if hasattr(converter, attr):
            try:
                setattr(converter, attr, None)
            except Exception:
                pass

    if not hasattr(converter, "load_ckpt"):
        raise RuntimeError("ToneColorConverter missing load_ckpt()")
    converter.load_ckpt(ckpt_path)
    return converter


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--request", required=True)
    ap.add_argument("--models-dir", required=True)
    ap.add_argument("--ffmpeg", default="")
    ap.add_argument("--report", required=True)
    args = ap.parse_args()

    with open(args.request, "r", encoding="utf-8") as f:
        items = json.load(f)

    try:
        try:
            pipeline = KPipeline(lang_code="a")
        except TypeError:
            pipeline = KPipeline("a")
    except TypeError:
        pipeline = KPipeline()

    device = "cuda" if torch.cuda.is_available() else "cpu"
    converter = load_converter(args.models_dir, device=device)

    report_dir = os.path.dirname(os.path.abspath(args.report))
    tmp_root = os.path.join(report_dir, "voice_preserving_tmp")
    os.makedirs(tmp_root, exist_ok=True)

    speaker_profile: dict[str, str] = {}
    for item in items:
        speaker = (item.get("speaker") or "").strip()
        profile = (item.get("tts_voice_profile_path") or "").strip()
        if not speaker or not profile:
            continue
        if not os.path.exists(profile):
            continue
        speaker_profile.setdefault(speaker, profile)

    speaker_se: dict[str, Any] = {}
    for speaker, profile in speaker_profile.items():
        try:
            ref_wav = run_ffmpeg_convert(
                args.ffmpeg,
                profile,
                os.path.join(tmp_root, f"ref_{safe_slug(speaker)}.wav"),
            )
            speaker_se[speaker] = converter.extract_se([ref_wav])
        except Exception as e:
            print(
                f"WARN speaker_embedding_failed speaker={speaker!r} profile={profile!r} err={e}",
                file=sys.stderr,
            )

    segments = []
    convert_ok = 0
    base_ok = 0

    for item in items:
        idx = item.get("index")
        speaker = (item.get("speaker") or "").strip()
        text = (item.get("text") or "").strip()
        out_path = (item.get("out_path") or "").strip()
        base_out_path = (item.get("base_out_path") or "").strip()
        if not text or not out_path or not base_out_path:
            continue

        seg_rec = {
            "index": idx,
            "speaker": speaker or None,
            "text_len": len(text),
            "base_out_path": base_out_path,
            "out_path": out_path,
            "used_voice_preserving": False,
            "error": None,
        }

        try:
            kokoro_synthesize(pipeline, text, base_out_path, voice_id="")
            base_ok += 1

            tgt_se = speaker_se.get(speaker)
            if tgt_se is not None:
                try:
                    src_se = converter.extract_se([base_out_path])
                    converter.convert(
                        audio_src_path=base_out_path,
                        src_se=src_se,
                        tgt_se=tgt_se,
                        output_path=out_path,
                        message="",
                    )
                    if file_exists(out_path):
                        convert_ok += 1
                        seg_rec["used_voice_preserving"] = True
                    else:
                        raise RuntimeError("convert produced no output")
                except Exception as e:
                    seg_rec["error"] = f"convert_failed: {e}"

            if not file_exists(out_path):
                os.makedirs(os.path.dirname(out_path), exist_ok=True)
                shutil.copyfile(base_out_path, out_path)
        except Exception as e:
            seg_rec["error"] = seg_rec["error"] or f"segment_failed: {e}"
            if base_out_path and not file_exists(base_out_path):
                os.makedirs(os.path.dirname(base_out_path), exist_ok=True)
                sf.write(base_out_path, np.zeros((24000 // 5,), dtype=np.float32), 24000)
            if (
                out_path
                and not file_exists(out_path)
                and base_out_path
                and file_exists(base_out_path)
            ):
                os.makedirs(os.path.dirname(out_path), exist_ok=True)
                shutil.copyfile(base_out_path, out_path)

        segments.append(seg_rec)

    report = {
        "schema_version": 1,
        "created_at_ms": int(time.time() * 1000),
        "device": device,
        "segments_total": len(segments),
        "segments_base_ok": base_ok,
        "segments_converted_ok": convert_ok,
        "speakers_with_profiles": sorted(list(speaker_profile.keys())),
        "speakers_with_embeddings": sorted(list(speaker_se.keys())),
        "segments": segments,
    }

    with open(args.report, "w", encoding="utf-8") as f:
        json.dump(report, f, ensure_ascii=False, indent=2)


if __name__ == "__main__":
    main()
"###;
            std::fs::write(&script_path, script)?;

            log_line(
                paths,
                job_id,
                "info",
                "tts_preview_voice_preserving_python_begin",
                serde_json::json!({ "request_path": &request_path, "segments": request.len() }),
            )?;

            let mut py_cmd = cmd::command(&venv_python);
            py_cmd.arg(&script_path);
            py_cmd.arg("--request").arg(&request_path);
            py_cmd
                .arg("--models-dir")
                .arg(paths.python_models_dir().join("openvoice_v2"));
            py_cmd.arg("--ffmpeg").arg(paths.ffmpeg_cmd());
            py_cmd
                .arg("--report")
                .arg(artifacts_dir.join("tts_voice_preserving_report.json"));
            py_cmd.env("PYTHONNOUSERSITE", "1");
            py_cmd.env(
                "XDG_CACHE_HOME",
                paths.cache_dir().join("python").to_string_lossy().to_string(),
            );
            py_cmd.env(
                "HF_HOME",
                paths.cache_dir().join("huggingface").to_string_lossy().to_string(),
            );
            py_cmd.env(
                "HUGGINGFACE_HUB_CACHE",
                paths.cache_dir()
                    .join("huggingface")
                    .join("hub")
                    .to_string_lossy()
                    .to_string(),
            );
            py_cmd.env("HF_HUB_OFFLINE", "1");
            py_cmd.env("TRANSFORMERS_OFFLINE", "1");
            let output = py_cmd.output().map_err(|e| {
                EngineError::InstallFailed(format!("failed to run voice-preserving TTS script: {e}"))
            })?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(EngineError::InstallFailed(format!(
                    "voice-preserving TTS script failed (code={:?}): {}",
                    output.status.code(),
                    stderr.trim()
                )));
            }
            set_progress(paths, job_id, 0.80)?;

            #[derive(Serialize)]
            struct TtsManifestSegment {
                index: u32,
                start_ms: i64,
                end_ms: i64,
                speaker: Option<String>,
                #[serde(default)]
                tts_voice_profile_path: Option<String>,
                text: String,
                audio_path: Option<String>,
                audio_exists: bool,
            }

            #[derive(Serialize)]
            struct TtsManifest {
                schema_version: u32,
                backend: String,
                item_id: String,
                track_id: String,
                segments: Vec<TtsManifestSegment>,
            }

            let mut manifest_segments: Vec<TtsManifestSegment> = Vec::new();
            for seg in &doc.segments {
                let audio_path = segments_dir.join(format!("seg_{:04}.wav", seg.index));
                let exists = audio_path.exists();
                let speaker = seg
                    .speaker
                    .as_ref()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
                let tts_voice_profile_path = speaker
                    .as_ref()
                    .and_then(|k| tts_voice_profile_path_by_speaker.get(k).cloned());
                manifest_segments.push(TtsManifestSegment {
                    index: seg.index,
                    start_ms: seg.start_ms,
                    end_ms: seg.end_ms,
                    speaker,
                    tts_voice_profile_path,
                    text: seg.text.clone(),
                    audio_path: if exists {
                        Some(audio_path.to_string_lossy().to_string())
                    } else {
                        None
                    },
                    audio_exists: exists,
                });
            }

            let manifest = TtsManifest {
                schema_version: 1,
                backend: "voice_preserving_local_v1".to_string(),
                item_id: item.id.clone(),
                track_id: source_track.id.clone(),
                segments: manifest_segments,
            };

            let manifest_path = out_dir.join("manifest.json");
            std::fs::write(
                &manifest_path,
                format!("{}\n", serde_json::to_string_pretty(&manifest)?),
            )?;
            set_progress(paths, job_id, 0.95)?;

            log_line(
                paths,
                job_id,
                "info",
                "tts_preview_done",
                serde_json::json!({
                    "manifest_path": &manifest_path,
                    "segments_dir": &segments_dir
                }),
            )?;
        }
        JobType::MixDubPreviewV1 => {
            set_progress(paths, job_id, 0.05)?;
            let p: MixDubPreviewV1Params = serde_json::from_str(params_json)?;

            if is_canceled(paths, job_id)? {
                log_line(paths, job_id, "info", "job_canceled", serde_json::json!({}))?;
                return Ok(());
            }

            log_line(
                paths,
                job_id,
                "info",
                "mix_dub_preview_begin",
                serde_json::json!({ "item_id": &p.item_id }),
            )?;

            let item = library::get_item_by_id(paths, &p.item_id)?;
            let item_dir = paths.derived_item_dir(&item.id);

            let background_path =
                separation_background_path_best_effort(paths, &item.id).ok_or_else(|| {
                    EngineError::InstallFailed(
                        "background stem not found; run Separate first (Spleeter or Demucs)"
                            .to_string(),
                    )
                })?;

            let voice_preserving_manifest = item_dir
                .join("tts_preview")
                .join("dub_voice_preserving_v1")
                .join("manifest.json");
            let neural_manifest = item_dir
                .join("tts_preview")
                .join("tts_neural_local_v1")
                .join("manifest.json");
            let pyttsx3_manifest = item_dir
                .join("tts_preview")
                .join("pyttsx3_v1")
                .join("manifest.json");

            let manifest_path = if voice_preserving_manifest.exists() {
                voice_preserving_manifest
            } else if neural_manifest.exists() {
                neural_manifest
            } else if pyttsx3_manifest.exists() {
                pyttsx3_manifest
            } else {
                neural_manifest
            };
            if !manifest_path.exists() {
                return Err(EngineError::InstallFailed(
                    "TTS manifest not found; run TTS preview or voice-preserving dub first".to_string(),
                ));
            }

            let manifest_bytes = std::fs::read(&manifest_path)?;
            let manifest: TtsPreviewManifest = serde_json::from_slice(&manifest_bytes)?;

            let out_dir = item_dir.join("dub_preview");
            std::fs::create_dir_all(&out_dir)?;
            let final_path = out_dir.join("mix_dub_preview_v1.wav");

            // Crash-safe / resumable behavior: if the expected final output already exists,
            // treat this step as complete.
            if final_path.exists() {
                set_progress(paths, job_id, 1.0)?;
                log_line(
                    paths,
                    job_id,
                    "info",
                    "mix_dub_preview_resume_skip_existing",
                    serde_json::json!({ "out_path": &final_path }),
                )?;

                if p.batch_on_import {
                    let rules = config::load_batch_on_import_rules(paths).unwrap_or_default();
                    if rules.auto_dub_preview
                        && !mux_output_exists(paths, &item.id)
                        && !item_has_active_job(paths, &item.id, JobType::MuxDubPreviewV1.as_str())
                            .unwrap_or(false)
                    {
                        let batch_id = job_batch_id(paths, job_id).ok().flatten();
                        let params_json = serde_json::to_string(&MuxDubPreviewV1Params {
                            item_id: item.id.clone(),
                            output_container: None,
                            keep_original_audio: None,
                            dubbed_audio_lang: None,
                            original_audio_lang: None,
                            batch_on_import: true,
                        })?;
                        let _ = enqueue_with_type_item_and_batch_id(
                            paths,
                            JobType::MuxDubPreviewV1,
                            params_json,
                            Some(item.id.clone()),
                            batch_id,
                        )?;
                    }
                }

                return Ok(());
            }

            let ducking_strength = p.ducking_strength.unwrap_or(0.6).clamp(0.0, 1.0);
            let loudness_target_lufs = p
                .loudness_target_lufs
                .unwrap_or(-16.0)
                .clamp(-40.0, -5.0);
            let timing_fit_enabled = p.timing_fit_enabled.unwrap_or(false);
            let timing_fit_min_factor = p.timing_fit_min_factor.unwrap_or(0.85).clamp(0.5, 1.0);
            let timing_fit_max_factor = p.timing_fit_max_factor.unwrap_or(1.25).clamp(1.0, 3.0);

            #[derive(Serialize)]
            struct TimingFitEntry {
                index: u32,
                start_ms: i64,
                end_ms: i64,
                window_ms: i64,
                duration_ms: Option<i64>,
                required_factor: Option<f32>,
                applied_factor: Option<f32>,
                stretched: bool,
                note: Option<String>,
            }

            let mut inputs: Vec<(TtsPreviewManifestSegment, PathBuf)> = Vec::new();
            for seg in &manifest.segments {
                let audio_path = match seg.audio_path.as_deref() {
                    Some(v) if !v.trim().is_empty() => PathBuf::from(v),
                    _ => continue,
                };
                if !seg.audio_exists || !audio_path.exists() {
                    continue;
                }
                inputs.push((seg.clone(), audio_path));
            }

            // If there is no TTS audio, output just the background stem.
            if inputs.is_empty() {
                std::fs::copy(&background_path, &final_path)?;
                set_progress(paths, job_id, 1.0)?;
                log_line(
                    paths,
                    job_id,
                    "info",
                    "mix_dub_preview_done",
                    serde_json::json!({ "out_path": &final_path, "overlays": 0, "mode": "background_only" }),
                )?;
                return Ok(());
            }

            // Single-pass mixer limits.
            let max_single_pass_segments = 120_usize;
            let use_single_pass = inputs.len() <= max_single_pass_segments;

            let mut timing_fit_entries: Vec<TimingFitEntry> = Vec::new();
            if timing_fit_enabled {
                for (seg, audio_path) in &inputs {
                    let window_ms = (seg.end_ms - seg.start_ms).max(0);
                    let duration_ms = ffmpeg::probe(paths, audio_path)
                        .ok()
                        .and_then(|p| p.duration_ms);
                    let required_factor = match (duration_ms, window_ms) {
                        (Some(d), w) if d > 0 && w > 0 => Some((d as f32) / (w as f32)),
                        _ => None,
                    };
                    timing_fit_entries.push(TimingFitEntry {
                        index: seg.index,
                        start_ms: seg.start_ms,
                        end_ms: seg.end_ms,
                        window_ms,
                        duration_ms,
                        required_factor,
                        applied_factor: None,
                        stretched: false,
                        note: None,
                    });
                }
            }

            let mut used_legacy = false;
            if use_single_pass {
                set_progress(paths, job_id, 0.15)?;

                // Build a single filter_complex graph:
                // 1) mix all delayed TTS segments into a "speech bus"
                // 2) sidechain-compress the background using speech (ducking)
                // 3) mix background + speech
                // 4) loudness normalize and limit
                let mut filter = String::new();
                filter.push_str(
                    "[0:a]aresample=44100,aformat=sample_fmts=fltp:channel_layouts=stereo[bg0];",
                );

                for (i, (seg, audio_path)) in inputs.iter().enumerate() {
                    let input_idx = i + 1;
                    let delay_ms = seg.start_ms.max(0);
                    let window_ms = (seg.end_ms - seg.start_ms).max(0);
                    let window_s = (window_ms as f64) / 1000.0;

                    let mut applied_factor: Option<f32> = None;
                    let mut note: Option<String> = None;
                    if timing_fit_enabled && window_ms > 0 {
                        let duration_ms = ffmpeg::probe(paths, audio_path)
                            .ok()
                            .and_then(|p| p.duration_ms);
                        if let Some(dur) = duration_ms {
                            if dur > window_ms {
                                let required = (dur as f32) / (window_ms as f32);
                                let clamped = required.clamp(timing_fit_min_factor, timing_fit_max_factor);
                                applied_factor = Some(clamped);
                                if required > timing_fit_max_factor {
                                    note = Some("required factor exceeded max; clamped + trimmed".to_string());
                                }
                            }
                        }
                    }

                    if timing_fit_enabled {
                        if let Some(entry) = timing_fit_entries.iter_mut().find(|e| e.index == seg.index) {
                            entry.applied_factor = applied_factor;
                            entry.stretched = applied_factor.unwrap_or(1.0) > 1.001;
                            if entry.note.is_none() {
                                entry.note = note.clone();
                            }
                        }
                    }

                    filter.push_str(&format!(
                        "[{input_idx}:a]aresample=44100,aformat=sample_fmts=fltp:channel_layouts=stereo"
                    ));
                    if let Some(factor) = applied_factor {
                        if factor > 1.001 {
                            filter.push(',');
                            filter.push_str(&atempo_chain_for_factor(factor));
                        }
                        if timing_fit_enabled {
                            filter.push(',');
                            filter.push_str(&format!("atrim=end={window_s:.3}"));
                        }
                    } else if timing_fit_enabled {
                        filter.push(',');
                        filter.push_str(&format!("atrim=end={window_s:.3}"));
                    }
                    filter.push_str(&format!(
                        ",adelay={delay_ms}|{delay_ms}[s{i}];"
                    ));
                }

                // Speech bus
                for i in 0..inputs.len() {
                    filter.push_str(&format!("[s{i}]"));
                }
                filter.push_str(&format!(
                    "amix=inputs={}:duration=longest:dropout_transition=0:normalize=0[tts0];",
                    inputs.len()
                ));

                // Ducking + mix
                if ducking_strength > 0.001 {
                    let threshold = (0.15 - ducking_strength * 0.10).clamp(0.02, 0.25);
                    let ratio = (1.0 + ducking_strength * 9.0).clamp(1.0, 20.0);
                    filter.push_str(&format!(
                        "[bg0][tts0]sidechaincompress=threshold={threshold:.4}:ratio={ratio:.3}:attack=20:release=250[bgd];"
                    ));
                    filter.push_str("[bgd][tts0]amix=inputs=2:duration=first:dropout_transition=0:normalize=0[mix0];");
                } else {
                    filter.push_str("[bg0][tts0]amix=inputs=2:duration=first:dropout_transition=0:normalize=0[mix0];");
                }

                // Loudness normalize + limiter
                filter.push_str(&format!(
                    "[mix0]loudnorm=I={loudness_target_lufs:.1}:TP=-1.5:LRA=11:linear=true,alimiter=limit=0.98[out]"
                ));

                set_progress(paths, job_id, 0.25)?;
                log_line(
                    paths,
                    job_id,
                    "info",
                    "mix_dub_preview_single_pass_begin",
                    serde_json::json!({
                        "segments": inputs.len(),
                        "ducking_strength": ducking_strength,
                        "loudness_target_lufs": loudness_target_lufs,
                        "timing_fit_enabled": timing_fit_enabled
                    }),
                )?;

                let mut ff = cmd::command(paths.ffmpeg_cmd());
                ff.args(["-nostdin", "-y"]);
                ff.arg("-i").arg(&background_path);
                for (_, audio_path) in &inputs {
                    ff.arg("-i").arg(audio_path);
                }
                ff.arg("-filter_complex").arg(&filter);
                ff.args(["-map", "[out]"]);
                ff.args(["-c:a", "pcm_s16le", "-ar", "44100", "-ac", "2"]);
                ff.arg(&final_path);

                let output = ff.output().map_err(|e| match e.kind() {
                    std::io::ErrorKind::NotFound => EngineError::ExternalToolMissing {
                        tool: "ffmpeg".to_string(),
                    },
                    _ => EngineError::Io(e),
                });

                match output {
                    Ok(o) if o.status.success() => {
                        set_progress(paths, job_id, 0.90)?;
                    }
                    Ok(o) => {
                        used_legacy = true;
                        log_line(
                            paths,
                            job_id,
                            "warn",
                            "mix_dub_preview_single_pass_failed_fallback",
                            serde_json::json!({
                                "stderr": String::from_utf8_lossy(&o.stderr).trim().to_string()
                            }),
                        )?;
                    }
                    Err(e) => {
                        used_legacy = true;
                        log_line(
                            paths,
                            job_id,
                            "warn",
                            "mix_dub_preview_single_pass_error_fallback",
                            serde_json::json!({ "error": e.to_string() }),
                        )?;
                    }
                }
            } else {
                used_legacy = true;
            }

            if used_legacy {
                // Fallback: legacy iterative overlay mixing.
                used_legacy = true;
                let mut current_mix = background_path.clone();
                let mut mixed_count = 0_usize;
                let total = inputs.len().max(1) as f32;

                for (i, (seg, audio_path)) in inputs.iter().enumerate() {
                    if is_canceled(paths, job_id)? {
                        log_line(paths, job_id, "info", "job_canceled", serde_json::json!({}))?;
                        return Ok(());
                    }

                    let progress = 0.10 + 0.70 * ((i as f32) / total);
                    set_progress(paths, job_id, progress)?;

                    mixed_count += 1;
                    let delay_ms = seg.start_ms.max(0);
                    let step_out = artifacts_dir.join(format!("mix_step_{mixed_count:04}.wav"));

                    let filter = format!(
                        concat!(
                            "[0:a]aresample=44100,aformat=sample_fmts=fltp:channel_layouts=stereo[bg];",
                            "[1:a]aresample=44100,aformat=sample_fmts=fltp:channel_layouts=stereo,",
                            "adelay={}|{}[tts];",
                            "[bg][tts]amix=inputs=2:duration=first:dropout_transition=0:normalize=0[m]"
                        ),
                        delay_ms,
                        delay_ms
                    );

                    let output = cmd::command(paths.ffmpeg_cmd())
                        .args(["-nostdin", "-y"])
                        .arg("-i")
                        .arg(&current_mix)
                        .arg("-i")
                        .arg(audio_path)
                        .arg("-filter_complex")
                        .arg(&filter)
                        .args(["-map", "[m]"])
                        .args(["-c:a", "pcm_s16le", "-ar", "44100", "-ac", "2"])
                        .arg(&step_out)
                        .output()
                        .map_err(|e| match e.kind() {
                            std::io::ErrorKind::NotFound => EngineError::ExternalToolMissing {
                                tool: "ffmpeg".to_string(),
                            },
                            _ => EngineError::Io(e),
                        })?;

                    if !output.status.success() {
                        return Err(EngineError::ExternalToolFailed {
                            tool: "ffmpeg".to_string(),
                            code: output.status.code(),
                            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
                        });
                    }

                    current_mix = step_out;
                }

                if current_mix != final_path {
                    if final_path.exists() {
                        let _ = std::fs::remove_file(&final_path);
                    }
                    if std::fs::rename(&current_mix, &final_path).is_err() {
                        std::fs::copy(&current_mix, &final_path)?;
                    }
                }

                // Best-effort loudness normalization on the legacy output.
                let loud_path = artifacts_dir.join("mix_dub_preview_loudnorm_tmp.wav");
                let ln_filter = format!(
                    "loudnorm=I={loudness_target_lufs:.1}:TP=-1.5:LRA=11:linear=true,alimiter=limit=0.98"
                );
                let ln_out = cmd::command(paths.ffmpeg_cmd())
                    .args(["-nostdin", "-y"])
                    .arg("-i")
                    .arg(&final_path)
                    .args(["-af", &ln_filter])
                    .args(["-c:a", "pcm_s16le", "-ar", "44100", "-ac", "2"])
                    .arg(&loud_path)
                    .output()
                    .map_err(|e| match e.kind() {
                        std::io::ErrorKind::NotFound => EngineError::ExternalToolMissing {
                            tool: "ffmpeg".to_string(),
                        },
                        _ => EngineError::Io(e),
                    })?;
                if ln_out.status.success() && loud_path.exists() {
                    let _ = std::fs::rename(&loud_path, &final_path);
                }
            }

            if timing_fit_enabled {
                let report_path = artifacts_dir.join("timing_fit_report.json");
                let report_json = serde_json::to_string_pretty(&timing_fit_entries)?;
                std::fs::write(&report_path, format!("{report_json}\n"))?;
            }

            set_progress(paths, job_id, 0.95)?;
            log_line(
                paths,
                job_id,
                "info",
                "mix_dub_preview_done",
                serde_json::json!({
                    "out_path": &final_path,
                    "overlays": inputs.len(),
                    "mode": if used_legacy { "legacy_fallback" } else { "single_pass" },
                    "ducking_strength": ducking_strength,
                    "loudness_target_lufs": loudness_target_lufs,
                    "timing_fit_enabled": timing_fit_enabled
                }),
            )?;

            if p.batch_on_import {
                let rules = config::load_batch_on_import_rules(paths).unwrap_or_default();
                if rules.auto_dub_preview
                    && !mux_output_exists(paths, &item.id)
                    && !item_has_active_job(paths, &item.id, JobType::MuxDubPreviewV1.as_str())
                        .unwrap_or(false)
                {
                    let batch_id = job_batch_id(paths, job_id).ok().flatten();
                    let params_json = serde_json::to_string(&MuxDubPreviewV1Params {
                        item_id: item.id.clone(),
                        output_container: None,
                        keep_original_audio: None,
                        dubbed_audio_lang: None,
                        original_audio_lang: None,
                        batch_on_import: true,
                    })?;
                    let _ = enqueue_with_type_item_and_batch_id(
                        paths,
                        JobType::MuxDubPreviewV1,
                        params_json,
                        Some(item.id.clone()),
                        batch_id,
                    )?;
                }
            }
        }
        JobType::MuxDubPreviewV1 => {
            set_progress(paths, job_id, 0.05)?;
            let p: MuxDubPreviewV1Params = serde_json::from_str(params_json)?;

            if is_canceled(paths, job_id)? {
                log_line(paths, job_id, "info", "job_canceled", serde_json::json!({}))?;
                return Ok(());
            }

            log_line(
                paths,
                job_id,
                "info",
                "mux_dub_preview_begin",
                serde_json::json!({ "item_id": &p.item_id }),
            )?;

            let item = library::get_item_by_id(paths, &p.item_id)?;
            let media_path = PathBuf::from(&item.media_path);
            if !media_path.exists() {
                return Err(EngineError::InstallFailed(
                    "original media path does not exist".to_string(),
                ));
            }

            let item_dir = paths.derived_item_dir(&item.id);
            let dub_audio_path = item_dir
                .join("dub_preview")
                .join("mix_dub_preview_v1.wav");
            if !dub_audio_path.exists() {
                return Err(EngineError::InstallFailed(
                    "dub preview audio not found; run Mix dub first".to_string(),
                ));
            }

            let out_dir = item_dir.join("dub_preview");
            std::fs::create_dir_all(&out_dir)?;
            let container = p
                .output_container
                .as_deref()
                .map(|v| v.trim().to_lowercase())
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| "mp4".to_string());
            let ext = if container == "mkv" { "mkv" } else { "mp4" };
            let out_path = out_dir.join(format!("mux_dub_preview_v1.{ext}"));

            if out_path.exists() {
                set_progress(paths, job_id, 1.0)?;
                log_line(
                    paths,
                    job_id,
                    "info",
                    "mux_dub_preview_resume_skip_existing",
                    serde_json::json!({ "out_path": &out_path }),
                )?;
                return Ok(());
            }

            let keep_original_audio = p.keep_original_audio.unwrap_or(false);
            let dubbed_lang = normalize_lang_tag(p.dubbed_audio_lang.as_deref()).unwrap_or("eng");
            let original_lang =
                normalize_lang_tag(p.original_audio_lang.as_deref()).unwrap_or("und");

            let mut ff = cmd::command(paths.ffmpeg_cmd());
            ff.args(["-nostdin", "-y"]);
            ff.arg("-i").arg(&media_path);
            ff.arg("-i").arg(&dub_audio_path);
            ff.args(["-map", "0:v:0?"]);
            // Put dubbed audio first so it's the default track in most players.
            ff.args(["-map", "1:a:0"]);
            if keep_original_audio {
                ff.args(["-map", "0:a:0?"]);
            }
            ff.args(["-c:v", "copy"]);
            ff.args(["-c:a", "aac", "-b:a", "192k"]);
            ff.args(["-shortest"]);
            if ext == "mp4" {
                ff.args(["-movflags", "+faststart"]);
            }

            // Best-effort language metadata.
            ff.args(["-metadata:s:a:0", &format!("language={dubbed_lang}")]);
            if keep_original_audio {
                ff.args(["-metadata:s:a:1", &format!("language={original_lang}")]);
                ff.args(["-disposition:a:0", "default"]);
                ff.args(["-disposition:a:1", "0"]);
            }

            ff.arg(&out_path);

            let output = ff.output().map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => EngineError::ExternalToolMissing {
                    tool: "ffmpeg".to_string(),
                },
                _ => EngineError::Io(e),
            })?;

            if !output.status.success() {
                return Err(EngineError::ExternalToolFailed {
                    tool: "ffmpeg".to_string(),
                    code: output.status.code(),
                    stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
                });
            }

            set_progress(paths, job_id, 0.95)?;
            log_line(
                paths,
                job_id,
                "info",
                "mux_dub_preview_done",
                serde_json::json!({ "out_path": &out_path, "container": ext, "keep_original_audio": keep_original_audio, "dubbed_lang": dubbed_lang, "original_lang": original_lang }),
            )?;
        }
        JobType::SeparateAudioSpleeter => {
            set_progress(paths, job_id, 0.05)?;
            let p: SeparateAudioSpleeterParams = serde_json::from_str(params_json)?;

            if is_canceled(paths, job_id)? {
                log_line(paths, job_id, "info", "job_canceled", serde_json::json!({}))?;
                return Ok(());
            }

            log_line(
                paths,
                job_id,
                "info",
                "separate_begin",
                serde_json::json!({ "item_id": &p.item_id, "backend": "spleeter:2stems" }),
            )?;

            let pack = tools::spleeter_pack_status(paths);
            if !pack.installed {
                return Err(EngineError::InstallFailed(
                    "Spleeter is not installed. Open Diagnostics -> Tools -> Install Spleeter."
                        .to_string(),
                ));
            }

            let item = library::get_item_by_id(paths, &p.item_id)?;
            let media_path = Path::new(&item.media_path);

            let sep_dir = paths
                .derived_item_dir(&item.id)
                .join("separation")
                .join("spleeter_2stems");
            std::fs::create_dir_all(&sep_dir)?;

            let vocals_dst = sep_dir.join("vocals.wav");
            let background_dst = sep_dir.join("background.wav");
            if vocals_dst.exists()
                && background_dst.exists()
                && std::fs::metadata(&vocals_dst).map(|m| m.len()).unwrap_or(0) > 0
                && std::fs::metadata(&background_dst).map(|m| m.len()).unwrap_or(0) > 0
            {
                set_progress(paths, job_id, 1.0)?;
                log_line(
                    paths,
                    job_id,
                    "info",
                    "separate_resume_skip_existing",
                    serde_json::json!({ "vocals_path": &vocals_dst, "background_path": &background_dst }),
                )?;

                if p.batch_on_import {
                    let rules = config::load_batch_on_import_rules(paths).unwrap_or_default();
                    if rules.auto_dub_preview
                        && tts_manifest_exists(paths, &item.id)
                        && !mix_output_exists(paths, &item.id)
                        && !item_has_active_job(paths, &item.id, JobType::MixDubPreviewV1.as_str())
                            .unwrap_or(false)
                    {
                        let batch_id = job_batch_id(paths, job_id).ok().flatten();
                        let params_json = serde_json::to_string(&MixDubPreviewV1Params {
                            item_id: item.id.clone(),
                            ducking_strength: None,
                            loudness_target_lufs: None,
                            timing_fit_enabled: None,
                            timing_fit_min_factor: None,
                            timing_fit_max_factor: None,
                            batch_on_import: true,
                        })?;
                        let _ = enqueue_with_type_item_and_batch_id(
                            paths,
                            JobType::MixDubPreviewV1,
                            params_json,
                            Some(item.id.clone()),
                            batch_id,
                        )?;
                    }
                }

                return Ok(());
            }

            let audio_path = sep_dir.join("mix_44k.wav");
            log_line(
                paths,
                job_id,
                "info",
                "separate_extract_audio_begin",
                serde_json::json!({ "path": &item.media_path, "audio_path": &audio_path }),
            )?;
            if audio_path.exists() && std::fs::metadata(&audio_path).map(|m| m.len()).unwrap_or(0) > 0 {
                log_line(
                    paths,
                    job_id,
                    "info",
                    "separate_extract_audio_resume_skip_existing",
                    serde_json::json!({ "audio_path": &audio_path }),
                )?;
            } else {
                ffmpeg::extract_audio_wav_44k_stereo(paths, media_path, &audio_path)?;
            }
            set_progress(paths, job_id, 0.25)?;

            if is_canceled(paths, job_id)? {
                log_line(paths, job_id, "info", "job_canceled", serde_json::json!({}))?;
                return Ok(());
            }

            let venv_python = tools::python_venv_python_path(paths).map_err(|_| {
                EngineError::InstallFailed(
                    "Python toolchain is not set up. Open Diagnostics -> Tools -> Setup Python toolchain."
                        .to_string(),
                )
            })?;

            let raw_dir = sep_dir.join("raw");
            std::fs::create_dir_all(&raw_dir)?;

            log_line(
                paths,
                job_id,
                "info",
                "separate_spleeter_begin",
                serde_json::json!({ "audio_path": &audio_path, "raw_dir": &raw_dir }),
            )?;

            let ffmpeg_dir = paths.ffmpeg_dir();
            let old_path = std::env::var_os("PATH").unwrap_or_default();
            let ffmpeg_path = format!(
                "{};{}",
                ffmpeg_dir.to_string_lossy(),
                old_path.to_string_lossy()
            );

            // Use Spleeter's Python API instead of the CLI entrypoint.
            //
            // The CLI layer depends on Typer internals that can break across Typer versions,
            // while the separation backend itself (Separator) remains stable.
            //
            // We run a dedicated script file (not `-c`/stdin) so multiprocessing can correctly
            // re-spawn the main module on Windows.
            let sep_script_path = sep_dir.join("spleeter_separate.py");
            let sep_script = r#"
import argparse

from spleeter.separator import Separator


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--input", required=True)
    ap.add_argument("--output", required=True)
    args = ap.parse_args()

    separator = Separator("spleeter:2stems")
    separator.separate_to_file(args.input, args.output)
    print("spleeter_separate_ok")


if __name__ == "__main__":
    main()
"#;
            std::fs::write(&sep_script_path, sep_script)?;

            let output = {
                let mut cmd = cmd::command(&venv_python);
                cmd.arg(&sep_script_path);
                cmd.arg("--input").arg(&audio_path);
                cmd.arg("--output").arg(&raw_dir);
                cmd.env("PATH", ffmpeg_path);
                cmd.env("PYTHONNOUSERSITE", "1");
                cmd.env(
                    "XDG_CACHE_HOME",
                    paths.cache_dir().join("python").to_string_lossy().to_string(),
                );
                cmd.env(
                    "MODEL_PATH",
                    paths
                        .python_models_dir()
                        .join("spleeter")
                        .to_string_lossy()
                        .to_string(),
                );
                cmd.output()
            }
            .map_err(|e| EngineError::InstallFailed(format!("failed to run spleeter: {e}")))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(EngineError::InstallFailed(format!(
                    "spleeter failed (code={:?}): {}",
                    output.status.code(),
                    stderr.trim()
                )));
            }
            let split_stdout = String::from_utf8_lossy(&output.stdout);
            let split_stderr = String::from_utf8_lossy(&output.stderr);
            if !split_stderr.trim().is_empty() {
                log_line(
                    paths,
                    job_id,
                    "warn",
                    "separate_spleeter_warning",
                    serde_json::json!({ "stderr": split_stderr.trim() }),
                )?;
            }
            if !split_stdout.trim().is_empty() {
                log_line(
                    paths,
                    job_id,
                    "info",
                    "separate_spleeter_stdout",
                    serde_json::json!({ "stdout": split_stdout.trim() }),
                )?;
            }

            let stem_name = audio_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("audio");
            let stems_dir = raw_dir.join(stem_name);
            let stems_file = |dir: &Path| -> (PathBuf, PathBuf) {
                (dir.join("vocals.wav"), dir.join("accompaniment.wav"))
            };

            let mut candidate_dirs: Vec<PathBuf> = vec![
                stems_dir.clone(),
                raw_dir.join(audio_path.file_name().and_then(|s| s.to_str()).unwrap_or("audio.wav")),
            ];
            if let Some(file_name) = audio_path.file_name().and_then(|n| n.to_str()) {
                let dir = raw_dir.join(file_name);
                if !candidate_dirs.contains(&dir) {
                    candidate_dirs.push(dir);
                }
            }
            if let Some(stem) = audio_path.file_stem().and_then(|n| n.to_str()) {
                let alt = format!("{stem}.wav");
                candidate_dirs.push(raw_dir.join(alt));
            }
            if !candidate_dirs.iter().any(|d| d == &raw_dir) {
                candidate_dirs.push(raw_dir.clone());
            }
            candidate_dirs.dedup();

            let mut vocals_src: Option<PathBuf> = None;
            let mut background_src: Option<PathBuf> = None;
            let mut found_pair_dir: Option<PathBuf> = None;

            for candidate_dir in &candidate_dirs {
                let (vocals, accompaniment) = stems_file(candidate_dir);
                if vocals.exists() && accompaniment.exists() {
                    vocals_src = Some(vocals);
                    background_src = Some(accompaniment);
                    found_pair_dir = Some(candidate_dir.clone());
                    break;
                }
            }

            if vocals_src.is_none() || background_src.is_none() {
                let mut scan_queue: VecDeque<(PathBuf, usize)> = VecDeque::new();
                scan_queue.push_back((raw_dir.clone(), 0));
                let max_scan_depth = 4usize;
                let mut pairs: HashMap<PathBuf, (Option<PathBuf>, Option<PathBuf>)> = HashMap::new();

                while let Some((dir, depth)) = scan_queue.pop_front() {
                    if !dir.exists() {
                        continue;
                    }
                    let rd = match std::fs::read_dir(&dir) {
                        Ok(r) => r,
                        Err(_) => continue,
                    };

                    for entry in rd {
                        let entry = entry?;
                        let path = entry.path();
                        let meta = entry.metadata()?;
                        if meta.is_dir() {
                            if depth < max_scan_depth {
                                scan_queue.push_back((path, depth + 1));
                            }
                            continue;
                        }

                        let filename = path
                            .file_name()
                            .and_then(|value| value.to_str())
                            .unwrap_or_default();
                        if filename != "vocals.wav" && filename != "accompaniment.wav" {
                            continue;
                        }

                        let parent = match path.parent() {
                            Some(parent) => parent.to_path_buf(),
                            None => continue,
                        };

                        let pair = pairs.entry(parent).or_insert((None, None));
                        match filename {
                            "vocals.wav" => pair.0 = Some(path),
                            "accompaniment.wav" => pair.1 = Some(path),
                            _ => {}
                        }

                        if pair.0.is_some() && pair.1.is_some() {
                            vocals_src = pair.0.clone();
                            background_src = pair.1.clone();
                            found_pair_dir = Some(
                                pair.0
                                    .as_ref()
                                    .and_then(|p| p.parent().map(|p| p.to_path_buf()))
                                    .unwrap_or_else(|| raw_dir.clone()),
                            );
                            break;
                        }
                    }

                    if vocals_src.is_some() && background_src.is_some() {
                        break;
                    }
                }
            }

            let vocals_src = vocals_src.ok_or_else(|| {
                EngineError::InstallFailed(format!(
                    "spleeter stem extraction output not found; expected vocals.wav and accompaniment.wav. stdout={}, stderr={}",
                    split_stdout.trim(),
                    split_stderr.trim()
                ))
            })?;
            let background_src = background_src.ok_or_else(|| {
                EngineError::InstallFailed(format!(
                    "spleeter stem extraction output not found; expected vocals.wav and accompaniment.wav. stdout={}, stderr={}",
                    split_stdout.trim(),
                    split_stderr.trim()
                ))
            })?;

            let found_pair_dir = found_pair_dir.unwrap_or_else(|| stems_dir.clone());
            log_line(
                paths,
                job_id,
                "info",
                "separate_spleeter_outputs_discovered",
                serde_json::json!({
                    "raw_dir": &raw_dir,
                    "expected_dir": &stems_dir,
                    "discovered_dir": &found_pair_dir,
                    "vocals_src": &vocals_src,
                    "background_src": &background_src,
                }),
            )?;

            if vocals_dst.exists() {
                let _ = std::fs::remove_file(&vocals_dst);
            }
            if background_dst.exists() {
                let _ = std::fs::remove_file(&background_dst);
            }

            if std::fs::rename(&vocals_src, &vocals_dst).is_err() {
                std::fs::copy(&vocals_src, &vocals_dst)?;
                let _ = std::fs::remove_file(&vocals_src);
            }
            if std::fs::rename(&background_src, &background_dst).is_err() {
                std::fs::copy(&background_src, &background_dst)?;
                let _ = std::fs::remove_file(&background_src);
            }

            let _ = std::fs::remove_dir_all(&stems_dir);
            set_progress(paths, job_id, 0.95)?;

            log_line(
                paths,
                job_id,
                "info",
                "separate_done",
                serde_json::json!({
                    "vocals_path": &vocals_dst,
                    "background_path": &background_dst,
                }),
            )?;

            if p.batch_on_import {
                let rules = config::load_batch_on_import_rules(paths).unwrap_or_default();
                if rules.auto_dub_preview
                    && tts_manifest_exists(paths, &item.id)
                    && !mix_output_exists(paths, &item.id)
                    && !item_has_active_job(paths, &item.id, JobType::MixDubPreviewV1.as_str())
                        .unwrap_or(false)
                {
                    let batch_id = job_batch_id(paths, job_id).ok().flatten();
                    let params_json = serde_json::to_string(&MixDubPreviewV1Params {
                        item_id: item.id.clone(),
                        ducking_strength: None,
                        loudness_target_lufs: None,
                        timing_fit_enabled: None,
                        timing_fit_min_factor: None,
                        timing_fit_max_factor: None,
                        batch_on_import: true,
                    })?;
                    let _ = enqueue_with_type_item_and_batch_id(
                        paths,
                        JobType::MixDubPreviewV1,
                        params_json,
                        Some(item.id.clone()),
                        batch_id,
                    )?;
                }
            }
        }
        JobType::SeparateAudioDemucsV1 => {
            set_progress(paths, job_id, 0.05)?;
            let p: SeparateAudioDemucsV1Params = serde_json::from_str(params_json)?;

            if is_canceled(paths, job_id)? {
                log_line(paths, job_id, "info", "job_canceled", serde_json::json!({}))?;
                return Ok(());
            }

            log_line(
                paths,
                job_id,
                "info",
                "separate_begin",
                serde_json::json!({ "item_id": &p.item_id, "backend": "demucs:two_stems_vocals_v1" }),
            )?;

            let pack = tools::demucs_pack_status(paths);
            if !pack.installed {
                return Err(EngineError::InstallFailed(
                    "Demucs separation pack is not installed. Open Diagnostics -> Tools -> Install Demucs separation pack."
                        .to_string(),
                ));
            }

            let item = library::get_item_by_id(paths, &p.item_id)?;
            let media_path = Path::new(&item.media_path);

            let sep_dir = paths
                .derived_item_dir(&item.id)
                .join("separation")
                .join("demucs_two_stems_v1");
            std::fs::create_dir_all(&sep_dir)?;

            let vocals_dst = sep_dir.join("vocals.wav");
            let background_dst = sep_dir.join("background.wav");
            if vocals_dst.exists()
                && background_dst.exists()
                && std::fs::metadata(&vocals_dst).map(|m| m.len()).unwrap_or(0) > 0
                && std::fs::metadata(&background_dst).map(|m| m.len()).unwrap_or(0) > 0
            {
                set_progress(paths, job_id, 1.0)?;
                log_line(
                    paths,
                    job_id,
                    "info",
                    "separate_resume_skip_existing",
                    serde_json::json!({ "vocals_path": &vocals_dst, "background_path": &background_dst }),
                )?;

                if p.batch_on_import {
                    let rules = config::load_batch_on_import_rules(paths).unwrap_or_default();
                    if rules.auto_dub_preview
                        && tts_manifest_exists(paths, &item.id)
                        && !mix_output_exists(paths, &item.id)
                        && !item_has_active_job(paths, &item.id, JobType::MixDubPreviewV1.as_str())
                            .unwrap_or(false)
                    {
                        let batch_id = job_batch_id(paths, job_id).ok().flatten();
                        let params_json = serde_json::to_string(&MixDubPreviewV1Params {
                            item_id: item.id.clone(),
                            ducking_strength: None,
                            loudness_target_lufs: None,
                            timing_fit_enabled: None,
                            timing_fit_min_factor: None,
                            timing_fit_max_factor: None,
                            batch_on_import: true,
                        })?;
                        let _ = enqueue_with_type_item_and_batch_id(
                            paths,
                            JobType::MixDubPreviewV1,
                            params_json,
                            Some(item.id.clone()),
                            batch_id,
                        )?;
                    }
                }

                return Ok(());
            }

            let audio_path = sep_dir.join("mix_44k.wav");
            log_line(
                paths,
                job_id,
                "info",
                "separate_extract_audio_begin",
                serde_json::json!({ "path": &item.media_path, "audio_path": &audio_path }),
            )?;
            if audio_path.exists() && std::fs::metadata(&audio_path).map(|m| m.len()).unwrap_or(0) > 0 {
                log_line(
                    paths,
                    job_id,
                    "info",
                    "separate_extract_audio_resume_skip_existing",
                    serde_json::json!({ "audio_path": &audio_path }),
                )?;
            } else {
                ffmpeg::extract_audio_wav_44k_stereo(paths, media_path, &audio_path)?;
            }
            set_progress(paths, job_id, 0.25)?;

            if is_canceled(paths, job_id)? {
                log_line(paths, job_id, "info", "job_canceled", serde_json::json!({}))?;
                return Ok(());
            }

            let venv_python = tools::python_venv_python_path(paths).map_err(|_| {
                EngineError::InstallFailed(
                    "Python toolchain is not set up. Open Diagnostics -> Tools -> Setup Python toolchain."
                        .to_string(),
                )
            })?;

            let raw_dir = sep_dir.join("raw");
            std::fs::create_dir_all(&raw_dir)?;

            log_line(
                paths,
                job_id,
                "info",
                "separate_demucs_begin",
                serde_json::json!({ "audio_path": &audio_path, "raw_dir": &raw_dir }),
            )?;

            let torch_home = paths.python_models_dir().join("demucs");
            std::fs::create_dir_all(&torch_home)?;

            let output = {
                let mut cmd = cmd::command(&venv_python);
                cmd.args(["-m", "demucs_infer"]);
                cmd.args(["--two-stems", "vocals"]);
                cmd.arg("-o").arg(&raw_dir);
                cmd.arg(&audio_path);
                cmd.env("PYTHONNOUSERSITE", "1");
                cmd.env(
                    "XDG_CACHE_HOME",
                    paths.cache_dir().join("python").to_string_lossy().to_string(),
                );
                cmd.env("TORCH_HOME", torch_home.to_string_lossy().to_string());
                cmd.output()
            }
            .map_err(|e| EngineError::InstallFailed(format!("failed to run demucs: {e}")))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(EngineError::InstallFailed(format!(
                    "demucs failed (code={:?}): {}",
                    output.status.code(),
                    stderr.trim()
                )));
            }

            let mut vocals_src: Option<PathBuf> = None;
            let mut background_src: Option<PathBuf> = None;
            let mut stack: Vec<PathBuf> = vec![raw_dir.clone()];
            while let Some(dir) = stack.pop() {
                let entries = match std::fs::read_dir(&dir) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        stack.push(path);
                        continue;
                    }
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
                    if name == "vocals.wav" {
                        vocals_src = Some(path);
                    } else if name == "no_vocals.wav" || name == "accompaniment.wav" {
                        background_src = Some(path);
                    }
                    if vocals_src.is_some() && background_src.is_some() {
                        break;
                    }
                }
                if vocals_src.is_some() && background_src.is_some() {
                    break;
                }
            }

            let vocals_src = vocals_src.ok_or_else(|| {
                EngineError::InstallFailed("demucs output not found (vocals.wav)".to_string())
            })?;
            let background_src = background_src.ok_or_else(|| {
                EngineError::InstallFailed("demucs output not found (no_vocals.wav)".to_string())
            })?;

            if vocals_dst.exists() {
                let _ = std::fs::remove_file(&vocals_dst);
            }
            if background_dst.exists() {
                let _ = std::fs::remove_file(&background_dst);
            }
            if std::fs::rename(&vocals_src, &vocals_dst).is_err() {
                std::fs::copy(&vocals_src, &vocals_dst)?;
            }
            if std::fs::rename(&background_src, &background_dst).is_err() {
                std::fs::copy(&background_src, &background_dst)?;
            }

            set_progress(paths, job_id, 0.95)?;
            log_line(
                paths,
                job_id,
                "info",
                "separate_done",
                serde_json::json!({ "vocals_path": &vocals_dst, "background_path": &background_dst }),
            )?;

            if p.batch_on_import {
                let rules = config::load_batch_on_import_rules(paths).unwrap_or_default();
                if rules.auto_dub_preview
                    && tts_manifest_exists(paths, &item.id)
                    && !mix_output_exists(paths, &item.id)
                    && !item_has_active_job(paths, &item.id, JobType::MixDubPreviewV1.as_str())
                        .unwrap_or(false)
                {
                    let batch_id = job_batch_id(paths, job_id).ok().flatten();
                    let params_json = serde_json::to_string(&MixDubPreviewV1Params {
                        item_id: item.id.clone(),
                        ducking_strength: None,
                        loudness_target_lufs: None,
                        timing_fit_enabled: None,
                        timing_fit_min_factor: None,
                        timing_fit_max_factor: None,
                        batch_on_import: true,
                    })?;
                    let _ = enqueue_with_type_item_and_batch_id(
                        paths,
                        JobType::MixDubPreviewV1,
                        params_json,
                        Some(item.id.clone()),
                        batch_id,
                    )?;
                }
            }
        }
        JobType::CleanVocalsV1 => {
            set_progress(paths, job_id, 0.05)?;
            let p: CleanVocalsV1Params = serde_json::from_str(params_json)?;

            if is_canceled(paths, job_id)? {
                log_line(paths, job_id, "info", "job_canceled", serde_json::json!({}))?;
                return Ok(());
            }

            log_line(
                paths,
                job_id,
                "info",
                "clean_vocals_begin",
                serde_json::json!({ "item_id": &p.item_id }),
            )?;

            let item = library::get_item_by_id(paths, &p.item_id)?;
            let vocals_src = separation_vocals_path_best_effort(paths, &item.id).ok_or_else(|| {
                EngineError::InstallFailed(
                    "vocals stem not found; run Separate first (Spleeter or Demucs)".to_string(),
                )
            })?;

            let out_dir = paths.derived_item_dir(&item.id).join("cleanup");
            std::fs::create_dir_all(&out_dir)?;
            let out_path = out_dir.join("vocals_clean_v1.wav");

            if out_path.exists() && std::fs::metadata(&out_path).map(|m| m.len()).unwrap_or(0) > 0 {
                set_progress(paths, job_id, 1.0)?;
                log_line(
                    paths,
                    job_id,
                    "info",
                    "clean_vocals_resume_skip_existing",
                    serde_json::json!({ "out_path": &out_path }),
                )?;
                return Ok(());
            }

            let filter = "highpass=f=80,lowpass=f=12000,afftdn=nf=-25";
            let output = cmd::command(paths.ffmpeg_cmd())
                .args(["-nostdin", "-y"])
                .arg("-i")
                .arg(&vocals_src)
                .args(["-af", filter])
                .args(["-c:a", "pcm_s16le", "-ar", "44100", "-ac", "2"])
                .arg(&out_path)
                .output()
                .map_err(|e| match e.kind() {
                    std::io::ErrorKind::NotFound => EngineError::ExternalToolMissing {
                        tool: "ffmpeg".to_string(),
                    },
                    _ => EngineError::Io(e),
                })?;

            if !output.status.success() {
                return Err(EngineError::ExternalToolFailed {
                    tool: "ffmpeg".to_string(),
                    code: output.status.code(),
                    stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
                });
            }

            set_progress(paths, job_id, 0.95)?;
            log_line(
                paths,
                job_id,
                "info",
                "clean_vocals_done",
                serde_json::json!({ "out_path": &out_path, "filter": filter }),
            )?;
        }
        JobType::QcReportV1 => {
            set_progress(paths, job_id, 0.05)?;
            let p: QcReportV1Params = serde_json::from_str(params_json)?;

            if is_canceled(paths, job_id)? {
                log_line(paths, job_id, "info", "job_canceled", serde_json::json!({}))?;
                return Ok(());
            }

            log_line(
                paths,
                job_id,
                "info",
                "qc_report_begin",
                serde_json::json!({ "item_id": &p.item_id, "track_id": &p.track_id }),
            )?;

            let track = subtitle_tracks::get_track(paths, &p.track_id)?;
            if track.item_id != p.item_id {
                return Err(EngineError::InstallFailed(format!(
                    "qc report item_id mismatch: params.item_id={} track.item_id={}",
                    p.item_id, track.item_id
                )));
            }

            let doc = subtitle_tracks::load_document(paths, &p.track_id)?;
            let item = library::get_item_by_id(paths, &p.item_id)?;

            let out_dir = paths.derived_item_dir(&item.id).join("qc");
            std::fs::create_dir_all(&out_dir)?;
            let out_path = out_dir.join(format!("qc_report_v1_{}.json", p.track_id));

            if out_path.exists() && std::fs::metadata(&out_path).map(|m| m.len()).unwrap_or(0) > 0 {
                set_progress(paths, job_id, 1.0)?;
                log_line(
                    paths,
                    job_id,
                    "info",
                    "qc_report_resume_skip_existing",
                    serde_json::json!({ "out_path": &out_path }),
                )?;
                return Ok(());
            }

            #[derive(Debug, Clone, Serialize)]
            struct QcThresholds {
                cps_warn: f32,
                cps_fail: f32,
                line_chars_warn: usize,
                line_chars_fail: usize,
                overlap_warn_ms: i64,
            }

            #[derive(Debug, Clone, Serialize)]
            struct QcIssue {
                kind: String,
                severity: String,
                segment_index: u32,
                start_ms: i64,
                end_ms: i64,
                message: String,
                value: Option<f64>,
            }

            #[derive(Debug, Clone, Serialize)]
            struct QcSummary {
                total_segments: usize,
                issues_total: usize,
                issues_by_kind: std::collections::BTreeMap<String, usize>,
            }

            #[derive(Debug, Clone, Serialize)]
            struct QcReportV1 {
                schema_version: u32,
                generated_at_ms: i64,
                item_id: String,
                track_id: String,
                lang: String,
                thresholds: QcThresholds,
                tts_backend: Option<String>,
                tts_manifest_path: Option<String>,
                issues: Vec<QcIssue>,
                summary: QcSummary,
            }

            fn wav_duration_ms_best_effort(path: &Path) -> Option<i64> {
                let reader = hound::WavReader::open(path).ok()?;
                let spec = reader.spec();
                if spec.sample_rate == 0 {
                    return None;
                }
                let frames = reader.duration() as f64;
                let seconds = frames / (spec.sample_rate as f64);
                Some((seconds * 1000.0).round() as i64)
            }

            #[derive(Debug, Clone, Deserialize)]
            struct TtsManifestMeta {
                #[serde(default)]
                backend: Option<String>,
                #[serde(default)]
                track_id: Option<String>,
                #[serde(default)]
                segments: Vec<TtsPreviewManifestSegment>,
            }

            let item_dir = paths.derived_item_dir(&item.id);
            let mut tts_backend: Option<String> = None;
            let mut tts_manifest_path: Option<String> = None;
            let mut tts_duration_by_index: HashMap<u32, i64> = HashMap::new();

            let manifest_candidates = [
                item_dir
                    .join("tts_preview")
                    .join("dub_voice_preserving_v1")
                    .join("manifest.json"),
                item_dir
                    .join("tts_preview")
                    .join("tts_neural_local_v1")
                    .join("manifest.json"),
                item_dir
                    .join("tts_preview")
                    .join("pyttsx3_v1")
                    .join("manifest.json"),
            ];

            for manifest_path in manifest_candidates {
                if !manifest_path.exists() {
                    continue;
                }
                let bytes = match std::fs::read(&manifest_path) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let meta: TtsManifestMeta = match serde_json::from_slice(&bytes) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                if meta
                    .track_id
                    .as_deref()
                    .map(|v| v.trim())
                    .filter(|v| !v.is_empty())
                    != Some(p.track_id.as_str())
                {
                    continue;
                }

                tts_backend = meta.backend.clone();
                tts_manifest_path = Some(manifest_path.to_string_lossy().to_string());

                for seg in meta.segments {
                    if !seg.audio_exists {
                        continue;
                    }
                    let audio_path = seg
                        .audio_path
                        .as_deref()
                        .map(|v| v.trim())
                        .filter(|v| !v.is_empty())
                        .map(PathBuf::from);
                    let Some(audio_path) = audio_path else {
                        continue;
                    };
                    if !audio_path.exists() {
                        continue;
                    }

                    if let Some(ms) = wav_duration_ms_best_effort(&audio_path) {
                        tts_duration_by_index.insert(seg.index, ms);
                    } else if let Ok(probe) = ffmpeg::probe(paths, &audio_path) {
                        if let Some(ms) = probe.duration_ms {
                            tts_duration_by_index.insert(seg.index, ms);
                        }
                    }
                }

                break;
            }

            let thresholds = QcThresholds {
                cps_warn: 17.0,
                cps_fail: 23.0,
                line_chars_warn: 42,
                line_chars_fail: 55,
                overlap_warn_ms: 40,
            };

            let mut issues: Vec<QcIssue> = Vec::new();
            let mut prev_end_ms: Option<i64> = None;

            for seg in &doc.segments {
                let window_ms = (seg.end_ms - seg.start_ms).max(0);
                let seconds = (window_ms as f64) / 1000.0;
                let text = seg.text.trim();
                let char_count = text.chars().filter(|c| !c.is_whitespace()).count();

                if text.is_empty() {
                    issues.push(QcIssue {
                        kind: "empty_text".to_string(),
                        severity: "warn".to_string(),
                        segment_index: seg.index,
                        start_ms: seg.start_ms,
                        end_ms: seg.end_ms,
                        message: "Segment text is empty.".to_string(),
                        value: None,
                    });
                }

                for line in seg.text.replace('\r', "").split('\n') {
                    let len = line.chars().count();
                    if len >= thresholds.line_chars_fail {
                        issues.push(QcIssue {
                            kind: "line_length".to_string(),
                            severity: "fail".to_string(),
                            segment_index: seg.index,
                            start_ms: seg.start_ms,
                            end_ms: seg.end_ms,
                            message: format!("Line exceeds {} chars (got {}).", thresholds.line_chars_fail, len),
                            value: Some(len as f64),
                        });
                    } else if len >= thresholds.line_chars_warn {
                        issues.push(QcIssue {
                            kind: "line_length".to_string(),
                            severity: "warn".to_string(),
                            segment_index: seg.index,
                            start_ms: seg.start_ms,
                            end_ms: seg.end_ms,
                            message: format!("Line exceeds {} chars (got {}).", thresholds.line_chars_warn, len),
                            value: Some(len as f64),
                        });
                    }
                }

                if seconds > 0.05 && char_count > 0 {
                    let cps = (char_count as f64) / seconds;
                    if cps >= thresholds.cps_fail as f64 {
                        issues.push(QcIssue {
                            kind: "cps".to_string(),
                            severity: "fail".to_string(),
                            segment_index: seg.index,
                            start_ms: seg.start_ms,
                            end_ms: seg.end_ms,
                            message: format!("High reading speed: {:.1} CPS.", cps),
                            value: Some(cps),
                        });
                    } else if cps >= thresholds.cps_warn as f64 {
                        issues.push(QcIssue {
                            kind: "cps".to_string(),
                            severity: "warn".to_string(),
                            segment_index: seg.index,
                            start_ms: seg.start_ms,
                            end_ms: seg.end_ms,
                            message: format!("Reading speed: {:.1} CPS.", cps),
                            value: Some(cps),
                        });
                    }
                }

                if let Some(prev_end) = prev_end_ms {
                    if seg.start_ms < prev_end - thresholds.overlap_warn_ms {
                        issues.push(QcIssue {
                            kind: "overlap".to_string(),
                            severity: "warn".to_string(),
                            segment_index: seg.index,
                            start_ms: seg.start_ms,
                            end_ms: seg.end_ms,
                            message: format!(
                                "Segment overlaps previous by {} ms.",
                                (prev_end - seg.start_ms).max(0)
                            ),
                            value: Some(((prev_end - seg.start_ms).max(0)) as f64),
                        });
                    }
                }
                prev_end_ms = Some(seg.end_ms);

                if let Some(tts_ms) = tts_duration_by_index.get(&seg.index).copied() {
                    if window_ms > 0 && tts_ms > window_ms + 120 {
                        issues.push(QcIssue {
                            kind: "tts_timing".to_string(),
                            severity: "fail".to_string(),
                            segment_index: seg.index,
                            start_ms: seg.start_ms,
                            end_ms: seg.end_ms,
                            message: format!(
                                "Dub audio longer than window (tts={}ms window={}ms).",
                                tts_ms, window_ms
                            ),
                            value: Some(((tts_ms - window_ms) as f64).max(0.0)),
                        });
                    } else if window_ms > 0 && tts_ms < (window_ms / 2).saturating_sub(200) {
                        issues.push(QcIssue {
                            kind: "tts_timing".to_string(),
                            severity: "warn".to_string(),
                            segment_index: seg.index,
                            start_ms: seg.start_ms,
                            end_ms: seg.end_ms,
                            message: format!(
                                "Dub audio much shorter than window (tts={}ms window={}ms).",
                                tts_ms, window_ms
                            ),
                            value: Some(((window_ms - tts_ms) as f64).max(0.0)),
                        });
                    }
                }
            }

            let mut by_kind: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
            for issue in &issues {
                *by_kind.entry(issue.kind.clone()).or_insert(0) += 1;
            }

            let report = QcReportV1 {
                schema_version: 1,
                generated_at_ms: now_ms(),
                item_id: item.id.clone(),
                track_id: track.id.clone(),
                lang: doc.lang.clone(),
                thresholds,
                tts_backend,
                tts_manifest_path,
                issues: issues.clone(),
                summary: QcSummary {
                    total_segments: doc.segments.len(),
                    issues_total: issues.len(),
                    issues_by_kind: by_kind,
                },
            };

            let json = serde_json::to_string_pretty(&report)?;
            std::fs::write(&out_path, format!("{json}\n"))?;

            set_progress(paths, job_id, 0.95)?;
            log_line(
                paths,
                job_id,
                "info",
                "qc_report_done",
                serde_json::json!({ "out_path": &out_path, "issues": report.summary.issues_total }),
            )?;
        }
        JobType::ExportPackV1 => {
            set_progress(paths, job_id, 0.05)?;
            let p: ExportPackV1Params = serde_json::from_str(params_json)?;

            if is_canceled(paths, job_id)? {
                log_line(paths, job_id, "info", "job_canceled", serde_json::json!({}))?;
                return Ok(());
            }

            log_line(
                paths,
                job_id,
                "info",
                "export_pack_begin",
                serde_json::json!({ "item_id": &p.item_id }),
            )?;

            let item = library::get_item_by_id(paths, &p.item_id)?;
            let item_dir = paths.derived_item_dir(&item.id);
            let export_dir = item_dir.join("exports");
            std::fs::create_dir_all(&export_dir)?;

            let out_path = export_dir.join("export_pack_v1.zip");
            let tmp_path = export_dir.join(format!("export_pack_v1.{job_id}.tmp.zip"));

            if tmp_path.exists() {
                let _ = std::fs::remove_file(&tmp_path);
            }

            #[derive(Debug, Clone, Serialize)]
            struct ExportEntry {
                zip_path: String,
                src_path: String,
                bytes: u64,
            }

            #[derive(Debug, Clone, Serialize)]
            struct ExportProvenance {
                schema_version: u32,
                generated_at_ms: i64,
                engine_version: String,
                item_id: String,
                item_title: String,
                source_type: String,
                source_uri: String,
                media_path: String,
                included: Vec<ExportEntry>,
                jobs: Vec<serde_json::Value>,
            }

            let mut files: Vec<(PathBuf, String)> = Vec::new();

            let dub_dir = item_dir.join("dub_preview");
            let mix_wav = dub_dir.join("mix_dub_preview_v1.wav");
            if mix_wav.exists() {
                files.push((mix_wav, "dub_preview/mix_dub_preview_v1.wav".to_string()));
            }

            let mux_mp4 = dub_dir.join("mux_dub_preview_v1.mp4");
            let mux_mkv = dub_dir.join("mux_dub_preview_v1.mkv");
            if mux_mp4.exists() {
                files.push((mux_mp4, "dub_preview/mux_dub_preview_v1.mp4".to_string()));
            } else if mux_mkv.exists() {
                files.push((mux_mkv, "dub_preview/mux_dub_preview_v1.mkv".to_string()));
            }

            if let Some(bg) = separation_background_path_best_effort(paths, &item.id) {
                files.push((bg, "separation/background.wav".to_string()));
            }
            if let Some(vocals) = separation_vocals_path_best_effort(paths, &item.id) {
                files.push((vocals, "separation/vocals.wav".to_string()));
            }

            let cleaned = item_dir.join("cleanup").join("vocals_clean_v1.wav");
            if cleaned.exists() {
                files.push((cleaned, "cleanup/vocals_clean_v1.wav".to_string()));
            }

            // Include latest subtitle tracks (best-effort).
            let tracks = subtitle_tracks::list_tracks(paths, &item.id)?;
            let mut latest: HashMap<(String, String, String), subtitle_tracks::SubtitleTrackRow> = HashMap::new();
            for t in tracks {
                let key = (t.kind.clone(), t.lang.clone(), t.format.clone());
                let replace = match latest.get(&key) {
                    Some(existing) => t.version > existing.version,
                    None => true,
                };
                if replace {
                    latest.insert(key, t);
                }
            }
            for (_k, t) in latest {
                let src = PathBuf::from(&t.path);
                if !src.exists() {
                    continue;
                }
                let base = format!(
                    "subtitles/{kind}.{lang}.v{version}.json",
                    kind = t.kind,
                    lang = t.lang,
                    version = t.version
                );
                files.push((src.clone(), base.clone()));

                let srt = src.with_extension("srt");
                if srt.exists() {
                    files.push((srt, base.replace(".json", ".srt")));
                }
                let vtt = src.with_extension("vtt");
                if vtt.exists() {
                    files.push((vtt, base.replace(".json", ".vtt")));
                }
            }

            let integrity_path = crate::tools::pack_integrity_manifest_status(paths).manifest_path;
            let integrity_path = PathBuf::from(integrity_path);
            if integrity_path.exists() {
                files.push((
                    integrity_path,
                    "integrity/pack_integrity_manifest.json".to_string(),
                ));
            }

            // Best-effort include QC reports and timing-fit artifacts.
            let qc_dir = item_dir.join("qc");
            if qc_dir.exists() {
                if let Ok(entries) = std::fs::read_dir(&qc_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if !path.is_file() {
                            continue;
                        }
                        let name = path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("")
                            .to_string();
                        if name.to_lowercase().ends_with(".json") {
                            files.push((path, format!("qc/{name}")));
                        }
                    }
                }
            }
            let timing_fit_report = paths
                .job_artifacts_dir(job_id)
                .join("timing_fit_report.json");
            if timing_fit_report.exists() {
                files.push((timing_fit_report, "dub_preview/timing_fit_report.json".to_string()));
            }

            // Collect relevant job rows for provenance (best-effort).
            let conn = db::open(paths)?;
            db::migrate(&conn)?;
            let mut jobs_json: Vec<serde_json::Value> = Vec::new();
            let mut stmt = conn.prepare(
                r#"
SELECT id, type, status, progress, error, created_at_ms, started_at_ms, finished_at_ms, params_json
FROM job
WHERE item_id=?1
ORDER BY created_at_ms ASC
"#,
            )?;
            let mut rows = stmt.query(params![&item.id])?;
            while let Some(row) = rows.next()? {
                let id: String = row.get(0)?;
                let ty: String = row.get(1)?;
                let status: String = row.get(2)?;
                let progress: f32 = row.get(3)?;
                let error: Option<String> = row.get(4)?;
                let created_at_ms: i64 = row.get(5)?;
                let started_at_ms: Option<i64> = row.get(6)?;
                let finished_at_ms: Option<i64> = row.get(7)?;
                let params_json_str: String = row.get(8)?;
                jobs_json.push(serde_json::json!({
                    "id": id,
                    "type": ty,
                    "status": status,
                    "progress": progress,
                    "error": error,
                    "created_at_ms": created_at_ms,
                    "started_at_ms": started_at_ms,
                    "finished_at_ms": finished_at_ms,
                    "params_json": params_json_str,
                }));
            }

            let file = std::fs::File::create(&tmp_path)?;
            let mut zip = zip::ZipWriter::new(file);
            let options = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);

            let mut included: Vec<ExportEntry> = Vec::new();
            for (src, zip_path) in &files {
                if !src.exists() {
                    continue;
                }
                let bytes = std::fs::metadata(src).map(|m| m.len()).unwrap_or(0);
                let zip_path = zip_path.replace('\\', "/");
                zip.start_file(&zip_path, options).map_err(|e| {
                    EngineError::InstallFailed(format!("zip start file failed ({zip_path}): {e}"))
                })?;
                let mut f = std::fs::File::open(src)?;
                std::io::copy(&mut f, &mut zip)?;
                included.push(ExportEntry {
                    zip_path,
                    src_path: src.to_string_lossy().to_string(),
                    bytes,
                });
            }

            let provenance = ExportProvenance {
                schema_version: 1,
                generated_at_ms: now_ms(),
                engine_version: crate::diagnostics::engine_version().to_string(),
                item_id: item.id.clone(),
                item_title: item.title.clone(),
                source_type: item.source_type.clone(),
                source_uri: item.source_uri.clone(),
                media_path: item.media_path.clone(),
                included: included.clone(),
                jobs: jobs_json,
            };
            let prov_json = serde_json::to_string_pretty(&provenance)?;
            zip.start_file("provenance/manifest.json", options).map_err(|e| {
                EngineError::InstallFailed(format!("zip start file failed (provenance/manifest.json): {e}"))
            })?;
            zip.write_all(prov_json.as_bytes())?;
            zip.write_all(b"\n")?;

            zip.finish().map_err(|e| EngineError::InstallFailed(format!("zip finish failed: {e}")))?;

            if out_path.exists() {
                let _ = std::fs::remove_file(&out_path);
            }
            if std::fs::rename(&tmp_path, &out_path).is_err() {
                std::fs::copy(&tmp_path, &out_path)?;
                let _ = std::fs::remove_file(&tmp_path);
            }

            let bytes = std::fs::metadata(&out_path).map(|m| m.len()).unwrap_or(0);
            set_progress(paths, job_id, 0.95)?;
            log_line(
                paths,
                job_id,
                "info",
                "export_pack_done",
                serde_json::json!({ "out_path": &out_path, "bytes": bytes }),
            )?;
        }
        JobType::InstallPhase2PacksV1 => {
            let _p: InstallPhase2PacksV1Params =
                serde_json::from_str(params_json).unwrap_or_default();

            if is_canceled(paths, job_id)? {
                log_line(paths, job_id, "info", "job_canceled", serde_json::json!({}))?;
                return Ok(());
            }

            log_line(
                paths,
                job_id,
                "info",
                "install_phase2_packs_begin",
                serde_json::json!({}),
            )?;

            let install_root = paths
                .install_logs_dir()
                .join("phase2")
                .join(job_id);
            std::fs::create_dir_all(&install_root)?;
            let state_path = install_root.join("state.json");
            let latest_path = paths
                .install_logs_dir()
                .join("phase2")
                .join("latest.json");
            if let Some(parent) = latest_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            #[derive(Debug, Clone, Serialize)]
            struct Phase2InstallStep {
                id: String,
                title: String,
                status: String,
                started_at_ms: Option<i64>,
                finished_at_ms: Option<i64>,
                estimated_bytes: Option<u64>,
                delta_bytes: Option<i64>,
                error: Option<String>,
                log_path: String,
            }

            #[derive(Debug, Clone, Serialize)]
            struct Phase2InstallState {
                schema_version: u32,
                job_id: String,
                started_at_ms: i64,
                updated_at_ms: i64,
                steps: Vec<Phase2InstallStep>,
            }

            fn write_state(path: &Path, latest: &Path, state: &Phase2InstallState) -> Result<()> {
                let json = serde_json::to_string_pretty(state)?;
                std::fs::write(path, format!("{json}\n"))?;
                // Best-effort copy to a stable "latest" location.
                let _ = std::fs::write(latest, format!("{json}\n"));
                Ok(())
            }

            fn append_log_line(path: &Path, line: &str) {
                if let Ok(mut file) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                {
                    let _ = writeln!(file, "{}", line.trim_end());
                }
            }

            let started_at_ms = now_ms();
            let plan = tools::phase2_packs_install_plan();
            let mut steps: Vec<Phase2InstallStep> = Vec::new();
            for item in plan {
                let log_path = install_root.join(format!("{}.log", item.id));
                steps.push(Phase2InstallStep {
                    id: item.id,
                    title: item.title,
                    status: if item.supported {
                        "queued".to_string()
                    } else {
                        "skipped".to_string()
                    },
                    started_at_ms: None,
                    finished_at_ms: None,
                    estimated_bytes: item.estimated_bytes,
                    delta_bytes: None,
                    error: None,
                    log_path: log_path.to_string_lossy().to_string(),
                });
            }

            let mut state = Phase2InstallState {
                schema_version: 1,
                job_id: job_id.to_string(),
                started_at_ms,
                updated_at_ms: now_ms(),
                steps,
            };
            write_state(&state_path, &latest_path, &state)?;

            let total_steps = state.steps.iter().filter(|s| s.status != "skipped").count().max(1);
            let mut completed_steps = 0_usize;

            for step_index in 0..state.steps.len() {
                if is_canceled(paths, job_id)? {
                    log_line(paths, job_id, "info", "job_canceled", serde_json::json!({}))?;
                    return Ok(());
                }
                if state.steps[step_index].status == "skipped" {
                    continue;
                }

                let (step_id, step_title, step_log_path) = {
                    let step = &mut state.steps[step_index];
                    step.status = "running".to_string();
                    step.started_at_ms = Some(now_ms());
                    step.error = None;
                    state.updated_at_ms = now_ms();
                    (step.id.clone(), step.title.clone(), step.log_path.clone())
                };

                write_state(&state_path, &latest_path, &state)?;

                let log_path = PathBuf::from(&step_log_path);
                append_log_line(
                    &log_path,
                    &format!("begin step={step_id} title={step_title}"),
                );

                let before =
                    crate::diagnostics::directory_size_bytes_best_effort(&paths.python_toolchain_dir())
                        as i64;
                let result: Result<()> = match step_id.as_str() {
                    "portable_python_win64" => {
                        let status = tools::python_toolchain_status(paths);
                        if status.base_available {
                            append_log_line(&log_path, "skip: base python already available");
                            Ok(())
                        } else {
                            append_log_line(&log_path, "install: portable python");
                            let _ = tools::install_portable_python(paths)?;
                            Ok(())
                        }
                    }
                    "python_toolchain" => {
                        append_log_line(&log_path, "install: python toolchain");
                        let _ = tools::install_python_toolchain(paths)?;
                        Ok(())
                    }
                    "spleeter" => {
                        append_log_line(&log_path, "install: spleeter pack");
                        let _ = tools::install_spleeter_pack(paths)?;
                        Ok(())
                    }
                    "diarization" => {
                        append_log_line(&log_path, "install: diarization pack");
                        let _ = tools::install_diarization_pack(paths)?;
                        Ok(())
                    }
                    "tts_preview" => {
                        append_log_line(&log_path, "install: tts preview pack");
                        let _ = tools::install_tts_preview_pack(paths)?;
                        Ok(())
                    }
                    "tts_neural_local_v1" => {
                        append_log_line(&log_path, "install: neural tts local v1 pack");
                        let _ = tools::install_tts_neural_local_v1_pack(paths)?;
                        Ok(())
                    }
                    "tts_voice_preserving_local_v1" => {
                        append_log_line(&log_path, "install: voice-preserving dub pack");
                        let _ = tools::install_tts_voice_preserving_local_v1_pack(paths)?;
                        Ok(())
                    }
                    other => Err(EngineError::InstallFailed(format!(
                        "unknown phase2 pack step id: {other}"
                    ))),
                };

                let after =
                    crate::diagnostics::directory_size_bytes_best_effort(&paths.python_toolchain_dir())
                        as i64;
                let delta_bytes = after.saturating_sub(before);
                let finished_at_ms = now_ms();

                match result {
                    Ok(()) => {
                        {
                            let step = &mut state.steps[step_index];
                            step.status = "done".to_string();
                            step.delta_bytes = Some(delta_bytes);
                            step.finished_at_ms = Some(finished_at_ms);
                        }
                        append_log_line(&log_path, "done");
                        completed_steps += 1;
                    }
                    Err(err) => {
                        {
                            let step = &mut state.steps[step_index];
                            step.status = "failed".to_string();
                            step.delta_bytes = Some(delta_bytes);
                            step.finished_at_ms = Some(finished_at_ms);
                            step.error = Some(err.to_string());
                        }
                        append_log_line(&log_path, &format!("failed: {}", err.to_string()));
                        state.updated_at_ms = now_ms();
                        write_state(&state_path, &latest_path, &state)?;
                        return Err(err);
                    }
                }

                state.updated_at_ms = now_ms();
                write_state(&state_path, &latest_path, &state)?;

                let progress = 0.10 + 0.85 * ((completed_steps as f32) / (total_steps as f32));
                set_progress(paths, job_id, progress)?;
            }

            set_progress(paths, job_id, 0.98)?;
            log_line(
                paths,
                job_id,
                "info",
                "install_phase2_packs_done",
                serde_json::json!({
                    "state_path": &state_path,
                    "latest_path": &latest_path,
                    "install_root": &install_root
                }),
            )?;
        }
        JobType::DummySleep => {
            let p: DummySleepParams = serde_json::from_str(params_json)?;
            let total = p.seconds.max(1);

            for i in 0..total {
                if is_canceled(paths, job_id)? {
                    log_line(paths, job_id, "info", "job_canceled", serde_json::json!({}))?;
                    return Ok(());
                }
                thread::sleep(Duration::from_secs(1));
                let progress = ((i + 1) as f32) / (total as f32);
                set_progress(paths, job_id, progress)?;
            }
        }
    }

    if is_canceled(paths, job_id)? {
        log_line(paths, job_id, "info", "job_canceled", serde_json::json!({}))?;
        return Ok(());
    }

    set_succeeded(paths, job_id)?;
    log_line(
        paths,
        job_id,
        "info",
        "job_succeeded",
        serde_json::json!({}),
    )?;
    Ok(())
}

fn set_progress(paths: &AppPaths, job_id: &str, progress: f32) -> Result<()> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    conn.execute(
        "UPDATE job SET progress=?1 WHERE id=?2 AND status=?3",
        params![
            progress.clamp(0.0, 1.0),
            job_id,
            JobStatus::Running.as_str()
        ],
    )?;
    Ok(())
}

fn set_succeeded(paths: &AppPaths, job_id: &str) -> Result<()> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    conn.execute(
        "UPDATE job SET status=?1, progress=1.0, finished_at_ms=?2, error=NULL WHERE id=?3 AND status=?4",
        params![
            JobStatus::Succeeded.as_str(),
            now_ms(),
            job_id,
            JobStatus::Running.as_str()
        ],
    )?;
    Ok(())
}

fn set_failed(paths: &AppPaths, job_id: &str, error: &str) -> Result<()> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    conn.execute(
        "UPDATE job SET status=?1, finished_at_ms=?2, error=?3 WHERE id=?4 AND status=?5",
        params![
            JobStatus::Failed.as_str(),
            now_ms(),
            error,
            job_id,
            JobStatus::Running.as_str()
        ],
    )?;
    Ok(())
}

fn is_canceled(paths: &AppPaths, job_id: &str) -> Result<bool> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    let status: String = conn.query_row("SELECT status FROM job WHERE id=?1", [job_id], |row| {
        row.get(0)
    })?;
    Ok(status == JobStatus::Canceled.as_str())
}

fn is_queue_paused(paths: &AppPaths) -> Result<bool> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    is_queue_paused_conn(&conn)
}

fn get_max_concurrency(paths: &AppPaths) -> Result<usize> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    get_max_concurrency_conn(&conn)
}

fn get_max_concurrency_conn(conn: &rusqlite::Connection) -> Result<usize> {
    let value: std::result::Result<String, rusqlite::Error> = conn.query_row(
        "SELECT value FROM meta WHERE key=?1",
        [META_KEY_JOBS_MAX_CONCURRENCY],
        |row| row.get(0),
    );
    match value {
        Ok(v) => match v.trim().parse::<usize>() {
            Ok(parsed) => Ok(parsed.clamp(1, MAX_MAX_CONCURRENT_JOBS)),
            Err(_) => Ok(DEFAULT_MAX_CONCURRENT_JOBS),
        },
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(DEFAULT_MAX_CONCURRENT_JOBS),
        Err(err) => Err(EngineError::Database(err)),
    }
}

fn is_queue_paused_conn(conn: &rusqlite::Connection) -> Result<bool> {
    let value: std::result::Result<String, rusqlite::Error> = conn.query_row(
        "SELECT value FROM meta WHERE key=?1",
        [META_KEY_JOBS_QUEUE_PAUSED],
        |row| row.get(0),
    );
    match value {
        Ok(v) => {
            let v = v.trim();
            Ok(v == "1" || v.eq_ignore_ascii_case("true"))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(false),
        Err(err) => Err(EngineError::Database(err)),
    }
}

fn remove_job_log_files(base_path: &Path) -> usize {
    let mut removed = 0_usize;
    if base_path.exists() && std::fs::remove_file(base_path).is_ok() {
        removed += 1;
    }
    for i in 1..=JOB_LOG_MAX_BACKUPS {
        let backup = path_with_suffix(base_path, &format!(".{i}"));
        if backup.exists() && std::fs::remove_file(backup).is_ok() {
            removed += 1;
        }
    }
    removed
}

fn clear_dir_entries(dir: &Path) -> Result<usize> {
    if !dir.exists() {
        return Ok(0);
    }

    let mut removed = 0_usize;
    for entry in std::fs::read_dir(dir)? {
        let entry = match entry {
            Ok(v) => v,
            Err(_) => continue,
        };
        let path = entry.path();
        let outcome = if path.is_dir() {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        if outcome.is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

fn log_line(
    paths: &AppPaths,
    job_id: &str,
    level: &str,
    event: &str,
    data: serde_json::Value,
) -> Result<()> {
    let line = serde_json::json!({
        "ts_ms": now_ms(),
        "job_id": job_id,
        "level": level,
        "event": event,
        "data": data
    })
    .to_string();

    let path = paths.job_logs_dir().join(format!("{job_id}.jsonl"));
    std::fs::create_dir_all(paths.job_logs_dir())?;
    rotate_job_log_if_needed(&path)?;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?
        .write_all(format!("{line}\n").as_bytes())?;
    Ok(())
}

fn rotate_job_log_if_needed(path: &Path) -> Result<()> {
    let len = match std::fs::metadata(path) {
        Ok(m) => m.len(),
        Err(_) => return Ok(()),
    };

    if len < JOB_LOG_ROTATE_BYTES {
        return Ok(());
    }

    rotate_file_backups(path, JOB_LOG_MAX_BACKUPS)?;
    Ok(())
}

fn rotate_file_backups(path: &Path, max_backups: usize) -> std::io::Result<()> {
    if max_backups == 0 {
        let _ = std::fs::remove_file(path);
        return Ok(());
    }

    for i in (1..=max_backups).rev() {
        let dst = path_with_suffix(path, &format!(".{i}"));
        let src = if i == 1 {
            path.to_path_buf()
        } else {
            path_with_suffix(path, &format!(".{}", i - 1))
        };

        if !src.exists() {
            continue;
        }

        if dst.exists() {
            let _ = std::fs::remove_file(&dst);
        }
        std::fs::rename(src, dst)?;
    }
    Ok(())
}

fn path_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let file_name = match path.file_name() {
        Some(n) => n.to_string_lossy().to_string(),
        None => suffix.to_string(),
    };
    path.with_file_name(format!("{file_name}{suffix}"))
}

fn prune_job_logs(paths: &AppPaths) -> Result<()> {
    let dir = paths.job_logs_dir();
    if !dir.exists() {
        return Ok(());
    }

    let now = SystemTime::now();
    let cutoff = now
        .checked_sub(Duration::from_secs(JOB_LOG_MAX_AGE_DAYS * 24 * 60 * 60))
        .unwrap_or(SystemTime::UNIX_EPOCH);

    let mut candidates: Vec<(PathBuf, SystemTime, u64)> = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = match entry {
            Ok(v) => v,
            Err(_) => continue,
        };
        let meta = match entry.metadata() {
            Ok(v) => v,
            Err(_) => continue,
        };
        if !meta.is_file() {
            continue;
        }
        let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let path = entry.path();
        let size = meta.len();

        if modified < cutoff {
            let _ = std::fs::remove_file(&path);
            continue;
        }

        candidates.push((path, modified, size));
    }

    candidates.sort_by_key(|(_, modified, _)| *modified);
    let mut total: u64 = candidates.iter().map(|(_, _, size)| *size).sum();
    for (path, _modified, size) in candidates {
        if total <= JOB_LOG_TOTAL_CAP_BYTES {
            break;
        }
        let _ = std::fs::remove_file(&path);
        total = total.saturating_sub(size);
    }

    Ok(())
}

fn normalize_and_expand_download_targets(
    paths: &AppPaths,
    inputs: Vec<String>,
    auth_cookie: Option<&str>,
    use_browser_cookies: bool,
) -> Result<Vec<DownloadTarget>> {
    let urls = normalize_direct_urls(inputs)?;
    let mut targets: Vec<DownloadTarget> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for url in urls {
        if is_instagram_user_profile_url(&url) {
            let remaining = MAX_DOWNLOAD_BATCH_URLS.saturating_sub(targets.len());
            if remaining == 0 {
                return Err(EngineError::InstallFailed(format!(
                    "batch limit exceeded: max {MAX_DOWNLOAD_BATCH_URLS} URLs per submission"
                )));
            }

            let expanded = match expand_instagram_profile_media_targets(
                &url,
                remaining + 1,
                auth_cookie,
            ) {
                Ok(values) if !values.is_empty() => values,
                Ok(_) | Err(_) => {
                    let fallback_urls = expand_yt_dlp_urls(
                        paths,
                        &url,
                        remaining + 1,
                        auth_cookie,
                        use_browser_cookies_for_url(&url, use_browser_cookies),
                    )?;
                    fallback_urls
                        .into_iter()
                        .map(|value| DownloadTarget {
                            url: value,
                            provider: DOWNLOAD_PROVIDER_YOUTUBE_YT_DLP,
                        })
                        .collect()
                }
            };

            if expanded.is_empty() {
                return Err(EngineError::InstallFailed(format!(
                    "no downloadable entries found for {}",
                    redact_url_for_log(&url)
                )));
            }

            for candidate in expanded {
                let normalized = normalize_direct_url(&candidate.url)?;
                if !seen.insert(normalized.clone()) {
                    continue;
                }
                targets.push(DownloadTarget {
                    url: normalized,
                    provider: candidate.provider,
                });
                if targets.len() > MAX_DOWNLOAD_BATCH_URLS {
                    return Err(EngineError::InstallFailed(format!(
                        "batch limit exceeded: max {MAX_DOWNLOAD_BATCH_URLS} URLs per submission"
                    )));
                }
            }
            continue;
        }

        if is_instagram_post_like_url(&url) {
            let remaining = MAX_DOWNLOAD_BATCH_URLS.saturating_sub(targets.len());
            if remaining == 0 {
                return Err(EngineError::InstallFailed(format!(
                    "batch limit exceeded: max {MAX_DOWNLOAD_BATCH_URLS} URLs per submission"
                )));
            }

            if let Ok(expanded) = expand_instagram_post_media_targets(&url, auth_cookie) {
                if !expanded.is_empty() {
                    for candidate in expanded {
                        let normalized = normalize_direct_url(&candidate.url)?;
                        if !seen.insert(normalized.clone()) {
                            continue;
                        }
                        targets.push(DownloadTarget {
                            url: normalized,
                            provider: candidate.provider,
                        });
                        if targets.len() > MAX_DOWNLOAD_BATCH_URLS {
                            return Err(EngineError::InstallFailed(format!(
                                "batch limit exceeded: max {MAX_DOWNLOAD_BATCH_URLS} URLs per submission"
                            )));
                        }
                    }
                    continue;
                }
            }
        }

        if is_youtube_url(&url) || is_playlist_candidate_url(&url) {
            let remaining = MAX_DOWNLOAD_BATCH_URLS.saturating_sub(targets.len());
            if remaining == 0 {
                return Err(EngineError::InstallFailed(format!(
                    "batch limit exceeded: max {MAX_DOWNLOAD_BATCH_URLS} URLs per submission"
                )));
            }

            let expanded = expand_yt_dlp_urls(
                paths,
                &url,
                remaining + 1,
                auth_cookie,
                use_browser_cookies_for_url(&url, use_browser_cookies),
            )?;
            if expanded.is_empty() {
                return Err(EngineError::InstallFailed(format!(
                    "no downloadable entries found for {}",
                    redact_url_for_log(&url)
                )));
            }

            for candidate in expanded {
                let normalized = normalize_direct_url(&candidate)?;
                if !seen.insert(normalized.clone()) {
                    continue;
                }
                targets.push(DownloadTarget {
                    url: normalized,
                    provider: DOWNLOAD_PROVIDER_YOUTUBE_YT_DLP,
                });
                if targets.len() > MAX_DOWNLOAD_BATCH_URLS {
                    return Err(EngineError::InstallFailed(format!(
                        "batch limit exceeded: max {MAX_DOWNLOAD_BATCH_URLS} URLs per submission"
                    )));
                }
            }
            continue;
        }

        if !seen.insert(url.clone()) {
            continue;
        }
        let instagram = is_instagram_url(&url);
        let provider = if is_likely_direct_media_url(&url) {
            DOWNLOAD_PROVIDER_DIRECT_HTTP
        } else if instagram {
            DOWNLOAD_PROVIDER_YOUTUBE_YT_DLP
        } else {
            // Most non-direct page URLs require extractor logic (embed/manifest handling).
            DOWNLOAD_PROVIDER_YOUTUBE_YT_DLP
        };
        targets.push(DownloadTarget {
            url,
            provider,
        });
        if targets.len() > MAX_DOWNLOAD_BATCH_URLS {
            return Err(EngineError::InstallFailed(format!(
                "batch limit exceeded: max {MAX_DOWNLOAD_BATCH_URLS} URLs per submission"
            )));
        }
    }

    Ok(targets)
}

fn normalize_direct_urls(inputs: Vec<String>) -> Result<Vec<String>> {
    let mut output: Vec<String> = Vec::new();
    for input in inputs {
        for part in input.split(|ch| matches!(ch, '\n' | '\r' | '\t' | ',' | ';' | ' ')) {
            let trimmed = part.trim();
            if trimmed.is_empty() {
                continue;
            }
            let normalized = normalize_direct_url(trimmed)?;
            if !output.iter().any(|existing| existing == &normalized) {
                output.push(normalized);
            }
        }
    }
    Ok(output)
}

fn normalize_auth_cookie(value: Option<String>) -> Option<String> {
    let raw = value.unwrap_or_default();
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(from_json) = cookie_json_to_header(trimmed) {
        return Some(from_json);
    }

    let path = Path::new(trimmed);
    if path.exists() && path.is_file() {
        if let Ok(contents) = std::fs::read_to_string(path) {
            if let Some(from_json) = cookie_json_to_header(&contents) {
                return Some(from_json);
            }
            let from_file = contents.trim();
            if !from_file.is_empty() {
                return Some(from_file.to_string());
            }
        }
    }

    Some(trimmed.to_string())
}

fn normalize_output_subdir(value: Option<String>) -> Option<String> {
    let raw = value.unwrap_or_default();
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let safe = sanitize_filename_component(trimmed);
    if safe.is_empty() {
        None
    } else {
        Some(safe)
    }
}

fn normalize_output_dir(value: Option<String>) -> Option<String> {
    let raw = value.unwrap_or_default();
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn parse_cookie_header_pairs(cookie_header: &str) -> Vec<(String, String)> {
    let mut pairs: Vec<(String, String)> = Vec::new();
    for part in cookie_header.split(';') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some((name, value)) = trimmed.split_once('=') else {
            continue;
        };
        let name = name.trim();
        if name.is_empty() || name.contains(' ') || name.contains('\t') {
            continue;
        }
        pairs.push((name.to_string(), value.trim().to_string()));
    }
    pairs
}

fn cookie_file_domain_for_url(url: &str) -> Result<String> {
    let parsed = Url::parse(url)
        .map_err(|_| EngineError::InstallFailed(format!("invalid URL for cookies: {}", redact_url_for_log(url))))?;
    let host = parsed
        .host_str()
        .ok_or_else(|| EngineError::InstallFailed("cookie URL missing host".to_string()))?
        .to_ascii_lowercase();
    if host.ends_with("instagram.com") {
        Ok(".instagram.com".to_string())
    } else {
        Ok(host)
    }
}

fn write_cookie_header_as_netscape_file(
    paths: &AppPaths,
    job_id: &str,
    url: &str,
    cookie_header: &str,
) -> Result<PathBuf> {
    let pairs = parse_cookie_header_pairs(cookie_header);
    if pairs.is_empty() {
        return Err(EngineError::InstallFailed(
            "cookie value did not contain valid key=value pairs".to_string(),
        ));
    }

    let artifacts_dir = paths.job_artifacts_dir(job_id);
    std::fs::create_dir_all(&artifacts_dir)?;
    let cookie_path = artifacts_dir.join("yt_dlp_cookies.txt");

    write_cookie_header_as_netscape_path(&cookie_path, url, &pairs)?;
    Ok(cookie_path)
}

fn write_cookie_header_as_netscape_temp_file(
    paths: &AppPaths,
    url: &str,
    cookie_header: &str,
) -> Result<PathBuf> {
    let pairs = parse_cookie_header_pairs(cookie_header);
    if pairs.is_empty() {
        return Err(EngineError::InstallFailed(
            "cookie value did not contain valid key=value pairs".to_string(),
        ));
    }

    let dir = paths.cache_dir().join("yt_dlp_cookie_files");
    std::fs::create_dir_all(&dir)?;
    let cookie_path = dir.join(format!("cookie_{}.txt", Uuid::new_v4()));
    write_cookie_header_as_netscape_path(&cookie_path, url, &pairs)?;
    Ok(cookie_path)
}

fn write_cookie_header_as_netscape_path(
    cookie_path: &Path,
    url: &str,
    pairs: &[(String, String)],
) -> Result<()> {
    let domain = cookie_file_domain_for_url(url)?;
    let include_subdomains = if domain.starts_with('.') {
        "TRUE"
    } else {
        "FALSE"
    };
    let secure = if url.to_ascii_lowercase().starts_with("https://") {
        "TRUE"
    } else {
        "FALSE"
    };

    let mut contents = String::from("# Netscape HTTP Cookie File\n");
    for (name, value) in pairs {
        contents.push_str(&format!(
            "{domain}\t{include_subdomains}\t/\t{secure}\t2147483647\t{name}\t{value}\n"
        ));
    }
    std::fs::write(cookie_path, contents)?;
    Ok(())
}

fn strip_browser_cookie_args(args: &mut Vec<String>) -> bool {
    let mut i = 0_usize;
    while i < args.len() {
        if args[i] == "--cookies-from-browser" {
            args.remove(i);
            if i < args.len() {
                args.remove(i);
            }
            return true;
        }
        i += 1;
    }
    false
}

fn cookie_json_to_header(raw_json: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(raw_json).ok()?;
    let mut pairs: Vec<(String, String)> = Vec::new();

    fn push_pair(pairs: &mut Vec<(String, String)>, name: &str, value: &str) {
        let name = name.trim();
        if name.is_empty() || name.contains(';') || name.contains('=') {
            return;
        }
        pairs.push((name.to_string(), value.trim().to_string()));
    }

    fn collect(value: &serde_json::Value, pairs: &mut Vec<(String, String)>) {
        match value {
            serde_json::Value::Array(values) => {
                for item in values {
                    collect(item, pairs);
                }
            }
            serde_json::Value::Object(map) => {
                if let (Some(name), Some(value)) = (map.get("name"), map.get("value")) {
                    if let (Some(name), Some(value)) = (name.as_str(), value.as_str()) {
                        push_pair(pairs, name, value);
                    }
                    return;
                }
                if let Some(cookies) = map.get("cookies") {
                    collect(cookies, pairs);
                    return;
                }
                for (key, value) in map {
                    if let Some(value) = value.as_str() {
                        push_pair(pairs, key, value);
                    }
                }
            }
            serde_json::Value::String(value) => {
                if let Some((name, v)) = value.trim().split_once('=') {
                    push_pair(pairs, name, v);
                }
            }
            _ => {}
        }
    }

    collect(&value, &mut pairs);
    if pairs.is_empty() {
        return None;
    }

    let mut dedup_seen: HashSet<String> = HashSet::new();
    let mut dedup_pairs: Vec<(String, String)> = Vec::new();
    for (name, value) in pairs.into_iter().rev() {
        if dedup_seen.insert(name.clone()) {
            dedup_pairs.push((name, value));
        }
    }
    dedup_pairs.reverse();

    Some(
        dedup_pairs
            .into_iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; "),
    )
}

fn strip_range_query_params(raw_url: &str) -> String {
    let mut parsed = match Url::parse(raw_url) {
        Ok(v) => v,
        Err(_) => return raw_url.to_string(),
    };
    let pairs: Vec<(String, String)> = parsed.query_pairs().into_owned().collect();
    if pairs.is_empty() {
        return raw_url.to_string();
    }

    let mut kept: Vec<(String, String)> = Vec::new();
    for (k, v) in pairs {
        let key = k.to_ascii_lowercase();
        if key == "range"
            || key == "bytestart"
            || key == "byteend"
            || key == "start"
            || key == "end"
        {
            continue;
        }
        kept.push((k, v));
    }
    if kept.is_empty() {
        parsed.set_query(None);
        return parsed.to_string();
    }

    parsed.set_query(None);
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (k, v) in kept {
        serializer.append_pair(&k, &v);
    }
    let query = serializer.finish();
    parsed.set_query(Some(&query));
    parsed.to_string()
}

fn normalize_direct_url(value: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(EngineError::InstallFailed("empty URL provided".to_string()));
    }
    let redacted = redact_url_for_log(trimmed);

    let uri: ureq::http::Uri = trimmed
        .parse()
        .map_err(|_| EngineError::InstallFailed("invalid URL format".to_string()))?;

    let scheme = uri.scheme_str().unwrap_or_default();
    if scheme != "http" && scheme != "https" {
        return Err(EngineError::InstallFailed(format!(
            "unsupported URL scheme for {redacted}; only http/https are allowed"
        )));
    }
    if uri.authority().is_none() {
        return Err(EngineError::InstallFailed(format!(
            "URL is missing host: {redacted}"
        )));
    }

    Ok(trimmed.to_string())
}

fn redact_url_for_log(value: &str) -> String {
    match value.parse::<ureq::http::Uri>() {
        Ok(uri) => {
            let scheme = uri.scheme_str().unwrap_or("http");
            let authority = uri
                .authority()
                .map(|a| a.as_str().to_string())
                .unwrap_or_else(|| "unknown-host".to_string());
            format!("{scheme}://{authority}/...")
        }
        Err(_) => "[invalid-url]".to_string(),
    }
}

fn read_ytdlp_archive_ids(path: &Path) -> std::io::Result<HashSet<String>> {
    let mut out: HashSet<String> = HashSet::new();
    if !path.exists() {
        return Ok(out);
    }

    let file = File::open(path)?;
    let reader = BufReader::new(file);
    for line in reader.lines().flatten() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Common format: "<extractor> <id>" (e.g., "youtube dQw4w9WgXcQ").
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() == 2 {
            out.insert(parts[1].to_string());
        } else {
            out.insert(trimmed.to_string());
        }
    }
    Ok(out)
}

fn append_youtube_archive_on_success(paths: &AppPaths, subscription_id: &str, url: &str) -> Result<()> {
    let Some(video_id) = subscriptions::youtube_video_id_from_url(url) else {
        return Ok(());
    };

    let Some(sub) = subscriptions::get_youtube_subscription_by_id(paths, subscription_id)? else {
        return Ok(());
    };

    let archive_path = subscriptions::youtube_subscription_archive_path(paths, &sub)?;
    if let Some(parent) = archive_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&archive_path)?;
    writeln!(file, "youtube {video_id}")?;
    Ok(())
}

fn host_from_url(url: &str) -> Option<String> {
    url.parse::<ureq::http::Uri>()
        .ok()?
        .authority()
        .map(|a| a.as_str().to_ascii_lowercase())
}

fn is_youtube_url(url: &str) -> bool {
    let host = match host_from_url(url) {
        Some(v) => v,
        None => return false,
    };

    host == "youtube.com"
        || host == "www.youtube.com"
        || host == "m.youtube.com"
        || host == "music.youtube.com"
        || host == "youtu.be"
        || host.ends_with(".youtube.com")
}

fn is_instagram_url(url: &str) -> bool {
    let host = match host_from_url(url) {
        Some(v) => v,
        None => return false,
    };
    host == "instagram.com" || host == "www.instagram.com" || host.ends_with(".instagram.com")
}

fn is_instagram_media_asset_url(url: &str) -> bool {
    let parsed = match url.parse::<ureq::http::Uri>() {
        Ok(value) => value,
        Err(_) => return false,
    };
    let host = parsed
        .authority()
        .map(|authority| authority.host().to_ascii_lowercase())
        .unwrap_or_default();
    if host.contains("instagram") {
        return true;
    }
    if !host.ends_with("fbcdn.net") {
        return false;
    }
    parsed.path().to_ascii_lowercase().contains("instagram")
}

fn instagram_username_from_url(url: &str) -> Option<String> {
    if !is_instagram_url(url) {
        return None;
    }
    let parsed = url.parse::<ureq::http::Uri>().ok()?;
    let segments: Vec<&str> = parsed
        .path()
        .split('/')
        .filter(|part| !part.trim().is_empty())
        .collect();
    if segments.is_empty() {
        return None;
    }

    let first = segments[0].to_ascii_lowercase();
    let reserved = [
        "p", "reel", "reels", "tv", "stories", "explore", "accounts", "direct", "api",
        "graphql", "about",
    ];
    if reserved.iter().any(|value| *value == first) {
        return None;
    }
    if !first
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '.' || ch == '_')
    {
        return None;
    }
    Some(first)
}

fn is_instagram_user_profile_url(url: &str) -> bool {
    instagram_username_from_url(url).is_some()
}

fn is_instagram_post_like_url(url: &str) -> bool {
    if !is_instagram_url(url) {
        return false;
    }
    let parsed = match url.parse::<ureq::http::Uri>() {
        Ok(v) => v,
        Err(_) => return false,
    };
    let path = parsed.path().to_ascii_lowercase();
    path.starts_with("/p/")
        || path.starts_with("/reel/")
        || path.starts_with("/reels/")
        || path.starts_with("/tv/")
}

fn instagram_shortcode_from_url(url: &str) -> Option<String> {
    if !is_instagram_post_like_url(url) {
        return None;
    }
    let parsed = url.parse::<ureq::http::Uri>().ok()?;
    let segments: Vec<&str> = parsed
        .path()
        .split('/')
        .filter(|part| !part.trim().is_empty())
        .collect();
    if segments.len() < 2 {
        return None;
    }
    let shortcode = segments[1].trim();
    if shortcode.is_empty() {
        None
    } else {
        Some(shortcode.to_string())
    }
}

fn instagram_shortcode_to_media_id(shortcode: &str) -> Option<String> {
    if shortcode.trim().is_empty() {
        return None;
    }
    const ALPHABET: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut value: u128 = 0;
    for ch in shortcode.chars() {
        let index = ALPHABET.find(ch)? as u128;
        value = value.checked_mul(64)?;
        value = value.checked_add(index)?;
    }
    Some(value.to_string())
}

fn is_likely_youtube_video_url(url: &str) -> bool {
    let uri = match url.parse::<ureq::http::Uri>() {
        Ok(v) => v,
        Err(_) => return false,
    };

    let host = uri
        .authority()
        .map(|a| a.as_str().to_ascii_lowercase())
        .unwrap_or_default();
    let path = uri.path();
    if host == "youtu.be" {
        return true;
    }
    if path.starts_with("/shorts/") || path.starts_with("/live/") {
        return true;
    }
    path.starts_with("/watch")
}

fn effective_download_provider(provider: &str, url: &str) -> &'static str {
    let normalized = provider.trim();
    if is_instagram_url(url) && is_likely_direct_media_url(url) {
        return DOWNLOAD_PROVIDER_DIRECT_HTTP;
    }
    if normalized == DOWNLOAD_PROVIDER_YOUTUBE_YT_DLP
        || is_youtube_url(url)
        || is_instagram_url(url)
    {
        DOWNLOAD_PROVIDER_YOUTUBE_YT_DLP
    } else {
        DOWNLOAD_PROVIDER_DIRECT_HTTP
    }
}

fn is_playlist_candidate_url(url: &str) -> bool {
    if is_youtube_url(url) {
        let path = url
            .parse::<ureq::http::Uri>()
            .ok()
            .map(|u| u.path().to_string())
            .unwrap_or_default();
        // Single youtube videos are expanded earlier and should stay single-file at download step.
        return !(path.starts_with("/watch")
            || path.starts_with("/shorts/")
            || path.starts_with("/live/")
            || url.contains("youtu.be/"));
    }
    if is_instagram_url(url) {
        let path = url
            .parse::<ureq::http::Uri>()
            .ok()
            .map(|u| u.path().to_ascii_lowercase())
            .unwrap_or_default();
        // /p/, /reel/, /tv/ are usually single posts; profiles should expand.
        return !(path.starts_with("/p/")
            || path.starts_with("/reel/")
            || path.starts_with("/tv/")
            || path.starts_with("/stories/"));
    }
    false
}

fn use_browser_cookies_for_url(url: &str, requested: bool) -> bool {
    let _ = url;
    requested
}

#[derive(Debug)]
enum CommandRunError {
    Spawn(std::io::Error),
    Wait(std::io::Error),
    Canceled,
    TimedOut(u64),
}

fn kill_child_process_tree(child: &mut std::process::Child) {
    #[cfg(windows)]
    {
        let pid = child.id().to_string();
        let _ = cmd::command("taskkill")
            .args(["/PID", &pid, "/T", "/F"])
            .status();
    }

    let _ = child.kill();
    let _ = child.wait();
}

fn run_command_output_with_control(
    paths: &AppPaths,
    cmd: &mut std::process::Command,
    job_id: Option<&str>,
    timeout_secs: u64,
) -> std::result::Result<std::process::Output, CommandRunError> {
    use std::io::ErrorKind;
    use std::process::Stdio;
    use std::time::Instant;

    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(CommandRunError::Spawn)?;

    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| CommandRunError::Wait(std::io::Error::new(ErrorKind::Other, "stdout pipe missing")))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| CommandRunError::Wait(std::io::Error::new(ErrorKind::Other, "stderr pipe missing")))?;

    let stdout_handle = thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        buf
    });
    let stderr_handle = thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stderr.read_to_end(&mut buf);
        buf
    });

    let started = Instant::now();
    let mut abort_reason: Option<CommandRunError> = None;

    loop {
        if abort_reason.is_none() {
            if let Some(id) = job_id {
                if is_canceled(paths, id).unwrap_or(false) {
                    kill_child_process_tree(&mut child);
                    abort_reason = Some(CommandRunError::Canceled);
                }
            }
        }
        if abort_reason.is_none() && timeout_secs > 0 && started.elapsed() >= Duration::from_secs(timeout_secs) {
            kill_child_process_tree(&mut child);
            abort_reason = Some(CommandRunError::TimedOut(timeout_secs));
        }

        match child.try_wait() {
            Ok(Some(status)) => {
                let stdout = stdout_handle.join().unwrap_or_default();
                let stderr = stderr_handle.join().unwrap_or_default();
                if let Some(reason) = abort_reason {
                    return Err(reason);
                }
                return Ok(std::process::Output {
                    status,
                    stdout,
                    stderr,
                });
            }
            Ok(None) => {
                thread::sleep(Duration::from_millis(EXTERNAL_CMD_POLL_INTERVAL_MS));
            }
            Err(err) => {
                kill_child_process_tree(&mut child);
                let _ = stdout_handle.join();
                let _ = stderr_handle.join();
                return Err(CommandRunError::Wait(err));
            }
        }
    }
}

fn bundled_yt_dlp_path(paths: &AppPaths) -> PathBuf {
    let mut path = paths.tools_dir().join("yt-dlp").join("yt-dlp");
    if cfg!(windows) {
        path.set_extension("exe");
    }
    path
}

fn ensure_bundled_yt_dlp(paths: &AppPaths) -> Result<Option<PathBuf>> {
    let bundled = bundled_yt_dlp_path(paths);
    if bundled.exists() {
        return Ok(Some(bundled));
    }

    let _ = paths;
    Ok(None)
}

fn run_yt_dlp(
    paths: &AppPaths,
    args: &[String],
    job_id: Option<&str>,
    timeout_secs: u64,
) -> Result<std::process::Output> {
    let mut failures: Vec<String> = Vec::new();
    let mut candidates: Vec<(String, Vec<String>)> = Vec::new();
    match ensure_bundled_yt_dlp(paths) {
        Ok(Some(bundled)) if bundled.exists() => {
            candidates.push((bundled.to_string_lossy().to_string(), Vec::new()));
        }
        Ok(_) => {}
        Err(err) => {
            failures.push(format!("bundled yt-dlp bootstrap failed: {err}"));
        }
    }
    candidates.push(("yt-dlp".to_string(), Vec::new()));
    candidates.push((
        "python".to_string(),
        vec!["-m".to_string(), "yt_dlp".to_string()],
    ));
    candidates.push((
        "python3".to_string(),
        vec!["-m".to_string(), "yt_dlp".to_string()],
    ));

    for (program, prefix) in candidates {
        let mut cmd = cmd::command(&program);
        cmd.args(prefix);
        cmd.args(args);
        match run_command_output_with_control(paths, &mut cmd, job_id, timeout_secs) {
            Ok(output) => {
                if output.status.success() {
                    return Ok(output);
                }

                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                failures.push(format!(
                    "{program} failed (code={:?}): {}",
                    output.status.code(),
                    if stderr.is_empty() {
                        "unknown error".to_string()
                    } else {
                        stderr
                    }
                ));
                continue;
            }
            Err(CommandRunError::Spawn(e)) if e.kind() == std::io::ErrorKind::NotFound => {
                continue;
            }
            Err(CommandRunError::Spawn(e)) => {
                failures.push(format!("{program} could not start: {e}"));
                continue;
            }
            Err(CommandRunError::Wait(e)) => {
                failures.push(format!("{program} failed while running: {e}"));
                continue;
            }
            Err(CommandRunError::Canceled) => {
                return Err(EngineError::InstallFailed(
                    "job canceled while running yt-dlp".to_string(),
                ));
            }
            Err(CommandRunError::TimedOut(limit)) => {
                failures.push(format!("{program} timed out after {limit}s"));
                continue;
            }
        }
    }

    if !failures.is_empty() {
        return Err(EngineError::InstallFailed(format!(
            "yt-dlp failed with all available executables: {}",
            failures.join(" | ")
        )));
    }

    Err(EngineError::InstallFailed(
        "yt-dlp is required for YouTube and many webpage video links. Install it with `winget install yt-dlp.yt-dlp` or `pip install -U yt-dlp`.".to_string(),
    ))
}

fn expand_yt_dlp_urls(
    paths: &AppPaths,
    url: &str,
    limit: usize,
    auth_cookie: Option<&str>,
    use_browser_cookies: bool,
) -> Result<Vec<String>> {
    let limit = limit.max(1);
    let mut args = vec![
        "--socket-timeout".to_string(),
        "30".to_string(),
        "--flat-playlist".to_string(),
        "--skip-download".to_string(),
        "--ignore-errors".to_string(),
        "--no-warnings".to_string(),
        "--print".to_string(),
        "webpage_url".to_string(),
        "--playlist-end".to_string(),
        limit.to_string(),
        url.to_string(),
    ];

    let mut cookie_file_path: Option<PathBuf> = None;
    let mut using_cookie_file = false;
    if let Some(cookie) = auth_cookie {
        let trimmed = cookie.trim();
        if !trimmed.is_empty() {
            let cookie_file = write_cookie_header_as_netscape_temp_file(paths, url, trimmed)?;
            args.push("--cookies".to_string());
            args.push(cookie_file.to_string_lossy().to_string());
            cookie_file_path = Some(cookie_file);
            using_cookie_file = true;
        }
    }

    let mut using_browser_cookies = false;
    if use_browser_cookies && !using_cookie_file {
        args.push("--cookies-from-browser".to_string());
        args.push("chrome".to_string());
        using_browser_cookies = true;
    }

    let output_res = match run_yt_dlp(paths, &args, None, YT_DLP_EXPAND_TIMEOUT_SECS) {
        Ok(output) => Ok(output),
        Err(first_err) => {
            if !using_browser_cookies {
                Err(first_err)
            } else {
                let mut retry_args = args.clone();
                if !strip_browser_cookie_args(&mut retry_args) {
                    Err(first_err)
                } else {
                    match run_yt_dlp(paths, &retry_args, None, YT_DLP_EXPAND_TIMEOUT_SECS) {
                        Ok(output) => Ok(output),
                        Err(second_err) => Err(EngineError::InstallFailed(format!(
                            "{first_err}; retry without browser cookies failed: {second_err}"
                        ))),
                    }
                }
            }
        }
    };
    if let Some(path) = cookie_file_path {
        let _ = std::fs::remove_file(path);
    }
    let output = output_res?;
    let mut seen: HashSet<String> = HashSet::new();
    let mut urls: Vec<String> = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if seen.insert(trimmed.to_string()) {
            urls.push(trimmed.to_string());
        }
    }

    if urls.is_empty() && is_likely_youtube_video_url(url) {
        urls.push(url.to_string());
    }

    Ok(urls)
}

fn expand_instagram_profile_media_targets(
    profile_url: &str,
    limit: usize,
    auth_cookie: Option<&str>,
) -> Result<Vec<DownloadTarget>> {
    let username = instagram_username_from_url(profile_url).ok_or_else(|| {
        EngineError::InstallFailed(format!(
            "invalid instagram profile URL: {}",
            redact_url_for_log(profile_url)
        ))
    })?;
    let profile_page_url = format!("https://www.instagram.com/{username}/");
    let profile_info_url = format!(
        "https://i.instagram.com/api/v1/users/web_profile_info/?username={username}"
    );

    let profile_info = download_instagram_json(
        &profile_info_url,
        auth_cookie,
        Some(&profile_page_url),
    )?;
    let user_id = profile_info
        .get("data")
        .and_then(|v| v.get("user"))
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| {
            EngineError::InstallFailed(format!(
                "instagram profile metadata missing user id for {}",
                redact_url_for_log(profile_url)
            ))
        })?;

    let target_limit = limit.max(1).min(MAX_DOWNLOAD_BATCH_URLS);
    let mut out: Vec<DownloadTarget> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut next_max_id: Option<String> = None;

    while out.len() < target_limit {
        let mut feed_url = format!("https://i.instagram.com/api/v1/feed/user/{user_id}/?count=12");
        if let Some(cursor) = next_max_id.as_deref() {
            if !cursor.trim().is_empty() {
                feed_url.push_str("&max_id=");
                feed_url.push_str(cursor.trim());
            }
        }

        let feed_json = download_instagram_json(&feed_url, auth_cookie, Some(&profile_page_url))?;
        let items = feed_json
            .get("items")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        if items.is_empty() {
            break;
        }

        for item in items {
            for media_url in extract_instagram_item_media_urls(&item) {
                let normalized = normalize_direct_url(&media_url)?;
                if seen.insert(normalized.clone()) {
                    out.push(DownloadTarget {
                        url: normalized,
                        provider: DOWNLOAD_PROVIDER_DIRECT_HTTP,
                    });
                    if out.len() >= target_limit {
                        break;
                    }
                }
            }
            if out.len() >= target_limit {
                break;
            }
        }

        if out.len() >= target_limit {
            break;
        }

        let more_available = feed_json
            .get("more_available")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        next_max_id = feed_json
            .get("next_max_id")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string());
        if !more_available || next_max_id.as_deref().unwrap_or("").trim().is_empty() {
            break;
        }
    }

    Ok(out)
}

fn expand_instagram_post_media_targets(
    post_url: &str,
    auth_cookie: Option<&str>,
) -> Result<Vec<DownloadTarget>> {
    let shortcode = instagram_shortcode_from_url(post_url).ok_or_else(|| {
        EngineError::InstallFailed(format!(
            "invalid instagram post URL: {}",
            redact_url_for_log(post_url)
        ))
    })?;
    let media_id = instagram_shortcode_to_media_id(&shortcode).ok_or_else(|| {
        EngineError::InstallFailed(format!(
            "unable to decode instagram shortcode for {}",
            redact_url_for_log(post_url)
        ))
    })?;
    let info_url = format!("https://i.instagram.com/api/v1/media/{media_id}/info/");
    let payload = download_instagram_json(&info_url, auth_cookie, Some(post_url))?;

    let items = payload
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if items.is_empty() {
        return Ok(Vec::new());
    }

    let mut out: Vec<DownloadTarget> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for item in items {
        for media_url in extract_instagram_item_media_urls(&item) {
            let normalized = normalize_direct_url(&media_url)?;
            if seen.insert(normalized.clone()) {
                out.push(DownloadTarget {
                    url: normalized,
                    provider: DOWNLOAD_PROVIDER_DIRECT_HTTP,
                });
            }
        }
    }

    Ok(out)
}

fn extract_instagram_item_media_urls(item: &serde_json::Value) -> Vec<String> {
    let media_type = item.get("media_type").and_then(|v| v.as_i64());
    if media_type == Some(8) {
        let mut out: Vec<String> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        if let Some(nodes) = item.get("carousel_media").and_then(|v| v.as_array()) {
            for node in nodes {
                if let Some(url) = extract_instagram_primary_media_url(node) {
                    if seen.insert(url.clone()) {
                        out.push(url);
                    }
                }
            }
        }
        return out;
    }

    extract_instagram_primary_media_url(item)
        .map(|value| vec![value])
        .unwrap_or_default()
}

fn extract_instagram_primary_media_url(item: &serde_json::Value) -> Option<String> {
    extract_best_instagram_candidate_url(item.get("video_versions").and_then(|v| v.as_array()))
        .or_else(|| {
            extract_best_instagram_candidate_url(
                item.get("image_versions2")
                    .and_then(|v| v.get("candidates"))
                    .and_then(|v| v.as_array()),
            )
        })
}

fn extract_best_instagram_candidate_url(
    candidates: Option<&Vec<serde_json::Value>>,
) -> Option<String> {
    let candidates = candidates?;
    let mut best_url: Option<String> = None;
    let mut best_score: i64 = -1;

    for candidate in candidates {
        let url = candidate.get("url").and_then(|v| v.as_str())?.trim();
        if url.is_empty() {
            continue;
        }
        let score = instagram_candidate_score(candidate);
        if score > best_score {
            best_score = score;
            best_url = Some(url.to_string());
        }
    }

    best_url
}

fn instagram_candidate_score(candidate: &serde_json::Value) -> i64 {
    let width = candidate.get("width").and_then(|v| v.as_i64()).unwrap_or(0);
    let height = candidate.get("height").and_then(|v| v.as_i64()).unwrap_or(0);
    let width = width.max(0);
    let height = height.max(0);
    width.saturating_mul(height)
}

fn download_instagram_json(
    url: &str,
    auth_cookie: Option<&str>,
    referer: Option<&str>,
) -> Result<serde_json::Value> {
    let agent = build_http_agent(25);
    let mut request = agent
        .get(url)
        .header("X-IG-App-ID", INSTAGRAM_API_APP_ID)
        .header("X-Requested-With", "XMLHttpRequest")
        .header("Accept", "application/json");
    if let Some(ref_url) = referer {
        let trimmed = ref_url.trim();
        if !trimmed.is_empty() {
            request = request.header("Referer", trimmed);
        }
    }
    if let Some(cookie) = auth_cookie {
        let trimmed = cookie.trim();
        if !trimmed.is_empty() {
            request = request.header("Cookie", trimmed);
        }
    }

    let mut response = request.call().map_err(|err| {
        EngineError::InstallFailed(format!(
            "instagram api request failed for {}: {err}",
            redact_url_for_log(url)
        ))
    })?;
    let status = response.status().as_u16();
    if status >= 400 {
        return Err(EngineError::InstallFailed(format!(
            "instagram api http {status} for {}",
            redact_url_for_log(url)
        )));
    }

    let mut body = String::new();
    response
        .body_mut()
        .as_reader()
        .take(4 * 1024 * 1024)
        .read_to_string(&mut body)?;
    if body.trim().is_empty() {
        return Err(EngineError::InstallFailed(format!(
            "instagram api returned empty body for {}",
            redact_url_for_log(url)
        )));
    }

    serde_json::from_str(&body).map_err(|err| {
        EngineError::InstallFailed(format!(
            "instagram api returned invalid json for {}: {err}",
            redact_url_for_log(url)
        ))
    })
}

fn download_url_to_library(
    paths: &AppPaths,
    url: &str,
    job_id: &str,
    provider: &str,
    auth_cookie: Option<&str>,
    output_dir: Option<&str>,
    output_subdir: Option<&str>,
    use_browser_cookies: bool,
) -> Result<PathBuf> {
    if provider == DOWNLOAD_PROVIDER_YOUTUBE_YT_DLP {
        return download_yt_dlp_url_to_library(
            paths,
            url,
            job_id,
            auth_cookie,
            output_dir,
            output_subdir,
            use_browser_cookies,
        );
    }

    match download_direct_http_url_to_library(paths, url, job_id, auth_cookie, output_dir, output_subdir) {
        Ok(path) => Ok(path),
        Err(direct_err) => {
            if is_canceled(paths, job_id).unwrap_or(false) {
                return Err(EngineError::InstallFailed("job canceled".to_string()));
            }
            // Fallback for webpage URLs and hosts that need extractor logic.
            match download_yt_dlp_url_to_library(
                paths,
                url,
                job_id,
                auth_cookie,
                output_dir,
                output_subdir,
                use_browser_cookies,
            ) {
                Ok(path) => Ok(path),
                Err(yt_err) => Err(EngineError::InstallFailed(format!(
                    "direct download failed for {} ({direct_err}); yt-dlp fallback failed ({yt_err})",
                    redact_url_for_log(url)
                ))),
            }
        }
    }
}

fn resolve_downloads_dir(paths: &AppPaths, output_subdir: Option<&str>) -> Result<PathBuf> {
    resolve_downloads_dir_with_override(paths, None, output_subdir)
}

fn resolve_downloads_dir_with_override(
    paths: &AppPaths,
    output_dir: Option<&str>,
    output_subdir: Option<&str>,
) -> Result<PathBuf> {
    let resolved = if let Some(raw_output_dir) = output_dir {
        let trimmed = raw_output_dir.trim();
        if trimmed.is_empty() {
            return Err(EngineError::InstallFailed(
                "output folder path is empty".to_string(),
            ));
        }
        let mut custom_dir = PathBuf::from(trimmed);
        if !custom_dir.is_absolute() {
            custom_dir = std::env::current_dir()?.join(custom_dir);
        }
        custom_dir
    } else {
        let base_dir = paths.effective_download_dir()?;
        if !base_dir.exists() {
            return Err(EngineError::InstallFailed(format!(
                "download folder not found: {}. Choose an existing folder or create a new one from Library.",
                base_dir.to_string_lossy()
            )));
        }
        if !base_dir.is_dir() {
            return Err(EngineError::InstallFailed(format!(
                "download path is not a folder: {}",
                base_dir.to_string_lossy()
            )));
        }
        ensure_default_download_subdirs(&base_dir)?;
        if let Some(subdir) = output_subdir {
            let subdir = subdir.trim();
            if subdir.is_empty() {
                base_dir
            } else {
                base_dir.join(subdir)
            }
        } else {
            base_dir
        }
    };

    if !resolved.exists() {
        std::fs::create_dir_all(&resolved)?;
    }
    if !resolved.is_dir() {
        return Err(EngineError::InstallFailed(format!(
            "download output path is not a folder: {}",
            resolved.to_string_lossy()
        )));
    }
    Ok(resolved)
}

fn ensure_default_download_subdirs(base_dir: &Path) -> Result<()> {
    for subdir in [
        DEFAULT_VIDEO_OUTPUT_SUBDIR,
        DEFAULT_INSTAGRAM_OUTPUT_SUBDIR,
        DEFAULT_IMAGES_OUTPUT_SUBDIR,
    ] {
        std::fs::create_dir_all(base_dir.join(subdir))?;
    }
    Ok(())
}

fn default_job_folder_name(job_id: &str) -> String {
    let suffix = &job_id[..job_id.len().min(12)];
    format!("job_{}_{}", now_ms(), suffix)
}

fn default_direct_job_output_dir(
    paths: &AppPaths,
    _provider: &str,
    url: &str,
    job_id: &str,
) -> Result<String> {
    let category = if is_instagram_url(url) || is_instagram_media_asset_url(url) {
        DEFAULT_INSTAGRAM_OUTPUT_SUBDIR
    } else {
        DEFAULT_VIDEO_OUTPUT_SUBDIR
    };
    let base_dir = paths.effective_download_dir()?;
    if !base_dir.exists() {
        return Err(EngineError::InstallFailed(format!(
            "download folder not found: {}. Choose an existing folder or create a new one from Library.",
            base_dir.to_string_lossy()
        )));
    }
    if !base_dir.is_dir() {
        return Err(EngineError::InstallFailed(format!(
            "download path is not a folder: {}",
            base_dir.to_string_lossy()
        )));
    }
    ensure_default_download_subdirs(&base_dir)?;
    let out = base_dir.join(category).join(default_job_folder_name(job_id));
    Ok(out.to_string_lossy().to_string())
}

fn download_direct_http_url_to_library(
    paths: &AppPaths,
    url: &str,
    job_id: &str,
    auth_cookie: Option<&str>,
    output_dir: Option<&str>,
    output_subdir: Option<&str>,
) -> Result<PathBuf> {
    let mut last_err =
        match download_direct_media_asset(paths, url, job_id, auth_cookie, output_dir, output_subdir) {
        Ok(path) => return Ok(path),
        Err(err) => Some(err.to_string()),
    };

    let media_candidates = discover_embedded_media_urls(paths, job_id, url, auth_cookie)?;
    if media_candidates.is_empty() {
        return Err(EngineError::InstallFailed(format!(
            "no downloadable media URLs found in page {} ({})",
            redact_url_for_log(url),
            last_err.unwrap_or_else(|| "direct fetch failed".to_string())
        )));
    }

    for candidate in media_candidates {
        if is_canceled(paths, job_id)? {
            return Err(EngineError::InstallFailed("job canceled".to_string()));
        }

        match download_direct_media_asset(paths, &candidate, job_id, auth_cookie, output_dir, output_subdir) {
            Ok(path) => return Ok(path),
            Err(e) => last_err = Some(e.to_string()),
        }

        if should_try_yt_dlp_candidate(&candidate) {
            match download_yt_dlp_url_to_library(
                paths,
                &candidate,
                job_id,
                auth_cookie,
                output_dir,
                output_subdir,
                use_browser_cookies_for_url(&candidate, false),
            ) {
                Ok(path) => return Ok(path),
                Err(e) => last_err = Some(e.to_string()),
            }
        }
    }

    Err(EngineError::InstallFailed(format!(
        "embedded media download failed for {}: {}",
        redact_url_for_log(url),
        last_err.unwrap_or_else(|| "no valid media candidates".to_string())
    )))
}

fn build_http_agent(timeout_secs: u64) -> ureq::Agent {
    let mut config = ureq::Agent::config_builder();
    config = config
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(timeout_secs.max(1))))
        .user_agent(DEFAULT_HTTP_USER_AGENT);
    config.build().into()
}

fn call_get_with_cookie(
    agent: &ureq::Agent,
    url: &str,
    auth_cookie: Option<&str>,
) -> std::result::Result<ureq::http::Response<ureq::Body>, ureq::Error> {
    let mut request = agent.get(url);
    if let Some(cookie) = auth_cookie {
        let trimmed = cookie.trim();
        if !trimmed.is_empty() {
            request = request.header("Cookie", trimmed);
        }
    }
    request.call()
}

fn download_direct_media_asset(
    paths: &AppPaths,
    url: &str,
    job_id: &str,
    auth_cookie: Option<&str>,
    output_dir: Option<&str>,
    output_subdir: Option<&str>,
) -> Result<PathBuf> {
    if is_canceled(paths, job_id)? {
        return Err(EngineError::InstallFailed("job canceled".to_string()));
    }

    let request_url = strip_range_query_params(url);
    let downloads_dir = resolve_downloads_dir_with_override(paths, output_dir, output_subdir)?;
    std::fs::create_dir_all(&downloads_dir)?;

    let agent = build_http_agent(60);
    let mut response = call_get_with_cookie(&agent, &request_url, auth_cookie).map_err(|err| {
        EngineError::InstallFailed(format!(
            "request failed for {}: {err}",
            redact_url_for_log(url)
        ))
    })?;

    let status = response.status().as_u16();
    if status >= 400 {
        return Err(EngineError::InstallFailed(format!(
            "http {status} for {}",
            redact_url_for_log(url)
        )));
    }

    let content_type = header_string(&response, "content-type");
    let filename = suggested_download_filename(&request_url, job_id);
    let final_path = downloads_dir.join(filename);
    let temp_name = format!(
        "{}.part",
        final_path
            .file_name()
            .and_then(|v| v.to_str())
            .unwrap_or("download.bin")
    );
    let temp_path = downloads_dir.join(temp_name);
    let _ = std::fs::remove_file(&temp_path);

    let mut output = std::fs::File::create(&temp_path)?;
    let mut body_reader = response.body_mut().as_reader();
    let mut buf = [0_u8; 64 * 1024];
    let mut sniff_prefix = Vec::with_capacity(DIRECT_DOWNLOAD_SNIFF_BYTES);
    let mut bytes_written: u64 = 0;

    loop {
        if is_canceled(paths, job_id)? {
            let _ = std::fs::remove_file(&temp_path);
            return Err(EngineError::InstallFailed("job canceled".to_string()));
        }

        let read = body_reader.read(&mut buf).map_err(|err| {
            let _ = std::fs::remove_file(&temp_path);
            EngineError::InstallFailed(format!(
                "failed reading response body for {}: {err}",
                redact_url_for_log(url)
            ))
        })?;
        if read == 0 {
            break;
        }

        if sniff_prefix.len() < DIRECT_DOWNLOAD_SNIFF_BYTES {
            let take = (DIRECT_DOWNLOAD_SNIFF_BYTES - sniff_prefix.len()).min(read);
            sniff_prefix.extend_from_slice(&buf[..take]);
        }

        output.write_all(&buf[..read]).map_err(|err| {
            let _ = std::fs::remove_file(&temp_path);
            EngineError::InstallFailed(format!(
                "failed writing media file for {}: {err}",
                redact_url_for_log(url)
            ))
        })?;
        bytes_written = bytes_written.saturating_add(read as u64);
    }
    output.flush()?;
    drop(output);

    if bytes_written == 0 {
        let _ = std::fs::remove_file(&temp_path);
        return Err(EngineError::InstallFailed(format!(
            "downloaded file is empty for {}",
            redact_url_for_log(url)
        )));
    }

    if is_non_media_response(&content_type, &sniff_prefix)
        || looks_like_stream_manifest(&content_type, &sniff_prefix)
    {
        let _ = std::fs::remove_file(&temp_path);
        return Err(EngineError::InstallFailed(format!(
            "URL did not resolve to a direct media file: {}",
            redact_url_for_log(url)
        )));
    }

    if final_path.exists() {
        let _ = std::fs::remove_file(&final_path);
    }
    std::fs::rename(&temp_path, &final_path)?;

    if let Err(err) = ffmpeg::probe(paths, &final_path) {
        let _ = std::fs::remove_file(&final_path);
        return Err(EngineError::InstallFailed(format!(
            "downloaded file from {} is not valid playable media: {err}",
            redact_url_for_log(url)
        )));
    }

    Ok(final_path)
}

fn discover_embedded_media_urls(
    paths: &AppPaths,
    job_id: &str,
    start_url: &str,
    auth_cookie: Option<&str>,
) -> Result<Vec<String>> {
    let start_url = normalize_direct_url(start_url)?;
    let agent = build_http_agent(25);

    let mut queue: VecDeque<String> = VecDeque::new();
    queue.push_back(start_url.clone());

    let mut queued: HashSet<String> = HashSet::new();
    queued.insert(start_url.clone());

    let mut visited: HashSet<String> = HashSet::new();
    let mut found: Vec<String> = Vec::new();
    let mut found_set: HashSet<String> = HashSet::new();

    while let Some(page_url) = queue.pop_front() {
        if is_canceled(paths, job_id)? {
            return Err(EngineError::InstallFailed("job canceled".to_string()));
        }
        if visited.len() >= EMBED_CRAWL_MAX_PAGES || found.len() >= EMBED_CRAWL_MAX_CANDIDATES {
            break;
        }
        if !visited.insert(page_url.clone()) {
            continue;
        }

        if is_likely_direct_media_url(&page_url) {
            push_unique_url(
                &mut found,
                &mut found_set,
                page_url.clone(),
                EMBED_CRAWL_MAX_CANDIDATES,
            );
            continue;
        }

        let mut response = match call_get_with_cookie(&agent, &page_url, auth_cookie) {
            Ok(resp) => resp,
            Err(_) => continue,
        };

        if response.status().as_u16() >= 400 {
            continue;
        }

        let content_type = header_string(&response, "content-type");
        if is_probable_media_content_type(&content_type) {
            push_unique_url(
                &mut found,
                &mut found_set,
                page_url.clone(),
                EMBED_CRAWL_MAX_CANDIDATES,
            );
            continue;
        }

        if !is_embedded_discovery_content_type(&content_type) {
            continue;
        }

        let mut body = Vec::new();
        if response
            .body_mut()
            .as_reader()
            .take(EMBED_FETCH_MAX_BODY_BYTES)
            .read_to_end(&mut body)
            .is_err()
        {
            continue;
        }
        if body.is_empty() {
            continue;
        }

        let html = String::from_utf8_lossy(&body).into_owned();
        let document = Html::parse_document(&html);
        let Ok(base_url) = Url::parse(&page_url) else {
            continue;
        };
        let (media_urls, frame_urls) = extract_embedded_urls(&document, &html, &base_url);

        for media_url in media_urls {
            push_unique_url(
                &mut found,
                &mut found_set,
                media_url,
                EMBED_CRAWL_MAX_CANDIDATES,
            );
        }

        for frame_url in frame_urls {
            if found.len() >= EMBED_CRAWL_MAX_CANDIDATES {
                break;
            }
            if visited.contains(&frame_url) || queued.contains(&frame_url) {
                continue;
            }
            if visited.len() + queue.len() >= EMBED_CRAWL_MAX_PAGES {
                break;
            }
            queue.push_back(frame_url.clone());
            queued.insert(frame_url);
        }
    }

    Ok(found)
}

fn extract_embedded_urls(
    document: &Html,
    html: &str,
    base_url: &Url,
) -> (Vec<String>, Vec<String>) {
    let selector_media = Selector::parse("video[src], audio[src], source[src], a[href]")
        .expect("valid media selector");
    let selector_meta = Selector::parse("meta[content]").expect("valid meta selector");
    let selector_frames = Selector::parse("iframe[src], frame[src], embed[src], object[data]")
        .expect("valid iframe selector");

    let mut media_urls: Vec<String> = Vec::new();
    let mut media_set: HashSet<String> = HashSet::new();
    let mut frame_urls: Vec<String> = Vec::new();
    let mut frame_set: HashSet<String> = HashSet::new();

    for tag in document.select(&selector_media) {
        let attr = if tag.value().name() == "a" {
            "href"
        } else {
            "src"
        };
        let Some(raw) = tag.value().attr(attr) else {
            continue;
        };
        let Some(normalized) = normalize_url_with_base(raw, base_url) else {
            continue;
        };
        if is_likely_direct_media_url(&normalized) {
            push_unique_url(
                &mut media_urls,
                &mut media_set,
                normalized,
                EMBED_CRAWL_MAX_CANDIDATES,
            );
        }
    }

    for meta in document.select(&selector_meta) {
        let marker = meta
            .value()
            .attr("property")
            .or_else(|| meta.value().attr("name"))
            .unwrap_or("")
            .to_ascii_lowercase();
        if !marker.contains("video") && !marker.contains("stream") {
            continue;
        }
        let Some(raw) = meta.value().attr("content") else {
            continue;
        };
        let Some(normalized) = normalize_url_with_base(raw, base_url) else {
            continue;
        };
        if is_likely_direct_media_url(&normalized) {
            push_unique_url(
                &mut media_urls,
                &mut media_set,
                normalized,
                EMBED_CRAWL_MAX_CANDIDATES,
            );
        } else if is_likely_embed_page_url(&normalized) {
            push_unique_url(
                &mut frame_urls,
                &mut frame_set,
                normalized,
                EMBED_CRAWL_MAX_PAGES,
            );
        }
    }

    for frame in document.select(&selector_frames) {
        let attr = if frame.value().name() == "object" {
            "data"
        } else {
            "src"
        };
        let Some(raw) = frame.value().attr(attr) else {
            continue;
        };
        let Some(normalized) = normalize_url_with_base(raw, base_url) else {
            continue;
        };
        if is_likely_direct_media_url(&normalized) {
            push_unique_url(
                &mut media_urls,
                &mut media_set,
                normalized,
                EMBED_CRAWL_MAX_CANDIDATES,
            );
        } else {
            push_unique_url(
                &mut frame_urls,
                &mut frame_set,
                normalized,
                EMBED_CRAWL_MAX_PAGES,
            );
        }
    }

    let html_unescaped = html.replace("\\/", "/");
    let absolute_media = Regex::new(
        r#"(?i)https?://[^"'<>\s]+?\.(?:mp4|m4v|mov|webm|mkv|flv|avi|wmv|mpg|mpeg|ts|m2ts|mp3|m4a|aac|wav|flac|ogg|opus|m3u8|mpd|m4s)(?:\?[^"'<>\s]*)?"#,
    )
    .expect("valid absolute media regex");
    for m in absolute_media.find_iter(&html_unescaped) {
        let Some(normalized) = normalize_url_with_base(m.as_str(), base_url) else {
            continue;
        };
        if is_likely_direct_media_url(&normalized) {
            push_unique_url(
                &mut media_urls,
                &mut media_set,
                normalized,
                EMBED_CRAWL_MAX_CANDIDATES,
            );
        }
    }

    let kv_url = Regex::new(r#"(?i)(?:file|src|source|url)\s*[:=]\s*["']([^"']+)["']"#)
        .expect("valid kv url regex");
    for caps in kv_url.captures_iter(&html_unescaped) {
        let Some(raw) = caps.get(1).map(|m| m.as_str()) else {
            continue;
        };
        let Some(normalized) = normalize_url_with_base(raw, base_url) else {
            continue;
        };
        if is_likely_direct_media_url(&normalized) {
            push_unique_url(
                &mut media_urls,
                &mut media_set,
                normalized,
                EMBED_CRAWL_MAX_CANDIDATES,
            );
        } else if is_likely_embed_page_url(&normalized) {
            push_unique_url(
                &mut frame_urls,
                &mut frame_set,
                normalized,
                EMBED_CRAWL_MAX_PAGES,
            );
        }
    }

    (media_urls, frame_urls)
}

fn push_unique_url(out: &mut Vec<String>, seen: &mut HashSet<String>, value: String, limit: usize) {
    if out.len() >= limit {
        return;
    }
    if seen.insert(value.clone()) {
        out.push(value);
    }
}

fn normalize_url_with_base(raw_url: &str, base_url: &Url) -> Option<String> {
    let cleaned = raw_url
        .trim()
        .trim_matches(|ch| matches!(ch, '"' | '\'' | '(' | ')' | '[' | ']'))
        .replace("&amp;", "&")
        .replace("\\u0026", "&")
        .replace("\\/", "/");

    if cleaned.is_empty()
        || cleaned.starts_with("data:")
        || cleaned.starts_with("blob:")
        || cleaned.starts_with("javascript:")
        || cleaned.starts_with('#')
    {
        return None;
    }

    let mut parsed = if cleaned.starts_with("//") {
        Url::parse(&format!("{}:{}", base_url.scheme(), cleaned)).ok()?
    } else if let Ok(url) = Url::parse(&cleaned) {
        url
    } else {
        base_url.join(&cleaned).ok()?
    };

    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return None;
    }
    parsed.set_fragment(None);
    Some(parsed.to_string())
}

fn is_likely_direct_media_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    if lower.contains("googlevideo.com/videoplayback")
        || lower.contains("mime=video")
        || lower.contains("mime=audio")
    {
        return true;
    }

    let Ok(parsed) = Url::parse(url) else {
        return false;
    };
    let path = parsed.path().to_ascii_lowercase();
    path.ends_with(".mp4")
        || path.ends_with(".m4v")
        || path.ends_with(".mov")
        || path.ends_with(".webm")
        || path.ends_with(".mkv")
        || path.ends_with(".flv")
        || path.ends_with(".avi")
        || path.ends_with(".wmv")
        || path.ends_with(".mpg")
        || path.ends_with(".mpeg")
        || path.ends_with(".ts")
        || path.ends_with(".m2ts")
        || path.ends_with(".mp3")
        || path.ends_with(".m4a")
        || path.ends_with(".aac")
        || path.ends_with(".wav")
        || path.ends_with(".flac")
        || path.ends_with(".ogg")
        || path.ends_with(".opus")
        || path.ends_with(".m3u8")
        || path.ends_with(".mpd")
        || path.ends_with(".m4s")
}

fn is_likely_embed_page_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.contains("/embed/")
        || lower.contains("player")
        || lower.contains("/iframe/")
        || lower.contains("/video/")
        || lower.contains("/watch")
        || lower.contains("/media/")
        || lower.contains("youtube.com/embed/")
        || lower.contains("player.vimeo.com/video/")
        || lower.contains("dailymotion.com/embed/")
}

fn should_try_yt_dlp_candidate(url: &str) -> bool {
    is_likely_embed_page_url(url) || is_stream_manifest_url(url) || !is_likely_direct_media_url(url)
}

fn is_stream_manifest_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.contains(".m3u8") || lower.contains(".mpd") || lower.contains(".m4s")
}

fn looks_like_stream_manifest(content_type: &str, sniff_prefix: &[u8]) -> bool {
    let ctype = content_type.to_ascii_lowercase();
    if ctype.contains("x-mpegurl")
        || ctype.contains("vnd.apple.mpegurl")
        || ctype.contains("dash+xml")
    {
        return true;
    }

    if sniff_prefix.is_empty() {
        return false;
    }

    let head = String::from_utf8_lossy(sniff_prefix).to_ascii_lowercase();
    head.trim_start().starts_with("#extm3u") || head.contains("<mpd")
}

fn is_embedded_discovery_content_type(content_type: &str) -> bool {
    if content_type.is_empty() {
        return true;
    }
    content_type.contains("text/html")
        || content_type.contains("application/xhtml+xml")
        || content_type.contains("application/json")
        || content_type.contains("text/javascript")
        || content_type.contains("application/javascript")
        || content_type.starts_with("text/")
}

fn header_string(response: &ureq::http::Response<ureq::Body>, key: &str) -> String {
    response
        .headers()
        .get(key)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn download_yt_dlp_url_to_library(
    paths: &AppPaths,
    url: &str,
    job_id: &str,
    auth_cookie: Option<&str>,
    output_dir: Option<&str>,
    output_subdir: Option<&str>,
    use_browser_cookies: bool,
) -> Result<PathBuf> {
    let downloads_dir = resolve_downloads_dir_with_override(paths, output_dir, output_subdir)?;

    let suffix = &job_id[..job_id.len().min(8)];
    let template = format!("%(title).80B_{suffix}_%(id)s.%(ext)s");

    let mut args = vec![
        "--socket-timeout".to_string(),
        "30".to_string(),
        "--retries".to_string(),
        "3".to_string(),
        "--fragment-retries".to_string(),
        "3".to_string(),
        "--no-warnings".to_string(),
        "--ignore-errors".to_string(),
        "--restrict-filenames".to_string(),
        "--no-progress".to_string(),
        "--print".to_string(),
        "after_move:filepath".to_string(),
        "-P".to_string(),
        downloads_dir.to_string_lossy().to_string(),
        "-o".to_string(),
        template,
        url.to_string(),
    ];

    if !is_playlist_candidate_url(url) {
        args.insert(0, "--no-playlist".to_string());
    }

    let ffmpeg_cmd = paths.ffmpeg_cmd();
    if ffmpeg_cmd.exists() {
        args.push("--ffmpeg-location".to_string());
        args.push(ffmpeg_cmd.to_string_lossy().to_string());
    }

    let mut using_cookie_file = false;
    let mut cookie_file_path: Option<PathBuf> = None;
    if let Some(cookie) = auth_cookie {
        let trimmed = cookie.trim();
        if !trimmed.is_empty() {
            let cookie_file = write_cookie_header_as_netscape_file(paths, job_id, url, trimmed)?;
            args.push("--cookies".to_string());
            args.push(cookie_file.to_string_lossy().to_string());
            cookie_file_path = Some(cookie_file);
            using_cookie_file = true;
        }
    }

    let mut using_browser_cookies = false;
    if use_browser_cookies_for_url(url, use_browser_cookies) && !using_cookie_file {
        args.push("--cookies-from-browser".to_string());
        args.push("chrome".to_string());
        using_browser_cookies = true;
    }

    let output_res = match run_yt_dlp(paths, &args, Some(job_id), YT_DLP_DOWNLOAD_TIMEOUT_SECS) {
        Ok(output) => Ok(output),
        Err(first_err) => {
            if !using_browser_cookies {
                Err(first_err)
            } else {
                let mut retry_args = args.clone();
                if !strip_browser_cookie_args(&mut retry_args) {
                    Err(first_err)
                } else {
                    match run_yt_dlp(paths, &retry_args, Some(job_id), YT_DLP_DOWNLOAD_TIMEOUT_SECS) {
                        Ok(output) => Ok(output),
                        Err(second_err) => Err(EngineError::InstallFailed(format!(
                            "{first_err}; retry without browser cookies failed: {second_err}"
                        ))),
                    }
                }
            }
        }
    };
    if let Some(path) = cookie_file_path {
        let _ = std::fs::remove_file(path);
    }
    let output = output_res?;
    let downloaded = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .last()
        .map(PathBuf::from)
        .ok_or_else(|| {
            EngineError::InstallFailed(format!(
                "yt-dlp did not report an output file for {}",
                redact_url_for_log(url)
            ))
        })?;

    let downloaded = if downloaded.is_absolute() {
        downloaded
    } else {
        downloads_dir.join(downloaded)
    };
    let meta = std::fs::metadata(&downloaded).map_err(|_| {
        EngineError::InstallFailed(format!(
            "yt-dlp reported a missing file for {}",
            redact_url_for_log(url)
        ))
    })?;
    if meta.len() == 0 {
        return Err(EngineError::InstallFailed(format!(
            "yt-dlp downloaded an empty file for {}",
            redact_url_for_log(url)
        )));
    }

    Ok(downloaded)
}

fn write_job_cookie_secret(paths: &AppPaths, job_id: &str, cookie_header: &str) -> Result<()> {
    let cookie_header = cookie_header.trim();
    if cookie_header.is_empty() {
        return Ok(());
    }

    paths.ensure_dirs()?;
    let path = paths.job_cookie_secret_path(job_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let tmp_path = path.with_extension("tmp");
    std::fs::write(&tmp_path, format!("{cookie_header}\n"))?;
    if path.exists() {
        let _ = std::fs::remove_file(&path);
    }
    std::fs::rename(&tmp_path, &path)?;
    Ok(())
}

fn read_job_cookie_secret(paths: &AppPaths, job_id: &str) -> Option<String> {
    let path = paths.job_cookie_secret_path(job_id);
    let contents = std::fs::read_to_string(path).ok()?;
    let trimmed = contents.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn remove_job_cookie_secret(paths: &AppPaths, job_id: &str) {
    let path = paths.job_cookie_secret_path(job_id);
    let _ = std::fs::remove_file(path);
}

fn delete_job_by_id(paths: &AppPaths, job_id: &str) -> Result<()> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    let _ = conn.execute("DELETE FROM job WHERE id=?1", [job_id])?;
    Ok(())
}

fn is_non_media_response(content_type: &str, sniff_prefix: &[u8]) -> bool {
    let ctype = content_type.trim().to_ascii_lowercase();
    if !ctype.is_empty() {
        if is_probable_media_content_type(&ctype) {
            return false;
        }
        if ctype.starts_with("text/")
            || ctype.contains("html")
            || ctype.contains("json")
            || ctype.contains("xml")
            || ctype.contains("javascript")
            || ctype.contains("x-mpegurl")
            || ctype.contains("vnd.apple.mpegurl")
        {
            return true;
        }
    }
    looks_like_textual_error_payload(sniff_prefix)
}

fn is_probable_media_content_type(content_type: &str) -> bool {
    let ctype = content_type.to_ascii_lowercase();
    ctype.starts_with("video/")
        || ctype.starts_with("audio/")
        || ctype.contains("application/octet-stream")
        || ctype.contains("application/mp4")
        || ctype.contains("application/x-matroska")
        || ctype.contains("application/ogg")
}

fn looks_like_textual_error_payload(sniff_prefix: &[u8]) -> bool {
    if sniff_prefix.is_empty() {
        return false;
    }
    let head = String::from_utf8_lossy(sniff_prefix);
    let trimmed = head.trim_start().to_ascii_lowercase();
    trimmed.starts_with("<!doctype html")
        || trimmed.starts_with("<html")
        || trimmed.starts_with("<?xml")
        || trimmed.starts_with("{\"")
        || trimmed.starts_with("{")
        || trimmed.starts_with("[")
}

fn suggested_download_filename(url: &str, job_id: &str) -> String {
    let raw_name = url
        .parse::<ureq::http::Uri>()
        .ok()
        .and_then(|uri| {
            uri.path()
                .rsplit('/')
                .next()
                .map(|segment| segment.to_string())
        })
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "download.mp4".to_string());

    let mut safe_name = sanitize_filename_component(&raw_name);
    if safe_name.is_empty() {
        safe_name = "download.mp4".to_string();
    }

    let mut path = PathBuf::from(&safe_name);
    if path.extension().is_none() {
        path.set_extension("mp4");
    }

    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("download");
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("mp4");
    let suffix = &job_id[..job_id.len().min(8)];
    format!("{stem}_{suffix}.{ext}")
}

fn sanitize_filename_component(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }

    let trimmed = out.trim_matches(|ch| ch == '.' || ch == '_').to_string();
    if trimmed.is_empty() {
        return String::new();
    }

    let mut limited = trimmed;
    if limited.len() > 80 {
        limited.truncate(80);
    }
    limited
}

fn atempo_chain_for_factor(factor: f32) -> String {
    let mut remaining = factor.max(0.0001) as f64;
    let mut parts: Vec<f64> = Vec::new();

    // FFmpeg atempo supports [0.5, 2.0]. Chain filters if needed.
    while remaining > 2.0 {
        parts.push(2.0);
        remaining /= 2.0;
    }
    while remaining < 0.5 {
        parts.push(0.5);
        remaining /= 0.5;
    }
    parts.push(remaining);

    parts
        .into_iter()
        .map(|v| format!("atempo={:.6}", v))
        .collect::<Vec<_>>()
        .join(",")
}

fn normalize_lang_tag(raw: Option<&str>) -> Option<&'static str> {
    let v = raw?.trim().to_lowercase();
    if v.is_empty() {
        return None;
    }
    match v.as_str() {
        "en" | "eng" | "english" => Some("eng"),
        "ja" | "jpn" | "japanese" => Some("jpn"),
        "ko" | "kor" | "korean" => Some("kor"),
        "und" | "unknown" => Some("und"),
        _ => None,
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
    use rusqlite::params;

    #[test]
    fn running_jobs_are_marked_failed_after_restart_recovery() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        db::ensure_schema(&paths).expect("schema");

        let job = enqueue_dummy_sleep(&paths, 10).expect("enqueue");

        let conn = db::open(&paths).expect("open");
        db::migrate(&conn).expect("migrate");
        conn.execute(
            "UPDATE job SET status=?1, started_at_ms=?2 WHERE id=?3",
            params![JobStatus::Running.as_str(), now_ms(), job.id],
        )
        .expect("force running");

        let updated = requeue_orphaned_running_jobs(&conn).expect("requeue");
        assert_eq!(updated, 1);

        let (status, started_at_ms, finished_at_ms, error): (
            String,
            Option<i64>,
            Option<i64>,
            Option<String>,
        ) = conn
            .query_row(
                "SELECT status, started_at_ms, finished_at_ms, error FROM job WHERE id=?1",
                [job.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("select");
        assert_eq!(status, JobStatus::Failed.as_str());
        assert!(started_at_ms.is_none());
        assert!(finished_at_ms.is_some());
        assert_eq!(error.as_deref(), Some("interrupted by app shutdown"));
    }

    #[test]
    fn rotate_file_backups_shifts_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let log = dir.path().join("job.jsonl");

        std::fs::write(&log, "main").expect("write main");
        std::fs::write(path_with_suffix(&log, ".1"), "b1").expect("write b1");
        std::fs::write(path_with_suffix(&log, ".2"), "b2").expect("write b2");

        rotate_file_backups(&log, 3).expect("rotate");

        assert!(!log.exists());
        assert_eq!(
            std::fs::read_to_string(path_with_suffix(&log, ".1")).expect("r1"),
            "main"
        );
        assert_eq!(
            std::fs::read_to_string(path_with_suffix(&log, ".2")).expect("r2"),
            "b1"
        );
        assert_eq!(
            std::fs::read_to_string(path_with_suffix(&log, ".3")).expect("r3"),
            "b2"
        );
    }

    #[test]
    fn normalize_direct_url_allows_http_https_only() {
        assert!(normalize_direct_url("https://example.com/video.mp4").is_ok());
        assert!(normalize_direct_url("http://example.com/video.mp4").is_ok());
        assert!(normalize_direct_url("ftp://example.com/video.mp4").is_err());
        assert!(normalize_direct_url("file:///tmp/video.mp4").is_err());
    }

    #[test]
    fn normalize_direct_urls_splits_and_dedupes() {
        let urls = vec![
            "https://example.com/a.mp4, https://example.com/b.mp4".to_string(),
            "https://example.com/a.mp4\nhttps://example.com/c.mp4".to_string(),
        ];
        let out = normalize_direct_urls(urls).expect("normalize");
        assert_eq!(out.len(), 3);
        assert_eq!(out[0], "https://example.com/a.mp4");
        assert_eq!(out[1], "https://example.com/b.mp4");
        assert_eq!(out[2], "https://example.com/c.mp4");
    }

    #[test]
    fn youtube_url_detection_covers_common_hosts() {
        assert!(is_youtube_url("https://youtube.com/watch?v=abc"));
        assert!(is_youtube_url("https://www.youtube.com/watch?v=abc"));
        assert!(is_youtube_url("https://youtu.be/abc"));
        assert!(!is_youtube_url("https://vimeo.com/1234"));
    }

    #[test]
    fn likely_youtube_video_url_detects_watch_and_shorts() {
        assert!(is_likely_youtube_video_url(
            "https://www.youtube.com/watch?v=abc123"
        ));
        assert!(is_likely_youtube_video_url("https://youtu.be/abc123"));
        assert!(is_likely_youtube_video_url(
            "https://www.youtube.com/shorts/abc123"
        ));
        assert!(!is_likely_youtube_video_url(
            "https://www.youtube.com/@channel/videos"
        ));
    }

    #[test]
    fn effective_provider_prefers_youtube_for_youtube_urls() {
        let url = "https://www.youtube.com/watch?v=abc";
        assert_eq!(
            effective_download_provider(DOWNLOAD_PROVIDER_DIRECT_HTTP, url),
            DOWNLOAD_PROVIDER_YOUTUBE_YT_DLP
        );
        assert_eq!(
            effective_download_provider(
                DOWNLOAD_PROVIDER_YOUTUBE_YT_DLP,
                "https://example.com/a.mp4"
            ),
            DOWNLOAD_PROVIDER_YOUTUBE_YT_DLP
        );
        assert_eq!(
            effective_download_provider(DOWNLOAD_PROVIDER_DIRECT_HTTP, "https://example.com/a.mp4"),
            DOWNLOAD_PROVIDER_DIRECT_HTTP
        );
    }

    #[test]
    fn normalize_and_expand_enforces_batch_cap_for_direct_urls() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        let mut urls = Vec::new();
        for i in 0..=MAX_DOWNLOAD_BATCH_URLS {
            urls.push(format!("https://example.com/video-{i}.mp4"));
        }
        let err = normalize_and_expand_download_targets(&paths, urls, None, false)
            .expect_err("must fail");
        assert!(
            err.to_string().contains("batch limit exceeded"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn queue_pause_state_roundtrip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        db::ensure_schema(&paths).expect("schema");

        let initial = get_queue_control(&paths).expect("state");
        assert!(!initial.paused);

        let paused = set_queue_paused(&paths, true).expect("pause");
        assert!(paused.paused);
        assert!(get_queue_control(&paths).expect("state").paused);

        let resumed = set_queue_paused(&paths, false).expect("resume");
        assert!(!resumed.paused);
        assert!(!get_queue_control(&paths).expect("state").paused);
    }

    #[test]
    fn runtime_settings_default_to_four_and_can_change() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        db::ensure_schema(&paths).expect("schema");

        let initial = get_runtime_settings(&paths).expect("runtime");
        assert_eq!(initial.max_concurrency, DEFAULT_MAX_CONCURRENT_JOBS);

        let updated = set_runtime_max_concurrency(&paths, 9).expect("set runtime");
        assert_eq!(updated.max_concurrency, 9);
        assert_eq!(
            get_runtime_settings(&paths)
                .expect("runtime")
                .max_concurrency,
            9
        );
    }

    #[test]
    fn normalize_auth_cookie_accepts_json_cookie_arrays() {
        let cookie = normalize_auth_cookie(Some(
            r#"[{"name":"sessionid","value":"abc"},{"name":"csrftoken","value":"xyz"}]"#
                .to_string(),
        ))
        .expect("cookie");
        assert_eq!(cookie, "sessionid=abc; csrftoken=xyz");
    }

    #[test]
    fn strip_range_query_params_removes_partial_download_keys() {
        let url = "https://cdn.example.com/video.mp4?token=abc&range=0-999999&start=0";
        let out = strip_range_query_params(url);
        assert_eq!(out, "https://cdn.example.com/video.mp4?token=abc");
    }

    #[test]
    fn cancel_all_jobs_marks_queued_and_running_as_canceled() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        db::ensure_schema(&paths).expect("schema");

        let queued = enqueue_dummy_sleep(&paths, 3).expect("enqueue queued");
        let running = enqueue_dummy_sleep(&paths, 3).expect("enqueue running");

        let conn = db::open(&paths).expect("open");
        db::migrate(&conn).expect("migrate");
        conn.execute(
            "UPDATE job SET status=?1, started_at_ms=?2 WHERE id=?3",
            params![JobStatus::Running.as_str(), now_ms(), &running.id],
        )
        .expect("set running");

        let updated = cancel_all_jobs(&paths).expect("cancel all");
        assert_eq!(updated, 2);

        let status_queued: String = conn
            .query_row("SELECT status FROM job WHERE id=?1", [&queued.id], |row| {
                row.get(0)
            })
            .expect("status queued");
        let status_running: String = conn
            .query_row("SELECT status FROM job WHERE id=?1", [&running.id], |row| {
                row.get(0)
            })
            .expect("status running");
        assert_eq!(status_queued, JobStatus::Canceled.as_str());
        assert_eq!(status_running, JobStatus::Canceled.as_str());
    }

    #[test]
    fn flush_jobs_cache_removes_terminal_jobs_and_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        db::ensure_schema(&paths).expect("schema");

        let succeeded = enqueue_dummy_sleep(&paths, 1).expect("enqueue succeeded");
        let failed = enqueue_dummy_sleep(&paths, 1).expect("enqueue failed");
        let queued = enqueue_dummy_sleep(&paths, 1).expect("enqueue queued");

        let conn = db::open(&paths).expect("open");
        db::migrate(&conn).expect("migrate");
        conn.execute(
            "UPDATE job SET status=?1, finished_at_ms=?2 WHERE id=?3",
            params![JobStatus::Succeeded.as_str(), now_ms(), &succeeded.id],
        )
        .expect("mark succeeded");
        conn.execute(
            "UPDATE job SET status=?1, finished_at_ms=?2, error=?4 WHERE id=?3",
            params![
                JobStatus::Failed.as_str(),
                now_ms(),
                &failed.id,
                "forced failure"
            ],
        )
        .expect("mark failed");

        let succeeded_log = PathBuf::from(&succeeded.logs_path);
        let failed_log = PathBuf::from(&failed.logs_path);
        std::fs::create_dir_all(paths.job_logs_dir()).expect("job logs dir");
        std::fs::write(&succeeded_log, "ok").expect("write succeeded log");
        std::fs::write(path_with_suffix(&succeeded_log, ".1"), "ok-backup")
            .expect("write succeeded backup");
        std::fs::write(&failed_log, "failed").expect("write failed log");

        let succeeded_artifacts = paths.job_artifacts_dir(&succeeded.id);
        let failed_artifacts = paths.job_artifacts_dir(&failed.id);
        std::fs::create_dir_all(&succeeded_artifacts).expect("succeeded artifacts");
        std::fs::create_dir_all(&failed_artifacts).expect("failed artifacts");
        std::fs::write(succeeded_artifacts.join("a.txt"), "a").expect("artifact file");
        std::fs::write(failed_artifacts.join("b.txt"), "b").expect("artifact file");

        std::fs::create_dir_all(paths.cache_dir()).expect("cache dir");
        std::fs::write(paths.cache_dir().join("tmp.bin"), "x").expect("cache file");
        std::fs::create_dir_all(paths.cache_dir().join("tmpdir")).expect("cache subdir");

        let summary = flush_jobs_cache(&paths).expect("flush");
        assert_eq!(summary.removed_jobs, 2);
        assert!(summary.removed_log_files >= 2);
        assert_eq!(summary.removed_artifact_dirs, 2);
        assert_eq!(summary.removed_output_dirs, 0);
        assert!(summary.removed_cache_entries >= 2);

        let remaining = list_jobs(&paths, 20, 0).expect("list");
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, queued.id);
        assert_eq!(remaining[0].status.as_str(), JobStatus::Queued.as_str());
        assert!(!succeeded_log.exists());
        assert!(!failed_log.exists());
        assert!(!succeeded_artifacts.exists());
        assert!(!failed_artifacts.exists());
    }

    #[test]
    fn flush_jobs_cache_removes_image_batch_output_subdir() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        db::ensure_schema(&paths).expect("schema");

        let downloads = dir.path().join("downloads");
        std::fs::create_dir_all(&downloads).expect("downloads dir");
        paths
            .set_download_dir_override(&downloads)
            .expect("set download override");

        let job = enqueue_download_image_batch(
            &paths,
            vec!["https://example.com/forum".to_string()],
            Some(2),
            Some(0),
            Some(false),
            Some(false),
            vec![],
            Some("wipe_me".to_string()),
            None,
            None,
        )
        .expect("enqueue image batch");

        let conn = db::open(&paths).expect("open");
        db::migrate(&conn).expect("migrate");
        conn.execute(
            "UPDATE job SET status=?1, finished_at_ms=?2, error=?3 WHERE id=?4",
            params![JobStatus::Failed.as_str(), now_ms(), "forced", &job.id],
        )
        .expect("mark failed");

        let output_dir = downloads.join("wipe_me");
        std::fs::create_dir_all(&output_dir).expect("output dir");
        std::fs::write(output_dir.join("thumb.jpg"), "x").expect("output file");

        let summary = flush_jobs_cache(&paths).expect("flush");
        assert_eq!(summary.removed_jobs, 1);
        assert_eq!(summary.removed_output_dirs, 1);
        assert!(!output_dir.exists());
    }

    #[test]
    fn enqueue_download_image_batch_creates_expected_job() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        db::ensure_schema(&paths).expect("schema");

        let job = enqueue_download_image_batch(
            &paths,
            vec!["https://example.com/blog".to_string()],
            Some(25),
            Some(100),
            Some(false),
            Some(true),
            vec!["avatar".to_string()],
            Some("dad-images".to_string()),
            None,
            Some("session=abc123".to_string()),
        )
        .expect("enqueue image batch");
        assert_eq!(job.job_type, "download_image_batch");

        let conn = db::open(&paths).expect("open");
        db::migrate(&conn).expect("migrate");
        let params_json: String = conn
            .query_row("SELECT params_json FROM job WHERE id=?1", [job.id.as_str()], |row| {
                row.get(0)
            })
            .expect("params");
        let params: DownloadImageBatchParams =
            serde_json::from_str(&params_json).expect("parse params");
        assert_eq!(params.max_pages, 25);
        assert_eq!(params.delay_ms, 100);
        assert_eq!(params.output_subdir, "dad-images");
        assert_eq!(params.auth_cookie.as_deref(), None);
        assert_eq!(params.start_urls.len(), 1);
        assert!(!params_json.contains("session=abc123"));

        let cookie_path = paths.job_cookie_secret_path(&job.id);
        assert!(cookie_path.exists(), "cookie secret should exist on disk");
        let stored = std::fs::read_to_string(cookie_path).expect("read cookie secret");
        assert_eq!(stored.trim(), "session=abc123");
    }

    #[test]
    fn enqueue_download_instagram_batch_preserves_direct_provider_for_media_targets() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        db::ensure_schema(&paths).expect("schema");

        let jobs = enqueue_download_instagram_batch(
            &paths,
            vec!["https://www.instagram.com/stories/sample.mp4".to_string()],
            None,
            None,
            None,
        )
        .expect("enqueue instagram batch");
        assert_eq!(jobs.len(), 1);

        let conn = db::open(&paths).expect("open");
        db::migrate(&conn).expect("migrate");
        let params_json: String = conn
            .query_row("SELECT params_json FROM job WHERE id=?1", [jobs[0].id.clone()], |row| {
                row.get(0)
            })
            .expect("params");
        let params: DownloadDirectUrlParams =
            serde_json::from_str(&params_json).expect("parse params");

        assert_eq!(params.provider, DOWNLOAD_PROVIDER_DIRECT_HTTP);
        assert!(!params.use_browser_cookies);
    }

    #[test]
    fn default_direct_job_output_dir_routes_instagram_cdn_to_instagram_folder() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());

        let downloads_root = dir.path().join("downloads");
        std::fs::create_dir_all(&downloads_root).expect("downloads root");
        paths
            .set_download_dir_override(&downloads_root)
            .expect("set override");

        let out = default_direct_job_output_dir(
            &paths,
            DOWNLOAD_PROVIDER_DIRECT_HTTP,
            "https://scontent-bru2-1.cdninstagram.com/v/t51.2885-15/sample.jpg",
            "12345678-abcd",
        )
        .expect("default output dir");

        let out_path = PathBuf::from(out);
        assert!(out_path.starts_with(downloads_root.join(DEFAULT_INSTAGRAM_OUTPUT_SUBDIR)));
    }

    #[test]
    fn suggested_download_filename_has_suffix_and_extension() {
        let name = suggested_download_filename("https://example.com/video", "12345678-abcd");
        assert!(name.starts_with("video_12345678."));
        assert!(name.ends_with(".mp4"));
    }

    #[test]
    fn non_media_response_detection_flags_html_and_json() {
        assert!(is_non_media_response(
            "text/html; charset=utf-8",
            b"<!doctype html><html></html>"
        ));
        assert!(is_non_media_response(
            "application/json",
            br#"{"error":"forbidden"}"#
        ));
    }

    #[test]
    fn non_media_response_detection_allows_video_and_audio_content_types() {
        assert!(!is_non_media_response("video/mp4", b"xxxx"));
        assert!(!is_non_media_response("audio/mpeg", b"ID3...."));
    }

    #[test]
    fn parse_cookie_header_pairs_parses_valid_entries() {
        let pairs = parse_cookie_header_pairs("sessionid=abc123; csrftoken=xyz; bad");
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0].0, "sessionid");
        assert_eq!(pairs[0].1, "abc123");
        assert_eq!(pairs[1].0, "csrftoken");
        assert_eq!(pairs[1].1, "xyz");
    }

    #[test]
    fn strip_browser_cookie_args_removes_flag_and_value() {
        let mut args = vec![
            "--no-warnings".to_string(),
            "--cookies-from-browser".to_string(),
            "chrome".to_string(),
            "https://example.com".to_string(),
        ];
        assert!(strip_browser_cookie_args(&mut args));
        assert!(!args.iter().any(|value| value == "--cookies-from-browser"));
        assert!(!args.iter().any(|value| value == "chrome"));
    }

    #[test]
    fn extract_instagram_item_media_urls_supports_photo_posts() {
        let item = serde_json::json!({
            "media_type": 1,
            "image_versions2": {
                "candidates": [
                    {"url": "https://cdn.example.com/img_small.jpg", "width": 320, "height": 320},
                    {"url": "https://cdn.example.com/img_large.jpg", "width": 1080, "height": 1080}
                ]
            }
        });
        let out = extract_instagram_item_media_urls(&item);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0], "https://cdn.example.com/img_large.jpg");
    }

    #[test]
    fn extract_instagram_item_media_urls_supports_carousel_posts() {
        let item = serde_json::json!({
            "media_type": 8,
            "carousel_media": [
                {
                    "media_type": 1,
                    "image_versions2": {
                        "candidates": [
                            {"url": "https://cdn.example.com/car_a.jpg", "width": 800, "height": 600}
                        ]
                    }
                },
                {
                    "media_type": 2,
                    "video_versions": [
                        {"url": "https://cdn.example.com/car_b.mp4", "width": 720, "height": 1280}
                    ]
                }
            ]
        });
        let out = extract_instagram_item_media_urls(&item);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0], "https://cdn.example.com/car_a.jpg");
        assert_eq!(out[1], "https://cdn.example.com/car_b.mp4");
    }

    #[test]
    fn instagram_shortcode_to_media_id_decodes_known_value() {
        let media_id = instagram_shortcode_to_media_id("Cx4Qd9vIBTh").expect("decode");
        assert_eq!(media_id, "3204383562771993825");
    }

    #[test]
    fn instagram_shortcode_from_url_extracts_post_codes() {
        let code =
            instagram_shortcode_from_url("https://www.instagram.com/p/Cx4Qd9vIBTh/?utm_source=test")
                .expect("code");
        assert_eq!(code, "Cx4Qd9vIBTh");
    }
}
