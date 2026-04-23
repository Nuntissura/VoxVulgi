use crate::ffmpeg;
use crate::paths::AppPaths;
use crate::{db, Result};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

const THUMB_CACHE_MAX_BYTES: u64 = 512 * 1024 * 1024;
const THUMB_CACHE_MAX_AGE_DAYS: i64 = 45;

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

fn library_item_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<LibraryItem> {
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThumbnailCacheStatus {
    pub cache_dir: String,
    pub total_bytes: u64,
    pub total_files: usize,
    pub max_bytes: u64,
    pub max_age_days: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThumbnailCacheClearSummary {
    pub removed_entries: usize,
    pub removed_bytes: u64,
}

fn thumbnail_cache_path(paths: &AppPaths, item_id: &str) -> PathBuf {
    paths
        .thumbnail_cache_dir()
        .join(thumbnail_cache_file_name(item_id))
}

fn thumbnail_timestamp_seconds(duration_ms: Option<i64>) -> f64 {
    match duration_ms {
        Some(ms) if ms > 0 => {
            let dur_s = (ms as f64) / 1000.0;
            (dur_s * 0.10).min(5.0).max(0.0)
        }
        _ => 0.0,
    }
}

fn set_item_thumbnail_path(
    paths: &AppPaths,
    item_id: &str,
    thumbnail_path: Option<&Path>,
) -> Result<()> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    let stored = thumbnail_path.map(|value| value.to_string_lossy().to_string());
    conn.execute(
        "UPDATE library_item SET thumbnail_path=?1 WHERE id=?2",
        params![stored, item_id],
    )?;
    Ok(())
}

pub fn ensure_thumbnail_path(paths: &AppPaths, item_id: &str) -> Result<Option<PathBuf>> {
    paths.ensure_dirs()?;
    let item = get_item_by_id(paths, item_id)?;

    if let Some(existing) = item
        .thumbnail_path
        .as_deref()
        .map(PathBuf::from)
        .filter(|path| path.is_file())
    {
        return Ok(Some(existing));
    }

    let thumbnail_path = thumbnail_cache_path(paths, item_id);
    if thumbnail_path.is_file() {
        set_item_thumbnail_path(paths, item_id, Some(&thumbnail_path))?;
        return Ok(Some(thumbnail_path));
    }

    let media_path = PathBuf::from(item.media_path.trim());
    if !media_path.is_file() {
        if item.thumbnail_path.is_some() {
            set_item_thumbnail_path(paths, item_id, None)?;
        }
        return Ok(None);
    }

    match ffmpeg::generate_thumbnail(
        paths,
        &media_path,
        &thumbnail_path,
        thumbnail_timestamp_seconds(item.duration_ms),
    ) {
        Ok(()) => {
            set_item_thumbnail_path(paths, item_id, Some(&thumbnail_path))?;
            prune_thumbnail_cache(paths, THUMB_CACHE_MAX_BYTES, THUMB_CACHE_MAX_AGE_DAYS);
            Ok(Some(thumbnail_path))
        }
        Err(crate::EngineError::ExternalToolMissing { .. })
        | Err(crate::EngineError::ExternalToolFailed { .. }) => {
            if thumbnail_path.exists() {
                let _ = std::fs::remove_file(&thumbnail_path);
            }
            if item.thumbnail_path.is_some() {
                set_item_thumbnail_path(paths, item_id, None)?;
            }
            Ok(None)
        }
        Err(_) => {
            if thumbnail_path.exists() {
                let _ = std::fs::remove_file(&thumbnail_path);
            }
            if item.thumbnail_path.is_some() {
                set_item_thumbnail_path(paths, item_id, None)?;
            }
            Ok(None)
        }
    }
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
        .query_map(params![limit as i64, offset as i64], library_item_from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(items)
}

pub fn list_localization_workspace_items(
    paths: &AppPaths,
    limit: usize,
    offset: usize,
) -> Result<Vec<LibraryItem>> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;

    let mut stmt = conn.prepare(
        r#"
SELECT
  library_item.id,
  library_item.created_at_ms,
  library_item.source_type,
  library_item.source_uri,
  library_item.title,
  library_item.media_path,
  library_item.duration_ms,
  library_item.width,
  library_item.height,
  library_item.container,
  library_item.video_codec,
  library_item.audio_codec,
  library_item.thumbnail_path
FROM localization_workspace_item
JOIN library_item ON library_item.id = localization_workspace_item.item_id
ORDER BY localization_workspace_item.selected_at_ms DESC, library_item.created_at_ms DESC
LIMIT ?1 OFFSET ?2
"#,
    )?;

    let items = stmt
        .query_map(params![limit as i64, offset as i64], library_item_from_row)?
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
        library_item_from_row,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => {
            crate::EngineError::InstallFailed(format!("library item not found: {item_id}"))
        }
        other => crate::EngineError::Database(other),
    })
}

pub fn add_item_to_localization_workspace(
    paths: &AppPaths,
    item_id: &str,
    selection_source: &str,
    selection_path: Option<&str>,
) -> Result<()> {
    let item_id = item_id.trim();
    let selection_source = selection_source.trim();
    if item_id.is_empty() {
        return Err(crate::EngineError::InstallFailed(
            "item_id is required for localization workspace".to_string(),
        ));
    }
    if selection_source.is_empty() {
        return Err(crate::EngineError::InstallFailed(
            "selection_source is required for localization workspace".to_string(),
        ));
    }

    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    conn.execute(
        r#"
INSERT INTO localization_workspace_item (
  item_id,
  selected_at_ms,
  selection_source,
  selection_path
) VALUES (?1, ?2, ?3, ?4)
ON CONFLICT(item_id) DO UPDATE SET
  selected_at_ms=excluded.selected_at_ms,
  selection_source=excluded.selection_source,
  selection_path=excluded.selection_path
"#,
        params![
            item_id,
            now_ms(),
            selection_source,
            selection_path
                .map(|value| value.trim())
                .filter(|value| !value.is_empty()),
        ],
    )?;
    Ok(())
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

    let thumbnail_path = thumbnail_cache_path(paths, &id);
    let timestamp_seconds = thumbnail_timestamp_seconds(probe.duration_ms);

    let thumbnail_path_str =
        match ffmpeg::generate_thumbnail(paths, media_path, &thumbnail_path, timestamp_seconds) {
            Ok(()) => Some(thumbnail_path.to_string_lossy().to_string()),
            Err(crate::EngineError::ExternalToolMissing { .. }) => None,
            Err(crate::EngineError::ExternalToolFailed { .. }) => None,
            Err(_) => None,
        };
    prune_thumbnail_cache(paths, THUMB_CACHE_MAX_BYTES, THUMB_CACHE_MAX_AGE_DAYS);

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

pub fn thumbnail_cache_status(paths: &AppPaths) -> Result<ThumbnailCacheStatus> {
    paths.ensure_dirs()?;
    let cache_dir = paths.thumbnail_cache_dir();
    let mut total_bytes = 0_u64;
    let mut total_files = 0_usize;

    if cache_dir.exists() {
        let entries = std::fs::read_dir(&cache_dir)?;
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_file() {
                continue;
            }
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            total_files += 1;
            total_bytes = total_bytes.saturating_add(meta.len());
        }
    }

    Ok(ThumbnailCacheStatus {
        cache_dir: cache_dir.to_string_lossy().to_string(),
        total_bytes,
        total_files,
        max_bytes: THUMB_CACHE_MAX_BYTES,
        max_age_days: THUMB_CACHE_MAX_AGE_DAYS,
    })
}

pub fn clear_thumbnail_cache(paths: &AppPaths) -> Result<ThumbnailCacheClearSummary> {
    paths.ensure_dirs()?;
    let cache_dir = paths.thumbnail_cache_dir();
    if !cache_dir.exists() {
        return Ok(ThumbnailCacheClearSummary {
            removed_entries: 0,
            removed_bytes: 0,
        });
    }

    let mut removed_entries = 0_usize;
    let mut removed_bytes = 0_u64;
    let entries = std::fs::read_dir(&cache_dir)?;
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_file() {
            continue;
        }
        let bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
        if std::fs::remove_file(&path).is_ok() {
            removed_entries += 1;
            removed_bytes = removed_bytes.saturating_add(bytes);
        }
    }

    Ok(ThumbnailCacheClearSummary {
        removed_entries,
        removed_bytes,
    })
}

fn thumbnail_cache_file_name(item_id: &str) -> String {
    let mut out = String::with_capacity(item_id.len());
    for ch in item_id.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    let mut trimmed = out.trim_matches('_').to_string();
    if trimmed.is_empty() {
        trimmed = "item".to_string();
    }
    if trimmed.len() > 80 {
        trimmed.truncate(80);
    }
    format!("{trimmed}.jpg")
}

fn prune_thumbnail_cache(paths: &AppPaths, max_bytes: u64, max_age_days: i64) {
    let cache_dir = paths.thumbnail_cache_dir();
    if !cache_dir.exists() {
        return;
    }

    let now = SystemTime::now();
    let max_age_secs = (max_age_days.max(1) as u64)
        .saturating_mul(24)
        .saturating_mul(60)
        .saturating_mul(60);

    struct Entry {
        path: PathBuf,
        bytes: u64,
        modified: SystemTime,
    }

    let mut entries: Vec<Entry> = Vec::new();
    let mut total_bytes = 0_u64;

    let Ok(iter) = std::fs::read_dir(&cache_dir) else {
        return;
    };
    for entry in iter.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_file() {
            continue;
        }
        let path = entry.path();
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        let modified = meta.modified().unwrap_or(UNIX_EPOCH);
        let age_secs = now
            .duration_since(modified)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if age_secs > max_age_secs {
            let _ = std::fs::remove_file(&path);
            continue;
        }
        let bytes = meta.len();
        total_bytes = total_bytes.saturating_add(bytes);
        entries.push(Entry {
            path,
            bytes,
            modified,
        });
    }

    if total_bytes <= max_bytes {
        return;
    }

    entries.sort_by_key(|entry| entry.modified);
    for entry in entries {
        if total_bytes <= max_bytes {
            break;
        }
        if std::fs::remove_file(&entry.path).is_ok() {
            total_bytes = total_bytes.saturating_sub(entry.bytes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use filetime::{set_file_mtime, FileTime};
    use rusqlite::params;

    #[test]
    fn thumbnail_cache_file_name_is_sanitized() {
        let key = thumbnail_cache_file_name("  ab/cd:ef?gh  ");
        assert_eq!(key, "ab_cd_ef_gh.jpg");
    }

    #[test]
    fn prune_thumbnail_cache_evicts_oldest_first() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        paths.ensure_dirs().expect("dirs");
        let cache = paths.thumbnail_cache_dir();

        let old = cache.join("old.jpg");
        let mid = cache.join("mid.jpg");
        let fresh = cache.join("fresh.jpg");
        std::fs::write(&old, vec![1_u8; 60]).expect("old");
        std::fs::write(&mid, vec![2_u8; 60]).expect("mid");
        std::fs::write(&fresh, vec![3_u8; 60]).expect("fresh");

        let now = std::time::SystemTime::now();
        set_file_mtime(
            &old,
            FileTime::from_system_time(
                now.checked_sub(std::time::Duration::from_secs(300))
                    .expect("old ts"),
            ),
        )
        .expect("set old");
        set_file_mtime(
            &mid,
            FileTime::from_system_time(
                now.checked_sub(std::time::Duration::from_secs(200))
                    .expect("mid ts"),
            ),
        )
        .expect("set mid");
        set_file_mtime(
            &fresh,
            FileTime::from_system_time(
                now.checked_sub(std::time::Duration::from_secs(100))
                    .expect("fresh ts"),
            ),
        )
        .expect("set fresh");

        prune_thumbnail_cache(&paths, 120, 3650);

        assert!(
            !old.exists(),
            "oldest file should be evicted first when over cache budget"
        );
        assert!(mid.exists(), "newer file should remain");
        assert!(fresh.exists(), "newest file should remain");
    }

    #[test]
    fn ensure_thumbnail_path_reuses_cached_file_and_updates_db() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        paths.ensure_dirs().expect("dirs");
        db::ensure_schema(&paths).expect("schema");

        let item_id = "item-thumb-cache";
        let media_path = dir.path().join("sample.mp4");
        std::fs::write(&media_path, b"not-a-real-video").expect("media");
        let cached_thumb = thumbnail_cache_path(&paths, item_id);
        std::fs::write(&cached_thumb, b"jpeg").expect("thumb");

        let conn = db::open(&paths).expect("db");
        db::migrate(&conn).expect("migrate");
        conn.execute(
            r#"
INSERT INTO library_item (
  id, created_at_ms, source_type, source_uri, title, media_path,
  duration_ms, width, height, container, video_codec, audio_codec, thumbnail_path
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, NULL, NULL, NULL, NULL, NULL, NULL)
"#,
            params![
                item_id,
                1_i64,
                "local_file",
                media_path.to_string_lossy().to_string(),
                "Sample",
                media_path.to_string_lossy().to_string(),
            ],
        )
        .expect("insert");

        let resolved = ensure_thumbnail_path(&paths, item_id)
            .expect("resolve")
            .expect("thumbnail");
        assert_eq!(resolved, cached_thumb);

        let stored: Option<String> = conn
            .query_row(
                "SELECT thumbnail_path FROM library_item WHERE id=?1",
                [item_id],
                |row| row.get(0),
            )
            .expect("stored path");
        assert_eq!(
            stored.as_deref(),
            Some(cached_thumb.to_string_lossy().as_ref())
        );
    }

    #[test]
    fn ensure_thumbnail_path_clears_stale_reference_when_media_missing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        paths.ensure_dirs().expect("dirs");
        db::ensure_schema(&paths).expect("schema");

        let item_id = "item-thumb-missing";
        let missing_media = dir.path().join("missing.mp4");
        let stale_thumb = dir.path().join("stale.jpg");

        let conn = db::open(&paths).expect("db");
        db::migrate(&conn).expect("migrate");
        conn.execute(
            r#"
INSERT INTO library_item (
  id, created_at_ms, source_type, source_uri, title, media_path,
  duration_ms, width, height, container, video_codec, audio_codec, thumbnail_path
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, NULL, NULL, NULL, NULL, NULL, ?7)
"#,
            params![
                item_id,
                1_i64,
                "local_file",
                missing_media.to_string_lossy().to_string(),
                "Missing",
                missing_media.to_string_lossy().to_string(),
                stale_thumb.to_string_lossy().to_string(),
            ],
        )
        .expect("insert");

        let resolved = ensure_thumbnail_path(&paths, item_id).expect("resolve");
        assert!(
            resolved.is_none(),
            "missing media should not yield a thumbnail"
        );

        let stored: Option<String> = conn
            .query_row(
                "SELECT thumbnail_path FROM library_item WHERE id=?1",
                [item_id],
                |row| row.get(0),
            )
            .expect("stored path");
        assert!(
            stored.is_none(),
            "stale thumbnail reference should be cleared"
        );
    }
}
