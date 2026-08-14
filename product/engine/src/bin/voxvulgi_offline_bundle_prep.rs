use std::path::{Path, PathBuf};

use voxvulgi_engine::models::ModelStore;
use voxvulgi_engine::paths::AppPaths;
use voxvulgi_engine::pinned_dependency_manifest;
use voxvulgi_engine::{cmd, db, tools, EngineError, Result};

fn main() -> std::result::Result<(), String> {
    run().map_err(|e| e.to_string())
}

fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_help();
        return Ok(());
    }

    let mut stage_base_dir: Option<PathBuf> = None;
    let mut out_dir: Option<PathBuf> = None;
    let mut force = false;
    let mut export_only = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--stage-base-dir" | "--base-dir" => {
                i += 1;
                let v = args.get(i).ok_or_else(|| {
                    EngineError::InstallFailed("--stage-base-dir requires a value".to_string())
                })?;
                stage_base_dir = Some(PathBuf::from(v));
            }
            "--out-dir" => {
                i += 1;
                let v = args.get(i).ok_or_else(|| {
                    EngineError::InstallFailed("--out-dir requires a value".to_string())
                })?;
                out_dir = Some(PathBuf::from(v));
            }
            "--force" => force = true,
            "--export-only" => export_only = true,
            other => {
                return Err(EngineError::InstallFailed(format!(
                    "unknown arg: {other} (try --help)"
                )));
            }
        }
        i += 1;
    }

    let stage_base_dir = stage_base_dir.ok_or_else(|| {
        EngineError::InstallFailed("missing required --stage-base-dir".to_string())
    })?;
    let out_dir = out_dir
        .ok_or_else(|| EngineError::InstallFailed("missing required --out-dir".to_string()))?;

    if force && export_only {
        return Err(EngineError::InstallFailed(
            "--force and --export-only cannot be used together".to_string(),
        ));
    }

    let paths = AppPaths::new(stage_base_dir.clone());
    paths.ensure_dirs()?;
    db::ensure_schema(&paths)?;

    if force {
        let _ = std::fs::remove_dir_all(paths.tools_dir());
        let _ = std::fs::remove_dir_all(paths.models_dir());
        let _ = std::fs::remove_dir_all(paths.cache_dir().join("huggingface"));
        let _ = std::fs::remove_dir_all(paths.voice_backends_dir());
        paths.ensure_dirs()?;
    }

    println!("stage base dir: {}", paths.base_dir.to_string_lossy());
    println!("out dir: {}", out_dir.to_string_lossy());

    if export_only {
        println!("export-only requested; reusing the existing prepared stage.");
        export_offline_payload(&paths, &out_dir)?;
        println!("done.");
        return Ok(());
    }

    // Phase 1: FFmpeg + whisper model.
    {
        let status = tools::ffmpeg_tools_status(&paths);
        if !status.installed {
            println!("installing ffmpeg tools...");
            let next = tools::install_ffmpeg_tools(&paths)?;
            if !next.installed {
                return Err(EngineError::InstallFailed(
                    "FFmpeg install did not result in installed=true".to_string(),
                ));
            }
        } else {
            println!("ffmpeg tools already installed.");
        }
    }

    #[cfg(windows)]
    {
        let ytdlp = tools::ytdlp_tools_status(&paths);
        let yt_dlp_pin = &pinned_dependency_manifest::manifest().yt_dlp_windows;
        if !ytdlp.bundled_installed
            || ytdlp.ytdlp_version.as_deref() != Some(yt_dlp_pin.version.as_str())
        {
            println!("installing yt-dlp tools...");
            let _ = tools::install_ytdlp_tools(&paths)?;
        } else {
            println!("yt-dlp already installed (bundled).");
        }

        let js_runtime = tools::js_runtime_tools_status(&paths);
        if !js_runtime.bundled_deno_installed {
            println!("installing Deno JS runtime...");
            let next = tools::install_js_runtime_tools(&paths)?;
            if !next.bundled_deno_installed {
                return Err(EngineError::InstallFailed(
                    "Deno JS runtime install did not result in bundled_deno_installed=true"
                        .to_string(),
                ));
            }
        } else {
            println!("Deno JS runtime already installed (bundled).");
        }

        // A fresh prep process has no in-memory launch attestation. Verify the installed bytes
        // authoritatively before deciding whether the pinned offline payload can be reused.
        let po_provider = match tools::verify_youtube_po_provider_node_modules(&paths) {
            Ok(status) => status,
            Err(error) => {
                let status = tools::youtube_po_provider_install_status(&paths);
                println!(
                    "existing YouTube PO provider is {}: {error}",
                    status.node_modules_integrity_state
                );
                status
            }
        };
        if !po_provider.installed {
            println!("installing pinned localhost YouTube PO provider...");
            let next = tools::install_youtube_po_provider(&paths)?;
            if !next.installed || !next.security_audit_passed {
                return Err(EngineError::InstallFailed(
                    "YouTube PO provider install did not pass pinned readiness and security gates"
                        .to_string(),
                ));
            }
        } else {
            println!("YouTube PO provider already installed and verified.");
        }
        // Audit-only carrier for offline hydration. Runtime adoption authenticates the copied
        // complete trees against roots embedded in the executable and never trusts this JSON.
        tools::write_youtube_po_provider_portable_attestation(&paths)?;
    }

    // Phase 2: Portable Python + venv + packs.
    #[cfg(windows)]
    {
        let portable = tools::portable_python_status(&paths);
        if !portable.installed {
            println!("installing portable python...");
            let next = tools::install_portable_python(&paths)?;
            if !next.installed {
                return Err(EngineError::InstallFailed(
                    "portable python install did not result in installed=true".to_string(),
                ));
            }
        } else {
            println!("portable python already installed.");
        }
    }

    // Ensure venv exists (prefers portable python if present).
    {
        let py = tools::python_toolchain_status(&paths);
        if !py.venv_exists {
            println!("setting up python toolchain (venv)...");
            let next = tools::install_python_toolchain(&paths)?;
            if !next.venv_exists {
                return Err(EngineError::InstallFailed(
                    "python toolchain install did not result in venv_exists=true".to_string(),
                ));
            }
        } else {
            println!("python venv already present.");
        }
    }

    println!("installing packs...");
    {
        let status = tools::spleeter_pack_status(&paths);
        if !status.installed {
            let _ = tools::install_spleeter_pack(&paths)?;
        } else {
            println!("spleeter pack already installed.");
        }
    }
    {
        let status = tools::demucs_pack_status(&paths);
        if !status.installed {
            let _ = tools::install_demucs_pack(&paths)?;
        } else {
            println!("demucs pack already installed.");
        }
    }
    {
        let status = tools::diarization_pack_status(&paths);
        if !status.installed {
            let _ = tools::install_diarization_pack(&paths)?;
        } else {
            println!("diarization pack already installed.");
        }
    }
    {
        let status = tools::tts_preview_pack_status(&paths);
        if !status.installed {
            let _ = tools::install_tts_preview_pack(&paths)?;
        } else {
            println!("tts preview pack already installed.");
        }
    }
    {
        let status = tools::tts_neural_local_v1_pack_status(&paths);
        if !status.installed {
            let _ = tools::install_tts_neural_local_v1_pack(&paths)?;
        } else {
            println!("neural tts pack already installed.");
        }
    }
    {
        let status = tools::tts_voice_preserving_local_v1_pack_status(&paths);
        if !status.installed {
            let _ = tools::install_tts_voice_preserving_local_v1_pack(&paths)?;
        } else {
            println!("voice-preserving pack already installed.");
        }
    }
    {
        let status = tools::cosyvoice_pack_status(&paths);
        if !status.installed {
            let seed_src = Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")))
                .join("desktop")
                .join("src-tauri")
                .join("voice_backends_seed")
                .join("cosyvoice");
            if !seed_src
                .join("cosyvoice")
                .join("cli")
                .join("cosyvoice.py")
                .is_file()
            {
                return Err(EngineError::InstallFailed(format!(
                    "CosyVoice managed seed is missing from {}",
                    seed_src.to_string_lossy()
                )));
            }
            println!("seeding managed CosyVoice runtime code...");
            copy_tree(&seed_src, &paths.cosyvoice_backend_dir())?;
            println!("installing managed CosyVoice 2 pack...");
            let next = tools::install_voice_clone_cosyvoice_v1_pack(&paths)?;
            if !next.installed {
                return Err(EngineError::InstallFailed(format!(
                    "CosyVoice install did not result in installed=true: {}",
                    next.status_detail
                )));
            }
        } else {
            println!("CosyVoice 2 pack already installed.");
        }
    }

    {
        // Ship the KO/JA default ASR model (large-v3 q5_0) AND keep tiny as an offline
        // fallback the operator can revert to via the ASR model setting without a download.
        let store = ModelStore::new(paths.clone());
        let asr_models = [AppPaths::DEFAULT_ASR_MODEL_ID, "whispercpp-tiny"];
        for model_id in asr_models {
            println!("installing engine model {model_id}...");
            let inv = store.inventory()?;
            let installed = inv.models.iter().any(|m| m.id == model_id && m.installed);
            if !installed {
                store.install_model(model_id)?;
            } else {
                println!("{model_id} already installed.");
            }
        }
    }

    println!("pre-downloading demucs weights (best-effort)...");
    let _ = predownload_demucs_weights(&paths);

    println!("exporting offline payload...");
    export_offline_payload(&paths, &out_dir)?;

    println!("done.");
    Ok(())
}

fn predownload_demucs_weights(paths: &AppPaths) -> Result<()> {
    let venv_python = tools::python_venv_python_path(paths)?;

    let work_dir = paths.cache_dir().join("offline_prep");
    std::fs::create_dir_all(&work_dir)?;

    let wav_path = work_dir.join("tone_1s.wav");
    write_test_wav_44k_mono_16bit(&wav_path)?;

    let output_dir = work_dir.join("demucs_out");
    if output_dir.exists() {
        let _ = std::fs::remove_dir_all(&output_dir);
    }
    std::fs::create_dir_all(&output_dir)?;

    let torch_home = paths.python_models_dir().join("demucs");
    std::fs::create_dir_all(&torch_home)?;

    let mut command = cmd::command(&venv_python);
    command.args(["-m", "demucs_infer"]);
    command.args(["--two-stems", "vocals"]);
    command.arg("-o").arg(&output_dir);
    command.arg(&wav_path);
    command.env("PYTHONNOUSERSITE", "1");
    command.env(
        "XDG_CACHE_HOME",
        paths
            .cache_dir()
            .join("python")
            .to_string_lossy()
            .to_string(),
    );
    command.env("TORCH_HOME", torch_home.to_string_lossy().to_string());

    let output = command.output().map_err(|e| {
        EngineError::InstallFailed(format!("failed to run demucs predownload: {e}"))
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(EngineError::InstallFailed(format!(
            "demucs predownload failed (code={:?}): {}",
            output.status.code(),
            stderr.trim()
        )));
    }

    Ok(())
}

fn export_offline_payload(paths: &AppPaths, out_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(out_dir)?;

    let payload_path = out_dir.join("payload.zip");
    if payload_path.exists() {
        let _ = std::fs::remove_file(&payload_path);
    }
    if out_dir.join("manifest.json").exists() {
        let _ = std::fs::remove_file(out_dir.join("manifest.json"));
    }

    // Best-effort cleanup from the previous directory-based export format.
    for legacy in ["tools", "models", "cache"] {
        let path = out_dir.join(legacy);
        if path.exists() {
            let _ = std::fs::remove_dir_all(&path);
        }
    }

    let tools_src = paths.tools_dir();
    let models_src = paths.models_dir();
    let hf_cache_src = paths.cache_dir().join("huggingface");

    copy_tree(&tools_src, &out_dir.join("tools"))?;
    copy_tree(&models_src, &out_dir.join("models"))?;
    copy_tree(&hf_cache_src, &out_dir.join("cache").join("huggingface"))?;

    let payload_bytes = dir_size(&out_dir.join("tools"))?
        + dir_size(&out_dir.join("models"))?
        + dir_size(&out_dir.join("cache").join("huggingface"))?;

    let bundle_id = format!("offline_full_win64_{}", chrono_yyyymmdd_hhmmss());
    let manifest = serde_json::json!({
        "schema_version": 1,
        "bundle_id": bundle_id,
        "created_at_ms": now_ms(),
        "payload_format": "directory",
        "payload_bytes": payload_bytes,
    });
    std::fs::write(
        out_dir.join("manifest.json"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(&manifest).unwrap_or_else(|_| "{}".to_string())
        ),
    )?;

    Ok(())
}

fn copy_tree(src_root: &Path, dst_root: &Path) -> Result<()> {
    if !src_root.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(dst_root)?;

    let mut stack: Vec<PathBuf> = vec![src_root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = match entry {
                Ok(v) => v,
                Err(_) => continue,
            };
            let path = entry.path();
            let rel = match path.strip_prefix(src_root) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if should_skip_payload_entry(&rel) {
                continue;
            }
            let file_type = entry.file_type().map_err(|error| {
                EngineError::InstallFailed(format!(
                    "failed to inspect offline payload entry {}: {error}",
                    path.display()
                ))
            })?;
            let dst = dst_root.join(rel);
            if file_type.is_dir() {
                std::fs::create_dir_all(&dst)?;
                stack.push(path);
            } else if file_type.is_file() || file_type.is_symlink() {
                copy_payload_file(&path, &dst, file_type.is_symlink())?;
            }
        }
    }
    Ok(())
}

fn copy_payload_file(src: &Path, dst: &Path, is_symlink: bool) -> Result<()> {
    if is_symlink {
        let target_metadata = std::fs::metadata(src).map_err(|error| {
            EngineError::InstallFailed(format!(
                "offline payload contains an unreadable symbolic link {}: {error}",
                src.display()
            ))
        })?;
        if !target_metadata.is_file() {
            return Err(EngineError::InstallFailed(format!(
                "offline payload only supports symbolic links to files; {} resolves to a non-file",
                src.display()
            )));
        }
    }
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // std::fs::copy follows a file symlink and materializes the target bytes at the
    // destination. Hugging Face snapshots rely on these links; copying only
    // DirEntry::file_type().is_file() silently produced incomplete installers.
    std::fs::copy(src, dst)?;
    Ok(())
}

fn should_skip_payload_entry(relative_path: &Path) -> bool {
    relative_path.components().any(|component| {
        component == std::path::Component::Normal("_voxvulgi_stale_python_artifacts".as_ref())
            || component == std::path::Component::Normal("venv_cosyvoice".as_ref())
    })
}

fn dir_size(root: &Path) -> Result<u64> {
    if !root.exists() {
        return Ok(0);
    }
    let mut total = 0_u64;
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = match entry {
                Ok(v) => v,
                Err(_) => continue,
            };
            let path = entry.path();
            let file_type = match entry.file_type() {
                Ok(v) => v,
                Err(_) => continue,
            };
            if file_type.is_dir() {
                stack.push(path);
            } else if file_type.is_file() {
                total =
                    total.saturating_add(std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0));
            }
        }
    }
    Ok(total)
}

fn write_test_wav_44k_mono_16bit(path: &Path) -> Result<()> {
    // Minimal PCM WAV writer: 1s, 44.1kHz, mono, 16-bit.
    let sample_rate: u32 = 44_100;
    let channels: u16 = 1;
    let bits_per_sample: u16 = 16;
    let seconds: u32 = 1;
    let total_samples: u32 = sample_rate * seconds;

    let byte_rate: u32 = sample_rate * channels as u32 * (bits_per_sample as u32 / 8);
    let block_align: u16 = channels * (bits_per_sample / 8);

    let data_bytes: u32 = total_samples * channels as u32 * (bits_per_sample as u32 / 8);
    let riff_chunk_size: u32 = 36 + data_bytes;

    let mut out = Vec::<u8>::with_capacity((44 + data_bytes) as usize);

    // RIFF header
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&riff_chunk_size.to_le_bytes());
    out.extend_from_slice(b"WAVE");

    // fmt chunk
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // PCM fmt chunk size
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM format
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&bits_per_sample.to_le_bytes());

    // data chunk
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_bytes.to_le_bytes());

    // 440Hz sine wave at low amplitude.
    let freq_hz: f64 = 440.0;
    let amp: f64 = 0.08;
    for n in 0..total_samples {
        let t = (n as f64) / (sample_rate as f64);
        let v = (amp * (2.0 * std::f64::consts::PI * freq_hz * t).sin()).clamp(-1.0, 1.0);
        let sample = (v * (i16::MAX as f64)) as i16;
        out.extend_from_slice(&sample.to_le_bytes());
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, out)?;
    Ok(())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn chrono_yyyymmdd_hhmmss() -> String {
    // Avoid extra dependencies: format based on system time (UTC is fine for IDs).
    // Include time so each prep run gets a unique bundle id.
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = secs / 86_400;
    let seconds_of_day = secs % 86_400;
    // 1970-01-01 is day 0; use a simple civil-from-days conversion.
    let (y, m, d) = civil_from_days(days as i64);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;
    format!("{y:04}{m:02}{d:02}_{hour:02}{minute:02}{second:02}")
}

// Howard Hinnant's civil-from-days (public domain).
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = mp + if mp < 10 { 3 } else { -9 }; // [1, 12]
    let y = y + if m <= 2 { 1 } else { 0 };
    (y, m, d)
}

fn print_help() {
    println!(
        r#"voxvulgi_offline_bundle_prep

Prepares a full offline payload (Phase 1 + Phase 2) in a staging app-data directory
and exports it into the Tauri bundled resources folder.

Usage:
  cargo run --bin voxvulgi_offline_bundle_prep -- \
    --stage-base-dir "<repo>/tmp_offline_bundle_stage" \
    --out-dir "<repo>/product/desktop/src-tauri/offline" \
    [--force | --export-only]

Notes:
  - Downloads required tools/models during prep (build-time), but the exported payload is local-only.
  - --export-only re-exports an already prepared stage without installs, downloads, or warmups.
  - The desktop app bootstraps the payload into the real app-data dir on first run.
"#
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_payload_keeps_separate_cosyvoice_inputs_out_of_tools_export() {
        assert!(should_skip_payload_entry(Path::new(
            "python/venv_cosyvoice/Lib/site-packages/torch/__init__.py"
        )));
        assert!(!should_skip_payload_entry(Path::new(
            "python/venv/Lib/site-packages/torch/__init__.py"
        )));
    }

    #[test]
    fn copy_tree_copies_regular_files() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let destination = temp.path().join("destination");
        std::fs::create_dir_all(source.join("nested")).unwrap();
        std::fs::write(source.join("nested").join("weights.bin"), b"weights").unwrap();

        copy_tree(&source, &destination).unwrap();

        assert_eq!(
            std::fs::read(destination.join("nested").join("weights.bin")).unwrap(),
            b"weights"
        );
    }

    #[cfg(windows)]
    #[test]
    fn copy_tree_materializes_file_symlinks() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let destination = temp.path().join("destination");
        let blobs = source.join("blobs");
        let snapshot = source.join("snapshots").join("revision");
        std::fs::create_dir_all(&blobs).unwrap();
        std::fs::create_dir_all(&snapshot).unwrap();
        let target = blobs.join("model.bin");
        std::fs::write(&target, b"model-bytes").unwrap();
        let link = snapshot.join("model.bin");
        if let Err(error) = std::os::windows::fs::symlink_file(&target, &link) {
            if error.raw_os_error() == Some(1314) {
                eprintln!("skipped: Windows test process lacks symbolic-link privilege");
                return;
            }
            panic!("failed to create test symlink: {error}");
        }

        copy_tree(&source, &destination).unwrap();

        let copied = destination
            .join("snapshots")
            .join("revision")
            .join("model.bin");
        assert_eq!(std::fs::read(&copied).unwrap(), b"model-bytes");
        assert!(!std::fs::symlink_metadata(copied)
            .unwrap()
            .file_type()
            .is_symlink());
    }

    #[test]
    fn copy_payload_file_materializes_configured_real_symlink() {
        let Some(source) = std::env::var_os("VOXVULGI_TEST_FILE_SYMLINK") else {
            eprintln!("skipped: VOXVULGI_TEST_FILE_SYMLINK was not provided");
            return;
        };
        let source = PathBuf::from(source);
        assert!(
            std::fs::symlink_metadata(&source)
                .unwrap()
                .file_type()
                .is_symlink(),
            "configured source must be a symbolic link"
        );
        let expected = std::fs::read(&source).unwrap();
        let temp = tempfile::tempdir().unwrap();
        let copied = temp.path().join("materialized.bin");

        copy_payload_file(&source, &copied, true).unwrap();

        assert_eq!(std::fs::read(&copied).unwrap(), expected);
        assert!(!std::fs::symlink_metadata(copied)
            .unwrap()
            .file_type()
            .is_symlink());
    }
}
