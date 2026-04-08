use crate::paths::AppPaths;
use crate::{db, jobs, library, EngineError, Result};
use csv::ReaderBuilder;
use regex::Regex;
use rusqlite::{params, OpenFlags};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::path::PathBuf;
use std::sync::OnceLock;
use url::Url;
use uuid::Uuid;

const EXPORT_SCHEMA_VERSION: u32 = 1;
const DEFAULT_SUBSCRIPTION_MAP: &str = "subscription";
const DEFAULT_REFRESH_INTERVAL_MINUTES: i64 = 60;
const MIN_REFRESH_INTERVAL_MINUTES: i64 = 5;
const MAX_REFRESH_INTERVAL_MINUTES: i64 = 10080;
const FOURKVDP_SUBSCRIPTIONS_JSON_FILENAME: &str = "subscriptions.json";
const FOURKVDP_SUBSCRIPTION_ENTRIES_CSV_FILENAME: &str = "subscription_entries.csv";
const YT_DLP_ARCHIVE_FILENAME: &str = "voxvulgi_youtube_archive.txt";
const DEFAULT_LEGACY_ANALYSIS_MAX_DEPTH: usize = 4;
const DEFAULT_LEGACY_ANALYSIS_MAX_FILES: usize = 2500;
const DEFAULT_LEGACY_IMPORT_MAX_DEPTH: usize = 8;
const DEFAULT_LEGACY_IMPORT_MAX_FILES: usize = 25000;
const MAX_LEGACY_ANALYSIS_MAX_DEPTH: usize = 16;
const MAX_LEGACY_ANALYSIS_MAX_FILES: usize = 100000;
const LEGACY_CONTAINER_HINT_SCAN_DIR_LIMIT: usize = 120;
const LEGACY_SAMPLE_NAME_LIMIT: usize = 24;
const LEGACY_4KVDP_GROUP_ALL: &str = "Legacy 4KVDP";
const LEGACY_4KVDP_GROUP_SUBSCRIPTIONS: &str = "Legacy 4KVDP Subscriptions";
const LEGACY_4KVDP_GROUP_PLAYLISTS: &str = "Legacy 4KVDP Playlists";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoutubeSubscriptionRow {
    pub id: String,
    pub title: String,
    pub source_url: String,
    pub folder_map: String,
    pub output_dir_override: Option<String>,
    pub use_browser_cookies: bool,
    pub auth_session_configured: bool,
    pub active: bool,
    pub preset_id: Option<String>,
    pub refresh_interval_minutes: i64,
    pub last_queued_at_ms: Option<i64>,
    pub last_error_at_ms: Option<i64>,
    pub consecutive_failures: i64,
    pub next_allowed_refresh_at_ms: Option<i64>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    #[serde(default)]
    pub group_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoutubeSubscriptionUpsert {
    pub id: Option<String>,
    pub title: String,
    pub source_url: String,
    pub folder_map: Option<String>,
    pub output_dir_override: Option<String>,
    pub use_browser_cookies: bool,
    #[serde(default)]
    pub auth_session_input: Option<String>,
    #[serde(default)]
    pub clear_auth_session: bool,
    pub active: bool,
    pub preset_id: Option<String>,
    #[serde(default)]
    pub group_ids: Vec<String>,
    pub refresh_interval_minutes: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoutubeSubscriptionGroupRow {
    pub id: String,
    pub name: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoutubeSubscriptionGroupUpsert {
    pub id: Option<String>,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoutubeSubscriptionArchiveSeedSummary {
    pub scanned_dir: String,
    pub archive_files_updated: usize,
    pub inferred_ids: usize,
    pub appended_ids: usize,
    pub skipped_existing_ids: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExistingDownloadsImportSummary {
    pub scanned_dir: String,
    pub discovered_media_files: usize,
    pub imported_items: usize,
    pub skipped_existing_items: usize,
    pub failures: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyArchiveContainerHint {
    pub relative_path: String,
    pub media_file_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyArchiveManagedContainerHint {
    pub container_kind: String,
    pub relative_path: String,
    pub title: String,
    pub source_url: String,
    pub matched_root_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyArchiveAnalysisSummary {
    pub root_path: String,
    pub install_path: Option<String>,
    pub install_path_exists: bool,
    pub legacy_state_db_path: Option<String>,
    pub legacy_state_db_exists: bool,
    pub media_file_count: usize,
    pub detected_4kvdp_install: bool,
    pub detected_4kvdp_subscriptions_json: bool,
    pub detected_4kvdp_subscription_entries_csv: bool,
    pub detected_channel_dirs: usize,
    pub detected_playlist_dirs: usize,
    pub top_level_dir_count: usize,
    pub top_level_file_count: usize,
    pub managed_container_count: usize,
    pub managed_subscription_count: usize,
    pub managed_playlist_count: usize,
    pub matched_managed_dirs: usize,
    pub unmatched_top_level_dirs: usize,
    pub scan_max_depth: usize,
    pub scan_max_files: usize,
    pub local_report_path: String,
    pub warnings: Vec<String>,
    pub container_hints: Vec<LegacyArchiveContainerHint>,
    pub managed_container_hints: Vec<LegacyArchiveManagedContainerHint>,
    pub sample_unmatched_dirs: Vec<String>,
    pub sample_top_level_files: Vec<String>,
    pub sample_media_paths: Vec<String>,
    pub recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoutubeSubscriptionsExportSummary {
    pub out_path: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoutubeSubscriptionsImportSummary {
    pub total_in_file: usize,
    pub inserted: usize,
    pub updated: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoutubeSubscriptionsImport4kvdpSummary {
    pub total_in_subscriptions_json: usize,
    pub imported_subscriptions: usize,
    pub inserted: usize,
    pub updated: usize,
    pub skipped_non_youtube: usize,
    pub archive_seeded_subscriptions: usize,
    pub archive_seeded_entries: usize,
    pub archive_skipped_entries: usize,
    pub archive_seed_failures: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoutubeSubscriptionsImport4kvdpStateSummary {
    pub sqlite_path: String,
    pub total_in_legacy_state: usize,
    pub imported_sources: usize,
    pub imported_subscription_sources: usize,
    pub imported_playlist_sources: usize,
    pub inserted: usize,
    pub updated: usize,
    pub skipped_non_youtube: usize,
    pub mapped_to_selected_root: usize,
    pub retained_existing_legacy_dir: usize,
    pub missing_target_dirs: usize,
    pub archive_seeded_subscriptions: usize,
    pub archive_seeded_entries: usize,
    pub archive_skipped_entries: usize,
    pub archive_seed_failures: usize,
    pub group_names: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct YoutubeSubscriptionsExportFile {
    schema_version: u32,
    exported_at_ms: i64,
    app: String,
    subscriptions: Vec<YoutubeSubscriptionsExportEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct YoutubeSubscriptionsExportEntry {
    title: String,
    source_url: String,
    folder_map: Option<String>,
    output_dir_override: Option<String>,
    use_browser_cookies: bool,
    active: bool,
    #[serde(default)]
    preset_id: Option<String>,
    #[serde(default)]
    group_ids: Vec<String>,
    #[serde(default)]
    refresh_interval_minutes: Option<i64>,
}

pub fn list_youtube_subscriptions(paths: &AppPaths) -> Result<Vec<YoutubeSubscriptionRow>> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;

    let mut stmt = conn.prepare(
        r#"
SELECT
  id,
  title,
  source_url,
  folder_map,
  output_dir_override,
  use_browser_cookies,
  active,
  preset_id,
  refresh_interval_minutes,
  last_queued_at_ms,
  last_error_at_ms,
  consecutive_failures,
  next_allowed_refresh_at_ms,
  created_at_ms,
  updated_at_ms
FROM youtube_subscription
ORDER BY active DESC, updated_at_ms DESC, created_at_ms DESC
"#,
    )?;

    let rows = stmt
        .query_map([], row_to_subscription)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let rows = hydrate_group_ids(&conn, rows)?;
    Ok(hydrate_auth_session_flags(paths, rows))
}

pub fn upsert_youtube_subscription(
    paths: &AppPaths,
    req: YoutubeSubscriptionUpsert,
) -> Result<YoutubeSubscriptionRow> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;

    let normalized = normalize_upsert(req)?;
    let now = now_ms();
    let input_id = normalized.id.clone();
    let mut updated_existing = false;

    if let Some(id) = input_id.as_deref() {
        let changed = conn.execute(
            r#"
UPDATE youtube_subscription
SET
  title = ?1,
  source_url = ?2,
  folder_map = ?3,
  output_dir_override = ?4,
  use_browser_cookies = ?5,
  active = ?6,
  preset_id = ?7,
  refresh_interval_minutes = ?8,
  updated_at_ms = ?9
WHERE id = ?10
"#,
            params![
                &normalized.title,
                &normalized.source_url,
                &normalized.folder_map,
                &normalized.output_dir_override,
                bool_to_i64(normalized.use_browser_cookies),
                bool_to_i64(normalized.active),
                &normalized.preset_id,
                normalized.refresh_interval_minutes,
                now,
                id,
            ],
        )?;
        if changed > 0 {
            updated_existing = true;
        }
    }

    if !updated_existing {
        let id = input_id.unwrap_or_else(|| Uuid::new_v4().to_string());
        conn.execute(
            r#"
INSERT INTO youtube_subscription (
  id,
  title,
  source_url,
  folder_map,
  output_dir_override,
  use_browser_cookies,
  active,
  preset_id,
  refresh_interval_minutes,
  last_queued_at_ms,
  last_error_at_ms,
  consecutive_failures,
  next_allowed_refresh_at_ms,
  created_at_ms,
  updated_at_ms
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, NULL, 0, NULL, ?10, ?10)
ON CONFLICT(source_url) DO UPDATE SET
  title = excluded.title,
  folder_map = excluded.folder_map,
  output_dir_override = excluded.output_dir_override,
  use_browser_cookies = excluded.use_browser_cookies,
  active = excluded.active,
  preset_id = excluded.preset_id,
  refresh_interval_minutes = excluded.refresh_interval_minutes,
  updated_at_ms = excluded.updated_at_ms
"#,
            params![
                id,
                &normalized.title,
                &normalized.source_url,
                &normalized.folder_map,
                &normalized.output_dir_override,
                bool_to_i64(normalized.use_browser_cookies),
                bool_to_i64(normalized.active),
                &normalized.preset_id,
                normalized.refresh_interval_minutes,
                now,
            ],
        )?;
    }

    let mut row = subscription_by_source_url_conn(&conn, normalized.source_url.as_str())?
        .ok_or_else(|| {
            EngineError::InstallFailed("failed to load saved subscription".to_string())
        })?;
    set_subscription_group_memberships_conn(&conn, &row.id, &normalized.group_ids)?;
    sync_auth_session_secret(
        paths,
        row.id.as_str(),
        normalized.auth_session_input.as_deref(),
        normalized.clear_auth_session,
    )?;
    row.group_ids = normalized.group_ids;
    row.auth_session_configured = youtube_subscription_has_auth_session(paths, &row.id);
    Ok(row)
}

pub fn delete_youtube_subscription(paths: &AppPaths, id: &str) -> Result<()> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    conn.execute("DELETE FROM youtube_subscription WHERE id = ?1", [id])?;
    jobs::remove_auth_cookie_secret_path(&paths.youtube_subscription_cookie_secret_path(id));
    Ok(())
}

pub fn get_youtube_subscription_by_id(
    paths: &AppPaths,
    id: &str,
) -> Result<Option<YoutubeSubscriptionRow>> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    let row = subscription_by_id_conn(&conn, id)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let mut hydrated = hydrate_group_ids(&conn, vec![row])?;
    let mut row = hydrated.pop();
    if let Some(value) = row.as_mut() {
        value.auth_session_configured = youtube_subscription_has_auth_session(paths, &value.id);
    }
    Ok(row)
}

pub fn queue_youtube_subscription(paths: &AppPaths, id: &str) -> Result<Vec<jobs::JobRow>> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    let sub = subscription_by_id_conn(&conn, id)?
        .ok_or_else(|| EngineError::InstallFailed(format!("subscription not found: {id}")))?;
    drop(conn);
    queue_subscription_internal(paths, &sub, Some(Uuid::new_v4().to_string()))
}

pub fn queue_all_active_youtube_subscriptions(paths: &AppPaths) -> Result<Vec<jobs::JobRow>> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    let mut stmt = conn.prepare(
        r#"
SELECT
  id,
  title,
  source_url,
  folder_map,
  output_dir_override,
  use_browser_cookies,
  active,
  preset_id,
  refresh_interval_minutes,
  last_queued_at_ms,
  last_error_at_ms,
  consecutive_failures,
  next_allowed_refresh_at_ms,
  created_at_ms,
  updated_at_ms
FROM youtube_subscription
WHERE active = 1
ORDER BY updated_at_ms DESC, created_at_ms DESC
"#,
    )?;
    let rows = stmt
        .query_map([], row_to_subscription)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    drop(conn);

    let now = now_ms();
    let batch_id = Some(Uuid::new_v4().to_string());
    let mut all_jobs: Vec<jobs::JobRow> = Vec::new();
    for sub in rows {
        if !is_subscription_due(&sub, now) {
            continue;
        }
        if !is_subscription_backoff_ready(&sub, now) {
            continue;
        }
        let mut queued = queue_subscription_internal(paths, &sub, batch_id.clone())?;
        all_jobs.append(&mut queued);
    }
    Ok(all_jobs)
}

fn is_subscription_due(sub: &YoutubeSubscriptionRow, now_ms_value: i64) -> bool {
    let Some(last_queued) = sub.last_queued_at_ms else {
        return true;
    };
    let interval_ms = sub
        .refresh_interval_minutes
        .max(1)
        .saturating_mul(60)
        .saturating_mul(1000);
    now_ms_value.saturating_sub(last_queued) >= interval_ms
}

fn is_subscription_backoff_ready(sub: &YoutubeSubscriptionRow, now_ms_value: i64) -> bool {
    match sub.next_allowed_refresh_at_ms {
        Some(next_allowed) => now_ms_value >= next_allowed,
        None => true,
    }
}

pub fn list_youtube_subscription_groups(
    paths: &AppPaths,
) -> Result<Vec<YoutubeSubscriptionGroupRow>> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    list_groups_conn(&conn)
}

pub fn upsert_youtube_subscription_group(
    paths: &AppPaths,
    req: YoutubeSubscriptionGroupUpsert,
) -> Result<YoutubeSubscriptionGroupRow> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    let now = now_ms();
    let name = req.name.trim();
    if name.is_empty() {
        return Err(EngineError::InstallFailed(
            "group name cannot be empty".to_string(),
        ));
    }

    let mut normalized_name = name.to_string();
    if normalized_name.len() > 100 {
        normalized_name.truncate(100);
    }

    if let Some(id) = req.id.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        let changed = conn.execute(
            "UPDATE youtube_subscription_group SET name = ?1, updated_at_ms = ?2 WHERE id = ?3",
            params![normalized_name, now, id],
        )?;
        if changed > 0 {
            return get_group_by_id_conn(&conn, id)?
                .ok_or_else(|| EngineError::InstallFailed("group save failed".to_string()));
        }
    }

    let id = req
        .id
        .as_deref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    conn.execute(
        r#"
INSERT INTO youtube_subscription_group (id, name, created_at_ms, updated_at_ms)
VALUES (?1, ?2, ?3, ?3)
ON CONFLICT(id) DO UPDATE SET
  name = excluded.name,
  updated_at_ms = excluded.updated_at_ms
"#,
        params![id, normalized_name, now],
    )?;
    get_group_by_id_conn(&conn, &id)?
        .ok_or_else(|| EngineError::InstallFailed("group save failed".to_string()))
}

pub fn delete_youtube_subscription_group(paths: &AppPaths, group_id: &str) -> Result<()> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    conn.execute(
        "DELETE FROM youtube_subscription_group WHERE id = ?1",
        params![group_id],
    )?;
    Ok(())
}

pub fn set_youtube_subscription_groups(
    paths: &AppPaths,
    subscription_id: &str,
    group_ids: Vec<String>,
) -> Result<Vec<String>> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    set_subscription_group_memberships_conn(&conn, subscription_id, &group_ids)?;
    list_group_ids_for_subscription_conn(&conn, subscription_id)
}

pub fn queue_youtube_subscription_group(
    paths: &AppPaths,
    group_id: &str,
) -> Result<Vec<jobs::JobRow>> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    let mut stmt = conn.prepare(
        r#"
SELECT
  sub.id,
  sub.title,
  sub.source_url,
  sub.folder_map,
  sub.output_dir_override,
  sub.use_browser_cookies,
  sub.active,
  sub.preset_id,
  sub.refresh_interval_minutes,
  sub.last_queued_at_ms,
  sub.last_error_at_ms,
  sub.consecutive_failures,
  sub.next_allowed_refresh_at_ms,
  sub.created_at_ms,
  sub.updated_at_ms
FROM youtube_subscription sub
JOIN youtube_subscription_group_member gm ON gm.subscription_id = sub.id
WHERE gm.group_id = ?1 AND sub.active = 1
ORDER BY sub.updated_at_ms DESC, sub.created_at_ms DESC
"#,
    )?;
    let rows = stmt
        .query_map(params![group_id], row_to_subscription)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    drop(conn);

    let now = now_ms();
    let batch_id = Some(Uuid::new_v4().to_string());
    let mut queued_jobs: Vec<jobs::JobRow> = Vec::new();
    for sub in rows {
        if !is_subscription_due(&sub, now) {
            continue;
        }
        if !is_subscription_backoff_ready(&sub, now) {
            continue;
        }
        let mut queued = queue_subscription_internal(paths, &sub, batch_id.clone())?;
        queued_jobs.append(&mut queued);
    }
    Ok(queued_jobs)
}

pub fn record_subscription_refresh_success(paths: &AppPaths, subscription_id: &str) -> Result<()> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    conn.execute(
        r#"
UPDATE youtube_subscription
SET
  consecutive_failures = 0,
  last_error_at_ms = NULL,
  next_allowed_refresh_at_ms = NULL,
  updated_at_ms = ?1
WHERE id = ?2
"#,
        params![now_ms(), subscription_id],
    )?;
    Ok(())
}

pub fn record_subscription_refresh_failure(paths: &AppPaths, subscription_id: &str) -> Result<()> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    let now = now_ms();
    let current_failures: i64 = conn
        .query_row(
            "SELECT consecutive_failures FROM youtube_subscription WHERE id = ?1",
            params![subscription_id],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or(0);

    let next_failures = current_failures.saturating_add(1);
    let delay_minutes =
        5_i64.saturating_mul(1_i64 << (next_failures.saturating_sub(1).min(6) as u32));
    let delay_ms = delay_minutes
        .saturating_mul(60)
        .saturating_mul(1000)
        .min(24 * 60 * 60 * 1000);

    conn.execute(
        r#"
UPDATE youtube_subscription
SET
  consecutive_failures = ?1,
  last_error_at_ms = ?2,
  next_allowed_refresh_at_ms = ?3,
  updated_at_ms = ?2
WHERE id = ?4
"#,
        params![
            next_failures,
            now,
            now.saturating_add(delay_ms),
            subscription_id
        ],
    )?;
    Ok(())
}

pub fn seed_archive_from_scan(
    paths: &AppPaths,
    scan_dir: &Path,
    subscription_id: Option<String>,
) -> Result<YoutubeSubscriptionArchiveSeedSummary> {
    let scan_dir = scan_dir
        .canonicalize()
        .unwrap_or_else(|_| scan_dir.to_path_buf());
    if !scan_dir.exists() || !scan_dir.is_dir() {
        return Err(EngineError::InstallFailed(format!(
            "scan folder not found: {}",
            scan_dir.to_string_lossy()
        )));
    }

    let inferred_ids = infer_youtube_ids_from_dir(&scan_dir);
    if inferred_ids.is_empty() {
        return Ok(YoutubeSubscriptionArchiveSeedSummary {
            scanned_dir: scan_dir.to_string_lossy().to_string(),
            archive_files_updated: 0,
            inferred_ids: 0,
            appended_ids: 0,
            skipped_existing_ids: 0,
        });
    }

    let target_subscriptions =
        resolve_seed_target_subscriptions(paths, &scan_dir, subscription_id)?;
    let mut archive_files_updated = 0_usize;
    let mut appended_ids = 0_usize;
    let mut skipped_existing_ids = 0_usize;
    for sub in target_subscriptions {
        let archive_path = ensure_youtube_subscription_archive_state(paths, &sub)?;
        let (appended, skipped_existing) = merge_archive_file(&archive_path, &inferred_ids)?;
        if appended > 0 {
            archive_files_updated += 1;
        }
        appended_ids = appended_ids.saturating_add(appended);
        skipped_existing_ids = skipped_existing_ids.saturating_add(skipped_existing);
    }

    Ok(YoutubeSubscriptionArchiveSeedSummary {
        scanned_dir: scan_dir.to_string_lossy().to_string(),
        archive_files_updated,
        inferred_ids: inferred_ids.len(),
        appended_ids,
        skipped_existing_ids,
    })
}

pub fn import_existing_downloads_index_only(
    paths: &AppPaths,
    scan_dir: &Path,
) -> Result<ExistingDownloadsImportSummary> {
    import_existing_downloads_index_only_with_limits(paths, scan_dir, None, None)
}

pub fn import_existing_downloads_index_only_with_limits(
    paths: &AppPaths,
    scan_dir: &Path,
    max_depth: Option<usize>,
    max_files: Option<usize>,
) -> Result<ExistingDownloadsImportSummary> {
    let scan_dir = scan_dir
        .canonicalize()
        .unwrap_or_else(|_| scan_dir.to_path_buf());
    if !scan_dir.exists() || !scan_dir.is_dir() {
        return Err(EngineError::InstallFailed(format!(
            "scan folder not found: {}",
            scan_dir.to_string_lossy()
        )));
    }

    let max_depth = normalize_legacy_scan_limit(
        max_depth,
        DEFAULT_LEGACY_IMPORT_MAX_DEPTH,
        MAX_LEGACY_ANALYSIS_MAX_DEPTH,
    );
    let max_files = normalize_legacy_scan_limit(
        max_files,
        DEFAULT_LEGACY_IMPORT_MAX_FILES,
        MAX_LEGACY_ANALYSIS_MAX_FILES,
    );
    let media_files = collect_media_files(&scan_dir, max_depth, max_files);
    let mut imported_items = 0_usize;
    let mut skipped_existing_items = 0_usize;
    let mut failures = 0_usize;
    let conn = db::open(paths)?;
    db::migrate(&conn)?;

    for file in media_files.iter() {
        let canonical = file.canonicalize().unwrap_or_else(|_| file.clone());
        let media_path = canonical.to_string_lossy().to_string();
        let exists: Option<String> = conn
            .query_row(
                "SELECT id FROM library_item WHERE media_path = ?1 LIMIT 1",
                params![media_path],
                |row| row.get(0),
            )
            .optional()?;
        if exists.is_some() {
            skipped_existing_items += 1;
            continue;
        }

        match library::import_local_file(paths, &canonical) {
            Ok(_) => imported_items += 1,
            Err(_) => failures += 1,
        }
    }

    Ok(ExistingDownloadsImportSummary {
        scanned_dir: scan_dir.to_string_lossy().to_string(),
        discovered_media_files: media_files.len(),
        imported_items,
        skipped_existing_items,
        failures,
    })
}

pub fn analyze_legacy_archive_root(
    paths: &AppPaths,
    scan_dir: &Path,
    install_path: Option<&Path>,
    max_depth: Option<usize>,
    max_files: Option<usize>,
) -> Result<LegacyArchiveAnalysisSummary> {
    let scan_dir = scan_dir
        .canonicalize()
        .unwrap_or_else(|_| scan_dir.to_path_buf());
    if !scan_dir.exists() || !scan_dir.is_dir() {
        return Err(EngineError::InstallFailed(format!(
            "scan folder not found: {}",
            scan_dir.to_string_lossy()
        )));
    }

    let scan_max_depth = normalize_legacy_scan_limit(
        max_depth,
        DEFAULT_LEGACY_ANALYSIS_MAX_DEPTH,
        MAX_LEGACY_ANALYSIS_MAX_DEPTH,
    );
    let scan_max_files = normalize_legacy_scan_limit(
        max_files,
        DEFAULT_LEGACY_ANALYSIS_MAX_FILES,
        MAX_LEGACY_ANALYSIS_MAX_FILES,
    );
    let media_files = collect_media_files(&scan_dir, scan_max_depth, scan_max_files);
    let sample_media_paths = media_files
        .iter()
        .take(8)
        .map(|path| path.to_string_lossy().to_string())
        .collect::<Vec<_>>();

    let mut container_counts: Vec<LegacyArchiveContainerHint> = Vec::new();
    let mut managed_container_hints: Vec<LegacyArchiveManagedContainerHint> = Vec::new();
    let mut detected_channel_dirs = 0_usize;
    let mut detected_playlist_dirs = 0_usize;
    let mut detected_4kvdp_install = false;
    let mut warnings: Vec<String> = Vec::new();
    let normalized_install_path = normalize_optional_existing_path(install_path);
    let install_path_exists = normalized_install_path
        .as_ref()
        .map(|path| path.exists())
        .unwrap_or(false);
    let legacy_state_db_path = detect_legacy_4kvdp_state_db_path();
    let legacy_state_db_exists = legacy_state_db_path
        .as_ref()
        .map(|path| path.is_file())
        .unwrap_or(false);
    let mut legacy_state_rows: Vec<Legacy4kvdpStateRow> = Vec::new();
    if let Some(path) = legacy_state_db_path.as_ref() {
        match open_legacy_4kvdp_state_db(path).and_then(|conn| read_legacy_4kvdp_state_rows(&conn))
        {
            Ok(rows) => {
                legacy_state_rows = rows;
                if !legacy_state_rows.is_empty() {
                    detected_4kvdp_install = true;
                }
            }
            Err(err) => warnings.push(format!(
                "Detected a 4KVDP app-state database but could not read it cleanly: {err}"
            )),
        }
    }

    let mut top_level_dirs: Vec<(String, PathBuf)> = Vec::new();
    let mut top_level_dir_name_map: HashMap<String, PathBuf> = HashMap::new();
    let mut sample_top_level_files: Vec<String> = Vec::new();
    let mut top_level_file_count = 0_usize;

    let entries = std::fs::read_dir(&scan_dir)?;
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_file() {
            top_level_file_count = top_level_file_count.saturating_add(1);
            if sample_top_level_files.len() < LEGACY_SAMPLE_NAME_LIMIT {
                sample_top_level_files.push(
                    path.file_name()
                        .and_then(|value| value.to_str())
                        .unwrap_or_default()
                        .to_string(),
                );
            }
            continue;
        }
        if !file_type.is_dir() {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_string();
        let lowered = name.to_ascii_lowercase();
        if lowered.contains("4k") && lowered.contains("video") {
            detected_4kvdp_install = true;
        }
        if lowered.contains("playlist")
            || lowered.starts_with("pl")
            || lowered.contains(" watch later")
        {
            detected_playlist_dirs = detected_playlist_dirs.saturating_add(1);
        }
        if lowered.contains("channel")
            || lowered.starts_with('@')
            || lowered.starts_with("uc")
            || lowered.contains("subscription")
        {
            detected_channel_dirs = detected_channel_dirs.saturating_add(1);
        }
        let key = legacy_name_key(&name);
        top_level_dir_name_map
            .entry(key)
            .or_insert_with(|| path.clone());
        top_level_dirs.push((name, path));
    }

    let mut managed_container_count = 0_usize;
    let mut managed_subscription_count = 0_usize;
    let mut managed_playlist_count = 0_usize;
    let mut matched_managed_dirs = 0_usize;
    let mut managed_name_keys: HashSet<String> = HashSet::new();
    let mut matched_name_keys: HashSet<String> = HashSet::new();
    for row in legacy_state_rows.iter() {
        let service = row.service_name.trim().to_ascii_lowercase();
        let url = row.source_url.trim();
        if service != "youtube" || url.is_empty() {
            continue;
        }
        managed_container_count = managed_container_count.saturating_add(1);
        let kind = classify_legacy_4kvdp_kind(row.container_type, url);
        match kind {
            Legacy4kvdpContainerKind::Subscription => {
                managed_subscription_count = managed_subscription_count.saturating_add(1)
            }
            Legacy4kvdpContainerKind::Playlist => {
                managed_playlist_count = managed_playlist_count.saturating_add(1)
            }
        }

        let Some(base_name) = fourkvd_basename(&row.dirname) else {
            continue;
        };
        let key = legacy_name_key(&base_name);
        managed_name_keys.insert(key.clone());
        let matched_root_path = top_level_dir_name_map.get(&key).cloned();
        if matched_root_path.is_some() && matched_name_keys.insert(key) {
            matched_managed_dirs = matched_managed_dirs.saturating_add(1);
        }
        if managed_container_hints.len() < LEGACY_SAMPLE_NAME_LIMIT {
            managed_container_hints.push(LegacyArchiveManagedContainerHint {
                container_kind: kind.as_str().to_string(),
                relative_path: base_name.clone(),
                title: if row.title.trim().is_empty() {
                    base_name
                } else {
                    row.title.trim().to_string()
                },
                source_url: url.to_string(),
                matched_root_path: matched_root_path
                    .map(|value| value.to_string_lossy().to_string()),
            });
        }
    }

    let top_level_dir_count = top_level_dirs.len();
    let unmatched_top_level_dirs = top_level_dirs
        .iter()
        .filter(|(name, _)| !managed_name_keys.contains(&legacy_name_key(name)))
        .count();
    let sample_unmatched_dirs = top_level_dirs
        .iter()
        .filter(|(name, _)| !managed_name_keys.contains(&legacy_name_key(name)))
        .take(LEGACY_SAMPLE_NAME_LIMIT)
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();

    let container_hint_targets =
        bounded_container_hint_targets(&top_level_dirs, &managed_name_keys);
    for (_, path) in container_hint_targets {
        let count = collect_media_files(path.as_path(), 2, 500).len();
        if count == 0 {
            continue;
        }
        let relative_path = path
            .strip_prefix(&scan_dir)
            .ok()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());
        container_counts.push(LegacyArchiveContainerHint {
            relative_path,
            media_file_count: count,
        });
    }

    container_counts.sort_by(|a, b| {
        b.media_file_count
            .cmp(&a.media_file_count)
            .then_with(|| a.relative_path.cmp(&b.relative_path))
    });
    container_counts.truncate(24);

    let detected_4kvdp_subscriptions_json = scan_dir
        .join(FOURKVDP_SUBSCRIPTIONS_JSON_FILENAME)
        .is_file();
    let detected_4kvdp_subscription_entries_csv = scan_dir
        .join(FOURKVDP_SUBSCRIPTION_ENTRIES_CSV_FILENAME)
        .is_file();
    if detected_4kvdp_subscriptions_json || detected_4kvdp_subscription_entries_csv {
        detected_4kvdp_install = true;
    }
    if install_path_exists {
        detected_4kvdp_install = true;
    }
    if scan_dir.to_string_lossy().starts_with("\\\\") {
        warnings.push(
            "UNC/NAS path detected. VoxVulgi stays read-only here; start with bounded analysis and index incrementally if the share is slow."
                .to_string(),
        );
    }
    if media_files.len() >= scan_max_files {
        warnings.push(format!(
            "Sample limit reached at {scan_max_files} media files. This report is intentionally bounded; increase the limit or index per container/subfolder for large archives."
        ));
    }
    if !install_path_exists && install_path.is_some() {
        warnings.push(
            "The supplied 4K Video Downloader install path does not exist on disk. Metadata detection therefore relied on the archive root only."
                .to_string(),
        );
    }
    if !legacy_state_db_exists {
        warnings.push(
            "No 4KVDP app-state SQLite database was auto-detected in Local AppData. JSON/CSV export import remains available, but managed container mapping will be weaker."
                .to_string(),
        );
    }
    if top_level_file_count > 0 {
        warnings.push(format!(
            "The selected legacy root has {top_level_file_count} loose top-level media file(s). Treat these as manual single-item archives and index them in smaller batches after the managed folders are mapped."
        ));
    }
    if top_level_dir_count > LEGACY_CONTAINER_HINT_SCAN_DIR_LIMIT {
        warnings.push(format!(
            "Container hint scanning is intentionally capped to {LEGACY_CONTAINER_HINT_SCAN_DIR_LIMIT} top-level folders per analysis run so large NAS archives stay responsive."
        ));
    }

    let mut summary = LegacyArchiveAnalysisSummary {
        root_path: scan_dir.to_string_lossy().to_string(),
        install_path: normalized_install_path
            .as_ref()
            .map(|path| path.to_string_lossy().to_string()),
        install_path_exists,
        legacy_state_db_path: legacy_state_db_path
            .as_ref()
            .map(|path| path.to_string_lossy().to_string()),
        legacy_state_db_exists,
        media_file_count: media_files.len(),
        detected_4kvdp_install,
        detected_4kvdp_subscriptions_json,
        detected_4kvdp_subscription_entries_csv,
        detected_channel_dirs,
        detected_playlist_dirs,
        top_level_dir_count,
        top_level_file_count,
        managed_container_count,
        managed_subscription_count,
        managed_playlist_count,
        matched_managed_dirs,
        unmatched_top_level_dirs,
        scan_max_depth,
        scan_max_files,
        local_report_path: String::new(),
        warnings,
        container_hints: container_counts,
        managed_container_hints,
        sample_unmatched_dirs,
        sample_top_level_files,
        sample_media_paths,
        recommendations: build_legacy_archive_recommendations(
            legacy_state_db_exists,
            managed_container_count,
            managed_subscription_count,
            managed_playlist_count,
            matched_managed_dirs,
            unmatched_top_level_dirs,
            top_level_file_count,
            scan_max_files,
        ),
    };
    summary.local_report_path =
        write_legacy_archive_report(paths, &summary).unwrap_or_else(|_| String::new());

    Ok(summary)
}

fn legacy_name_key(value: &str) -> String {
    value.trim().to_lowercase()
}

fn bounded_container_hint_targets(
    top_level_dirs: &[(String, PathBuf)],
    managed_name_keys: &HashSet<String>,
) -> Vec<(String, PathBuf)> {
    let mut selected: Vec<(String, PathBuf)> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for (name, path) in top_level_dirs.iter() {
        let key = legacy_name_key(name);
        if !managed_name_keys.contains(&key) || !seen.insert(key) {
            continue;
        }
        selected.push((name.clone(), path.clone()));
        if selected.len() >= LEGACY_CONTAINER_HINT_SCAN_DIR_LIMIT {
            return selected;
        }
    }

    for (name, path) in top_level_dirs.iter() {
        let key = legacy_name_key(name);
        if !seen.insert(key) {
            continue;
        }
        selected.push((name.clone(), path.clone()));
        if selected.len() >= LEGACY_CONTAINER_HINT_SCAN_DIR_LIMIT {
            break;
        }
    }

    selected
}

fn build_legacy_archive_recommendations(
    legacy_state_db_exists: bool,
    managed_container_count: usize,
    managed_subscription_count: usize,
    managed_playlist_count: usize,
    matched_managed_dirs: usize,
    unmatched_top_level_dirs: usize,
    top_level_file_count: usize,
    scan_max_files: usize,
) -> Vec<String> {
    let mut out = Vec::new();
    if legacy_state_db_exists && managed_container_count > 0 {
        out.push(format!(
            "Import the detected 4KVDP app-state first: {managed_container_count} managed containers were found ({managed_subscription_count} subscription/channel sources and {managed_playlist_count} playlist sources). VoxVulgi can preserve their source URLs, folder mapping, and refresh state from that database."
        ));
        out.push(format!(
            "Map managed containers against the selected root before broad indexing: {matched_managed_dirs} top-level folders already match 4KVDP-managed output directories."
        ));
        out.push(
            "Use the SQLite-based import before any refresh jobs so VoxVulgi can seed yt-dlp archive files from legacy subscription entries and avoid re-downloading known videos."
                .to_string(),
        );
    } else {
        out.push(
            "If the old 4KVDP app-state database is available, import it before indexing the NAS root so VoxVulgi can preserve managed subscription/playlist intent instead of inferring everything from filenames."
                .to_string(),
        );
    }

    if unmatched_top_level_dirs > 0 {
        out.push(format!(
            "Treat the remaining {unmatched_top_level_dirs} top-level folders as manual legacy containers. Index them incrementally by folder/theme/date bucket instead of one giant root pass."
        ));
    }
    if top_level_file_count > 0 {
        out.push(format!(
            "Handle the {top_level_file_count} loose top-level files last. They are best treated as single-item legacy archives rather than subscription or playlist folders."
        ));
    }
    out.push(format!(
        "Keep the analysis bounded on this archive: the current run sampled at most {scan_max_files} media files and the container-hint scan is capped so NAS reads stay deliberate."
    ));
    out
}

fn normalize_legacy_scan_limit(
    value: Option<usize>,
    default_value: usize,
    hard_max: usize,
) -> usize {
    value.unwrap_or(default_value).clamp(1, hard_max)
}

fn normalize_optional_existing_path(path: Option<&Path>) -> Option<PathBuf> {
    let raw = path?;
    let trimmed = raw.as_os_str().to_string_lossy().trim().to_string();
    if trimmed.is_empty() {
        return None;
    }
    let candidate = PathBuf::from(trimmed);
    Some(candidate.canonicalize().unwrap_or(candidate))
}

fn legacy_archive_report_dir(paths: &AppPaths) -> Result<PathBuf> {
    let dir = paths
        .derived_dir()
        .join("reconciliation")
        .join("legacy_archive");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn write_legacy_archive_report(
    paths: &AppPaths,
    summary: &LegacyArchiveAnalysisSummary,
) -> Result<String> {
    let dir = legacy_archive_report_dir(paths)?;
    let out_path = dir.join(format!("legacy_archive_analysis_{}.json", now_ms()));
    let payload = serde_json::to_string_pretty(summary)?;
    std::fs::write(&out_path, format!("{payload}\n"))?;
    Ok(out_path.to_string_lossy().to_string())
}

pub fn export_youtube_subscriptions_json(
    paths: &AppPaths,
    out_path: &Path,
) -> Result<YoutubeSubscriptionsExportSummary> {
    let rows = list_youtube_subscriptions(paths)?;
    let payload = YoutubeSubscriptionsExportFile {
        schema_version: EXPORT_SCHEMA_VERSION,
        exported_at_ms: now_ms(),
        app: "VoxVulgi".to_string(),
        subscriptions: rows
            .iter()
            .map(|row| YoutubeSubscriptionsExportEntry {
                title: row.title.clone(),
                source_url: row.source_url.clone(),
                folder_map: Some(row.folder_map.clone()),
                output_dir_override: row.output_dir_override.clone(),
                use_browser_cookies: row.use_browser_cookies,
                active: row.active,
                preset_id: row.preset_id.clone(),
                group_ids: row.group_ids.clone(),
                refresh_interval_minutes: Some(row.refresh_interval_minutes),
            })
            .collect(),
    };

    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        out_path,
        format!("{}\n", serde_json::to_string_pretty(&payload)?),
    )?;

    Ok(YoutubeSubscriptionsExportSummary {
        out_path: out_path.to_string_lossy().to_string(),
        count: payload.subscriptions.len(),
    })
}

pub fn import_youtube_subscriptions_json(
    paths: &AppPaths,
    in_path: &Path,
) -> Result<YoutubeSubscriptionsImportSummary> {
    let bytes = std::fs::read(in_path)?;
    let payload: YoutubeSubscriptionsExportFile = serde_json::from_slice(&bytes)?;
    if payload.schema_version != EXPORT_SCHEMA_VERSION {
        return Err(EngineError::InstallFailed(format!(
            "unsupported subscriptions export schema_version: {}",
            payload.schema_version
        )));
    }

    let conn = db::open(paths)?;
    db::migrate(&conn)?;

    let mut inserted = 0_usize;
    let mut updated = 0_usize;
    let now = now_ms();
    for raw in &payload.subscriptions {
        let normalized = normalize_upsert(YoutubeSubscriptionUpsert {
            id: None,
            title: raw.title.clone(),
            source_url: raw.source_url.clone(),
            folder_map: raw.folder_map.clone(),
            output_dir_override: raw.output_dir_override.clone(),
            use_browser_cookies: raw.use_browser_cookies,
            auth_session_input: None,
            clear_auth_session: false,
            active: raw.active,
            preset_id: raw.preset_id.clone(),
            group_ids: raw.group_ids.clone(),
            refresh_interval_minutes: raw.refresh_interval_minutes,
        })?;

        let existed =
            subscription_by_source_url_conn(&conn, normalized.source_url.as_str())?.is_some();
        conn.execute(
            r#"
INSERT INTO youtube_subscription (
  id,
  title,
  source_url,
  folder_map,
  output_dir_override,
  use_browser_cookies,
  active,
  preset_id,
  refresh_interval_minutes,
  last_queued_at_ms,
  last_error_at_ms,
  consecutive_failures,
  next_allowed_refresh_at_ms,
  created_at_ms,
  updated_at_ms
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, NULL, 0, NULL, ?10, ?10)
ON CONFLICT(source_url) DO UPDATE SET
  title = excluded.title,
  folder_map = excluded.folder_map,
  output_dir_override = excluded.output_dir_override,
  use_browser_cookies = excluded.use_browser_cookies,
  active = excluded.active,
  preset_id = excluded.preset_id,
  refresh_interval_minutes = excluded.refresh_interval_minutes,
  updated_at_ms = excluded.updated_at_ms
"#,
            params![
                Uuid::new_v4().to_string(),
                normalized.title,
                normalized.source_url,
                normalized.folder_map,
                normalized.output_dir_override,
                bool_to_i64(normalized.use_browser_cookies),
                bool_to_i64(normalized.active),
                normalized.preset_id,
                normalized.refresh_interval_minutes,
                now,
            ],
        )?;
        if let Some(saved) = subscription_by_source_url_conn(&conn, normalized.source_url.as_str())?
        {
            set_subscription_group_memberships_conn(&conn, &saved.id, &normalized.group_ids)?;
        }

        if existed {
            updated += 1;
        } else {
            inserted += 1;
        }
    }

    Ok(YoutubeSubscriptionsImportSummary {
        total_in_file: payload.subscriptions.len(),
        inserted,
        updated,
    })
}

#[derive(Debug, Clone, Deserialize)]
struct FourkvdSubscription {
    id: i64,
    #[serde(default)]
    dirname: String,
    #[serde(default)]
    service: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    handler: String,
    #[serde(default)]
    state: Option<i64>,
    #[serde(default)]
    metadata: Vec<FourkvdSubscriptionMetadata>,
}

#[derive(Debug, Clone, Deserialize)]
struct FourkvdSubscriptionMetadata {
    #[serde(default)]
    r#type: i64,
    #[serde(default)]
    value: String,
}

#[derive(Debug, Clone, Deserialize)]
struct FourkvdSubscriptionEntryRow {
    downloader_subscription_info_id: i64,
    reference: String,
    status: i64,
}

#[derive(Debug, Clone)]
struct Legacy4kvdpStateRow {
    id: i64,
    container_type: i64,
    dirname: String,
    title: String,
    service_name: String,
    source_url: String,
    active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Legacy4kvdpContainerKind {
    Subscription,
    Playlist,
}

impl Legacy4kvdpContainerKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Subscription => "subscription",
            Self::Playlist => "playlist",
        }
    }
}

#[derive(Debug, Clone)]
struct LegacyResolvedOutputDir {
    path: PathBuf,
    matched_root_dir: bool,
    retained_legacy_dir: bool,
}

pub fn import_youtube_subscriptions_4kvdp_dir(
    paths: &AppPaths,
    dir: &Path,
) -> Result<YoutubeSubscriptionsImport4kvdpSummary> {
    let subscriptions_path = dir.join(FOURKVDP_SUBSCRIPTIONS_JSON_FILENAME);
    if !subscriptions_path.exists() {
        return Err(EngineError::InstallFailed(format!(
            "4KVDP import: missing {} in {}",
            FOURKVDP_SUBSCRIPTIONS_JSON_FILENAME,
            dir.to_string_lossy()
        )));
    }

    let bytes = std::fs::read(&subscriptions_path)?;
    let raw_subs: Vec<FourkvdSubscription> = serde_json::from_slice(&bytes)?;

    let conn = db::open(paths)?;
    db::migrate(&conn)?;

    let mut inserted = 0_usize;
    let mut updated = 0_usize;
    let mut skipped_non_youtube = 0_usize;
    let mut imported_subscriptions = 0_usize;
    let now = now_ms();

    // Map 4KVDP subscription id -> normalized source_url (for archive seeding).
    let mut fourk_id_to_source_url: HashMap<i64, String> = HashMap::new();

    for raw in &raw_subs {
        let service = raw.service.trim().to_ascii_lowercase();
        let url = raw.url.trim();
        if service != "youtube" || url.is_empty() {
            skipped_non_youtube += 1;
            continue;
        }

        let title = fourkvd_title(raw);
        let source_url = normalize_youtube_url(url.to_string())?;
        let folder_map = default_folder_map(&title, &source_url);
        let output_dir_override =
            normalize_output_dir(Some(fourkvd_normalize_dirname(&raw.dirname)));
        let active = raw.state.unwrap_or(1) != 0;

        let normalized = normalize_upsert(YoutubeSubscriptionUpsert {
            id: None,
            title,
            source_url: source_url.clone(),
            folder_map: Some(folder_map),
            output_dir_override,
            use_browser_cookies: false,
            auth_session_input: None,
            clear_auth_session: false,
            active,
            preset_id: None,
            group_ids: Vec::new(),
            refresh_interval_minutes: Some(DEFAULT_REFRESH_INTERVAL_MINUTES),
        })?;

        let existed =
            subscription_by_source_url_conn(&conn, normalized.source_url.as_str())?.is_some();
        conn.execute(
            r#"
INSERT INTO youtube_subscription (
  id,
  title,
  source_url,
  folder_map,
  output_dir_override,
  use_browser_cookies,
  active,
  preset_id,
  refresh_interval_minutes,
  last_queued_at_ms,
  last_error_at_ms,
  consecutive_failures,
  next_allowed_refresh_at_ms,
  created_at_ms,
  updated_at_ms
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, NULL, 0, NULL, ?10, ?10)
ON CONFLICT(source_url) DO UPDATE SET
  title = excluded.title,
  folder_map = excluded.folder_map,
  output_dir_override = excluded.output_dir_override,
  use_browser_cookies = excluded.use_browser_cookies,
  active = excluded.active,
  preset_id = excluded.preset_id,
  refresh_interval_minutes = excluded.refresh_interval_minutes,
  updated_at_ms = excluded.updated_at_ms
"#,
            params![
                Uuid::new_v4().to_string(),
                normalized.title,
                normalized.source_url,
                normalized.folder_map,
                normalized.output_dir_override,
                bool_to_i64(normalized.use_browser_cookies),
                bool_to_i64(normalized.active),
                normalized.preset_id,
                normalized.refresh_interval_minutes,
                now,
            ],
        )?;

        imported_subscriptions += 1;
        if existed {
            updated += 1;
        } else {
            inserted += 1;
        }

        fourk_id_to_source_url.insert(raw.id, source_url);
    }

    // Optional: seed archive files from subscription_entries.csv.
    let entries_path = dir.join(FOURKVDP_SUBSCRIPTION_ENTRIES_CSV_FILENAME);
    let (
        archive_seeded_subscriptions,
        archive_seeded_entries,
        archive_skipped_entries,
        archive_seed_failures,
    ) = if entries_path.exists() {
        seed_archives_from_4kvdp_entries(paths, &conn, &fourk_id_to_source_url, &entries_path)?
    } else {
        (0, 0, 0, 0)
    };

    Ok(YoutubeSubscriptionsImport4kvdpSummary {
        total_in_subscriptions_json: raw_subs.len(),
        imported_subscriptions,
        inserted,
        updated,
        skipped_non_youtube,
        archive_seeded_subscriptions,
        archive_seeded_entries,
        archive_skipped_entries,
        archive_seed_failures,
    })
}

pub fn import_youtube_subscriptions_4kvdp_state(
    paths: &AppPaths,
    root_dir: &Path,
    sqlite_path: Option<&Path>,
) -> Result<YoutubeSubscriptionsImport4kvdpStateSummary> {
    let root_dir = root_dir
        .canonicalize()
        .unwrap_or_else(|_| root_dir.to_path_buf());
    if !root_dir.exists() || !root_dir.is_dir() {
        return Err(EngineError::InstallFailed(format!(
            "legacy root not found: {}",
            root_dir.to_string_lossy()
        )));
    }

    let sqlite_path = resolve_legacy_4kvdp_state_db_path(sqlite_path).ok_or_else(|| {
        EngineError::InstallFailed(
            "4KVDP app-state database not found. Analyze the legacy root first or provide a valid SQLite path."
                .to_string(),
        )
    })?;
    let legacy_conn = open_legacy_4kvdp_state_db(&sqlite_path)?;
    let rows = read_legacy_4kvdp_state_rows(&legacy_conn)?;

    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    let now = now_ms();

    let group_all_id = ensure_subscription_group_by_name_conn(&conn, LEGACY_4KVDP_GROUP_ALL)?;
    let group_subscription_id =
        ensure_subscription_group_by_name_conn(&conn, LEGACY_4KVDP_GROUP_SUBSCRIPTIONS)?;
    let group_playlist_id =
        ensure_subscription_group_by_name_conn(&conn, LEGACY_4KVDP_GROUP_PLAYLISTS)?;

    let mut inserted = 0_usize;
    let mut updated = 0_usize;
    let mut skipped_non_youtube = 0_usize;
    let mut imported_sources = 0_usize;
    let mut imported_subscription_sources = 0_usize;
    let mut imported_playlist_sources = 0_usize;
    let mut mapped_to_selected_root = 0_usize;
    let mut retained_existing_legacy_dir = 0_usize;
    let mut missing_target_dirs = 0_usize;
    let mut fourk_id_to_source_url: HashMap<i64, String> = HashMap::new();

    for raw in &rows {
        let service = raw.service_name.trim().to_ascii_lowercase();
        let url = raw.source_url.trim();
        if service != "youtube" || url.is_empty() {
            skipped_non_youtube += 1;
            continue;
        }

        let source_url = match normalize_youtube_url(url.to_string()) {
            Ok(v) => v,
            Err(_) => {
                skipped_non_youtube += 1;
                continue;
            }
        };
        let kind = classify_legacy_4kvdp_kind(raw.container_type, &source_url);
        let title = raw
            .title
            .trim()
            .to_string()
            .chars()
            .take(160)
            .collect::<String>();
        let title = if title.is_empty() {
            fourkvd_basename(&raw.dirname).unwrap_or_else(|| "Imported subscription".to_string())
        } else {
            title
        };
        let resolved_dir = resolve_legacy_output_dir(&root_dir, &raw.dirname);
        if resolved_dir.matched_root_dir {
            mapped_to_selected_root += 1;
        } else if resolved_dir.retained_legacy_dir {
            retained_existing_legacy_dir += 1;
        } else {
            missing_target_dirs += 1;
        }

        let normalized = normalize_upsert(YoutubeSubscriptionUpsert {
            id: None,
            title,
            source_url: source_url.clone(),
            folder_map: Some(default_folder_map(raw.title.as_str(), &source_url)),
            output_dir_override: Some(resolved_dir.path.to_string_lossy().to_string()),
            use_browser_cookies: false,
            auth_session_input: None,
            clear_auth_session: false,
            active: raw.active,
            preset_id: None,
            group_ids: Vec::new(),
            refresh_interval_minutes: Some(DEFAULT_REFRESH_INTERVAL_MINUTES),
        })?;

        let existed =
            subscription_by_source_url_conn(&conn, normalized.source_url.as_str())?.is_some();
        conn.execute(
            r#"
INSERT INTO youtube_subscription (
  id,
  title,
  source_url,
  folder_map,
  output_dir_override,
  use_browser_cookies,
  active,
  preset_id,
  refresh_interval_minutes,
  last_queued_at_ms,
  last_error_at_ms,
  consecutive_failures,
  next_allowed_refresh_at_ms,
  created_at_ms,
  updated_at_ms
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, NULL, 0, NULL, ?10, ?10)
ON CONFLICT(source_url) DO UPDATE SET
  title = excluded.title,
  folder_map = excluded.folder_map,
  output_dir_override = excluded.output_dir_override,
  use_browser_cookies = excluded.use_browser_cookies,
  active = excluded.active,
  preset_id = excluded.preset_id,
  refresh_interval_minutes = excluded.refresh_interval_minutes,
  updated_at_ms = excluded.updated_at_ms
"#,
            params![
                Uuid::new_v4().to_string(),
                normalized.title,
                normalized.source_url,
                normalized.folder_map,
                normalized.output_dir_override,
                bool_to_i64(normalized.use_browser_cookies),
                bool_to_i64(normalized.active),
                normalized.preset_id,
                normalized.refresh_interval_minutes,
                now,
            ],
        )?;

        let sub =
            subscription_by_source_url_conn(&conn, source_url.as_str())?.ok_or_else(|| {
                EngineError::InstallFailed(
                    "failed to reload imported legacy subscription".to_string(),
                )
            })?;
        let group_ids = match kind {
            Legacy4kvdpContainerKind::Subscription => {
                imported_subscription_sources += 1;
                vec![group_all_id.clone(), group_subscription_id.clone()]
            }
            Legacy4kvdpContainerKind::Playlist => {
                imported_playlist_sources += 1;
                vec![group_all_id.clone(), group_playlist_id.clone()]
            }
        };
        set_subscription_group_memberships_conn(&conn, &sub.id, &group_ids)?;

        imported_sources += 1;
        if existed {
            updated += 1;
        } else {
            inserted += 1;
        }
        fourk_id_to_source_url.insert(raw.id, source_url);
    }

    let (
        archive_seeded_subscriptions,
        archive_seeded_entries,
        archive_skipped_entries,
        archive_seed_failures,
    ) = seed_archives_from_4kvdp_state_entries(
        paths,
        &conn,
        &legacy_conn,
        &fourk_id_to_source_url,
    )?;

    Ok(YoutubeSubscriptionsImport4kvdpStateSummary {
        sqlite_path: sqlite_path.to_string_lossy().to_string(),
        total_in_legacy_state: rows.len(),
        imported_sources,
        imported_subscription_sources,
        imported_playlist_sources,
        inserted,
        updated,
        skipped_non_youtube,
        mapped_to_selected_root,
        retained_existing_legacy_dir,
        missing_target_dirs,
        archive_seeded_subscriptions,
        archive_seeded_entries,
        archive_skipped_entries,
        archive_seed_failures,
        group_names: vec![
            LEGACY_4KVDP_GROUP_ALL.to_string(),
            LEGACY_4KVDP_GROUP_SUBSCRIPTIONS.to_string(),
            LEGACY_4KVDP_GROUP_PLAYLISTS.to_string(),
        ],
    })
}

fn seed_archives_from_4kvdp_entries(
    paths: &AppPaths,
    conn: &rusqlite::Connection,
    fourk_id_to_source_url: &HashMap<i64, String>,
    entries_path: &Path,
) -> Result<(usize, usize, usize, usize)> {
    let mut rdr = ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_path(entries_path)?;

    let mut by_source_url: HashMap<String, HashSet<String>> = HashMap::new();
    let mut seeded_entries = 0_usize;
    let mut skipped_entries = 0_usize;

    for result in rdr.deserialize::<FourkvdSubscriptionEntryRow>() {
        let row = match result {
            Ok(v) => v,
            Err(_) => {
                skipped_entries += 1;
                continue;
            }
        };
        // Observed in the exported DB: status=1 overwhelmingly means “downloaded/known”;
        // status=0 is rare and treated as “not downloaded / pending / unavailable”.
        if row.status != 1 {
            skipped_entries += 1;
            continue;
        }
        let Some(source_url) = fourk_id_to_source_url.get(&row.downloader_subscription_info_id)
        else {
            skipped_entries += 1;
            continue;
        };
        let Some(video_id) = youtube_video_id_from_url(row.reference.as_str()) else {
            skipped_entries += 1;
            continue;
        };
        by_source_url
            .entry(source_url.clone())
            .or_default()
            .insert(video_id);
        seeded_entries += 1;
    }

    let mut seeded_subs = 0_usize;
    let mut failures = 0_usize;
    for (source_url, ids) in by_source_url {
        let Some(sub) = subscription_by_source_url_conn(conn, source_url.as_str())? else {
            continue;
        };

        let archive_path = match ensure_youtube_subscription_archive_state(paths, &sub) {
            Ok(path) => path,
            Err(_) => {
                failures += 1;
                continue;
            }
        };

        if merge_archive_file(&archive_path, &ids).is_err() {
            failures += 1;
            continue;
        }
        seeded_subs += 1;
    }

    Ok((seeded_subs, seeded_entries, skipped_entries, failures))
}

fn seed_archives_from_4kvdp_state_entries(
    paths: &AppPaths,
    conn: &rusqlite::Connection,
    legacy_conn: &rusqlite::Connection,
    fourk_id_to_source_url: &HashMap<i64, String>,
) -> Result<(usize, usize, usize, usize)> {
    let mut stmt = legacy_conn.prepare(
        r#"
SELECT downloader_subscription_info_id, reference, status
FROM subscription_entries
ORDER BY downloader_subscription_info_id ASC, id ASC
"#,
    )?;

    let mut by_source_url: HashMap<String, HashSet<String>> = HashMap::new();
    let mut seeded_entries = 0_usize;
    let mut skipped_entries = 0_usize;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let subscription_id: i64 = row.get(0)?;
        let reference: String = row.get(1)?;
        let status: i64 = row.get(2)?;
        if status != 1 {
            skipped_entries += 1;
            continue;
        }
        let Some(source_url) = fourk_id_to_source_url.get(&subscription_id) else {
            skipped_entries += 1;
            continue;
        };
        let Some(video_id) = youtube_video_id_from_url(reference.as_str()) else {
            skipped_entries += 1;
            continue;
        };
        by_source_url
            .entry(source_url.clone())
            .or_default()
            .insert(video_id);
        seeded_entries += 1;
    }

    let mut seeded_subs = 0_usize;
    let mut failures = 0_usize;
    for (source_url, ids) in by_source_url {
        let Some(sub) = subscription_by_source_url_conn(conn, source_url.as_str())? else {
            continue;
        };

        let archive_path = match ensure_youtube_subscription_archive_state(paths, &sub) {
            Ok(path) => path,
            Err(_) => {
                failures += 1;
                continue;
            }
        };

        if merge_archive_file(&archive_path, &ids).is_err() {
            failures += 1;
            continue;
        }
        seeded_subs += 1;
    }

    Ok((seeded_subs, seeded_entries, skipped_entries, failures))
}

fn ensure_subscription_group_by_name_conn(
    conn: &rusqlite::Connection,
    name: &str,
) -> Result<String> {
    if let Some(id) = conn
        .query_row(
            "SELECT id FROM youtube_subscription_group WHERE lower(name) = lower(?1) LIMIT 1",
            params![name],
            |row| row.get::<_, String>(0),
        )
        .optional()?
    {
        return Ok(id);
    }

    let id = Uuid::new_v4().to_string();
    let now = now_ms();
    conn.execute(
        "INSERT INTO youtube_subscription_group (id, name, created_at_ms, updated_at_ms) VALUES (?1, ?2, ?3, ?3)",
        params![&id, name, now],
    )?;
    Ok(id)
}

fn detect_legacy_4kvdp_state_db_path() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)?
        .join("4kdownload.com")
        .join("4K Video Downloader+")
        .join("4K Video Downloader+");
    if !base.is_dir() {
        return None;
    }

    let mut candidates = std::fs::read_dir(&base)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let is_sqlite = path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("sqlite"))
                .unwrap_or(false);
            if !is_sqlite {
                return None;
            }
            let len = entry.metadata().ok()?.len();
            Some((len, path))
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    candidates.into_iter().map(|(_, path)| path).next()
}

fn resolve_legacy_4kvdp_state_db_path(explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = normalize_optional_existing_path(explicit) {
        if path.is_file() {
            return Some(path);
        }
    }
    detect_legacy_4kvdp_state_db_path()
}

fn open_legacy_4kvdp_state_db(path: &Path) -> Result<rusqlite::Connection> {
    rusqlite::Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(Into::into)
}

fn read_legacy_4kvdp_state_rows(conn: &rusqlite::Connection) -> Result<Vec<Legacy4kvdpStateRow>> {
    let mut stmt = conn.prepare(
        r#"
SELECT
  s.id,
  s.type,
  COALESCE(s.dirname, ''),
  COALESCE(MAX(CASE WHEN m.type = 1 THEN m.value END), ''),
  COALESCE(u.service_name, ''),
  COALESCE(u.url, ''),
  COALESCE(st.state, 1)
FROM downloader_subscription_info s
LEFT JOIN subscription_url_description u ON u.downloader_subscription_info_id = s.id
LEFT JOIN subscription_state st ON st.downloader_subscription_info_id = s.id
LEFT JOIN subscription_metadata m ON m.downloader_subscription_info_id = s.id
GROUP BY s.id, s.type, s.dirname, u.service_name, u.url, st.state
ORDER BY s.id ASC
"#,
    )?;

    let rows = stmt
        .query_map([], |row| {
            Ok(Legacy4kvdpStateRow {
                id: row.get(0)?,
                container_type: row.get(1)?,
                dirname: row.get(2)?,
                title: row.get(3)?,
                service_name: row.get(4)?,
                source_url: row.get(5)?,
                active: row.get::<_, i64>(6)? != 0,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn classify_legacy_4kvdp_kind(container_type: i64, source_url: &str) -> Legacy4kvdpContainerKind {
    if container_type == 3 || is_youtube_playlist_reference(source_url) {
        Legacy4kvdpContainerKind::Playlist
    } else {
        Legacy4kvdpContainerKind::Subscription
    }
}

fn is_youtube_playlist_reference(source_url: &str) -> bool {
    let Ok(parsed) = Url::parse(source_url) else {
        return false;
    };
    let Some(host) = parsed.host_str().map(|value| value.to_ascii_lowercase()) else {
        return false;
    };
    if host != "youtube.com" && host != "www.youtube.com" && !host.ends_with(".youtube.com") {
        return false;
    }
    parsed.query_pairs().any(|(key, _)| key == "list") || parsed.path().starts_with("/playlist")
}

fn resolve_legacy_output_dir(root_dir: &Path, legacy_dirname: &str) -> LegacyResolvedOutputDir {
    let normalized_dir = PathBuf::from(fourkvd_normalize_dirname(legacy_dirname));
    if let Some(base_name) = fourkvd_basename(legacy_dirname) {
        let root_candidate = root_dir.join(base_name);
        if root_candidate.is_dir() {
            return LegacyResolvedOutputDir {
                path: root_candidate,
                matched_root_dir: true,
                retained_legacy_dir: false,
            };
        }
        if normalized_dir.is_dir() {
            return LegacyResolvedOutputDir {
                path: normalized_dir,
                matched_root_dir: false,
                retained_legacy_dir: true,
            };
        }
        return LegacyResolvedOutputDir {
            path: root_candidate,
            matched_root_dir: false,
            retained_legacy_dir: false,
        };
    }

    if normalized_dir.is_dir() {
        return LegacyResolvedOutputDir {
            path: normalized_dir,
            matched_root_dir: false,
            retained_legacy_dir: true,
        };
    }

    LegacyResolvedOutputDir {
        path: root_dir.to_path_buf(),
        matched_root_dir: false,
        retained_legacy_dir: false,
    }
}

fn merge_archive_file(path: &Path, video_ids: &HashSet<String>) -> std::io::Result<(usize, usize)> {
    let mut existing: HashSet<String> = HashSet::new();
    if path.exists() {
        if let Ok(file) = std::fs::File::open(path) {
            let reader = BufReader::new(file);
            for line in reader.lines().flatten() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                // Accept either “youtube <id>” or raw “<id>” in existing files.
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() == 2 {
                    existing.insert(parts[1].to_string());
                } else {
                    existing.insert(trimmed.to_string());
                }
            }
        }
    }

    let mut merged: Vec<String> = existing.into_iter().collect();
    let mut appended = 0_usize;
    let mut skipped_existing = 0_usize;
    for id in video_ids {
        if !merged.iter().any(|v| v == id) {
            merged.push(id.clone());
            appended += 1;
        } else {
            skipped_existing += 1;
        }
    }
    merged.sort();

    let mut file = std::fs::File::create(path)?;
    for id in merged {
        writeln!(file, "youtube {id}")?;
    }
    Ok((appended, skipped_existing))
}

fn read_archive_file_ids(path: &Path) -> std::io::Result<HashSet<String>> {
    let mut out: HashSet<String> = HashSet::new();
    if !path.exists() {
        return Ok(out);
    }

    let file = std::fs::File::open(path)?;
    let reader = BufReader::new(file);
    for line in reader.lines().flatten() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() == 2 {
            out.insert(parts[1].to_string());
        } else {
            out.insert(trimmed.to_string());
        }
    }
    Ok(out)
}

pub fn youtube_subscription_output_dir(
    paths: &AppPaths,
    sub: &YoutubeSubscriptionRow,
) -> Result<PathBuf> {
    if let Some(override_dir) = normalize_output_dir(sub.output_dir_override.clone()) {
        let mut p = PathBuf::from(override_dir);
        if !p.is_absolute() {
            p = std::env::current_dir()?.join(p);
        }
        return Ok(p);
    }

    let base_dir = paths.effective_download_dir()?;
    Ok(base_dir
        .join("video")
        .join("subscriptions")
        .join(sanitize_folder_map(&sub.folder_map)))
}

fn legacy_output_youtube_subscription_archive_path(
    paths: &AppPaths,
    sub: &YoutubeSubscriptionRow,
) -> Result<PathBuf> {
    Ok(youtube_subscription_output_dir(paths, sub)?.join(YT_DLP_ARCHIVE_FILENAME))
}

pub fn youtube_subscription_archive_path(
    paths: &AppPaths,
    sub: &YoutubeSubscriptionRow,
) -> Result<PathBuf> {
    Ok(paths.youtube_subscription_archive_state_path(&sub.id))
}

pub fn ensure_youtube_subscription_archive_state(
    paths: &AppPaths,
    sub: &YoutubeSubscriptionRow,
) -> Result<PathBuf> {
    let archive_path = youtube_subscription_archive_path(paths, sub)?;
    if let Some(parent) = archive_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if !archive_path.exists() {
        let legacy_path = legacy_output_youtube_subscription_archive_path(paths, sub)?;
        if legacy_path != archive_path && legacy_path.exists() {
            let legacy_ids = read_archive_file_ids(&legacy_path)?;
            if !legacy_ids.is_empty() {
                merge_archive_file(&archive_path, &legacy_ids)?;
            }
        }
    }
    Ok(archive_path)
}

pub fn load_youtube_subscription_archive_ids(
    paths: &AppPaths,
    sub: &YoutubeSubscriptionRow,
) -> Result<HashSet<String>> {
    let archive_path = ensure_youtube_subscription_archive_state(paths, sub)?;
    read_archive_file_ids(&archive_path).map_err(Into::into)
}

pub fn youtube_subscriptions_archive_stats(
    paths: &AppPaths,
) -> Result<HashMap<String, usize>> {
    let subs = list_youtube_subscriptions(paths)?;
    let mut stats = HashMap::with_capacity(subs.len());
    for sub in &subs {
        let count = match load_youtube_subscription_archive_ids(paths, sub) {
            Ok(ids) => ids.len(),
            Err(_) => 0,
        };
        stats.insert(sub.id.clone(), count);
    }
    Ok(stats)
}

fn fourkvd_title(raw: &FourkvdSubscription) -> String {
    if let Some(value) = raw
        .metadata
        .iter()
        .find(|m| m.r#type == 1)
        .map(|m| m.value.trim().to_string())
        .filter(|v| !v.is_empty())
    {
        return value;
    }

    if let Some(last) = fourkvd_basename(&raw.dirname) {
        if !last.is_empty() {
            return last;
        }
    }

    "Imported subscription".to_string()
}

fn fourkvd_basename(dirname: &str) -> Option<String> {
    let trimmed = dirname.trim();
    if trimmed.is_empty() {
        return None;
    }
    let parts: Vec<&str> = trimmed
        .trim_end_matches('/')
        .trim_end_matches('\\')
        .split(|ch| ch == '/' || ch == '\\')
        .filter(|p| !p.trim().is_empty())
        .collect();
    parts.last().map(|v| v.to_string())
}

fn fourkvd_normalize_dirname(dirname: &str) -> String {
    let trimmed = dirname.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    if cfg!(windows) {
        // 4KVDP exports often use `//server/share/...` and `/` separators. Convert to a normal UNC path.
        return trimmed.replace('/', "\\"); // leading `//` becomes `\\\\`.
    }

    trimmed.to_string()
}

pub(crate) fn youtube_video_id_from_url(url: &str) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    let host = parsed.host_str()?.to_ascii_lowercase();
    if host == "youtu.be" {
        return parsed
            .path_segments()
            .and_then(|mut s| s.next())
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
    }
    if host == "youtube.com" || host == "www.youtube.com" || host.ends_with(".youtube.com") {
        let path = parsed.path();
        if path.starts_with("/watch") {
            for (k, v) in parsed.query_pairs() {
                if k == "v" {
                    let out = v.trim().to_string();
                    if !out.is_empty() {
                        return Some(out);
                    }
                }
            }
        }
        if let Some(id) = path.strip_prefix("/shorts/") {
            let out = id.split('/').next().unwrap_or("").trim().to_string();
            if !out.is_empty() {
                return Some(out);
            }
        }
        if let Some(id) = path.strip_prefix("/live/") {
            let out = id.split('/').next().unwrap_or("").trim().to_string();
            if !out.is_empty() {
                return Some(out);
            }
        }
    }
    None
}

fn queue_subscription_internal(
    paths: &AppPaths,
    sub: &YoutubeSubscriptionRow,
    batch_id: Option<String>,
) -> Result<Vec<jobs::JobRow>> {
    let output_dir = youtube_subscription_output_dir(paths, sub)?
        .to_string_lossy()
        .to_string();
    let auth_cookie =
        jobs::read_auth_cookie_secret_path(&paths.youtube_subscription_cookie_secret_path(&sub.id));
    let queued = jobs::enqueue_youtube_subscription_refresh_v1(
        paths,
        sub.id.clone(),
        Some(output_dir),
        batch_id,
        auth_cookie,
    )?;

    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    conn.execute(
        "UPDATE youtube_subscription SET last_queued_at_ms = ?1, updated_at_ms = ?1 WHERE id = ?2",
        params![now_ms(), sub.id],
    )?;

    Ok(vec![queued])
}

fn hydrate_group_ids(
    conn: &rusqlite::Connection,
    mut rows: Vec<YoutubeSubscriptionRow>,
) -> Result<Vec<YoutubeSubscriptionRow>> {
    for row in rows.iter_mut() {
        row.group_ids = list_group_ids_for_subscription_conn(conn, &row.id)?;
    }
    Ok(rows)
}

fn hydrate_auth_session_flags(
    paths: &AppPaths,
    mut rows: Vec<YoutubeSubscriptionRow>,
) -> Vec<YoutubeSubscriptionRow> {
    for row in rows.iter_mut() {
        row.auth_session_configured = youtube_subscription_has_auth_session(paths, &row.id);
    }
    rows
}

fn youtube_subscription_has_auth_session(paths: &AppPaths, subscription_id: &str) -> bool {
    paths
        .youtube_subscription_cookie_secret_path(subscription_id)
        .exists()
}

fn sync_auth_session_secret(
    paths: &AppPaths,
    subscription_id: &str,
    auth_session_input: Option<&str>,
    clear_auth_session: bool,
) -> Result<()> {
    let secret_path = paths.youtube_subscription_cookie_secret_path(subscription_id);
    if let Some(value) = auth_session_input.map(str::trim).filter(|value| !value.is_empty()) {
        jobs::write_auth_cookie_secret_path(&secret_path, value)?;
    } else if clear_auth_session {
        jobs::remove_auth_cookie_secret_path(&secret_path);
    }
    Ok(())
}

fn list_groups_conn(conn: &rusqlite::Connection) -> Result<Vec<YoutubeSubscriptionGroupRow>> {
    let mut stmt = conn.prepare(
        r#"
SELECT id, name, created_at_ms, updated_at_ms
FROM youtube_subscription_group
ORDER BY lower(name) ASC, created_at_ms ASC
"#,
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(YoutubeSubscriptionGroupRow {
                id: row.get(0)?,
                name: row.get(1)?,
                created_at_ms: row.get(2)?,
                updated_at_ms: row.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn get_group_by_id_conn(
    conn: &rusqlite::Connection,
    group_id: &str,
) -> Result<Option<YoutubeSubscriptionGroupRow>> {
    conn.query_row(
        "SELECT id, name, created_at_ms, updated_at_ms FROM youtube_subscription_group WHERE id = ?1",
        params![group_id],
        |row| {
            Ok(YoutubeSubscriptionGroupRow {
                id: row.get(0)?,
                name: row.get(1)?,
                created_at_ms: row.get(2)?,
                updated_at_ms: row.get(3)?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

fn list_group_ids_for_subscription_conn(
    conn: &rusqlite::Connection,
    subscription_id: &str,
) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT group_id FROM youtube_subscription_group_member WHERE subscription_id = ?1 ORDER BY group_id ASC",
    )?;
    let rows = stmt
        .query_map(params![subscription_id], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn set_subscription_group_memberships_conn(
    conn: &rusqlite::Connection,
    subscription_id: &str,
    group_ids: &[String],
) -> Result<()> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut normalized: Vec<String> = Vec::new();
    for raw in group_ids {
        let trimmed = raw.trim();
        if trimmed.is_empty() || !seen.insert(trimmed.to_string()) {
            continue;
        }
        normalized.push(trimmed.to_string());
    }

    conn.execute(
        "DELETE FROM youtube_subscription_group_member WHERE subscription_id = ?1",
        params![subscription_id],
    )?;
    let now = now_ms();
    for group_id in normalized {
        let exists = get_group_by_id_conn(conn, &group_id)?.is_some();
        if !exists {
            continue;
        }
        conn.execute(
            "INSERT OR IGNORE INTO youtube_subscription_group_member (subscription_id, group_id, created_at_ms) VALUES (?1, ?2, ?3)",
            params![subscription_id, group_id, now],
        )?;
    }
    Ok(())
}

fn resolve_seed_target_subscriptions(
    paths: &AppPaths,
    scan_dir: &Path,
    subscription_id: Option<String>,
) -> Result<Vec<YoutubeSubscriptionRow>> {
    if let Some(id) = subscription_id
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        if let Some(sub) = get_youtube_subscription_by_id(paths, id)? {
            return Ok(vec![sub]);
        }
        return Err(EngineError::InstallFailed(format!(
            "subscription not found: {id}"
        )));
    }

    let mut subs = list_youtube_subscriptions(paths)?
        .into_iter()
        .filter(|sub| sub.active)
        .collect::<Vec<_>>();
    if subs.is_empty() {
        return Ok(Vec::new());
    }

    let scan_dir = scan_dir
        .canonicalize()
        .unwrap_or_else(|_| scan_dir.to_path_buf());
    let mut matched: Vec<YoutubeSubscriptionRow> = Vec::new();
    for sub in subs.iter() {
        let output_dir = youtube_subscription_output_dir(paths, sub)?
            .canonicalize()
            .unwrap_or_else(|_| youtube_subscription_output_dir(paths, sub).unwrap_or_default());
        if scan_dir.starts_with(&output_dir) || output_dir.starts_with(&scan_dir) {
            matched.push(sub.clone());
        }
    }
    if matched.is_empty() {
        matched.append(&mut subs);
    }
    Ok(matched)
}

fn infer_youtube_ids_from_dir(scan_dir: &Path) -> HashSet<String> {
    static YT_ID_RE: OnceLock<Regex> = OnceLock::new();
    let regex = YT_ID_RE.get_or_init(|| {
        Regex::new(r"(?i)(?:^|[^A-Za-z0-9_-])([A-Za-z0-9_-]{11})(?:$|[^A-Za-z0-9_-])").unwrap()
    });
    let mut ids: HashSet<String> = HashSet::new();
    let mut stack = vec![scan_dir.to_path_buf()];
    let max_depth = 6_usize;
    while let Some(dir) = stack.pop() {
        let depth = dir
            .strip_prefix(scan_dir)
            .ok()
            .map(|p| p.components().count())
            .unwrap_or(0);
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                if depth < max_depth {
                    stack.push(path);
                }
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            let candidate = path
                .file_stem()
                .and_then(|v| v.to_str())
                .unwrap_or_default()
                .to_string();
            for caps in regex.captures_iter(&candidate) {
                if let Some(m) = caps.get(1) {
                    let value = m.as_str();
                    if value
                        .chars()
                        .any(|ch| ch.is_ascii_digit() || ch == '-' || ch == '_')
                    {
                        ids.insert(value.to_string());
                    }
                }
            }
        }
    }
    ids
}

fn collect_media_files(scan_dir: &Path, max_depth: usize, max_files: usize) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = Vec::new();
    let mut stack: Vec<(PathBuf, usize)> = vec![(scan_dir.to_path_buf(), 0)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                if depth < max_depth {
                    stack.push((path, depth + 1));
                }
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            if !is_media_file_ext(&path) {
                continue;
            }
            files.push(path);
            if files.len() >= max_files {
                return files;
            }
        }
    }
    files
}

fn is_media_file_ext(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(
        ext.as_str(),
        "mp4"
            | "mkv"
            | "mov"
            | "webm"
            | "m4v"
            | "avi"
            | "mp3"
            | "m4a"
            | "wav"
            | "flac"
            | "aac"
            | "ogg"
            | "opus"
    )
}

fn subscription_by_id_conn(
    conn: &rusqlite::Connection,
    id: &str,
) -> Result<Option<YoutubeSubscriptionRow>> {
    let mut stmt = conn.prepare(
        r#"
SELECT
  id,
  title,
  source_url,
  folder_map,
  output_dir_override,
  use_browser_cookies,
  active,
  preset_id,
  refresh_interval_minutes,
  last_queued_at_ms,
  last_error_at_ms,
  consecutive_failures,
  next_allowed_refresh_at_ms,
  created_at_ms,
  updated_at_ms
FROM youtube_subscription
WHERE id = ?1
"#,
    )?;

    let row = stmt.query_row([id], row_to_subscription).optional()?;
    Ok(row)
}

fn subscription_by_source_url_conn(
    conn: &rusqlite::Connection,
    source_url: &str,
) -> Result<Option<YoutubeSubscriptionRow>> {
    let mut stmt = conn.prepare(
        r#"
SELECT
  id,
  title,
  source_url,
  folder_map,
  output_dir_override,
  use_browser_cookies,
  active,
  preset_id,
  refresh_interval_minutes,
  last_queued_at_ms,
  last_error_at_ms,
  consecutive_failures,
  next_allowed_refresh_at_ms,
  created_at_ms,
  updated_at_ms
FROM youtube_subscription
WHERE source_url = ?1
"#,
    )?;

    let row = stmt
        .query_row([source_url], row_to_subscription)
        .optional()?;
    Ok(row)
}

fn normalize_upsert(req: YoutubeSubscriptionUpsert) -> Result<NormalizedSubscriptionInput> {
    let title = normalize_title(req.title)?;
    let source_url = normalize_youtube_url(req.source_url)?;
    let folder_map = req
        .folder_map
        .as_ref()
        .map(|v| sanitize_folder_map(v))
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| default_folder_map(&title, &source_url));
    let output_dir_override = normalize_output_dir(req.output_dir_override);
    let preset_id = req
        .preset_id
        .as_deref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let group_ids = req
        .group_ids
        .into_iter()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .collect::<Vec<_>>();
    let id = req
        .id
        .as_deref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());

    Ok(NormalizedSubscriptionInput {
        id,
        title,
        source_url,
        folder_map,
        output_dir_override,
        use_browser_cookies: req.use_browser_cookies,
        auth_session_input: jobs::normalize_auth_cookie(req.auth_session_input)?,
        clear_auth_session: req.clear_auth_session,
        active: req.active,
        preset_id,
        group_ids,
        refresh_interval_minutes: normalize_refresh_interval_minutes(req.refresh_interval_minutes),
    })
}

fn normalize_refresh_interval_minutes(value: Option<i64>) -> i64 {
    value
        .unwrap_or(DEFAULT_REFRESH_INTERVAL_MINUTES)
        .clamp(MIN_REFRESH_INTERVAL_MINUTES, MAX_REFRESH_INTERVAL_MINUTES)
}

fn normalize_title(raw: String) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(EngineError::InstallFailed(
            "subscription title cannot be empty".to_string(),
        ));
    }
    let mut out = trimmed.to_string();
    if out.len() > 200 {
        out.truncate(200);
    }
    Ok(out)
}

fn normalize_youtube_url(raw: String) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(EngineError::InstallFailed(
            "subscription URL cannot be empty".to_string(),
        ));
    }
    let mut parsed = Url::parse(trimmed)
        .map_err(|_| EngineError::InstallFailed(format!("invalid URL: {trimmed}")))?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err(EngineError::InstallFailed(
            "subscription URL must use http/https".to_string(),
        ));
    }

    let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
    let is_youtube = host == "youtu.be" || host == "youtube.com" || host.ends_with(".youtube.com");
    if !is_youtube {
        return Err(EngineError::InstallFailed(
            "subscription URL must be a YouTube URL".to_string(),
        ));
    }

    parsed.set_fragment(None);
    Ok(parsed.to_string())
}

fn sanitize_folder_map(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    let mut trimmed = out.trim_matches(|ch| ch == '_' || ch == '.').to_string();
    if trimmed.len() > 80 {
        trimmed.truncate(80);
    }
    trimmed
}

fn default_folder_map(title: &str, source_url: &str) -> String {
    let by_title = sanitize_folder_map(title);
    if !by_title.is_empty() {
        return by_title;
    }

    if let Ok(parsed) = Url::parse(source_url) {
        let path = parsed
            .path_segments()
            .and_then(|mut seg| seg.next_back())
            .unwrap_or_default();
        let from_url = sanitize_folder_map(path);
        if !from_url.is_empty() {
            return from_url;
        }
    }

    DEFAULT_SUBSCRIPTION_MAP.to_string()
}

fn normalize_output_dir(value: Option<String>) -> Option<String> {
    let raw = value.unwrap_or_default();
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn row_to_subscription(row: &rusqlite::Row<'_>) -> rusqlite::Result<YoutubeSubscriptionRow> {
    Ok(YoutubeSubscriptionRow {
        id: row.get(0)?,
        title: row.get(1)?,
        source_url: row.get(2)?,
        folder_map: row.get(3)?,
        output_dir_override: row.get(4)?,
        use_browser_cookies: i64_to_bool(row.get::<_, i64>(5)?),
        auth_session_configured: false,
        active: i64_to_bool(row.get::<_, i64>(6)?),
        preset_id: row.get(7)?,
        refresh_interval_minutes: row.get(8)?,
        last_queued_at_ms: row.get(9)?,
        last_error_at_ms: row.get(10)?,
        consecutive_failures: row.get(11)?,
        next_allowed_refresh_at_ms: row.get(12)?,
        created_at_ms: row.get(13)?,
        updated_at_ms: row.get(14)?,
        group_ids: Vec::new(),
    })
}

fn bool_to_i64(value: bool) -> i64 {
    if value {
        1
    } else {
        0
    }
}

fn i64_to_bool(value: i64) -> bool {
    value != 0
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[derive(Debug, Clone)]
struct NormalizedSubscriptionInput {
    id: Option<String>,
    title: String,
    source_url: String,
    folder_map: String,
    output_dir_override: Option<String>,
    use_browser_cookies: bool,
    auth_session_input: Option<String>,
    clear_auth_session: bool,
    active: bool,
    preset_id: Option<String>,
    group_ids: Vec<String>,
    refresh_interval_minutes: i64,
}

trait OptionalRowExt<T> {
    fn optional(self) -> rusqlite::Result<Option<T>>;
}

impl<T> OptionalRowExt<T> for rusqlite::Result<T> {
    fn optional(self) -> rusqlite::Result<Option<T>> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::AppPaths;

    #[test]
    fn import_upserts_by_source_url() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        crate::db::ensure_schema(&paths).expect("schema");

        let _ = upsert_youtube_subscription(
            &paths,
            YoutubeSubscriptionUpsert {
                id: None,
                title: "Original".to_string(),
                source_url: "https://www.youtube.com/@example/videos".to_string(),
                folder_map: Some("example_map".to_string()),
                output_dir_override: None,
                use_browser_cookies: false,
                auth_session_input: None,
                clear_auth_session: false,
                active: true,
                preset_id: None,
                group_ids: Vec::new(),
                refresh_interval_minutes: Some(DEFAULT_REFRESH_INTERVAL_MINUTES),
            },
        )
        .expect("seed");

        let import_path = dir.path().join("subscriptions_import.json");
        let payload = serde_json::json!({
            "schema_version": 1,
            "exported_at_ms": 0,
            "app": "VoxVulgi",
            "subscriptions": [
                {
                    "title": "Updated title",
                    "source_url": "https://www.youtube.com/@example/videos",
                    "folder_map": "updated_map",
                    "output_dir_override": null,
                    "use_browser_cookies": true,
                    "active": true,
                    "refresh_interval_minutes": 90
                },
                {
                    "title": "Second",
                    "source_url": "https://www.youtube.com/playlist?list=PL123456",
                    "folder_map": "second_map",
                    "output_dir_override": null,
                    "use_browser_cookies": false,
                    "active": true,
                    "refresh_interval_minutes": 30
                }
            ]
        });
        std::fs::write(
            &import_path,
            format!(
                "{}\n",
                serde_json::to_string_pretty(&payload).expect("json")
            ),
        )
        .expect("write import");

        let summary = import_youtube_subscriptions_json(&paths, &import_path).expect("import");
        assert_eq!(summary.total_in_file, 2);
        assert_eq!(summary.inserted, 1);
        assert_eq!(summary.updated, 1);

        let rows = list_youtube_subscriptions(&paths).expect("list");
        assert_eq!(rows.len(), 2);
        let updated = rows
            .iter()
            .find(|row| row.source_url.contains("@example"))
            .expect("updated row");
        assert_eq!(updated.title, "Updated title");
        assert_eq!(updated.folder_map, "updated_map");
        assert!(updated.use_browser_cookies);
        assert_eq!(updated.refresh_interval_minutes, 90);
    }

    #[test]
    fn queue_uses_subscription_folder_map_output() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        crate::db::ensure_schema(&paths).expect("schema");
        paths
            .set_download_dir_override(&dir.path().join("downloads"))
            .expect("set download dir");

        let sub = upsert_youtube_subscription(
            &paths,
            YoutubeSubscriptionUpsert {
                id: None,
                title: "Map Test".to_string(),
                source_url: "https://www.youtube.com/watch?v=abc123".to_string(),
                folder_map: Some("mapped_channel".to_string()),
                output_dir_override: None,
                use_browser_cookies: false,
                auth_session_input: None,
                clear_auth_session: false,
                active: true,
                preset_id: None,
                group_ids: Vec::new(),
                refresh_interval_minutes: Some(DEFAULT_REFRESH_INTERVAL_MINUTES),
            },
        )
        .expect("upsert");

        let queued = queue_youtube_subscription(&paths, &sub.id).expect("queue");
        assert_eq!(queued.len(), 1);

        let conn = crate::db::open(&paths).expect("db open");
        crate::db::migrate(&conn).expect("migrate");
        let params_json: String = conn
            .query_row(
                "SELECT params_json FROM job WHERE id = ?1",
                [queued[0].id.clone()],
                |row| row.get(0),
            )
            .expect("params");
        let params: serde_json::Value = serde_json::from_str(&params_json).expect("params json");
        let output_dir = params
            .get("output_dir")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        assert!(
            output_dir.contains("video")
                && output_dir.contains("subscriptions")
                && output_dir.contains("mapped_channel"),
            "expected mapped subscription folder in output_dir, got {output_dir}"
        );
    }

    #[test]
    fn upsert_saved_auth_session_persists_secret_and_attaches_to_refresh_job() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        crate::db::ensure_schema(&paths).expect("schema");

        let sub = upsert_youtube_subscription(
            &paths,
            YoutubeSubscriptionUpsert {
                id: None,
                title: "Auth".to_string(),
                source_url: "https://www.youtube.com/@auth/videos".to_string(),
                folder_map: None,
                output_dir_override: None,
                use_browser_cookies: false,
                auth_session_input: Some(
                    r#"[{"name":"SAPISID","value":"cookie123"}]"#.to_string(),
                ),
                clear_auth_session: false,
                active: true,
                preset_id: None,
                group_ids: Vec::new(),
                refresh_interval_minutes: Some(DEFAULT_REFRESH_INTERVAL_MINUTES),
            },
        )
        .expect("upsert");

        assert!(sub.auth_session_configured, "saved auth session should be surfaced");
        let stored = jobs::read_auth_cookie_secret_path(
            &paths.youtube_subscription_cookie_secret_path(&sub.id),
        )
        .expect("subscription auth secret");
        assert_eq!(stored, "SAPISID=cookie123");

        let queued = queue_youtube_subscription(&paths, &sub.id).expect("queue");
        let job_secret = jobs::read_auth_cookie_secret_path(&paths.job_cookie_secret_path(&queued[0].id))
            .expect("job auth secret");
        assert_eq!(job_secret, "SAPISID=cookie123");
    }

    #[test]
    fn upsert_clamps_refresh_interval_minutes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        crate::db::ensure_schema(&paths).expect("schema");

        let low = upsert_youtube_subscription(
            &paths,
            YoutubeSubscriptionUpsert {
                id: None,
                title: "Low".to_string(),
                source_url: "https://www.youtube.com/@low/videos".to_string(),
                folder_map: None,
                output_dir_override: None,
                use_browser_cookies: false,
                auth_session_input: None,
                clear_auth_session: false,
                active: true,
                preset_id: None,
                group_ids: Vec::new(),
                refresh_interval_minutes: Some(1),
            },
        )
        .expect("upsert low");
        assert_eq!(low.refresh_interval_minutes, MIN_REFRESH_INTERVAL_MINUTES);

        let high = upsert_youtube_subscription(
            &paths,
            YoutubeSubscriptionUpsert {
                id: None,
                title: "High".to_string(),
                source_url: "https://www.youtube.com/@high/videos".to_string(),
                folder_map: None,
                output_dir_override: None,
                use_browser_cookies: false,
                auth_session_input: None,
                clear_auth_session: false,
                active: true,
                preset_id: None,
                group_ids: Vec::new(),
                refresh_interval_minutes: Some(999999),
            },
        )
        .expect("upsert high");
        assert_eq!(high.refresh_interval_minutes, MAX_REFRESH_INTERVAL_MINUTES);
    }

    #[test]
    fn queue_all_active_respects_refresh_interval() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        crate::db::ensure_schema(&paths).expect("schema");

        let due = upsert_youtube_subscription(
            &paths,
            YoutubeSubscriptionUpsert {
                id: None,
                title: "Due".to_string(),
                source_url: "https://www.youtube.com/@due/videos".to_string(),
                folder_map: None,
                output_dir_override: None,
                use_browser_cookies: false,
                auth_session_input: None,
                clear_auth_session: false,
                active: true,
                preset_id: None,
                group_ids: Vec::new(),
                refresh_interval_minutes: Some(5),
            },
        )
        .expect("upsert due");
        let not_due = upsert_youtube_subscription(
            &paths,
            YoutubeSubscriptionUpsert {
                id: None,
                title: "Not Due".to_string(),
                source_url: "https://www.youtube.com/@notdue/videos".to_string(),
                folder_map: None,
                output_dir_override: None,
                use_browser_cookies: false,
                auth_session_input: None,
                clear_auth_session: false,
                active: true,
                preset_id: None,
                group_ids: Vec::new(),
                refresh_interval_minutes: Some(60),
            },
        )
        .expect("upsert not due");

        let now = now_ms();
        let conn = crate::db::open(&paths).expect("open db");
        crate::db::migrate(&conn).expect("migrate");
        conn.execute(
            "UPDATE youtube_subscription SET last_queued_at_ms = ?1 WHERE id = ?2",
            params![now - (6 * 60 * 1000), due.id],
        )
        .expect("set due last queued");
        conn.execute(
            "UPDATE youtube_subscription SET last_queued_at_ms = ?1 WHERE id = ?2",
            params![now - (30 * 60 * 1000), not_due.id],
        )
        .expect("set not due last queued");

        let queued = queue_all_active_youtube_subscriptions(&paths).expect("queue active");
        assert_eq!(queued.len(), 1);

        let rows = list_youtube_subscriptions(&paths).expect("list");
        let due_row = rows.iter().find(|row| row.id == due.id).expect("due row");
        let not_due_row = rows
            .iter()
            .find(|row| row.id == not_due.id)
            .expect("not due row");
        assert!(
            due_row.last_queued_at_ms.unwrap_or(0) >= now,
            "due row should be re-queued"
        );
        assert_eq!(
            not_due_row.last_queued_at_ms.unwrap_or(0),
            now - (30 * 60 * 1000),
            "not due row should keep original last_queued_at_ms"
        );
    }

    #[test]
    fn import_4kvdp_dir_seeds_archive() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        crate::db::ensure_schema(&paths).expect("schema");

        let export_dir = dir.path().join("4kvdp_export");
        std::fs::create_dir_all(&export_dir).expect("mkdir");

        // Two youtube subscriptions + one non-youtube entry.
        let subs_json = serde_json::json!([
            {
                "id": 395,
                "service": "youtube",
                "url": "https://www.youtube.com/channel/UCi_YgCDnd1bz70I6YgBi1rw",
                "dirname": format!("{}/out/Marshmallow", dir.path().to_string_lossy()),
                "state": 1,
                "metadata": [{ "type": 1, "value": "marshmallow" }]
            },
            {
                "id": 396,
                "service": "youtube",
                "url": "http://www.youtube.com/playlist?list=PLFt9cqwyhCQ8mES1Vy0rrFKNyeh9zlJlZ",
                "dirname": format!("{}/out/Playlist", dir.path().to_string_lossy()),
                "state": 1,
                "metadata": [{ "type": 1, "value": "playlist_title" }]
            },
            { "id": 1, "service": "other", "url": "https://example.com", "dirname": "x" }
        ]);
        std::fs::write(
            export_dir.join(FOURKVDP_SUBSCRIPTIONS_JSON_FILENAME),
            serde_json::to_string_pretty(&subs_json).unwrap(),
        )
        .expect("write subs");

        // Seed only status=1 into archive.
        let entries_csv = "\
downloader_subscription_info_id,entry_id,reference,status\n\
395,1,https://www.youtube.com/watch?v=AAAA1111,1\n\
395,2,https://www.youtube.com/watch?v=BBBB2222,0\n\
396,3,https://youtu.be/CCCC3333,1\n\
999,4,https://www.youtube.com/watch?v=DDDD4444,1\n\
";
        std::fs::write(
            export_dir.join(FOURKVDP_SUBSCRIPTION_ENTRIES_CSV_FILENAME),
            entries_csv,
        )
        .expect("write csv");

        let summary = import_youtube_subscriptions_4kvdp_dir(&paths, &export_dir).expect("import");
        assert_eq!(summary.imported_subscriptions, 2);
        assert_eq!(summary.inserted, 2);
        assert_eq!(summary.archive_seeded_subscriptions, 2);
        assert!(summary.archive_seeded_entries >= 2);

        let rows = list_youtube_subscriptions(&paths).expect("list");
        assert_eq!(rows.len(), 2);

        let sub_a = rows
            .iter()
            .find(|s| s.source_url.contains("channel/UCi_"))
            .unwrap();
        let sub_b = rows
            .iter()
            .find(|s| s.source_url.contains("playlist?list=PLFt9"))
            .unwrap();

        let arch_a = youtube_subscription_archive_path(&paths, sub_a).expect("arch a");
        let arch_b = youtube_subscription_archive_path(&paths, sub_b).expect("arch b");
        let a_text = std::fs::read_to_string(arch_a).expect("read a");
        let b_text = std::fs::read_to_string(arch_b).expect("read b");
        assert!(a_text.contains("youtube AAAA1111"));
        assert!(!a_text.contains("BBBB2222")); // status=0 should not seed
        assert!(b_text.contains("youtube CCCC3333"));
    }

    fn with_localappdata_override<T>(path: &Path, f: impl FnOnce() -> T) -> T {
        static LOCK: OnceLock<std::sync::Mutex<()>> = OnceLock::new();
        let _guard = LOCK
            .get_or_init(|| std::sync::Mutex::new(()))
            .lock()
            .expect("lock");
        let previous = std::env::var_os("LOCALAPPDATA");
        std::env::set_var("LOCALAPPDATA", path);
        let result = f();
        match previous {
            Some(value) => std::env::set_var("LOCALAPPDATA", value),
            None => std::env::remove_var("LOCALAPPDATA"),
        }
        result
    }

    fn seed_legacy_4kvdp_state_db(sqlite_path: &Path, root_dir: &Path) -> rusqlite::Result<()> {
        if let Some(parent) = sqlite_path.parent() {
            std::fs::create_dir_all(parent).expect("mkdir sqlite parent");
        }
        let conn = rusqlite::Connection::open(sqlite_path)?;
        conn.execute_batch(
            r#"
CREATE TABLE downloader_subscription_info (
  id INTEGER PRIMARY KEY,
  type INTEGER NOT NULL,
  dirname TEXT,
  parent_id INTEGER,
  uuid TEXT
);
CREATE TABLE subscription_url_description (
  downloader_subscription_info_id INTEGER,
  id INTEGER PRIMARY KEY,
  type INTEGER,
  service_name TEXT,
  url TEXT,
  handler_name TEXT
);
CREATE TABLE subscription_state (
  downloader_subscription_info_id INTEGER,
  id INTEGER PRIMARY KEY,
  state INTEGER
);
CREATE TABLE subscription_metadata (
  downloader_subscription_info_id INTEGER,
  id INTEGER PRIMARY KEY,
  type INTEGER,
  value TEXT
);
CREATE TABLE subscription_entries (
  downloader_subscription_info_id INTEGER,
  id INTEGER PRIMARY KEY,
  reference TEXT,
  status INTEGER
);
"#,
        )?;

        let sub_dir = root_dir.join("Creator Videos");
        let playlist_dir = root_dir.join("Playlist Folder");
        conn.execute(
            "INSERT INTO downloader_subscription_info (id, type, dirname, parent_id, uuid) VALUES (1, 1, ?1, NULL, 'uuid-sub')",
            params![sub_dir.to_string_lossy().replace('\\', "/")],
        )?;
        conn.execute(
            "INSERT INTO downloader_subscription_info (id, type, dirname, parent_id, uuid) VALUES (2, 3, ?1, NULL, 'uuid-playlist')",
            params![playlist_dir.to_string_lossy().replace('\\', "/")],
        )?;
        conn.execute(
            "INSERT INTO subscription_url_description (downloader_subscription_info_id, id, type, service_name, url, handler_name) VALUES (1, 1, 2, 'youtube', 'https://www.youtube.com/@creator/videos', 'youtube')",
            [],
        )?;
        conn.execute(
            "INSERT INTO subscription_url_description (downloader_subscription_info_id, id, type, service_name, url, handler_name) VALUES (2, 2, 1, 'youtube', 'https://www.youtube.com/playlist?list=PLTEST1234567890', 'youtube')",
            [],
        )?;
        conn.execute(
            "INSERT INTO subscription_state (downloader_subscription_info_id, id, state) VALUES (1, 1, 1)",
            [],
        )?;
        conn.execute(
            "INSERT INTO subscription_state (downloader_subscription_info_id, id, state) VALUES (2, 2, 1)",
            [],
        )?;
        conn.execute(
            "INSERT INTO subscription_metadata (downloader_subscription_info_id, id, type, value) VALUES (1, 1, 1, 'Creator Videos')",
            [],
        )?;
        conn.execute(
            "INSERT INTO subscription_metadata (downloader_subscription_info_id, id, type, value) VALUES (2, 2, 1, 'Playlist Folder')",
            [],
        )?;
        conn.execute(
            "INSERT INTO subscription_entries (downloader_subscription_info_id, id, reference, status) VALUES (1, 1, 'https://www.youtube.com/watch?v=AAAA1111AAA', 1)",
            [],
        )?;
        conn.execute(
            "INSERT INTO subscription_entries (downloader_subscription_info_id, id, reference, status) VALUES (2, 2, 'https://youtu.be/BBBB2222BBB', 1)",
            [],
        )?;
        Ok(())
    }

    #[test]
    fn analyze_legacy_archive_root_correlates_4kvdp_state_and_root_shape() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        crate::db::ensure_schema(&paths).expect("schema");

        let root = dir.path().join("legacy_root");
        std::fs::create_dir_all(root.join("Creator Videos")).expect("mkdir sub");
        std::fs::create_dir_all(root.join("Playlist Folder")).expect("mkdir playlist");
        std::fs::create_dir_all(root.join("Manual Folder")).expect("mkdir manual");
        std::fs::write(root.join("loose_file.mp4"), b"x").expect("seed loose");
        std::fs::write(root.join("Creator Videos").join("one.mp4"), b"x").expect("seed media");

        let localapp = dir.path().join("LocalAppData");
        let sqlite_path = localapp
            .join("4kdownload.com")
            .join("4K Video Downloader+")
            .join("4K Video Downloader+")
            .join("legacy.sqlite");
        seed_legacy_4kvdp_state_db(&sqlite_path, &root).expect("seed sqlite");

        let summary = with_localappdata_override(&localapp, || {
            analyze_legacy_archive_root(&paths, &root, None, Some(3), Some(100)).expect("analyze")
        });

        assert!(summary.legacy_state_db_exists);
        assert_eq!(summary.managed_container_count, 2);
        assert_eq!(summary.managed_subscription_count, 1);
        assert_eq!(summary.managed_playlist_count, 1);
        assert_eq!(summary.matched_managed_dirs, 2);
        assert_eq!(summary.unmatched_top_level_dirs, 1);
        assert_eq!(summary.top_level_file_count, 1);
        assert!(summary
            .sample_unmatched_dirs
            .iter()
            .any(|value| value == "Manual Folder"));
        assert!(summary
            .recommendations
            .iter()
            .any(|line| line.contains("Import the detected 4KVDP app-state first")));
    }

    #[test]
    fn import_4kvdp_state_maps_to_root_and_seeds_archives() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        crate::db::ensure_schema(&paths).expect("schema");

        let root = dir.path().join("legacy_root");
        std::fs::create_dir_all(root.join("Creator Videos")).expect("mkdir sub");
        std::fs::create_dir_all(root.join("Playlist Folder")).expect("mkdir playlist");

        let sqlite_path = dir.path().join("legacy.sqlite");
        seed_legacy_4kvdp_state_db(&sqlite_path, &root).expect("seed sqlite");

        let summary = import_youtube_subscriptions_4kvdp_state(&paths, &root, Some(&sqlite_path))
            .expect("import state");
        assert_eq!(summary.imported_sources, 2);
        assert_eq!(summary.imported_subscription_sources, 1);
        assert_eq!(summary.imported_playlist_sources, 1);
        assert_eq!(summary.mapped_to_selected_root, 2);
        assert_eq!(summary.archive_seeded_subscriptions, 2);

        let rows = list_youtube_subscriptions(&paths).expect("list");
        assert_eq!(rows.len(), 2);
        let creator = rows
            .iter()
            .find(|row| row.source_url.contains("@creator/videos"))
            .expect("creator row");
        let playlist = rows
            .iter()
            .find(|row| row.source_url.contains("playlist?list=PLTEST1234567890"))
            .expect("playlist row");
        let creator_output_dir = PathBuf::from(
            creator
                .output_dir_override
                .clone()
                .expect("creator output dir"),
        );
        let playlist_output_dir = PathBuf::from(
            playlist
                .output_dir_override
                .clone()
                .expect("playlist output dir"),
        );
        assert_eq!(
            creator_output_dir
                .file_name()
                .and_then(|value| value.to_str()),
            Some("Creator Videos")
        );
        assert_eq!(
            playlist_output_dir
                .file_name()
                .and_then(|value| value.to_str()),
            Some("Playlist Folder")
        );
        assert_eq!(creator.group_ids.len(), 2);
        assert!(creator.group_ids != playlist.group_ids);

        let creator_archive =
            youtube_subscription_archive_path(&paths, creator).expect("creator archive");
        let playlist_archive =
            youtube_subscription_archive_path(&paths, playlist).expect("playlist archive");
        assert!(creator_archive.starts_with(paths.youtube_subscription_state_dir()));
        assert!(playlist_archive.starts_with(paths.youtube_subscription_state_dir()));
        assert!(std::fs::read_to_string(creator_archive)
            .expect("read creator archive")
            .contains("youtube AAAA1111AAA"));
        assert!(std::fs::read_to_string(playlist_archive)
            .expect("read playlist archive")
            .contains("youtube BBBB2222BBB"));
    }

    #[test]
    fn ensure_archive_state_merges_legacy_output_archive_into_app_managed_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        crate::db::ensure_schema(&paths).expect("schema");

        let legacy_output_dir = dir.path().join("legacy_output");
        std::fs::create_dir_all(&legacy_output_dir).expect("mkdir legacy output");
        let legacy_archive_path = legacy_output_dir.join(YT_DLP_ARCHIVE_FILENAME);
        std::fs::write(
            &legacy_archive_path,
            "youtube dQw4w9WgXcQ\nyoutube 5NV6Rdv1a3I\n",
        )
        .expect("seed legacy archive");

        let sub = upsert_youtube_subscription(
            &paths,
            YoutubeSubscriptionUpsert {
                id: None,
                title: "Legacy NAS sub".to_string(),
                source_url: "https://www.youtube.com/@legacy/videos".to_string(),
                folder_map: Some("legacy_nas_sub".to_string()),
                output_dir_override: Some(legacy_output_dir.to_string_lossy().to_string()),
                use_browser_cookies: false,
                auth_session_input: None,
                clear_auth_session: false,
                active: true,
                preset_id: None,
                group_ids: Vec::new(),
                refresh_interval_minutes: Some(DEFAULT_REFRESH_INTERVAL_MINUTES),
            },
        )
        .expect("upsert sub");

        let archive_path =
            ensure_youtube_subscription_archive_state(&paths, &sub).expect("ensure archive state");
        let archived_ids =
            load_youtube_subscription_archive_ids(&paths, &sub).expect("load archived ids");

        assert!(archive_path.starts_with(paths.youtube_subscription_state_dir()));
        assert_ne!(archive_path, legacy_archive_path);
        assert!(archive_path.is_file());
        assert!(legacy_archive_path.is_file());
        assert!(archived_ids.contains("dQw4w9WgXcQ"));
        assert!(archived_ids.contains("5NV6Rdv1a3I"));
    }

    #[test]
    fn infer_youtube_ids_from_dir_extracts_ids_from_media_filenames() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let nested = root.join("nested");
        std::fs::create_dir_all(&nested).expect("mkdir");

        std::fs::write(root.join("ChannelName - dQw4w9WgXcQ.mp4"), b"x").expect("seed root file");
        std::fs::write(nested.join("download_[5NV6Rdv1a3I].mkv"), b"x").expect("seed nested file");
        std::fs::write(nested.join("ignore_text_only.mp4"), b"x").expect("seed text file");

        let ids = infer_youtube_ids_from_dir(root);
        assert!(ids.contains("dQw4w9WgXcQ"));
        assert!(ids.contains("5NV6Rdv1a3I"));
        assert_eq!(ids.len(), 2);
    }

    #[test]
    fn queue_all_active_skips_subscriptions_under_backoff() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        crate::db::ensure_schema(&paths).expect("schema");

        let sub = upsert_youtube_subscription(
            &paths,
            YoutubeSubscriptionUpsert {
                id: None,
                title: "Backoff".to_string(),
                source_url: "https://www.youtube.com/watch?v=dQw4w9WgXcQ".to_string(),
                folder_map: Some("backoff".to_string()),
                output_dir_override: None,
                use_browser_cookies: false,
                auth_session_input: None,
                clear_auth_session: false,
                active: true,
                preset_id: None,
                group_ids: Vec::new(),
                refresh_interval_minutes: Some(MIN_REFRESH_INTERVAL_MINUTES),
            },
        )
        .expect("upsert");

        record_subscription_refresh_failure(&paths, &sub.id).expect("record failure");
        let blocked = queue_all_active_youtube_subscriptions(&paths).expect("queue blocked");
        assert!(
            blocked.is_empty(),
            "subscription should be blocked by backoff"
        );

        let conn = crate::db::open(&paths).expect("open");
        crate::db::migrate(&conn).expect("migrate");
        conn.execute(
            "UPDATE youtube_subscription SET next_allowed_refresh_at_ms = ?1, last_queued_at_ms = NULL WHERE id = ?2",
            params![now_ms().saturating_sub(1000), &sub.id],
        )
        .expect("force ready");

        let queued = queue_all_active_youtube_subscriptions(&paths).expect("queue ready");
        assert_eq!(queued.len(), 1);
    }
}
