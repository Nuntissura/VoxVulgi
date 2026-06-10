use crate::paths::AppPaths;
use crate::{config, db, library, subscriptions, EngineError, Result};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

const ACTIVE_VIDEO_LIBRARY_META_KEY: &str = "active_video_library_id";
const VIDEO_LIBRARY_BUNDLE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoLibraryRow {
    pub id: String,
    pub name: String,
    pub root_path: String,
    pub exists: bool,
    pub active: bool,
    pub selected: bool,
    pub kind: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoLibraryUpsert {
    pub id: Option<String>,
    pub name: String,
    pub root_path: String,
    #[serde(default)]
    pub set_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoLibraryBundleFile {
    pub schema_version: u32,
    pub exported_at_ms: i64,
    pub app: String,
    pub active_video_library_id: Option<String>,
    pub libraries: Vec<VideoLibraryRow>,
    pub youtube_subscriptions: Vec<subscriptions::YoutubeSubscriptionRow>,
    pub library_items: Vec<library::LibraryItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoLibraryBundleSummary {
    pub path: String,
    pub libraries: usize,
    pub youtube_subscriptions: usize,
    pub library_items: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoLibraryMetadataTransferRequest {
    pub source_library_id: String,
    pub target_library_id: String,
    pub mode: String,
    #[serde(default)]
    pub include_items: bool,
    #[serde(default)]
    pub include_subscriptions: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoLibraryMetadataTransferSummary {
    pub source_library_id: String,
    pub target_library_id: String,
    pub mode: String,
    pub items_matched: usize,
    pub items_copied: usize,
    pub items_moved: usize,
    pub subscriptions_moved: usize,
}

pub fn list_video_libraries(paths: &AppPaths) -> Result<Vec<VideoLibraryRow>> {
    let conn = db::open_readonly(paths)?;
    let selected_id = active_video_library_id_conn(&conn)?;

    let mut stmt = conn.prepare(
        r#"
SELECT id, name, root_path, active, kind, created_at_ms, updated_at_ms
FROM video_library
ORDER BY active DESC, updated_at_ms DESC, name COLLATE NOCASE
"#,
    )?;
    let rows = stmt
        .query_map([], |row| row_to_video_library(row, selected_id.as_deref()))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn ensure_default_video_library(paths: &AppPaths) -> Result<()> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    ensure_default_video_library_conn(paths, &conn)
}

pub fn selected_video_library(paths: &AppPaths) -> Result<VideoLibraryRow> {
    let mut rows = list_video_libraries(paths)?;
    if let Some(index) = rows.iter().position(|row| row.selected) {
        return Ok(rows.remove(index));
    }
    rows.into_iter()
        .next()
        .ok_or_else(|| EngineError::InstallFailed("no video libraries configured".to_string()))
}

pub fn selected_video_library_root(paths: &AppPaths) -> Result<PathBuf> {
    Ok(PathBuf::from(selected_video_library(paths)?.root_path))
}

pub fn get_video_library_by_id(paths: &AppPaths, id: &str) -> Result<Option<VideoLibraryRow>> {
    // WP-0226: read-only connection bypasses job-runner write queue.
    let conn = db::open_readonly(paths)?;
    let selected_id = active_video_library_id_conn(&conn)?;
    let row = conn
        .query_row(
            r#"
SELECT id, name, root_path, active, kind, created_at_ms, updated_at_ms
FROM video_library
WHERE id = ?1
"#,
            params![id],
            |row| row_to_video_library(row, selected_id.as_deref()),
        )
        .optional()?;
    Ok(row)
}

pub fn upsert_video_library(paths: &AppPaths, req: VideoLibraryUpsert) -> Result<VideoLibraryRow> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    let name = normalize_library_name(req.name)?;
    let root = normalize_library_root(req.root_path)?;
    let now = now_ms();
    let id = req
        .id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let root_text = root.to_string_lossy().to_string();

    conn.execute(
        r#"
INSERT INTO video_library (id, name, root_path, active, kind, created_at_ms, updated_at_ms)
VALUES (?1, ?2, ?3, 1, 'custom', ?4, ?4)
ON CONFLICT(id) DO UPDATE SET
  name = excluded.name,
  root_path = excluded.root_path,
  active = 1,
  updated_at_ms = excluded.updated_at_ms
ON CONFLICT(root_path) DO UPDATE SET
  name = excluded.name,
  active = 1,
  updated_at_ms = excluded.updated_at_ms
"#,
        params![id, name, root_text, now],
    )?;

    let saved_id: String = conn.query_row(
        "SELECT id FROM video_library WHERE root_path = ?1",
        params![root.to_string_lossy().to_string()],
        |row| row.get(0),
    )?;
    if req.set_active {
        set_active_video_library_conn(&conn, &saved_id)?;
    } else if active_video_library_id_conn(&conn)?.is_none() {
        set_active_video_library_conn(&conn, &saved_id)?;
    }
    get_video_library_by_id(paths, &saved_id)?
        .ok_or_else(|| EngineError::InstallFailed("failed to load video library".to_string()))
}

pub fn set_active_video_library(paths: &AppPaths, id: &str) -> Result<VideoLibraryRow> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    let exists: Option<i64> = conn
        .query_row(
            "SELECT active FROM video_library WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .optional()?;
    match exists {
        Some(1) => set_active_video_library_conn(&conn, id)?,
        Some(_) => {
            return Err(EngineError::InstallFailed(format!(
                "video library is disabled: {id}"
            )));
        }
        None => {
            return Err(EngineError::InstallFailed(format!(
                "video library not found: {id}"
            )));
        }
    }
    get_video_library_by_id(paths, id)?
        .ok_or_else(|| EngineError::InstallFailed("failed to load video library".to_string()))
}

pub fn remove_video_library(paths: &AppPaths, id: &str) -> Result<Vec<VideoLibraryRow>> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    conn.execute("DELETE FROM video_library WHERE id = ?1", params![id])?;

    let selected_id = active_video_library_id_conn(&conn)?;
    if selected_id.as_deref() == Some(id) {
        if let Some(next_id) = conn
            .query_row(
                "SELECT id FROM video_library WHERE active = 1 ORDER BY updated_at_ms DESC LIMIT 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?
        {
            set_active_video_library_conn(&conn, &next_id)?;
        } else {
            conn.execute(
                "DELETE FROM meta WHERE key = ?1",
                params![ACTIVE_VIDEO_LIBRARY_META_KEY],
            )?;
        }
    }
    ensure_default_video_library_conn(paths, &conn)?;
    drop(conn);
    list_video_libraries(paths)
}

pub fn export_video_library_bundle(
    paths: &AppPaths,
    out_path: &Path,
) -> Result<VideoLibraryBundleSummary> {
    let libraries = list_video_libraries(paths)?;
    let active_video_library_id = libraries
        .iter()
        .find(|library| library.selected)
        .map(|library| library.id.clone());
    let roots = libraries
        .iter()
        .map(|library| library.root_path.clone())
        .collect::<Vec<_>>();
    let youtube_subscriptions = subscriptions::list_youtube_subscriptions(paths)?;
    let library_items = library::list_items_under_roots(paths, &roots)?;
    let bundle = VideoLibraryBundleFile {
        schema_version: VIDEO_LIBRARY_BUNDLE_SCHEMA_VERSION,
        exported_at_ms: now_ms(),
        app: "VoxVulgi".to_string(),
        active_video_library_id,
        libraries,
        youtube_subscriptions,
        library_items,
    };

    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        out_path,
        format!("{}\n", serde_json::to_string_pretty(&bundle)?),
    )?;
    Ok(VideoLibraryBundleSummary {
        path: out_path.to_string_lossy().to_string(),
        libraries: bundle.libraries.len(),
        youtube_subscriptions: bundle.youtube_subscriptions.len(),
        library_items: bundle.library_items.len(),
    })
}

pub fn import_video_library_bundle(
    paths: &AppPaths,
    in_path: &Path,
) -> Result<VideoLibraryBundleSummary> {
    let bytes = std::fs::read(in_path)?;
    let bundle: VideoLibraryBundleFile = serde_json::from_slice(&bytes)?;
    if bundle.schema_version != VIDEO_LIBRARY_BUNDLE_SCHEMA_VERSION {
        return Err(EngineError::InstallFailed(format!(
            "unsupported video library bundle schema_version: {}",
            bundle.schema_version
        )));
    }

    for row in &bundle.libraries {
        let _ = upsert_video_library(
            paths,
            VideoLibraryUpsert {
                id: Some(row.id.clone()),
                name: row.name.clone(),
                root_path: row.root_path.clone(),
                set_active: false,
            },
        )?;
    }

    if let Some(active_id) = bundle.active_video_library_id.as_deref() {
        if get_video_library_by_id(paths, active_id)?.is_some() {
            let _ = set_active_video_library(paths, active_id)?;
        }
    }

    for row in &bundle.youtube_subscriptions {
        let _ = subscriptions::upsert_youtube_subscription(
            paths,
            subscriptions::YoutubeSubscriptionUpsert {
                id: Some(row.id.clone()),
                title: row.title.clone(),
                source_url: row.source_url.clone(),
                folder_map: Some(row.folder_map.clone()),
                output_dir_override: row.output_dir_override.clone(),
                library_id: row.library_id.clone(),
                use_browser_cookies: row.use_browser_cookies,
                browser_cookie_source: row.browser_cookie_source.clone(),
                auth_session_input: None,
                clear_auth_session: false,
                active: row.active,
                preset_id: row.preset_id.clone(),
                group_ids: row.group_ids.clone(),
                refresh_interval_minutes: Some(row.refresh_interval_minutes),
            },
        )?;
    }

    for item in &bundle.library_items {
        library::upsert_item_metadata(paths, item)?;
    }

    Ok(VideoLibraryBundleSummary {
        path: in_path.to_string_lossy().to_string(),
        libraries: bundle.libraries.len(),
        youtube_subscriptions: bundle.youtube_subscriptions.len(),
        library_items: bundle.library_items.len(),
    })
}

pub fn transfer_video_library_metadata(
    paths: &AppPaths,
    req: VideoLibraryMetadataTransferRequest,
) -> Result<VideoLibraryMetadataTransferSummary> {
    let mode = req.mode.trim().to_ascii_lowercase();
    let copy = match mode.as_str() {
        "copy" => true,
        "move" => false,
        _ => {
            return Err(EngineError::InstallFailed(format!(
                "unsupported transfer mode: {}",
                req.mode
            )));
        }
    };
    let source = get_video_library_by_id(paths, &req.source_library_id)?.ok_or_else(|| {
        EngineError::InstallFailed(format!(
            "source video library not found: {}",
            req.source_library_id
        ))
    })?;
    let target = get_video_library_by_id(paths, &req.target_library_id)?.ok_or_else(|| {
        EngineError::InstallFailed(format!(
            "target video library not found: {}",
            req.target_library_id
        ))
    })?;
    if source.id == target.id {
        return Err(EngineError::InstallFailed(
            "source and target libraries must be different".to_string(),
        ));
    }

    let mut item_summary = library::LibraryItemTransferSummary {
        source_library_id: source.id.clone(),
        target_library_id: target.id.clone(),
        mode: mode.clone(),
        items_matched: 0,
        items_copied: 0,
        items_moved: 0,
    };
    if req.include_items {
        item_summary = library::transfer_item_metadata_between_roots(
            paths,
            &source.id,
            &source.root_path,
            &target.id,
            &target.root_path,
            copy,
        )?;
    }

    let mut subscriptions_moved = 0_usize;
    if req.include_subscriptions {
        if copy {
            return Err(EngineError::InstallFailed(
                "copying saved subscriptions is not supported because subscription source URLs are unique; use move for subscriptions".to_string(),
            ));
        }
        let conn = db::open(paths)?;
        db::migrate(&conn)?;
        subscriptions_moved = conn.execute(
            "UPDATE youtube_subscription SET library_id = ?1, updated_at_ms = ?2 WHERE library_id = ?3",
            params![&target.id, now_ms(), &source.id],
        )?;
    }

    Ok(VideoLibraryMetadataTransferSummary {
        source_library_id: source.id,
        target_library_id: target.id,
        mode,
        items_matched: item_summary.items_matched,
        items_copied: item_summary.items_copied,
        items_moved: item_summary.items_moved,
        subscriptions_moved,
    })
}

pub(crate) fn default_video_library_root(paths: &AppPaths) -> Result<PathBuf> {
    let feature_roots = config::load_feature_storage_roots_config(paths)?;
    if let Some(root) = feature_roots
        .video_root
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        return Ok(PathBuf::from(root));
    }
    Ok(paths.effective_download_dir()?.join("video"))
}

fn ensure_default_video_library_conn(paths: &AppPaths, conn: &rusqlite::Connection) -> Result<()> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM video_library", [], |row| row.get(0))?;
    if count > 0 {
        return Ok(());
    }
    let root = default_video_library_root(paths)?;
    let now = now_ms();
    let id = "default-video-library".to_string();
    conn.execute(
        r#"
INSERT INTO video_library (id, name, root_path, active, kind, created_at_ms, updated_at_ms)
VALUES (?1, 'Default video library', ?2, 1, 'default', ?3, ?3)
"#,
        params![id, root.to_string_lossy().to_string(), now],
    )?;
    set_active_video_library_conn(conn, &id)?;
    Ok(())
}

fn active_video_library_id_conn(conn: &rusqlite::Connection) -> Result<Option<String>> {
    let active = conn
        .query_row(
            "SELECT value FROM meta WHERE key = ?1",
            params![ACTIVE_VIDEO_LIBRARY_META_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    Ok(active)
}

fn set_active_video_library_conn(conn: &rusqlite::Connection, id: &str) -> Result<()> {
    conn.execute(
        r#"
INSERT INTO meta(key, value) VALUES(?1, ?2)
ON CONFLICT(key) DO UPDATE SET value=excluded.value
"#,
        params![ACTIVE_VIDEO_LIBRARY_META_KEY, id],
    )?;
    Ok(())
}

fn row_to_video_library(
    row: &rusqlite::Row<'_>,
    selected_id: Option<&str>,
) -> rusqlite::Result<VideoLibraryRow> {
    let id: String = row.get(0)?;
    let root_path: String = row.get(2)?;
    Ok(VideoLibraryRow {
        selected: selected_id == Some(id.as_str()),
        exists: Path::new(&root_path).is_dir(),
        id,
        name: row.get(1)?,
        root_path,
        active: row.get::<_, i64>(3)? != 0,
        kind: row.get(4)?,
        created_at_ms: row.get(5)?,
        updated_at_ms: row.get(6)?,
    })
}

fn normalize_library_name(raw: String) -> Result<String> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(EngineError::InstallFailed(
            "video library name cannot be empty".to_string(),
        ));
    }
    Ok(name.chars().take(120).collect())
}

fn normalize_library_root(raw: String) -> Result<PathBuf> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(EngineError::InstallFailed(
            "video library root cannot be empty".to_string(),
        ));
    }
    if trimmed.contains('\0') {
        return Err(EngineError::InstallFailed(
            "video library root contains invalid characters".to_string(),
        ));
    }
    let mut path = PathBuf::from(trimmed);
    if !path.is_absolute() {
        path = std::env::current_dir()?.join(path);
    }
    Ok(path.canonicalize().unwrap_or(path))
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

    #[test]
    fn default_bootstrap_creates_default_library_without_creating_root() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_state"));
        crate::db::ensure_schema(&paths).expect("schema");
        ensure_default_video_library(&paths).expect("default library");

        let rows = list_video_libraries(&paths).expect("list");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "default-video-library");
        assert!(rows[0].selected);
        assert!(rows[0].root_path.ends_with("video"));
    }

    #[test]
    fn upsert_and_remove_video_library_does_not_touch_content() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_state"));
        let root = dir.path().join("nas_library");
        std::fs::create_dir_all(&root).expect("mkdir root");
        std::fs::write(root.join("keep.mp4"), b"x").expect("seed media");

        let saved = upsert_video_library(
            &paths,
            VideoLibraryUpsert {
                id: None,
                name: "NAS Kpop".to_string(),
                root_path: root.to_string_lossy().to_string(),
                set_active: true,
            },
        )
        .expect("upsert");
        assert!(saved.selected);

        let rows = remove_video_library(&paths, &saved.id).expect("remove");
        assert!(root.join("keep.mp4").exists());
        assert!(rows.iter().all(|row| row.id != saved.id));
    }

    #[test]
    fn video_library_bundle_roundtrip_restores_libraries_subscriptions_and_items_without_media_files(
    ) {
        let dir = tempfile::tempdir().expect("tempdir");
        let source_paths = AppPaths::new(dir.path().join("source_state"));
        crate::db::ensure_schema(&source_paths).expect("source schema");
        let library_root = dir.path().join("NAS Root");
        std::fs::create_dir_all(&library_root).expect("library root");
        let library = upsert_video_library(
            &source_paths,
            VideoLibraryUpsert {
                id: None,
                name: "NAS Root".to_string(),
                root_path: library_root.to_string_lossy().to_string(),
                set_active: true,
            },
        )
        .expect("library");
        crate::subscriptions::upsert_youtube_subscription(
            &source_paths,
            crate::subscriptions::YoutubeSubscriptionUpsert {
                id: None,
                title: "Channel".to_string(),
                source_url: "https://www.youtube.com/@channel/videos".to_string(),
                folder_map: Some("Channel".to_string()),
                output_dir_override: None,
                library_id: Some(library.id.clone()),
                use_browser_cookies: false,
                browser_cookie_source: None,
                auth_session_input: None,
                clear_auth_session: false,
                active: true,
                preset_id: None,
                group_ids: Vec::new(),
                refresh_interval_minutes: Some(60),
            },
        )
        .expect("subscription");
        let media_path = PathBuf::from(&library.root_path)
            .join("Channel")
            .join("missing_video_abc123.mp4");
        crate::library::upsert_item_metadata(
            &source_paths,
            &crate::library::LibraryItem {
                id: "item-1".to_string(),
                created_at_ms: 1,
                source_type: "youtube_yt_dlp_v1".to_string(),
                source_uri: "https://www.youtube.com/watch?v=abc123".to_string(),
                title: "Missing video".to_string(),
                media_path: media_path.to_string_lossy().to_string(),
                duration_ms: Some(1000),
                width: Some(1920),
                height: Some(1080),
                container: Some("mp4".to_string()),
                video_codec: Some("h264".to_string()),
                audio_codec: Some("aac".to_string()),
                thumbnail_path: None,
            },
        )
        .expect("item metadata");

        let bundle_path = dir.path().join("library_bundle.json");
        let exported = export_video_library_bundle(&source_paths, &bundle_path).expect("export");
        assert_eq!(exported.libraries, 1);
        assert_eq!(exported.youtube_subscriptions, 1);
        assert_eq!(exported.library_items, 1);

        let target_paths = AppPaths::new(dir.path().join("target_state"));
        crate::db::ensure_schema(&target_paths).expect("target schema");
        let imported = import_video_library_bundle(&target_paths, &bundle_path).expect("import");
        assert_eq!(imported.libraries, 1);
        assert_eq!(imported.youtube_subscriptions, 1);
        assert_eq!(imported.library_items, 1);
        assert!(!media_path.exists(), "import must not create media files");

        let target_libraries = list_video_libraries(&target_paths).expect("libraries");
        assert_eq!(target_libraries.len(), 1);
        assert_eq!(target_libraries[0].name, "NAS Root");
        let items = crate::library::list_items_under_roots(
            &target_paths,
            &[target_libraries[0].root_path.clone()],
        )
        .expect("items");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "item-1");
    }

    #[test]
    fn transfer_video_library_metadata_moves_subscriptions_and_copies_items_without_touching_files()
    {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_state"));
        crate::db::ensure_schema(&paths).expect("schema");
        let source_root = dir.path().join("source_library");
        let target_root = dir.path().join("target_library");
        std::fs::create_dir_all(source_root.join("Channel")).expect("source folder");
        std::fs::create_dir_all(&target_root).expect("target folder");
        let source = upsert_video_library(
            &paths,
            VideoLibraryUpsert {
                id: None,
                name: "Source".to_string(),
                root_path: source_root.to_string_lossy().to_string(),
                set_active: true,
            },
        )
        .expect("source");
        let stored_source_channel = PathBuf::from(&source.root_path).join("Channel");
        std::fs::create_dir_all(&stored_source_channel).expect("stored source folder");
        let source_file = stored_source_channel.join("video_abc123.mp4");
        std::fs::write(&source_file, b"media").expect("media");
        let target = upsert_video_library(
            &paths,
            VideoLibraryUpsert {
                id: None,
                name: "Target".to_string(),
                root_path: target_root.to_string_lossy().to_string(),
                set_active: false,
            },
        )
        .expect("target");
        let sub = crate::subscriptions::upsert_youtube_subscription(
            &paths,
            crate::subscriptions::YoutubeSubscriptionUpsert {
                id: None,
                title: "Channel".to_string(),
                source_url: "https://www.youtube.com/@channel/videos".to_string(),
                folder_map: Some("Channel".to_string()),
                output_dir_override: None,
                library_id: Some(source.id.clone()),
                use_browser_cookies: false,
                browser_cookie_source: None,
                auth_session_input: None,
                clear_auth_session: false,
                active: true,
                preset_id: None,
                group_ids: Vec::new(),
                refresh_interval_minutes: Some(60),
            },
        )
        .expect("subscription");
        crate::library::upsert_item_metadata(
            &paths,
            &crate::library::LibraryItem {
                id: "item-copy".to_string(),
                created_at_ms: 1,
                source_type: "youtube_yt_dlp_v1".to_string(),
                source_uri: "https://www.youtube.com/watch?v=abc123".to_string(),
                title: "Video".to_string(),
                media_path: source_file.to_string_lossy().to_string(),
                duration_ms: None,
                width: None,
                height: None,
                container: Some("mp4".to_string()),
                video_codec: None,
                audio_codec: None,
                thumbnail_path: None,
            },
        )
        .expect("item");

        let copied = transfer_video_library_metadata(
            &paths,
            VideoLibraryMetadataTransferRequest {
                source_library_id: source.id.clone(),
                target_library_id: target.id.clone(),
                mode: "copy".to_string(),
                include_items: true,
                include_subscriptions: false,
            },
        )
        .expect("copy");
        assert_eq!(copied.items_copied, 1);
        assert!(source_file.exists(), "copy must not move or delete media");

        let moved_subs = transfer_video_library_metadata(
            &paths,
            VideoLibraryMetadataTransferRequest {
                source_library_id: source.id.clone(),
                target_library_id: target.id.clone(),
                mode: "move".to_string(),
                include_items: false,
                include_subscriptions: true,
            },
        )
        .expect("move subscriptions");
        assert_eq!(moved_subs.subscriptions_moved, 1);
        let moved = crate::subscriptions::get_youtube_subscription_by_id(&paths, &sub.id)
            .expect("load sub")
            .expect("sub exists");
        assert_eq!(moved.library_id.as_deref(), Some(target.id.as_str()));
        assert!(
            source_file.exists(),
            "subscription move must not touch media"
        );
    }
}
