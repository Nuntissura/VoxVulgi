use tauri::{Manager, State};
use voxvulgi_engine::models::ModelStore;
use voxvulgi_engine::paths::AppPaths;
use voxvulgi_engine::{db, jobs, library, subtitle_tracks, subtitles, tools};

#[derive(Debug, Clone)]
struct AppState {
    paths: AppPaths,
    runner: jobs::JobRunnerHandle,
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
}

#[derive(Debug, Clone, serde::Serialize)]
struct DownloadDirStatus {
    current_dir: String,
    default_dir: String,
    exists: bool,
    using_default: bool,
}

fn ensure_media_output_layout(root: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
    for sub in ["video", "instagram", "images"] {
        std::fs::create_dir_all(root.join(sub)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn build_download_dir_status(paths: &AppPaths) -> Result<DownloadDirStatus, String> {
    let default_dir = paths.default_download_dir();
    let override_dir = paths.download_dir_override().map_err(|e| e.to_string())?;
    let current_dir = override_dir.clone().unwrap_or_else(|| default_dir.clone());
    let exists = current_dir.exists() && current_dir.is_dir();

    Ok(DownloadDirStatus {
        current_dir: current_dir.to_string_lossy().to_string(),
        default_dir: default_dir.to_string_lossy().to_string(),
        exists,
        using_default: override_dir.is_none(),
    })
}

#[tauri::command]
fn diagnostics_info(state: State<'_, AppState>) -> DiagnosticsInfo {
    DiagnosticsInfo {
        app_data_dir: state.paths.base_dir.to_string_lossy().to_string(),
        db_path: state
            .paths
            .db_dir()
            .join("app.sqlite")
            .to_string_lossy()
            .to_string(),
    }
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
fn models_inventory(
    state: State<'_, AppState>,
) -> Result<voxvulgi_engine::models::ModelInventory, String> {
    let store = ModelStore::new(state.paths.clone());
    store.inventory().map_err(|e| e.to_string())
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
fn tools_ffmpeg_status(state: State<'_, AppState>) -> tools::FfmpegToolsStatus {
    tools::ffmpeg_tools_status(&state.paths)
}

#[tauri::command]
fn tools_ffmpeg_install(state: State<'_, AppState>) -> Result<tools::FfmpegToolsStatus, String> {
    tools::install_ffmpeg_tools(&state.paths).map_err(|e| e.to_string())
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
fn library_get(
    state: State<'_, AppState>,
    item_id: String,
) -> Result<library::LibraryItem, String> {
    library::get_item_by_id(&state.paths, &item_id).map_err(|e| e.to_string())
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
fn jobs_list(
    state: State<'_, AppState>,
    limit: usize,
    offset: usize,
) -> Result<Vec<jobs::JobRow>, String> {
    jobs::list_jobs(&state.paths, limit, offset).map_err(|e| e.to_string())
}

#[tauri::command]
fn jobs_enqueue_import_local(
    state: State<'_, AppState>,
    path: String,
) -> Result<jobs::JobRow, String> {
    jobs::enqueue_import_local(&state.paths, path).map_err(|e| e.to_string())
}

#[tauri::command]
fn jobs_enqueue_download_batch(
    state: State<'_, AppState>,
    urls: Vec<String>,
    auth_cookie: Option<String>,
    output_dir: Option<String>,
    use_browser_cookies: Option<bool>,
) -> Result<Vec<jobs::JobRow>, String> {
    jobs::enqueue_download_direct_url_batch(
        &state.paths,
        urls,
        auth_cookie,
        output_dir,
        use_browser_cookies,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn jobs_enqueue_instagram_batch(
    state: State<'_, AppState>,
    urls: Vec<String>,
    auth_cookie: Option<String>,
    output_dir: Option<String>,
) -> Result<Vec<jobs::JobRow>, String> {
    jobs::enqueue_download_instagram_batch(&state.paths, urls, auth_cookie, output_dir)
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
fn jobs_flush_cache(state: State<'_, AppState>) -> Result<jobs::JobFlushSummary, String> {
    jobs::flush_jobs_cache(&state.paths).map_err(|e| e.to_string())
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let base_dir = app.path().app_data_dir()?;
            let paths = AppPaths::new(AppPaths::normalize_base_dir(&base_dir));
            paths.ensure_dirs()?;
            db::ensure_schema(&paths)?;
            let runner = jobs::start_runner(paths.clone())?;
            app.manage(AppState { paths, runner });
            Ok(())
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            diagnostics_info,
            downloads_dir_set,
            downloads_dir_status,
            downloads_dir_use_default,
            library_get,
            library_list,
            jobs_cancel,
            jobs_cancel_all,
            jobs_enqueue_dummy,
            jobs_enqueue_asr_local,
            jobs_enqueue_download_batch,
            jobs_enqueue_instagram_batch,
            jobs_enqueue_image_batch,
            jobs_enqueue_import_local,
            jobs_enqueue_translate_local,
            jobs_flush_cache,
            jobs_list,
            jobs_queue_control_get,
            jobs_queue_control_set,
            jobs_runtime_settings_get,
            jobs_runtime_settings_set,
            jobs_retry,
            models_inventory,
            models_install,
            models_install_demo,
            subtitles_export_doc_srt,
            subtitles_export_doc_vtt,
            subtitles_list_tracks,
            subtitles_load_track,
            subtitles_save_new_version,
            tools_ffmpeg_install,
            tools_ffmpeg_status,
            window_close,
            window_minimize,
            window_start_drag,
            window_toggle_maximize
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
