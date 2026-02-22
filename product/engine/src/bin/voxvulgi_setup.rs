use std::path::PathBuf;

use voxvulgi_engine::models::ModelStore;
use voxvulgi_engine::paths::AppPaths;
use voxvulgi_engine::{db, tools};

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_help();
        return Ok(());
    }

    let mut base_dir: Option<PathBuf> = None;
    let mut install_all = false;
    let mut install_ffmpeg = false;
    let mut install_models: Vec<String> = Vec::new();
    let mut force = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--base-dir" => {
                i += 1;
                let v = args
                    .get(i)
                    .ok_or_else(|| "--base-dir requires a value".to_string())?;
                base_dir = Some(PathBuf::from(v));
            }
            "--install-all" => install_all = true,
            "--install-ffmpeg" => install_ffmpeg = true,
            "--install-model" => {
                i += 1;
                let v = args
                    .get(i)
                    .ok_or_else(|| "--install-model requires a value".to_string())?;
                install_models.push(v.to_string());
            }
            "--force" => force = true,
            other => return Err(format!("unknown arg: {other} (try --help)")),
        }
        i += 1;
    }

    if install_all {
        install_ffmpeg = true;
        if !install_models.iter().any(|m| m == "whispercpp-tiny") {
            install_models.push("whispercpp-tiny".to_string());
        }
    }

    if !install_ffmpeg && install_models.is_empty() {
        return Err("nothing to do (pass --install-all or flags)".to_string());
    }

    let base_dir = base_dir
        .or_else(default_base_dir)
        .ok_or_else(|| "could not determine base dir; pass --base-dir".to_string())?;

    let paths = AppPaths::new(base_dir);
    paths.ensure_dirs().map_err(|e| e.to_string())?;
    db::ensure_schema(&paths).map_err(|e| e.to_string())?;

    // Ensure glossary exists for translation WPs.
    let glossary = paths.glossary_path();
    if !glossary.exists() {
        if let Some(parent) = glossary.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(&glossary, "{\n}\n").map_err(|e| e.to_string())?;
    }

    println!("Base dir: {}", paths.base_dir.to_string_lossy());
    println!("Glossary: {}", glossary.to_string_lossy());

    if install_ffmpeg {
        let status = tools::ffmpeg_tools_status(&paths);
        if status.installed && !force {
            println!("FFmpeg: already installed ({})", status.ffmpeg_path);
        } else {
            println!("FFmpeg: installing...");
            let next = tools::install_ffmpeg_tools(&paths).map_err(|e| e.to_string())?;
            if !next.installed {
                return Err("FFmpeg install did not result in installed=true".to_string());
            }
            println!("FFmpeg: installed ({})", next.ffmpeg_path);
        }
    }

    if !install_models.is_empty() {
        let store = ModelStore::new(paths.clone());
        for model_id in install_models {
            let inventory = store.inventory().map_err(|e| e.to_string())?;
            let item = inventory.models.iter().find(|m| m.id == model_id);
            let installed = item.map(|m| m.installed).unwrap_or(false);
            if installed && !force {
                println!("Model {model_id}: already installed");
                continue;
            }
            println!("Model {model_id}: installing...");
            store.install_model(&model_id).map_err(|e| e.to_string())?;
            println!("Model {model_id}: installed");
        }
    }

    Ok(())
}

fn default_base_dir() -> Option<PathBuf> {
    let env_dir =
        std::env::var("VOXVULGI_BASE_DIR").or_else(|_| std::env::var("YTFETCH_BASE_DIR"));
    if let Ok(v) = env_dir {
        let t = v.trim();
        if !t.is_empty() {
            return Some(PathBuf::from(t));
        }
    }

    // Match Tauri's app_data_dir() behavior (Roaming on Windows).
    if cfg!(windows) {
        if let Ok(appdata) = std::env::var("APPDATA") {
            let t = appdata.trim();
            if !t.is_empty() {
                return Some(PathBuf::from(t).join("com.voxvulgi.voxvulgi"));
            }
        }
    }

    None
}

fn print_help() {
    println!(
        r#"voxvulgi_setup

Bootstraps runtime dependencies (FFmpeg tools + local models) into the app data directory.

Usage:
  cargo run --bin voxvulgi_setup -- --install-all
  cargo run --bin voxvulgi_setup -- --install-ffmpeg
  cargo run --bin voxvulgi_setup -- --install-model whispercpp-tiny

Options:
  --base-dir <path>     Override base dir (default: %APPDATA%\com.voxvulgi.voxvulgi on Windows)
  --install-all         Install FFmpeg + whispercpp-tiny
  --install-ffmpeg      Install FFmpeg tools into <base-dir>\tools\ffmpeg
  --install-model <id>  Install a model from the manifest
  --force               Reinstall even if present
"#
    );
}
