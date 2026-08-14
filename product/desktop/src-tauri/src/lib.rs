use base64::Engine as _;
#[cfg(target_os = "windows")]
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use sha2::{Digest, Sha256};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc::{sync_channel, SyncSender, TrySendError},
    Arc, Mutex, OnceLock,
};
use std::time::{Duration, Instant};
use sysinfo::{ProcessesToUpdate, System};
use tauri::{Emitter, Manager, State};
use tauri_runtime::ResizeDirection as TauriResizeDirection;
#[cfg(target_os = "windows")]
use windows_sys::Win32::{
    Foundation::{HWND, RECT},
    Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits,
        GetWindowDC, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        HGDIOBJ, RGBQUAD, SRCCOPY,
    },
    UI::WindowsAndMessaging::{GetWindowRect, ShowWindowAsync, SW_HIDE},
};

#[cfg(target_os = "windows")]
extern "system" {
    fn PrintWindow(hwnd: HWND, hdcblt: windows_sys::Win32::Graphics::Gdi::HDC, nflags: u32) -> i32;
}

// ---------------------------------------------------------------------------
// Agent Bridge — localhost HTTP API for headless agent control (WP-0171)
// ---------------------------------------------------------------------------

static AGENT_APP_HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();
static AGENT_BRIDGE_STATE: OnceLock<Arc<Mutex<AgentBridgeInner>>> = OnceLock::new();
static AGENT_BRIDGE_FILES_DIR: OnceLock<std::path::PathBuf> = OnceLock::new();
static DIAGNOSTICS_TRACE_WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static YOUTUBE_AUTH_OPERATION_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static YOUTUBE_PROTECTION_MUTATION_GENERATIONS: OnceLock<
    Mutex<std::collections::HashMap<String, u64>>,
> = OnceLock::new();
static YOUTUBE_PROTECTION_MUTATION_LOCKS: OnceLock<
    Mutex<std::collections::HashMap<String, Arc<Mutex<()>>>>,
> = OnceLock::new();
static DIAGNOSTICS_TRACE_QUEUE: OnceLock<SyncSender<DiagnosticsTraceWriteRequest>> =
    OnceLock::new();
static DIAGNOSTICS_TRACE_DROPPED_PENDING: AtomicU64 = AtomicU64::new(0);
static DIAGNOSTICS_TRACE_DROPPED_TOTAL: AtomicU64 = AtomicU64::new(0);
static DIAGNOSTICS_TRACE_ASYNC_WRITE_FAILURES_TOTAL: AtomicU64 = AtomicU64::new(0);
static DIAGNOSTICS_TRACE_QUEUE_REJECTIONS_TOTAL: AtomicU64 = AtomicU64::new(0);
static DIAGNOSTICS_TRACE_ROTATIONS_TOTAL: AtomicU64 = AtomicU64::new(0);
static DIAGNOSTICS_TRACE_COMPRESSED_TOTAL: AtomicU64 = AtomicU64::new(0);
static DIAGNOSTICS_ID_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static DIAGNOSTICS_CAPTURE_STATE: OnceLock<Mutex<DiagnosticsCaptureStatus>> = OnceLock::new();
#[cfg(test)]
static DIAGNOSTICS_CAPTURE_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static AGENT_UI_REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static JOBS_BATCH_OPERATION_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static YOUTUBE_RETENTION_WORKER_RUNNING: AtomicBool = AtomicBool::new(false);
static YOUTUBE_RETENTION_WORKER_CANCELLED: AtomicBool = AtomicBool::new(false);
static YOUTUBE_RETENTION_WORKER_WAKE: OnceLock<(Mutex<()>, std::sync::Condvar)> = OnceLock::new();
static JOBS_BATCH_OPERATIONS: OnceLock<
    Mutex<std::collections::HashMap<String, JobsBatchOperationSnapshot>>,
> = OnceLock::new();
// WP-0221: exposed so the freeze-detector Worker can POST to /agent/freeze_event
// without relying on Tauri IPC (which routes through the WebView main thread we
// are trying to observe).
static AGENT_BRIDGE_PORT: OnceLock<u16> = OnceLock::new();

fn run_youtube_protection_mutation<T, F>(
    operation: &str,
    mutation_generation: u64,
    action: F,
) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String>,
{
    if mutation_generation == 0 {
        return Err("YouTube protection mutation generation must be non-zero".to_string());
    }
    {
        let mut generations = YOUTUBE_PROTECTION_MUTATION_GENERATIONS
            .get_or_init(|| Mutex::new(std::collections::HashMap::new()))
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let latest = generations.entry(operation.to_string()).or_insert(0);
        if mutation_generation < *latest {
            return Err(format!(
                "stale YouTube protection mutation generation {mutation_generation}; latest is {latest}"
            ));
        }
        *latest = mutation_generation;
    }
    let operation_lock = {
        let mut locks = YOUTUBE_PROTECTION_MUTATION_LOCKS
            .get_or_init(|| Mutex::new(std::collections::HashMap::new()))
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Arc::clone(
            locks
                .entry(operation.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(()))),
        )
    };
    let _operation_guard = operation_lock
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let latest = YOUTUBE_PROTECTION_MUTATION_GENERATIONS
        .get_or_init(|| Mutex::new(std::collections::HashMap::new()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(operation)
        .copied()
        .unwrap_or(0);
    if latest != mutation_generation {
        return Err(format!(
            "stale YouTube protection mutation generation {mutation_generation}; latest is {latest}"
        ));
    }
    action()
}

#[derive(Debug, Default)]
struct AgentBridgeInner {
    current_page: String,
    editor_item_id: Option<String>,
    safe_mode: bool,
    agent_headless: bool,
    snapshot_tx: Option<std::sync::mpsc::Sender<String>>,
    dump_tx: Option<std::sync::mpsc::Sender<String>>,
    ui_request_tx: Option<std::sync::mpsc::Sender<String>>,
}

fn agent_bridge_state() -> &'static Arc<Mutex<AgentBridgeInner>> {
    AGENT_BRIDGE_STATE.get_or_init(|| Arc::new(Mutex::new(AgentBridgeInner::default())))
}

#[cfg(target_os = "windows")]
fn hide_agent_headless_window(window: &tauri::WebviewWindow) -> Result<(), String> {
    let handle = window.window_handle().map_err(|error| error.to_string())?;
    let hwnd = match handle.as_raw() {
        RawWindowHandle::Win32(handle) => handle.hwnd.get() as HWND,
        _ => return Err("headless window hide requires a Win32 window".to_string()),
    };
    let accepted = unsafe { ShowWindowAsync(hwnd, SW_HIDE) };
    if accepted == 0 {
        return Err("ShowWindowAsync(SW_HIDE) failed for headless window".to_string());
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn hide_agent_headless_window(window: &tauri::WebviewWindow) -> Result<(), String> {
    window.hide().map_err(|error| error.to_string())
}

fn spawn_agent_bridge(app_data_dir: &std::path::Path) {
    let _ = AGENT_BRIDGE_FILES_DIR.set(app_data_dir.to_path_buf());
    let port_file = app_data_dir.join("agent_bridge_port.txt");
    let json_file = app_data_dir.join("agent_bridge.json");

    std::thread::spawn(move || {
        let listener = match std::net::TcpListener::bind("127.0.0.1:0") {
            Ok(l) => l,
            Err(_) => return,
        };
        let port = match listener.local_addr() {
            Ok(addr) => addr.port(),
            Err(_) => return,
        };
        let _ = AGENT_BRIDGE_PORT.set(port);
        let _ = std::fs::write(&port_file, port.to_string());
        // Sidecar with PID + start time so agents can detect a stale port file
        // (e.g., after a crash) without hitting the network and timing out.
        let _ = std::fs::write(
            &json_file,
            serde_json::json!({
                "port": port,
                "pid": std::process::id(),
                "started_at_ms": now_epoch_ms_i64(),
            })
            .to_string(),
        );

        let _ = listener.set_nonblocking(false);

        for stream in listener.incoming() {
            match stream {
                Ok(mut stream) => {
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
                    std::thread::spawn(move || {
                        handle_agent_request(&mut stream);
                    });
                }
                Err(_) => continue,
            }
        }
    });
}

fn agent_bridge_marker_owned_by_process(raw: &str, process_id: u32) -> bool {
    serde_json::from_str::<serde_json::Value>(raw)
        .ok()
        .and_then(|value| value.get("pid").and_then(|pid| pid.as_u64()))
        .map(|pid| pid == u64::from(process_id))
        .unwrap_or(false)
}

fn cleanup_agent_bridge_files() {
    if let Some(dir) = AGENT_BRIDGE_FILES_DIR.get() {
        let json_path = dir.join("agent_bridge.json");
        let owned = std::fs::read_to_string(&json_path)
            .ok()
            .map(|raw| agent_bridge_marker_owned_by_process(&raw, std::process::id()))
            .unwrap_or(false);
        if owned {
            let _ = std::fs::remove_file(dir.join("agent_bridge_port.txt"));
            let _ = std::fs::remove_file(json_path);
        }
    }
}

/// WP-0252 Item 2a: write the stop sentinel the watcher supervisor polls, so it exits
/// promptly on graceful app shutdown (the PID-liveness gate is the freeze-proof backstop).
fn signal_watcher_stop() {
    if let Some(dir) = AGENT_BRIDGE_FILES_DIR.get() {
        let watch_root = dir.join("diagnostics").join("external_watch");
        let _ = std::fs::create_dir_all(&watch_root);
        let _ = std::fs::write(watch_root.join("stop.flag"), "stop\n");
    }
}

/// WP-0252 Item 2a: launch the bundled external-watcher supervisor detached (no window,
/// not in the app's console/job group) so it survives a WebView freeze, relaunches the
/// watcher if it crashes while the app PID is alive, and exits when the app exits. Lives
/// next to `desktop.exe` under `resource_dir/watcher/`.
#[cfg(windows)]
fn spawn_watcher_supervisor(resource_dir: &std::path::Path, app_data_dir: &std::path::Path) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    let watcher_dir = resource_dir.join("watcher");
    let supervisor = watcher_dir.join("vv_watch_supervisor.ps1");
    if !supervisor.is_file() {
        return;
    }
    let _ = std::process::Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            &supervisor.to_string_lossy(),
            "-AppPid",
            &std::process::id().to_string(),
            "-AppDataDir",
            &app_data_dir.to_string_lossy(),
            "-WatcherDir",
            &watcher_dir.to_string_lossy(),
        ])
        .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
        .spawn();
}

#[cfg(not(windows))]
fn spawn_watcher_supervisor(_resource_dir: &std::path::Path, _app_data_dir: &std::path::Path) {}

/// Default ON; operator can disable without a rebuild via `config/watcher_enabled.txt`
/// containing `0`/`false`/`off`/`no`.
fn watcher_enabled(paths: &AppPaths) -> bool {
    match std::fs::read_to_string(paths.config_dir().join("watcher_enabled.txt")) {
        Ok(value) => {
            let t = value.trim().to_ascii_lowercase();
            !(t == "0" || t == "false" || t == "off" || t == "no")
        }
        Err(_) => true,
    }
}

/// WP-0254: 4KVDP-style startup auto-check + auto-download of due subscriptions. Default
/// ON (operator choice); disablable without a rebuild via `config/subscription_auto_sync.txt`
/// containing `0`/`false`/`off`/`no`. Unlike the WP-0227/WP-0228 pack-install regression,
/// this only enqueues *due* subscription refreshes into the conservative recurring lane
/// (limit 1), so it cannot swamp the app at startup.
fn subscription_auto_sync_enabled(paths: &AppPaths) -> bool {
    match std::fs::read_to_string(paths.config_dir().join("subscription_auto_sync.txt")) {
        Ok(value) => {
            let t = value.trim().to_ascii_lowercase();
            !(t == "0" || t == "false" || t == "off" || t == "no")
        }
        Err(_) => true,
    }
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// WP-0252 Item 1 (self-contained CosyVoice): seed the ~3 MB CosyVoice runtime code
/// (the `cosyvoice` package + Matcha-TTS + prompt assets, pinned commit) from the bundled
/// resource into app-data on first run, so the engine install fn (which downloads the venv
/// + model on demand) has the code it needs. Never clobbers an existing checkout/install.
fn seed_cosyvoice_backend_if_missing(
    resource_dir: &std::path::Path,
    app_data_dir: &std::path::Path,
) {
    let src = resource_dir.join("voice_backends_seed").join("cosyvoice");
    let dst = app_data_dir.join("voice_backends").join("cosyvoice");
    let marker = |root: &std::path::Path| root.join("cosyvoice").join("cli").join("cosyvoice.py");
    if marker(&dst).is_file() {
        return; // already present (full checkout or prior seed) — do not overwrite
    }
    if !marker(&src).is_file() {
        return; // seed resource absent (e.g. a dev build without the resource)
    }
    let _ = copy_dir_recursive(&src, &dst);
}

fn handle_agent_request(stream: &mut std::net::TcpStream) {
    use std::io::{BufRead, BufReader, Read, Write};

    let mut reader = BufReader::new(stream.try_clone().unwrap_or_else(|_| return_dummy_stream()));
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }
    let parts: Vec<&str> = request_line.trim().split_whitespace().collect();
    if parts.len() < 2 {
        return;
    }
    let method = parts[0];
    let path = parts[1];

    // Read headers to find Content-Length
    let mut content_length: usize = 0;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).is_err() || header.trim().is_empty() {
            break;
        }
        let lower = header.to_ascii_lowercase();
        if lower.starts_with("content-length:") {
            content_length = lower
                .trim_start_matches("content-length:")
                .trim()
                .parse()
                .unwrap_or(0);
        }
    }

    // Read body
    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        let _ = reader.read_exact(&mut body);
    }
    let body_str = String::from_utf8_lossy(&body);

    // WP-0224: handle CORS preflight (OPTIONS) for every route — Workers in
    // the WebView post `Content-Type: application/json` which triggers a
    // browser preflight. Returning 404 here was silently blocking the
    // follow-up POST and producing the v0.1.18-v0.1.22 "Worker alive but
    // never reaches the bridge" pattern.
    let (status, response_body) = if method == "OPTIONS" {
        ("204 No Content", String::new())
    } else {
        match (method, path) {
            ("GET", "/agent/health") => ("200 OK", r#"{"status":"ok"}"#.to_string()),
            ("GET", "/agent/state") => ("200 OK", agent_handle_state()),
            // WP-0261: read-only per-subscription activity so an external monitor can see
            // subscription refresh + fan-out progress without stealing focus.
            ("GET", "/agent/subscriptions_activity") => agent_handle_subscriptions_activity(),
            // WP-0270: canonical scheduler-track state. This deliberately shares the engine
            // producer used by the Jobs controls and diagnostics instead of grouping rendered
            // rows or duplicating gate calculations on the bridge thread.
            ("GET", "/agent/jobs_tracks") => agent_handle_jobs_tracks(),
            ("POST", "/agent/navigate") => agent_handle_navigate(&body_str),
            ("POST", "/agent/snapshot") => agent_handle_snapshot(&body_str),
            ("POST", "/agent/dump") => agent_handle_dump(&body_str),
            ("POST", "/agent/ui_audit") => agent_handle_ui_request("audit", &body_str),
            ("POST", "/agent/ui_action") => agent_handle_ui_request("action", &body_str),
            ("POST", "/agent/subscription_status") => agent_handle_subscription_status(&body_str),
            ("POST", "/agent/freeze_event") => agent_handle_freeze_event(&body_str),
            ("POST", "/agent/freeze_dump") => agent_handle_freeze_dump(&body_str),
            _ => ("404 Not Found", r#"{"error":"not found"}"#.to_string()),
        }
    };

    // WP-0224: add CORS headers to every response so the freeze-detector
    // Worker (origin `http://tauri.localhost`) can POST to the bridge on
    // `127.0.0.1:<port>`. Localhost-only listener means `*` is safe.
    let response = format!(
        "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\nAccess-Control-Max-Age: 86400\r\nConnection: close\r\n\r\n{}",
        status,
        response_body.len(),
        response_body
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

fn return_dummy_stream() -> std::net::TcpStream {
    // This should never actually be called — it exists to satisfy the type system
    std::net::TcpStream::connect("127.0.0.1:1").unwrap()
}

fn agent_handle_state() -> String {
    let state = agent_bridge_state().lock().unwrap();
    serde_json::json!({
        "current_page": state.current_page,
        "editor_item_id": state.editor_item_id,
        "safe_mode": state.safe_mode,
        "agent_headless": state.agent_headless,
        "app_version": env!("CARGO_PKG_VERSION"),
    })
    .to_string()
}

// WP-0261: bridge handler for GET /agent/subscriptions_activity. Returns the same read-only
// per-subscription activity rows the UI polls (subscriptions::youtube_subscriptions_activity), so
// an external monitor can watch subscription refresh + child-download fan-out. Runs on the bridge
// thread and reads the DB read-only, so it stays off the writer path and works even under load.
fn agent_handle_subscriptions_activity() -> (&'static str, String) {
    let app = match AGENT_APP_HANDLE.get() {
        Some(a) => a,
        None => {
            return (
                "503 Service Unavailable",
                r#"{"error":"app handle unavailable"}"#.to_string(),
            )
        }
    };
    let state = match app.try_state::<AppState>() {
        Some(s) => s,
        None => {
            return (
                "503 Service Unavailable",
                r#"{"error":"app state unavailable"}"#.to_string(),
            )
        }
    };
    match subscriptions::youtube_subscriptions_activity(&state.paths) {
        Ok(rows) => match serde_json::to_string(&rows) {
            Ok(body) => ("200 OK", body),
            Err(e) => (
                "500 Internal Server Error",
                format!(r#"{{"error":"serialize: {}"}}"#, e),
            ),
        },
        Err(e) => (
            "500 Internal Server Error",
            format!(r#"{{"error":"{}"}}"#, e.to_string().replace('"', "'")),
        ),
    }
}

// WP-0270: bridge handler for GET /agent/jobs_tracks. The producer opens the job DB read-only
// and executes one bounded grouped aggregate plus a tiny in-process runner gate-state read, so
// the endpoint remains useful to agents while the WebView is unavailable and never changes queue
// truth, settings, or scheduler state.
fn agent_handle_jobs_tracks() -> (&'static str, String) {
    let app = match AGENT_APP_HANDLE.get() {
        Some(app) => app,
        None => {
            return (
                "503 Service Unavailable",
                r#"{"error":"app handle unavailable"}"#.to_string(),
            )
        }
    };
    let state = match app.try_state::<AppState>() {
        Some(state) => state,
        None => {
            return (
                "503 Service Unavailable",
                r#"{"error":"app state unavailable"}"#.to_string(),
            )
        }
    };
    jobs_tracks_bridge_response(&state.paths)
}

fn jobs_tracks_bridge_response(paths: &AppPaths) -> (&'static str, String) {
    match jobs::get_job_tracks_runtime_snapshot(paths) {
        Ok(snapshot) => match serde_json::to_string(&snapshot) {
            Ok(body) => ("200 OK", body),
            Err(error) => (
                "500 Internal Server Error",
                format!(r#"{{"error":"serialize: {}"}}"#, error),
            ),
        },
        Err(error) => (
            "503 Service Unavailable",
            format!(r#"{{"error":"{}"}}"#, error.to_string().replace('"', "'")),
        ),
    }
}

fn agent_handle_navigate(body: &str) -> (&'static str, String) {
    let parsed: serde_json::Value = match serde_json::from_str::<serde_json::Value>(body) {
        Ok(v) => v,
        Err(_) => return ("400 Bad Request", r#"{"error":"invalid json"}"#.to_string()),
    };
    let page = match parsed.get("page").and_then(|v| v.as_str()) {
        Some(p) => p.to_string(),
        None => {
            return (
                "400 Bad Request",
                r#"{"error":"missing page field"}"#.to_string(),
            )
        }
    };
    let valid = [
        "localization",
        "video_ingest",
        "instagram_archive",
        "image_archive",
        "media_library",
        "jobs",
        "diagnostics",
        "options",
    ];
    if !valid.contains(&page.as_str()) {
        return (
            "400 Bad Request",
            format!(
                r#"{{"error":"invalid page","valid":{}}}"#,
                serde_json::json!(valid)
            ),
        );
    }
    let item_id = parsed
        .get("item_id")
        .or_else(|| parsed.get("itemId"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string);
    let section_id = parsed
        .get("section_id")
        .or_else(|| parsed.get("sectionId"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string);
    if let Some(app) = AGENT_APP_HANDLE.get() {
        if item_id.is_some() || section_id.is_some() {
            let _ = app.emit(
                "agent-navigate",
                serde_json::json!({
                    "page": page,
                    "item_id": item_id,
                    "section_id": section_id,
                }),
            );
        } else {
            let _ = app.emit("agent-navigate", &page);
        }
    }
    (
        "200 OK",
        serde_json::json!({
            "navigated": page,
            "item_id": item_id,
            "section_id": section_id,
        })
        .to_string(),
    )
}

fn build_snapshot_artifact_path(
    subfolder: Option<&str>,
    label: Option<&str>,
    extension: &str,
    fallback_label: &str,
) -> Result<std::path::PathBuf, String> {
    let mut snapshots_dir = std::env::current_dir().unwrap_or_default();
    while !snapshots_dir.join("governance").exists() && snapshots_dir.parent().is_some() {
        snapshots_dir = snapshots_dir.parent().unwrap().to_path_buf();
    }
    let mut target_dir = snapshots_dir.join("governance").join("snapshots");
    if let Some(sub) = subfolder {
        let sanitized = sub.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
        if !sanitized.is_empty() {
            target_dir = target_dir.join(sanitized);
        }
    }
    if !target_dir.exists() {
        std::fs::create_dir_all(&target_dir)
            .map_err(|e| format!("Failed to create snapshot dir: {}", e))?;
    }

    let label_part = label
        .map(|l| l.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|', ' '], "_"))
        .filter(|l| !l.is_empty());
    let file_name = match label_part {
        Some(l) => format!("{}_{}.{}", l, now_epoch_ms_i64(), extension),
        None => format!("{}_{}.{}", fallback_label, now_epoch_ms_i64(), extension),
    };
    Ok(target_dir.join(file_name))
}

#[cfg(target_os = "windows")]
fn try_native_agent_snapshot(subfolder: &str, label: &str) -> Result<String, String> {
    let app = AGENT_APP_HANDLE
        .get()
        .ok_or_else(|| "agent app handle is unavailable".to_string())?;
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main webview window is unavailable".to_string())?;
    let handle = window.window_handle().map_err(|e| e.to_string())?;
    let hwnd = match handle.as_raw() {
        RawWindowHandle::Win32(handle) => handle.hwnd.get() as HWND,
        _ => return Err("native snapshot is only implemented for Win32 windows".to_string()),
    };
    capture_hwnd_snapshot_png(hwnd, subfolder, label)
}

#[cfg(not(target_os = "windows"))]
fn try_native_agent_snapshot(_subfolder: &str, _label: &str) -> Result<String, String> {
    Err("native snapshot is not implemented on this platform".to_string())
}

fn native_snapshot_has_visual_content(rgba: &[u8]) -> bool {
    let pixel_count = rgba.len() / 4;
    if pixel_count == 0 {
        return false;
    }

    let visible_pixels = rgba
        .chunks_exact(4)
        .filter(|px| px[0] > 12 || px[1] > 12 || px[2] > 12)
        .count();
    let minimum_visible_pixels = (pixel_count / 100).max(32);
    visible_pixels >= minimum_visible_pixels
}

fn emit_agent_snapshot_request(subfolder: &str, label: &str, scroll_top: Option<f64>) {
    if let Some(app) = AGENT_APP_HANDLE.get() {
        let _ = app.emit(
            "agent-snapshot-request",
            serde_json::json!({
                "subfolder": subfolder,
                "label": label,
                "scroll_top": scroll_top,
            }),
        );
    }
}

#[cfg(target_os = "windows")]
fn capture_hwnd_snapshot_png(hwnd: HWND, subfolder: &str, label: &str) -> Result<String, String> {
    if hwnd.is_null() {
        return Err("window handle is null".to_string());
    }
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    let got_rect = unsafe { GetWindowRect(hwnd, &mut rect as *mut RECT) };
    if got_rect == 0 {
        return Err("GetWindowRect failed".to_string());
    }
    let width = rect.right.saturating_sub(rect.left);
    let height = rect.bottom.saturating_sub(rect.top);
    if width <= 0 || height <= 0 {
        return Err(format!("invalid window capture size: {width}x{height}"));
    }

    let window_dc = unsafe { GetWindowDC(hwnd) };
    if window_dc.is_null() {
        return Err("GetWindowDC failed".to_string());
    }
    let memory_dc = unsafe { CreateCompatibleDC(window_dc) };
    if memory_dc.is_null() {
        unsafe {
            ReleaseDC(hwnd, window_dc);
        }
        return Err("CreateCompatibleDC failed".to_string());
    }
    let bitmap = unsafe { CreateCompatibleBitmap(window_dc, width, height) };
    if bitmap.is_null() {
        unsafe {
            DeleteDC(memory_dc);
            ReleaseDC(hwnd, window_dc);
        }
        return Err("CreateCompatibleBitmap failed".to_string());
    }

    let previous = unsafe { SelectObject(memory_dc, bitmap as HGDIOBJ) };
    const PW_RENDERFULLCONTENT: u32 = 0x0000_0002;
    let printed = unsafe { PrintWindow(hwnd, memory_dc, PW_RENDERFULLCONTENT) };
    if printed == 0 {
        let copied = unsafe { BitBlt(memory_dc, 0, 0, width, height, window_dc, 0, 0, SRCCOPY) };
        if copied == 0 {
            unsafe {
                SelectObject(memory_dc, previous);
                DeleteObject(bitmap as HGDIOBJ);
                DeleteDC(memory_dc);
                ReleaseDC(hwnd, window_dc);
            }
            return Err("PrintWindow and BitBlt both failed".to_string());
        }
    }

    let mut bitmap_info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            biSizeImage: (width as u32)
                .saturating_mul(height as u32)
                .saturating_mul(4),
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        },
        bmiColors: [RGBQUAD {
            rgbBlue: 0,
            rgbGreen: 0,
            rgbRed: 0,
            rgbReserved: 0,
        }],
    };
    let mut bgra = vec![0u8; (width as usize) * (height as usize) * 4];
    let scanlines = unsafe {
        GetDIBits(
            memory_dc,
            bitmap,
            0,
            height as u32,
            bgra.as_mut_ptr() as *mut core::ffi::c_void,
            &mut bitmap_info as *mut BITMAPINFO,
            DIB_RGB_COLORS,
        )
    };

    unsafe {
        SelectObject(memory_dc, previous);
        DeleteObject(bitmap as HGDIOBJ);
        DeleteDC(memory_dc);
        ReleaseDC(hwnd, window_dc);
    }

    if scanlines == 0 {
        return Err("GetDIBits failed".to_string());
    }

    let mut rgba = Vec::with_capacity(bgra.len());
    for px in bgra.chunks_exact(4) {
        rgba.push(px[2]);
        rgba.push(px[1]);
        rgba.push(px[0]);
        rgba.push(px[3]);
    }
    if !native_snapshot_has_visual_content(&rgba) {
        return Err("native snapshot was blank; falling back to frontend renderer".to_string());
    }

    let path = build_snapshot_artifact_path(Some(subfolder), Some(label), "png", "snapshot")?;
    let file = std::fs::File::create(&path)
        .map_err(|e| format!("Failed to create native snapshot: {e}"))?;
    let writer = std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(writer, width as u32, height as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut png_writer = encoder
        .write_header()
        .map_err(|e| format!("Failed to start PNG snapshot: {e}"))?;
    png_writer
        .write_image_data(&rgba)
        .map_err(|e| format!("Failed to write PNG snapshot: {e}"))?;

    let abs_path = std::fs::canonicalize(&path)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string();
    Ok(abs_path)
}

fn agent_handle_snapshot(body: &str) -> (&'static str, String) {
    let parsed: serde_json::Value = serde_json::from_str(body).unwrap_or(serde_json::json!({}));
    let subfolder = parsed
        .get("subfolder")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let label = parsed
        .get("label")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let scroll_top = parsed
        .get("scroll_top")
        .or_else(|| parsed.get("scrollTop"))
        .and_then(|v| v.as_f64());

    if scroll_top.is_none() {
        if let Ok(path) = try_native_agent_snapshot(&subfolder, &label) {
            return (
                "200 OK",
                format!(r#"{{"path":"{}"}}"#, path.replace('\\', "\\\\")),
            );
        }
    }

    let (tx, rx) = std::sync::mpsc::channel::<String>();

    {
        let mut state = agent_bridge_state().lock().unwrap();
        state.snapshot_tx = Some(tx);
    }

    // Wait for frontend to complete the snapshot (up to 30 seconds for heavy pages under load).
    // Re-emit periodically because the bridge can be available before the WebView has registered
    // its listener during startup; a single early emit is otherwise lost.
    let started_at = Instant::now();
    loop {
        emit_agent_snapshot_request(&subfolder, &label, scroll_top);
        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(path) => {
                return (
                    "200 OK",
                    format!(r#"{{"path":"{}"}}"#, path.replace('\\', "\\\\")),
                )
            }
            Err(_) if started_at.elapsed() < Duration::from_secs(30) => continue,
            Err(_) => {
                // Clear stale sender so late-arriving captures don't contaminate the next request
                let mut state = agent_bridge_state().lock().unwrap();
                state.snapshot_tx = None;
                return (
                    "504 Gateway Timeout",
                    r#"{"error":"snapshot timed out (30s)"}"#.to_string(),
                );
            }
        }
    }
}

fn agent_handle_dump(body: &str) -> (&'static str, String) {
    let parsed: serde_json::Value = serde_json::from_str(body).unwrap_or(serde_json::json!({}));
    let subfolder = parsed
        .get("subfolder")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let label = parsed
        .get("label")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let (tx, rx) = std::sync::mpsc::channel::<String>();
    {
        let mut state = agent_bridge_state().lock().unwrap();
        state.dump_tx = Some(tx);
    }

    if let Some(app) = AGENT_APP_HANDLE.get() {
        let _ = app.emit(
            "agent-dump-request",
            serde_json::json!({
                "subfolder": subfolder,
                "label": label,
            }),
        );
    }

    match rx.recv_timeout(Duration::from_secs(10)) {
        Ok(path) => (
            "200 OK",
            format!(r#"{{"path":"{}"}}"#, path.replace('\\', "\\\\")),
        ),
        Err(_) => {
            let mut state = agent_bridge_state().lock().unwrap();
            state.dump_tx = None;
            (
                "504 Gateway Timeout",
                r#"{"error":"dump timed out (10s)"}"#.to_string(),
            )
        }
    }
}

fn validate_agent_ui_request(
    agent_headless: bool,
    body: &str,
) -> Result<serde_json::Value, (&'static str, String)> {
    if !agent_headless {
        return Err((
            "403 Forbidden",
            r#"{"error":"UI audit routes require --agent-headless"}"#.to_string(),
        ));
    }
    if body.len() > 16 * 1024 {
        return Err((
            "413 Payload Too Large",
            r#"{"error":"UI audit request exceeds 16 KiB"}"#.to_string(),
        ));
    }
    let parsed: serde_json::Value = serde_json::from_str(body)
        .map_err(|_| ("400 Bad Request", r#"{"error":"invalid json"}"#.to_string()))?;
    if !parsed.is_object() {
        return Err((
            "400 Bad Request",
            r#"{"error":"UI audit request must be a JSON object"}"#.to_string(),
        ));
    }
    Ok(parsed)
}

fn agent_handle_ui_request(operation: &'static str, body: &str) -> (&'static str, String) {
    let agent_headless = agent_bridge_state().lock().unwrap().agent_headless;
    let request = match validate_agent_ui_request(agent_headless, body) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let request_id = format!(
        "ui-{}-{}",
        now_epoch_ms_i64(),
        AGENT_UI_REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    {
        let mut state = agent_bridge_state().lock().unwrap();
        if state.ui_request_tx.is_some() {
            return (
                "409 Conflict",
                r#"{"error":"another UI audit request is already active"}"#.to_string(),
            );
        }
        state.ui_request_tx = Some(tx);
    }

    let app = match AGENT_APP_HANDLE.get() {
        Some(app) => app,
        None => {
            agent_bridge_state().lock().unwrap().ui_request_tx = None;
            return (
                "503 Service Unavailable",
                r#"{"error":"agent app handle unavailable"}"#.to_string(),
            );
        }
    };
    if app
        .emit(
            "agent-ui-request",
            serde_json::json!({
                "request_id": request_id,
                "operation": operation,
                "request": request,
            }),
        )
        .is_err()
    {
        agent_bridge_state().lock().unwrap().ui_request_tx = None;
        return (
            "503 Service Unavailable",
            r#"{"error":"failed to emit UI audit request"}"#.to_string(),
        );
    }

    match rx.recv_timeout(Duration::from_secs(15)) {
        Ok(payload) => {
            if serde_json::from_str::<serde_json::Value>(&payload).is_err() {
                return (
                    "502 Bad Gateway",
                    r#"{"error":"frontend returned invalid UI audit JSON"}"#.to_string(),
                );
            }
            ("200 OK", payload)
        }
        Err(_) => {
            agent_bridge_state().lock().unwrap().ui_request_tx = None;
            (
                "504 Gateway Timeout",
                r#"{"error":"UI audit request timed out (15s)"}"#.to_string(),
            )
        }
    }
}

fn agent_handle_subscription_status(body: &str) -> (&'static str, String) {
    if !agent_bridge_state().lock().unwrap().agent_headless {
        return (
            "403 Forbidden",
            r#"{"error":"subscription status changes require --agent-headless"}"#.to_string(),
        );
    }
    if body.len() > 16 * 1024 {
        return (
            "413 Payload Too Large",
            r#"{"error":"request exceeds 16 KiB"}"#.to_string(),
        );
    }
    let parsed: serde_json::Value = match serde_json::from_str::<serde_json::Value>(body) {
        Ok(value) if value.is_object() => value,
        _ => return ("400 Bad Request", r#"{"error":"invalid json"}"#.to_string()),
    };
    let id = parsed
        .get("id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_default();
    let status = parsed
        .get("status")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_default();
    if id.is_empty() || status.is_empty() {
        return (
            "400 Bad Request",
            r#"{"error":"id and status are required"}"#.to_string(),
        );
    }

    let app = match AGENT_APP_HANDLE.get() {
        Some(app) => app,
        None => {
            return (
                "503 Service Unavailable",
                r#"{"error":"app handle unavailable"}"#.to_string(),
            )
        }
    };
    let state = match app.try_state::<AppState>() {
        Some(state) => state,
        None => {
            return (
                "503 Service Unavailable",
                r#"{"error":"app state unavailable"}"#.to_string(),
            )
        }
    };
    match subscriptions::set_youtube_subscription_manual_status(
        &state.paths,
        id,
        status,
        subscriptions::YOUTUBE_SUBSCRIPTION_STATUS_ACTOR_ASSISTANT,
    ) {
        Ok(receipt) => {
            append_diagnostics_trace_row_best_effort(
                &state.paths,
                "subscription_manual_status_changed",
                serde_json::json!({
                    "actor": subscriptions::YOUTUBE_SUBSCRIPTION_STATUS_ACTOR_ASSISTANT,
                    "subscription_id": receipt.subscription.id,
                    "source_status": receipt.subscription.source_status,
                    "canceled_refresh_jobs": receipt.canceled_refresh_jobs,
                }),
                "info",
            );
            match serde_json::to_string(&receipt) {
                Ok(body) => ("200 OK", body),
                Err(error) => (
                    "500 Internal Server Error",
                    serde_json::json!({ "error": error.to_string() }).to_string(),
                ),
            }
        }
        Err(error) => (
            "400 Bad Request",
            serde_json::json!({ "error": error.to_string() }).to_string(),
        ),
    }
}

// WP-0221: Freeze-event ingress for the Worker-driven freeze detector.
// The freeze-detector Worker runs on its own browser thread and POSTs here
// when the WebView main thread stops answering pings, so the report path
// must not depend on Tauri IPC (which routes through the very thread we are
// trying to observe).
fn agent_handle_freeze_event(body: &str) -> (&'static str, String) {
    let parsed: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return ("400 Bad Request", r#"{"error":"invalid json"}"#.to_string()),
    };

    let event = parsed
        .get("event")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("freeze_detected")
        .to_string();
    if event != "freeze_detected" && event != "freeze_recovered" && event != "worker_alive" {
        return (
            "400 Bad Request",
            r#"{"error":"event must be freeze_detected, freeze_recovered, or worker_alive"}"#
                .to_string(),
        );
    }

    let details = parsed
        .get("details")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let level = parsed
        .get("level")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("warn")
        .to_string();

    if let Some(app) = AGENT_APP_HANDLE.get() {
        if let Some(state) = app.try_state::<AppState>() {
            append_diagnostics_trace_row_best_effort(&state.paths, &event, details, &level);
        }
    }

    ("200 OK", r#"{"status":"ok"}"#.to_string())
}

// WP-0221: Freeze report bundling. POSTed by `vvfreeze.cmd` (or the Diagnostics
// page button) to capture a single self-contained JSON report that an agent
// can read without operator relay. Runs on the bridge thread so it works
// even while the WebView main thread is frozen.
//
// Output layout (under the active diagnostics trace dir):
//   freeze_reports/freeze_report_<ts>.json   (timestamped, kept)
//   freeze_reports/freeze_report_latest.json (overwritten each call, the
//                                             stable path agents should read)
//
// Request body (all optional):
//   { "limit": <usize, default 1000, clamped 1..=5000>,
//     "note":  "<free-form operator note saved into the report>" }
fn agent_handle_freeze_dump(body: &str) -> (&'static str, String) {
    let parsed: serde_json::Value = serde_json::from_str(body).unwrap_or(serde_json::json!({}));
    let limit = parsed
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or(1000)
        .clamp(1, 5000);
    let note = parsed
        .get("note")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    let app = match AGENT_APP_HANDLE.get() {
        Some(a) => a,
        None => {
            return (
                "503 Service Unavailable",
                r#"{"error":"app handle unavailable"}"#.to_string(),
            )
        }
    };
    let state = match app.try_state::<AppState>() {
        Some(s) => s,
        None => {
            return (
                "503 Service Unavailable",
                r#"{"error":"app state unavailable"}"#.to_string(),
            )
        }
    };
    let paths = state.paths.clone();

    let trace_dir = match paths.effective_diagnostics_trace_dir() {
        Ok(d) => d,
        Err(e) => {
            return (
                "500 Internal Server Error",
                format!(r#"{{"error":"trace dir: {}"}}"#, e),
            )
        }
    };
    let out_dir = trace_dir.join("freeze_reports");
    if let Err(e) = std::fs::create_dir_all(&out_dir) {
        return (
            "500 Internal Server Error",
            format!(r#"{{"error":"create dir: {}"}}"#, e),
        );
    }

    let recent_trace = read_recent_diagnostics_trace_entries(&paths, limit).unwrap_or_default();

    let (current_page, editor_item_id, safe_mode) = {
        let s = agent_bridge_state().lock().unwrap();
        (
            s.current_page.clone(),
            s.editor_item_id.clone(),
            s.safe_mode,
        )
    };

    let report = serde_json::json!({
        "wp": "WP-0221",
        "generated_at_ms": now_epoch_ms_i64(),
        "app_version": env!("CARGO_PKG_VERSION"),
        "pid": std::process::id(),
        "bridge_port": AGENT_BRIDGE_PORT.get().copied(),
        "agent_state": {
            "current_page": current_page,
            "editor_item_id": editor_item_id,
            "safe_mode": safe_mode,
        },
        "note": note,
        "trace_limit_requested": limit,
        "recent_trace_count": recent_trace.len(),
        "recent_trace": recent_trace,
    });
    let body = match serde_json::to_string_pretty(&report) {
        Ok(s) => s,
        Err(e) => {
            return (
                "500 Internal Server Error",
                format!(r#"{{"error":"serialize: {}"}}"#, e),
            )
        }
    };

    let ts = now_epoch_ms_i64();
    let timestamped = out_dir.join(format!("freeze_report_{}.json", ts));
    let latest = out_dir.join("freeze_report_latest.json");
    if let Err(e) = std::fs::write(&timestamped, body.as_bytes()) {
        return (
            "500 Internal Server Error",
            format!(r#"{{"error":"write timestamped: {}"}}"#, e),
        );
    }
    let _ = std::fs::write(&latest, body.as_bytes());

    append_diagnostics_trace_row_best_effort(
        &paths,
        "freeze_report_written",
        serde_json::json!({
            "timestamped_path": timestamped.to_string_lossy().to_string(),
            "latest_path": latest.to_string_lossy().to_string(),
            "trace_rows_included": recent_trace.len(),
        }),
        "info",
    );

    (
        "200 OK",
        serde_json::json!({
            "path": timestamped.to_string_lossy().to_string(),
            "latest_path": latest.to_string_lossy().to_string(),
            "trace_rows_included": recent_trace.len(),
        })
        .to_string(),
    )
}

use voxvulgi_engine::models::ModelStore;
use voxvulgi_engine::paths::AppPaths;
use voxvulgi_engine::{
    config, db, diagnostics, instagram_subscriptions, jobs, library, media_cleanup, root_rebind,
    speakers, subscriptions, subtitle_tracks, subtitles, tools, translate, video_libraries,
    voice_backend_adapters, voice_backends, voice_benchmarks, voice_cast_packs, voice_cleanup,
    voice_library, voice_plans, voice_reference_candidates, voice_reference_curation,
    voice_templates,
};

#[derive(Debug, Clone, serde::Deserialize)]
struct OfflineBundleManifest {
    schema_version: u32,
    bundle_id: String,
    #[serde(default)]
    payload_zip: Option<String>,
    #[serde(default)]
    payload_bytes: Option<u64>,
    #[serde(default)]
    payload_sha256: Option<String>,
    #[serde(default)]
    payload_sha256_algorithm: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct Phase2InstallLatestState {
    exists: bool,
    path: String,
    state: Option<serde_json::Value>,
    active: bool,
    stale: bool,
    job_status: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum ArtifactKind {
    SeparationStem,
    CleanupAudio,
    CleanupManifest,
    TtsManifest,
    TtsRequest,
    TtsReport,
    DubMix,
    DubSpeechStem,
    DubMux,
    ExportPack,
    QcReport,
    BenchmarkReport,
    ReferenceCurationReport,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum ArtifactRerunKind {
    SeparateSpleeter,
    SeparateDemucs,
    CleanVocals,
    TtsPyttsx3,
    TtsNeuralLocalV1,
    DubVoicePreservingV1,
    ExperimentalVoiceBackendRenderV1,
    MixDubPreviewV1,
    MuxDubPreviewV1,
    ExportPackV1,
}

#[derive(Debug, Clone, serde::Serialize)]
struct ArtifactInfo {
    id: String,
    title: String,
    path: String,
    exists: bool,
    group: String,
    kind: ArtifactKind,
    job_type: Option<String>,
    variant_label: Option<String>,
    track_id: Option<String>,
    mux_container: Option<String>,
    tts_backend_id: Option<String>,
    voice_clone_outcome: Option<jobs::VoiceCloneRunOutcome>,
    voice_clone_requested_segments: Option<usize>,
    voice_clone_converted_segments: Option<usize>,
    voice_clone_fallback_segments: Option<usize>,
    voice_clone_standard_tts_segments: Option<usize>,
    rerun_kind: Option<ArtifactRerunKind>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct ArtifactVoiceCloneMeta {
    #[serde(default)]
    voice_clone_outcome: Option<jobs::VoiceCloneRunOutcome>,
    #[serde(default)]
    voice_clone_requested_segments: Option<usize>,
    #[serde(default)]
    voice_clone_converted_segments: Option<usize>,
    #[serde(default)]
    voice_clone_fallback_segments: Option<usize>,
    #[serde(default)]
    voice_clone_standard_tts_segments: Option<usize>,
}

#[derive(Debug, Clone)]
struct AppState {
    paths: AppPaths,
    runner: Option<jobs::JobRunnerHandle>,
    safe_mode_enabled: Arc<AtomicBool>,
    safe_mode_cli: bool,
    startup: Arc<Mutex<StartupTracker>>,
}

impl Drop for AppState {
    fn drop(&mut self) {
        if let Some(runner) = &self.runner {
            runner.stop();
        }
    }
}

fn runtime_background_work_enabled(safe_mode_enabled: bool, agent_headless: bool) -> bool {
    !safe_mode_enabled && !agent_headless
}

#[derive(Debug, Clone, serde::Serialize)]
struct DiagnosticsInfo {
    app_data_dir: String,
    db_path: String,
    app_name: String,
    app_version: String,
    engine_version: String,
}

#[derive(Debug, Clone)]
struct StartupTracker {
    offline_bundle_state: String,
    offline_bundle_started_at_ms: Option<i64>,
    offline_bundle_finished_at_ms: Option<i64>,
    offline_bundle_error: Option<String>,
    progress_pct: f32,
    active_phase_id: Option<String>,
    phases: Vec<StartupPhase>,
}

impl StartupTracker {
    fn new() -> Self {
        Self {
            offline_bundle_state: "not_started".to_string(),
            offline_bundle_started_at_ms: None,
            offline_bundle_finished_at_ms: None,
            offline_bundle_error: None,
            progress_pct: 0.0,
            active_phase_id: None,
            phases: vec![
                StartupPhase::new("app_dirs", "App data + output layout"),
                StartupPhase::new("db_schema", "Database schema"),
                StartupPhase::new("job_runner", "Job runner"),
                StartupPhase::new("offline_bundle", "Offline bundle hydration"),
            ],
        }
    }

    fn set_phase_state(&mut self, phase_id: &str, state: &str, error: Option<String>) {
        let now = now_epoch_ms_i64();
        if let Some(phase) = self.phases.iter_mut().find(|phase| phase.id == phase_id) {
            phase.state = state.to_string();
            if matches!(state, "pending" | "running") {
                phase.started_at_ms = phase.started_at_ms.or(Some(now));
                phase.finished_at_ms = None;
                phase.error = None;
            } else {
                phase.started_at_ms = phase.started_at_ms.or(Some(now));
                phase.finished_at_ms = Some(now);
                phase.error = error.clone();
            }
        }

        if phase_id == "offline_bundle" {
            self.offline_bundle_state = if state == "skipped" {
                "skipped_safe_mode".to_string()
            } else {
                state.to_string()
            };
            match state {
                "pending" | "running" => {
                    self.offline_bundle_started_at_ms =
                        self.offline_bundle_started_at_ms.or(Some(now));
                    self.offline_bundle_finished_at_ms = None;
                    self.offline_bundle_error = None;
                }
                "ready" | "skipped" => {
                    self.offline_bundle_started_at_ms =
                        self.offline_bundle_started_at_ms.or(Some(now));
                    self.offline_bundle_finished_at_ms = Some(now);
                    self.offline_bundle_error = None;
                }
                "error" => {
                    self.offline_bundle_started_at_ms =
                        self.offline_bundle_started_at_ms.or(Some(now));
                    self.offline_bundle_finished_at_ms = Some(now);
                    self.offline_bundle_error = error;
                }
                _ => {}
            }
        }

        let total = self.phases.len().max(1) as f32;
        let completed = self
            .phases
            .iter()
            .filter(|phase| matches!(phase.state.as_str(), "ready" | "skipped" | "error"))
            .count() as f32;
        self.progress_pct = (completed / total).clamp(0.0, 1.0);
        self.active_phase_id = self
            .phases
            .iter()
            .find(|phase| matches!(phase.state.as_str(), "running" | "pending"))
            .map(|phase| phase.id.clone());
    }
}

#[derive(Debug, Clone, serde::Serialize)]
struct StartupPhase {
    id: String,
    label: String,
    state: String,
    started_at_ms: Option<i64>,
    finished_at_ms: Option<i64>,
    error: Option<String>,
}

impl StartupPhase {
    fn new(id: &str, label: &str) -> Self {
        Self {
            id: id.to_string(),
            label: label.to_string(),
            state: "pending".to_string(),
            started_at_ms: None,
            finished_at_ms: None,
            error: None,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
struct StartupStatus {
    offline_bundle_state: String,
    offline_bundle_started_at_ms: Option<i64>,
    offline_bundle_finished_at_ms: Option<i64>,
    offline_bundle_error: Option<String>,
    progress_pct: f32,
    active_phase_id: Option<String>,
    phases: Vec<StartupPhase>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct DownloadDirStatus {
    current_dir: String,
    default_dir: String,
    exists: bool,
    using_default: bool,
    feature_roots: Vec<FeatureStorageRootStatus>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct FeatureStorageRootStatus {
    key: String,
    label: String,
    current_dir: String,
    default_dir: String,
    override_dir: Option<String>,
    exists: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
struct ShellPathResult {
    path: String,
    method: String,
}

#[derive(Debug, Clone, serde::Serialize)]
struct ShellPathStatus {
    path: String,
    exists: bool,
    is_dir: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
struct SafeModeStatus {
    enabled: bool,
    persisted_enabled: bool,
    cli_enabled: bool,
    queue_paused: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
struct DiagnosticsTraceDirStatus {
    current_dir: String,
    default_dir: String,
    exists: bool,
    using_default: bool,
    retained_age_ms: i64,
    rotation_count: u64,
    compressed_files: u64,
    aggregate_path: String,
    sampling_mode: String,
    queue_capacity: usize,
    dropped_events_total: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
struct DiagnosticsKeyCount {
    key: String,
    count: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
struct DiagnosticsRecentJobFailure {
    id: String,
    job_type: String,
    item_id: Option<String>,
    created_at_ms: i64,
    error: String,
}

#[derive(Debug, Clone, serde::Serialize)]
struct DiagnosticsJobQueueSnapshot {
    total: u64,
    queued: u64,
    running: u64,
    succeeded: u64,
    failed: u64,
    canceled: u64,
    active_batch_count: u64,
    recent_failures: Vec<DiagnosticsRecentJobFailure>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct DiagnosticsLibrarySnapshot {
    total_items: u64,
    by_source_type: Vec<DiagnosticsKeyCount>,
    by_provider: Vec<DiagnosticsKeyCount>,
    subtitle_track_count: u64,
    translated_en_track_count: u64,
    item_speaker_count: u64,
    item_voice_plan_count: u64,
    voice_template_count: u64,
    voice_cast_pack_count: u64,
    voice_library_profile_count: u64,
    youtube_subscription_count: u64,
    instagram_subscription_count: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
struct DiagnosticsFeatureHealthRow {
    feature: String,
    status: String,
    detail: String,
}

#[derive(Debug, Clone, serde::Serialize)]
struct DiagnosticsAppStateSnapshot {
    generated_at_ms: i64,
    app: DiagnosticsInfo,
    startup: StartupStatus,
    download_roots: DownloadDirStatus,
    diagnostics_trace_dir: DiagnosticsTraceDirStatus,
    ffmpeg: tools::FfmpegToolsStatus,
    ytdlp: tools::YtDlpToolsStatus,
    js_runtime: tools::JsRuntimeToolsStatus,
    python: tools::PythonToolchainStatus,
    portable_python: tools::PortablePythonStatus,
    spleeter: tools::SpleeterPackStatus,
    demucs: tools::DemucsPackStatus,
    diarization: tools::DiarizationPackStatus,
    tts_preview: tools::TtsPreviewPackStatus,
    tts_neural_local_v1: tools::TtsNeuralLocalV1PackStatus,
    tts_voice_preserving_local_v1: tools::TtsVoicePreservingLocalV1PackStatus,
    voice_backend_catalog: voice_backends::VoiceBackendCatalog,
    voice_backend_recommendation: voice_backends::VoiceBackendRecommendation,
    voice_backend_adapter_count: usize,
    models: voxvulgi_engine::models::ModelInventory,
    performance_tier: tools::PerformanceTierStatus,
    batch_on_import_rules: config::BatchOnImportRules,
    optional_diarization_backend: config::OptionalDiarizationBackendStatus,
    storage: diagnostics::StorageBreakdown,
    thumbnail_cache: library::ThumbnailCacheStatus,
    jobs: DiagnosticsJobQueueSnapshot,
    // WP-0270: exact same canonical producer exposed through Jobs controls and
    // GET /agent/jobs_tracks; do not derive this from the diagnostics recent-job preview.
    jobs_tracks: jobs::JobTracksRuntimeSnapshot,
    library: DiagnosticsLibrarySnapshot,
    recent_trace: Vec<DiagnosticsTraceEntry>,
    feature_health: Vec<DiagnosticsFeatureHealthRow>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct DiagnosticsAppStateSnapshotExport {
    generated_at_ms: i64,
    json_path: String,
    markdown_path: String,
    json_bytes: u64,
    markdown_bytes: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
struct DiagnosticsTraceClearSummary {
    removed_entries: usize,
    removed_bytes: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct DiagnosticsProcessSnapshot {
    pid: Option<u32>,
    cpu_percent: Option<f32>,
    rss_bytes: Option<u64>,
    virtual_bytes: Option<u64>,
    system_used_bytes: Option<u64>,
    system_total_bytes: Option<u64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct DiagnosticsTraceEntry {
    ts_ms: i64,
    event: String,
    level: String,
    details: serde_json::Value,
    process: Option<DiagnosticsProcessSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    incident_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    span_id: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct DiagnosticsCaptureStatus {
    mode: String,
    armed_trigger: Option<String>,
    incident_id: Option<String>,
    armed_at_ms: Option<i64>,
    started_at_ms: Option<i64>,
    expires_at_ms: Option<i64>,
    max_trace_bytes: u64,
    trace_bytes: u64,
    dropped_events: u64,
    artifact_dir: Option<String>,
    #[serde(default)]
    root_span_id: Option<String>,
}

impl Default for DiagnosticsCaptureStatus {
    fn default() -> Self {
        Self {
            mode: "normal".to_string(),
            armed_trigger: None,
            incident_id: None,
            armed_at_ms: None,
            started_at_ms: None,
            expires_at_ms: None,
            max_trace_bytes: DIAGNOSTICS_TRACE_NORMAL_MAX_BYTES,
            trace_bytes: 0,
            dropped_events: 0,
            artifact_dir: None,
            root_span_id: None,
        }
    }
}

struct DiagnosticsTraceWriteRequest {
    paths: AppPaths,
    event: String,
    details: serde_json::Value,
    level: String,
}

#[derive(Debug, Clone, serde::Serialize)]
struct DiagnosticsTraceEnqueueReceipt {
    accepted: bool,
    dropped_events_total: u64,
    async_write_failures_total: u64,
    pending_loss_events: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
struct DiagnosticsPanelTransitionReceipt {
    incident_id: Option<String>,
    panel_span_id: String,
    parent_span_id: Option<String>,
    capture_mode: String,
    activated_armed_capture: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
struct JobsBatchOperationSnapshot {
    request_id: String,
    mode: String,
    batch_query: String,
    state: String,
    started_at_ms: i64,
    finished_at_ms: Option<i64>,
    summary: Option<jobs::RetryBatchFailedSummary>,
    error: Option<String>,
}

fn jobs_batch_operations(
) -> &'static Mutex<std::collections::HashMap<String, JobsBatchOperationSnapshot>> {
    JOBS_BATCH_OPERATIONS.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

fn prune_jobs_batch_operations(
    operations: &mut std::collections::HashMap<String, JobsBatchOperationSnapshot>,
    now_ms: i64,
) {
    const COMPLETED_RETENTION_MS: i64 = 60 * 60 * 1000;
    const MAX_RECEIPTS: usize = 128;
    operations.retain(|_, operation| {
        operation.state == "running"
            || operation
                .finished_at_ms
                .map(|finished_at_ms| {
                    now_ms.saturating_sub(finished_at_ms) <= COMPLETED_RETENTION_MS
                })
                .unwrap_or(true)
    });
    if operations.len() < MAX_RECEIPTS {
        return;
    }
    let mut completed = operations
        .values()
        .filter(|operation| operation.state != "running")
        .map(|operation| {
            (
                operation.finished_at_ms.unwrap_or(operation.started_at_ms),
                operation.request_id.clone(),
            )
        })
        .collect::<Vec<_>>();
    completed.sort_by_key(|(finished_at_ms, _)| *finished_at_ms);
    for (_, request_id) in completed {
        if operations.len() < MAX_RECEIPTS {
            break;
        }
        operations.remove(&request_id);
    }
}

#[derive(Debug, Clone, serde::Serialize)]
struct ItemOutputs {
    item_id: String,
    source_media_path: String,
    source_media_exists: bool,
    derived_item_dir: String,
    dub_preview_dir: String,
    source_track_count: usize,
    source_usable_segment_count: usize,
    latest_source_track_path: Option<String>,
    translated_en_track_count: usize,
    translated_en_usable_segment_count: usize,
    translated_en_speaker_count: usize,
    latest_translated_en_track_path: Option<String>,
    mix_dub_preview_v1_wav_path: String,
    mix_dub_preview_v1_wav_exists: bool,
    mux_dub_preview_v1_mp4_path: String,
    mux_dub_preview_v1_mp4_exists: bool,
    mux_dub_preview_v1_mkv_path: String,
    mux_dub_preview_v1_mkv_exists: bool,
    export_pack_v1_zip_path: String,
    export_pack_v1_zip_exists: bool,
    terminal_state: String,
    terminal_summary: String,
    terminal_detail: String,
    terminal_stage_label: Option<String>,
    terminal_progress: Option<f32>,
    terminal_error: Option<String>,
    deliverable_path: Option<String>,
    deliverable_exists: bool,
    recent_jobs: Vec<jobs::JobRow>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct ExportedFile {
    out_path: String,
    file_bytes: u64,
}

#[derive(Debug, Default, Clone)]
struct CopySummary {
    copied_files: u64,
    skipped_files: u64,
    copied_bytes: u64,
}

#[derive(Debug, Default, Clone)]
struct ZipExtractSummary {
    extracted_files: u64,
    skipped_files: u64,
    extracted_bytes: u64,
}

fn now_epoch_ms_i64() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn job_status_as_str(status: &jobs::JobStatus) -> &'static str {
    match status {
        jobs::JobStatus::Queued => "queued",
        jobs::JobStatus::Running => "running",
        jobs::JobStatus::Succeeded => "succeeded",
        jobs::JobStatus::Failed => "failed",
        jobs::JobStatus::Canceled => "canceled",
    }
}

fn phase2_step_status_is_active(status: &str) -> bool {
    matches!(status, "queued" | "running")
}

fn phase2_latest_state_has_active_steps(state: &serde_json::Value) -> bool {
    state
        .get("steps")
        .and_then(|steps| steps.as_array())
        .map(|steps| {
            steps.iter().any(|step| {
                step.get("status")
                    .and_then(|value| value.as_str())
                    .map(phase2_step_status_is_active)
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

fn mark_phase2_active_steps_terminal(
    state: &mut serde_json::Value,
    status: &str,
    finished_at_ms: Option<i64>,
    message: &str,
) {
    let finished_at_ms = finished_at_ms.unwrap_or_else(now_epoch_ms_i64);
    if let Some(steps) = state
        .get_mut("steps")
        .and_then(|value| value.as_array_mut())
    {
        for step in steps {
            let current_status = step
                .get("status")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            if !phase2_step_status_is_active(current_status) {
                continue;
            }
            if let Some(obj) = step.as_object_mut() {
                obj.insert("status".to_string(), serde_json::json!(status));
                obj.insert(
                    "finished_at_ms".to_string(),
                    serde_json::json!(finished_at_ms),
                );
                obj.insert("error".to_string(), serde_json::json!(message));
            }
        }
    }
    if let Some(obj) = state.as_object_mut() {
        obj.insert(
            "normalized_at_ms".to_string(),
            serde_json::json!(now_epoch_ms_i64()),
        );
        obj.insert("normalization_note".to_string(), serde_json::json!(message));
    }
}

fn normalize_phase2_latest_state(
    paths: &AppPaths,
    mut state: serde_json::Value,
) -> (serde_json::Value, bool, bool, Option<String>) {
    let active = phase2_latest_state_has_active_steps(&state);
    if !active {
        return (state, false, false, None);
    }

    let Some(job_id) = state
        .get("job_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        mark_phase2_active_steps_terminal(
            &mut state,
            "stale",
            None,
            "Installer state is stale; no matching job id was recorded.",
        );
        return (state, false, true, None);
    };

    match jobs::get_job(paths, job_id) {
        Ok(Some(job)) => {
            let job_status = job_status_as_str(&job.status).to_string();
            match job.status {
                jobs::JobStatus::Queued | jobs::JobStatus::Running => {
                    (state, true, false, Some(job_status))
                }
                jobs::JobStatus::Succeeded => {
                    mark_phase2_active_steps_terminal(
                        &mut state,
                        "done",
                        job.finished_at_ms,
                        "Installer job finished after this state file stopped updating.",
                    );
                    (state, false, false, Some(job_status))
                }
                jobs::JobStatus::Failed => {
                    let detail = job
                        .error
                        .as_deref()
                        .unwrap_or("Installer job failed before this state file could finish.");
                    let status = if detail.to_ascii_lowercase().contains("interrupted") {
                        "interrupted"
                    } else {
                        "failed"
                    };
                    mark_phase2_active_steps_terminal(
                        &mut state,
                        status,
                        job.finished_at_ms,
                        detail,
                    );
                    (state, false, true, Some(job_status))
                }
                jobs::JobStatus::Canceled => {
                    mark_phase2_active_steps_terminal(
                        &mut state,
                        "canceled",
                        job.finished_at_ms,
                        "Installer job was canceled before this state file could finish.",
                    );
                    (state, false, true, Some(job_status))
                }
            }
        }
        Ok(None) => {
            mark_phase2_active_steps_terminal(
                &mut state,
                "stale",
                None,
                "Installer state is stale; the matching job was not found.",
            );
            (state, false, true, None)
        }
        Err(err) => {
            let err_text = err.to_string();
            let err_text_lower = err_text.to_ascii_lowercase();
            if err_text_lower.contains("database is locked")
                || err_text_lower.contains("database table is locked")
                || err_text_lower.contains("sqlite_locked")
                || err_text_lower.contains("database is busy")
                || err_text_lower.contains("database busy")
                || err_text_lower.contains("sqlite_busy")
            {
                if let Some(obj) = state.as_object_mut() {
                    obj.insert(
                        "normalization_note".to_string(),
                        serde_json::json!("Installer job state could not be verified due temporary database lock; retaining active steps."),
                    );
                }
                return (state, active, false, None);
            }
            mark_phase2_active_steps_terminal(
                &mut state,
                "stale",
                None,
                &format!("Installer state could not be checked against the job database: {err}"),
            );
            (state, false, true, None)
        }
    }
}

fn startup_phase_label(phase_id: &str) -> &'static str {
    match phase_id {
        "app_dirs" => "App data + output layout",
        "db_schema" => "Database schema",
        "job_runner" => "Job runner",
        "offline_bundle" => "Offline bundle hydration",
        _ => "Startup task",
    }
}

fn diagnostics_trace_file_path(paths: &AppPaths) -> Result<std::path::PathBuf, String> {
    let dir = paths
        .effective_diagnostics_trace_dir()
        .map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("diagnostics_trace.jsonl"))
}

const DIAGNOSTICS_TRACE_NORMAL_MAX_BYTES: u64 = 32 * 1024 * 1024;
const DIAGNOSTICS_TRACE_INCIDENT_MAX_BYTES: u64 = 64 * 1024 * 1024;
const DIAGNOSTICS_TRACE_RETAINED_FILES: usize = 5;
const DIAGNOSTICS_TRACE_RETAINED_AGE_MS: i64 = 24 * 60 * 60 * 1000;
const DIAGNOSTICS_TRACE_QUEUE_CAPACITY: usize = 2048;
const DIAGNOSTICS_INCIDENT_DURATION_MS: i64 = 10 * 60 * 1000;
const DIAGNOSTICS_TRACE_MAX_ROW_BYTES: usize = 256 * 1024;
const DIAGNOSTICS_TRACE_TAIL_READ_BYTES: u64 = 4 * 1024 * 1024;
const DIAGNOSTICS_INCIDENT_RETAINED_COUNT: usize = 12;
const DIAGNOSTICS_INCIDENT_RETAINED_AGE_MS: i64 = 14 * 24 * 60 * 60 * 1000;

fn redact_diagnostics_value(value: serde_json::Value) -> serde_json::Value {
    fn sensitive_key(key: &str) -> bool {
        let k = key.to_ascii_lowercase();
        [
            "password",
            "passwd",
            "token",
            "secret",
            "cookie",
            "authorization",
            "api_key",
            "apikey",
            "proxy",
            "path",
            "file",
            "dir",
            "root",
            "url",
            "command_line",
        ]
        .iter()
        .any(|part| k.contains(part))
    }
    fn redact_text(input: String) -> String {
        fn redact_url_userinfo(token: &str) -> String {
            let Some(scheme) = token.find("://") else {
                return token.to_string();
            };
            let authority_start = scheme + 3;
            let authority_end = token[authority_start..]
                .find(['/', '?', '#'])
                .map(|v| authority_start + v)
                .unwrap_or(token.len());
            let Some(at_rel) = token[authority_start..authority_end].rfind('@') else {
                return token.to_string();
            };
            format!(
                "{}<redacted>@{}",
                &token[..authority_start],
                &token[authority_start + at_rel + 1..]
            )
        }
        fn tokenize_quoted(input: &str) -> Vec<String> {
            let mut tokens = Vec::new();
            let mut current = String::new();
            let mut quote = None;
            for character in input.chars() {
                if let Some(active_quote) = quote {
                    current.push(character);
                    if character == active_quote {
                        quote = None;
                    }
                } else if matches!(character, '\'' | '"') {
                    quote = Some(character);
                    current.push(character);
                } else if character.is_whitespace() {
                    if !current.is_empty() {
                        tokens.push(std::mem::take(&mut current));
                    }
                } else {
                    current.push(character);
                }
            }
            if !current.is_empty() {
                tokens.push(current);
            }
            tokens
        }
        fn sensitive_name(value: &str) -> bool {
            matches!(
                value
                    .trim()
                    .trim_start_matches('-')
                    .trim_end_matches(':')
                    .trim_matches(['\'', '"'])
                    .to_ascii_lowercase()
                    .as_str(),
                "password"
                    | "passwd"
                    | "token"
                    | "secret"
                    | "key"
                    | "api-key"
                    | "api_key"
                    | "apikey"
                    | "cookies"
                    | "cookie"
                    | "proxy"
                    | "authorization"
            )
        }
        fn authorization_name(value: &str) -> bool {
            matches!(value
                .trim()
                .trim_start_matches('-')
                .trim_end_matches(':')
                .trim_matches(['\'', '"'])
                .to_ascii_lowercase()
                .as_str(), "authorization" | "proxy-authorization")
        }
        fn authorization_scheme(value: &str) -> bool {
            matches!(
                value
                    .trim()
                    .trim_matches(['\'', '"'])
                    .trim_end_matches([':', '='])
                    .to_ascii_lowercase()
                    .as_str(),
                "bearer" | "basic"
            )
        }
        static QUOTED_HEADER_AUTH_RE: OnceLock<regex::Regex> = OnceLock::new();
        static AUTH_TUPLE_RE: OnceLock<regex::Regex> = OnceLock::new();
        let quoted_header_auth_re = QUOTED_HEADER_AUTH_RE.get_or_init(|| regex::Regex::new(
            r#"(?i)(--header(?:=|\s+))(["'])(\s*(?:proxy-)?authorization\s*:)\s*[^"']*(["'])"#,
        ).expect("valid quoted header redaction regex"));
        let input = quoted_header_auth_re
            .replace_all(&input, "$1$2$3 <redacted>$4")
            .to_string();
        let auth_tuple_re = AUTH_TUPLE_RE.get_or_init(|| regex::Regex::new(
            r#"(?i)\b((?:proxy-)?authorization)\b\s*[:=]\s*(?:bearer|basic)\s+(?:"[^"]*"|'[^']*'|[^\s,;:=]+)"#,
        ).expect("valid authorization tuple regex"));
        let input = auth_tuple_re.replace_all(&input, "$1: <redacted>").to_string();
        let mut out = Vec::new();
        let mut redact_next = false;
        // 0 = none, 1 = authorization value or scheme, 2 = credential after a scheme.
        let mut authorization_state = 0_u8;
        for token in tokenize_quoted(&input) {
            if authorization_state != 0 {
                if matches!(token.as_str(), "=" | ":") {
                    out.push(token);
                    continue;
                }
                let value = token.trim_start_matches([':', '=']);
                let prefix_bytes = token.len() - value.len();
                if value.is_empty() {
                    out.push(token);
                    continue;
                }
                let was_scheme = authorization_state == 1 && authorization_scheme(value);
                out.push(format!("{}<redacted>", &token[..prefix_bytes]));
                authorization_state = if was_scheme { 2 } else { 0 };
                continue;
            }
            if redact_next {
                if matches!(token.as_str(), "=" | ":") {
                    out.push(token);
                    continue;
                }
                out.push("<redacted>".to_string());
                redact_next = false;
                continue;
            }
            let lower = token.to_ascii_lowercase();
            if lower == "bearer" {
                out.push(token.to_string());
                redact_next = true;
                continue;
            }
            if authorization_name(&token) {
                out.push(token.to_string());
                authorization_state = 1;
                continue;
            }
            if sensitive_name(&token) {
                out.push(token.to_string());
                redact_next = true;
                continue;
            }
            if let Some((name, value)) = token.split_once('=') {
                if sensitive_name(name) {
                    out.push(format!("{name}=<redacted>"));
                    if authorization_name(name) {
                        authorization_state = if value.is_empty() {
                            1
                        } else if authorization_scheme(value) {
                            2
                        } else {
                            0
                        };
                    } else {
                        redact_next = value.is_empty();
                    }
                    continue;
                }
            }
            if let Some((name, value)) = token.split_once(':') {
                if sensitive_name(name) {
                    out.push(format!("{name}:<redacted>"));
                    if authorization_name(name) {
                        authorization_state = if value.is_empty() {
                            1
                        } else if authorization_scheme(value) {
                            2
                        } else {
                            0
                        };
                    } else {
                        redact_next = value.is_empty();
                    }
                    continue;
                }
            }
            if lower.starts_with("authorization:") {
                out.push("authorization:<redacted>".to_string());
                continue;
            }
            if token.contains(":\\")
                || token.starts_with("\\\\")
                || token.contains("/Users/")
                || token.contains("/home/")
            {
                out.push("<redacted_path>".to_string());
                continue;
            }
            out.push(redact_url_userinfo(&token));
        }
        out.join(" ")
    }
    match value {
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.into_iter()
                .map(|(key, value)| {
                    let value = if sensitive_key(&key) {
                        serde_json::Value::String("<redacted>".into())
                    } else {
                        redact_diagnostics_value(value)
                    };
                    (key, value)
                })
                .collect(),
        ),
        serde_json::Value::Array(values) => {
            serde_json::Value::Array(values.into_iter().map(redact_diagnostics_value).collect())
        }
        serde_json::Value::String(value) => serde_json::Value::String(redact_text(value)),
        other => other,
    }
}

fn diagnostics_capture_state_file(paths: &AppPaths) -> Result<std::path::PathBuf, String> {
    let dir = paths
        .effective_diagnostics_trace_dir()
        .map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("capture_state.json"))
}

fn diagnostics_capture_state() -> &'static Mutex<DiagnosticsCaptureStatus> {
    DIAGNOSTICS_CAPTURE_STATE.get_or_init(|| Mutex::new(DiagnosticsCaptureStatus::default()))
}

fn ensure_diagnostics_trace_mutation_allowed(paths: &AppPaths) -> Result<(), String> {
    let status = load_diagnostics_capture_state(paths);
    if status.mode != "normal" || status.armed_trigger.is_some() {
        return Err(format!(
            "diagnostics trace folder is pinned while capture is {}",
            if status.mode == "incident" {
                "active"
            } else {
                "armed"
            }
        ));
    }
    Ok(())
}

fn diagnostics_unique_id(prefix: &str) -> String {
    format!(
        "{prefix}-{}-{}-{}",
        now_epoch_ms_i64(),
        std::process::id(),
        DIAGNOSTICS_ID_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}

fn write_json_atomic(path: &std::path::Path, value: &impl serde::Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    voxvulgi_engine::persistence::atomic_write_bytes(path, &bytes).map_err(|e| e.to_string())
}

fn persist_diagnostics_capture_state(
    paths: &AppPaths,
    status: &DiagnosticsCaptureStatus,
) -> Result<(), String> {
    write_json_atomic(&diagnostics_capture_state_file(paths)?, status)
}

fn load_diagnostics_capture_state(paths: &AppPaths) -> DiagnosticsCaptureStatus {
    let loaded = diagnostics_capture_state_file(paths)
        .ok()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|raw| serde_json::from_str::<DiagnosticsCaptureStatus>(&raw).ok())
        .unwrap_or_default();
    let mut state = diagnostics_capture_state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *state = loaded.clone();
    loaded
}

fn finalize_diagnostics_incident_manifest(
    status: &DiagnosticsCaptureStatus,
    outcome: &str,
) -> Result<(), String> {
    let Some(artifact_dir) = status.artifact_dir.as_deref() else {
        return Ok(());
    };
    let incident_dir = std::path::PathBuf::from(artifact_dir);
    let manifest = serde_json::json!({
        "wp": "WP-0298",
        "incident_id": status.incident_id,
        "status": outcome,
        "trace_path": incident_dir.join("trace.jsonl").to_string_lossy(),
        "started_at_ms": status.started_at_ms,
        "finished_at_ms": now_epoch_ms_i64(),
        "capture": status,
    });
    write_json_atomic(&incident_dir.join("manifest.json"), &manifest)
}

fn rotate_diagnostics_trace_if_needed(
    path: &std::path::Path,
    max_bytes: u64,
    incoming_bytes: u64,
) -> Result<(), String> {
    #[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
    struct RotationState {
        current_started_at_ms: i64,
        last_rotation_at_ms: Option<i64>,
        rotation_count: u64,
        compressed_files: u64,
        last_rotation_reason: Option<String>,
    }
    let state_path = path.with_file_name("diagnostics_trace.rotation_state.json");
    let state_was_missing = !state_path.exists();
    let mut state = std::fs::read_to_string(&state_path)
        .ok()
        .and_then(|raw| serde_json::from_str::<RotationState>(&raw).ok())
        .unwrap_or_default();
    let now = now_epoch_ms_i64();
    if state.current_started_at_ms <= 0 {
        state.current_started_at_ms = std::fs::metadata(path)
            .ok()
            .and_then(|meta| meta.created().or_else(|_| meta.modified()).ok())
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis() as i64)
            .unwrap_or(now);
    }
    let bytes = std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0);
    let size_due = bytes.saturating_add(incoming_bytes) > max_bytes;
    let age_due = bytes > 0
        && now.saturating_sub(state.current_started_at_ms) >= DIAGNOSTICS_TRACE_RETAINED_AGE_MS;
    if !size_due && !age_due {
        // Persist the generation start once, not on every accepted trace row. Rewriting this
        // small state file for every event used to turn diagnostics into its own I/O hotspot.
        if state_was_missing {
            write_json_atomic(
                &state_path,
                &serde_json::to_value(&state).map_err(|e| e.to_string())?,
            )?;
        }
        return Ok(());
    }
    reconcile_diagnostics_trace_rotation(path)?;
    let generation_id = diagnostics_unique_id("tracegen");
    let journal_path = path.with_file_name("diagnostics_trace.rotation_journal.json");
    write_json_atomic(
        &journal_path,
        &serde_json::json!({
            "schema_version": 1,
            "generation_id": generation_id,
            "stage": "prepared",
            "created_at_ms": now,
        }),
    )?;
    reconcile_diagnostics_trace_rotation(path)?;
    state.current_started_at_ms = now;
    state.last_rotation_at_ms = Some(now);
    state.rotation_count = state.rotation_count.saturating_add(1);
    state.compressed_files = count_compressed_trace_files(path) as u64;
    state.last_rotation_reason = Some(if size_due { "size" } else { "age" }.to_string());
    DIAGNOSTICS_TRACE_ROTATIONS_TOTAL.store(state.rotation_count, Ordering::Relaxed);
    DIAGNOSTICS_TRACE_COMPRESSED_TOTAL.store(state.compressed_files, Ordering::Relaxed);
    write_json_atomic(
        &state_path,
        &serde_json::to_value(&state).map_err(|e| e.to_string())?,
    )?;
    Ok(())
}

fn reconcile_diagnostics_trace_rotation(path: &std::path::Path) -> Result<(), String> {
    let journal_path = path.with_file_name("diagnostics_trace.rotation_journal.json");
    let legacy_pending = path.with_file_name("diagnostics_trace.rotation_pending.jsonl");
    if legacy_pending.exists() && !journal_path.exists() {
        let generation_id = diagnostics_unique_id("legacy-tracegen");
        let generation_path = trace_generation_path(path, &generation_id, "jsonl");
        std::fs::rename(&legacy_pending, &generation_path).map_err(|e| e.to_string())?;
    }

    if journal_path.exists() {
        let journal: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&journal_path).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        let generation_id = journal
            .get("generation_id")
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "diagnostics rotation journal has no generation id".to_string())?;
        let generation_path = trace_generation_path(path, generation_id, "jsonl");
        if !generation_path.exists() && path.exists() {
            std::fs::rename(path, &generation_path).map_err(|e| e.to_string())?;
        }
        finalize_trace_generation(path, generation_id)?;
        std::fs::remove_file(&journal_path).map_err(|e| e.to_string())?;
    }

    // Migrate every historical numbered generation once. Keeping legacy ZIPs beside immutable
    // generations would let the retained set grow to twice the advertised bound, so ZIPs are
    // renamed into the same immutable namespace before the single global prune below.
    for index in 1..=DIAGNOSTICS_TRACE_RETAINED_FILES {
        let legacy = path.with_file_name(format!("diagnostics_trace.{index}.jsonl"));
        if legacy.exists() {
            let generation_id = diagnostics_unique_id(&format!("legacy-{index}"));
            std::fs::rename(&legacy, trace_generation_path(path, &generation_id, "jsonl"))
                .map_err(|e| e.to_string())?;
        }
        let legacy_zip = path.with_file_name(format!("diagnostics_trace.{index}.zip"));
        if legacy_zip.exists() {
            let generation_id = diagnostics_unique_id(&format!("legacy-zip-{index}"));
            std::fs::rename(&legacy_zip, trace_generation_path(path, &generation_id, "zip"))
                .map_err(|e| e.to_string())?;
        }
    }

    // A crash may occur after the active file was renamed but before its journal was durable.
    // Immutable generation names make those orphans safe to discover and finish exactly once.
    for generation_path in trace_generation_files(path, "jsonl")? {
        if let Some(generation_id) = trace_generation_id(&generation_path, "jsonl") {
            finalize_trace_generation(path, &generation_id)?;
        }
    }
    prune_trace_generations(path)
}

fn trace_generation_path(path: &std::path::Path, generation_id: &str, extension: &str) -> std::path::PathBuf {
    path.with_file_name(format!("diagnostics_trace.generation.{generation_id}.{extension}"))
}

fn trace_generation_id(path: &std::path::Path, extension: &str) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    name.strip_prefix("diagnostics_trace.generation.")?
        .strip_suffix(&format!(".{extension}"))
        .map(str::to_string)
}

fn trace_generation_files(path: &std::path::Path, extension: &str) -> Result<Vec<std::path::PathBuf>, String> {
    let Some(parent) = path.parent() else { return Ok(Vec::new()); };
    let mut files = std::fs::read_dir(parent)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|candidate| trace_generation_id(candidate, extension).is_some())
        .collect::<Vec<_>>();
    files.sort_by_key(|candidate| {
        candidate.metadata().ok().and_then(|meta| meta.modified().ok())
    });
    Ok(files)
}

fn finalize_trace_generation(path: &std::path::Path, generation_id: &str) -> Result<(), String> {
    let source = trace_generation_path(path, generation_id, "jsonl");
    let destination = trace_generation_path(path, generation_id, "zip");
    if source.exists() {
        merge_rotated_trace_aggregate(path, &source, generation_id)?;
        if !destination.exists() {
            compress_trace_jsonl(&source, &destination)?;
        } else {
            std::fs::remove_file(&source).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn prune_trace_generations(path: &std::path::Path) -> Result<(), String> {
    let mut generations = trace_generation_files(path, "zip")?;
    generations.sort_by_key(|candidate| {
        std::cmp::Reverse(candidate.metadata().ok().and_then(|meta| meta.modified().ok()))
    });
    for old in generations.into_iter().skip(DIAGNOSTICS_TRACE_RETAINED_FILES) {
        std::fs::remove_file(old).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn count_compressed_trace_files(path: &std::path::Path) -> usize {
    let immutable = trace_generation_files(path, "zip")
        .map(|files| files.len())
        .unwrap_or(0);
    immutable
        + (1..=DIAGNOSTICS_TRACE_RETAINED_FILES)
            .filter(|index| {
                path.with_file_name(format!("diagnostics_trace.{index}.zip"))
                    .exists()
            })
            .count()
}

fn compress_trace_jsonl(
    source: &std::path::Path,
    destination: &std::path::Path,
) -> Result<(), String> {
    use std::io::{Read as _, Write as _};
    let temp = destination.with_extension("zip.tmp");
    let output = std::fs::File::create(&temp).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipWriter::new(output);
    archive
        .start_file(
            "diagnostics_trace.jsonl",
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated),
        )
        .map_err(|e| e.to_string())?;
    let mut input = std::fs::File::open(source).map_err(|e| e.to_string())?;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = input.read(&mut buffer).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        archive
            .write_all(&buffer[..read])
            .map_err(|e| e.to_string())?;
    }
    archive.finish().map_err(|e| e.to_string())?;
    std::fs::rename(&temp, destination).map_err(|e| e.to_string())?;
    std::fs::remove_file(source).map_err(|e| e.to_string())
}

fn merge_rotated_trace_aggregate(
    trace_path: &std::path::Path,
    source: &std::path::Path,
    generation_id: &str,
) -> Result<(), String> {
    use std::io::BufRead as _;
    let aggregate_path = trace_path.with_file_name("diagnostics_trace.aggregate.json");
    let mut aggregate = std::fs::read_to_string(&aggregate_path)
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .unwrap_or_else(
            || serde_json::json!({"schema_version":1,"events":{},"rows_total":0,"updated_at_ms":0,"merged_generations":[]}),
        );
    let merged = aggregate
        .get("merged_generations")
        .and_then(|value| value.as_array())
        .map(|values| values.iter().any(|value| value.as_str() == Some(generation_id)))
        .unwrap_or(false);
    if merged {
        return Ok(());
    }
    let events = aggregate["events"]
        .as_object_mut()
        .ok_or_else(|| "diagnostics aggregate events is not an object".to_string())?;
    let reader = std::io::BufReader::new(std::fs::File::open(source).map_err(|e| e.to_string())?);
    let mut added = 0_u64;
    for line in reader.lines() {
        let line = line.map_err(|e| e.to_string())?;
        let Ok(row) = serde_json::from_str::<DiagnosticsTraceEntry>(&line) else {
            continue;
        };
        let count = events
            .get(&row.event)
            .and_then(|value| value.as_u64())
            .unwrap_or(0);
        events.insert(row.event, serde_json::json!(count.saturating_add(1)));
        added = added.saturating_add(1);
    }
    aggregate["rows_total"] = serde_json::json!(aggregate["rows_total"]
        .as_u64()
        .unwrap_or(0)
        .saturating_add(added));
    aggregate["updated_at_ms"] = serde_json::json!(now_epoch_ms_i64());
    if !aggregate["merged_generations"].is_array() {
        aggregate["merged_generations"] = serde_json::json!([]);
    }
    aggregate["merged_generations"]
        .as_array_mut()
        .expect("merged generation array")
        .push(serde_json::json!(generation_id));
    if let Some(generations) = aggregate["merged_generations"].as_array_mut() {
        const AGGREGATE_GENERATION_RECEIPT_CAPACITY: usize = 64;
        if generations.len() > AGGREGATE_GENERATION_RECEIPT_CAPACITY {
            generations.drain(..generations.len() - AGGREGATE_GENERATION_RECEIPT_CAPACITY);
        }
    }
    write_json_atomic(&aggregate_path, &aggregate)
}

fn prune_diagnostics_incidents(paths: &AppPaths, active: Option<&str>) -> Result<(), String> {
    let root = paths
        .effective_diagnostics_trace_dir()
        .map_err(|e| e.to_string())?
        .join("incidents");
    if !root.exists() {
        return Ok(());
    }
    let now = now_epoch_ms_i64();
    let mut dirs = std::fs::read_dir(&root)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .collect::<Vec<_>>();
    dirs.sort_by_key(|entry| {
        std::cmp::Reverse(entry.metadata().ok().and_then(|m| m.modified().ok()))
    });
    let active_present = active.is_some_and(|id| {
        dirs.iter()
            .any(|entry| entry.file_name().to_string_lossy() == id)
    });
    let nonactive_capacity =
        DIAGNOSTICS_INCIDENT_RETAINED_COUNT.saturating_sub(usize::from(active_present));
    let mut retained_nonactive = 0usize;
    for entry in dirs {
        let name = entry.file_name().to_string_lossy().to_string();
        if active == Some(name.as_str()) {
            continue;
        }
        let modified_ms = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let expired = now.saturating_sub(modified_ms) > DIAGNOSTICS_INCIDENT_RETAINED_AGE_MS;
        if expired || retained_nonactive >= nonactive_capacity {
            std::fs::remove_dir_all(entry.path()).map_err(|e| e.to_string())?;
        } else {
            retained_nonactive += 1;
        }
    }
    Ok(())
}

fn diagnostics_capture_envelope(
    paths: &AppPaths,
    event: &str,
    details: &serde_json::Value,
) -> (Option<String>, Option<String>, u64, bool) {
    let now = now_epoch_ms_i64();
    let mut state = diagnostics_capture_state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if state.expires_at_ms.is_some_and(|expires| expires <= now) {
        let previous = state.clone();
        state.mode = "normal".to_string();
        state.armed_trigger = None;
        state.incident_id = None;
        state.started_at_ms = None;
        state.expires_at_ms = None;
        state.max_trace_bytes = DIAGNOSTICS_TRACE_NORMAL_MAX_BYTES;
        state.artifact_dir = None;
        state.root_span_id = None;
        if finalize_diagnostics_incident_manifest(&previous, "completed_expired").is_err() {
            record_diagnostics_persistence_failure();
        }
        if persist_diagnostics_capture_state(paths, &state).is_err() {
            record_diagnostics_persistence_failure();
        }
    }
    let should_trigger = match state.armed_trigger.as_deref() {
        Some("panel_switch") => event == "panel_switch",
        Some("job_start") => matches!(event, "job_started" | "job_track_dispatched"),
        _ => false,
    };
    if should_trigger {
        let incident_id = state
            .incident_id
            .clone()
            .unwrap_or_else(|| diagnostics_unique_id("incident"));
        let trace_dir = paths.effective_diagnostics_trace_dir().ok();
        state.mode = "incident".to_string();
        state.armed_trigger = None;
        state.incident_id = Some(incident_id.clone());
        state.started_at_ms = Some(now);
        state.expires_at_ms = Some(now + DIAGNOSTICS_INCIDENT_DURATION_MS);
        state.max_trace_bytes = DIAGNOSTICS_TRACE_INCIDENT_MAX_BYTES;
        state.artifact_dir = trace_dir.map(|dir| {
            dir.join("incidents")
                .join(&incident_id)
                .to_string_lossy()
                .to_string()
        });
        state.root_span_id = details
            .get("span_id")
            .and_then(|value| value.as_str())
            .map(str::to_string)
            .or_else(|| {
                details
                    .get("transition_id")
                    .map(|value| format!("panel-{value}"))
            });
        if persist_diagnostics_capture_state(paths, &state).is_err() {
            record_diagnostics_persistence_failure();
        }
    }
    let span_id = details
        .get("span_id")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .or_else(|| {
            details
                .get("transition_id")
                .map(|value| format!("panel-{value}"))
        })
        .or_else(|| {
            details
                .get("invocation_id")
                .map(|value| format!("invoke-{value}"))
        });
    let incident_id = (state.mode == "incident")
        .then(|| state.incident_id.clone())
        .flatten();
    (incident_id, span_id, state.max_trace_bytes, should_trigger)
}

fn activate_panel_capture_before_navigation(
    paths: &AppPaths,
    page: &str,
    transition_id: u64,
    panel_span_id: &str,
    parent_span_id: Option<&str>,
) -> Result<DiagnosticsPanelTransitionReceipt, String> {
    let page = page.trim();
    let panel_span_id = panel_span_id.trim();
    if page.is_empty() || panel_span_id.is_empty() {
        return Err("panel transition page and span_id are required".to_string());
    }
    let _write_guard = DIAGNOSTICS_TRACE_WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "diagnostics trace write lock is poisoned".to_string())?;
    let details = serde_json::json!({
        "page": page,
        "transition_id": transition_id,
        "span_id": panel_span_id,
        "parent_span_id": parent_span_id,
    });
    let (incident_id, _, _, activated_armed_capture) =
        diagnostics_capture_envelope(paths, "panel_switch", &details);
    let capture_mode = diagnostics_capture_state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .mode
        .clone();
    Ok(DiagnosticsPanelTransitionReceipt {
        incident_id,
        panel_span_id: panel_span_id.to_string(),
        parent_span_id: parent_span_id.map(str::to_string),
        capture_mode,
        activated_armed_capture,
    })
}

fn cancel_superseded_panel_capture(
    paths: &AppPaths,
    incident_id: &str,
    panel_span_id: &str,
) -> Result<bool, String> {
    let _write_guard = DIAGNOSTICS_TRACE_WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "diagnostics trace write lock is poisoned".to_string())?;
    let mut state = diagnostics_capture_state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if state.mode != "incident"
        || state.armed_trigger.is_some()
        || state.incident_id.as_deref() != Some(incident_id)
        || state.root_span_id.as_deref() != Some(panel_span_id)
    {
        return Ok(false);
    }
    let now = now_epoch_ms_i64();
    state.mode = "normal".to_string();
    state.armed_trigger = Some("panel_switch".to_string());
    state.armed_at_ms = Some(now);
    state.started_at_ms = None;
    state.expires_at_ms = Some(now + DIAGNOSTICS_INCIDENT_DURATION_MS);
    state.max_trace_bytes = DIAGNOSTICS_TRACE_INCIDENT_MAX_BYTES;
    state.artifact_dir = None;
    state.root_span_id = None;
    persist_diagnostics_capture_state(paths, &state)?;
    Ok(true)
}

fn capture_process_snapshot() -> Option<DiagnosticsProcessSnapshot> {
    let pid = sysinfo::get_current_pid().ok()?;
    let mut system = System::new();
    system.refresh_memory();
    system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    let process = system.process(pid)?;
    Some(DiagnosticsProcessSnapshot {
        pid: Some(pid.as_u32()),
        cpu_percent: Some(process.cpu_usage()),
        rss_bytes: Some(process.memory()),
        virtual_bytes: Some(process.virtual_memory()),
        system_used_bytes: Some(system.used_memory()),
        system_total_bytes: Some(system.total_memory()),
    })
}

fn append_diagnostics_trace_row(
    paths: &AppPaths,
    event: String,
    mut details: serde_json::Value,
    level: String,
) -> Result<String, String> {
    let _write_guard = DIAGNOSTICS_TRACE_WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "diagnostics trace write lock is poisoned".to_string())?;
    let path = diagnostics_trace_file_path(paths)?;
    let (incident_id, span_id, max_trace_bytes, _) =
        diagnostics_capture_envelope(paths, &event, &details);
    if incident_id.is_some() {
        let root_span_id = diagnostics_capture_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .root_span_id
            .clone();
        if let (Some(root_span_id), Some(object)) = (root_span_id, details.as_object_mut()) {
            if span_id.as_deref() != Some(root_span_id.as_str()) {
                object
                    .entry("parent_span_id".to_string())
                    .or_insert(serde_json::Value::String(root_span_id));
            }
        }
    }
    let include_process_snapshot = matches!(
        event.as_str(),
        "runtime_sample"
            | "freeze_detected"
            | "freeze_recovered"
            | "event_loop_skew"
            | "command_slow"
            | "database_locked"
            | "database_busy"
    );
    let mut row = DiagnosticsTraceEntry {
        ts_ms: now_epoch_ms_i64(),
        event,
        level,
        details: redact_diagnostics_value(details),
        process: include_process_snapshot
            .then(capture_process_snapshot)
            .flatten(),
        incident_id: incident_id.clone(),
        span_id,
    };

    let mut line = serde_json::to_string(&row).map_err(|e| e.to_string())?;
    if line.len().saturating_add(1) > DIAGNOSTICS_TRACE_MAX_ROW_BYTES {
        let original_bytes = line.len().saturating_add(1);
        row.details = serde_json::json!({
            "reason": "trace_row_too_large",
            "original_event": row.event,
            "original_bytes": original_bytes,
            "max_row_bytes": DIAGNOSTICS_TRACE_MAX_ROW_BYTES,
        });
        row.event = "diagnostics_event_truncated".to_string();
        row.level = "warn".to_string();
        line = serde_json::to_string(&row).map_err(|e| e.to_string())?;
        record_diagnostics_loss(None);
    }
    let line_bytes = line.len().saturating_add(1) as u64;
    rotate_diagnostics_trace_if_needed(&path, max_trace_bytes, line_bytes)?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| e.to_string())?;

    use std::io::Write as _;
    writeln!(file, "{line}").map_err(|e| e.to_string())?;

    // Clone once. Re-locking DIAGNOSTICS_CAPTURE_STATE in this branch used to
    // self-deadlock an armed capture on its first triggering event.
    let capture_snapshot = diagnostics_capture_state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    if let (Some(incident_id), Some(artifact_dir)) =
        (incident_id, capture_snapshot.artifact_dir.clone())
    {
        let incident_dir = std::path::PathBuf::from(artifact_dir);
        std::fs::create_dir_all(&incident_dir).map_err(|e| e.to_string())?;
        let incident_path = incident_dir.join("trace.jsonl");
        let incident_bytes = std::fs::metadata(&incident_path)
            .map(|meta| meta.len())
            .unwrap_or(0);
        if incident_bytes.saturating_add(line_bytes) <= DIAGNOSTICS_TRACE_INCIDENT_MAX_BYTES {
            let mut incident_file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&incident_path)
                .map_err(|e| e.to_string())?;
            writeln!(incident_file, "{line}").map_err(|e| e.to_string())?;
        } else {
            record_diagnostics_loss(None);
        }
        let manifest_path = incident_dir.join("manifest.json");
        if !manifest_path.exists() {
            let manifest = serde_json::json!({
                "wp": "WP-0298",
                "incident_id": incident_id,
                "status": "capturing",
                "trace_path": incident_path.to_string_lossy(),
                "updated_at_ms": now_epoch_ms_i64(),
                "capture": capture_snapshot,
            });
            if write_json_atomic(&manifest_path, &manifest).is_err() {
                record_diagnostics_persistence_failure();
            }
            if prune_diagnostics_incidents(paths, Some(&incident_id)).is_err() {
                record_diagnostics_persistence_failure();
            }
        }
    }

    if let Ok(bytes) = std::fs::metadata(&path).map(|meta| meta.len()) {
        let mut state = diagnostics_capture_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.trace_bytes = bytes;
        state.dropped_events = DIAGNOSTICS_TRACE_DROPPED_TOTAL.load(Ordering::Relaxed);
    }

    Ok(path.to_string_lossy().to_string())
}

fn record_diagnostics_loss(category_total: Option<&AtomicU64>) {
    DIAGNOSTICS_TRACE_DROPPED_PENDING.fetch_add(1, Ordering::Relaxed);
    DIAGNOSTICS_TRACE_DROPPED_TOTAL.fetch_add(1, Ordering::Relaxed);
    if let Some(counter) = category_total {
        counter.fetch_add(1, Ordering::Relaxed);
    }
}

fn record_diagnostics_persistence_failure() {
    record_diagnostics_loss(Some(&DIAGNOSTICS_TRACE_ASYNC_WRITE_FAILURES_TOTAL));
}

fn flush_pending_diagnostics_loss_receipt(paths: &AppPaths) {
    let pending = DIAGNOSTICS_TRACE_DROPPED_PENDING.load(Ordering::Acquire);
    if pending == 0 {
        return;
    }
    match append_diagnostics_trace_row(
        paths,
        "diagnostics_events_dropped".to_string(),
        serde_json::json!({
            "dropped_events": pending,
            "dropped_events_total": DIAGNOSTICS_TRACE_DROPPED_TOTAL.load(Ordering::Relaxed),
            "async_write_failures_total": DIAGNOSTICS_TRACE_ASYNC_WRITE_FAILURES_TOTAL.load(Ordering::Relaxed),
            "queue_rejections_total": DIAGNOSTICS_TRACE_QUEUE_REJECTIONS_TOTAL.load(Ordering::Relaxed),
            "queue_capacity": DIAGNOSTICS_TRACE_QUEUE_CAPACITY,
        }),
        "warn".to_string(),
    ) {
        Ok(_) => {
            DIAGNOSTICS_TRACE_DROPPED_PENDING.fetch_sub(pending, Ordering::AcqRel);
        }
        Err(_) => {
            record_diagnostics_persistence_failure();
        }
    }
}

fn diagnostics_trace_queue() -> &'static SyncSender<DiagnosticsTraceWriteRequest> {
    DIAGNOSTICS_TRACE_QUEUE.get_or_init(|| {
        let (sender, receiver) =
            sync_channel::<DiagnosticsTraceWriteRequest>(DIAGNOSTICS_TRACE_QUEUE_CAPACITY);
        std::thread::Builder::new()
            .name("voxvulgi-diagnostics-writer".to_string())
            .spawn(move || {
                while let Ok(request) = receiver.recv() {
                    flush_pending_diagnostics_loss_receipt(&request.paths);
                    if append_diagnostics_trace_row(
                        &request.paths,
                        request.event,
                        request.details,
                        request.level,
                    )
                    .is_err()
                    {
                        record_diagnostics_persistence_failure();
                    }
                }
            })
            .unwrap_or_else(|error| panic!("diagnostics writer thread failed to start: {error}"));
        sender
    })
}

fn append_diagnostics_trace_row_best_effort(
    paths: &AppPaths,
    event: &str,
    details: serde_json::Value,
    level: &str,
) -> DiagnosticsTraceEnqueueReceipt {
    let request = DiagnosticsTraceWriteRequest {
        paths: paths.clone(),
        event: event.to_string(),
        details,
        level: level.to_string(),
    };
    let accepted = match diagnostics_trace_queue().try_send(request) {
        Ok(()) => true,
        Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {
            record_diagnostics_loss(Some(&DIAGNOSTICS_TRACE_QUEUE_REJECTIONS_TOTAL));
            false
        }
    };
    DiagnosticsTraceEnqueueReceipt {
        accepted,
        dropped_events_total: DIAGNOSTICS_TRACE_DROPPED_TOTAL.load(Ordering::Relaxed),
        async_write_failures_total: DIAGNOSTICS_TRACE_ASYNC_WRITE_FAILURES_TOTAL
            .load(Ordering::Relaxed),
        pending_loss_events: DIAGNOSTICS_TRACE_DROPPED_PENDING.load(Ordering::Relaxed),
    }
}

fn trace_database_command_error(
    paths: &AppPaths,
    command: &'static str,
    message: String,
) -> String {
    let lower = message.to_ascii_lowercase();
    let event = if lower.contains("database is locked")
        || lower.contains("database table is locked")
        || lower.contains("sqlite_locked")
    {
        Some("database_locked")
    } else if lower.contains("database is busy")
        || lower.contains("database busy")
        || lower.contains("sqlite_busy")
    {
        Some("database_busy")
    } else {
        None
    };

    if let Some(event) = event {
        append_diagnostics_trace_row_best_effort(
            paths,
            event,
            serde_json::json!({
                "cmd": command,
                "error": message,
            }),
            "warn",
        );
    }

    message
}

// WP-0221: RAII timer for instrumented Tauri commands. Construct at the top of
// a command; on drop it records elapsed_ms. A `command_slow` row is also
// emitted when elapsed >= 500 ms so the trace identifies any single hang
// without flooding on fast calls.
struct InvokeTimer {
    paths: AppPaths,
    name: &'static str,
    invocation_id: u64,
    started: std::time::Instant,
    started_at_ms: i64,
    request_id: Option<String>,
    span_id: Option<String>,
}

#[derive(Clone)]
struct InvokePhaseRecorder {
    paths: AppPaths,
    name: &'static str,
    invocation_id: u64,
    request_id: Option<String>,
    span_id: Option<String>,
}

impl InvokePhaseRecorder {
    fn phase(&self, phase: &'static str, elapsed: Duration) {
        let _ = append_diagnostics_trace_row_best_effort(
            &self.paths,
            "command_phase",
            serde_json::json!({
                "cmd": self.name,
                "invocation_id": self.invocation_id,
                "span_id": self.span_id,
                "request_id": self.request_id,
                "phase": phase,
                "elapsed_ms": elapsed.as_millis() as u64,
            }),
            "info",
        );
    }
}

static INVOKE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

impl InvokeTimer {
    fn start(paths: AppPaths, name: &'static str) -> Self {
        Self::start_with_context(paths, name, None, None)
    }
    fn start_with_context(
        paths: AppPaths,
        name: &'static str,
        request_id: Option<String>,
        span_id: Option<String>,
    ) -> Self {
        let invocation_id = INVOKE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let started_at_ms = now_epoch_ms_i64();
        append_diagnostics_trace_row_best_effort(
            &paths,
            "command_started",
            serde_json::json!({
                "cmd": name,
                "invocation_id": invocation_id,
                "started_at_ms": started_at_ms,
                "request_id": request_id,
                "span_id": span_id,
            }),
            "info",
        );
        Self {
            paths,
            name,
            invocation_id,
            started: std::time::Instant::now(),
            started_at_ms,
            request_id,
            span_id,
        }
    }

    fn phase(&self, phase: &'static str, elapsed: Duration) {
        self.phase_recorder().phase(phase, elapsed);
    }

    fn phase_recorder(&self) -> InvokePhaseRecorder {
        InvokePhaseRecorder {
            paths: self.paths.clone(),
            name: self.name,
            invocation_id: self.invocation_id,
            request_id: self.request_id.clone(),
            span_id: self.span_id.clone(),
        }
    }
}

impl Drop for InvokeTimer {
    fn drop(&mut self) {
        let elapsed_ms = self.started.elapsed().as_millis() as u64;
        append_diagnostics_trace_row_best_effort(
            &self.paths,
            "command_completed",
            serde_json::json!({
                "cmd": self.name,
                "invocation_id": self.invocation_id,
                "started_at_ms": self.started_at_ms,
                "elapsed_ms": elapsed_ms,
                "request_id": self.request_id,
                "span_id": self.span_id,
            }),
            "info",
        );
        if elapsed_ms >= 500 {
            append_diagnostics_trace_row_best_effort(
                &self.paths,
                "command_slow",
                serde_json::json!({
                    "cmd": self.name,
                    "invocation_id": self.invocation_id,
                    "started_at_ms": self.started_at_ms,
                    "elapsed_ms": elapsed_ms,
                    "request_id": self.request_id,
                    "span_id": self.span_id,
                }),
                "warn",
            );
        }
    }
}

// WP-0221: Process-scheduling skew heartbeat. Spawned on a dedicated OS thread
// at app boot. Sleeps for `target_interval_ms` and measures wall-clock vs
// expected. If the process is starved (hung syscall, SMB stall, AV scan,
// DLL loader lock, OS thrash), the actual interval exceeds expected by more
// than `skew_threshold_ms` and we emit one `event_loop_skew` trace row.
fn spawn_event_loop_skew_heartbeat(paths: AppPaths) {
    const TARGET_INTERVAL_MS: u64 = 250;
    const SKEW_THRESHOLD_MS: u64 = 500;
    std::thread::spawn(move || {
        let mut last = std::time::Instant::now();
        loop {
            std::thread::sleep(Duration::from_millis(TARGET_INTERVAL_MS));
            let now = std::time::Instant::now();
            let elapsed_ms = now.duration_since(last).as_millis() as u64;
            last = now;
            let skew_ms = elapsed_ms.saturating_sub(TARGET_INTERVAL_MS);
            if skew_ms >= SKEW_THRESHOLD_MS {
                append_diagnostics_trace_row_best_effort(
                    &paths,
                    "event_loop_skew",
                    serde_json::json!({
                        "target_interval_ms": TARGET_INTERVAL_MS,
                        "actual_interval_ms": elapsed_ms,
                        "skew_ms": skew_ms,
                    }),
                    "warn",
                );
            }
        }
    });
}

fn read_recent_diagnostics_trace_entries(
    paths: &AppPaths,
    limit: usize,
) -> Result<Vec<DiagnosticsTraceEntry>, String> {
    let path = diagnostics_trace_file_path(paths)?;
    let _guard = DIAGNOSTICS_TRACE_WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "diagnostics trace write lock is poisoned".to_string())?;
    reconcile_diagnostics_trace_rotation(&path)?;

    use std::collections::VecDeque;
    use std::io::{Read as _, Seek as _, SeekFrom};
    let mut bytes = Vec::new();
    for zip_path in trace_generation_files(&path, "zip")? {
        let file = std::fs::File::open(&zip_path).map_err(|e| e.to_string())?;
        let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
        let mut entry = archive.by_index(0).map_err(|e| e.to_string())?;
        let entry_was_truncated = entry.size() > DIAGNOSTICS_TRACE_TAIL_READ_BYTES;
        let mut ring = VecDeque::with_capacity(DIAGNOSTICS_TRACE_TAIL_READ_BYTES as usize);
        let mut chunk = [0_u8; 64 * 1024];
        loop {
            let read = entry.read(&mut chunk).map_err(|e| e.to_string())?;
            if read == 0 { break; }
            for byte in &chunk[..read] {
                if ring.len() == DIAGNOSTICS_TRACE_TAIL_READ_BYTES as usize { ring.pop_front(); }
                ring.push_back(*byte);
            }
        }
        let mut candidate_bytes = Vec::from(ring);
        if entry_was_truncated {
            if let Some(pos) = candidate_bytes.iter().position(|byte| *byte == b'\n') {
                candidate_bytes.drain(..=pos);
            }
        }
        bytes.extend(candidate_bytes);
        if bytes.len() as u64 > DIAGNOSTICS_TRACE_TAIL_READ_BYTES {
            let excess = bytes.len() - DIAGNOSTICS_TRACE_TAIL_READ_BYTES as usize;
            bytes.drain(..excess);
            if let Some(pos) = bytes.iter().position(|byte| *byte == b'\n') { bytes.drain(..=pos); }
        }
    }
    for index in (0..=DIAGNOSTICS_TRACE_RETAINED_FILES).rev() {
        let jsonl = if index == 0 {
            path.clone()
        } else {
            path.with_file_name(format!("diagnostics_trace.{index}.jsonl"))
        };
        let zip_path = path.with_file_name(format!("diagnostics_trace.{index}.zip"));
        let mut candidate_bytes = Vec::new();
        if jsonl.exists() {
            let mut file = std::fs::File::open(&jsonl).map_err(|e| e.to_string())?;
            let length = file.metadata().map_err(|e| e.to_string())?.len();
            let start = length.saturating_sub(DIAGNOSTICS_TRACE_TAIL_READ_BYTES);
            file.seek(SeekFrom::Start(start))
                .map_err(|e| e.to_string())?;
            file.take(DIAGNOSTICS_TRACE_TAIL_READ_BYTES)
                .read_to_end(&mut candidate_bytes)
                .map_err(|e| e.to_string())?;
            if start > 0 {
                if let Some(pos) = candidate_bytes.iter().position(|byte| *byte == b'\n') {
                    candidate_bytes.drain(..=pos);
                }
            }
        } else if index > 0 && zip_path.exists() {
            let file = std::fs::File::open(&zip_path).map_err(|e| e.to_string())?;
            let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
            let mut entry = archive.by_index(0).map_err(|e| e.to_string())?;
            let entry_was_truncated = entry.size() > DIAGNOSTICS_TRACE_TAIL_READ_BYTES;
            let mut ring = VecDeque::with_capacity(DIAGNOSTICS_TRACE_TAIL_READ_BYTES as usize);
            let mut chunk = [0_u8; 64 * 1024];
            loop {
                let read = entry.read(&mut chunk).map_err(|e| e.to_string())?;
                if read == 0 {
                    break;
                }
                for byte in &chunk[..read] {
                    if ring.len() == DIAGNOSTICS_TRACE_TAIL_READ_BYTES as usize {
                        ring.pop_front();
                    }
                    ring.push_back(*byte);
                }
            }
            candidate_bytes.extend(ring);
            if entry_was_truncated {
                if let Some(pos) = candidate_bytes.iter().position(|byte| *byte == b'\n') {
                    candidate_bytes.drain(..=pos);
                }
            }
        }
        bytes.extend(candidate_bytes);
        if bytes.len() as u64 > DIAGNOSTICS_TRACE_TAIL_READ_BYTES {
            let excess = bytes.len() - DIAGNOSTICS_TRACE_TAIL_READ_BYTES as usize;
            bytes.drain(..excess);
            if let Some(pos) = bytes.iter().position(|byte| *byte == b'\n') {
                bytes.drain(..=pos);
            }
        }
    }
    let text = String::from_utf8_lossy(&bytes);
    let mut entries: Vec<DiagnosticsTraceEntry> = text
        .lines()
        .filter(|line| line.len() <= DIAGNOSTICS_TRACE_MAX_ROW_BYTES)
        .filter_map(|line| serde_json::from_str::<DiagnosticsTraceEntry>(line).ok())
        .collect();

    if entries.len() > limit {
        let drain_until = entries.len().saturating_sub(limit);
        entries.drain(0..drain_until);
    }

    Ok(entries)
}

fn set_startup_phase(
    startup: &Arc<Mutex<StartupTracker>>,
    paths: &AppPaths,
    phase_id: &str,
    state: &str,
    error: Option<String>,
) {
    if let Ok(mut tracker) = startup.lock() {
        tracker.set_phase_state(phase_id, state, error.clone());
    }
    append_diagnostics_trace_row_best_effort(
        paths,
        "startup_phase",
        serde_json::json!({
            "phase_id": phase_id,
            "label": startup_phase_label(phase_id),
            "state": state,
            "error": error,
        }),
        if state == "error" { "error" } else { "info" },
    );
}

fn is_safe_relative_path(path: &std::path::Path) -> bool {
    !path.components().any(|c| {
        matches!(
            c,
            std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_)
        )
    })
}

fn extract_payload_zip_best_effort(
    zip_path: &std::path::Path,
    paths: &AppPaths,
) -> Result<ZipExtractSummary, String> {
    use zip::result::ZipError;

    let file = std::fs::File::open(zip_path).map_err(|e| {
        format!(
            "failed to open payload zip {}: {e}",
            zip_path.to_string_lossy()
        )
    })?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| {
        format!(
            "failed to read payload zip {}: {e}",
            zip_path.to_string_lossy()
        )
    })?;

    let mut summary = ZipExtractSummary::default();

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| match e {
            ZipError::FileNotFound => "payload zip entry missing".to_string(),
            other => format!("payload zip read failed: {other}"),
        })?;

        let name = entry.name().replace('\\', "/");

        let (dst_root, rel) = if let Some(rest) = name.strip_prefix("tools/") {
            (paths.tools_dir(), rest)
        } else if let Some(rest) = name.strip_prefix("models/") {
            (paths.models_dir(), rest)
        } else if let Some(rest) = name.strip_prefix("cache/huggingface/") {
            (paths.cache_dir().join("huggingface"), rest)
        } else {
            continue;
        };

        let rel = rel.trim_start_matches('/');
        if rel.is_empty() {
            continue;
        }

        let rel_path = std::path::Path::new(rel);
        if !is_safe_relative_path(rel_path) {
            return Err(format!("unsafe payload zip path: {name}"));
        }

        let out_path = dst_root.join(rel_path);
        if entry.is_dir() {
            std::fs::create_dir_all(&out_path)
                .map_err(|e| format!("failed to create dir {}: {e}", out_path.to_string_lossy()))?;
            continue;
        }

        if let Ok(meta) = std::fs::metadata(&out_path) {
            let expected = entry.size();
            if expected > 0 && meta.is_file() && meta.len() == expected {
                summary.skipped_files += 1;
                continue;
            }
        }

        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create dir {}: {e}", parent.to_string_lossy()))?;
        }

        let tmp = out_path.with_extension("extracting");
        let _ = std::fs::remove_file(&tmp);

        {
            let mut out_file = std::fs::File::create(&tmp)
                .map_err(|e| format!("failed to create file {}: {e}", tmp.to_string_lossy()))?;
            std::io::copy(&mut entry, &mut out_file)
                .map_err(|e| format!("failed to extract {}: {e}", name))?;
        }

        if out_path.exists() {
            let _ = std::fs::remove_file(&out_path);
        }
        if std::fs::rename(&tmp, &out_path).is_err() {
            std::fs::copy(&tmp, &out_path).map_err(|e| {
                format!(
                    "failed to finalize extract {} -> {}: {e}",
                    tmp.to_string_lossy(),
                    out_path.to_string_lossy()
                )
            })?;
            let _ = std::fs::remove_file(&tmp);
        }

        summary.extracted_files += 1;
        summary.extracted_bytes += entry.size();
    }

    Ok(summary)
}

fn find_offline_bundle_root(resource_dir: &std::path::Path) -> Option<std::path::PathBuf> {
    let candidates = [
        resource_dir.join("offline"),
        resource_dir.join("resources").join("offline"),
        resource_dir.join("offline_bundle"),
        resource_dir.join("resources").join("offline_bundle"),
    ];

    for candidate in candidates {
        let manifest_path = candidate.join("manifest.json");
        if manifest_path.is_file() {
            return Some(candidate);
        }
    }

    None
}

fn read_offline_bundle_manifest(
    bundle_root: &std::path::Path,
) -> Result<OfflineBundleManifest, String> {
    let manifest_path = bundle_root.join("manifest.json");
    let bytes = std::fs::read(&manifest_path).map_err(|e| {
        format!(
            "failed to read offline bundle manifest {}: {e}",
            manifest_path.to_string_lossy()
        )
    })?;
    serde_json::from_slice::<OfflineBundleManifest>(&bytes).map_err(|e| {
        format!(
            "offline bundle manifest is invalid JSON ({}): {e}",
            manifest_path.to_string_lossy()
        )
    })
}

fn sha256_hex_file(path: &std::path::Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|e| {
        format!(
            "failed to open payload zip {} for hashing: {e}",
            path.to_string_lossy()
        )
    })?;
    let mut hasher = Sha256::new();
    let mut buf = [0_u8; 1024 * 1024];
    loop {
        use std::io::Read as _;
        let read = file.read(&mut buf).map_err(|e| {
            format!(
                "failed to read payload zip {} for hashing: {e}",
                path.to_string_lossy()
            )
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    Ok(hex::encode_upper(hasher.finalize()))
}

fn verify_offline_payload_integrity(
    manifest: &OfflineBundleManifest,
    payload_zip_path: &std::path::Path,
) -> Result<(), String> {
    if let Some(expected_bytes) = manifest.payload_bytes {
        let actual_bytes = std::fs::metadata(payload_zip_path)
            .map_err(|e| {
                format!(
                    "failed to stat payload zip {}: {e}",
                    payload_zip_path.to_string_lossy()
                )
            })?
            .len();
        if actual_bytes != expected_bytes {
            return Err(format!(
                "offline bundle payload byte mismatch for {}: expected={} actual={}",
                payload_zip_path.to_string_lossy(),
                expected_bytes,
                actual_bytes
            ));
        }
    }

    if let Some(expected_sha256) = manifest
        .payload_sha256
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        let algorithm = manifest
            .payload_sha256_algorithm
            .as_deref()
            .unwrap_or("sha256")
            .trim()
            .to_ascii_lowercase();
        if algorithm != "sha256" {
            return Err(format!(
                "unsupported offline payload hash algorithm: {}",
                manifest
                    .payload_sha256_algorithm
                    .as_deref()
                    .unwrap_or("sha256")
            ));
        }
        let actual = sha256_hex_file(payload_zip_path)?;
        if !actual.eq_ignore_ascii_case(expected_sha256) {
            return Err(format!(
                "offline bundle payload sha256 mismatch for {}: expected={} actual={}",
                payload_zip_path.to_string_lossy(),
                expected_sha256,
                actual
            ));
        }
    }

    Ok(())
}

fn offline_bundle_marker_path(paths: &AppPaths) -> std::path::PathBuf {
    paths.config_dir().join("offline_bundle_applied_v1.json")
}

fn offline_bundle_already_applied(paths: &AppPaths, bundle_id: &str) -> bool {
    let marker = offline_bundle_marker_path(paths);
    let Ok(bytes) = std::fs::read(marker) else {
        return false;
    };
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return false;
    };
    v.get("bundle_id")
        .and_then(|s| s.as_str())
        .map(|s| s == bundle_id)
        .unwrap_or(false)
}

fn write_offline_bundle_marker(
    paths: &AppPaths,
    bundle_root: &std::path::Path,
    bundle_id: &str,
) -> Result<(), String> {
    let marker = offline_bundle_marker_path(paths);
    if let Some(parent) = marker.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let record = serde_json::json!({
        "schema_version": 1,
        "bundle_id": bundle_id,
        "bundle_root": bundle_root.to_string_lossy(),
        "applied_at_ms": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64,
    });

    std::fs::write(
        &marker,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&record).unwrap_or_else(|_| "{}".to_string())
        ),
    )
    .map_err(|e| {
        format!(
            "failed to write offline bundle marker {}: {e}",
            marker.to_string_lossy()
        )
    })?;
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct OfflineBundleRuntimeReadyFlags {
    ffmpeg_installed: bool,
    ytdlp_available: bool,
    js_runtime_available: bool,
    portable_python_installed: bool,
    venv_ready: bool,
    diarization_installed: bool,
    neural_tts_installed: bool,
    voice_preserving_installed: bool,
}

fn offline_bundle_runtime_ready_from_flags(flags: OfflineBundleRuntimeReadyFlags) -> bool {
    flags.ffmpeg_installed
        && flags.ytdlp_available
        && flags.js_runtime_available
        && flags.portable_python_installed
        && flags.venv_ready
        && flags.diarization_installed
        && flags.neural_tts_installed
        && flags.voice_preserving_installed
}

fn offline_bundle_runtime_already_ready(paths: &AppPaths) -> bool {
    let ffmpeg = tools::ffmpeg_tools_status(paths);
    let ytdlp = tools::ytdlp_tools_status(paths);
    let js_runtime = tools::js_runtime_tools_status(paths);
    let python = tools::python_toolchain_status(paths);
    let portable_python = tools::portable_python_status(paths);
    let diarization = tools::diarization_pack_status(paths);
    let neural_tts = tools::tts_neural_local_v1_pack_status(paths);
    let voice_preserving = tools::tts_voice_preserving_local_v1_pack_status(paths);

    offline_bundle_runtime_ready_from_flags(OfflineBundleRuntimeReadyFlags {
        ffmpeg_installed: ffmpeg.installed,
        ytdlp_available: ytdlp.available,
        js_runtime_available: js_runtime.available,
        portable_python_installed: portable_python.installed,
        venv_ready: python.venv_exists && python.venv_python_version.is_some(),
        diarization_installed: diarization.installed,
        neural_tts_installed: neural_tts.installed,
        voice_preserving_installed: voice_preserving.installed,
    })
}

fn copy_tree_best_effort(
    src_root: &std::path::Path,
    dst_root: &std::path::Path,
) -> Result<CopySummary, String> {
    if !src_root.exists() {
        return Ok(CopySummary::default());
    }

    let mut summary = CopySummary::default();
    let mut stack: Vec<std::path::PathBuf> = vec![src_root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir)
            .map_err(|e| format!("failed to read dir {}: {e}", dir.to_string_lossy()))?;

        for entry in entries.flatten() {
            let path = entry.path();
            let file_type = match entry.file_type() {
                Ok(v) => v,
                Err(_) => continue,
            };

            let rel = match path.strip_prefix(src_root) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let dst = dst_root.join(rel);

            if file_type.is_dir() {
                std::fs::create_dir_all(&dst)
                    .map_err(|e| format!("failed to create dir {}: {e}", dst.to_string_lossy()))?;
                stack.push(path);
                continue;
            }

            if !file_type.is_file() {
                continue;
            }

            let src_meta = match std::fs::metadata(&path) {
                Ok(v) => v,
                Err(_) => continue,
            };

            if let Ok(dst_meta) = std::fs::metadata(&dst) {
                if dst_meta.len() == src_meta.len() && src_meta.len() > 0 {
                    summary.skipped_files += 1;
                    continue;
                }
            }

            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    format!("failed to create dir {}: {e}", parent.to_string_lossy())
                })?;
            }

            let tmp = dst.with_extension("copying");
            let _ = std::fs::remove_file(&tmp);

            std::fs::copy(&path, &tmp).map_err(|e| {
                format!(
                    "failed to copy {} -> {}: {e}",
                    path.to_string_lossy(),
                    tmp.to_string_lossy()
                )
            })?;

            if dst.exists() {
                let _ = std::fs::remove_file(&dst);
            }
            if std::fs::rename(&tmp, &dst).is_err() {
                std::fs::copy(&tmp, &dst).map_err(|e| {
                    format!(
                        "failed to finalize copy {} -> {}: {e}",
                        tmp.to_string_lossy(),
                        dst.to_string_lossy()
                    )
                })?;
                let _ = std::fs::remove_file(&tmp);
            }

            summary.copied_files += 1;
            summary.copied_bytes += src_meta.len();
        }
    }

    Ok(summary)
}

fn patch_venv_pyvenv_cfg_best_effort(paths: &AppPaths) -> Result<(), String> {
    let venv_dir = paths.python_venv_dir();
    let cfg_path = venv_dir.join("pyvenv.cfg");
    if !cfg_path.is_file() {
        return Ok(());
    }

    let portable_dir = paths.python_portable_dir();
    let portable_python = paths.python_portable_python_exe();
    if !portable_python.is_file() {
        return Ok(());
    }

    let raw = std::fs::read_to_string(&cfg_path)
        .map_err(|e| format!("failed to read {}: {e}", cfg_path.to_string_lossy()))?;

    let mut out: Vec<String> = Vec::new();
    let mut wrote_home = false;
    let mut wrote_executable = false;
    let mut wrote_command = false;

    for line in raw.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("home =") {
            out.push(format!("home = {}", portable_dir.to_string_lossy()));
            wrote_home = true;
            continue;
        }
        if trimmed.starts_with("executable =") {
            out.push(format!(
                "executable = {}",
                portable_python.to_string_lossy()
            ));
            wrote_executable = true;
            continue;
        }
        if trimmed.starts_with("command =") {
            out.push(format!(
                "command = {} -m venv {}",
                portable_python.to_string_lossy(),
                venv_dir.to_string_lossy()
            ));
            wrote_command = true;
            continue;
        }
        out.push(line.to_string());
    }

    if !wrote_home {
        out.push(format!("home = {}", portable_dir.to_string_lossy()));
    }
    if !wrote_executable {
        out.push(format!(
            "executable = {}",
            portable_python.to_string_lossy()
        ));
    }
    if !wrote_command {
        out.push(format!(
            "command = {} -m venv {}",
            portable_python.to_string_lossy(),
            venv_dir.to_string_lossy()
        ));
    }

    std::fs::write(&cfg_path, format!("{}\n", out.join("\n")))
        .map_err(|e| format!("failed to write {}: {e}", cfg_path.to_string_lossy()))?;
    Ok(())
}

fn apply_offline_bundle_if_present(
    paths: &AppPaths,
    resource_dir: &std::path::Path,
) -> Result<(), String> {
    let Some(bundle_root) = find_offline_bundle_root(resource_dir) else {
        return Ok(());
    };

    let manifest = read_offline_bundle_manifest(&bundle_root)?;
    if manifest.schema_version != 1 {
        return Err(format!(
            "unsupported offline bundle schema_version: {}",
            manifest.schema_version
        ));
    }

    patch_venv_pyvenv_cfg_best_effort(paths)?;

    if offline_bundle_already_applied(paths, &manifest.bundle_id) {
        return Ok(());
    }

    if offline_bundle_runtime_already_ready(paths) {
        write_offline_bundle_marker(paths, &bundle_root, &manifest.bundle_id)?;
        eprintln!(
            "offline bundle: runtime already ready; recorded bundle_id={} without rehydrating payload",
            manifest.bundle_id,
        );
        return Ok(());
    }

    eprintln!(
        "offline bundle: applying bundle_id={} from {} into {}",
        manifest.bundle_id,
        bundle_root.to_string_lossy(),
        paths.base_dir.to_string_lossy()
    );

    let payload_zip_name = manifest
        .payload_zip
        .clone()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "payload.zip".to_string());
    let payload_zip_path = bundle_root.join(&payload_zip_name);

    if payload_zip_path.is_file() {
        verify_offline_payload_integrity(&manifest, &payload_zip_path)?;
        let sum = extract_payload_zip_best_effort(&payload_zip_path, paths)?;
        patch_venv_pyvenv_cfg_best_effort(paths)?;
        write_offline_bundle_marker(paths, &bundle_root, &manifest.bundle_id)?;

        eprintln!(
            "offline bundle: extracted payload zip {} (files={} bytes={} skipped={})",
            payload_zip_name, sum.extracted_files, sum.extracted_bytes, sum.skipped_files,
        );

        return Ok(());
    }

    // Back-compat: directory-based bundle format.
    let tools_src = bundle_root.join("tools");
    let models_src = bundle_root.join("models");
    let hf_cache_src = bundle_root.join("cache").join("huggingface");

    if !tools_src.exists() && !models_src.exists() && !hf_cache_src.exists() {
        return Err(format!(
            "offline bundle is missing payload.zip and has no legacy directories (bundle_root={})",
            bundle_root.to_string_lossy()
        ));
    }

    let tools_sum = copy_tree_best_effort(&tools_src, &paths.tools_dir())?;
    let models_sum = copy_tree_best_effort(&models_src, &paths.models_dir())?;
    let hf_sum = copy_tree_best_effort(&hf_cache_src, &paths.cache_dir().join("huggingface"))?;

    patch_venv_pyvenv_cfg_best_effort(paths)?;
    write_offline_bundle_marker(paths, &bundle_root, &manifest.bundle_id)?;

    eprintln!(
        "offline bundle: copied tools(files={} bytes={} skipped={}), models(files={} bytes={} skipped={}), hf_cache(files={} bytes={} skipped={})",
        tools_sum.copied_files,
        tools_sum.copied_bytes,
        tools_sum.skipped_files,
        models_sum.copied_files,
        models_sum.copied_bytes,
        models_sum.skipped_files,
        hf_sum.copied_files,
        hf_sum.copied_bytes,
        hf_sum.skipped_files,
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;
    use voxvulgi_engine::{config, db, paths::AppPaths};

    fn retention_receipt(has_more: bool) -> voxvulgi_engine::youtube_protection::DownloaderRetentionDrainReceipt {
        voxvulgi_engine::youtube_protection::DownloaderRetentionDrainReceipt {
            batches: 1,
            deleted: 100,
            complete: !has_more,
            has_more,
            cutoff_ms: 1,
            elapsed_ms: 1,
            budget_exhausted: has_more,
        }
    }

    #[test]
    fn retention_worker_reschedules_finite_rounds_until_durable_completion() {
        let mut drains = 0_u64;
        let mut persistence = Vec::new();
        let mut waits = Vec::new();
        let complete = run_youtube_retention_worker_loop(
            || {
                drains = drains.saturating_add(1);
                Ok(retention_receipt(drains <= 5))
            },
            |pending, failures| {
                persistence.push((pending, failures));
                true
            },
            || false,
            |duration| {
                waits.push(duration);
                false
            },
            YoutubeRetentionWorkerPolicy {
                max_cycles_per_round: 2,
                max_failures_per_round: 2,
                inter_batch_delay: Duration::from_millis(1),
                failure_retry_delay: Duration::from_millis(2),
                round_backoff: Duration::from_millis(3),
            },
        );
        assert!(complete);
        assert_eq!(drains, 6, "backlog must continue beyond one finite round");
        assert_eq!(persistence.last(), Some(&(false, 0)));
        assert!(
            waits.iter().filter(|duration| **duration == Duration::from_millis(3)).count() >= 2,
            "each exhausted finite round must yield before rescheduling"
        );
    }

    #[test]
    fn retention_worker_cancellation_preserves_pending_continuation() {
        let mut drains = 0_u64;
        let mut persistence = Vec::new();
        let cancelled = run_youtube_retention_worker_loop(
            || {
                drains = drains.saturating_add(1);
                Ok(retention_receipt(true))
            },
            |pending, failures| {
                persistence.push((pending, failures));
                true
            },
            || false,
            |_| true,
            YoutubeRetentionWorkerPolicy {
                max_cycles_per_round: 16,
                max_failures_per_round: 3,
                inter_batch_delay: Duration::from_millis(1),
                failure_retry_delay: Duration::from_millis(1),
                round_backoff: Duration::from_millis(1),
            },
        );
        assert!(!cancelled);
        assert_eq!(drains, 1, "cancellation must bound work immediately");
        assert_eq!(persistence.last(), Some(&(true, 0)));
        assert!(!persistence.iter().any(|(pending, _)| !pending));
    }

    #[test]
    fn offline_startup_ready_requires_verified_provider_and_redacts_failure() {
        assert_eq!(
            offline_provider_verification_startup_outcome(Ok(())),
            ("ready", None)
        );
        let (phase, error) = offline_provider_verification_startup_outcome(Err(
            "provider verification failed Authorization: Bearer fake-secret token=also-secret"
                .to_string(),
        ));
        let error = error.expect("redacted startup error");
        assert_eq!(phase, "error");
        assert!(!error.contains("fake-secret"));
        assert!(!error.contains("also-secret"));
        assert!(error.contains("provider verification failed"));
    }

    #[test]
    fn youtube_protection_mutation_generation_serializes_overlap_and_rejects_stale_intent() {
        let operation = format!(
            "test-mutation-{}-{}",
            std::process::id(),
            DIAGNOSTICS_ID_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        );
        let final_value = Arc::new(AtomicU64::new(0));
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();

        let old_operation = operation.clone();
        let old_value = Arc::clone(&final_value);
        let old = std::thread::spawn(move || {
            run_youtube_protection_mutation(&old_operation, 1, || {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                old_value.store(1, Ordering::SeqCst);
                Ok(())
            })
        });
        entered_rx.recv().unwrap();

        let new_operation = operation.clone();
        let new_value = Arc::clone(&final_value);
        let newer = std::thread::spawn(move || {
            run_youtube_protection_mutation(&new_operation, 2, || {
                new_value.store(2, Ordering::SeqCst);
                Ok(())
            })
        });
        for _ in 0..500 {
            let registered = YOUTUBE_PROTECTION_MUTATION_GENERATIONS
                .get()
                .and_then(|generations| generations.lock().ok()?.get(&operation).copied())
                == Some(2);
            if registered {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(
            YOUTUBE_PROTECTION_MUTATION_GENERATIONS
                .get()
                .unwrap()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .get(&operation)
                .copied(),
            Some(2),
            "newer intent must register before the older writer is released"
        );
        assert!(run_youtube_protection_mutation(&operation, 1, || Ok(())).is_err());
        release_tx.send(()).unwrap();
        old.join().unwrap().unwrap();
        newer.join().unwrap().unwrap();
        assert_eq!(final_value.load(Ordering::SeqCst), 2);

        // A bounded history reset may continue multiple batches under the same
        // authoritative generation, while a different operation remains independent.
        run_youtube_protection_mutation(&operation, 2, || Ok(())).unwrap();
        let independent = format!("{operation}-other");
        run_youtube_protection_mutation(&independent, 1, || Ok(())).unwrap();
        assert!(run_youtube_protection_mutation(&independent, 0, || Ok(())).is_err());
    }

    #[test]
    fn catalog_mutations_preserve_options_owned_downloader_fields_on_new_default() {
        let mut current = config::DownloadPresetsConfig::default();
        let current_default = current.presets.first_mut().expect("current default");
        current_default.yt_dlp_concurrent_fragments = 7;
        current_default.yt_dlp_limit_rate = Some("7M".to_string());
        current_default.yt_dlp_throttled_rate = Some("77K".to_string());
        current_default.yt_dlp_file_access_retries = 17;
        current_default.yt_dlp_retries = 27;
        current_default.yt_dlp_fragment_retries = 37;
        current_default.yt_dlp_sleep_interval = 47;
        current_default.yt_dlp_sleep_requests = 57;

        let mut next = config::DownloadPresetsConfig::default();
        let mut candidate = next.presets[0].clone();
        candidate.id = "new-default".to_string();
        candidate.yt_dlp_concurrent_fragments = 1;
        candidate.yt_dlp_limit_rate = Some("1M".to_string());
        candidate.yt_dlp_throttled_rate = Some("1K".to_string());
        candidate.yt_dlp_file_access_retries = 1;
        candidate.yt_dlp_retries = 1;
        candidate.yt_dlp_fragment_retries = 1;
        candidate.yt_dlp_sleep_interval = 1;
        candidate.yt_dlp_sleep_requests = 1;
        next.default_preset_id = Some(candidate.id.clone());
        next.presets.push(candidate);

        let protected = preserve_options_owned_downloader_fields(&current, next);
        let selected = protected
            .presets
            .iter()
            .find(|preset| Some(&preset.id) == protected.default_preset_id.as_ref())
            .expect("protected default");
        assert_eq!(selected.yt_dlp_concurrent_fragments, 7);
        assert_eq!(selected.yt_dlp_limit_rate.as_deref(), Some("7M"));
        assert_eq!(selected.yt_dlp_throttled_rate.as_deref(), Some("77K"));
        assert_eq!(selected.yt_dlp_file_access_retries, 17);
        assert_eq!(selected.yt_dlp_retries, 27);
        assert_eq!(selected.yt_dlp_fragment_retries, 37);
        assert_eq!(selected.yt_dlp_sleep_interval, 47);
        assert_eq!(selected.yt_dlp_sleep_requests, 57);
    }

    fn options_safety_fixture() -> config::DownloadPresetsConfig {
        let mut current = config::DownloadPresetsConfig::default();
        let preset = current.presets.first_mut().expect("default preset");
        preset.yt_dlp_concurrent_fragments = 7;
        preset.yt_dlp_limit_rate = Some("7M".to_string());
        preset.yt_dlp_throttled_rate = Some("77K".to_string());
        preset.yt_dlp_file_access_retries = 17;
        preset.yt_dlp_retries = 27;
        preset.yt_dlp_fragment_retries = 37;
        preset.yt_dlp_sleep_interval = 47;
        preset.yt_dlp_sleep_requests = 57;
        current
    }

    fn assert_options_safety_fields(preset: &config::DownloadPreset) {
        assert_eq!(preset.yt_dlp_concurrent_fragments, 7);
        assert_eq!(preset.yt_dlp_limit_rate.as_deref(), Some("7M"));
        assert_eq!(preset.yt_dlp_throttled_rate.as_deref(), Some("77K"));
        assert_eq!(preset.yt_dlp_file_access_retries, 17);
        assert_eq!(preset.yt_dlp_retries, 27);
        assert_eq!(preset.yt_dlp_fragment_retries, 37);
        assert_eq!(preset.yt_dlp_sleep_interval, 47);
        assert_eq!(preset.yt_dlp_sleep_requests, 57);
    }

    #[test]
    fn downloader_safety_survives_invalid_default_id() {
        let current = options_safety_fixture();
        let mut next = config::DownloadPresetsConfig::default();
        next.default_preset_id = Some("missing-default".to_string());
        let protected = preserve_options_owned_downloader_fields(&current, next);
        let selected = protected
            .default_preset_id
            .as_ref()
            .and_then(|id| protected.presets.iter().find(|preset| &preset.id == id))
            .expect("valid repaired default");
        assert_options_safety_fields(selected);
    }

    #[test]
    fn downloader_safety_survives_empty_catalog_and_final_preset_deletion() {
        let current = options_safety_fixture();
        let protected = preserve_options_owned_downloader_fields(
            &current,
            config::DownloadPresetsConfig {
                default_preset_id: None,
                presets: Vec::new(),
            },
        );
        assert_eq!(
            protected.presets.len(),
            1,
            "deleting the final preset must recreate a usable catalog"
        );
        assert_options_safety_fields(&protected.presets[0]);
    }

    #[test]
    fn downloader_safety_survives_json_import_shape() {
        let current = options_safety_fixture();
        let imported: config::DownloadPresetsConfig = serde_json::from_value(serde_json::json!({
            "default_preset_id": "imported",
            "presets": [{
                "id": "imported",
                "title": "Imported",
                "path_template": "{channel}",
                "filename_template": "{title}_{id}",
                "format_preference": null,
                "quality_preference": "best",
                "subtitle_mode": "auto"
            }]
        }))
        .expect("legacy import payload");
        let protected = preserve_options_owned_downloader_fields(&current, imported);
        assert_options_safety_fields(&protected.presets[0]);
    }

    #[test]
    fn diagnostics_trace_rotation_is_size_and_count_bounded() {
        let dir = tempfile::tempdir().expect("tempdir");
        let trace = dir.path().join("diagnostics_trace.jsonl");
        std::fs::write(&trace, b"current").expect("write trace");
        std::fs::write(trace.with_file_name("diagnostics_trace.1.jsonl"), b"old1").unwrap();
        std::fs::write(trace.with_file_name("diagnostics_trace.2.jsonl"), b"old2").unwrap();

        rotate_diagnostics_trace_if_needed(&trace, 4, 1).expect("rotate");

        assert!(!trace.exists());
        let mut contents = trace_generation_files(&trace, "zip")
            .unwrap()
            .into_iter()
            .map(|path| {
                let file = std::fs::File::open(path).unwrap();
                let mut zip = zip::ZipArchive::new(file).unwrap();
                let mut bytes = Vec::new();
                std::io::Read::read_to_end(&mut zip.by_index(0).unwrap(), &mut bytes).unwrap();
                bytes
            })
            .collect::<Vec<_>>();
        contents.sort();
        assert_eq!(contents, vec![b"current".to_vec(), b"old1".to_vec(), b"old2".to_vec()]);
        assert!(dir.path().join("diagnostics_trace.aggregate.json").exists());
    }

    #[test]
    fn armed_panel_capture_writes_correlated_incident_artifacts() {
        let _capture_test_guard = DIAGNOSTICS_CAPTURE_TEST_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        paths.ensure_dirs().expect("ensure dirs");
        let armed = DiagnosticsCaptureStatus {
            mode: "normal".to_string(),
            armed_trigger: Some("panel_switch".to_string()),
            incident_id: Some("incident-test".to_string()),
            armed_at_ms: Some(now_epoch_ms_i64()),
            started_at_ms: None,
            expires_at_ms: Some(now_epoch_ms_i64() + DIAGNOSTICS_INCIDENT_DURATION_MS),
            max_trace_bytes: DIAGNOSTICS_TRACE_INCIDENT_MAX_BYTES,
            trace_bytes: 0,
            dropped_events: 0,
            artifact_dir: None,
            root_span_id: None,
        };
        persist_diagnostics_capture_state(&paths, &armed).expect("persist armed state");
        *diagnostics_capture_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = armed;

        append_diagnostics_trace_row(
            &paths,
            "panel_switch".to_string(),
            serde_json::json!({ "transition_id": 7, "page": "jobs" }),
            "info".to_string(),
        )
        .expect("append panel trace");

        let rows = read_recent_diagnostics_trace_entries(&paths, 10).expect("read trace");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].incident_id.as_deref(), Some("incident-test"));
        assert_eq!(rows[0].span_id.as_deref(), Some("panel-7"));
        let status = load_diagnostics_capture_state(&paths);
        assert_eq!(status.mode, "incident");
        assert!(status.armed_trigger.is_none());
        let artifact_dir = std::path::PathBuf::from(status.artifact_dir.expect("artifact dir"));
        assert!(artifact_dir.join("trace.jsonl").is_file());
        assert!(artifact_dir.join("manifest.json").is_file());
        *diagnostics_capture_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = DiagnosticsCaptureStatus::default();
    }

    #[test]
    fn panel_activation_precedes_first_destination_command_and_preserves_parent_span() {
        let _capture_test_guard = DIAGNOSTICS_CAPTURE_TEST_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        paths.ensure_dirs().expect("ensure dirs");
        let armed = DiagnosticsCaptureStatus {
            mode: "normal".to_string(),
            armed_trigger: Some("panel_switch".to_string()),
            incident_id: Some("incident-race".to_string()),
            armed_at_ms: Some(now_epoch_ms_i64()),
            expires_at_ms: Some(now_epoch_ms_i64() + DIAGNOSTICS_INCIDENT_DURATION_MS),
            ..DiagnosticsCaptureStatus::default()
        };
        persist_diagnostics_capture_state(&paths, &armed).expect("persist armed state");
        *diagnostics_capture_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = armed;

        let activation = activate_panel_capture_before_navigation(
            &paths,
            "jobs",
            41,
            "panel-41",
            Some("operator-click-7"),
        )
        .expect("activate panel capture");
        assert_eq!(activation.incident_id.as_deref(), Some("incident-race"));
        assert_eq!(activation.panel_span_id, "panel-41");
        assert_eq!(activation.parent_span_id.as_deref(), Some("operator-click-7"));
        assert_eq!(activation.capture_mode, "incident");
        assert!(activation.activated_armed_capture);
        assert_eq!(load_diagnostics_capture_state(&paths).mode, "incident");

        // Counterexample order: a newly mounted page command arrives before the asynchronous
        // frontend panel row drains. Activation must already make it a child of the panel span.
        append_diagnostics_trace_row(
            &paths,
            "command_phase".to_string(),
            serde_json::json!({
                "cmd": "jobs_overview",
                "phase": "db_open",
                "request_id": "jobs-request-1",
                "span_id": "jobs-span-1",
            }),
            "info".to_string(),
        )
        .expect("first destination command trace");
        let rows = read_recent_diagnostics_trace_entries(&paths, 5).expect("read trace");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].incident_id.as_deref(), Some("incident-race"));
        assert_eq!(rows[0].span_id.as_deref(), Some("jobs-span-1"));
        assert_eq!(rows[0].details["parent_span_id"], "panel-41");
        *diagnostics_capture_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = DiagnosticsCaptureStatus::default();
    }

    #[test]
    fn superseded_panel_activation_rearms_before_the_next_transition_claims_root() {
        let _capture_test_guard = DIAGNOSTICS_CAPTURE_TEST_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        paths.ensure_dirs().expect("ensure dirs");
        let armed = DiagnosticsCaptureStatus {
            mode: "normal".to_string(),
            armed_trigger: Some("panel_switch".to_string()),
            incident_id: Some("incident-supersession".to_string()),
            armed_at_ms: Some(now_epoch_ms_i64()),
            expires_at_ms: Some(now_epoch_ms_i64() + DIAGNOSTICS_INCIDENT_DURATION_MS),
            max_trace_bytes: DIAGNOSTICS_TRACE_INCIDENT_MAX_BYTES,
            ..DiagnosticsCaptureStatus::default()
        };
        persist_diagnostics_capture_state(&paths, &armed).expect("persist armed state");
        *diagnostics_capture_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = armed;

        let first = activate_panel_capture_before_navigation(
            &paths,
            "jobs",
            51,
            "panel-51",
            Some("operator-click-a"),
        )
        .expect("activate superseded panel");
        assert!(first.activated_armed_capture);
        assert!(cancel_superseded_panel_capture(
            &paths,
            first.incident_id.as_deref().expect("incident"),
            &first.panel_span_id,
        )
        .expect("cancel superseded activation"));
        let rearmed = load_diagnostics_capture_state(&paths);
        assert_eq!(rearmed.mode, "normal");
        assert_eq!(rearmed.armed_trigger.as_deref(), Some("panel_switch"));
        assert!(rearmed.root_span_id.is_none());

        let second = activate_panel_capture_before_navigation(
            &paths,
            "media_library",
            52,
            "panel-52",
            Some("operator-click-b"),
        )
        .expect("activate winning panel");
        assert_eq!(second.incident_id.as_deref(), Some("incident-supersession"));
        assert!(second.activated_armed_capture);
        let active = load_diagnostics_capture_state(&paths);
        assert_eq!(active.mode, "incident");
        assert_eq!(active.root_span_id.as_deref(), Some("panel-52"));
        assert!(!cancel_superseded_panel_capture(
            &paths,
            "incident-supersession",
            "panel-51",
        )
        .expect("stale cancel must be refused"));
        assert_eq!(
            load_diagnostics_capture_state(&paths).root_span_id.as_deref(),
            Some("panel-52")
        );
        *diagnostics_capture_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = DiagnosticsCaptureStatus::default();
    }

    #[test]
    fn active_or_armed_capture_pins_trace_folder_mutation() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        paths.ensure_dirs().expect("ensure dirs");
        for (mode, trigger) in [
            ("normal", Some("job_start".to_string())),
            ("incident", None),
        ] {
            let status = DiagnosticsCaptureStatus {
                mode: mode.to_string(),
                armed_trigger: trigger,
                incident_id: Some("pin-test".to_string()),
                expires_at_ms: Some(now_epoch_ms_i64() + 60_000),
                ..DiagnosticsCaptureStatus::default()
            };
            persist_diagnostics_capture_state(&paths, &status).expect("persist capture state");
            assert!(ensure_diagnostics_trace_mutation_allowed(&paths).is_err());
        }
        let normal = DiagnosticsCaptureStatus::default();
        persist_diagnostics_capture_state(&paths, &normal).expect("persist normal state");
        assert!(ensure_diagnostics_trace_mutation_allowed(&paths).is_ok());
    }

    #[test]
    fn oversized_rows_are_replaced_and_tail_reads_stay_bounded() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        paths.ensure_dirs().expect("ensure dirs");
        persist_diagnostics_capture_state(&paths, &DiagnosticsCaptureStatus::default())
            .expect("normal capture state");
        *diagnostics_capture_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = DiagnosticsCaptureStatus::default();
        append_diagnostics_trace_row(
            &paths,
            "oversized".to_string(),
            serde_json::json!({ "payload": "x".repeat(DIAGNOSTICS_TRACE_MAX_ROW_BYTES * 2) }),
            "info".to_string(),
        )
        .expect("append bounded replacement");
        let trace = diagnostics_trace_file_path(&paths).expect("trace path");
        assert!(std::fs::metadata(trace).expect("trace metadata").len() < 8 * 1024);
        let rows = read_recent_diagnostics_trace_entries(&paths, 5).expect("tail rows");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].event, "diagnostics_event_truncated");
    }

    #[test]
    fn diagnostics_sink_recursively_redacts_secrets_paths_and_urls() {
        let value = redact_diagnostics_value(serde_json::json!({
            "nested": { "authorization": "Bearer abc", "error": "--password hunter2 --cookies C:\\private\\cookies.txt --proxy http://user:pass@proxy.invalid --api-key=abcdef --secret hidden --key keyvalue https://alice:pw@example.invalid/a" },
            "media_path": "Z:\\private\\video.mkv", "source_url": "https://user:pass@example.invalid/x"
        }));
        let text = value.to_string();
        assert!(!text.contains("hunter2"));
        assert!(!text.contains("Bearer abc"));
        assert!(!text.contains("Z:\\\\private"));
        assert!(!text.contains("user:pass"));
        for secret in ["cookies.txt", "abcdef", "hidden", "keyvalue", "alice:pw"] {
            assert!(!text.contains(secret), "leaked {secret}");
        }
        assert!(text.contains("<redacted>"));
    }

    #[test]
    fn diagnostics_sink_persists_redacted_bare_and_quoted_key_value_text() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        paths.ensure_dirs().expect("ensure dirs");
        let normal = DiagnosticsCaptureStatus::default();
        persist_diagnostics_capture_state(&paths, &normal).expect("persist normal state");
        *diagnostics_capture_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = normal;

        append_diagnostics_trace_row(
            &paths,
            "redaction_sink_probe".to_string(),
            serde_json::json!({
                "message": "password=alpha token = \"bravo two\" apikey='charlie three' api_key delta secret = \"echo five\" key=\"foxtrot six\" \"password\" = \"golf seven\" --token 'hotel eight'",
                "nested": { "api_key": "india-nine" }
            }),
            "warn".to_string(),
        )
        .expect("persist redacted sink row");

        let trace_path = diagnostics_trace_file_path(&paths).expect("trace path");
        let persisted = std::fs::read_to_string(trace_path).expect("read persisted trace");
        for secret in [
            "alpha",
            "bravo two",
            "charlie three",
            "delta",
            "echo five",
            "foxtrot six",
            "golf seven",
            "hotel eight",
            "india-nine",
        ] {
            assert!(!persisted.contains(secret), "sink leaked {secret}");
        }
        assert!(persisted.contains("<redacted>"));
    }

    #[test]
    fn diagnostics_sink_persists_redacted_spaced_and_combined_colon_forms() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        paths.ensure_dirs().expect("ensure dirs");
        let normal = DiagnosticsCaptureStatus::default();
        persist_diagnostics_capture_state(&paths, &normal).expect("persist normal state");
        *diagnostics_capture_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = normal;

        append_diagnostics_trace_row(
            &paths,
            "colon_redaction_sink_probe".to_string(),
            serde_json::json!({
                "message": "password : \"colon alpha\" token:colon-bravo api_key : 'colon charlie' apikey:colon-delta secret : colon-echo action:read"
            }),
            "warn".to_string(),
        )
        .expect("persist colon-redacted sink row");

        let trace_path = diagnostics_trace_file_path(&paths).expect("trace path");
        let persisted = std::fs::read_to_string(trace_path).expect("read persisted trace");
        for secret in [
            "colon alpha",
            "colon-bravo",
            "colon charlie",
            "colon-delta",
            "colon-echo",
        ] {
            assert!(!persisted.contains(secret), "sink leaked {secret}");
        }
        assert!(persisted.contains("action:read"));
        assert!(persisted.contains("<redacted>"));
    }

    #[test]
    fn diagnostics_sink_persists_complete_authorization_redaction() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        paths.ensure_dirs().expect("ensure dirs");
        let normal = DiagnosticsCaptureStatus::default();
        persist_diagnostics_capture_state(&paths, &normal).expect("persist normal state");
        *diagnostics_capture_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = normal;

        append_diagnostics_trace_row(
            &paths,
            "authorization_redaction_sink_probe".to_string(),
            serde_json::json!({
                "messages": [
                    "Authorization: Bearer bearer-alpha action:read",
                    "authorization : basic basic-bravo action:read",
                    "AUTHORIZATION:Basic compact-charlie action:read",
                    "Authorization:Bearer adjacent-delta action:read",
                    "Authorization : bEaReR : malformed-echo action:read",
                    "Authorization :Bearer attached-foxtrot action:read",
                    "Authorization =Basic equals-golf action:read",
                    "Authorization:",
                    "authorization :",
                    "AUTHORIZATION:Bearer"
                ]
            }),
            "warn".to_string(),
        )
        .expect("persist authorization-redacted sink row");

        let trace_path = diagnostics_trace_file_path(&paths).expect("trace path");
        let persisted = std::fs::read_to_string(trace_path).expect("read persisted trace");
        for secret in [
            "bearer-alpha",
            "basic-bravo",
            "compact-charlie",
            "adjacent-delta",
            "malformed-echo",
            "attached-foxtrot",
            "equals-golf",
        ] {
            assert!(!persisted.contains(secret), "sink leaked {secret}");
        }
        assert!(!persisted.to_ascii_lowercase().contains("bearer"));
        assert!(!persisted.to_ascii_lowercase().contains("basic"));
        assert_eq!(persisted.matches("action:read").count(), 7);
        assert!(persisted.contains("<redacted>"));
    }

    #[test]
    fn diagnostics_sink_matches_shared_proxy_and_quoted_header_redaction_vectors() {
        #[derive(serde::Deserialize)]
        struct Vector {
            input: String,
            secrets: Vec<String>,
            preserve: Vec<String>,
        }
        let vectors: Vec<Vector> = serde_json::from_str(include_str!(
            "../../../diagnostics/redaction_adversarial_vectors.json"
        ))
        .expect("shared redaction vectors");
        for vector in vectors {
            let redacted = redact_diagnostics_value(serde_json::json!({ "message": vector.input }));
            let text = redacted.to_string();
            for secret in vector.secrets {
                assert!(!text.to_ascii_lowercase().contains(&secret.to_ascii_lowercase()), "secret leaked: {text}");
            }
            for context in vector.preserve {
                assert!(text.to_ascii_lowercase().contains(&context.to_ascii_lowercase()), "context lost: {text}");
            }
            assert!(text.contains("<redacted>"));
        }
    }

    #[test]
    fn accepted_async_write_failure_is_counted_and_emitted_after_recovery() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        paths.ensure_dirs().expect("ensure dirs");
        let normal = DiagnosticsCaptureStatus::default();
        persist_diagnostics_capture_state(&paths, &normal).expect("persist normal state");
        *diagnostics_capture_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = normal;
        let trace_dir = paths
            .effective_diagnostics_trace_dir()
            .expect("effective trace dir");
        if trace_dir.exists() {
            std::fs::remove_dir_all(&trace_dir).expect("remove temporary trace dir");
        }
        std::fs::write(&trace_dir, b"failure injection").expect("block trace directory");
        let failure_baseline = DIAGNOSTICS_TRACE_ASYNC_WRITE_FAILURES_TOTAL.load(Ordering::Acquire);

        let accepted = append_diagnostics_trace_row_best_effort(
            &paths,
            "accepted_before_disk_failure",
            serde_json::json!({ "probe": true }),
            "info",
        );
        assert!(
            accepted.accepted,
            "failure must occur after accepted enqueue"
        );
        for _ in 0..200 {
            if DIAGNOSTICS_TRACE_ASYNC_WRITE_FAILURES_TOTAL.load(Ordering::Acquire)
                > failure_baseline
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            DIAGNOSTICS_TRACE_ASYNC_WRITE_FAILURES_TOTAL.load(Ordering::Acquire) > failure_baseline,
            "writer failure was not counted"
        );

        std::fs::remove_file(&trace_dir).expect("remove failure injection file");
        std::fs::create_dir_all(&trace_dir).expect("recover trace directory");
        let recovery_receipt = append_diagnostics_trace_row_best_effort(
            &paths,
            "write_after_disk_recovery",
            serde_json::json!({ "probe": true }),
            "info",
        );
        assert!(recovery_receipt.accepted);
        assert!(recovery_receipt.dropped_events_total > accepted.dropped_events_total);
        assert!(recovery_receipt.async_write_failures_total > failure_baseline);
        assert!(recovery_receipt.pending_loss_events > 0);

        let rows = (0..200)
            .find_map(|_| {
                let rows = read_recent_diagnostics_trace_entries(&paths, 20).ok()?;
                let loss_visible = rows.iter().any(|row| {
                    row.event == "diagnostics_events_dropped"
                        && row
                            .details
                            .get("async_write_failures_total")
                            .and_then(|value| value.as_u64())
                            .is_some_and(|total| total > failure_baseline)
                });
                let recovery_visible = rows
                    .iter()
                    .any(|row| row.event == "write_after_disk_recovery");
                if loss_visible && recovery_visible {
                    Some(rows)
                } else {
                    std::thread::sleep(Duration::from_millis(10));
                    None
                }
            })
            .expect("bounded loss event and recovered write");
        assert_eq!(
            rows.iter()
                .filter(|row| row.event == "diagnostics_events_dropped")
                .count(),
            1,
            "recovery must coalesce pending loss into one bounded event"
        );
    }

    #[test]
    fn concurrent_same_request_phases_keep_distinct_matching_invocation_ids() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        paths.ensure_dirs().expect("ensure dirs");
        let normal = DiagnosticsCaptureStatus::default();
        persist_diagnostics_capture_state(&paths, &normal).expect("persist normal state");
        *diagnostics_capture_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = normal;
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));

        let handles: Vec<_> = (0..2)
            .map(|_| {
                let paths = paths.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    let timer = InvokeTimer::start_with_context(
                        paths,
                        "concurrent_phase_probe",
                        Some("shared-request".to_string()),
                        Some("shared-span".to_string()),
                    );
                    let invocation_id = timer.invocation_id;
                    timer.phase("db_storage_observation", Duration::from_millis(1));
                    drop(timer);
                    invocation_id
                })
            })
            .collect();
        let invocation_ids: Vec<u64> = handles
            .into_iter()
            .map(|handle| handle.join().expect("phase thread"))
            .collect();
        assert_ne!(invocation_ids[0], invocation_ids[1]);

        let rows = (0..200)
            .find_map(|_| {
                let rows = read_recent_diagnostics_trace_entries(&paths, 20).ok()?;
                let matched = rows
                    .iter()
                    .filter(|row| {
                        row.details.get("cmd").and_then(|value| value.as_str())
                            == Some("concurrent_phase_probe")
                    })
                    .count();
                if matched >= 6 {
                    Some(rows)
                } else {
                    std::thread::sleep(Duration::from_millis(10));
                    None
                }
            })
            .expect("queued start, phase, and completion rows");
        for invocation_id in invocation_ids {
            let correlated: Vec<_> = rows
                .iter()
                .filter(|row| {
                    row.details
                        .get("invocation_id")
                        .and_then(|value| value.as_u64())
                        == Some(invocation_id)
                })
                .collect();
            assert_eq!(correlated.len(), 3, "one start/phase/completion chain");
            for event in ["command_started", "command_phase", "command_completed"] {
                assert!(
                    correlated.iter().any(|row| row.event == event),
                    "missing {event}"
                );
            }
            for row in correlated {
                assert_eq!(
                    row.details
                        .get("request_id")
                        .and_then(|value| value.as_str()),
                    Some("shared-request")
                );
                assert_eq!(
                    row.details.get("span_id").and_then(|value| value.as_str()),
                    Some("shared-span")
                );
            }
        }
    }

    #[test]
    fn interrupted_rotation_is_reconciled_and_tail_spans_rotated_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        paths.ensure_dirs().expect("dirs");
        let trace = diagnostics_trace_file_path(&paths).expect("trace");
        let old = serde_json::to_string(&DiagnosticsTraceEntry {
            ts_ms: 1,
            event: "old".into(),
            level: "info".into(),
            details: serde_json::json!({}),
            process: None,
            incident_id: None,
            span_id: None,
        })
        .unwrap();
        let pending = serde_json::to_string(&DiagnosticsTraceEntry {
            ts_ms: 2,
            event: "pending".into(),
            level: "info".into(),
            details: serde_json::json!({}),
            process: None,
            incident_id: None,
            span_id: None,
        })
        .unwrap();
        let current = serde_json::to_string(&DiagnosticsTraceEntry {
            ts_ms: 3,
            event: "current".into(),
            level: "info".into(),
            details: serde_json::json!({}),
            process: None,
            incident_id: None,
            span_id: None,
        })
        .unwrap();
        std::fs::write(
            trace.with_file_name("diagnostics_trace.1.jsonl"),
            format!("{old}\n"),
        )
        .unwrap();
        std::fs::write(
            trace.with_file_name("diagnostics_trace.rotation_pending.jsonl"),
            format!("{pending}\n"),
        )
        .unwrap();
        std::fs::write(&trace, format!("{current}\n")).unwrap();
        let rows = read_recent_diagnostics_trace_entries(&paths, 10).expect("tail");
        assert_eq!(
            rows.iter().map(|r| r.event.as_str()).collect::<Vec<_>>(),
            vec!["old", "pending", "current"]
        );
        assert!(!trace
            .with_file_name("diagnostics_trace.rotation_pending.jsonl")
            .exists());
        let mut archived = trace_generation_files(&trace, "zip")
            .unwrap()
            .into_iter()
            .map(|path| {
                let file = std::fs::File::open(path).unwrap();
                let mut zip = zip::ZipArchive::new(file).unwrap();
                let mut text = String::new();
                std::io::Read::read_to_string(&mut zip.by_index(0).unwrap(), &mut text).unwrap();
                text
            })
            .collect::<Vec<_>>();
        archived.sort();
        assert_eq!(archived, vec![format!("{old}\n"), format!("{pending}\n")]);
    }

    #[test]
    fn immutable_rotation_reconciliation_is_idempotent_at_every_persistence_boundary() {
        for boundary in ["prepared", "captured", "aggregated", "compressed", "orphan"] {
            let dir = tempfile::tempdir().expect("tempdir");
            let trace = dir.path().join("diagnostics_trace.jsonl");
            let generation_id = format!("fixture-{boundary}");
            let source = trace_generation_path(&trace, &generation_id, "jsonl");
            let zip = trace_generation_path(&trace, &generation_id, "zip");
            let row = serde_json::to_string(&DiagnosticsTraceEntry {
                ts_ms: 1,
                event: format!("boundary_{boundary}"),
                level: "info".into(),
                details: serde_json::json!({}),
                process: None,
                incident_id: None,
                span_id: None,
            })
            .unwrap();
            if boundary == "prepared" {
                std::fs::write(&trace, format!("{row}\n")).unwrap();
            } else {
                std::fs::write(&source, format!("{row}\n")).unwrap();
            }
            if boundary != "orphan" {
                write_json_atomic(
                    &trace.with_file_name("diagnostics_trace.rotation_journal.json"),
                    &serde_json::json!({"schema_version":1,"generation_id":generation_id,"stage":boundary}),
                )
                .unwrap();
            }
            if matches!(boundary, "aggregated" | "compressed") {
                merge_rotated_trace_aggregate(&trace, &source, &generation_id).unwrap();
            }
            if boundary == "compressed" {
                compress_trace_jsonl(&source, &zip).unwrap();
            }

            reconcile_diagnostics_trace_rotation(&trace).expect("first recovery");
            reconcile_diagnostics_trace_rotation(&trace).expect("idempotent recovery");
            assert!(!trace.with_file_name("diagnostics_trace.rotation_journal.json").exists());
            assert!(!source.exists());
            assert!(zip.exists());
            let aggregate: serde_json::Value = serde_json::from_str(
                &std::fs::read_to_string(trace.with_file_name("diagnostics_trace.aggregate.json")).unwrap(),
            )
            .unwrap();
            assert_eq!(aggregate["rows_total"].as_u64(), Some(1), "boundary {boundary}");
            assert_eq!(aggregate["merged_generations"].as_array().unwrap().len(), 1);
        }
    }

    #[test]
    fn rotation_reconciliation_enforces_one_global_generation_bound_across_legacy_and_immutable() {
        let dir = tempfile::tempdir().expect("tempdir");
        let trace = dir.path().join("diagnostics_trace.jsonl");
        for index in 1..=4 {
            let source = dir.path().join(format!("legacy-source-{index}.jsonl"));
            std::fs::write(&source, format!("{{\"event\":\"legacy-{index}\"}}\n")).unwrap();
            compress_trace_jsonl(
                &source,
                &trace.with_file_name(format!("diagnostics_trace.{index}.zip")),
            )
            .unwrap();
        }
        for index in 1..=3 {
            let source = trace_generation_path(&trace, &format!("immutable-{index}"), "jsonl");
            std::fs::write(&source, format!("{{\"event\":\"immutable-{index}\"}}\n")).unwrap();
            compress_trace_jsonl(
                &source,
                &trace_generation_path(&trace, &format!("immutable-{index}"), "zip"),
            )
            .unwrap();
        }

        reconcile_diagnostics_trace_rotation(&trace).expect("reconcile mixed generations");

        assert_eq!(trace_generation_files(&trace, "zip").unwrap().len(), DIAGNOSTICS_TRACE_RETAINED_FILES);
        assert_eq!(count_compressed_trace_files(&trace), DIAGNOSTICS_TRACE_RETAINED_FILES);
        for index in 1..=DIAGNOSTICS_TRACE_RETAINED_FILES {
            assert!(!trace.with_file_name(format!("diagnostics_trace.{index}.zip")).exists());
        }
    }

    #[test]
    fn diagnostics_trace_rotates_by_age_and_exposes_compaction_state() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        paths.ensure_dirs().expect("dirs");
        let trace = diagnostics_trace_file_path(&paths).expect("trace");
        std::fs::write(&trace, b"{\"event\":\"aged\"}\n").unwrap();
        let state_path = trace.with_file_name("diagnostics_trace.rotation_state.json");
        write_json_atomic(
            &state_path,
            &serde_json::json!({
                "current_started_at_ms": now_epoch_ms_i64() - DIAGNOSTICS_TRACE_RETAINED_AGE_MS - 1,
                "last_rotation_at_ms": null,
                "rotation_count": 0,
                "compressed_files": 0,
                "last_rotation_reason": null
            }),
        )
        .unwrap();
        rotate_diagnostics_trace_if_needed(&trace, u64::MAX, 1).expect("age rotation");
        assert_eq!(trace_generation_files(&trace, "zip").unwrap().len(), 1);
        let status = build_diagnostics_trace_dir_status(&paths).expect("status");
        assert_eq!(status.retained_age_ms, DIAGNOSTICS_TRACE_RETAINED_AGE_MS);
        assert_eq!(status.rotation_count, 1);
        assert_eq!(status.sampling_mode, "bounded_queue_no_sampling");
        assert_eq!(status.queue_capacity, DIAGNOSTICS_TRACE_QUEUE_CAPACITY);
    }

    #[test]
    fn diagnostics_trace_does_not_rewrite_rotation_state_for_each_row() {
        let dir = tempfile::tempdir().expect("tempdir");
        let trace = dir.path().join("diagnostics_trace.jsonl");
        std::fs::write(&trace, b"{\"event\":\"current\"}\n").unwrap();
        let state_path = trace.with_file_name("diagnostics_trace.rotation_state.json");
        let persisted = serde_json::json!({
            "current_started_at_ms": now_epoch_ms_i64(),
            "last_rotation_at_ms": null,
            "rotation_count": 17,
            "compressed_files": 2,
            "last_rotation_reason": "sentinel"
        });
        write_json_atomic(&state_path, &persisted).unwrap();
        let before = std::fs::read(&state_path).unwrap();
        rotate_diagnostics_trace_if_needed(&trace, u64::MAX, 1).expect("no rotation");
        assert_eq!(
            std::fs::read(&state_path).unwrap(),
            before,
            "normal appends must not add an atomic state-file rewrite"
        );
    }

    #[test]
    fn incident_retention_is_globally_count_bounded_and_keeps_active() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        let incidents = paths
            .effective_diagnostics_trace_dir()
            .unwrap()
            .join("incidents");
        for i in 0..20 {
            std::fs::create_dir_all(incidents.join(format!("incident-{i:02}"))).unwrap();
        }
        prune_diagnostics_incidents(&paths, Some("incident-00")).expect("prune");
        assert!(incidents.join("incident-00").exists());
        assert!(
            std::fs::read_dir(incidents).unwrap().count() <= DIAGNOSTICS_INCIDENT_RETAINED_COUNT
        );
    }

    #[test]
    fn jobs_batch_operation_receipts_are_age_and_count_bounded() {
        let now_ms = 10_000_000_i64;
        let mut operations = std::collections::HashMap::new();
        operations.insert(
            "running".to_string(),
            JobsBatchOperationSnapshot {
                request_id: "running".to_string(),
                mode: "retry".to_string(),
                batch_query: "batch-running".to_string(),
                state: "running".to_string(),
                started_at_ms: 1,
                finished_at_ms: None,
                summary: None,
                error: None,
            },
        );
        operations.insert(
            "expired".to_string(),
            JobsBatchOperationSnapshot {
                request_id: "expired".to_string(),
                mode: "dry_run".to_string(),
                batch_query: "batch-expired".to_string(),
                state: "succeeded".to_string(),
                started_at_ms: 1,
                finished_at_ms: Some(now_ms - (60 * 60 * 1000) - 1),
                summary: None,
                error: None,
            },
        );
        for index in 0..140 {
            let request_id = format!("completed-{index:03}");
            operations.insert(
                request_id.clone(),
                JobsBatchOperationSnapshot {
                    request_id,
                    mode: "repair".to_string(),
                    batch_query: format!("batch-{index:03}"),
                    state: "succeeded".to_string(),
                    started_at_ms: now_ms - 1_000 + index,
                    finished_at_ms: Some(now_ms - 1_000 + index),
                    summary: None,
                    error: None,
                },
            );
        }

        prune_jobs_batch_operations(&mut operations, now_ms);

        assert!(operations.contains_key("running"));
        assert!(!operations.contains_key("expired"));
        assert!(operations.len() < 128);
    }

    #[test]
    fn verify_offline_payload_integrity_accepts_matching_bytes_and_hash() {
        let dir = tempfile::tempdir().expect("tempdir");
        let payload = dir.path().join("payload.zip");
        std::fs::write(&payload, b"payload-bytes").expect("write");

        let manifest = OfflineBundleManifest {
            schema_version: 1,
            bundle_id: "bundle".to_string(),
            payload_zip: Some("payload.zip".to_string()),
            payload_bytes: Some(13),
            payload_sha256: Some(
                "808B59664B6ADB9274E3BBD0766E7AEC9659786C22FDB825C49CA7FDA1C6236E".to_string(),
            ),
            payload_sha256_algorithm: Some("sha256".to_string()),
        };

        verify_offline_payload_integrity(&manifest, &payload).expect("verify");
    }

    #[test]
    fn verify_offline_payload_integrity_rejects_hash_mismatch() {
        let dir = tempfile::tempdir().expect("tempdir");
        let payload = dir.path().join("payload.zip");
        std::fs::write(&payload, b"payload-bytes").expect("write");

        let manifest = OfflineBundleManifest {
            schema_version: 1,
            bundle_id: "bundle".to_string(),
            payload_zip: Some("payload.zip".to_string()),
            payload_bytes: Some(13),
            payload_sha256: Some("DEADBEEF".to_string()),
            payload_sha256_algorithm: Some("sha256".to_string()),
        };

        let err = verify_offline_payload_integrity(&manifest, &payload).expect_err("mismatch");
        assert!(err.contains("sha256 mismatch"));
    }

    #[cfg(windows)]
    #[test]
    fn youtube_sign_in_browser_candidates_are_source_specific() {
        let firefox = youtube_browser_windows_candidates("firefox");
        let chrome = youtube_browser_windows_candidates("chrome");
        let edge = youtube_browser_windows_candidates("edge");
        let opera = youtube_browser_windows_candidates("opera");

        assert!(firefox.iter().any(|path| path.ends_with("firefox.exe")));
        assert!(chrome.iter().any(|path| path.ends_with("chrome.exe")));
        assert!(edge.iter().any(|path| path.ends_with("msedge.exe")));
        assert!(opera.iter().any(|path| path.ends_with("opera.exe")));
        assert!(youtube_browser_windows_candidates("unsupported").is_empty());
    }

    #[test]
    fn offline_bundle_runtime_ready_requires_localization_voice_runtime() {
        let ready = OfflineBundleRuntimeReadyFlags {
            ffmpeg_installed: true,
            ytdlp_available: true,
            js_runtime_available: true,
            portable_python_installed: true,
            venv_ready: true,
            diarization_installed: true,
            neural_tts_installed: true,
            voice_preserving_installed: true,
        };

        assert!(offline_bundle_runtime_ready_from_flags(ready));

        let mut missing_voice = ready;
        missing_voice.voice_preserving_installed = false;
        assert!(!offline_bundle_runtime_ready_from_flags(missing_voice));

        let mut missing_diarization = ready;
        missing_diarization.diarization_installed = false;
        assert!(!offline_bundle_runtime_ready_from_flags(
            missing_diarization
        ));

        let mut missing_python = ready;
        missing_python.venv_ready = false;
        assert!(!offline_bundle_runtime_ready_from_flags(missing_python));
    }

    #[test]
    fn phase2_latest_state_marks_interrupted_steps_when_job_failed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        let job = jobs::enqueue_dummy_sleep(&paths, 1).expect("enqueue");
        let finished_at_ms = now_epoch_ms_i64();
        let conn = db::open(&paths).expect("db");
        db::migrate(&conn).expect("migrate");
        conn.execute(
            "UPDATE job SET status='failed', error='interrupted by app shutdown', finished_at_ms=?1 WHERE id=?2",
            (finished_at_ms, &job.id),
        )
        .expect("mark failed");

        let state = serde_json::json!({
            "schema_version": 1,
            "job_id": job.id,
            "steps": [
                { "id": "portable_python_win64", "status": "done", "error": null },
                { "id": "python_toolchain", "status": "running", "error": null },
                { "id": "tts_voice_preserving_local_v1", "status": "queued", "error": null }
            ]
        });

        let (normalized, active, stale, job_status) = normalize_phase2_latest_state(&paths, state);
        assert!(!active);
        assert!(stale);
        assert_eq!(job_status.as_deref(), Some("failed"));

        let statuses = normalized
            .get("steps")
            .and_then(|value| value.as_array())
            .expect("steps")
            .iter()
            .map(|step| {
                step.get("status")
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
            })
            .collect::<Vec<_>>();
        assert_eq!(statuses, vec!["done", "interrupted", "interrupted"]);
    }

    #[test]
    fn artifact_info_serializes_runtime_contract_in_snake_case() {
        let artifact = ArtifactInfo {
            id: "artifact-1".to_string(),
            title: "Dub mux".to_string(),
            path: "D:\\tmp\\mux.mp4".to_string(),
            exists: true,
            group: "dub".to_string(),
            kind: ArtifactKind::DubMux,
            job_type: Some("mux_dub_preview_v1".to_string()),
            variant_label: Some("Take B".to_string()),
            track_id: Some("track-en".to_string()),
            mux_container: Some("mp4".to_string()),
            tts_backend_id: Some("openvoice_v2".to_string()),
            voice_clone_outcome: Some(jobs::VoiceCloneRunOutcome::ClonePreserved),
            voice_clone_requested_segments: Some(4),
            voice_clone_converted_segments: Some(4),
            voice_clone_fallback_segments: Some(0),
            voice_clone_standard_tts_segments: Some(0),
            rerun_kind: Some(ArtifactRerunKind::MuxDubPreviewV1),
        };

        let value = serde_json::to_value(&artifact).expect("serialize artifact");
        assert_eq!(value["kind"], "dub_mux");
        assert_eq!(value["rerun_kind"], "mux_dub_preview_v1");
        assert_eq!(value["job_type"], "mux_dub_preview_v1");
        assert_eq!(value["variant_label"], "Take B");
        assert_eq!(value["track_id"], "track-en");
        assert_eq!(value["mux_container"], "mp4");
        assert_eq!(value["tts_backend_id"], "openvoice_v2");
        assert_eq!(value["voice_clone_outcome"], "clone_preserved");
        assert_eq!(value["voice_clone_requested_segments"], 4);
        assert_eq!(value["voice_clone_converted_segments"], 4);
    }

    #[test]
    fn agent_ui_request_validation_is_headless_bounded_and_object_only() {
        let foreground = validate_agent_ui_request(false, "{}").expect_err("foreground rejected");
        assert_eq!(foreground.0, "403 Forbidden");

        let invalid = validate_agent_ui_request(true, "[]").expect_err("array rejected");
        assert_eq!(invalid.0, "400 Bad Request");

        let oversized = format!(r#"{{"value":"{}"}}"#, "x".repeat(17 * 1024));
        let too_large =
            validate_agent_ui_request(true, &oversized).expect_err("oversized rejected");
        assert_eq!(too_large.0, "413 Payload Too Large");

        let accepted = validate_agent_ui_request(true, r#"{"limit":700,"include_offscreen":true}"#)
            .expect("headless object accepted");
        assert_eq!(accepted["limit"], 700);
        assert_eq!(accepted["include_offscreen"], true);
    }

    #[test]
    fn agent_bridge_cleanup_only_owns_its_exact_pid_marker() {
        assert!(agent_bridge_marker_owned_by_process(
            r#"{"pid":42,"port":51000}"#,
            42
        ));
        assert!(!agent_bridge_marker_owned_by_process(
            r#"{"pid":43,"port":51000}"#,
            42
        ));
        assert!(!agent_bridge_marker_owned_by_process("not json", 42));
    }

    #[test]
    fn headless_audit_disables_runtime_background_work() {
        assert!(runtime_background_work_enabled(false, false));
        assert!(!runtime_background_work_enabled(true, false));
        assert!(!runtime_background_work_enabled(false, true));
        assert!(!runtime_background_work_enabled(true, true));
    }

    #[test]
    fn build_download_dir_status_includes_feature_defaults_from_base_root() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        paths
            .set_download_dir_override(&dir.path().join("storage"))
            .expect("set base root");

        let status = build_download_dir_status(&paths).expect("status");
        let video = status
            .feature_roots
            .iter()
            .find(|root| root.key == "video")
            .expect("video root");
        let localization = status
            .feature_roots
            .iter()
            .find(|root| root.key == "localization")
            .expect("localization root");

        assert!(
            video.current_dir.ends_with("storage\\video")
                || video.current_dir.ends_with("storage/video")
        );
        assert!(
            localization
                .current_dir
                .ends_with("storage\\localization\\en")
                || localization
                    .current_dir
                    .ends_with("storage/localization/en")
        );
    }

    #[test]
    fn build_download_dir_status_prefers_feature_overrides() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        paths
            .set_download_dir_override(&dir.path().join("storage"))
            .expect("set base root");
        let override_dir = dir.path().join("video_override");
        std::fs::create_dir_all(&override_dir).expect("create override");
        config::save_feature_storage_roots_config(
            &paths,
            &config::FeatureStorageRootsConfig {
                video_root: Some(override_dir.to_string_lossy().to_string()),
                instagram_root: None,
                image_root: None,
                localization_root: None,
            },
        )
        .expect("save overrides");

        let status = build_download_dir_status(&paths).expect("status");
        let video = status
            .feature_roots
            .iter()
            .find(|root| root.key == "video")
            .expect("video root");
        assert_eq!(
            video.override_dir.as_deref(),
            Some(override_dir.to_string_lossy().as_ref())
        );
        assert_eq!(video.current_dir, override_dir.to_string_lossy());
    }

    #[test]
    fn shell_paths_status_reports_missing_and_existing_paths() {
        let dir = tempfile::tempdir().expect("tempdir");
        let existing = dir.path().join("exists.txt");
        std::fs::write(&existing, "ok").expect("write existing");
        let missing = dir.path().join("missing.txt");

        let rows = shell_paths_status_impl(
            None,
            vec![
                existing.to_string_lossy().to_string(),
                missing.to_string_lossy().to_string(),
            ],
        )
        .expect("status rows");

        assert_eq!(rows.len(), 2);
        assert!(rows[0].exists);
        assert!(!rows[0].is_dir);
        assert!(!rows[1].exists);
        assert!(!rows[1].is_dir);
    }

    #[test]
    fn jobs_tracks_bridge_response_serializes_the_canonical_engine_snapshot_shape() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        db::ensure_schema(&paths).expect("ensure schema");
        jobs::enqueue_dummy_sleep(&paths, 1).expect("enqueue other-video job");

        let expected = serde_json::to_value(
            jobs::get_job_tracks_runtime_snapshot(&paths).expect("engine snapshot"),
        )
        .expect("serialize engine snapshot");
        let (status, body) = jobs_tracks_bridge_response(&paths);
        assert_eq!(status, "200 OK");
        let bridge: serde_json::Value = serde_json::from_str(&body).expect("bridge json");
        assert_eq!(
            bridge, expected,
            "bridge must not project a different truth"
        );
        assert_eq!(bridge["tracks"].as_array().map(Vec::len), Some(6));
        assert_eq!(bridge["tracks"][0]["track"], "youtube_single");
        assert!(bridge["tracks"][0].get("configured_budget").is_some());
        assert!(bridge["tracks"][0].get("total").is_some());
        assert!(bridge["unclassified"].is_object());
        assert!(bridge["youtube_gate"].is_object());
    }

    #[test]
    fn diagnostics_app_state_snapshot_export_writes_json_and_markdown() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        paths.ensure_dirs().expect("ensure dirs");
        db::ensure_schema(&paths).expect("ensure schema");

        let startup = StartupStatus {
            offline_bundle_state: "ready".to_string(),
            offline_bundle_started_at_ms: Some(1),
            offline_bundle_finished_at_ms: Some(2),
            offline_bundle_error: None,
            progress_pct: 1.0,
            active_phase_id: None,
            phases: vec![
                StartupPhase {
                    id: "app_dirs".to_string(),
                    label: "App data + output layout".to_string(),
                    state: "ready".to_string(),
                    started_at_ms: Some(1),
                    finished_at_ms: Some(2),
                    error: None,
                },
                StartupPhase {
                    id: "offline_bundle".to_string(),
                    label: "Offline bundle hydration".to_string(),
                    state: "ready".to_string(),
                    started_at_ms: Some(1),
                    finished_at_ms: Some(2),
                    error: None,
                },
            ],
        };

        let snapshot = build_diagnostics_app_state_snapshot(
            &paths,
            "VoxVulgi".to_string(),
            "0.1.5".to_string(),
            startup,
        )
        .expect("snapshot");
        let export = write_diagnostics_app_state_snapshot_exports(
            &snapshot,
            &dir.path().join("support").join("app-state"),
        )
        .expect("export");

        let json_text = std::fs::read_to_string(&export.json_path).expect("json");
        let markdown_text = std::fs::read_to_string(&export.markdown_path).expect("markdown");
        assert!(json_text.contains("\"feature_health\""));
        assert!(json_text.contains("\"jobs_tracks\""));
        assert_eq!(snapshot.jobs_tracks.tracks.len(), 6);
        assert!(markdown_text.contains("# VoxVulgi app-state snapshot"));
        assert!(markdown_text.contains("## Feature health"));
        assert!(markdown_text.contains("## Scheduler tracks"));
        for track in [
            "youtube_single",
            "youtube_recurring",
            "instagram",
            "other_video",
            "image_archive",
            "localization",
        ] {
            assert!(markdown_text.contains(&format!("| `{track}` |")));
        }
        assert!(markdown_text.contains("| `unclassified` |"));
        assert!(markdown_text.contains("### Shared YouTube start gate"));
        assert!(markdown_text.contains("Next eligible start:"));
        assert!(markdown_text.contains("Hold reason:"));

        if let Ok(proof_dir) = std::env::var("VOXVULGI_WP0135_PROOF_DIR") {
            let proof_dir = std::path::PathBuf::from(proof_dir);
            std::fs::create_dir_all(&proof_dir).expect("create proof dir");
            std::fs::copy(
                &export.json_path,
                proof_dir.join("sample_app_state_snapshot.json"),
            )
            .expect("copy json proof");
            std::fs::copy(
                &export.markdown_path,
                proof_dir.join("sample_app_state_snapshot.md"),
            )
            .expect("copy markdown proof");
        }
    }

    #[test]
    fn localization_consumer_falls_back_only_for_canonical_absence_not_lineage_failure() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app"));
        paths.ensure_dirs().expect("dirs");
        db::ensure_schema(&paths).expect("schema");
        let item_id = "consumer-lineage-item";
        let generation_id = format!("localization-preview-{}", "a".repeat(64));
        let dub_dir = paths.derived_item_dir(item_id).join("dub_preview");
        std::fs::create_dir_all(&dub_dir).expect("dub dir");
        let legacy_fixed = dub_dir.join("mux_dub_preview_v1.mkv");
        std::fs::write(&legacy_fixed, b"unverified-fixed-path-bytes").expect("legacy fixed");
        let immutable_path = dub_dir.join(format!("mux_dub_preview_v1.gen-{}.mkv", "a".repeat(64)));
        let conn = db::open(&paths).expect("db");
        conn.execute(
            "INSERT INTO library_item(id,created_at_ms,source_type,source_uri,title,media_path) VALUES(?1,1,'local_file','file://source','Source','source.mp4')",
            [item_id],
        )
        .expect("item");
        conn.execute(
            "INSERT INTO job(id,item_id,type,status,progress,params_json,created_at_ms,logs_path) VALUES('mux-consumer',?1,'mux_dub_preview_v1','succeeded',1,'{}',1,'mux.log')",
            [item_id],
        )
        .expect("job");
        conn.execute(
            "INSERT INTO localization_preview_publication(generation_id,item_id,variant_key,input_fingerprint_sha256,input_fingerprint_json,artifact_path,artifact_bytes,artifact_sha256,staging_path,source_job_id,phase,created_at_ms,updated_at_ms) VALUES(?1,?2,'','fingerprint','{}',?3,1,'deadbeef',?4,'mux-consumer','committed',1,1)",
            rusqlite::params![
                generation_id,
                item_id,
                immutable_path.to_string_lossy(),
                dub_dir.join("missing-staging.mkv").to_string_lossy()
            ],
        )
        .expect("publication");
        conn.execute(
            "INSERT INTO localization_preview_active(item_id,variant_key,generation_id,source_job_created_at_ms,source_job_id,updated_at_ms) VALUES(?1,'',?2,1,'mux-consumer',1)",
            rusqlite::params![item_id, generation_id],
        )
        .expect("active");
        drop(conn);

        let error = localization_preview_consumer_path(&paths, item_id, None, legacy_fixed.clone())
            .expect_err("missing committed immutable bytes must not fall back");
        assert!(error.contains("immutable-lineage verification"), "{error}");

        let absent_fixed = dub_dir.join("legacy-without-active.mkv");
        assert_eq!(
            localization_preview_consumer_path(
                &paths,
                "item-with-no-active-publication",
                None,
                absent_fixed.clone(),
            )
            .expect("canonical absence permits legacy routing"),
            absent_fixed
        );

        let published_item = "consumer-published-item";
        let published_generation = format!("localization-preview-{}", "c".repeat(64));
        let published_dir = paths.derived_item_dir(published_item).join("dub_preview");
        std::fs::create_dir_all(&published_dir).expect("published dir");
        let published_fixed = published_dir.join("mux_dub_preview_v1.mkv");
        std::fs::write(&published_fixed, b"legacy-must-not-mask-published-active")
            .expect("published fixed fallback");
        let conn = db::open(&paths).expect("published db");
        conn.execute(
            "INSERT INTO library_item(id,created_at_ms,source_type,source_uri,title,media_path)
             VALUES(?1,1,'local_file','file://published','Published','source.mp4')",
            [published_item],
        )
        .expect("published item");
        conn.execute(
            "INSERT INTO job(id,item_id,type,status,progress,params_json,created_at_ms,logs_path)
             VALUES('mux-published-consumer',?1,'mux_dub_preview_v1','succeeded',1,'{}',1,'mux.log')",
            [published_item],
        )
        .expect("published job");
        conn.execute(
            "INSERT INTO localization_preview_publication(
               generation_id,item_id,variant_key,input_fingerprint_sha256,input_fingerprint_json,
               artifact_path,artifact_bytes,artifact_sha256,staging_path,source_job_id,phase,
               created_at_ms,updated_at_ms
             ) VALUES(?1,?2,'','published-fingerprint','{}',?3,1,'published-hash',?4,
                      'mux-published-consumer','published',1,1)",
            rusqlite::params![
                published_generation,
                published_item,
                published_dir.join(format!("mux_dub_preview_v1.gen-{}.mkv", "c".repeat(64))).to_string_lossy(),
                published_dir.join("published-stage.mkv").to_string_lossy(),
            ],
        )
        .expect("published publication");
        conn.execute_batch(
            "DROP TRIGGER trg_localization_preview_active_committed_insert;
             DROP TRIGGER trg_localization_preview_active_committed_update;",
        )
        .expect("simulate upgraded pre-v44 active pointer");
        conn.execute(
            "INSERT INTO localization_preview_active(
               item_id,variant_key,generation_id,source_job_created_at_ms,source_job_id,updated_at_ms
             ) VALUES(?1,'',?2,1,'mux-published-consumer',1)",
            rusqlite::params![published_item, published_generation],
        )
        .expect("published active pointer");
        drop(conn);
        let error = localization_preview_consumer_path(
            &paths,
            published_item,
            None,
            published_fixed,
        )
        .expect_err("published active lineage must not route to fixed legacy bytes");
        assert!(error.contains("non-committed publication phase published"), "{error}");
    }
}

fn ensure_media_output_layout(root: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
    for sub in ["video", "instagram", "images", "localization"] {
        std::fs::create_dir_all(root.join(sub)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn feature_root_default_dir(base_root: &std::path::Path, key: &str) -> std::path::PathBuf {
    match key {
        "video" => base_root.join("video"),
        "instagram" => base_root.join("instagram"),
        "images" => base_root.join("images"),
        "localization" => base_root.join("localization").join("en"),
        _ => base_root.to_path_buf(),
    }
}

fn feature_root_label(key: &str) -> &'static str {
    match key {
        "video" => "Video Archiver",
        "instagram" => "Instagram Archiver",
        "images" => "Image Archive",
        "localization" => "Localization Studio exports",
        _ => "Feature",
    }
}

fn set_feature_root_override(
    roots: &mut config::FeatureStorageRootsConfig,
    feature: &str,
    value: Option<String>,
) -> Result<(), String> {
    match feature {
        "video" => roots.video_root = value,
        "instagram" => roots.instagram_root = value,
        "images" => roots.image_root = value,
        "localization" => roots.localization_root = value,
        _ => return Err(format!("unknown storage feature: {feature}")),
    }
    Ok(())
}

fn mime_from_path(path: &std::path::Path) -> &'static str {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "webp" => "image/webp",
        _ => "image/jpeg",
    }
}

fn build_download_dir_status(paths: &AppPaths) -> Result<DownloadDirStatus, String> {
    let default_dir = paths.default_download_dir();
    let override_dir = paths.download_dir_override().map_err(|e| e.to_string())?;
    let current_dir = override_dir.clone().unwrap_or_else(|| default_dir.clone());
    if current_dir.exists() && current_dir.is_dir() {
        ensure_media_output_layout(&current_dir)?;
    }
    let exists = current_dir.exists() && current_dir.is_dir();
    let feature_roots_config =
        config::load_feature_storage_roots_config(paths).map_err(|e| e.to_string())?;
    let feature_roots = [
        ("video", feature_roots_config.video_root.clone()),
        ("instagram", feature_roots_config.instagram_root.clone()),
        ("images", feature_roots_config.image_root.clone()),
        (
            "localization",
            feature_roots_config.localization_root.clone(),
        ),
    ]
    .into_iter()
    .map(|(key, override_value)| {
        let default_feature_dir = feature_root_default_dir(&current_dir, key);
        let current_feature_dir = override_value
            .as_deref()
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| default_feature_dir.clone());
        if current_feature_dir.exists() && current_feature_dir.is_dir() {
            std::fs::create_dir_all(&current_feature_dir).map_err(|e| e.to_string())?;
        }
        Ok(FeatureStorageRootStatus {
            key: key.to_string(),
            label: feature_root_label(key).to_string(),
            current_dir: current_feature_dir.to_string_lossy().to_string(),
            default_dir: default_feature_dir.to_string_lossy().to_string(),
            override_dir: override_value,
            exists: current_feature_dir.exists() && current_feature_dir.is_dir(),
        })
    })
    .collect::<Result<Vec<_>, String>>()?;

    Ok(DownloadDirStatus {
        current_dir: current_dir.to_string_lossy().to_string(),
        default_dir: default_dir.to_string_lossy().to_string(),
        exists,
        using_default: override_dir.is_none(),
        feature_roots,
    })
}

fn normalize_existing_shell_path(path: String, label: &str) -> Result<std::path::PathBuf, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(format!("{label} is empty"));
    }
    if trimmed.contains('\0') {
        return Err(format!("{label} contains invalid characters"));
    }
    let mut target = std::path::PathBuf::from(trimmed);
    if !target.is_absolute() {
        target = std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(target);
    }
    let normalized = target.canonicalize().unwrap_or(target);
    if !normalized.exists() {
        return Err(format!(
            "{label} does not exist: {}",
            normalized.to_string_lossy()
        ));
    }
    Ok(normalized)
}

fn normalize_shell_path(path: String, label: &str) -> Result<std::path::PathBuf, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(format!("{label} is empty"));
    }
    if trimmed.contains('\0') {
        return Err(format!("{label} contains invalid characters"));
    }
    let mut target = std::path::PathBuf::from(trimmed);
    if !target.is_absolute() {
        target = std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(target);
    }
    Ok(target.canonicalize().unwrap_or(target))
}

fn run_shell_command(command: &mut std::process::Command, action: &str) -> Result<(), String> {
    let status = command.status().map_err(|e| format!("{action}: {e}"))?;
    if !status.success() {
        return Err(format!(
            "{action} failed with exit code {:?}",
            status.code()
        ));
    }
    Ok(())
}

fn shell_open_target(path: &std::path::Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let mut command = std::process::Command::new("cmd");
        command.arg("/C").arg("start").arg("").arg(path.as_os_str());
        return run_shell_command(&mut command, "open path");
    }

    #[cfg(target_os = "macos")]
    {
        let mut command = std::process::Command::new("open");
        command.arg(path.as_os_str());
        return run_shell_command(&mut command, "open path");
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(path.as_os_str());
        return run_shell_command(&mut command, "open path");
    }
}

fn shell_reveal_target(path: &std::path::Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let mut command = std::process::Command::new("explorer");
        let is_select = !path.is_dir();
        if path.is_dir() {
            command.arg(path.as_os_str());
        } else {
            command.arg("/select,").arg(path.as_os_str());
        }
        // WP-0222: explorer.exe /select,<file> returns exit code 1 even when
        // it successfully opens Explorer and selects the file. Treat exit 1
        // as success only for the /select, call path; folder-only reveals
        // keep strict success semantics.
        let status = command.status().map_err(|e| format!("reveal path: {e}"))?;
        if !status.success() {
            let code = status.code();
            if is_select && code == Some(1) {
                return Ok(());
            }
            return Err(format!("reveal path failed with exit code {:?}", code));
        }
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        let mut command = std::process::Command::new("open");
        if path.is_dir() {
            command.arg(path.as_os_str());
        } else {
            command.arg("-R").arg(path.as_os_str());
        }
        return run_shell_command(&mut command, "reveal path");
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let parent = if path.is_dir() {
            path.to_path_buf()
        } else {
            path.parent()
                .ok_or_else(|| format!("path has no parent: {}", path.to_string_lossy()))?
                .to_path_buf()
        };
        let mut command = std::process::Command::new("xdg-open");
        command.arg(parent.as_os_str());
        return run_shell_command(&mut command, "reveal path");
    }
}

fn resolve_shell_alias(
    paths: Option<&AppPaths>,
    path: std::path::PathBuf,
) -> Result<std::path::PathBuf, String> {
    let Some(paths) = paths else {
        return Ok(path);
    };
    root_rebind::resolve_active_alias_path(paths, &path, false).map_err(|error| error.to_string())
}

fn shell_paths_status_impl(
    app_paths: Option<&AppPaths>,
    requested_paths: Vec<String>,
) -> Result<Vec<ShellPathStatus>, String> {
    let mut rows = Vec::with_capacity(requested_paths.len());
    for path in requested_paths {
        let normalized = normalize_shell_path(path, "Path")?;
        let resolved = resolve_shell_alias(app_paths, normalized)?;
        let (exists, is_dir) =
            match voxvulgi_engine::paths::probe_path_bounded(&resolved, Duration::from_millis(300))
            {
                voxvulgi_engine::paths::BoundedPathKind::Directory => (true, true),
                voxvulgi_engine::paths::BoundedPathKind::File => (true, false),
                voxvulgi_engine::paths::BoundedPathKind::Missing
                | voxvulgi_engine::paths::BoundedPathKind::Unreachable => (false, false),
            };
        rows.push(ShellPathStatus {
            path: resolved.to_string_lossy().to_string(),
            exists,
            is_dir,
        });
    }
    Ok(rows)
}

#[tauri::command]
fn shell_paths_status(
    state: State<'_, AppState>,
    paths: Vec<String>,
) -> Result<Vec<ShellPathStatus>, String> {
    shell_paths_status_impl(Some(&state.paths), paths)
}

#[tauri::command]
fn shell_open_path(state: State<'_, AppState>, path: String) -> Result<ShellPathResult, String> {
    let normalized = normalize_shell_path(path, "Path")?;
    let normalized = resolve_shell_alias(Some(&state.paths), normalized)?;
    if !normalized.exists() {
        return Err(format!(
            "Path does not exist: {}",
            normalized.to_string_lossy()
        ));
    }
    shell_open_target(&normalized)?;
    Ok(ShellPathResult {
        path: normalized.to_string_lossy().to_string(),
        method: "shell_open_path".to_string(),
    })
}

#[tauri::command]
fn shell_reveal_path(state: State<'_, AppState>, path: String) -> Result<ShellPathResult, String> {
    let normalized = normalize_shell_path(path, "Path")?;
    let normalized = resolve_shell_alias(Some(&state.paths), normalized)?;
    if !normalized.exists() {
        return Err(format!(
            "Path does not exist: {}",
            normalized.to_string_lossy()
        ));
    }
    shell_reveal_target(&normalized)?;
    Ok(ShellPathResult {
        path: normalized.to_string_lossy().to_string(),
        method: "shell_reveal_path".to_string(),
    })
}

#[tauri::command]
fn shell_open_parent_dir(
    state: State<'_, AppState>,
    path: String,
) -> Result<ShellPathResult, String> {
    let normalized = normalize_shell_path(path, "Path")?;
    let normalized = resolve_shell_alias(Some(&state.paths), normalized)?;
    if !normalized.exists() {
        return Err(format!(
            "Path does not exist: {}",
            normalized.to_string_lossy()
        ));
    }
    let target = if normalized.is_dir() {
        normalized
    } else {
        normalized
            .parent()
            .ok_or_else(|| "Path has no parent directory".to_string())?
            .to_path_buf()
    };
    shell_open_target(&target)?;
    Ok(ShellPathResult {
        path: target.to_string_lossy().to_string(),
        method: "shell_open_parent_dir".to_string(),
    })
}

fn build_diagnostics_trace_dir_status(
    paths: &AppPaths,
) -> Result<DiagnosticsTraceDirStatus, String> {
    let default_dir = paths.default_diagnostics_trace_dir();
    let override_dir = paths
        .diagnostics_trace_dir_override()
        .map_err(|e| e.to_string())?;
    let current_dir = override_dir.clone().unwrap_or_else(|| default_dir.clone());
    let exists = current_dir.exists() && current_dir.is_dir();
    let rotation_state =
        std::fs::read_to_string(current_dir.join("diagnostics_trace.rotation_state.json"))
            .ok()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
            .unwrap_or_default();
    let persisted_rotation_count = rotation_state
        .get("rotation_count")
        .and_then(|value| value.as_u64())
        .unwrap_or_else(|| DIAGNOSTICS_TRACE_ROTATIONS_TOTAL.load(Ordering::Relaxed));

    Ok(DiagnosticsTraceDirStatus {
        current_dir: current_dir.to_string_lossy().to_string(),
        default_dir: default_dir.to_string_lossy().to_string(),
        exists,
        using_default: override_dir.is_none(),
        retained_age_ms: DIAGNOSTICS_TRACE_RETAINED_AGE_MS,
        rotation_count: persisted_rotation_count,
        compressed_files: count_compressed_trace_files(&current_dir.join("diagnostics_trace.jsonl"))
            as u64,
        aggregate_path: current_dir
            .join("diagnostics_trace.aggregate.json")
            .to_string_lossy()
            .to_string(),
        sampling_mode: "bounded_queue_no_sampling".to_string(),
        queue_capacity: DIAGNOSTICS_TRACE_QUEUE_CAPACITY,
        dropped_events_total: DIAGNOSTICS_TRACE_DROPPED_TOTAL.load(Ordering::Relaxed),
    })
}

fn path_size_bytes_best_effort(path: &std::path::Path) -> u64 {
    if let Ok(meta) = std::fs::symlink_metadata(path) {
        if meta.is_file() {
            return meta.len();
        }
        if !meta.is_dir() {
            return 0;
        }
    } else {
        return 0;
    }

    let mut total = 0u64;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(v) => v,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let p = entry.path();
            let meta = match std::fs::symlink_metadata(&p) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if meta.is_file() {
                total = total.saturating_add(meta.len());
            } else if meta.is_dir() {
                stack.push(p);
            }
        }
    }
    total
}

fn clear_dir_entries_with_bytes(
    dir: &std::path::Path,
) -> Result<DiagnosticsTraceClearSummary, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let entries = std::fs::read_dir(dir).map_err(|e| e.to_string())?;

    let mut removed_entries: usize = 0;
    let mut removed_bytes: u64 = 0;

    for entry in entries.flatten() {
        let path = entry.path();
        removed_bytes = removed_bytes.saturating_add(path_size_bytes_best_effort(&path));
        let meta = match std::fs::symlink_metadata(&path) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if meta.is_dir() {
            std::fs::remove_dir_all(&path).map_err(|e| e.to_string())?;
        } else {
            std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
        removed_entries += 1;
    }

    Ok(DiagnosticsTraceClearSummary {
        removed_entries,
        removed_bytes,
    })
}

fn diagnostics_count_value(conn: &rusqlite::Connection, sql: &str) -> Result<u64, String> {
    conn.query_row(sql, [], |row| row.get::<_, i64>(0))
        .map(|value| value.max(0) as u64)
        .map_err(|e| e.to_string())
}

fn diagnostics_key_counts(
    conn: &rusqlite::Connection,
    sql: &str,
) -> Result<Vec<DiagnosticsKeyCount>, String> {
    let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            let key: Option<String> = row.get(0)?;
            let count: i64 = row.get(1)?;
            Ok(DiagnosticsKeyCount {
                key: key
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
                    .unwrap_or_else(|| "(unknown)".to_string()),
                count: count.max(0) as u64,
            })
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

fn build_job_queue_snapshot(paths: &AppPaths) -> Result<DiagnosticsJobQueueSnapshot, String> {
    let conn = db::open(paths).map_err(|e| e.to_string())?;
    db::migrate(&conn).map_err(|e| e.to_string())?;
    let total = diagnostics_count_value(&conn, "SELECT COUNT(*) FROM job")?;
    let queued = diagnostics_count_value(&conn, "SELECT COUNT(*) FROM job WHERE status='queued'")?;
    let running =
        diagnostics_count_value(&conn, "SELECT COUNT(*) FROM job WHERE status='running'")?;
    let succeeded =
        diagnostics_count_value(&conn, "SELECT COUNT(*) FROM job WHERE status='succeeded'")?;
    let failed = diagnostics_count_value(&conn, "SELECT COUNT(*) FROM job WHERE status='failed'")?;
    let canceled =
        diagnostics_count_value(&conn, "SELECT COUNT(*) FROM job WHERE status='canceled'")?;
    let active_batch_count = diagnostics_count_value(
        &conn,
        "SELECT COUNT(DISTINCT batch_id) FROM job WHERE batch_id IS NOT NULL AND TRIM(batch_id) <> '' AND status IN ('queued','running')",
    )?;

    let mut stmt = conn
        .prepare(
            "SELECT id, type, item_id, created_at_ms, COALESCE(error, '') \
             FROM job WHERE status='failed' ORDER BY created_at_ms DESC LIMIT 10",
        )
        .map_err(|e| e.to_string())?;
    let failures = stmt
        .query_map([], |row| {
            Ok(DiagnosticsRecentJobFailure {
                id: row.get(0)?,
                job_type: row.get(1)?,
                item_id: row.get(2)?,
                created_at_ms: row.get::<_, i64>(3)?,
                error: row.get::<_, String>(4)?,
            })
        })
        .map_err(|e| e.to_string())?;
    let mut recent_failures = Vec::new();
    for row in failures {
        recent_failures.push(row.map_err(|e| e.to_string())?);
    }

    Ok(DiagnosticsJobQueueSnapshot {
        total,
        queued,
        running,
        succeeded,
        failed,
        canceled,
        active_batch_count,
        recent_failures,
    })
}

fn build_library_snapshot(paths: &AppPaths) -> Result<DiagnosticsLibrarySnapshot, String> {
    let conn = db::open(paths).map_err(|e| e.to_string())?;
    db::migrate(&conn).map_err(|e| e.to_string())?;

    Ok(DiagnosticsLibrarySnapshot {
        total_items: diagnostics_count_value(&conn, "SELECT COUNT(*) FROM library_item")?,
        by_source_type: diagnostics_key_counts(
            &conn,
            "SELECT source_type, COUNT(*) FROM library_item GROUP BY source_type ORDER BY COUNT(*) DESC, source_type ASC",
        )?,
        by_provider: diagnostics_key_counts(
            &conn,
            "SELECT provider, COUNT(*) FROM ingest_provenance GROUP BY provider ORDER BY COUNT(*) DESC, provider ASC",
        )?,
        subtitle_track_count: diagnostics_count_value(&conn, "SELECT COUNT(*) FROM subtitle_track")?,
        translated_en_track_count: diagnostics_count_value(
            &conn,
            "SELECT COUNT(*) FROM subtitle_track WHERE kind='translated' AND lang='en'",
        )?,
        item_speaker_count: diagnostics_count_value(&conn, "SELECT COUNT(*) FROM item_speaker")?,
        item_voice_plan_count: diagnostics_count_value(&conn, "SELECT COUNT(*) FROM item_voice_plan")?,
        voice_template_count: diagnostics_count_value(&conn, "SELECT COUNT(*) FROM voice_template")?,
        voice_cast_pack_count: diagnostics_count_value(&conn, "SELECT COUNT(*) FROM voice_cast_pack")?,
        voice_library_profile_count: voice_library::list_voice_library_profiles(paths, None)
            .map(|rows| rows.len() as u64)
            .map_err(|e| e.to_string())?,
        youtube_subscription_count: subscriptions::list_youtube_subscriptions(paths)
            .map(|rows| rows.len() as u64)
            .map_err(|e| e.to_string())?,
        instagram_subscription_count: instagram_subscriptions::list_instagram_subscriptions(paths)
            .map(|rows| rows.len() as u64)
            .map_err(|e| e.to_string())?,
    })
}

fn build_feature_health_rows(
    paths: &AppPaths,
    startup: &StartupStatus,
    ffmpeg: &tools::FfmpegToolsStatus,
    ytdlp: &tools::YtDlpToolsStatus,
    js_runtime: &tools::JsRuntimeToolsStatus,
    python: &tools::PythonToolchainStatus,
    neural: &tools::TtsNeuralLocalV1PackStatus,
    voice_preserving: &tools::TtsVoicePreservingLocalV1PackStatus,
    models: &voxvulgi_engine::models::ModelInventory,
    trace_dir: &DiagnosticsTraceDirStatus,
    jobs: &DiagnosticsJobQueueSnapshot,
) -> Vec<DiagnosticsFeatureHealthRow> {
    // Track the configured/active ASR model so the health row stays truthful after an
    // operator swaps the ASR model (default large-v3 q5_0; tiny is only a fallback).
    let active_asr = paths.effective_asr_model_id();
    let whisper_ready = models
        .models
        .iter()
        .any(|model| model.id == active_asr && model.installed);
    let startup_blocked = startup
        .phases
        .iter()
        .any(|phase| matches!(phase.state.as_str(), "pending" | "running"));

    vec![
        DiagnosticsFeatureHealthRow {
            feature: "Startup hydration".to_string(),
            status: if startup_blocked {
                format!("loading {}%", (startup.progress_pct * 100.0).round())
            } else if startup.offline_bundle_state == "error" {
                "error".to_string()
            } else {
                "ready".to_string()
            },
            detail: startup
                .phases
                .iter()
                .find(|phase| phase.id == startup.active_phase_id.clone().unwrap_or_default())
                .map(|phase| phase.label.clone())
                .unwrap_or_else(|| startup.offline_bundle_state.clone()),
        },
        DiagnosticsFeatureHealthRow {
            feature: "Video/Instagram archivers".to_string(),
            status: if ffmpeg.installed && ytdlp.available && js_runtime.available {
                "ready".to_string()
            } else if startup_blocked {
                "loading".to_string()
            } else {
                "partial".to_string()
            },
            detail: format!(
                "FFmpeg={} / yt-dlp={} / JS runtime={}",
                if ffmpeg.installed { "ready" } else { "missing" },
                if ytdlp.available { "ready" } else { "missing" },
                if js_runtime.available {
                    "ready"
                } else {
                    "missing"
                }
            ),
        },
        DiagnosticsFeatureHealthRow {
            feature: "Localization core".to_string(),
            status: if ffmpeg.installed && whisper_ready {
                "ready".to_string()
            } else if startup_blocked {
                "loading".to_string()
            } else {
                "blocked".to_string()
            },
            detail: format!(
                "FFmpeg={} / Whisper.cpp={}",
                if ffmpeg.installed { "ready" } else { "missing" },
                if whisper_ready { "ready" } else { "missing" }
            ),
        },
        DiagnosticsFeatureHealthRow {
            feature: "Voice-preserving dubbing".to_string(),
            status: if python.venv_exists && neural.installed && voice_preserving.installed {
                "ready".to_string()
            } else if startup_blocked {
                "loading".to_string()
            } else {
                "partial".to_string()
            },
            detail: format!(
                "Python venv={} / neural pack={} / voice pack={}",
                if python.venv_exists {
                    "ready"
                } else {
                    "missing"
                },
                if neural.installed { "ready" } else { "missing" },
                if voice_preserving.installed {
                    "ready"
                } else {
                    "missing"
                }
            ),
        },
        DiagnosticsFeatureHealthRow {
            feature: "Diagnostics trace".to_string(),
            status: if trace_dir.exists {
                "ready".to_string()
            } else {
                "blocked".to_string()
            },
            detail: trace_dir.current_dir.clone(),
        },
        DiagnosticsFeatureHealthRow {
            feature: "Job engine".to_string(),
            status: if jobs.running > 0 {
                "busy".to_string()
            } else {
                "ready".to_string()
            },
            detail: format!(
                "{} queued / {} running / {} failed",
                jobs.queued, jobs.running, jobs.failed
            ),
        },
    ]
}

fn render_diagnostics_app_state_snapshot_markdown(
    snapshot: &DiagnosticsAppStateSnapshot,
) -> String {
    let mut md = String::new();
    md.push_str("# VoxVulgi app-state snapshot\n\n");
    md.push_str(&format!(
        "- Generated: `{}`\n- App: `{} {}`\n- Engine: `{}`\n\n",
        snapshot.generated_at_ms,
        snapshot.app.app_name,
        snapshot.app.app_version,
        snapshot.app.engine_version
    ));

    md.push_str("## Startup\n\n");
    md.push_str(&format!(
        "- Offline bundle: `{}`\n- Progress: `{:.0}%`\n- Active phase: `{}`\n\n",
        snapshot.startup.offline_bundle_state,
        snapshot.startup.progress_pct * 100.0,
        snapshot
            .startup
            .active_phase_id
            .clone()
            .unwrap_or_else(|| "-".to_string())
    ));

    md.push_str("## Feature health\n\n");
    for row in &snapshot.feature_health {
        md.push_str(&format!(
            "- **{}**: `{}` - {}\n",
            row.feature, row.status, row.detail
        ));
    }
    md.push('\n');

    md.push_str("## Roots\n\n");
    md.push_str(&format!(
        "- App data: `{}`\n- DB: `{}`\n- Download root: `{}`\n- Diagnostics trace: `{}`\n\n",
        snapshot.app.app_data_dir,
        snapshot.app.db_path,
        snapshot.download_roots.current_dir,
        snapshot.diagnostics_trace_dir.current_dir
    ));
    for root in &snapshot.download_roots.feature_roots {
        md.push_str(&format!("- {}: `{}`\n", root.label, root.current_dir));
    }
    md.push('\n');

    md.push_str("## Library and jobs\n\n");
    md.push_str(&format!(
        "- Library items: `{}`\n- Subtitle tracks: `{}` (`{}` translated/en)\n- Voice templates: `{}`\n- Cast packs: `{}`\n- Voice library profiles: `{}`\n- YouTube subscriptions: `{}`\n- Instagram subscriptions: `{}`\n- Jobs: total `{}`, queued `{}`, running `{}`, failed `{}`\n\n",
        snapshot.library.total_items,
        snapshot.library.subtitle_track_count,
        snapshot.library.translated_en_track_count,
        snapshot.library.voice_template_count,
        snapshot.library.voice_cast_pack_count,
        snapshot.library.voice_library_profile_count,
        snapshot.library.youtube_subscription_count,
        snapshot.library.instagram_subscription_count,
        snapshot.jobs.total,
        snapshot.jobs.queued,
        snapshot.jobs.running,
        snapshot.jobs.failed
    ));

    // WP-0270: export the same engine-owned canonical track snapshot that Jobs and the
    // read-only agent bridge consume. Do not derive these totals from recent_jobs.
    md.push_str("## Scheduler tracks\n\n");
    md.push_str("| Track | Configured budget | Effective budget | Paused | Queued | Running | Succeeded | Failed | Canceled | Total | Hold reason |\n");
    md.push_str("| --- | ---: | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |\n");
    for track in &snapshot.jobs_tracks.tracks {
        let track_name = match track.track {
            jobs::JobTrack::YoutubeSingle => "youtube_single",
            jobs::JobTrack::YoutubeRecurring => "youtube_recurring",
            jobs::JobTrack::Instagram => "instagram",
            jobs::JobTrack::OtherVideo => "other_video",
            jobs::JobTrack::ImageArchive => "image_archive",
            jobs::JobTrack::Localization => "localization",
        };
        let hold_reason = track
            .hold_reason
            .as_deref()
            .unwrap_or("-")
            .replace('|', "\\|")
            .replace(['\r', '\n'], " ");
        md.push_str(&format!(
            "| `{}` | `{}` | `{}` | `{}` | `{}` | `{}` | `{}` | `{}` | `{}` | `{}` | {} |\n",
            track_name,
            track.configured_budget,
            track.effective_budget,
            track.paused,
            track.queued,
            track.running,
            track.succeeded,
            track.failed,
            track.canceled,
            track.total,
            hold_reason,
        ));
    }
    let unclassified = &snapshot.jobs_tracks.unclassified;
    md.push_str(&format!(
        "| `unclassified` | `-` | `-` | `-` | `{}` | `{}` | `{}` | `{}` | `{}` | `{}` | legacy rows awaiting track repair |\n\n",
        unclassified.queued,
        unclassified.running,
        unclassified.succeeded,
        unclassified.failed,
        unclassified.canceled,
        unclassified.total,
    ));
    let gate = &snapshot.jobs_tracks.youtube_gate;
    let gate_hold_reason = gate
        .hold_reason
        .as_deref()
        .unwrap_or("-")
        .replace('|', "\\|")
        .replace(['\r', '\n'], " ");
    md.push_str("### Shared YouTube start gate\n\n");
    md.push_str(&format!(
        "- State: `{}`\n- Next eligible start: `{}`\n- Hold reason: `{}`\n\n",
        gate.state,
        gate.next_eligible_at_ms
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".to_string()),
        gate_hold_reason,
    ));

    if !snapshot.jobs.recent_failures.is_empty() {
        md.push_str("## Recent failures\n\n");
        for failure in &snapshot.jobs.recent_failures {
            md.push_str(&format!(
                "- `{}` / `{}` / item `{}`\n  - {}\n",
                failure.id,
                failure.job_type,
                failure.item_id.clone().unwrap_or_else(|| "-".to_string()),
                failure.error
            ));
        }
        md.push('\n');
    }

    md
}

fn write_diagnostics_app_state_snapshot_exports(
    snapshot: &DiagnosticsAppStateSnapshot,
    out_path: &std::path::Path,
) -> Result<DiagnosticsAppStateSnapshotExport, String> {
    let mut json_path = out_path.to_path_buf();
    let has_json_extension = json_path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.eq_ignore_ascii_case("json"))
        .unwrap_or(false);
    if !has_json_extension {
        json_path.set_extension("json");
    }
    if let Some(parent) = json_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            format!(
                "failed to create snapshot export dir {}: {e}",
                parent.to_string_lossy()
            )
        })?;
    }
    let markdown_path = json_path.with_extension("md");
    let json_bytes = serde_json::to_vec_pretty(snapshot).map_err(|e| e.to_string())?;
    let mut json_payload = json_bytes;
    json_payload.push(b'\n');
    std::fs::write(&json_path, &json_payload).map_err(|e| {
        format!(
            "failed to write snapshot json {}: {e}",
            json_path.to_string_lossy()
        )
    })?;
    let markdown = render_diagnostics_app_state_snapshot_markdown(snapshot);
    std::fs::write(&markdown_path, markdown.as_bytes()).map_err(|e| {
        format!(
            "failed to write snapshot markdown {}: {e}",
            markdown_path.to_string_lossy()
        )
    })?;
    let markdown_bytes = std::fs::metadata(&markdown_path)
        .map(|meta| meta.len())
        .unwrap_or(markdown.len() as u64);
    Ok(DiagnosticsAppStateSnapshotExport {
        generated_at_ms: snapshot.generated_at_ms,
        json_path: json_path.to_string_lossy().to_string(),
        markdown_path: markdown_path.to_string_lossy().to_string(),
        json_bytes: json_payload.len() as u64,
        markdown_bytes,
    })
}

fn build_diagnostics_app_state_snapshot(
    paths: &AppPaths,
    app_name: String,
    app_version: String,
    startup: StartupStatus,
) -> Result<DiagnosticsAppStateSnapshot, String> {
    let app = DiagnosticsInfo {
        app_data_dir: paths.base_dir.to_string_lossy().to_string(),
        db_path: paths
            .db_dir()
            .join("app.sqlite")
            .to_string_lossy()
            .to_string(),
        app_name,
        app_version,
        engine_version: diagnostics::engine_version().to_string(),
    };
    let download_roots = build_download_dir_status(paths)?;
    let diagnostics_trace_dir = build_diagnostics_trace_dir_status(paths)?;
    let ffmpeg = tools::ffmpeg_tools_status(paths);
    let ytdlp = tools::ytdlp_tools_status(paths);
    let js_runtime = tools::js_runtime_tools_status(paths);
    let python = tools::python_toolchain_status(paths);
    let portable_python = tools::portable_python_status(paths);
    let spleeter = tools::spleeter_pack_status(paths);
    let demucs = tools::demucs_pack_status(paths);
    let diarization = tools::diarization_pack_status(paths);
    let tts_preview = tools::tts_preview_pack_status(paths);
    let tts_neural_local_v1 = tools::tts_neural_local_v1_pack_status(paths);
    let tts_voice_preserving_local_v1 = tools::tts_voice_preserving_local_v1_pack_status(paths);
    let voice_backend_catalog = voice_backends::backend_catalog(paths);
    let voice_backend_recommendation = voice_backends::recommend_backend(paths, Default::default());
    let voice_backend_adapter_count = voice_backend_adapters::list_voice_backend_adapters(paths)
        .map(|rows| rows.len())
        .unwrap_or(0);
    let models = ModelStore::new(paths.clone())
        .inventory()
        .map_err(|e| e.to_string())?;
    let performance_tier = tools::performance_tier_status(paths);
    let batch_on_import_rules =
        config::load_batch_on_import_rules(paths).map_err(|e| e.to_string())?;
    let optional_diarization_backend =
        config::load_optional_diarization_backend_status(paths).map_err(|e| e.to_string())?;
    let storage = diagnostics::storage_breakdown(paths).map_err(|e| e.to_string())?;
    let thumbnail_cache = library::thumbnail_cache_status(paths).map_err(|e| e.to_string())?;
    let jobs = build_job_queue_snapshot(paths)?;
    let jobs_tracks = jobs::get_job_tracks_runtime_snapshot(paths).map_err(|e| e.to_string())?;
    let library = build_library_snapshot(paths)?;
    let recent_trace = read_recent_diagnostics_trace_entries(paths, 40)?;
    let feature_health = build_feature_health_rows(
        paths,
        &startup,
        &ffmpeg,
        &ytdlp,
        &js_runtime,
        &python,
        &tts_neural_local_v1,
        &tts_voice_preserving_local_v1,
        &models,
        &diagnostics_trace_dir,
        &jobs,
    );

    Ok(DiagnosticsAppStateSnapshot {
        generated_at_ms: now_epoch_ms_i64(),
        app,
        startup,
        download_roots,
        diagnostics_trace_dir,
        ffmpeg,
        ytdlp,
        js_runtime,
        python,
        portable_python,
        spleeter,
        demucs,
        diarization,
        tts_preview,
        tts_neural_local_v1,
        tts_voice_preserving_local_v1,
        voice_backend_catalog,
        voice_backend_recommendation,
        voice_backend_adapter_count,
        models,
        performance_tier,
        batch_on_import_rules,
        optional_diarization_backend,
        storage,
        thumbnail_cache,
        jobs,
        jobs_tracks,
        library,
        recent_trace,
        feature_health,
    })
}

fn current_startup_status(state: &AppState) -> Result<StartupStatus, String> {
    let startup = state
        .startup
        .lock()
        .map_err(|_| "startup status lock poisoned".to_string())?;
    Ok(StartupStatus {
        offline_bundle_state: startup.offline_bundle_state.clone(),
        offline_bundle_started_at_ms: startup.offline_bundle_started_at_ms,
        offline_bundle_finished_at_ms: startup.offline_bundle_finished_at_ms,
        offline_bundle_error: startup.offline_bundle_error.clone(),
        progress_pct: startup.progress_pct,
        active_phase_id: startup.active_phase_id.clone(),
        phases: startup.phases.clone(),
    })
}

#[tauri::command]
fn diagnostics_info(app: tauri::AppHandle, state: State<'_, AppState>) -> DiagnosticsInfo {
    let package = app.package_info();
    DiagnosticsInfo {
        app_data_dir: state.paths.base_dir.to_string_lossy().to_string(),
        db_path: state
            .paths
            .db_dir()
            .join("app.sqlite")
            .to_string_lossy()
            .to_string(),
        app_name: package.name.to_string(),
        app_version: package.version.to_string(),
        engine_version: diagnostics::engine_version().to_string(),
    }
}

fn localization_job_type_label(job_type: &str) -> &'static str {
    match job_type {
        "import_local" => "Import local media",
        "asr_local" => "Speech recognition",
        "translate_local" => "Translate to English",
        "diarize_local_v1" => "Label speakers",
        "dub_voice_preserving_v1" => "Dub speech generation",
        "tts_preview_pyttsx3_v1" | "tts_neural_local_v1" => "TTS preview",
        "mix_dub_preview_v1" => "Mix dub",
        "mux_dub_preview_v1" => "Mux preview",
        "export_pack_v1" => "Export pack",
        "qc_report_v1" => "QC report",
        _ => "Localization job",
    }
}

fn job_status_label(status: &jobs::JobStatus) -> &'static str {
    match status {
        jobs::JobStatus::Queued => "queued",
        jobs::JobStatus::Running => "running",
        jobs::JobStatus::Succeeded => "succeeded",
        jobs::JobStatus::Failed => "failed",
        jobs::JobStatus::Canceled => "canceled",
    }
}

fn is_english_lang_tag(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "en" | "eng" | "en-us" | "en-gb"
    )
}

#[derive(Debug, Default)]
struct TrackAvailabilitySummary {
    track_count: usize,
    usable_segment_count: usize,
    speaker_count: usize,
    latest_track_path: Option<String>,
}

fn summarize_tracks_for_outputs(
    _paths: &AppPaths,
    tracks: &[subtitle_tracks::SubtitleTrackRow],
    include: impl Fn(&subtitle_tracks::SubtitleTrackRow) -> bool,
) -> TrackAvailabilitySummary {
    let mut summary = TrackAvailabilitySummary::default();
    let mut latest_version = i64::MIN;
    let mut speakers = std::collections::BTreeSet::<String>::new();

    for track in tracks.iter().filter(|track| include(track)) {
        summary.track_count += 1;
        if track.version >= latest_version {
            latest_version = track.version;
            summary.latest_track_path = Some(track.path.clone());
        }
        if let Ok(doc) = subtitle_tracks::load_document_from_path(std::path::Path::new(&track.path))
        {
            summary.usable_segment_count += subtitles::usable_segment_count(&doc);
            for speaker in doc
                .segments
                .iter()
                .filter_map(|segment| segment.speaker.as_deref())
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                speakers.insert(speaker.to_string());
            }
        }
    }

    summary.speaker_count = speakers.len();
    summary
}

fn latest_mix_job_used_source_audio_fallback(jobs: &[jobs::JobRow]) -> bool {
    let latest_mix = jobs
        .iter()
        .filter(|job| {
            job.job_type == "mix_dub_preview_v1" && matches!(job.status, jobs::JobStatus::Succeeded)
        })
        .max_by_key(|job| job.finished_at_ms.unwrap_or(job.created_at_ms));
    let Some(job) = latest_mix else {
        return false;
    };
    let log_path = job.logs_path.trim();
    if log_path.is_empty() {
        return false;
    }
    std::fs::read_to_string(log_path)
        .map(|text| text.contains("source_audio_fallback"))
        .unwrap_or(false)
}

#[allow(clippy::too_many_arguments)]
fn localization_terminal_outcome(
    jobs: &[jobs::JobRow],
    source: &TrackAvailabilitySummary,
    translated_en: &TrackAvailabilitySummary,
    mix_exists: bool,
    source_audio_fallback_mix: bool,
    mux_mp4_path: &std::path::Path,
    mux_mp4_exists: bool,
    mux_mkv_path: &std::path::Path,
    mux_mkv_exists: bool,
    export_pack_path: &std::path::Path,
    export_pack_exists: bool,
    derived_item_dir: &std::path::Path,
) -> (
    String,
    String,
    String,
    Option<String>,
    Option<f32>,
    Option<String>,
    Option<String>,
    bool,
) {
    let active = jobs.iter().find(|job| {
        matches!(
            job.status,
            jobs::JobStatus::Running | jobs::JobStatus::Queued
        )
    });
    if let Some(job) = active {
        let label = localization_job_type_label(&job.job_type).to_string();
        let status = job_status_label(&job.status);
        return (
            "running".to_string(),
            format!(
                "{} {}%",
                label,
                ((job.progress).clamp(0.0, 1.0) * 100.0).round() as i64
            ),
            format!(
                "{label} is {status}. Working folder: {}",
                derived_item_dir.to_string_lossy()
            ),
            Some(label),
            Some(job.progress.clamp(0.0, 1.0)),
            None,
            None,
            false,
        );
    }

    if source_audio_fallback_mix && (mix_exists || mux_mp4_exists || mux_mkv_exists) {
        return (
            "dub_needs_separation".to_string(),
            "Needs clean background separation".to_string(),
            "The existing dub preview was mixed over the original source audio. Run source separation so the English dub is mixed over a clean background instead of the Korean voice track.".to_string(),
            Some("Separate background".to_string()),
            None,
            None,
            None,
            false,
        );
    }

    let deliverable = if export_pack_exists {
        Some(("Export pack ready", export_pack_path))
    } else if mux_mkv_exists {
        Some(("Preview MKV ready", mux_mkv_path))
    } else if mux_mp4_exists {
        // Historical compatibility only. New mux jobs and exports are MKV-only.
        Some(("Legacy preview MP4 ready", mux_mp4_path))
    } else {
        None
    };
    if let Some((summary, path)) = deliverable {
        return (
            if export_pack_exists {
                "export_ready"
            } else {
                "preview_ready"
            }
            .to_string(),
            summary.to_string(),
            path.to_string_lossy().to_string(),
            Some(summary.to_string()),
            Some(1.0),
            None,
            Some(path.to_string_lossy().to_string()),
            true,
        );
    }

    let failed = jobs
        .iter()
        .find(|job| matches!(job.status, jobs::JobStatus::Failed));
    if let Some(job) = failed {
        let label = localization_job_type_label(&job.job_type).to_string();
        let detail = job
            .error
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "No error detail recorded.".to_string());
        return (
            "failed".to_string(),
            format!("Failed before deliverable: {label}"),
            detail.clone(),
            Some(label),
            None,
            Some(detail),
            None,
            false,
        );
    }

    if mix_exists {
        return (
            "dub_audio_ready".to_string(),
            "Dub audio ready".to_string(),
            "Dub mix exists, but no muxed preview video or export pack exists yet.".to_string(),
            Some("Mix dub".to_string()),
            Some(1.0),
            None,
            None,
            false,
        );
    }
    if translated_en.usable_segment_count > 0 && translated_en.speaker_count > 0 {
        return (
            "speaker_labels_ready".to_string(),
            "Translation and speaker labels ready".to_string(),
            format!(
                "{} usable English segment(s), {} speaker label(s). No dub preview deliverable exists yet.",
                translated_en.usable_segment_count, translated_en.speaker_count
            ),
            Some("Label speakers".to_string()),
            Some(1.0),
            None,
            None,
            false,
        );
    }
    if translated_en.usable_segment_count > 0 {
        return (
            "translation_ready".to_string(),
            "Translation ready".to_string(),
            format!(
                "{} usable English segment(s). Speaker labeling and dub stages have not produced a preview yet.",
                translated_en.usable_segment_count
            ),
            Some("Translate to English".to_string()),
            Some(1.0),
            None,
            None,
            false,
        );
    }
    if source.usable_segment_count > 0 {
        return (
            "captions_ready".to_string(),
            "Captions ready".to_string(),
            format!(
                "{} usable source caption segment(s). Translation has not produced English deliverables yet.",
                source.usable_segment_count
            ),
            Some("Speech recognition".to_string()),
            Some(1.0),
            None,
            None,
            false,
        );
    }

    (
        "imported_only".to_string(),
        "Imported only".to_string(),
        format!(
            "The source is in the Localization workspace. No caption, translation, preview, or export artifact exists yet. Working folder: {}",
            derived_item_dir.to_string_lossy()
        ),
        Some("Ready to start".to_string()),
        None,
        None,
        None,
        false,
    )
}

fn localization_preview_consumer_path(
    paths: &AppPaths,
    item_id: &str,
    variant_label: Option<&str>,
    legacy_fixed_path: std::path::PathBuf,
) -> Result<std::path::PathBuf, String> {
    match jobs::localization_preview_consumer_outcome(paths, item_id, variant_label) {
        jobs::LocalizationPreviewConsumerOutcome::Active(path) => Ok(path),
        // Only the canonical absence of an active publication authorizes compatibility with
        // fixed-path MKVs produced by older builds. Integrity/path/hash failures are not absence.
        jobs::LocalizationPreviewConsumerOutcome::CanonicalAbsence => Ok(legacy_fixed_path),
        jobs::LocalizationPreviewConsumerOutcome::LineageFailure(error) => Err(format!(
            "Active localization preview failed immutable-lineage verification: {error}"
        )),
    }
}

fn build_item_outputs(paths: &AppPaths, item_id: &str) -> Result<ItemOutputs, String> {
    let item_id = item_id.trim().to_string();
    if item_id.is_empty() {
        return Err("missing required key itemId".to_string());
    }

    let item = library::get_item_by_id(paths, &item_id).map_err(|e| e.to_string())?;
    let item_dir = paths.derived_item_dir(&item_id);
    let dub_preview_dir = item_dir.join("dub_preview");
    let mix_path = dub_preview_dir.join("mix_dub_preview_v1.wav");
    let mux_mp4_path = dub_preview_dir.join("mux_dub_preview_v1.mp4");
    let mux_mkv_path = localization_preview_consumer_path(
        paths,
        &item_id,
        None,
        dub_preview_dir.join("mux_dub_preview_v1.mkv"),
    )?;
    let export_pack_path = item_dir.join("exports").join("export_pack_v1.zip");
    let tracks = subtitle_tracks::list_tracks(paths, &item_id).unwrap_or_default();
    let source_summary =
        summarize_tracks_for_outputs(paths, &tracks, |track| track.kind == "source");
    let translated_en_summary = summarize_tracks_for_outputs(paths, &tracks, |track| {
        track.kind == "translated" && is_english_lang_tag(&track.lang)
    });
    let item_jobs = jobs::list_jobs_for_item(paths, &item_id, 80, 0).unwrap_or_default();
    let mix_exists = mix_path.exists();
    let mux_mp4_exists = mux_mp4_path.exists();
    let mux_mkv_exists = mux_mkv_path.exists();
    let export_pack_exists = export_pack_path.exists();
    let source_audio_fallback_mix = latest_mix_job_used_source_audio_fallback(&item_jobs);
    let (
        terminal_state,
        terminal_summary,
        terminal_detail,
        terminal_stage_label,
        terminal_progress,
        terminal_error,
        deliverable_path,
        deliverable_exists,
    ) = localization_terminal_outcome(
        &item_jobs,
        &source_summary,
        &translated_en_summary,
        mix_exists,
        source_audio_fallback_mix,
        &mux_mp4_path,
        mux_mp4_exists,
        &mux_mkv_path,
        mux_mkv_exists,
        &export_pack_path,
        export_pack_exists,
        &item_dir,
    );
    let recent_jobs = item_jobs.iter().take(40).cloned().collect();

    let resolved_source_media = library::resolve_media_path(paths, &item.media_path)
        .unwrap_or_else(|_| std::path::PathBuf::from(&item.media_path));
    Ok(ItemOutputs {
        item_id,
        source_media_path: resolved_source_media.to_string_lossy().to_string(),
        source_media_exists: resolved_source_media.is_file(),
        derived_item_dir: item_dir.to_string_lossy().to_string(),
        dub_preview_dir: dub_preview_dir.to_string_lossy().to_string(),
        source_track_count: source_summary.track_count,
        source_usable_segment_count: source_summary.usable_segment_count,
        latest_source_track_path: source_summary.latest_track_path,
        translated_en_track_count: translated_en_summary.track_count,
        translated_en_usable_segment_count: translated_en_summary.usable_segment_count,
        translated_en_speaker_count: translated_en_summary.speaker_count,
        latest_translated_en_track_path: translated_en_summary.latest_track_path,
        mix_dub_preview_v1_wav_path: mix_path.to_string_lossy().to_string(),
        mix_dub_preview_v1_wav_exists: mix_exists,
        mux_dub_preview_v1_mp4_path: mux_mp4_path.to_string_lossy().to_string(),
        mux_dub_preview_v1_mp4_exists: mux_mp4_exists,
        mux_dub_preview_v1_mkv_path: mux_mkv_path.to_string_lossy().to_string(),
        mux_dub_preview_v1_mkv_exists: mux_mkv_exists,
        export_pack_v1_zip_path: export_pack_path.to_string_lossy().to_string(),
        export_pack_v1_zip_exists: export_pack_exists,
        terminal_state,
        terminal_summary,
        terminal_detail,
        terminal_stage_label,
        terminal_progress,
        terminal_error,
        deliverable_path,
        deliverable_exists,
        recent_jobs,
    })
}

fn job_status_from_db(value: &str) -> jobs::JobStatus {
    match value {
        "queued" => jobs::JobStatus::Queued,
        "running" => jobs::JobStatus::Running,
        "succeeded" => jobs::JobStatus::Succeeded,
        "canceled" => jobs::JobStatus::Canceled,
        _ => jobs::JobStatus::Failed,
    }
}

fn query_localization_home_jobs_by_item(
    conn: &rusqlite::Connection,
    item_ids: &[String],
) -> Result<std::collections::BTreeMap<String, Vec<jobs::JobRow>>, String> {
    if item_ids.is_empty() {
        return Ok(std::collections::BTreeMap::new());
    }
    let placeholders = std::iter::repeat("?")
        .take(item_ids.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        r#"
SELECT
  id,
  item_id,
  batch_id,
  type,
  status,
  progress,
  error,
  created_at_ms,
  started_at_ms,
  finished_at_ms,
  logs_path,
  params_json,
  track
FROM job
WHERE item_id IN ({placeholders})
ORDER BY item_id ASC, created_at_ms DESC
"#
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(
            rusqlite::params_from_iter(item_ids.iter().map(String::as_str)),
            |row| {
                let status_str: String = row.get(4)?;
                let persisted_track: Option<String> = row.get(12)?;
                Ok(jobs::JobRow {
                    id: row.get(0)?,
                    item_id: row.get(1)?,
                    batch_id: row.get(2)?,
                    job_type: row.get(3)?,
                    status: job_status_from_db(&status_str),
                    progress: row.get(5)?,
                    error: row.get(6)?,
                    created_at_ms: row.get(7)?,
                    started_at_ms: row.get(8)?,
                    finished_at_ms: row.get(9)?,
                    logs_path: row.get(10)?,
                    params_json: row.get(11)?,
                    target_title: None,
                    retry_of_job_id: None,
                    retry_replacement_job_id: None,
                    track: jobs::durable_job_track_label(persisted_track.as_deref()),
                })
            },
        )
        .map_err(|e| e.to_string())?;

    let mut by_item = std::collections::BTreeMap::<String, Vec<jobs::JobRow>>::new();
    for row in rows {
        let job = row.map_err(|e| e.to_string())?;
        let Some(item_id) = job.item_id.clone() else {
            continue;
        };
        let entry = by_item.entry(item_id).or_default();
        if entry.len() < 80 {
            entry.push(job);
        }
    }
    Ok(by_item)
}

fn query_localization_home_tracks_by_item(
    conn: &rusqlite::Connection,
    item_ids: &[String],
) -> Result<std::collections::BTreeMap<String, Vec<subtitle_tracks::SubtitleTrackRow>>, String> {
    if item_ids.is_empty() {
        return Ok(std::collections::BTreeMap::new());
    }
    let placeholders = std::iter::repeat("?")
        .take(item_ids.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        r#"
SELECT
  id,
  item_id,
  kind,
  lang,
  format,
  path,
  created_by,
  version
FROM subtitle_track
WHERE item_id IN ({placeholders})
ORDER BY item_id ASC, kind ASC, lang ASC, version DESC
"#
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(
            rusqlite::params_from_iter(item_ids.iter().map(String::as_str)),
            |row| {
                Ok(subtitle_tracks::SubtitleTrackRow {
                    id: row.get(0)?,
                    item_id: row.get(1)?,
                    kind: row.get(2)?,
                    lang: row.get(3)?,
                    format: row.get(4)?,
                    path: row.get(5)?,
                    created_by: row.get(6)?,
                    version: row.get(7)?,
                })
            },
        )
        .map_err(|e| e.to_string())?;

    let mut by_item =
        std::collections::BTreeMap::<String, Vec<subtitle_tracks::SubtitleTrackRow>>::new();
    for row in rows {
        let track = row.map_err(|e| e.to_string())?;
        by_item
            .entry(track.item_id.clone())
            .or_default()
            .push(track);
    }
    Ok(by_item)
}

fn build_localization_home_item_outputs(
    paths: &AppPaths,
    item_id: &str,
    tracks: &[subtitle_tracks::SubtitleTrackRow],
    item_jobs: &[jobs::JobRow],
) -> Result<ItemOutputs, String> {
    let source_media = library::get_item_by_id(paths, item_id)
        .ok()
        .and_then(|item| library::resolve_media_path(paths, &item.media_path).ok());
    let item_dir = paths.derived_item_dir(item_id);
    let dub_preview_dir = item_dir.join("dub_preview");
    let mix_path = dub_preview_dir.join("mix_dub_preview_v1.wav");
    let mux_mp4_path = dub_preview_dir.join("mux_dub_preview_v1.mp4");
    let mux_mkv_path = localization_preview_consumer_path(
        paths,
        item_id,
        None,
        dub_preview_dir.join("mux_dub_preview_v1.mkv"),
    )?;
    let export_pack_path = item_dir.join("exports").join("export_pack_v1.zip");
    let source_summary =
        summarize_tracks_for_outputs(paths, tracks, |track| track.kind == "source");
    let translated_en_summary = summarize_tracks_for_outputs(paths, tracks, |track| {
        track.kind == "translated" && is_english_lang_tag(&track.lang)
    });
    let item_jobs = item_jobs.iter().take(80).cloned().collect::<Vec<_>>();
    let mix_exists = mix_path.exists();
    let mux_mp4_exists = mux_mp4_path.exists();
    let mux_mkv_exists = mux_mkv_path.exists();
    let export_pack_exists = export_pack_path.exists();
    let source_audio_fallback_mix = latest_mix_job_used_source_audio_fallback(&item_jobs);
    let (
        terminal_state,
        terminal_summary,
        terminal_detail,
        terminal_stage_label,
        terminal_progress,
        terminal_error,
        deliverable_path,
        deliverable_exists,
    ) = localization_terminal_outcome(
        &item_jobs,
        &source_summary,
        &translated_en_summary,
        mix_exists,
        source_audio_fallback_mix,
        &mux_mp4_path,
        mux_mp4_exists,
        &mux_mkv_path,
        mux_mkv_exists,
        &export_pack_path,
        export_pack_exists,
        &item_dir,
    );

    Ok(ItemOutputs {
        item_id: item_id.to_string(),
        source_media_path: source_media
            .as_ref()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_default(),
        source_media_exists: source_media.as_ref().is_some_and(|path| path.is_file()),
        derived_item_dir: item_dir.to_string_lossy().to_string(),
        dub_preview_dir: dub_preview_dir.to_string_lossy().to_string(),
        source_track_count: source_summary.track_count,
        source_usable_segment_count: source_summary.usable_segment_count,
        latest_source_track_path: source_summary.latest_track_path,
        translated_en_track_count: translated_en_summary.track_count,
        translated_en_usable_segment_count: translated_en_summary.usable_segment_count,
        translated_en_speaker_count: translated_en_summary.speaker_count,
        latest_translated_en_track_path: translated_en_summary.latest_track_path,
        mix_dub_preview_v1_wav_path: mix_path.to_string_lossy().to_string(),
        mix_dub_preview_v1_wav_exists: mix_exists,
        mux_dub_preview_v1_mp4_path: mux_mp4_path.to_string_lossy().to_string(),
        mux_dub_preview_v1_mp4_exists: mux_mp4_exists,
        mux_dub_preview_v1_mkv_path: mux_mkv_path.to_string_lossy().to_string(),
        mux_dub_preview_v1_mkv_exists: mux_mkv_exists,
        export_pack_v1_zip_path: export_pack_path.to_string_lossy().to_string(),
        export_pack_v1_zip_exists: export_pack_exists,
        terminal_state,
        terminal_summary,
        terminal_detail,
        terminal_stage_label,
        terminal_progress,
        terminal_error,
        deliverable_path,
        deliverable_exists,
        recent_jobs: item_jobs.iter().take(40).cloned().collect(),
    })
}

#[tauri::command]
#[allow(non_snake_case)]
async fn item_outputs(
    state: State<'_, AppState>,
    item_id: Option<String>,
    itemId: Option<String>,
) -> Result<ItemOutputs, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "item_outputs");
    let item_id = item_id
        .or(itemId)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| "missing required key itemId".to_string())?;
    let paths = state.paths.clone();
    let trace_paths = paths.clone();
    let result = tauri::async_runtime::spawn_blocking(move || build_item_outputs(&paths, &item_id))
        .await
        .map_err(|e| e.to_string())?;
    result.map_err(|e| trace_database_command_error(&trace_paths, "item_outputs", e))
}

#[tauri::command]
#[allow(non_snake_case)]
async fn localization_home_item_outputs(
    state: State<'_, AppState>,
    item_ids: Option<Vec<String>>,
    itemIds: Option<Vec<String>>,
) -> Result<Vec<ItemOutputs>, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "localization_home_item_outputs");
    let item_ids: Vec<String> = item_ids
        .or(itemIds)
        .unwrap_or_default()
        .into_iter()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .take(40)
        .collect();
    if item_ids.is_empty() {
        return Ok(Vec::new());
    }
    let paths = state.paths.clone();
    let trace_paths = paths.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let conn = db::open_readonly(&paths).map_err(|e| e.to_string())?;
        let jobs_by_item = query_localization_home_jobs_by_item(&conn, &item_ids)?;
        let tracks_by_item = query_localization_home_tracks_by_item(&conn, &item_ids)?;
        let outputs = item_ids
            .iter()
            .map(|item_id| {
                let jobs = jobs_by_item.get(item_id).map(Vec::as_slice).unwrap_or(&[]);
                let tracks = tracks_by_item
                    .get(item_id)
                    .map(Vec::as_slice)
                    .unwrap_or(&[]);
                build_localization_home_item_outputs(&paths, item_id, tracks, jobs)
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(outputs)
    })
    .await
    .map_err(|e| e.to_string())?;
    result.map_err(|e| {
        trace_database_command_error(&trace_paths, "localization_home_item_outputs", e)
    })
}

// WP-0245: batched read for the Jobs page and other panels that previously
// fanned out per-item `library_get` invokes from a `Promise.all`. One Tauri
// dispatch, one read-only connection, one `SELECT … WHERE id IN (...)`.
// Missing ids are silently skipped; order is not guaranteed to match input
// (callers index by `LibraryItem.id`).
#[tauri::command]
#[allow(non_snake_case)]
async fn library_get_many(
    state: State<'_, AppState>,
    item_ids: Option<Vec<String>>,
    itemIds: Option<Vec<String>>,
) -> Result<Vec<library::LibraryItem>, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "library_get_many");
    let ids: Vec<String> = item_ids
        .or(itemIds)
        .unwrap_or_default()
        .into_iter()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .take(200)
        .collect();
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let paths = state.paths.clone();
    let trace_paths = paths.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let id_refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        library::list_items_by_ids(&paths, &id_refs).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?;
    result.map_err(|e| trace_database_command_error(&trace_paths, "library_get_many", e))
}

// WP-0245: batched equivalent of `item_outputs` for panels (Jobs, Library)
// that previously fanned out one Tauri dispatch per visible item. Uses one
// read-only SQLite connection and bounded batch queries for the whole request.
// Order of returned vec matches input order.
#[tauri::command]
#[allow(non_snake_case)]
async fn item_outputs_many(
    state: State<'_, AppState>,
    item_ids: Option<Vec<String>>,
    itemIds: Option<Vec<String>>,
) -> Result<Vec<ItemOutputs>, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "item_outputs_many");
    let ids: Vec<String> = item_ids
        .or(itemIds)
        .unwrap_or_default()
        .into_iter()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .take(200)
        .collect();
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let paths = state.paths.clone();
    let trace_paths = paths.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let conn = db::open_readonly(&paths).map_err(|e| e.to_string())?;
        let jobs_by_item = query_localization_home_jobs_by_item(&conn, &ids)?;
        let tracks_by_item = query_localization_home_tracks_by_item(&conn, &ids)?;
        let outputs = ids
            .iter()
            .map(|item_id| {
                let jobs = jobs_by_item.get(item_id).map(Vec::as_slice).unwrap_or(&[]);
                let tracks = tracks_by_item
                    .get(item_id)
                    .map(Vec::as_slice)
                    .unwrap_or(&[]);
                build_localization_home_item_outputs(&paths, item_id, tracks, jobs)
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok::<Vec<ItemOutputs>, String>(outputs)
    })
    .await
    .map_err(|e| e.to_string())?;
    result.map_err(|e| trace_database_command_error(&trace_paths, "item_outputs_many", e))
}

#[tauri::command]
async fn library_thumbnail_data_url(
    state: State<'_, AppState>,
    item_id: Option<String>,
    itemId: Option<String>,
) -> Result<Option<String>, String> {
    let paths = state.paths.clone();
    let item_id = item_id
        .or(itemId)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| "missing required key itemId".to_string())?;

    tauri::async_runtime::spawn_blocking(move || {
        let Some(path) =
            library::ensure_thumbnail_path(&paths, &item_id).map_err(|e| e.to_string())?
        else {
            return Ok(None);
        };
        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
        let mime = mime_from_path(&path);
        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
        Ok(Some(format!("data:{mime};base64,{encoded}")))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
#[allow(non_snake_case)]
fn item_qc_report_v1_load(
    state: State<'_, AppState>,
    item_id: Option<String>,
    itemId: Option<String>,
    track_id: Option<String>,
    trackId: Option<String>,
    variant_label: Option<String>,
    variantLabel: Option<String>,
) -> Result<Option<serde_json::Value>, String> {
    let item_id = item_id
        .or(itemId)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| "missing required key itemId".to_string())?;
    let track_id = track_id
        .or(trackId)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| "missing required key trackId".to_string())?;

    let variant_label = variant_label
        .or(variantLabel)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let file_name = match variant_label.as_deref() {
        Some(label) => format!("qc_report_v1_{track_id}_{label}.json"),
        None => format!("qc_report_v1_{track_id}.json"),
    };
    let path = state
        .paths
        .derived_item_dir(&item_id)
        .join("qc")
        .join(file_name);
    if !path.exists() {
        return Ok(None);
    }

    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    let parsed: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    Ok(Some(parsed))
}

fn normalize_variant_label(raw: Option<&str>) -> Option<String> {
    let value = raw?.trim();
    if value.is_empty() {
        return None;
    }
    let mut out = String::new();
    let mut prev_underscore = false;
    for ch in value.chars() {
        let mapped = if ch.is_ascii_alphanumeric() {
            ch.to_ascii_lowercase()
        } else {
            '_'
        };
        if mapped == '_' {
            if prev_underscore {
                continue;
            }
            prev_underscore = true;
        } else {
            prev_underscore = false;
        }
        out.push(mapped);
    }
    let trimmed = out.trim_matches('_').to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn load_artifact_voice_clone_meta(
    kind: &ArtifactKind,
    path: &std::path::Path,
) -> Option<ArtifactVoiceCloneMeta> {
    if !matches!(kind, ArtifactKind::TtsManifest) || !path.exists() {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice::<ArtifactVoiceCloneMeta>(&bytes).ok()
}

fn qc_report_identity(file_name: &str) -> (Option<String>, Option<String>) {
    let Some(stem) = file_name.strip_suffix(".json") else {
        return (None, None);
    };
    let Some(rest) = stem.strip_prefix("qc_report_v1_") else {
        return (None, None);
    };
    let mut parts = rest.splitn(2, '_');
    let track_id = parts.next().map(|value| value.trim().to_string());
    let variant_label = normalize_variant_label(parts.next());
    (track_id.filter(|value| !value.is_empty()), variant_label)
}

#[tauri::command]
#[allow(non_snake_case)]
fn item_artifacts_list_v1(
    state: State<'_, AppState>,
    item_id: Option<String>,
    itemId: Option<String>,
) -> Result<Vec<ArtifactInfo>, String> {
    let item_id = item_id
        .or(itemId)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| "missing required key itemId".to_string())?;

    let item_dir = state.paths.derived_item_dir(&item_id);
    let mut out: Vec<ArtifactInfo> = Vec::new();

    let mut push = |id: &str,
                    title: &str,
                    group: &str,
                    kind: ArtifactKind,
                    job_type: Option<&str>,
                    variant_label: Option<String>,
                    track_id: Option<String>,
                    mux_container: Option<&str>,
                    tts_backend_id: Option<&str>,
                    rerun_kind: Option<ArtifactRerunKind>,
                    path: std::path::PathBuf| {
        let voice_clone_meta = load_artifact_voice_clone_meta(&kind, &path);
        out.push(ArtifactInfo {
            id: id.to_string(),
            title: title.to_string(),
            path: path.to_string_lossy().to_string(),
            exists: path.exists(),
            group: group.to_string(),
            kind,
            job_type: job_type.map(|value| value.to_string()),
            variant_label,
            track_id,
            mux_container: mux_container.map(|value| value.to_string()),
            tts_backend_id: tts_backend_id.map(|value| value.to_string()),
            voice_clone_outcome: voice_clone_meta
                .as_ref()
                .and_then(|value| value.voice_clone_outcome.clone()),
            voice_clone_requested_segments: voice_clone_meta
                .as_ref()
                .and_then(|value| value.voice_clone_requested_segments),
            voice_clone_converted_segments: voice_clone_meta
                .as_ref()
                .and_then(|value| value.voice_clone_converted_segments),
            voice_clone_fallback_segments: voice_clone_meta
                .as_ref()
                .and_then(|value| value.voice_clone_fallback_segments),
            voice_clone_standard_tts_segments: voice_clone_meta
                .as_ref()
                .and_then(|value| value.voice_clone_standard_tts_segments),
            rerun_kind,
        });
    };

    // Separation
    push(
        "sep_spleeter_vocals",
        "Vocals (Spleeter)",
        "Separation",
        ArtifactKind::SeparationStem,
        Some("separate_audio_spleeter"),
        None,
        None,
        None,
        None,
        Some(ArtifactRerunKind::SeparateSpleeter),
        item_dir
            .join("separation")
            .join("spleeter_2stems")
            .join("vocals.wav"),
    );
    push(
        "sep_spleeter_background",
        "Background (Spleeter)",
        "Separation",
        ArtifactKind::SeparationStem,
        Some("separate_audio_spleeter"),
        None,
        None,
        None,
        None,
        Some(ArtifactRerunKind::SeparateSpleeter),
        item_dir
            .join("separation")
            .join("spleeter_2stems")
            .join("background.wav"),
    );
    push(
        "sep_demucs_vocals",
        "Vocals (Demucs)",
        "Separation",
        ArtifactKind::SeparationStem,
        Some("separate_audio_demucs_v1"),
        None,
        None,
        None,
        None,
        Some(ArtifactRerunKind::SeparateDemucs),
        item_dir
            .join("separation")
            .join("demucs_two_stems_v1")
            .join("vocals.wav"),
    );
    push(
        "sep_demucs_background",
        "Background (Demucs)",
        "Separation",
        ArtifactKind::SeparationStem,
        Some("separate_audio_demucs_v1"),
        None,
        None,
        None,
        None,
        Some(ArtifactRerunKind::SeparateDemucs),
        item_dir
            .join("separation")
            .join("demucs_two_stems_v1")
            .join("background.wav"),
    );

    // Vocals cleanup
    push(
        "cleanup_vocals",
        "Vocals cleaned",
        "Cleanup",
        ArtifactKind::CleanupAudio,
        Some("clean_vocals_v1"),
        None,
        None,
        None,
        None,
        Some(ArtifactRerunKind::CleanVocals),
        item_dir.join("cleanup").join("vocals_clean_v1.wav"),
    );

    // TTS manifests
    push(
        "tts_pyttsx3_manifest",
        "TTS manifest (pyttsx3)",
        "TTS",
        ArtifactKind::TtsManifest,
        Some("tts_preview_pyttsx3_v1"),
        None,
        None,
        None,
        Some("pyttsx3_v1"),
        Some(ArtifactRerunKind::TtsPyttsx3),
        item_dir
            .join("tts_preview")
            .join("pyttsx3_v1")
            .join("manifest.json"),
    );
    push(
        "tts_neural_manifest",
        "TTS manifest (neural local v1)",
        "TTS",
        ArtifactKind::TtsManifest,
        Some("tts_neural_local_v1"),
        None,
        None,
        None,
        Some("tts_neural_local_v1"),
        Some(ArtifactRerunKind::TtsNeuralLocalV1),
        item_dir
            .join("tts_preview")
            .join("tts_neural_local_v1")
            .join("manifest.json"),
    );
    push(
        "tts_voice_preserving_manifest",
        "TTS manifest (voice-preserving)",
        "TTS",
        ArtifactKind::TtsManifest,
        Some("dub_voice_preserving_v1"),
        None,
        None,
        None,
        Some("openvoice_v2"),
        Some(ArtifactRerunKind::DubVoicePreservingV1),
        item_dir
            .join("tts_preview")
            .join("dub_voice_preserving_v1")
            .join("manifest.json"),
    );
    let voice_preserving_variants_dir = item_dir
        .join("tts_preview")
        .join("dub_voice_preserving_v1")
        .join("variants");
    if let Ok(entries) = std::fs::read_dir(&voice_preserving_variants_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(label) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            push(
                &format!("tts_voice_preserving_manifest_variant_{label}"),
                &format!("TTS manifest (voice-preserving {label})"),
                "TTS alternates",
                ArtifactKind::TtsManifest,
                Some("dub_voice_preserving_v1"),
                normalize_variant_label(Some(label)),
                None,
                None,
                Some("openvoice_v2"),
                Some(ArtifactRerunKind::DubVoicePreservingV1),
                path.join("manifest.json"),
            );
        }
    }
    let tts_root = item_dir.join("tts_preview");
    if let Ok(entries) = std::fs::read_dir(&tts_root) {
        for entry in entries.flatten() {
            let backend_dir = entry.path();
            if !backend_dir.is_dir() {
                continue;
            }
            let Some(backend_id) = backend_dir.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if matches!(
                backend_id,
                "pyttsx3_v1" | "tts_neural_local_v1" | "dub_voice_preserving_v1"
            ) {
                continue;
            }

            push(
                &format!("tts_manifest_backend_{backend_id}"),
                &format!("TTS manifest ({backend_id})"),
                "TTS experiments",
                ArtifactKind::TtsManifest,
                Some("experimental_voice_backend_render_v1"),
                None,
                None,
                None,
                Some(backend_id),
                Some(ArtifactRerunKind::ExperimentalVoiceBackendRenderV1),
                backend_dir.join("manifest.json"),
            );
            push(
                &format!("tts_request_backend_{backend_id}"),
                &format!("TTS request ({backend_id})"),
                "TTS experiments",
                ArtifactKind::TtsRequest,
                Some("experimental_voice_backend_render_v1"),
                None,
                None,
                None,
                Some(backend_id),
                Some(ArtifactRerunKind::ExperimentalVoiceBackendRenderV1),
                backend_dir.join("request.json"),
            );
            push(
                &format!("tts_report_backend_{backend_id}"),
                &format!("TTS report ({backend_id})"),
                "TTS experiments",
                ArtifactKind::TtsReport,
                Some("experimental_voice_backend_render_v1"),
                None,
                None,
                None,
                Some(backend_id),
                Some(ArtifactRerunKind::ExperimentalVoiceBackendRenderV1),
                backend_dir.join("report.json"),
            );

            let variants_dir = backend_dir.join("variants");
            let Ok(variant_entries) = std::fs::read_dir(&variants_dir) else {
                continue;
            };
            for variant_entry in variant_entries.flatten() {
                let variant_path = variant_entry.path();
                if !variant_path.is_dir() {
                    continue;
                }
                let Some(label) = variant_path.file_name().and_then(|value| value.to_str()) else {
                    continue;
                };
                push(
                    &format!("tts_manifest_backend_{backend_id}_variant_{label}"),
                    &format!("TTS manifest ({backend_id} {label})"),
                    "TTS experiment alternates",
                    ArtifactKind::TtsManifest,
                    Some("experimental_voice_backend_render_v1"),
                    normalize_variant_label(Some(label)),
                    None,
                    None,
                    Some(backend_id),
                    Some(ArtifactRerunKind::ExperimentalVoiceBackendRenderV1),
                    variant_path.join("manifest.json"),
                );
                push(
                    &format!("tts_request_backend_{backend_id}_variant_{label}"),
                    &format!("TTS request ({backend_id} {label})"),
                    "TTS experiment alternates",
                    ArtifactKind::TtsRequest,
                    Some("experimental_voice_backend_render_v1"),
                    normalize_variant_label(Some(label)),
                    None,
                    None,
                    Some(backend_id),
                    Some(ArtifactRerunKind::ExperimentalVoiceBackendRenderV1),
                    variant_path.join("request.json"),
                );
                push(
                    &format!("tts_report_backend_{backend_id}_variant_{label}"),
                    &format!("TTS report ({backend_id} {label})"),
                    "TTS experiment alternates",
                    ArtifactKind::TtsReport,
                    Some("experimental_voice_backend_render_v1"),
                    normalize_variant_label(Some(label)),
                    None,
                    None,
                    Some(backend_id),
                    Some(ArtifactRerunKind::ExperimentalVoiceBackendRenderV1),
                    variant_path.join("report.json"),
                );
            }
        }
    }

    // Dub preview
    push(
        "dub_mix",
        "Mix dub preview (WAV)",
        "Dub preview",
        ArtifactKind::DubMix,
        Some("mix_dub_preview_v1"),
        None,
        None,
        None,
        None,
        Some(ArtifactRerunKind::MixDubPreviewV1),
        item_dir.join("dub_preview").join("mix_dub_preview_v1.wav"),
    );
    push(
        "dub_speech_stem",
        "Speech stem (WAV)",
        "Dub preview",
        ArtifactKind::DubSpeechStem,
        Some("mix_dub_preview_v1"),
        None,
        None,
        None,
        None,
        Some(ArtifactRerunKind::MixDubPreviewV1),
        item_dir
            .join("dub_preview")
            .join("speech_dub_preview_v1.wav"),
    );
    push(
        "dub_mux_mp4",
        "Legacy mux dub preview (MP4)",
        "Dub preview",
        ArtifactKind::DubMux,
        Some("mux_dub_preview_v1"),
        None,
        None,
        Some("mp4"),
        None,
        Some(ArtifactRerunKind::MuxDubPreviewV1),
        item_dir.join("dub_preview").join("mux_dub_preview_v1.mp4"),
    );
    push(
        "dub_mux_mkv",
        "Mux dub preview (MKV)",
        "Dub preview",
        ArtifactKind::DubMux,
        Some("mux_dub_preview_v1"),
        None,
        None,
        Some("mkv"),
        None,
        Some(ArtifactRerunKind::MuxDubPreviewV1),
        localization_preview_consumer_path(
            &state.paths,
            &item_id,
            None,
            item_dir.join("dub_preview").join("mux_dub_preview_v1.mkv"),
        )?,
    );
    let alternate_dir = item_dir.join("dub_preview").join("alternates");
    if let Ok(entries) = std::fs::read_dir(&alternate_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(label) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            push(
                &format!("dub_mix_variant_{label}"),
                &format!("Mix dub preview ({label})"),
                "Dub alternates",
                ArtifactKind::DubMix,
                Some("mix_dub_preview_v1"),
                normalize_variant_label(Some(label)),
                None,
                None,
                None,
                Some(ArtifactRerunKind::MixDubPreviewV1),
                path.join("mix_dub_preview_v1.wav"),
            );
            push(
                &format!("dub_speech_stem_variant_{label}"),
                &format!("Speech stem ({label})"),
                "Dub alternates",
                ArtifactKind::DubSpeechStem,
                Some("mix_dub_preview_v1"),
                normalize_variant_label(Some(label)),
                None,
                None,
                None,
                Some(ArtifactRerunKind::MixDubPreviewV1),
                path.join("speech_dub_preview_v1.wav"),
            );
            push(
                &format!("dub_mux_mp4_variant_{label}"),
                &format!("Legacy mux dub preview MP4 ({label})"),
                "Dub alternates",
                ArtifactKind::DubMux,
                Some("mux_dub_preview_v1"),
                normalize_variant_label(Some(label)),
                None,
                Some("mp4"),
                None,
                Some(ArtifactRerunKind::MuxDubPreviewV1),
                path.join("mux_dub_preview_v1.mp4"),
            );
            push(
                &format!("dub_mux_mkv_variant_{label}"),
                &format!("Mux dub preview MKV ({label})"),
                "Dub alternates",
                ArtifactKind::DubMux,
                Some("mux_dub_preview_v1"),
                normalize_variant_label(Some(label)),
                None,
                Some("mkv"),
                None,
                Some(ArtifactRerunKind::MuxDubPreviewV1),
                localization_preview_consumer_path(
                    &state.paths,
                    &item_id,
                    Some(label),
                    path.join("mux_dub_preview_v1.mkv"),
                )?,
            );
        }
    }

    // Export
    push(
        "export_pack",
        "Export pack (zip)",
        "Export",
        ArtifactKind::ExportPack,
        Some("export_pack_v1"),
        None,
        None,
        None,
        None,
        Some(ArtifactRerunKind::ExportPackV1),
        item_dir.join("exports").join("export_pack_v1.zip"),
    );
    let export_dir = item_dir.join("exports");
    if let Ok(entries) = std::fs::read_dir(&export_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if name == "export_pack_v1.zip" || !name.to_ascii_lowercase().ends_with(".zip") {
                continue;
            }
            push(
                &format!("export_{}", name.replace('.', "_")),
                &format!("Export pack ({name})"),
                "Export alternates",
                ArtifactKind::ExportPack,
                Some("export_pack_v1"),
                normalize_variant_label(Some(
                    name.strip_prefix("export_pack_v1_")
                        .and_then(|value| value.strip_suffix(".zip"))
                        .unwrap_or(""),
                )),
                None,
                None,
                None,
                Some(ArtifactRerunKind::ExportPackV1),
                path,
            );
        }
    }

    let voice_cleanup_dir = item_dir.join("voice").join("cleanup");
    if let Ok(speaker_dirs) = std::fs::read_dir(&voice_cleanup_dir) {
        for speaker_dir in speaker_dirs.flatten() {
            let speaker_path = speaker_dir.path();
            if !speaker_path.is_dir() {
                continue;
            }
            let speaker_label = speaker_path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("speaker");
            if let Ok(cleanups) = std::fs::read_dir(&speaker_path) {
                for cleanup in cleanups.flatten() {
                    let cleanup_path = cleanup.path();
                    if !cleanup_path.is_dir() {
                        continue;
                    }
                    let cleanup_id = cleanup_path
                        .file_name()
                        .and_then(|value| value.to_str())
                        .unwrap_or("cleanup");
                    let manifest_path = cleanup_path.join("manifest.json");
                    let speaker_title = if manifest_path.exists() {
                        std::fs::read(&manifest_path)
                            .ok()
                            .and_then(|bytes| {
                                serde_json::from_slice::<voice_cleanup::VoiceReferenceCleanupRecord>(
                                    &bytes,
                                )
                                .ok()
                            })
                            .map(|manifest| {
                                let label = manifest.speaker_key.trim();
                                if label.is_empty() {
                                    speaker_label.to_string()
                                } else {
                                    label.to_string()
                                }
                            })
                            .unwrap_or_else(|| speaker_label.to_string())
                    } else {
                        speaker_label.to_string()
                    };
                    push(
                        &format!("voice_cleanup_{speaker_label}_{cleanup_id}"),
                        &format!("Voice cleanup {speaker_title} ({cleanup_id})"),
                        "Voice cleanup",
                        ArtifactKind::CleanupAudio,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        cleanup_path.join("cleaned_ref.wav"),
                    );
                    push(
                        &format!("voice_cleanup_manifest_{speaker_label}_{cleanup_id}"),
                        &format!("Voice cleanup manifest {speaker_title} ({cleanup_id})"),
                        "Voice cleanup",
                        ArtifactKind::CleanupManifest,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        manifest_path,
                    );
                }
            }
        }
    }

    // QC reports
    let qc_dir = item_dir.join("qc");
    if qc_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&qc_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name.to_lowercase().ends_with(".json") {
                    push(
                        &format!("qc_{name}"),
                        &format!("QC report ({name})"),
                        "QC",
                        ArtifactKind::QcReport,
                        Some("qc_report_v1"),
                        qc_report_identity(name).1,
                        qc_report_identity(name).0,
                        None,
                        None,
                        None,
                        path,
                    );
                }
            }
        }
    }

    let benchmark_dir = item_dir.join("voice_benchmark");
    if benchmark_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&benchmark_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                    continue;
                };
                let lower = name.to_ascii_lowercase();
                if !(lower.ends_with(".json") || lower.ends_with(".md")) {
                    continue;
                }
                push(
                    &format!("benchmark_{}", name.replace('.', "_")),
                    &format!("Voice benchmark ({name})"),
                    "Benchmark",
                    ArtifactKind::BenchmarkReport,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    path,
                );
            }
        }
    }

    let curation_dir = item_dir.join("voice_reference_curation");
    if curation_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&curation_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                    continue;
                };
                let lower = name.to_ascii_lowercase();
                if !(lower.ends_with(".json") || lower.ends_with(".md")) {
                    continue;
                }
                push(
                    &format!("reference_curation_{}", name.replace('.', "_")),
                    &format!("Reference curation ({name})"),
                    "Reference curation",
                    ArtifactKind::ReferenceCurationReport,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    path,
                );
            }
        }
    }

    out.sort_by(|a, b| {
        a.group
            .cmp(&b.group)
            .then_with(|| a.title.cmp(&b.title))
            .then_with(|| a.id.cmp(&b.id))
    });

    Ok(out)
}

#[tauri::command]
async fn item_export_mux_preview_mp4(
    state: State<'_, AppState>,
    item_id: Option<String>,
    itemId: Option<String>,
    out_path: Option<String>,
    outPath: Option<String>,
) -> Result<ExportedFile, String> {
    let paths = state.paths.clone();
    let item_id = item_id
        .or(itemId)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| "missing required key itemId".to_string())?;
    let out_path = out_path
        .or(outPath)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| "missing required key outPath".to_string())?;

    tauri::async_runtime::spawn_blocking(move || {
        let dub_dir = paths.derived_item_dir(&item_id).join("dub_preview");
        let src_mkv = localization_preview_consumer_path(
            &paths,
            &item_id,
            None,
            dub_dir.join("mux_dub_preview_v1.mkv"),
        )?;
        let out_ext = std::path::Path::new(&out_path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        if out_ext != "mkv" {
            return Err("managed video exports must use an .mkv destination".to_string());
        }
        let legacy_mp4 = dub_dir.join("mux_dub_preview_v1.mp4");
        let src = if src_mkv.is_file() {
            src_mkv
        } else if legacy_mp4.is_file() {
            legacy_mp4
        } else {
            return Err("No playable muxed preview source was found".to_string());
        };

        let dst = std::path::PathBuf::from(&out_path);
        if let Some(parent) = dst.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
        }
        jobs::export_managed_video_as_mkv(&paths, &src, &dst).map_err(|e| e.to_string())?;
        let bytes = std::fs::metadata(&dst).map(|m| m.len()).unwrap_or(0);
        Ok(ExportedFile {
            out_path: dst.to_string_lossy().to_string(),
            file_bytes: bytes,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn item_export_source_media(
    state: State<'_, AppState>,
    item_id: Option<String>,
    itemId: Option<String>,
    out_path: Option<String>,
    outPath: Option<String>,
) -> Result<ExportedFile, String> {
    let paths = state.paths.clone();
    let item_id = item_id
        .or(itemId)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| "missing required key itemId".to_string())?;
    let out_path = out_path
        .or(outPath)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| "missing required key outPath".to_string())?;

    tauri::async_runtime::spawn_blocking(move || {
        let item = library::get_item_by_id(&paths, &item_id).map_err(|e| e.to_string())?;
        let src =
            library::resolve_media_path(&paths, &item.media_path).map_err(|e| e.to_string())?;
        if !src.is_file() {
            return Err(format!("source media not found: {}", item.media_path));
        }
        let dst = std::path::PathBuf::from(&out_path);
        let out_ext = dst
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        if !out_ext.eq_ignore_ascii_case("mkv") {
            return Err("managed source-video exports must use an .mkv destination".to_string());
        }
        if let Some(parent) = dst.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
        }
        jobs::export_managed_video_as_mkv(&paths, &src, &dst).map_err(|e| e.to_string())?;
        let bytes = std::fs::metadata(&dst).map(|m| m.len()).unwrap_or(0);
        Ok(ExportedFile {
            out_path: dst.to_string_lossy().to_string(),
            file_bytes: bytes,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn diagnostics_storage_breakdown(
    state: State<'_, AppState>,
) -> Result<diagnostics::StorageBreakdown, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || diagnostics::storage_breakdown(&paths))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn diagnostics_clear_cache(
    state: State<'_, AppState>,
) -> Result<diagnostics::CacheClearSummary, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || diagnostics::clear_cache(&paths))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn diagnostics_thumbnail_cache_status(
    state: State<'_, AppState>,
) -> Result<library::ThumbnailCacheStatus, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || library::thumbnail_cache_status(&paths))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn diagnostics_thumbnail_cache_clear(
    state: State<'_, AppState>,
) -> Result<library::ThumbnailCacheClearSummary, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || library::clear_thumbnail_cache(&paths))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn diagnostics_export_bundle(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    out_path: String,
) -> Result<diagnostics::DiagnosticsBundleResult, String> {
    let out_path = out_path.trim().to_string();
    if out_path.is_empty() {
        return Err("out_path is empty".to_string());
    }

    let package = app.package_info();
    let app_name = package.name.to_string();
    let app_version = package.version.to_string();
    let paths = state.paths.clone();

    tauri::async_runtime::spawn_blocking(move || {
        diagnostics::export_diagnostics_bundle(
            &paths,
            std::path::PathBuf::from(out_path),
            &app_name,
            &app_version,
        )
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn diagnostics_generate_licensing_report(
    state: State<'_, AppState>,
) -> Result<diagnostics::LicensingReportResult, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || diagnostics::generate_licensing_report(&paths))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn jobs_log_retention_policy() -> jobs::JobLogRetentionPolicy {
    jobs::job_log_retention_policy()
}

#[tauri::command]
async fn jobs_prune_logs(state: State<'_, AppState>) -> Result<(), String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || jobs::prune_job_logs_now(&paths))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn window_minimize(window: tauri::Window) -> Result<(), String> {
    window.minimize().map_err(|e| e.to_string())
}

#[tauri::command]
fn window_toggle_maximize(window: tauri::Window) -> Result<(), String> {
    let is_maximized = window.is_maximized().map_err(|e| e.to_string())?;
    if is_maximized {
        window.unmaximize().map_err(|e| e.to_string())
    } else {
        window.maximize().map_err(|e| e.to_string())
    }
}

#[tauri::command]
fn window_close(window: tauri::Window) -> Result<(), String> {
    window.close().map_err(|e| e.to_string())
}

#[tauri::command]
fn window_start_drag(window: tauri::Window) -> Result<(), String> {
    window.start_dragging().map_err(|e| e.to_string())
}

fn parse_window_resize_direction(direction: &str) -> Result<TauriResizeDirection, String> {
    match direction {
        "East" => Ok(TauriResizeDirection::East),
        "North" => Ok(TauriResizeDirection::North),
        "NorthEast" => Ok(TauriResizeDirection::NorthEast),
        "NorthWest" => Ok(TauriResizeDirection::NorthWest),
        "South" => Ok(TauriResizeDirection::South),
        "SouthEast" => Ok(TauriResizeDirection::SouthEast),
        "SouthWest" => Ok(TauriResizeDirection::SouthWest),
        "West" => Ok(TauriResizeDirection::West),
        _ => Err(format!("unsupported resize direction: {direction}")),
    }
}

#[tauri::command]
fn window_start_resize_drag(window: tauri::Window, direction: String) -> Result<(), String> {
    let direction = parse_window_resize_direction(&direction)?;
    window
        .start_resize_dragging(direction)
        .map_err(|e| e.to_string())
}

fn build_safe_mode_status(state: &AppState) -> Result<SafeModeStatus, String> {
    let persisted_enabled = config::load_safe_mode_config(&state.paths)
        .map(|value| value.enabled)
        .unwrap_or(false);
    let queue_paused = jobs::get_queue_control(&state.paths)
        .map(|value| value.paused)
        .unwrap_or(false);

    Ok(SafeModeStatus {
        enabled: state.safe_mode_enabled.load(Ordering::SeqCst),
        persisted_enabled,
        cli_enabled: state.safe_mode_cli,
        queue_paused,
    })
}

#[tauri::command]
fn safe_mode_status(state: State<'_, AppState>) -> Result<SafeModeStatus, String> {
    build_safe_mode_status(&state)
}

#[tauri::command]
fn startup_status(state: State<'_, AppState>) -> Result<StartupStatus, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "startup_status");
    current_startup_status(&state)
}

// WP-0221: expose the agent-bridge port to the frontend so the freeze-detector
// Worker can POST `/agent/freeze_event` directly (bypassing Tauri IPC, which
// routes through the WebView main thread we are trying to observe).
#[tauri::command]
fn agent_bridge_port() -> Option<u16> {
    AGENT_BRIDGE_PORT.get().copied()
}

// WP-0221: in-app trigger for the freeze-report dump (the same one
// `POST /agent/freeze_dump` produces). Useful when the app is still
// responsive enough to click; for unresponsive states use `vvfreeze.cmd`
// at the repo root, which hits the agent bridge on its own thread.
#[tauri::command]
fn agent_freeze_dump_now(note: Option<String>) -> Result<serde_json::Value, String> {
    let body = serde_json::json!({ "limit": 1000, "note": note }).to_string();
    let (status, response) = agent_handle_freeze_dump(&body);
    if !status.starts_with("200") {
        return Err(response);
    }
    serde_json::from_str(&response).map_err(|e| e.to_string())
}

#[tauri::command]
async fn diagnostics_app_state_snapshot(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<DiagnosticsAppStateSnapshot, String> {
    let package = app.package_info();
    let app_name = package.name.to_string();
    let app_version = package.version.to_string();
    let paths = state.paths.clone();
    let startup = current_startup_status(&state)?;

    tauri::async_runtime::spawn_blocking(move || {
        build_diagnostics_app_state_snapshot(&paths, app_name, app_version, startup)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn diagnostics_export_app_state_snapshot(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    out_path: String,
) -> Result<DiagnosticsAppStateSnapshotExport, String> {
    let out_path = out_path.trim().to_string();
    if out_path.is_empty() {
        return Err("out_path is empty".to_string());
    }

    let package = app.package_info();
    let app_name = package.name.to_string();
    let app_version = package.version.to_string();
    let paths = state.paths.clone();
    let startup = current_startup_status(&state)?;

    tauri::async_runtime::spawn_blocking(move || {
        let snapshot =
            build_diagnostics_app_state_snapshot(&paths, app_name, app_version, startup)?;
        write_diagnostics_app_state_snapshot_exports(&snapshot, &std::path::PathBuf::from(out_path))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn safe_mode_set(state: State<'_, AppState>, enabled: bool) -> Result<SafeModeStatus, String> {
    config::save_safe_mode_config(&state.paths, &config::SafeModeConfig { enabled })
        .map_err(|e| e.to_string())?;
    state.safe_mode_enabled.store(enabled, Ordering::SeqCst);

    let _ = jobs::set_queue_paused(&state.paths, enabled);
    build_safe_mode_status(&state)
}

#[tauri::command]
fn downloads_dir_status(state: State<'_, AppState>) -> Result<DownloadDirStatus, String> {
    build_download_dir_status(&state.paths)
}

#[tauri::command]
fn downloads_dir_set(
    state: State<'_, AppState>,
    path: String,
    create_if_missing: bool,
) -> Result<DownloadDirStatus, String> {
    let mut dir = std::path::PathBuf::from(path.trim());
    if dir.as_os_str().is_empty() {
        return Err("folder path is empty".to_string());
    }
    if !dir.is_absolute() {
        dir = std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(dir);
    }

    if create_if_missing {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    if !dir.exists() {
        return Err(format!("folder does not exist: {}", dir.to_string_lossy()));
    }
    if !dir.is_dir() {
        return Err(format!("path is not a folder: {}", dir.to_string_lossy()));
    }

    let normalized = dir.canonicalize().unwrap_or(dir);
    ensure_media_output_layout(&normalized)?;
    state
        .paths
        .set_download_dir_override(&normalized)
        .map_err(|e| e.to_string())?;
    build_download_dir_status(&state.paths)
}

#[tauri::command]
fn downloads_dir_use_default(
    state: State<'_, AppState>,
    create_if_missing: bool,
) -> Result<DownloadDirStatus, String> {
    let default_dir = state.paths.default_download_dir();
    if create_if_missing {
        std::fs::create_dir_all(&default_dir).map_err(|e| e.to_string())?;
    }
    if !default_dir.exists() {
        return Err(format!(
            "default folder does not exist: {}",
            default_dir.to_string_lossy()
        ));
    }
    if !default_dir.is_dir() {
        return Err(format!(
            "default path is not a folder: {}",
            default_dir.to_string_lossy()
        ));
    }
    ensure_media_output_layout(&default_dir)?;

    state
        .paths
        .clear_download_dir_override()
        .map_err(|e| e.to_string())?;
    build_download_dir_status(&state.paths)
}

#[tauri::command]
fn downloads_feature_root_set(
    state: State<'_, AppState>,
    feature: String,
    path: String,
    create_if_missing: bool,
) -> Result<DownloadDirStatus, String> {
    let feature = feature.trim().to_string();
    if feature.is_empty() {
        return Err("feature is empty".to_string());
    }

    let mut dir = std::path::PathBuf::from(path.trim());
    if dir.as_os_str().is_empty() {
        return Err("folder path is empty".to_string());
    }
    if !dir.is_absolute() {
        dir = std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(dir);
    }

    if create_if_missing {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    if !dir.exists() {
        return Err(format!("folder does not exist: {}", dir.to_string_lossy()));
    }
    if !dir.is_dir() {
        return Err(format!("path is not a folder: {}", dir.to_string_lossy()));
    }

    let normalized = dir.canonicalize().unwrap_or(dir);
    let normalized = normalized.to_string_lossy().to_string();
    config::update_feature_storage_roots_config(&state.paths, |roots| {
        set_feature_root_override(roots, &feature, Some(normalized.clone()))
            .map_err(voxvulgi_engine::EngineError::InstallFailed)
    })
    .map_err(|e| e.to_string())?;
    build_download_dir_status(&state.paths)
}

#[tauri::command]
fn downloads_feature_root_use_default(
    state: State<'_, AppState>,
    feature: String,
    create_if_missing: bool,
) -> Result<DownloadDirStatus, String> {
    let feature = feature.trim().to_string();
    if feature.is_empty() {
        return Err("feature is empty".to_string());
    }
    config::update_feature_storage_roots_config(&state.paths, |roots| {
        set_feature_root_override(roots, &feature, None)
            .map_err(voxvulgi_engine::EngineError::InstallFailed)
    })
    .map_err(|e| e.to_string())?;

    if create_if_missing {
        let status = build_download_dir_status(&state.paths)?;
        let target = status
            .feature_roots
            .into_iter()
            .find(|root| root.key == feature)
            .ok_or_else(|| format!("unknown storage feature: {feature}"))?;
        let dir = std::path::PathBuf::from(target.current_dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }

    build_download_dir_status(&state.paths)
}

#[tauri::command]
fn diagnostics_trace_dir_status(
    state: State<'_, AppState>,
) -> Result<DiagnosticsTraceDirStatus, String> {
    build_diagnostics_trace_dir_status(&state.paths)
}

#[tauri::command]
fn diagnostics_capture_status(
    state: State<'_, AppState>,
) -> Result<DiagnosticsCaptureStatus, String> {
    let _write_guard = DIAGNOSTICS_TRACE_WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "diagnostics trace write lock is poisoned".to_string())?;
    let mut status = load_diagnostics_capture_state(&state.paths);
    let now = now_epoch_ms_i64();
    if status.expires_at_ms.is_some_and(|expires| expires <= now) {
        let _ = finalize_diagnostics_incident_manifest(&status, "completed_expired");
        status = DiagnosticsCaptureStatus::default();
        persist_diagnostics_capture_state(&state.paths, &status)?;
        *diagnostics_capture_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = status.clone();
    }
    status.trace_bytes = diagnostics_trace_file_path(&state.paths)
        .ok()
        .and_then(|path| std::fs::metadata(path).ok())
        .map(|meta| meta.len())
        .unwrap_or(0);
    status.dropped_events = DIAGNOSTICS_TRACE_DROPPED_TOTAL.load(Ordering::Relaxed);
    Ok(status)
}

#[tauri::command]
async fn diagnostics_capture_panel_transition(
    state: State<'_, AppState>,
    page: String,
    transition_id: u64,
    span_id: String,
    parent_span_id: Option<String>,
) -> Result<DiagnosticsPanelTransitionReceipt, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        activate_panel_capture_before_navigation(
            &paths,
            &page,
            transition_id,
            &span_id,
            parent_span_id.as_deref(),
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
fn diagnostics_capture_panel_transition_cancel(
    state: State<'_, AppState>,
    incident_id: String,
    span_id: String,
) -> Result<bool, String> {
    cancel_superseded_panel_capture(&state.paths, incident_id.trim(), span_id.trim())
}

#[tauri::command]
fn diagnostics_capture_arm(
    state: State<'_, AppState>,
    trigger: String,
) -> Result<DiagnosticsCaptureStatus, String> {
    let _write_guard = DIAGNOSTICS_TRACE_WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "diagnostics trace write lock is poisoned".to_string())?;
    let trigger = trigger.trim();
    if !matches!(trigger, "panel_switch" | "job_start") {
        return Err("capture trigger must be panel_switch or job_start".to_string());
    }
    let now = now_epoch_ms_i64();
    let incident_id = diagnostics_unique_id("incident");
    let status = DiagnosticsCaptureStatus {
        mode: "normal".to_string(),
        armed_trigger: Some(trigger.to_string()),
        incident_id: Some(incident_id),
        armed_at_ms: Some(now),
        started_at_ms: None,
        expires_at_ms: Some(now + DIAGNOSTICS_INCIDENT_DURATION_MS),
        max_trace_bytes: DIAGNOSTICS_TRACE_INCIDENT_MAX_BYTES,
        trace_bytes: diagnostics_trace_file_path(&state.paths)
            .ok()
            .and_then(|path| std::fs::metadata(path).ok())
            .map(|meta| meta.len())
            .unwrap_or(0),
        dropped_events: DIAGNOSTICS_TRACE_DROPPED_TOTAL.load(Ordering::Relaxed),
        artifact_dir: None,
        root_span_id: None,
    };
    persist_diagnostics_capture_state(&state.paths, &status)?;
    *diagnostics_capture_state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = status.clone();
    append_diagnostics_trace_row_best_effort(
        &state.paths,
        "diagnostics_capture_armed",
        serde_json::json!({
            "trigger": trigger,
            "incident_id": status.incident_id,
            "expires_at_ms": status.expires_at_ms,
        }),
        "info",
    );
    Ok(status)
}

#[tauri::command]
fn diagnostics_capture_disarm(
    state: State<'_, AppState>,
) -> Result<DiagnosticsCaptureStatus, String> {
    let _write_guard = DIAGNOSTICS_TRACE_WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "diagnostics trace write lock is poisoned".to_string())?;
    let previous = load_diagnostics_capture_state(&state.paths);
    let _ = finalize_diagnostics_incident_manifest(&previous, "completed_disarmed");
    let status = DiagnosticsCaptureStatus::default();
    persist_diagnostics_capture_state(&state.paths, &status)?;
    *diagnostics_capture_state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = status.clone();
    append_diagnostics_trace_row_best_effort(
        &state.paths,
        "diagnostics_capture_disarmed",
        serde_json::json!({ "incident_id": previous.incident_id }),
        "info",
    );
    Ok(status)
}

#[tauri::command]
fn diagnostics_trace_dir_set(
    state: State<'_, AppState>,
    path: String,
    create_if_missing: bool,
) -> Result<DiagnosticsTraceDirStatus, String> {
    let _write_guard = DIAGNOSTICS_TRACE_WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "diagnostics trace write lock is poisoned".to_string())?;
    ensure_diagnostics_trace_mutation_allowed(&state.paths)?;
    let mut dir = std::path::PathBuf::from(path.trim());
    if dir.as_os_str().is_empty() {
        return Err("folder path is empty".to_string());
    }
    if !dir.is_absolute() {
        dir = std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(dir);
    }

    if create_if_missing {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    if !dir.exists() {
        return Err(format!("folder does not exist: {}", dir.to_string_lossy()));
    }
    if !dir.is_dir() {
        return Err(format!("path is not a folder: {}", dir.to_string_lossy()));
    }

    let normalized = dir.canonicalize().unwrap_or(dir);
    state
        .paths
        .set_diagnostics_trace_dir_override(&normalized)
        .map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&normalized).map_err(|e| e.to_string())?;
    build_diagnostics_trace_dir_status(&state.paths)
}

#[tauri::command]
fn diagnostics_trace_dir_use_default(
    state: State<'_, AppState>,
    create_if_missing: bool,
) -> Result<DiagnosticsTraceDirStatus, String> {
    let _write_guard = DIAGNOSTICS_TRACE_WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "diagnostics trace write lock is poisoned".to_string())?;
    ensure_diagnostics_trace_mutation_allowed(&state.paths)?;
    let default_dir = state.paths.default_diagnostics_trace_dir();
    if create_if_missing {
        std::fs::create_dir_all(&default_dir).map_err(|e| e.to_string())?;
    }
    if !default_dir.exists() {
        return Err(format!(
            "default folder does not exist: {}",
            default_dir.to_string_lossy()
        ));
    }
    if !default_dir.is_dir() {
        return Err(format!(
            "default path is not a folder: {}",
            default_dir.to_string_lossy()
        ));
    }
    state
        .paths
        .clear_diagnostics_trace_dir_override()
        .map_err(|e| e.to_string())?;
    build_diagnostics_trace_dir_status(&state.paths)
}

#[tauri::command]
async fn diagnostics_trace_clear(
    state: State<'_, AppState>,
) -> Result<DiagnosticsTraceClearSummary, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _write_guard = DIAGNOSTICS_TRACE_WRITE_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .map_err(|_| "diagnostics trace write lock is poisoned".to_string())?;
        ensure_diagnostics_trace_mutation_allowed(&paths)?;
        let dir = paths
            .effective_diagnostics_trace_dir()
            .map_err(|e| e.to_string())?;
        clear_dir_entries_with_bytes(&dir)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn diagnostics_trace_write_event(
    state: State<'_, AppState>,
    event: String,
    details: Option<serde_json::Value>,
    level: Option<String>,
) -> Result<DiagnosticsTraceEnqueueReceipt, String> {
    let event = event.trim().to_string();
    if event.is_empty() {
        return Err("event is empty".to_string());
    }

    Ok(append_diagnostics_trace_row_best_effort(
        &state.paths,
        &event,
        details.unwrap_or(serde_json::Value::Null),
        level.as_deref().unwrap_or("info"),
    ))
}

#[tauri::command]
async fn diagnostics_trace_recent(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<DiagnosticsTraceEntry>, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        read_recent_diagnostics_trace_entries(&paths, limit.unwrap_or(120).clamp(1, 1000))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn config_batch_on_import_get(
    state: State<'_, AppState>,
) -> Result<config::BatchOnImportRules, String> {
    config::load_batch_on_import_rules(&state.paths).map_err(|e| e.to_string())
}

#[tauri::command]
fn config_batch_on_import_set(
    state: State<'_, AppState>,
    rules: config::BatchOnImportRules,
) -> Result<config::BatchOnImportRules, String> {
    config::save_batch_on_import_rules(&state.paths, &rules).map_err(|e| e.to_string())?;
    Ok(rules)
}

#[tauri::command]
fn root_rebind_dry_run(
    state: State<'_, AppState>,
    from_root: String,
) -> Result<root_rebind::RootRebindTaskStatus, String> {
    let paths = state.paths.clone();
    let from_root = from_root.trim().to_string();
    root_rebind::submit_root_rebind_task("dry_run", move || {
        root_rebind::root_rebind_dry_run(&paths, &from_root)
    })
    .map_err(|error| error.to_string())
}

#[tauri::command]
fn root_rebind_prepare(
    state: State<'_, AppState>,
    from_root: String,
    to_root: String,
    evidence: Vec<root_rebind::RootIdentityEvidence>,
) -> Result<root_rebind::RootRebindTaskStatus, String> {
    let paths = state.paths.clone();
    let from_root = from_root.trim().to_string();
    let to_root = to_root.trim().to_string();
    root_rebind::submit_root_rebind_task_cancellable("prepare", move |cancellation| {
        root_rebind::prepare_root_rebind_cancellable(
            &paths,
            &from_root,
            std::path::Path::new(&to_root),
            &evidence,
            &cancellation,
        )
    })
    .map_err(|error| error.to_string())
}

#[tauri::command]
fn root_rebind_apply(
    state: State<'_, AppState>,
    receipt_id: String,
    confirmation: String,
) -> Result<root_rebind::RootRebindTaskStatus, String> {
    let receipt_id = receipt_id.trim().to_string();
    if confirmation.trim() != format!("APPLY:{receipt_id}") {
        return Err(format!(
            "root rebind apply requires confirmation APPLY:{receipt_id}"
        ));
    }
    let paths = state.paths.clone();
    root_rebind::submit_root_rebind_task_cancellable("apply", move |cancellation| {
        root_rebind::apply_prepared_root_rebind_cancellable(
            &paths,
            &receipt_id,
            None,
            &cancellation,
        )
    })
    .map_err(|error| error.to_string())
}

#[tauri::command]
fn root_rebind_status(
    state: State<'_, AppState>,
    receipt_id: Option<String>,
) -> Result<Vec<root_rebind::RootRebindReceipt>, String> {
    if let Some(receipt_id) = receipt_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return root_rebind::root_rebind_receipt_status(&state.paths, receipt_id)
            .map(|receipt| vec![receipt])
            .map_err(|error| error.to_string());
    }
    root_rebind::list_root_rebind_receipts(&state.paths).map_err(|error| error.to_string())
}

#[tauri::command]
fn root_rebind_rollback(
    state: State<'_, AppState>,
    receipt_id: String,
    confirmation: String,
) -> Result<root_rebind::RootRebindTaskStatus, String> {
    let receipt_id = receipt_id.trim().to_string();
    if confirmation.trim() != format!("ROLLBACK:{receipt_id}") {
        return Err(format!(
            "root rebind rollback requires confirmation ROLLBACK:{receipt_id}"
        ));
    }
    let paths = state.paths.clone();
    root_rebind::submit_root_rebind_task("rollback", move || {
        root_rebind::rollback_root_rebind(&paths, &receipt_id)
    })
    .map_err(|error| error.to_string())
}

#[tauri::command]
fn root_rebind_recover(
    state: State<'_, AppState>,
) -> Result<root_rebind::RootRebindTaskStatus, String> {
    let paths = state.paths.clone();
    root_rebind::submit_root_rebind_task("recover", move || {
        root_rebind::reconcile_incomplete_root_rebinds(&paths)
    })
    .map_err(|error| error.to_string())
}

#[tauri::command]
fn root_rebind_task_status(
    task_id: String,
    wait_timeout_ms: Option<u64>,
) -> Result<root_rebind::RootRebindTaskStatus, String> {
    root_rebind::root_rebind_task_status(task_id.trim(), wait_timeout_ms)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn root_rebind_task_cancel(
    task_id: String,
) -> Result<root_rebind::RootRebindTaskStatus, String> {
    root_rebind::cancel_root_rebind_task(task_id.trim()).map_err(|error| error.to_string())
}

#[derive(Debug, serde::Serialize)]
struct YoutubeAuthStatus {
    manual_cookie_configured: bool,
    browser_cookie_source: Option<String>,
    last_verified_at_ms: Option<i64>,
    reconnect_required_at_ms: Option<i64>,
    credential_generation: u64,
    credential_fingerprint: String,
    cleanup_warning: Option<String>,
}

fn youtube_auth_status(
    paths: &AppPaths,
    config_value: &config::YoutubeAuthConfig,
) -> Result<YoutubeAuthStatus, String> {
    let revision = config::youtube_auth_revision(paths)
        .map_err(|error| jobs::redact_auth_credential_locators(&error.to_string()))?;
    Ok(YoutubeAuthStatus {
        manual_cookie_configured: config_value
            .netscape_cookie_json
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty()),
        browser_cookie_source: config_value.browser_cookie_source.clone(),
        last_verified_at_ms: config_value.last_verified_at_ms,
        reconnect_required_at_ms: config_value.reconnect_required_at_ms,
        credential_generation: revision.credential_generation,
        credential_fingerprint: revision.credential_fingerprint,
        cleanup_warning: None,
    })
}

#[tauri::command]
fn config_youtube_auth_get(state: State<'_, AppState>) -> Result<YoutubeAuthStatus, String> {
    let config_value = config::load_youtube_auth_config(&state.paths)
        .map_err(|e| jobs::redact_auth_credential_locators(&e.to_string()))?;
    youtube_auth_status(&state.paths, &config_value)
}

#[tauri::command]
fn config_youtube_auth_set(
    state: State<'_, AppState>,
    config_value: config::YoutubeAuthConfig,
    expected_credential_generation: Option<u64>,
    expected_credential_fingerprint: Option<String>,
) -> Result<YoutubeAuthStatus, String> {
    let _operation_guard = YOUTUBE_AUTH_OPERATION_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "youtube auth operation lock is poisoned".to_string())?;
    let config_value = config::YoutubeAuthConfig {
        netscape_cookie_json: jobs::normalize_youtube_auth_cookie_for_storage(
            config_value.netscape_cookie_json,
        )
        .map_err(|e| jobs::redact_auth_credential_locators(&e.to_string()))?,
        browser_cookie_source: jobs::normalize_browser_cookie_source(
            config_value.browser_cookie_source.as_deref(),
        )
        .map_err(|e| jobs::redact_auth_credential_locators(&e.to_string()))?,
        last_verified_at_ms: None,
        reconnect_required_at_ms: None,
    };
    // The engine commits the credential CAS before clearing only the previous credential's
    // circuit. A stale writer therefore has zero runtime side effects and cannot clear a hold
    // belonging to the winning credential.
    let saved = jobs::replace_youtube_auth_config_and_clear_previous_block(
        &state.paths,
        config_value,
        expected_credential_generation,
        expected_credential_fingerprint.as_deref(),
    )
    .map_err(|e| jobs::redact_auth_credential_locators(&e.to_string()))?;
    let mut status = youtube_auth_status(&state.paths, &saved.config)?;
    status.cleanup_warning = saved.cleanup_warning;
    Ok(status)
}

const YOUTUBE_SIGN_IN_URL: &str = "https://www.youtube.com/";

#[derive(Debug, serde::Serialize)]
struct YoutubeSignInLaunchResult {
    browser_source: String,
    url: String,
}

#[cfg(windows)]
fn youtube_browser_windows_candidates(browser_source: &str) -> Vec<std::path::PathBuf> {
    let program_files = std::env::var_os("ProgramFiles").map(std::path::PathBuf::from);
    let program_files_x86 = std::env::var_os("ProgramFiles(x86)").map(std::path::PathBuf::from);
    let local_app_data = std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from);
    let mut candidates = Vec::new();
    let mut push_under = |root: &Option<std::path::PathBuf>, relative: &str| {
        if let Some(root) = root {
            candidates.push(root.join(relative));
        }
    };
    match browser_source {
        "firefox" => {
            push_under(&program_files, "Mozilla Firefox/firefox.exe");
            push_under(&program_files_x86, "Mozilla Firefox/firefox.exe");
        }
        "chrome" => {
            push_under(&program_files, "Google/Chrome/Application/chrome.exe");
            push_under(&program_files_x86, "Google/Chrome/Application/chrome.exe");
            push_under(&local_app_data, "Google/Chrome/Application/chrome.exe");
        }
        "edge" => {
            push_under(&program_files, "Microsoft/Edge/Application/msedge.exe");
            push_under(&program_files_x86, "Microsoft/Edge/Application/msedge.exe");
            push_under(&local_app_data, "Microsoft/Edge/Application/msedge.exe");
        }
        "opera" => {
            push_under(&local_app_data, "Programs/Opera/opera.exe");
            push_under(&program_files, "Opera/opera.exe");
            push_under(&program_files_x86, "Opera/opera.exe");
        }
        _ => {}
    }
    candidates
}

#[cfg(windows)]
fn launch_youtube_sign_in_in_browser(browser_source: &str) -> Result<(), String> {
    let executable = youtube_browser_windows_candidates(browser_source)
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| {
            format!(
                "Could not find {browser_source} on this computer. Open youtube.com in that browser yourself, sign in, then return here."
            )
        })?;
    std::process::Command::new(&executable)
        .arg(YOUTUBE_SIGN_IN_URL)
        .spawn()
        .map_err(|err| format!("Could not open {}: {err}", executable.display()))?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn launch_youtube_sign_in_in_browser(browser_source: &str) -> Result<(), String> {
    let app = match browser_source {
        "firefox" => "Firefox",
        "chrome" => "Google Chrome",
        "edge" => "Microsoft Edge",
        "opera" => "Opera",
        _ => return Err(format!("Unsupported browser: {browser_source}")),
    };
    std::process::Command::new("open")
        .args(["-a", app, YOUTUBE_SIGN_IN_URL])
        .spawn()
        .map_err(|err| format!("Could not open {app}: {err}"))?;
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn launch_youtube_sign_in_in_browser(browser_source: &str) -> Result<(), String> {
    let executable = match browser_source {
        "firefox" => "firefox",
        "chrome" => "google-chrome",
        "edge" => "microsoft-edge",
        "opera" => "opera",
        _ => return Err(format!("Unsupported browser: {browser_source}")),
    };
    std::process::Command::new(executable)
        .arg(YOUTUBE_SIGN_IN_URL)
        .spawn()
        .map_err(|err| format!("Could not open {executable}: {err}"))?;
    Ok(())
}

#[tauri::command]
fn youtube_auth_open_sign_in(browser_source: String) -> Result<YoutubeSignInLaunchResult, String> {
    let browser_source = jobs::normalize_browser_cookie_source(Some(&browser_source))
        .map_err(|err| err.to_string())?
        .ok_or_else(|| "Choose a supported browser first.".to_string())?;
    launch_youtube_sign_in_in_browser(&browser_source)?;
    Ok(YoutubeSignInLaunchResult {
        browser_source,
        url: YOUTUBE_SIGN_IN_URL.to_string(),
    })
}

#[tauri::command]
fn config_youtube_auth_preflight(
    state: State<'_, AppState>,
    url: Option<String>,
) -> Result<jobs::YoutubeAuthPreflightResult, String> {
    let _operation_guard = YOUTUBE_AUTH_OPERATION_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "youtube auth operation lock is poisoned".to_string())?;
    jobs::youtube_auth_preflight(&state.paths, url)
        .map_err(|e| jobs::redact_auth_credential_locators(&e.to_string()))
}

// ---- WP-0263: global Instagram auth in Options (mirrors config_youtube_auth_*) ----

/// Frontend->backend payload for `config_instagram_auth_set`. `cookie` is the pasted Instagram
/// session cookie (a `Cookie:` header string, or a browser-extension cookie JSON array — the
/// engine normalizes both). Empty/omitted clears the saved global Instagram cookie.
#[derive(Debug, Clone, serde::Deserialize)]
struct InstagramAuthConfigInput {
    #[serde(default)]
    cookie: Option<String>,
}

/// Backend->frontend status for `config_instagram_auth_get` / `config_instagram_auth_set`.
/// The secret itself is never returned; only whether a global Instagram cookie is configured.
#[derive(Debug, Clone, serde::Serialize)]
struct InstagramAuthConfigStatus {
    configured: bool,
    credential_generation: u64,
    credential_fingerprint: String,
    cleanup_warning: Option<String>,
}

/// Whether a global Instagram cookie is configured. Mirrors `config_youtube_auth_get`, but
/// never returns the secret to the frontend — only the `configured` flag.
#[tauri::command]
fn config_instagram_auth_get(
    state: State<'_, AppState>,
) -> Result<InstagramAuthConfigStatus, String> {
    let revision = jobs::instagram_auth_revision(&state.paths)
        .map_err(|e| jobs::redact_auth_credential_locators(&e.to_string()))?;
    Ok(InstagramAuthConfigStatus {
        configured: revision.configured,
        credential_generation: revision.credential_generation,
        credential_fingerprint: revision.credential_fingerprint,
        cleanup_warning: revision.cleanup_warning,
    })
}

/// Save (or clear, when `config_value.cookie` is empty/omitted) the global Instagram cookie.
/// Mirrors `config_youtube_auth_set`: normalizes + stores the secret, then clears any armed
/// Instagram auth block so a fresh login lets the fleet retry immediately.
#[tauri::command]
fn config_instagram_auth_set(
    state: State<'_, AppState>,
    config_value: InstagramAuthConfigInput,
    expected_credential_generation: Option<u64>,
    expected_credential_fingerprint: Option<String>,
) -> Result<InstagramAuthConfigStatus, String> {
    let revision = jobs::replace_global_instagram_auth_cookie(
        &state.paths,
        config_value.cookie,
        expected_credential_generation,
        expected_credential_fingerprint.as_deref(),
    )
        .map_err(|e| jobs::redact_auth_credential_locators(&e.to_string()))?;
    Ok(InstagramAuthConfigStatus {
        configured: revision.configured,
        credential_generation: revision.credential_generation,
        credential_fingerprint: revision.credential_fingerprint,
        cleanup_warning: revision.cleanup_warning,
    })
}

/// "Test saved Instagram cookies" — mirrors `config_youtube_auth_preflight`.
#[tauri::command]
fn config_instagram_auth_preflight(
    state: State<'_, AppState>,
    url: Option<String>,
) -> Result<jobs::InstagramAuthPreflightResult, String> {
    jobs::instagram_auth_preflight(&state.paths, url)
        .map_err(|e| jobs::redact_auth_credential_locators(&e.to_string()))
}

#[tauri::command]
fn config_diarization_optional_status(
    state: State<'_, AppState>,
) -> Result<config::OptionalDiarizationBackendStatus, String> {
    config::load_optional_diarization_backend_status(&state.paths).map_err(|e| e.to_string())
}

#[tauri::command]
fn config_diarization_optional_set(
    state: State<'_, AppState>,
    config_value: config::OptionalDiarizationBackendConfig,
    token: Option<String>,
) -> Result<config::OptionalDiarizationBackendStatus, String> {
    config::save_optional_diarization_backend_config(&state.paths, &config_value, token.as_deref())
        .map_err(|e| e.to_string())?;
    config::load_optional_diarization_backend_status(&state.paths).map_err(|e| e.to_string())
}

#[tauri::command]
fn config_diarization_optional_clear_token(
    state: State<'_, AppState>,
) -> Result<config::OptionalDiarizationBackendStatus, String> {
    config::clear_optional_diarization_backend_token(&state.paths).map_err(|e| e.to_string())?;
    config::load_optional_diarization_backend_status(&state.paths).map_err(|e| e.to_string())
}

#[tauri::command]
async fn models_inventory(
    state: State<'_, AppState>,
) -> Result<voxvulgi_engine::models::ModelInventory, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let store = ModelStore::new(paths);
        store.inventory().map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn models_install_demo(state: State<'_, AppState>) -> Result<(), String> {
    let store = ModelStore::new(state.paths.clone());
    store
        .install_model("demo-ja-asr")
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn models_install(state: State<'_, AppState>, model_id: String) -> Result<(), String> {
    let store = ModelStore::new(state.paths.clone());
    store.install_model(&model_id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn tools_ffmpeg_status(
    state: State<'_, AppState>,
) -> Result<tools::FfmpegToolsStatus, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || Ok(tools::ffmpeg_tools_status(&paths)))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn tools_ffmpeg_install(state: State<'_, AppState>) -> Result<tools::FfmpegToolsStatus, String> {
    tools::install_ffmpeg_tools(&state.paths).map_err(|e| e.to_string())
}

#[tauri::command]
async fn tools_ytdlp_status(state: State<'_, AppState>) -> Result<tools::YtDlpToolsStatus, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || Ok(tools::ytdlp_tools_status(&paths)))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn tools_ytdlp_install(state: State<'_, AppState>) -> Result<tools::YtDlpToolsStatus, String> {
    tools::install_ytdlp_tools(&state.paths).map_err(|e| e.to_string())
}

#[tauri::command]
async fn tools_js_runtime_status(
    state: State<'_, AppState>,
) -> Result<tools::JsRuntimeToolsStatus, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || Ok(tools::js_runtime_tools_status(&paths)))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn tools_js_runtime_install(
    state: State<'_, AppState>,
) -> Result<tools::JsRuntimeToolsStatus, String> {
    tools::install_js_runtime_tools(&state.paths).map_err(|e| e.to_string())
}

#[tauri::command]
async fn tools_python_status(
    state: State<'_, AppState>,
) -> Result<tools::PythonToolchainStatus, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || Ok(tools::python_toolchain_status(&paths)))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn tools_python_install(
    state: State<'_, AppState>,
) -> Result<tools::PythonToolchainStatus, String> {
    tools::install_python_toolchain(&state.paths).map_err(|e| e.to_string())
}

#[tauri::command]
async fn tools_python_portable_status(
    state: State<'_, AppState>,
) -> Result<tools::PortablePythonStatus, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || Ok(tools::portable_python_status(&paths)))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn tools_python_portable_install(
    state: State<'_, AppState>,
) -> Result<tools::PortablePythonStatus, String> {
    tools::install_portable_python(&state.paths).map_err(|e| e.to_string())
}

#[tauri::command]
fn tools_phase2_packs_install_plan() -> Vec<tools::Phase2PackPlanItem> {
    tools::phase2_packs_install_plan()
}

#[tauri::command]
async fn tools_phase2_packs_install_latest_state(
    state: State<'_, AppState>,
) -> Result<Phase2InstallLatestState, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let path = paths.install_logs_dir().join("phase2").join("latest.json");

        if !path.exists() {
            return Ok(Phase2InstallLatestState {
                exists: false,
                path: path.to_string_lossy().to_string(),
                state: None,
                active: false,
                stale: false,
                job_status: None,
            });
        }

        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
        let parsed: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        let (state, active, stale, job_status) = normalize_phase2_latest_state(&paths, parsed);
        Ok(Phase2InstallLatestState {
            exists: true,
            path: path.to_string_lossy().to_string(),
            state: Some(state),
            active,
            stale,
            job_status,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn tools_pack_integrity_manifest_status(
    state: State<'_, AppState>,
) -> Result<tools::PackIntegrityManifestStatus, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || Ok(tools::pack_integrity_manifest_status(&paths)))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn tools_pack_integrity_manifest_generate(
    state: State<'_, AppState>,
) -> Result<tools::PackIntegrityManifestResult, String> {
    tools::generate_pack_integrity_manifest(&state.paths).map_err(|e| e.to_string())
}

#[tauri::command]
async fn tools_performance_tier_status(
    state: State<'_, AppState>,
) -> Result<tools::PerformanceTierStatus, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || Ok(tools::performance_tier_status(&paths)))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn tools_spleeter_status(
    state: State<'_, AppState>,
) -> Result<tools::SpleeterPackStatus, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || Ok(tools::spleeter_pack_status(&paths)))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn tools_spleeter_install(
    state: State<'_, AppState>,
) -> Result<tools::SpleeterPackStatus, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "tools_spleeter_install");
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        tools::install_spleeter_pack(&paths).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn tools_demucs_status(
    state: State<'_, AppState>,
) -> Result<tools::DemucsPackStatus, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || Ok(tools::demucs_pack_status(&paths)))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn tools_demucs_install(
    state: State<'_, AppState>,
) -> Result<tools::DemucsPackStatus, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "tools_demucs_install");
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        tools::install_demucs_pack(&paths).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn tools_diarization_status(
    state: State<'_, AppState>,
) -> Result<tools::DiarizationPackStatus, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || Ok(tools::diarization_pack_status(&paths)))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn tools_diarization_install(
    state: State<'_, AppState>,
) -> Result<tools::DiarizationPackStatus, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "tools_diarization_install");
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        tools::install_diarization_pack(&paths).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn tools_tts_preview_status(
    state: State<'_, AppState>,
) -> Result<tools::TtsPreviewPackStatus, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || Ok(tools::tts_preview_pack_status(&paths)))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn tools_tts_preview_install(
    state: State<'_, AppState>,
) -> Result<tools::TtsPreviewPackStatus, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "tools_tts_preview_install");
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        tools::install_tts_preview_pack(&paths).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn tools_tts_neural_local_v1_status(
    state: State<'_, AppState>,
) -> Result<tools::TtsNeuralLocalV1PackStatus, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || Ok(tools::tts_neural_local_v1_pack_status(&paths)))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn tools_tts_neural_local_v1_install(
    state: State<'_, AppState>,
) -> Result<tools::TtsNeuralLocalV1PackStatus, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "tools_tts_neural_local_v1_install");
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        tools::install_tts_neural_local_v1_pack(&paths).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn tools_tts_voice_preserving_local_v1_status(
    state: State<'_, AppState>,
) -> Result<tools::TtsVoicePreservingLocalV1PackStatus, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        Ok(tools::tts_voice_preserving_local_v1_pack_status(&paths))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn tools_tts_voice_preserving_local_v1_install(
    state: State<'_, AppState>,
) -> Result<tools::TtsVoicePreservingLocalV1PackStatus, String> {
    let _timer = InvokeTimer::start(
        state.paths.clone(),
        "tools_tts_voice_preserving_local_v1_install",
    );
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        tools::install_tts_voice_preserving_local_v1_pack(&paths).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn voice_backends_catalog(
    state: State<'_, AppState>,
) -> Result<voice_backends::VoiceBackendCatalog, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || Ok(voice_backends::backend_catalog(&paths)))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn voice_backends_recommend(
    state: State<'_, AppState>,
    request: Option<voice_backends::VoiceBackendRecommendationRequest>,
) -> Result<voice_backends::VoiceBackendRecommendation, String> {
    let paths = state.paths.clone();
    let request = request.unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || {
        Ok(voice_backends::recommend_backend(&paths, request))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
#[allow(non_snake_case)]
async fn voice_benchmark_generate(
    state: State<'_, AppState>,
    item_id: Option<String>,
    itemId: Option<String>,
    track_id: Option<String>,
    trackId: Option<String>,
    goal: Option<String>,
) -> Result<voice_benchmarks::VoiceBenchmarkReport, String> {
    let paths = state.paths.clone();
    let item_id = item_id
        .or(itemId)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "missing required key itemId".to_string())?;
    let track_id = track_id
        .or(trackId)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "missing required key trackId".to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        voice_benchmarks::generate_voice_benchmark_report(
            &paths,
            &item_id,
            &track_id,
            goal.as_deref(),
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
#[allow(non_snake_case)]
async fn voice_benchmark_load(
    state: State<'_, AppState>,
    item_id: Option<String>,
    itemId: Option<String>,
    track_id: Option<String>,
    trackId: Option<String>,
    goal: Option<String>,
) -> Result<Option<voice_benchmarks::VoiceBenchmarkReport>, String> {
    let paths = state.paths.clone();
    let item_id = item_id
        .or(itemId)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "missing required key itemId".to_string())?;
    let track_id = track_id
        .or(trackId)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "missing required key trackId".to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        voice_benchmarks::load_voice_benchmark_report(&paths, &item_id, &track_id, goal.as_deref())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
#[allow(non_snake_case)]
async fn voice_benchmark_history_list(
    state: State<'_, AppState>,
    item_id: Option<String>,
    itemId: Option<String>,
    track_id: Option<String>,
    trackId: Option<String>,
    goal: Option<String>,
) -> Result<Vec<voice_benchmarks::VoiceBenchmarkHistoryEntry>, String> {
    let paths = state.paths.clone();
    let item_id = item_id
        .or(itemId)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "missing required key itemId".to_string())?;
    let track_id = track_id
        .or(trackId)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "missing required key trackId".to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        voice_benchmarks::list_voice_benchmark_history(&paths, &item_id, &track_id, goal.as_deref())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
#[allow(non_snake_case)]
async fn voice_benchmark_leaderboard_export(
    state: State<'_, AppState>,
    item_id: Option<String>,
    itemId: Option<String>,
    track_id: Option<String>,
    trackId: Option<String>,
    goal: Option<String>,
) -> Result<voice_benchmarks::VoiceBenchmarkLeaderboardExport, String> {
    let paths = state.paths.clone();
    let item_id = item_id
        .or(itemId)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "missing required key itemId".to_string())?;
    let track_id = track_id
        .or(trackId)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "missing required key trackId".to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        voice_benchmarks::export_voice_benchmark_leaderboard(
            &paths,
            &item_id,
            &track_id,
            goal.as_deref(),
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
#[allow(non_snake_case)]
async fn voice_reference_curation_generate(
    state: State<'_, AppState>,
    item_id: Option<String>,
    itemId: Option<String>,
    speaker_key: Option<String>,
    speakerKey: Option<String>,
) -> Result<voice_reference_curation::VoiceReferenceCurationReport, String> {
    let paths = state.paths.clone();
    let item_id = item_id
        .or(itemId)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "missing required key itemId".to_string())?;
    let speaker_key = speaker_key
        .or(speakerKey)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "missing required key speakerKey".to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        voice_reference_curation::generate_reference_curation_report(&paths, &item_id, &speaker_key)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
#[allow(non_snake_case)]
async fn voice_reference_curation_load(
    state: State<'_, AppState>,
    item_id: Option<String>,
    itemId: Option<String>,
    speaker_key: Option<String>,
    speakerKey: Option<String>,
) -> Result<Option<voice_reference_curation::VoiceReferenceCurationReport>, String> {
    let paths = state.paths.clone();
    let item_id = item_id
        .or(itemId)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "missing required key itemId".to_string())?;
    let speaker_key = speaker_key
        .or(speakerKey)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "missing required key speakerKey".to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        voice_reference_curation::load_reference_curation_report(&paths, &item_id, &speaker_key)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
#[allow(non_snake_case)]
async fn voice_reference_curation_apply(
    state: State<'_, AppState>,
    item_id: Option<String>,
    itemId: Option<String>,
    speaker_key: Option<String>,
    speakerKey: Option<String>,
    mode: Option<String>,
) -> Result<speakers::ItemSpeakerSetting, String> {
    let paths = state.paths.clone();
    let item_id = item_id
        .or(itemId)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "missing required key itemId".to_string())?;
    let speaker_key = speaker_key
        .or(speakerKey)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "missing required key speakerKey".to_string())?;
    let mode = mode.unwrap_or_else(|| "ranked".to_string());
    tauri::async_runtime::spawn_blocking(move || {
        voice_reference_curation::apply_reference_curation(&paths, &item_id, &speaker_key, &mode)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
#[allow(non_snake_case)]
async fn voice_reference_candidates_generate(
    state: State<'_, AppState>,
    request: voice_reference_candidates::VoiceReferenceCandidateGenerationRequest,
) -> Result<voice_reference_candidates::VoiceReferenceCandidateReport, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        voice_reference_candidates::generate_reference_candidates(&paths, request)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
#[allow(non_snake_case)]
async fn voice_reference_candidates_load(
    state: State<'_, AppState>,
    item_id: Option<String>,
    itemId: Option<String>,
    speaker_key: Option<String>,
    speakerKey: Option<String>,
) -> Result<Option<voice_reference_candidates::VoiceReferenceCandidateReport>, String> {
    let paths = state.paths.clone();
    let item_id = item_id
        .or(itemId)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "missing required key itemId".to_string())?;
    let speaker_key = speaker_key
        .or(speakerKey)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    tauri::async_runtime::spawn_blocking(move || {
        voice_reference_candidates::load_reference_candidates(
            &paths,
            &item_id,
            speaker_key.as_deref(),
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
#[allow(non_snake_case)]
async fn voice_reference_candidates_apply(
    state: State<'_, AppState>,
    item_id: Option<String>,
    itemId: Option<String>,
    speaker_key: Option<String>,
    speakerKey: Option<String>,
    mode: Option<String>,
) -> Result<speakers::ItemSpeakerSetting, String> {
    let paths = state.paths.clone();
    let item_id = item_id
        .or(itemId)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "missing required key itemId".to_string())?;
    let speaker_key = speaker_key
        .or(speakerKey)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "missing required key speakerKey".to_string())?;
    let mode = mode.unwrap_or_else(|| "append".to_string());
    tauri::async_runtime::spawn_blocking(move || {
        voice_reference_candidates::apply_reference_candidate(&paths, &item_id, &speaker_key, &mode)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
#[allow(non_snake_case)]
async fn item_voice_plan_get(
    state: State<'_, AppState>,
    item_id: Option<String>,
    itemId: Option<String>,
) -> Result<Option<voice_plans::ItemVoicePlan>, String> {
    let paths = state.paths.clone();
    let item_id = item_id
        .or(itemId)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "missing required key itemId".to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        voice_plans::get_item_voice_plan(&paths, &item_id).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
#[allow(non_snake_case)]
async fn item_voice_plan_upsert(
    state: State<'_, AppState>,
    item_id: Option<String>,
    itemId: Option<String>,
    plan: voice_plans::ItemVoicePlanUpsert,
) -> Result<voice_plans::ItemVoicePlan, String> {
    let paths = state.paths.clone();
    let item_id = item_id
        .or(itemId)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "missing required key itemId".to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        voice_plans::upsert_item_voice_plan(&paths, &item_id, plan).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
#[allow(non_snake_case)]
async fn item_voice_plan_delete(
    state: State<'_, AppState>,
    item_id: Option<String>,
    itemId: Option<String>,
) -> Result<(), String> {
    let paths = state.paths.clone();
    let item_id = item_id
        .or(itemId)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "missing required key itemId".to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        voice_plans::delete_item_voice_plan(&paths, &item_id).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
#[allow(non_snake_case)]
async fn item_voice_plan_promote_recommendation(
    state: State<'_, AppState>,
    item_id: Option<String>,
    itemId: Option<String>,
    recommendation: voice_backends::VoiceBackendRecommendation,
) -> Result<voice_plans::ItemVoicePlan, String> {
    let paths = state.paths.clone();
    let item_id = item_id
        .or(itemId)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "missing required key itemId".to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        voice_plans::promote_recommendation_to_item_voice_plan(&paths, &item_id, recommendation)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
#[allow(non_snake_case)]
async fn item_voice_plan_promote_benchmark_candidate(
    state: State<'_, AppState>,
    item_id: Option<String>,
    itemId: Option<String>,
    track_id: Option<String>,
    trackId: Option<String>,
    goal: Option<String>,
    candidate_id: Option<String>,
    candidateId: Option<String>,
) -> Result<voice_plans::ItemVoicePlan, String> {
    let paths = state.paths.clone();
    let item_id = item_id
        .or(itemId)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "missing required key itemId".to_string())?;
    let track_id = track_id
        .or(trackId)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "missing required key trackId".to_string())?;
    let candidate_id = candidate_id
        .or(candidateId)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "missing required key candidateId".to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        voice_plans::promote_benchmark_candidate_to_item_voice_plan(
            &paths,
            &item_id,
            &track_id,
            goal.as_deref(),
            &candidate_id,
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn voice_backend_adapters_list(
    state: State<'_, AppState>,
) -> Result<Vec<voice_backend_adapters::VoiceBackendAdapterDetail>, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        voice_backend_adapters::list_voice_backend_adapters(&paths).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn voice_backend_adapter_upsert(
    state: State<'_, AppState>,
    config: voice_backend_adapters::VoiceBackendAdapterConfig,
) -> Result<voice_backend_adapters::VoiceBackendAdapterDetail, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        voice_backend_adapters::upsert_voice_backend_adapter(&paths, config)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn voice_backend_adapter_apply_starter_recipe(
    config: voice_backend_adapters::VoiceBackendAdapterConfig,
    recipe_id: String,
) -> Result<voice_backend_adapters::VoiceBackendAdapterConfig, String> {
    voice_backend_adapters::apply_voice_backend_starter_recipe(config, recipe_id.trim())
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn voice_backend_adapter_delete(
    state: State<'_, AppState>,
    backend_id: String,
) -> Result<(), String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        voice_backend_adapters::delete_voice_backend_adapter(&paths, backend_id.trim())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn voice_backend_adapter_probe(
    state: State<'_, AppState>,
    backend_id: String,
) -> Result<voice_backend_adapters::VoiceBackendAdapterDetail, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        voice_backend_adapters::probe_voice_backend_adapter(&paths, backend_id.trim())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn tools_tts_preview_pyttsx3_voices(
    state: State<'_, AppState>,
) -> Result<Vec<tools::Pyttsx3Voice>, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        tools::tts_preview_pyttsx3_list_voices(&paths).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn speakers_list(
    state: State<'_, AppState>,
    item_id: String,
) -> Result<Vec<speakers::ItemSpeakerSetting>, String> {
    speakers::list_item_speaker_settings(&state.paths, &item_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn speakers_upsert(
    state: State<'_, AppState>,
    item_id: String,
    speaker_key: String,
    display_name: Option<String>,
    voice_profile_id: Option<String>,
    tts_voice_id: Option<String>,
    tts_voice_profile_path: Option<String>,
    tts_voice_profile_paths: Option<Vec<String>>,
    style_preset: Option<String>,
    prosody_preset: Option<String>,
    pronunciation_overrides: Option<String>,
    render_mode: Option<String>,
    subtitle_prosody_mode: Option<String>,
) -> Result<speakers::ItemSpeakerSetting, String> {
    speakers::upsert_item_speaker_setting(
        &state.paths,
        &item_id,
        &speaker_key,
        display_name,
        voice_profile_id,
        tts_voice_id,
        tts_voice_profile_path,
        tts_voice_profile_paths,
        style_preset,
        prosody_preset,
        pronunciation_overrides,
        render_mode,
        subtitle_prosody_mode,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn voice_templates_list(
    state: State<'_, AppState>,
) -> Result<Vec<voice_templates::VoiceTemplate>, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        voice_templates::list_voice_templates(&paths).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn voice_templates_get(
    state: State<'_, AppState>,
    template_id: String,
) -> Result<voice_templates::VoiceTemplateDetail, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        voice_templates::get_voice_template(&paths, &template_id).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn voice_templates_create_from_item(
    state: State<'_, AppState>,
    item_id: String,
    name: String,
) -> Result<voice_templates::VoiceTemplateDetail, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        voice_templates::create_voice_template_from_item(&paths, &item_id, &name)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn voice_templates_delete(
    state: State<'_, AppState>,
    template_id: String,
) -> Result<(), String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        voice_templates::delete_voice_template(&paths, &template_id).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn voice_templates_update_speaker(
    state: State<'_, AppState>,
    template_id: String,
    speaker_key: String,
    update: voice_templates::VoiceTemplateSpeakerUpdate,
) -> Result<voice_templates::VoiceTemplateDetail, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        voice_templates::update_voice_template_speaker(&paths, &template_id, &speaker_key, update)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn voice_templates_add_reference(
    state: State<'_, AppState>,
    template_id: String,
    speaker_key: String,
    source_path: String,
    label: Option<String>,
) -> Result<voice_templates::VoiceTemplateDetail, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        voice_templates::add_voice_template_reference(
            &paths,
            &template_id,
            &speaker_key,
            &source_path,
            label,
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn voice_templates_remove_reference(
    state: State<'_, AppState>,
    template_id: String,
    speaker_key: String,
    reference_id: String,
) -> Result<voice_templates::VoiceTemplateDetail, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        voice_templates::remove_voice_template_reference(
            &paths,
            &template_id,
            &speaker_key,
            &reference_id,
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn voice_templates_apply_to_item(
    state: State<'_, AppState>,
    item_id: String,
    template_id: String,
    mappings: Vec<voice_templates::VoiceTemplateApplyMapping>,
    seed_voice_plan: bool,
) -> Result<Vec<speakers::ItemSpeakerSetting>, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        voice_templates::apply_voice_template_to_item(
            &paths,
            &item_id,
            &template_id,
            &mappings,
            seed_voice_plan,
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn voice_templates_clear_voice_plan_default(
    state: State<'_, AppState>,
    template_id: String,
) -> Result<voice_templates::VoiceTemplateDetail, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        voice_templates::clear_voice_template_voice_plan_default(&paths, &template_id)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn voice_templates_promote_benchmark_candidate_default(
    state: State<'_, AppState>,
    template_id: String,
    item_id: String,
    track_id: String,
    goal: Option<String>,
    candidate_id: String,
) -> Result<voice_templates::VoiceTemplateDetail, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        voice_templates::promote_benchmark_candidate_to_voice_template_voice_plan_default(
            &paths,
            &template_id,
            &item_id,
            &track_id,
            goal.as_deref(),
            &candidate_id,
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn voice_cast_packs_list(
    state: State<'_, AppState>,
) -> Result<Vec<voice_cast_packs::VoiceCastPack>, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        voice_cast_packs::list_voice_cast_packs(&paths).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn voice_cast_packs_get(
    state: State<'_, AppState>,
    pack_id: String,
) -> Result<voice_cast_packs::VoiceCastPackDetail, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        voice_cast_packs::get_voice_cast_pack(&paths, &pack_id).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn voice_cast_packs_create_from_template(
    state: State<'_, AppState>,
    template_id: String,
    name: String,
) -> Result<voice_cast_packs::VoiceCastPackDetail, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        voice_cast_packs::create_voice_cast_pack_from_template(&paths, &template_id, &name)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn voice_cast_packs_update(
    state: State<'_, AppState>,
    pack_id: String,
    name: String,
) -> Result<voice_cast_packs::VoiceCastPackDetail, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        voice_cast_packs::update_voice_cast_pack(&paths, &pack_id, &name).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn voice_cast_packs_delete(
    state: State<'_, AppState>,
    pack_id: String,
) -> Result<(), String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        voice_cast_packs::delete_voice_cast_pack(&paths, &pack_id).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn voice_cast_packs_apply_to_item(
    state: State<'_, AppState>,
    item_id: String,
    pack_id: String,
    mappings: Vec<voice_cast_packs::VoiceCastPackApplyMapping>,
    seed_voice_plan: bool,
) -> Result<Vec<speakers::ItemSpeakerSetting>, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        voice_cast_packs::apply_voice_cast_pack_to_item(
            &paths,
            &item_id,
            &pack_id,
            &mappings,
            seed_voice_plan,
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn voice_cast_packs_clear_voice_plan_default(
    state: State<'_, AppState>,
    pack_id: String,
) -> Result<voice_cast_packs::VoiceCastPackDetail, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        voice_cast_packs::clear_voice_cast_pack_voice_plan_default(&paths, &pack_id)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn voice_cast_packs_promote_benchmark_candidate_default(
    state: State<'_, AppState>,
    pack_id: String,
    item_id: String,
    track_id: String,
    goal: Option<String>,
    candidate_id: String,
) -> Result<voice_cast_packs::VoiceCastPackDetail, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        voice_cast_packs::promote_benchmark_candidate_to_voice_cast_pack_voice_plan_default(
            &paths,
            &pack_id,
            &item_id,
            &track_id,
            goal.as_deref(),
            &candidate_id,
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn voice_library_list(
    state: State<'_, AppState>,
    kind: Option<String>,
) -> Result<Vec<voice_library::VoiceLibraryProfile>, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        voice_library::list_voice_library_profiles(&paths, kind.as_deref())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn voice_library_get(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<voice_library::VoiceLibraryProfileDetail, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        voice_library::get_voice_library_profile(&paths, &profile_id).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn voice_library_create(
    state: State<'_, AppState>,
    kind: String,
    name: String,
    description: Option<String>,
) -> Result<voice_library::VoiceLibraryProfileDetail, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        voice_library::create_voice_library_profile(&paths, &kind, &name, description)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn voice_library_create_from_item_speaker(
    state: State<'_, AppState>,
    item_id: String,
    speaker_key: String,
    kind: String,
    name: String,
    description: Option<String>,
) -> Result<voice_library::VoiceLibraryProfileDetail, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        voice_library::create_voice_library_profile_from_item_speaker(
            &paths,
            &item_id,
            &speaker_key,
            &kind,
            &name,
            description,
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn voice_library_update(
    state: State<'_, AppState>,
    profile_id: String,
    update: voice_library::VoiceLibraryProfileUpdate,
) -> Result<voice_library::VoiceLibraryProfileDetail, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        voice_library::update_voice_library_profile(&paths, &profile_id, update)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn voice_library_add_reference(
    state: State<'_, AppState>,
    profile_id: String,
    source_path: String,
    label: Option<String>,
) -> Result<voice_library::VoiceLibraryProfileDetail, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        voice_library::add_voice_library_reference(&paths, &profile_id, &source_path, label)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn voice_library_remove_reference(
    state: State<'_, AppState>,
    profile_id: String,
    reference_id: String,
) -> Result<voice_library::VoiceLibraryProfileDetail, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        voice_library::remove_voice_library_reference(&paths, &profile_id, &reference_id)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn voice_library_apply_to_item(
    state: State<'_, AppState>,
    item_id: String,
    speaker_key: String,
    profile_id: String,
) -> Result<speakers::ItemSpeakerSetting, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        voice_library::apply_voice_library_profile_to_item(
            &paths,
            &item_id,
            &speaker_key,
            &profile_id,
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn voice_library_fork(
    state: State<'_, AppState>,
    profile_id: String,
    name: String,
) -> Result<voice_library::VoiceLibraryProfileDetail, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        voice_library::fork_voice_library_profile(&paths, &profile_id, &name)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn voice_library_delete(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<(), String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        voice_library::delete_voice_library_profile(&paths, &profile_id).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn voice_library_suggest_for_item(
    state: State<'_, AppState>,
    item_id: String,
    kind: Option<String>,
) -> Result<Vec<voice_library::VoiceLibrarySuggestion>, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        voice_library::suggest_voice_library_profiles_for_item(&paths, &item_id, kind.as_deref())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn voice_cleanup_run_for_speaker(
    state: State<'_, AppState>,
    item_id: String,
    speaker_key: String,
    source_path: String,
    options: Option<voice_cleanup::VoiceReferenceCleanupOptions>,
) -> Result<voice_cleanup::VoiceReferenceCleanupRecord, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        voice_cleanup::run_item_speaker_reference_cleanup(
            &paths,
            &item_id,
            &speaker_key,
            &source_path,
            options.unwrap_or_default(),
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn voice_cleanup_list_for_speaker(
    state: State<'_, AppState>,
    item_id: String,
    speaker_key: String,
) -> Result<Vec<voice_cleanup::VoiceReferenceCleanupRecord>, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        voice_cleanup::list_item_speaker_cleanups(&paths, &item_id, &speaker_key)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn library_list(
    state: State<'_, AppState>,
    limit: usize,
    offset: usize,
    file_status: Option<String>,
) -> Result<Vec<library::LibraryItem>, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "library_list");
    let paths = state.paths.clone();
    let trace_paths = paths.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        library::list_items_by_file_status(&paths, limit, offset, file_status.as_deref())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?;
    result.map_err(|e| trace_database_command_error(&trace_paths, "library_list", e))
}

#[tauri::command]
async fn library_query(
    state: State<'_, AppState>,
    limit: usize,
    offset: usize,
    file_status: Option<String>,
    query: Option<String>,
    media_type: Option<String>,
    source: Option<String>,
    single_video_only: Option<bool>,
    sort_by: Option<String>,
    direction: Option<String>,
    request_id: Option<String>,
    span_id: Option<String>,
) -> Result<library::LibraryPage, String> {
    let timer = InvokeTimer::start_with_context(
        state.paths.clone(),
        "library_query",
        request_id.clone(),
        span_id.clone(),
    );
    let phase_recorder = timer.phase_recorder();
    let paths = state.paths.clone();
    let trace_paths = paths.clone();
    let queued_at = Instant::now();
    let result = tauri::async_runtime::spawn_blocking(move || {
        phase_recorder.phase("dispatch_queue_wait", queued_at.elapsed());
        let db_started = Instant::now();
        let result = library::query_items_page(
            &paths,
            limit,
            offset,
            file_status.as_deref(),
            query.as_deref(),
            media_type.as_deref(),
            source.as_deref(),
            single_video_only.unwrap_or(false),
            sort_by.as_deref(),
            direction.as_deref(),
        )
        .map_err(|e| e.to_string());
        phase_recorder.phase("db_open_prepare_step_map", db_started.elapsed());
        result
    })
    .await
    .map_err(|e| e.to_string())?;
    let serialization_started = Instant::now();
    let _ = serde_json::to_vec(&result);
    timer.phase("serialization", serialization_started.elapsed());
    result.map_err(|e| trace_database_command_error(&trace_paths, "library_query", e))
}

#[tauri::command]
async fn library_file_delete(
    state: State<'_, AppState>,
    item_ids: Vec<String>,
    mode: String,
) -> Result<library::LibraryFileDeleteReceipt, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "library_file_delete");
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        library::delete_library_item_files(&paths, &item_ids, &mode, "operator")
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn library_operator_deleted_redownload(
    state: State<'_, AppState>,
    item_ids: Vec<String>,
    subscription_id: Option<String>,
) -> Result<jobs::ManualDeletedRedownloadReceipt, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "library_operator_deleted_redownload");
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        jobs::enqueue_manual_deleted_redownload(&paths, item_ids, subscription_id)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn library_list_youtube_video_candidates(
    state: State<'_, AppState>,
    limit: usize,
    offset: usize,
) -> Result<Vec<library::LibraryItem>, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "library_list_youtube_video_candidates");
    let paths = state.paths.clone();
    let trace_paths = paths.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        library::list_youtube_video_candidates(&paths, limit, offset).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?;
    result.map_err(|e| {
        trace_database_command_error(&trace_paths, "library_list_youtube_video_candidates", e)
    })
}

/// WP-0268 canonical single-video history. This intentionally has a new command name rather
/// than reusing the legacy broad-candidate command so consumers cannot mistake a heuristic list
/// for durable provenance.
#[tauri::command]
async fn library_youtube_single_history(
    state: State<'_, AppState>,
    limit: usize,
    offset: usize,
    query: Option<String>,
    direction: Option<String>,
) -> Result<library::YoutubeSingleHistoryPage, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "library_youtube_single_history");
    let paths = state.paths.clone();
    let trace_paths = paths.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        library::list_youtube_single_history(
            &paths,
            limit,
            offset,
            query.as_deref(),
            direction.as_deref(),
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?;
    result.map_err(|e| {
        trace_database_command_error(&trace_paths, "library_youtube_single_history", e)
    })
}

/// Secondary diagnostic count for the Single videos workspace. Kept separate from the canonical
/// page so a cold full-library legacy scan cannot delay navigation or history rendering.
#[tauri::command]
async fn library_youtube_single_unclassified_total(
    state: State<'_, AppState>,
) -> Result<usize, String> {
    let _timer = InvokeTimer::start(
        state.paths.clone(),
        "library_youtube_single_unclassified_total",
    );
    let paths = state.paths.clone();
    let trace_paths = paths.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        library::count_youtube_single_unclassified(&paths).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?;
    result.map_err(|error| {
        trace_database_command_error(
            &trace_paths,
            "library_youtube_single_unclassified_total",
            error,
        )
    })
}

/// Run one bounded legacy-provenance backfill step. Callers can repeat while `has_more` is true;
/// no schema migration or output-path inference occurs on the UI thread.
#[tauri::command]
async fn library_download_lineage_backfill_step(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Result<library::DownloadLineageBackfillState, String> {
    let _timer = InvokeTimer::start(
        state.paths.clone(),
        "library_download_lineage_backfill_step",
    );
    let paths = state.paths.clone();
    let trace_paths = paths.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        library::backfill_download_lineage_batch(&paths, limit.unwrap_or(200))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?;
    result.map_err(|e| {
        trace_database_command_error(&trace_paths, "library_download_lineage_backfill_step", e)
    })
}

#[tauri::command]
async fn library_resync_local_fallback(
    state: State<'_, AppState>,
) -> Result<library::FallbackResyncReport, String> {
    // WP-0253 Item 2d: move items saved to the local fallback during a NAS outage back
    // onto the configured root (copy -> verify -> relink -> delete-after-verify). No-op
    // when the root is unreachable or nothing fell back.
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        library::resync_local_fallback_downloads(&paths).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn localization_workspace_list(
    state: State<'_, AppState>,
    limit: usize,
    offset: usize,
) -> Result<Vec<library::LibraryItem>, String> {
    library::list_localization_workspace_items(&state.paths, limit, offset)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn library_get(
    state: State<'_, AppState>,
    item_id: String,
) -> Result<library::LibraryItem, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "library_get");
    library::get_item_by_id(&state.paths, &item_id)
        .map_err(|e| trace_database_command_error(&state.paths, "library_get", e.to_string()))
}

#[tauri::command]
async fn youtube_subscriptions_list(
    state: State<'_, AppState>,
) -> Result<Vec<subscriptions::YoutubeSubscriptionRow>, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "youtube_subscriptions_list");
    let paths = state.paths.clone();
    let trace_paths = paths.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        subscriptions::list_youtube_subscriptions(&paths).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?;
    result.map_err(|e| trace_database_command_error(&trace_paths, "youtube_subscriptions_list", e))
}

#[tauri::command]
fn youtube_subscriptions_output_dir(
    state: State<'_, AppState>,
    id: String,
) -> Result<String, String> {
    let sub = subscriptions::get_youtube_subscription_by_id(&state.paths, &id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("subscription not found: {id}"))?;
    subscriptions::youtube_subscription_output_dir(&state.paths, &sub)
        .map(|path| path.to_string_lossy().to_string())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn youtube_subscriptions_preview_output_dir(
    state: State<'_, AppState>,
    request: subscriptions::YoutubeSubscriptionOutputPreviewRequest,
) -> Result<subscriptions::YoutubeSubscriptionOutputPreview, String> {
    subscriptions::preview_youtube_subscription_output_dir(&state.paths, request)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn youtube_subscriptions_upsert(
    state: State<'_, AppState>,
    subscription: subscriptions::YoutubeSubscriptionUpsert,
) -> Result<subscriptions::YoutubeSubscriptionRow, String> {
    subscriptions::upsert_youtube_subscription(&state.paths, subscription)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn youtube_subscriptions_set_library(
    state: State<'_, AppState>,
    id: String,
    library_id: Option<String>,
) -> Result<subscriptions::YoutubeSubscriptionRow, String> {
    subscriptions::set_youtube_subscription_library(&state.paths, &id, library_id.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn youtube_subscriptions_set_manual_status(
    state: State<'_, AppState>,
    id: String,
    status: String,
) -> Result<subscriptions::YoutubeSubscriptionStatusChangeReceipt, String> {
    let receipt = subscriptions::set_youtube_subscription_manual_status(
        &state.paths,
        &id,
        &status,
        subscriptions::YOUTUBE_SUBSCRIPTION_STATUS_ACTOR_OPERATOR,
    )
    .map_err(|error| error.to_string())?;
    append_diagnostics_trace_row_best_effort(
        &state.paths,
        "subscription_manual_status_changed",
        serde_json::json!({
            "actor": subscriptions::YOUTUBE_SUBSCRIPTION_STATUS_ACTOR_OPERATOR,
            "subscription_id": receipt.subscription.id,
            "source_status": receipt.subscription.source_status,
            "canceled_refresh_jobs": receipt.canceled_refresh_jobs,
        }),
        "info",
    );
    Ok(receipt)
}

#[tauri::command]
fn youtube_subscriptions_delete(state: State<'_, AppState>, id: String) -> Result<(), String> {
    subscriptions::delete_youtube_subscription(&state.paths, &id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn video_libraries_list(
    state: State<'_, AppState>,
) -> Result<Vec<video_libraries::VideoLibraryRow>, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "video_libraries_list");
    let paths = state.paths.clone();
    let trace_paths = paths.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        video_libraries::list_video_libraries(&paths).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?;
    result.map_err(|e| trace_database_command_error(&trace_paths, "video_libraries_list", e))
}

#[tauri::command]
fn video_libraries_upsert(
    state: State<'_, AppState>,
    library: video_libraries::VideoLibraryUpsert,
) -> Result<video_libraries::VideoLibraryRow, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "video_libraries_upsert");
    video_libraries::upsert_video_library(&state.paths, library).map_err(|e| e.to_string())
}

#[tauri::command]
fn video_libraries_set_active(
    state: State<'_, AppState>,
    id: String,
) -> Result<video_libraries::VideoLibraryRow, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "video_libraries_set_active");
    video_libraries::set_active_video_library(&state.paths, &id).map_err(|e| e.to_string())
}

#[tauri::command]
fn video_libraries_remove(
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<video_libraries::VideoLibraryRow>, String> {
    video_libraries::remove_video_library(&state.paths, &id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn video_library_bundle_export(
    state: State<'_, AppState>,
    out_path: String,
) -> Result<video_libraries::VideoLibraryBundleSummary, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "video_library_bundle_export");
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        video_libraries::export_video_library_bundle(&paths, &std::path::PathBuf::from(out_path))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn video_library_bundle_import(
    state: State<'_, AppState>,
    in_path: String,
) -> Result<video_libraries::VideoLibraryBundleSummary, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "video_library_bundle_import");
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        video_libraries::import_video_library_bundle(&paths, &std::path::PathBuf::from(in_path))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn video_library_metadata_transfer(
    state: State<'_, AppState>,
    request: video_libraries::VideoLibraryMetadataTransferRequest,
) -> Result<video_libraries::VideoLibraryMetadataTransferSummary, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "video_library_metadata_transfer");
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        video_libraries::transfer_video_library_metadata(&paths, request).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn youtube_subscriptions_queue_one(
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<jobs::JobRow>, String> {
    subscriptions::queue_youtube_subscription(&state.paths, &id).map_err(|e| e.to_string())
}

// FREEZE FIX (2026-06-16): same as update_all — enqueuing the due subscriptions resolves
// each one's output dir (can stat a NAS path), so do it on a background thread instead of
// the UI thread. Returns immediately; jobs appear in the recurring lane as they are created.
#[tauri::command]
fn youtube_subscriptions_queue_all_active(state: State<'_, AppState>) -> Result<(), String> {
    let paths = state.paths.clone();
    std::thread::spawn(move || {
        if let Err(e) = subscriptions::queue_all_active_youtube_subscriptions(&paths) {
            append_diagnostics_trace_row_best_effort(
                &paths,
                "subscription_queue_due_error",
                serde_json::json!({ "error": e.to_string() }),
                "warn",
            );
        }
    });
    Ok(())
}

// WP-0254/WP-0255: "Update all subscriptions" button — clears any recurring "Stop" and
// refreshes every active subscription now (ignores the due gate). Feeds the conservative
// recurring lane so it cannot starve single one-off downloads.
//
// FREEZE FIX (2026-06-16): enqueuing 255 subscriptions resolves each one's output dir,
// which can stat a NAS path; doing that on the UI thread froze the app (badly during a NAS
// hiccup, where every stat hits the SMB timeout). Return immediately and do the enqueue on
// a background thread — jobs appear in the recurring lane (limit 1) as they are created.
#[tauri::command]
fn youtube_subscriptions_update_all(state: State<'_, AppState>) -> Result<(), String> {
    jobs::set_recurring_paused(&state.paths, false).map_err(|e| e.to_string())?;
    let paths = state.paths.clone();
    std::thread::spawn(move || {
        if let Err(e) = subscriptions::queue_all_active_youtube_subscriptions_now(&paths) {
            append_diagnostics_trace_row_best_effort(
                &paths,
                "subscription_update_all_error",
                serde_json::json!({ "error": e.to_string() }),
                "warn",
            );
        }
    });
    Ok(())
}

// WP-0254: "Stop" button — pauses only the recurring lane (playlist/channel/subscription
// syncing). Single one-off downloads and localization keep running. Queued recurring work
// is remembered and resumes on next startup or "Update all".
#[tauri::command]
fn youtube_subscriptions_stop_recurring(state: State<'_, AppState>) -> Result<bool, String> {
    jobs::set_recurring_paused(&state.paths, true).map_err(|e| e.to_string())
}

#[tauri::command]
fn youtube_subscriptions_recurring_paused(state: State<'_, AppState>) -> Result<bool, String> {
    jobs::is_recurring_paused(&state.paths).map_err(|e| e.to_string())
}

#[tauri::command]
fn youtube_subscriptions_export_json(
    state: State<'_, AppState>,
    out_path: String,
) -> Result<subscriptions::YoutubeSubscriptionsExportSummary, String> {
    subscriptions::export_youtube_subscriptions_json(
        &state.paths,
        &std::path::PathBuf::from(out_path),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn youtube_subscriptions_import_json(
    state: State<'_, AppState>,
    in_path: String,
) -> Result<subscriptions::YoutubeSubscriptionsImportSummary, String> {
    subscriptions::import_youtube_subscriptions_json(
        &state.paths,
        &std::path::PathBuf::from(in_path),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn youtube_subscriptions_import_4kvdp_dir(
    state: State<'_, AppState>,
    dir_path: String,
) -> Result<subscriptions::YoutubeSubscriptionsImport4kvdpSummary, String> {
    subscriptions::import_youtube_subscriptions_4kvdp_dir(
        &state.paths,
        &std::path::PathBuf::from(dir_path),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn youtube_subscriptions_import_4kvdp_state(
    state: State<'_, AppState>,
    root_path: String,
    sqlite_path: Option<String>,
) -> Result<subscriptions::YoutubeSubscriptionsImport4kvdpStateSummary, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let sqlite_path = sqlite_path
            .as_deref()
            .map(std::path::PathBuf::from)
            .unwrap_or_default();
        subscriptions::import_youtube_subscriptions_4kvdp_state(
            &paths,
            &std::path::PathBuf::from(root_path),
            if sqlite_path.as_os_str().is_empty() {
                None
            } else {
                Some(sqlite_path.as_path())
            },
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn youtube_imported_identity_enrich_4kvdp(
    state: State<'_, AppState>,
    sqlite_path: Option<String>,
    dry_run: Option<bool>,
    max_items: Option<usize>,
) -> Result<subscriptions::YoutubeImportedIdentityEnrichmentSummary, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let sqlite_path = sqlite_path
            .as_deref()
            .map(std::path::PathBuf::from)
            .unwrap_or_default();
        subscriptions::enrich_imported_youtube_identity_4kvdp(
            &paths,
            if sqlite_path.as_os_str().is_empty() {
                None
            } else {
                Some(sqlite_path.as_path())
            },
            dry_run.unwrap_or(true),
            max_items,
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn youtube_subscription_groups_list(
    state: State<'_, AppState>,
) -> Result<Vec<subscriptions::YoutubeSubscriptionGroupRow>, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "youtube_subscription_groups_list");
    let paths = state.paths.clone();
    let trace_paths = paths.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        subscriptions::list_youtube_subscription_groups(&paths).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?;
    result.map_err(|e| {
        trace_database_command_error(&trace_paths, "youtube_subscription_groups_list", e)
    })
}

#[tauri::command]
fn youtube_subscription_groups_upsert(
    state: State<'_, AppState>,
    group: subscriptions::YoutubeSubscriptionGroupUpsert,
) -> Result<subscriptions::YoutubeSubscriptionGroupRow, String> {
    subscriptions::upsert_youtube_subscription_group(&state.paths, group).map_err(|e| e.to_string())
}

#[tauri::command]
fn youtube_subscription_groups_delete(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    subscriptions::delete_youtube_subscription_group(&state.paths, &id).map_err(|e| e.to_string())
}

#[tauri::command]
fn youtube_subscription_groups_set_for_subscription(
    state: State<'_, AppState>,
    subscription_id: String,
    group_ids: Vec<String>,
) -> Result<Vec<String>, String> {
    subscriptions::set_youtube_subscription_groups(&state.paths, &subscription_id, group_ids)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn youtube_subscription_groups_clear_memberships(
    state: State<'_, AppState>,
) -> Result<usize, String> {
    subscriptions::clear_youtube_subscription_group_memberships(&state.paths)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn youtube_subscriptions_queue_group(
    state: State<'_, AppState>,
    group_id: String,
) -> Result<Vec<jobs::JobRow>, String> {
    subscriptions::queue_youtube_subscription_group(&state.paths, &group_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn youtube_subscriptions_seed_archive_scan(
    state: State<'_, AppState>,
    scan_dir: String,
    subscription_id: Option<String>,
) -> Result<subscriptions::YoutubeSubscriptionArchiveSeedSummary, String> {
    subscriptions::seed_archive_from_scan(
        &state.paths,
        &std::path::PathBuf::from(scan_dir),
        subscription_id,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn youtube_subscriptions_archive_stats(
    state: State<'_, AppState>,
    request_id: Option<String>,
    span_id: Option<String>,
) -> Result<std::collections::HashMap<String, usize>, String> {
    let timer = InvokeTimer::start_with_context(
        state.paths.clone(),
        "youtube_subscriptions_archive_stats",
        request_id,
        span_id,
    );
    let phase_recorder = timer.phase_recorder();
    let paths = state.paths.clone();
    let trace_paths = paths.clone();
    let queued_at = Instant::now();
    let result = tauri::async_runtime::spawn_blocking(move || {
        phase_recorder.phase("dispatch_queue_wait", queued_at.elapsed());
        let storage_started = Instant::now();
        let result =
            subscriptions::youtube_subscriptions_archive_stats(&paths).map_err(|e| e.to_string());
        phase_recorder.phase("db_storage", storage_started.elapsed());
        result
    })
    .await
    .map_err(|e| e.to_string())?;
    let serialization_started = Instant::now();
    let _ = serde_json::to_vec(&result);
    timer.phase("serialization", serialization_started.elapsed());
    result.map_err(|e| {
        trace_database_command_error(&trace_paths, "youtube_subscriptions_archive_stats", e)
    })
}

#[tauri::command]
async fn youtube_subscriptions_active_refresh_ids(
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    let _timer = InvokeTimer::start(
        state.paths.clone(),
        "youtube_subscriptions_active_refresh_ids",
    );
    let paths = state.paths.clone();
    let trace_paths = paths.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        jobs::active_youtube_subscription_refresh_ids(&paths).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?;
    result.map(|s| s.into_iter().collect()).map_err(|e| {
        trace_database_command_error(&trace_paths, "youtube_subscriptions_active_refresh_ids", e)
    })
}

// WP-0261: live per-subscription activity for the consumer "Processing now" signal.
#[tauri::command]
async fn youtube_subscriptions_activity(
    state: State<'_, AppState>,
) -> Result<Vec<subscriptions::SubscriptionActivityRow>, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "youtube_subscriptions_activity");
    let paths = state.paths.clone();
    let trace_paths = paths.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        subscriptions::youtube_subscriptions_activity(&paths).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?;
    result.map_err(|e| {
        trace_database_command_error(&trace_paths, "youtube_subscriptions_activity", e)
    })
}

// WP-0257 pacing follow-up: live per-subscription DOWNLOAD activity (queued + running child
// downloads and the current running title). Unlike youtube_subscriptions_activity, this keeps
// reporting a subscription's downloads after its refresh finished enumerating, so the UI can show
// downloads still draining. Read-only + bounded (aggregated in SQL, title query LIMIT-capped), so
// it never contends with the runner's writer path and cannot lock the DB.
#[tauri::command]
async fn subscription_download_activity(
    state: State<'_, AppState>,
    request_id: Option<String>,
    span_id: Option<String>,
) -> Result<Vec<subscriptions::SubscriptionDownloadActivityRow>, String> {
    let timer = InvokeTimer::start_with_context(
        state.paths.clone(),
        "subscription_download_activity",
        request_id,
        span_id,
    );
    let phase_recorder = timer.phase_recorder();
    let paths = state.paths.clone();
    let trace_paths = paths.clone();
    let queued_at = Instant::now();
    let result = tauri::async_runtime::spawn_blocking(move || {
        phase_recorder.phase("dispatch_queue_wait", queued_at.elapsed());
        let db_started = Instant::now();
        let result =
            subscriptions::subscription_download_activity(&paths).map_err(|e| e.to_string());
        phase_recorder.phase("db_open_prepare_step_map", db_started.elapsed());
        result
    })
    .await
    .map_err(|e| e.to_string())?;
    let serialization_started = Instant::now();
    let _ = serde_json::to_vec(&result);
    timer.phase("serialization", serialization_started.elapsed());
    result.map_err(|e| {
        trace_database_command_error(&trace_paths, "subscription_download_activity", e)
    })
}

#[tauri::command]
async fn subscription_projections_rebuild(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let timer = InvokeTimer::start(state.paths.clone(), "subscription_projections_rebuild");
    let phase_recorder = timer.phase_recorder();
    let paths = state.paths.clone();
    let queued_at = Instant::now();
    let result = tauri::async_runtime::spawn_blocking(move || {
        phase_recorder.phase("dispatch_queue_wait", queued_at.elapsed());
        let archive = subscriptions::rebuild_youtube_subscription_archive_rollups(&paths)
            .map_err(|error| error.to_string())?;
        let activity = subscriptions::rebuild_subscription_activity_rollup(&paths)
            .map_err(|error| error.to_string())?;
        Ok::<_, String>(serde_json::json!({
            "archive_subscription_count": archive.len(),
            "activity_subscription_count": activity.len(),
            "reconciled_at_ms": now_epoch_ms_i64(),
        }))
    })
    .await
    .map_err(|error| error.to_string())?;
    timer.phase("rebuild_reconciliation", timer.started.elapsed());
    result
}

// WP-0257/WP-0284: READ-ONLY per-subscription detail projections. Available and
// operator-deleted items come from canonical source membership; pending items come from queued
// download jobs. Runs on the blocking pool over bounded read-only queries, so it never locks the
// DB.
#[tauri::command]
async fn youtube_subscription_videos(
    state: State<'_, AppState>,
    subscription_id: String,
    limit: usize,
) -> Result<subscriptions::YoutubeSubscriptionVideos, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "youtube_subscription_videos");
    let paths = state.paths.clone();
    let trace_paths = paths.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        subscriptions::youtube_subscription_videos(&paths, &subscription_id, limit)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?;
    result.map_err(|e| trace_database_command_error(&trace_paths, "youtube_subscription_videos", e))
}

#[tauri::command]
async fn youtube_subscriptions_import_existing_downloads(
    state: State<'_, AppState>,
    scan_dir: String,
    max_depth: Option<usize>,
    max_files: Option<usize>,
) -> Result<subscriptions::ExistingDownloadsImportSummary, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        subscriptions::import_existing_downloads_index_only_with_limits(
            &paths,
            &std::path::PathBuf::from(scan_dir),
            max_depth,
            max_files,
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn legacy_archive_analyze(
    state: State<'_, AppState>,
    root_path: String,
    install_path: Option<String>,
    max_depth: Option<usize>,
    max_files: Option<usize>,
) -> Result<subscriptions::LegacyArchiveAnalysisSummary, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let install_path = install_path
            .as_deref()
            .map(std::path::PathBuf::from)
            .unwrap_or_default();
        subscriptions::analyze_legacy_archive_root(
            &paths,
            &std::path::PathBuf::from(root_path),
            if install_path.as_os_str().is_empty() {
                None
            } else {
                Some(install_path.as_path())
            },
            max_depth,
            max_files,
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn instagram_subscriptions_list(
    state: State<'_, AppState>,
) -> Result<Vec<instagram_subscriptions::InstagramSubscriptionRow>, String> {
    instagram_subscriptions::list_instagram_subscriptions(&state.paths).map_err(|e| {
        trace_database_command_error(&state.paths, "instagram_subscriptions_list", e.to_string())
    })
}

#[tauri::command]
fn instagram_subscriptions_upsert(
    state: State<'_, AppState>,
    subscription: instagram_subscriptions::InstagramSubscriptionUpsert,
) -> Result<instagram_subscriptions::InstagramSubscriptionRow, String> {
    instagram_subscriptions::upsert_instagram_subscription(&state.paths, subscription)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn instagram_subscriptions_delete(state: State<'_, AppState>, id: String) -> Result<(), String> {
    instagram_subscriptions::delete_instagram_subscription(&state.paths, &id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn instagram_subscriptions_queue_one(
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<jobs::JobRow>, String> {
    instagram_subscriptions::queue_instagram_subscription(&state.paths, &id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn instagram_subscriptions_queue_all_active(
    state: State<'_, AppState>,
) -> Result<Vec<jobs::JobRow>, String> {
    let _timer = InvokeTimer::start(
        state.paths.clone(),
        "instagram_subscriptions_queue_all_active",
    );
    instagram_subscriptions::queue_all_active_instagram_subscriptions(&state.paths).map_err(|e| {
        trace_database_command_error(
            &state.paths,
            "instagram_subscriptions_queue_all_active",
            e.to_string(),
        )
    })
}

#[tauri::command]
fn instagram_subscriptions_output_dir(
    state: State<'_, AppState>,
    id: String,
) -> Result<String, String> {
    let row = instagram_subscriptions::list_instagram_subscriptions(&state.paths)
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|sub| sub.id == id)
        .ok_or_else(|| format!("instagram subscription not found: {id}"))?;
    instagram_subscriptions::instagram_subscription_output_dir(&state.paths, &row)
        .map(|path| path.to_string_lossy().to_string())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn download_presets_get(
    state: State<'_, AppState>,
) -> Result<config::DownloadPresetsConfig, String> {
    config::load_download_presets_config(&state.paths).map_err(|e| e.to_string())
}

#[tauri::command]
fn download_presets_default_safety_patch(
    state: State<'_, AppState>,
    expected_default_preset_id: String,
    patch: config::DownloadPresetSafetyPatch,
) -> Result<config::DownloadPresetsConfig, String> {
    config::patch_default_download_preset_safety_fields(
        &state.paths,
        &expected_default_preset_id,
        &patch,
    )
    .map_err(|e| e.to_string())
}

fn preserve_options_owned_downloader_fields(
    current: &config::DownloadPresetsConfig,
    mut next: config::DownloadPresetsConfig,
) -> config::DownloadPresetsConfig {
    let current_default = current
        .default_preset_id
        .as_ref()
        .and_then(|id| current.presets.iter().find(|preset| &preset.id == id))
        .or_else(|| current.presets.first());
    if next.presets.is_empty() {
        next = config::DownloadPresetsConfig::default();
    }
    let next_default_id = next
        .default_preset_id
        .clone()
        .filter(|id| next.presets.iter().any(|preset| preset.id == *id))
        .or_else(|| next.presets.first().map(|preset| preset.id.clone()));
    let Some(current_default) = current_default else {
        return next;
    };
    let Some(next_default_id) = next_default_id else {
        return next;
    };
    next.default_preset_id = Some(next_default_id.clone());
    if let Some(next_default) = next
        .presets
        .iter_mut()
        .find(|preset| preset.id == next_default_id)
    {
        next_default.yt_dlp_concurrent_fragments = current_default.yt_dlp_concurrent_fragments;
        next_default.yt_dlp_limit_rate = current_default.yt_dlp_limit_rate.clone();
        next_default.yt_dlp_throttled_rate = current_default.yt_dlp_throttled_rate.clone();
        next_default.yt_dlp_file_access_retries = current_default.yt_dlp_file_access_retries;
        next_default.yt_dlp_retries = current_default.yt_dlp_retries;
        next_default.yt_dlp_fragment_retries = current_default.yt_dlp_fragment_retries;
        next_default.yt_dlp_sleep_interval = current_default.yt_dlp_sleep_interval;
        next_default.yt_dlp_sleep_requests = current_default.yt_dlp_sleep_requests;
    }
    next
}

#[tauri::command]
fn download_presets_catalog_set(
    state: State<'_, AppState>,
    config_value: config::DownloadPresetsConfig,
) -> Result<config::DownloadPresetsConfig, String> {
    config::update_download_presets_config(&state.paths, |current| {
        Ok(preserve_options_owned_downloader_fields(
            &current,
            config_value,
        ))
    })
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn download_presets_export_json(
    state: State<'_, AppState>,
    out_path: String,
) -> Result<(), String> {
    let config_value =
        config::load_download_presets_config(&state.paths).map_err(|e| e.to_string())?;
    let out_path = std::path::PathBuf::from(out_path);
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(&config_value).map_err(|e| e.to_string())?;
    std::fs::write(out_path, format!("{json}\n")).map_err(|e| e.to_string())
}

#[tauri::command]
fn download_presets_import_json(
    state: State<'_, AppState>,
    in_path: String,
) -> Result<config::DownloadPresetsConfig, String> {
    let bytes = std::fs::read(std::path::PathBuf::from(in_path)).map_err(|e| e.to_string())?;
    let parsed: config::DownloadPresetsConfig =
        serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    config::update_download_presets_config(&state.paths, |current| {
        Ok(preserve_options_owned_downloader_fields(&current, parsed))
    })
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn subtitles_list_tracks(
    state: State<'_, AppState>,
    item_id: String,
) -> Result<Vec<subtitle_tracks::SubtitleTrackRow>, String> {
    subtitle_tracks::list_tracks(&state.paths, &item_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn subtitles_load_track(
    state: State<'_, AppState>,
    track_id: String,
) -> Result<subtitles::SubtitleDocument, String> {
    subtitle_tracks::load_document(&state.paths, &track_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn subtitles_save_new_version(
    state: State<'_, AppState>,
    track_id: String,
    doc: subtitles::SubtitleDocument,
) -> Result<subtitle_tracks::SubtitleTrackRow, String> {
    subtitle_tracks::save_new_version(&state.paths, &track_id, doc).map_err(|e| e.to_string())
}

#[tauri::command]
fn subtitles_export_doc_srt(
    doc: subtitles::SubtitleDocument,
    out_path: String,
) -> Result<(), String> {
    let out_path = std::path::PathBuf::from(out_path);
    subtitle_tracks::export_document_srt(&doc, &out_path).map_err(|e| e.to_string())
}

#[tauri::command]
fn subtitles_export_doc_vtt(
    doc: subtitles::SubtitleDocument,
    out_path: String,
) -> Result<(), String> {
    let out_path = std::path::PathBuf::from(out_path);
    subtitle_tracks::export_document_vtt(&doc, &out_path).map_err(|e| e.to_string())
}

#[tauri::command]
async fn jobs_list(
    state: State<'_, AppState>,
    limit: usize,
    offset: usize,
) -> Result<Vec<jobs::JobRow>, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "jobs_list");
    let paths = state.paths.clone();
    let trace_paths = paths.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        jobs::list_jobs(&paths, limit, offset).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?;
    result.map_err(|e| trace_database_command_error(&trace_paths, "jobs_list", e))
}

#[tauri::command]
async fn jobs_list_live(state: State<'_, AppState>) -> Result<Vec<jobs::JobRow>, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "jobs_list_live");
    let paths = state.paths.clone();
    let trace_paths = paths.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        jobs::list_jobs_live_snapshot(&paths, 2_000, 2_000, 1_000).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?;
    result.map_err(|e| trace_database_command_error(&trace_paths, "jobs_list_live", e))
}

#[tauri::command]
async fn jobs_overview(
    state: State<'_, AppState>,
    view: Option<String>,
    track: Option<String>,
    request_id: Option<String>,
    span_id: Option<String>,
) -> Result<jobs::JobsOverviewSnapshot, String> {
    let timer =
        InvokeTimer::start_with_context(state.paths.clone(), "jobs_overview", request_id, span_id);
    let phase_recorder = timer.phase_recorder();
    let paths = state.paths.clone();
    let trace_paths = paths.clone();
    let queued_at = Instant::now();
    let result = tauri::async_runtime::spawn_blocking(move || {
        phase_recorder.phase("dispatch_queue_wait", queued_at.elapsed());
        let db_started = Instant::now();
        let result = jobs::jobs_overview_snapshot(&paths, view.as_deref(), track.as_deref())
            .map_err(|e| e.to_string());
        phase_recorder.phase("db_open_prepare_step_map", db_started.elapsed());
        result
    })
    .await
    .map_err(|e| e.to_string())?;
    let serialization_started = Instant::now();
    let _ = serde_json::to_vec(&result);
    timer.phase("serialization", serialization_started.elapsed());
    result.map_err(|e| trace_database_command_error(&trace_paths, "jobs_overview", e))
}

#[tauri::command]
async fn jobs_track_activity(
    state: State<'_, AppState>,
    track: String,
    limit: usize,
    offset: usize,
) -> Result<jobs::JobsTrackActivityPage, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "jobs_track_activity");
    let paths = state.paths.clone();
    let trace_paths = paths.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        jobs::jobs_track_activity_page(&paths, &track, limit, offset).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?;
    result.map_err(|e| trace_database_command_error(&trace_paths, "jobs_track_activity", e))
}

#[tauri::command]
async fn jobs_progress_many(
    state: State<'_, AppState>,
    job_ids: Vec<String>,
) -> Result<Vec<jobs::JobRow>, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "jobs_progress_many");
    let paths = state.paths.clone();
    let trace_paths = paths.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        jobs::jobs_progress_many(&paths, &job_ids).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?;
    result.map_err(|e| trace_database_command_error(&trace_paths, "jobs_progress_many", e))
}

#[tauri::command]
async fn jobs_search(
    state: State<'_, AppState>,
    query: String,
    limit: usize,
    track: Option<String>,
) -> Result<Vec<jobs::JobRow>, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "jobs_search");
    let paths = state.paths.clone();
    let trace_paths = paths.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        jobs::search_jobs_for_track(&paths, &query, limit, track.as_deref())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?;
    result.map_err(|e| trace_database_command_error(&trace_paths, "jobs_search", e))
}

#[tauri::command]
async fn jobs_list_for_item(
    state: State<'_, AppState>,
    item_id: Option<String>,
    itemId: Option<String>,
    limit: usize,
    offset: usize,
) -> Result<Vec<jobs::JobRow>, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "jobs_list_for_item");
    let item_id = item_id
        .or(itemId)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| "missing required key itemId".to_string())?;
    let paths = state.paths.clone();
    let trace_paths = paths.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        jobs::list_jobs_for_item(&paths, &item_id, limit, offset).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?;
    result.map_err(|e| trace_database_command_error(&trace_paths, "jobs_list_for_item", e))
}

#[tauri::command]
fn jobs_enqueue_import_local(
    state: State<'_, AppState>,
    path: String,
    add_to_localization_workspace: Option<bool>,
    apply_batch_on_import: Option<bool>,
) -> Result<jobs::JobRow, String> {
    jobs::enqueue_import_local(
        &state.paths,
        path,
        add_to_localization_workspace.unwrap_or(false),
        apply_batch_on_import.unwrap_or(true),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn jobs_enqueue_install_phase2_packs_v1(
    state: State<'_, AppState>,
) -> Result<jobs::JobRow, String> {
    jobs::enqueue_install_phase2_packs_v1(&state.paths).map_err(|e| e.to_string())
}

#[tauri::command]
fn jobs_enqueue_download_batch(
    state: State<'_, AppState>,
    urls: Vec<String>,
    auth_cookie: Option<String>,
    output_dir: Option<String>,
    use_browser_cookies: Option<bool>,
    browser_cookie_source: Option<String>,
    preset_id: Option<String>,
    approved_missing_item_ids: Option<Vec<String>>,
) -> Result<Vec<jobs::JobRow>, String> {
    jobs::enqueue_download_direct_url_batch_with_repairs(
        &state.paths,
        urls,
        auth_cookie,
        output_dir,
        use_browser_cookies,
        browser_cookie_source,
        preset_id,
        approved_missing_item_ids.unwrap_or_default(),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn library_download_preflight(
    state: State<'_, AppState>,
    urls: Vec<String>,
    request_id: Option<String>,
    span_id: Option<String>,
) -> Result<Vec<library::DownloadPreflightRow>, String> {
    let causal_request_id = request_id.clone();
    let causal_span_id = span_id.clone();
    let (causal_incident_id, _, _, _) = diagnostics_capture_envelope(
        &state.paths,
        "media_path_probe_context",
        &serde_json::json!({
            "request_id": causal_request_id.clone(),
            "span_id": causal_span_id.clone(),
        }),
    );
    let timer = InvokeTimer::start_with_context(
        state.paths.clone(),
        "library_download_preflight",
        request_id,
        span_id,
    );
    let phase_recorder = timer.phase_recorder();
    let paths = state.paths.clone();
    let queued_at = Instant::now();
    let result = tauri::async_runtime::spawn_blocking(move || {
        phase_recorder.phase("dispatch_queue_wait", queued_at.elapsed());
        let db_storage_started = Instant::now();
        let causal = library::MediaProbeCausalEnvelope {
            request_id: causal_request_id,
            span_id: causal_span_id,
            incident_id: causal_incident_id,
        };
        let result = library::preflight_download_urls_with_causal(&paths, &urls, Some(&causal))
            .map_err(|e| e.to_string());
        phase_recorder.phase("db_storage_observation", db_storage_started.elapsed());
        result
    })
    .await
    .map_err(|e| e.to_string())?;
    let serialization_started = Instant::now();
    let _ = serde_json::to_vec(&result);
    timer.phase("serialization", serialization_started.elapsed());
    result
}

#[tauri::command]
async fn library_canonical_media_relocate(
    state: State<'_, AppState>,
    item_id: String,
    new_path: String,
) -> Result<library::LibraryItem, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        library::relocate_canonical_media(&paths, &item_id, std::path::Path::new(&new_path))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn library_canonical_source_replace(
    state: State<'_, AppState>,
    service: String,
    media_id: String,
    new_url: String,
) -> Result<(), String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        library::replace_canonical_source_url(&paths, &service, &media_id, &new_url)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn library_canonical_record_remove(
    state: State<'_, AppState>,
    item_id: String,
) -> Result<bool, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        library::remove_canonical_library_record(&paths, &item_id).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn jobs_enqueue_instagram_batch(
    state: State<'_, AppState>,
    urls: Vec<String>,
    auth_cookie: Option<String>,
    output_dir: Option<String>,
    use_browser_cookies: Option<bool>,
    browser_cookie_source: Option<String>,
) -> Result<Vec<jobs::JobRow>, String> {
    jobs::enqueue_download_instagram_batch(
        &state.paths,
        urls,
        auth_cookie,
        output_dir,
        use_browser_cookies,
        browser_cookie_source,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn jobs_enqueue_image_batch(
    state: State<'_, AppState>,
    start_urls: Vec<String>,
    max_pages: Option<usize>,
    delay_ms: Option<u64>,
    allow_cross_domain: Option<bool>,
    follow_content_links: Option<bool>,
    skip_url_keywords: Option<Vec<String>>,
    output_subdir: Option<String>,
    output_dir: Option<String>,
    auth_cookie: Option<String>,
) -> Result<jobs::JobRow, String> {
    jobs::enqueue_download_image_batch(
        &state.paths,
        start_urls,
        max_pages,
        delay_ms,
        allow_cross_domain,
        follow_content_links,
        skip_url_keywords.unwrap_or_default(),
        output_subdir,
        output_dir,
        auth_cookie,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn jobs_enqueue_dummy(state: State<'_, AppState>, seconds: u64) -> Result<jobs::JobRow, String> {
    jobs::enqueue_dummy_sleep(&state.paths, seconds).map_err(|e| e.to_string())
}

#[tauri::command]
fn jobs_enqueue_asr_local(
    state: State<'_, AppState>,
    item_id: String,
    lang: Option<String>,
) -> Result<jobs::JobRow, String> {
    jobs::enqueue_asr_local(&state.paths, item_id, lang).map_err(|e| e.to_string())
}

#[tauri::command]
fn jobs_enqueue_translate_local(
    state: State<'_, AppState>,
    item_id: String,
    source_track_id: String,
) -> Result<jobs::JobRow, String> {
    jobs::enqueue_translate_local(&state.paths, item_id, source_track_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn jobs_enqueue_diarize_local_v1(
    state: State<'_, AppState>,
    item_id: Option<String>,
    itemId: Option<String>,
    source_track_id: Option<String>,
    sourceTrackId: Option<String>,
    backend: Option<String>,
    speaker_count: Option<jobs::DiarizationSpeakerCountRequest>,
    speakerCount: Option<jobs::DiarizationSpeakerCountRequest>,
) -> Result<jobs::JobRow, String> {
    let item_id = item_id
        .or(itemId)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| "missing required key itemId".to_string())?;
    let source_track_id = source_track_id
        .or(sourceTrackId)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| "missing required key sourceTrackId".to_string())?;

    jobs::enqueue_diarize_local_v1_with_backend_and_speaker_count(
        &state.paths,
        item_id,
        source_track_id,
        backend,
        speaker_count.or(speakerCount).unwrap_or_default(),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn jobs_enqueue_tts_preview_pyttsx3_v1(
    state: State<'_, AppState>,
    item_id: String,
    source_track_id: String,
) -> Result<jobs::JobRow, String> {
    jobs::enqueue_tts_preview_pyttsx3_v1(&state.paths, item_id, source_track_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn jobs_enqueue_tts_neural_local_v1(
    state: State<'_, AppState>,
    item_id: String,
    source_track_id: String,
) -> Result<jobs::JobRow, String> {
    jobs::enqueue_tts_neural_local_v1(&state.paths, item_id, source_track_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn jobs_enqueue_dub_voice_preserving_v1(
    state: State<'_, AppState>,
    item_id: String,
    source_track_id: String,
) -> Result<jobs::JobRow, String> {
    jobs::enqueue_dub_voice_preserving_v1(&state.paths, item_id, source_track_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn jobs_enqueue_experimental_voice_backend_render_v1(
    state: State<'_, AppState>,
    item_id: Option<String>,
    itemId: Option<String>,
    source_track_id: Option<String>,
    sourceTrackId: Option<String>,
    backend_id: Option<String>,
    backendId: Option<String>,
    variant_label: Option<String>,
    variantLabel: Option<String>,
    auto_pipeline: Option<bool>,
    autoPipeline: Option<bool>,
    separation_backend: Option<String>,
    separationBackend: Option<String>,
    queue_qc: Option<bool>,
    queueQc: Option<bool>,
    queue_export_pack: Option<bool>,
    queueExportPack: Option<bool>,
) -> Result<jobs::JobRow, String> {
    let item_id = item_id
        .or(itemId)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| "missing required key itemId".to_string())?;
    let source_track_id = source_track_id
        .or(sourceTrackId)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| "missing required key sourceTrackId".to_string())?;
    let backend_id = backend_id
        .or(backendId)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| "missing required key backendId".to_string())?;

    jobs::enqueue_experimental_voice_backend_render_v1(
        &state.paths,
        item_id,
        source_track_id,
        backend_id,
        variant_label.or(variantLabel),
        auto_pipeline.or(autoPipeline).unwrap_or(true),
        separation_backend.or(separationBackend),
        queue_qc.or(queueQc).unwrap_or(true),
        queue_export_pack.or(queueExportPack).unwrap_or(false),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn jobs_enqueue_experimental_backend_batch_v1(
    state: State<'_, AppState>,
    request: jobs::ExperimentalBackendBatchRequest,
) -> Result<jobs::ExperimentalBackendBatchQueueSummary, String> {
    jobs::enqueue_experimental_backend_batch_v1(&state.paths, request).map_err(|e| e.to_string())
}

#[tauri::command]
fn jobs_enqueue_mix_dub_preview_v1(
    state: State<'_, AppState>,
    item_id: Option<String>,
    itemId: Option<String>,
    ducking_strength: Option<f32>,
    duckingStrength: Option<f32>,
    loudness_target_lufs: Option<f32>,
    loudnessTargetLufs: Option<f32>,
    timing_fit_enabled: Option<bool>,
    timingFitEnabled: Option<bool>,
    timing_fit_min_factor: Option<f32>,
    timingFitMinFactor: Option<f32>,
    timing_fit_max_factor: Option<f32>,
    timingFitMaxFactor: Option<f32>,
) -> Result<jobs::JobRow, String> {
    let item_id = item_id
        .or(itemId)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| "missing required key itemId".to_string())?;

    jobs::enqueue_mix_dub_preview_v1_with_options(
        &state.paths,
        item_id,
        ducking_strength.or(duckingStrength),
        loudness_target_lufs.or(loudnessTargetLufs),
        timing_fit_enabled.or(timingFitEnabled),
        timing_fit_min_factor.or(timingFitMinFactor),
        timing_fit_max_factor.or(timingFitMaxFactor),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn jobs_enqueue_mux_dub_preview_v1(
    state: State<'_, AppState>,
    item_id: Option<String>,
    itemId: Option<String>,
    output_container: Option<String>,
    outputContainer: Option<String>,
    keep_original_audio: Option<bool>,
    keepOriginalAudio: Option<bool>,
    dubbed_audio_lang: Option<String>,
    dubbedAudioLang: Option<String>,
    original_audio_lang: Option<String>,
    originalAudioLang: Option<String>,
) -> Result<jobs::JobRow, String> {
    let item_id = item_id
        .or(itemId)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| "missing required key itemId".to_string())?;

    jobs::enqueue_mux_dub_preview_v1_with_options(
        &state.paths,
        item_id,
        output_container.or(outputContainer),
        keep_original_audio.or(keepOriginalAudio),
        dubbed_audio_lang.or(dubbedAudioLang),
        original_audio_lang.or(originalAudioLang),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn jobs_enqueue_separate_audio_spleeter(
    state: State<'_, AppState>,
    item_id: String,
) -> Result<jobs::JobRow, String> {
    jobs::enqueue_separate_audio_spleeter(&state.paths, item_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn jobs_enqueue_separate_audio_demucs_v1(
    state: State<'_, AppState>,
    item_id: String,
) -> Result<jobs::JobRow, String> {
    jobs::enqueue_separate_audio_demucs_v1(&state.paths, item_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn jobs_enqueue_clean_vocals_v1(
    state: State<'_, AppState>,
    item_id: String,
) -> Result<jobs::JobRow, String> {
    jobs::enqueue_clean_vocals_v1(&state.paths, item_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn jobs_enqueue_qc_report_v1(
    state: State<'_, AppState>,
    item_id: String,
    track_id: String,
    variant_label: Option<String>,
    variantLabel: Option<String>,
) -> Result<jobs::JobRow, String> {
    let variant_label = variant_label.or(variantLabel);
    if variant_label
        .as_deref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .is_some()
    {
        jobs::enqueue_qc_report_v1_with_variant(&state.paths, item_id, track_id, variant_label)
            .map_err(|e| e.to_string())
    } else {
        jobs::enqueue_qc_report_v1(&state.paths, item_id, track_id).map_err(|e| e.to_string())
    }
}

#[tauri::command]
fn jobs_enqueue_export_pack_v1(
    state: State<'_, AppState>,
    item_id: String,
) -> Result<jobs::JobRow, String> {
    jobs::enqueue_export_pack_v1(&state.paths, item_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn jobs_enqueue_localization_batch_v1(
    state: State<'_, AppState>,
    request: jobs::LocalizationBatchRequest,
) -> Result<jobs::LocalizationBatchQueueSummary, String> {
    jobs::enqueue_localization_batch_v1(&state.paths, request).map_err(|e| e.to_string())
}

#[tauri::command]
fn jobs_enqueue_localization_run_v1(
    state: State<'_, AppState>,
    request: jobs::LocalizationRunRequest,
) -> Result<jobs::LocalizationRunQueueSummary, String> {
    jobs::enqueue_localization_run_v1(&state.paths, request).map_err(|e| e.to_string())
}

#[tauri::command]
fn jobs_enqueue_voice_ab_preview_v1(
    state: State<'_, AppState>,
    request: jobs::VoiceAbPreviewRequest,
) -> Result<jobs::VoiceAbPreviewQueueSummary, String> {
    jobs::enqueue_voice_ab_preview_v1(&state.paths, request).map_err(|e| e.to_string())
}

#[tauri::command]
#[allow(non_snake_case)]
fn jobs_cancel(
    state: State<'_, AppState>,
    job_id: Option<String>,
    jobId: Option<String>,
) -> Result<(), String> {
    let job_id = job_id
        .or(jobId)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| "missing required key jobId".to_string())?;
    jobs::cancel_job(&state.paths, &job_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn jobs_cancel_all(state: State<'_, AppState>) -> Result<usize, String> {
    jobs::cancel_all_jobs(&state.paths).map_err(|e| e.to_string())
}

#[tauri::command]
#[allow(non_snake_case)]
fn jobs_delete_terminal(
    state: State<'_, AppState>,
    job_id: Option<String>,
    jobId: Option<String>,
) -> Result<bool, String> {
    let job_id = job_id
        .or(jobId)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| "missing required key jobId".to_string())?;
    jobs::delete_terminal_job(&state.paths, &job_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn jobs_delete_terminal_matching_search(
    state: State<'_, AppState>,
    query: String,
    limit: usize,
) -> Result<jobs::ClearTerminalJobsSearchSummary, String> {
    jobs::delete_terminal_jobs_matching_search(&state.paths, &query, limit)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn jobs_queue_control_get(
    state: State<'_, AppState>,
) -> Result<jobs::JobQueueControlState, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "jobs_queue_control_get");
    let paths = state.paths.clone();
    let trace_paths = paths.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        jobs::get_queue_control(&paths).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?;
    result.map_err(|e| trace_database_command_error(&trace_paths, "jobs_queue_control_get", e))
}

#[tauri::command]
async fn jobs_queue_control_set(
    state: State<'_, AppState>,
    paused: bool,
) -> Result<jobs::JobQueueControlState, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "jobs_queue_control_set");
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        jobs::set_queue_paused(&paths, paused).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn youtube_queue_identity_reconcile(
    state: State<'_, AppState>,
    dry_run: Option<bool>,
    after_job_id: Option<String>,
    limit: Option<usize>,
) -> Result<jobs::YoutubeQueueIdentityReconcileSummary, String> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        jobs::youtube_queue_identity_reconcile(
            &paths,
            dry_run.unwrap_or(true),
            after_job_id.as_deref(),
            limit,
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn media_cleanup_get(
    state: State<'_, AppState>,
    run_id: String,
) -> Result<Option<media_cleanup::MediaCleanupRun>, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "media_cleanup_get");
    media_cleanup::get_run(&state.paths, &run_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn media_cleanup_latest(
    state: State<'_, AppState>,
) -> Result<Option<media_cleanup::MediaCleanupRun>, String> {
    media_cleanup::latest_run(&state.paths).map_err(|e| e.to_string())
}

#[tauri::command]
fn media_cleanup_create(
    state: State<'_, AppState>,
    roots: Vec<String>,
    quarantine_root: Option<String>,
) -> Result<media_cleanup::MediaCleanupRun, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "media_cleanup_create");
    media_cleanup::create_inventory_run(&state.paths, roots, quarantine_root)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn media_cleanup_inventory_advance(
    state: State<'_, AppState>,
    run_id: String,
    max_files: Option<usize>,
) -> Result<media_cleanup::MediaCleanupAdvanceSummary, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "media_cleanup_inventory_advance");
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        media_cleanup::advance_inventory(&paths, &run_id, max_files).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn media_cleanup_hash_advance(
    state: State<'_, AppState>,
    run_id: String,
    max_files: Option<usize>,
) -> Result<media_cleanup::MediaCleanupAdvanceSummary, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "media_cleanup_hash_advance");
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        media_cleanup::advance_hashing(&paths, &run_id, max_files).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn media_cleanup_groups(
    state: State<'_, AppState>,
    run_id: String,
) -> Result<Vec<media_cleanup::MediaCleanupGroup>, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "media_cleanup_groups");
    media_cleanup::list_groups(&state.paths, &run_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn media_cleanup_group_decide(
    state: State<'_, AppState>,
    run_id: String,
    group_id: String,
    decision: String,
    keeper_path: Option<String>,
) -> Result<media_cleanup::MediaCleanupGroup, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "media_cleanup_group_decide");
    media_cleanup::set_group_decision(
        &state.paths,
        &run_id,
        &group_id,
        &decision,
        keeper_path.as_deref(),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn media_cleanup_apply(
    state: State<'_, AppState>,
    run_id: String,
) -> Result<media_cleanup::MediaCleanupApplySummary, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "media_cleanup_apply");
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        media_cleanup::apply_approved_groups(&paths, &run_id).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn media_cleanup_rollback(
    state: State<'_, AppState>,
    run_id: String,
) -> Result<media_cleanup::MediaCleanupApplySummary, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "media_cleanup_rollback");
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        media_cleanup::rollback_run(&paths, &run_id).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn jobs_runtime_settings_get(
    state: State<'_, AppState>,
) -> Result<jobs::JobRuntimeSettings, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "jobs_runtime_settings_get");
    jobs::get_runtime_settings(&state.paths).map_err(|e| {
        trace_database_command_error(&state.paths, "jobs_runtime_settings_get", e.to_string())
    })
}

#[tauri::command]
fn jobs_runtime_settings_set(
    state: State<'_, AppState>,
    max_concurrency: usize,
) -> Result<jobs::JobRuntimeSettings, String> {
    // Compatibility-only: WP-0269's scheduler does not read this global setting. New product
    // controls must use `jobs_track_runtime_get/set` below.
    jobs::set_runtime_max_concurrency(&state.paths, max_concurrency).map_err(|e| e.to_string())
}

/// WP-0270 canonical controls/snapshot getter. The response is intentionally identical to
/// GET /agent/jobs_tracks and the `jobs_tracks` diagnostics-app-state field.
#[tauri::command]
async fn jobs_track_runtime_get(
    state: State<'_, AppState>,
) -> Result<jobs::JobTracksRuntimeSnapshot, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "jobs_track_runtime_get");
    let paths = state.paths.clone();
    let trace_paths = paths.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        jobs::get_job_tracks_runtime_snapshot(&paths).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?;
    result.map_err(|e| trace_database_command_error(&trace_paths, "jobs_track_runtime_get", e))
}

/// WP-0270 atomic six-track setting update. Engine validation happens before the transaction;
/// after commit this returns the same canonical snapshot a fresh runner/bridge read observes.
#[tauri::command]
fn jobs_track_runtime_set(
    state: State<'_, AppState>,
    settings: jobs::JobTrackRuntimeSettings,
) -> Result<jobs::JobTracksRuntimeSnapshot, String> {
    jobs::set_job_track_runtime_settings(&state.paths, settings)
        .map_err(|error| error.to_string())?;
    jobs::get_job_tracks_runtime_snapshot(&state.paths).map_err(|error| error.to_string())
}

// WP-0257 (#3/#4): operator-tunable anti-bot pacing (Options -> Anti-bot pacing).
#[tauri::command]
fn antibot_pacing_get(state: State<'_, AppState>) -> Result<jobs::AntiBotPacingSettings, String> {
    jobs::get_antibot_pacing(&state.paths).map_err(|e| e.to_string())
}

#[tauri::command]
fn antibot_pacing_set(
    state: State<'_, AppState>,
    settings: jobs::AntiBotPacingSettings,
    mutation_generation: u64,
) -> Result<jobs::AntiBotPacingSettings, String> {
    run_youtube_protection_mutation("pacing", mutation_generation, || {
        jobs::set_antibot_pacing_with_generation(&state.paths, settings, mutation_generation)
            .map_err(|e| e.to_string())
    })
}

#[tauri::command]
async fn youtube_protection_status_get(
    state: State<'_, AppState>,
    operation: Option<String>,
    request_id: Option<String>,
    span_id: Option<String>,
) -> Result<jobs::YoutubeProtectionStatus, String> {
    let timer = InvokeTimer::start_with_context(state.paths.clone(), "youtube_protection_status_get", request_id, span_id);
    timer.phase("blocking_dispatch", Duration::ZERO);
    let recorder = timer.phase_recorder();
    let paths = state.paths.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let started = std::time::Instant::now();
        let result = jobs::get_youtube_protection_status(&paths, operation.as_deref())
            .map_err(|error| error.to_string());
        recorder.phase("policy_db_and_provider_health", started.elapsed());
        result
    }).await.map_err(|error| error.to_string())?;
    result
}

#[tauri::command]
async fn youtube_protection_return_to_baseline(
    state: State<'_, AppState>,
    operation: Option<String>,
    request_id: Option<String>,
    span_id: Option<String>,
) -> Result<jobs::YoutubeProtectionStatus, String> {
    let timer = InvokeTimer::start_with_context(state.paths.clone(), "youtube_protection_return_to_baseline", request_id, span_id);
    let recorder = timer.phase_recorder();
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let started = std::time::Instant::now();
        let result = jobs::return_youtube_protection_to_baseline(&paths, operation.as_deref()).map_err(|error| error.to_string());
        recorder.phase("policy_mutation", started.elapsed());
        result
    }).await.map_err(|error| error.to_string())?
}

#[tauri::command]
async fn youtube_protection_history_get(
    state: State<'_, AppState>,
    operation: Option<String>,
    limit: Option<usize>,
    request_id: Option<String>,
    span_id: Option<String>,
) -> Result<voxvulgi_engine::youtube_protection::DownloaderPolicyHistory, String> {
    let timer = InvokeTimer::start_with_context(state.paths.clone(), "youtube_protection_history_get", request_id, span_id);
    let recorder = timer.phase_recorder();
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let started = std::time::Instant::now();
        let result = jobs::get_youtube_protection_history(&paths, operation.as_deref(), limit.unwrap_or(100)).map_err(|error| error.to_string());
        recorder.phase("policy_history_page", started.elapsed());
        result
    }).await.map_err(|error| error.to_string())?
}

#[tauri::command]
async fn youtube_protection_history_replay(
    state: State<'_, AppState>,
    operation: Option<String>,
    limit: Option<usize>,
    request_id: Option<String>,
    span_id: Option<String>,
) -> Result<voxvulgi_engine::youtube_protection::DownloaderPolicyReplayReceipt, String> {
    let timer = InvokeTimer::start_with_context(state.paths.clone(), "youtube_protection_history_replay", request_id, span_id);
    let recorder = timer.phase_recorder();
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let started = std::time::Instant::now();
        let result = jobs::replay_youtube_protection_history(&paths, operation.as_deref(), limit.unwrap_or(500)).map_err(|error| error.to_string());
        recorder.phase("policy_replay", started.elapsed());
        result
    }).await.map_err(|error| error.to_string())?
}

#[tauri::command]
async fn youtube_protection_tuning_get(
    state: State<'_, AppState>,
    request_id: Option<String>,
    span_id: Option<String>,
) -> Result<voxvulgi_engine::youtube_protection::YoutubeProtectionTuning, String> {
    let _timer = InvokeTimer::start_with_context(state.paths.clone(), "youtube_protection_tuning_get", request_id, span_id);
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || jobs::get_youtube_protection_tuning(&paths).map_err(|e| e.to_string()))
        .await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn youtube_protection_tuning_set(
    state: State<'_, AppState>,
    tuning: voxvulgi_engine::youtube_protection::YoutubeProtectionTuning,
    mutation_generation: u64,
    request_id: Option<String>,
    span_id: Option<String>,
) -> Result<voxvulgi_engine::youtube_protection::YoutubeProtectionTuning, String> {
    let _timer = InvokeTimer::start_with_context(state.paths.clone(), "youtube_protection_tuning_set", request_id, span_id);
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || run_youtube_protection_mutation("tuning", mutation_generation, || jobs::set_youtube_protection_tuning_with_generation(&paths, tuning, mutation_generation).map_err(|e| e.to_string())))
        .await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn youtube_protection_tuning_reset(
    state: State<'_, AppState>,
    mutation_generation: u64,
    request_id: Option<String>,
    span_id: Option<String>,
) -> Result<voxvulgi_engine::youtube_protection::YoutubeProtectionTuning, String> {
    let _timer = InvokeTimer::start_with_context(state.paths.clone(), "youtube_protection_tuning_reset", request_id, span_id);
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || run_youtube_protection_mutation("tuning", mutation_generation, || jobs::reset_youtube_protection_tuning_with_generation(&paths, mutation_generation).map_err(|e| e.to_string())))
        .await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn youtube_protection_history_export(
    state: State<'_, AppState>,
    operation: Option<String>,
    request_id: Option<String>,
    span_id: Option<String>,
) -> Result<jobs::YoutubeProtectionHistoryExportReceipt, String> {
    let timer = InvokeTimer::start_with_context(state.paths.clone(), "youtube_protection_history_export", request_id, span_id);
    let recorder = timer.phase_recorder();
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let started = std::time::Instant::now();
        let result = jobs::export_youtube_protection_history(&paths, operation.as_deref()).map_err(|e| e.to_string());
        recorder.phase("policy_export_stream", started.elapsed());
        result
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn youtube_protection_history_reset(
    state: State<'_, AppState>,
    operation: Option<String>,
    mutation_generation: u64,
    request_id: Option<String>,
    span_id: Option<String>,
) -> Result<voxvulgi_engine::youtube_protection::DownloaderHistoryResetReceipt, String> {
    let timer = InvokeTimer::start_with_context(state.paths.clone(), "youtube_protection_history_reset", request_id, span_id);
    let recorder = timer.phase_recorder();
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let started = std::time::Instant::now();
        // Download and enumeration batches are one operator reset intent. A newer first batch
        // invalidates every older continuation, including the other policy sub-operation.
        let result = run_youtube_protection_mutation("history_reset", mutation_generation, || {
            jobs::reset_youtube_protection_history_with_generation(&paths, operation.as_deref(), mutation_generation).map_err(|e| e.to_string())
        });
        recorder.phase("policy_reset_batch", started.elapsed());
        result
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn jobs_cleanup_preview(state: State<'_, AppState>) -> Result<jobs::JobCleanupPreview, String> {
    jobs::preview_jobs_cleanup(&state.paths).map_err(|e| e.to_string())
}

#[tauri::command]
fn jobs_item_artifact_retention_policy(
    _state: State<'_, AppState>,
) -> Result<jobs::ItemArtifactRetentionPolicy, String> {
    Ok(jobs::item_artifact_retention_policy())
}

#[tauri::command]
fn jobs_flush_cache(
    state: State<'_, AppState>,
    options: Option<jobs::JobCleanupOptions>,
) -> Result<jobs::JobCleanupSummary, String> {
    jobs::flush_jobs_cache(&state.paths, options).map_err(|e| e.to_string())
}

#[tauri::command]
#[allow(non_snake_case)]
fn jobs_clear_failed_for_item(
    state: State<'_, AppState>,
    item_id: Option<String>,
    itemId: Option<String>,
    options: Option<jobs::ClearFailedJobsForItemOptions>,
) -> Result<jobs::ClearFailedJobsForItemSummary, String> {
    let item_id = item_id
        .or(itemId)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| "missing required key itemId".to_string())?;
    jobs::clear_failed_jobs_for_item(&state.paths, &item_id, options).map_err(|e| e.to_string())
}

#[tauri::command]
#[allow(non_snake_case)]
fn jobs_retry(
    state: State<'_, AppState>,
    job_id: Option<String>,
    jobId: Option<String>,
) -> Result<jobs::JobRow, String> {
    let job_id = job_id
        .or(jobId)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| "missing required key jobId".to_string())?;
    jobs::retry_job(&state.paths, &job_id).map_err(|e| e.to_string())
}

#[tauri::command]
#[allow(non_snake_case)]
async fn jobs_retry_batch_failed(
    state: State<'_, AppState>,
    batch_id: Option<String>,
    batchId: Option<String>,
) -> Result<jobs::RetryBatchFailedSummary, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "jobs_retry_batch_failed");
    let batch_id = batch_id
        .or(batchId)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| "missing required key batchId".to_string())?;
    let paths = state.paths.clone();
    let trace_paths = paths.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        jobs::retry_failed_jobs_for_batch(&paths, &batch_id).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?;
    result.map_err(|e| trace_database_command_error(&trace_paths, "jobs_retry_batch_failed", e))
}

#[tauri::command]
#[allow(non_snake_case)]
async fn jobs_retry_batch_failed_dry_run(
    state: State<'_, AppState>,
    batch_id: Option<String>,
    batchId: Option<String>,
) -> Result<jobs::RetryBatchFailedSummary, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "jobs_retry_batch_failed_dry_run");
    let batch_id = batch_id
        .or(batchId)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| "missing required key batchId".to_string())?;
    let paths = state.paths.clone();
    let trace_paths = paths.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        jobs::retry_failed_jobs_for_batch_dry_run(&paths, &batch_id).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?;
    result.map_err(|e| {
        trace_database_command_error(&trace_paths, "jobs_retry_batch_failed_dry_run", e)
    })
}

#[tauri::command]
#[allow(non_snake_case)]
async fn jobs_repair_batch(
    state: State<'_, AppState>,
    batch_id: Option<String>,
    batchId: Option<String>,
) -> Result<jobs::RetryBatchFailedSummary, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "jobs_repair_batch");
    let batch_id = batch_id
        .or(batchId)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| "missing required key batchId".to_string())?;
    let paths = state.paths.clone();
    let trace_paths = paths.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        jobs::repair_batch(&paths, &batch_id).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?;
    result.map_err(|e| trace_database_command_error(&trace_paths, "jobs_repair_batch", e))
}

#[tauri::command]
#[allow(non_snake_case)]
fn jobs_batch_operation_start(
    state: State<'_, AppState>,
    mode: String,
    batch_id: Option<String>,
    batchId: Option<String>,
) -> Result<JobsBatchOperationSnapshot, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "jobs_batch_operation_start");
    let mode = mode.trim().to_ascii_lowercase();
    if !matches!(mode.as_str(), "dry_run" | "retry" | "repair") {
        return Err("mode must be dry_run, retry, or repair".to_string());
    }
    let batch_query = batch_id
        .or(batchId)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "missing required key batchId".to_string())?;
    let started_at_ms = now_epoch_ms_i64();
    let request_id = {
        let mut operations = jobs_batch_operations()
            .lock()
            .map_err(|_| "batch operation registry is unavailable".to_string())?;
        prune_jobs_batch_operations(&mut operations, started_at_ms);
        if let Some(existing) = operations.values().find(|operation| {
            operation.state == "running"
                && operation.mode == mode
                && operation.batch_query == batch_query
        }) {
            return Ok(existing.clone());
        }
        if operations.len() >= 128 {
            return Err(
                "too many batch operations are still running; wait for one to finish".to_string(),
            );
        }
        let sequence = JOBS_BATCH_OPERATION_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let request_id = format!("jobs-batch-{started_at_ms}-{sequence}");
        operations.insert(
            request_id.clone(),
            JobsBatchOperationSnapshot {
                request_id: request_id.clone(),
                mode: mode.clone(),
                batch_query: batch_query.clone(),
                state: "running".to_string(),
                started_at_ms,
                finished_at_ms: None,
                summary: None,
                error: None,
            },
        );
        request_id
    };

    let paths = state.paths.clone();
    let operation_request_id = request_id.clone();
    let operation_mode = mode.clone();
    let operation_batch_query = batch_query.clone();
    append_diagnostics_trace_row_best_effort(
        &paths,
        "jobs_batch_operation_started",
        serde_json::json!({
            "request_id": request_id.clone(),
            "mode": mode.clone(),
            "batch_query": batch_query.clone(),
        }),
        "info",
    );
    tauri::async_runtime::spawn(async move {
        let worker_paths = paths.clone();
        let worker_mode = operation_mode.clone();
        let worker_batch_query = operation_batch_query.clone();
        let result = tauri::async_runtime::spawn_blocking(move || match worker_mode.as_str() {
            "dry_run" => {
                jobs::retry_failed_jobs_for_batch_dry_run(&worker_paths, &worker_batch_query)
            }
            "retry" => jobs::retry_failed_jobs_for_batch(&worker_paths, &worker_batch_query),
            "repair" => jobs::repair_batch(&worker_paths, &worker_batch_query),
            _ => unreachable!("validated batch operation mode"),
        })
        .await
        .map_err(|error| format!("batch operation worker failed: {error}"))
        .and_then(|result| result.map_err(|error| error.to_string()));
        let finished_at_ms = now_epoch_ms_i64();
        let elapsed_ms = finished_at_ms.saturating_sub(started_at_ms);
        let (event, level, trace_error) = match &result {
            Ok(_) => ("jobs_batch_operation_completed", "info", None),
            Err(error) => ("jobs_batch_operation_failed", "error", Some(error.clone())),
        };
        if let Ok(mut operations) = jobs_batch_operations().lock() {
            if let Some(operation) = operations.get_mut(&operation_request_id) {
                operation.state = if result.is_ok() {
                    "succeeded".to_string()
                } else {
                    "failed".to_string()
                };
                operation.finished_at_ms = Some(finished_at_ms);
                match result {
                    Ok(summary) => operation.summary = Some(summary),
                    Err(error) => operation.error = Some(error),
                }
            }
        }
        append_diagnostics_trace_row_best_effort(
            &paths,
            event,
            serde_json::json!({
                "request_id": operation_request_id,
                "mode": operation_mode,
                "batch_query": operation_batch_query,
                "started_at_ms": started_at_ms,
                "finished_at_ms": finished_at_ms,
                "elapsed_ms": elapsed_ms,
                "error": trace_error,
            }),
            level,
        );
    });

    jobs_batch_operations()
        .lock()
        .map_err(|_| "batch operation registry is unavailable".to_string())?
        .get(&request_id)
        .cloned()
        .ok_or_else(|| "batch operation receipt disappeared".to_string())
}

#[tauri::command]
#[allow(non_snake_case)]
fn jobs_batch_operation_get(
    request_id: Option<String>,
    requestId: Option<String>,
) -> Result<JobsBatchOperationSnapshot, String> {
    let request_id = request_id
        .or(requestId)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "missing required key requestId".to_string())?;
    let now_ms = now_epoch_ms_i64();
    let mut operations = jobs_batch_operations()
        .lock()
        .map_err(|_| "batch operation registry is unavailable".to_string())?;
    prune_jobs_batch_operations(&mut operations, now_ms);
    operations
        .get(&request_id)
        .cloned()
        .ok_or_else(|| format!("batch operation receipt not found: {request_id}"))
}

#[tauri::command]
#[allow(non_snake_case)]
async fn jobs_batch_detail(
    state: State<'_, AppState>,
    batch_id: Option<String>,
    batchId: Option<String>,
) -> Result<jobs::JobBatchDetail, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "jobs_batch_detail");
    let batch_id = batch_id
        .or(batchId)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| "missing required key batchId".to_string())?;
    let paths = state.paths.clone();
    let trace_paths = paths.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        jobs::get_batch_detail(&paths, &batch_id).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?;
    result.map_err(|e| trace_database_command_error(&trace_paths, "jobs_batch_detail", e))
}

#[tauri::command]
#[allow(non_snake_case)]
async fn jobs_detail(
    state: State<'_, AppState>,
    job_id: Option<String>,
    jobId: Option<String>,
) -> Result<jobs::JobDetail, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "jobs_detail");
    let job_id = job_id
        .or(jobId)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| "missing required key jobId".to_string())?;
    let paths = state.paths.clone();
    let trace_paths = paths.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        jobs::get_job_detail(&paths, &job_id).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?;
    result.map_err(|e| trace_database_command_error(&trace_paths, "jobs_detail", e))
}

#[tauri::command]
#[allow(non_snake_case)]
async fn jobs_backfill_titles_for_batch(
    state: State<'_, AppState>,
    batch_id: Option<String>,
    batchId: Option<String>,
    limit: Option<usize>,
) -> Result<jobs::JobTitleBackfillSummary, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "jobs_backfill_titles_for_batch");
    let batch_id = batch_id
        .or(batchId)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| "missing required key batchId".to_string())?;
    let paths = state.paths.clone();
    let trace_paths = paths.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        jobs::backfill_job_titles_for_batch(&paths, &batch_id, limit.unwrap_or(500))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?;
    result.map_err(|e| {
        trace_database_command_error(&trace_paths, "jobs_backfill_titles_for_batch", e)
    })
}

#[tauri::command]
#[allow(non_snake_case)]
async fn jobs_export_unresolved_batch(
    state: State<'_, AppState>,
    batch_id: Option<String>,
    batchId: Option<String>,
    format: Option<String>,
) -> Result<jobs::JobExportPayload, String> {
    let _timer = InvokeTimer::start(state.paths.clone(), "jobs_export_unresolved_batch");
    let batch_id = batch_id
        .or(batchId)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| "missing required key batchId".to_string())?;
    let format = format.unwrap_or_else(|| "csv".to_string());
    let paths = state.paths.clone();
    let trace_paths = paths.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        jobs::export_unresolved_jobs_for_batch(&paths, &batch_id, &format)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?;
    result
        .map_err(|e| trace_database_command_error(&trace_paths, "jobs_export_unresolved_batch", e))
}

#[tauri::command]
fn admin_save_snapshot(
    base64_data: String,
    subfolder: Option<String>,
    label: Option<String>,
) -> Result<String, String> {
    let b64 = if let Some(stripped) = base64_data.strip_prefix("data:image/png;base64,") {
        stripped
    } else {
        &base64_data
    };

    let decoded = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, b64)
        .map_err(|e| format!("Failed to decode base64: {}", e))?;

    let mut snapshots_dir = std::env::current_dir().unwrap_or_default();
    while !snapshots_dir.join("governance").exists() && snapshots_dir.parent().is_some() {
        snapshots_dir = snapshots_dir.parent().unwrap().to_path_buf();
    }
    let mut target_dir = snapshots_dir.join("governance").join("snapshots");
    if let Some(ref sub) = subfolder {
        let sanitized = sub.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
        if !sanitized.is_empty() {
            target_dir = target_dir.join(sanitized);
        }
    }
    if !target_dir.exists() {
        std::fs::create_dir_all(&target_dir)
            .map_err(|e| format!("Failed to create snapshot dir: {}", e))?;
    }

    let label_part = label
        .map(|l| l.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|', ' '], "_"))
        .filter(|l| !l.is_empty());
    let file_name = match label_part {
        Some(l) => format!("{}_{}.png", l, now_epoch_ms_i64()),
        None => format!("snapshot_{}.png", now_epoch_ms_i64()),
    };
    let path = target_dir.join(file_name);

    std::fs::write(&path, decoded).map_err(|e| format!("Failed to write snapshot: {}", e))?;
    let abs_path = std::fs::canonicalize(&path)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string();
    Ok(abs_path)
}

#[tauri::command]
fn agent_report_state(page: String, editor_item_id: Option<String>, safe_mode: bool) {
    let mut state = agent_bridge_state().lock().unwrap();
    state.current_page = page;
    state.editor_item_id = editor_item_id;
    state.safe_mode = safe_mode;
}

#[tauri::command]
fn admin_save_dump(
    json_data: String,
    subfolder: Option<String>,
    label: Option<String>,
) -> Result<String, String> {
    let mut snapshots_dir = std::env::current_dir().unwrap_or_default();
    while !snapshots_dir.join("governance").exists() && snapshots_dir.parent().is_some() {
        snapshots_dir = snapshots_dir.parent().unwrap().to_path_buf();
    }
    let mut target_dir = snapshots_dir.join("governance").join("snapshots");
    if let Some(ref sub) = subfolder {
        let sanitized = sub.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
        if !sanitized.is_empty() {
            target_dir = target_dir.join(sanitized);
        }
    }
    if !target_dir.exists() {
        std::fs::create_dir_all(&target_dir)
            .map_err(|e| format!("Failed to create dump dir: {}", e))?;
    }

    let label_part = label
        .map(|l| l.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|', ' '], "_"))
        .filter(|l| !l.is_empty());
    let file_name = match label_part {
        Some(l) => format!("{}_{}.dump.json", l, now_epoch_ms_i64()),
        None => format!("dump_{}.dump.json", now_epoch_ms_i64()),
    };
    let path = target_dir.join(file_name);

    std::fs::write(&path, json_data.as_bytes())
        .map_err(|e| format!("Failed to write dump: {}", e))?;
    let abs_path = std::fs::canonicalize(&path)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string();
    Ok(abs_path)
}

// ---------------------------------------------------------------------------
// Per-segment clone breakdown (WP-0186)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct TtsManifestSegmentCloneInfo {
    index: u32,
    speaker: Option<String>,
    voice_clone_intent: Option<String>,
    voice_clone_outcome: Option<String>,
    voice_clone_error: Option<String>,
}

#[tauri::command]
fn tts_manifest_clone_segments(path: String) -> Result<Vec<TtsManifestSegmentCloneInfo>, String> {
    let data = std::fs::read(&path).map_err(|e| format!("Failed to read manifest: {e}"))?;
    let parsed: serde_json::Value =
        serde_json::from_slice(&data).map_err(|e| format!("Failed to parse manifest: {e}"))?;
    let segments = parsed
        .get("segments")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let result: Vec<TtsManifestSegmentCloneInfo> = segments
        .iter()
        .map(|seg| TtsManifestSegmentCloneInfo {
            index: seg.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            speaker: seg
                .get("speaker")
                .and_then(|v| v.as_str())
                .map(String::from),
            voice_clone_intent: seg
                .get("voice_clone_intent")
                .and_then(|v| v.as_str())
                .map(String::from),
            voice_clone_outcome: seg
                .get("voice_clone_outcome")
                .and_then(|v| v.as_str())
                .map(String::from),
            voice_clone_error: seg
                .get("voice_clone_error")
                .and_then(|v| v.as_str())
                .map(String::from),
        })
        .collect();
    Ok(result)
}

// ---------------------------------------------------------------------------
// Glossary commands (WP-0177)
// ---------------------------------------------------------------------------

#[tauri::command]
fn glossary_get(
    state: State<'_, AppState>,
) -> Result<std::collections::BTreeMap<String, String>, String> {
    translate::glossary_load(&state.paths).map_err(|e| e.to_string())
}

#[tauri::command]
fn glossary_set(
    state: State<'_, AppState>,
    entries: std::collections::BTreeMap<String, String>,
) -> Result<(), String> {
    translate::glossary_save(&state.paths, &entries).map_err(|e| e.to_string())
}

#[tauri::command]
fn glossary_export_csv(state: State<'_, AppState>, path: String) -> Result<usize, String> {
    translate::glossary_export_csv(&state.paths, &std::path::PathBuf::from(path))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn glossary_import_csv(state: State<'_, AppState>, path: String) -> Result<usize, String> {
    translate::glossary_import_csv(&state.paths, &std::path::PathBuf::from(path))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn agent_snapshot_complete(path: String) {
    let mut state = agent_bridge_state().lock().unwrap();
    if let Some(tx) = state.snapshot_tx.take() {
        let _ = tx.send(path);
    }
}

#[tauri::command]
fn agent_dump_complete(path: String) {
    let mut state = agent_bridge_state().lock().unwrap();
    if let Some(tx) = state.dump_tx.take() {
        let _ = tx.send(path);
    }
}

#[tauri::command]
fn agent_ui_request_complete(payload: String) {
    let mut state = agent_bridge_state().lock().unwrap();
    if let Some(tx) = state.ui_request_tx.take() {
        let _ = tx.send(payload);
    }
}

#[derive(Clone, Copy)]
struct YoutubeRetentionWorkerPolicy {
    max_cycles_per_round: u64,
    max_failures_per_round: u32,
    inter_batch_delay: Duration,
    failure_retry_delay: Duration,
    round_backoff: Duration,
}

fn run_youtube_retention_worker_loop<D, P, C, W>(
    mut drain: D,
    mut persist: P,
    mut is_cancelled: C,
    mut wait: W,
    policy: YoutubeRetentionWorkerPolicy,
) -> bool
where
    D: FnMut() -> Result<voxvulgi_engine::youtube_protection::DownloaderRetentionDrainReceipt, String>,
    P: FnMut(bool, u32) -> bool,
    C: FnMut() -> bool,
    W: FnMut(Duration) -> bool,
{
    let mut consecutive_failures = 0_u32;
    loop {
        if is_cancelled() {
            let _persisted = persist(true, consecutive_failures);
            return false;
        }
        let mut cycles_this_round = 0_u64;
        while cycles_this_round < policy.max_cycles_per_round {
            if is_cancelled() {
                let _persisted = persist(true, consecutive_failures);
                return false;
            }
            cycles_this_round = cycles_this_round.saturating_add(1);
            match drain() {
                Ok(receipt) if receipt.has_more => {
                    consecutive_failures = 0;
                    let _persisted = persist(true, 0);
                    if cycles_this_round < policy.max_cycles_per_round
                        && wait(policy.inter_batch_delay)
                    {
                        let _persisted = persist(true, 0);
                        return false;
                    }
                }
                Ok(_) => {
                    if persist(false, 0) {
                        return true;
                    }
                    // A completed delete pass is not durable completion until the continuation
                    // row is cleared. Back off and retry the projection instead of silently
                    // abandoning a pending startup continuation.
                    break;
                }
                Err(_) => {
                    consecutive_failures = consecutive_failures.saturating_add(1);
                    let _persisted = persist(true, consecutive_failures);
                    if consecutive_failures >= policy.max_failures_per_round {
                        break;
                    }
                    if wait(policy.failure_retry_delay) {
                        let _persisted = persist(true, consecutive_failures);
                        return false;
                    }
                }
            }
        }
        // Each finite round yields before rescheduling itself. This keeps the singleton worker
        // live until durable completion without a startup hot loop or unbounded thread spawn.
        let _persisted = persist(true, consecutive_failures);
        if wait(policy.round_backoff) {
            let _persisted = persist(true, consecutive_failures);
            return false;
        }
    }
}

fn spawn_youtube_retention_worker(paths: AppPaths) {
    if YOUTUBE_RETENTION_WORKER_RUNNING
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    YOUTUBE_RETENTION_WORKER_CANCELLED.store(false, Ordering::Release);
    let spawn = std::thread::Builder::new()
        .name("youtube-retention-worker".to_string())
        .spawn(move || {
            struct RunningGuard;
            impl Drop for RunningGuard {
                fn drop(&mut self) {
                    YOUTUBE_RETENTION_WORKER_RUNNING.store(false, Ordering::Release);
                }
            }
            let _running = RunningGuard;
            let mut cycle = 0_u64;
            let _completed = run_youtube_retention_worker_loop(
                || {
                    cycle = cycle.saturating_add(1);
                    let receipt = voxvulgi_engine::youtube_protection::drain_expired_outcomes(
                        &paths,
                        now_epoch_ms_i64(),
                        100,
                        8,
                        2_000,
                    )
                    .map_err(|error| error.to_string());
                    append_diagnostics_trace_row_best_effort(
                        &paths,
                        "youtube_protection_retention_drain",
                        match &receipt {
                            Ok(receipt) => serde_json::json!({
                                "cycle": cycle,
                                "batches": receipt.batches,
                                "deleted": receipt.deleted,
                                "complete": receipt.complete,
                                "has_more": receipt.has_more,
                                "elapsed_ms": receipt.elapsed_ms,
                                "budget_exhausted": receipt.budget_exhausted,
                            }),
                            Err(error) => serde_json::json!({ "cycle": cycle, "error": error }),
                        },
                        if receipt.is_ok() { "info" } else { "warn" },
                    );
                    receipt
                },
                |pending, failures| {
                    match voxvulgi_engine::youtube_protection::persist_retention_continuation(
                        &paths, pending, failures,
                    ) {
                        Ok(_) => true,
                        Err(error) => {
                            append_diagnostics_trace_row_best_effort(
                                &paths,
                                "youtube_protection_retention_continuation_failed",
                                serde_json::json!({
                                    "pending": pending,
                                    "consecutive_failures": failures,
                                    "error": error.to_string(),
                                }),
                                "warn",
                            );
                            false
                        }
                    }
                },
                || YOUTUBE_RETENTION_WORKER_CANCELLED.load(Ordering::Acquire),
                wait_for_youtube_retention_cancel,
                YoutubeRetentionWorkerPolicy {
                    max_cycles_per_round: 16,
                    max_failures_per_round: 3,
                    inter_batch_delay: Duration::from_secs(2),
                    failure_retry_delay: Duration::from_secs(30),
                    round_backoff: Duration::from_secs(30),
                },
            );
        });
    if spawn.is_err() {
        YOUTUBE_RETENTION_WORKER_RUNNING.store(false, Ordering::Release);
    }
}

fn wait_for_youtube_retention_cancel(timeout: Duration) -> bool {
    if YOUTUBE_RETENTION_WORKER_CANCELLED.load(Ordering::Acquire) {
        return true;
    }
    let (lock, wake) = YOUTUBE_RETENTION_WORKER_WAKE
        .get_or_init(|| (Mutex::new(()), std::sync::Condvar::new()));
    let guard = lock
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _ = wake.wait_timeout_while(guard, timeout, |_| {
        !YOUTUBE_RETENTION_WORKER_CANCELLED.load(Ordering::Acquire)
    });
    YOUTUBE_RETENTION_WORKER_CANCELLED.load(Ordering::Acquire)
}

fn cancel_youtube_retention_worker() {
    YOUTUBE_RETENTION_WORKER_CANCELLED.store(true, Ordering::Release);
    if let Some((_, wake)) = YOUTUBE_RETENTION_WORKER_WAKE.get() {
        wake.notify_all();
    }
}

fn offline_provider_verification_startup_outcome(
    verification: Result<(), String>,
) -> (&'static str, Option<String>) {
    match verification {
        Ok(()) => ("ready", None),
        Err(error) => {
            let redacted = redact_diagnostics_value(serde_json::Value::String(error))
                .as_str()
                .unwrap_or("provider integrity verification failed")
                .to_string();
            ("error", Some(redacted))
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let cli_args = std::env::args().collect::<Vec<_>>();
            let cli_agent_headless = cli_args
                .iter()
                .any(|value| value.trim() == "--agent-headless");
            if cli_agent_headless {
                if let Some(window) = app.get_webview_window("main") {
                    hide_agent_headless_window(&window).map_err(std::io::Error::other)?;
                }
            }
            let base_dir = app.path().app_data_dir()?;
            let paths = AppPaths::new(AppPaths::normalize_base_dir(&base_dir));
            let _ = voxvulgi_engine::diagnostics::install_trace_sink(Arc::new(
                |paths, event, level, details| {
                    let _ = append_diagnostics_trace_row_best_effort(paths, event, details, level);
                },
            ));
            let startup = Arc::new(Mutex::new(StartupTracker::new()));
            let _ = AGENT_APP_HANDLE.set(app.handle().clone());
            spawn_agent_bridge(&AppPaths::normalize_base_dir(&base_dir));
            set_startup_phase(&startup, &paths, "app_dirs", "running", None);
            paths.ensure_dirs()?;
            // WP-0298: restore an armed/active bounded incident capture before any
            // panel or job event can arrive. Expired state is normalized lazily by
            // diagnostics_capture_status/the trace envelope.
            let _ = load_diagnostics_capture_state(&paths);
            set_startup_phase(&startup, &paths, "app_dirs", "ready", None);
            let cli_safe_mode = cli_args.iter().any(|value| value.trim() == "--safe-mode");
            let persisted_safe_mode = config::load_safe_mode_config(&paths)
                .map(|value| value.enabled)
                .unwrap_or(false);
            let safe_mode_enabled = cli_safe_mode || persisted_safe_mode;
            {
                let mut bridge_state = agent_bridge_state().lock().unwrap();
                bridge_state.agent_headless = cli_agent_headless;
                bridge_state.safe_mode = safe_mode_enabled;
            }
            let runtime_background_work =
                runtime_background_work_enabled(safe_mode_enabled, cli_agent_headless);
            if !runtime_background_work {
                set_startup_phase(&startup, &paths, "offline_bundle", "skipped", None);
            } else if let Ok(resource_dir) = app.path().resource_dir() {
                set_startup_phase(&startup, &paths, "offline_bundle", "pending", None);
                let startup_for_thread = Arc::clone(&startup);
                let paths_for_bundle = paths.clone();
                std::thread::spawn(move || {
                    set_startup_phase(
                        &startup_for_thread,
                        &paths_for_bundle,
                        "offline_bundle",
                        "running",
                        None,
                    );
                    let result = apply_offline_bundle_if_present(&paths_for_bundle, &resource_dir);
                    match result {
                        Ok(()) => {
                            // Full production dependency-tree authentication is intentionally
                            // performed only on this background hydration boundary. Hot Options
                            // status polling reads the current-process attestation and never
                            // treats a persisted receipt as executable trust.
                            let provider_integrity =
                                voxvulgi_engine::tools::verify_youtube_po_provider_node_modules(
                                    &paths_for_bundle,
                                );
                            let provider_integrity_ready = matches!(
                                &provider_integrity,
                                Ok(status) if status.installed
                            );
                            append_diagnostics_trace_row_best_effort(
                                &paths_for_bundle,
                                "youtube_provider_integrity_verified",
                                match &provider_integrity {
                                    Ok(status) => serde_json::json!({
                                        "installed": status.installed,
                                        "tree_sha256_hex": status.node_modules_tree_sha256_hex,
                                        "verified_at_ms": status.node_modules_verified_at_ms,
                                    }),
                                    Err(error) => serde_json::json!({
                                        "installed": false,
                                        "error": error.to_string(),
                                    }),
                                },
                                if provider_integrity_ready { "info" } else { "warn" },
                            );
                            let verification = match &provider_integrity {
                                Ok(status) if status.installed => Ok(()),
                                Ok(status) => Err(status.readiness_error.clone().unwrap_or_else(|| {
                                    "provider integrity verification did not establish executable readiness"
                                        .to_string()
                                })),
                                Err(error) => Err(error.to_string()),
                            };
                            let (phase, error) =
                                offline_provider_verification_startup_outcome(verification);
                            set_startup_phase(
                                &startup_for_thread,
                                &paths_for_bundle,
                                "offline_bundle",
                                phase,
                                error,
                            );
                        }
                        Err(error) => {
                            set_startup_phase(
                                &startup_for_thread,
                                &paths_for_bundle,
                                "offline_bundle",
                                "error",
                                Some(error),
                            );
                        }
                    }
                });
            } else {
                set_startup_phase(
                    &startup,
                    &paths,
                    "offline_bundle",
                    "error",
                    Some("resource directory unavailable".to_string()),
                );
            }
            // WP-0252: seed the bundled CosyVoice runtime code on first run, and start the
            // detached external-watcher supervisor (skipped in safe mode / when disabled).
            if runtime_background_work {
                if let Ok(resource_dir) = app.path().resource_dir() {
                    seed_cosyvoice_backend_if_missing(&resource_dir, &base_dir);
                    if watcher_enabled(&paths) {
                        spawn_watcher_supervisor(&resource_dir, &base_dir);
                    }
                }
            }
            set_startup_phase(&startup, &paths, "db_schema", "running", None);
            db::ensure_schema(&paths)?;
            video_libraries::ensure_default_video_library(&paths)?;
            set_startup_phase(&startup, &paths, "db_schema", "ready", None);
            if runtime_background_work {
                let archive_reconcile_paths = paths.clone();
                std::thread::Builder::new()
                    .name("youtube-archive-reconcile".to_string())
                    .spawn(move || {
                        let outcome = subscriptions::reconcile_youtube_archive_merge_journals(
                            &archive_reconcile_paths,
                        );
                        append_diagnostics_trace_row_best_effort(
                            &archive_reconcile_paths,
                            "youtube_archive_startup_reconcile",
                            match &outcome {
                                Ok(recovered) => serde_json::json!({ "recovered": recovered }),
                                Err(error) => serde_json::json!({ "error": error.to_string() }),
                            },
                            if outcome.is_ok() { "info" } else { "warn" },
                        );
                    })
                    .map_err(std::io::Error::other)?;
                spawn_youtube_retention_worker(paths.clone());
            }
            // Root rebind recovery can canonicalize and hash files on disconnected storage.
            // Queue it only after schema readiness on the same fixed bounded executor used by
            // the command surface; startup never waits on a NAS probe and headless mode remains
            // mutation-free.
            if runtime_background_work {
                let reconcile_paths = paths.clone();
                match root_rebind::submit_root_rebind_task("startup_recover", move || {
                    let outcome = root_rebind::reconcile_incomplete_root_rebinds(&reconcile_paths);
                    match &outcome {
                        Ok(receipts) if !receipts.is_empty() => {
                            append_diagnostics_trace_row_best_effort(
                                &reconcile_paths,
                                "root_rebind_startup_reconciled",
                                serde_json::json!({
                                    "receipt_ids": receipts.iter().map(|receipt| receipt.id.clone()).collect::<Vec<_>>(),
                                    "receipt_count": receipts.len(),
                                }),
                                "info",
                            );
                        }
                        Ok(_) => {}
                        Err(error) => {
                            append_diagnostics_trace_row_best_effort(
                                &reconcile_paths,
                                "root_rebind_startup_reconcile_failed",
                                serde_json::json!({ "error": error.to_string() }),
                                "warn",
                            );
                        }
                    }
                    outcome
                }) {
                    Ok(ticket) => append_diagnostics_trace_row_best_effort(
                        &paths,
                        "root_rebind_startup_reconcile_queued",
                        serde_json::json!({ "task_id": ticket.task_id, "state": ticket.state }),
                        "info",
                    ),
                    Err(error) => append_diagnostics_trace_row_best_effort(
                        &paths,
                        "root_rebind_startup_reconcile_queue_failed",
                        serde_json::json!({ "error": error.to_string() }),
                        "warn",
                    ),
                };
            }
            // WP-0253 Item 2d: if the configured download root (e.g. a NAS share) is
            // reachable again, move any local-fallback downloads back onto it (copy ->
            // verify -> relink -> delete-after-verify). Background thread; no-op when the
            // root is unreachable or nothing fell back, so it never blocks startup.
            if runtime_background_work {
                let resync_paths = paths.clone();
                std::thread::spawn(move || {
                    // Run once at startup, then poll every 5 min so a mid-session NAS
                    // reconnect also triggers the move-back. Cheap no-op when the root is
                    // unreachable or nothing fell back; serialized (single loop thread) so
                    // resyncs never overlap.
                    loop {
                        let _ = library::resync_local_fallback_downloads(&resync_paths);
                        std::thread::sleep(std::time::Duration::from_secs(300));
                    }
                });
            }
            if safe_mode_enabled && !cli_agent_headless {
                let _ = jobs::set_queue_paused(&paths, true);
            }
            let runner = if runtime_background_work {
                set_startup_phase(&startup, &paths, "job_runner", "running", None);
                let runner = jobs::start_runner(paths.clone())?;
                set_startup_phase(&startup, &paths, "job_runner", "ready", None);
                Some(runner)
            } else {
                set_startup_phase(&startup, &paths, "job_runner", "skipped", None);
                None
            };
            // WP-0254: 4KVDP-style startup auto-check + auto-download. After a short delay
            // (so it never competes with first-paint startup work), enqueue DUE active
            // subscriptions into the conservative recurring lane (limit 1, one channel at a
            // time). Background + best-effort; gated by safe-mode + config flag. Unlike the
            // WP-0227/WP-0228 pack-install regression, this is light (due-only refreshes,
            // paced by the recurring lane) so it cannot swamp the app.
            if runtime_background_work && subscription_auto_sync_enabled(&paths) {
                let auto_sync_paths = paths.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(20));
                    match subscriptions::queue_all_active_youtube_subscriptions(&auto_sync_paths) {
                        Ok(jobs) => append_diagnostics_trace_row_best_effort(
                            &auto_sync_paths,
                            "subscription_auto_sync",
                            serde_json::json!({ "queued_refresh_jobs": jobs.len() }),
                            "info",
                        ),
                        Err(e) => append_diagnostics_trace_row_best_effort(
                            &auto_sync_paths,
                            "subscription_auto_sync_error",
                            serde_json::json!({ "error": e.to_string() }),
                            "warn",
                        ),
                    }
                });
                // WP-0263: Instagram passive startup auto-check. Gated by the same
                // safe-mode + auto-sync config as YouTube. Enqueues only DUE Instagram
                // subscriptions, one profile at a time, honoring the conservative Instagram
                // enumeration cooldown (Meta anti-bot is stricter). Runs on its own delayed
                // thread so it never competes with first-paint or the YouTube auto-sync.
                let ig_auto_sync_paths = paths.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(45));
                    match instagram_subscriptions::queue_all_active_instagram_subscriptions(
                        &ig_auto_sync_paths,
                    ) {
                        Ok(jobs) => append_diagnostics_trace_row_best_effort(
                            &ig_auto_sync_paths,
                            "instagram_subscription_auto_sync",
                            serde_json::json!({ "queued_download_jobs": jobs.len() }),
                            "info",
                        ),
                        Err(e) => append_diagnostics_trace_row_best_effort(
                            &ig_auto_sync_paths,
                            "instagram_subscription_auto_sync_error",
                            serde_json::json!({ "error": e.to_string() }),
                            "warn",
                        ),
                    }
                });
            }
            let trace_paths = paths.clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(Duration::from_secs(30));
                append_diagnostics_trace_row_best_effort(
                    &trace_paths,
                    "runtime_sample",
                    serde_json::json!({
                        "source": "background_sampler",
                    }),
                    "info",
                );
            });
            // WP-0221: process-scheduling skew heartbeat for freeze diagnosis.
            spawn_event_loop_skew_heartbeat(paths.clone());
            // WP-0228 (v0.1.26): WP-0227's auto-enqueue of the Phase2
            // voice-pack install on startup was a net regression for this
            // operator. The install's disk + subprocess load made the rest
            // of the app unusable while it ran ("freezes all the time, have
            // not touched it once because nothing happens because of
            // freeze"). The auto-enqueue is removed; voice-pack install
            // returns to being explicitly operator-triggered from the
            // Diagnostics page. The WP-0227 resume logic in the install
            // job handler is kept — when the operator does click Install,
            // it correctly carries forward any previously-completed steps
            // rather than restarting from scratch.
            app.manage(AppState {
                paths,
                runner,
                safe_mode_enabled: Arc::new(AtomicBool::new(safe_mode_enabled)),
                safe_mode_cli: cli_safe_mode,
                startup,
            });
            Ok(())
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            diagnostics_info,
            diagnostics_clear_cache,
            diagnostics_thumbnail_cache_clear,
            diagnostics_thumbnail_cache_status,
            diagnostics_export_bundle,
            diagnostics_app_state_snapshot,
            diagnostics_export_app_state_snapshot,
            diagnostics_generate_licensing_report,
            diagnostics_storage_breakdown,
            item_outputs,
            item_outputs_many,
            localization_home_item_outputs,
            library_get_many,
            library_thumbnail_data_url,
            item_artifacts_list_v1,
            item_export_mux_preview_mp4,
            item_qc_report_v1_load,
            diagnostics_trace_clear,
            diagnostics_capture_status,
            diagnostics_capture_panel_transition,
            diagnostics_capture_panel_transition_cancel,
            diagnostics_capture_arm,
            diagnostics_capture_disarm,
            diagnostics_trace_dir_set,
            diagnostics_trace_dir_status,
            diagnostics_trace_dir_use_default,
            diagnostics_trace_recent,
            diagnostics_trace_write_event,
            agent_bridge_port,
            agent_freeze_dump_now,
            safe_mode_set,
            safe_mode_status,
            startup_status,
            downloads_dir_set,
            downloads_dir_status,
            downloads_dir_use_default,
            downloads_feature_root_set,
            downloads_feature_root_use_default,
            config_batch_on_import_get,
            config_batch_on_import_set,
            root_rebind_dry_run,
            root_rebind_prepare,
            root_rebind_apply,
            root_rebind_status,
            root_rebind_rollback,
            root_rebind_recover,
            root_rebind_task_status,
            root_rebind_task_cancel,
            config_youtube_auth_get,
            youtube_auth_open_sign_in,
            config_youtube_auth_preflight,
            config_youtube_auth_set,
            config_instagram_auth_get,
            config_instagram_auth_preflight,
            config_instagram_auth_set,
            config_diarization_optional_clear_token,
            config_diarization_optional_set,
            config_diarization_optional_status,
            download_presets_export_json,
            download_presets_get,
            download_presets_import_json,
            download_presets_catalog_set,
            download_presets_default_safety_patch,
            library_get,
            library_list,
            library_query,
            library_file_delete,
            library_operator_deleted_redownload,
            library_list_youtube_video_candidates,
            library_youtube_single_history,
            library_youtube_single_unclassified_total,
            library_download_lineage_backfill_step,
            library_resync_local_fallback,
            localization_workspace_list,
            youtube_subscription_groups_delete,
            youtube_subscription_groups_clear_memberships,
            youtube_subscription_groups_list,
            youtube_subscription_groups_set_for_subscription,
            youtube_subscription_groups_upsert,
            youtube_subscriptions_list,
            youtube_subscriptions_output_dir,
            youtube_subscriptions_preview_output_dir,
            youtube_subscriptions_upsert,
            youtube_subscriptions_set_library,
            youtube_subscriptions_set_manual_status,
            youtube_subscriptions_delete,
            video_libraries_list,
            video_libraries_upsert,
            video_libraries_set_active,
            video_libraries_remove,
            video_library_bundle_export,
            video_library_bundle_import,
            video_library_metadata_transfer,
            youtube_subscriptions_import_existing_downloads,
            legacy_archive_analyze,
            youtube_subscriptions_queue_one,
            youtube_subscriptions_queue_all_active,
            youtube_subscriptions_update_all,
            youtube_subscriptions_stop_recurring,
            youtube_subscriptions_recurring_paused,
            youtube_subscriptions_queue_group,
            youtube_subscriptions_export_json,
            youtube_subscriptions_import_json,
            youtube_subscriptions_import_4kvdp_dir,
            youtube_subscriptions_import_4kvdp_state,
            youtube_imported_identity_enrich_4kvdp,
            youtube_subscriptions_seed_archive_scan,
            youtube_subscriptions_archive_stats,
            youtube_subscriptions_active_refresh_ids,
            youtube_subscriptions_activity,
            subscription_download_activity,
            subscription_projections_rebuild,
            youtube_subscription_videos,
            instagram_subscriptions_list,
            instagram_subscriptions_upsert,
            instagram_subscriptions_delete,
            instagram_subscriptions_queue_one,
            instagram_subscriptions_queue_all_active,
            instagram_subscriptions_output_dir,
            jobs_cancel,
            jobs_cancel_all,
            jobs_backfill_titles_for_batch,
            jobs_batch_detail,
            jobs_delete_terminal,
            jobs_delete_terminal_matching_search,
            jobs_detail,
            jobs_enqueue_dummy,
            jobs_enqueue_asr_local,
            jobs_enqueue_download_batch,
            library_download_preflight,
            library_canonical_media_relocate,
            library_canonical_source_replace,
            library_canonical_record_remove,
            jobs_enqueue_instagram_batch,
            jobs_enqueue_image_batch,
            jobs_enqueue_import_local,
            jobs_enqueue_install_phase2_packs_v1,
            jobs_enqueue_diarize_local_v1,
            jobs_enqueue_tts_preview_pyttsx3_v1,
            jobs_enqueue_tts_neural_local_v1,
            jobs_enqueue_dub_voice_preserving_v1,
            jobs_enqueue_experimental_voice_backend_render_v1,
            jobs_enqueue_experimental_backend_batch_v1,
            jobs_enqueue_mix_dub_preview_v1,
            jobs_enqueue_mux_dub_preview_v1,
            jobs_enqueue_separate_audio_spleeter,
            jobs_enqueue_separate_audio_demucs_v1,
            jobs_enqueue_clean_vocals_v1,
            jobs_enqueue_qc_report_v1,
            jobs_enqueue_export_pack_v1,
            jobs_enqueue_localization_batch_v1,
            jobs_enqueue_localization_run_v1,
            jobs_enqueue_voice_ab_preview_v1,
            jobs_enqueue_translate_local,
            jobs_cleanup_preview,
            jobs_flush_cache,
            jobs_clear_failed_for_item,
            jobs_list,
            jobs_list_for_item,
            jobs_search,
            jobs_list_live,
            jobs_overview,
            jobs_track_activity,
            jobs_progress_many,
            jobs_queue_control_get,
            jobs_queue_control_set,
            youtube_queue_identity_reconcile,
            media_cleanup_get,
            media_cleanup_latest,
            media_cleanup_create,
            media_cleanup_inventory_advance,
            media_cleanup_hash_advance,
            media_cleanup_groups,
            media_cleanup_group_decide,
            media_cleanup_apply,
            media_cleanup_rollback,
            jobs_item_artifact_retention_policy,
            jobs_log_retention_policy,
            jobs_prune_logs,
            jobs_runtime_settings_get,
            jobs_runtime_settings_set,
            jobs_track_runtime_get,
            jobs_track_runtime_set,
            antibot_pacing_get,
            antibot_pacing_set,
            youtube_protection_status_get,
            youtube_protection_return_to_baseline,
            youtube_protection_history_get,
            youtube_protection_history_replay,
            youtube_protection_tuning_get,
            youtube_protection_tuning_set,
            youtube_protection_tuning_reset,
            youtube_protection_history_export,
            youtube_protection_history_reset,
            jobs_export_unresolved_batch,
            jobs_repair_batch,
            jobs_retry,
            jobs_retry_batch_failed,
            jobs_retry_batch_failed_dry_run,
            jobs_batch_operation_start,
            jobs_batch_operation_get,
            models_inventory,
            models_install,
            models_install_demo,
            speakers_list,
            speakers_upsert,
            voice_library_add_reference,
            voice_library_apply_to_item,
            voice_library_create,
            voice_library_create_from_item_speaker,
            voice_library_delete,
            voice_library_fork,
            voice_library_get,
            voice_library_list,
            voice_library_remove_reference,
            voice_library_suggest_for_item,
            voice_library_update,
            voice_backends_catalog,
            voice_backends_recommend,
            voice_benchmark_generate,
            voice_benchmark_history_list,
            voice_benchmark_leaderboard_export,
            voice_benchmark_load,
            voice_reference_curation_generate,
            voice_reference_curation_load,
            voice_reference_curation_apply,
            voice_reference_candidates_generate,
            voice_reference_candidates_load,
            voice_reference_candidates_apply,
            item_voice_plan_get,
            item_voice_plan_upsert,
            item_voice_plan_delete,
            item_voice_plan_promote_recommendation,
            item_voice_plan_promote_benchmark_candidate,
            voice_backend_adapters_list,
            voice_backend_adapter_apply_starter_recipe,
            voice_backend_adapter_upsert,
            voice_backend_adapter_delete,
            voice_backend_adapter_probe,
            voice_cleanup_list_for_speaker,
            voice_cleanup_run_for_speaker,
            voice_templates_apply_to_item,
            voice_templates_add_reference,
            voice_templates_clear_voice_plan_default,
            voice_templates_create_from_item,
            voice_templates_delete,
            voice_templates_get,
            voice_templates_list,
            voice_templates_promote_benchmark_candidate_default,
            voice_templates_remove_reference,
            voice_templates_update_speaker,
            voice_cast_packs_apply_to_item,
            voice_cast_packs_clear_voice_plan_default,
            voice_cast_packs_create_from_template,
            voice_cast_packs_delete,
            voice_cast_packs_get,
            voice_cast_packs_list,
            voice_cast_packs_promote_benchmark_candidate_default,
            voice_cast_packs_update,
            item_export_source_media,
            subtitles_export_doc_srt,
            subtitles_export_doc_vtt,
            subtitles_list_tracks,
            subtitles_load_track,
            subtitles_save_new_version,
            shell_paths_status,
            shell_open_parent_dir,
            shell_open_path,
            shell_reveal_path,
            tools_ffmpeg_install,
            tools_ffmpeg_status,
            tools_js_runtime_install,
            tools_js_runtime_status,
            tools_python_install,
            tools_python_status,
            tools_python_portable_install,
            tools_python_portable_status,
            tools_phase2_packs_install_plan,
            tools_phase2_packs_install_latest_state,
            tools_pack_integrity_manifest_generate,
            tools_pack_integrity_manifest_status,
            tools_performance_tier_status,
            tools_diarization_install,
            tools_diarization_status,
            tools_spleeter_install,
            tools_spleeter_status,
            tools_demucs_install,
            tools_demucs_status,
            tools_tts_preview_install,
            tools_tts_preview_status,
            tools_tts_preview_pyttsx3_voices,
            tools_tts_neural_local_v1_install,
            tools_tts_neural_local_v1_status,
            tools_tts_voice_preserving_local_v1_install,
            tools_tts_voice_preserving_local_v1_status,
            tools_ytdlp_install,
            tools_ytdlp_status,
            window_close,
            window_minimize,
            window_start_drag,
            window_start_resize_drag,
            window_toggle_maximize,
            admin_save_snapshot,
            admin_save_dump,
            tts_manifest_clone_segments,
            glossary_get,
            glossary_set,
            glossary_export_csv,
            glossary_import_csv,
            agent_report_state,
            agent_snapshot_complete,
            agent_dump_complete,
            agent_ui_request_complete
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app_handle, event| {
            if let tauri::RunEvent::Exit = event {
                cancel_youtube_retention_worker();
                signal_watcher_stop();
                cleanup_agent_bridge_files();
            }
        });
}
