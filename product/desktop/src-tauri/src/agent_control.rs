//! Product-owned, token-authenticated agent operations; no arbitrary IPC or SQL execution.
use super::*;
use serde_json::{json, Value};
use std::collections::HashMap;

const CATALOG: &str = include_str!("../../src/lib/agentManual.json");
static OPERATIONS: OnceLock<Mutex<HashMap<String, Value>>> = OnceLock::new();
static SESSION_STARTED: OnceLock<i64> = OnceLock::new();
static MUTATION: OnceLock<Mutex<()>> = OnceLock::new();

static EXPLICIT_HEADLESS_RUNNER: OnceLock<Mutex<Option<jobs::JobRunnerHandle>>> = OnceLock::new();
pub(super) fn stop_explicit_runner() -> bool {
    let Some(runner) = EXPLICIT_HEADLESS_RUNNER.get() else { return true; };
    let owned = runner.lock().unwrap().take();
    owned.is_none_or(|runner|runner.stop_and_join(jobs::JOB_RUNNER_SHUTDOWN_TIMEOUT).is_ok())
}

pub(super) fn catalog() -> Value {
    serde_json::from_str(CATALOG).expect("embedded agent manual must be valid JSON")
}

pub(super) fn capabilities() -> Value {
    let mut value = catalog();
    let actions_enabled = agent_bridge_state().lock().unwrap().agent_headless || agent_live_actions_enabled();
    value["runtime"] = json!({
        "app_version": env!("CARGO_PKG_VERSION"),
        "agent_headless": agent_bridge_state().lock().unwrap().agent_headless,
        "actions_enabled": actions_enabled,
        "disabled_reason": if actions_enabled { Value::Null } else { json!("VOXVULGI_AGENT_LIVE_ACTIONS explicitly disables live mutations; inspection remains available") },
        "backend_independent_of_webview": true,
        "computer_use_required": false
    });
    value
}

fn reconcile_receipt(mut value: Value) -> Value {
    if (value["status"] == "accepted" || value["status"] == "running") && (value["process_id"].as_u64() != Some(std::process::id() as u64) || value["session_started_at_ms"].as_i64() != Some(*SESSION_STARTED.get_or_init(now_epoch_ms_i64))) {
        value["status"] = json!("interrupted");
        value["recovery"] = json!("inspect canonical original and replacement jobs before issuing a new operation_id");
    }
    value
}

fn validate_input(request: &Value, descriptor: &Value) -> Result<(), String> {
    let schema = &descriptor["input_schema"];
    let object = request.as_object().ok_or("request must be an object")?;
    let properties = schema["properties"].as_object().ok_or("command schema unavailable")?;
    for required in schema["required"].as_array().ok_or("command schema unavailable")? {
        let key = required.as_str().unwrap_or("");
        if key != "bridge_token" && !object.contains_key(key) { return Err(format!("missing {key}")); }
    }
    for (key, value) in object {
        let rule = properties.get(key).ok_or_else(||format!("unsupported field: {key}"))?;
        match rule["type"].as_str() {
            Some("string") if !value.is_string() => return Err(format!("{key} must be a string")),
            Some("integer") => {
                let n = value.as_u64().ok_or_else(||format!("{key} must be a nonnegative integer"))?;
                if n < rule["minimum"].as_u64().unwrap_or(0) || n > rule["maximum"].as_u64().unwrap_or(u64::MAX) { return Err(format!("{key} is outside the declared range")); }
            },
            Some("array") => {
                let list = value.as_array().ok_or_else(||format!("{key} must be an array"))?;
                if list.is_empty() || list.len() > rule["maxItems"].as_u64().unwrap_or(25) as usize || list.iter().any(|v|!v.is_string()) { return Err(format!("invalid {key} array")); }
                for (index, value) in list.iter().enumerate() {
                    if list[..index].contains(value) { return Err(format!("duplicate {key} entry")); }
                }
            },
            _ => {}
        }
    }
    Ok(())
}

fn token(value: &str) -> bool {
    !value.is_empty() && value.len() <= 80 && value.bytes().all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
}

fn ids(request: &Value) -> Result<Vec<String>, String> {
    let values = request["job_ids"].as_array().ok_or("job_ids array is required")?;
    if values.is_empty() || values.len() > 25 { return Err("select 1-25 exact job IDs".into()); }
    let mut result = Vec::new();
    for value in values {
        let id = value.as_str().ok_or("job IDs must be strings")?;
        if !token(id) || result.iter().any(|v| v == id) { return Err("invalid or duplicate job ID".into()); }
        result.push(id.to_string());
    }
    result.sort();
    Ok(result)
}

fn confirm(request: &Value, command: &str, ids: &[String]) -> Result<(), String> {
    let expected = format!("{}:{}", command.to_ascii_uppercase(), ids.join(","));
    if request["confirmation"].as_str() != Some(&expected) {
        return Err(format!("exact confirmation required: {expected}"));
    }
    Ok(())
}

fn safe_job(row: jobs::JobRow) -> Value {
    let params: Value = serde_json::from_str(&row.params_json).unwrap_or(Value::Null);
    json!({
        "id": row.id, "type": row.job_type, "status": row.status,
        "progress": row.progress, "title": row.target_title, "track": row.track,
        "batch_id": row.batch_id, "item_id": row.item_id, "attempt_no": row.attempt_no,
        "error": row.error.map(|v|redact_diagnostics_value(json!(v))),
        "source_url": params.get("url").cloned().map(redact_diagnostics_value), "created_at_ms": row.created_at_ms,
        "started_at_ms": row.started_at_ms, "finished_at_ms": row.finished_at_ms,
        // WP-0321 S6: downloads reuse the same row on retry (attempt_no > 1 is the signal);
        // other job types still insert+link a replacement row and populate these fields.
        "retry_of_job_id": row.retry_of_job_id, "replacement_job_id": row.retry_replacement_job_id,
        "request_sleep_secs": params.get("yt_dlp_sleep_requests"),
        "download_sleep_secs": params.get("yt_dlp_sleep_interval"),
        "preset_id": params.get("preset_id"),
        "stage": if row.job_type == "download_direct_url" && row.status == jobs::JobStatus::Running && row.progress <= 0.05001 {
            "preparing_metadata_or_waiting_for_provider_pacing"
        } else { "job_progress" }
    })
}

/// WP-0321 S6: bounded (<=5) attempt history for a job from `job_attempt`, newest first. No
/// engine helper exposes this shape yet, so this reads the same table the design's
/// `reopen_terminal_download_conn` archives into, through the standard read-only lane.
fn recent_attempts(paths: &AppPaths, job_id: &str) -> Result<Vec<Value>, String> {
    let conn = db::open_readonly(paths).map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT attempt_no,status,error,started_at_ms,finished_at_ms,batch_id FROM job_attempt WHERE job_id=?1 ORDER BY attempt_no DESC LIMIT 5")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![job_id], |r| {
            Ok(json!({
                "attempt_no": r.get::<_, i64>(0)?,
                "status": r.get::<_, String>(1)?,
                "error": r.get::<_, Option<String>>(2)?,
                "started_at_ms": r.get::<_, Option<i64>>(3)?,
                "finished_at_ms": r.get::<_, Option<i64>>(4)?,
                "batch_id": r.get::<_, Option<String>>(5)?,
            }))
        })
        .map_err(|e| e.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

/// WP-0321 S6: `jobs.retry` / `downloads.restart_current` reopen the same row for downloads
/// (`row.id == original_id`, `attempt_no` incremented); other job types still insert a
/// replacement row, so `reopened` is false and the replacement carries its own id.
fn reopen_receipt(original_id: &str, row: jobs::JobRow) -> Value {
    json!({
        "original_job_id": original_id, "ok": true,
        "job": {"id": row.id, "attempt_no": row.attempt_no},
        "reopened": row.id == original_id
    })
}

fn execute(paths: &AppPaths, command: &str, request: &Value) -> Result<Value, String> {
    match command {
        "subscriptions.failed_downloads" => jobs::failed_subscription_downloads(paths,
            request["since_ms"].as_i64().unwrap_or(0), request["until_ms"].as_i64().unwrap_or_else(now_epoch_ms_i64),
            request["after_id"].as_str().unwrap_or(""), request["limit"].as_u64().unwrap_or(200) as usize).map_err(|e|e.to_string()),
        "youtube.return_to_baseline" => serde_json::to_value(jobs::return_youtube_protection_to_baseline(paths, Some("download")).map_err(|e|e.to_string())?).map_err(|e|e.to_string()),
        "youtube.controlled_probe" => serde_json::to_value(jobs::request_youtube_controlled_probe(paths).map_err(|e|e.to_string())?).map_err(|e|e.to_string()),
        "queue.resume" => {
            if agent_bridge_state().lock().unwrap().agent_headless {
                let mut runner = EXPLICIT_HEADLESS_RUNNER.get_or_init(Default::default).lock().unwrap();
                if runner.is_none() { *runner = Some(jobs::start_runner(paths.clone()).map_err(|e|e.to_string())?); }
            }
            jobs::set_recurring_paused(paths, false).map_err(|e|e.to_string())?;
            let control = jobs::set_queue_paused(paths, false).map_err(|e|e.to_string())?;
            Ok(json!({"queue":control,"completion":"resumed; inspect canonical jobs and output quality receipts"}))
        }
        "jobs.activity" => {
            let mut page = jobs::operator_activity_page(paths, request["source"].as_str().unwrap_or("all"), request["view"].as_str().unwrap_or("now"), request["offset"].as_u64().unwrap_or(0) as usize, request["limit"].as_u64().unwrap_or(20) as usize).map_err(|e|e.to_string())?;
            if let Some(rows) = page["jobs"].as_array_mut() {
                for row in rows {
                    if let Some(job) = row["job"].as_object_mut() { job.remove("params_json"); }
                    *row = redact_diagnostics_value(row.clone());
                }
            }
            Ok(page)
        }
        "jobs.list" => {
            let limit = request["limit"].as_u64().unwrap_or(50).clamp(1, 200) as usize;
            let offset = request["offset"].as_u64().unwrap_or(0).min(1_000_000) as usize;
            let rows = jobs::list_jobs(paths, limit, offset).map_err(|e| e.to_string())?;
            Ok(json!({"jobs":rows.into_iter().map(safe_job).collect::<Vec<_>>(),"limit":limit,"offset":offset,"scope":"paged_canonical_job_store","next_offset":offset+limit}))
        }
        "jobs.inspect" => {
            let selected = ids(request)?;
            let rows = selected.iter().map(|id| jobs::get_job(paths, id)
                .map_err(|e| e.to_string())?.ok_or_else(|| format!("job not found: {id}")))
                .collect::<Result<Vec<_>, _>>()?;
            let jobs = rows.into_iter().map(|row| {
                let attempts = recent_attempts(paths, &row.id).unwrap_or_default();
                let mut value = safe_job(row);
                value["attempts"] = json!(attempts);
                value
            }).collect::<Vec<_>>();
            Ok(json!({"jobs":jobs,"scope":"exact_canonical_ids"}))
        }
        "jobs.logs" => {
            use std::io::{Read, Seek, SeekFrom};
            let selected = ids(request)?;
            if selected.len() != 1 { return Err("jobs.logs requires exactly one job ID".into()); }
            let row = jobs::get_job(paths, &selected[0]).map_err(|e|e.to_string())?.ok_or("job not found")?;
            let mut file = std::fs::File::open(&row.logs_path).map_err(|e|e.to_string())?;
            let len = file.metadata().map_err(|e|e.to_string())?.len();
            let start = len.saturating_sub(65536);
            file.seek(SeekFrom::Start(start)).map_err(|e|e.to_string())?;
            let mut bytes = Vec::new();
            file.take(65536).read_to_end(&mut bytes).map_err(|e|e.to_string())?;
            let text = String::from_utf8_lossy(&bytes);
            let lines = text.lines().skip(if start > 0 {1}else{0}).map(|line| {
                redact_diagnostics_value(serde_json::from_str::<Value>(line).unwrap_or_else(|_|json!(line)))
            }).collect::<Vec<_>>();
            Ok(json!({"job_id":row.id,"tail":lines,"truncated":start>0,"max_bytes":65536}))
        }
        "jobs.overview" => {
            let snapshot = jobs::jobs_overview_snapshot(paths, request["view"].as_str(), request["track"].as_str()).map_err(|e|e.to_string())?;
            let rows = snapshot.jobs.clone().into_iter().map(safe_job).collect::<Vec<_>>();
            let mut value = serde_json::to_value(snapshot).map_err(|e|e.to_string())?;
            value["jobs"] = json!(rows);
            value["scope"] = json!("canonical_counts_with_bounded_previews");
            Ok(value)
        }
        "downloads.presets" => serde_json::to_value(config::load_download_presets_config(paths).map_err(|e|e.to_string())?).map_err(|e|e.to_string()),
        "downloads.pacing" => {
            let expected = request["preset_id"].as_str().ok_or("preset_id is required")?;
            let sleep = request["request_sleep_secs"].as_u64().filter(|v| *v <= 600).ok_or("request_sleep_secs must be 0-600")?;
            let download = request["download_sleep_secs"].as_u64().filter(|v| *v <= 3600).ok_or("download_sleep_secs must be 0-3600")?;
            let saved = config::update_download_presets_config(paths, |mut current| {
                if current.default_preset_id.as_deref() != Some(expected) {
                    return Err(voxvulgi_engine::EngineError::InstallFailed("default preset changed; reload".into()));
                }
                let preset = current.presets.iter_mut().find(|p| p.id == expected)
                    .ok_or_else(|| voxvulgi_engine::EngineError::InstallFailed("preset missing".into()))?;
                preset.yt_dlp_sleep_requests = sleep as u32;
                preset.yt_dlp_sleep_interval = download as u32;
                Ok(current)
            }).map_err(|e|e.to_string())?;
            Ok(json!({"saved":saved,"running_attempts_changed":false,"applies_to":"new submissions, retries and queued jobs at execution"}))
        }
        "downloads.enqueue" => {
            let urls = request["urls"].as_array().ok_or("urls is required")?;
            if urls.is_empty() || urls.len() > 25 { return Err("provide 1-25 URLs".into()); }
            let urls = urls.iter().map(|v| v.as_str().filter(|s|s.len()<=2048).map(str::to_string).ok_or("invalid URL".to_string())).collect::<Result<Vec<_>,_>>()?;
            let rows = jobs::enqueue_download_direct_url_batch_with_repairs(paths, urls, None,
                request["output_dir"].as_str().map(str::to_string), None, None,
                request["preset_id"].as_str().map(str::to_string), Vec::new()).map_err(|e|e.to_string())?;
            Ok(json!({"jobs":rows.into_iter().map(safe_job).collect::<Vec<_>>(),"completion":"queued_only; inspect canonical jobs for progress"}))
        }
        "jobs.retry" | "downloads.restart_current" | "jobs.cancel" => {
            let selected = ids(request)?;
            if command != "jobs.retry" { confirm(request, command, &selected)?; }
            // Validate the whole selection before changing any row.
            for id in &selected {
                let row = jobs::get_job(paths, id).map_err(|e|e.to_string())?.ok_or_else(||format!("job not found: {id}"))?;
                if command == "downloads.restart_current" && row.job_type != "download_direct_url" { return Err(format!("{id} is not a direct video download")); }
            }
            let results = selected.iter().map(|id| {
                match command {
                    "downloads.restart_current" => jobs::restart_download_current(paths, id).map(|row| reopen_receipt(id, row)),
                    "jobs.retry" => jobs::retry_job(paths, id).map(|row| reopen_receipt(id, row)),
                    _ => jobs::cancel_job(paths, id).and_then(|_| jobs::get_job(paths,id)).map(|r|json!({"original_job_id":id,"ok":true,"job":r.map(safe_job).unwrap_or(Value::Null)}))
                }.unwrap_or_else(|e| json!({"original_job_id":id,"ok":false,"error":e.to_string(),"recovery":"inspect job attempts; repeating restart_current is idempotent"}))
            }).collect::<Vec<_>>();
            Ok(json!({"all_succeeded":results.iter().all(|r|r["ok"]==true),"results":results,"completion":"operation result; verify replacement progress separately"}))
        }
        "jobs.purge_terminal_history" => {
            let older_than_days = request["older_than_days"].as_u64().filter(|v| *v <= 3650).ok_or("older_than_days must be 0-3650")? as u32;
            let include_succeeded = request["include_succeeded"].as_bool().ok_or("include_succeeded must be a boolean")?;
            let dry_run = request["dry_run"].as_bool().ok_or("dry_run must be a boolean")?;
            if !dry_run {
                let expected = format!("JOBS.PURGE_TERMINAL_HISTORY:{older_than_days}:{include_succeeded}");
                if request["confirmation"].as_str() != Some(expected.as_str()) {
                    return Err(format!("exact confirmation required: {expected}"));
                }
            }
            serde_json::to_value(jobs::purge_terminal_job_history(paths, older_than_days, include_succeeded, dry_run).map_err(|e|e.to_string())?).map_err(|e|e.to_string())
        }
        _ => Err("unsupported command; read /agent/capabilities and /agent/manual".into())
    }
}

fn is_read(command: &str) -> bool {
    matches!(command, "jobs.list" | "jobs.inspect" | "jobs.logs" | "jobs.overview" | "jobs.activity" | "downloads.presets" | "subscriptions.failed_downloads" | "operation.get")
}

pub(super) fn handle(body: &str) -> (&'static str, String) {
    let request = match validate_agent_ui_request("audit", true, true, AGENT_UI_ACTION_TOKEN.get().map(String::as_str), body) {
        Ok(value) => value, Err(error) => return error,
    };
    let command = request["command"].as_str().unwrap_or("").to_string();
    let actor = request["actor_id"].as_str().unwrap_or("");
    if !token(actor) { return ("400 Bad Request", json!({"error":"actor_id is required (1-80 letters, digits, dot, colon, dash, underscore)"}).to_string()); }
    let Some(app) = AGENT_APP_HANDLE.get() else { return ("503 Service Unavailable", "{}".into()); };
    let Some(state) = app.try_state::<AppState>() else {
        return ("503 Service Unavailable", json!({"error":"application startup is not ready; retry after startup completes"}).to_string());
    };
    if command == "operation.get" {
        let id = request["operation_id"].as_str().unwrap_or("");
        if !token(id) { return ("400 Bad Request",json!({"error":"invalid operation_id"}).to_string()); }
        let key = format!("{:x}", Sha256::digest(format!("{actor}\0{id}").as_bytes()));
        let receipt = OPERATIONS.get_or_init(Default::default).lock().unwrap().get(&key).cloned()
            .or_else(|| std::fs::read_to_string(state.paths.base_dir.join("diagnostics/agent_operations").join(format!("{key}.json"))).ok().and_then(|s|serde_json::from_str(&s).ok()));
        return match receipt { Some(r)=>("200 OK",reconcile_receipt(r).to_string()), None=>("404 Not Found",json!({"error":"operation not found"}).to_string()) };
    }
    let descriptor = catalog()["commands"].as_array().unwrap().iter().find(|c|c["name"]==command).cloned();
    let Some(descriptor) = descriptor else {
        return ("400 Bad Request",json!({"error":"unknown command","discovery":"/agent/capabilities"}).to_string());
    };
    if let Err(error) = validate_input(&request, &descriptor) {
        return ("400 Bad Request",json!({"error":error}).to_string());
    }
    if is_read(&command) {
        return match execute(&state.paths,&command,&request) {
            Ok(value)=>("200 OK",json!({"ok":true,"command":command,"result":value}).to_string()),
            Err(error)=>("400 Bad Request",json!({"ok":false,"error":error}).to_string())
        };
    }
    if !agent_bridge_state().lock().unwrap().agent_headless && !agent_live_actions_enabled() {
        return ("403 Forbidden",json!({"error":"live actions explicitly disabled; inspection remains available"}).to_string());
    }
    let operation = request["operation_id"].as_str().unwrap_or("");
    if !token(operation) { return ("400 Bad Request",json!({"error":"operation_id required for mutations"}).to_string()); }
    let key = format!("{:x}", Sha256::digest(format!("{actor}\0{operation}").as_bytes()));
    let hash = format!("{:x}",Sha256::digest(request.to_string().as_bytes()));
    let paths = state.paths.clone();
    let folder = paths.base_dir.join("diagnostics/agent_operations");
    let receipt_path = folder.join(format!("{key}.json"));
    let mut operations = OPERATIONS.get_or_init(Default::default).lock().unwrap();
    let prior = operations.get(&key).cloned().or_else(||std::fs::read_to_string(&receipt_path).ok().and_then(|s|serde_json::from_str::<Value>(&s).ok()));
    if let Some(prior) = prior {
        if prior["request_hash"] != hash { return ("409 Conflict",json!({"error":"operation_id already used for different input"}).to_string()); }
        return ("200 OK",reconcile_receipt(prior).to_string());
    }
    if operations.values().filter(|r|r["status"]=="accepted" || r["status"]=="running").count() >= 4 {
        return ("429 Too Many Requests",json!({"error":"four backend operations already active; poll and retry"}).to_string());
    }
    operations.retain(|_,r|r["status"]=="accepted" || r["status"]=="running");
    let receipt = json!({"operation_id":operation,"actor_id":actor,"command":command,"request_hash":hash,"status":"accepted","process_id":std::process::id(),"session_started_at_ms":*SESSION_STARTED.get_or_init(now_epoch_ms_i64),"started_at_ms":now_epoch_ms_i64(),"poll":"POST /agent/command with command=operation.get and same actor_id/operation_id"});
    if let Err(error)=std::fs::create_dir_all(&folder).map_err(|e|e.to_string()).and_then(|_|write_json_atomic(&receipt_path,&receipt)) {
        return ("500 Internal Server Error",json!({"error":format!("cannot persist operation before execution: {error}")}).to_string());
    }
    operations.insert(key.clone(),receipt.clone());
    drop(operations);
    let mut final_receipt = receipt.clone();
    std::thread::spawn(move || {
        let _serial = MUTATION.get_or_init(||Mutex::new(())).lock().unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(||execute(&paths,&command,&request)));
        match result {
            Ok(Ok(value)) => { final_receipt["status"]=json!(if value.get("all_succeeded")==Some(&json!(false)){"partial_failure"}else{"completed"}); final_receipt["result"]=value; },
            Ok(Err(error)) => {final_receipt["status"]=json!("failed");final_receipt["error"]=json!(error);},
            Err(_) => {final_receipt["status"]=json!("failed");final_receipt["error"]=json!("operation panicked; inspect canonical jobs before recovery");}
        }
        final_receipt["finished_at_ms"]=json!(now_epoch_ms_i64());
        if let Err(e)=write_json_atomic(&receipt_path,&final_receipt) {final_receipt["receipt_persistence_error"]=json!(e.to_string());}
        OPERATIONS.get_or_init(Default::default).lock().unwrap().insert(key,final_receipt);
    });
    ("202 Accepted",receipt.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fake_job_row(id: &str, attempt_no: u32) -> jobs::JobRow {
        jobs::JobRow {
            id: id.to_string(), item_id: None, batch_id: None, job_type: "download_direct_url".into(),
            status: jobs::JobStatus::Queued, progress: 0.0, error: None, created_at_ms: 0,
            started_at_ms: None, finished_at_ms: None, logs_path: String::new(), params_json: "{}".into(),
            target_title: None, target_title_provenance: None, target_title_problem: None,
            retry_of_job_id: None, retry_replacement_job_id: None, track: "youtube_single".into(), attempt_no,
        }
    }
    // WP-0321 S6: downloads reopen the same row on retry, so the receipt's job.id must equal
    // the original_job_id and attempt_no must reflect the new attempt; `reopened` says so.
    #[test] fn retry_receipt_returns_same_id_and_attempt_no() {
        let receipt = reopen_receipt("job-1", fake_job_row("job-1", 2));
        assert_eq!(receipt["original_job_id"], "job-1");
        assert_eq!(receipt["ok"], true);
        assert_eq!(receipt["job"]["id"], "job-1");
        assert_eq!(receipt["job"]["attempt_no"], 2);
        assert_eq!(receipt["reopened"], true);
        assert!(receipt["job"].get("attempt_no").is_some(), "attempt_no must be present for the operator to see the new try count");
        // Non-download job types still insert a replacement row: a different id means not reopened.
        let replacement = reopen_receipt("job-1", fake_job_row("job-2", 1));
        assert_eq!(replacement["reopened"], false);
    }
    #[test] fn schemas_reject_unknown_fields_and_out_of_range_input() {
        let c = catalog();
        let descriptor = c["commands"].as_array().unwrap().iter().find(|v|v["name"]=="jobs.list").unwrap();
        assert!(validate_input(&json!({"actor_id":"test","command":"jobs.list","limit":201}),descriptor).is_err());
        assert!(validate_input(&json!({"actor_id":"test","command":"jobs.list","sql":"DELETE"}),descriptor).is_err());
        assert!(validate_input(&json!({"actor_id":"test","command":"jobs.list","limit":25}),descriptor).is_ok());
    }
    #[test] fn missing_token_never_reaches_dispatch() {
        let (status, _) = handle(r#"{"command":"jobs.cancel","actor_id":"test"}"#);
        assert_eq!(status, "403 Forbidden");
    }
    #[test] fn interrupted_receipt_is_not_reported_as_still_running() {
        let receipt = reconcile_receipt(json!({"status":"accepted","process_id":0}));
        assert_eq!(receipt["status"],"interrupted");
        assert_eq!(reconcile_receipt(json!({"status":"completed","process_id":0}))["status"],"completed");
    }
    #[test] fn catalog_read_classification_matches_dispatch() {
        for command in catalog()["commands"].as_array().unwrap() {
            assert_eq!(is_read(command["name"].as_str().unwrap()), command["read_only"].as_bool().unwrap(), "{}", command["name"]);
        }
    }
    #[test] fn catalog_and_dispatch_are_explicit() {
        let c=catalog(); assert_eq!(c["schema_version"],1);
        for name in ["jobs.list","jobs.inspect","jobs.overview","downloads.presets","downloads.pacing","downloads.enqueue","jobs.retry","jobs.cancel","downloads.restart_current","operation.get","jobs.purge_terminal_history"] {
            assert!(c["commands"].as_array().unwrap().iter().any(|v|v["name"]==name));
        }
    }
    #[test] fn purge_terminal_history_schema_rejects_out_of_range_and_requires_operation_id() {
        let c = catalog();
        let descriptor = c["commands"].as_array().unwrap().iter().find(|v|v["name"]=="jobs.purge_terminal_history").unwrap();
        assert!(!is_read("jobs.purge_terminal_history"), "dry_run vs execute is a runtime distinction, not a schema one; the command must go through the mutation/operation_id path either way");
        assert!(validate_input(&json!({"actor_id":"test","command":"jobs.purge_terminal_history","older_than_days":4000,"include_succeeded":false,"dry_run":true,"operation_id":"op-1"}),descriptor).is_err(), "older_than_days above 3650 must be rejected");
        assert!(validate_input(&json!({"actor_id":"test","command":"jobs.purge_terminal_history","older_than_days":30,"include_succeeded":false,"dry_run":false}),descriptor).is_err(), "operation_id is required even for an execute run");
        assert!(validate_input(&json!({"actor_id":"test","command":"jobs.purge_terminal_history","older_than_days":30,"include_succeeded":false,"dry_run":true,"operation_id":"op-1"}),descriptor).is_ok(), "dry_run does not require confirmation at the schema level");
        assert!(validate_input(&json!({"actor_id":"test","command":"jobs.purge_terminal_history","older_than_days":30,"include_succeeded":true,"dry_run":false,"confirmation":"JOBS.PURGE_TERMINAL_HISTORY:30:true","operation_id":"op-2"}),descriptor).is_ok());
    }
    #[test] fn exact_selection_and_confirmation() {
        assert!(ids(&json!({"job_ids":[]})).is_err());
        assert!(ids(&json!({"job_ids":["a","a"]})).is_err());
        let selected=ids(&json!({"job_ids":["b","a"]})).unwrap();
        assert!(confirm(&json!({}),"jobs.cancel",&selected).is_err());
        assert!(confirm(&json!({"confirmation":"JOBS.CANCEL:a,b"}),"jobs.cancel",&selected).is_ok());
        assert!(!token("../escape")); assert!(!token(""));
    }
}
