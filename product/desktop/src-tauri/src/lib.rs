use base64::Engine as _;
use sha2::{Digest, Sha256};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, OnceLock,
};
use std::time::Duration;
use sysinfo::{ProcessesToUpdate, System};
use tauri::{Emitter, Manager, State};
use tauri_runtime::ResizeDirection as TauriResizeDirection;

// ---------------------------------------------------------------------------
// Agent Bridge — localhost HTTP API for headless agent control (WP-0171)
// ---------------------------------------------------------------------------

static AGENT_APP_HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();
static AGENT_BRIDGE_STATE: OnceLock<Arc<Mutex<AgentBridgeInner>>> = OnceLock::new();

#[derive(Debug, Default)]
struct AgentBridgeInner {
    current_page: String,
    editor_item_id: Option<String>,
    safe_mode: bool,
    snapshot_tx: Option<std::sync::mpsc::Sender<String>>,
    dump_tx: Option<std::sync::mpsc::Sender<String>>,
}

fn agent_bridge_state() -> &'static Arc<Mutex<AgentBridgeInner>> {
    AGENT_BRIDGE_STATE.get_or_init(|| Arc::new(Mutex::new(AgentBridgeInner::default())))
}

fn spawn_agent_bridge(app_data_dir: &std::path::Path) {
    let port_file = app_data_dir.join("agent_bridge_port.txt");
    let port_file_cleanup = port_file.clone();

    std::thread::spawn(move || {
        let listener = match std::net::TcpListener::bind("127.0.0.1:0") {
            Ok(l) => l,
            Err(_) => return,
        };
        let port = match listener.local_addr() {
            Ok(addr) => addr.port(),
            Err(_) => return,
        };
        let _ = std::fs::write(&port_file_cleanup, port.to_string());

        // Non-blocking accept with 2 second timeout so we can check for shutdown
        let _ = listener.set_nonblocking(false);

        for stream in listener.incoming() {
            match stream {
                Ok(mut stream) => {
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
                    handle_agent_request(&mut stream);
                }
                Err(_) => continue,
            }
        }
    });

    // Clean up port file on app exit (best-effort via a Drop guard isn't easy here,
    // but the port file is harmless if stale — agent checks /health first)
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

    let (status, response_body) = match (method, path) {
        ("GET", "/agent/health") => ("200 OK", r#"{"status":"ok"}"#.to_string()),
        ("GET", "/agent/state") => ("200 OK", agent_handle_state()),
        ("POST", "/agent/navigate") => agent_handle_navigate(&body_str),
        ("POST", "/agent/snapshot") => agent_handle_snapshot(&body_str),
        ("POST", "/agent/dump") => agent_handle_dump(&body_str),
        _ => ("404 Not Found", r#"{"error":"not found"}"#.to_string()),
    };

    let response = format!(
        "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
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
    })
    .to_string()
}

fn agent_handle_navigate(body: &str) -> (&'static str, String) {
    let parsed: serde_json::Value = match serde_json::from_str(body) {
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

    let (tx, rx) = std::sync::mpsc::channel::<String>();

    {
        let mut state = agent_bridge_state().lock().unwrap();
        state.snapshot_tx = Some(tx);
    }

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

    // Wait for frontend to complete the snapshot (up to 30 seconds for heavy pages under load)
    match rx.recv_timeout(Duration::from_secs(30)) {
        Ok(path) => (
            "200 OK",
            format!(r#"{{"path":"{}"}}"#, path.replace('\\', "\\\\")),
        ),
        Err(_) => {
            // Clear stale sender so late-arriving captures don't contaminate the next request
            let mut state = agent_bridge_state().lock().unwrap();
            state.snapshot_tx = None;
            (
                "504 Gateway Timeout",
                r#"{"error":"snapshot timed out (30s)"}"#.to_string(),
            )
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
use voxvulgi_engine::models::ModelStore;
use voxvulgi_engine::paths::AppPaths;
use voxvulgi_engine::{
    config, db, diagnostics, instagram_subscriptions, jobs, library, speakers, subscriptions,
    subtitle_tracks, subtitles, tools, translate, voice_backend_adapters, voice_backends,
    voice_benchmarks, voice_cast_packs, voice_cleanup, voice_library, voice_plans,
    voice_reference_candidates, voice_reference_curation, voice_templates,
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
    runner: jobs::JobRunnerHandle,
    safe_mode_enabled: Arc<AtomicBool>,
    safe_mode_cli: bool,
    startup: Arc<Mutex<StartupTracker>>,
}

impl Drop for AppState {
    fn drop(&mut self) {
        self.runner.stop();
    }
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
    details: serde_json::Value,
    level: String,
) -> Result<String, String> {
    let path = diagnostics_trace_file_path(paths)?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| e.to_string())?;

    let row = DiagnosticsTraceEntry {
        ts_ms: now_epoch_ms_i64(),
        event,
        level,
        details,
        process: capture_process_snapshot(),
    };

    use std::io::Write as _;
    writeln!(
        file,
        "{}",
        serde_json::to_string(&row).map_err(|e| e.to_string())?
    )
    .map_err(|e| e.to_string())?;

    Ok(path.to_string_lossy().to_string())
}

fn append_diagnostics_trace_row_best_effort(
    paths: &AppPaths,
    event: &str,
    details: serde_json::Value,
    level: &str,
) {
    let _ = append_diagnostics_trace_row(paths, event.to_string(), details, level.to_string());
}

fn read_recent_diagnostics_trace_entries(
    paths: &AppPaths,
    limit: usize,
) -> Result<Vec<DiagnosticsTraceEntry>, String> {
    let path = diagnostics_trace_file_path(paths)?;
    if !path.exists() {
        return Ok(Vec::new());
    }

    let file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
    let reader = std::io::BufReader::new(file);
    use std::io::BufRead as _;
    let mut entries: Vec<DiagnosticsTraceEntry> = reader
        .lines()
        .map_while(|line| line.ok())
        .filter_map(|line| serde_json::from_str::<DiagnosticsTraceEntry>(&line).ok())
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

    if offline_bundle_already_applied(paths, &manifest.bundle_id) {
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
    use voxvulgi_engine::{config, paths::AppPaths};

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

        let rows = shell_paths_status(vec![
            existing.to_string_lossy().to_string(),
            missing.to_string_lossy().to_string(),
        ])
        .expect("status rows");

        assert_eq!(rows.len(), 2);
        assert!(rows[0].exists);
        assert!(!rows[0].is_dir);
        assert!(!rows[1].exists);
        assert!(!rows[1].is_dir);
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
        assert!(markdown_text.contains("# VoxVulgi app-state snapshot"));
        assert!(markdown_text.contains("## Feature health"));

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
        if path.is_dir() {
            command.arg(path.as_os_str());
        } else {
            command.arg("/select,").arg(path.as_os_str());
        }
        return run_shell_command(&mut command, "reveal path");
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

#[tauri::command]
fn shell_paths_status(paths: Vec<String>) -> Result<Vec<ShellPathStatus>, String> {
    let mut rows = Vec::with_capacity(paths.len());
    for path in paths {
        let normalized = normalize_shell_path(path, "Path")?;
        let meta = std::fs::metadata(&normalized);
        let (exists, is_dir) = match meta {
            Ok(value) => (true, value.is_dir()),
            Err(_) => (false, false),
        };
        rows.push(ShellPathStatus {
            path: normalized.to_string_lossy().to_string(),
            exists,
            is_dir,
        });
    }
    Ok(rows)
}

#[tauri::command]
fn shell_open_path(path: String) -> Result<ShellPathResult, String> {
    let normalized = normalize_existing_shell_path(path, "Path")?;
    shell_open_target(&normalized)?;
    Ok(ShellPathResult {
        path: normalized.to_string_lossy().to_string(),
        method: "shell_open_path".to_string(),
    })
}

#[tauri::command]
fn shell_reveal_path(path: String) -> Result<ShellPathResult, String> {
    let normalized = normalize_existing_shell_path(path, "Path")?;
    shell_reveal_target(&normalized)?;
    Ok(ShellPathResult {
        path: normalized.to_string_lossy().to_string(),
        method: "shell_reveal_path".to_string(),
    })
}

#[tauri::command]
fn shell_open_parent_dir(path: String) -> Result<ShellPathResult, String> {
    let normalized = normalize_existing_shell_path(path, "Path")?;
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

    Ok(DiagnosticsTraceDirStatus {
        current_dir: current_dir.to_string_lossy().to_string(),
        default_dir: default_dir.to_string_lossy().to_string(),
        exists,
        using_default: override_dir.is_none(),
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
    let whisper_ready = models
        .models
        .iter()
        .any(|model| model.id == "whispercpp-tiny" && model.installed);
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
    let library = build_library_snapshot(paths)?;
    let recent_trace = read_recent_diagnostics_trace_entries(paths, 40)?;
    let feature_health = build_feature_health_rows(
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
    paths: &AppPaths,
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
        if let Ok(doc) = subtitle_tracks::load_document(paths, &track.id) {
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

#[allow(clippy::too_many_arguments)]
fn localization_terminal_outcome(
    jobs: &[jobs::JobRow],
    source: &TrackAvailabilitySummary,
    translated_en: &TrackAvailabilitySummary,
    mix_exists: bool,
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

    let deliverable = if export_pack_exists {
        Some(("Export pack ready", export_pack_path))
    } else if mux_mp4_exists {
        Some(("Preview MP4 ready", mux_mp4_path))
    } else if mux_mkv_exists {
        Some(("Preview MKV ready", mux_mkv_path))
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
            Some(job.progress.clamp(0.0, 1.0)),
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

#[tauri::command]
fn item_outputs(
    state: State<'_, AppState>,
    item_id: Option<String>,
    itemId: Option<String>,
) -> Result<ItemOutputs, String> {
    let item_id = item_id
        .or(itemId)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| "missing required key itemId".to_string())?;

    let item = library::get_item_by_id(&state.paths, &item_id).map_err(|e| e.to_string())?;
    let item_dir = state.paths.derived_item_dir(&item_id);
    let dub_preview_dir = item_dir.join("dub_preview");
    let mix_path = dub_preview_dir.join("mix_dub_preview_v1.wav");
    let mux_mp4_path = dub_preview_dir.join("mux_dub_preview_v1.mp4");
    let mux_mkv_path = dub_preview_dir.join("mux_dub_preview_v1.mkv");
    let export_pack_path = item_dir.join("exports").join("export_pack_v1.zip");
    let tracks = subtitle_tracks::list_tracks(&state.paths, &item_id).unwrap_or_default();
    let source_summary =
        summarize_tracks_for_outputs(&state.paths, &tracks, |track| track.kind == "source");
    let translated_en_summary = summarize_tracks_for_outputs(&state.paths, &tracks, |track| {
        track.kind == "translated" && is_english_lang_tag(&track.lang)
    });
    let item_jobs = jobs::list_jobs_for_item(&state.paths, &item_id, 80, 0).unwrap_or_default();
    let mix_exists = mix_path.exists();
    let mux_mp4_exists = mux_mp4_path.exists();
    let mux_mkv_exists = mux_mkv_path.exists();
    let export_pack_exists = export_pack_path.exists();
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
        &mux_mp4_path,
        mux_mp4_exists,
        &mux_mkv_path,
        mux_mkv_exists,
        &export_pack_path,
        export_pack_exists,
        &item_dir,
    );

    Ok(ItemOutputs {
        item_id,
        source_media_path: item.media_path.clone(),
        source_media_exists: std::path::Path::new(&item.media_path).exists(),
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
    })
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
        "Mux dub preview (MP4)",
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
        item_dir.join("dub_preview").join("mux_dub_preview_v1.mkv"),
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
                &format!("Mux dub preview MP4 ({label})"),
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
                path.join("mux_dub_preview_v1.mkv"),
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
        let src_mp4 = dub_dir.join("mux_dub_preview_v1.mp4");
        let src_mkv = dub_dir.join("mux_dub_preview_v1.mkv");
        let out_ext = std::path::Path::new(&out_path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let src = match out_ext.as_str() {
            "mp4" => {
                if src_mp4.exists() {
                    src_mp4
                } else if src_mkv.exists() {
                    return Err(
                        "muxed preview exists only as MKV; choose a .mkv export path".to_string(),
                    );
                } else {
                    return Err("muxed preview not found".to_string());
                }
            }
            "mkv" => {
                if src_mkv.exists() {
                    src_mkv
                } else if src_mp4.exists() {
                    return Err(
                        "muxed preview exists only as MP4; choose a .mp4 export path".to_string(),
                    );
                } else {
                    return Err("muxed preview not found".to_string());
                }
            }
            _ => {
                if src_mp4.exists() {
                    src_mp4
                } else if src_mkv.exists() {
                    src_mkv
                } else {
                    return Err("muxed preview not found".to_string());
                }
            }
        };

        let dst = std::path::PathBuf::from(&out_path);
        if let Some(parent) = dst.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
        }
        std::fs::copy(&src, &dst).map_err(|e| e.to_string())?;
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
    current_startup_status(&state)
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
    let mut roots =
        config::load_feature_storage_roots_config(&state.paths).map_err(|e| e.to_string())?;
    set_feature_root_override(
        &mut roots,
        &feature,
        Some(normalized.to_string_lossy().to_string()),
    )?;
    config::save_feature_storage_roots_config(&state.paths, &roots).map_err(|e| e.to_string())?;
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
    let mut roots =
        config::load_feature_storage_roots_config(&state.paths).map_err(|e| e.to_string())?;
    set_feature_root_override(&mut roots, &feature, None)?;
    config::save_feature_storage_roots_config(&state.paths, &roots).map_err(|e| e.to_string())?;

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
fn diagnostics_trace_dir_set(
    state: State<'_, AppState>,
    path: String,
    create_if_missing: bool,
) -> Result<DiagnosticsTraceDirStatus, String> {
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
) -> Result<String, String> {
    let event = event.trim().to_string();
    if event.is_empty() {
        return Err("event is empty".to_string());
    }

    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        append_diagnostics_trace_row(
            &paths,
            event,
            details.unwrap_or(serde_json::Value::Null),
            level.unwrap_or_else(|| "info".to_string()),
        )
    })
    .await
    .map_err(|e| e.to_string())?
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
fn config_youtube_auth_get(
    state: State<'_, AppState>,
) -> Result<config::YoutubeAuthConfig, String> {
    config::load_youtube_auth_config(&state.paths).map_err(|e| e.to_string())
}

#[tauri::command]
fn config_youtube_auth_set(
    state: State<'_, AppState>,
    config_value: config::YoutubeAuthConfig,
) -> Result<config::YoutubeAuthConfig, String> {
    config::save_youtube_auth_config(&state.paths, &config_value).map_err(|e| e.to_string())?;
    Ok(config_value)
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
            });
        }

        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
        let parsed: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        Ok(Phase2InstallLatestState {
            exists: true,
            path: path.to_string_lossy().to_string(),
            state: Some(parsed),
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
fn tools_spleeter_install(state: State<'_, AppState>) -> Result<tools::SpleeterPackStatus, String> {
    tools::install_spleeter_pack(&state.paths).map_err(|e| e.to_string())
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
fn tools_demucs_install(state: State<'_, AppState>) -> Result<tools::DemucsPackStatus, String> {
    tools::install_demucs_pack(&state.paths).map_err(|e| e.to_string())
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
fn tools_diarization_install(
    state: State<'_, AppState>,
) -> Result<tools::DiarizationPackStatus, String> {
    tools::install_diarization_pack(&state.paths).map_err(|e| e.to_string())
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
fn tools_tts_preview_install(
    state: State<'_, AppState>,
) -> Result<tools::TtsPreviewPackStatus, String> {
    tools::install_tts_preview_pack(&state.paths).map_err(|e| e.to_string())
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
fn tools_tts_neural_local_v1_install(
    state: State<'_, AppState>,
) -> Result<tools::TtsNeuralLocalV1PackStatus, String> {
    tools::install_tts_neural_local_v1_pack(&state.paths).map_err(|e| e.to_string())
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
fn tools_tts_voice_preserving_local_v1_install(
    state: State<'_, AppState>,
) -> Result<tools::TtsVoicePreservingLocalV1PackStatus, String> {
    tools::install_tts_voice_preserving_local_v1_pack(&state.paths).map_err(|e| e.to_string())
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
fn library_list(
    state: State<'_, AppState>,
    limit: usize,
    offset: usize,
) -> Result<Vec<library::LibraryItem>, String> {
    library::list_items(&state.paths, limit, offset).map_err(|e| e.to_string())
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
    library::get_item_by_id(&state.paths, &item_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn youtube_subscriptions_list(
    state: State<'_, AppState>,
) -> Result<Vec<subscriptions::YoutubeSubscriptionRow>, String> {
    subscriptions::list_youtube_subscriptions(&state.paths).map_err(|e| e.to_string())
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
fn youtube_subscriptions_upsert(
    state: State<'_, AppState>,
    subscription: subscriptions::YoutubeSubscriptionUpsert,
) -> Result<subscriptions::YoutubeSubscriptionRow, String> {
    subscriptions::upsert_youtube_subscription(&state.paths, subscription)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn youtube_subscriptions_delete(state: State<'_, AppState>, id: String) -> Result<(), String> {
    subscriptions::delete_youtube_subscription(&state.paths, &id).map_err(|e| e.to_string())
}

#[tauri::command]
fn youtube_subscriptions_queue_one(
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<jobs::JobRow>, String> {
    subscriptions::queue_youtube_subscription(&state.paths, &id).map_err(|e| e.to_string())
}

#[tauri::command]
fn youtube_subscriptions_queue_all_active(
    state: State<'_, AppState>,
) -> Result<Vec<jobs::JobRow>, String> {
    subscriptions::queue_all_active_youtube_subscriptions(&state.paths).map_err(|e| e.to_string())
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
fn youtube_subscription_groups_list(
    state: State<'_, AppState>,
) -> Result<Vec<subscriptions::YoutubeSubscriptionGroupRow>, String> {
    subscriptions::list_youtube_subscription_groups(&state.paths).map_err(|e| e.to_string())
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
fn youtube_subscriptions_archive_stats(
    state: State<'_, AppState>,
) -> Result<std::collections::HashMap<String, usize>, String> {
    subscriptions::youtube_subscriptions_archive_stats(&state.paths).map_err(|e| e.to_string())
}

#[tauri::command]
fn youtube_subscriptions_active_refresh_ids(
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    jobs::active_youtube_subscription_refresh_ids(&state.paths)
        .map(|s| s.into_iter().collect())
        .map_err(|e| e.to_string())
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
    instagram_subscriptions::list_instagram_subscriptions(&state.paths).map_err(|e| e.to_string())
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
    instagram_subscriptions::queue_all_active_instagram_subscriptions(&state.paths)
        .map_err(|e| e.to_string())
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
fn download_presets_set(
    state: State<'_, AppState>,
    config_value: config::DownloadPresetsConfig,
) -> Result<config::DownloadPresetsConfig, String> {
    config::save_download_presets_config(&state.paths, &config_value).map_err(|e| e.to_string())?;
    config::load_download_presets_config(&state.paths).map_err(|e| e.to_string())
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
    config::save_download_presets_config(&state.paths, &parsed).map_err(|e| e.to_string())?;
    config::load_download_presets_config(&state.paths).map_err(|e| e.to_string())
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
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        jobs::list_jobs(&paths, limit, offset).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn jobs_list_for_item(
    state: State<'_, AppState>,
    item_id: Option<String>,
    itemId: Option<String>,
    limit: usize,
    offset: usize,
) -> Result<Vec<jobs::JobRow>, String> {
    let item_id = item_id
        .or(itemId)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| "missing required key itemId".to_string())?;
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        jobs::list_jobs_for_item(&paths, &item_id, limit, offset).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
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
    preset_id: Option<String>,
) -> Result<Vec<jobs::JobRow>, String> {
    jobs::enqueue_download_direct_url_batch(
        &state.paths,
        urls,
        auth_cookie,
        output_dir,
        use_browser_cookies,
        preset_id,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn jobs_enqueue_instagram_batch(
    state: State<'_, AppState>,
    urls: Vec<String>,
    auth_cookie: Option<String>,
    output_dir: Option<String>,
    use_browser_cookies: Option<bool>,
) -> Result<Vec<jobs::JobRow>, String> {
    jobs::enqueue_download_instagram_batch(
        &state.paths,
        urls,
        auth_cookie,
        output_dir,
        use_browser_cookies,
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

    jobs::enqueue_diarize_local_v1_with_backend(&state.paths, item_id, source_track_id, backend)
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
fn jobs_queue_control_get(
    state: State<'_, AppState>,
) -> Result<jobs::JobQueueControlState, String> {
    jobs::get_queue_control(&state.paths).map_err(|e| e.to_string())
}

#[tauri::command]
fn jobs_queue_control_set(
    state: State<'_, AppState>,
    paused: bool,
) -> Result<jobs::JobQueueControlState, String> {
    jobs::set_queue_paused(&state.paths, paused).map_err(|e| e.to_string())
}

#[tauri::command]
fn jobs_runtime_settings_get(
    state: State<'_, AppState>,
) -> Result<jobs::JobRuntimeSettings, String> {
    jobs::get_runtime_settings(&state.paths).map_err(|e| e.to_string())
}

#[tauri::command]
fn jobs_runtime_settings_set(
    state: State<'_, AppState>,
    max_concurrency: usize,
) -> Result<jobs::JobRuntimeSettings, String> {
    jobs::set_runtime_max_concurrency(&state.paths, max_concurrency).map_err(|e| e.to_string())
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let base_dir = app.path().app_data_dir()?;
            let paths = AppPaths::new(AppPaths::normalize_base_dir(&base_dir));
            let startup = Arc::new(Mutex::new(StartupTracker::new()));
            let _ = AGENT_APP_HANDLE.set(app.handle().clone());
            spawn_agent_bridge(&AppPaths::normalize_base_dir(&base_dir));
            set_startup_phase(&startup, &paths, "app_dirs", "running", None);
            paths.ensure_dirs()?;
            set_startup_phase(&startup, &paths, "app_dirs", "ready", None);
            let cli_safe_mode = std::env::args().any(|value| value.trim() == "--safe-mode");
            let persisted_safe_mode = config::load_safe_mode_config(&paths)
                .map(|value| value.enabled)
                .unwrap_or(false);
            let safe_mode_enabled = cli_safe_mode || persisted_safe_mode;
            if safe_mode_enabled {
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
                            set_startup_phase(
                                &startup_for_thread,
                                &paths_for_bundle,
                                "offline_bundle",
                                "ready",
                                None,
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
            set_startup_phase(&startup, &paths, "db_schema", "running", None);
            db::ensure_schema(&paths)?;
            set_startup_phase(&startup, &paths, "db_schema", "ready", None);
            if safe_mode_enabled {
                let _ = jobs::set_queue_paused(&paths, true);
            }
            set_startup_phase(&startup, &paths, "job_runner", "running", None);
            let runner = jobs::start_runner(paths.clone())?;
            set_startup_phase(&startup, &paths, "job_runner", "ready", None);
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
            library_thumbnail_data_url,
            item_artifacts_list_v1,
            item_export_mux_preview_mp4,
            item_qc_report_v1_load,
            diagnostics_trace_clear,
            diagnostics_trace_dir_set,
            diagnostics_trace_dir_status,
            diagnostics_trace_dir_use_default,
            diagnostics_trace_recent,
            diagnostics_trace_write_event,
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
            config_youtube_auth_get,
            config_youtube_auth_set,
            config_diarization_optional_clear_token,
            config_diarization_optional_set,
            config_diarization_optional_status,
            download_presets_export_json,
            download_presets_get,
            download_presets_import_json,
            download_presets_set,
            library_get,
            library_list,
            localization_workspace_list,
            youtube_subscription_groups_delete,
            youtube_subscription_groups_list,
            youtube_subscription_groups_set_for_subscription,
            youtube_subscription_groups_upsert,
            youtube_subscriptions_list,
            youtube_subscriptions_output_dir,
            youtube_subscriptions_upsert,
            youtube_subscriptions_delete,
            youtube_subscriptions_import_existing_downloads,
            legacy_archive_analyze,
            youtube_subscriptions_queue_one,
            youtube_subscriptions_queue_all_active,
            youtube_subscriptions_queue_group,
            youtube_subscriptions_export_json,
            youtube_subscriptions_import_json,
            youtube_subscriptions_import_4kvdp_dir,
            youtube_subscriptions_import_4kvdp_state,
            youtube_subscriptions_seed_archive_scan,
            youtube_subscriptions_archive_stats,
            youtube_subscriptions_active_refresh_ids,
            instagram_subscriptions_list,
            instagram_subscriptions_upsert,
            instagram_subscriptions_delete,
            instagram_subscriptions_queue_one,
            instagram_subscriptions_queue_all_active,
            instagram_subscriptions_output_dir,
            jobs_cancel,
            jobs_cancel_all,
            jobs_enqueue_dummy,
            jobs_enqueue_asr_local,
            jobs_enqueue_download_batch,
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
            jobs_queue_control_get,
            jobs_queue_control_set,
            jobs_item_artifact_retention_policy,
            jobs_log_retention_policy,
            jobs_prune_logs,
            jobs_runtime_settings_get,
            jobs_runtime_settings_set,
            jobs_retry,
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
            agent_dump_complete
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
