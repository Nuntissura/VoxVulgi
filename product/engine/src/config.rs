use crate::paths::AppPaths;
use crate::{persistence, EngineError, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

const DEFAULT_YT_DLP_CONCURRENT_FRAGMENTS: u32 = 4;
const DEFAULT_YT_DLP_FRAGMENT_RETRIES: u32 = 3;
const DEFAULT_YT_DLP_RETRIES: u32 = 3;
const DEFAULT_YT_DLP_FILE_ACCESS_RETRIES: u32 = 10;
const DEFAULT_YT_DLP_THROTTLED_RATE: &str = "100K";
const DEFAULT_YT_DLP_SLEEP_INTERVAL_SECS: u32 = 0;
const DEFAULT_YT_DLP_SLEEP_REQUESTS: u32 = 0;
const DEFAULT_DOWNLOAD_PATH_TEMPLATE: &str = "{channel}";
const DEFAULT_DOWNLOAD_FORMAT_PREFERENCE: &str = "bv*+ba/b";
const LEGACY_MP4_DOWNLOAD_FORMAT_PREFERENCE: &str = "bv*[ext=mp4]+ba[ext=m4a]/b[ext=mp4]/bv*+ba/b";
static DOWNLOAD_PRESETS_CONFIG_WRITE_LOCK: Mutex<()> = Mutex::new(());
static FEATURE_STORAGE_ROOTS_WRITE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchOnImportRules {
    pub auto_asr: bool,
    pub auto_translate: bool,
    pub auto_separate: bool,
    pub auto_diarize: bool,
    pub auto_dub_preview: bool,
}

impl Default for BatchOnImportRules {
    fn default() -> Self {
        Self {
            auto_asr: false,
            auto_translate: false,
            auto_separate: false,
            auto_diarize: false,
            auto_dub_preview: false,
        }
    }
}

pub fn load_batch_on_import_rules(paths: &AppPaths) -> Result<BatchOnImportRules> {
    let path = paths.batch_on_import_rules_path();
    if !path.exists() {
        return Ok(BatchOnImportRules::default());
    }
    let bytes = std::fs::read(&path)?;
    let parsed: BatchOnImportRules = serde_json::from_slice(&bytes).map_err(|e| {
        EngineError::InstallFailed(format!(
            "failed to parse batch_on_import_rules at {}: {e}",
            path.to_string_lossy()
        ))
    })?;
    Ok(parsed)
}

pub fn save_batch_on_import_rules(paths: &AppPaths, rules: &BatchOnImportRules) -> Result<()> {
    let path = paths.batch_on_import_rules_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(rules)?;
    let text = format!("{json}\n");
    persistence::atomic_write_text(&path, &text)?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafeModeConfig {
    pub enabled: bool,
}

impl Default for SafeModeConfig {
    fn default() -> Self {
        Self { enabled: false }
    }
}

pub fn load_safe_mode_config(paths: &AppPaths) -> Result<SafeModeConfig> {
    let path = paths.safe_mode_config_path();
    if !path.exists() {
        return Ok(SafeModeConfig::default());
    }

    let bytes = std::fs::read(&path)?;
    let parsed: SafeModeConfig = serde_json::from_slice(&bytes).map_err(|e| {
        EngineError::InstallFailed(format!(
            "failed to parse safe mode config at {}: {e}",
            path.to_string_lossy()
        ))
    })?;
    Ok(parsed)
}

pub fn save_safe_mode_config(paths: &AppPaths, config: &SafeModeConfig) -> Result<()> {
    let path = paths.safe_mode_config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(config)?;
    let text = format!("{json}\n");
    persistence::atomic_write_text(&path, &text)?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct FeatureStorageRootsConfig {
    #[serde(default)]
    pub video_root: Option<String>,
    #[serde(default)]
    pub instagram_root: Option<String>,
    #[serde(default)]
    pub image_root: Option<String>,
    #[serde(default)]
    pub localization_root: Option<String>,
}

pub fn load_feature_storage_roots_config(paths: &AppPaths) -> Result<FeatureStorageRootsConfig> {
    let path = paths.feature_storage_roots_config_path();
    if !path.exists() {
        return Ok(FeatureStorageRootsConfig::default());
    }
    let bytes = std::fs::read(&path)?;
    let parsed: FeatureStorageRootsConfig = serde_json::from_slice(&bytes).map_err(|e| {
        EngineError::InstallFailed(format!(
            "failed to parse feature storage roots at {}: {e}",
            path.to_string_lossy()
        ))
    })?;
    Ok(normalize_feature_storage_roots_config(parsed))
}

pub fn save_feature_storage_roots_config(
    paths: &AppPaths,
    config: &FeatureStorageRootsConfig,
) -> Result<()> {
    let _guard = FEATURE_STORAGE_ROOTS_WRITE_LOCK.lock().map_err(|_| {
        EngineError::InstallFailed("feature storage roots writer lock is poisoned".to_string())
    })?;
    save_feature_storage_roots_config_unlocked(paths, config)
}

fn save_feature_storage_roots_config_unlocked(
    paths: &AppPaths,
    config: &FeatureStorageRootsConfig,
) -> Result<()> {
    let normalized = normalize_feature_storage_roots_config(config.clone());
    let path = paths.feature_storage_roots_config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(&normalized)?;
    let text = format!("{json}\n");
    persistence::atomic_write_text(&path, &text)?;
    Ok(())
}

pub fn update_feature_storage_roots_config<F>(
    paths: &AppPaths,
    update: F,
) -> Result<FeatureStorageRootsConfig>
where
    F: FnOnce(&mut FeatureStorageRootsConfig) -> Result<()>,
{
    let _guard = FEATURE_STORAGE_ROOTS_WRITE_LOCK.lock().map_err(|_| {
        EngineError::InstallFailed("feature storage roots writer lock is poisoned".to_string())
    })?;
    let mut current = load_feature_storage_roots_config(paths)?;
    update(&mut current)?;
    current = normalize_feature_storage_roots_config(current);
    save_feature_storage_roots_config_unlocked(paths, &current)?;
    let persisted = load_feature_storage_roots_config(paths)?;
    if persisted != current {
        return Err(EngineError::InstallFailed(
            "feature storage roots write readback did not match".to_string(),
        ));
    }
    Ok(persisted)
}

pub fn compare_exchange_feature_storage_video_root(
    paths: &AppPaths,
    expected: &str,
    replacement: &str,
) -> Result<FeatureStorageRootsConfig> {
    update_feature_storage_roots_config(paths, |current| match current.video_root.as_deref() {
        Some(value) if value == replacement => Ok(()),
        Some(value) if value == expected => {
            current.video_root = Some(replacement.to_string());
            Ok(())
        }
        _ => Err(EngineError::InstallFailed(
            "root rebind refused to overwrite changed feature video_root".to_string(),
        )),
    })
}

pub fn with_feature_storage_roots_config_lock<T, F>(paths: &AppPaths, inspect: F) -> Result<T>
where
    F: FnOnce(&FeatureStorageRootsConfig) -> Result<T>,
{
    let _guard = FEATURE_STORAGE_ROOTS_WRITE_LOCK.lock().map_err(|_| {
        EngineError::InstallFailed("feature storage roots writer lock is poisoned".to_string())
    })?;
    let current = load_feature_storage_roots_config(paths)?;
    inspect(&current)
}

fn normalize_feature_storage_roots_config(
    mut config: FeatureStorageRootsConfig,
) -> FeatureStorageRootsConfig {
    config.video_root = normalize_optional_path(config.video_root);
    config.instagram_root = normalize_optional_path(config.instagram_root);
    config.image_root = normalize_optional_path(config.image_root);
    config.localization_root = normalize_optional_path(config.localization_root);
    config
}

fn normalize_optional_path(value: Option<String>) -> Option<String> {
    value
        .map(|raw| raw.trim().to_string())
        .filter(|raw| !raw.is_empty())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadPreset {
    pub id: String,
    pub title: String,
    pub path_template: String,
    pub filename_template: String,
    pub format_preference: Option<String>,
    pub quality_preference: Option<String>,
    pub subtitle_mode: Option<String>,
    #[serde(default)]
    pub yt_dlp_concurrent_fragments: u32,
    #[serde(default)]
    pub yt_dlp_limit_rate: Option<String>,
    #[serde(default)]
    pub yt_dlp_throttled_rate: Option<String>,
    #[serde(default)]
    pub yt_dlp_file_access_retries: u32,
    #[serde(default)]
    pub yt_dlp_retries: u32,
    #[serde(default)]
    pub yt_dlp_fragment_retries: u32,
    #[serde(default)]
    pub yt_dlp_sleep_interval: u32,
    #[serde(default)]
    pub yt_dlp_sleep_requests: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadPresetsConfig {
    pub default_preset_id: Option<String>,
    pub presets: Vec<DownloadPreset>,
}

impl Default for DownloadPresetsConfig {
    fn default() -> Self {
        let preset = DownloadPreset {
            id: "default".to_string(),
            title: "Default".to_string(),
            path_template: DEFAULT_DOWNLOAD_PATH_TEMPLATE.to_string(),
            filename_template: "{title}_{id}".to_string(),
            // The source selector is deliberately container-neutral. The engine execution
            // boundary owns the final managed-video container and always finalizes as MKV.
            format_preference: Some(DEFAULT_DOWNLOAD_FORMAT_PREFERENCE.to_string()),
            quality_preference: Some("best".to_string()),
            subtitle_mode: Some("auto".to_string()),
            yt_dlp_concurrent_fragments: DEFAULT_YT_DLP_CONCURRENT_FRAGMENTS,
            yt_dlp_limit_rate: None,
            yt_dlp_throttled_rate: Some(DEFAULT_YT_DLP_THROTTLED_RATE.to_string()),
            yt_dlp_file_access_retries: DEFAULT_YT_DLP_FILE_ACCESS_RETRIES,
            yt_dlp_retries: DEFAULT_YT_DLP_RETRIES,
            yt_dlp_fragment_retries: DEFAULT_YT_DLP_FRAGMENT_RETRIES,
            yt_dlp_sleep_interval: DEFAULT_YT_DLP_SLEEP_INTERVAL_SECS,
            yt_dlp_sleep_requests: DEFAULT_YT_DLP_SLEEP_REQUESTS,
        };
        Self {
            default_preset_id: Some(preset.id.clone()),
            presets: vec![preset],
        }
    }
}

fn load_download_presets_config_unlocked(paths: &AppPaths) -> Result<DownloadPresetsConfig> {
    let path = paths.download_presets_config_path();
    if !path.exists() {
        return Ok(DownloadPresetsConfig::default());
    }
    let bytes = std::fs::read(&path)?;
    let parsed: DownloadPresetsConfig = serde_json::from_slice(&bytes).map_err(|e| {
        EngineError::InstallFailed(format!(
            "failed to parse download presets config at {}: {e}",
            path.to_string_lossy()
        ))
    })?;
    normalize_download_presets_config(parsed)
}

pub fn load_download_presets_config(paths: &AppPaths) -> Result<DownloadPresetsConfig> {
    load_download_presets_config_unlocked(paths)
}

fn save_download_presets_config_unlocked(
    paths: &AppPaths,
    config: &DownloadPresetsConfig,
) -> Result<()> {
    let normalized = normalize_download_presets_config(config.clone())?;
    let path = paths.download_presets_config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(&normalized)?;
    let text = format!("{json}\n");
    persistence::atomic_write_text(&path, &text)?;
    Ok(())
}

pub fn save_download_presets_config(
    paths: &AppPaths,
    config: &DownloadPresetsConfig,
) -> Result<()> {
    let _guard = DOWNLOAD_PRESETS_CONFIG_WRITE_LOCK.lock().map_err(|_| {
        EngineError::InstallFailed("download preset writer lock is poisoned".to_string())
    })?;
    save_download_presets_config_unlocked(paths, config)
}

/// Serialize read/merge/write operations across the Options field writer and the Library catalog
/// writer. The config file's atomic replace protects readers from partial JSON; this lock protects
/// writers from replacing changes made after their own stale frontend snapshot was loaded.
pub fn update_download_presets_config<F>(
    paths: &AppPaths,
    update: F,
) -> Result<DownloadPresetsConfig>
where
    F: FnOnce(DownloadPresetsConfig) -> Result<DownloadPresetsConfig>,
{
    let _guard = DOWNLOAD_PRESETS_CONFIG_WRITE_LOCK.lock().map_err(|_| {
        EngineError::InstallFailed("download preset writer lock is poisoned".to_string())
    })?;
    let current = load_download_presets_config_unlocked(paths)?;
    let next = update(current)?;
    save_download_presets_config_unlocked(paths, &next)?;
    load_download_presets_config_unlocked(paths)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DownloadPresetSafetyPatch {
    pub yt_dlp_concurrent_fragments: u32,
    pub yt_dlp_limit_rate: Option<String>,
    pub yt_dlp_throttled_rate: Option<String>,
    pub yt_dlp_file_access_retries: u32,
    pub yt_dlp_retries: u32,
    pub yt_dlp_fragment_retries: u32,
    pub yt_dlp_sleep_interval: u32,
    pub yt_dlp_sleep_requests: u32,
}

/// Patch only the eight Options-owned safety fields on the authoritative current catalog.
/// `expected_default_preset_id` is a CAS token: unrelated catalog additions are preserved, while
/// a concurrent default switch/delete is rejected instead of applying stale fields to a new row.
pub fn patch_default_download_preset_safety_fields(
    paths: &AppPaths,
    expected_default_preset_id: &str,
    patch: &DownloadPresetSafetyPatch,
) -> Result<DownloadPresetsConfig> {
    let expected_default_preset_id = expected_default_preset_id.trim().to_string();
    if expected_default_preset_id.is_empty() {
        return Err(EngineError::InstallFailed(
            "download preset patch requires the expected default preset id".to_string(),
        ));
    }
    update_download_presets_config(paths, |mut current| {
        let current_default_id = current
            .default_preset_id
            .clone()
            .filter(|id| current.presets.iter().any(|preset| preset.id == *id))
            .or_else(|| current.presets.first().map(|preset| preset.id.clone()))
            .ok_or_else(|| {
                EngineError::InstallFailed(
                    "the authoritative download preset catalog has no default preset".to_string(),
                )
            })?;
        if current_default_id != expected_default_preset_id {
            return Err(EngineError::InstallFailed(format!(
                "download preset catalog changed concurrently (expected default {expected_default_preset_id}, found {current_default_id}); reload settings before saving"
            )));
        }
        let preset = current
            .presets
            .iter_mut()
            .find(|preset| preset.id == current_default_id)
            .ok_or_else(|| {
                EngineError::InstallFailed(
                    "the authoritative default download preset is unavailable".to_string(),
                )
            })?;
        preset.yt_dlp_concurrent_fragments = patch.yt_dlp_concurrent_fragments;
        preset.yt_dlp_limit_rate = patch.yt_dlp_limit_rate.clone();
        preset.yt_dlp_throttled_rate = patch.yt_dlp_throttled_rate.clone();
        preset.yt_dlp_file_access_retries = patch.yt_dlp_file_access_retries;
        preset.yt_dlp_retries = patch.yt_dlp_retries;
        preset.yt_dlp_fragment_retries = patch.yt_dlp_fragment_retries;
        preset.yt_dlp_sleep_interval = patch.yt_dlp_sleep_interval;
        preset.yt_dlp_sleep_requests = patch.yt_dlp_sleep_requests;
        Ok(current)
    })
}

fn normalize_download_presets_config(
    mut config: DownloadPresetsConfig,
) -> Result<DownloadPresetsConfig> {
    let mut cleaned: Vec<DownloadPreset> = Vec::new();
    for preset in config.presets.into_iter() {
        let id = preset.id.trim();
        let title = preset.title.trim();
        if id.is_empty() || title.is_empty() {
            continue;
        }
        let path_template = preset.path_template.trim();
        let path_template = if path_template.is_empty()
            || (id == "default" && title == "Default" && path_template == "{provider}/{channel}")
        {
            DEFAULT_DOWNLOAD_PATH_TEMPLATE.to_string()
        } else {
            path_template.to_string()
        };
        let filename_template = preset.filename_template.trim();
        let format_preference = preset
            .format_preference
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        // Migrate only the exact built-in legacy default. Custom selectors remain operator
        // choices, but none of them can bypass the MKV execution boundary in jobs.rs.
        let format_preference = if id == "default"
            && title == "Default"
            && format_preference.as_deref() == Some(LEGACY_MP4_DOWNLOAD_FORMAT_PREFERENCE)
        {
            Some(DEFAULT_DOWNLOAD_FORMAT_PREFERENCE.to_string())
        } else {
            format_preference
        };
        cleaned.push(DownloadPreset {
            id: id.to_string(),
            title: title.to_string(),
            path_template,
            filename_template: if filename_template.is_empty() {
                "{title}_{id}".to_string()
            } else {
                filename_template.to_string()
            },
            format_preference,
            quality_preference: preset
                .quality_preference
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty()),
            subtitle_mode: preset
                .subtitle_mode
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty()),
            yt_dlp_concurrent_fragments: normalize_positive_u32_with_fallback(
                preset.yt_dlp_concurrent_fragments,
                DEFAULT_YT_DLP_CONCURRENT_FRAGMENTS,
            ),
            yt_dlp_limit_rate: preset
                .yt_dlp_limit_rate
                .map(|raw| raw.trim().to_string())
                .filter(|raw| !raw.is_empty()),
            yt_dlp_throttled_rate: normalize_presets_text_value(
                preset.yt_dlp_throttled_rate,
                DEFAULT_YT_DLP_THROTTLED_RATE,
            ),
            yt_dlp_file_access_retries: normalize_positive_u32_with_fallback(
                preset.yt_dlp_file_access_retries,
                DEFAULT_YT_DLP_FILE_ACCESS_RETRIES,
            ),
            yt_dlp_retries: normalize_positive_u32_with_fallback(
                preset.yt_dlp_retries,
                DEFAULT_YT_DLP_RETRIES,
            ),
            yt_dlp_fragment_retries: normalize_positive_u32_with_fallback(
                preset.yt_dlp_fragment_retries,
                DEFAULT_YT_DLP_FRAGMENT_RETRIES,
            ),
            yt_dlp_sleep_interval: normalize_positive_u32_with_fallback(
                preset.yt_dlp_sleep_interval,
                DEFAULT_YT_DLP_SLEEP_INTERVAL_SECS,
            ),
            yt_dlp_sleep_requests: normalize_positive_u32_with_fallback(
                preset.yt_dlp_sleep_requests,
                DEFAULT_YT_DLP_SLEEP_REQUESTS,
            ),
        });
    }
    if cleaned.is_empty() {
        return Ok(DownloadPresetsConfig::default());
    }

    let default_id = config
        .default_preset_id
        .as_deref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .filter(|id| cleaned.iter().any(|preset| preset.id == *id))
        .or_else(|| cleaned.first().map(|preset| preset.id.clone()));

    config.presets = cleaned;
    config.default_preset_id = default_id;
    Ok(config)
}

fn normalize_positive_u32_with_fallback(value: u32, fallback: u32) -> u32 {
    if value == 0 {
        fallback
    } else {
        value
    }
}

fn normalize_presets_text_value(value: Option<String>, fallback: &str) -> Option<String> {
    value
        .map(|raw| raw.trim().to_string())
        .filter(|raw| !raw.is_empty())
        .or_else(|| Some(fallback.to_string()))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionalDiarizationBackendConfig {
    pub enabled: bool,
    /// Backend id (e.g. "baseline", "pyannote_byo_v1").
    pub backend: String,
    /// Optional python executable path for BYO backends.
    pub python_exe: Option<String>,
    /// Optional model id / repo id for the backend (if applicable).
    pub model_id: Option<String>,
    /// Optional local model path for the backend (if applicable).
    pub local_model_path: Option<String>,
}

impl Default for OptionalDiarizationBackendConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            backend: "baseline".to_string(),
            python_exe: None,
            model_id: None,
            local_model_path: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct OptionalDiarizationBackendStatus {
    pub config: OptionalDiarizationBackendConfig,
    pub token_present: bool,
    pub token_path: String,
    pub config_path: String,
}

pub fn load_optional_diarization_backend_status(
    paths: &AppPaths,
) -> Result<OptionalDiarizationBackendStatus> {
    let config_path = paths.diarization_optional_backend_config_path();
    let token_path = paths.diarization_optional_backend_token_path();

    let mut config = OptionalDiarizationBackendConfig::default();
    if config_path.exists() {
        let bytes = std::fs::read(&config_path)?;
        config = serde_json::from_slice(&bytes).map_err(|e| {
            EngineError::InstallFailed(format!(
                "failed to parse diarization optional backend config at {}: {e}",
                config_path.to_string_lossy()
            ))
        })?;
    }

    Ok(OptionalDiarizationBackendStatus {
        config,
        token_present: token_path.exists() && token_path.is_file(),
        token_path: token_path.to_string_lossy().to_string(),
        config_path: config_path.to_string_lossy().to_string(),
    })
}

pub fn save_optional_diarization_backend_config(
    paths: &AppPaths,
    config: &OptionalDiarizationBackendConfig,
    token: Option<&str>,
) -> Result<()> {
    let config_path = paths.diarization_optional_backend_config_path();
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(config)?;
    let text = format!("{json}\n");
    persistence::atomic_write_text(&config_path, &text)?;

    if let Some(token) = token {
        write_secret_token(&paths.diarization_optional_backend_token_path(), token)?;
    }

    Ok(())
}

pub fn clear_optional_diarization_backend_token(paths: &AppPaths) -> Result<()> {
    let token_path = paths.diarization_optional_backend_token_path();
    if token_path.exists() {
        std::fs::remove_file(token_path)?;
    }
    Ok(())
}

pub fn read_optional_diarization_backend_token(paths: &AppPaths) -> Result<Option<String>> {
    let token_path = paths.diarization_optional_backend_token_path();
    if !token_path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(token_path)?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    Ok(Some(trimmed.to_string()))
}

fn write_secret_token(path: &Path, token: &str) -> Result<()> {
    let token = token.trim();
    if token.is_empty() {
        return Err(EngineError::InstallFailed("token is empty".to_string()));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = format!("{token}\n");
    persistence::atomic_write_text(path, &text)?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct YoutubeAuthConfig {
    #[serde(default)]
    pub netscape_cookie_json: Option<String>,
    #[serde(default)]
    pub browser_cookie_source: Option<String>,
    #[serde(default)]
    pub last_verified_at_ms: Option<i64>,
    #[serde(default)]
    pub reconnect_required_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct YoutubeAuthDiskConfig {
    #[serde(default)]
    netscape_cookie_json: Option<String>,
    #[serde(default)]
    browser_cookie_source: Option<String>,
    #[serde(default)]
    last_verified_at_ms: Option<i64>,
    #[serde(default)]
    reconnect_required_at_ms: Option<i64>,
    #[serde(default)]
    credential_generation: u64,
    #[serde(default)]
    credential_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YoutubeAuthRevision {
    pub credential_generation: u64,
    pub credential_fingerprint: String,
}

static YOUTUBE_AUTH_WRITER_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

thread_local! {
    // Auth consumers are synchronous on one worker thread. Remember the revision observed when
    // credentials were resolved so a later network result cannot mark a replacement credential
    // verified/rejected merely because it finished after that replacement was committed.
    static OBSERVED_YOUTUBE_AUTH_REVISIONS: RefCell<HashMap<PathBuf, YoutubeAuthRevision>> =
        RefCell::new(HashMap::new());
}

fn youtube_auth_writer_lock() -> &'static Mutex<()> {
    YOUTUBE_AUTH_WRITER_LOCK.get_or_init(|| Mutex::new(()))
}

fn youtube_auth_credential_fingerprint(config: &YoutubeAuthConfig) -> String {
    let mut hasher = Sha256::new();
    match (
        config
            .netscape_cookie_json
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty()),
        config
            .browser_cookie_source
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty()),
    ) {
        (Some(cookie), _) => {
            hasher.update(b"manual-cookie\0");
            hasher.update(cookie.as_bytes());
        }
        (None, Some(browser)) => {
            hasher.update(b"browser-cookie\0");
            hasher.update(browser.as_bytes());
        }
        (None, None) => hasher.update(b"disconnected\0"),
    }
    hex::encode(hasher.finalize())
}

fn youtube_auth_disk_from_parts(
    config: YoutubeAuthConfig,
    credential_generation: u64,
) -> YoutubeAuthDiskConfig {
    let credential_fingerprint = youtube_auth_credential_fingerprint(&config);
    YoutubeAuthDiskConfig {
        netscape_cookie_json: config.netscape_cookie_json,
        browser_cookie_source: config.browser_cookie_source,
        last_verified_at_ms: config.last_verified_at_ms,
        reconnect_required_at_ms: config.reconnect_required_at_ms,
        credential_generation,
        credential_fingerprint,
    }
}

fn load_youtube_auth_disk_unlocked(paths: &AppPaths) -> Result<YoutubeAuthDiskConfig> {
    let path = paths.youtube_auth_config_path();
    if !path.exists() {
        return Ok(youtube_auth_disk_from_parts(
            YoutubeAuthConfig::default(),
            0,
        ));
    }
    let bytes = std::fs::read(&path)?;
    let mut parsed: YoutubeAuthDiskConfig = serde_json::from_slice(&bytes).map_err(|e| {
        EngineError::InstallFailed(format!(
            "failed to parse youtube auth config at {}: {e}",
            path.to_string_lossy()
        ))
    })?;
    let config = YoutubeAuthConfig {
        netscape_cookie_json: parsed.netscape_cookie_json.clone(),
        browser_cookie_source: parsed.browser_cookie_source.clone(),
        last_verified_at_ms: parsed.last_verified_at_ms,
        reconnect_required_at_ms: parsed.reconnect_required_at_ms,
    };
    parsed.credential_fingerprint = youtube_auth_credential_fingerprint(&config);
    Ok(parsed)
}

fn youtube_auth_config_from_disk(disk: &YoutubeAuthDiskConfig) -> YoutubeAuthConfig {
    YoutubeAuthConfig {
        netscape_cookie_json: disk.netscape_cookie_json.clone(),
        browser_cookie_source: disk.browser_cookie_source.clone(),
        last_verified_at_ms: disk.last_verified_at_ms,
        reconnect_required_at_ms: disk.reconnect_required_at_ms,
    }
}

fn save_youtube_auth_disk_unlocked(paths: &AppPaths, disk: &YoutubeAuthDiskConfig) -> Result<()> {
    let path = paths.youtube_auth_config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(disk)?;
    persistence::atomic_write_text(&path, &format!("{json}\n"))?;
    Ok(())
}

pub fn load_youtube_auth_config(paths: &AppPaths) -> Result<YoutubeAuthConfig> {
    let _guard = youtube_auth_writer_lock().lock().map_err(|_| {
        EngineError::InstallFailed("youtube auth writer lock is poisoned".to_string())
    })?;
    let disk = load_youtube_auth_disk_unlocked(paths)?;
    OBSERVED_YOUTUBE_AUTH_REVISIONS.with(|observed| {
        observed.borrow_mut().insert(
            paths.youtube_auth_config_path(),
            YoutubeAuthRevision {
                credential_generation: disk.credential_generation,
                credential_fingerprint: disk.credential_fingerprint.clone(),
            },
        );
    });
    Ok(youtube_auth_config_from_disk(&disk))
}

pub fn save_youtube_auth_config(paths: &AppPaths, config: &YoutubeAuthConfig) -> Result<()> {
    let _guard = youtube_auth_writer_lock().lock().map_err(|_| {
        EngineError::InstallFailed("youtube auth writer lock is poisoned".to_string())
    })?;
    let current = load_youtube_auth_disk_unlocked(paths)?;
    let next = youtube_auth_disk_from_parts(
        config.clone(),
        current.credential_generation.saturating_add(1),
    );
    save_youtube_auth_disk_unlocked(paths, &next)
}

pub fn youtube_auth_revision(paths: &AppPaths) -> Result<YoutubeAuthRevision> {
    let _guard = youtube_auth_writer_lock().lock().map_err(|_| {
        EngineError::InstallFailed("youtube auth writer lock is poisoned".to_string())
    })?;
    let disk = load_youtube_auth_disk_unlocked(paths)?;
    Ok(YoutubeAuthRevision {
        credential_generation: disk.credential_generation,
        credential_fingerprint: disk.credential_fingerprint,
    })
}

pub fn replace_youtube_auth_config(
    paths: &AppPaths,
    mut next: YoutubeAuthConfig,
    expected_generation: Option<u64>,
    expected_fingerprint: Option<&str>,
) -> Result<YoutubeAuthConfig> {
    let _guard = youtube_auth_writer_lock().lock().map_err(|_| {
        EngineError::InstallFailed("youtube auth writer lock is poisoned".to_string())
    })?;
    let current = load_youtube_auth_disk_unlocked(paths)?;
    if expected_generation.is_none() || expected_fingerprint.is_none() {
        return Err(EngineError::InstallFailed(
            "youtube credential revision is unavailable; reload canonical auth status before mutating credentials"
                .to_string(),
        ));
    }
    if expected_generation.is_some_and(|expected| expected != current.credential_generation)
        || expected_fingerprint.is_some_and(|expected| expected != current.credential_fingerprint)
    {
        return Err(EngineError::InstallFailed(
            "youtube credentials changed concurrently; reload the saved credential status before retrying"
                .to_string(),
        ));
    }
    next.last_verified_at_ms = None;
    next.reconnect_required_at_ms = None;
    let next_disk = youtube_auth_disk_from_parts(
        next.clone(),
        current.credential_generation.saturating_add(1),
    );
    save_youtube_auth_disk_unlocked(paths, &next_disk)?;
    Ok(next)
}

fn update_youtube_auth_verification_if_current(
    paths: &AppPaths,
    expected_generation: u64,
    expected_fingerprint: &str,
    checked_at_ms: i64,
    verified: bool,
) -> Result<YoutubeAuthConfig> {
    let _guard = youtube_auth_writer_lock().lock().map_err(|_| {
        EngineError::InstallFailed("youtube auth writer lock is poisoned".to_string())
    })?;
    let mut disk = load_youtube_auth_disk_unlocked(paths)?;
    if disk.credential_generation != expected_generation
        || disk.credential_fingerprint != expected_fingerprint
    {
        return Err(EngineError::InstallFailed(
            "youtube credential verification became stale because the saved credentials changed"
                .to_string(),
        ));
    }
    if disk.netscape_cookie_json.is_none() && disk.browser_cookie_source.is_none() {
        return Ok(youtube_auth_config_from_disk(&disk));
    }
    if verified {
        disk.last_verified_at_ms = Some(checked_at_ms);
        disk.reconnect_required_at_ms = None;
    } else {
        disk.last_verified_at_ms = None;
        disk.reconnect_required_at_ms = Some(checked_at_ms);
    }
    save_youtube_auth_disk_unlocked(paths, &disk)?;
    Ok(youtube_auth_config_from_disk(&disk))
}

pub fn mark_youtube_auth_verified(
    paths: &AppPaths,
    checked_at_ms: i64,
) -> Result<YoutubeAuthConfig> {
    let revision = OBSERVED_YOUTUBE_AUTH_REVISIONS
        .with(|observed| {
            observed
                .borrow()
                .get(&paths.youtube_auth_config_path())
                .cloned()
        })
        .map(Ok)
        .unwrap_or_else(|| youtube_auth_revision(paths))?;
    update_youtube_auth_verification_if_current(
        paths,
        revision.credential_generation,
        &revision.credential_fingerprint,
        checked_at_ms,
        true,
    )
}

pub fn mark_youtube_auth_reconnect_required(
    paths: &AppPaths,
    rejected_at_ms: i64,
) -> Result<YoutubeAuthConfig> {
    let revision = OBSERVED_YOUTUBE_AUTH_REVISIONS
        .with(|observed| {
            observed
                .borrow()
                .get(&paths.youtube_auth_config_path())
                .cloned()
        })
        .map(Ok)
        .unwrap_or_else(|| youtube_auth_revision(paths))?;
    update_youtube_auth_verification_if_current(
        paths,
        revision.credential_generation,
        &revision.credential_fingerprint,
        rejected_at_ms,
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manual_auth(value: &str) -> YoutubeAuthConfig {
        YoutubeAuthConfig {
            netscape_cookie_json: Some(value.to_string()),
            browser_cookie_source: None,
            last_verified_at_ms: None,
            reconnect_required_at_ms: None,
        }
    }

    fn replace_current_youtube_auth(
        paths: &AppPaths,
        next: YoutubeAuthConfig,
    ) -> Result<YoutubeAuthConfig> {
        let revision = youtube_auth_revision(paths)?;
        replace_youtube_auth_config(
            paths,
            next,
            Some(revision.credential_generation),
            Some(&revision.credential_fingerprint),
        )
    }

    #[test]
    fn youtube_auth_requires_a_complete_revision_even_at_generation_zero() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app"));
        paths.ensure_dirs().expect("dirs");
        let initial = youtube_auth_revision(&paths).expect("initial revision");
        assert_eq!(initial.credential_generation, 0);
        assert!(replace_youtube_auth_config(
            &paths,
            manual_auth("SID=missing-revision"),
            None,
            None,
        )
        .is_err());
        assert!(replace_youtube_auth_config(
            &paths,
            manual_auth("SID=partial-revision"),
            Some(initial.credential_generation),
            None,
        )
        .is_err());
        let saved = replace_youtube_auth_config(
            &paths,
            manual_auth("SID=complete-revision"),
            Some(initial.credential_generation),
            Some(&initial.credential_fingerprint),
        )
        .expect("complete generation-zero revision");
        assert_eq!(saved.netscape_cookie_json.as_deref(), Some("SID=complete-revision"));
    }

    #[test]
    fn youtube_auth_revision_cas_rejects_stale_verification_and_replacement() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app"));
        paths.ensure_dirs().expect("dirs");
        replace_current_youtube_auth(&paths, manual_auth("SID=old"))
            .expect("initial auth");
        let stale = youtube_auth_revision(&paths).expect("stale revision");

        let disconnected = replace_youtube_auth_config(
            &paths,
            YoutubeAuthConfig::default(),
            Some(stale.credential_generation),
            Some(&stale.credential_fingerprint),
        )
        .expect("disconnect");
        assert!(disconnected.netscape_cookie_json.is_none());

        assert!(update_youtube_auth_verification_if_current(
            &paths,
            stale.credential_generation,
            &stale.credential_fingerprint,
            123,
            true,
        )
        .is_err());
        assert!(replace_youtube_auth_config(
            &paths,
            manual_auth("SID=stale-resurrection"),
            Some(stale.credential_generation),
            Some(&stale.credential_fingerprint),
        )
        .is_err());
        let current = load_youtube_auth_config(&paths).expect("current auth");
        assert!(current.netscape_cookie_json.is_none());
        assert!(current.browser_cookie_source.is_none());
        assert!(current.last_verified_at_ms.is_none());
    }

    #[test]
    fn youtube_auth_concurrent_replacements_have_one_authoritative_winner() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app"));
        paths.ensure_dirs().expect("dirs");
        replace_current_youtube_auth(&paths, manual_auth("SID=base"))
            .expect("base auth");
        let revision = youtube_auth_revision(&paths).expect("revision");
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let handles: Vec<_> = ["SID=left", "SID=right"]
            .into_iter()
            .map(|cookie| {
                let paths = paths.clone();
                let barrier = barrier.clone();
                let revision = revision.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    replace_youtube_auth_config(
                        &paths,
                        manual_auth(cookie),
                        Some(revision.credential_generation),
                        Some(&revision.credential_fingerprint),
                    )
                })
            })
            .collect();
        barrier.wait();
        let results: Vec<_> = handles
            .into_iter()
            .map(|handle| handle.join().expect("writer thread"))
            .collect();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(results.iter().filter(|result| result.is_err()).count(), 1);
        let current = load_youtube_auth_config(&paths).expect("current");
        assert!(matches!(
            current.netscape_cookie_json.as_deref(),
            Some("SID=left") | Some("SID=right")
        ));
    }

    #[test]
    fn youtube_auth_public_mark_rejects_result_for_replaced_observed_credential() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app"));
        paths.ensure_dirs().expect("dirs");
        replace_current_youtube_auth(&paths, manual_auth("SID=observed"))
            .expect("initial auth");
        load_youtube_auth_config(&paths).expect("operation observes credential");

        let replacement_paths = paths.clone();
        std::thread::spawn(move || {
            replace_current_youtube_auth(&replacement_paths, manual_auth("SID=replacement"))
            .expect("replace credential");
        })
        .join()
        .expect("replacement thread");

        assert!(mark_youtube_auth_verified(&paths, 123).is_err());
        let current = load_youtube_auth_config(&paths).expect("current credential");
        assert_eq!(
            current.netscape_cookie_json.as_deref(),
            Some("SID=replacement")
        );
        assert!(current.last_verified_at_ms.is_none());
    }

    #[test]
    fn default_download_preset_uses_channel_without_provider_layer() {
        let config = DownloadPresetsConfig::default();
        let preset = config.presets.first().expect("default preset");

        assert_eq!(preset.id, "default");
        assert_eq!(preset.path_template, "{channel}");
        assert_eq!(
            preset.format_preference.as_deref(),
            Some(DEFAULT_DOWNLOAD_FORMAT_PREFERENCE)
        );
    }

    #[test]
    fn youtube_auth_verification_state_persists_ready_and_reconnect_required() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        save_youtube_auth_config(
            &paths,
            &YoutubeAuthConfig {
                browser_cookie_source: Some("firefox".to_string()),
                ..Default::default()
            },
        )
        .expect("save auth source");

        let verified = mark_youtube_auth_verified(&paths, 1234).expect("mark verified");
        assert_eq!(verified.last_verified_at_ms, Some(1234));
        assert_eq!(verified.reconnect_required_at_ms, None);

        let rejected = mark_youtube_auth_reconnect_required(&paths, 5678).expect("mark reconnect");
        assert_eq!(rejected.last_verified_at_ms, None);
        assert_eq!(rejected.reconnect_required_at_ms, Some(5678));

        let loaded = load_youtube_auth_config(&paths).expect("reload auth");
        assert_eq!(loaded.browser_cookie_source.as_deref(), Some("firefox"));
        assert_eq!(loaded.last_verified_at_ms, None);
        assert_eq!(loaded.reconnect_required_at_ms, Some(5678));
    }

    #[test]
    fn youtube_auth_verification_state_does_not_create_a_session() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());

        let config = mark_youtube_auth_verified(&paths, 1234).expect("no-op without auth");
        assert_eq!(config.last_verified_at_ms, None);
        assert_eq!(config.reconnect_required_at_ms, None);
        assert!(!paths.youtube_auth_config_path().exists());
    }

    #[test]
    fn normalize_rewrites_legacy_builtin_default_path_template() {
        let mut config = DownloadPresetsConfig::default();
        config.presets[0].path_template = "{provider}/{channel}".to_string();

        let normalized = normalize_download_presets_config(config).expect("normalize");
        let preset = normalized.presets.first().expect("default preset");

        assert_eq!(preset.id, "default");
        assert_eq!(preset.title, "Default");
        assert_eq!(preset.path_template, "{channel}");
    }

    #[test]
    fn normalize_migrates_exact_legacy_builtin_mp4_selector_without_touching_custom_presets() {
        let mut config = DownloadPresetsConfig::default();
        config.presets[0].format_preference =
            Some(LEGACY_MP4_DOWNLOAD_FORMAT_PREFERENCE.to_string());
        config.presets.push(DownloadPreset {
            id: "custom".to_string(),
            title: "Custom".to_string(),
            path_template: "{channel}".to_string(),
            filename_template: "{title}_{id}".to_string(),
            format_preference: Some(LEGACY_MP4_DOWNLOAD_FORMAT_PREFERENCE.to_string()),
            quality_preference: Some("best".to_string()),
            subtitle_mode: Some("auto".to_string()),
            yt_dlp_concurrent_fragments: DEFAULT_YT_DLP_CONCURRENT_FRAGMENTS,
            yt_dlp_limit_rate: None,
            yt_dlp_throttled_rate: Some(DEFAULT_YT_DLP_THROTTLED_RATE.to_string()),
            yt_dlp_file_access_retries: DEFAULT_YT_DLP_FILE_ACCESS_RETRIES,
            yt_dlp_retries: DEFAULT_YT_DLP_RETRIES,
            yt_dlp_fragment_retries: DEFAULT_YT_DLP_FRAGMENT_RETRIES,
            yt_dlp_sleep_interval: DEFAULT_YT_DLP_SLEEP_INTERVAL_SECS,
            yt_dlp_sleep_requests: DEFAULT_YT_DLP_SLEEP_REQUESTS,
        });

        let normalized = normalize_download_presets_config(config).expect("normalize");
        assert_eq!(
            normalized.presets[0].format_preference.as_deref(),
            Some(DEFAULT_DOWNLOAD_FORMAT_PREFERENCE)
        );
        assert_eq!(
            normalized.presets[1].format_preference.as_deref(),
            Some(LEGACY_MP4_DOWNLOAD_FORMAT_PREFERENCE)
        );
    }

    #[test]
    fn options_safety_patch_merges_unrelated_catalog_change_and_rejects_default_switch() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        let mut initial = DownloadPresetsConfig::default();
        let mut alternate = initial.presets[0].clone();
        alternate.id = "alternate".to_string();
        alternate.title = "Alternate".to_string();
        initial.presets.push(alternate);
        save_download_presets_config(&paths, &initial).expect("save initial catalog");

        // Counterexample from the review: Library mutates the catalog after Options loaded its
        // snapshot. The field patch must reread and preserve the newly added catalog row.
        update_download_presets_config(&paths, |mut current| {
            let mut library_added = current.presets[0].clone();
            library_added.id = "library-added".to_string();
            library_added.title = "Library added".to_string();
            library_added.filename_template = "library-{id}".to_string();
            current.presets.push(library_added);
            Ok(current)
        })
        .expect("library catalog mutation");

        let patch = DownloadPresetSafetyPatch {
            yt_dlp_concurrent_fragments: 1,
            yt_dlp_limit_rate: Some("4M".to_string()),
            yt_dlp_throttled_rate: Some("20K".to_string()),
            yt_dlp_file_access_retries: 22,
            yt_dlp_retries: 8,
            yt_dlp_fragment_retries: 8,
            yt_dlp_sleep_interval: 8,
            yt_dlp_sleep_requests: 6,
        };
        let merged = patch_default_download_preset_safety_fields(&paths, "default", &patch)
            .expect("merge safety patch");
        assert!(merged.presets.iter().any(|preset| {
            preset.id == "library-added" && preset.filename_template == "library-{id}"
        }));
        let patched = merged
            .presets
            .iter()
            .find(|preset| preset.id == "default")
            .expect("default preset");
        assert_eq!(patched.yt_dlp_concurrent_fragments, 1);
        assert_eq!(patched.yt_dlp_limit_rate.as_deref(), Some("4M"));
        assert_eq!(patched.yt_dlp_throttled_rate.as_deref(), Some("20K"));

        // A Library default switch changes the row to which the fields would apply. CAS rejects
        // the stale Options action and leaves the authoritative catalog untouched.
        update_download_presets_config(&paths, |mut current| {
            current.default_preset_id = Some("alternate".to_string());
            Ok(current)
        })
        .expect("switch default");
        let before = load_download_presets_config(&paths).expect("before rejected patch");
        let error = patch_default_download_preset_safety_fields(&paths, "default", &patch)
            .expect_err("stale default must be rejected");
        assert!(error.to_string().contains("changed concurrently"));
        let after = load_download_presets_config(&paths).expect("after rejected patch");
        assert_eq!(before.default_preset_id, after.default_preset_id);
        assert_eq!(before.presets.len(), after.presets.len());
    }

    #[test]
    fn concurrent_feature_root_updates_preserve_unrelated_fields() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app"));
        paths.ensure_dirs().expect("dirs");
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let mut handles = Vec::new();
        for field in ["video", "instagram"] {
            let paths = paths.clone();
            let barrier = std::sync::Arc::clone(&barrier);
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                update_feature_storage_roots_config(&paths, |config| {
                    if field == "video" {
                        config.video_root = Some(r"C:\video".to_string());
                    } else {
                        config.instagram_root = Some(r"D:\instagram".to_string());
                    }
                    Ok(())
                })
                .expect("serialized update");
            }));
        }
        barrier.wait();
        for handle in handles {
            handle.join().expect("update thread");
        }
        let stored = load_feature_storage_roots_config(&paths).expect("stored roots");
        assert_eq!(stored.video_root.as_deref(), Some(r"C:\video"));
        assert_eq!(stored.instagram_root.as_deref(), Some(r"D:\instagram"));
    }

    #[test]
    fn feature_video_root_compare_exchange_rejects_stale_expected_value() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app"));
        save_feature_storage_roots_config(
            &paths,
            &FeatureStorageRootsConfig {
                video_root: Some(r"E:\current".to_string()),
                ..FeatureStorageRootsConfig::default()
            },
        )
        .expect("initial roots");
        let error =
            compare_exchange_feature_storage_video_root(&paths, r"C:\stale", r"D:\replacement")
                .expect_err("stale compare-and-swap must have zero side effects");
        assert!(error.to_string().contains("refused to overwrite changed"));
        assert_eq!(
            load_feature_storage_roots_config(&paths)
                .expect("unchanged roots")
                .video_root
                .as_deref(),
            Some(r"E:\current")
        );
    }
}
