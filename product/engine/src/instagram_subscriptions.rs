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
const DEFAULT_FOLDER_MAP: &str = "instagram";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstagramSubscriptionRow {
    pub id: String,
    pub title: String,
    pub source_url: String,
    pub folder_map: String,
    pub output_dir_override: Option<String>,
    pub use_browser_cookies: bool,
    pub auth_session_configured: bool,
    pub active: bool,
    pub refresh_interval_minutes: i64,
    pub last_queued_at_ms: Option<i64>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstagramSubscriptionUpsert {
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
    pub refresh_interval_minutes: Option<i64>,
}

pub fn list_instagram_subscriptions(paths: &AppPaths) -> Result<Vec<InstagramSubscriptionRow>> {
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
  refresh_interval_minutes,
  last_queued_at_ms,
  created_at_ms,
  updated_at_ms
FROM instagram_subscription
ORDER BY active DESC, updated_at_ms DESC, created_at_ms DESC
"#,
    )?;
    let rows = stmt
        .query_map([], row_to_subscription)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(hydrate_auth_session_flags(paths, rows))
}

pub fn upsert_instagram_subscription(
    paths: &AppPaths,
    req: InstagramSubscriptionUpsert,
) -> Result<InstagramSubscriptionRow> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;

    let normalized = normalize_upsert(req)?;
    let now = now_ms();
    let input_id = normalized.id.clone();
    let mut updated_existing = false;

    if let Some(id) = input_id.as_deref() {
        let changed = conn.execute(
            r#"
UPDATE instagram_subscription
SET
  title = ?1,
  source_url = ?2,
  folder_map = ?3,
  output_dir_override = ?4,
  use_browser_cookies = ?5,
  active = ?6,
  refresh_interval_minutes = ?7,
  updated_at_ms = ?8
WHERE id = ?9
"#,
            params![
                normalized.title,
                normalized.source_url,
                normalized.folder_map,
                normalized.output_dir_override,
                bool_to_i64(normalized.use_browser_cookies),
                bool_to_i64(normalized.active),
                normalized.refresh_interval_minutes,
                now,
                id,
            ],
        )?;
        updated_existing = changed > 0;
    }

    if !updated_existing {
        let id = input_id.unwrap_or_else(|| Uuid::new_v4().to_string());
        conn.execute(
            r#"
INSERT INTO instagram_subscription (
  id,
  title,
  source_url,
  folder_map,
  output_dir_override,
  use_browser_cookies,
  active,
  refresh_interval_minutes,
  last_queued_at_ms,
  created_at_ms,
  updated_at_ms
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, ?9, ?9)
ON CONFLICT(source_url) DO UPDATE SET
  title = excluded.title,
  folder_map = excluded.folder_map,
  output_dir_override = excluded.output_dir_override,
  use_browser_cookies = excluded.use_browser_cookies,
  active = excluded.active,
  refresh_interval_minutes = excluded.refresh_interval_minutes,
  updated_at_ms = excluded.updated_at_ms
"#,
            params![
                id,
                normalized.title,
                normalized.source_url,
                normalized.folder_map,
                normalized.output_dir_override,
                bool_to_i64(normalized.use_browser_cookies),
                bool_to_i64(normalized.active),
                normalized.refresh_interval_minutes,
                now,
            ],
        )?;
    }

    let mut row =
        subscription_by_source_url_conn(&conn, &normalized.source_url)?.ok_or_else(|| {
            EngineError::InstallFailed("failed to load saved Instagram subscription".to_string())
        })?;
    sync_auth_session_secret(
        paths,
        row.id.as_str(),
        normalized.auth_session_input.as_deref(),
        normalized.clear_auth_session,
    )?;
    row.auth_session_configured = instagram_subscription_has_auth_session(paths, &row.id);
    Ok(row)
}

pub fn delete_instagram_subscription(paths: &AppPaths, id: &str) -> Result<()> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    conn.execute("DELETE FROM instagram_subscription WHERE id = ?1", [id])?;
    jobs::remove_auth_cookie_secret_path(&paths.instagram_subscription_cookie_secret_path(id));
    Ok(())
}

pub fn queue_instagram_subscription(paths: &AppPaths, id: &str) -> Result<Vec<jobs::JobRow>> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    let sub = subscription_by_id_conn(&conn, id)?.ok_or_else(|| {
        EngineError::InstallFailed(format!("instagram subscription not found: {id}"))
    })?;
    drop(conn);
    queue_subscription_internal(paths, &sub)
}

pub fn queue_all_active_instagram_subscriptions(paths: &AppPaths) -> Result<Vec<jobs::JobRow>> {
    let rows = list_instagram_subscriptions(paths)?;
    let now = now_ms();
    let mut queued_jobs: Vec<jobs::JobRow> = Vec::new();
    for sub in rows {
        if !sub.active || !is_subscription_due(&sub, now) {
            continue;
        }
        let mut queued = queue_subscription_internal(paths, &sub)?;
        queued_jobs.append(&mut queued);
    }
    Ok(queued_jobs)
}

pub fn instagram_subscription_output_dir(
    paths: &AppPaths,
    sub: &InstagramSubscriptionRow,
) -> Result<PathBuf> {
    if let Some(override_dir) = normalize_output_dir(sub.output_dir_override.clone()) {
        let mut out = PathBuf::from(override_dir);
        if !out.is_absolute() {
            out = std::env::current_dir()?.join(out);
        }
        return Ok(out);
    }

    let base_dir = paths.effective_download_dir()?;
    Ok(base_dir
        .join("instagram")
        .join("subscriptions")
        .join(sanitize_folder_map(&sub.folder_map)))
}

fn queue_subscription_internal(
    paths: &AppPaths,
    sub: &InstagramSubscriptionRow,
) -> Result<Vec<jobs::JobRow>> {
    let output_dir = instagram_subscription_output_dir(paths, sub)?
        .to_string_lossy()
        .to_string();
    let auth_cookie = jobs::read_auth_cookie_secret_path(
        &paths.instagram_subscription_cookie_secret_path(&sub.id),
    );
    let queued = jobs::enqueue_download_instagram_batch(
        paths,
        vec![sub.source_url.clone()],
        auth_cookie,
        Some(output_dir),
        Some(sub.use_browser_cookies),
    )?;

    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    conn.execute(
        "UPDATE instagram_subscription SET last_queued_at_ms = ?1, updated_at_ms = ?1 WHERE id = ?2",
        params![now_ms(), sub.id],
    )?;

    Ok(queued)
}

fn is_subscription_due(sub: &InstagramSubscriptionRow, now_ms_value: i64) -> bool {
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

fn hydrate_auth_session_flags(
    paths: &AppPaths,
    mut rows: Vec<InstagramSubscriptionRow>,
) -> Vec<InstagramSubscriptionRow> {
    for row in rows.iter_mut() {
        row.auth_session_configured = instagram_subscription_has_auth_session(paths, &row.id);
    }
    rows
}

fn instagram_subscription_has_auth_session(paths: &AppPaths, subscription_id: &str) -> bool {
    paths
        .instagram_subscription_cookie_secret_path(subscription_id)
        .exists()
}

fn sync_auth_session_secret(
    paths: &AppPaths,
    subscription_id: &str,
    auth_session_input: Option<&str>,
    clear_auth_session: bool,
) -> Result<()> {
    let secret_path = paths.instagram_subscription_cookie_secret_path(subscription_id);
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

fn normalize_upsert(
    req: InstagramSubscriptionUpsert,
) -> Result<NormalizedInstagramSubscriptionInput> {
    let title = normalize_title(req.title)?;
    let source_url = normalize_instagram_url(req.source_url)?;
    let folder_map = req
        .folder_map
        .as_deref()
        .map(sanitize_folder_map)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default_folder_map(&title, &source_url));

    Ok(NormalizedInstagramSubscriptionInput {
        id: req
            .id
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        title,
        source_url,
        folder_map,
        output_dir_override: normalize_output_dir(req.output_dir_override),
        use_browser_cookies: req.use_browser_cookies,
        auth_session_input: jobs::normalize_auth_cookie(req.auth_session_input)?,
        clear_auth_session: req.clear_auth_session,
        active: req.active,
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
            "instagram subscription title cannot be empty".to_string(),
        ));
    }
    let mut out = trimmed.to_string();
    if out.len() > 200 {
        out.truncate(200);
    }
    Ok(out)
}

fn normalize_instagram_url(raw: String) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(EngineError::InstallFailed(
            "instagram subscription URL cannot be empty".to_string(),
        ));
    }
    let mut parsed = Url::parse(trimmed)
        .map_err(|_| EngineError::InstallFailed(format!("invalid URL: {trimmed}")))?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err(EngineError::InstallFailed(
            "instagram subscription URL must use http/https".to_string(),
        ));
    }
    let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
    let is_instagram =
        host == "instagram.com" || host == "www.instagram.com" || host.ends_with(".instagram.com");
    if !is_instagram {
        return Err(EngineError::InstallFailed(
            "instagram subscription URL must be an instagram.com URL".to_string(),
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
            .and_then(|mut segments| segments.next_back())
            .unwrap_or_default();
        let from_url = sanitize_folder_map(path);
        if !from_url.is_empty() {
            return from_url;
        }
    }
    DEFAULT_FOLDER_MAP.to_string()
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

fn row_to_subscription(row: &rusqlite::Row<'_>) -> rusqlite::Result<InstagramSubscriptionRow> {
    Ok(InstagramSubscriptionRow {
        id: row.get(0)?,
        title: row.get(1)?,
        source_url: row.get(2)?,
        folder_map: row.get(3)?,
        output_dir_override: row.get(4)?,
        use_browser_cookies: i64_to_bool(row.get::<_, i64>(5)?),
        auth_session_configured: false,
        active: i64_to_bool(row.get::<_, i64>(6)?),
        refresh_interval_minutes: row.get(7)?,
        last_queued_at_ms: row.get(8)?,
        created_at_ms: row.get(9)?,
        updated_at_ms: row.get(10)?,
    })
}

fn subscription_by_id_conn(
    conn: &rusqlite::Connection,
    id: &str,
) -> Result<Option<InstagramSubscriptionRow>> {
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
  refresh_interval_minutes,
  last_queued_at_ms,
  created_at_ms,
  updated_at_ms
FROM instagram_subscription
WHERE id = ?1
"#,
    )?;
    stmt.query_row([id], row_to_subscription)
        .optional()
        .map_err(Into::into)
}

fn subscription_by_source_url_conn(
    conn: &rusqlite::Connection,
    source_url: &str,
) -> Result<Option<InstagramSubscriptionRow>> {
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
  refresh_interval_minutes,
  last_queued_at_ms,
  created_at_ms,
  updated_at_ms
FROM instagram_subscription
WHERE source_url = ?1
"#,
    )?;
    stmt.query_row([source_url], row_to_subscription)
        .optional()
        .map_err(Into::into)
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
struct NormalizedInstagramSubscriptionInput {
    id: Option<String>,
    title: String,
    source_url: String,
    folder_map: String,
    output_dir_override: Option<String>,
    use_browser_cookies: bool,
    auth_session_input: Option<String>,
    clear_auth_session: bool,
    active: bool,
    refresh_interval_minutes: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_instagram_subscription_uses_subscription_output_dir() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        crate::db::ensure_schema(&paths).expect("schema");
        paths
            .set_download_dir_override(&dir.path().join("downloads"))
            .expect("set download dir");

        let sub = upsert_instagram_subscription(
            &paths,
            InstagramSubscriptionUpsert {
                id: None,
                title: "Archive profile".to_string(),
                source_url: "https://www.instagram.com/example/".to_string(),
                folder_map: Some("example_profile".to_string()),
                output_dir_override: None,
                use_browser_cookies: true,
                auth_session_input: None,
                clear_auth_session: false,
                active: true,
                refresh_interval_minutes: Some(60),
            },
        )
        .expect("upsert");

        let queued = queue_instagram_subscription(&paths, &sub.id).expect("queue");
        assert_eq!(queued.len(), 1);

        let conn = crate::db::open(&paths).expect("db");
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
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        assert!(
            output_dir.contains("instagram")
                && output_dir.contains("subscriptions")
                && output_dir.contains("example_profile"),
            "expected instagram subscription folder in output_dir, got {output_dir}"
        );
    }

    #[test]
    fn upsert_saved_auth_session_persists_secret_and_attaches_to_jobs() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        crate::db::ensure_schema(&paths).expect("schema");

        let sub = upsert_instagram_subscription(
            &paths,
            InstagramSubscriptionUpsert {
                id: None,
                title: "Auth profile".to_string(),
                source_url: "https://www.instagram.com/authprofile/".to_string(),
                folder_map: Some("authprofile".to_string()),
                output_dir_override: None,
                use_browser_cookies: false,
                auth_session_input: Some("sessionid=abc123".to_string()),
                clear_auth_session: false,
                active: true,
                refresh_interval_minutes: Some(60),
            },
        )
        .expect("upsert");

        assert!(
            sub.auth_session_configured,
            "saved auth session should be surfaced"
        );
        let stored = jobs::read_auth_cookie_secret_path(
            &paths.instagram_subscription_cookie_secret_path(&sub.id),
        )
        .expect("subscription auth secret");
        assert_eq!(stored, "sessionid=abc123");

        let queued = queue_instagram_subscription(&paths, &sub.id).expect("queue");
        let job_secret =
            jobs::read_auth_cookie_secret_path(&paths.job_cookie_secret_path(&queued[0].id))
                .expect("job auth secret");
        assert_eq!(job_secret, "sessionid=abc123");
    }
}
