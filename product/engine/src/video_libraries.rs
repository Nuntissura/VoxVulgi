use crate::paths::AppPaths;
use crate::{config, db, EngineError, Result};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

const ACTIVE_VIDEO_LIBRARY_META_KEY: &str = "active_video_library_id";

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

pub fn list_video_libraries(paths: &AppPaths) -> Result<Vec<VideoLibraryRow>> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    ensure_default_video_library_conn(paths, &conn)?;
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
    drop(conn);
    list_video_libraries(paths)
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
    fn list_bootstraps_default_library_without_creating_root() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_state"));
        crate::db::ensure_schema(&paths).expect("schema");

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
}
