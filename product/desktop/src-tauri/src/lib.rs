use tauri::{Manager, State};
use voxvulgi_engine::models::ModelStore;
use voxvulgi_engine::paths::AppPaths;
use voxvulgi_engine::{
    config, db, diagnostics, jobs, library, speakers, subtitle_tracks, subtitles, subscriptions, tools,
};

#[derive(Debug, Clone, serde::Deserialize)]
struct OfflineBundleManifest {
    schema_version: u32,
    bundle_id: String,
    #[serde(default)]
    payload_zip: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct Phase2InstallLatestState {
    exists: bool,
    path: String,
    state: Option<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct ArtifactInfo {
    id: String,
    title: String,
    path: String,
    exists: bool,
    group: String,
}

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
    app_name: String,
    app_version: String,
    engine_version: String,
}

#[derive(Debug, Clone, serde::Serialize)]
struct DownloadDirStatus {
    current_dir: String,
    default_dir: String,
    exists: bool,
    using_default: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
struct DiagnosticsTraceDirStatus {
    current_dir: String,
    default_dir: String,
    exists: bool,
    using_default: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
struct DiagnosticsTraceClearSummary {
    removed_entries: usize,
    removed_bytes: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
struct ItemOutputs {
    item_id: String,
    derived_item_dir: String,
    dub_preview_dir: String,
    mix_dub_preview_v1_wav_path: String,
    mix_dub_preview_v1_wav_exists: bool,
    mux_dub_preview_v1_mp4_path: String,
    mux_dub_preview_v1_mp4_exists: bool,
    mux_dub_preview_v1_mkv_path: String,
    mux_dub_preview_v1_mkv_exists: bool,
    export_pack_v1_zip_path: String,
    export_pack_v1_zip_exists: bool,
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

fn extract_payload_zip_best_effort(zip_path: &std::path::Path, paths: &AppPaths) -> Result<ZipExtractSummary, String> {
    use zip::result::ZipError;

    let file = std::fs::File::open(zip_path)
        .map_err(|e| format!("failed to open payload zip {}: {e}", zip_path.to_string_lossy()))?;
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
            std::fs::create_dir_all(&out_path).map_err(|e| {
                format!("failed to create dir {}: {e}", out_path.to_string_lossy())
            })?;
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
            std::fs::create_dir_all(parent).map_err(|e| {
                format!("failed to create dir {}: {e}", parent.to_string_lossy())
            })?;
        }

        let tmp = out_path.with_extension("extracting");
        let _ = std::fs::remove_file(&tmp);

        {
            let mut out_file = std::fs::File::create(&tmp).map_err(|e| {
                format!("failed to create file {}: {e}", tmp.to_string_lossy())
            })?;
            std::io::copy(&mut entry, &mut out_file).map_err(|e| {
                format!("failed to extract {}: {e}", name)
            })?;
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

fn read_offline_bundle_manifest(bundle_root: &std::path::Path) -> Result<OfflineBundleManifest, String> {
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

fn offline_bundle_marker_path(paths: &AppPaths) -> std::path::PathBuf {
    paths
        .config_dir()
        .join("offline_bundle_applied_v1.json")
}

fn offline_bundle_already_applied(
    paths: &AppPaths,
    bundle_id: &str,
) -> bool {
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

fn write_offline_bundle_marker(paths: &AppPaths, bundle_root: &std::path::Path, bundle_id: &str) -> Result<(), String> {
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

    std::fs::write(&marker, format!("{}\n", serde_json::to_string_pretty(&record).unwrap_or_else(|_| "{}".to_string())))
        .map_err(|e| format!("failed to write offline bundle marker {}: {e}", marker.to_string_lossy()))?;
    Ok(())
}

fn copy_tree_best_effort(src_root: &std::path::Path, dst_root: &std::path::Path) -> Result<CopySummary, String> {
    if !src_root.exists() {
        return Ok(CopySummary::default());
    }

    let mut summary = CopySummary::default();
    let mut stack: Vec<std::path::PathBuf> = vec![src_root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir).map_err(|e| {
            format!("failed to read dir {}: {e}", dir.to_string_lossy())
        })?;

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
                std::fs::create_dir_all(&dst).map_err(|e| {
                    format!("failed to create dir {}: {e}", dst.to_string_lossy())
                })?;
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
            out.push(format!("executable = {}", portable_python.to_string_lossy()));
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
        out.push(format!("executable = {}", portable_python.to_string_lossy()));
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

fn apply_offline_bundle_if_present(paths: &AppPaths, resource_dir: &std::path::Path) -> Result<(), String> {
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
        let sum = extract_payload_zip_best_effort(&payload_zip_path, paths)?;
        patch_venv_pyvenv_cfg_best_effort(paths)?;
        write_offline_bundle_marker(paths, &bundle_root, &manifest.bundle_id)?;

        eprintln!(
            "offline bundle: extracted payload zip {} (files={} bytes={} skipped={})",
            payload_zip_name,
            sum.extracted_files,
            sum.extracted_bytes,
            sum.skipped_files,
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

fn build_diagnostics_trace_dir_status(paths: &AppPaths) -> Result<DiagnosticsTraceDirStatus, String> {
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

fn clear_dir_entries_with_bytes(dir: &std::path::Path) -> Result<DiagnosticsTraceClearSummary, String> {
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

    let item_dir = state.paths.derived_item_dir(&item_id);
    let dub_preview_dir = item_dir.join("dub_preview");
    let mix_path = dub_preview_dir.join("mix_dub_preview_v1.wav");
    let mux_mp4_path = dub_preview_dir.join("mux_dub_preview_v1.mp4");
    let mux_mkv_path = dub_preview_dir.join("mux_dub_preview_v1.mkv");
    let export_pack_path = item_dir.join("exports").join("export_pack_v1.zip");

    Ok(ItemOutputs {
        item_id,
        derived_item_dir: item_dir.to_string_lossy().to_string(),
        dub_preview_dir: dub_preview_dir.to_string_lossy().to_string(),
        mix_dub_preview_v1_wav_path: mix_path.to_string_lossy().to_string(),
        mix_dub_preview_v1_wav_exists: mix_path.exists(),
        mux_dub_preview_v1_mp4_path: mux_mp4_path.to_string_lossy().to_string(),
        mux_dub_preview_v1_mp4_exists: mux_mp4_path.exists(),
        mux_dub_preview_v1_mkv_path: mux_mkv_path.to_string_lossy().to_string(),
        mux_dub_preview_v1_mkv_exists: mux_mkv_path.exists(),
        export_pack_v1_zip_path: export_pack_path.to_string_lossy().to_string(),
        export_pack_v1_zip_exists: export_pack_path.exists(),
    })
}

#[tauri::command]
#[allow(non_snake_case)]
fn item_qc_report_v1_load(
    state: State<'_, AppState>,
    item_id: Option<String>,
    itemId: Option<String>,
    track_id: Option<String>,
    trackId: Option<String>,
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

    let path = state
        .paths
        .derived_item_dir(&item_id)
        .join("qc")
        .join(format!("qc_report_v1_{track_id}.json"));
    if !path.exists() {
        return Ok(None);
    }

    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    let parsed: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    Ok(Some(parsed))
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

    let mut push = |id: &str, title: &str, group: &str, path: std::path::PathBuf| {
        out.push(ArtifactInfo {
            id: id.to_string(),
            title: title.to_string(),
            path: path.to_string_lossy().to_string(),
            exists: path.exists(),
            group: group.to_string(),
        });
    };

    // Separation
    push(
        "sep_spleeter_vocals",
        "Vocals (Spleeter)",
        "Separation",
        item_dir.join("separation").join("spleeter_2stems").join("vocals.wav"),
    );
    push(
        "sep_spleeter_background",
        "Background (Spleeter)",
        "Separation",
        item_dir
            .join("separation")
            .join("spleeter_2stems")
            .join("background.wav"),
    );
    push(
        "sep_demucs_vocals",
        "Vocals (Demucs)",
        "Separation",
        item_dir.join("separation").join("demucs_two_stems_v1").join("vocals.wav"),
    );
    push(
        "sep_demucs_background",
        "Background (Demucs)",
        "Separation",
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
        item_dir.join("cleanup").join("vocals_clean_v1.wav"),
    );

    // TTS manifests
    push(
        "tts_pyttsx3_manifest",
        "TTS manifest (pyttsx3)",
        "TTS",
        item_dir.join("tts_preview").join("pyttsx3_v1").join("manifest.json"),
    );
    push(
        "tts_neural_manifest",
        "TTS manifest (neural local v1)",
        "TTS",
        item_dir
            .join("tts_preview")
            .join("tts_neural_local_v1")
            .join("manifest.json"),
    );
    push(
        "tts_voice_preserving_manifest",
        "TTS manifest (voice-preserving)",
        "TTS",
        item_dir
            .join("tts_preview")
            .join("dub_voice_preserving_v1")
            .join("manifest.json"),
    );

    // Dub preview
    push(
        "dub_mix",
        "Mix dub preview (WAV)",
        "Dub preview",
        item_dir.join("dub_preview").join("mix_dub_preview_v1.wav"),
    );
    push(
        "dub_mux_mp4",
        "Mux dub preview (MP4)",
        "Dub preview",
        item_dir.join("dub_preview").join("mux_dub_preview_v1.mp4"),
    );
    push(
        "dub_mux_mkv",
        "Mux dub preview (MKV)",
        "Dub preview",
        item_dir.join("dub_preview").join("mux_dub_preview_v1.mkv"),
    );

    // Export
    push(
        "export_pack",
        "Export pack (zip)",
        "Export",
        item_dir.join("exports").join("export_pack_v1.zip"),
    );

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
                        path,
                    );
                }
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
                    return Err("muxed preview exists only as MKV; choose a .mkv export path".to_string());
                } else {
                    return Err("muxed preview not found".to_string());
                }
            }
            "mkv" => {
                if src_mkv.exists() {
                    src_mkv
                } else if src_mp4.exists() {
                    return Err("muxed preview exists only as MP4; choose a .mp4 export path".to_string());
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
        diagnostics::export_diagnostics_bundle(&paths, std::path::PathBuf::from(out_path), &app_name, &app_version)
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
fn diagnostics_trace_clear(
    state: State<'_, AppState>,
) -> Result<DiagnosticsTraceClearSummary, String> {
    let dir = state
        .paths
        .effective_diagnostics_trace_dir()
        .map_err(|e| e.to_string())?;
    clear_dir_entries_with_bytes(&dir)
}

#[tauri::command]
fn diagnostics_trace_write_event(
    state: State<'_, AppState>,
    event: String,
    details: Option<serde_json::Value>,
    level: Option<String>,
) -> Result<String, String> {
    let event = event.trim().to_string();
    if event.is_empty() {
        return Err("event is empty".to_string());
    }

    let dir = state
        .paths
        .effective_diagnostics_trace_dir()
        .map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let path = dir.join("diagnostics_trace.jsonl");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| e.to_string())?;

    let ts_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let row = serde_json::json!({
        "ts_ms": ts_ms,
        "event": event,
        "level": level.unwrap_or_else(|| "info".to_string()),
        "details": details.unwrap_or(serde_json::Value::Null),
    });

    use std::io::Write as _;
    writeln!(file, "{}", row).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
fn config_batch_on_import_get(state: State<'_, AppState>) -> Result<config::BatchOnImportRules, String> {
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
    config::save_optional_diarization_backend_config(
        &state.paths,
        &config_value,
        token.as_deref(),
    )
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
fn tools_ytdlp_status(state: State<'_, AppState>) -> tools::YtDlpToolsStatus {
    tools::ytdlp_tools_status(&state.paths)
}

#[tauri::command]
fn tools_ytdlp_install(state: State<'_, AppState>) -> Result<tools::YtDlpToolsStatus, String> {
    tools::install_ytdlp_tools(&state.paths).map_err(|e| e.to_string())
}

#[tauri::command]
fn tools_python_status(state: State<'_, AppState>) -> tools::PythonToolchainStatus {
    tools::python_toolchain_status(&state.paths)
}

#[tauri::command]
fn tools_python_install(
    state: State<'_, AppState>,
) -> Result<tools::PythonToolchainStatus, String> {
    tools::install_python_toolchain(&state.paths).map_err(|e| e.to_string())
}

#[tauri::command]
fn tools_python_portable_status(state: State<'_, AppState>) -> tools::PortablePythonStatus {
    tools::portable_python_status(&state.paths)
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
fn tools_phase2_packs_install_latest_state(
    state: State<'_, AppState>,
) -> Result<Phase2InstallLatestState, String> {
    let path = state
        .paths
        .install_logs_dir()
        .join("phase2")
        .join("latest.json");

    if !path.exists() {
        return Ok(Phase2InstallLatestState {
            exists: false,
            path: path.to_string_lossy().to_string(),
            state: None,
        });
    }

    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    let parsed: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    Ok(Phase2InstallLatestState {
        exists: true,
        path: path.to_string_lossy().to_string(),
        state: Some(parsed),
    })
}

#[tauri::command]
fn tools_pack_integrity_manifest_status(state: State<'_, AppState>) -> tools::PackIntegrityManifestStatus {
    tools::pack_integrity_manifest_status(&state.paths)
}

#[tauri::command]
fn tools_pack_integrity_manifest_generate(
    state: State<'_, AppState>,
) -> Result<tools::PackIntegrityManifestResult, String> {
    tools::generate_pack_integrity_manifest(&state.paths).map_err(|e| e.to_string())
}

#[tauri::command]
fn tools_performance_tier_status(state: State<'_, AppState>) -> tools::PerformanceTierStatus {
    tools::performance_tier_status(&state.paths)
}

#[tauri::command]
fn tools_spleeter_status(state: State<'_, AppState>) -> tools::SpleeterPackStatus {
    tools::spleeter_pack_status(&state.paths)
}

#[tauri::command]
fn tools_spleeter_install(state: State<'_, AppState>) -> Result<tools::SpleeterPackStatus, String> {
    tools::install_spleeter_pack(&state.paths).map_err(|e| e.to_string())
}

#[tauri::command]
fn tools_demucs_status(state: State<'_, AppState>) -> tools::DemucsPackStatus {
    tools::demucs_pack_status(&state.paths)
}

#[tauri::command]
fn tools_demucs_install(state: State<'_, AppState>) -> Result<tools::DemucsPackStatus, String> {
    tools::install_demucs_pack(&state.paths).map_err(|e| e.to_string())
}

#[tauri::command]
fn tools_diarization_status(state: State<'_, AppState>) -> tools::DiarizationPackStatus {
    tools::diarization_pack_status(&state.paths)
}

#[tauri::command]
fn tools_diarization_install(
    state: State<'_, AppState>,
) -> Result<tools::DiarizationPackStatus, String> {
    tools::install_diarization_pack(&state.paths).map_err(|e| e.to_string())
}

#[tauri::command]
fn tools_tts_preview_status(state: State<'_, AppState>) -> tools::TtsPreviewPackStatus {
    tools::tts_preview_pack_status(&state.paths)
}

#[tauri::command]
fn tools_tts_preview_install(
    state: State<'_, AppState>,
) -> Result<tools::TtsPreviewPackStatus, String> {
    tools::install_tts_preview_pack(&state.paths).map_err(|e| e.to_string())
}

#[tauri::command]
fn tools_tts_neural_local_v1_status(state: State<'_, AppState>) -> tools::TtsNeuralLocalV1PackStatus {
    tools::tts_neural_local_v1_pack_status(&state.paths)
}

#[tauri::command]
fn tools_tts_neural_local_v1_install(
    state: State<'_, AppState>,
) -> Result<tools::TtsNeuralLocalV1PackStatus, String> {
    tools::install_tts_neural_local_v1_pack(&state.paths).map_err(|e| e.to_string())
}

#[tauri::command]
fn tools_tts_voice_preserving_local_v1_status(
    state: State<'_, AppState>,
) -> tools::TtsVoicePreservingLocalV1PackStatus {
    tools::tts_voice_preserving_local_v1_pack_status(&state.paths)
}

#[tauri::command]
fn tools_tts_voice_preserving_local_v1_install(
    state: State<'_, AppState>,
) -> Result<tools::TtsVoicePreservingLocalV1PackStatus, String> {
    tools::install_tts_voice_preserving_local_v1_pack(&state.paths).map_err(|e| e.to_string())
}

#[tauri::command]
fn tools_tts_preview_pyttsx3_voices(state: State<'_, AppState>) -> Result<Vec<tools::Pyttsx3Voice>, String> {
    tools::tts_preview_pyttsx3_list_voices(&state.paths).map_err(|e| e.to_string())
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
    tts_voice_id: Option<String>,
    tts_voice_profile_path: Option<String>,
) -> Result<speakers::ItemSpeakerSetting, String> {
    speakers::upsert_item_speaker_setting(
        &state.paths,
        &item_id,
        &speaker_key,
        display_name,
        tts_voice_id,
        tts_voice_profile_path,
    )
    .map_err(|e| e.to_string())
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
fn youtube_subscriptions_list(
    state: State<'_, AppState>,
) -> Result<Vec<subscriptions::YoutubeSubscriptionRow>, String> {
    subscriptions::list_youtube_subscriptions(&state.paths).map_err(|e| e.to_string())
}

#[tauri::command]
fn youtube_subscriptions_upsert(
    state: State<'_, AppState>,
    subscription: subscriptions::YoutubeSubscriptionUpsert,
) -> Result<subscriptions::YoutubeSubscriptionRow, String> {
    subscriptions::upsert_youtube_subscription(&state.paths, subscription).map_err(|e| e.to_string())
}

#[tauri::command]
fn youtube_subscriptions_delete(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
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
    subscriptions::export_youtube_subscriptions_json(&state.paths, &std::path::PathBuf::from(out_path))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn youtube_subscriptions_import_json(
    state: State<'_, AppState>,
    in_path: String,
) -> Result<subscriptions::YoutubeSubscriptionsImportSummary, String> {
    subscriptions::import_youtube_subscriptions_json(&state.paths, &std::path::PathBuf::from(in_path))
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
fn jobs_enqueue_install_phase2_packs_v1(state: State<'_, AppState>) -> Result<jobs::JobRow, String> {
    jobs::enqueue_install_phase2_packs_v1(&state.paths).map_err(|e| e.to_string())
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
    jobs::enqueue_tts_neural_local_v1(&state.paths, item_id, source_track_id).map_err(|e| e.to_string())
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
) -> Result<jobs::JobRow, String> {
    jobs::enqueue_qc_report_v1(&state.paths, item_id, track_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn jobs_enqueue_export_pack_v1(
    state: State<'_, AppState>,
    item_id: String,
) -> Result<jobs::JobRow, String> {
    jobs::enqueue_export_pack_v1(&state.paths, item_id).map_err(|e| e.to_string())
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
            if let Ok(resource_dir) = app.path().resource_dir() {
                apply_offline_bundle_if_present(&paths, &resource_dir)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
            }
            db::ensure_schema(&paths)?;
            let runner = jobs::start_runner(paths.clone())?;
            app.manage(AppState { paths, runner });
            Ok(())
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
         .invoke_handler(tauri::generate_handler![
             diagnostics_info,
             diagnostics_clear_cache,
             diagnostics_export_bundle,
             diagnostics_generate_licensing_report,
             diagnostics_storage_breakdown,
             item_outputs,
             item_artifacts_list_v1,
             item_export_mux_preview_mp4,
             item_qc_report_v1_load,
             diagnostics_trace_clear,
             diagnostics_trace_dir_set,
             diagnostics_trace_dir_status,
             diagnostics_trace_dir_use_default,
             diagnostics_trace_write_event,
             downloads_dir_set,
             downloads_dir_status,
             downloads_dir_use_default,
             config_batch_on_import_get,
             config_batch_on_import_set,
             config_diarization_optional_clear_token,
             config_diarization_optional_set,
             config_diarization_optional_status,
             library_get,
             library_list,
             youtube_subscriptions_list,
             youtube_subscriptions_upsert,
             youtube_subscriptions_delete,
             youtube_subscriptions_queue_one,
             youtube_subscriptions_queue_all_active,
             youtube_subscriptions_export_json,
             youtube_subscriptions_import_json,
             youtube_subscriptions_import_4kvdp_dir,
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
            jobs_enqueue_mix_dub_preview_v1,
            jobs_enqueue_mux_dub_preview_v1,
            jobs_enqueue_separate_audio_spleeter,
            jobs_enqueue_separate_audio_demucs_v1,
            jobs_enqueue_clean_vocals_v1,
            jobs_enqueue_qc_report_v1,
            jobs_enqueue_export_pack_v1,
            jobs_enqueue_translate_local,
            jobs_flush_cache,
            jobs_list,
            jobs_queue_control_get,
            jobs_queue_control_set,
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
            subtitles_export_doc_srt,
            subtitles_export_doc_vtt,
            subtitles_list_tracks,
            subtitles_load_track,
            subtitles_save_new_version,
             tools_ffmpeg_install,
             tools_ffmpeg_status,
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
            window_toggle_maximize
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

