//! Bounded process-local telemetry. Canonical job status remains in the job store.
use crate::paths::AppPaths;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Activity {
    pub updated_at_ms: i64,
    pub transfer_updated_at_ms: i64,
    pub phase: String,
    pub title: Option<String>,
    pub downloaded_bytes: Option<f64>,
    pub total_bytes: Option<f64>,
    pub speed: Option<f64>,
    pub eta: Option<f64>,
    pub lines: VecDeque<String>,
}
static LIVE: OnceLock<Mutex<HashMap<String, Activity>>> = OnceLock::new();
fn key(paths: &AppPaths, id: &str) -> String { format!("{}:{id}", paths.job_logs_dir().display()) }
fn now() -> i64 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64 }
fn number(value: &serde_json::Value, name: &str) -> Option<f64> {
    value[name].as_f64().filter(|v| v.is_finite() && *v >= 0.0)
}
pub fn get(paths: &AppPaths, id: &str) -> Option<Activity> {
    LIVE.get_or_init(Default::default).lock().ok()?.get(&key(paths, id)).cloned()
}
/// WP-0321 S6: a reopened/retried durable row starts a new attempt on the same id. Drop the prior
/// attempt's live telemetry (phase, transfer stats, log lines) so the next attempt's worker starts
/// from a clean slate instead of a stale finished/failed snapshot bleeding into the new run.
pub fn reset(paths: &AppPaths, id: &str) {
    if let Ok(mut all) = LIVE.get_or_init(Default::default).lock() {
        all.remove(&key(paths, id));
    }
}
pub fn note(paths: &AppPaths, id: &str, message: &str) {
    let prior = get(paths, id).unwrap_or_default();
    let mut all = LIVE.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    let key = key(paths, id);
    if !all.contains_key(&key) && all.len() >= 256 {
        if let Some(oldest) = all.iter().min_by_key(|(_, v)| v.updated_at_ms).map(|(k, _)| k.clone()) { all.remove(&oldest); }
    }
    let row = all.entry(key).or_insert(prior);
    row.updated_at_ms = now();
    row.lines.push_back(format!("{}  {}", row.updated_at_ms, message.chars().take(400).collect::<String>()));
    while row.lines.len() > 40 { row.lines.pop_front(); }
}
pub fn observe(paths: &AppPaths, id: &str, phase: &str, message: &str, progress: Option<&serde_json::Value>, title: Option<&str>) {
    let Ok(mut all) = LIVE.get_or_init(Default::default).lock() else { return; };
    let key = key(paths, id);
    if !all.contains_key(&key) && all.len() >= 256 {
        if let Some(oldest) = all.iter().min_by_key(|(_, v)| v.updated_at_ms).map(|(k, _)| k.clone()) { all.remove(&oldest); }
    }
    let row = all.entry(key).or_default();
    row.updated_at_ms = now();
    row.phase = phase.to_string();
    if let Some(title) = title.filter(|s| !s.trim().is_empty()) { row.title = Some(title.chars().take(500).collect()); }
    if let Some(p) = progress {
        row.transfer_updated_at_ms = row.updated_at_ms;
        row.downloaded_bytes = number(p, "downloaded_bytes");
        row.total_bytes = number(p, "total_bytes").or_else(|| number(p, "total_bytes_estimate"));
        row.speed = number(p, "speed");
        row.eta = number(p, "eta");
    } else if phase != "downloading" {
        row.speed = None;
        row.eta = None;
        row.downloaded_bytes = None;
        row.total_bytes = None;
    }
    if !message.is_empty() && !row.lines.back().is_some_and(|line| line.ends_with(&format!("  {message}"))) {
        row.lines.push_back(format!("{}  {}", row.updated_at_ms, message.chars().take(400).collect::<String>()));
        while row.lines.len() > 40 { row.lines.pop_front(); }
    }
}

/// Only explicit provider templates are interpreted as telemetry; never publish raw info JSON.
pub fn provider_line(paths: &AppPaths, id: &str, line: &str) {
    if let Some(raw) = line.trim().strip_prefix("VV_ACTIVITY:") {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else { return; };
        let p = &value["progress"];
        let title = value["title"].as_str();
        let phase = if p["status"] == "finished" { "processing" } else { "downloading" };
        let message = if phase == "processing" { "Transfer finished; preparing the saved media".to_string() } else { format!("Receiving media: {} bytes", number(p, "downloaded_bytes").unwrap_or(0.0) as u64) };
        observe(paths, id, phase, &message, Some(p), title);
    } else if let Some(raw) = line.trim().strip_prefix("VV_POSTPROCESS:") {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) {
            if value["postprocessor"] == "VoxVulgiArchivePolicy" {
                observe(paths, id, "preparing", "Selecting source quality and subtitles", None, None);
            } else {
                observe(paths, id, "processing", "Combining media and embedding subtitles", None, None);
            }
        }
    } else if let Some(raw) = line.trim().strip_prefix("VV_MEDIA_PRE:") {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) {
            observe(paths, id, "preparing", "Media identified; preparing transfer", None, value["title"].as_str());
        }
    } else if line.starts_with('[') || line.starts_with("WARNING:") || line.starts_with("ERROR:") {
        // Human-readable output is a diagnostic message, never a state/progress protocol.
        let lower = line.to_ascii_lowercase();
        if ["cookie", "authorization", "token", "password", "secret", "bearer"].iter().any(|key| lower.contains(key)) {
            note(paths, id, "Downloader reported authentication/session information (redacted)");
        } else {
            static URLS: OnceLock<regex::Regex> = OnceLock::new();
            let urls = URLS.get_or_init(|| regex::Regex::new(r"https?://\S+").unwrap());
            let message = urls.replace_all(line.trim(), "<source URL>");
            note(paths, id, &crate::jobs::redact_auth_credential_locators(&message));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn numeric_progress_rejects_unknown_and_negative_values() {
        assert_eq!(number(&serde_json::json!({"eta": "NA"}), "eta"), None);
        assert_eq!(number(&serde_json::json!({"eta": -1}), "eta"), None);
        assert_eq!(number(&serde_json::json!({"eta": 12}), "eta"), Some(12.0));
    }
    #[test]
    fn provider_activity_is_structured_bounded_and_has_no_raw_secrets() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().to_path_buf());
        provider_line(&paths, "a", r#"VV_MEDIA_PRE:{"title":"A real title","cookies":"TOP_SECRET"}"#);
        provider_line(&paths, "a", r#"VV_ACTIVITY:{"title":"A real title","progress":{"status":"downloading","downloaded_bytes":50,"total_bytes":100,"speed":20,"eta":3,"info_dict":{"token":"TOP_SECRET"}}}"#);
        let activity = get(&paths, "a").unwrap();
        assert_eq!(activity.title.as_deref(), Some("A real title"));
        assert_eq!(activity.downloaded_bytes, Some(50.0));
        assert_eq!(activity.phase, "downloading");
        assert!(!serde_json::to_string(&activity).unwrap().contains("TOP_SECRET"));
        provider_line(&paths, "a", "WARNING: cookie TOP_SECRET https://a.test?token=TOP_SECRET");
        provider_line(&paths, "a", "[download] https://a.test/private?key=TOP_SECRET");
        assert!(!serde_json::to_string(&get(&paths, "a")).unwrap().contains("TOP_SECRET"));
        provider_line(&paths, "a", "VV_ACTIVITY:broken");
        assert_eq!(get(&paths, "a").unwrap().downloaded_bytes, Some(50.0));
        provider_line(&paths, "a", r#"VV_POSTPROCESS:{"status":"started","postprocessor":"VoxVulgiArchivePolicy"}"#);
        assert_eq!(get(&paths, "a").unwrap().phase, "preparing");
        provider_line(&paths, "a", r#"VV_POSTPROCESS:{"status":"started"}"#);
        let activity = get(&paths, "a").unwrap();
        assert_eq!(activity.phase, "processing");
        assert_eq!(activity.speed, None);
        assert_eq!(activity.total_bytes, None);
        for index in 0..100 { note(&paths, "a", &format!("step {index}")); }
        assert_eq!(get(&paths, "a").unwrap().lines.len(), 40);
        let other = AppPaths::new(dir.path().join("other"));
        assert!(get(&other, "a").is_none());
    }

    #[test]
    fn s6_live_activity_reset_on_new_attempt() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().to_path_buf());
        provider_line(&paths, "job-1", r#"VV_ACTIVITY:{"title":"Attempt 1","progress":{"status":"downloading","downloaded_bytes":10,"total_bytes":100}}"#);
        assert!(get(&paths, "job-1").is_some());
        reset(&paths, "job-1");
        assert!(get(&paths, "job-1").is_none());
        // A fresh attempt starts clean, not carrying over the previous attempt's phase/bytes.
        provider_line(&paths, "job-1", r#"VV_ACTIVITY:{"title":"Attempt 2","progress":{"status":"downloading","downloaded_bytes":0,"total_bytes":200}}"#);
        let activity = get(&paths, "job-1").unwrap();
        assert_eq!(activity.title.as_deref(), Some("Attempt 2"));
        assert_eq!(activity.downloaded_bytes, Some(0.0));
        assert_eq!(activity.total_bytes, Some(200.0));
    }
}
