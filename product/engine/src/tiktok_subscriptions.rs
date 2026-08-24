use crate::paths::AppPaths;
use crate::{db, jobs, EngineError, Result};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use url::Url;
use uuid::Uuid;

const DEFAULT_REFRESH_INTERVAL_MINUTES: i64 = 60;
const MIN_REFRESH_INTERVAL_MINUTES: i64 = 5;
const MAX_REFRESH_INTERVAL_MINUTES: i64 = 10080;
const DEFAULT_MAX_ITEMS: i64 = 30;
const MAX_ITEMS: i64 = 500;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TiktokSubscriptionRow {
    pub id: String,
    pub title: String,
    pub source_url: String,
    pub canonical_profile_id: Option<String>,
    pub folder_map: String,
    pub output_dir_override: Option<String>,
    pub use_browser_cookies: bool,
    pub browser_cookie_source: Option<String>,
    pub active: bool,
    pub refresh_interval_minutes: i64,
    pub max_items_per_refresh: i64,
    pub last_queued_at_ms: Option<i64>,
    pub last_attempt_at_ms: Option<i64>,
    pub last_success_at_ms: Option<i64>,
    pub last_error_at_ms: Option<i64>,
    pub last_error: Option<String>,
    pub consecutive_failures: i64,
    pub next_allowed_refresh_at_ms: Option<i64>,
    pub provider_name: String,
    pub provider_version: Option<String>,
    pub capability_epoch: i64,
    pub last_failure_class: Option<String>,
    pub last_failure_message_hash: Option<String>,
    pub hold_reason: Option<String>,
    pub consecutive_successes: i64,
    pub last_canonical_discovery_count: i64,
    pub cursor_json: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TiktokSubscriptionUpsert {
    pub id: Option<String>,
    pub title: String,
    pub source_url: String,
    pub folder_map: Option<String>,
    pub output_dir_override: Option<String>,
    #[serde(default)]
    pub use_browser_cookies: bool,
    #[serde(default)]
    pub browser_cookie_source: Option<String>,
    pub active: bool,
    pub refresh_interval_minutes: Option<i64>,
    pub max_items_per_refresh: Option<i64>,
}

const SELECT_FIELDS: &str = "id,title,source_url,canonical_profile_id,folder_map,output_dir_override,use_browser_cookies,browser_cookie_source,active,refresh_interval_minutes,max_items_per_refresh,last_queued_at_ms,last_attempt_at_ms,last_success_at_ms,last_error_at_ms,last_error,consecutive_failures,next_allowed_refresh_at_ms,provider_name,provider_version,capability_epoch,last_failure_class,last_failure_message_hash,hold_reason,consecutive_successes,last_canonical_discovery_count,cursor_json,created_at_ms,updated_at_ms";

pub fn list_tiktok_subscriptions(paths: &AppPaths) -> Result<Vec<TiktokSubscriptionRow>> {
    let conn = db::open_readonly(paths)?;
    let mut stmt = conn.prepare(&format!(
        "SELECT {SELECT_FIELDS} FROM tiktok_subscription ORDER BY active DESC, updated_at_ms DESC"
    ))?;
    let rows = stmt
        .query_map([], row_to_subscription)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn upsert_tiktok_subscription(
    paths: &AppPaths,
    req: TiktokSubscriptionUpsert,
) -> Result<TiktokSubscriptionRow> {
    let title = normalize_title(&req.title)?;
    let source_url = normalize_profile_url(&req.source_url)?;
    let folder_map = sanitize_folder_map(
        req.folder_map
            .as_deref()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or(&title),
    );
    let output_dir_override = req
        .output_dir_override
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let browser_cookie_source = if req.use_browser_cookies {
        Some(
            jobs::normalize_browser_cookie_source(req.browser_cookie_source.as_deref())?
                .ok_or_else(|| {
                    EngineError::InstallFailed("choose a browser for TikTok cookies".to_string())
                })?,
        )
    } else {
        None
    };
    let interval = req
        .refresh_interval_minutes
        .unwrap_or(DEFAULT_REFRESH_INTERVAL_MINUTES)
        .clamp(MIN_REFRESH_INTERVAL_MINUTES, MAX_REFRESH_INTERVAL_MINUTES);
    let max_items = req
        .max_items_per_refresh
        .unwrap_or(DEFAULT_MAX_ITEMS)
        .clamp(1, MAX_ITEMS);
    let id = req
        .id
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let now = now_ms();
    let conn = db::write_context(paths)?;
    conn.execute(
        r#"INSERT INTO tiktok_subscription(id,title,source_url,folder_map,output_dir_override,use_browser_cookies,browser_cookie_source,active,refresh_interval_minutes,max_items_per_refresh,provider_name,provider_version,capability_epoch,created_at_ms,updated_at_ms)
VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'yt-dlp',?11,1,?12,?12)
ON CONFLICT(source_url) DO UPDATE SET title=excluded.title,folder_map=excluded.folder_map,output_dir_override=excluded.output_dir_override,use_browser_cookies=excluded.use_browser_cookies,browser_cookie_source=excluded.browser_cookie_source,active=excluded.active,refresh_interval_minutes=excluded.refresh_interval_minutes,max_items_per_refresh=excluded.max_items_per_refresh,provider_version=excluded.provider_version,hold_reason=NULL,next_allowed_refresh_at_ms=NULL,updated_at_ms=excluded.updated_at_ms"#,
        params![id,title,source_url,folder_map,output_dir_override,req.use_browser_cookies as i64,browser_cookie_source,req.active as i64,interval,max_items,crate::pinned_dependency_manifest::manifest().yt_dlp_windows.version,now],
    )?;
    by_source(&conn, &source_url)?.ok_or_else(|| {
        EngineError::InstallFailed("failed to load saved TikTok subscription".to_string())
    })
}

pub fn delete_tiktok_subscription(paths: &AppPaths, id: &str) -> Result<()> {
    let conn = db::write_context(paths)?;
    let changed = conn.execute(
        "UPDATE tiktok_subscription SET active=0,hold_reason='Archived by operator',next_allowed_refresh_at_ms=NULL,updated_at_ms=?1 WHERE id=?2",
        params![now_ms(), id],
    )?;
    if changed == 0 {
        return Err(EngineError::InstallFailed(format!(
            "TikTok subscription not found: {id}"
        )));
    }
    Ok(())
}

pub fn queue_tiktok_subscription(paths: &AppPaths, id: &str) -> Result<Vec<jobs::JobRow>> {
    let conn = db::write_context(paths)?;
    let row = by_id(&conn, id)?.ok_or_else(|| {
        EngineError::InstallFailed(format!("TikTok subscription not found: {id}"))
    })?;
    if !row.active {
        return Err(EngineError::InstallFailed(
            "TikTok subscription is archived; reactivate it before queueing".to_string(),
        ));
    }
    drop(conn);
    queue_row(paths, &row)
}

pub fn queue_all_active_tiktok_subscriptions(paths: &AppPaths) -> Result<Vec<jobs::JobRow>> {
    if jobs::get_queue_control(paths)?.paused {
        return Ok(Vec::new());
    }
    let now = now_ms();
    let mut rows = list_tiktok_subscriptions(paths)?;
    rows.retain(|row| {
        row.active
            && row.hold_reason.is_none()
            && row.next_allowed_refresh_at_ms.unwrap_or(0) <= now
            && is_due(row, now)
    });
    rows.sort_by_key(|row| row.last_queued_at_ms.unwrap_or(i64::MIN));
    match rows.first() {
        Some(row) => queue_row(paths, row),
        None => Ok(Vec::new()),
    }
}

pub fn tiktok_subscription_output_dir(
    paths: &AppPaths,
    row: &TiktokSubscriptionRow,
) -> Result<PathBuf> {
    if let Some(value) = row.output_dir_override.as_deref() {
        let path = PathBuf::from(value);
        return Ok(if path.is_absolute() {
            path
        } else {
            std::env::current_dir()?.join(path)
        });
    }
    let configured = crate::config::load_feature_storage_roots_config(paths)?;
    let base_dir = configured
        .tiktok_root
        .map(PathBuf::from)
        .unwrap_or(paths.effective_download_dir()?.join("tiktok"));
    Ok(base_dir.join("subscriptions").join(&row.folder_map))
}

fn queue_row(paths: &AppPaths, row: &TiktokSubscriptionRow) -> Result<Vec<jobs::JobRow>> {
    let queued = vec![jobs::enqueue_tiktok_subscription_refresh_v1(
        paths,
        row.id.clone(),
        row.title.clone(),
        row.source_url.clone(),
        tiktok_subscription_output_dir(paths, row)?
            .to_string_lossy()
            .to_string(),
        row.max_items_per_refresh as usize,
        if row.use_browser_cookies {
            row.browser_cookie_source.clone()
        } else {
            None
        },
    )?];
    let now = now_ms();
    let conn = db::write_context(paths)?;
    conn.execute(
        "UPDATE tiktok_subscription SET last_queued_at_ms=?1,last_attempt_at_ms=?1,updated_at_ms=?1 WHERE id=?2",
        params![now, row.id],
    )?;
    Ok(queued)
}

fn is_due(row: &TiktokSubscriptionRow, now: i64) -> bool {
    row.last_queued_at_ms.is_none_or(|last| {
        now.saturating_sub(last) >= row.refresh_interval_minutes.saturating_mul(60_000)
    })
}

fn by_id(conn: &rusqlite::Connection, id: &str) -> Result<Option<TiktokSubscriptionRow>> {
    conn.query_row(
        &format!("SELECT {SELECT_FIELDS} FROM tiktok_subscription WHERE id=?1"),
        [id],
        row_to_subscription,
    )
    .optional()
    .map_err(Into::into)
}

fn by_source(conn: &rusqlite::Connection, source: &str) -> Result<Option<TiktokSubscriptionRow>> {
    conn.query_row(
        &format!("SELECT {SELECT_FIELDS} FROM tiktok_subscription WHERE source_url=?1"),
        [source],
        row_to_subscription,
    )
    .optional()
    .map_err(Into::into)
}

fn row_to_subscription(row: &rusqlite::Row<'_>) -> rusqlite::Result<TiktokSubscriptionRow> {
    Ok(TiktokSubscriptionRow {
        id: row.get(0)?,
        title: row.get(1)?,
        source_url: row.get(2)?,
        canonical_profile_id: row.get(3)?,
        folder_map: row.get(4)?,
        output_dir_override: row.get(5)?,
        use_browser_cookies: row.get::<_, i64>(6)? != 0,
        browser_cookie_source: row.get(7)?,
        active: row.get::<_, i64>(8)? != 0,
        refresh_interval_minutes: row.get(9)?,
        max_items_per_refresh: row.get(10)?,
        last_queued_at_ms: row.get(11)?,
        last_attempt_at_ms: row.get(12)?,
        last_success_at_ms: row.get(13)?,
        last_error_at_ms: row.get(14)?,
        last_error: row.get(15)?,
        consecutive_failures: row.get(16)?,
        next_allowed_refresh_at_ms: row.get(17)?,
        provider_name: row.get(18)?,
        provider_version: row.get(19)?,
        capability_epoch: row.get(20)?,
        last_failure_class: row.get(21)?,
        last_failure_message_hash: row.get(22)?,
        hold_reason: row.get(23)?,
        consecutive_successes: row.get(24)?,
        last_canonical_discovery_count: row.get(25)?,
        cursor_json: row.get(26)?,
        created_at_ms: row.get(27)?,
        updated_at_ms: row.get(28)?,
    })
}

fn normalize_title(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(EngineError::InstallFailed(
            "TikTok subscription title cannot be empty".to_string(),
        ));
    }
    Ok(value.chars().take(200).collect())
}

fn normalize_profile_url(value: &str) -> Result<String> {
    let mut url = Url::parse(value.trim())
        .map_err(|_| EngineError::InstallFailed("invalid TikTok profile URL".to_string()))?;
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    if !(host == "tiktok.com" || host.ends_with(".tiktok.com")) || !url.path().starts_with("/@") {
        return Err(EngineError::InstallFailed(
            "TikTok subscriptions require a tiktok.com/@profile URL".to_string(),
        ));
    }
    url.set_query(None);
    url.set_fragment(None);
    Ok(url.to_string())
}

fn sanitize_folder_map(value: &str) -> String {
    let value: String = value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    value.trim_matches(['_', '.']).chars().take(80).collect()
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiktok_rows_are_provider_specific_and_survive_restart() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        db::ensure_schema(&paths).expect("schema");
        let saved = upsert_tiktok_subscription(
            &paths,
            TiktokSubscriptionUpsert {
                id: None,
                title: "Creator".to_string(),
                source_url: "https://www.tiktok.com/@creator".to_string(),
                folder_map: None,
                output_dir_override: None,
                use_browser_cookies: false,
                browser_cookie_source: None,
                active: true,
                refresh_interval_minutes: Some(30),
                max_items_per_refresh: Some(12),
            },
        )
        .expect("save");
        assert_eq!(saved.max_items_per_refresh, 12);
        let conn = db::open_readonly(&paths).expect("db");
        let youtube_rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM youtube_subscription WHERE id=?1",
                [&saved.id],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!(youtube_rows, 0);
        drop(conn);
        assert_eq!(
            list_tiktok_subscriptions(&AppPaths::new(dir.path().to_path_buf()))
                .expect("list")
                .len(),
            1
        );

        let queued = queue_tiktok_subscription(&paths, &saved.id).expect("queue refresh");
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0].job_type, "tiktok_subscription_refresh_v1");
        assert_eq!(queued[0].track, "tiktok_recurring");
        let params: serde_json::Value =
            serde_json::from_str(&queued[0].params_json).expect("refresh params");
        assert_eq!(params["max_items"], 12);
        assert_eq!(params["source_page_url"], saved.source_url);

        delete_tiktok_subscription(&paths, &saved.id).expect("archive subscription");
        let archived = list_tiktok_subscriptions(&paths).expect("list archived");
        assert_eq!(
            archived.len(),
            1,
            "archive must retain the subscription row"
        );
        assert!(!archived[0].active);
        assert_eq!(
            archived[0].hold_reason.as_deref(),
            Some("Archived by operator")
        );
        assert!(
            queue_tiktok_subscription(&paths, &saved.id).is_err(),
            "archived subscriptions must not enqueue work"
        );
        assert_eq!(
            jobs::list_jobs(&paths, 10, 0).expect("retained jobs").len(),
            1,
            "archive must retain existing job history"
        );
    }
}
