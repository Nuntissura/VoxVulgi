use crate::paths::AppPaths;
use crate::{db, EngineError, Result};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

const HASH_WINDOW_BYTES: usize = 64 * 1024;

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
    media_ids: Vec<String>,
    source_library_media_path: Option<String>,
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
            .any(|root| paths_overlap(Path::new(root), quarantine_path))
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
    let mut queue: Vec<String> = serde_json::from_str(&row.0)?;
    let library_by_path = library_identity_by_normalized_path(&conn)?;
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
        conn.execute(
            r#"
INSERT INTO media_cleanup_file (
  run_id, path, size_bytes, modified_ms, library_item_id, media_id, state, updated_at_ms
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'inventoried', ?7)
ON CONFLICT(run_id, path) DO UPDATE SET
  size_bytes=excluded.size_bytes,
  modified_ms=excluded.modified_ms,
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
                library_item_id,
                media_id,
                now_ms()
            ],
        )?;
    }
    let stage = if queue.is_empty() {
        "hashing"
    } else {
        "inventory"
    };
    conn.execute(
        r#"
UPDATE media_cleanup_run SET
  scan_queue_json=?1,
  stage=?2,
  status=CASE WHEN ?2='hashing' THEN 'running' ELSE 'paused' END,
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
    if stage == "review" || stage == "complete" {
        return Ok(MediaCleanupAdvanceSummary {
            run: get_run_conn(&conn, run_id)?.expect("run exists"),
            processed_files: 0,
            remaining_inventory_entries: 0,
        });
    }

    let prefix_rows = pending_prefix_rows(&conn, run_id, max_files)?;
    if !prefix_rows.is_empty() {
        for row in &prefix_rows {
            match staged_hashes_with_cache(&conn, row, false) {
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
            match staged_hashes_with_cache(&conn, row, true) {
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
    if let Some(keeper) = keeper_path.map(str::trim).filter(|value| !value.is_empty()) {
        let keeper_key = Path::new(keeper)
            .canonicalize()
            .map(|path| normalize_path_key(&path.to_string_lossy()))
            .unwrap_or_else(|_| normalize_path_key(keeper));
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
            .find(|(path, _)| normalize_path_key(path) == keeper_key)
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
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    ensure_cleanup_apply_boundary(&conn)?;
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
            match apply_one_action(&conn, run_id, &group, member, Path::new(&quarantine_root)) {
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
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
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
            let source_library_path = if let Some(source_item) = action.4.as_deref() {
                let current_path =
                    library_item_media_path(&conn, source_item)?.ok_or_else(|| {
                        EngineError::InstallFailed(format!(
                            "rollback source library item is missing: {source_item}"
                        ))
                    })?;
                let original_library_path = relink_journal
                    .source_library_media_path
                    .as_deref()
                    .unwrap_or(&action.1);
                let keeper_path: String = conn.query_row(
                    "SELECT keeper_path FROM media_cleanup_action WHERE id=?1",
                    [&action.0],
                    |row| row.get::<_, String>(0),
                )?;
                if current_path != original_library_path
                    && !paths_equivalent(&current_path, original_library_path)
                    && !paths_equivalent(&current_path, &keeper_path)
                {
                    return Err(EngineError::InstallFailed(format!(
                        "rollback refused to overwrite a library path changed after cleanup: item={source_item}; current={current_path}"
                    )));
                }
                Some((current_path, original_library_path.to_string()))
            } else {
                None
            };
            if quarantine_exists {
                move_verified(&quarantine, &source, action.6, &action.7)?;
            } else {
                verify_path(&source, action.6, &action.7)?;
            }
            let database_result = (|| -> Result<()> {
                let tx = conn.unchecked_transaction()?;
                if let Some(source_item) = action.4.as_deref() {
                    if source_library_path
                        .as_ref()
                        .is_some_and(|(current, original)| current != original)
                    {
                        let (current_path, original_path) =
                            source_library_path.as_ref().expect("checked above");
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
                if let (Some(keeper_item), Some(source_item)) =
                    (action.3.as_deref(), action.4.as_deref())
                {
                    for media_id in &relink_journal.media_ids {
                        let changed = tx.execute(
                            "UPDATE media_source_identity SET library_item_id=?1, updated_at_ms=?2 WHERE service='youtube' AND media_id=?3 AND library_item_id=?4",
                            params![source_item, now_ms(), media_id, keeper_item],
                        )?;
                        if changed == 0 {
                            let current_item = tx
                                .query_row(
                                    "SELECT library_item_id FROM media_source_identity WHERE service='youtube' AND media_id=?1",
                                    [media_id],
                                    |row| row.get::<_, Option<String>>(0),
                                )
                                .optional()?
                                .flatten();
                            if current_item.as_deref() != Some(source_item) {
                                return Err(EngineError::InstallFailed(format!(
                                    "rollback identity changed after cleanup: youtube:{media_id}"
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
                tx.commit()?;
                Ok(())
            })();
            if let Err(database_error) = database_result {
                if quarantine_exists && source.exists() && !quarantine.exists() {
                    if let Err(compensation_error) =
                        move_verified(&source, &quarantine, action.6, &action.7)
                    {
                        return Err(EngineError::InstallFailed(format!(
                            "rollback database update failed ({database_error}); restoring the quarantine copy also failed ({compensation_error})"
                        )));
                    }
                }
                return Err(database_error);
            }
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

fn apply_one_action(
    conn: &rusqlite::Connection,
    run_id: &str,
    group: &MediaCleanupGroup,
    member: &MediaCleanupGroupMember,
    quarantine_root: &Path,
) -> Result<u64> {
    ensure_cleanup_apply_boundary(conn)?;
    let source = PathBuf::from(&member.path);
    let keeper = PathBuf::from(&group.keeper_path);
    if normalize_path_key(&member.path) == normalize_path_key(&group.keeper_path) {
        return Err(EngineError::InstallFailed(format!(
            "cleanup source resolves to the selected keeper path: {}",
            member.path
        )));
    }
    let metadata = std::fs::metadata(&source)?;
    if metadata.len() != group.size_bytes {
        return Err(EngineError::SizeMismatch {
            path: source,
            expected: group.size_bytes,
            actual: metadata.len(),
        });
    }
    let actual_hash = full_sha256(&source)?;
    if actual_hash != group.full_sha256 {
        return Err(EngineError::HashMismatch {
            path: source,
            expected: group.full_sha256.clone(),
            actual: actual_hash,
        });
    }
    verify_path(
        &keeper,
        i64::try_from(group.size_bytes).unwrap_or(i64::MAX),
        &group.full_sha256,
    )?;
    let source_library_path = if let Some(source_item) = member.library_item_id.as_deref() {
        let current_path = library_item_media_path(conn, source_item)?.ok_or_else(|| {
            EngineError::InstallFailed(format!(
                "cleanup source library item is missing: {source_item}"
            ))
        })?;
        if !paths_equivalent(&current_path, &member.path) {
            return Err(EngineError::InstallFailed(format!(
                "cleanup source library path changed after inventory: item={source_item}; expected={}; current={current_path}",
                member.path
            )));
        }
        Some(current_path)
    } else {
        None
    };
    if let Some(keeper_item) = group.keeper_library_item_id.as_deref() {
        let current_path = library_item_media_path(conn, keeper_item)?.ok_or_else(|| {
            EngineError::InstallFailed(format!(
                "cleanup keeper library item is missing: {keeper_item}"
            ))
        })?;
        if !paths_equivalent(&current_path, &group.keeper_path) {
            return Err(EngineError::InstallFailed(format!(
                "cleanup keeper library path changed after approval: item={keeper_item}; expected={}; current={current_path}",
                group.keeper_path
            )));
        }
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
    let relinked_media_ids = if let (Some(source_item), Some(keeper_item)) = (
        member.library_item_id.as_deref(),
        group.keeper_library_item_id.as_deref(),
    ) {
        let mut stmt = conn.prepare(
            "SELECT media_id FROM media_source_identity WHERE service='youtube' AND library_item_id=?1",
        )?;
        let ids = stmt
            .query_map([source_item], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let _ = keeper_item;
        ids
    } else {
        Vec::new()
    };
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
                version: 1,
                media_ids: relinked_media_ids.clone(),
                source_library_media_path: source_library_path.clone(),
            })?,
            i64::try_from(group.size_bytes).unwrap_or(i64::MAX),
            group.full_sha256,
            now
        ],
    )?;
    let result = (|| -> Result<()> {
        ensure_cleanup_apply_boundary(conn)?;
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
        let tx = conn.unchecked_transaction()?;
        if let Some(source_item) = member.library_item_id.as_deref() {
            let changed = tx.execute(
                "UPDATE library_item SET media_path=?1 WHERE id=?2 AND media_path=?3",
                params![group.keeper_path, source_item, source_library_path],
            )?;
            if changed != 1 {
                return Err(EngineError::InstallFailed(format!(
                    "cleanup source library path changed concurrently: {source_item}"
                )));
            }
        }
        if let (Some(source_item), Some(keeper_item)) = (
            member.library_item_id.as_deref(),
            group.keeper_library_item_id.as_deref(),
        ) {
            for media_id in &relinked_media_ids {
                let changed = tx.execute(
                    "UPDATE media_source_identity SET library_item_id=?1, repair_state='ready', updated_at_ms=?2 WHERE service='youtube' AND media_id=?3 AND library_item_id=?4",
                    params![keeper_item, now_ms(), media_id, source_item],
                )?;
                if changed != 1 {
                    return Err(EngineError::InstallFailed(format!(
                        "cleanup identity changed concurrently: youtube:{media_id}"
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
        tx.commit()?;
        Ok(())
    })();
    if let Err(error) = result {
        let moved_to_quarantine = quarantine_path.exists() && !source.exists();
        let recovery_error = if moved_to_quarantine {
            move_verified(
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
        let error_text = recovery_error
            .map(|recovery| {
                format!(
                    "{error}; restoring the source after the database failure also failed: {recovery}"
                )
            })
            .unwrap_or_else(|| error.to_string());
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
        return Err(error);
    }
    Ok(group.size_bytes)
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
            "media cleanup apply requires the global queue to remain paused".to_string(),
        ));
    }
    let running_direct_jobs: i64 = conn.query_row(
        "SELECT COUNT(*) FROM job WHERE status='running' AND type='download_direct_url'",
        [],
        |row| row.get(0),
    )?;
    if running_direct_jobs != 0 {
        return Err(EngineError::InstallFailed(format!(
            "media cleanup apply requires zero running direct-download jobs; found {running_direct_jobs}"
        )));
    }
    Ok(())
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

fn parse_cleanup_relink_journal(value: &str) -> Result<CleanupRelinkJournal> {
    if let Ok(journal) = serde_json::from_str::<CleanupRelinkJournal>(value) {
        return Ok(journal);
    }
    let media_ids = serde_json::from_str::<Vec<String>>(value)?;
    Ok(CleanupRelinkJournal {
        version: 0,
        media_ids,
        source_library_media_path: None,
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
    let metadata = std::fs::metadata(path)?;
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
    for (hash, size, count) in groups {
        let keeper = conn.query_row(
            r#"
SELECT path, library_item_id
FROM media_cleanup_file
WHERE run_id=?1 AND full_sha256=?2 AND size_bytes=?3
ORDER BY
  CASE WHEN media_id IS NOT NULL THEN 0 ELSE 1 END,
  CASE WHEN library_item_id IS NOT NULL THEN 0 ELSE 1 END,
  lower(path) ASC
LIMIT 1
"#,
            params![run_id, hash, size],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        )?;
        let group_id = format!("sha256:{}", hash);
        let reclaimable = size.saturating_mul(count.saturating_sub(1));
        reclaimable_total = reclaimable_total.saturating_add(reclaimable);
        conn.execute(
            "INSERT INTO media_cleanup_group (run_id, group_id, full_sha256, size_bytes, member_count, keeper_path, keeper_library_item_id, reclaimable_bytes, decision, created_at_ms, updated_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'pending', ?9, ?9)",
            params![run_id, group_id, hash, size, count, keeper.0, keeper.1, reclaimable, now_ms()],
        )?;
        conn.execute(
            "UPDATE media_cleanup_file SET group_id=?1 WHERE run_id=?2 AND full_sha256=?3 AND size_bytes=?4",
            params![group_id, run_id, hash, size],
        )?;
    }
    let duplicate_groups: i64 = conn.query_row(
        "SELECT COUNT(*) FROM media_cleanup_group WHERE run_id=?1",
        [run_id],
        |row| row.get(0),
    )?;
    conn.execute(
        "UPDATE media_cleanup_run SET status='review', stage='review', duplicate_groups=?1, reclaimable_bytes=?2, updated_at_ms=?3 WHERE id=?4",
        params![duplicate_groups, reclaimable_total, now_ms(), run_id],
    )?;
    Ok(())
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
    conn: &rusqlite::Connection,
) -> Result<HashMap<String, (Option<String>, Option<String>)>> {
    let mut stmt = conn.prepare(
        r#"
SELECT li.media_path, li.id, i.media_id
FROM library_item li
LEFT JOIN media_source_identity i
  ON i.library_item_id=li.id AND i.service='youtube'
"#,
    )?;
    let mut out = HashMap::new();
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let path: String = row.get(0)?;
        let item_id: String = row.get(1)?;
        let media_id: Option<String> = row.get(2)?;
        let mut keys = vec![normalize_path_key(&path)];
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

fn paths_overlap(root: &Path, quarantine: &Path) -> bool {
    let root = normalize_path_key(&root.to_string_lossy());
    let quarantine = normalize_path_key(&quarantine.to_string_lossy());
    quarantine == root
        || quarantine.starts_with(&(root.clone() + "\\"))
        || root.starts_with(&(quarantine + "\\"))
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

    fn advance_to_review(paths: &AppPaths, run_id: &str) {
        loop {
            let run = get_run(paths, run_id).expect("run").expect("exists");
            match run.stage.as_str() {
                "inventory" => {
                    advance_inventory(paths, run_id, Some(2)).expect("inventory");
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
    fn cleanup_relink_journal_reads_legacy_actions() {
        let journal =
            parse_cleanup_relink_journal(r#"["legacy-media-id"]"#).expect("legacy journal");
        assert_eq!(journal.version, 0);
        assert_eq!(journal.media_ids, vec!["legacy-media-id"]);
        assert_eq!(journal.source_library_media_path, None);
    }

    #[test]
    fn inventory_hash_quarantine_and_rollback_are_recoverable() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_data"));
        db::ensure_schema(&paths).expect("schema");
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
            "INSERT INTO media_source_identity (service, media_id, canonical_url, library_item_id, repair_state, created_at_ms, updated_at_ms) VALUES ('youtube', 'dup12345678', 'https://youtu.be/dup12345678', 'duplicate-item', 'ready', 1, 1)",
            [],
        )
        .expect("identity");
        let library_map = library_identity_by_normalized_path(&conn).expect("library path map");
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
        let applied_identity_item: String = conn
            .query_row(
                "SELECT library_item_id FROM media_source_identity WHERE service='youtube' AND media_id='dup12345678'",
                [],
                |row| row.get(0),
            )
            .expect("applied identity");
        assert_eq!(applied_identity_item, "keeper-item");
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
        let rolled_back_identity_item: String = conn
            .query_row(
                "SELECT library_item_id FROM media_source_identity WHERE service='youtube' AND media_id='dup12345678'",
                [],
                |row| row.get(0),
            )
            .expect("rolled-back identity");
        assert_eq!(rolled_back_identity_item, "duplicate-item");
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
    }

    #[test]
    fn apply_database_failure_restores_source_and_preserves_metadata_truth() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_data"));
        db::ensure_schema(&paths).expect("schema");
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
