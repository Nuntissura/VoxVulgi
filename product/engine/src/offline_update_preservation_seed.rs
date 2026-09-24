use crate::config::{self, DownloadPresetsConfig};
use crate::library::{self, LibraryItem};
use crate::paths::AppPaths;
use crate::subscriptions::{
    self, YoutubeSubscriptionGroupRow, YoutubeSubscriptionGroupUpsert, YoutubeSubscriptionRow,
    YoutubeSubscriptionUpsert,
};
use crate::video_libraries::{self, VideoLibraryRow, VideoLibraryUpsert};
use crate::{EngineError, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const GROUP_ID: &str = "offline-update-proof-subscription-list";
const PLAYLIST_ID: &str = "offline-update-proof-playlist";
const LIBRARY_ID: &str = "offline-update-proof-library";
const PREFERENCE_MARKER: &str = "Offline update preservation preset";

#[derive(Debug, Clone, Serialize)]
pub struct OfflineUpdateSeedCounts {
    pub preferences: usize,
    pub subscription_lists: usize,
    pub playlists: usize,
    pub video_libraries: usize,
    pub library_items: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct OfflineUpdatePreservationSeedReceipt {
    pub schema_version: u32,
    pub kind: &'static str,
    pub outcome: &'static str,
    pub generated_at_ms: i64,
    pub engine_version: String,
    pub app_base_dir: String,
    pub seed_media_path: String,
    pub seed_media_sha256: String,
    pub preference_config_path: String,
    pub preference_marker: &'static str,
    pub preference_default_preset_id: String,
    pub subscription_list: YoutubeSubscriptionGroupRow,
    pub playlist: YoutubeSubscriptionRow,
    pub video_library: VideoLibraryRow,
    pub library_item: LibraryItem,
    pub counts: OfflineUpdateSeedCounts,
    pub logical_sha256: BTreeMap<String, String>,
    pub receipt_path: String,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis() as i64)
        .unwrap_or_default()
}

fn sha256_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn sha256_file(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path)?;
    if bytes.is_empty() {
        return Err(EngineError::InstallFailed(format!(
            "update-preservation seed file is empty: {}",
            path.display()
        )));
    }
    Ok(sha256_bytes(&bytes))
}

fn sha256_json<T: Serialize>(value: &T) -> Result<String> {
    Ok(sha256_bytes(&serde_json::to_vec(value)?))
}

fn normalize_absolute_file(path: &Path) -> Result<PathBuf> {
    let path = std::fs::canonicalize(path)?;
    if !path.is_file() {
        return Err(EngineError::InstallFailed(format!(
            "update-preservation seed media is not a file: {}",
            path.display()
        )));
    }
    Ok(path)
}

fn configure_representative_preference(paths: &AppPaths) -> Result<DownloadPresetsConfig> {
    let mut config = config::load_download_presets_config(paths)?;
    let preset = config.presets.first_mut().ok_or_else(|| {
        EngineError::InstallFailed("download preference catalog has no preset".to_string())
    })?;
    preset.title = PREFERENCE_MARKER.to_string();
    preset.yt_dlp_concurrent_fragments = 3;
    preset.yt_dlp_limit_rate = Some("17M".to_string());
    preset.yt_dlp_sleep_requests = 2;
    config.default_preset_id = Some(preset.id.clone());
    config::save_download_presets_config(paths, &config)?;
    let stored = config::load_download_presets_config(paths)?;
    let stored_default = stored
        .default_preset_id
        .as_deref()
        .and_then(|id| stored.presets.iter().find(|preset| preset.id == id))
        .ok_or_else(|| {
            EngineError::InstallFailed(
                "seeded download preference default could not be read back".to_string(),
            )
        })?;
    if stored_default.title != PREFERENCE_MARKER
        || stored_default.yt_dlp_concurrent_fragments != 3
        || stored_default.yt_dlp_limit_rate.as_deref() != Some("17M")
        || stored_default.yt_dlp_sleep_requests != 2
    {
        return Err(EngineError::InstallFailed(
            "seeded download preferences failed exact readback".to_string(),
        ));
    }
    Ok(stored)
}

pub fn seed_offline_update_preservation_state(
    paths: &AppPaths,
    seed_media: &Path,
    receipt_path: &Path,
) -> Result<OfflineUpdatePreservationSeedReceipt> {
    let base_dir = std::fs::canonicalize(&paths.base_dir)?;
    let receipt_parent = receipt_path.parent().ok_or_else(|| {
        EngineError::InstallFailed("update-preservation receipt has no parent".to_string())
    })?;
    std::fs::create_dir_all(receipt_parent)?;
    let canonical_receipt_parent = std::fs::canonicalize(receipt_parent)?;
    if !canonical_receipt_parent.starts_with(&base_dir) {
        return Err(EngineError::InstallFailed(format!(
            "update-preservation receipt must stay under the isolated app-data root: {}",
            receipt_path.display()
        )));
    }

    let media_path = normalize_absolute_file(seed_media)?;
    let media_sha256 = sha256_file(&media_path)?;
    let stored_preferences = configure_representative_preference(paths)?;

    let library_root = paths.base_dir.join("offline_update_proof_library");
    std::fs::create_dir_all(&library_root)?;
    let video_library = video_libraries::upsert_video_library(
        paths,
        VideoLibraryUpsert {
            id: Some(LIBRARY_ID.to_string()),
            name: "Offline update preservation library".to_string(),
            root_path: library_root.to_string_lossy().to_string(),
            set_active: false,
        },
    )?;
    if video_library.id != LIBRARY_ID || !video_library.active {
        return Err(EngineError::InstallFailed(
            "seeded video library failed exact readback".to_string(),
        ));
    }

    let subscription_list = subscriptions::upsert_youtube_subscription_group(
        paths,
        YoutubeSubscriptionGroupUpsert {
            id: Some(GROUP_ID.to_string()),
            name: "Offline update preservation subscriptions".to_string(),
        },
    )?;
    if subscription_list.id != GROUP_ID {
        return Err(EngineError::InstallFailed(
            "seeded subscription list failed exact readback".to_string(),
        ));
    }

    let playlist = subscriptions::upsert_youtube_subscription(
        paths,
        YoutubeSubscriptionUpsert {
            id: Some(PLAYLIST_ID.to_string()),
            title: "Offline update preservation playlist".to_string(),
            source_url: "https://www.youtube.com/playlist?list=PLVOXVULGIOFFLINEUPDATE".to_string(),
            folder_map: Some("offline_update_playlist".to_string()),
            output_dir_override: None,
            library_id: Some(video_library.id.clone()),
            use_browser_cookies: false,
            browser_cookie_source: None,
            auth_session_input: None,
            clear_auth_session: false,
            active: true,
            preset_id: stored_preferences.default_preset_id.clone(),
            group_ids: vec![subscription_list.id.clone()],
            refresh_interval_minutes: Some(733),
        },
    )?;
    if playlist.id != PLAYLIST_ID
        || playlist.group_ids != vec![GROUP_ID.to_string()]
        || playlist.library_id.as_deref() != Some(LIBRARY_ID)
        || !playlist.source_url.contains("/playlist?")
    {
        return Err(EngineError::InstallFailed(
            "seeded playlist failed exact readback".to_string(),
        ));
    }

    let library_item = library::import_local_file(paths, &media_path)?;
    let library_item = library::get_item_by_id(paths, &library_item.id)?;
    if library_item.id.trim().is_empty()
        || library_item.source_type != "local_file"
        || library_item.source_uri != media_path.to_string_lossy()
    {
        return Err(EngineError::InstallFailed(
            "seeded library item failed exact readback".to_string(),
        ));
    }

    let preference_path = paths.download_presets_config_path();
    let preference_sha256 = sha256_file(&preference_path)?;
    let mut logical_sha256 = BTreeMap::new();
    logical_sha256.insert("preferences".to_string(), preference_sha256);
    logical_sha256.insert(
        "subscription_list".to_string(),
        sha256_json(&subscription_list)?,
    );
    logical_sha256.insert("playlist".to_string(), sha256_json(&playlist)?);
    logical_sha256.insert("video_library".to_string(), sha256_json(&video_library)?);
    logical_sha256.insert("library_item".to_string(), sha256_json(&library_item)?);

    let default_preset_id = stored_preferences
        .default_preset_id
        .clone()
        .ok_or_else(|| {
            EngineError::InstallFailed("seeded preference has no default preset id".to_string())
        })?;
    let receipt = OfflineUpdatePreservationSeedReceipt {
        schema_version: 1,
        kind: "voxvulgi_offline_update_preservation_seed",
        outcome: "seeded",
        generated_at_ms: now_ms(),
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        app_base_dir: base_dir.to_string_lossy().to_string(),
        seed_media_path: media_path.to_string_lossy().to_string(),
        seed_media_sha256: media_sha256,
        preference_config_path: preference_path.to_string_lossy().to_string(),
        preference_marker: PREFERENCE_MARKER,
        preference_default_preset_id: default_preset_id,
        subscription_list,
        playlist,
        video_library,
        library_item,
        counts: OfflineUpdateSeedCounts {
            preferences: 1,
            subscription_lists: 1,
            playlists: 1,
            video_libraries: 1,
            library_items: 1,
        },
        logical_sha256,
        receipt_path: receipt_path.to_string_lossy().to_string(),
    };
    let body = serde_json::to_string_pretty(&receipt)?;
    crate::persistence::atomic_write_text(receipt_path, &format!("{body}\n"))?;
    let stored: serde_json::Value = serde_json::from_slice(&std::fs::read(receipt_path)?)?;
    if stored.get("outcome").and_then(serde_json::Value::as_str) != Some("seeded") {
        return Err(EngineError::InstallFailed(
            "update-preservation seed receipt failed readback".to_string(),
        ));
    }
    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_are_stable_nonempty_proof_identities() {
        for value in [GROUP_ID, PLAYLIST_ID, LIBRARY_ID, PREFERENCE_MARKER] {
            assert!(!value.trim().is_empty());
            assert!(!value.contains(' ') || value == PREFERENCE_MARKER);
        }
        assert!(
            "https://www.youtube.com/playlist?list=PLVOXVULGIOFFLINEUPDATE".contains("/playlist?")
        );
    }
}
