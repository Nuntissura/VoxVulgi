use crate::paths::AppPaths;
use crate::{db, jobs, library, persistence, root_rebind, video_libraries, EngineError, Result};
use csv::ReaderBuilder;
use regex::Regex;
use rusqlite::{params, OpenFlags};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};
use url::Url;
use uuid::Uuid;

const EXPORT_SCHEMA_VERSION: u32 = 2;
const MIN_SUPPORTED_EXPORT_SCHEMA_VERSION: u32 = 1;
const DEFAULT_SUBSCRIPTION_MAP: &str = "subscription";
pub const YOUTUBE_SUBSCRIPTION_STATUS_NORMAL: &str = "normal";
pub const YOUTUBE_SUBSCRIPTION_STATUS_UNAVAILABLE: &str = "unavailable";
pub const YOUTUBE_SUBSCRIPTION_STATUS_DELETED: &str = "deleted";
pub const YOUTUBE_SUBSCRIPTION_STATUS_ACTOR_OPERATOR: &str = "operator";
pub const YOUTUBE_SUBSCRIPTION_STATUS_ACTOR_ASSISTANT: &str = "assistant";
// WP-0255: operator does not expect uploads frequently per subscription; default a new
// subscription to a 12-hour refresh (was 60 min). UI edits this in hours. Storage stays
// minutes; existing rows keep their stored value. MIN 5 min / MAX 7 days unchanged.
const DEFAULT_REFRESH_INTERVAL_MINUTES: i64 = 720;
const MIN_REFRESH_INTERVAL_MINUTES: i64 = 5;
const MAX_REFRESH_INTERVAL_MINUTES: i64 = 10080;
const FOURKVDP_SUBSCRIPTIONS_JSON_FILENAME: &str = "subscriptions.json";
const FOURKVDP_SUBSCRIPTION_ENTRIES_CSV_FILENAME: &str = "subscription_entries.csv";
const YT_DLP_ARCHIVE_FILENAME: &str = "voxvulgi_youtube_archive.txt";
const YT_DLP_ARCHIVE_MERGE_JOURNAL_FILENAME: &str = "voxvulgi_youtube_archive.merge-journal";
const YOUTUBE_ARCHIVE_JOURNAL_REPLAY_LIMIT: usize = 128;
const YOUTUBE_ARCHIVE_CARRIER_CLEANUP_RETRY_LIMIT: usize = 16;
const YOUTUBE_ARCHIVE_CARRIER_CLEANUP_ERROR_PREFIX: &str = "archive carrier cleanup:";
static YOUTUBE_ARCHIVE_MUTATION_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Debug, Default, PartialEq, Eq)]
struct ArchiveCarrierCleanupReport {
    removed: usize,
    failed: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct YoutubeArchiveMergeCarrier {
    schema_version: u32,
    intent_id: String,
}

#[derive(Debug, Clone)]
struct YoutubeArchiveMergeIntent {
    intent_id: String,
    subscription_id: String,
    target_archive_path: String,
    source_archive_sha256: String,
    intended_archive_sha256: String,
    intended_video_ids: Vec<String>,
    phase: String,
}

#[derive(Debug, Clone)]
struct YoutubeArchiveMergeIntentRow {
    intent_id: String,
    subscription_id: String,
    target_archive_path: String,
    source_archive_sha256: String,
    intended_archive_sha256: String,
    intended_video_ids_json: String,
    phase: String,
}
const DEFAULT_LEGACY_ANALYSIS_MAX_DEPTH: usize = 4;
const DEFAULT_LEGACY_ANALYSIS_MAX_FILES: usize = 2500;
const DEFAULT_LEGACY_IMPORT_MAX_DEPTH: usize = 8;
const DEFAULT_LEGACY_IMPORT_MAX_FILES: usize = 25000;
const MAX_LEGACY_ANALYSIS_MAX_DEPTH: usize = 16;
const MAX_LEGACY_ANALYSIS_MAX_FILES: usize = 100000;
const LEGACY_CONTAINER_HINT_SCAN_DIR_LIMIT: usize = 120;
const LEGACY_SAMPLE_NAME_LIMIT: usize = 24;
// WP-0259: operator treats 4K Video Downloader-imported and new subscriptions identically and
// wants no "legacy" wording in the app. Neutral display names for the groups an import creates
// (identifiers kept internal). Existing "Legacy 4KVDP*" groups are renamed in place by db v19.
const LEGACY_4KVDP_GROUP_ALL: &str = "Imported";
const LEGACY_4KVDP_GROUP_SUBSCRIPTIONS: &str = "Imported subscriptions";
const LEGACY_4KVDP_GROUP_PLAYLISTS: &str = "Imported playlists";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoutubeSubscriptionRow {
    pub id: String,
    pub title: String,
    pub source_url: String,
    pub folder_map: String,
    pub output_dir_override: Option<String>,
    pub library_id: Option<String>,
    pub use_browser_cookies: bool,
    pub browser_cookie_source: Option<String>,
    pub auth_session_configured: bool,
    pub active: bool,
    #[serde(default = "default_youtube_subscription_source_status")]
    pub source_status: String,
    #[serde(default)]
    pub source_status_changed_at_ms: Option<i64>,
    #[serde(default)]
    pub source_status_change_source: Option<String>,
    pub preset_id: Option<String>,
    pub refresh_interval_minutes: i64,
    pub last_queued_at_ms: Option<i64>,
    pub last_error_at_ms: Option<i64>,
    // WP-0264: failure-state telegraphing. Raw (truncated ~500 chars) error text from the last
    // failed refresh, so the FE `classifyFailure` can derive a state + required action without a
    // per-poll join to the job. Cleared (NULL) on a successful refresh. Additive schema v21.
    #[serde(default)]
    pub last_error_message: Option<String>,
    pub consecutive_failures: i64,
    pub next_allowed_refresh_at_ms: Option<i64>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    // WP-0255: honest per-subscription progress (additive schema v18). Written by the
    // refresh job on completion; only `archive` downloaded count comes from the fs side
    // channel (archive_stats), the rest live on the row.
    #[serde(default)]
    pub last_checked_at_ms: Option<i64>,
    #[serde(default)]
    pub upstream_total: Option<i64>,
    #[serde(default)]
    pub last_new_found: Option<i64>,
    #[serde(default)]
    pub last_refresh_queued: Option<i64>,
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
    #[serde(default)]
    pub library_id: Option<String>,
    pub use_browser_cookies: bool,
    #[serde(default)]
    pub browser_cookie_source: Option<String>,
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
pub struct YoutubeSubscriptionStatusChangeReceipt {
    pub subscription: YoutubeSubscriptionRow,
    pub canceled_refresh_jobs: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoutubeSubscriptionOutputPreviewRequest {
    pub title: String,
    pub source_url: String,
    pub folder_map: Option<String>,
    pub output_dir_override: Option<String>,
    #[serde(default)]
    pub library_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoutubeSubscriptionOutputPreview {
    pub path: String,
    pub exists: bool,
    pub uses_output_override: bool,
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
    pub identity_scanned_items: usize,
    pub identity_exact_items: usize,
    pub identity_ambiguous_items: usize,
    pub identity_unresolved_items: usize,
    pub identity_conflict_items: usize,
    pub source_memberships_added: usize,
    pub group_names: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct YoutubeImportedIdentityEnrichmentSummary {
    pub sqlite_path: String,
    pub dry_run: bool,
    pub source_schema_supported: bool,
    pub source_download_evidence_rows: usize,
    pub complete: bool,
    pub next_cursor: Option<String>,
    pub scanned_items: usize,
    pub exact_items: usize,
    pub ambiguous_items: usize,
    pub unresolved_items: usize,
    pub conflict_items: usize,
    pub already_linked_items: usize,
    pub evidence_rows_written: usize,
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
    #[serde(default)]
    browser_cookie_source: Option<String>,
    active: bool,
    #[serde(default = "default_youtube_subscription_source_status")]
    source_status: String,
    #[serde(default)]
    preset_id: Option<String>,
    #[serde(default)]
    group_ids: Vec<String>,
    #[serde(default)]
    refresh_interval_minutes: Option<i64>,
}

pub fn list_youtube_subscriptions(paths: &AppPaths) -> Result<Vec<YoutubeSubscriptionRow>> {
    // WP-0224: read-only connection bypasses the job-runner write queue.
    let conn = db::open_readonly(paths)?;

    // WP-0223: replaced N+1 (one main SELECT + one hydrate query per row)
    // with a single SELECT that GROUP_CONCATs group_ids via a correlated
    // subquery. With 50 subscriptions this cuts ~51 DB round-trips to 1.
    // Newline is the separator because subscription/group IDs are UUIDs
    // (hex + dashes only), so it cannot collide with a real id character.
    let mut stmt = conn.prepare(
        r#"
SELECT
  s.id,
  s.title,
  s.source_url,
  s.folder_map,
  s.output_dir_override,
  s.library_id,
  s.browser_cookie_source,
  s.use_browser_cookies,
  s.active,
  s.preset_id,
  s.refresh_interval_minutes,
  s.last_queued_at_ms,
  s.last_error_at_ms,
  s.consecutive_failures,
  s.next_allowed_refresh_at_ms,
  s.created_at_ms,
  s.updated_at_ms,
  s.source_status,
  s.source_status_changed_at_ms,
  s.source_status_change_source,
  s.last_checked_at_ms,
  s.upstream_total,
  s.last_new_found,
  s.last_refresh_queued,
  s.last_error_message,
  COALESCE(
    (SELECT GROUP_CONCAT(m.group_id, char(10))
     FROM youtube_subscription_group_member m
     WHERE m.subscription_id = s.id),
    ''
  ) AS group_ids_concat
FROM youtube_subscription s
ORDER BY
  CASE s.source_status WHEN 'deleted' THEN 2 WHEN 'unavailable' THEN 1 ELSE 0 END,
  s.active DESC,
  s.updated_at_ms DESC,
  s.created_at_ms DESC
"#,
    )?;

    let rows = stmt
        .query_map([], |row| {
            let mut subscription = row_to_subscription(row)?;
            // WP-0255: progress fields (schema v18) follow the WP-0282 status fields.
            subscription.last_checked_at_ms = row.get(20)?;
            subscription.upstream_total = row.get(21)?;
            subscription.last_new_found = row.get(22)?;
            subscription.last_refresh_queued = row.get(23)?;
            // WP-0264: persisted refresh error; group concat remains the final field.
            subscription.last_error_message = row.get(24)?;
            let concat: String = row.get(25)?;
            if !concat.is_empty() {
                let mut ids: Vec<String> = concat.split('\n').map(String::from).collect();
                ids.sort();
                subscription.group_ids = ids;
            }
            Ok(subscription)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
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
    validate_video_library_id(&conn, normalized.library_id.as_deref())?;

    if let Some(id) = input_id.as_deref() {
        let changed = conn.execute(
            r#"
UPDATE youtube_subscription
SET
  title = ?1,
  source_status = CASE
    WHEN source_status = 'unavailable' AND source_url <> ?2 THEN 'normal'
    ELSE source_status
  END,
  source_status_changed_at_ms = CASE
    WHEN source_status = 'unavailable' AND source_url <> ?2 THEN ?11
    ELSE source_status_changed_at_ms
  END,
  source_status_change_source = CASE
    WHEN source_status = 'unavailable' AND source_url <> ?2 THEN 'url_edit'
    ELSE source_status_change_source
  END,
  consecutive_failures = CASE WHEN source_url <> ?2 THEN 0 ELSE consecutive_failures END,
  last_error_at_ms = CASE WHEN source_url <> ?2 THEN NULL ELSE last_error_at_ms END,
  last_error_message = CASE WHEN source_url <> ?2 THEN NULL ELSE last_error_message END,
  next_allowed_refresh_at_ms = CASE WHEN source_url <> ?2 THEN NULL ELSE next_allowed_refresh_at_ms END,
  source_url = ?2,
  folder_map = ?3,
  output_dir_override = ?4,
  library_id = ?5,
  browser_cookie_source = ?6,
  use_browser_cookies = ?7,
  active = CASE WHEN source_status = 'deleted' THEN 0 ELSE ?8 END,
  preset_id = ?9,
  refresh_interval_minutes = ?10,
  updated_at_ms = ?11
WHERE id = ?12
"#,
            params![
                &normalized.title,
                &normalized.source_url,
                &normalized.folder_map,
                &normalized.output_dir_override,
                &normalized.library_id,
                &normalized.browser_cookie_source,
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
  library_id,
  browser_cookie_source,
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
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, NULL, NULL, 0, NULL, ?12, ?12)
ON CONFLICT(source_url) DO UPDATE SET
  title = excluded.title,
  folder_map = excluded.folder_map,
  output_dir_override = excluded.output_dir_override,
  library_id = COALESCE(excluded.library_id, youtube_subscription.library_id),
  browser_cookie_source = excluded.browser_cookie_source,
  use_browser_cookies = excluded.use_browser_cookies,
  active = CASE
    WHEN youtube_subscription.source_status = 'deleted' THEN 0
    ELSE excluded.active
  END,
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
                &normalized.library_id,
                &normalized.browser_cookie_source,
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

pub fn set_youtube_subscription_manual_status(
    paths: &AppPaths,
    id: &str,
    status: &str,
    actor: &str,
) -> Result<YoutubeSubscriptionStatusChangeReceipt> {
    let id = id.trim();
    if id.is_empty() {
        return Err(EngineError::InstallFailed(
            "subscription id is required".to_string(),
        ));
    }
    let normalized_status = status.trim().to_ascii_lowercase();
    if !matches!(
        normalized_status.as_str(),
        YOUTUBE_SUBSCRIPTION_STATUS_NORMAL | YOUTUBE_SUBSCRIPTION_STATUS_DELETED
    ) {
        return Err(EngineError::InstallFailed(
            "manual subscription status must be normal or deleted".to_string(),
        ));
    }
    let normalized_actor = actor.trim().to_ascii_lowercase();
    if !matches!(
        normalized_actor.as_str(),
        YOUTUBE_SUBSCRIPTION_STATUS_ACTOR_OPERATOR | YOUTUBE_SUBSCRIPTION_STATUS_ACTOR_ASSISTANT
    ) {
        return Err(EngineError::InstallFailed(
            "manual subscription status actor must be operator or assistant".to_string(),
        ));
    }

    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    let now = now_ms();
    let changed = conn.execute(
        r#"
UPDATE youtube_subscription
SET
  source_status = ?1,
  source_status_changed_at_ms = ?2,
  source_status_change_source = ?3,
  active = CASE WHEN ?1 = 'deleted' THEN 0 ELSE 1 END,
  consecutive_failures = CASE WHEN ?1 = 'normal' THEN 0 ELSE consecutive_failures END,
  last_error_at_ms = CASE WHEN ?1 = 'normal' THEN NULL ELSE last_error_at_ms END,
  last_error_message = CASE WHEN ?1 = 'normal' THEN NULL ELSE last_error_message END,
  next_allowed_refresh_at_ms = CASE WHEN ?1 = 'normal' THEN NULL ELSE next_allowed_refresh_at_ms END,
  updated_at_ms = ?2
WHERE id = ?4
"#,
        params![normalized_status, now, normalized_actor, id],
    )?;
    if changed == 0 {
        return Err(EngineError::InstallFailed(format!(
            "subscription not found: {id}"
        )));
    }
    drop(conn);

    let canceled_refresh_jobs = if normalized_status == YOUTUBE_SUBSCRIPTION_STATUS_DELETED {
        jobs::cancel_youtube_subscription_refresh_jobs(paths, id)?
    } else {
        0
    };
    let subscription = get_youtube_subscription_by_id(paths, id)?.ok_or_else(|| {
        EngineError::InstallFailed(format!("subscription not found after status update: {id}"))
    })?;
    Ok(YoutubeSubscriptionStatusChangeReceipt {
        subscription,
        canceled_refresh_jobs,
    })
}

pub fn set_youtube_subscription_library(
    paths: &AppPaths,
    id: &str,
    library_id: Option<&str>,
) -> Result<YoutubeSubscriptionRow> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    let normalized_library_id = library_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    if let Some(target_id) = normalized_library_id.as_deref() {
        let exists: Option<i64> = conn
            .query_row(
                "SELECT active FROM video_library WHERE id = ?1",
                params![target_id],
                |row| row.get(0),
            )
            .optional()?;
        if exists != Some(1) {
            return Err(EngineError::InstallFailed(format!(
                "video library not found or disabled: {target_id}"
            )));
        }
    }
    let changed = conn.execute(
        "UPDATE youtube_subscription SET library_id = ?1, updated_at_ms = ?2 WHERE id = ?3",
        params![normalized_library_id, now_ms(), id],
    )?;
    if changed == 0 {
        return Err(EngineError::InstallFailed(format!(
            "subscription not found: {id}"
        )));
    }
    let row = subscription_by_id_conn(&conn, id)?.ok_or_else(|| {
        EngineError::InstallFailed(format!("subscription not found after update: {id}"))
    })?;
    let mut hydrated = hydrate_group_ids(&conn, vec![row])?;
    let mut row = hydrated.pop().expect("one subscription row");
    row.auth_session_configured = youtube_subscription_has_auth_session(paths, &row.id);
    Ok(row)
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
    ensure_subscription_is_not_deleted(&sub)?;
    drop(conn);
    queue_subscription_internal(paths, &sub, Some(Uuid::new_v4().to_string()))
}

pub fn queue_all_active_youtube_subscriptions(paths: &AppPaths) -> Result<Vec<jobs::JobRow>> {
    queue_active_youtube_subscriptions(paths, false)
}

// WP-0254: "Update all subscriptions" — refresh every active subscription now, ignoring
// the per-subscription refresh-interval (due) gate. Failure backoff is still honored so a
// repeatedly-failing subscription is not hammered. Feeds the conservative recurring lane.
pub fn queue_all_active_youtube_subscriptions_now(paths: &AppPaths) -> Result<Vec<jobs::JobRow>> {
    queue_active_youtube_subscriptions(paths, true)
}

fn queue_active_youtube_subscriptions(paths: &AppPaths, force: bool) -> Result<Vec<jobs::JobRow>> {
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
  library_id,
  browser_cookie_source,
  use_browser_cookies,
  active,
  preset_id,
  refresh_interval_minutes,
  last_queued_at_ms,
  last_error_at_ms,
  consecutive_failures,
  next_allowed_refresh_at_ms,
  created_at_ms,
  updated_at_ms,
  source_status,
  source_status_changed_at_ms,
  source_status_change_source
FROM youtube_subscription
WHERE active = 1 AND source_status <> 'deleted'
ORDER BY COALESCE(last_queued_at_ms, 0) ASC, updated_at_ms DESC
"#,
    )?;
    let rows = stmt
        .query_map([], row_to_subscription)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    drop(conn);

    let now = now_ms();
    let batch_id = Some(Uuid::new_v4().to_string());
    // WP-0257 (#4): trickle. "Update all" (force) enqueues at most `update_all_batch_size`
    // subscriptions per invocation (most-overdue first, via the ORDER BY above) so it doesn't
    // flood the queue; the recurring-lane cooldown then paces their dispatch. The due-path
    // (startup auto-sync) is uncapped because the cooldown already paces it.
    let cap = if force {
        jobs::youtube_update_all_effective_batch_size(paths)
    } else {
        usize::MAX
    };
    // Preserve the most-overdue selection boundary before source priority. Otherwise a forced
    // 25-subscription tranche would continually choose newer feed rows and starve an overdue
    // playlist. Once this cohort is fixed, feed pages go first and claim shared canonical IDs.
    let mut selected_subscriptions: Vec<YoutubeSubscriptionRow> = Vec::new();
    for sub in rows {
        if selected_subscriptions.len() >= cap {
            break;
        }
        if !force && !is_subscription_due(&sub, now) {
            continue;
        }
        if !is_subscription_backoff_ready(&sub, now) {
            continue;
        }
        selected_subscriptions.push(sub);
    }
    selected_subscriptions.sort_by_key(|sub| subscription_refresh_source_priority(&sub.source_url));

    let mut all_jobs: Vec<jobs::JobRow> = Vec::new();
    for sub in selected_subscriptions {
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
    // WP-0224: read-only connection bypasses the job-runner write queue.
    let conn = db::open_readonly(paths)?;
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

pub fn clear_youtube_subscription_group_memberships(paths: &AppPaths) -> Result<usize> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    let removed = conn.execute("DELETE FROM youtube_subscription_group_member", [])?;
    Ok(removed)
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
  sub.library_id,
  sub.browser_cookie_source,
  sub.use_browser_cookies,
  sub.active,
  sub.preset_id,
  sub.refresh_interval_minutes,
  sub.last_queued_at_ms,
  sub.last_error_at_ms,
  sub.consecutive_failures,
  sub.next_allowed_refresh_at_ms,
  sub.created_at_ms,
  sub.updated_at_ms,
  sub.source_status,
  sub.source_status_changed_at_ms,
  sub.source_status_change_source
FROM youtube_subscription sub
JOIN youtube_subscription_group_member gm ON gm.subscription_id = sub.id
WHERE gm.group_id = ?1 AND sub.active = 1 AND sub.source_status <> 'deleted'
ORDER BY sub.updated_at_ms DESC, sub.created_at_ms DESC
"#,
    )?;
    let mut rows = stmt
        .query_map(params![group_id], row_to_subscription)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    drop(conn);

    rows.sort_by_key(|sub| subscription_refresh_source_priority(&sub.source_url));

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
  -- WP-0264: clear the persisted failure text so a recovered subscription shows NO state
  -- in the panel (the FE keys the state chip off consecutive_failures>0 + last_error_message).
  last_error_message = NULL,
  next_allowed_refresh_at_ms = NULL,
  source_status = CASE
    WHEN source_status = 'unavailable' THEN 'normal'
    ELSE source_status
  END,
  source_status_changed_at_ms = CASE
    WHEN source_status = 'unavailable' THEN ?1
    ELSE source_status_changed_at_ms
  END,
  source_status_change_source = CASE
    WHEN source_status = 'unavailable' THEN 'refresh_success'
    ELSE source_status_change_source
  END,
  last_checked_at_ms = ?1,
  updated_at_ms = ?1
WHERE id = ?2
"#,
        params![now_ms(), subscription_id],
    )?;
    Ok(())
}

// WP-0264: max stored length for the raw error text. ~500 chars is enough for the FE
// `classifyFailure` to match the decisive HTTP status / phrase, while bounding the row size
// (yt-dlp tracebacks can be multi-KB). Truncation is on a char boundary, not a byte boundary.
const MAX_LAST_ERROR_MESSAGE_CHARS: usize = 500;

fn truncate_error_message(error_message: Option<&str>) -> Option<String> {
    error_message.map(|raw| {
        if raw.chars().count() > MAX_LAST_ERROR_MESSAGE_CHARS {
            raw.chars().take(MAX_LAST_ERROR_MESSAGE_CHARS).collect()
        } else {
            raw.to_string()
        }
    })
}

fn is_confirmed_http_404_refresh_error(error_message: Option<&str>) -> bool {
    let Some(raw) = error_message else {
        return false;
    };
    let lower = raw.to_ascii_lowercase();
    lower.contains("http error 404")
        || lower.contains("http response error 404")
        || lower.contains("404: not found")
        || lower.contains("status code 404")
        || lower.contains("status=404")
        || lower.contains("status: 404")
}

/// Back-compat shim (pre-WP-0264 signature). Callers that do not yet have the error text
/// route here and persist a NULL `last_error_message`. Prefer
/// [`record_subscription_refresh_failure_with_error`] so the subscription panel can classify
/// the failure state (WP-0264).
pub fn record_subscription_refresh_failure(paths: &AppPaths, subscription_id: &str) -> Result<()> {
    record_subscription_refresh_failure_with_error(paths, subscription_id, None)
}

/// WP-0264: record a failed refresh AND persist the (truncated) raw error text so the FE can
/// classify the failure into a state + required action. Increments `consecutive_failures`,
/// stamps `last_error_at_ms`, applies the existing exponential backoff, and writes the
/// truncated error into `last_error_message` (NULL when `error_message` is None).
pub fn record_subscription_refresh_failure_with_error(
    paths: &AppPaths,
    subscription_id: &str,
    error_message: Option<&str>,
) -> Result<()> {
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

    let truncated_error = truncate_error_message(error_message);
    let confirmed_http_404 = is_confirmed_http_404_refresh_error(error_message);

    conn.execute(
        r#"
UPDATE youtube_subscription
SET
  consecutive_failures = ?1,
  last_error_at_ms = ?2,
  last_error_message = ?5,
  next_allowed_refresh_at_ms = ?3,
  last_checked_at_ms = ?2,
  source_status = CASE
    WHEN ?6 = 1 AND source_status <> 'deleted' THEN 'unavailable'
    ELSE source_status
  END,
  source_status_changed_at_ms = CASE
    WHEN ?6 = 1 AND source_status <> 'deleted' THEN ?2
    ELSE source_status_changed_at_ms
  END,
  source_status_change_source = CASE
    WHEN ?6 = 1 AND source_status <> 'deleted' THEN 'refresh_404'
    ELSE source_status_change_source
  END,
  updated_at_ms = ?2
WHERE id = ?4
"#,
        params![
            next_failures,
            now,
            now.saturating_add(delay_ms),
            subscription_id,
            truncated_error,
            bool_to_i64(confirmed_http_404),
        ],
    )?;
    Ok(())
}

/// WP-0255: persist the per-subscription progress counts the refresh job already computes
/// (upstream playlist/channel length, new videos found, downloads enqueued). Schema v18.
/// Does NOT bump `updated_at_ms` — the success/failure recorders own the "last checked"
/// timestamp and list ordering; this only fills the count columns the UI shows as "X of Y".
pub fn record_subscription_refresh_counts(
    paths: &AppPaths,
    subscription_id: &str,
    upstream_total: i64,
    new_found: i64,
    queued: i64,
) -> Result<()> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    conn.execute(
        r#"
UPDATE youtube_subscription
SET
  upstream_total = ?1,
  last_new_found = ?2,
  last_refresh_queued = ?3
WHERE id = ?4
"#,
        params![upstream_total, new_found, queued, subscription_id],
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
        let (appended, skipped_existing) = merge_archive_file(paths, &archive_path, &inferred_ids)?;
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
                browser_cookie_source: row.browser_cookie_source.clone(),
                active: row.active,
                source_status: row.source_status.clone(),
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
    if !(MIN_SUPPORTED_EXPORT_SCHEMA_VERSION..=EXPORT_SCHEMA_VERSION)
        .contains(&payload.schema_version)
    {
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
        let imported_source_status = normalize_imported_source_status(&raw.source_status)?;
        let normalized = normalize_upsert(YoutubeSubscriptionUpsert {
            id: None,
            title: raw.title.clone(),
            source_url: raw.source_url.clone(),
            folder_map: raw.folder_map.clone(),
            output_dir_override: raw.output_dir_override.clone(),
            library_id: None,
            use_browser_cookies: raw.use_browser_cookies,
            browser_cookie_source: raw.browser_cookie_source.clone(),
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
  library_id,
  browser_cookie_source,
  use_browser_cookies,
  active,
  source_status,
  source_status_changed_at_ms,
  source_status_change_source,
  preset_id,
  refresh_interval_minutes,
  last_queued_at_ms,
  last_error_at_ms,
  consecutive_failures,
  next_allowed_refresh_at_ms,
  created_at_ms,
  updated_at_ms
) VALUES (
  ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8,
  CASE WHEN ?10 = 'deleted' THEN 0 ELSE ?9 END,
  ?10,
  CASE WHEN ?10 = 'normal' THEN NULL ELSE ?13 END,
  CASE WHEN ?10 = 'normal' THEN NULL ELSE 'operator_import' END,
  ?11, ?12, NULL, NULL, 0, NULL, ?13, ?13
)
ON CONFLICT(source_url) DO UPDATE SET
  title = excluded.title,
  folder_map = excluded.folder_map,
  output_dir_override = excluded.output_dir_override,
  library_id = COALESCE(excluded.library_id, youtube_subscription.library_id),
  browser_cookie_source = excluded.browser_cookie_source,
  use_browser_cookies = excluded.use_browser_cookies,
  active = CASE
    WHEN youtube_subscription.source_status = 'deleted' OR excluded.source_status = 'deleted'
      THEN 0
    ELSE excluded.active
  END,
  source_status = CASE
    WHEN youtube_subscription.source_status = 'deleted' OR excluded.source_status = 'deleted'
      THEN 'deleted'
    WHEN excluded.source_status = 'unavailable' THEN 'unavailable'
    ELSE youtube_subscription.source_status
  END,
  source_status_changed_at_ms = CASE
    WHEN excluded.source_status <> 'normal'
      AND youtube_subscription.source_status <> excluded.source_status
      THEN excluded.source_status_changed_at_ms
    ELSE youtube_subscription.source_status_changed_at_ms
  END,
  source_status_change_source = CASE
    WHEN excluded.source_status <> 'normal'
      AND youtube_subscription.source_status <> excluded.source_status
      THEN excluded.source_status_change_source
    ELSE youtube_subscription.source_status_change_source
  END,
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
                normalized.library_id,
                normalized.browser_cookie_source,
                bool_to_i64(normalized.use_browser_cookies),
                bool_to_i64(normalized.active),
                imported_source_status,
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

fn normalize_imported_source_status(value: &str) -> Result<String> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        YOUTUBE_SUBSCRIPTION_STATUS_NORMAL
        | YOUTUBE_SUBSCRIPTION_STATUS_UNAVAILABLE
        | YOUTUBE_SUBSCRIPTION_STATUS_DELETED => Ok(normalized),
        _ => Err(EngineError::InstallFailed(format!(
            "unsupported subscription source_status in import: {value}"
        ))),
    }
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

#[derive(Debug, Clone)]
struct FourkvdDownloadEvidence {
    record_key: String,
    filename: String,
    source_url: String,
    media_id: String,
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
            library_id: None,
            use_browser_cookies: false,
            browser_cookie_source: None,
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
  library_id,
  browser_cookie_source,
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
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, NULL, NULL, 0, NULL, ?12, ?12)
ON CONFLICT(source_url) DO UPDATE SET
  title = excluded.title,
  folder_map = excluded.folder_map,
  output_dir_override = excluded.output_dir_override,
  library_id = COALESCE(excluded.library_id, youtube_subscription.library_id),
  browser_cookie_source = excluded.browser_cookie_source,
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
                normalized.library_id,
                normalized.browser_cookie_source,
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
        _source_memberships_added,
    ) = if entries_path.exists() {
        seed_archives_from_4kvdp_entries(paths, &conn, &fourk_id_to_source_url, &entries_path)?
    } else {
        (0, 0, 0, 0, 0)
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
            library_id: None,
            use_browser_cookies: false,
            browser_cookie_source: None,
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
  library_id,
  browser_cookie_source,
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
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, NULL, NULL, 0, NULL, ?12, ?12)
ON CONFLICT(source_url) DO UPDATE SET
  title = excluded.title,
  folder_map = excluded.folder_map,
  output_dir_override = excluded.output_dir_override,
  library_id = COALESCE(excluded.library_id, youtube_subscription.library_id),
  browser_cookie_source = excluded.browser_cookie_source,
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
                normalized.library_id,
                normalized.browser_cookie_source,
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
        source_memberships_added,
    ) = seed_archives_from_4kvdp_state_entries(
        paths,
        &conn,
        &legacy_conn,
        &fourk_id_to_source_url,
    )?;
    let identity_summary = enrich_imported_youtube_identity_4kvdp_conn(
        &conn,
        &legacy_conn,
        &sqlite_path,
        false,
        None,
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
        identity_scanned_items: identity_summary.scanned_items,
        identity_exact_items: identity_summary.exact_items,
        identity_ambiguous_items: identity_summary.ambiguous_items,
        identity_unresolved_items: identity_summary.unresolved_items,
        identity_conflict_items: identity_summary.conflict_items,
        source_memberships_added,
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
) -> Result<(usize, usize, usize, usize, usize)> {
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
    let mut memberships_added = 0_usize;
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

        let merged = merge_archive_file(paths, &archive_path, &ids);
        if merged.is_err() {
            failures += 1;
            continue;
        }
        for media_id in &ids {
            memberships_added +=
                upsert_imported_source_membership(conn, media_id, &sub, "4kvdp_export_entry")?;
        }
        seeded_subs += 1;
    }

    Ok((
        seeded_subs,
        seeded_entries,
        skipped_entries,
        failures,
        memberships_added,
    ))
}

fn seed_archives_from_4kvdp_state_entries(
    paths: &AppPaths,
    conn: &rusqlite::Connection,
    legacy_conn: &rusqlite::Connection,
    fourk_id_to_source_url: &HashMap<i64, String>,
) -> Result<(usize, usize, usize, usize, usize)> {
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
    let mut memberships_added = 0_usize;
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

        let merged = merge_archive_file(paths, &archive_path, &ids);
        if merged.is_err() {
            failures += 1;
            continue;
        }
        for media_id in &ids {
            memberships_added += upsert_imported_source_membership(
                conn,
                media_id,
                &sub,
                "4kvdp_subscription_entry",
            )?;
        }
        seeded_subs += 1;
    }

    Ok((
        seeded_subs,
        seeded_entries,
        skipped_entries,
        failures,
        memberships_added,
    ))
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

pub fn enrich_imported_youtube_identity_4kvdp(
    paths: &AppPaths,
    sqlite_path: Option<&Path>,
    dry_run: bool,
    max_items: Option<usize>,
) -> Result<YoutubeImportedIdentityEnrichmentSummary> {
    let sqlite_path = resolve_legacy_4kvdp_state_db_path(sqlite_path).ok_or_else(|| {
        EngineError::InstallFailed(
            "4K Video Downloader app-state database not found; provide a valid SQLite path"
                .to_string(),
        )
    })?;
    let legacy_conn = open_legacy_4kvdp_state_db(&sqlite_path)?;
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    enrich_imported_youtube_identity_4kvdp_conn(
        &conn,
        &legacy_conn,
        &sqlite_path,
        dry_run,
        max_items,
    )
}

fn enrich_imported_youtube_identity_4kvdp_conn(
    conn: &rusqlite::Connection,
    legacy_conn: &rusqlite::Connection,
    sqlite_path: &Path,
    dry_run: bool,
    max_items: Option<usize>,
) -> Result<YoutubeImportedIdentityEnrichmentSummary> {
    const BATCH_SIZE: i64 = 500;

    let source_schema_supported = has_4kvdp_download_evidence_schema(legacy_conn)?;
    let download_evidence = read_4kvdp_download_evidence(legacy_conn)?;
    let source_download_evidence_rows = download_evidence.len();
    let mut evidence_by_path: HashMap<String, Vec<FourkvdDownloadEvidence>> = HashMap::new();
    for evidence in download_evidence {
        evidence_by_path
            .entry(normalize_import_match_path(&evidence.filename))
            .or_default()
            .push(evidence);
    }

    let metadata = std::fs::metadata(sqlite_path)?;
    let source_size = i64::try_from(metadata.len()).unwrap_or(i64::MAX);
    let source_modified_ms = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|value| i64::try_from(value.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0);
    let source_path = sqlite_path.to_string_lossy().to_string();

    let mut summary = YoutubeImportedIdentityEnrichmentSummary {
        sqlite_path: source_path.clone(),
        dry_run,
        source_schema_supported,
        source_download_evidence_rows,
        ..YoutubeImportedIdentityEnrichmentSummary::default()
    };
    let mut scanned_this_call = 0_usize;
    let mut cursor = String::new();
    if !dry_run {
        if let Some(checkpoint) = conn
            .query_row(
                r#"
SELECT source_size, source_modified_ms, last_library_item_id, status,
       scanned_items, exact_items, ambiguous_items, unresolved_items, conflict_items
FROM media_import_enrichment_checkpoint
WHERE source_path=?1
"#,
                [&source_path],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, i64>(7)?,
                        row.get::<_, i64>(8)?,
                    ))
                },
            )
            .optional()?
        {
            if checkpoint.0 == source_size
                && checkpoint.1 == source_modified_ms
                && matches!(checkpoint.3.as_str(), "in_progress" | "paused")
            {
                cursor = checkpoint.2.unwrap_or_default();
                summary.scanned_items = usize::try_from(checkpoint.4).unwrap_or(0);
                summary.exact_items = usize::try_from(checkpoint.5).unwrap_or(0);
                summary.ambiguous_items = usize::try_from(checkpoint.6).unwrap_or(0);
                summary.unresolved_items = usize::try_from(checkpoint.7).unwrap_or(0);
                summary.conflict_items = usize::try_from(checkpoint.8).unwrap_or(0);
            } else {
                conn.execute(
                    "DELETE FROM media_import_enrichment_checkpoint WHERE source_path=?1",
                    [&source_path],
                )?;
            }
        }
        upsert_import_enrichment_checkpoint(
            conn,
            &source_path,
            source_size,
            source_modified_ms,
            if cursor.is_empty() {
                None
            } else {
                Some(&cursor)
            },
            "in_progress",
            &summary,
        )?;
    }

    loop {
        let query_limit = max_items
            .map(|limit| {
                limit
                    .max(1)
                    .saturating_sub(scanned_this_call)
                    .min(BATCH_SIZE as usize)
            })
            .unwrap_or(BATCH_SIZE as usize);
        if query_limit == 0 {
            summary.complete = false;
            summary.next_cursor = if cursor.is_empty() {
                None
            } else {
                Some(cursor.clone())
            };
            return Ok(summary);
        }
        let items = {
            let mut stmt = conn.prepare(
                r#"
SELECT id, media_path
FROM library_item
WHERE origin='4kvdp_import' AND id > ?1
ORDER BY id ASC
LIMIT ?2
"#,
            )?;
            let rows = stmt
                .query_map(params![&cursor, query_limit as i64], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            rows
        };
        if items.is_empty() {
            break;
        }

        let tx = if dry_run {
            None
        } else {
            Some(conn.unchecked_transaction()?)
        };
        let write_conn: &rusqlite::Connection = tx.as_ref().map(|value| &**value).unwrap_or(conn);

        for (item_id, media_path) in &items {
            summary.scanned_items += 1;
            scanned_this_call += 1;
            let normalized_path = normalize_import_match_path(media_path);
            let candidates = evidence_by_path
                .get(&normalized_path)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let mut by_media_id: HashMap<&str, &FourkvdDownloadEvidence> = HashMap::new();
            for candidate in candidates {
                by_media_id
                    .entry(candidate.media_id.as_str())
                    .or_insert(candidate);
            }

            match by_media_id.len() {
                0 => {
                    summary.unresolved_items += 1;
                    if !dry_run {
                        summary.evidence_rows_written += upsert_import_evidence(
                            write_conn,
                            item_id,
                            None,
                            "4kvdp_exact_path",
                            &format!("library_item:{item_id}"),
                            Some(media_path),
                            None,
                            "unresolved",
                            r#"{"reason":"no_exact_download_path"}"#,
                        )?;
                    }
                }
                1 => {
                    let evidence = *by_media_id.values().next().expect("single evidence");
                    let mut match_state = "exact";
                    let linked_item = write_conn
                        .query_row(
                            "SELECT library_item_id FROM media_source_identity WHERE service='youtube' AND media_id=?1",
                            [&evidence.media_id],
                            |row| row.get::<_, Option<String>>(0),
                        )
                        .optional()?
                        .flatten();
                    match linked_item {
                        Some(existing) if existing != *item_id => {
                            summary.conflict_items += 1;
                            match_state = "conflict";
                        }
                        Some(_) => {
                            summary.already_linked_items += 1;
                        }
                        None if !dry_run => {
                            ensure_imported_media_identity(
                                write_conn,
                                &evidence.media_id,
                                &evidence.source_url,
                            )?;
                            write_conn.execute(
                                "UPDATE media_source_identity SET library_item_id=?1, repair_state='ready', updated_at_ms=?2 WHERE service='youtube' AND media_id=?3 AND library_item_id IS NULL",
                                params![item_id, now_ms(), &evidence.media_id],
                            )?;
                            write_conn.execute(
                                r#"
INSERT INTO ingest_provenance (
  item_id, provider, source_url, rights_note, attested_at_ms, created_at_ms
) VALUES (?1, '4kvdp_import', ?2, 'Imported from read-only 4K Video Downloader evidence.', ?3, ?3)
ON CONFLICT(item_id) DO NOTHING
"#,
                                params![item_id, &evidence.source_url, now_ms()],
                            )?;
                        }
                        None => {}
                    }
                    if match_state == "exact" {
                        summary.exact_items += 1;
                    }
                    if !dry_run {
                        for candidate in candidates {
                            summary.evidence_rows_written += upsert_import_evidence(
                                write_conn,
                                item_id,
                                Some(&candidate.media_id),
                                "4kvdp_exact_path",
                                &candidate.record_key,
                                Some(&candidate.filename),
                                Some(&candidate.source_url),
                                match_state,
                                r#"{"path_match":"normalized_exact"}"#,
                            )?;
                        }
                    }
                }
                _ => {
                    summary.ambiguous_items += 1;
                    if !dry_run {
                        for candidate in candidates {
                            summary.evidence_rows_written += upsert_import_evidence(
                                write_conn,
                                item_id,
                                Some(&candidate.media_id),
                                "4kvdp_exact_path",
                                &candidate.record_key,
                                Some(&candidate.filename),
                                Some(&candidate.source_url),
                                "ambiguous",
                                r#"{"reason":"multiple_media_ids_for_exact_path"}"#,
                            )?;
                        }
                    }
                }
            }
        }

        cursor = items.last().map(|row| row.0.clone()).unwrap_or(cursor);
        if let Some(tx) = tx {
            upsert_import_enrichment_checkpoint(
                &tx,
                &source_path,
                source_size,
                source_modified_ms,
                Some(&cursor),
                "in_progress",
                &summary,
            )?;
            tx.commit()?;
        }
        if max_items
            .map(|limit| scanned_this_call >= limit.max(1))
            .unwrap_or(false)
        {
            summary.complete = false;
            summary.next_cursor = Some(cursor.clone());
            if !dry_run {
                upsert_import_enrichment_checkpoint(
                    conn,
                    &source_path,
                    source_size,
                    source_modified_ms,
                    Some(&cursor),
                    "paused",
                    &summary,
                )?;
            }
            return Ok(summary);
        }
    }

    summary.complete = true;
    summary.next_cursor = None;
    if !dry_run {
        upsert_import_enrichment_checkpoint(
            conn,
            &source_path,
            source_size,
            source_modified_ms,
            None,
            "complete",
            &summary,
        )?;
    }
    Ok(summary)
}

fn read_4kvdp_download_evidence(
    legacy_conn: &rusqlite::Connection,
) -> Result<Vec<FourkvdDownloadEvidence>> {
    if !has_4kvdp_download_evidence_schema(legacy_conn)? {
        return Ok(Vec::new());
    }
    let mut stmt = legacy_conn.prepare(
        r#"
SELECT d.id, m.id, u.id, COALESCE(d.filename, ''), COALESCE(u.url, '')
FROM download_item d
JOIN media_item_description m ON m.download_item_id=d.id
JOIN url_description u ON u.media_item_description_id=m.id
WHERE lower(COALESCE(u.service_name, ''))='youtube'
ORDER BY d.id ASC, m.id ASC, u.id ASC
"#,
    )?;
    let rows = stmt
        .query_map([], |row| {
            let download_id: i64 = row.get(0)?;
            let description_id: i64 = row.get(1)?;
            let url_id: i64 = row.get(2)?;
            let filename: String = row.get(3)?;
            let source_url: String = row.get(4)?;
            Ok((
                format!("{download_id}:{description_id}:{url_id}"),
                filename,
                source_url,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(rows
        .into_iter()
        .filter_map(|(record_key, filename, source_url)| {
            if filename.trim().is_empty() {
                return None;
            }
            let media_id = youtube_video_id_from_url(&source_url)?;
            Some(FourkvdDownloadEvidence {
                record_key,
                filename,
                source_url,
                media_id,
            })
        })
        .collect())
}

fn has_4kvdp_download_evidence_schema(legacy_conn: &rusqlite::Connection) -> Result<bool> {
    for table in ["download_item", "media_item_description", "url_description"] {
        let exists = legacy_conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
            [table],
            |row| row.get::<_, i64>(0),
        )? != 0;
        if !exists {
            return Ok(false);
        }
    }
    Ok(true)
}

fn ensure_imported_media_identity(
    conn: &rusqlite::Connection,
    media_id: &str,
    source_url: &str,
) -> Result<()> {
    let canonical_url = if source_url.trim().is_empty() {
        format!("https://www.youtube.com/watch?v={media_id}")
    } else {
        source_url.trim().to_string()
    };
    let now = now_ms();
    conn.execute(
        r#"
INSERT INTO media_source_identity (
  service, media_id, canonical_url, library_item_id, active_job_id, repair_state,
  last_failed_url, last_error, created_at_ms, updated_at_ms
) VALUES ('youtube', ?1, ?2, NULL, NULL, 'imported_unresolved', NULL, NULL, ?3, ?3)
ON CONFLICT(service, media_id) DO UPDATE SET
  canonical_url=CASE
    WHEN media_source_identity.canonical_url='' THEN excluded.canonical_url
    ELSE media_source_identity.canonical_url
  END,
  updated_at_ms=excluded.updated_at_ms
"#,
        params![media_id, canonical_url, now],
    )?;
    conn.execute(
        "INSERT OR IGNORE INTO media_source_alias (service, media_id, source_url, created_at_ms) VALUES ('youtube', ?1, ?2, ?3)",
        params![media_id, canonical_url, now],
    )?;
    Ok(())
}

fn upsert_imported_source_membership(
    conn: &rusqlite::Connection,
    media_id: &str,
    sub: &YoutubeSubscriptionRow,
    evidence_kind: &str,
) -> Result<usize> {
    let canonical_url = format!("https://www.youtube.com/watch?v={media_id}");
    ensure_imported_media_identity(conn, media_id, &canonical_url)?;
    let source_kind = youtube_source_membership_kind(&sub.source_url);
    let now = now_ms();
    let inserted = conn.execute(
        r#"
INSERT OR IGNORE INTO media_source_membership (
  service, media_id, source_subscription_id, source_kind, source_url_snapshot,
  source_title_snapshot, evidence_kind, created_at_ms, updated_at_ms
) VALUES ('youtube', ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
"#,
        params![
            media_id,
            &sub.id,
            source_kind,
            &sub.source_url,
            &sub.title,
            evidence_kind,
            now
        ],
    )?;
    if inserted == 0 {
        conn.execute(
            r#"
UPDATE media_source_membership SET
  source_kind=?1,
  source_url_snapshot=?2,
  source_title_snapshot=?3,
  evidence_kind=?4,
  updated_at_ms=?5
WHERE service='youtube' AND media_id=?6 AND source_subscription_id=?7
"#,
            params![
                source_kind,
                &sub.source_url,
                &sub.title,
                evidence_kind,
                now,
                media_id,
                &sub.id
            ],
        )?;
    }
    conn.execute(
        "INSERT OR IGNORE INTO media_source_association (id, service, media_id, origin_kind, source_subscription_id, source_job_id, created_at_ms) VALUES (?1, 'youtube', ?2, ?3, ?4, NULL, ?5)",
        params![Uuid::new_v4().to_string(), media_id, source_kind, &sub.id, now],
    )?;
    Ok(inserted)
}

fn youtube_source_membership_kind(source_url: &str) -> &'static str {
    let lower = source_url.trim().to_ascii_lowercase();
    if lower.contains("/playlist") || lower.contains("list=") {
        "playlist"
    } else if lower.trim_end_matches('/').ends_with("/shorts") {
        "shorts_page"
    } else if lower.trim_end_matches('/').ends_with("/videos") {
        "videos_page"
    } else if youtube_video_id_from_url(source_url).is_some() {
        "direct_video"
    } else {
        "channel_page"
    }
}

/// Channel, `/videos`, and `/shorts` sources are the preferred discovery paths for a refresh
/// cohort. A playlist still records its membership and can recover a video if no canonical item
/// is present or actively claimed; this ranking only avoids its becoming the first owner.
fn subscription_refresh_source_priority(source_url: &str) -> u8 {
    match youtube_source_membership_kind(source_url) {
        "playlist" => 1,
        _ => 0,
    }
}

fn normalize_import_match_path(path: &str) -> String {
    let mut value = path.trim().replace('/', "\\");
    if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
        value = format!(r"\\{rest}");
    } else if let Some(rest) = value.strip_prefix(r"\\?\") {
        value = rest.to_string();
    }
    while value.ends_with('\\') && value.len() > 3 {
        value.pop();
    }
    value.to_ascii_lowercase()
}

fn upsert_import_evidence(
    conn: &rusqlite::Connection,
    library_item_id: &str,
    media_id: Option<&str>,
    evidence_kind: &str,
    source_record_key: &str,
    source_path: Option<&str>,
    source_url: Option<&str>,
    match_state: &str,
    details_json: &str,
) -> Result<usize> {
    let now = now_ms();
    conn.execute(
        r#"
INSERT INTO media_import_evidence (
  id, library_item_id, service, media_id, evidence_kind, source_record_key,
  source_path_snapshot, source_url_snapshot, match_state, details_json,
  created_at_ms, updated_at_ms
) VALUES (?1, ?2, 'youtube', ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)
ON CONFLICT DO UPDATE SET
  source_path_snapshot=excluded.source_path_snapshot,
  source_url_snapshot=excluded.source_url_snapshot,
  match_state=excluded.match_state,
  details_json=excluded.details_json,
  updated_at_ms=excluded.updated_at_ms
"#,
        params![
            Uuid::new_v4().to_string(),
            library_item_id,
            media_id,
            evidence_kind,
            source_record_key,
            source_path,
            source_url,
            match_state,
            details_json,
            now
        ],
    )
    .map_err(Into::into)
}

fn upsert_import_enrichment_checkpoint(
    conn: &rusqlite::Connection,
    source_path: &str,
    source_size: i64,
    source_modified_ms: i64,
    last_library_item_id: Option<&str>,
    status: &str,
    summary: &YoutubeImportedIdentityEnrichmentSummary,
) -> Result<()> {
    conn.execute(
        r#"
INSERT INTO media_import_enrichment_checkpoint (
  source_path, source_size, source_modified_ms, last_library_item_id, status,
  scanned_items, exact_items, ambiguous_items, unresolved_items, conflict_items, updated_at_ms
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
ON CONFLICT(source_path) DO UPDATE SET
  source_size=excluded.source_size,
  source_modified_ms=excluded.source_modified_ms,
  last_library_item_id=excluded.last_library_item_id,
  status=excluded.status,
  scanned_items=excluded.scanned_items,
  exact_items=excluded.exact_items,
  ambiguous_items=excluded.ambiguous_items,
  unresolved_items=excluded.unresolved_items,
  conflict_items=excluded.conflict_items,
  updated_at_ms=excluded.updated_at_ms
"#,
        params![
            source_path,
            source_size,
            source_modified_ms,
            last_library_item_id,
            status,
            i64::try_from(summary.scanned_items).unwrap_or(i64::MAX),
            i64::try_from(summary.exact_items).unwrap_or(i64::MAX),
            i64::try_from(summary.ambiguous_items).unwrap_or(i64::MAX),
            i64::try_from(summary.unresolved_items).unwrap_or(i64::MAX),
            i64::try_from(summary.conflict_items).unwrap_or(i64::MAX),
            now_ms()
        ],
    )?;
    Ok(())
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

fn merge_archive_file(
    paths: &AppPaths,
    path: &Path,
    video_ids: &HashSet<String>,
) -> Result<(usize, usize)> {
    let _guard = lock_youtube_archive_mutation()?;
    let subscription_id = archive_subscription_id_from_path(paths, path)?;
    reconcile_archive_merge_for_subscription_locked(paths, &subscription_id)?;
    merge_archive_file_locked(paths, path, video_ids)
}

fn lock_youtube_archive_mutation() -> Result<MutexGuard<'static, ()>> {
    YOUTUBE_ARCHIVE_MUTATION_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| EngineError::InstallFailed("youtube archive mutation lock is poisoned".into()))
}

fn archive_merge_journal_path(path: &Path) -> PathBuf {
    path.with_file_name(YT_DLP_ARCHIVE_MERGE_JOURNAL_FILENAME)
}

fn archive_subscription_id_from_path(paths: &AppPaths, path: &Path) -> Result<String> {
    let subscription_id = path
        .parent()
        .and_then(Path::file_name)
        .map(|value| value.to_string_lossy().to_string())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            EngineError::InstallFailed("canonical archive path has no subscription identity".into())
        })?;
    if paths.youtube_subscription_archive_state_path(&subscription_id) != path {
        return Err(EngineError::InstallFailed(
            "youtube archive merge target is not the canonical subscription archive".into(),
        ));
    }
    Ok(subscription_id)
}

fn canonical_archive_body(video_ids: &[String]) -> String {
    video_ids
        .iter()
        .map(|id| format!("youtube {id}\n"))
        .collect::<String>()
}

fn archive_body_sha256(body: &str) -> String {
    use sha2::{Digest as _, Sha256};
    format!("{:X}", Sha256::digest(body.as_bytes()))
}

fn archive_file_sha256(path: &Path) -> Result<String> {
    use sha2::{Digest as _, Sha256};
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(error.into()),
    };
    Ok(format!("{:X}", Sha256::digest(bytes)))
}

fn write_archive_merge_journal(path: &Path, intent_id: &str) -> Result<()> {
    let carrier = YoutubeArchiveMergeCarrier {
        schema_version: 2,
        intent_id: intent_id.to_string(),
    };
    persistence::atomic_write_text(path, &serde_json::to_string(&carrier)?)?;
    Ok(())
}

fn read_archive_merge_carrier(path: &Path) -> Result<YoutubeArchiveMergeCarrier> {
    let raw = std::fs::read_to_string(path)?;
    let carrier: YoutubeArchiveMergeCarrier = serde_json::from_str(&raw).map_err(|_| {
        EngineError::InstallFailed(
            "youtube archive merge carrier is legacy, unreadable, or untrusted".to_string(),
        )
    })?;
    if carrier.schema_version != 2
        || Uuid::parse_str(&carrier.intent_id)
            .map(|value| value.to_string() != carrier.intent_id)
            .unwrap_or(true)
    {
        return Err(EngineError::InstallFailed(
            "youtube archive merge carrier identity is invalid".to_string(),
        ));
    }
    Ok(carrier)
}

fn create_archive_merge_intent(
    paths: &AppPaths,
    subscription_id: &str,
    target_archive_path: &Path,
    source_archive_sha256: &str,
    intended_video_ids: &[String],
) -> Result<String> {
    let expected_target = paths.youtube_subscription_archive_state_path(subscription_id);
    let mut canonical_ids = intended_video_ids.to_vec();
    canonical_ids.sort();
    canonical_ids.dedup();
    if target_archive_path != expected_target
        || canonical_ids != intended_video_ids
        || intended_video_ids.iter().any(|id| {
            id.trim().is_empty() || id.trim() != id || id.chars().any(char::is_whitespace)
        })
    {
        return Err(EngineError::InstallFailed(
            "youtube archive merge intent target or members are invalid".into(),
        ));
    }
    let mut conn = db::open(paths)?;
    db::migrate(&conn)?;
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let subscription_exists: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM youtube_subscription WHERE id=?1)",
        [subscription_id],
        |row| row.get(0),
    )?;
    if !subscription_exists {
        return Err(EngineError::InstallFailed(format!(
            "youtube archive merge subscription does not exist: {subscription_id}"
        )));
    }
    let intent_id = Uuid::new_v4().to_string();
    let intended_body = canonical_archive_body(intended_video_ids);
    tx.execute(
        "INSERT INTO youtube_archive_merge_intent(
           intent_id,subscription_id,target_archive_path,source_archive_sha256,
           intended_archive_sha256,intended_video_ids_json,phase,created_at_ms,updated_at_ms
         ) VALUES(?1,?2,?3,?4,?5,?6,'prepared',?7,?7)",
        params![
            intent_id,
            subscription_id,
            target_archive_path.to_string_lossy(),
            source_archive_sha256,
            archive_body_sha256(&intended_body),
            serde_json::to_string(intended_video_ids)?,
            now_ms(),
        ],
    )?;
    tx.commit()?;
    Ok(intent_id)
}

fn load_pending_archive_merge_intent_rows(
    paths: &AppPaths,
) -> Result<Vec<YoutubeArchiveMergeIntentRow>> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    let mut stmt = conn.prepare(
        "SELECT intent.intent_id,intent.subscription_id,intent.target_archive_path,
                intent.source_archive_sha256,intent.intended_archive_sha256,
                intent.intended_video_ids_json,intent.phase
         FROM youtube_archive_merge_intent intent
         LEFT JOIN youtube_archive_merge_intent_failure failure
           ON failure.intent_id=intent.intent_id AND failure.resolved_at_ms IS NULL
         WHERE intent.phase IN ('prepared','published')
         ORDER BY CASE WHEN failure.intent_id IS NULL THEN 0 ELSE 1 END ASC,
                  COALESCE(failure.last_failed_at_ms,intent.created_at_ms) ASC,
                  intent.intent_id ASC
         LIMIT ?1",
    )?;
    let rows = stmt.query_map([YOUTUBE_ARCHIVE_JOURNAL_REPLAY_LIMIT as i64], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
        ))
    })?;
    let mut intents = Vec::new();
    for row in rows {
        let (
            intent_id,
            subscription_id,
            target_archive_path,
            source_archive_sha256,
            intended_archive_sha256,
            intended_video_ids_json,
            phase,
        ) = row?;
        intents.push(YoutubeArchiveMergeIntentRow {
            intent_id,
            subscription_id,
            target_archive_path,
            source_archive_sha256,
            intended_archive_sha256,
            intended_video_ids_json,
            phase,
        });
    }
    Ok(intents)
}

fn load_pending_archive_merge_intent_row_for_subscription(
    paths: &AppPaths,
    subscription_id: &str,
) -> Result<Option<YoutubeArchiveMergeIntentRow>> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    conn.query_row(
        "SELECT intent_id,subscription_id,target_archive_path,source_archive_sha256,
                intended_archive_sha256,intended_video_ids_json,phase
         FROM youtube_archive_merge_intent
         WHERE subscription_id=?1 AND phase IN ('prepared','published')
         ORDER BY created_at_ms ASC,intent_id ASC LIMIT 1",
        [subscription_id],
        |row| {
            Ok(YoutubeArchiveMergeIntentRow {
                intent_id: row.get(0)?,
                subscription_id: row.get(1)?,
                target_archive_path: row.get(2)?,
                source_archive_sha256: row.get(3)?,
                intended_archive_sha256: row.get(4)?,
                intended_video_ids_json: row.get(5)?,
                phase: row.get(6)?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

fn parse_archive_merge_intent_row(
    row: YoutubeArchiveMergeIntentRow,
) -> Result<YoutubeArchiveMergeIntent> {
    let intended_video_ids = serde_json::from_str::<Vec<String>>(&row.intended_video_ids_json)
        .map_err(|error| {
            EngineError::InstallFailed(format!(
                "trusted archive intent members are invalid: {error}"
            ))
        })?;
    Ok(YoutubeArchiveMergeIntent {
        intent_id: row.intent_id,
        subscription_id: row.subscription_id,
        target_archive_path: row.target_archive_path,
        source_archive_sha256: row.source_archive_sha256,
        intended_archive_sha256: row.intended_archive_sha256,
        intended_video_ids,
        phase: row.phase,
    })
}

fn validate_archive_merge_intent(
    paths: &AppPaths,
    intent: &YoutubeArchiveMergeIntent,
) -> Result<PathBuf> {
    if Uuid::parse_str(&intent.intent_id)
        .map(|value| value.to_string() != intent.intent_id)
        .unwrap_or(true)
        || !matches!(intent.phase.as_str(), "prepared" | "published")
        || intent.intended_video_ids.iter().any(|id| {
            id.trim().is_empty() || id.trim() != id || id.chars().any(char::is_whitespace)
        })
    {
        return Err(EngineError::InstallFailed(
            "trusted archive intent binding is invalid".into(),
        ));
    }
    let mut canonical_ids = intent.intended_video_ids.clone();
    canonical_ids.sort();
    canonical_ids.dedup();
    if canonical_ids != intent.intended_video_ids
        || archive_body_sha256(&canonical_archive_body(&canonical_ids))
            != intent.intended_archive_sha256
    {
        return Err(EngineError::InstallFailed(
            "trusted archive intent content is invalid".into(),
        ));
    }
    let expected_path = paths.youtube_subscription_archive_state_path(&intent.subscription_id);
    if expected_path.to_string_lossy() != intent.target_archive_path {
        return Err(EngineError::InstallFailed(
            "trusted archive intent target is invalid".into(),
        ));
    }
    Ok(expected_path)
}

fn advance_archive_merge_intent_phase(
    paths: &AppPaths,
    intent_id: &str,
    from_phase: &str,
    to_phase: &str,
) -> Result<()> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    let advanced = conn.execute(
        "UPDATE youtube_archive_merge_intent SET phase=?1,updated_at_ms=?2
         WHERE intent_id=?3 AND phase=?4",
        params![to_phase, now_ms(), intent_id, from_phase],
    )?;
    if advanced != 1 {
        return Err(EngineError::InstallFailed(
            "youtube archive intent phase CAS failed".into(),
        ));
    }
    Ok(())
}

fn remove_archive_merge_carrier_if_owned(path: &Path, intent_id: &str) -> Result<bool> {
    let carrier = match read_archive_merge_carrier(path) {
        Ok(carrier) => carrier,
        Err(_) => return Ok(false),
    };
    if carrier.intent_id != intent_id {
        return Ok(false);
    }
    std::fs::remove_file(path)?;
    Ok(true)
}

/// Reconcile a bounded number of interrupted canonical archive publications. The journal is the
/// durable proof that filesystem bytes may be ahead of SQLite, so it is removed only after the
/// exact canonical file has been projected successfully. Replays are idempotent.
pub fn reconcile_youtube_archive_merge_journals(paths: &AppPaths) -> Result<usize> {
    let _guard = lock_youtube_archive_mutation()?;
    reconcile_archive_merge_journals_locked(paths)
}

fn record_archive_merge_intent_failure(
    paths: &AppPaths,
    row: &YoutubeArchiveMergeIntentRow,
    error: &EngineError,
) -> Result<()> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    let failed_at_ms = now_ms();
    conn.execute(
        "INSERT INTO youtube_archive_merge_intent_failure(
           intent_id,subscription_id,error_message,failure_count,
           first_failed_at_ms,last_failed_at_ms,resolved_at_ms
         ) VALUES(?1,?2,?3,1,?4,?4,NULL)
         ON CONFLICT(intent_id) DO UPDATE SET
           error_message=excluded.error_message,
           failure_count=youtube_archive_merge_intent_failure.failure_count+1,
           last_failed_at_ms=excluded.last_failed_at_ms,
           resolved_at_ms=NULL",
        params![
            row.intent_id,
            row.subscription_id,
            error.to_string(),
            failed_at_ms,
        ],
    )?;
    Ok(())
}

fn reconcile_archive_merge_intent_row(
    paths: &AppPaths,
    row: YoutubeArchiveMergeIntentRow,
) -> Result<()> {
    let intent = parse_archive_merge_intent_row(row)?;
    let archive_path = validate_archive_merge_intent(paths, &intent)?;
    let current_sha256 = archive_file_sha256(&archive_path)?;
    if intent.phase == "prepared" {
        if current_sha256 == intent.source_archive_sha256 {
            persistence::atomic_write_text(
                &archive_path,
                &canonical_archive_body(&intent.intended_video_ids),
            )?;
        } else if current_sha256 != intent.intended_archive_sha256 {
            return Err(EngineError::InstallFailed(
                "youtube archive changed outside its trusted merge intent".into(),
            ));
        }
        advance_archive_merge_intent_phase(paths, &intent.intent_id, "prepared", "published")?;
    } else if current_sha256 != intent.intended_archive_sha256 {
        return Err(EngineError::InstallFailed(
            "published youtube archive does not match its trusted intent".into(),
        ));
    }
    apply_archive_merge_projection(
        paths,
        &intent.intent_id,
        &intent.subscription_id,
        &intent.intended_video_ids,
    )?;
    let journal = archive_merge_journal_path(&archive_path);
    let _ = remove_archive_merge_carrier_if_owned(&journal, &intent.intent_id)?;
    Ok(())
}

fn reconcile_archive_merge_for_subscription_locked(
    paths: &AppPaths,
    subscription_id: &str,
) -> Result<usize> {
    let Some(row) = load_pending_archive_merge_intent_row_for_subscription(paths, subscription_id)?
    else {
        return Ok(0);
    };
    match reconcile_archive_merge_intent_row(paths, row.clone()) {
        Ok(()) => Ok(1),
        Err(error) => {
            if let Err(evidence_error) = record_archive_merge_intent_failure(paths, &row, &error) {
                return Err(EngineError::InstallFailed(format!(
                    "{error}; durable archive quarantine evidence could not be written: {evidence_error}"
                )));
            }
            Err(error)
        }
    }
}

fn reconcile_archive_merge_journals_locked(paths: &AppPaths) -> Result<usize> {
    let mut recovered = 0;
    let mut evidence_errors = Vec::new();
    for row in load_pending_archive_merge_intent_rows(paths)? {
        match reconcile_archive_merge_intent_row(paths, row.clone()) {
            Ok(()) => recovered += 1,
            Err(error) => {
                if let Err(evidence_error) =
                    record_archive_merge_intent_failure(paths, &row, &error)
                {
                    evidence_errors.push(format!("{}: {}", row.intent_id, evidence_error));
                }
            }
        }
    }
    // Ordinary locked, malformed, or unsafe carriers are isolated and durably retryable. If that
    // retry ownership itself cannot be persisted, fail closed so startup/callers cannot report a
    // clean reconciliation or advance the durable cursor past unowned work.
    cleanup_committed_archive_merge_carriers(paths)?;
    if !evidence_errors.is_empty() {
        return Err(EngineError::InstallFailed(format!(
            "one or more archive quarantine receipts could not be written: {}",
            evidence_errors.join("; ")
        )));
    }
    Ok(recovered)
}

fn cleanup_committed_archive_merge_carriers(
    paths: &AppPaths,
) -> Result<ArchiveCarrierCleanupReport> {
    cleanup_committed_archive_merge_carriers_with(paths, &mut |path| std::fs::remove_file(path))
}

fn cleanup_committed_archive_merge_carriers_with<F>(
    paths: &AppPaths,
    remove_file: &mut F,
) -> Result<ArchiveCarrierCleanupReport>
where
    F: FnMut(&Path) -> std::io::Result<()>,
{
    let (after_intent_id, cursor_generation) = load_archive_carrier_cleanup_cursor(paths)?;

    let retry_candidates = load_failed_committed_archive_carrier_cleanup_candidates(
        paths,
        YOUTUBE_ARCHIVE_CARRIER_CLEANUP_RETRY_LIMIT,
    )?;
    let mut page_candidates = load_committed_archive_carrier_cleanup_candidates(
        paths,
        after_intent_id.as_deref(),
        YOUTUBE_ARCHIVE_JOURNAL_REPLAY_LIMIT,
    )?;
    // UUID intent IDs are not insertion ordered. Once the keyset reaches the end, wrap once so
    // newer committed intents that sort before the prior cursor are not starved indefinitely.
    if page_candidates.is_empty() && after_intent_id.is_some() {
        page_candidates = load_committed_archive_carrier_cleanup_candidates(
            paths,
            None,
            YOUTUBE_ARCHIVE_JOURNAL_REPLAY_LIMIT,
        )?;
    }

    let page_tail = page_candidates
        .last()
        .map(|(intent_id, _)| intent_id.clone());
    let mut seen = HashSet::new();
    let candidates = retry_candidates
        .into_iter()
        .chain(page_candidates)
        .filter(|(intent_id, _)| seen.insert(intent_id.clone()))
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return Ok(ArchiveCarrierCleanupReport::default());
    }
    let receipt_conn = db::open(paths)?;
    db::migrate(&receipt_conn)?;
    let mut report = ArchiveCarrierCleanupReport::default();
    for (intent_id, subscription_id) in candidates {
        match cleanup_committed_archive_merge_carrier(
            paths,
            &intent_id,
            &subscription_id,
            remove_file,
        ) {
            Ok(removed) => {
                report.removed += usize::from(removed);
                if let Err(error) =
                    resolve_archive_carrier_cleanup_failure(&receipt_conn, &intent_id)
                {
                    eprintln!(
                        "youtube archive carrier cleanup resolution receipt deferred for {intent_id}: {error}"
                    );
                }
            }
            Err(error) => {
                report.failed += 1;
                record_archive_carrier_cleanup_failure(
                    &receipt_conn,
                    &intent_id,
                    &subscription_id,
                    &error,
                )
                .map_err(|receipt_error| {
                    EngineError::InstallFailed(format!(
                        "youtube archive carrier cleanup failed and retry ownership could not be persisted for {intent_id}: {receipt_error}"
                    ))
                })?;
            }
        }
    }

    // Advance durably only after every candidate in the selected page has had an independent
    // attempt. A crash before this CAS safely replays the page; a competing process that already
    // advanced the generation wins without letting this stale pass overwrite newer progress.
    if let Some(last_intent_id) = page_tail {
        advance_archive_carrier_cleanup_cursor(
            &receipt_conn,
            cursor_generation,
            after_intent_id.as_deref(),
            &last_intent_id,
        )?;
    }
    Ok(report)
}

fn load_archive_carrier_cleanup_cursor(paths: &AppPaths) -> Result<(Option<String>, i64)> {
    let conn = db::open_readonly(paths)?;
    conn.query_row(
        "SELECT after_intent_id,generation
         FROM youtube_archive_carrier_cleanup_cursor WHERE singleton=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .map_err(EngineError::from)
}

fn advance_archive_carrier_cleanup_cursor(
    conn: &rusqlite::Connection,
    expected_generation: i64,
    expected_after_intent_id: Option<&str>,
    next_after_intent_id: &str,
) -> Result<bool> {
    let changed = conn.execute(
        "UPDATE youtube_archive_carrier_cleanup_cursor
         SET after_intent_id=?1,generation=generation+1,updated_at_ms=?2
         WHERE singleton=1 AND generation=?3
           AND ((after_intent_id IS NULL AND ?4 IS NULL) OR after_intent_id=?4)",
        params![
            next_after_intent_id,
            now_ms(),
            expected_generation,
            expected_after_intent_id,
        ],
    )?;
    Ok(changed == 1)
}

fn cleanup_committed_archive_merge_carrier<F>(
    paths: &AppPaths,
    intent_id: &str,
    subscription_id: &str,
    remove_file: &mut F,
) -> Result<bool>
where
    F: FnMut(&Path) -> std::io::Result<()>,
{
    let journal_path = validated_archive_merge_carrier_path(paths, subscription_id)?;
    let Some(journal_path) = journal_path else {
        return Ok(false);
    };
    let carrier = read_archive_merge_carrier(&journal_path)?;
    if carrier.intent_id != intent_id {
        return Err(EngineError::InstallFailed(
            "youtube archive carrier intent binding does not match its committed database row"
                .into(),
        ));
    }
    remove_file(&journal_path)?;
    Ok(true)
}

fn validated_archive_merge_carrier_path(
    paths: &AppPaths,
    subscription_id: &str,
) -> Result<Option<PathBuf>> {
    let mut components = Path::new(subscription_id).components();
    let safe_component = matches!(components.next(), Some(std::path::Component::Normal(_)))
        && components.next().is_none()
        && !subscription_id.trim().is_empty()
        && !subscription_id.contains('/')
        && !subscription_id.contains('\\');
    if !safe_component {
        return Err(EngineError::InstallFailed(
            "youtube archive carrier cleanup refused an unsafe subscription identifier".into(),
        ));
    }

    let state_root = paths.youtube_subscription_state_dir();
    let journal_path =
        archive_merge_journal_path(&paths.youtube_subscription_archive_state_path(subscription_id));
    if !journal_path.starts_with(&state_root) {
        return Err(EngineError::InstallFailed(
            "youtube archive carrier cleanup path escaped the managed state root".into(),
        ));
    }
    if !journal_path.try_exists()? {
        return Ok(None);
    }
    let canonical_root = std::fs::canonicalize(&state_root)?;
    let canonical_journal = std::fs::canonicalize(&journal_path)?;
    if !canonical_journal.starts_with(&canonical_root) {
        return Err(EngineError::InstallFailed(
            "youtube archive carrier cleanup canonical path escaped the managed state root".into(),
        ));
    }
    Ok(Some(canonical_journal))
}

fn record_archive_carrier_cleanup_failure(
    conn: &rusqlite::Connection,
    intent_id: &str,
    subscription_id: &str,
    error: &EngineError,
) -> Result<()> {
    let failed_at_ms = now_ms();
    let message = format!("{YOUTUBE_ARCHIVE_CARRIER_CLEANUP_ERROR_PREFIX} {error}");
    conn.execute(
        "INSERT INTO youtube_archive_merge_intent_failure(
           intent_id,subscription_id,error_message,failure_count,
           first_failed_at_ms,last_failed_at_ms,resolved_at_ms
         ) VALUES(?1,?2,?3,1,?4,?4,NULL)
         ON CONFLICT(intent_id) DO UPDATE SET
           error_message=excluded.error_message,
           failure_count=youtube_archive_merge_intent_failure.failure_count+1,
           last_failed_at_ms=excluded.last_failed_at_ms,
           resolved_at_ms=NULL",
        params![intent_id, subscription_id, message, failed_at_ms],
    )?;
    Ok(())
}

fn resolve_archive_carrier_cleanup_failure(
    conn: &rusqlite::Connection,
    intent_id: &str,
) -> Result<()> {
    conn.execute(
        "UPDATE youtube_archive_merge_intent_failure
         SET resolved_at_ms=?1
         WHERE intent_id=?2 AND resolved_at_ms IS NULL
           AND error_message LIKE ?3",
        params![
            now_ms(),
            intent_id,
            format!("{YOUTUBE_ARCHIVE_CARRIER_CLEANUP_ERROR_PREFIX}%"),
        ],
    )?;
    Ok(())
}

fn load_failed_committed_archive_carrier_cleanup_candidates(
    paths: &AppPaths,
    limit: usize,
) -> Result<Vec<(String, String)>> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let conn = db::open_readonly(paths)?;
    let mut stmt = conn.prepare(
        "SELECT intent.intent_id,intent.subscription_id
         FROM youtube_archive_merge_intent_failure failure
         JOIN youtube_archive_merge_intent intent ON intent.intent_id=failure.intent_id
         WHERE failure.resolved_at_ms IS NULL
           AND failure.error_message LIKE ?1
           AND intent.phase='committed'
         ORDER BY failure.last_failed_at_ms ASC,intent.intent_id ASC
         LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(
            params![
                format!("{YOUTUBE_ARCHIVE_CARRIER_CLEANUP_ERROR_PREFIX}%"),
                limit as i64,
            ],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn load_committed_archive_carrier_cleanup_candidates(
    paths: &AppPaths,
    after_intent_id: Option<&str>,
    limit: usize,
) -> Result<Vec<(String, String)>> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let conn = db::open_readonly(paths)?;
    let rows = if let Some(after_intent_id) = after_intent_id {
        let mut stmt = conn.prepare(
            "SELECT intent_id,subscription_id
             FROM youtube_archive_merge_intent
             WHERE phase='committed' AND intent_id>?1
             ORDER BY intent_id ASC
             LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![after_intent_id, limit as i64], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    } else {
        let mut stmt = conn.prepare(
            "SELECT intent_id,subscription_id
             FROM youtube_archive_merge_intent
             WHERE phase='committed'
             ORDER BY intent_id ASC
             LIMIT ?1",
        )?;
        let rows = stmt
            .query_map([limit as i64], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    Ok(rows)
}

fn active_archive_merge_quarantine_subscription_ids(paths: &AppPaths) -> Result<HashSet<String>> {
    let conn = db::open_readonly(paths)?;
    let mut stmt = conn.prepare(
        "SELECT failure.subscription_id
         FROM youtube_archive_merge_intent_failure failure
         JOIN youtube_archive_merge_intent intent ON intent.intent_id=failure.intent_id
         WHERE failure.resolved_at_ms IS NULL AND intent.phase<>'committed'",
    )?;
    let subscription_ids = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<HashSet<_>>>()
        .map_err(EngineError::from)?;
    Ok(subscription_ids)
}

fn merge_archive_file_locked(
    paths: &AppPaths,
    path: &Path,
    video_ids: &HashSet<String>,
) -> Result<(usize, usize)> {
    let source_archive_sha256 = archive_file_sha256(path)?;
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

    if appended == 0 {
        return Ok((appended, skipped_existing));
    }

    let subscription_id = archive_subscription_id_from_path(paths, path)?;
    if archive_file_sha256(path)? != source_archive_sha256 {
        return Err(EngineError::InstallFailed(
            "youtube archive changed while preparing trusted publication".into(),
        ));
    }
    let intent_id = create_archive_merge_intent(
        paths,
        &subscription_id,
        path,
        &source_archive_sha256,
        &merged,
    )?;
    let journal = archive_merge_journal_path(path);
    write_archive_merge_journal(&journal, &intent_id)?;
    if archive_file_sha256(path)? != source_archive_sha256 {
        return Err(EngineError::InstallFailed(
            "youtube archive changed before trusted publication".into(),
        ));
    }
    let body = canonical_archive_body(&merged);
    persistence::atomic_write_text(path, &body)?;
    advance_archive_merge_intent_phase(paths, &intent_id, "prepared", "published")?;
    apply_archive_merge_projection(paths, &intent_id, &subscription_id, &merged)?;
    let _ = remove_archive_merge_carrier_if_owned(&journal, &intent_id)?;
    Ok((appended, skipped_existing))
}

fn apply_archive_merge_projection(
    paths: &AppPaths,
    intent_id: &str,
    subscription_id: &str,
    merged: &[String],
) -> Result<()> {
    let mut conn = db::open(paths)?;
    db::migrate(&conn)?;
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    for video_id in merged {
        tx.execute(
            "INSERT OR IGNORE INTO youtube_subscription_archive_member(subscription_id,video_id,discovered_at_ms) VALUES(?1,?2,?3)",
            params![subscription_id, video_id, now_ms()],
        )?;
    }
    tx.execute(
        "INSERT INTO youtube_subscription_archive_rollup(subscription_id,video_count,rebuilt_at_ms,source) VALUES(?1,?2,?3,'archive_event') ON CONFLICT(subscription_id) DO UPDATE SET video_count=excluded.video_count,rebuilt_at_ms=excluded.rebuilt_at_ms,source=excluded.source",
        params![subscription_id, merged.len() as i64, now_ms()],
    )?;
    let advanced = tx.execute(
        "UPDATE derived_projection_state SET dirty=1,updated_at_ms=updated_at_ms+1 WHERE projection='youtube_archive'",
        [],
    )?;
    if advanced != 1 {
        return Err(EngineError::InstallFailed(
            "youtube archive projection generation row is missing".into(),
        ));
    }
    let committed = tx.execute(
        "UPDATE youtube_archive_merge_intent SET phase='committed',updated_at_ms=?1
         WHERE intent_id=?2 AND subscription_id=?3 AND phase='published'
           AND intended_archive_sha256=?4 AND intended_video_ids_json=?5",
        params![
            now_ms(),
            intent_id,
            subscription_id,
            archive_body_sha256(&canonical_archive_body(merged)),
            serde_json::to_string(merged)?,
        ],
    )?;
    if committed != 1 {
        return Err(EngineError::InstallFailed(
            "youtube archive intent commit CAS failed".into(),
        ));
    }
    tx.execute(
        "UPDATE youtube_archive_merge_intent_failure
         SET resolved_at_ms=?1
         WHERE intent_id=?2 AND resolved_at_ms IS NULL",
        params![now_ms(), intent_id],
    )?;
    tx.commit()?;
    Ok(())
}

pub(crate) fn merge_youtube_archive_member(
    paths: &AppPaths,
    subscription_id: &str,
    video_id: &str,
) -> Result<()> {
    let path = paths.youtube_subscription_archive_state_path(subscription_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    merge_archive_file(paths, &path, &HashSet::from([video_id.to_string()]))?;
    Ok(())
}

pub(crate) fn mark_youtube_archive_projection_dirty(paths: &AppPaths) -> Result<()> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    let changed = conn.execute(
        "UPDATE derived_projection_state SET dirty=1,updated_at_ms=updated_at_ms+1 WHERE projection='youtube_archive'",
        [],
    )?;
    if changed != 1 {
        return Err(EngineError::InstallFailed(
            "youtube archive projection generation row is missing".to_string(),
        ));
    }
    Ok(())
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
        return root_rebind::resolve_active_alias_path(paths, &p, false);
    }

    let library_root = if let Some(library_id) = sub.library_id.as_deref() {
        match video_libraries::get_video_library_by_id(paths, library_id)? {
            Some(library) => PathBuf::from(library.root_path),
            None => video_libraries::default_video_library_root(paths)?,
        }
    } else {
        video_libraries::default_video_library_root(paths)?
    };
    let library_root = root_rebind::resolve_active_alias_path(paths, &library_root, false)?;
    Ok(library_root.join(sanitize_folder_map(&sub.folder_map)))
}

pub fn preview_youtube_subscription_output_dir(
    paths: &AppPaths,
    req: YoutubeSubscriptionOutputPreviewRequest,
) -> Result<YoutubeSubscriptionOutputPreview> {
    let normalized = normalize_upsert(YoutubeSubscriptionUpsert {
        id: None,
        title: req.title,
        source_url: req.source_url,
        folder_map: req.folder_map,
        output_dir_override: req.output_dir_override,
        library_id: req.library_id,
        use_browser_cookies: false,
        browser_cookie_source: None,
        auth_session_input: None,
        clear_auth_session: false,
        active: true,
        preset_id: None,
        group_ids: Vec::new(),
        refresh_interval_minutes: Some(DEFAULT_REFRESH_INTERVAL_MINUTES),
    })?;

    let (target, uses_output_override) = if let Some(override_dir) = normalized.output_dir_override
    {
        let mut p = PathBuf::from(override_dir);
        if !p.is_absolute() {
            p = std::env::current_dir()?.join(p);
        }
        (p, true)
    } else {
        let library_root = if let Some(library_id) = normalized.library_id.as_deref() {
            let library =
                video_libraries::get_video_library_by_id(paths, library_id)?.ok_or_else(|| {
                    EngineError::InstallFailed(format!(
                        "video library not found or disabled: {library_id}"
                    ))
                })?;
            if !library.active {
                return Err(EngineError::InstallFailed(format!(
                    "video library not found or disabled: {library_id}"
                )));
            }
            PathBuf::from(library.root_path)
        } else {
            video_libraries::default_video_library_root(paths)?
        };
        (
            library_root.join(sanitize_folder_map(&normalized.folder_map)),
            false,
        )
    };

    Ok(YoutubeSubscriptionOutputPreview {
        path: target.to_string_lossy().to_string(),
        exists: target.is_dir(),
        uses_output_override,
    })
}

// WP-0257: read-only detail-pane payload for a single subscription. `title` mirrors the queued
// job's `target_title` column (may be null before the job resolves a title); `url` is the source
// URL read from the job's params_json.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingVideo {
    pub title: Option<String>,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoutubeSubscriptionVideos {
    pub downloaded: Vec<library::LibraryItem>,
    pub deleted: Vec<library::LibraryItem>,
    pub pending: Vec<PendingVideo>,
}

/// Max rows returned per projection (downloaded / deleted / pending). The caller-supplied `limit` is clamped
/// into `[1, MAX]` so a bad value can never turn this into an unbounded scan.
const MAX_SUBSCRIPTION_VIDEOS_LIMIT: usize = 500;

/// WP-0257: READ-ONLY per-subscription video listing for the detail pane.
///
/// Returns canonical source-membership projections for available and operator-deleted library
/// items (bounded, newest-first), plus still-queued download jobs (title + URL). Folder placement
/// is not treated as subscription identity, so moved media remains attached to its source. This
/// is the deliberate opposite of a hot-loop DB writer: all reads use [`db::open_readonly`], every
/// query has a hard `LIMIT`, and nothing here writes or migrates, so it cannot lock `app.sqlite`
/// or block the job runner.
pub fn youtube_subscription_videos(
    paths: &AppPaths,
    subscription_id: &str,
    limit: usize,
) -> Result<YoutubeSubscriptionVideos> {
    let limit = limit.clamp(1, MAX_SUBSCRIPTION_VIDEOS_LIMIT);

    // Load the subscription over a READ-ONLY connection. Deliberately NOT
    // `get_youtube_subscription_by_id`, which opens a writable connection and runs `migrate()`.
    {
        let conn = db::open_readonly(paths)?;
        subscription_by_id_conn(&conn, subscription_id)?.ok_or_else(|| {
            EngineError::InstallFailed(format!("subscription not found: {subscription_id}"))
        })?;
    }

    let downloaded = library::list_subscription_items_by_file_status(
        paths,
        subscription_id,
        library::LIBRARY_FILE_STATUS_AVAILABLE,
        limit,
    )?;
    let deleted = library::list_subscription_items_by_file_status(
        paths,
        subscription_id,
        library::LIBRARY_FILE_STATUS_OPERATOR_DELETED,
        limit,
    )?;
    let pending = list_pending_subscription_videos(paths, subscription_id, limit)?;

    Ok(YoutubeSubscriptionVideos {
        downloaded,
        deleted,
        pending,
    })
}

/// Read-only, bounded lookup of the still-`queued` `download_direct_url` jobs that carry this
/// `subscription_id` in their `params_json`. Pre-filters in SQL with an escaped `LIKE` on the
/// exact `"subscription_id":"<id>"` needle (the id is a UUID, so no realistic false positives),
/// then re-parses `params_json` per row to confirm the id and pull the source `url`. The job
/// `target_title` column supplies the display title.
fn list_pending_subscription_videos(
    paths: &AppPaths,
    subscription_id: &str,
    limit: usize,
) -> Result<Vec<PendingVideo>> {
    let conn = db::open_readonly(paths)?;
    // The literal `"subscription_id":"..."` contains `_`, a LIKE wildcard, so it must be escaped.
    let needle = format!("\"subscription_id\":\"{subscription_id}\"");
    let pattern = format!("%{}%", escape_like_pipe(&needle));

    let mut stmt = conn.prepare(
        r#"
SELECT target_title, params_json
FROM job
WHERE type = 'download_direct_url'
  AND status = 'queued'
  AND params_json LIKE ?1 ESCAPE '|'
ORDER BY created_at_ms DESC
LIMIT ?2
"#,
    )?;
    let rows = stmt
        .query_map(params![pattern, limit as i64], |row| {
            Ok((row.get::<_, Option<String>>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);

    let mut pending = Vec::with_capacity(rows.len());
    for (target_title, params_json) in rows {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&params_json) else {
            continue;
        };
        // Confirm the match is the actual `subscription_id` field, not an incidental substring
        // elsewhere in params_json.
        if value.get("subscription_id").and_then(|v| v.as_str()) != Some(subscription_id) {
            continue;
        }
        let Some(url) = value
            .get("url")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty())
        else {
            continue;
        };
        pending.push(PendingVideo {
            title: target_title
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty()),
            url: url.to_string(),
        });
    }
    Ok(pending)
}

/// Escape SQLite `LIKE` wildcards (`|`, `%`, `_`) using `|` as the ESCAPE character, matching the
/// convention used by the library-side prefix queries.
fn escape_like_pipe(value: &str) -> String {
    value
        .replace('|', "||")
        .replace('%', "|%")
        .replace('_', "|_")
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

pub fn extract_youtube_id_from_filename(filename: &str) -> Option<&str> {
    let stem = if let Some(dot_idx) = filename.rfind('.') {
        &filename[..dot_idx]
    } else {
        filename
    };

    if let Some(start) = stem.rfind('[') {
        if let Some(end) = stem[start + 1..].find(']') {
            let candidate = stem[start + 1..start + 1 + end].trim();
            if candidate.len() == 11
                && candidate
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            {
                return Some(candidate);
            }
        }
    }

    if let Some(hyphen_idx) = stem.rfind('-') {
        let candidate = stem[hyphen_idx + 1..].trim();
        if candidate.len() == 11
            && candidate
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Some(candidate);
        }
    }

    if let Some(us_idx) = stem.rfind('_') {
        let candidate = stem[us_idx + 1..].trim();
        if candidate.len() == 11
            && candidate
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Some(candidate);
        }
    }

    let candidate = stem.trim();
    if candidate.len() == 11
        && candidate
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Some(candidate);
    }

    None
}

pub fn ensure_youtube_subscription_archive_state(
    paths: &AppPaths,
    sub: &YoutubeSubscriptionRow,
) -> Result<PathBuf> {
    let archive_path = youtube_subscription_archive_path(paths, sub)?;
    if let Some(parent) = archive_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut discovered_ids: HashSet<String> = HashSet::new();

    let legacy_path = legacy_output_youtube_subscription_archive_path(paths, sub)?;
    if legacy_path != archive_path && legacy_path.exists() {
        if let Ok(legacy_ids) = read_archive_file_ids(&legacy_path) {
            discovered_ids.extend(legacy_ids);
        }
    }

    // WP-0147: Check recorded media source memberships for this subscription
    if let Ok(conn) = db::open_readonly(paths) {
        if let Ok(mut stmt) = conn.prepare(
            "SELECT media_id FROM media_source_membership WHERE source_subscription_id = ?1 AND service = 'youtube'"
        ) {
            if let Ok(rows) = stmt.query_map([&sub.id], |r| r.get::<_, String>(0)) {
                for id in rows.flatten() {
                    let trimmed = id.trim();
                    if !trimmed.is_empty() {
                        discovered_ids.insert(trimmed.to_string());
                    }
                }
            }
        }
    }

    // WP-0147: Read-only scan of existing output folder for previously downloaded media
    if let Ok(output_dir) = youtube_subscription_output_dir(paths, sub) {
        if output_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&output_dir) {
                for entry in entries.flatten() {
                    if let Ok(ft) = entry.file_type() {
                        if ft.is_file() {
                            if let Some(name) = entry.file_name().to_str() {
                                let lower = name.to_ascii_lowercase();
                                if lower.ends_with(".mkv")
                                    || lower.ends_with(".mp4")
                                    || lower.ends_with(".webm")
                                    || lower.ends_with(".ts")
                                    || lower.ends_with(".m4v")
                                {
                                    if let Some(vid) = extract_youtube_id_from_filename(name) {
                                        discovered_ids.insert(vid.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if !discovered_ids.is_empty() {
        let existing_ids = if archive_path.exists() {
            read_archive_file_ids(&archive_path).unwrap_or_default()
        } else {
            HashSet::new()
        };
        let missing_ids: HashSet<String> = discovered_ids
            .into_iter()
            .filter(|id| !existing_ids.contains(id))
            .collect();
        if !missing_ids.is_empty() {
            merge_archive_file(paths, &archive_path, &missing_ids)?;
            for vid in &missing_ids {
                let _ = record_youtube_archive_member(paths, &sub.id, vid);
            }
        }
    } else if !archive_path.exists() {
        std::fs::File::create(&archive_path)?;
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

// WP-0258: this function walks per-subscription yt-dlp archive files on disk (observed up to
// 43s) and is polled repeatedly by the UI, adding load under DB contention. A short-TTL
// in-process cache serves recent results so back-to-back UI polls do not re-scan the disk.
// Read-only and thread-safe; the public signature and Tauri command are unchanged.
pub fn youtube_subscriptions_archive_stats(paths: &AppPaths) -> Result<HashMap<String, usize>> {
    // This is a hot panel poll. It must never take the archive writer lock, reconcile journals,
    // migrate, or open SQLite read/write. Startup/explicit repair and the bounded background
    // rebuild own those mutations; the panel consumes only their last committed projection.
    let conn = db::open_readonly(paths)?;
    // Dirty/empty projections are intentionally returned as-is. Repair is owned by the explicit
    // `subscription_projections_rebuild` command (and startup repair), never by a hot UI poll.
    let mut stmt = conn
        .prepare("SELECT subscription_id, video_count FROM youtube_subscription_archive_rollup")?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
        })?
        .collect::<rusqlite::Result<HashMap<_, _>>>()?;
    Ok(rows)
}

pub fn record_youtube_archive_member(
    paths: &AppPaths,
    subscription_id: &str,
    video_id: &str,
) -> Result<()> {
    let mut conn = db::open(paths)?;
    db::migrate(&conn)?;
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    tx.execute(
        "INSERT OR IGNORE INTO youtube_subscription_archive_member(subscription_id,video_id,discovered_at_ms) VALUES(?1,?2,?3)",
        params![subscription_id, video_id, now_ms()],
    )?;
    tx.execute(
        "INSERT INTO youtube_subscription_archive_rollup(subscription_id,video_count,rebuilt_at_ms,source) SELECT ?1,COUNT(*),?2,'incremental' FROM youtube_subscription_archive_member WHERE subscription_id=?1 ON CONFLICT(subscription_id) DO UPDATE SET video_count=excluded.video_count, rebuilt_at_ms=excluded.rebuilt_at_ms, source=excluded.source",
        params![subscription_id, now_ms()],
    )?;
    // This incremental update is complete for its member, but it must not erase dirty evidence
    // raised by an unrelated subscription or interrupted canonical reconciliation.
    tx.execute("UPDATE derived_projection_state SET updated_at_ms=updated_at_ms+1 WHERE projection='youtube_archive'", [])?;
    tx.commit()?;
    Ok(())
}

pub fn rebuild_youtube_subscription_archive_rollups(
    paths: &AppPaths,
) -> Result<HashMap<String, usize>> {
    let _guard = lock_youtube_archive_mutation()?;
    reconcile_archive_merge_journals_locked(paths)?;
    for _ in 0..4 {
        let generation = db::open_readonly(paths)?.query_row(
            "SELECT updated_at_ms FROM derived_projection_state WHERE projection='youtube_archive'",
            [],
            |r| r.get::<_, i64>(0),
        )?;
        let file_generation = youtube_archive_file_generation(paths)?;
        let quarantined = active_archive_merge_quarantine_subscription_ids(paths)?;
        let mut canonical = compute_youtube_subscriptions_archive_stats(paths)?;
        canonical.retain(|subscription_id, _| !quarantined.contains(subscription_id));
        if youtube_archive_file_generation(paths)? != file_generation {
            continue;
        }
        let mut conn = db::open(paths)?;
        db::migrate(&conn)?;
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let current_generation: i64 = tx.query_row(
            "SELECT updated_at_ms FROM derived_projection_state WHERE projection='youtube_archive'",
            [],
            |r| r.get(0),
        )?;
        if current_generation != generation {
            drop(tx);
            continue;
        }
        tx.execute(
            "DELETE FROM youtube_subscription_archive_member
             WHERE subscription_id NOT IN (
               SELECT failure.subscription_id
               FROM youtube_archive_merge_intent_failure failure
               JOIN youtube_archive_merge_intent intent ON intent.intent_id=failure.intent_id
               WHERE failure.resolved_at_ms IS NULL AND intent.phase<>'committed'
             )",
            [],
        )?;
        tx.execute(
            "DELETE FROM youtube_subscription_archive_rollup
             WHERE subscription_id NOT IN (
               SELECT failure.subscription_id
               FROM youtube_archive_merge_intent_failure failure
               JOIN youtube_archive_merge_intent intent ON intent.intent_id=failure.intent_id
               WHERE failure.resolved_at_ms IS NULL AND intent.phase<>'committed'
             )",
            [],
        )?;
        for subscription_id in canonical.keys() {
            let archive_path = paths.youtube_subscription_archive_state_path(subscription_id);
            for video_id in read_archive_file_ids(&archive_path)? {
                tx.execute(
                "INSERT OR IGNORE INTO youtube_subscription_archive_member(subscription_id,video_id,discovered_at_ms) VALUES(?1,?2,?3)",
                params![subscription_id, video_id, now_ms()],
            )?;
            }
        }
        for (subscription_id, video_count) in &canonical {
            tx.execute(
            "INSERT INTO youtube_subscription_archive_rollup(subscription_id,video_count,rebuilt_at_ms,source) VALUES(?1,?2,?3,'canonical_rebuild')",
            params![subscription_id, *video_count as i64, now_ms()],
        )?;
        }
        let cleared = tx.execute(
            "UPDATE derived_projection_state
             SET dirty=CASE WHEN EXISTS(
               SELECT 1 FROM youtube_archive_merge_intent_failure failure
               JOIN youtube_archive_merge_intent intent ON intent.intent_id=failure.intent_id
               WHERE failure.resolved_at_ms IS NULL AND intent.phase<>'committed'
             ) THEN 1 ELSE 0 END
             WHERE projection='youtube_archive' AND updated_at_ms=?1",
            [generation],
        )?;
        if cleared != 1 {
            drop(tx);
            continue;
        }
        tx.commit()?;
        if youtube_archive_file_generation(paths)? != file_generation {
            // A writer outside the DB transaction changed canonical bytes. Re-dirty and retry;
            // never return a rollup proven against a different archive generation.
            mark_youtube_archive_projection_dirty(paths)?;
            continue;
        }
        return youtube_subscriptions_archive_stats(paths);
    }
    Err(EngineError::InstallFailed(
        "youtube archive rollup remained dirty during rebuild".into(),
    ))
}

fn youtube_archive_file_generation(paths: &AppPaths) -> Result<Vec<(String, u64, u128, String)>> {
    use sha2::{Digest as _, Sha256};
    use std::io::Read as _;

    let state_dir = paths.youtube_subscription_state_dir();
    let entries = match std::fs::read_dir(&state_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut generation = Vec::new();
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let Some(subscription_id) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        let archive = paths.youtube_subscription_archive_state_path(&subscription_id);
        let Ok(metadata) = std::fs::metadata(&archive) else {
            continue;
        };
        let modified_ns = metadata
            .modified()
            .ok()
            .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|value| value.as_nanos())
            .unwrap_or(0);
        // Length and timestamp alone miss an adversarial same-size rewrite on coarse or
        // restored mtimes. Canonical archive text is the rebuild authority, so bind the CAS
        // snapshot to its exact bytes.
        let mut file = std::fs::File::open(&archive)?;
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
        generation.push((
            subscription_id,
            metadata.len(),
            modified_ns,
            hex::encode(hasher.finalize()),
        ));
    }
    generation.sort();
    Ok(generation)
}

fn compute_youtube_subscriptions_archive_stats(paths: &AppPaths) -> Result<HashMap<String, usize>> {
    let state_dir = paths.youtube_subscription_state_dir();
    let entries = match std::fs::read_dir(&state_dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(HashMap::new()),
        Err(err) => return Err(err.into()),
    };
    let mut stats = HashMap::new();
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let id = match entry.file_name().into_string() {
            Ok(value) => value,
            Err(_) => continue,
        };
        let archive_path = entry.path().join(YT_DLP_ARCHIVE_FILENAME);
        let count = if archive_path.is_file() {
            read_archive_file_ids(&archive_path)
                .map(|ids| ids.len())
                .unwrap_or(0)
        } else {
            0
        };
        stats.insert(id, count);
    }
    Ok(stats)
}

/// WP-0261: live per-subscription activity for the consumer "Processing now" signal.
/// Read-only + lean: derives phase + child-download counts from the job table only (joined by
/// batch_id = the refresh job id), so it stays off the writer path and does NOT call the slow
/// filesystem archive_stats — the UI already has downloaded/total/title from its other polls.
#[derive(Debug, Clone, Serialize)]
pub struct SubscriptionActivityRow {
    pub subscription_id: String,
    pub phase: String, // "checking" | "downloading" | "idle"
    pub queued: i64,
    pub running: i64,
    pub succeeded: i64,
    pub failed: i64,
    pub current_title: Option<String>,
    pub current_progress: Option<f32>,
}

pub fn youtube_subscriptions_activity(paths: &AppPaths) -> Result<Vec<SubscriptionActivityRow>> {
    let conn = db::open_readonly(paths)?;

    // WP-0257 pacing follow-up: only report subscriptions whose refresh is ACTUALLY running.
    // "Update all" enqueues the whole batch as `queued` refresh jobs, but the recurring lane
    // dispatches them one at a time behind the anti-bot cooldown. Including `queued` refreshes
    // here made every waiting subscription render as "Checking … for new videos", so the
    // "processing now" list flooded with hundreds of rows while only one channel was live.
    // Restricting to `running` keeps this list to the handful actually being checked.
    //
    // Running refresh jobs -> (subscription_id, refresh_job_id). The refresh job id is the
    // batch_id shared by its fanned-out child download jobs.
    let mut stmt = conn.prepare(
        "SELECT id, params_json FROM job \
         WHERE type = 'youtube_subscription_refresh_v1' AND status = 'running'",
    )?;
    let refreshers: Vec<(String, String)> = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .filter_map(|(job_id, params)| {
            serde_json::from_str::<serde_json::Value>(&params)
                .ok()
                .and_then(|v| {
                    v.get("subscription_id")
                        .and_then(|s| s.as_str())
                        .map(String::from)
                })
                .map(|sub_id| (sub_id, job_id))
        })
        .collect();
    drop(stmt);

    let mut out = Vec::new();
    for (sub_id, refresh_job_id) in refreshers {
        let mut cnt = conn.prepare(
            "SELECT status, COUNT(*) FROM job \
             WHERE type = 'download_direct_url' AND batch_id = ?1 GROUP BY status",
        )?;
        let counts = cnt
            .query_map(params![refresh_job_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(cnt);

        let (mut queued, mut running, mut succeeded, mut failed) = (0_i64, 0_i64, 0_i64, 0_i64);
        for (status, c) in counts {
            match status.as_str() {
                "queued" => queued += c,
                "running" => running += c,
                "succeeded" => succeeded += c,
                "failed" | "canceled" => failed += c,
                _ => {}
            }
        }
        let child_total = queued + running + succeeded + failed;

        let mut inflight = conn.prepare(
            "SELECT target_title, progress FROM job \
             WHERE type = 'download_direct_url' AND batch_id = ?1 AND status = 'running' \
             ORDER BY started_at_ms DESC LIMIT 1",
        )?;
        let (current_title, current_progress) = inflight
            .query_row(params![refresh_job_id], |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<f32>>(1)?,
                ))
            })
            .optional()?
            .unwrap_or((None, None));
        drop(inflight);

        let phase = if running > 0 || queued > 0 {
            "downloading"
        } else if child_total == 0 {
            "checking"
        } else {
            "idle"
        };

        out.push(SubscriptionActivityRow {
            subscription_id: sub_id,
            phase: phase.to_string(),
            queued,
            running,
            succeeded,
            failed,
            current_title,
            current_progress,
        });
    }

    Ok(out)
}

/// Parse the `subscription_id` carried in a `youtube_subscription_refresh_v1` job's params.
fn subscription_id_from_refresh_params(params_json: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(params_json)
        .ok()
        .and_then(|v| {
            v.get("subscription_id")
                .and_then(|s| s.as_str())
                .map(String::from)
        })
}

/// WP-0257 pacing follow-up: READ-ONLY per-subscription DOWNLOAD activity.
///
/// `youtube_subscriptions_activity` is keyed off *running refresh* jobs, so it goes blank the
/// moment a channel finishes enumerating — even while that channel's newly-found videos are still
/// downloading. This command reports refresh batches that still contain queued or running
/// `download_direct_url` jobs (including batches whose refresh job already completed), so the UI
/// can keep showing "N waiting / M downloading · <title>" after enumeration ends. Succeeded and
/// failed counts remain included for those active drain batches; fully terminal historical batches
/// are deliberately outside this live-activity projection.
///
/// Child downloads are joined back to their subscription via `batch_id = refresh_job_id` (the
/// refresh job carries `subscription_id` in its params). Non-subscription one-off/playlist
/// downloads have a `batch_id` that does not resolve to a refresh job and are naturally excluded.
///
/// Safety: single read-only connection (`db::open_readonly`, never the writer path); counts are
/// aggregated in SQL so memory is bounded by the subscription count rather than the download
/// backlog; the title lookup is `LIMIT`-capped. It cannot lock the DB or grow unbounded.
#[derive(Debug, Clone, Serialize)]
pub struct SubscriptionDownloadActivityRow {
    pub subscription_id: String,
    pub queued: i64,
    pub running: i64,
    pub succeeded: i64,
    pub failed: i64,
    pub current_title: Option<String>,
    pub current_progress: Option<f32>,
}

fn compute_subscription_download_activity(
    paths: &AppPaths,
) -> Result<Vec<SubscriptionDownloadActivityRow>> {
    let conn = db::open_readonly(paths)?;

    // First identify only batches that still have queued/running child downloads, then aggregate
    // every status inside those active drains. The prior query joined and grouped the entire direct
    // download history on every page entry (2.06s on the operator's 775MB DB). This shape uses the
    // existing `(type,status,...)` and `(batch_id,created_at_ms)` indexes and measured ~0.40s while
    // preserving queued/running plus done/failed progress for current drains.
    let mut stmt = conn.prepare(
        "WITH active_batches AS MATERIALIZED ( \
           SELECT DISTINCT batch_id \
           FROM job \
           WHERE type = 'download_direct_url' \
             AND status IN ('queued', 'running') \
             AND batch_id IS NOT NULL \
         ) \
         SELECT r.params_json, d.status, COUNT(*) \
         FROM active_batches a \
         JOIN job r ON r.id = a.batch_id \
           AND r.type = 'youtube_subscription_refresh_v1' \
         JOIN job d ON d.batch_id = a.batch_id \
           AND d.type = 'download_direct_url' \
         GROUP BY r.id, d.status",
    )?;
    let count_rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);

    // Preserve first-seen order so the output is stable for the polling UI.
    let mut order: Vec<String> = Vec::new();
    let mut agg: HashMap<String, (i64, i64, i64, i64)> = HashMap::new();
    for (params_json, status, count) in count_rows {
        let Some(sub_id) = subscription_id_from_refresh_params(&params_json) else {
            continue;
        };
        let entry = agg.entry(sub_id.clone()).or_insert_with(|| {
            order.push(sub_id.clone());
            (0, 0, 0, 0)
        });
        match status.as_str() {
            "queued" => entry.0 += count,
            "running" => entry.1 += count,
            "succeeded" => entry.2 += count,
            "failed" | "canceled" => entry.3 += count,
            _ => {}
        }
    }

    // Current running title + progress per subscription (latest-started wins). LIMIT-bounded so a
    // huge in-flight set never blows up the payload; recurring lane keeps this small.
    let mut title_stmt = conn.prepare(
        "SELECT r.params_json, d.target_title \
         , d.progress \
         FROM job d \
         JOIN job r ON r.id = d.batch_id \
         WHERE d.type = 'download_direct_url' \
         AND d.status = 'running' \
           AND r.type = 'youtube_subscription_refresh_v1' \
         ORDER BY d.started_at_ms DESC \
         LIMIT 200",
    )?;
    let title_rows = title_stmt
        .query_map([], |row| {
            let params_json = row.get::<_, String>(0)?;
            let title = row.get::<_, Option<String>>(1)?;
            let progress = row.get::<_, Option<f32>>(2)?;
            Ok((params_json, title, progress))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(title_stmt);

    let mut titles: HashMap<String, String> = HashMap::new();
    let mut current_progress_by_sub_id: HashMap<String, f32> = HashMap::new();
    for (params_json, title, progress) in title_rows {
        let Some(sub_id) = subscription_id_from_refresh_params(&params_json) else {
            continue;
        };
        if let Some(t) = title.filter(|t| !t.trim().is_empty()) {
            titles.entry(sub_id.clone()).or_insert(t);
        }
        if let Some(p) = progress.filter(|value| value.is_finite()) {
            current_progress_by_sub_id.entry(sub_id).or_insert(p);
        }
    }

    let mut out = Vec::with_capacity(order.len());
    for sub_id in order {
        let (queued, running, succeeded, failed) =
            agg.get(&sub_id).copied().unwrap_or((0, 0, 0, 0));
        let current_title = titles.get(&sub_id).cloned();
        let current_progress = current_progress_by_sub_id.get(&sub_id).copied();
        out.push(SubscriptionDownloadActivityRow {
            subscription_id: sub_id,
            queued,
            running,
            succeeded,
            failed,
            current_title,
            current_progress,
        });
    }
    Ok(out)
}

/// Event-path projection update for one canonical job mutation. This keeps the polling surface
/// current without turning each poll into a full-history rebuild; the global dirty bit remains a
/// repair signal until the debounced canonical reconciler proves a stable generation.
pub(crate) fn refresh_subscription_activity_rollup_for_job(
    paths: &AppPaths,
    job_id: &str,
) -> Result<()> {
    let mut conn = db::open(paths)?;
    db::migrate(&conn)?;
    let subscription_id: Option<String> = conn
        .query_row(
            "SELECT COALESCE(json_extract(j.params_json,'$.subscription_id'), json_extract(parent.params_json,'$.subscription_id')) FROM job j LEFT JOIN job parent ON parent.id=j.batch_id WHERE j.id=?1",
            [job_id],
            |row| row.get(0),
        )
        .optional()?
        .flatten();
    let Some(subscription_id) = subscription_id.filter(|value| !value.trim().is_empty()) else {
        return Ok(());
    };
    // Acquire the writer reservation before taking the canonical snapshot. Otherwise an older
    // caller can read counts, pause, and overwrite a newer caller's projection after it commits.
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let counts: (i64, i64, i64, i64) = tx.query_row(
        "WITH active_batches AS MATERIALIZED (SELECT DISTINCT batch_id FROM job WHERE type='download_direct_url' AND status IN ('queued','running') AND batch_id IS NOT NULL AND json_extract(params_json,'$.subscription_id')=?1) SELECT COALESCE(SUM(status='queued'),0),COALESCE(SUM(status='running'),0),COALESCE(SUM(status='succeeded'),0),COALESCE(SUM(status IN ('failed','canceled')),0) FROM job WHERE type='download_direct_url' AND batch_id IN (SELECT batch_id FROM active_batches)",
        [&subscription_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?;
    let current: Option<(Option<String>, Option<f32>)> = tx
        .query_row(
            "SELECT target_title,progress FROM job WHERE type='download_direct_url' AND status='running' AND json_extract(params_json,'$.subscription_id')=?1 ORDER BY started_at_ms DESC LIMIT 1",
            [&subscription_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    if counts.0 + counts.1 == 0 {
        tx.execute(
            "DELETE FROM subscription_activity_rollup WHERE subscription_id=?1",
            [&subscription_id],
        )?;
    } else {
        tx.execute(
            "INSERT INTO subscription_activity_rollup(subscription_id,queued,running,succeeded,failed,current_title,current_progress,rebuilt_at_ms,source) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,'job_event') ON CONFLICT(subscription_id) DO UPDATE SET queued=excluded.queued,running=excluded.running,succeeded=excluded.succeeded,failed=excluded.failed,current_title=excluded.current_title,current_progress=excluded.current_progress,rebuilt_at_ms=excluded.rebuilt_at_ms,source=excluded.source",
            params![subscription_id, counts.0, counts.1, counts.2, counts.3, current.as_ref().and_then(|value| value.0.clone()), current.and_then(|value| value.1), now_ms()],
        )?;
    }
    tx.execute(
        "UPDATE derived_projection_state SET dirty=1,updated_at_ms=updated_at_ms+1 WHERE projection='subscription_activity'",
        [],
    )?;
    tx.commit()?;
    Ok(())
}

pub fn rebuild_subscription_activity_rollup(
    paths: &AppPaths,
) -> Result<Vec<SubscriptionDownloadActivityRow>> {
    for _ in 0..4 {
        let generation = db::open_readonly(paths)?.query_row("SELECT updated_at_ms FROM derived_projection_state WHERE projection='subscription_activity'", [], |r| r.get::<_, i64>(0))?;
        let rows = compute_subscription_download_activity(paths)?;
        let mut conn = db::open(paths)?;
        db::migrate(&conn)?;
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let current_generation: i64 = tx.query_row("SELECT updated_at_ms FROM derived_projection_state WHERE projection='subscription_activity'", [], |r| r.get(0))?;
        if current_generation != generation {
            drop(tx);
            continue;
        }
        tx.execute("DELETE FROM subscription_activity_rollup", [])?;
        for row in &rows {
            tx.execute(
            "INSERT INTO subscription_activity_rollup(subscription_id,queued,running,succeeded,failed,current_title,current_progress,rebuilt_at_ms,source) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,'canonical_rebuild')",
            params![row.subscription_id, row.queued, row.running, row.succeeded, row.failed, row.current_title, row.current_progress, now_ms()],
        )?;
        }
        let cleared = tx.execute("UPDATE derived_projection_state SET dirty=0 WHERE projection='subscription_activity' AND updated_at_ms=?1", [generation])?;
        if cleared != 1 {
            drop(tx);
            continue;
        }
        tx.commit()?;
        return Ok(rows);
    }
    Err(EngineError::InstallFailed(
        "subscription activity rollup remained dirty during rebuild".into(),
    ))
}

pub fn subscription_download_activity(
    paths: &AppPaths,
) -> Result<Vec<SubscriptionDownloadActivityRow>> {
    // Like archive stats, this is a routine panel poll over a committed rollup. Dirty-state
    // inspection and row projection share one read-only connection; rebuilding stays on the
    // debounced background path and never turns the UI read into a writer.
    let conn = db::open_readonly(paths)?;
    // Return the last committed projection even when dirty. Full-history rebuilding belongs to
    // the explicit/startup reconciler and must never be spawned by a routine pane poll.
    let mut stmt = conn.prepare("SELECT subscription_id,queued,running,succeeded,failed,current_title,current_progress FROM subscription_activity_rollup ORDER BY subscription_id")?;
    let rows = stmt
        .query_map([], |row| {
            Ok(SubscriptionDownloadActivityRow {
                subscription_id: row.get(0)?,
                queued: row.get(1)?,
                running: row.get(2)?,
                succeeded: row.get(3)?,
                failed: row.get(4)?,
                current_title: row.get(5)?,
                current_progress: row.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
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
    ensure_subscription_is_not_deleted(sub)?;
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
        sub.preset_id.clone(),
    )?;

    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    conn.execute(
        "UPDATE youtube_subscription SET last_queued_at_ms = ?1, updated_at_ms = ?1 WHERE id = ?2",
        params![now_ms(), sub.id],
    )?;

    Ok(vec![queued])
}

fn ensure_subscription_is_not_deleted(sub: &YoutubeSubscriptionRow) -> Result<()> {
    if sub.source_status == YOUTUBE_SUBSCRIPTION_STATUS_DELETED {
        return Err(EngineError::InstallFailed(format!(
            "subscription is marked deleted and cannot be queued: {}",
            sub.id
        )));
    }
    Ok(())
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
    // WP-0252 Item 2b: read the cookie-secrets directory ONCE into a set instead of one
    // filesystem stat per subscription (was 255 syscalls on the subscription-list command
    // path, a measured contributor to list latency).
    let cookie_files: std::collections::HashSet<String> =
        std::fs::read_dir(paths.youtube_subscription_secrets_dir())
            .map(|entries| {
                entries
                    .flatten()
                    .filter_map(|entry| entry.file_name().into_string().ok())
                    .collect()
            })
            .unwrap_or_default();
    for row in rows.iter_mut() {
        row.auth_session_configured = cookie_files.contains(&format!("{}.cookie.txt", row.id));
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
    if let Some(value) = auth_session_input
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
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
  library_id,
  browser_cookie_source,
  use_browser_cookies,
  active,
  preset_id,
  refresh_interval_minutes,
  last_queued_at_ms,
  last_error_at_ms,
  consecutive_failures,
  next_allowed_refresh_at_ms,
  created_at_ms,
  updated_at_ms,
  source_status,
  source_status_changed_at_ms,
  source_status_change_source
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
  library_id,
  browser_cookie_source,
  use_browser_cookies,
  active,
  preset_id,
  refresh_interval_minutes,
  last_queued_at_ms,
  last_error_at_ms,
  consecutive_failures,
  next_allowed_refresh_at_ms,
  created_at_ms,
  updated_at_ms,
  source_status,
  source_status_changed_at_ms,
  source_status_change_source
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
    let library_id = req
        .library_id
        .as_deref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let browser_cookie_source = normalize_subscription_browser_cookie_source(
        req.use_browser_cookies,
        req.browser_cookie_source.as_deref(),
    )?;
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
        library_id,
        use_browser_cookies: req.use_browser_cookies,
        browser_cookie_source,
        auth_session_input: jobs::normalize_auth_cookie(req.auth_session_input)?,
        clear_auth_session: req.clear_auth_session,
        active: req.active,
        preset_id,
        group_ids,
        refresh_interval_minutes: normalize_refresh_interval_minutes(req.refresh_interval_minutes),
    })
}

fn normalize_subscription_browser_cookie_source(
    use_browser_cookies: bool,
    source: Option<&str>,
) -> Result<Option<String>> {
    if !use_browser_cookies {
        return Ok(None);
    }
    jobs::normalize_browser_cookie_source(source)?.map_or_else(
        || {
            Err(EngineError::InstallFailed(
                "browser cookies are enabled, but no browser was selected. Choose Chrome, Firefox, Edge, or Opera.".to_string(),
            ))
        },
        |browser| Ok(Some(browser)),
    )
}

fn validate_video_library_id(conn: &rusqlite::Connection, library_id: Option<&str>) -> Result<()> {
    let Some(library_id) = library_id else {
        return Ok(());
    };
    let exists: Option<i64> = conn
        .query_row(
            "SELECT active FROM video_library WHERE id = ?1",
            params![library_id],
            |row| row.get(0),
        )
        .optional()?;
    if exists != Some(1) {
        return Err(EngineError::InstallFailed(format!(
            "video library not found or disabled: {library_id}"
        )));
    }
    Ok(())
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
    let is_supported = host == "youtu.be"
        || host == "youtube.com"
        || host.ends_with(".youtube.com")
        || host == "tiktok.com"
        || host.ends_with(".tiktok.com")
        || host == "instagram.com"
        || host.ends_with(".instagram.com")
        || host == "instagr.am";
    if !is_supported {
        return Err(EngineError::InstallFailed(
            "subscription URL must be a supported provider URL (YouTube, Instagram, or TikTok)"
                .to_string(),
        ));
    }

    parsed.set_fragment(None);
    Ok(parsed.to_string())
}

fn sanitize_folder_map(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch.is_control() || matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') {
            out.push('_');
        } else {
            out.push(ch);
        }
    }
    let mut trimmed = out
        .trim()
        .trim_matches(|ch| ch == '_' || ch == '.')
        .to_string();
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
        library_id: row.get(5)?,
        browser_cookie_source: row.get(6)?,
        use_browser_cookies: i64_to_bool(row.get::<_, i64>(7)?),
        auth_session_configured: false,
        active: i64_to_bool(row.get::<_, i64>(8)?),
        source_status: row.get(17)?,
        source_status_changed_at_ms: row.get(18)?,
        source_status_change_source: row.get(19)?,
        preset_id: row.get(9)?,
        refresh_interval_minutes: row.get(10)?,
        last_queued_at_ms: row.get(11)?,
        last_error_at_ms: row.get(12)?,
        // WP-0264: default None here; only the UI list + group queries select + populate
        // last_error_message (like the WP-0255 progress fields below), so the internal
        // queue/lookup SELECTs that share this mapper stay unchanged.
        last_error_message: None,
        consecutive_failures: row.get(13)?,
        next_allowed_refresh_at_ms: row.get(14)?,
        created_at_ms: row.get(15)?,
        updated_at_ms: row.get(16)?,
        // WP-0255: progress fields default to None here; only the UI list query
        // (list_youtube_subscriptions) selects + populates them, so the internal
        // queue/lookup SELECTs that share this mapper stay unchanged.
        last_checked_at_ms: None,
        upstream_total: None,
        last_new_found: None,
        last_refresh_queued: None,
        group_ids: Vec::new(),
    })
}

fn default_youtube_subscription_source_status() -> String {
    YOUTUBE_SUBSCRIPTION_STATUS_NORMAL.to_string()
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
    library_id: Option<String>,
    use_browser_cookies: bool,
    browser_cookie_source: Option<String>,
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

    fn seed_archive_subscription(paths: &AppPaths, subscription_id: &str) {
        let conn = crate::db::open(paths).expect("archive fixture db");
        crate::db::migrate(&conn).expect("archive fixture schema");
        conn.execute(
            "INSERT OR IGNORE INTO youtube_subscription(
               id,title,source_url,folder_map,active,created_at_ms,updated_at_ms
             ) VALUES(?1,?2,?3,?4,1,1,1)",
            params![
                subscription_id,
                format!("Archive fixture {subscription_id}"),
                format!("https://www.youtube.com/@{subscription_id}"),
                format!("map-{subscription_id}"),
            ],
        )
        .expect("seed archive subscription");
    }

    fn seed_committed_archive_carrier(
        paths: &AppPaths,
        subscription_id: &str,
        video_id: &str,
    ) -> (String, PathBuf) {
        seed_archive_subscription(paths, subscription_id);
        let archive = paths.youtube_subscription_archive_state_path(subscription_id);
        let intended = vec![video_id.to_string()];
        let intent_id = create_archive_merge_intent(
            paths,
            subscription_id,
            &archive,
            &archive_file_sha256(&archive).expect("source archive hash"),
            &intended,
        )
        .expect("archive intent");
        let carrier = archive_merge_journal_path(&archive);
        write_archive_merge_journal(&carrier, &intent_id).expect("archive carrier");
        persistence::atomic_write_text(&archive, &canonical_archive_body(&intended))
            .expect("archive publish");
        advance_archive_merge_intent_phase(paths, &intent_id, "prepared", "published")
            .expect("published phase");
        apply_archive_merge_projection(paths, &intent_id, subscription_id, &intended)
            .expect("committed projection");
        (intent_id, carrier)
    }

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
                library_id: None,
                use_browser_cookies: false,
                browser_cookie_source: None,
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
                    "browser_cookie_source": "firefox",
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
        assert_eq!(updated.browser_cookie_source.as_deref(), Some("firefox"));
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
                library_id: None,
                use_browser_cookies: false,
                browser_cookie_source: None,
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
                && output_dir.contains("mapped_channel")
                && !output_dir.contains("youtube"),
            "expected mapped subscription folder in output_dir, got {output_dir}"
        );
    }

    #[test]
    fn subscription_output_uses_bound_video_library_without_youtube_layer() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_state"));
        crate::db::ensure_schema(&paths).expect("schema");

        let library_root = dir.path().join("NAS Library");
        std::fs::create_dir_all(&library_root).expect("mkdir library");
        let library = crate::video_libraries::upsert_video_library(
            &paths,
            crate::video_libraries::VideoLibraryUpsert {
                id: None,
                name: "NAS".to_string(),
                root_path: library_root.to_string_lossy().to_string(),
                set_active: true,
            },
        )
        .expect("save library");

        let sub = upsert_youtube_subscription(
            &paths,
            YoutubeSubscriptionUpsert {
                id: None,
                title: "Weekly ZOA".to_string(),
                source_url: "https://www.youtube.com/@weekly/videos".to_string(),
                folder_map: Some("[[]] WEEEKLY [[]] (ZOA) [FANCAM]".to_string()),
                output_dir_override: None,
                library_id: None,
                use_browser_cookies: false,
                browser_cookie_source: None,
                auth_session_input: None,
                clear_auth_session: false,
                active: true,
                preset_id: None,
                group_ids: Vec::new(),
                refresh_interval_minutes: Some(DEFAULT_REFRESH_INTERVAL_MINUTES),
            },
        )
        .expect("upsert sub");
        let sub = set_youtube_subscription_library(&paths, &sub.id, Some(&library.id))
            .expect("bind library");

        let output_dir = youtube_subscription_output_dir(&paths, &sub).expect("output dir");
        assert_eq!(
            output_dir.file_name().and_then(|value| value.to_str()),
            Some("[[]] WEEEKLY [[]] (ZOA) [FANCAM]")
        );
        assert!(
            !output_dir
                .components()
                .any(|component| component.as_os_str().to_string_lossy() == "youtube"),
            "subscription output should not add a youtube folder layer"
        );
    }

    #[test]
    fn subscription_output_override_still_wins_over_bound_library() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_state"));
        crate::db::ensure_schema(&paths).expect("schema");
        let override_dir = dir.path().join("legacy_target");
        let library_root = dir.path().join("new_library");
        std::fs::create_dir_all(&override_dir).expect("mkdir override");
        std::fs::create_dir_all(&library_root).expect("mkdir library");
        let library = crate::video_libraries::upsert_video_library(
            &paths,
            crate::video_libraries::VideoLibraryUpsert {
                id: None,
                name: "New library".to_string(),
                root_path: library_root.to_string_lossy().to_string(),
                set_active: true,
            },
        )
        .expect("save library");
        let sub = upsert_youtube_subscription(
            &paths,
            YoutubeSubscriptionUpsert {
                id: None,
                title: "Pinned legacy".to_string(),
                source_url: "https://www.youtube.com/@legacy/videos".to_string(),
                folder_map: Some("legacy".to_string()),
                output_dir_override: Some(override_dir.to_string_lossy().to_string()),
                library_id: None,
                use_browser_cookies: false,
                browser_cookie_source: None,
                auth_session_input: None,
                clear_auth_session: false,
                active: true,
                preset_id: None,
                group_ids: Vec::new(),
                refresh_interval_minutes: Some(DEFAULT_REFRESH_INTERVAL_MINUTES),
            },
        )
        .expect("upsert sub");
        let sub = set_youtube_subscription_library(&paths, &sub.id, Some(&library.id))
            .expect("bind library");

        assert_eq!(
            youtube_subscription_output_dir(&paths, &sub).expect("output dir"),
            override_dir
        );
    }

    #[test]
    fn preview_subscription_output_reports_existing_library_target() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_state"));
        crate::db::ensure_schema(&paths).expect("schema");

        let library_root = dir.path().join("4K Video 21-08-2025");
        let existing = library_root.join("Existing Channel");
        std::fs::create_dir_all(&existing).expect("existing channel dir");
        let library = crate::video_libraries::upsert_video_library(
            &paths,
            crate::video_libraries::VideoLibraryUpsert {
                id: None,
                name: "NAS".to_string(),
                root_path: library_root.to_string_lossy().to_string(),
                set_active: true,
            },
        )
        .expect("save library");

        let preview = preview_youtube_subscription_output_dir(
            &paths,
            YoutubeSubscriptionOutputPreviewRequest {
                title: "Existing Channel".to_string(),
                source_url: "https://www.youtube.com/@existing/videos".to_string(),
                folder_map: Some("Existing Channel".to_string()),
                output_dir_override: None,
                library_id: Some(library.id),
            },
        )
        .expect("preview");

        assert_eq!(
            PathBuf::from(&preview.path),
            existing.canonicalize().expect("canonical existing")
        );
        assert!(preview.exists);
        assert!(!preview.uses_output_override);
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
                library_id: None,
                use_browser_cookies: false,
                browser_cookie_source: None,
                auth_session_input: Some(r#"[{"name":"SAPISID","value":"cookie123"}]"#.to_string()),
                clear_auth_session: false,
                active: true,
                preset_id: None,
                group_ids: Vec::new(),
                refresh_interval_minutes: Some(DEFAULT_REFRESH_INTERVAL_MINUTES),
            },
        )
        .expect("upsert");

        assert!(
            sub.auth_session_configured,
            "saved auth session should be surfaced"
        );
        let stored = jobs::read_auth_cookie_secret_path(
            &paths.youtube_subscription_cookie_secret_path(&sub.id),
        )
        .expect("subscription auth secret");
        assert_eq!(stored, "SAPISID=cookie123");

        let queued = queue_youtube_subscription(&paths, &sub.id).expect("queue");
        let job_secret =
            jobs::read_auth_cookie_secret_path(&paths.job_cookie_secret_path(&queued[0].id))
                .expect("job auth secret");
        assert_eq!(job_secret, "SAPISID=cookie123");
    }

    #[test]
    fn upsert_persists_browser_cookie_source_for_recurring_refresh() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        crate::db::ensure_schema(&paths).expect("schema");

        let sub = upsert_youtube_subscription(
            &paths,
            YoutubeSubscriptionUpsert {
                id: None,
                title: "Browser source".to_string(),
                source_url: "https://www.youtube.com/@browser/videos".to_string(),
                folder_map: None,
                output_dir_override: None,
                library_id: None,
                use_browser_cookies: true,
                browser_cookie_source: Some("firefox".to_string()),
                auth_session_input: None,
                clear_auth_session: false,
                active: true,
                preset_id: None,
                group_ids: Vec::new(),
                refresh_interval_minutes: Some(DEFAULT_REFRESH_INTERVAL_MINUTES),
            },
        )
        .expect("upsert");

        assert_eq!(sub.browser_cookie_source.as_deref(), Some("firefox"));
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
                library_id: None,
                use_browser_cookies: false,
                browser_cookie_source: None,
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
                library_id: None,
                use_browser_cookies: false,
                browser_cookie_source: None,
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
                library_id: None,
                use_browser_cookies: false,
                browser_cookie_source: None,
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
                library_id: None,
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

    fn seed_4kvdp_download_evidence(
        sqlite_path: &Path,
        filename: &str,
        source_url: &str,
    ) -> rusqlite::Result<()> {
        let conn = rusqlite::Connection::open(sqlite_path)?;
        conn.execute_batch(
            r#"
CREATE TABLE IF NOT EXISTS download_item (
  id INTEGER PRIMARY KEY,
  filename TEXT,
  state INTEGER,
  position INTEGER,
  timestampNs BIGINT
);
CREATE TABLE IF NOT EXISTS media_item_description (
  download_item_id INTEGER NOT NULL,
  id INTEGER PRIMARY KEY,
  item_index INTEGER,
  title TEXT,
  duration INTEGER,
  publishing_timestamp INTEGER
);
CREATE TABLE IF NOT EXISTS url_description (
  media_item_description_id INTEGER NOT NULL,
  id INTEGER PRIMARY KEY,
  type INTEGER,
  service_name TEXT,
  url TEXT,
  handler_name TEXT
);
"#,
        )?;
        conn.execute(
            "INSERT INTO download_item (id, filename, state, position, timestampNs) VALUES (10, ?1, 1, 0, 0)",
            [filename],
        )?;
        conn.execute(
            "INSERT INTO media_item_description (download_item_id, id, item_index, title, duration, publishing_timestamp) VALUES (10, 20, 0, 'Exact video', 1000, 0)",
            [],
        )?;
        conn.execute(
            "INSERT INTO url_description (media_item_description_id, id, type, service_name, url, handler_name) VALUES (20, 30, 1, 'youtube', ?1, 'youtube')",
            [source_url],
        )?;
        Ok(())
    }

    #[test]
    fn import_path_normalization_matches_extended_unc_without_changing_structure() {
        assert_eq!(
            normalize_import_match_path(r"\\?\UNC\MediaNas\Videos\Clip.mp4"),
            normalize_import_match_path(r"\\MediaNas\Videos\Clip.mp4")
        );
        assert_eq!(
            normalize_import_match_path(r"\\?\C:\Archive\Clip.mp4"),
            normalize_import_match_path(r"C:\Archive\Clip.mp4")
        );
    }

    #[test]
    fn imported_identity_enrichment_links_exact_only_and_preserves_unresolved() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        crate::db::ensure_schema(&paths).expect("schema");
        let sqlite_path = dir.path().join("state.sqlite");
        let root = dir.path().join("imported_root");
        std::fs::create_dir_all(&root).expect("root");
        seed_legacy_4kvdp_state_db(&sqlite_path, &root).expect("state db");
        seed_4kvdp_download_evidence(
            &sqlite_path,
            r"\\?\UNC\MediaNas\Videos\Exact.mp4",
            "https://www.youtube.com/watch?v=EXACT123456",
        )
        .expect("download evidence");

        let conn = crate::db::open(&paths).expect("open app db");
        crate::db::migrate(&conn).expect("migrate app db");
        conn.execute(
            "INSERT INTO library_item (id, created_at_ms, source_type, source_uri, title, media_path, origin) VALUES ('exact-item', 1, 'local_file', ?1, 'Exact', ?1, '4kvdp_import')",
            [r"\\MediaNas\Videos\Exact.mp4"],
        )
        .expect("exact item");
        conn.execute(
            "INSERT INTO library_item (id, created_at_ms, source_type, source_uri, title, media_path, origin) VALUES ('unresolved-item', 2, 'local_file', ?1, 'Unknown', ?1, '4kvdp_import')",
            [r"\\MediaNas\Videos\Unknown.mp4"],
        )
        .expect("unresolved item");
        drop(conn);

        let summary =
            enrich_imported_youtube_identity_4kvdp(&paths, Some(&sqlite_path), false, None)
                .expect("enrich");
        assert_eq!(summary.scanned_items, 2);
        assert_eq!(summary.exact_items, 1);
        assert_eq!(summary.unresolved_items, 1);
        assert_eq!(summary.ambiguous_items, 0);

        let conn = crate::db::open_readonly(&paths).expect("readonly");
        let linked: Option<String> = conn
            .query_row(
                "SELECT library_item_id FROM media_source_identity WHERE service='youtube' AND media_id='EXACT123456'",
                [],
                |row| row.get(0),
            )
            .expect("identity");
        assert_eq!(linked.as_deref(), Some("exact-item"));
        let unresolved_state: String = conn
            .query_row(
                "SELECT match_state FROM media_import_evidence WHERE library_item_id='unresolved-item'",
                [],
                |row| row.get(0),
            )
            .expect("unresolved evidence");
        assert_eq!(unresolved_state, "unresolved");
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
        assert_eq!(summary.source_memberships_added, 2);

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
        let conn = crate::db::open_readonly(&paths).expect("membership db");
        let membership_kinds = conn
            .prepare("SELECT source_kind FROM media_source_membership ORDER BY source_kind ASC")
            .expect("membership query")
            .query_map([], |row| row.get::<_, String>(0))
            .expect("membership rows")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("membership collect");
        assert_eq!(
            membership_kinds,
            vec!["playlist".to_string(), "videos_page".to_string()]
        );
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
                library_id: None,
                use_browser_cookies: false,
                browser_cookie_source: None,
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
    fn ensure_archive_state_reconciles_existing_folder_media_files_and_memberships() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        crate::db::ensure_schema(&paths).expect("schema");

        let output_dir = dir.path().join("mapped_sub_folder");
        std::fs::create_dir_all(&output_dir).expect("mkdir output dir");

        // Seed some media files with video IDs in brackets and suffixes
        std::fs::write(output_dir.join("Episode 1 [dQw4w9WgXcQ].mkv"), b"video1")
            .expect("seed file 1");
        std::fs::write(output_dir.join("Episode 2 - 5NV6Rdv1a3I.mp4"), b"video2")
            .expect("seed file 2");
        std::fs::write(output_dir.join("Loose Note.txt"), b"note").expect("seed note");

        let sub = upsert_youtube_subscription(
            &paths,
            YoutubeSubscriptionUpsert {
                id: None,
                title: "Folder Recon Sub".to_string(),
                source_url: "https://www.youtube.com/@recon/videos".to_string(),
                folder_map: Some("mapped_sub_folder".to_string()),
                output_dir_override: Some(output_dir.to_string_lossy().to_string()),
                library_id: None,
                use_browser_cookies: false,
                browser_cookie_source: None,
                auth_session_input: None,
                clear_auth_session: false,
                active: true,
                preset_id: None,
                group_ids: Vec::new(),
                refresh_interval_minutes: Some(DEFAULT_REFRESH_INTERVAL_MINUTES),
            },
        )
        .expect("upsert sub");

        // Insert identity and membership in DB
        let conn = crate::db::open(&paths).expect("db open");
        conn.execute(
            "INSERT INTO media_source_identity (service, media_id, canonical_url, created_at_ms, updated_at_ms) VALUES ('youtube', 'MEMB1111AAA', 'https://www.youtube.com/watch?v=MEMB1111AAA', 1, 1)",
            [],
        ).expect("insert identity");
        conn.execute(
            "INSERT INTO media_source_membership (service, media_id, source_subscription_id, source_kind, source_url_snapshot, source_title_snapshot, evidence_kind, created_at_ms, updated_at_ms) VALUES ('youtube', 'MEMB1111AAA', ?1, 'videos_page', 'https://youtube.com', 'Title', 'runtime', 1, 1)",
            rusqlite::params![sub.id],
        ).expect("insert membership");

        let archive_path =
            ensure_youtube_subscription_archive_state(&paths, &sub).expect("ensure archive state");
        let archived_ids =
            load_youtube_subscription_archive_ids(&paths, &sub).expect("load archived ids");

        assert!(archive_path.is_file());
        assert!(archived_ids.contains("dQw4w9WgXcQ"));
        assert!(archived_ids.contains("5NV6Rdv1a3I"));
        assert!(archived_ids.contains("MEMB1111AAA"));
    }

    #[test]
    fn archive_stats_do_not_migrate_legacy_output_archives() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_state"));
        crate::db::ensure_schema(&paths).expect("schema");

        let legacy_output_dir = dir.path().join("legacy_output");
        std::fs::create_dir_all(&legacy_output_dir).expect("mkdir legacy output");
        let legacy_archive_path = legacy_output_dir.join(YT_DLP_ARCHIVE_FILENAME);
        std::fs::write(&legacy_archive_path, "youtube dQw4w9WgXcQ\n").expect("seed legacy archive");

        let sub = upsert_youtube_subscription(
            &paths,
            YoutubeSubscriptionUpsert {
                id: None,
                title: "Legacy stats sub".to_string(),
                source_url: "https://www.youtube.com/@legacy-stats/videos".to_string(),
                folder_map: Some("legacy_stats".to_string()),
                output_dir_override: Some(legacy_output_dir.to_string_lossy().to_string()),
                library_id: None,
                use_browser_cookies: false,
                browser_cookie_source: None,
                auth_session_input: None,
                clear_auth_session: false,
                active: true,
                preset_id: None,
                group_ids: Vec::new(),
                refresh_interval_minutes: Some(DEFAULT_REFRESH_INTERVAL_MINUTES),
            },
        )
        .expect("upsert sub");

        let managed_archive_path =
            youtube_subscription_archive_path(&paths, &sub).expect("archive path");
        assert!(!managed_archive_path.exists());

        let stats = youtube_subscriptions_archive_stats(&paths).expect("rollup stats");

        assert!(
            !stats.contains_key(&sub.id),
            "routine stats scan should omit DB-only subscriptions; the UI renders missing stats as zero"
        );
        assert!(
            !managed_archive_path.exists(),
            "routine stats refresh must not create or merge archive state"
        );
        assert!(
            legacy_archive_path.exists(),
            "legacy archive remains available for explicit queue/download migration"
        );
    }

    #[test]
    fn routine_projection_reads_do_not_schedule_or_clear_dirty_repair_state() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_state"));
        crate::db::ensure_schema(&paths).expect("schema");
        let conn = crate::db::open(&paths).expect("db");
        conn.execute(
            "INSERT INTO youtube_subscription_archive_rollup(subscription_id,video_count,rebuilt_at_ms,source) VALUES('stale-archive',7,1,'fixture')",
            [],
        )
        .expect("archive projection");
        conn.execute(
            "INSERT INTO subscription_activity_rollup(subscription_id,queued,running,succeeded,failed,current_title,current_progress,rebuilt_at_ms,source) VALUES('stale-activity',1,0,0,0,NULL,NULL,1,'fixture')",
            [],
        )
        .expect("activity projection");
        conn.execute(
            "UPDATE derived_projection_state SET dirty=1,updated_at_ms=401 WHERE projection='youtube_archive'",
            [],
        )
        .expect("dirty archive");
        conn.execute(
            "UPDATE derived_projection_state SET dirty=1,updated_at_ms=402 WHERE projection='subscription_activity'",
            [],
        )
        .expect("dirty activity");
        drop(conn);

        let archive = youtube_subscriptions_archive_stats(&paths).expect("archive poll");
        let activity = subscription_download_activity(&paths).expect("activity poll");
        assert_eq!(archive.get("stale-archive"), Some(&7));
        assert_eq!(activity.len(), 1);
        std::thread::sleep(std::time::Duration::from_millis(1_000));

        let conn = crate::db::open_readonly(&paths).expect("readonly verification");
        for (projection, expected_generation) in [
            ("youtube_archive", 401_i64),
            ("subscription_activity", 402_i64),
        ] {
            let state: (i64, i64) = conn
                .query_row(
                    "SELECT dirty,updated_at_ms FROM derived_projection_state WHERE projection=?1",
                    [projection],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .expect("projection state");
            assert_eq!(state, (1, expected_generation));
        }
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM youtube_subscription_archive_rollup",
                [],
                |row| row.get::<_, i64>(0),
            )
            .expect("archive rows"),
            1
        );
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM subscription_activity_rollup",
                [],
                |row| row.get::<_, i64>(0),
            )
            .expect("activity rows"),
            1
        );
    }

    #[test]
    fn archive_rollup_rebuild_counts_managed_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_state"));
        let subscription_id = "sub-managed-only";
        let archive_path = paths.youtube_subscription_archive_state_path(subscription_id);
        std::fs::create_dir_all(archive_path.parent().expect("archive parent")).expect("mkdir");
        std::fs::write(&archive_path, "youtube video-a\nvideo-b\n\n").expect("write archive");
        crate::db::ensure_schema(&paths).expect("schema");

        let stats = rebuild_youtube_subscription_archive_rollups(&paths).expect("rebuild stats");

        assert_eq!(stats.get(subscription_id), Some(&2));
        let projected = youtube_subscriptions_archive_stats(&paths).expect("projected stats");
        assert_eq!(projected.get(subscription_id), Some(&2));
    }

    #[test]
    fn legacy_or_self_authored_archive_journal_cannot_authorize_projection() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_state"));
        crate::db::ensure_schema(&paths).expect("schema");
        let subscription_id = "sub-crash-replay";
        seed_archive_subscription(&paths, subscription_id);
        let archive = paths.youtube_subscription_archive_state_path(subscription_id);
        std::fs::create_dir_all(archive.parent().expect("archive parent")).expect("mkdir");
        std::fs::write(&archive, "youtube first\nyoutube second\n").expect("published bytes");
        let journal = archive_merge_journal_path(&archive);
        let forged = serde_json::json!({
            "schema_version": 1,
            "subscription_id": subscription_id,
            "intended_video_ids": ["forged"],
            "intended_archive_sha256": archive_body_sha256("youtube forged\n"),
        });
        persistence::atomic_write_text(&journal, &forged.to_string()).expect("forged journal");
        let conn = crate::db::open(&paths).expect("db");
        conn.execute(
            "INSERT INTO youtube_subscription_archive_rollup(subscription_id,video_count,rebuilt_at_ms,source) VALUES(?1,1,1,'stale_fixture')",
            [subscription_id],
        )
        .expect("stale rollup");
        drop(conn);

        assert_eq!(
            reconcile_youtube_archive_merge_journals(&paths).expect("reconcile"),
            0
        );
        assert!(
            journal.exists(),
            "untrusted legacy evidence remains available for diagnosis"
        );
        assert_eq!(
            youtube_subscriptions_archive_stats(&paths)
                .expect("stats")
                .get(subscription_id),
            Some(&1),
            "editable journal content must not alter the trusted DB projection"
        );
    }

    #[test]
    fn malformed_oldest_archive_intent_is_quarantined_while_healthy_later_recovers() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_state"));
        crate::db::ensure_schema(&paths).expect("schema");
        seed_archive_subscription(&paths, "sub-malformed");
        seed_archive_subscription(&paths, "sub-healthy");

        let malformed_archive = paths.youtube_subscription_archive_state_path("sub-malformed");
        std::fs::create_dir_all(malformed_archive.parent().unwrap()).unwrap();
        let malformed_intent = Uuid::new_v4().to_string();
        let conn = crate::db::open(&paths).unwrap();
        conn.execute(
            "INSERT INTO youtube_archive_merge_intent(
               intent_id,subscription_id,target_archive_path,source_archive_sha256,
               intended_archive_sha256,intended_video_ids_json,phase,created_at_ms,updated_at_ms
             ) VALUES(?1,'sub-malformed',?2,?3,?4,'not-json','prepared',1,1)",
            params![
                malformed_intent,
                malformed_archive.to_string_lossy(),
                archive_file_sha256(&malformed_archive).unwrap(),
                archive_body_sha256("youtube never-trusted\n"),
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO youtube_subscription_archive_rollup(subscription_id,video_count,rebuilt_at_ms,source)
             VALUES('sub-malformed',7,1,'last_trusted')",
            [],
        )
        .unwrap();
        drop(conn);
        let malformed_carrier = archive_merge_journal_path(&malformed_archive);
        write_archive_merge_journal(&malformed_carrier, &malformed_intent).unwrap();

        let healthy_archive = paths.youtube_subscription_archive_state_path("sub-healthy");
        let healthy_intended = vec!["healthy-video".to_string()];
        let healthy_intent = create_archive_merge_intent(
            &paths,
            "sub-healthy",
            &healthy_archive,
            &archive_file_sha256(&healthy_archive).unwrap(),
            &healthy_intended,
        )
        .unwrap();
        write_archive_merge_journal(
            &archive_merge_journal_path(&healthy_archive),
            &healthy_intent,
        )
        .unwrap();

        assert_eq!(reconcile_youtube_archive_merge_journals(&paths).unwrap(), 1);
        let conn = crate::db::open_readonly(&paths).unwrap();
        let malformed_phase: String = conn
            .query_row(
                "SELECT phase FROM youtube_archive_merge_intent WHERE intent_id=?1",
                [&malformed_intent],
                |row| row.get(0),
            )
            .unwrap();
        let healthy_phase: String = conn
            .query_row(
                "SELECT phase FROM youtube_archive_merge_intent WHERE intent_id=?1",
                [&healthy_intent],
                |row| row.get(0),
            )
            .unwrap();
        let failure: (String, i64, Option<i64>) = conn
            .query_row(
                "SELECT error_message,failure_count,resolved_at_ms
                 FROM youtube_archive_merge_intent_failure WHERE intent_id=?1",
                [&malformed_intent],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(malformed_phase, "prepared");
        assert_eq!(healthy_phase, "committed");
        assert!(failure.0.contains("members are invalid"));
        assert_eq!(failure.1, 1);
        assert_eq!(failure.2, None);
        assert!(malformed_carrier.is_file());
        assert!(!archive_merge_journal_path(&healthy_archive).exists());
        drop(conn);

        let rebuilt = rebuild_youtube_subscription_archive_rollups(&paths).unwrap();
        assert_eq!(rebuilt.get("sub-malformed"), Some(&7));
        assert_eq!(rebuilt.get("sub-healthy"), Some(&1));
    }

    #[test]
    fn bounded_archive_repair_page_does_not_starve_unattempted_later_intent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_state"));
        crate::db::ensure_schema(&paths).expect("schema");
        let mut conn = crate::db::open(&paths).expect("db");
        let tx = conn.transaction().expect("transaction");
        for index in 0..YOUTUBE_ARCHIVE_JOURNAL_REPLAY_LIMIT {
            let subscription_id = format!("sub-quarantined-{index:03}");
            let intent_id = Uuid::new_v4().to_string();
            let target = paths.youtube_subscription_archive_state_path(&subscription_id);
            tx.execute(
                "INSERT INTO youtube_subscription(
                   id,title,source_url,folder_map,active,created_at_ms,updated_at_ms
                 ) VALUES(?1,?2,?3,?4,1,1,1)",
                params![
                    subscription_id,
                    format!("Quarantined {index}"),
                    format!("https://www.youtube.com/@quarantined-{index}"),
                    format!("quarantined-{index}"),
                ],
            )
            .unwrap();
            tx.execute(
                "INSERT INTO youtube_archive_merge_intent(
                   intent_id,subscription_id,target_archive_path,source_archive_sha256,
                   intended_archive_sha256,intended_video_ids_json,phase,created_at_ms,updated_at_ms
                 ) VALUES(?1,?2,?3,?4,?5,'not-json','prepared',?6,?6)",
                params![
                    intent_id,
                    subscription_id,
                    target.to_string_lossy(),
                    archive_file_sha256(&target).unwrap(),
                    archive_body_sha256("youtube invalid\n"),
                    index as i64 + 1,
                ],
            )
            .unwrap();
            tx.execute(
                "INSERT INTO youtube_archive_merge_intent_failure(
                   intent_id,subscription_id,error_message,failure_count,
                   first_failed_at_ms,last_failed_at_ms,resolved_at_ms
                 ) VALUES(?1,?2,'already quarantined',1,1,1,NULL)",
                params![intent_id, subscription_id],
            )
            .unwrap();
        }
        let healthy_subscription = "sub-unattempted-later";
        let healthy_intent = Uuid::new_v4().to_string();
        let healthy_target = paths.youtube_subscription_archive_state_path(healthy_subscription);
        tx.execute(
            "INSERT INTO youtube_subscription(
               id,title,source_url,folder_map,active,created_at_ms,updated_at_ms
             ) VALUES(?1,'Healthy later','https://www.youtube.com/@healthy-later',
                      'healthy-later',1,1,1)",
            [healthy_subscription],
        )
        .unwrap();
        tx.execute(
            "INSERT INTO youtube_archive_merge_intent(
               intent_id,subscription_id,target_archive_path,source_archive_sha256,
               intended_archive_sha256,intended_video_ids_json,phase,created_at_ms,updated_at_ms
             ) VALUES(?1,?2,?3,?4,?5,?6,'prepared',999,999)",
            params![
                healthy_intent,
                healthy_subscription,
                healthy_target.to_string_lossy(),
                archive_file_sha256(&healthy_target).unwrap(),
                archive_body_sha256("youtube healthy\n"),
                serde_json::to_string(&vec!["healthy".to_string()]).unwrap(),
            ],
        )
        .unwrap();
        tx.commit().unwrap();

        let pending = load_pending_archive_merge_intent_rows(&paths).unwrap();
        assert_eq!(pending.len(), YOUTUBE_ARCHIVE_JOURNAL_REPLAY_LIMIT);
        assert_eq!(pending[0].intent_id, healthy_intent);
        assert!(pending
            .iter()
            .any(|row| row.subscription_id == healthy_subscription));
    }

    #[test]
    fn archive_hash_mismatch_isolated_requested_fail_closed_and_retry_after_repair() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_state"));
        crate::db::ensure_schema(&paths).expect("schema");
        seed_archive_subscription(&paths, "sub-damaged");
        seed_archive_subscription(&paths, "sub-unrelated");

        let damaged_archive = paths.youtube_subscription_archive_state_path("sub-damaged");
        std::fs::create_dir_all(damaged_archive.parent().unwrap()).unwrap();
        let trusted_source = "youtube original\n";
        std::fs::write(&damaged_archive, trusted_source).unwrap();
        let damaged_intended = vec!["original".to_string(), "pending".to_string()];
        let damaged_intent = create_archive_merge_intent(
            &paths,
            "sub-damaged",
            &damaged_archive,
            &archive_file_sha256(&damaged_archive).unwrap(),
            &damaged_intended,
        )
        .unwrap();
        let damaged_carrier = archive_merge_journal_path(&damaged_archive);
        write_archive_merge_journal(&damaged_carrier, &damaged_intent).unwrap();
        std::fs::write(&damaged_archive, "youtube external-change\n").unwrap();

        merge_youtube_archive_member(&paths, "sub-unrelated", "unrelated-video")
            .expect("unrelated merge must not run damaged global recovery");
        assert_eq!(
            read_archive_file_ids(&paths.youtube_subscription_archive_state_path("sub-unrelated"))
                .unwrap(),
            HashSet::from(["unrelated-video".to_string()])
        );

        let requested_error =
            merge_youtube_archive_member(&paths, "sub-damaged", "must-not-publish")
                .expect_err("the damaged requested subscription must fail closed");
        assert!(requested_error.to_string().contains("changed outside"));
        assert_eq!(
            std::fs::read_to_string(&damaged_archive).unwrap(),
            "youtube external-change\n"
        );
        assert!(damaged_carrier.is_file());
        let conn = crate::db::open_readonly(&paths).unwrap();
        let pending_member_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM youtube_subscription_archive_member
                 WHERE subscription_id='sub-damaged'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(pending_member_count, 0);
        drop(conn);

        std::fs::write(&damaged_archive, trusted_source).unwrap();
        merge_youtube_archive_member(&paths, "sub-damaged", "after-repair")
            .expect("trusted source restoration permits exact retry then requested merge");
        assert_eq!(
            read_archive_file_ids(&damaged_archive).unwrap(),
            HashSet::from([
                "original".to_string(),
                "pending".to_string(),
                "after-repair".to_string(),
            ])
        );
        assert!(!damaged_carrier.exists());
        let conn = crate::db::open_readonly(&paths).unwrap();
        let failure: (i64, Option<i64>) = conn
            .query_row(
                "SELECT failure_count,resolved_at_ms
                 FROM youtube_archive_merge_intent_failure WHERE intent_id=?1",
                [&damaged_intent],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert!(failure.0 >= 1);
        assert!(
            failure.1.is_some(),
            "successful retry resolves but retains failure evidence"
        );
    }

    #[test]
    fn archive_merge_crash_matrix_recovers_every_trusted_intent_phase() {
        for (carrier_written, file_was_published, phase_was_published) in [
            (false, false, false),
            (true, false, false),
            (true, true, false),
            (true, true, true),
        ] {
            let dir = tempfile::tempdir().expect("tempdir");
            let paths = AppPaths::new(dir.path().join("app_state"));
            crate::db::ensure_schema(&paths).expect("schema");
            let subscription_id = format!(
                "sub-crash-{}-{}-{}",
                carrier_written as u8, file_was_published as u8, phase_was_published as u8
            );
            seed_archive_subscription(&paths, &subscription_id);
            let archive = paths.youtube_subscription_archive_state_path(&subscription_id);
            std::fs::create_dir_all(archive.parent().expect("archive parent")).expect("mkdir");
            std::fs::write(&archive, "youtube first\n").expect("source archive");
            let intended = vec!["first".to_string(), "second".to_string()];
            let intent_id = create_archive_merge_intent(
                &paths,
                &subscription_id,
                &archive,
                &archive_file_sha256(&archive).unwrap(),
                &intended,
            )
            .expect("trusted intent");
            let journal = archive_merge_journal_path(&archive);
            if carrier_written {
                write_archive_merge_journal(&journal, &intent_id).expect("carrier");
            }
            if file_was_published {
                persistence::atomic_write_text(&archive, &canonical_archive_body(&intended))
                    .expect("published archive");
            }
            if phase_was_published {
                advance_archive_merge_intent_phase(&paths, &intent_id, "prepared", "published")
                    .expect("published phase");
            }

            assert_eq!(reconcile_youtube_archive_merge_journals(&paths).unwrap(), 1);
            assert_eq!(
                read_archive_file_ids(&archive).unwrap(),
                HashSet::from(["first".to_string(), "second".to_string()])
            );
            assert_eq!(
                youtube_subscriptions_archive_stats(&paths)
                    .unwrap()
                    .get(&subscription_id),
                Some(&2)
            );
            assert!(!journal.exists());
            let phase: String = crate::db::open_readonly(&paths)
                .unwrap()
                .query_row(
                    "SELECT phase FROM youtube_archive_merge_intent WHERE intent_id=?1",
                    [&intent_id],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(phase, "committed");
        }
    }

    #[test]
    fn committed_archive_intent_cleans_carrier_after_projection_crash() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_state"));
        crate::db::ensure_schema(&paths).expect("schema");
        seed_archive_subscription(&paths, "sub-after-commit");
        let archive = paths.youtube_subscription_archive_state_path("sub-after-commit");
        let intended = vec!["durable".to_string()];
        let intent_id = create_archive_merge_intent(
            &paths,
            "sub-after-commit",
            &archive,
            &archive_file_sha256(&archive).unwrap(),
            &intended,
        )
        .unwrap();
        let carrier = archive_merge_journal_path(&archive);
        write_archive_merge_journal(&carrier, &intent_id).unwrap();
        persistence::atomic_write_text(&archive, &canonical_archive_body(&intended)).unwrap();
        advance_archive_merge_intent_phase(&paths, &intent_id, "prepared", "published").unwrap();
        apply_archive_merge_projection(&paths, &intent_id, "sub-after-commit", &intended).unwrap();

        assert!(
            carrier.exists(),
            "fixture represents crash before carrier cleanup"
        );
        assert_eq!(reconcile_youtube_archive_merge_journals(&paths).unwrap(), 0);
        assert!(
            !carrier.exists(),
            "committed trusted carrier is cleaned idempotently"
        );
    }

    #[test]
    fn committed_carrier_cleanup_ignores_over_limit_untrusted_prefix_and_stays_bounded() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_state"));
        crate::db::ensure_schema(&paths).expect("schema");
        seed_archive_subscription(&paths, "sub-valid-after-junk");
        let archive = paths.youtube_subscription_archive_state_path("sub-valid-after-junk");
        let intended = vec!["durable-after-junk".to_string()];
        let intent_id = create_archive_merge_intent(
            &paths,
            "sub-valid-after-junk",
            &archive,
            &archive_file_sha256(&archive).unwrap(),
            &intended,
        )
        .unwrap();
        let carrier = archive_merge_journal_path(&archive);
        write_archive_merge_journal(&carrier, &intent_id).unwrap();
        persistence::atomic_write_text(&archive, &canonical_archive_body(&intended)).unwrap();
        advance_archive_merge_intent_phase(&paths, &intent_id, "prepared", "published").unwrap();
        apply_archive_merge_projection(&paths, &intent_id, "sub-valid-after-junk", &intended)
            .unwrap();

        let state_root = paths.youtube_subscription_state_dir();
        let mut junk_carriers = Vec::new();
        for index in 0..(YOUTUBE_ARCHIVE_JOURNAL_REPLAY_LIMIT + 17) {
            let junk_dir = state_root.join(format!("0000-untrusted-{index:04}"));
            std::fs::create_dir_all(&junk_dir).unwrap();
            let junk = junk_dir.join(YT_DLP_ARCHIVE_MERGE_JOURNAL_FILENAME);
            std::fs::write(&junk, format!("untrusted-{index}")).unwrap();
            junk_carriers.push(junk);
        }

        let candidates = load_committed_archive_carrier_cleanup_candidates(
            &paths,
            None,
            YOUTUBE_ARCHIVE_JOURNAL_REPLAY_LIMIT,
        )
        .unwrap();
        assert!(candidates.len() <= YOUTUBE_ARCHIVE_JOURNAL_REPLAY_LIMIT);
        assert_eq!(
            candidates,
            vec![(intent_id.clone(), "sub-valid-after-junk".into())]
        );
        assert_eq!(
            cleanup_committed_archive_merge_carriers(&paths).unwrap(),
            ArchiveCarrierCleanupReport {
                removed: 1,
                failed: 0,
            }
        );
        assert!(
            !carrier.exists(),
            "trusted committed carrier must not be starved by junk"
        );
        assert!(
            junk_carriers.iter().all(|path| path.exists()),
            "untrusted directories and malformed carriers must never be deleted"
        );
    }

    #[test]
    fn committed_carrier_cleanup_failure_does_not_starve_later_candidate_and_retries() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_state"));
        crate::db::ensure_schema(&paths).expect("schema");
        let first = seed_committed_archive_carrier(&paths, "sub-cleanup-a", "video-a");
        let second = seed_committed_archive_carrier(&paths, "sub-cleanup-b", "video-b");
        let ((failed_intent, failed_carrier), (healthy_intent, healthy_carrier)) =
            if first.0 < second.0 {
                (first, second)
            } else {
                (second, first)
            };
        let failed_canonical = std::fs::canonicalize(&failed_carrier).expect("failed carrier path");
        let mut injected_failure = |path: &Path| {
            if path == failed_canonical {
                Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "injected locked carrier",
                ))
            } else {
                std::fs::remove_file(path)
            }
        };

        let report = cleanup_committed_archive_merge_carriers_with(&paths, &mut injected_failure)
            .expect("cleanup isolates candidate failure");
        assert_eq!(
            report,
            ArchiveCarrierCleanupReport {
                removed: 1,
                failed: 1,
            }
        );
        assert!(
            failed_carrier.exists(),
            "failed carrier must remain retryable"
        );
        assert!(
            !healthy_carrier.exists(),
            "a failed earlier carrier must not starve a later healthy carrier"
        );
        let failure: (String, i64, Option<i64>) = crate::db::open_readonly(&paths)
            .unwrap()
            .query_row(
                "SELECT error_message,failure_count,resolved_at_ms
                 FROM youtube_archive_merge_intent_failure WHERE intent_id=?1",
                [&failed_intent],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("durable cleanup failure");
        assert!(failure
            .0
            .starts_with(YOUTUBE_ARCHIVE_CARRIER_CLEANUP_ERROR_PREFIX));
        assert_eq!(failure.1, 1);
        assert_eq!(failure.2, None);

        let retry = cleanup_committed_archive_merge_carriers(&paths).expect("cleanup retry");
        assert_eq!(retry.removed, 1);
        assert!(!failed_carrier.exists(), "repaired carrier must be retried");
        let resolved: Option<i64> = crate::db::open_readonly(&paths)
            .unwrap()
            .query_row(
                "SELECT resolved_at_ms FROM youtube_archive_merge_intent_failure WHERE intent_id=?1",
                [&failed_intent],
                |row| row.get(0),
            )
            .expect("resolved cleanup failure");
        assert!(resolved.is_some());
        assert_ne!(failed_intent, healthy_intent);
    }

    #[test]
    fn committed_carrier_cleanup_refuses_traversal_id_and_preserves_matching_outside_carrier() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_state"));
        crate::db::ensure_schema(&paths).expect("schema");
        let unsafe_subscription_id = "../../outside-subscription";
        seed_archive_subscription(&paths, unsafe_subscription_id);
        let intent_id = "00000000-0000-0000-0000-000000000001";
        let escaped_archive = paths.youtube_subscription_archive_state_path(unsafe_subscription_id);
        let outside_carrier = archive_merge_journal_path(&escaped_archive);
        write_archive_merge_journal(&outside_carrier, intent_id).expect("outside matching carrier");
        let conn = crate::db::open(&paths).expect("db");
        conn.execute(
            "INSERT INTO youtube_archive_merge_intent(
               intent_id,subscription_id,target_archive_path,source_archive_sha256,
               intended_archive_sha256,intended_video_ids_json,phase,created_at_ms,updated_at_ms
             ) VALUES(?1,?2,?3,'source','intended','[]','committed',1,1)",
            params![
                intent_id,
                unsafe_subscription_id,
                escaped_archive.to_string_lossy(),
            ],
        )
        .expect("direct SQL corrupt fixture");

        let report = cleanup_committed_archive_merge_carriers(&paths).expect("isolated cleanup");
        assert_eq!(
            report,
            ArchiveCarrierCleanupReport {
                removed: 0,
                failed: 1,
            }
        );
        assert!(
            outside_carrier.exists(),
            "unsafe DB identifier must never delete a matching carrier outside the state root"
        );
        let error_message: String = crate::db::open_readonly(&paths)
            .unwrap()
            .query_row(
                "SELECT error_message FROM youtube_archive_merge_intent_failure WHERE intent_id=?1",
                [intent_id],
                |row| row.get(0),
            )
            .expect("unsafe identifier receipt");
        assert!(error_message.contains("unsafe subscription identifier"));
    }

    #[test]
    fn committed_carrier_cleanup_large_history_uses_index_ordered_keyset_pages() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_state"));
        crate::db::ensure_schema(&paths).expect("schema");
        seed_archive_subscription(&paths, "sub-cleanup-plan");
        let mut conn = crate::db::open(&paths).expect("db");
        let tx = conn.transaction().expect("history tx");
        {
            let mut insert = tx
                .prepare(
                    "INSERT INTO youtube_archive_merge_intent(
                       intent_id,subscription_id,target_archive_path,source_archive_sha256,
                       intended_archive_sha256,intended_video_ids_json,phase,created_at_ms,updated_at_ms
                     ) VALUES(?1,'sub-cleanup-plan','archive','source','intended','[]','committed',?2,?2)",
                )
                .expect("insert statement");
            for index in 0..20_000_i64 {
                insert
                    .execute(params![format!("intent-{index:08}"), index])
                    .expect("committed history row");
            }
        }
        tx.commit().expect("history commit");

        for (sql, args) in [
            (
                "EXPLAIN QUERY PLAN SELECT intent_id,subscription_id
                 FROM youtube_archive_merge_intent
                 WHERE phase='committed'
                 ORDER BY intent_id ASC LIMIT ?1",
                vec![rusqlite::types::Value::Integer(128)],
            ),
            (
                "EXPLAIN QUERY PLAN SELECT intent_id,subscription_id
                 FROM youtube_archive_merge_intent
                 WHERE phase='committed' AND intent_id>?1
                 ORDER BY intent_id ASC LIMIT ?2",
                vec![
                    rusqlite::types::Value::Text("intent-00010000".into()),
                    rusqlite::types::Value::Integer(128),
                ],
            ),
        ] {
            let mut plan = conn.prepare(sql).expect("explain statement");
            let details = plan
                .query_map(rusqlite::params_from_iter(args), |row| {
                    row.get::<_, String>(3)
                })
                .expect("query plan")
                .collect::<rusqlite::Result<Vec<_>>>()
                .expect("plan details");
            let joined = details.join(" | ");
            assert!(
                joined.contains("idx_youtube_archive_merge_intent_phase_intent_cleanup"),
                "cleanup page must use its covering keyset index: {joined}"
            );
            assert!(
                !joined.contains("TEMP B-TREE"),
                "cleanup page must not sort committed history: {joined}"
            );
        }
        let first_page = load_committed_archive_carrier_cleanup_candidates(&paths, None, 128)
            .expect("first page");
        assert_eq!(first_page.len(), 128);
        let resumed_page = load_committed_archive_carrier_cleanup_candidates(
            &paths,
            Some(&first_page.last().unwrap().0),
            128,
        )
        .expect("resumed page");
        assert_eq!(resumed_page.len(), 128);
        assert!(resumed_page[0].0 > first_page.last().unwrap().0);
    }

    #[test]
    fn committed_carrier_cleanup_cursor_survives_reopen_and_reaches_later_carrier() {
        let dir = tempfile::tempdir().expect("tempdir");
        let base_dir = dir.path().join("app_state");
        let paths = AppPaths::new(base_dir.clone());
        crate::db::ensure_schema(&paths).expect("schema");
        let mut conn = crate::db::open(&paths).expect("db");
        let tx = conn.transaction().expect("history tx");
        for index in 0..=YOUTUBE_ARCHIVE_JOURNAL_REPLAY_LIMIT {
            let subscription_id = format!("restart-sub-{index:04}");
            let intent_id = format!("00000000-0000-0000-0000-{index:012}");
            tx.execute(
                "INSERT INTO youtube_subscription(
                   id,title,source_url,folder_map,active,created_at_ms,updated_at_ms
                 ) VALUES(?1,?2,?3,?4,1,1,1)",
                params![
                    subscription_id,
                    format!("Restart fixture {index}"),
                    format!("https://www.youtube.com/@restart-fixture-{index}"),
                    format!("restart-map-{index}"),
                ],
            )
            .expect("subscription row");
            tx.execute(
                "INSERT INTO youtube_archive_merge_intent(
                   intent_id,subscription_id,target_archive_path,source_archive_sha256,
                   intended_archive_sha256,intended_video_ids_json,phase,created_at_ms,updated_at_ms
                 ) VALUES(?1,?2,?3,'source','intended','[]','committed',?4,?4)",
                params![
                    intent_id,
                    subscription_id,
                    paths
                        .youtube_subscription_archive_state_path(&subscription_id)
                        .to_string_lossy(),
                    index as i64,
                ],
            )
            .expect("committed intent row");
        }
        tx.commit().expect("history commit");
        drop(conn);
        let later_subscription = format!("restart-sub-{:04}", YOUTUBE_ARCHIVE_JOURNAL_REPLAY_LIMIT);
        let later_intent = format!(
            "00000000-0000-0000-0000-{:012}",
            YOUTUBE_ARCHIVE_JOURNAL_REPLAY_LIMIT
        );
        let later_carrier = archive_merge_journal_path(
            &paths.youtube_subscription_archive_state_path(&later_subscription),
        );
        write_archive_merge_journal(&later_carrier, &later_intent).expect("later carrier");

        let first = cleanup_committed_archive_merge_carriers(&paths).expect("first startup page");
        assert_eq!(first, ArchiveCarrierCleanupReport::default());
        assert!(
            later_carrier.exists(),
            "later carrier is beyond the first page"
        );
        let persisted_after_first = load_archive_carrier_cleanup_cursor(&paths).unwrap();
        assert_eq!(
            persisted_after_first.0.as_deref(),
            Some("00000000-0000-0000-0000-000000000127")
        );
        assert_eq!(persisted_after_first.1, 1);

        drop(paths);
        let reopened = AppPaths::new(base_dir.clone());
        let second = cleanup_committed_archive_merge_carriers(&reopened)
            .expect("fresh process resumes durable cursor");
        let second_error = if second.failed > 0 {
            crate::db::open_readonly(&reopened)
                .unwrap()
                .query_row(
                    "SELECT error_message FROM youtube_archive_merge_intent_failure WHERE intent_id=?1",
                    [&later_intent],
                    |row| row.get::<_, String>(0),
                )
                .unwrap_or_else(|error| format!("missing failure receipt: {error}"))
        } else {
            String::new()
        };
        assert_eq!(
            second,
            ArchiveCarrierCleanupReport {
                removed: 1,
                failed: 0,
            },
            "fresh-process resumed cleanup report: {second_error}"
        );
        assert!(
            !later_carrier.exists(),
            "row 129 carrier must be reached after reopen"
        );
        let persisted_after_second = load_archive_carrier_cleanup_cursor(&reopened).unwrap();
        assert_eq!(
            persisted_after_second.0.as_deref(),
            Some(later_intent.as_str())
        );
        assert_eq!(persisted_after_second.1, 2);

        drop(reopened);
        let reopened_again = AppPaths::new(base_dir);
        let third = cleanup_committed_archive_merge_carriers(&reopened_again)
            .expect("repeated restart remains idempotent");
        assert_eq!(third.removed, 0);
    }

    #[test]
    fn cleanup_receipt_write_failure_is_truthful_and_does_not_advance_cursor() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_state"));
        crate::db::ensure_schema(&paths).expect("schema");
        let unsafe_subscription_id = "../../receipt-failure-outside";
        seed_archive_subscription(&paths, unsafe_subscription_id);
        let intent_id = "receipt-failure-intent";
        let escaped_archive = paths.youtube_subscription_archive_state_path(unsafe_subscription_id);
        let outside_carrier = archive_merge_journal_path(&escaped_archive);
        write_archive_merge_journal(&outside_carrier, intent_id).expect("outside carrier");
        let conn = crate::db::open(&paths).expect("db");
        conn.execute(
            "INSERT INTO youtube_archive_merge_intent(
               intent_id,subscription_id,target_archive_path,source_archive_sha256,
               intended_archive_sha256,intended_video_ids_json,phase,created_at_ms,updated_at_ms
             ) VALUES(?1,?2,?3,'source','intended','[]','committed',1,1)",
            params![
                intent_id,
                unsafe_subscription_id,
                escaped_archive.to_string_lossy(),
            ],
        )
        .expect("corrupt committed fixture");
        conn.execute_batch(
            "CREATE TRIGGER reject_archive_cleanup_failure_receipt
             BEFORE INSERT ON youtube_archive_merge_intent_failure
             BEGIN SELECT RAISE(ABORT, 'injected cleanup receipt write failure'); END;",
        )
        .expect("failure injection trigger");
        drop(conn);

        let before = load_archive_carrier_cleanup_cursor(&paths).unwrap();
        let error = reconcile_youtube_archive_merge_journals(&paths)
            .expect_err("receipt failure must be truthful to the caller");
        assert!(error
            .to_string()
            .contains("retry ownership could not be persisted"));
        assert_eq!(load_archive_carrier_cleanup_cursor(&paths).unwrap(), before);
        assert!(outside_carrier.exists());

        let conn = crate::db::open(&paths).expect("db repair");
        conn.execute_batch("DROP TRIGGER reject_archive_cleanup_failure_receipt;")
            .expect("remove injection");
        drop(conn);
        assert_eq!(reconcile_youtube_archive_merge_journals(&paths).unwrap(), 0);
        let failure_count: i64 = crate::db::open_readonly(&paths)
            .unwrap()
            .query_row(
                "SELECT failure_count FROM youtube_archive_merge_intent_failure WHERE intent_id=?1",
                [intent_id],
                |row| row.get(0),
            )
            .expect("durable retry ownership after repair");
        assert_eq!(failure_count, 1);
    }

    #[test]
    fn archive_merge_intent_requires_canonical_target_subscription_and_phase_chain() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_state"));
        crate::db::ensure_schema(&paths).expect("schema");
        seed_archive_subscription(&paths, "sub-contract");
        let intended = vec!["video".to_string()];
        let wrong_target = paths.youtube_subscription_archive_state_path("sub-other");
        assert!(create_archive_merge_intent(
            &paths,
            "sub-contract",
            &wrong_target,
            &archive_file_sha256(&wrong_target).unwrap(),
            &intended,
        )
        .is_err());
        let missing_target = paths.youtube_subscription_archive_state_path("sub-missing");
        assert!(create_archive_merge_intent(
            &paths,
            "sub-missing",
            &missing_target,
            &archive_file_sha256(&missing_target).unwrap(),
            &intended,
        )
        .is_err());

        let canonical = paths.youtube_subscription_archive_state_path("sub-contract");
        let intent_id = create_archive_merge_intent(
            &paths,
            "sub-contract",
            &canonical,
            &archive_file_sha256(&canonical).unwrap(),
            &intended,
        )
        .unwrap();
        let conn = crate::db::open(&paths).unwrap();
        assert!(
            conn.execute(
                "UPDATE youtube_archive_merge_intent SET phase='committed' WHERE intent_id=?1",
                [&intent_id],
            )
            .is_err(),
            "prepared must not skip the published phase"
        );
        assert!(conn.execute(
            "UPDATE youtube_archive_merge_intent SET target_archive_path='forged' WHERE intent_id=?1",
            [&intent_id],
        ).is_err(), "trusted lineage fields are immutable");
    }

    #[test]
    fn archive_merge_recovery_ignores_forged_and_relocated_carriers() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_state"));
        crate::db::ensure_schema(&paths).expect("schema");
        seed_archive_subscription(&paths, "sub-bound");
        seed_archive_subscription(&paths, "sub-other");
        let archive = paths.youtube_subscription_archive_state_path("sub-bound");
        std::fs::create_dir_all(archive.parent().expect("archive parent")).expect("mkdir");
        std::fs::write(&archive, "youtube existing\n").expect("existing archive");
        let intended = vec!["existing".to_string(), "trusted".to_string()];
        let intent_id = create_archive_merge_intent(
            &paths,
            "sub-bound",
            &archive,
            &archive_file_sha256(&archive).unwrap(),
            &intended,
        )
        .expect("trusted intent");
        let journal = archive_merge_journal_path(&archive);
        let forged = YoutubeArchiveMergeCarrier {
            schema_version: 2,
            intent_id: Uuid::new_v4().to_string(),
        };
        persistence::atomic_write_text(&journal, &serde_json::to_string(&forged).unwrap())
            .expect("forged carrier");
        let other_archive = paths.youtube_subscription_archive_state_path("sub-other");
        std::fs::create_dir_all(other_archive.parent().unwrap()).unwrap();
        let relocated = archive_merge_journal_path(&other_archive);
        write_archive_merge_journal(&relocated, &intent_id).expect("relocated carrier");

        assert_eq!(reconcile_youtube_archive_merge_journals(&paths).unwrap(), 1);
        assert!(
            journal.exists(),
            "forged carrier remains available for diagnosis"
        );
        assert!(
            relocated.exists(),
            "cross-subscription carrier is not adopted or removed"
        );
        assert_eq!(
            read_archive_file_ids(&archive).unwrap(),
            HashSet::from(["existing".to_string(), "trusted".to_string()]),
            "only trusted DB intent members may reach canonical bytes"
        );
        assert!(
            !other_archive.exists(),
            "relocated carrier must not publish another subscription"
        );
        let forged_members: i64 = crate::db::open_readonly(&paths).unwrap().query_row(
            "SELECT COUNT(*) FROM youtube_subscription_archive_member WHERE video_id='forged' OR subscription_id='sub-other'",
            [], |row| row.get(0),
        ).unwrap();
        assert_eq!(forged_members, 0);
    }

    #[test]
    fn failed_archive_projection_keeps_crash_journal_for_retry() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_state"));
        crate::db::ensure_schema(&paths).expect("schema");
        seed_archive_subscription(&paths, "sub-retry");
        let archive = paths.youtube_subscription_archive_state_path("sub-retry");
        std::fs::create_dir_all(archive.parent().expect("archive parent")).expect("mkdir");
        let intended = vec!["durable".to_string()];
        let intent_id = create_archive_merge_intent(
            &paths,
            "sub-retry",
            &archive,
            &archive_file_sha256(&archive).unwrap(),
            &intended,
        )
        .expect("trusted intent");
        let journal = archive_merge_journal_path(&archive);
        write_archive_merge_journal(&journal, &intent_id).expect("carrier");
        persistence::atomic_write_text(&archive, &canonical_archive_body(&intended)).unwrap();
        advance_archive_merge_intent_phase(&paths, &intent_id, "prepared", "published").unwrap();
        let conn = crate::db::open(&paths).expect("db");
        conn.execute("DROP TABLE youtube_subscription_archive_rollup", [])
            .expect("inject projection failure");
        drop(conn);

        assert_eq!(
            reconcile_youtube_archive_merge_journals(&paths).unwrap(),
            0,
            "bounded global repair quarantines this failure and continues"
        );
        assert!(
            journal.is_file(),
            "failed reconciliation must retain its retry cursor"
        );
        let failure: (i64, Option<i64>) = crate::db::open_readonly(&paths)
            .unwrap()
            .query_row(
                "SELECT failure_count,resolved_at_ms
                 FROM youtube_archive_merge_intent_failure WHERE intent_id=?1",
                [&intent_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(failure, (1, None));
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
                library_id: None,
                use_browser_cookies: false,
                browser_cookie_source: None,
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

    // WP-0284: subscription detail is membership-scoped rather than folder-prefix-scoped and
    // separates available from operator-deleted canonical items.
    #[test]
    fn youtube_subscription_videos_scopes_downloaded_and_pending() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        crate::db::ensure_schema(&paths).expect("schema");

        let output_dir = dir.path().join("FancamBot");
        std::fs::create_dir_all(&output_dir).expect("mkdir output");
        let sibling_dir = dir.path().join("FancamBot2");
        std::fs::create_dir_all(&sibling_dir).expect("mkdir sibling");

        let sub = upsert_youtube_subscription(
            &paths,
            YoutubeSubscriptionUpsert {
                id: None,
                title: "FancamBot".to_string(),
                source_url: "https://www.youtube.com/@fancambot/videos".to_string(),
                folder_map: None,
                output_dir_override: Some(output_dir.to_string_lossy().to_string()),
                library_id: None,
                use_browser_cookies: false,
                browser_cookie_source: None,
                auth_session_input: None,
                clear_auth_session: false,
                active: true,
                preset_id: None,
                group_ids: Vec::new(),
                refresh_interval_minutes: Some(MIN_REFRESH_INTERVAL_MINUTES),
            },
        )
        .expect("upsert");

        let conn = crate::db::open(&paths).expect("open");
        crate::db::migrate(&conn).expect("migrate");

        let insert_item = |id: &str, media_path: &str, created: i64| {
            conn.execute(
                r#"
INSERT INTO library_item (id, created_at_ms, source_type, source_uri, title, media_path)
VALUES (?1, ?2, 'url_direct', ?3, ?4, ?3)
"#,
                params![id, created, media_path, id],
            )
            .expect("insert library_item");
        };
        let under_output = output_dir
            .join("clip_one.mp4")
            .to_string_lossy()
            .to_string();
        let under_output_old = output_dir
            .join("clip_two.mp4")
            .to_string_lossy()
            .to_string();
        let under_sibling = sibling_dir.join("other.mp4").to_string_lossy().to_string();
        insert_item("item-new", &under_output, 2_000);
        insert_item("item-old", &under_output_old, 1_000);
        insert_item("item-sibling", &under_sibling, 3_000);
        for (item_id, media_id) in [("item-new", "membernew01"), ("item-old", "memberold01")] {
            conn.execute(
                "INSERT INTO media_source_identity (service, media_id, canonical_url, library_item_id, created_at_ms, updated_at_ms) VALUES ('youtube', ?1, ?2, ?3, 1, 1)",
                params![
                    media_id,
                    format!("https://www.youtube.com/watch?v={media_id}"),
                    item_id
                ],
            )
            .expect("identity");
            conn.execute(
                "INSERT INTO media_source_membership (service, media_id, source_subscription_id, source_kind, source_url_snapshot, source_title_snapshot, evidence_kind, created_at_ms, updated_at_ms) VALUES ('youtube', ?1, ?2, 'videos_page', ?3, 'FancamBot', 'test', 1, 1)",
                params![media_id, &sub.id, &sub.source_url],
            )
            .expect("membership");
        }
        conn.execute(
            "UPDATE library_item SET file_status='operator_deleted', file_status_changed_at_ms=3, file_status_change_source='operator', file_delete_method='permanent' WHERE id='item-old'",
            [],
        )
        .expect("mark deleted");

        let insert_job = |id: &str, status: &str, params: serde_json::Value, title: &str| {
            conn.execute(
                r#"
INSERT INTO job (id, type, status, progress, params_json, created_at_ms, logs_path, target_title)
VALUES (?1, 'download_direct_url', ?2, 0.0, ?3, ?4, '', ?5)
"#,
                params![
                    id,
                    status,
                    serde_json::to_string(&params).unwrap(),
                    now_ms(),
                    title
                ],
            )
            .expect("insert job");
        };
        insert_job(
            "job-queued",
            "queued",
            serde_json::json!({
                "url": "https://www.youtube.com/watch?v=aaaaaaaaaaa",
                "subscription_id": sub.id,
            }),
            "Pending Clip",
        );
        // Different subscription -> must be excluded.
        insert_job(
            "job-other-sub",
            "queued",
            serde_json::json!({
                "url": "https://www.youtube.com/watch?v=bbbbbbbbbbb",
                "subscription_id": "some-other-subscription-id",
            }),
            "Other Clip",
        );
        // Same subscription but already running -> must be excluded (not still-to-download).
        insert_job(
            "job-running",
            "running",
            serde_json::json!({
                "url": "https://www.youtube.com/watch?v=ccccccccccc",
                "subscription_id": sub.id,
            }),
            "Running Clip",
        );
        drop(conn);

        let result = youtube_subscription_videos(&paths, &sub.id, 50).expect("videos");

        let downloaded_paths: Vec<String> = result
            .downloaded
            .iter()
            .map(|i| i.media_path.clone())
            .collect();
        assert_eq!(
            result.downloaded.len(),
            1,
            "only available canonical membership items are returned: {downloaded_paths:?}"
        );
        assert!(downloaded_paths.contains(&under_output));
        assert!(
            !downloaded_paths.contains(&under_sibling),
            "folder proximity without membership must not create ownership"
        );
        assert_eq!(result.deleted.len(), 1);
        assert_eq!(result.deleted[0].id, "item-old");
        // Newest-first ordering.
        assert_eq!(result.downloaded[0].media_path, under_output);

        assert_eq!(result.pending.len(), 1, "only this sub's queued jobs");
        assert_eq!(
            result.pending[0].url,
            "https://www.youtube.com/watch?v=aaaaaaaaaaa"
        );
        assert_eq!(result.pending[0].title.as_deref(), Some("Pending Clip"));
    }

    #[test]
    fn subscription_download_activity_scopes_to_active_drain_batches() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        crate::db::ensure_schema(&paths).expect("schema");
        let conn = crate::db::open(&paths).expect("open");
        crate::db::migrate(&conn).expect("migrate");

        let insert_job = |id: &str,
                          batch_id: Option<&str>,
                          job_type: &str,
                          status: &str,
                          params_json: serde_json::Value,
                          title: Option<&str>| {
            conn.execute(
                r#"
INSERT INTO job (
  id, batch_id, type, status, progress, params_json, created_at_ms, logs_path, target_title
)
VALUES (?1, ?2, ?3, ?4, 0.0, ?5, ?6, '', ?7)
"#,
                params![
                    id,
                    batch_id,
                    job_type,
                    status,
                    serde_json::to_string(&params_json).unwrap(),
                    now_ms(),
                    title,
                ],
            )
            .expect("insert job");
        };

        insert_job(
            "refresh-active",
            None,
            "youtube_subscription_refresh_v1",
            "succeeded",
            serde_json::json!({"subscription_id": "sub-active"}),
            None,
        );
        insert_job(
            "active-queued",
            Some("refresh-active"),
            "download_direct_url",
            "queued",
            serde_json::json!({"subscription_id": "sub-active"}),
            Some("Waiting"),
        );
        insert_job(
            "active-done",
            Some("refresh-active"),
            "download_direct_url",
            "succeeded",
            serde_json::json!({"subscription_id": "sub-active"}),
            Some("Done"),
        );
        insert_job(
            "active-failed",
            Some("refresh-active"),
            "download_direct_url",
            "failed",
            serde_json::json!({"subscription_id": "sub-active"}),
            Some("Failed"),
        );

        insert_job(
            "refresh-terminal",
            None,
            "youtube_subscription_refresh_v1",
            "succeeded",
            serde_json::json!({"subscription_id": "sub-terminal"}),
            None,
        );
        insert_job(
            "terminal-done",
            Some("refresh-terminal"),
            "download_direct_url",
            "succeeded",
            serde_json::json!({"subscription_id": "sub-terminal"}),
            Some("Old done"),
        );
        drop(conn);

        let rows = rebuild_subscription_activity_rollup(&paths).expect("activity rebuild");
        assert_eq!(rows.len(), 1, "terminal-only history is not live activity");
        let active = &rows[0];
        assert_eq!(active.subscription_id, "sub-active");
        assert_eq!(active.queued, 1);
        assert_eq!(active.running, 0);
        assert_eq!(active.succeeded, 1);
        assert_eq!(active.failed, 1);
    }

    #[test]
    fn clear_youtube_subscription_group_memberships_keeps_groups_and_subscriptions() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        crate::db::ensure_schema(&paths).expect("schema");

        let group = upsert_youtube_subscription_group(
            &paths,
            YoutubeSubscriptionGroupUpsert {
                id: None,
                name: "Imported subscriptions".to_string(),
            },
        )
        .expect("group");
        let sub = upsert_youtube_subscription(
            &paths,
            YoutubeSubscriptionUpsert {
                id: None,
                title: "Grouped".to_string(),
                source_url: "https://www.youtube.com/@grouped/videos".to_string(),
                folder_map: None,
                output_dir_override: None,
                library_id: None,
                use_browser_cookies: false,
                browser_cookie_source: None,
                auth_session_input: None,
                clear_auth_session: false,
                active: true,
                preset_id: None,
                group_ids: vec![group.id.clone()],
                refresh_interval_minutes: Some(MIN_REFRESH_INTERVAL_MINUTES),
            },
        )
        .expect("subscription");
        assert_eq!(sub.group_ids, vec![group.id.clone()]);

        let removed = clear_youtube_subscription_group_memberships(&paths).expect("clear");

        assert_eq!(removed, 1);
        let groups = list_youtube_subscription_groups(&paths).expect("groups");
        assert_eq!(groups.len(), 1, "group rows are retained");
        let subscriptions = list_youtube_subscriptions(&paths).expect("subscriptions");
        assert_eq!(subscriptions.len(), 1, "subscription rows are retained");
        assert!(subscriptions[0].group_ids.is_empty());
    }

    #[test]
    fn manual_deleted_status_preserves_subscription_context_and_blocks_queueing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_state"));
        crate::db::ensure_schema(&paths).expect("schema");
        let output_dir = dir.path().join("archive");
        std::fs::create_dir_all(&output_dir).expect("output");

        let group = upsert_youtube_subscription_group(
            &paths,
            YoutubeSubscriptionGroupUpsert {
                id: None,
                name: "Preserved".to_string(),
            },
        )
        .expect("group");
        let sub = upsert_youtube_subscription(
            &paths,
            YoutubeSubscriptionUpsert {
                id: Some("sub-manual-deleted".to_string()),
                title: "Deleted source".to_string(),
                source_url: "https://www.youtube.com/channel/UCdeletedtest".to_string(),
                folder_map: Some("deleted_source".to_string()),
                output_dir_override: Some(output_dir.to_string_lossy().to_string()),
                library_id: None,
                use_browser_cookies: false,
                browser_cookie_source: None,
                auth_session_input: None,
                clear_auth_session: false,
                active: true,
                preset_id: None,
                group_ids: vec![group.id.clone()],
                refresh_interval_minutes: Some(MIN_REFRESH_INTERVAL_MINUTES),
            },
        )
        .expect("subscription");
        let queued = queue_youtube_subscription(&paths, &sub.id).expect("initial queue");
        assert_eq!(queued.len(), 1);

        let conn = crate::db::open(&paths).expect("open");
        crate::db::migrate(&conn).expect("migrate");
        conn.execute(
            "INSERT INTO media_source_identity \
             (service, media_id, canonical_url, created_at_ms, updated_at_ms) \
             VALUES ('youtube', 'preserved-video', 'https://www.youtube.com/watch?v=preserved-video', 1, 1)",
            [],
        )
        .expect("identity");
        conn.execute(
            "INSERT INTO media_source_membership \
             (service, media_id, source_subscription_id, source_kind, source_url_snapshot, \
              source_title_snapshot, evidence_kind, created_at_ms, updated_at_ms) \
             VALUES ('youtube', 'preserved-video', ?1, 'channel_page', ?2, ?3, 'test', 1, 1)",
            params![&sub.id, &sub.source_url, &sub.title],
        )
        .expect("membership");
        drop(conn);

        let receipt = set_youtube_subscription_manual_status(
            &paths,
            &sub.id,
            YOUTUBE_SUBSCRIPTION_STATUS_DELETED,
            YOUTUBE_SUBSCRIPTION_STATUS_ACTOR_ASSISTANT,
        )
        .expect("mark deleted");
        assert_eq!(receipt.subscription.source_status, "deleted");
        assert!(!receipt.subscription.active);
        assert_eq!(receipt.subscription.group_ids, vec![group.id.clone()]);
        assert_eq!(receipt.canceled_refresh_jobs, 1);

        let queue_error =
            queue_youtube_subscription(&paths, &sub.id).expect_err("deleted queue rejected");
        assert!(queue_error.to_string().contains("marked deleted"));
        assert!(
            queue_all_active_youtube_subscriptions_now(&paths)
                .expect("queue all")
                .is_empty(),
            "bulk queue excludes deleted"
        );
        assert!(
            queue_youtube_subscription_group(&paths, &group.id)
                .expect("queue group")
                .is_empty(),
            "group queue excludes deleted"
        );

        record_subscription_refresh_failure_with_error(
            &paths,
            &sub.id,
            Some("HTTP Error 404: Not Found"),
        )
        .expect("record failure");
        record_subscription_refresh_success(&paths, &sub.id).expect("record success");
        let still_deleted = get_youtube_subscription_by_id(&paths, &sub.id)
            .expect("read")
            .expect("row");
        assert_eq!(
            still_deleted.source_status, YOUTUBE_SUBSCRIPTION_STATUS_DELETED,
            "automatic refresh outcomes cannot change deleted"
        );

        let conn = crate::db::open_readonly(&paths).expect("readonly");
        let membership_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM media_source_membership WHERE source_subscription_id=?1",
                [&sub.id],
                |row| row.get(0),
            )
            .expect("membership count");
        assert_eq!(membership_count, 1, "source/video metadata is retained");
        drop(conn);

        let restored = set_youtube_subscription_manual_status(
            &paths,
            &sub.id,
            YOUTUBE_SUBSCRIPTION_STATUS_NORMAL,
            YOUTUBE_SUBSCRIPTION_STATUS_ACTOR_OPERATOR,
        )
        .expect("restore");
        assert_eq!(restored.subscription.source_status, "normal");
        assert!(restored.subscription.active);
        assert_eq!(restored.subscription.group_ids, vec![group.id]);
        assert_eq!(
            queue_youtube_subscription(&paths, &sub.id)
                .expect("queue restored")
                .len(),
            1
        );
    }

    #[test]
    fn exact_404_sets_unavailable_and_success_recovers_without_inferring_deleted() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_state"));
        crate::db::ensure_schema(&paths).expect("schema");
        let sub = upsert_youtube_subscription(
            &paths,
            YoutubeSubscriptionUpsert {
                id: Some("sub-unavailable".to_string()),
                title: "Unavailable playlist".to_string(),
                source_url: "https://www.youtube.com/playlist?list=PLunavailable".to_string(),
                folder_map: None,
                output_dir_override: Some(dir.path().join("archive").to_string_lossy().to_string()),
                library_id: None,
                use_browser_cookies: false,
                browser_cookie_source: None,
                auth_session_input: None,
                clear_auth_session: false,
                active: true,
                preset_id: None,
                group_ids: Vec::new(),
                refresh_interval_minutes: Some(MIN_REFRESH_INTERVAL_MINUTES),
            },
        )
        .expect("subscription");

        record_subscription_refresh_failure_with_error(
            &paths,
            &sub.id,
            Some("network connection timed out"),
        )
        .expect("network failure");
        assert_eq!(
            get_youtube_subscription_by_id(&paths, &sub.id)
                .expect("read")
                .expect("row")
                .source_status,
            YOUTUBE_SUBSCRIPTION_STATUS_NORMAL,
            "bad connection cannot set unavailable or deleted"
        );

        record_subscription_refresh_failure_with_error(
            &paths,
            &sub.id,
            Some("Unable to download API page: HTTP Error 404: Not Found"),
        )
        .expect("404 failure");
        let unavailable = get_youtube_subscription_by_id(&paths, &sub.id)
            .expect("read")
            .expect("row");
        assert_eq!(
            unavailable.source_status,
            YOUTUBE_SUBSCRIPTION_STATUS_UNAVAILABLE
        );
        assert_eq!(
            unavailable.source_status_change_source.as_deref(),
            Some("refresh_404")
        );
        assert!(
            unavailable.active,
            "unavailable remains eligible for a later paced recovery check"
        );

        record_subscription_refresh_success(&paths, &sub.id).expect("success");
        let recovered = get_youtube_subscription_by_id(&paths, &sub.id)
            .expect("read")
            .expect("row");
        assert_eq!(recovered.source_status, YOUTUBE_SUBSCRIPTION_STATUS_NORMAL);
        assert_eq!(
            recovered.source_status_change_source.as_deref(),
            Some("refresh_success")
        );
    }

    #[test]
    fn manual_status_command_rejects_automatic_status_and_unknown_actor() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_state"));
        crate::db::ensure_schema(&paths).expect("schema");
        let sub = upsert_youtube_subscription(
            &paths,
            YoutubeSubscriptionUpsert {
                id: Some("sub-authority".to_string()),
                title: "Authority".to_string(),
                source_url: "https://www.youtube.com/@authority/videos".to_string(),
                folder_map: None,
                output_dir_override: Some(dir.path().join("archive").to_string_lossy().to_string()),
                library_id: None,
                use_browser_cookies: false,
                browser_cookie_source: None,
                auth_session_input: None,
                clear_auth_session: false,
                active: true,
                preset_id: None,
                group_ids: Vec::new(),
                refresh_interval_minutes: Some(MIN_REFRESH_INTERVAL_MINUTES),
            },
        )
        .expect("subscription");

        let unavailable_error = set_youtube_subscription_manual_status(
            &paths,
            &sub.id,
            YOUTUBE_SUBSCRIPTION_STATUS_UNAVAILABLE,
            YOUTUBE_SUBSCRIPTION_STATUS_ACTOR_OPERATOR,
        )
        .expect_err("manual unavailable rejected");
        assert!(unavailable_error
            .to_string()
            .contains("must be normal or deleted"));

        let actor_error = set_youtube_subscription_manual_status(
            &paths,
            &sub.id,
            YOUTUBE_SUBSCRIPTION_STATUS_DELETED,
            "network_probe",
        )
        .expect_err("unknown actor rejected");
        assert!(actor_error
            .to_string()
            .contains("must be operator or assistant"));
        assert_eq!(
            get_youtube_subscription_by_id(&paths, &sub.id)
                .expect("read")
                .expect("row")
                .source_status,
            YOUTUBE_SUBSCRIPTION_STATUS_NORMAL
        );
    }

    #[test]
    fn incremental_archive_member_preserves_unrelated_dirty_state_and_advances_generation() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_state"));
        crate::db::ensure_schema(&paths).expect("schema");
        let conn = crate::db::open(&paths).expect("open");
        conn.execute("UPDATE derived_projection_state SET dirty=1,updated_at_ms=41 WHERE projection='youtube_archive'", []).unwrap();
        drop(conn);
        record_youtube_archive_member(&paths, "sub-a", "video-a").expect("record");
        let conn = crate::db::open_readonly(&paths).expect("read");
        let state: (i64, i64) = conn.query_row("SELECT dirty,updated_at_ms FROM derived_projection_state WHERE projection='youtube_archive'", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!(
            state,
            (1, 42),
            "incremental work must not clear unrelated reconciliation dirtiness"
        );
    }

    #[test]
    fn canonical_archive_rebuild_does_not_invalidate_its_own_generation_cas() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_state"));
        crate::db::ensure_schema(&paths).expect("schema");
        let conn = crate::db::open(&paths).expect("open");
        conn.execute("UPDATE derived_projection_state SET dirty=1,updated_at_ms=77 WHERE projection='youtube_archive'", []).unwrap();
        drop(conn);
        let rebuilt = rebuild_youtube_subscription_archive_rollups(&paths).expect("rebuild");
        assert!(rebuilt.is_empty());
        let conn = crate::db::open_readonly(&paths).expect("read");
        let state: (i64, i64) = conn.query_row("SELECT dirty,updated_at_ms FROM derived_projection_state WHERE projection='youtube_archive'", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!(state, (0, 77));
    }

    #[test]
    fn canonical_archive_file_merge_marks_dirty_before_mutation_and_advances_generation() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_state"));
        crate::db::ensure_schema(&paths).expect("schema");
        seed_archive_subscription(&paths, "sub-merge");
        let archive = paths.youtube_subscription_archive_state_path("sub-merge");
        std::fs::create_dir_all(archive.parent().unwrap()).expect("archive parent");
        let before: (i64, i64) = crate::db::open_readonly(&paths)
            .unwrap()
            .query_row(
                "SELECT dirty,updated_at_ms FROM derived_projection_state WHERE projection='youtube_archive'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        let ids = HashSet::from(["video-one".to_string()]);
        let merged = merge_archive_file(&paths, &archive, &ids).expect("canonical merge");
        assert_eq!(merged.0, 1);
        let after: (i64, i64) = crate::db::open_readonly(&paths)
            .unwrap()
            .query_row(
                "SELECT dirty,updated_at_ms FROM derived_projection_state WHERE projection='youtube_archive'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(after.0, 1);
        assert_eq!(after.1, before.1 + 1);
    }

    #[test]
    fn archive_file_generation_detects_external_mutation_even_without_db_writer() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_state"));
        let archive = paths.youtube_subscription_archive_state_path("sub-file-race");
        std::fs::create_dir_all(archive.parent().unwrap()).expect("archive parent");
        std::fs::write(&archive, b"youtube first\n").expect("first generation");
        let before = youtube_archive_file_generation(&paths).expect("before");
        // Equal-length replacement defeats size-only identities and can also evade timestamp
        // identities on coarse filesystems; the content digest must still change the CAS token.
        std::fs::write(&archive, b"youtube other\n").expect("same-size mutate");
        let after = youtube_archive_file_generation(&paths).expect("after");
        assert_ne!(
            before, after,
            "rebuild CAS must observe canonical file mutation"
        );
    }

    #[test]
    fn archive_merge_journal_recovery_preserves_canonical_bytes_and_marks_repair_dirty() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_state"));
        crate::db::ensure_schema(&paths).expect("schema");
        seed_archive_subscription(&paths, "sub-recovery");
        let archive = paths.youtube_subscription_archive_state_path("sub-recovery");
        std::fs::create_dir_all(archive.parent().unwrap()).unwrap();
        let intended = vec!["recovered-video".to_string()];
        let intent_id = create_archive_merge_intent(
            &paths,
            "sub-recovery",
            &archive,
            &archive_file_sha256(&archive).unwrap(),
            &intended,
        )
        .unwrap();
        persistence::atomic_write_text(&archive, "youtube recovered-video\n").unwrap();
        write_archive_merge_journal(&archive_merge_journal_path(&archive), &intent_id).unwrap();
        let conn = crate::db::open(&paths).unwrap();
        conn.execute("UPDATE derived_projection_state SET dirty=0,updated_at_ms=90 WHERE projection='youtube_archive'", []).unwrap();
        drop(conn);

        let _guard = lock_youtube_archive_mutation().unwrap();
        assert_eq!(reconcile_archive_merge_journals_locked(&paths).unwrap(), 1);
        drop(_guard);
        assert_eq!(
            std::fs::read_to_string(&archive).unwrap(),
            "youtube recovered-video\n"
        );
        assert!(!archive_merge_journal_path(&archive).exists());
        let state: (i64, i64) = crate::db::open_readonly(&paths).unwrap().query_row(
            "SELECT dirty,updated_at_ms FROM derived_projection_state WHERE projection='youtube_archive'",
            [], |row| Ok((row.get(0)?, row.get(1)?)),
        ).unwrap();
        assert_eq!(state, (1, 91));
    }

    #[test]
    fn concurrent_archive_merges_serialize_without_lost_members() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_state"));
        crate::db::ensure_schema(&paths).expect("schema");
        seed_archive_subscription(&paths, "sub-concurrent");
        let mut workers = Vec::new();
        for index in 0..16 {
            let paths = paths.clone();
            workers.push(std::thread::spawn(move || {
                merge_youtube_archive_member(&paths, "sub-concurrent", &format!("video-{index}"))
            }));
        }
        for worker in workers {
            worker.join().unwrap().unwrap();
        }
        let archive = paths.youtube_subscription_archive_state_path("sub-concurrent");
        let ids = read_archive_file_ids(&archive).unwrap();
        assert_eq!(ids.len(), 16);
        assert!(!archive_merge_journal_path(&archive).exists());
        let projected: i64 = crate::db::open_readonly(&paths).unwrap().query_row(
            "SELECT video_count FROM youtube_subscription_archive_rollup WHERE subscription_id='sub-concurrent'",
            [], |row| row.get(0),
        ).unwrap();
        assert_eq!(projected, 16);
    }

    #[test]
    fn job_events_update_activity_rollup_and_terminal_drain_removes_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_state"));
        crate::db::ensure_schema(&paths).expect("schema");
        let conn = crate::db::open(&paths).unwrap();
        conn.execute(
            "INSERT INTO job(id,type,status,progress,params_json,created_at_ms,logs_path) VALUES('refresh-event','youtube_subscription_refresh_v1','succeeded',1,?1,1,'refresh.log')",
            [serde_json::json!({"subscription_id":"sub-event"}).to_string()],
        ).unwrap();
        for (id, title) in [("child-one", "One"), ("child-two", "Two")] {
            conn.execute(
                "INSERT INTO job(id,batch_id,type,status,progress,params_json,created_at_ms,logs_path,target_title) VALUES(?1,'refresh-event','download_direct_url','queued',0,?2,2,'child.log',?3)",
                params![id, serde_json::json!({"subscription_id":"sub-event"}).to_string(), title],
            ).unwrap();
        }
        drop(conn);
        refresh_subscription_activity_rollup_for_job(&paths, "child-one").unwrap();
        let first = subscription_download_activity(&paths).unwrap();
        assert_eq!((first[0].queued, first[0].running), (2, 0));

        let conn = crate::db::open(&paths).unwrap();
        conn.execute(
            "UPDATE job SET status='running',started_at_ms=10,progress=.25 WHERE id='child-one'",
            [],
        )
        .unwrap();
        drop(conn);
        refresh_subscription_activity_rollup_for_job(&paths, "child-one").unwrap();
        let running = subscription_download_activity(&paths).unwrap();
        assert_eq!((running[0].queued, running[0].running), (1, 1));
        assert_eq!(running[0].current_title.as_deref(), Some("One"));

        let conn = crate::db::open(&paths).unwrap();
        conn.execute(
            "UPDATE job SET status='succeeded' WHERE id IN ('child-one','child-two')",
            [],
        )
        .unwrap();
        drop(conn);
        refresh_subscription_activity_rollup_for_job(&paths, "child-two").unwrap();
        assert!(subscription_download_activity(&paths).unwrap().is_empty());
    }

    #[test]
    fn activity_refresh_takes_snapshot_after_writer_reservation() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_state"));
        crate::db::ensure_schema(&paths).expect("schema");
        let conn = crate::db::open(&paths).expect("db");
        conn.execute(
            "INSERT INTO job(id,type,status,progress,params_json,created_at_ms,logs_path) VALUES('refresh-race','youtube_subscription_refresh_v1','succeeded',1,?1,1,'refresh.log')",
            [serde_json::json!({"subscription_id":"sub-race"}).to_string()],
        ).expect("refresh job");
        conn.execute(
            "INSERT INTO job(id,batch_id,type,status,progress,params_json,created_at_ms,logs_path,target_title) VALUES('child-race','refresh-race','download_direct_url','queued',0,?1,2,'child.log','Race')",
            [serde_json::json!({"subscription_id":"sub-race"}).to_string()],
        ).expect("child job");
        drop(conn);
        refresh_subscription_activity_rollup_for_job(&paths, "child-race").expect("initial rollup");

        // Hold an uncommitted terminal transition. A correct refresher blocks before its snapshot;
        // the old implementation read `queued` first, blocked only on BEGIN IMMEDIATE, and later
        // overwrote the terminal projection with that stale snapshot.
        let blocker = crate::db::open(&paths).expect("blocker");
        blocker
            .execute_batch(
                "BEGIN IMMEDIATE; UPDATE job SET status='succeeded' WHERE id='child-race';",
            )
            .expect("uncommitted terminal transition");
        let worker_paths = paths.clone();
        let worker = std::thread::spawn(move || {
            refresh_subscription_activity_rollup_for_job(&worker_paths, "child-race")
        });
        std::thread::sleep(std::time::Duration::from_millis(150));
        blocker
            .execute_batch("COMMIT;")
            .expect("commit terminal transition");
        worker.join().expect("worker join").expect("refresh");

        assert!(
            subscription_download_activity(&paths)
                .expect("activity")
                .is_empty(),
            "post-lock snapshot must observe the committed terminal state"
        );
    }

    #[test]
    fn successful_download_archive_append_marks_dirty_before_incremental_projection() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_state"));
        crate::db::ensure_schema(&paths).expect("schema");
        let subscription = upsert_youtube_subscription(
            &paths,
            YoutubeSubscriptionUpsert {
                id: Some("sub-canonical-append".to_string()),
                title: "Canonical append".to_string(),
                source_url: "https://www.youtube.com/@canonical/videos".to_string(),
                folder_map: None,
                output_dir_override: Some(dir.path().join("archive").to_string_lossy().to_string()),
                library_id: None,
                use_browser_cookies: false,
                browser_cookie_source: None,
                auth_session_input: None,
                clear_auth_session: false,
                active: true,
                preset_id: None,
                group_ids: vec![],
                refresh_interval_minutes: None,
            },
        )
        .expect("subscription");
        let conn = crate::db::open(&paths).expect("open");
        conn.execute(
            "UPDATE derived_projection_state SET dirty=0,updated_at_ms=100 WHERE projection='youtube_archive'",
            [],
        )
        .unwrap();
        drop(conn);

        crate::jobs::append_youtube_archive_on_success(
            &paths,
            &subscription.id,
            "https://www.youtube.com/watch?v=archive-fixture",
        )
        .expect("canonical append");

        let state: (i64, i64) = crate::db::open_readonly(&paths)
            .unwrap()
            .query_row(
                "SELECT dirty,updated_at_ms FROM derived_projection_state WHERE projection='youtube_archive'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            state.0, 1,
            "canonical byte append must leave rebuild evidence dirty"
        );
        assert!(
            state.1 >= 101,
            "committed archive projection must advance generation"
        );
        let committed_intents: i64 = crate::db::open_readonly(&paths)
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM youtube_archive_merge_intent
                 WHERE subscription_id=?1 AND phase='committed'",
                [&subscription.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            committed_intents, 1,
            "durable intent replaces the old non-transactional pre-write dirty marker"
        );
    }

    #[test]
    fn tiktok_subscription_creation_and_canonical_identity_inference() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(temp.path().to_path_buf());
        crate::db::ensure_schema(&paths).unwrap();

        let sub = upsert_youtube_subscription(
            &paths,
            YoutubeSubscriptionUpsert {
                id: None,
                title: "TikTok Creator Profile".to_string(),
                source_url: "https://www.tiktok.com/@tiktok_creator".to_string(),
                folder_map: Some("tiktok_creator".to_string()),
                output_dir_override: None,
                library_id: None,
                use_browser_cookies: false,
                browser_cookie_source: None,
                auth_session_input: None,
                clear_auth_session: false,
                active: true,
                preset_id: None,
                group_ids: vec![],
                refresh_interval_minutes: Some(1440),
            },
        )
        .expect("create tiktok subscription");

        assert_eq!(sub.title, "TikTok Creator Profile");
        assert_eq!(sub.source_url, "https://www.tiktok.com/@tiktok_creator");

        let conn = crate::db::open_readonly(&paths).unwrap();
        let loaded = subscription_by_id_conn(&conn, &sub.id).unwrap().unwrap();
        assert_eq!(loaded.id, sub.id);
        assert_eq!(loaded.source_url, "https://www.tiktok.com/@tiktok_creator");
    }
}
