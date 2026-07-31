use crate::paths::AppPaths;
use crate::{persistence, EngineError, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

const DEFAULT_YT_DLP_CONCURRENT_FRAGMENTS: u32 = 4;
const DEFAULT_YT_DLP_FRAGMENT_RETRIES: u32 = 3;
const DEFAULT_YT_DLP_RETRIES: u32 = 3;
const DEFAULT_YT_DLP_FILE_ACCESS_RETRIES: u32 = 10;
const DEFAULT_YT_DLP_THROTTLED_RATE: &str = "100K";
const DEFAULT_YT_DLP_SLEEP_INTERVAL_SECS: u32 = 0;
const DEFAULT_YT_DLP_SLEEP_REQUESTS: u32 = 0;
const DEFAULT_DOWNLOAD_PATH_TEMPLATE: &str = "{channel}";

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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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
            format_preference: Some("bv*[ext=mp4]+ba[ext=m4a]/b[ext=mp4]/bv*+ba/b".to_string()),
            quality_preference: Some("best".to_string()),
            subtitle_mode: Some("auto".to_string()),
            yt_dlp_concurrent_fragments: DEFAULT_YT_DLP_CONCURRENT_FRAGMENTS,
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

pub fn load_download_presets_config(paths: &AppPaths) -> Result<DownloadPresetsConfig> {
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

pub fn save_download_presets_config(
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
        cleaned.push(DownloadPreset {
            id: id.to_string(),
            title: title.to_string(),
            path_template,
            filename_template: if filename_template.is_empty() {
                "{title}_{id}".to_string()
            } else {
                filename_template.to_string()
            },
            format_preference: preset
                .format_preference
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty()),
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

pub fn load_youtube_auth_config(paths: &AppPaths) -> Result<YoutubeAuthConfig> {
    let path = paths.youtube_auth_config_path();
    if !path.exists() {
        return Ok(YoutubeAuthConfig::default());
    }
    let bytes = std::fs::read(&path)?;
    let parsed: YoutubeAuthConfig = serde_json::from_slice(&bytes).map_err(|e| {
        EngineError::InstallFailed(format!(
            "failed to parse youtube auth config at {}: {e}",
            path.to_string_lossy()
        ))
    })?;
    Ok(parsed)
}

pub fn save_youtube_auth_config(paths: &AppPaths, config: &YoutubeAuthConfig) -> Result<()> {
    let path = paths.youtube_auth_config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(config)?;
    let text = format!("{json}\n");
    persistence::atomic_write_text(&path, &text)?;
    Ok(())
}

pub fn mark_youtube_auth_verified(
    paths: &AppPaths,
    checked_at_ms: i64,
) -> Result<YoutubeAuthConfig> {
    let mut config = load_youtube_auth_config(paths)?;
    if config.netscape_cookie_json.is_none() && config.browser_cookie_source.is_none() {
        return Ok(config);
    }
    config.last_verified_at_ms = Some(checked_at_ms);
    config.reconnect_required_at_ms = None;
    save_youtube_auth_config(paths, &config)?;
    Ok(config)
}

pub fn mark_youtube_auth_reconnect_required(
    paths: &AppPaths,
    rejected_at_ms: i64,
) -> Result<YoutubeAuthConfig> {
    let mut config = load_youtube_auth_config(paths)?;
    if config.netscape_cookie_json.is_none() && config.browser_cookie_source.is_none() {
        return Ok(config);
    }
    config.last_verified_at_ms = None;
    config.reconnect_required_at_ms = Some(rejected_at_ms);
    save_youtube_auth_config(paths, &config)?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_download_preset_uses_channel_without_provider_layer() {
        let config = DownloadPresetsConfig::default();
        let preset = config.presets.first().expect("default preset");

        assert_eq!(preset.id, "default");
        assert_eq!(preset.path_template, "{channel}");
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
}
