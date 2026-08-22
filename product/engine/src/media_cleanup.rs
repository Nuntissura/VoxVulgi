use crate::paths::AppPaths;
use crate::{db, library, root_rebind, EngineError, Result};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::{BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[cfg(windows)]
use std::os::windows::io::AsRawHandle;
#[cfg(windows)]
use windows_sys::Win32::Storage::FileSystem::{
    GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
};

const HASH_WINDOW_BYTES: usize = 64 * 1024;

#[cfg(test)]
thread_local! {
    static FORCE_CLEANUP_APPLY_COMPENSATION_FAILURE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaCleanupRun {
    pub id: String,
    pub roots: Vec<String>,
    pub quarantine_root: Option<String>,
    pub status: String,
    pub stage: String,
    pub files_scanned: usize,
    pub bytes_scanned: u64,
    pub duplicate_groups: usize,
    pub reclaimable_bytes: u64,
    pub last_error: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaCleanupAdvanceSummary {
    pub run: MediaCleanupRun,
    pub processed_files: usize,
    pub remaining_inventory_entries: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaCleanupGroup {
    pub group_id: String,
    pub full_sha256: String,
    pub size_bytes: u64,
    pub member_count: usize,
    pub keeper_path: String,
    pub keeper_library_item_id: Option<String>,
    pub reclaimable_bytes: u64,
    pub decision: String,
    pub members: Vec<MediaCleanupGroupMember>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaCleanupGroupMember {
    pub path: String,
    pub library_item_id: Option<String>,
    pub media_id: Option<String>,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaCleanupApplySummary {
    pub run_id: String,
    pub approved_groups: usize,
    pub applied_actions: usize,
    pub failed_actions: usize,
    pub bytes_quarantined: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaCleanupReconciliationCandidate {
    pub candidate_id: String,
    pub kind: String,
    pub physical_path: Option<String>,
    pub library_item_id: Option<String>,
    pub library_path: Option<String>,
    pub evidence_kind: String,
    pub evidence_value: Option<String>,
    pub disposition: String,
    pub destination_library_item_id: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaCleanupReconciliationSummary {
    pub run_id: String,
    pub candidates: Vec<MediaCleanupReconciliationCandidate>,
    pub deterministic_relinks: usize,
    pub physical_files_to_index: usize,
    pub review_only: usize,
    pub applied: usize,
    pub failed: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaCleanupVariant {
    pub variant_id: String,
    pub service: String,
    pub media_id: String,
    pub member_paths: Vec<String>,
    pub evidence: serde_json::Value,
    pub status: String,
}

#[derive(Debug, Clone)]
struct CleanupFileRow {
    path: String,
    size_bytes: i64,
    modified_ms: i64,
    library_item_id: Option<String>,
    media_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CleanupRelinkJournal {
    version: u8,
    #[serde(default)]
    media_ids: Vec<String>,
    #[serde(default)]
    source_library_media_path: Option<String>,
    #[serde(default)]
    source_library_paths: Vec<CleanupLibraryPathJournal>,
    #[serde(default)]
    identities: Vec<CleanupIdentityRelinkJournal>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct CleanupLibraryPathJournal {
    library_item_id: String,
    original_media_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct CleanupIdentityRelinkJournal {
    service: String,
    media_id: String,
    source_library_item_id: String,
}

pub fn create_inventory_run(
    paths: &AppPaths,
    roots: Vec<String>,
    quarantine_root: Option<String>,
) -> Result<MediaCleanupRun> {
    let mut normalized_roots = Vec::new();
    for root in roots {
        let trimmed = root.trim();
        if trimmed.is_empty() {
            continue;
        }
        let path = PathBuf::from(trimmed);
        if !path.is_dir() {
            return Err(EngineError::InstallFailed(format!(
                "cleanup inventory root is not an available directory: {}",
                path.to_string_lossy()
            )));
        }
        normalized_roots.push(path.to_string_lossy().to_string());
    }
    normalized_roots.sort_by_key(|value| value.to_ascii_lowercase());
    normalized_roots.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    if normalized_roots.is_empty() {
        return Err(EngineError::InstallFailed(
            "at least one available cleanup inventory root is required".to_string(),
        ));
    }
    let quarantine_root = quarantine_root
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if let Some(quarantine) = quarantine_root.as_deref() {
        let quarantine_path = Path::new(quarantine);
        if normalized_roots
            .iter()
            .any(|root| paths_overlap(paths, Path::new(root), quarantine_path))
        {
            return Err(EngineError::InstallFailed(
                "quarantine folder must be outside every inventory root".to_string(),
            ));
        }
    }

    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    let id = Uuid::new_v4().to_string();
    let now = now_ms();
    conn.execute(
        r#"
INSERT INTO media_cleanup_run (
  id, roots_json, scan_queue_json, quarantine_root, status, stage,
  files_scanned, bytes_scanned, duplicate_groups, reclaimable_bytes,
  last_error, created_at_ms, updated_at_ms
) VALUES (?1, ?2, ?2, ?3, 'running', 'inventory', 0, 0, 0, 0, NULL, ?4, ?4)
"#,
        params![
            id,
            serde_json::to_string(&normalized_roots)?,
            quarantine_root,
            now
        ],
    )?;
    get_run_conn(&conn, &id)?.ok_or_else(|| {
        EngineError::InstallFailed("cleanup inventory run was not persisted".to_string())
    })
}

pub fn get_run(paths: &AppPaths, run_id: &str) -> Result<Option<MediaCleanupRun>> {
    let conn = db::open_readonly(paths)?;
    get_run_conn(&conn, run_id)
}

/// Canonical restart identity for cleanup work. The SQLite run row, not a browser-local
/// projection, owns recovery after a frontend restart or failed localStorage write.
pub fn latest_run(paths: &AppPaths) -> Result<Option<MediaCleanupRun>> {
    let conn = db::open_readonly(paths)?;
    let run_id = conn
        .query_row(
            "SELECT id FROM media_cleanup_run ORDER BY created_at_ms DESC, id DESC LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    match run_id {
        Some(run_id) => get_run_conn(&conn, &run_id),
        None => Ok(None),
    }
}

pub fn advance_inventory(
    paths: &AppPaths,
    run_id: &str,
    max_files: Option<usize>,
) -> Result<MediaCleanupAdvanceSummary> {
    let max_files = max_files.unwrap_or(500).clamp(1, 10_000);
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    let row = conn
        .query_row(
            "SELECT scan_queue_json, stage FROM media_cleanup_run WHERE id=?1",
            [run_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?
        .ok_or_else(|| EngineError::InstallFailed("cleanup run not found".to_string()))?;
    if row.1 != "inventory" {
        return Ok(MediaCleanupAdvanceSummary {
            run: get_run_conn(&conn, run_id)?.expect("run exists"),
            processed_files: 0,
            remaining_inventory_entries: serde_json::from_str::<Vec<String>>(&row.0)
                .unwrap_or_default()
                .len(),
        });
    }
    if cleanup_active_job_count(&conn)? > 0 {
        conn.execute(
            "UPDATE media_cleanup_run SET status='paused', updated_at_ms=?1 WHERE id=?2",
            params![now_ms(), run_id],
        )?;
        return Ok(MediaCleanupAdvanceSummary {
            run: get_run_conn(&conn, run_id)?.expect("run exists"),
            processed_files: 0,
            remaining_inventory_entries: serde_json::from_str::<Vec<String>>(&row.0)
                .unwrap_or_default()
                .len(),
        });
    }
    let mut queue: Vec<String> = serde_json::from_str(&row.0)?;
    let library_by_path = library_identity_by_normalized_path(paths, &conn)?;
    let mut processed_files = 0_usize;
    let mut bytes_scanned = 0_u64;

    while processed_files < max_files {
        let Some(current) = queue.pop() else {
            break;
        };
        let path = PathBuf::from(&current);
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&path) {
                let mut children = entries
                    .filter_map(|entry| entry.ok())
                    .map(|entry| entry.path().to_string_lossy().to_string())
                    .collect::<Vec<_>>();
                children.sort_by_key(|value| value.to_ascii_lowercase());
                queue.extend(children.into_iter().rev());
            }
            continue;
        }
        if !metadata.is_file() || !is_media_file(&path) {
            continue;
        }
        processed_files += 1;
        bytes_scanned = bytes_scanned.saturating_add(metadata.len());
        let canonical_path = path.canonicalize().unwrap_or(path);
        let path_text = canonical_path.to_string_lossy().to_string();
        let normalized = normalize_path_key(&path_text);
        let (library_item_id, media_id) = library_by_path
            .get(&normalized)
            .cloned()
            .unwrap_or((None, None));
        let modified_ms = modified_ms(&metadata);
        let file_identity = native_file_identity(&canonical_path).ok();
        conn.execute(
            r#"
INSERT INTO media_cleanup_file (
  run_id, path, size_bytes, modified_ms, file_identity, library_item_id, media_id, state, updated_at_ms
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'inventoried', ?8)
ON CONFLICT(run_id, path) DO UPDATE SET
  size_bytes=excluded.size_bytes,
  modified_ms=excluded.modified_ms,
  file_identity=excluded.file_identity,
  library_item_id=excluded.library_item_id,
  media_id=excluded.media_id,
  prefix_sha256=NULL,
  suffix_sha256=NULL,
  full_sha256=NULL,
  group_id=NULL,
  state='inventoried',
  last_error=NULL,
  updated_at_ms=excluded.updated_at_ms
"#,
            params![
                run_id,
                path_text,
                i64::try_from(metadata.len()).unwrap_or(i64::MAX),
                modified_ms,
                file_identity,
                library_item_id,
                media_id,
                now_ms()
            ],
        )?;
    }
    let stage = if queue.is_empty() {
        "reconciliation"
    } else {
        "inventory"
    };
    conn.execute(
        r#"
UPDATE media_cleanup_run SET
  scan_queue_json=?1,
  stage=?2,
  status='paused',
  files_scanned=files_scanned+?3,
  bytes_scanned=bytes_scanned+?4,
  updated_at_ms=?5
WHERE id=?6
"#,
        params![
            serde_json::to_string(&queue)?,
            stage,
            i64::try_from(processed_files).unwrap_or(i64::MAX),
            i64::try_from(bytes_scanned).unwrap_or(i64::MAX),
            now_ms(),
            run_id
        ],
    )?;
    Ok(MediaCleanupAdvanceSummary {
        run: get_run_conn(&conn, run_id)?.expect("run exists"),
        processed_files,
        remaining_inventory_entries: queue.len(),
    })
}

pub fn reconciliation_preview(
    paths: &AppPaths,
    run_id: &str,
) -> Result<MediaCleanupReconciliationSummary> {
    let mut conn = db::open(paths)?;
    db::migrate(&conn)?;
    let stage: String = conn
        .query_row(
            "SELECT stage FROM media_cleanup_run WHERE id=?1",
            [run_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| EngineError::InstallFailed("cleanup run not found".to_string()))?;
    if stage == "inventory" {
        return Err(EngineError::InstallFailed(
            "finish inventory before preparing reconciliation".to_string(),
        ));
    }
    let existing: i64 = conn.query_row(
        "SELECT COUNT(*) FROM media_cleanup_reconciliation_candidate WHERE run_id=?1",
        [run_id],
        |row| row.get(0),
    )?;
    if existing == 0 && stage == "reconciliation" {
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let still_empty: i64 = tx.query_row(
            "SELECT COUNT(*) FROM media_cleanup_reconciliation_candidate WHERE run_id=?1",
            [run_id],
            |row| row.get(0),
        )?;
        if still_empty == 0 {
            prepare_reconciliation_candidates(paths, &tx, run_id)?;
        }
        tx.commit()?;
    }
    reconciliation_summary_conn(&conn, run_id)
}

pub fn apply_reconciliation(
    paths: &AppPaths,
    run_id: &str,
) -> Result<MediaCleanupReconciliationSummary> {
    let mut conn = db::open(paths)?;
    db::migrate(&conn)?;
    let preview = reconciliation_preview(paths, run_id)?;
    ensure_cleanup_apply_boundary(&conn)?;
    let mut failed = 0_usize;

    for candidate in preview
        .candidates
        .iter()
        .filter(|row| row.disposition == "deterministic_relink")
    {
        let result = (|| -> Result<()> {
            let physical_path = candidate.physical_path.as_deref().ok_or_else(|| {
                EngineError::InstallFailed("relink candidate has no physical path".to_string())
            })?;
            let library_item_id = candidate.library_item_id.as_deref().ok_or_else(|| {
                EngineError::InstallFailed("relink candidate has no library item".to_string())
            })?;
            let old_path = candidate.library_path.as_deref().ok_or_else(|| {
                EngineError::InstallFailed("relink candidate has no prior library path".to_string())
            })?;
            verify_inventoried_file_unchanged(&conn, run_id, physical_path)?;
            match std::fs::metadata(old_path) {
                Ok(old_metadata) if old_metadata.is_file() && old_metadata.len() == 0 => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Ok(_) => {
                    return Err(EngineError::InstallFailed(format!(
                        "reconciliation refused because the old library path is no longer missing or zero-byte: {old_path}"
                    )));
                }
                Err(error) => {
                    return Err(EngineError::InstallFailed(format!(
                        "reconciliation could not prove the old library path missing: {old_path}: {error}"
                    )));
                }
            }
            let tx = begin_cleanup_apply_transaction(&mut conn)?;
            verify_inventoried_file_unchanged(&tx, run_id, physical_path)?;
            let current_path = library_item_media_path(&tx, library_item_id)?.ok_or_else(|| {
                EngineError::InstallFailed(format!(
                    "reconciliation library item is missing: {library_item_id}"
                ))
            })?;
            if current_path != old_path {
                return Err(EngineError::InstallFailed(format!(
                    "reconciliation library path changed after preview: item={library_item_id}; expected={old_path}; current={current_path}"
                )));
            }
            let changed = tx.execute(
                "UPDATE library_item SET media_path=?1 WHERE id=?2 AND media_path=?3",
                params![physical_path, library_item_id, old_path],
            )?;
            if changed != 1 {
                return Err(EngineError::InstallFailed(format!(
                    "reconciliation relink lost its canonical owner: {library_item_id}"
                )));
            }
            tx.execute(
                "UPDATE media_cleanup_file SET library_item_id=?1, updated_at_ms=?2 WHERE run_id=?3 AND path=?4",
                params![library_item_id, now_ms(), run_id, physical_path],
            )?;
            library::persist_media_path_observation_rewrite_invalidation(
                &tx,
                old_path,
                physical_path,
            )?;
            tx.execute(
                "UPDATE media_cleanup_reconciliation_candidate SET disposition='applied', destination_library_item_id=?1, applied_at_ms=?2, error=NULL, updated_at_ms=?2 WHERE run_id=?3 AND candidate_id=?4 AND disposition='deterministic_relink'",
                params![library_item_id, now_ms(), run_id, candidate.candidate_id],
            )?;
            tx.commit()?;
            library::invalidate_media_path_observation_rewrite_memory(old_path, physical_path);
            Ok(())
        })();
        if let Err(error) = result {
            failed += 1;
            conn.execute(
                "UPDATE media_cleanup_reconciliation_candidate SET error=?1, updated_at_ms=?2 WHERE run_id=?3 AND candidate_id=?4 AND disposition='deterministic_relink'",
                params![error.to_string(), now_ms(), run_id, candidate.candidate_id],
            )?;
        }
    }

    for candidate in preview
        .candidates
        .iter()
        .filter(|row| row.disposition == "index_new")
    {
        let result = (|| -> Result<String> {
            let physical_path = candidate.physical_path.as_deref().ok_or_else(|| {
                EngineError::InstallFailed("index candidate has no physical path".to_string())
            })?;
            let canonical_path = Path::new(physical_path).canonicalize()?;
            let canonical_path_string = canonical_path.to_string_lossy().to_string();
            verify_inventoried_file_unchanged(&conn, run_id, physical_path)?;
            let existing = library_items_for_media_path(paths, &conn, &canonical_path_string)?
                .into_iter()
                .next()
                .map(|item| item.library_item_id);
            let prepared_item = match existing {
                Some(_) => None,
                None => Some(library::prepare_media_item(
                    paths,
                    &canonical_path,
                    "local_file",
                    &canonical_path_string,
                    None,
                )?),
            };
            let tx = begin_cleanup_apply_transaction(&mut conn)?;
            verify_inventoried_file_unchanged(&tx, run_id, physical_path)?;
            let current_existing =
                library_items_for_media_path(paths, &tx, &canonical_path_string)?
                    .into_iter()
                    .next()
                    .map(|item| item.library_item_id);
            let item_id = if let Some(item_id) = current_existing {
                item_id
            } else {
                let item = prepared_item.as_ref().ok_or_else(|| {
                    EngineError::InstallFailed(
                        "reconciliation import lost its prepared media item".to_string(),
                    )
                })?;
                library::insert_library_item(&tx, item)?;
                item.id.clone()
            };
            tx.execute(
                "UPDATE media_cleanup_file SET library_item_id=?1, updated_at_ms=?2 WHERE run_id=?3 AND path=?4",
                params![item_id, now_ms(), run_id, physical_path],
            )?;
            tx.execute(
                "UPDATE media_cleanup_reconciliation_candidate SET disposition='applied', destination_library_item_id=?1, applied_at_ms=?2, error=NULL, updated_at_ms=?2 WHERE run_id=?3 AND candidate_id=?4 AND disposition='index_new'",
                params![item_id, now_ms(), run_id, candidate.candidate_id],
            )?;
            tx.commit()?;
            Ok(item_id)
        })();
        match result {
            Ok(_) => {}
            Err(error) => {
                failed += 1;
                conn.execute(
                    "UPDATE media_cleanup_reconciliation_candidate SET error=?1, updated_at_ms=?2 WHERE run_id=?3 AND candidate_id=?4 AND disposition='index_new'",
                    params![error.to_string(), now_ms(), run_id, candidate.candidate_id],
                )?;
            }
        }
    }

    conn.execute(
        "UPDATE media_cleanup_run SET stage=CASE WHEN ?1=0 THEN 'hashing' ELSE 'reconciliation' END, status=CASE WHEN ?1=0 THEN 'running' ELSE 'attention' END, updated_at_ms=?2 WHERE id=?3",
        params![i64::try_from(failed).unwrap_or(i64::MAX), now_ms(), run_id],
    )?;
    reconciliation_summary_conn(&conn, run_id)
}

fn prepare_reconciliation_candidates(
    paths: &AppPaths,
    conn: &rusqlite::Connection,
    run_id: &str,
) -> Result<()> {
    #[derive(Clone)]
    struct PhysicalRow {
        path: String,
        library_item_id: Option<String>,
        size_bytes: i64,
        file_identity: Option<String>,
        youtube_id: Option<String>,
        filename_key: String,
    }
    #[derive(Clone)]
    struct LibraryRow {
        id: String,
        media_path: String,
        youtube_ids: Vec<String>,
        filename_key: String,
    }

    let roots_json: String = conn.query_row(
        "SELECT roots_json FROM media_cleanup_run WHERE id=?1",
        [run_id],
        |row| row.get(0),
    )?;
    let roots = serde_json::from_str::<Vec<String>>(&roots_json)?;
    let mut physical_stmt = conn.prepare(
        "SELECT path,library_item_id,size_bytes,file_identity FROM media_cleanup_file WHERE run_id=?1 ORDER BY path",
    )?;
    let physical_rows = physical_stmt
        .query_map([run_id], |row| {
            let path = row.get::<_, String>(0)?;
            Ok(PhysicalRow {
                youtube_id: youtube_id_from_media_filename(&path),
                filename_key: media_filename_key(&path),
                path,
                library_item_id: row.get(1)?,
                size_bytes: row.get(2)?,
                file_identity: row.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut library_stmt = conn.prepare("SELECT id,media_path FROM library_item ORDER BY id")?;
    let base_library_rows = library_stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut identity_stmt = conn.prepare(
        "SELECT media_id FROM media_source_identity WHERE service='youtube' AND library_item_id=?1 ORDER BY media_id",
    )?;
    let mut library_rows = Vec::new();
    for (id, media_path) in base_library_rows {
        if !path_within_roots(paths, &media_path, &roots) {
            continue;
        }
        let youtube_ids = identity_stmt
            .query_map([&id], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        library_rows.push(LibraryRow {
            filename_key: media_filename_key(&media_path),
            id,
            media_path,
            youtube_ids,
        });
    }

    let mut physical_by_youtube: HashMap<String, Vec<String>> = HashMap::new();
    let mut physical_by_filename: HashMap<String, Vec<String>> = HashMap::new();
    for row in physical_rows
        .iter()
        .filter(|row| row.library_item_id.is_none())
    {
        if let Some(media_id) = row.youtube_id.as_ref() {
            physical_by_youtube
                .entry(media_id.clone())
                .or_default()
                .push(row.path.clone());
            conn.execute(
                "UPDATE media_cleanup_file SET media_id=?1, updated_at_ms=?2 WHERE run_id=?3 AND path=?4 AND media_id IS NULL",
                params![media_id, now_ms(), run_id, row.path],
            )?;
        }
        physical_by_filename
            .entry(row.filename_key.clone())
            .or_default()
            .push(row.path.clone());
    }
    let mut library_by_youtube: HashMap<String, Vec<String>> = HashMap::new();
    let mut library_by_filename: HashMap<String, Vec<String>> = HashMap::new();
    for row in &library_rows {
        for media_id in &row.youtube_ids {
            library_by_youtube
                .entry(media_id.clone())
                .or_default()
                .push(row.id.clone());
        }
        library_by_filename
            .entry(row.filename_key.clone())
            .or_default()
            .push(row.id.clone());
    }

    let mut matched_library_items = HashSet::new();
    let mut ambiguous_library_items = HashSet::new();
    let now = now_ms();
    for row in physical_rows
        .iter()
        .filter(|row| row.library_item_id.is_none())
    {
        let (evidence_kind, evidence_value, candidates, physical_matches) = row
            .youtube_id
            .as_ref()
            .filter(|id| library_by_youtube.contains_key(*id))
            .map(|id| {
                (
                    "youtube_id".to_string(),
                    Some(id.clone()),
                    library_by_youtube.get(id).cloned().unwrap_or_default(),
                    physical_by_youtube.get(id).map_or(0, Vec::len),
                )
            })
            .unwrap_or_else(|| {
                (
                    "exact_filename".to_string(),
                    Some(row.filename_key.clone()),
                    library_by_filename
                        .get(&row.filename_key)
                        .cloned()
                        .unwrap_or_default(),
                    physical_by_filename
                        .get(&row.filename_key)
                        .map_or(0, Vec::len),
                )
            });
        if candidates.len() != 1 || physical_matches != 1 {
            ambiguous_library_items.extend(candidates.iter().cloned());
        }
        let unique_library = (candidates.len() == 1 && physical_matches == 1)
            .then(|| library_rows.iter().find(|item| item.id == candidates[0]))
            .flatten();
        let (library_item_id, library_path, kind, disposition) = if let Some(item) = unique_library
        {
            matched_library_items.insert(item.id.clone());
            if row.file_identity.is_none() {
                (
                    Some(item.id.clone()),
                    Some(item.media_path.clone()),
                    "file_identity_unavailable".to_string(),
                    "review_only".to_string(),
                )
            } else {
                match std::fs::metadata(&item.media_path) {
                    Ok(metadata) if metadata.is_file() && metadata.len() == 0 => (
                        Some(item.id.clone()),
                        Some(item.media_path.clone()),
                        "zero_byte_library_path".to_string(),
                        "deterministic_relink".to_string(),
                    ),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => (
                        Some(item.id.clone()),
                        Some(item.media_path.clone()),
                        "missing_library_path".to_string(),
                        "deterministic_relink".to_string(),
                    ),
                    Ok(_) => (
                        Some(item.id.clone()),
                        Some(item.media_path.clone()),
                        "physical_and_library_both_present".to_string(),
                        "review_only".to_string(),
                    ),
                    Err(_) => (
                        Some(item.id.clone()),
                        Some(item.media_path.clone()),
                        "library_path_unavailable".to_string(),
                        "review_only".to_string(),
                    ),
                }
            }
        } else if candidates.is_empty() {
            if row.file_identity.is_some() {
                (
                    None,
                    None,
                    "physical_only".to_string(),
                    "index_new".to_string(),
                )
            } else {
                (
                    None,
                    None,
                    "file_identity_unavailable".to_string(),
                    "review_only".to_string(),
                )
            }
        } else {
            (
                None,
                None,
                "ambiguous_match".to_string(),
                "review_only".to_string(),
            )
        };
        insert_reconciliation_candidate(
            conn,
            run_id,
            &stable_cleanup_candidate_id(run_id, &row.path, library_item_id.as_deref()),
            &kind,
            Some(&row.path),
            library_item_id.as_deref(),
            library_path.as_deref(),
            &evidence_kind,
            evidence_value.as_deref(),
            &disposition,
            now,
        )?;
    }

    let inventoried_keys = physical_rows
        .iter()
        .flat_map(|row| path_identity_keys(paths, &row.path))
        .collect::<HashSet<_>>();
    for row in library_rows
        .iter()
        .filter(|row| !matched_library_items.contains(&row.id))
    {
        let library_keys = path_identity_keys(paths, &row.media_path);
        let inventoried = library_keys
            .iter()
            .any(|key| inventoried_keys.contains(key));
        let observation = conn
            .query_row(
                "SELECT state FROM media_availability_observation WHERE path=?1 AND invalidated_at_ms IS NULL ORDER BY observed_at_ms DESC LIMIT 1",
                [&row.media_path],
                |result| result.get::<_, String>(0),
            )
            .optional()?;
        let zero_byte = physical_rows.iter().any(|physical| {
            physical.size_bytes == 0
                && path_identity_keys(paths, &physical.path)
                    .iter()
                    .any(|key| library_keys.contains(key))
        });
        if !ambiguous_library_items.contains(&row.id)
            && !zero_byte
            && (inventoried || observation.as_deref() != Some("missing"))
        {
            continue;
        }
        insert_reconciliation_candidate(
            conn,
            run_id,
            &stable_cleanup_candidate_id(run_id, &row.media_path, Some(&row.id)),
            if zero_byte {
                "zero_byte_library_path"
            } else {
                "missing_library_path"
            },
            None,
            Some(&row.id),
            Some(&row.media_path),
            "no_unique_physical_match",
            None,
            "review_only",
            now,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn insert_reconciliation_candidate(
    conn: &rusqlite::Connection,
    run_id: &str,
    candidate_id: &str,
    kind: &str,
    physical_path: Option<&str>,
    library_item_id: Option<&str>,
    library_path: Option<&str>,
    evidence_kind: &str,
    evidence_value: Option<&str>,
    disposition: &str,
    now: i64,
) -> Result<()> {
    conn.execute(
        "INSERT INTO media_cleanup_reconciliation_candidate(run_id,candidate_id,kind,physical_path,library_item_id,library_path,evidence_kind,evidence_value,disposition,created_at_ms,updated_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?10)",
        params![run_id,candidate_id,kind,physical_path,library_item_id,library_path,evidence_kind,evidence_value,disposition,now],
    )?;
    Ok(())
}

fn reconciliation_summary_conn(
    conn: &rusqlite::Connection,
    run_id: &str,
) -> Result<MediaCleanupReconciliationSummary> {
    let mut stmt = conn.prepare(
        "SELECT candidate_id,kind,physical_path,library_item_id,library_path,evidence_kind,evidence_value,disposition,destination_library_item_id,error FROM media_cleanup_reconciliation_candidate WHERE run_id=?1 ORDER BY disposition,candidate_id",
    )?;
    let candidates = stmt
        .query_map([run_id], |row| {
            Ok(MediaCleanupReconciliationCandidate {
                candidate_id: row.get(0)?,
                kind: row.get(1)?,
                physical_path: row.get(2)?,
                library_item_id: row.get(3)?,
                library_path: row.get(4)?,
                evidence_kind: row.get(5)?,
                evidence_value: row.get(6)?,
                disposition: row.get(7)?,
                destination_library_item_id: row.get(8)?,
                error: row.get(9)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(MediaCleanupReconciliationSummary {
        run_id: run_id.to_string(),
        deterministic_relinks: candidates
            .iter()
            .filter(|row| row.disposition == "deterministic_relink")
            .count(),
        physical_files_to_index: candidates
            .iter()
            .filter(|row| row.disposition == "index_new")
            .count(),
        review_only: candidates
            .iter()
            .filter(|row| row.disposition == "review_only")
            .count(),
        applied: candidates
            .iter()
            .filter(|row| row.disposition == "applied")
            .count(),
        failed: candidates
            .iter()
            .filter(|row| row.error.is_some() && row.disposition != "applied")
            .count(),
        candidates,
    })
}

fn stable_cleanup_candidate_id(run_id: &str, physical_path: &str, item_id: Option<&str>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(run_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(normalize_path_key(physical_path).as_bytes());
    hasher.update(b"\0");
    hasher.update(item_id.unwrap_or_default().as_bytes());
    format!("reconcile-{}", &hex::encode(hasher.finalize())[..24])
}

fn media_filename_key(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
}

fn youtube_id_from_media_filename(path: &str) -> Option<String> {
    let stem = Path::new(path).file_stem()?.to_str()?.trim();
    let terminal = stem.strip_suffix(']').unwrap_or(stem);
    if terminal.len() < 11 || !terminal.is_char_boundary(terminal.len() - 11) {
        return None;
    }
    let candidate = &terminal[terminal.len() - 11..];
    if !candidate
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '-' || character == '_')
    {
        return None;
    }
    let prefix = &terminal[..terminal.len() - 11];
    if prefix.is_empty()
        || prefix
            .chars()
            .last()
            .is_some_and(|character| !character.is_ascii_alphanumeric())
    {
        Some(candidate.to_string())
    } else {
        None
    }
}

fn path_identity_keys(paths: &AppPaths, path: &str) -> Vec<String> {
    let mut keys = vec![normalize_path_key(path)];
    if let Ok(resolved) = root_rebind::resolve_active_alias_path(paths, Path::new(path), false) {
        keys.push(normalize_path_key(&resolved.to_string_lossy()));
    }
    if let Ok(canonical) = Path::new(path).canonicalize() {
        keys.push(normalize_path_key(&canonical.to_string_lossy()));
    }
    keys.sort();
    keys.dedup();
    keys
}

fn path_within_roots(paths: &AppPaths, candidate: &str, roots: &[String]) -> bool {
    let candidate_keys = path_identity_keys(paths, candidate);
    roots.iter().any(|root| {
        path_identity_keys(paths, root).iter().any(|root_key| {
            candidate_keys.iter().any(|candidate_key| {
                candidate_key == root_key || candidate_key.starts_with(&(root_key.clone() + "\\"))
            })
        })
    })
}

fn verify_inventoried_file_unchanged(
    conn: &rusqlite::Connection,
    run_id: &str,
    physical_path: &str,
) -> Result<()> {
    let (expected_size, expected_modified, expected_identity): (i64, i64, Option<String>) = conn
        .query_row(
            "SELECT size_bytes,modified_ms,file_identity FROM media_cleanup_file WHERE run_id=?1 AND path=?2",
            params![run_id, physical_path],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
    let expected_identity = expected_identity.ok_or_else(|| {
        EngineError::InstallFailed(format!(
            "cleanup inventory has no filesystem identity for {physical_path}; start a new inventory before applying"
        ))
    })?;
    let metadata = std::fs::symlink_metadata(physical_path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() == 0 {
        return Err(EngineError::InstallFailed(format!(
            "cleanup target is not a non-empty regular file: {physical_path}"
        )));
    }
    let current_identity = native_file_identity(Path::new(physical_path))?;
    if expected_size != i64::try_from(metadata.len()).unwrap_or(i64::MAX)
        || expected_modified != modified_ms(&metadata)
        || expected_identity != current_identity
    {
        return Err(EngineError::InstallFailed(format!(
            "cleanup target changed after inventory: {physical_path}"
        )));
    }
    Ok(())
}

#[cfg(windows)]
fn native_file_identity(path: &Path) -> Result<String> {
    let file = File::open(path)?;
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    let succeeded =
        unsafe { GetFileInformationByHandle(file.as_raw_handle() as _, &mut info as *mut _) };
    if succeeded == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(format!(
        "windows:{:08x}:{:08x}{:08x}",
        info.dwVolumeSerialNumber, info.nFileIndexHigh, info.nFileIndexLow
    ))
}

#[cfg(unix)]
fn native_file_identity(path: &Path) -> Result<String> {
    use std::os::unix::fs::MetadataExt;
    let metadata = std::fs::metadata(path)?;
    Ok(format!("unix:{}:{}", metadata.dev(), metadata.ino()))
}

#[cfg(not(any(windows, unix)))]
fn native_file_identity(path: &Path) -> Result<String> {
    let canonical = path.canonicalize()?;
    let metadata = std::fs::metadata(&canonical)?;
    Ok(format!(
        "fallback:{}:{}:{}",
        canonical.to_string_lossy(),
        metadata.len(),
        modified_ms(&metadata)
    ))
}

pub fn advance_hashing(
    paths: &AppPaths,
    run_id: &str,
    max_files: Option<usize>,
) -> Result<MediaCleanupAdvanceSummary> {
    let max_files = max_files.unwrap_or(25).clamp(1, 500);
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    let stage: String = conn
        .query_row(
            "SELECT stage FROM media_cleanup_run WHERE id=?1",
            [run_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| EngineError::InstallFailed("cleanup run not found".to_string()))?;
    if stage == "inventory" {
        return Err(EngineError::InstallFailed(
            "finish inventory before hashing".to_string(),
        ));
    }
    if stage == "reconciliation" {
        return Err(EngineError::InstallFailed(
            "review and apply the reconciliation preview before hashing".to_string(),
        ));
    }
    if stage == "review" || stage == "complete" {
        return Ok(MediaCleanupAdvanceSummary {
            run: get_run_conn(&conn, run_id)?.expect("run exists"),
            processed_files: 0,
            remaining_inventory_entries: 0,
        });
    }
    if cleanup_active_job_count(&conn)? > 0 {
        conn.execute(
            "UPDATE media_cleanup_run SET status='paused', updated_at_ms=?1 WHERE id=?2",
            params![now_ms(), run_id],
        )?;
        return Ok(MediaCleanupAdvanceSummary {
            run: get_run_conn(&conn, run_id)?.expect("run exists"),
            processed_files: 0,
            remaining_inventory_entries: count_pending_hash_rows(&conn, run_id)?,
        });
    }

    let prefix_rows = pending_prefix_rows(&conn, run_id, max_files)?;
    if !prefix_rows.is_empty() {
        for row in &prefix_rows {
            match verify_inventoried_file_unchanged(&conn, run_id, &row.path)
                .and_then(|_| staged_hashes_with_cache(&conn, row, false))
            {
                Ok((prefix, suffix, _)) => {
                    conn.execute(
                        "UPDATE media_cleanup_file SET prefix_sha256=?1, suffix_sha256=?2, state='staged_hashed', last_error=NULL, updated_at_ms=?3 WHERE run_id=?4 AND path=?5",
                        params![prefix, suffix, now_ms(), run_id, row.path],
                    )?;
                    upsert_digest_cache(
                        &conn,
                        &row.path,
                        row.size_bytes,
                        row.modified_ms,
                        Some(&prefix),
                        Some(&suffix),
                        None,
                    )?;
                }
                Err(error) => record_hash_error(&conn, run_id, &row.path, &error.to_string())?,
            }
        }
        conn.execute(
            "UPDATE media_cleanup_run SET status='paused', stage='hashing', updated_at_ms=?1 WHERE id=?2",
            params![now_ms(), run_id],
        )?;
        return Ok(MediaCleanupAdvanceSummary {
            run: get_run_conn(&conn, run_id)?.expect("run exists"),
            processed_files: prefix_rows.len(),
            remaining_inventory_entries: count_pending_hash_rows(&conn, run_id)?,
        });
    }

    let full_rows = pending_full_rows(&conn, run_id, max_files)?;
    if !full_rows.is_empty() {
        for row in &full_rows {
            match verify_inventoried_file_unchanged(&conn, run_id, &row.path)
                .and_then(|_| staged_hashes_with_cache(&conn, row, true))
            {
                Ok((prefix, suffix, full)) => {
                    conn.execute(
                        "UPDATE media_cleanup_file SET prefix_sha256=?1, suffix_sha256=?2, full_sha256=?3, state='fully_hashed', last_error=NULL, updated_at_ms=?4 WHERE run_id=?5 AND path=?6",
                        params![prefix, suffix, full, now_ms(), run_id, row.path],
                    )?;
                    upsert_digest_cache(
                        &conn,
                        &row.path,
                        row.size_bytes,
                        row.modified_ms,
                        Some(&prefix),
                        Some(&suffix),
                        Some(&full),
                    )?;
                }
                Err(error) => record_hash_error(&conn, run_id, &row.path, &error.to_string())?,
            }
        }
        return Ok(MediaCleanupAdvanceSummary {
            run: get_run_conn(&conn, run_id)?.expect("run exists"),
            processed_files: full_rows.len(),
            remaining_inventory_entries: count_pending_hash_rows(&conn, run_id)?,
        });
    }

    build_duplicate_groups(&conn, run_id)?;
    Ok(MediaCleanupAdvanceSummary {
        run: get_run_conn(&conn, run_id)?.expect("run exists"),
        processed_files: 0,
        remaining_inventory_entries: 0,
    })
}

pub fn list_groups(paths: &AppPaths, run_id: &str) -> Result<Vec<MediaCleanupGroup>> {
    let conn = db::open_readonly(paths)?;
    let mut stmt = conn.prepare(
        r#"
SELECT group_id, full_sha256, size_bytes, member_count, keeper_path,
       keeper_library_item_id, reclaimable_bytes, decision
FROM media_cleanup_group
WHERE run_id=?1
ORDER BY reclaimable_bytes DESC, group_id ASC
"#,
    )?;
    let base = stmt
        .query_map([run_id], |row| {
            Ok(MediaCleanupGroup {
                group_id: row.get(0)?,
                full_sha256: row.get(1)?,
                size_bytes: u64::try_from(row.get::<_, i64>(2)?).unwrap_or(0),
                member_count: usize::try_from(row.get::<_, i64>(3)?).unwrap_or(0),
                keeper_path: row.get(4)?,
                keeper_library_item_id: row.get(5)?,
                reclaimable_bytes: u64::try_from(row.get::<_, i64>(6)?).unwrap_or(0),
                decision: row.get(7)?,
                members: Vec::new(),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut groups = Vec::with_capacity(base.len());
    for mut group in base {
        let mut member_stmt = conn.prepare(
            "SELECT path, library_item_id, media_id, state FROM media_cleanup_file WHERE run_id=?1 AND group_id=?2 ORDER BY path",
        )?;
        group.members = member_stmt
            .query_map(params![run_id, group.group_id], |row| {
                Ok(MediaCleanupGroupMember {
                    path: row.get(0)?,
                    library_item_id: row.get(1)?,
                    media_id: row.get(2)?,
                    state: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        groups.push(group);
    }
    Ok(groups)
}

pub fn list_variants(paths: &AppPaths, run_id: &str) -> Result<Vec<MediaCleanupVariant>> {
    let conn = db::open_readonly(paths)?;
    let mut stmt = conn.prepare(
        "SELECT variant_id,service,media_id,member_paths_json,evidence_json,status FROM media_cleanup_variant WHERE run_id=?1 ORDER BY service,media_id,variant_id",
    )?;
    let variants = stmt
        .query_map([run_id], |row| {
            let member_paths_json = row.get::<_, String>(3)?;
            let evidence_json = row.get::<_, String>(4)?;
            Ok(MediaCleanupVariant {
                variant_id: row.get(0)?,
                service: row.get(1)?,
                media_id: row.get(2)?,
                member_paths: serde_json::from_str(&member_paths_json).unwrap_or_default(),
                evidence: serde_json::from_str(&evidence_json).unwrap_or(serde_json::Value::Null),
                status: row.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(variants)
}

pub fn set_group_decision(
    paths: &AppPaths,
    run_id: &str,
    group_id: &str,
    decision: &str,
    keeper_path: Option<&str>,
) -> Result<MediaCleanupGroup> {
    if !matches!(decision, "approved" | "rejected" | "pending") {
        return Err(EngineError::InstallFailed(
            "cleanup decision must be approved, rejected, or pending".to_string(),
        ));
    }
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    let stage = cleanup_run_stage(&conn, run_id)?;
    if stage != "review" {
        return Err(EngineError::InstallFailed(format!(
            "cleanup decision requires review stage; current stage is {stage}"
        )));
    }
    if let Some(keeper) = keeper_path.map(str::trim).filter(|value| !value.is_empty()) {
        let mut stmt = conn.prepare(
            "SELECT path, library_item_id FROM media_cleanup_file WHERE run_id=?1 AND group_id=?2",
        )?;
        let members = stmt
            .query_map(params![run_id, group_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let candidate_paths = members
            .iter()
            .map(|(path, _)| path.clone())
            .collect::<Vec<_>>();
        let Some((stored_keeper, keeper_item_id)) = members
            .into_iter()
            .find(|(path, _)| paths_equivalent_with_aliases(paths, path, keeper))
        else {
            return Err(EngineError::InstallFailed(
                format!(
                    "selected keeper is not a member of the duplicate group: selected={keeper}; members={}",
                    candidate_paths.join(" | ")
                ),
            ));
        };
        conn.execute(
            "UPDATE media_cleanup_group SET keeper_path=?1, keeper_library_item_id=?2, decision=?3, decision_at_ms=?4, updated_at_ms=?4 WHERE run_id=?5 AND group_id=?6",
            params![stored_keeper, keeper_item_id, decision, now_ms(), run_id, group_id],
        )?;
    } else {
        conn.execute(
            "UPDATE media_cleanup_group SET decision=?1, decision_at_ms=?2, updated_at_ms=?2 WHERE run_id=?3 AND group_id=?4",
            params![decision, now_ms(), run_id, group_id],
        )?;
    }
    list_groups(paths, run_id)?
        .into_iter()
        .find(|group| group.group_id == group_id)
        .ok_or_else(|| EngineError::InstallFailed("cleanup group not found".to_string()))
}

pub fn apply_approved_groups(paths: &AppPaths, run_id: &str) -> Result<MediaCleanupApplySummary> {
    let mut conn = db::open(paths)?;
    db::migrate(&conn)?;
    ensure_cleanup_apply_boundary(&conn)?;
    let stage = cleanup_run_stage(&conn, run_id)?;
    if stage != "review" && stage != "rollback" {
        return Err(EngineError::InstallFailed(format!(
            "cleanup quarantine requires review stage; current stage is {stage}"
        )));
    }
    let quarantine_root: String = conn
        .query_row(
            "SELECT quarantine_root FROM media_cleanup_run WHERE id=?1",
            [run_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()?
        .flatten()
        .ok_or_else(|| {
            EngineError::InstallFailed(
                "choose a quarantine folder before applying cleanup".to_string(),
            )
        })?;
    let groups = list_groups(paths, run_id)?
        .into_iter()
        .filter(|group| group.decision == "approved")
        .collect::<Vec<_>>();
    if groups.is_empty() {
        return Err(EngineError::InstallFailed(
            "approve at least one exact duplicate group before quarantine".to_string(),
        ));
    }
    let mut summary = MediaCleanupApplySummary {
        run_id: run_id.to_string(),
        approved_groups: groups.len(),
        applied_actions: 0,
        failed_actions: 0,
        bytes_quarantined: 0,
    };
    for group in groups {
        for member in group
            .members
            .iter()
            .filter(|member| member.path != group.keeper_path)
        {
            match apply_one_action(
                paths,
                &mut conn,
                run_id,
                &group,
                member,
                Path::new(&quarantine_root),
            ) {
                Ok(bytes) => {
                    summary.applied_actions += 1;
                    summary.bytes_quarantined = summary.bytes_quarantined.saturating_add(bytes);
                }
                Err(_) => summary.failed_actions += 1,
            }
        }
    }
    conn.execute(
        "UPDATE media_cleanup_run SET status=?1, stage='quarantine', updated_at_ms=?2 WHERE id=?3",
        params![
            if summary.failed_actions == 0 {
                "applied"
            } else {
                "attention"
            },
            now_ms(),
            run_id
        ],
    )?;
    Ok(summary)
}

pub fn rollback_run(paths: &AppPaths, run_id: &str) -> Result<MediaCleanupApplySummary> {
    let mut conn = db::open(paths)?;
    db::migrate(&conn)?;
    ensure_cleanup_apply_boundary(&conn)?;
    let mut stmt = conn.prepare(
        r#"
SELECT id, source_path, quarantine_path, keeper_library_item_id,
       source_library_item_id, relinked_media_ids_json, size_bytes, full_sha256
FROM media_cleanup_action
WHERE run_id=?1 AND status IN ('planned', 'applied', 'attention')
ORDER BY created_at_ms DESC
"#,
    )?;
    let actions = stmt
        .query_map([run_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, String>(7)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    let mut summary = MediaCleanupApplySummary {
        run_id: run_id.to_string(),
        approved_groups: 0,
        applied_actions: 0,
        failed_actions: 0,
        bytes_quarantined: 0,
    };
    for action in actions {
        let source = PathBuf::from(&action.1);
        let quarantine = PathBuf::from(&action.2);
        let result = (|| -> Result<()> {
            let tx = begin_cleanup_apply_transaction(&mut conn)?;
            let source_exists = source.exists();
            let quarantine_exists = quarantine.exists();
            if source_exists && quarantine_exists {
                return Err(EngineError::InstallFailed(format!(
                    "rollback found both source and quarantine copies: source={}; quarantine={}",
                    source.to_string_lossy(),
                    quarantine.to_string_lossy()
                )));
            }
            if !source_exists && !quarantine_exists {
                return Err(EngineError::InstallFailed(format!(
                    "rollback found neither source nor quarantine copy: source={}; quarantine={}",
                    source.to_string_lossy(),
                    quarantine.to_string_lossy()
                )));
            }
            let relink_journal = parse_cleanup_relink_journal(&action.5)?;
            let keeper_path: String = tx.query_row(
                "SELECT keeper_path FROM media_cleanup_action WHERE id=?1",
                [&action.0],
                |row| row.get::<_, String>(0),
            )?;
            let journaled_source_paths = if relink_journal.source_library_paths.is_empty() {
                action
                    .4
                    .as_deref()
                    .map(|source_item| CleanupLibraryPathJournal {
                        library_item_id: source_item.to_string(),
                        original_media_path: relink_journal
                            .source_library_media_path
                            .clone()
                            .unwrap_or_else(|| action.1.clone()),
                    })
                    .into_iter()
                    .collect::<Vec<_>>()
            } else {
                relink_journal.source_library_paths.clone()
            };
            let mut source_library_paths = Vec::new();
            for source_item in &journaled_source_paths {
                let current_path = library_item_media_path(&tx, &source_item.library_item_id)?
                    .ok_or_else(|| {
                        EngineError::InstallFailed(format!(
                            "rollback source library item is missing: {}",
                            source_item.library_item_id
                        ))
                    })?;
                if current_path != source_item.original_media_path
                    && !paths_equivalent_with_aliases(
                        paths,
                        &current_path,
                        &source_item.original_media_path,
                    )
                    && !paths_equivalent_with_aliases(paths, &current_path, &keeper_path)
                {
                    return Err(EngineError::InstallFailed(format!(
                        "rollback refused to overwrite a library path changed after cleanup: item={}; current={current_path}",
                        source_item.library_item_id
                    )));
                }
                source_library_paths.push((
                    source_item.library_item_id.clone(),
                    current_path,
                    source_item.original_media_path.clone(),
                ));
            }
            if quarantine_exists {
                move_verified(&quarantine, &source, action.6, &action.7)?;
            } else {
                verify_path(&source, action.6, &action.7)?;
            }
            let database_result = (|| -> Result<()> {
                for (source_item, current_path, original_path) in &source_library_paths {
                    if current_path != original_path {
                        let changed = tx.execute(
                            "UPDATE library_item SET media_path=?1 WHERE id=?2 AND media_path=?3",
                            params![original_path, source_item, current_path],
                        )?;
                        if changed != 1 {
                            return Err(EngineError::InstallFailed(format!(
                                "rollback source library path changed concurrently: {source_item}"
                            )));
                        }
                    }
                }
                if let Some(keeper_item) = action.3.as_deref() {
                    let journaled_identities = if relink_journal.identities.is_empty() {
                        action
                            .4
                            .as_deref()
                            .into_iter()
                            .flat_map(|source_item| {
                                relink_journal.media_ids.iter().map(move |media_id| {
                                    CleanupIdentityRelinkJournal {
                                        service: "youtube".to_string(),
                                        media_id: media_id.clone(),
                                        source_library_item_id: source_item.to_string(),
                                    }
                                })
                            })
                            .collect::<Vec<_>>()
                    } else {
                        relink_journal.identities.clone()
                    };
                    for identity in &journaled_identities {
                        let changed = tx.execute(
                            "UPDATE media_source_identity SET library_item_id=?1, updated_at_ms=?2 WHERE service=?3 AND media_id=?4 AND library_item_id=?5",
                            params![
                                identity.source_library_item_id,
                                now_ms(),
                                identity.service,
                                identity.media_id,
                                keeper_item
                            ],
                        )?;
                        if changed == 0 {
                            let current_item = tx
                                .query_row(
                                    "SELECT library_item_id FROM media_source_identity WHERE service=?1 AND media_id=?2",
                                    params![identity.service, identity.media_id],
                                    |row| row.get::<_, Option<String>>(0),
                                )
                                .optional()?
                                .flatten();
                            if current_item.as_deref()
                                != Some(identity.source_library_item_id.as_str())
                            {
                                return Err(EngineError::InstallFailed(format!(
                                    "rollback identity changed after cleanup: {}:{}",
                                    identity.service, identity.media_id
                                )));
                            }
                        }
                    }
                }
                let cleanup_file_changed = tx.execute(
                    "UPDATE media_cleanup_file SET state='fully_hashed', updated_at_ms=?1 WHERE run_id=?2 AND path=?3",
                    params![now_ms(), run_id, action.1],
                )?;
                if cleanup_file_changed != 1 {
                    return Err(EngineError::InstallFailed(format!(
                        "rollback cleanup file journal row is missing: {}",
                        action.1
                    )));
                }
                let action_changed = tx.execute(
                    "UPDATE media_cleanup_action SET status='rolled_back', rolled_back_at_ms=?1, updated_at_ms=?1 WHERE id=?2",
                    params![now_ms(), action.0],
                )?;
                if action_changed != 1 {
                    return Err(EngineError::InstallFailed(format!(
                        "rollback cleanup action journal row is missing: {}",
                        action.0
                    )));
                }
                for (_, current_path, original_path) in &source_library_paths {
                    library::persist_media_path_observation_rewrite_invalidation(
                        &tx,
                        current_path,
                        original_path,
                    )?;
                }
                library::persist_media_path_observation_rewrite_invalidation(
                    &tx,
                    &quarantine.to_string_lossy(),
                    &source.to_string_lossy(),
                )?;
                Ok(())
            })();
            if let Err(database_error) = database_result {
                drop(tx);
                let mut compensation_error = None;
                if quarantine_exists && source.exists() && !quarantine.exists() {
                    if let Err(error) = move_verified(&source, &quarantine, action.6, &action.7) {
                        compensation_error = Some(error);
                    }
                }
                // The metadata transaction rolled back, but the file may have crossed either
                // filesystem boundary. Persist invalidation independently before returning the
                // attention result so a restart cannot trust the pre-rollback observation.
                let invalidation_result = (|| -> Result<()> {
                    let tx = conn.unchecked_transaction()?;
                    for (_, current_path, original_path) in &source_library_paths {
                        library::persist_media_path_observation_rewrite_invalidation(
                            &tx,
                            current_path,
                            original_path,
                        )?;
                    }
                    library::persist_media_path_observation_rewrite_invalidation(
                        &tx,
                        &quarantine.to_string_lossy(),
                        &source.to_string_lossy(),
                    )?;
                    tx.commit()?;
                    for (_, current_path, original_path) in &source_library_paths {
                        library::invalidate_media_path_observation_rewrite_memory(
                            current_path,
                            original_path,
                        );
                    }
                    library::invalidate_media_path_observation_rewrite_memory(
                        &quarantine.to_string_lossy(),
                        &source.to_string_lossy(),
                    );
                    Ok(())
                })();
                if let Err(invalidation_error) = invalidation_result {
                    return Err(EngineError::InstallFailed(format!(
                        "rollback database update failed ({database_error}); attention-path availability invalidation also failed ({invalidation_error})"
                    )));
                }
                if let Some(compensation_error) = compensation_error {
                    return Err(EngineError::InstallFailed(format!(
                        "rollback database update failed ({database_error}); restoring the quarantine copy also failed ({compensation_error})"
                    )));
                }
                return Err(database_error);
            }
            tx.commit()?;
            for (_, current_path, original_path) in &source_library_paths {
                library::invalidate_media_path_observation_rewrite_memory(
                    current_path,
                    original_path,
                );
            }
            library::invalidate_media_path_observation_rewrite_memory(
                &quarantine.to_string_lossy(),
                &source.to_string_lossy(),
            );
            Ok(())
        })();
        match result {
            Ok(()) => summary.applied_actions += 1,
            Err(error) => {
                summary.failed_actions += 1;
                conn.execute(
                    "UPDATE media_cleanup_action SET status='attention', error=?1, updated_at_ms=?2 WHERE id=?3",
                    params![error.to_string(), now_ms(), action.0],
                )?;
            }
        }
    }
    let (reconciliation_rolled_back, reconciliation_failed) =
        rollback_reconciliation_relinks(paths, &mut conn, run_id)?;
    summary.applied_actions = summary
        .applied_actions
        .saturating_add(reconciliation_rolled_back);
    summary.failed_actions = summary.failed_actions.saturating_add(reconciliation_failed);
    conn.execute(
        "UPDATE media_cleanup_run SET status=?1, stage='rollback', updated_at_ms=?2 WHERE id=?3",
        params![
            if summary.failed_actions == 0 {
                "rolled_back"
            } else {
                "attention"
            },
            now_ms(),
            run_id
        ],
    )?;
    Ok(summary)
}

fn rollback_reconciliation_relinks(
    paths: &AppPaths,
    conn: &mut rusqlite::Connection,
    run_id: &str,
) -> Result<(usize, usize)> {
    let mut stmt = conn.prepare(
        "SELECT candidate_id,physical_path,library_item_id,library_path FROM media_cleanup_reconciliation_candidate WHERE run_id=?1 AND disposition='applied' AND library_item_id IS NOT NULL ORDER BY candidate_id DESC",
    )?;
    let rows = stmt
        .query_map([run_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    let mut rolled_back = 0_usize;
    let mut failed = 0_usize;
    for (candidate_id, physical_path, library_item_id, original_path) in rows {
        let result = (|| -> Result<()> {
            let tx = begin_cleanup_apply_transaction(conn)?;
            let current = library_item_media_path(&tx, &library_item_id)?.ok_or_else(|| {
                EngineError::InstallFailed(format!(
                    "reconciliation rollback library item is missing: {library_item_id}"
                ))
            })?;
            if !paths_equivalent_with_aliases(paths, &current, &physical_path)
                && !paths_equivalent_with_aliases(paths, &current, &original_path)
            {
                return Err(EngineError::InstallFailed(format!(
                    "reconciliation rollback refused to overwrite a path changed after apply: item={library_item_id}; current={current}"
                )));
            }
            if current != original_path {
                let changed = tx.execute(
                    "UPDATE library_item SET media_path=?1 WHERE id=?2 AND media_path=?3",
                    params![original_path, library_item_id, current],
                )?;
                if changed != 1 {
                    return Err(EngineError::InstallFailed(format!(
                        "reconciliation rollback path changed concurrently: {library_item_id}"
                    )));
                }
            }
            library::persist_media_path_observation_rewrite_invalidation(
                &tx,
                &current,
                &original_path,
            )?;
            tx.execute(
                "UPDATE media_cleanup_reconciliation_candidate SET disposition='rolled_back', error=NULL, updated_at_ms=?1 WHERE run_id=?2 AND candidate_id=?3 AND disposition='applied'",
                params![now_ms(), run_id, candidate_id],
            )?;
            tx.commit()?;
            library::invalidate_media_path_observation_rewrite_memory(&current, &original_path);
            Ok(())
        })();
        match result {
            Ok(()) => rolled_back += 1,
            Err(error) => {
                failed += 1;
                conn.execute(
                    "UPDATE media_cleanup_reconciliation_candidate SET disposition='rollback_attention', error=?1, updated_at_ms=?2 WHERE run_id=?3 AND candidate_id=?4",
                    params![error.to_string(), now_ms(), run_id, candidate_id],
                )?;
            }
        }
    }
    Ok((rolled_back, failed))
}

fn apply_one_action(
    paths: &AppPaths,
    conn: &mut rusqlite::Connection,
    run_id: &str,
    group: &MediaCleanupGroup,
    member: &MediaCleanupGroupMember,
    quarantine_root: &Path,
) -> Result<u64> {
    ensure_cleanup_apply_boundary(conn)?;
    let source = PathBuf::from(&member.path);
    let keeper = PathBuf::from(&group.keeper_path);
    if paths_equivalent_with_aliases(paths, &member.path, &group.keeper_path) {
        return Err(EngineError::InstallFailed(format!(
            "cleanup source resolves to the selected keeper path: {}",
            member.path
        )));
    }
    verify_inventoried_file_unchanged(conn, run_id, &member.path)?;
    verify_inventoried_file_unchanged(conn, run_id, &group.keeper_path)?;
    verify_path(
        &source,
        i64::try_from(group.size_bytes).unwrap_or(i64::MAX),
        &group.full_sha256,
    )?;
    verify_path(
        &keeper,
        i64::try_from(group.size_bytes).unwrap_or(i64::MAX),
        &group.full_sha256,
    )?;
    if let Some(source_item) = member.library_item_id.as_deref() {
        let current_path = library_item_media_path(conn, source_item)?.ok_or_else(|| {
            EngineError::InstallFailed(format!(
                "cleanup source library item is missing: {source_item}"
            ))
        })?;
        if !paths_equivalent_with_aliases(paths, &current_path, &member.path) {
            return Err(EngineError::InstallFailed(format!(
                "cleanup source library path changed after inventory: item={source_item}; expected={}; current={current_path}",
                member.path
            )));
        }
    }
    let source_library_paths = library_items_for_media_path(paths, conn, &member.path)?;
    let source_library_path = member.library_item_id.as_deref().and_then(|source_item| {
        source_library_paths
            .iter()
            .find(|item| item.library_item_id == source_item)
            .map(|item| item.original_media_path.clone())
    });
    if member.library_item_id.is_some() && source_library_path.is_none() {
        return Err(EngineError::InstallFailed(
            "cleanup source library owner disappeared after inventory".to_string(),
        ));
    }
    if let Some(keeper_item) = group.keeper_library_item_id.as_deref() {
        let current_path = library_item_media_path(conn, keeper_item)?.ok_or_else(|| {
            EngineError::InstallFailed(format!(
                "cleanup keeper library item is missing: {keeper_item}"
            ))
        })?;
        if !paths_equivalent_with_aliases(paths, &current_path, &group.keeper_path) {
            return Err(EngineError::InstallFailed(format!(
                "cleanup keeper library path changed after approval: item={keeper_item}; expected={}; current={current_path}",
                group.keeper_path
            )));
        }
    }
    let keeper_library_paths = library_items_for_media_path(paths, conn, &group.keeper_path)?;
    let keeper_library_path = group
        .keeper_library_item_id
        .as_deref()
        .and_then(|keeper_item| {
            keeper_library_paths
                .iter()
                .find(|item| item.library_item_id == keeper_item)
                .map(|item| item.original_media_path.clone())
        });
    if group.keeper_library_item_id.is_some() && keeper_library_path.is_none() {
        return Err(EngineError::InstallFailed(
            "cleanup keeper library owner disappeared after approval".to_string(),
        ));
    }
    let action_id = Uuid::new_v4().to_string();
    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| format!(".{}", value.to_ascii_lowercase()))
        .unwrap_or_default();
    let quarantine_path = quarantine_root.join(run_id).join(format!(
        "{}_{}{}",
        &group.full_sha256[..16],
        action_id,
        extension
    ));
    let relinked_identities = if group.keeper_library_item_id.is_some() {
        identities_for_library_items(conn, &source_library_paths)?
    } else {
        Vec::new()
    };
    let relinked_media_ids = relinked_identities
        .iter()
        .filter(|identity| identity.service == "youtube")
        .map(|identity| identity.media_id.clone())
        .collect::<Vec<_>>();
    let now = now_ms();
    conn.execute(
        r#"
INSERT INTO media_cleanup_action (
  id, run_id, group_id, source_path, quarantine_path, keeper_path,
  source_library_item_id, keeper_library_item_id, relinked_media_ids_json,
  size_bytes, full_sha256, status, created_at_ms, updated_at_ms
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'planned', ?12, ?12)
"#,
        params![
            action_id,
            run_id,
            group.group_id,
            member.path,
            quarantine_path.to_string_lossy(),
            group.keeper_path,
            member.library_item_id,
            group.keeper_library_item_id,
            serde_json::to_string(&CleanupRelinkJournal {
                version: 2,
                media_ids: relinked_media_ids.clone(),
                source_library_media_path: source_library_path.clone(),
                source_library_paths: source_library_paths.clone(),
                identities: relinked_identities.clone(),
            })?,
            i64::try_from(group.size_bytes).unwrap_or(i64::MAX),
            group.full_sha256,
            now
        ],
    )?;
    let result = (|| -> Result<()> {
        let tx = begin_cleanup_apply_transaction(conn)?;
        verify_inventoried_file_unchanged(&tx, run_id, &member.path)?;
        verify_inventoried_file_unchanged(&tx, run_id, &group.keeper_path)?;
        verify_path(
            &source,
            i64::try_from(group.size_bytes).unwrap_or(i64::MAX),
            &group.full_sha256,
        )?;
        verify_path(
            &keeper,
            i64::try_from(group.size_bytes).unwrap_or(i64::MAX),
            &group.full_sha256,
        )?;
        move_verified(
            &source,
            &quarantine_path,
            i64::try_from(group.size_bytes).unwrap_or(i64::MAX),
            &group.full_sha256,
        )?;
        for source_item in &source_library_paths {
            let changed = tx.execute(
                "UPDATE library_item SET media_path=?1 WHERE id=?2 AND media_path=?3",
                params![
                    group.keeper_path,
                    source_item.library_item_id,
                    source_item.original_media_path
                ],
            )?;
            if changed != 1 {
                return Err(EngineError::InstallFailed(format!(
                    "cleanup source library path changed concurrently: {}",
                    source_item.library_item_id
                )));
            }
        }
        if let Some(keeper_item) = group.keeper_library_item_id.as_deref() {
            for identity in &relinked_identities {
                let changed = tx.execute(
                    "UPDATE media_source_identity SET library_item_id=?1, repair_state='ready', updated_at_ms=?2 WHERE service=?3 AND media_id=?4 AND library_item_id=?5",
                    params![
                        keeper_item,
                        now_ms(),
                        identity.service,
                        identity.media_id,
                        identity.source_library_item_id
                    ],
                )?;
                if changed != 1 {
                    return Err(EngineError::InstallFailed(format!(
                        "cleanup identity changed concurrently: {}:{}",
                        identity.service, identity.media_id
                    )));
                }
            }
        }
        let cleanup_file_changed = tx.execute(
            "UPDATE media_cleanup_file SET state='quarantined', updated_at_ms=?1 WHERE run_id=?2 AND path=?3",
            params![now_ms(), run_id, member.path],
        )?;
        if cleanup_file_changed != 1 {
            return Err(EngineError::InstallFailed(format!(
                "cleanup file journal row is missing: {}",
                member.path
            )));
        }
        let action_changed = tx.execute(
            "UPDATE media_cleanup_action SET status='applied', applied_at_ms=?1, updated_at_ms=?1 WHERE id=?2",
            params![now_ms(), action_id],
        )?;
        if action_changed != 1 {
            return Err(EngineError::InstallFailed(format!(
                "cleanup action journal row is missing: {action_id}"
            )));
        }
        if source_library_paths.is_empty() {
            library::persist_media_path_observation_rewrite_invalidation(
                &tx,
                &member.path,
                &group.keeper_path,
            )?;
        }
        for source_item in &source_library_paths {
            library::persist_media_path_observation_rewrite_invalidation(
                &tx,
                &source_item.original_media_path,
                &group.keeper_path,
            )?;
        }
        if let Some(keeper_path) = keeper_library_path.as_deref() {
            library::persist_media_path_observation_rewrite_invalidation(
                &tx,
                &group.keeper_path,
                keeper_path,
            )?;
        }
        library::persist_media_path_observation_rewrite_invalidation(
            &tx,
            &source.to_string_lossy(),
            &quarantine_path.to_string_lossy(),
        )?;
        tx.commit()?;
        if source_library_paths.is_empty() {
            library::invalidate_media_path_observation_rewrite_memory(
                &member.path,
                &group.keeper_path,
            );
        }
        for source_item in &source_library_paths {
            library::invalidate_media_path_observation_rewrite_memory(
                &source_item.original_media_path,
                &group.keeper_path,
            );
        }
        if let Some(keeper_path) = keeper_library_path.as_deref() {
            library::invalidate_media_path_observation_rewrite_memory(
                &group.keeper_path,
                keeper_path,
            );
        }
        library::invalidate_media_path_observation_rewrite_memory(
            &source.to_string_lossy(),
            &quarantine_path.to_string_lossy(),
        );
        Ok(())
    })();
    if let Err(error) = result {
        let moved_to_quarantine = quarantine_path.exists() && !source.exists();
        let recovery_error = if moved_to_quarantine {
            restore_source_after_failed_cleanup_apply(
                &quarantine_path,
                &source,
                i64::try_from(group.size_bytes).unwrap_or(i64::MAX),
                &group.full_sha256,
            )
            .err()
        } else {
            None
        };
        let still_quarantined = quarantine_path.exists() && !source.exists();
        let invalidation_error = if still_quarantined {
            (|| -> Result<()> {
                let tx = conn.unchecked_transaction()?;
                if source_library_paths.is_empty() {
                    library::persist_media_path_observation_rewrite_invalidation(
                        &tx,
                        &member.path,
                        &quarantine_path.to_string_lossy(),
                    )?;
                }
                for source_item in &source_library_paths {
                    library::persist_media_path_observation_rewrite_invalidation(
                        &tx,
                        &source_item.original_media_path,
                        &quarantine_path.to_string_lossy(),
                    )?;
                }
                tx.commit()?;
                if source_library_paths.is_empty() {
                    library::invalidate_media_path_observation_rewrite_memory(
                        &member.path,
                        &quarantine_path.to_string_lossy(),
                    );
                }
                for source_item in &source_library_paths {
                    library::invalidate_media_path_observation_rewrite_memory(
                        &source_item.original_media_path,
                        &quarantine_path.to_string_lossy(),
                    );
                }
                Ok(())
            })()
            .err()
        } else {
            None
        };
        let error_text = recovery_error
            .as_ref()
            .map(|recovery| {
                format!(
                    "{error}; restoring the source after the database failure also failed: {recovery}"
                )
            })
            .unwrap_or_else(|| error.to_string());
        let error_text = invalidation_error
            .as_ref()
            .map(|invalidation| {
                format!("{error_text}; persisting attention-path availability invalidation also failed: {invalidation}")
            })
            .unwrap_or(error_text);
        conn.execute(
            "UPDATE media_cleanup_action SET status=?1, error=?2, updated_at_ms=?3 WHERE id=?4",
            params![
                if still_quarantined {
                    "attention"
                } else {
                    "failed"
                },
                error_text,
                now_ms(),
                action_id
            ],
        )?;
        if let Some(invalidation_error) = invalidation_error {
            return Err(invalidation_error);
        }
        return Err(error);
    }
    Ok(group.size_bytes)
}

fn begin_cleanup_apply_transaction(
    conn: &mut rusqlite::Connection,
) -> Result<rusqlite::Transaction<'_>> {
    // Claim the SQLite writer before the final queue/running-job check. Job dispatch also claims
    // through an IMMEDIATE transaction, so no runner that raced the outer pause check can become
    // running between this proof and the file move + metadata publication below.
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    ensure_cleanup_apply_boundary(&tx)?;
    Ok(tx)
}

fn restore_source_after_failed_cleanup_apply(
    quarantine: &Path,
    source: &Path,
    expected_size: i64,
    expected_sha256: &str,
) -> Result<()> {
    #[cfg(test)]
    if FORCE_CLEANUP_APPLY_COMPENSATION_FAILURE.with(|flag| flag.get()) {
        return Err(EngineError::InstallFailed(
            "forced cleanup apply compensation failure".to_string(),
        ));
    }
    move_verified(quarantine, source, expected_size, expected_sha256)
}

fn ensure_cleanup_apply_boundary(conn: &rusqlite::Connection) -> Result<()> {
    let paused = conn
        .query_row(
            "SELECT value FROM meta WHERE key='jobs_queue_paused'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .as_deref()
        == Some("1");
    if !paused {
        return Err(EngineError::InstallFailed(
            "media cleanup mutation requires the global queue to remain paused".to_string(),
        ));
    }
    let running_jobs: i64 = conn.query_row(
        "SELECT COUNT(*) FROM job WHERE status='running'",
        [],
        |row| row.get(0),
    )?;
    if running_jobs != 0 {
        return Err(EngineError::InstallFailed(format!(
            "media cleanup mutation requires zero running jobs; found {running_jobs}"
        )));
    }
    Ok(())
}

fn cleanup_active_job_count(conn: &rusqlite::Connection) -> Result<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM job WHERE status='running'",
        [],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn cleanup_run_stage(conn: &rusqlite::Connection, run_id: &str) -> Result<String> {
    conn.query_row(
        "SELECT stage FROM media_cleanup_run WHERE id=?1",
        [run_id],
        |row| row.get(0),
    )
    .optional()?
    .ok_or_else(|| EngineError::InstallFailed("cleanup run not found".to_string()))
}

fn library_item_media_path(conn: &rusqlite::Connection, item_id: &str) -> Result<Option<String>> {
    conn.query_row(
        "SELECT media_path FROM library_item WHERE id=?1",
        [item_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}

fn library_items_for_media_path(
    paths: &AppPaths,
    conn: &rusqlite::Connection,
    media_path: &str,
) -> Result<Vec<CleanupLibraryPathJournal>> {
    let mut stmt = conn.prepare("SELECT id, media_path FROM library_item ORDER BY id")?;
    let candidates = stmt
        .query_map([], |row| {
            Ok(CleanupLibraryPathJournal {
                library_item_id: row.get(0)?,
                original_media_path: row.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    candidates
        .into_iter()
        .filter_map(|candidate| {
            match paths_equivalent_with_aliases(paths, &candidate.original_media_path, media_path) {
                true => Some(Ok(candidate)),
                false => None,
            }
        })
        .collect()
}

fn identities_for_library_items(
    conn: &rusqlite::Connection,
    library_items: &[CleanupLibraryPathJournal],
) -> Result<Vec<CleanupIdentityRelinkJournal>> {
    let mut identities = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT service, media_id FROM media_source_identity WHERE library_item_id=?1 ORDER BY service, media_id",
    )?;
    for item in library_items {
        identities.extend(
            stmt.query_map([&item.library_item_id], |row| {
                Ok(CleanupIdentityRelinkJournal {
                    service: row.get(0)?,
                    media_id: row.get(1)?,
                    source_library_item_id: item.library_item_id.clone(),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?,
        );
    }
    Ok(identities)
}

fn parse_cleanup_relink_journal(value: &str) -> Result<CleanupRelinkJournal> {
    if let Ok(journal) = serde_json::from_str::<CleanupRelinkJournal>(value) {
        return Ok(journal);
    }
    let media_ids = serde_json::from_str::<Vec<String>>(value)?;
    Ok(CleanupRelinkJournal {
        version: 0,
        media_ids,
        source_library_media_path: None,
        source_library_paths: Vec::new(),
        identities: Vec::new(),
    })
}

fn move_verified(source: &Path, destination: &Path, size_bytes: i64, sha256: &str) -> Result<()> {
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if std::fs::rename(source, destination).is_err() {
        let partial = destination.with_extension("partial");
        let mut input = File::open(source)?;
        let mut output = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&partial)?;
        std::io::copy(&mut input, &mut output)?;
        output.flush()?;
        output.sync_all()?;
        verify_path(&partial, size_bytes, sha256)?;
        std::fs::rename(&partial, destination)?;
        verify_path(destination, size_bytes, sha256)?;
        std::fs::remove_file(source)?;
    } else if let Err(error) = verify_path(destination, size_bytes, sha256) {
        let _ = std::fs::rename(destination, source);
        return Err(error);
    }
    Ok(())
}

fn verify_path(path: &Path, size_bytes: i64, sha256: &str) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(EngineError::InstallFailed(format!(
            "cleanup verification requires a regular file: {}",
            path.to_string_lossy()
        )));
    }
    let expected_size = u64::try_from(size_bytes).unwrap_or(0);
    if metadata.len() != expected_size {
        return Err(EngineError::SizeMismatch {
            path: path.to_path_buf(),
            expected: expected_size,
            actual: metadata.len(),
        });
    }
    let actual = full_sha256(path)?;
    if actual != sha256 {
        return Err(EngineError::HashMismatch {
            path: path.to_path_buf(),
            expected: sha256.to_string(),
            actual,
        });
    }
    Ok(())
}

fn build_duplicate_groups(conn: &rusqlite::Connection, run_id: &str) -> Result<()> {
    #[derive(Clone)]
    struct ByteCandidate {
        path: String,
        library_item_id: Option<String>,
    }

    conn.execute("DELETE FROM media_cleanup_group WHERE run_id=?1", [run_id])?;
    conn.execute(
        "UPDATE media_cleanup_file SET group_id=NULL WHERE run_id=?1",
        [run_id],
    )?;
    let mut stmt = conn.prepare(
        r#"
SELECT full_sha256, size_bytes, COUNT(*)
FROM media_cleanup_file
WHERE run_id=?1 AND full_sha256 IS NOT NULL AND state='fully_hashed'
GROUP BY full_sha256, size_bytes
HAVING COUNT(*) > 1
ORDER BY full_sha256
"#,
    )?;
    let groups = stmt
        .query_map([run_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut reclaimable_total = 0_i64;
    for (hash, size, _) in groups {
        let mut member_stmt = conn.prepare(
            r#"
SELECT path, library_item_id
FROM media_cleanup_file
WHERE run_id=?1 AND full_sha256=?2 AND size_bytes=?3
ORDER BY
  CASE WHEN media_id IS NOT NULL THEN 0 ELSE 1 END,
  CASE WHEN library_item_id IS NOT NULL THEN 0 ELSE 1 END,
  lower(path) ASC
"#,
        )?;
        let candidates = member_stmt
            .query_map(params![run_id, hash, size], |row| {
                Ok(ByteCandidate {
                    path: row.get(0)?,
                    library_item_id: row.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut byte_clusters: Vec<Vec<ByteCandidate>> = Vec::new();
        for candidate in candidates {
            let mut placed = false;
            for cluster in &mut byte_clusters {
                match files_have_identical_bytes(
                    Path::new(&cluster[0].path),
                    Path::new(&candidate.path),
                ) {
                    Ok(true) => {
                        cluster.push(candidate.clone());
                        placed = true;
                        break;
                    }
                    Ok(false) => {}
                    Err(error) => {
                        conn.execute(
                            "UPDATE media_cleanup_file SET state='hash_error', last_error=?1, group_id=NULL, updated_at_ms=?2 WHERE run_id=?3 AND path=?4",
                            params![format!("byte confirmation failed: {error}"), now_ms(), run_id, candidate.path],
                        )?;
                        placed = true;
                        break;
                    }
                }
            }
            if !placed {
                byte_clusters.push(vec![candidate]);
            }
        }

        let mut duplicate_cluster_index = 0_usize;
        for cluster in byte_clusters
            .into_iter()
            .filter(|cluster| cluster.len() > 1)
        {
            let group_id = if duplicate_cluster_index == 0 {
                format!("sha256:{hash}")
            } else {
                format!("sha256:{hash}:bytes:{duplicate_cluster_index}")
            };
            duplicate_cluster_index += 1;
            let count = i64::try_from(cluster.len()).unwrap_or(i64::MAX);
            let reclaimable = size.saturating_mul(count.saturating_sub(1));
            reclaimable_total = reclaimable_total.saturating_add(reclaimable);
            let keeper = &cluster[0];
            conn.execute(
                "INSERT INTO media_cleanup_group (run_id, group_id, full_sha256, size_bytes, member_count, keeper_path, keeper_library_item_id, reclaimable_bytes, decision, created_at_ms, updated_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'pending', ?9, ?9)",
                params![run_id, group_id, hash, size, count, keeper.path, keeper.library_item_id, reclaimable, now_ms()],
            )?;
            for member in cluster {
                conn.execute(
                    "UPDATE media_cleanup_file SET group_id=?1 WHERE run_id=?2 AND path=?3",
                    params![group_id, run_id, member.path],
                )?;
            }
        }
    }
    let duplicate_groups: i64 = conn.query_row(
        "SELECT COUNT(*) FROM media_cleanup_group WHERE run_id=?1",
        [run_id],
        |row| row.get(0),
    )?;
    build_variant_review_rows(conn, run_id)?;
    conn.execute(
        "UPDATE media_cleanup_run SET status='review', stage='review', duplicate_groups=?1, reclaimable_bytes=?2, updated_at_ms=?3 WHERE id=?4",
        params![duplicate_groups, reclaimable_total, now_ms(), run_id],
    )?;
    Ok(())
}

fn files_have_identical_bytes(left: &Path, right: &Path) -> Result<bool> {
    let left_metadata = std::fs::metadata(left)?;
    let right_metadata = std::fs::metadata(right)?;
    if left_metadata.len() != right_metadata.len() {
        return Ok(false);
    }
    let mut left_reader = BufReader::new(File::open(left)?);
    let mut right_reader = BufReader::new(File::open(right)?);
    let mut left_buffer = [0_u8; HASH_WINDOW_BYTES];
    let mut right_buffer = [0_u8; HASH_WINDOW_BYTES];
    loop {
        let left_read = read_filled_chunk(&mut left_reader, &mut left_buffer)?;
        let right_read = read_filled_chunk(&mut right_reader, &mut right_buffer)?;
        if left_read != right_read || left_buffer[..left_read] != right_buffer[..right_read] {
            return Ok(false);
        }
        if left_read == 0 {
            return Ok(true);
        }
    }
}

fn read_filled_chunk(reader: &mut impl Read, buffer: &mut [u8]) -> Result<usize> {
    let mut filled = 0_usize;
    while filled < buffer.len() {
        let read = reader.read(&mut buffer[filled..])?;
        if read == 0 {
            break;
        }
        filled += read;
    }
    Ok(filled)
}

fn build_variant_review_rows(conn: &rusqlite::Connection, run_id: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM media_cleanup_variant WHERE run_id=?1",
        [run_id],
    )?;
    let mut identity_stmt = conn.prepare(
        r#"
SELECT media_id
FROM media_cleanup_file
WHERE run_id=?1 AND media_id IS NOT NULL
GROUP BY media_id
HAVING COUNT(*) > 1
ORDER BY media_id
"#,
    )?;
    let media_ids = identity_stmt
        .query_map([run_id], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for media_id in media_ids {
        let mut member_stmt = conn.prepare(
            r#"
SELECT f.path,f.size_bytes,f.full_sha256,f.group_id,f.library_item_id,
       li.duration_ms,li.width,li.height,li.container,li.video_codec,li.audio_codec
FROM media_cleanup_file f
LEFT JOIN library_item li ON li.id=f.library_item_id
WHERE f.run_id=?1 AND f.media_id=?2
ORDER BY lower(f.path)
"#,
        )?;
        let members = member_stmt
            .query_map(params![run_id, media_id], |row| {
                Ok(serde_json::json!({
                    "path": row.get::<_, String>(0)?,
                    "size_bytes": row.get::<_, i64>(1)?,
                    "full_sha256": row.get::<_, Option<String>>(2)?,
                    "byte_confirmed_group_id": row.get::<_, Option<String>>(3)?,
                    "library_item_id": row.get::<_, Option<String>>(4)?,
                    "duration_ms": row.get::<_, Option<i64>>(5)?,
                    "width": row.get::<_, Option<i64>>(6)?,
                    "height": row.get::<_, Option<i64>>(7)?,
                    "container": row.get::<_, Option<String>>(8)?,
                    "video_codec": row.get::<_, Option<String>>(9)?,
                    "audio_codec": row.get::<_, Option<String>>(10)?,
                }))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let member_paths = members
            .iter()
            .filter_map(|member| member.get("path").and_then(|value| value.as_str()))
            .map(str::to_string)
            .collect::<Vec<_>>();
        let full_hash_count = members
            .iter()
            .filter(|member| {
                member
                    .get("full_sha256")
                    .is_some_and(|value| !value.is_null())
            })
            .count();
        let exact_hashes = members
            .iter()
            .filter_map(|member| member.get("full_sha256").and_then(|value| value.as_str()))
            .collect::<HashSet<_>>();
        let byte_confirmed_groups = members
            .iter()
            .filter_map(|member| {
                member
                    .get("byte_confirmed_group_id")
                    .and_then(|value| value.as_str())
            })
            .collect::<HashSet<_>>();
        let byte_confirmation_complete = byte_confirmed_groups.len() == 1
            && members.iter().all(|member| {
                member
                    .get("byte_confirmed_group_id")
                    .is_some_and(|value| !value.is_null())
            });
        let metadata_complete = members.iter().all(|member| {
            member
                .get("duration_ms")
                .is_some_and(|value| !value.is_null())
                && member
                    .get("container")
                    .is_some_and(|value| !value.is_null())
                && member
                    .get("video_codec")
                    .is_some_and(|value| !value.is_null())
        });
        let evidence = serde_json::json!({
            "classification": if full_hash_count == members.len()
                && exact_hashes.len() == 1
                && byte_confirmation_complete
            {
                "same_identity_exact_bytes"
            } else {
                "same_identity_variant_review"
            },
            "byte_confirmation_complete": byte_confirmation_complete,
            "metadata_complete": metadata_complete,
            "members": members,
        });
        let variant_id = stable_variant_id(run_id, "youtube", &media_id);
        conn.execute(
            "INSERT INTO media_cleanup_variant(run_id,variant_id,service,media_id,member_paths_json,evidence_json,status,created_at_ms,updated_at_ms) VALUES(?1,?2,'youtube',?3,?4,?5,'review_only',?6,?6)",
            params![
                run_id,
                variant_id,
                media_id,
                serde_json::to_string(&member_paths)?,
                serde_json::to_string(&evidence)?,
                now_ms()
            ],
        )?;
    }
    Ok(())
}

fn stable_variant_id(run_id: &str, service: &str, media_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(run_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(service.as_bytes());
    hasher.update(b"\0");
    hasher.update(media_id.as_bytes());
    format!("variant-{}", &hex::encode(hasher.finalize())[..24])
}

fn pending_prefix_rows(
    conn: &rusqlite::Connection,
    run_id: &str,
    limit: usize,
) -> Result<Vec<CleanupFileRow>> {
    let mut stmt = conn.prepare(
        r#"
SELECT f.path, f.size_bytes, f.modified_ms, f.library_item_id, f.media_id
FROM media_cleanup_file f
WHERE f.run_id=?1
  AND f.prefix_sha256 IS NULL
  AND f.size_bytes IN (
    SELECT size_bytes FROM media_cleanup_file
    WHERE run_id=?1
    GROUP BY size_bytes HAVING COUNT(*) > 1
  )
ORDER BY f.size_bytes, f.path
LIMIT ?2
"#,
    )?;
    collect_cleanup_rows(
        &mut stmt,
        params![run_id, i64::try_from(limit).unwrap_or(i64::MAX)],
    )
}

fn pending_full_rows(
    conn: &rusqlite::Connection,
    run_id: &str,
    limit: usize,
) -> Result<Vec<CleanupFileRow>> {
    let mut stmt = conn.prepare(
        r#"
SELECT f.path, f.size_bytes, f.modified_ms, f.library_item_id, f.media_id
FROM media_cleanup_file f
WHERE f.run_id=?1
  AND f.full_sha256 IS NULL
  AND f.prefix_sha256 IS NOT NULL
  AND EXISTS (
    SELECT 1 FROM media_cleanup_file other
    WHERE other.run_id=f.run_id
      AND other.path<>f.path
      AND other.size_bytes=f.size_bytes
      AND other.prefix_sha256=f.prefix_sha256
      AND other.suffix_sha256=f.suffix_sha256
  )
ORDER BY f.size_bytes, f.path
LIMIT ?2
"#,
    )?;
    collect_cleanup_rows(
        &mut stmt,
        params![run_id, i64::try_from(limit).unwrap_or(i64::MAX)],
    )
}

fn collect_cleanup_rows<P: rusqlite::Params>(
    stmt: &mut rusqlite::Statement<'_>,
    params: P,
) -> Result<Vec<CleanupFileRow>> {
    Ok(stmt
        .query_map(params, |row| {
            Ok(CleanupFileRow {
                path: row.get(0)?,
                size_bytes: row.get(1)?,
                modified_ms: row.get(2)?,
                library_item_id: row.get(3)?,
                media_id: row.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}

fn staged_hashes(
    path: &Path,
    expected_size: i64,
    expected_modified_ms: i64,
    include_full: bool,
) -> Result<(String, String, String)> {
    let metadata = std::fs::metadata(path)?;
    if i64::try_from(metadata.len()).unwrap_or(i64::MAX) != expected_size
        || modified_ms(&metadata) != expected_modified_ms
    {
        return Err(EngineError::InstallFailed(format!(
            "file changed after inventory: {}",
            path.to_string_lossy()
        )));
    }
    let mut file = File::open(path)?;
    let prefix = hash_window(&mut file, 0, metadata.len())?;
    let suffix_offset = metadata.len().saturating_sub(HASH_WINDOW_BYTES as u64);
    let suffix = hash_window(&mut file, suffix_offset, metadata.len())?;
    let full = if include_full {
        full_sha256(path)?
    } else {
        String::new()
    };
    Ok((prefix, suffix, full))
}

fn staged_hashes_with_cache(
    conn: &rusqlite::Connection,
    row: &CleanupFileRow,
    include_full: bool,
) -> Result<(String, String, String)> {
    let metadata = std::fs::metadata(&row.path)?;
    if i64::try_from(metadata.len()).unwrap_or(i64::MAX) != row.size_bytes
        || modified_ms(&metadata) != row.modified_ms
    {
        return Err(EngineError::InstallFailed(format!(
            "file changed after inventory: {}",
            row.path
        )));
    }
    let cached = conn
        .query_row(
            r#"
SELECT prefix_sha256, suffix_sha256, full_sha256
FROM media_file_digest_cache
WHERE path=?1 AND size_bytes=?2 AND modified_ms=?3
"#,
            params![row.path, row.size_bytes, row.modified_ms],
            |cache_row| {
                Ok((
                    cache_row.get::<_, Option<String>>(0)?,
                    cache_row.get::<_, Option<String>>(1)?,
                    cache_row.get::<_, Option<String>>(2)?,
                ))
            },
        )
        .optional()?;
    if let Some((Some(prefix), Some(suffix), full)) = cached {
        if !include_full {
            return Ok((prefix, suffix, String::new()));
        }
        if let Some(full) = full {
            return Ok((prefix, suffix, full));
        }
    }
    staged_hashes(
        Path::new(&row.path),
        row.size_bytes,
        row.modified_ms,
        include_full,
    )
}

fn hash_window(file: &mut File, offset: u64, size: u64) -> Result<String> {
    file.seek(SeekFrom::Start(offset))?;
    let count = size.saturating_sub(offset).min(HASH_WINDOW_BYTES as u64) as usize;
    let mut buffer = vec![0_u8; count];
    file.read_exact(&mut buffer)?;
    Ok(hex::encode(Sha256::digest(buffer)))
}

fn full_sha256(path: &Path) -> Result<String> {
    let file = File::open(path)?;
    let mut reader = BufReader::with_capacity(1024 * 1024, file);
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn upsert_digest_cache(
    conn: &rusqlite::Connection,
    path: &str,
    size_bytes: i64,
    modified_ms: i64,
    prefix: Option<&str>,
    suffix: Option<&str>,
    full: Option<&str>,
) -> Result<()> {
    conn.execute(
        r#"
INSERT INTO media_file_digest_cache (
  path, size_bytes, modified_ms, prefix_sha256, suffix_sha256, full_sha256, verified_at_ms
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
ON CONFLICT(path) DO UPDATE SET
  size_bytes=excluded.size_bytes,
  modified_ms=excluded.modified_ms,
  prefix_sha256=CASE
    WHEN excluded.size_bytes=media_file_digest_cache.size_bytes
     AND excluded.modified_ms=media_file_digest_cache.modified_ms
    THEN COALESCE(excluded.prefix_sha256, media_file_digest_cache.prefix_sha256)
    ELSE excluded.prefix_sha256
  END,
  suffix_sha256=CASE
    WHEN excluded.size_bytes=media_file_digest_cache.size_bytes
     AND excluded.modified_ms=media_file_digest_cache.modified_ms
    THEN COALESCE(excluded.suffix_sha256, media_file_digest_cache.suffix_sha256)
    ELSE excluded.suffix_sha256
  END,
  full_sha256=CASE
    WHEN excluded.size_bytes=media_file_digest_cache.size_bytes
     AND excluded.modified_ms=media_file_digest_cache.modified_ms
    THEN COALESCE(excluded.full_sha256, media_file_digest_cache.full_sha256)
    ELSE excluded.full_sha256
  END,
  verified_at_ms=excluded.verified_at_ms
"#,
        params![
            path,
            size_bytes,
            modified_ms,
            prefix,
            suffix,
            full,
            now_ms()
        ],
    )?;
    Ok(())
}

fn record_hash_error(
    conn: &rusqlite::Connection,
    run_id: &str,
    path: &str,
    error: &str,
) -> Result<()> {
    conn.execute(
        "UPDATE media_cleanup_file SET state='attention', last_error=?1, updated_at_ms=?2 WHERE run_id=?3 AND path=?4",
        params![error.chars().take(2000).collect::<String>(), now_ms(), run_id, path],
    )?;
    Ok(())
}

fn count_pending_hash_rows(conn: &rusqlite::Connection, run_id: &str) -> Result<usize> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM media_cleanup_file WHERE run_id=?1 AND state IN ('inventoried','staged_hashed')",
        [run_id],
        |row| row.get(0),
    )?;
    Ok(usize::try_from(count).unwrap_or(usize::MAX))
}

fn get_run_conn(conn: &rusqlite::Connection, run_id: &str) -> Result<Option<MediaCleanupRun>> {
    conn.query_row(
        r#"
SELECT id, roots_json, quarantine_root, status, stage, files_scanned, bytes_scanned,
       duplicate_groups, reclaimable_bytes, last_error, created_at_ms, updated_at_ms
FROM media_cleanup_run WHERE id=?1
"#,
        [run_id],
        |row| {
            let roots_json: String = row.get(1)?;
            Ok(MediaCleanupRun {
                id: row.get(0)?,
                roots: serde_json::from_str(&roots_json).unwrap_or_default(),
                quarantine_root: row.get(2)?,
                status: row.get(3)?,
                stage: row.get(4)?,
                files_scanned: usize::try_from(row.get::<_, i64>(5)?).unwrap_or(0),
                bytes_scanned: u64::try_from(row.get::<_, i64>(6)?).unwrap_or(0),
                duplicate_groups: usize::try_from(row.get::<_, i64>(7)?).unwrap_or(0),
                reclaimable_bytes: u64::try_from(row.get::<_, i64>(8)?).unwrap_or(0),
                last_error: row.get(9)?,
                created_at_ms: row.get(10)?,
                updated_at_ms: row.get(11)?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

fn library_identity_by_normalized_path(
    paths: &AppPaths,
    conn: &rusqlite::Connection,
) -> Result<HashMap<String, (Option<String>, Option<String>)>> {
    let mut stmt = conn.prepare(
        r#"
SELECT li.media_path, li.id, i.media_id
FROM library_item li
LEFT JOIN media_source_identity i
  ON i.library_item_id=li.id AND i.service='youtube'
ORDER BY li.id, i.media_id
"#,
    )?;
    let mut out = HashMap::new();
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let path: String = row.get(0)?;
        let item_id: String = row.get(1)?;
        let media_id: Option<String> = row.get(2)?;
        let mut keys = vec![normalize_path_key(&path)];
        let resolved = root_rebind::resolve_active_alias_path(paths, Path::new(&path), false)?;
        keys.push(normalize_path_key(&resolved.to_string_lossy()));
        if let Ok(canonical) = Path::new(&path).canonicalize() {
            keys.push(normalize_path_key(&canonical.to_string_lossy()));
        }
        keys.sort();
        keys.dedup();
        for key in keys {
            let entry = out.entry(key).or_insert((Some(item_id.clone()), None));
            if entry.0.is_none() {
                entry.0 = Some(item_id.clone());
            }
            if entry.1.is_none() {
                entry.1 = media_id.clone();
            }
        }
    }
    Ok(out)
}

fn paths_overlap(paths: &AppPaths, root: &Path, quarantine: &Path) -> bool {
    let root_keys = path_identity_keys(paths, &root.to_string_lossy());
    let quarantine_keys = path_identity_keys(paths, &quarantine.to_string_lossy());
    root_keys.iter().any(|root_key| {
        quarantine_keys.iter().any(|quarantine_key| {
            quarantine_key == root_key
                || quarantine_key.starts_with(&(root_key.clone() + "\\"))
                || root_key.starts_with(&(quarantine_key.clone() + "\\"))
        })
    })
}

fn normalize_path_key(path: &str) -> String {
    let mut value = path.trim().replace('/', "\\");
    if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
        value = format!(r"\\{rest}");
    } else if let Some(rest) = value.strip_prefix(r"\\?\") {
        value = rest.to_string();
    }
    value.to_ascii_lowercase()
}

fn paths_equivalent(left: &str, right: &str) -> bool {
    if normalize_path_key(left) == normalize_path_key(right) {
        return true;
    }
    let Ok(left) = Path::new(left).canonicalize() else {
        return false;
    };
    let Ok(right) = Path::new(right).canonicalize() else {
        return false;
    };
    normalize_path_key(&left.to_string_lossy()) == normalize_path_key(&right.to_string_lossy())
}

fn paths_equivalent_with_aliases(paths: &AppPaths, left: &str, right: &str) -> bool {
    let left = root_rebind::resolve_active_alias_path(paths, Path::new(left), false)
        .unwrap_or_else(|_| PathBuf::from(left));
    let right = root_rebind::resolve_active_alias_path(paths, Path::new(right), false)
        .unwrap_or_else(|_| PathBuf::from(right));
    paths_equivalent(&left.to_string_lossy(), &right.to_string_lossy())
}

fn is_media_file(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_ascii_lowercase())
            .as_deref(),
        Some(
            "mp4"
                | "mkv"
                | "webm"
                | "mov"
                | "avi"
                | "m4v"
                | "ts"
                | "mpg"
                | "mpeg"
                | "mp3"
                | "m4a"
                | "flac"
                | "wav"
                | "ogg"
        )
    )
}

fn modified_ms(metadata: &std::fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| i64::try_from(value.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| i64::try_from(value.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleanup_overlap_gate_resolves_existing_path_aliases() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_data"));
        let root = dir.path().join("media");
        let quarantine = root.join("quarantine");
        let alias_parent = dir.path().join("alias_parent");
        std::fs::create_dir_all(&quarantine).expect("quarantine");
        std::fs::create_dir_all(&alias_parent).expect("alias parent");
        let aliased_root = alias_parent.join("..").join("media");
        assert!(paths_overlap(&paths, &aliased_root, &quarantine));
    }

    #[test]
    fn cleanup_overlap_gate_resolves_active_root_rebind_aliases() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_data"));
        paths.ensure_dirs().expect("app dirs");
        let physical_root = dir.path().join("media");
        let historical_root = dir.path().join("historical_media");
        std::fs::create_dir_all(&physical_root).expect("physical root");
        let mut aliases = root_rebind::RootAliasesConfig::default();
        aliases.aliases.push(root_rebind::RootAliasRecord {
            id: "cleanup-overlap".to_string(),
            from_root: historical_root.to_string_lossy().to_string(),
            to_root: physical_root.to_string_lossy().to_string(),
            verified_at_ms: 1,
            status: "active".to_string(),
            receipt_path: "fixture.json".to_string(),
        });
        std::fs::write(
            paths.root_aliases_config_path(),
            serde_json::to_vec_pretty(&aliases).expect("serialize aliases"),
        )
        .expect("write aliases");

        assert!(paths_overlap(
            &paths,
            &physical_root,
            &historical_root.join("quarantine")
        ));
    }

    #[test]
    fn youtube_filename_identity_accepts_all_letter_ids_and_prefers_trailing_token() {
        assert_eq!(
            youtube_id_from_media_filename("HelloWorldX - ABCDEFGHIJK.mkv").as_deref(),
            Some("ABCDEFGHIJK")
        );
        assert_eq!(
            youtube_id_from_media_filename("managed_title_ABCDEFGHIJK.mkv").as_deref(),
            Some("ABCDEFGHIJK")
        );
        assert_eq!(
            youtube_id_from_media_filename("managed_title_[ABCDEFGHIJK].mkv").as_deref(),
            Some("ABCDEFGHIJK")
        );
        assert_eq!(
            youtube_id_from_media_filename("ABCDEFGHIJK unrelated title.mkv"),
            None,
            "an arbitrary earlier 11-character word is not canonical identity evidence"
        );
    }

    #[test]
    fn cleanup_path_identity_gate_resolves_existing_aliases() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_data"));
        let media = dir.path().join("media");
        let alias_parent = dir.path().join("alias_parent");
        std::fs::create_dir_all(&media).expect("media");
        std::fs::create_dir_all(&alias_parent).expect("alias parent");
        let physical = media.join("same.mkv");
        std::fs::write(&physical, b"same bytes").expect("media file");
        let alias = alias_parent.join("..").join("media").join("same.mkv");
        assert!(paths_equivalent_with_aliases(
            &paths,
            &physical.to_string_lossy(),
            &alias.to_string_lossy()
        ));
    }

    #[test]
    fn cleanup_apply_transaction_blocks_concurrent_queue_state_writes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_data"));
        db::ensure_schema(&paths).expect("schema");
        let mut owner = db::open(&paths).expect("owner db");
        owner
            .execute(
                "INSERT INTO meta(key,value) VALUES('jobs_queue_paused','1') ON CONFLICT(key) DO UPDATE SET value='1'",
                [],
            )
            .expect("pause queue");

        let tx = begin_cleanup_apply_transaction(&mut owner).expect("cleanup transaction");
        let contender = db::open(&paths).expect("contender db");
        contender
            .busy_timeout(std::time::Duration::ZERO)
            .expect("zero busy timeout");
        let error = contender
            .execute(
                "UPDATE meta SET value='0' WHERE key='jobs_queue_paused'",
                [],
            )
            .expect_err("cleanup transaction must retain the SQLite writer boundary");
        assert!(matches!(
            error,
            rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error {
                    code: rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked,
                    ..
                },
                _
            )
        ));

        drop(tx);
        assert_eq!(
            contender
                .execute(
                    "UPDATE meta SET value='0' WHERE key='jobs_queue_paused'",
                    [],
                )
                .expect("write after cleanup transaction"),
            1
        );
    }

    #[test]
    fn cleanup_apply_boundary_rejects_every_running_job_type() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_data"));
        db::ensure_schema(&paths).expect("schema");
        let mut conn = db::open(&paths).expect("db");
        conn.execute(
            "INSERT INTO meta(key,value) VALUES('jobs_queue_paused','1') ON CONFLICT(key) DO UPDATE SET value='1'",
            [],
        )
        .expect("pause queue");
        conn.execute(
            "INSERT INTO job(id,type,status,progress,params_json,created_at_ms,logs_path) VALUES('running-localization','asr_local','running',0,'{}',1,'fixture.log')",
            [],
        )
        .expect("running localization job");

        let error = begin_cleanup_apply_transaction(&mut conn)
            .expect_err("any running job must block cleanup apply");
        assert!(error.to_string().contains("zero running jobs; found 1"));
        conn.execute(
            "UPDATE job SET status='canceled' WHERE id='running-localization'",
            [],
        )
        .expect("cancel fixture job");
        let tx = begin_cleanup_apply_transaction(&mut conn)
            .expect("cleanup transaction after running work drains");
        drop(tx);
    }

    #[test]
    fn cleanup_scan_and_hash_steps_yield_to_running_jobs() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_data"));
        db::ensure_schema(&paths).expect("schema");
        let media_root = dir.path().join("media");
        std::fs::create_dir_all(&media_root).expect("media root");
        std::fs::write(media_root.join("single.mkv"), b"single-media").expect("media");
        let run =
            create_inventory_run(&paths, vec![media_root.to_string_lossy().to_string()], None)
                .expect("run");
        let conn = db::open(&paths).expect("db");
        conn.execute(
            "INSERT INTO meta(key,value) VALUES('jobs_queue_paused','1') ON CONFLICT(key) DO UPDATE SET value='1'",
            [],
        )
        .expect("pause queue");
        conn.execute(
            "INSERT INTO job(id,type,status,progress,params_json,created_at_ms,logs_path) VALUES('cleanup-yield-job','youtube_download','running',0,'{}',1,'fixture.log')",
            [],
        )
        .expect("running job");
        drop(conn);

        let yielded_inventory =
            advance_inventory(&paths, &run.id, Some(100)).expect("inventory yield");
        assert_eq!(yielded_inventory.processed_files, 0);
        assert_eq!(yielded_inventory.run.stage, "inventory");
        let conn = db::open(&paths).expect("resume db");
        conn.execute(
            "UPDATE job SET status='canceled' WHERE id='cleanup-yield-job'",
            [],
        )
        .expect("cancel fixture job");
        drop(conn);
        while get_run(&paths, &run.id)
            .expect("run")
            .expect("run exists")
            .stage
            == "inventory"
        {
            advance_inventory(&paths, &run.id, Some(100)).expect("inventory resume");
        }
        reconciliation_preview(&paths, &run.id).expect("preview");
        apply_reconciliation(&paths, &run.id).expect("apply reconciliation");
        let conn = db::open(&paths).expect("running hash db");
        conn.execute(
            "UPDATE job SET status='running' WHERE id='cleanup-yield-job'",
            [],
        )
        .expect("restart fixture job");
        drop(conn);

        let yielded_hash = advance_hashing(&paths, &run.id, Some(100)).expect("hash yield");
        assert_eq!(yielded_hash.processed_files, 0);
        assert_eq!(yielded_hash.run.stage, "hashing");
        let conn = db::open(&paths).expect("finish hash db");
        conn.execute(
            "UPDATE job SET status='canceled' WHERE id='cleanup-yield-job'",
            [],
        )
        .expect("cancel hash fixture job");
        drop(conn);
        let resumed_hash = advance_hashing(&paths, &run.id, Some(100)).expect("hash resume");
        assert_eq!(resumed_hash.run.stage, "review");
    }

    #[test]
    fn cleanup_rollback_requires_the_same_paused_queue_boundary() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_data"));
        db::ensure_schema(&paths).expect("schema");
        let error = rollback_run(&paths, "missing-run")
            .expect_err("rollback must refuse an unpaused queue before filesystem work");
        assert!(error
            .to_string()
            .contains("media cleanup mutation requires the global queue to remain paused"));
    }

    #[test]
    fn latest_cleanup_run_is_recovered_from_sqlite_after_restart() {
        let dir = tempfile::tempdir().expect("tempdir");
        let app_root = dir.path().join("app_data");
        let paths = AppPaths::new(app_root.clone());
        db::ensure_schema(&paths).expect("schema");
        let media_root = dir.path().join("media");
        std::fs::create_dir_all(&media_root).expect("media root");

        assert!(latest_run(&paths).expect("empty lookup").is_none());
        let first =
            create_inventory_run(&paths, vec![media_root.to_string_lossy().to_string()], None)
                .expect("first run");
        std::thread::sleep(std::time::Duration::from_millis(2));
        let second =
            create_inventory_run(&paths, vec![media_root.to_string_lossy().to_string()], None)
                .expect("second run");

        let restarted_paths = AppPaths::new(app_root);
        let recovered = latest_run(&restarted_paths)
            .expect("restart lookup")
            .expect("latest run");
        assert_eq!(recovered.id, second.id);
        assert_ne!(recovered.id, first.id);
    }

    fn advance_to_review(paths: &AppPaths, run_id: &str) {
        loop {
            let run = get_run(paths, run_id).expect("run").expect("exists");
            match run.stage.as_str() {
                "inventory" => {
                    advance_inventory(paths, run_id, Some(2)).expect("inventory");
                }
                "reconciliation" => {
                    reconciliation_preview(paths, run_id).expect("reconciliation preview");
                    apply_reconciliation(paths, run_id).expect("reconciliation apply");
                }
                "hashing" => {
                    advance_hashing(paths, run_id, Some(2)).expect("hash");
                }
                "review" => break,
                other => panic!("unexpected stage {other}"),
            }
        }
    }

    #[test]
    fn reconciliation_is_dry_run_first_and_only_applies_unique_evidence() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_data"));
        db::ensure_schema(&paths).expect("schema");
        let conn = db::open(&paths).expect("db");
        conn.execute(
            "INSERT INTO meta(key,value) VALUES('jobs_queue_paused','1') ON CONFLICT(key) DO UPDATE SET value='1'",
            [],
        )
        .expect("pause queue");
        let media_root = dir.path().join("media");
        std::fs::create_dir_all(&media_root).expect("media root");
        let recovered = media_root.join("Channel - dQw4w9WgXcQ.mkv");
        let unmatched = media_root.join("unmatched-local-video.mkv");
        let missing = media_root.join("old - dQw4w9WgXcQ.mkv");
        std::fs::write(&recovered, b"recovered-video").expect("recovered physical");
        std::fs::write(&unmatched, b"unmatched-video").expect("unmatched physical");
        conn.execute(
            "INSERT INTO library_item(id,created_at_ms,source_type,source_uri,title,media_path,origin) VALUES('missing-owner',1,'url_direct','https://youtu.be/dQw4w9WgXcQ','Missing owner',?1,'youtube')",
            [missing.to_string_lossy().to_string()],
        )
        .expect("missing library row");
        conn.execute(
            "INSERT INTO media_source_identity(service,media_id,canonical_url,library_item_id,repair_state,created_at_ms,updated_at_ms) VALUES('youtube','dQw4w9WgXcQ','https://www.youtube.com/watch?v=dQw4w9WgXcQ','missing-owner','missing',1,1)",
            [],
        )
        .expect("missing identity");
        drop(conn);

        let run =
            create_inventory_run(&paths, vec![media_root.to_string_lossy().to_string()], None)
                .expect("run");
        while get_run(&paths, &run.id)
            .expect("run")
            .expect("run exists")
            .stage
            == "inventory"
        {
            advance_inventory(&paths, &run.id, Some(100)).expect("inventory");
        }

        let preview = reconciliation_preview(&paths, &run.id).expect("preview");
        assert_eq!(preview.deterministic_relinks, 1);
        assert_eq!(preview.physical_files_to_index, 1);
        assert_eq!(preview.review_only, 0);
        let conn = db::open(&paths).expect("dry-run db");
        assert_eq!(
            conn.query_row(
                "SELECT media_path FROM library_item WHERE id='missing-owner'",
                [],
                |row| row.get::<_, String>(0),
            )
            .expect("dry-run path"),
            missing.to_string_lossy()
        );
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM library_item WHERE media_path=?1",
                [unmatched.to_string_lossy().to_string()],
                |row| row.get::<_, i64>(0),
            )
            .expect("dry-run unmatched count"),
            0,
            "preview must not index physical-only media"
        );
        drop(conn);

        let applied = apply_reconciliation(&paths, &run.id).expect("apply reconciliation");
        assert_eq!(applied.applied, 2);
        assert_eq!(applied.failed, 0);
        assert_eq!(
            get_run(&paths, &run.id)
                .expect("run")
                .expect("run exists")
                .stage,
            "hashing"
        );
        let conn = db::open(&paths).expect("applied db");
        let relinked_path: String = conn
            .query_row(
                "SELECT media_path FROM library_item WHERE id='missing-owner'",
                [],
                |row| row.get(0),
            )
            .expect("relinked path");
        assert!(paths_equivalent(
            &relinked_path,
            &recovered.to_string_lossy()
        ));
        assert_eq!(
            library_items_for_media_path(&paths, &conn, &unmatched.to_string_lossy())
                .expect("indexed unmatched rows")
                .len(),
            1
        );
        drop(conn);
        let rolled_back = rollback_run(&paths, &run.id).expect("rollback reconciliation");
        assert_eq!(rolled_back.applied_actions, 1);
        assert_eq!(rolled_back.failed_actions, 0);
        let conn = db::open(&paths).expect("rollback db");
        assert_eq!(
            conn.query_row(
                "SELECT media_path FROM library_item WHERE id='missing-owner'",
                [],
                |row| row.get::<_, String>(0),
            )
            .expect("restored missing path"),
            missing.to_string_lossy()
        );
        assert_eq!(
            library_items_for_media_path(&paths, &conn, &unmatched.to_string_lossy())
                .expect("preserved indexed metadata")
                .len(),
            1,
            "rollback must preserve newly indexed library metadata"
        );
    }

    #[test]
    fn reconciliation_failure_remains_retryable_after_restart() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_data"));
        db::ensure_schema(&paths).expect("schema");
        let conn = db::open(&paths).expect("db");
        conn.execute(
            "INSERT INTO meta(key,value) VALUES('jobs_queue_paused','1') ON CONFLICT(key) DO UPDATE SET value='1'",
            [],
        )
        .expect("pause queue");
        let media_root = dir.path().join("media");
        std::fs::create_dir_all(&media_root).expect("media root");
        let recovered = media_root.join("Channel - dQw4w9WgXcQ.mkv");
        let missing = media_root.join("old - dQw4w9WgXcQ.mkv");
        std::fs::write(&recovered, b"recovered-video").expect("recovered physical");
        conn.execute(
            "INSERT INTO library_item(id,created_at_ms,source_type,source_uri,title,media_path,origin) VALUES('retry-owner',1,'url_direct','https://youtu.be/dQw4w9WgXcQ','Retry owner',?1,'youtube')",
            [missing.to_string_lossy().to_string()],
        )
        .expect("missing library row");
        conn.execute(
            "INSERT INTO media_source_identity(service,media_id,canonical_url,library_item_id,repair_state,created_at_ms,updated_at_ms) VALUES('youtube','dQw4w9WgXcQ','https://www.youtube.com/watch?v=dQw4w9WgXcQ','retry-owner','missing',1,1)",
            [],
        )
        .expect("missing identity");
        drop(conn);

        let run =
            create_inventory_run(&paths, vec![media_root.to_string_lossy().to_string()], None)
                .expect("run");
        while get_run(&paths, &run.id)
            .expect("run")
            .expect("run exists")
            .stage
            == "inventory"
        {
            advance_inventory(&paths, &run.id, Some(100)).expect("inventory");
        }
        let preview = reconciliation_preview(&paths, &run.id).expect("preview");
        assert_eq!(preview.deterministic_relinks, 1);

        std::fs::write(&missing, b"appeared-after-preview").expect("racing old path");
        let failed = apply_reconciliation(&paths, &run.id).expect("failed apply receipt");
        assert_eq!(failed.applied, 0);
        assert_eq!(failed.failed, 1);
        assert_eq!(failed.candidates[0].disposition, "deterministic_relink");
        assert!(failed.candidates[0].error.is_some());
        assert_eq!(
            get_run(&paths, &run.id)
                .expect("run")
                .expect("run exists")
                .stage,
            "reconciliation"
        );

        std::fs::remove_file(&missing).expect("remove disposable race fixture");
        let retried = apply_reconciliation(&paths, &run.id).expect("retry apply");
        assert_eq!(retried.applied, 1);
        assert_eq!(retried.failed, 0);
        assert_eq!(
            get_run(&paths, &run.id)
                .expect("run")
                .expect("run exists")
                .stage,
            "hashing"
        );
        let relinked_path: String = db::open(&paths)
            .expect("retry db")
            .query_row(
                "SELECT media_path FROM library_item WHERE id='retry-owner'",
                [],
                |row| row.get(0),
            )
            .expect("retry relinked path");
        assert!(paths_equivalent(
            &relinked_path,
            &recovered.to_string_lossy()
        ));
    }

    #[test]
    fn reconciliation_refuses_replaced_file_with_preserved_size_and_mtime() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_data"));
        db::ensure_schema(&paths).expect("schema");
        db::open(&paths)
            .expect("queue db")
            .execute(
                "INSERT INTO meta(key,value) VALUES('jobs_queue_paused','1') ON CONFLICT(key) DO UPDATE SET value='1'",
                [],
            )
            .expect("pause queue");
        let media_root = dir.path().join("media");
        std::fs::create_dir_all(&media_root).expect("media root");
        let physical = media_root.join("unmatched.mkv");
        let original = media_root.join("original-held-open.mkv");
        std::fs::write(&physical, b"original-bytes").expect("original physical");
        let original_mtime = filetime::FileTime::from_last_modification_time(
            &std::fs::metadata(&physical).expect("original metadata"),
        );

        let run =
            create_inventory_run(&paths, vec![media_root.to_string_lossy().to_string()], None)
                .expect("run");
        while get_run(&paths, &run.id)
            .expect("run")
            .expect("run exists")
            .stage
            == "inventory"
        {
            advance_inventory(&paths, &run.id, Some(100)).expect("inventory");
        }
        let preview = reconciliation_preview(&paths, &run.id).expect("preview");
        assert_eq!(preview.physical_files_to_index, 1);

        std::fs::rename(&physical, &original).expect("retain original identity");
        std::fs::write(&physical, b"replaced-bytes").expect("replacement physical");
        filetime::set_file_mtime(&physical, original_mtime).expect("restore visible mtime");
        assert_eq!(
            std::fs::metadata(&physical)
                .expect("replacement metadata")
                .len(),
            b"original-bytes".len() as u64
        );

        let result = apply_reconciliation(&paths, &run.id).expect("failure receipt");
        assert_eq!(result.applied, 0);
        assert_eq!(result.failed, 1);
        assert!(result.candidates[0]
            .error
            .as_deref()
            .is_some_and(|error| error.contains("changed after inventory")));
        assert_eq!(
            db::open(&paths)
                .expect("verification db")
                .query_row(
                    "SELECT COUNT(*) FROM library_item WHERE media_path=?1",
                    [physical.to_string_lossy().to_string()],
                    |row| row.get::<_, i64>(0),
                )
                .expect("indexed count"),
            0
        );
    }

    #[test]
    fn reconciliation_keeps_non_unique_identity_matches_review_only() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_data"));
        db::ensure_schema(&paths).expect("schema");
        db::open(&paths)
            .expect("queue db")
            .execute(
                "INSERT INTO meta(key,value) VALUES('jobs_queue_paused','1') ON CONFLICT(key) DO UPDATE SET value='1'",
                [],
            )
            .expect("pause queue");
        let media_root = dir.path().join("media");
        std::fs::create_dir_all(&media_root).expect("media root");
        for name in ["first - dQw4w9WgXcQ.mkv", "second - dQw4w9WgXcQ.mkv"] {
            std::fs::write(media_root.join(name), name.as_bytes()).expect("physical variant");
        }
        let missing = media_root.join("old - dQw4w9WgXcQ.mkv");
        let conn = db::open(&paths).expect("db");
        conn.execute(
            "INSERT INTO library_item(id,created_at_ms,source_type,source_uri,title,media_path,origin) VALUES('ambiguous-owner',1,'url_direct','https://youtu.be/dQw4w9WgXcQ','Ambiguous owner',?1,'youtube')",
            [missing.to_string_lossy().to_string()],
        )
        .expect("ambiguous library row");
        conn.execute(
            "INSERT INTO media_source_identity(service,media_id,canonical_url,library_item_id,repair_state,created_at_ms,updated_at_ms) VALUES('youtube','dQw4w9WgXcQ','https://www.youtube.com/watch?v=dQw4w9WgXcQ','ambiguous-owner','missing',1,1)",
            [],
        )
        .expect("ambiguous identity");
        drop(conn);
        let run =
            create_inventory_run(&paths, vec![media_root.to_string_lossy().to_string()], None)
                .expect("run");
        while get_run(&paths, &run.id)
            .expect("run")
            .expect("run exists")
            .stage
            == "inventory"
        {
            advance_inventory(&paths, &run.id, Some(100)).expect("inventory");
        }
        let preview = reconciliation_preview(&paths, &run.id).expect("preview");
        assert_eq!(preview.deterministic_relinks, 0);
        assert_eq!(preview.physical_files_to_index, 0);
        assert_eq!(preview.review_only, 3);
        assert!(preview
            .candidates
            .iter()
            .all(|candidate| candidate.disposition == "review_only"));
        assert_eq!(
            db::open(&paths)
                .expect("db")
                .query_row(
                    "SELECT media_path FROM library_item WHERE id='ambiguous-owner'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .expect("preserved path"),
            missing.to_string_lossy()
        );
        let applied = apply_reconciliation(&paths, &run.id).expect("apply no safe candidates");
        assert_eq!(applied.applied, 0);
        while get_run(&paths, &run.id)
            .expect("run")
            .expect("run exists")
            .stage
            == "hashing"
        {
            advance_hashing(&paths, &run.id, Some(100)).expect("hashing");
        }
        let variants = list_variants(&paths, &run.id).expect("variants");
        assert_eq!(variants.len(), 1);
        assert_eq!(variants[0].service, "youtube");
        assert_eq!(variants[0].media_id, "dQw4w9WgXcQ");
        assert_eq!(variants[0].member_paths.len(), 2);
        assert_eq!(variants[0].status, "review_only");
    }

    #[test]
    fn cleanup_relink_journal_reads_legacy_actions() {
        let journal =
            parse_cleanup_relink_journal(r#"["legacy-media-id"]"#).expect("legacy journal");
        assert_eq!(journal.version, 0);
        assert_eq!(journal.media_ids, vec!["legacy-media-id"]);
        assert_eq!(journal.source_library_media_path, None);
    }

    #[test]
    fn historical_mp4_cleanup_identity_matches_alias_resolved_physical_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_data"));
        db::ensure_schema(&paths).expect("schema");
        let target_root = dir.path().join("archive");
        std::fs::create_dir_all(&target_root).expect("target root");
        let physical_mp4 = target_root.join("legacy.mp4");
        std::fs::write(&physical_mp4, b"historical mp4").expect("historical fixture");
        let stored_mp4 = r"C:\old_archive\legacy.mp4";
        let conn = db::open(&paths).expect("db");
        conn.execute(
            "INSERT INTO library_item(id,created_at_ms,source_type,source_uri,title,media_path,container) VALUES('legacy-cleanup',1,'local_file',?1,'Legacy cleanup',?1,'mp4')",
            [stored_mp4],
        )
        .expect("historical library row");
        let aliases = crate::root_rebind::RootAliasesConfig {
            schema_version: 1,
            aliases: vec![crate::root_rebind::RootAliasRecord {
                id: "cleanup-alias".to_string(),
                from_root: r"C:\old_archive".to_string(),
                to_root: target_root.to_string_lossy().to_string(),
                verified_at_ms: 1,
                status: "active".to_string(),
                receipt_path: "receipt.json".to_string(),
            }],
        };
        crate::persistence::atomic_write_text(
            &paths.root_aliases_config_path(),
            &format!("{}\n", serde_json::to_string_pretty(&aliases).unwrap()),
        )
        .expect("alias config");

        let map = library_identity_by_normalized_path(&paths, &conn).expect("identity map");
        let physical_key = normalize_path_key(&physical_mp4.to_string_lossy());
        assert_eq!(
            map.get(&physical_key).and_then(|entry| entry.0.as_deref()),
            Some("legacy-cleanup")
        );
        assert!(paths_equivalent_with_aliases(
            &paths,
            stored_mp4,
            &physical_mp4.to_string_lossy()
        ));
    }

    #[test]
    fn inventory_hash_quarantine_and_rollback_are_recoverable() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_data"));
        db::ensure_schema(&paths).expect("schema");
        let queue = db::open(&paths).expect("queue state db");
        queue
            .execute(
                "INSERT INTO meta(key,value) VALUES('jobs_queue_paused','1') ON CONFLICT(key) DO UPDATE SET value='1'",
                [],
            )
            .expect("pause queue for cleanup apply boundary");
        drop(queue);
        let media_root = dir.path().join("media");
        let quarantine = dir.path().join("quarantine");
        std::fs::create_dir_all(&media_root).expect("media root");
        let keeper = media_root.join("a_keeper.mp4");
        let duplicate = media_root.join("b_duplicate.mp4");
        let distinct = media_root.join("distinct.mp4");
        std::fs::write(&keeper, b"identical-video-bytes").expect("keeper");
        std::fs::write(&duplicate, b"identical-video-bytes").expect("duplicate");
        std::fs::write(&distinct, b"different-video-bytes").expect("distinct");

        let conn = db::open(&paths).expect("db");
        conn.execute(
            "INSERT INTO library_item (id, created_at_ms, source_type, source_uri, title, media_path, origin) VALUES ('keeper-item', 1, 'local_file', ?1, 'Keeper', ?1, '4kvdp_import')",
            [keeper.to_string_lossy().to_string()],
        )
        .expect("keeper item");
        conn.execute(
            "INSERT INTO library_item (id, created_at_ms, source_type, source_uri, title, media_path, origin) VALUES ('duplicate-item', 2, 'local_file', ?1, 'Duplicate', ?1, '4kvdp_import')",
            [duplicate.to_string_lossy().to_string()],
        )
        .expect("duplicate item");
        conn.execute(
            "INSERT INTO library_item (id, created_at_ms, source_type, source_uri, title, media_path, origin) VALUES ('duplicate-item-secondary', 3, 'local_file', ?1, 'Duplicate secondary owner', ?1, 'instagram_import')",
            [duplicate.to_string_lossy().to_string()],
        )
        .expect("secondary duplicate item");
        conn.execute(
            "INSERT INTO media_source_identity (service, media_id, canonical_url, library_item_id, repair_state, created_at_ms, updated_at_ms) VALUES ('youtube', 'dup12345678', 'https://youtu.be/dup12345678', 'duplicate-item', 'ready', 1, 1)",
            [],
        )
        .expect("identity");
        conn.execute(
            "INSERT INTO media_source_identity (service, media_id, canonical_url, library_item_id, repair_state, created_at_ms, updated_at_ms) VALUES ('instagram', 'dup-secondary', 'https://instagram.com/p/dup-secondary', 'duplicate-item-secondary', 'ready', 1, 1)",
            [],
        )
        .expect("secondary identity");
        for media_path in [&duplicate, &keeper] {
            conn.execute(
                "INSERT INTO media_availability_observation(path,state,observed_at_ms,source,duration_ms,next_refresh_at_ms,invalidated_at_ms) VALUES(?1,'present',1,'cleanup-fixture',1,9999999999999,NULL)",
                [media_path.to_string_lossy().to_string()],
            )
            .expect("seed availability observation");
        }
        let library_map =
            library_identity_by_normalized_path(&paths, &conn).expect("library path map");
        assert_eq!(
            library_map
                .get(&normalize_path_key(&duplicate.to_string_lossy()))
                .and_then(|entry| entry.0.as_deref()),
            Some("duplicate-item")
        );
        drop(conn);

        let run = create_inventory_run(
            &paths,
            vec![media_root.to_string_lossy().to_string()],
            Some(quarantine.to_string_lossy().to_string()),
        )
        .expect("run");
        advance_to_review(&paths, &run.id);
        let groups = list_groups(&paths, &run.id).expect("groups");
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].member_count, 2);
        assert_eq!(
            groups[0].reclaimable_bytes,
            b"identical-video-bytes".len() as u64
        );
        let duplicate_member = groups[0]
            .members
            .iter()
            .find(|member| paths_equivalent(&member.path, &duplicate.to_string_lossy()))
            .unwrap_or_else(|| panic!("duplicate member missing: {:?}", groups[0].members));
        assert_eq!(
            duplicate_member.library_item_id.as_deref(),
            Some("duplicate-item"),
            "duplicate member: {duplicate_member:?}"
        );
        let duplicate_cleanup_path = duplicate_member.path.clone();

        let approved_group = set_group_decision(
            &paths,
            &run.id,
            &groups[0].group_id,
            "approved",
            Some(&keeper.to_string_lossy()),
        )
        .expect("approve");
        assert_eq!(
            approved_group.keeper_library_item_id.as_deref(),
            Some("keeper-item")
        );
        let applied = apply_approved_groups(&paths, &run.id).expect("apply");
        assert_eq!(applied.applied_actions, 1);
        assert!(set_group_decision(
            &paths,
            &run.id,
            &groups[0].group_id,
            "approved",
            Some(&keeper.to_string_lossy()),
        )
        .expect_err("post-apply decisions must be refused")
        .to_string()
        .contains("requires review stage"));
        assert!(apply_approved_groups(&paths, &run.id)
            .expect_err("repeat quarantine must be refused")
            .to_string()
            .contains("requires review stage"));
        assert!(keeper.is_file());
        assert!(!duplicate.exists());
        assert!(distinct.is_file());
        let conn = db::open(&paths).expect("db after apply");
        let applied_library_path: String = conn
            .query_row(
                "SELECT media_path FROM library_item WHERE id='duplicate-item'",
                [],
                |row| row.get(0),
            )
            .expect("applied library path");
        assert!(paths_equivalent(
            &applied_library_path,
            &keeper.to_string_lossy()
        ));
        let applied_secondary_library_path: String = conn
            .query_row(
                "SELECT media_path FROM library_item WHERE id='duplicate-item-secondary'",
                [],
                |row| row.get(0),
            )
            .expect("applied secondary library path");
        assert!(paths_equivalent(
            &applied_secondary_library_path,
            &keeper.to_string_lossy()
        ));
        let applied_identity_item: String = conn
            .query_row(
                "SELECT library_item_id FROM media_source_identity WHERE service='youtube' AND media_id='dup12345678'",
                [],
                |row| row.get(0),
            )
            .expect("applied identity");
        assert_eq!(applied_identity_item, "keeper-item");
        let applied_secondary_identity_item: String = conn
            .query_row(
                "SELECT library_item_id FROM media_source_identity WHERE service='instagram' AND media_id='dup-secondary'",
                [],
                |row| row.get(0),
            )
            .expect("applied secondary identity");
        assert_eq!(applied_secondary_identity_item, "keeper-item");
        for media_path in [&duplicate, &keeper] {
            let invalidated_at: Option<i64> = conn
                .query_row(
                    "SELECT invalidated_at_ms FROM media_availability_observation WHERE path=?1",
                    [media_path.to_string_lossy().to_string()],
                    |row| row.get(0),
                )
                .expect("applied observation");
            assert!(
                invalidated_at.is_some(),
                "cleanup apply must invalidate both old and new library paths: {}",
                media_path.to_string_lossy()
            );
            conn.execute(
                "UPDATE media_availability_observation SET invalidated_at_ms=NULL,next_refresh_at_ms=9999999999999 WHERE path=?1",
                [media_path.to_string_lossy().to_string()],
            )
            .expect("reseed observation before rollback");
        }
        conn.execute(
            "INSERT INTO media_availability_observation(path,state,observed_at_ms,source,duration_ms,next_refresh_at_ms,invalidated_at_ms) VALUES(?1,'present',2,'rollback-fixture',1,9999999999999,NULL) ON CONFLICT(path) DO UPDATE SET invalidated_at_ms=NULL,next_refresh_at_ms=9999999999999",
            [&applied_library_path],
        )
        .expect("seed exact pre-rollback library path");
        let preserved_title: String = conn
            .query_row(
                "SELECT title FROM library_item WHERE id='duplicate-item'",
                [],
                |row| row.get(0),
            )
            .expect("preserved title");
        assert_eq!(preserved_title, "Duplicate");
        drop(conn);

        let rolled_back = rollback_run(&paths, &run.id).expect("rollback");
        assert_eq!(rolled_back.applied_actions, 1);
        assert!(keeper.is_file());
        assert!(duplicate.is_file());
        assert_eq!(
            std::fs::read(&keeper).expect("keeper bytes"),
            std::fs::read(&duplicate).expect("duplicate bytes")
        );
        let conn = db::open(&paths).expect("db after rollback");
        let rolled_back_library_path: String = conn
            .query_row(
                "SELECT media_path FROM library_item WHERE id='duplicate-item'",
                [],
                |row| row.get(0),
            )
            .expect("rolled-back library path");
        assert_eq!(
            rolled_back_library_path,
            duplicate.to_string_lossy().to_string()
        );
        let rolled_back_secondary_library_path: String = conn
            .query_row(
                "SELECT media_path FROM library_item WHERE id='duplicate-item-secondary'",
                [],
                |row| row.get(0),
            )
            .expect("rolled-back secondary library path");
        assert_eq!(
            rolled_back_secondary_library_path,
            duplicate.to_string_lossy().to_string()
        );
        let rolled_back_identity_item: String = conn
            .query_row(
                "SELECT library_item_id FROM media_source_identity WHERE service='youtube' AND media_id='dup12345678'",
                [],
                |row| row.get(0),
            )
            .expect("rolled-back identity");
        assert_eq!(rolled_back_identity_item, "duplicate-item");
        let rolled_back_secondary_identity_item: String = conn
            .query_row(
                "SELECT library_item_id FROM media_source_identity WHERE service='instagram' AND media_id='dup-secondary'",
                [],
                |row| row.get(0),
            )
            .expect("rolled-back secondary identity");
        assert_eq!(
            rolled_back_secondary_identity_item,
            "duplicate-item-secondary"
        );
        let duplicate_text = duplicate.to_string_lossy().to_string();
        for media_path in [applied_library_path.as_str(), duplicate_text.as_str()] {
            let invalidated_at: Option<i64> = conn
                .query_row(
                    "SELECT invalidated_at_ms FROM media_availability_observation WHERE path=?1",
                    [media_path],
                    |row| row.get(0),
                )
                .expect("rolled-back observation");
            assert!(
                invalidated_at.is_some(),
                "cleanup rollback must invalidate both old and restored library paths: {}",
                media_path
            );
        }
        let rolled_back_file_state: String = conn
            .query_row(
                "SELECT state FROM media_cleanup_file WHERE run_id=?1 AND path=?2",
                params![run.id, duplicate_cleanup_path],
                |row| row.get(0),
            )
            .expect("rolled-back cleanup file state");
        assert_eq!(rolled_back_file_state, "fully_hashed");
        drop(conn);

        let reapplied = apply_approved_groups(&paths, &run.id).expect("reapply");
        assert_eq!(reapplied.applied_actions, 1);
        let conn = db::open(&paths).expect("rollback failure trigger db");
        let attention_quarantine_path: String = conn
            .query_row(
                "SELECT quarantine_path FROM media_cleanup_action WHERE run_id=?1 AND status='applied' ORDER BY created_at_ms DESC LIMIT 1",
                [&run.id],
                |row| row.get(0),
            )
            .expect("active quarantine path");
        conn.execute(
            "INSERT INTO media_availability_observation(path,state,observed_at_ms,source,duration_ms,next_refresh_at_ms,invalidated_at_ms) VALUES(?1,'present',3,'attention-fixture',1,9999999999999,NULL) ON CONFLICT(path) DO UPDATE SET invalidated_at_ms=NULL,next_refresh_at_ms=9999999999999",
            [&attention_quarantine_path],
        )
        .expect("seed quarantine observation before failed rollback");
        conn.execute_batch(
            r#"
CREATE TRIGGER force_cleanup_rollback_library_update_failure
BEFORE UPDATE OF media_path ON library_item
WHEN OLD.id='duplicate-item'
BEGIN
  SELECT RAISE(ABORT, 'forced cleanup rollback metadata failure');
END;
"#,
        )
        .expect("rollback failure trigger");
        drop(conn);

        let failed_rollback = rollback_run(&paths, &run.id).expect("failed rollback summary");
        assert_eq!(failed_rollback.applied_actions, 0);
        assert_eq!(failed_rollback.failed_actions, 1);
        assert!(!duplicate.exists());
        let conn = db::open(&paths).expect("db after failed rollback");
        let (library_path, action_status, quarantine_path): (String, String, String) = conn
            .query_row(
                r#"
SELECT li.media_path, action.status, action.quarantine_path
FROM library_item li
JOIN media_cleanup_action action ON action.source_library_item_id=li.id
WHERE li.id='duplicate-item' AND action.status='attention'
ORDER BY action.created_at_ms DESC
LIMIT 1
"#,
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("failed rollback state");
        assert!(paths_equivalent(&library_path, &keeper.to_string_lossy()));
        assert_eq!(action_status, "attention");
        assert!(Path::new(&quarantine_path).is_file());
        drop(conn);

        // Restart proof: the independent attention-path transaction must have invalidated every
        // location whose physical truth may have changed, even though the metadata transaction
        // rolled back and compensation returned the file to quarantine.
        let restarted = db::open(&paths).expect("restart observation db");
        for path in [
            keeper.to_string_lossy().to_string(),
            duplicate.to_string_lossy().to_string(),
            attention_quarantine_path,
        ] {
            let invalidated: Option<i64> = restarted
                .query_row(
                    "SELECT invalidated_at_ms FROM media_availability_observation WHERE path=?1",
                    [&path],
                    |row| row.get(0),
                )
                .optional()
                .expect("attention observation query")
                .flatten();
            assert!(
                invalidated.is_some(),
                "attention recovery must not leave a restart-stable observation for {path}"
            );
        }
    }

    #[test]
    fn apply_database_failure_restores_source_and_preserves_metadata_truth() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_data"));
        db::ensure_schema(&paths).expect("schema");
        db::open(&paths)
            .unwrap()
            .execute(
                "INSERT INTO meta(key,value) VALUES('jobs_queue_paused','1') ON CONFLICT(key) DO UPDATE SET value='1'",
                [],
            )
            .expect("pause queue for cleanup boundary");
        let media_root = dir.path().join("media");
        let quarantine = dir.path().join("quarantine");
        std::fs::create_dir_all(&media_root).expect("media root");
        let keeper = media_root.join("a_keeper.mp4");
        let duplicate = media_root.join("b_duplicate.mp4");
        std::fs::write(&keeper, b"identical-video-bytes").expect("keeper");
        std::fs::write(&duplicate, b"identical-video-bytes").expect("duplicate");

        let conn = db::open(&paths).expect("db");
        conn.execute(
            "INSERT INTO library_item (id, created_at_ms, source_type, source_uri, title, media_path, origin) VALUES ('keeper-item', 1, 'local_file', ?1, 'Keeper', ?1, '4kvdp_import')",
            [keeper.to_string_lossy().to_string()],
        )
        .expect("keeper item");
        conn.execute(
            "INSERT INTO library_item (id, created_at_ms, source_type, source_uri, title, media_path, origin) VALUES ('duplicate-item', 2, 'local_file', ?1, 'Duplicate', ?1, '4kvdp_import')",
            [duplicate.to_string_lossy().to_string()],
        )
        .expect("duplicate item");
        conn.execute(
            "INSERT INTO media_source_identity (service, media_id, canonical_url, library_item_id, repair_state, created_at_ms, updated_at_ms) VALUES ('youtube', 'dup12345678', 'https://youtu.be/dup12345678', 'duplicate-item', 'ready', 1, 1)",
            [],
        )
        .expect("identity");
        drop(conn);

        let run = create_inventory_run(
            &paths,
            vec![media_root.to_string_lossy().to_string()],
            Some(quarantine.to_string_lossy().to_string()),
        )
        .expect("run");
        advance_to_review(&paths, &run.id);
        let group = list_groups(&paths, &run.id)
            .expect("groups")
            .into_iter()
            .next()
            .expect("group");
        let approved_group = set_group_decision(
            &paths,
            &run.id,
            &group.group_id,
            "approved",
            Some(&keeper.to_string_lossy()),
        )
        .expect("approve");
        assert_eq!(
            approved_group.keeper_library_item_id.as_deref(),
            Some("keeper-item")
        );
        assert_eq!(
            approved_group
                .members
                .iter()
                .find(|member| paths_equivalent(&member.path, &duplicate.to_string_lossy()))
                .and_then(|member| member.library_item_id.as_deref()),
            Some("duplicate-item")
        );

        let conn = db::open(&paths).expect("trigger db");
        conn.execute_batch(
            r#"
CREATE TRIGGER force_cleanup_library_update_failure
BEFORE UPDATE OF media_path ON library_item
WHEN OLD.id='duplicate-item'
BEGIN
  SELECT RAISE(ABORT, 'forced cleanup metadata failure');
END;
"#,
        )
        .expect("failure trigger");
        drop(conn);

        let applied = apply_approved_groups(&paths, &run.id).expect("apply summary");
        assert_eq!(applied.applied_actions, 0);
        assert_eq!(applied.failed_actions, 1);
        assert!(keeper.is_file());
        assert!(duplicate.is_file());
        assert_eq!(
            std::fs::read(&keeper).expect("keeper bytes"),
            std::fs::read(&duplicate).expect("restored duplicate bytes")
        );

        let conn = db::open(&paths).expect("db after failed apply");
        let library_path: String = conn
            .query_row(
                "SELECT media_path FROM library_item WHERE id='duplicate-item'",
                [],
                |row| row.get(0),
            )
            .expect("library path");
        assert_eq!(library_path, duplicate.to_string_lossy().to_string());
        let identity_item: String = conn
            .query_row(
                "SELECT library_item_id FROM media_source_identity WHERE service='youtube' AND media_id='dup12345678'",
                [],
                |row| row.get(0),
            )
            .expect("identity");
        assert_eq!(identity_item, "duplicate-item");
        let (status, error, quarantine_path): (String, Option<String>, String) = conn
            .query_row(
                "SELECT status, error, quarantine_path FROM media_cleanup_action WHERE run_id=?1 ORDER BY created_at_ms DESC LIMIT 1",
                [&run.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("action");
        assert_eq!(status, "failed");
        assert!(error
            .as_deref()
            .is_some_and(|value| value.contains("forced cleanup metadata failure")));
        assert!(!Path::new(&quarantine_path).exists());
    }

    #[test]
    fn apply_database_and_restore_failure_durably_invalidates_attention_paths() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_data"));
        db::ensure_schema(&paths).expect("schema");
        db::open(&paths)
            .unwrap()
            .execute(
                "INSERT INTO meta(key,value) VALUES('jobs_queue_paused','1') ON CONFLICT(key) DO UPDATE SET value='1'",
                [],
            )
            .expect("pause queue for cleanup boundary");
        let media_root = dir.path().join("media");
        let quarantine = dir.path().join("quarantine");
        std::fs::create_dir_all(&media_root).expect("media root");
        let keeper = media_root.join("keeper.mkv");
        let duplicate = media_root.join("duplicate.mkv");
        std::fs::write(&keeper, b"same-bytes").expect("keeper");
        std::fs::write(&duplicate, b"same-bytes").expect("duplicate");

        let conn = db::open(&paths).expect("fixture db");
        conn.execute(
            "INSERT INTO library_item (id,created_at_ms,source_type,source_uri,title,media_path,origin) VALUES('attention-keeper',1,'local_file',?1,'Keeper',?1,'local_import')",
            [keeper.to_string_lossy().to_string()],
        ).unwrap();
        conn.execute(
            "INSERT INTO library_item (id,created_at_ms,source_type,source_uri,title,media_path,origin) VALUES('attention-source',2,'local_file',?1,'Source',?1,'local_import')",
            [duplicate.to_string_lossy().to_string()],
        ).unwrap();
        conn.execute(
            "INSERT INTO media_availability_observation(path,state,observed_at_ms,source,duration_ms,next_refresh_at_ms,invalidated_at_ms) VALUES(?1,'present',1,'fixture',1,9999999999999,NULL)",
            [duplicate.to_string_lossy().to_string()],
        ).unwrap();
        drop(conn);

        let run = create_inventory_run(
            &paths,
            vec![media_root.to_string_lossy().to_string()],
            Some(quarantine.to_string_lossy().to_string()),
        )
        .unwrap();
        advance_to_review(&paths, &run.id);
        let group = list_groups(&paths, &run.id).unwrap().remove(0);
        set_group_decision(
            &paths,
            &run.id,
            &group.group_id,
            "approved",
            Some(&keeper.to_string_lossy()),
        )
        .unwrap();
        let conn = db::open(&paths).unwrap();
        conn.execute_batch(
            "CREATE TRIGGER force_attention_metadata_failure BEFORE UPDATE OF media_path ON library_item WHEN OLD.id='attention-source' BEGIN SELECT RAISE(ABORT,'forced attention metadata failure'); END;",
        ).unwrap();
        drop(conn);

        FORCE_CLEANUP_APPLY_COMPENSATION_FAILURE.with(|flag| flag.set(true));
        let summary = apply_approved_groups(&paths, &run.id).expect("attention summary");
        FORCE_CLEANUP_APPLY_COMPENSATION_FAILURE.with(|flag| flag.set(false));
        assert_eq!(summary.failed_actions, 1);
        assert!(
            !duplicate.exists(),
            "forced compensation failure leaves source absent"
        );

        let restarted = db::open(&paths).expect("restart db");
        let (status, quarantine_path): (String, String) = restarted.query_row(
            "SELECT status,quarantine_path FROM media_cleanup_action WHERE run_id=?1 ORDER BY created_at_ms DESC LIMIT 1",
            [&run.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).unwrap();
        assert_eq!(status, "attention");
        assert!(Path::new(&quarantine_path).is_file());
        for path in [duplicate.to_string_lossy().to_string(), quarantine_path] {
            let invalidated: Option<i64> = restarted
                .query_row(
                    "SELECT invalidated_at_ms FROM media_availability_observation WHERE path=?1",
                    [&path],
                    |row| row.get(0),
                )
                .expect("durable invalidation row");
            assert!(invalidated.is_some(), "restart must not trust {path}");
        }
    }

    #[test]
    fn duplicate_grouping_rejects_simulated_digest_collision_by_bytes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_data"));
        db::ensure_schema(&paths).expect("schema");
        let media_root = dir.path().join("media");
        std::fs::create_dir_all(&media_root).expect("media root");
        let left = media_root.join("left.mkv");
        let right = media_root.join("right.mkv");
        std::fs::write(&left, b"aaaa").expect("left bytes");
        std::fs::write(&right, b"bbbb").expect("right bytes");
        let run =
            create_inventory_run(&paths, vec![media_root.to_string_lossy().to_string()], None)
                .expect("run");
        let conn = db::open(&paths).expect("db");
        for path in [&left, &right] {
            conn.execute(
                "INSERT INTO media_cleanup_file(run_id,path,size_bytes,modified_ms,media_id,state,full_sha256,updated_at_ms) VALUES(?1,?2,4,1,'ABCDEFGHIJK','fully_hashed','forced-collision',1)",
                params![&run.id, path.to_string_lossy().to_string()],
            )
            .expect("forced digest row");
        }

        build_duplicate_groups(&conn, &run.id).expect("collision grouping");
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM media_cleanup_group WHERE run_id=?1",
                [&run.id],
                |row| row.get::<_, i64>(0),
            )
            .expect("collision group count"),
            0,
            "matching stored digests must not override different bytes"
        );
        let collision_variants = list_variants(&paths, &run.id).expect("collision variants");
        assert_eq!(
            collision_variants[0].evidence["classification"],
            "same_identity_variant_review"
        );

        std::fs::write(&right, b"aaaa").expect("make exact duplicate");
        build_duplicate_groups(&conn, &run.id).expect("exact grouping");
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM media_cleanup_group WHERE run_id=?1",
                [&run.id],
                |row| row.get::<_, i64>(0),
            )
            .expect("exact group count"),
            1,
            "byte-identical candidates with the same digest should remain eligible"
        );
        let exact_variants = list_variants(&paths, &run.id).expect("exact variants");
        assert_eq!(
            exact_variants[0].evidence["classification"],
            "same_identity_exact_bytes"
        );
    }

    #[test]
    fn inventory_never_mutates_media_and_quarantine_cannot_overlap_root() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_data"));
        db::ensure_schema(&paths).expect("schema");
        let media_root = dir.path().join("media");
        std::fs::create_dir_all(&media_root).expect("root");
        let media = media_root.join("video.mp4");
        std::fs::write(&media, b"media").expect("media");
        let before = std::fs::read(&media).expect("before");
        assert!(create_inventory_run(
            &paths,
            vec![media_root.to_string_lossy().to_string()],
            Some(media_root.join("quarantine").to_string_lossy().to_string()),
        )
        .is_err());
        let run =
            create_inventory_run(&paths, vec![media_root.to_string_lossy().to_string()], None)
                .expect("run");
        advance_inventory(&paths, &run.id, Some(100)).expect("inventory");
        assert_eq!(std::fs::read(&media).expect("after"), before);
    }
}
