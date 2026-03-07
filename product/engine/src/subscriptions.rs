use crate::paths::AppPaths;
use crate::{db, jobs, library, EngineError, Result};
use csv::ReaderBuilder;
use regex::Regex;
use rusqlite::params;
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoutubeSubscriptionRow {
    pub id: String,
    pub title: String,
    pub source_url: String,
    pub folder_map: String,
    pub output_dir_override: Option<String>,
    pub use_browser_cookies: bool,
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
pub struct LegacyArchiveAnalysisSummary {
    pub root_path: String,
    pub install_path: Option<String>,
    pub install_path_exists: bool,
    pub media_file_count: usize,
    pub detected_4kvdp_install: bool,
    pub detected_4kvdp_subscriptions_json: bool,
    pub detected_4kvdp_subscription_entries_csv: bool,
    pub detected_channel_dirs: usize,
    pub detected_playlist_dirs: usize,
    pub scan_max_depth: usize,
    pub scan_max_files: usize,
    pub local_report_path: String,
    pub warnings: Vec<String>,
    pub container_hints: Vec<LegacyArchiveContainerHint>,
    pub sample_media_paths: Vec<String>,
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
    hydrate_group_ids(&conn, rows)
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
    row.group_ids = normalized.group_ids;
    Ok(row)
}

pub fn delete_youtube_subscription(paths: &AppPaths, id: &str) -> Result<()> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    conn.execute("DELETE FROM youtube_subscription WHERE id = ?1", [id])?;
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
    Ok(hydrated.pop())
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
        let archive_path = youtube_subscription_archive_path(paths, &sub)?;
        if let Some(parent) = archive_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
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
    let mut detected_channel_dirs = 0_usize;
    let mut detected_playlist_dirs = 0_usize;
    let mut detected_4kvdp_install = false;
    let mut warnings: Vec<String> = Vec::new();
    let normalized_install_path = normalize_optional_existing_path(install_path);
    let install_path_exists = normalized_install_path
        .as_ref()
        .map(|path| path.exists())
        .unwrap_or(false);

    let entries = std::fs::read_dir(&scan_dir)?;
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
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
        let count = collect_media_files(&path, 2, 500).len();
        if count > 0 {
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

    let mut summary = LegacyArchiveAnalysisSummary {
        root_path: scan_dir.to_string_lossy().to_string(),
        install_path: normalized_install_path
            .as_ref()
            .map(|path| path.to_string_lossy().to_string()),
        install_path_exists,
        media_file_count: media_files.len(),
        detected_4kvdp_install,
        detected_4kvdp_subscriptions_json,
        detected_4kvdp_subscription_entries_csv,
        detected_channel_dirs,
        detected_playlist_dirs,
        scan_max_depth,
        scan_max_files,
        local_report_path: String::new(),
        warnings,
        container_hints: container_counts,
        sample_media_paths,
    };
    summary.local_report_path =
        write_legacy_archive_report(paths, &summary).unwrap_or_else(|_| String::new());

    Ok(summary)
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

        let archive_path = youtube_subscription_archive_path(paths, &sub)?;
        if let Some(parent) = archive_path.parent() {
            if let Err(_) = std::fs::create_dir_all(parent) {
                failures += 1;
                continue;
            }
        }

        if merge_archive_file(&archive_path, &ids).is_err() {
            failures += 1;
            continue;
        }
        seeded_subs += 1;
    }

    Ok((seeded_subs, seeded_entries, skipped_entries, failures))
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

pub fn youtube_subscription_archive_path(
    paths: &AppPaths,
    sub: &YoutubeSubscriptionRow,
) -> Result<PathBuf> {
    Ok(youtube_subscription_output_dir(paths, sub)?.join(YT_DLP_ARCHIVE_FILENAME))
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
    let queued = jobs::enqueue_youtube_subscription_refresh_v1(
        paths,
        sub.id.clone(),
        Some(output_dir),
        batch_id,
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
