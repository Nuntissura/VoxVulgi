use crate::ffmpeg;
use crate::paths::AppPaths;
use crate::{db, Result};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryItem {
    pub id: String,
    pub created_at_ms: i64,
    pub source_type: String,
    pub source_uri: String,
    pub title: String,
    pub media_path: String,
    pub duration_ms: Option<i64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub container: Option<String>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub thumbnail_path: Option<String>,
}

pub fn list_items(paths: &AppPaths, limit: usize, offset: usize) -> Result<Vec<LibraryItem>> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;

    let mut stmt = conn.prepare(
        r#"
SELECT
  id,
  created_at_ms,
  source_type,
  source_uri,
  title,
  media_path,
  duration_ms,
  width,
  height,
  container,
  video_codec,
  audio_codec,
  thumbnail_path
FROM library_item
ORDER BY created_at_ms DESC
LIMIT ?1 OFFSET ?2
"#,
    )?;

    let items = stmt
        .query_map(params![limit as i64, offset as i64], |row| {
            Ok(LibraryItem {
                id: row.get(0)?,
                created_at_ms: row.get(1)?,
                source_type: row.get(2)?,
                source_uri: row.get(3)?,
                title: row.get(4)?,
                media_path: row.get(5)?,
                duration_ms: row.get(6)?,
                width: row.get(7)?,
                height: row.get(8)?,
                container: row.get(9)?,
                video_codec: row.get(10)?,
                audio_codec: row.get(11)?,
                thumbnail_path: row.get(12)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(items)
}

pub fn get_item_by_id(paths: &AppPaths, item_id: &str) -> Result<LibraryItem> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;

    conn.query_row(
        r#"
SELECT
  id,
  created_at_ms,
  source_type,
  source_uri,
  title,
  media_path,
  duration_ms,
  width,
  height,
  container,
  video_codec,
  audio_codec,
  thumbnail_path
FROM library_item
WHERE id=?1
"#,
        params![item_id],
        |row| {
            Ok(LibraryItem {
                id: row.get(0)?,
                created_at_ms: row.get(1)?,
                source_type: row.get(2)?,
                source_uri: row.get(3)?,
                title: row.get(4)?,
                media_path: row.get(5)?,
                duration_ms: row.get(6)?,
                width: row.get(7)?,
                height: row.get(8)?,
                container: row.get(9)?,
                video_codec: row.get(10)?,
                audio_codec: row.get(11)?,
                thumbnail_path: row.get(12)?,
            })
        },
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => {
            crate::EngineError::InstallFailed(format!("library item not found: {item_id}"))
        }
        other => crate::EngineError::Database(other),
    })
}

pub fn import_local_file(paths: &AppPaths, input_path: &Path) -> Result<LibraryItem> {
    let input_path = input_path.canonicalize()?;
    let source_uri = input_path.to_string_lossy().to_string();
    import_media_file(paths, &input_path, "local_file", &source_uri, None)
}

pub fn import_downloaded_file(
    paths: &AppPaths,
    downloaded_path: &Path,
    source_url: &str,
    rights_note: &str,
    provider: &str,
    attested_at_ms: i64,
) -> Result<LibraryItem> {
    let downloaded_path = downloaded_path.canonicalize()?;
    let source_url = source_url.trim();
    let rights_note = rights_note.trim();
    let provider = provider.trim();
    let item = import_media_file(paths, &downloaded_path, "url_direct", source_url, None)?;

    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    conn.execute(
        r#"
INSERT INTO ingest_provenance (
  item_id,
  provider,
  source_url,
  rights_note,
  attested_at_ms,
  created_at_ms
) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
"#,
        params![
            &item.id,
            provider,
            source_url,
            rights_note,
            attested_at_ms,
            now_ms(),
        ],
    )?;

    Ok(item)
}

fn import_media_file(
    paths: &AppPaths,
    media_path: &Path,
    source_type: &str,
    source_uri: &str,
    title_hint: Option<&str>,
) -> Result<LibraryItem> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;

    let id = Uuid::new_v4().to_string();
    let created_at_ms = now_ms();
    let title = title_hint
        .and_then(|s| {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .or_else(|| {
            media_path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "Untitled".to_string());
    let media_path_str = media_path.to_string_lossy().to_string();

    let derived_dir = paths.derived_item_dir(&id);
    std::fs::create_dir_all(&derived_dir)?;

    // Import should remain possible even when ffmpeg/ffprobe is not installed. Metadata and
    // thumbnails are best-effort.
    let probe = match ffmpeg::probe(paths, media_path) {
        Ok(v) => v,
        Err(crate::EngineError::ExternalToolMissing { .. }) => ffmpeg::MediaProbe {
            duration_ms: None,
            container: None,
            video_codec: None,
            audio_codec: None,
            width: None,
            height: None,
        },
        Err(crate::EngineError::ExternalToolFailed { .. }) => ffmpeg::MediaProbe {
            duration_ms: None,
            container: None,
            video_codec: None,
            audio_codec: None,
            width: None,
            height: None,
        },
        Err(e) => return Err(e),
    };

    let thumbnail_path = derived_dir.join("thumb.jpg");
    let timestamp_seconds = match probe.duration_ms {
        Some(ms) if ms > 0 => {
            let dur_s = (ms as f64) / 1000.0;
            (dur_s * 0.10).min(5.0).max(0.0)
        }
        _ => 0.0,
    };

    let thumbnail_path_str = match ffmpeg::generate_thumbnail(paths, media_path, &thumbnail_path, timestamp_seconds) {
        Ok(()) => Some(thumbnail_path.to_string_lossy().to_string()),
        Err(crate::EngineError::ExternalToolMissing { .. }) => None,
        Err(crate::EngineError::ExternalToolFailed { .. }) => None,
        Err(_) => None,
    };

    conn.execute(
        r#"
INSERT INTO library_item (
  id,
  created_at_ms,
  source_type,
  source_uri,
  title,
  media_path,
  duration_ms,
  width,
  height,
  container,
  video_codec,
  audio_codec,
  thumbnail_path
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
"#,
        params![
            &id,
            created_at_ms,
            source_type,
            source_uri,
            &title,
            &media_path_str,
            probe.duration_ms,
            probe.width,
            probe.height,
            probe.container,
            probe.video_codec,
            probe.audio_codec,
            thumbnail_path_str,
        ],
    )?;

    Ok(LibraryItem {
        id,
        created_at_ms,
        source_type: source_type.to_string(),
        source_uri: source_uri.to_string(),
        title,
        media_path: media_path_str,
        duration_ms: probe.duration_ms,
        width: probe.width,
        height: probe.height,
        container: probe.container,
        video_codec: probe.video_codec,
        audio_codec: probe.audio_codec,
        thumbnail_path: thumbnail_path_str,
    })
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

pub fn derived_dir_for_item(paths: &AppPaths, item_id: &str) -> PathBuf {
    paths.derived_item_dir(item_id)
}
