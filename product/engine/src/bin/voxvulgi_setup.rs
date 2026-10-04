use std::path::{Path, PathBuf};

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
    let mut install_cosyvoice = false;
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
            "--install-cosyvoice" => install_cosyvoice = true,
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

    if install_cosyvoice {
        validate_cosyvoice_setup_request(
            base_dir.as_deref(),
            install_all || install_ffmpeg || !install_models.is_empty() || force,
        )?;
        let mutable_base = resolve_mutable_cosyvoice_base(
            &base_dir.expect("validated explicit CosyVoice base"),
        )?;
        let paths = AppPaths::new(mutable_base);
        validate_cosyvoice_mutation_roots(&paths)?;
        let seed = paths.cosyvoice_backend_dir().join("cosyvoice/cli/cosyvoice.py");
        if !seed.is_file() {
            return Err(format!("CosyVoice requires its existing managed backend seed: {}", seed.display()));
        }
        println!("CosyVoice base dir: {}", paths.base_dir.display());
        let next = tools::install_voice_clone_cosyvoice_v1_pack(&paths)
            .map_err(|e| e.to_string())?;
        if !next.installed {
            return Err(format!("CosyVoice install did not result in installed=true: {}", next.status_detail));
        }
        println!("CosyVoice: installed ({})", next.status_detail);
        return Ok(());
    }

    if install_all {
        install_ffmpeg = true;
        // KO/JA default ASR (large-v3 q5_0) + tiny fallback.
        for model_id in ["whispercpp-large-v3-q5_0", "whispercpp-tiny"] {
            if !install_models.iter().any(|m| m == model_id) {
                install_models.push(model_id.to_string());
            }
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

fn validate_cosyvoice_setup_request(base: Option<&Path>, other_actions: bool) -> Result<(), String> {
    let base = base.ok_or_else(|| "--install-cosyvoice requires explicit --base-dir".to_string())?;
    if !base.is_absolute() {
        return Err("--install-cosyvoice requires an absolute --base-dir".to_string());
    }
    if other_actions {
        return Err("--install-cosyvoice must run alone, without other install flags or --force".to_string());
    }
    Ok(())
}

fn resolve_mutable_cosyvoice_base(base: &Path) -> Result<PathBuf, String> {
    // Inspect the supplied chain before canonicalization can hide a linked ancestor.
    for ancestor in base.ancestors() {
        let metadata = std::fs::symlink_metadata(ancestor).map_err(|e| {
            format!("CosyVoice base ancestor is unavailable: {}: {e}", ancestor.display())
        })?;
        #[cfg(windows)]
        let reparse = {
            use std::os::windows::fs::MetadataExt;
            metadata.file_attributes() & 0x400 != 0
        };
        #[cfg(not(windows))]
        let reparse = false;
        if metadata.file_type().is_symlink() || reparse || !metadata.is_dir() {
            return Err(format!("CosyVoice base ancestor must be an ordinary directory: {}", ancestor.display()));
        }
    }
    let physical = dunce::canonicalize(base).map_err(|e| e.to_string())?;
    for ancestor in physical.ancestors() {
        for marker in ["runtime_manifest.json", "immutable_cache.json"] {
            match std::fs::symlink_metadata(ancestor.join(marker)) {
                Ok(_) => return Err(format!("CosyVoice base is inside a protected runtime/prepared input: {}", ancestor.display())),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {},
                Err(e) => return Err(format!("Cannot inspect CosyVoice protection marker: {e}")),
            }
        }
    }
    Ok(physical)
}

fn validate_cosyvoice_mutation_roots(paths: &AppPaths) -> Result<(), String> {
    let backend = paths.cosyvoice_backend_dir();
    for target in [
        paths.python_toolchain_dir(),
        paths.python_models_dir(),
        paths.python_cosyvoice_venv_dir(),
        paths.python_install_state_dir(),
        paths.voice_backends_dir(),
        backend.clone(),
        paths.cosyvoice_model_parent_dir(),
        paths.cosyvoice_model_parent_dir().join("CosyVoice2-0.5B"),
        backend.join("wetext"),
        backend.join("cosyvoice/cli"),
    ] {
        // New target directories are checked through their nearest existing ancestor.
        let mut existing = target.as_path();
        loop {
            match std::fs::symlink_metadata(existing) {
                Ok(_) => break,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    existing = existing.parent().ok_or_else(|| "CosyVoice target has no existing ancestor".to_string())?;
                },
                Err(e) => return Err(format!("Cannot inspect CosyVoice mutation root: {e}")),
            }
        }
        let physical = resolve_mutable_cosyvoice_base(existing)?;
        if !physical.starts_with(&paths.base_dir) {
            return Err("CosyVoice mutation root escapes its explicit physical base".to_string());
        }
    }
    Ok(())
}

fn default_base_dir() -> Option<PathBuf> {
    let env_dir = std::env::var("VOXVULGI_BASE_DIR").or_else(|_| std::env::var("YTFETCH_BASE_DIR"));
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

Bootstraps FFmpeg/local models, or installs managed CosyVoice into an explicit mutable base.

Usage:
  cargo run --bin voxvulgi_setup -- --install-all
  cargo run --bin voxvulgi_setup -- --install-ffmpeg
  cargo run --bin voxvulgi_setup -- --install-model whispercpp-tiny
  cargo run --bin voxvulgi_setup -- --base-dir <absolute-mutable-base> --install-cosyvoice

Options:
  --base-dir <path>     Override base dir (default: %APPDATA%\com.voxvulgi.voxvulgi on Windows)
  --install-all         Install FFmpeg + whispercpp-large-v3-q5_0 (KO/JA default) + whispercpp-tiny (fallback)
  --install-ffmpeg      Install FFmpeg tools into <base-dir>\tools\ffmpeg
  --install-model <id>  Install a model from the manifest
  --install-cosyvoice   Install only managed CosyVoice; requires explicit absolute base and existing backend seed; refuses protected or linked bases; cannot combine with other install flags or --force
  --force               Reinstall even if present
"#
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosyvoice_setup_requires_explicit_absolute_base_before_mutation() {
        assert!(validate_cosyvoice_setup_request(None, false).unwrap_err().contains("explicit"));
        assert!(validate_cosyvoice_setup_request(Some(Path::new("relative-stage")), false).unwrap_err().contains("absolute"));
        let base = std::env::temp_dir().join("owned-cosyvoice-stage");
        assert!(validate_cosyvoice_setup_request(Some(&base), false).is_ok());
    }

    #[test]
    fn cosyvoice_setup_refuses_combined_or_force_actions() {
        let base = std::env::temp_dir().join("owned-cosyvoice-stage");
        assert!(validate_cosyvoice_setup_request(Some(&base), true).unwrap_err().contains("must run alone"));
    }
    #[test]
    fn cosyvoice_setup_refuses_immutable_marker_ancestor_before_install() {
        for marker in ["runtime_manifest.json", "immutable_cache.json"] {
            let fixture = tempfile::tempdir().unwrap();
            let child = fixture.path().join("child");
            std::fs::create_dir(&child).unwrap();
            assert!(resolve_mutable_cosyvoice_base(&child).is_ok());
            std::fs::write(fixture.path().join(marker), b"{}").unwrap();
            assert!(resolve_mutable_cosyvoice_base(&child).unwrap_err().contains("protected"));
        }
    }

    #[test]
    fn cosyvoice_setup_refuses_linked_base_before_install() {
        let fixture = tempfile::tempdir().unwrap();
        let target = fixture.path().join("target");
        let link = fixture.path().join("linked");
        std::fs::create_dir(&target).unwrap();
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let status = std::process::Command::new("cmd.exe")
                .creation_flags(0x08000000)
                .args(["/d", "/c", "mklink", "/J"])
                .arg(&link).arg(&target).status().unwrap();
            assert!(status.success(), "own fixture junction creation failed");
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(resolve_mutable_cosyvoice_base(&link).unwrap_err().contains("ordinary directory"));
        std::fs::create_dir(target.join("child")).unwrap();
        assert!(resolve_mutable_cosyvoice_base(&link.join("child")).unwrap_err().contains("ordinary directory"));
    }

    #[test]
    fn cosyvoice_setup_refuses_linked_child_mutation_root_before_install() {
        let fixture = tempfile::tempdir().unwrap();
        let target = fixture.path().join("outside");
        let base = fixture.path().join("base");
        std::fs::create_dir(&target).unwrap();
        std::fs::create_dir_all(base.join("tools")).unwrap();
        let paths = AppPaths::new(dunce::canonicalize(&base).unwrap());
        assert!(validate_cosyvoice_mutation_roots(&paths).is_ok());
        let link = paths.python_toolchain_dir();
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let status = std::process::Command::new("cmd.exe")
                .creation_flags(0x08000000)
                .args(["/d", "/c", "mklink", "/J"])
                .arg(&link).arg(&target).status().unwrap();
            assert!(status.success(), "own fixture junction creation failed");
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(validate_cosyvoice_mutation_roots(&paths).unwrap_err().contains("ordinary directory"));
    }

}
