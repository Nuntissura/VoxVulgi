use crate::paths::AppPaths;
use crate::{persistence, EngineError, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use uuid::Uuid;

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
static PROVIDER_TRANSFER_SETTINGS_WRITE_LOCK: Mutex<()> = Mutex::new(());
static FEATURE_STORAGE_ROOTS_WRITE_LOCK: Mutex<()> = Mutex::new(());
static LOCALIZATION_PIPELINE_PRESETS_WRITE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BatchOnImportRules {
    pub auto_asr: bool,
    pub auto_translate: bool,
    pub auto_separate: bool,
    pub auto_diarize: bool,
    pub auto_dub_preview: bool,
}

const LOCALIZATION_PIPELINE_PRESETS_SCHEMA_VERSION: u32 = 1;
const MAX_LOCALIZATION_PRESET_NAME_BYTES: usize = 160;
const MAX_LOCALIZATION_PRESET_INSTRUCTION_BYTES: usize = 1_024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalizationPipelinePreset {
    pub id: String,
    pub name: String,
    pub is_builtin: bool,
    pub asr_lang: String,
    pub batch_rules: BatchOnImportRules,
    pub translation_style: String,
    pub honorific_mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_translation_instruction: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_voice_template_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_voice_cast_pack_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalizationPipelinePresetCatalog {
    pub schema_version: u32,
    pub presets: Vec<LocalizationPipelinePreset>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ItemLocalizationPipelinePreset {
    pub schema_version: u32,
    pub preset: LocalizationPipelinePreset,
    pub voice_defaults_applied: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LocalizationPipelinePresetFile {
    schema_version: u32,
    presets: Vec<LocalizationPipelinePreset>,
}

fn localization_pipeline_builtin_presets() -> Vec<LocalizationPipelinePreset> {
    vec![
        LocalizationPipelinePreset {
            id: "builtin-japanese-anime".to_string(),
            name: "Japanese Anime".to_string(),
            is_builtin: true,
            asr_lang: "ja".to_string(),
            batch_rules: BatchOnImportRules {
                auto_asr: true,
                auto_translate: true,
                auto_separate: false,
                auto_diarize: true,
                auto_dub_preview: false,
            },
            translation_style: "informal".to_string(),
            honorific_mode: "preserve".to_string(),
            custom_translation_instruction: None,
            default_voice_template_id: None,
            default_voice_cast_pack_id: None,
        },
        LocalizationPipelinePreset {
            id: "builtin-korean-variety".to_string(),
            name: "Korean Variety".to_string(),
            is_builtin: true,
            asr_lang: "ko".to_string(),
            batch_rules: BatchOnImportRules {
                auto_asr: true,
                auto_translate: true,
                auto_separate: false,
                auto_diarize: true,
                auto_dub_preview: false,
            },
            translation_style: "informal".to_string(),
            honorific_mode: "translate".to_string(),
            custom_translation_instruction: None,
            default_voice_template_id: None,
            default_voice_cast_pack_id: None,
        },
        LocalizationPipelinePreset {
            id: "builtin-quick-subtitles-only".to_string(),
            name: "Quick Subtitles Only".to_string(),
            is_builtin: true,
            asr_lang: "auto".to_string(),
            batch_rules: BatchOnImportRules {
                auto_asr: true,
                auto_translate: false,
                auto_separate: false,
                auto_diarize: false,
                auto_dub_preview: false,
            },
            translation_style: "neutral".to_string(),
            honorific_mode: "preserve".to_string(),
            custom_translation_instruction: None,
            default_voice_template_id: None,
            default_voice_cast_pack_id: None,
        },
    ]
}

fn normalize_localization_preset_optional(
    value: Option<String>,
    field: &str,
) -> Result<Option<String>> {
    let value = value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if let Some(value) = value.as_deref() {
        if value.len() > MAX_LOCALIZATION_PRESET_INSTRUCTION_BYTES
            || value.chars().any(char::is_control)
        {
            return Err(EngineError::InstallFailed(format!(
                "localization preset {field} is invalid"
            )));
        }
    }
    Ok(value)
}

fn normalize_localization_pipeline_preset(
    mut preset: LocalizationPipelinePreset,
) -> Result<LocalizationPipelinePreset> {
    preset.id = preset.id.trim().to_string();
    preset.name = preset.name.trim().to_string();
    if preset.id.is_empty()
        || preset.id.len() > MAX_LOCALIZATION_PRESET_NAME_BYTES
        || preset.id.contains('/')
        || preset.id.contains('\\')
        || preset.id.chars().any(char::is_control)
    {
        return Err(EngineError::InstallFailed(
            "localization preset id is invalid".to_string(),
        ));
    }
    if preset.name.is_empty()
        || preset.name.len() > MAX_LOCALIZATION_PRESET_NAME_BYTES
        || preset.name.chars().any(char::is_control)
    {
        return Err(EngineError::InstallFailed(format!(
            "localization preset name must be 1-{MAX_LOCALIZATION_PRESET_NAME_BYTES} UTF-8 bytes without control characters"
        )));
    }
    preset.asr_lang = preset.asr_lang.trim().to_ascii_lowercase();
    if !matches!(preset.asr_lang.as_str(), "auto" | "ja" | "ko") {
        return Err(EngineError::InstallFailed(
            "localization preset asr_lang must be auto, ja, or ko".to_string(),
        ));
    }
    preset.translation_style = preset.translation_style.trim().to_ascii_lowercase();
    if !matches!(
        preset.translation_style.as_str(),
        "neutral" | "formal" | "informal" | "custom"
    ) {
        return Err(EngineError::InstallFailed(
            "localization preset translation_style is invalid".to_string(),
        ));
    }
    preset.honorific_mode = preset.honorific_mode.trim().to_ascii_lowercase();
    if !matches!(
        preset.honorific_mode.as_str(),
        "preserve" | "translate" | "drop"
    ) {
        return Err(EngineError::InstallFailed(
            "localization preset honorific_mode is invalid".to_string(),
        ));
    }
    preset.custom_translation_instruction = normalize_localization_preset_optional(
        preset.custom_translation_instruction,
        "custom translation instruction",
    )?;
    if preset.translation_style == "custom" && preset.custom_translation_instruction.is_none() {
        return Err(EngineError::InstallFailed(
            "custom translation style requires a custom instruction".to_string(),
        ));
    }
    preset.default_voice_template_id = normalize_localization_preset_optional(
        preset.default_voice_template_id,
        "default voice template id",
    )?;
    preset.default_voice_cast_pack_id = normalize_localization_preset_optional(
        preset.default_voice_cast_pack_id,
        "default voice cast pack id",
    )?;
    Ok(preset)
}

fn load_custom_localization_pipeline_presets(
    paths: &AppPaths,
) -> Result<Vec<LocalizationPipelinePreset>> {
    let path = paths.localization_pipeline_presets_path();
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file: LocalizationPipelinePresetFile = serde_json::from_slice(&std::fs::read(&path)?)
        .map_err(|error| {
            EngineError::InstallFailed(format!(
                "failed to parse localization pipeline presets at {}: {error}",
                path.to_string_lossy()
            ))
        })?;
    if file.schema_version != LOCALIZATION_PIPELINE_PRESETS_SCHEMA_VERSION {
        return Err(EngineError::InstallFailed(format!(
            "unsupported localization pipeline preset schema_version: {}",
            file.schema_version
        )));
    }
    let built_in_ids = localization_pipeline_builtin_presets()
        .into_iter()
        .map(|preset| preset.id)
        .collect::<std::collections::HashSet<_>>();
    let mut by_id = std::collections::BTreeMap::new();
    for mut preset in file.presets {
        preset.is_builtin = false;
        let preset = normalize_localization_pipeline_preset(preset)?;
        if built_in_ids.contains(&preset.id) || !preset.id.starts_with("custom-") {
            return Err(EngineError::InstallFailed(format!(
                "custom localization preset id is reserved or invalid: {}",
                preset.id
            )));
        }
        by_id.insert(preset.id.clone(), preset);
    }
    Ok(by_id.into_values().collect())
}

fn save_custom_localization_pipeline_presets(
    paths: &AppPaths,
    presets: Vec<LocalizationPipelinePreset>,
) -> Result<()> {
    let path = paths.localization_pipeline_presets_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = LocalizationPipelinePresetFile {
        schema_version: LOCALIZATION_PIPELINE_PRESETS_SCHEMA_VERSION,
        presets,
    };
    persistence::atomic_write_text(
        &path,
        &format!("{}\n", serde_json::to_string_pretty(&file)?),
    )?;
    Ok(())
}

pub fn load_localization_pipeline_presets(
    paths: &AppPaths,
) -> Result<LocalizationPipelinePresetCatalog> {
    let mut presets = localization_pipeline_builtin_presets();
    presets.extend(load_custom_localization_pipeline_presets(paths)?);
    Ok(LocalizationPipelinePresetCatalog {
        schema_version: LOCALIZATION_PIPELINE_PRESETS_SCHEMA_VERSION,
        presets,
    })
}

pub fn save_localization_pipeline_preset(
    paths: &AppPaths,
    mut preset: LocalizationPipelinePreset,
) -> Result<LocalizationPipelinePresetCatalog> {
    let _guard = LOCALIZATION_PIPELINE_PRESETS_WRITE_LOCK
        .lock()
        .map_err(|_| {
            EngineError::InstallFailed("localization preset writer lock is poisoned".to_string())
        })?;
    if preset.id.trim().is_empty() {
        preset.id = format!("custom-{}", Uuid::new_v4());
    }
    preset.is_builtin = false;
    let preset = normalize_localization_pipeline_preset(preset)?;
    if !preset.id.starts_with("custom-") {
        return Err(EngineError::InstallFailed(
            "built-in localization presets cannot be changed".to_string(),
        ));
    }
    let mut presets = load_custom_localization_pipeline_presets(paths)?;
    if let Some(existing) = presets.iter_mut().find(|existing| existing.id == preset.id) {
        *existing = preset;
    } else {
        presets.push(preset);
    }
    presets.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.id.cmp(&right.id))
    });
    save_custom_localization_pipeline_presets(paths, presets)?;
    load_localization_pipeline_presets(paths)
}

pub fn delete_localization_pipeline_preset(
    paths: &AppPaths,
    preset_id: &str,
) -> Result<LocalizationPipelinePresetCatalog> {
    let _guard = LOCALIZATION_PIPELINE_PRESETS_WRITE_LOCK
        .lock()
        .map_err(|_| {
            EngineError::InstallFailed("localization preset writer lock is poisoned".to_string())
        })?;
    let preset_id = preset_id.trim();
    if !preset_id.starts_with("custom-") {
        return Err(EngineError::InstallFailed(
            "built-in localization presets cannot be deleted".to_string(),
        ));
    }
    let mut presets = load_custom_localization_pipeline_presets(paths)?;
    let before = presets.len();
    presets.retain(|preset| preset.id != preset_id);
    if presets.len() == before {
        return Err(EngineError::InstallFailed(format!(
            "localization preset not found: {preset_id}"
        )));
    }
    save_custom_localization_pipeline_presets(paths, presets)?;
    load_localization_pipeline_presets(paths)
}

fn validate_localization_preset_item_id(item_id: &str) -> Result<&str> {
    let item_id = item_id.trim();
    if item_id.is_empty()
        || item_id == "."
        || item_id == ".."
        || item_id.contains('/')
        || item_id.contains('\\')
    {
        return Err(EngineError::InstallFailed(
            "invalid localization preset item id".to_string(),
        ));
    }
    Ok(item_id)
}

pub fn localization_pipeline_preset_by_id(
    paths: &AppPaths,
    preset_id: &str,
) -> Result<LocalizationPipelinePreset> {
    let preset_id = preset_id.trim();
    load_localization_pipeline_presets(paths)?
        .presets
        .into_iter()
        .find(|preset| preset.id == preset_id)
        .ok_or_else(|| {
            EngineError::InstallFailed(format!(
                "localization pipeline preset not found: {preset_id}"
            ))
        })
}

pub fn apply_localization_pipeline_preset_to_item(
    paths: &AppPaths,
    item_id: &str,
    preset: LocalizationPipelinePreset,
) -> Result<ItemLocalizationPipelinePreset> {
    let _guard = LOCALIZATION_PIPELINE_PRESETS_WRITE_LOCK
        .lock()
        .map_err(|_| {
            EngineError::InstallFailed("localization preset writer lock is poisoned".to_string())
        })?;
    let item_id = validate_localization_preset_item_id(item_id)?;
    let preset = normalize_localization_pipeline_preset(preset)?;
    let applied = ItemLocalizationPipelinePreset {
        schema_version: LOCALIZATION_PIPELINE_PRESETS_SCHEMA_VERSION,
        preset,
        voice_defaults_applied: false,
    };
    let path = paths.item_localization_pipeline_preset_path(item_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    persistence::atomic_write_text(
        &path,
        &format!("{}\n", serde_json::to_string_pretty(&applied)?),
    )?;
    Ok(applied)
}

pub fn load_item_localization_pipeline_preset(
    paths: &AppPaths,
    item_id: &str,
) -> Result<Option<ItemLocalizationPipelinePreset>> {
    let path = paths
        .item_localization_pipeline_preset_path(validate_localization_preset_item_id(item_id)?);
    if !path.exists() {
        return Ok(None);
    }
    let applied: ItemLocalizationPipelinePreset = serde_json::from_slice(&std::fs::read(&path)?)
        .map_err(|error| {
            EngineError::InstallFailed(format!(
                "failed to parse item localization pipeline preset at {}: {error}",
                path.to_string_lossy()
            ))
        })?;
    if applied.schema_version != LOCALIZATION_PIPELINE_PRESETS_SCHEMA_VERSION {
        return Err(EngineError::InstallFailed(format!(
            "unsupported item localization pipeline preset schema_version: {}",
            applied.schema_version
        )));
    }
    Ok(Some(ItemLocalizationPipelinePreset {
        schema_version: applied.schema_version,
        preset: normalize_localization_pipeline_preset(applied.preset)?,
        voice_defaults_applied: applied.voice_defaults_applied,
    }))
}

pub fn mark_item_localization_pipeline_voice_defaults_applied(
    paths: &AppPaths,
    item_id: &str,
) -> Result<Option<ItemLocalizationPipelinePreset>> {
    let _guard = LOCALIZATION_PIPELINE_PRESETS_WRITE_LOCK
        .lock()
        .map_err(|_| {
            EngineError::InstallFailed("localization preset writer lock is poisoned".to_string())
        })?;
    let Some(mut applied) = load_item_localization_pipeline_preset(paths, item_id)? else {
        return Ok(None);
    };
    applied.voice_defaults_applied = true;
    let path = paths
        .item_localization_pipeline_preset_path(validate_localization_preset_item_id(item_id)?);
    persistence::atomic_write_text(
        &path,
        &format!("{}\n", serde_json::to_string_pretty(&applied)?),
    )?;
    Ok(Some(applied))
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
    pub tiktok_root: Option<String>,
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
    config.tiktok_root = normalize_optional_path(config.tiktok_root);
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
pub struct ProviderTransferPolicy {
    pub concurrent_fragments: u32,
    pub limit_rate: Option<String>,
    pub sleep_interval_secs: u32,
    pub sleep_requests_secs: u32,
}

impl ProviderTransferPolicy {
    fn instagram_single_default() -> Self {
        Self {
            concurrent_fragments: 2,
            limit_rate: None,
            sleep_interval_secs: 1,
            sleep_requests_secs: 1,
        }
    }

    fn instagram_recurring_default() -> Self {
        Self {
            concurrent_fragments: 1,
            limit_rate: Some("4M".to_string()),
            sleep_interval_secs: 3,
            sleep_requests_secs: 1,
        }
    }

    fn tiktok_single_default() -> Self {
        Self {
            concurrent_fragments: 2,
            limit_rate: None,
            sleep_interval_secs: 0,
            sleep_requests_secs: 0,
        }
    }

    fn tiktok_recurring_default() -> Self {
        Self {
            concurrent_fragments: 1,
            limit_rate: Some("6M".to_string()),
            sleep_interval_secs: 2,
            sleep_requests_secs: 1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderTransferSettings {
    pub schema_version: u32,
    pub instagram_single: ProviderTransferPolicy,
    pub instagram_recurring: ProviderTransferPolicy,
    pub tiktok_single: ProviderTransferPolicy,
    pub tiktok_recurring: ProviderTransferPolicy,
    #[serde(default)]
    pub tiktok_browser_cookie_source: Option<String>,
    #[serde(default)]
    pub tiktok_api_hostname: Option<String>,
    #[serde(default)]
    pub tiktok_app_info: Option<String>,
    #[serde(default)]
    pub tiktok_device_id: Option<String>,
}

impl Default for ProviderTransferSettings {
    fn default() -> Self {
        Self {
            schema_version: 1,
            instagram_single: ProviderTransferPolicy::instagram_single_default(),
            instagram_recurring: ProviderTransferPolicy::instagram_recurring_default(),
            tiktok_single: ProviderTransferPolicy::tiktok_single_default(),
            tiktok_recurring: ProviderTransferPolicy::tiktok_recurring_default(),
            tiktok_browser_cookie_source: None,
            tiktok_api_hostname: None,
            tiktok_app_info: None,
            tiktok_device_id: None,
        }
    }
}

impl ProviderTransferSettings {
    pub fn policy_for_track(&self, track: &str) -> Option<&ProviderTransferPolicy> {
        match track {
            "instagram_single" => Some(&self.instagram_single),
            "instagram_recurring" => Some(&self.instagram_recurring),
            "tiktok_single" => Some(&self.tiktok_single),
            "tiktok_recurring" => Some(&self.tiktok_recurring),
            _ => None,
        }
    }
}

fn normalize_provider_limit_rate(value: Option<String>) -> Result<Option<String>> {
    let Some(value) = value
        .map(|raw| raw.trim().to_string())
        .filter(|raw| !raw.is_empty())
    else {
        return Ok(None);
    };
    if value.len() > 16 {
        return Err(EngineError::InstallFailed(
            "provider bandwidth cap is too long".to_string(),
        ));
    }
    let (number, suffix) = match value.chars().last() {
        Some(last) if last.is_ascii_alphabetic() => (&value[..value.len() - 1], Some(last)),
        _ => (value.as_str(), None),
    };
    if !suffix.is_none_or(|suffix| matches!(suffix.to_ascii_uppercase(), 'K' | 'M' | 'G'))
        || number.is_empty()
        || number.matches('.').count() > 1
        || !number.chars().all(|ch| ch.is_ascii_digit() || ch == '.')
        || number
            .parse::<f64>()
            .ok()
            .is_none_or(|number| !number.is_finite() || number <= 0.0)
    {
        return Err(EngineError::InstallFailed(format!(
            "invalid provider bandwidth cap '{value}'; use values such as 750K, 4M, or 1.5G"
        )));
    }
    Ok(Some(match suffix {
        Some(suffix) => format!("{}{}", number, suffix.to_ascii_uppercase()),
        None => number.to_string(),
    }))
}

fn normalize_provider_transfer_policy(
    mut policy: ProviderTransferPolicy,
) -> Result<ProviderTransferPolicy> {
    if !(1..=32).contains(&policy.concurrent_fragments) {
        return Err(EngineError::InstallFailed(
            "provider concurrent fragments must be between 1 and 32".to_string(),
        ));
    }
    if policy.sleep_interval_secs > 86_400 || policy.sleep_requests_secs > 10_000 {
        return Err(EngineError::InstallFailed(
            "provider pacing delay is outside the supported range".to_string(),
        ));
    }
    policy.limit_rate = normalize_provider_limit_rate(policy.limit_rate)?;
    Ok(policy)
}

fn normalize_provider_transfer_settings(
    mut settings: ProviderTransferSettings,
) -> Result<ProviderTransferSettings> {
    if settings.schema_version != 1 {
        return Err(EngineError::InstallFailed(format!(
            "unsupported provider transfer settings schema {}",
            settings.schema_version
        )));
    }
    settings.instagram_single = normalize_provider_transfer_policy(settings.instagram_single)?;
    settings.instagram_recurring =
        normalize_provider_transfer_policy(settings.instagram_recurring)?;
    settings.tiktok_single = normalize_provider_transfer_policy(settings.tiktok_single)?;
    settings.tiktok_recurring = normalize_provider_transfer_policy(settings.tiktok_recurring)?;
    settings.tiktok_browser_cookie_source = normalize_optional_provider_setting(
        settings.tiktok_browser_cookie_source,
        "TikTok browser cookie source",
        32,
        |value| matches!(value, "firefox" | "chrome" | "edge" | "opera"),
    )?;
    settings.tiktok_api_hostname = normalize_optional_provider_setting(
        settings.tiktok_api_hostname,
        "TikTok API hostname",
        255,
        |value| {
            value
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-'))
        },
    )?;
    settings.tiktok_app_info = normalize_optional_provider_setting(
        settings.tiktok_app_info,
        "TikTok app info",
        500,
        |value| !value.chars().any(|ch| matches!(ch, ';' | '\r' | '\n')),
    )?;
    settings.tiktok_device_id = normalize_optional_provider_setting(
        settings.tiktok_device_id,
        "TikTok device id",
        128,
        |value| {
            value
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
        },
    )?;
    Ok(settings)
}

fn normalize_optional_provider_setting<F>(
    value: Option<String>,
    label: &str,
    max_len: usize,
    predicate: F,
) -> Result<Option<String>>
where
    F: Fn(&str) -> bool,
{
    let Some(value) = value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    let normalized = if label == "TikTok browser cookie source" {
        value.to_ascii_lowercase()
    } else {
        value
    };
    if normalized.len() > max_len || !predicate(&normalized) {
        return Err(EngineError::InstallFailed(format!("invalid {label}")));
    }
    Ok(Some(normalized))
}

pub fn load_provider_transfer_settings(paths: &AppPaths) -> Result<ProviderTransferSettings> {
    let path = paths.provider_transfer_settings_path();
    if !path.exists() {
        return Ok(ProviderTransferSettings::default());
    }
    let bytes = std::fs::read(&path)?;
    let parsed = serde_json::from_slice::<ProviderTransferSettings>(&bytes).map_err(|error| {
        EngineError::InstallFailed(format!(
            "failed to parse provider transfer settings at {}: {error}",
            path.to_string_lossy()
        ))
    })?;
    normalize_provider_transfer_settings(parsed)
}

pub fn save_provider_transfer_settings(
    paths: &AppPaths,
    settings: &ProviderTransferSettings,
) -> Result<ProviderTransferSettings> {
    let _guard = PROVIDER_TRANSFER_SETTINGS_WRITE_LOCK.lock().map_err(|_| {
        EngineError::InstallFailed("provider transfer settings writer lock is poisoned".to_string())
    })?;
    let normalized = normalize_provider_transfer_settings(settings.clone())?;
    let path = paths.provider_transfer_settings_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = format!("{}\n", serde_json::to_string_pretty(&normalized)?);
    persistence::atomic_write_text(&path, &text)?;
    load_provider_transfer_settings(paths)
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
        assert_eq!(
            saved.netscape_cookie_json.as_deref(),
            Some("SID=complete-revision")
        );
    }

    #[test]
    fn youtube_auth_revision_cas_rejects_stale_verification_and_replacement() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app"));
        paths.ensure_dirs().expect("dirs");
        replace_current_youtube_auth(&paths, manual_auth("SID=old")).expect("initial auth");
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
        replace_current_youtube_auth(&paths, manual_auth("SID=base")).expect("base auth");
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
        replace_current_youtube_auth(&paths, manual_auth("SID=observed")).expect("initial auth");
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

    #[test]
    fn localization_pipeline_presets_include_three_immutable_builtins() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app"));
        let catalog = load_localization_pipeline_presets(&paths).expect("catalog");
        assert_eq!(catalog.schema_version, 1);
        assert_eq!(catalog.presets.len(), 3);
        assert!(catalog.presets.iter().all(|preset| preset.is_builtin));
        assert_eq!(catalog.presets[0].name, "Japanese Anime");
        assert_eq!(catalog.presets[0].asr_lang, "ja");
        assert!(catalog.presets[0].batch_rules.auto_translate);
        assert!(catalog.presets[0].batch_rules.auto_diarize);
        assert_eq!(catalog.presets[2].name, "Quick Subtitles Only");
        assert!(catalog.presets[2].batch_rules.auto_asr);
        assert!(!catalog.presets[2].batch_rules.auto_translate);
    }

    #[test]
    fn localization_pipeline_custom_preset_crud_is_atomic_and_preserves_builtins() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app"));
        paths.ensure_dirs().expect("dirs");
        let catalog = save_localization_pipeline_preset(
            &paths,
            LocalizationPipelinePreset {
                id: String::new(),
                name: " My show ".to_string(),
                is_builtin: true,
                asr_lang: "ko".to_string(),
                batch_rules: BatchOnImportRules {
                    auto_asr: true,
                    auto_translate: true,
                    auto_separate: false,
                    auto_diarize: true,
                    auto_dub_preview: true,
                },
                translation_style: "formal".to_string(),
                honorific_mode: "drop".to_string(),
                custom_translation_instruction: None,
                default_voice_template_id: Some(" template-1 ".to_string()),
                default_voice_cast_pack_id: None,
            },
        )
        .expect("create custom preset");
        assert_eq!(catalog.presets.len(), 4);
        let created = catalog
            .presets
            .iter()
            .find(|preset| !preset.is_builtin)
            .expect("custom preset")
            .clone();
        assert!(created.id.starts_with("custom-"));
        assert_eq!(created.name, "My show");
        assert_eq!(
            created.default_voice_template_id.as_deref(),
            Some("template-1")
        );

        let applied = apply_localization_pipeline_preset_to_item(&paths, "item-1", created.clone())
            .expect("apply to item");
        assert!(!applied.voice_defaults_applied);
        assert_eq!(
            load_item_localization_pipeline_preset(&paths, "item-1")
                .expect("load applied")
                .expect("applied preset")
                .preset
                .id,
            created.id
        );
        assert!(
            mark_item_localization_pipeline_voice_defaults_applied(&paths, "item-1")
                .expect("mark applied")
                .expect("marked preset")
                .voice_defaults_applied
        );

        let mut updated = created.clone();
        updated.name = "My edited show".to_string();
        let catalog = save_localization_pipeline_preset(&paths, updated).expect("update");
        assert_eq!(catalog.presets.len(), 4);
        assert!(catalog
            .presets
            .iter()
            .any(|preset| preset.name == "My edited show"));

        let catalog = delete_localization_pipeline_preset(&paths, &created.id).expect("delete");
        assert_eq!(catalog.presets.len(), 3);
        assert!(catalog.presets.iter().all(|preset| preset.is_builtin));
    }

    #[test]
    fn localization_pipeline_presets_reject_reserved_and_unsafe_values() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app"));
        let built_in = localization_pipeline_builtin_presets()
            .into_iter()
            .next()
            .expect("built in");
        let error = save_localization_pipeline_preset(&paths, built_in)
            .expect_err("built in overwrite must fail");
        assert!(error.to_string().contains("cannot be changed"));
        let error = delete_localization_pipeline_preset(&paths, "builtin-japanese-anime")
            .expect_err("built in deletion must fail");
        assert!(error.to_string().contains("cannot be deleted"));

        let error = apply_localization_pipeline_preset_to_item(
            &paths,
            "../outside",
            localization_pipeline_builtin_presets()[0].clone(),
        )
        .expect_err("item traversal must fail");
        assert!(error
            .to_string()
            .contains("invalid localization preset item id"));

        let mut unsafe_preset = localization_pipeline_builtin_presets()[0].clone();
        unsafe_preset.id = String::new();
        unsafe_preset.name = "bad\0name".to_string();
        let error = save_localization_pipeline_preset(&paths, unsafe_preset)
            .expect_err("control character must fail");
        assert!(error.to_string().contains("without control characters"));

        let mut incomplete_custom = localization_pipeline_builtin_presets()[0].clone();
        incomplete_custom.id = String::new();
        incomplete_custom.translation_style = "custom".to_string();
        let error = save_localization_pipeline_preset(&paths, incomplete_custom)
            .expect_err("custom style without instruction must fail");
        assert!(error.to_string().contains("requires a custom instruction"));
    }

    #[test]
    fn provider_transfer_settings_are_independent_validated_and_restart_durable() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app"));
        paths.ensure_dirs().expect("dirs");

        let defaults = load_provider_transfer_settings(&paths).expect("defaults");
        assert_eq!(defaults.instagram_single.concurrent_fragments, 2);
        assert_eq!(
            defaults.instagram_recurring.limit_rate.as_deref(),
            Some("4M")
        );
        assert_eq!(defaults.tiktok_recurring.limit_rate.as_deref(), Some("6M"));

        let mut changed = defaults.clone();
        changed.instagram_single.concurrent_fragments = 7;
        changed.instagram_single.limit_rate = Some("1.5g".to_string());
        changed.instagram_recurring.sleep_interval_secs = 17;
        changed.tiktok_single.limit_rate = Some("750k".to_string());
        changed.tiktok_recurring.sleep_requests_secs = 9;
        changed.tiktok_browser_cookie_source = Some("Firefox".to_string());
        changed.tiktok_api_hostname = Some("api16-normal-c-useast1a.tiktokv.com".to_string());
        changed.tiktok_device_id = Some("stable_device_01".to_string());
        let saved = save_provider_transfer_settings(&paths, &changed).expect("save");
        assert_eq!(saved.instagram_single.limit_rate.as_deref(), Some("1.5G"));
        assert_eq!(saved.tiktok_single.limit_rate.as_deref(), Some("750K"));
        assert_eq!(
            saved.tiktok_browser_cookie_source.as_deref(),
            Some("firefox")
        );

        let restarted = AppPaths::new(dir.path().join("app"));
        assert_eq!(
            load_provider_transfer_settings(&restarted).expect("restart load"),
            saved
        );

        let mut invalid = saved.clone();
        invalid.instagram_recurring.concurrent_fragments = 0;
        assert!(save_provider_transfer_settings(&paths, &invalid).is_err());
        assert_eq!(
            load_provider_transfer_settings(&paths).expect("unchanged"),
            saved
        );

        invalid = saved.clone();
        invalid.tiktok_recurring.limit_rate = Some("4M;bad".to_string());
        assert!(save_provider_transfer_settings(&paths, &invalid).is_err());
        assert_eq!(
            load_provider_transfer_settings(&paths).expect("still unchanged"),
            saved
        );

        invalid = saved.clone();
        invalid.tiktok_api_hostname = Some("https://unsafe.example".to_string());
        assert!(save_provider_transfer_settings(&paths, &invalid).is_err());
    }
}
