use crate::paths::AppPaths;
use crate::{EngineError, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

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
    std::fs::write(&path, format!("{json}\n"))?;
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
    std::fs::write(&path, format!("{json}\n"))?;
    Ok(())
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
            path_template: "{provider}/{channel}".to_string(),
            filename_template: "{title}_{id}".to_string(),
            format_preference: Some("bestvideo+bestaudio/best".to_string()),
            quality_preference: Some("best".to_string()),
            subtitle_mode: Some("auto".to_string()),
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

pub fn save_download_presets_config(paths: &AppPaths, config: &DownloadPresetsConfig) -> Result<()> {
    let normalized = normalize_download_presets_config(config.clone())?;
    let path = paths.download_presets_config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(&normalized)?;
    std::fs::write(&path, format!("{json}\n"))?;
    Ok(())
}

fn normalize_download_presets_config(mut config: DownloadPresetsConfig) -> Result<DownloadPresetsConfig> {
    let mut cleaned: Vec<DownloadPreset> = Vec::new();
    for preset in config.presets.into_iter() {
        let id = preset.id.trim();
        let title = preset.title.trim();
        if id.is_empty() || title.is_empty() {
            continue;
        }
        let path_template = preset.path_template.trim();
        let filename_template = preset.filename_template.trim();
        cleaned.push(DownloadPreset {
            id: id.to_string(),
            title: title.to_string(),
            path_template: if path_template.is_empty() {
                "{provider}/{channel}".to_string()
            } else {
                path_template.to_string()
            },
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

pub fn load_optional_diarization_backend_status(paths: &AppPaths) -> Result<OptionalDiarizationBackendStatus> {
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
    std::fs::write(&config_path, format!("{json}\n"))?;

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
    std::fs::write(path, format!("{token}\n"))?;
    Ok(())
}
