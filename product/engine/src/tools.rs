use crate::paths::AppPaths;
use crate::{EngineError, Result};
use serde::{Deserialize, Serialize};
use std::io::Write;

#[derive(Debug, Clone, Serialize)]
pub struct FfmpegToolsStatus {
    pub installed: bool,
    pub ffmpeg_path: String,
    pub ffprobe_path: String,
    pub ffmpeg_version: Option<String>,
    pub ffprobe_version: Option<String>,
}

pub fn ffmpeg_tools_status(paths: &AppPaths) -> FfmpegToolsStatus {
    let ffmpeg_path = paths.ffmpeg_bin_path();
    let ffprobe_path = paths.ffprobe_bin_path();
    let installed = ffmpeg_path.exists() && ffprobe_path.exists();
    let ffmpeg_version = tool_version_first_line(paths.ffmpeg_cmd());
    let ffprobe_version = tool_version_first_line(paths.ffprobe_cmd());

    FfmpegToolsStatus {
        installed,
        ffmpeg_path: ffmpeg_path.to_string_lossy().to_string(),
        ffprobe_path: ffprobe_path.to_string_lossy().to_string(),
        ffmpeg_version,
        ffprobe_version,
    }
}

pub fn install_ffmpeg_tools(paths: &AppPaths) -> Result<FfmpegToolsStatus> {
    paths.ensure_dirs()?;

    let destination = paths.ffmpeg_dir();
    std::fs::create_dir_all(&destination)?;

    let download_url = ffmpeg_sidecar::download::ffmpeg_download_url()
        .map_err(|e| EngineError::InstallFailed(e.to_string()))?;
    let archive_path =
        ffmpeg_sidecar::download::download_ffmpeg_package(download_url, &destination)
            .map_err(|e| EngineError::InstallFailed(e.to_string()))?;
    ffmpeg_sidecar::download::unpack_ffmpeg(&archive_path, &destination)
        .map_err(|e| EngineError::InstallFailed(e.to_string()))?;

    Ok(ffmpeg_tools_status(paths))
}

fn tool_version_first_line(program: impl AsRef<std::ffi::OsStr>) -> Option<String> {
    let output = crate::cmd::command(program).arg("-version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let first = text.lines().next()?.trim();
    if first.is_empty() {
        return None;
    }
    Some(first.to_string())
}

#[derive(Debug, Clone, Serialize)]
pub struct YtDlpToolsStatus {
    pub available: bool,
    pub bundled_installed: bool,
    pub bundled_path: String,
    pub ytdlp_path: String,
    pub ytdlp_version: Option<String>,
}

pub fn ytdlp_tools_status(paths: &AppPaths) -> YtDlpToolsStatus {
    let bundled = bundled_ytdlp_path(paths);
    let bundled_installed = bundled.exists();

    let mut resolved_path = String::new();
    let mut resolved_version: Option<String> = None;
    let mut available = false;

    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if bundled_installed {
        candidates.push(bundled.clone());
    }
    candidates.push(std::path::PathBuf::from("yt-dlp"));

    for candidate in candidates {
        let version = tool_version_first_line_with_arg(&candidate, "--version");
        if version.is_some() {
            available = true;
            resolved_path = candidate.to_string_lossy().to_string();
            resolved_version = version;
            break;
        }
    }

    YtDlpToolsStatus {
        available,
        bundled_installed,
        bundled_path: bundled.to_string_lossy().to_string(),
        ytdlp_path: resolved_path,
        ytdlp_version: resolved_version,
    }
}

pub fn install_ytdlp_tools(paths: &AppPaths) -> Result<YtDlpToolsStatus> {
    paths.ensure_dirs()?;

    #[cfg(not(windows))]
    {
        let _ = paths;
        return Err(EngineError::InstallFailed(
            "automatic yt-dlp install is only supported on Windows for now".to_string(),
        ));
    }

    #[cfg(windows)]
    {
        const YT_DLP_WINDOWS_DOWNLOAD_URL: &str =
            "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe";

        let destination = bundled_ytdlp_path(paths);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let tmp_path = destination.with_extension("download");

        let resp = ureq::get(YT_DLP_WINDOWS_DOWNLOAD_URL).call().map_err(|e| {
            EngineError::InstallFailed(format!("yt-dlp download failed: {e}"))
        })?;
        let status = resp.status();
        if status.as_u16() >= 400 {
            return Err(EngineError::InstallFailed(format!(
                "yt-dlp download failed (status={status})"
            )));
        }

        {
            let mut reader = resp.into_body().into_reader();
            let mut file = std::fs::File::create(&tmp_path)?;
            std::io::copy(&mut reader, &mut file)?;
            file.flush()?;
        }

        let min_size = 512 * 1024_u64;
        let downloaded_size = std::fs::metadata(&tmp_path).map(|m| m.len()).unwrap_or(0);
        if downloaded_size < min_size {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(EngineError::InstallFailed(
                "downloaded yt-dlp is unexpectedly small".to_string(),
            ));
        }

        if destination.exists() {
            let _ = std::fs::remove_file(&destination);
        }
        if std::fs::rename(&tmp_path, &destination).is_err() {
            std::fs::copy(&tmp_path, &destination)?;
            let _ = std::fs::remove_file(&tmp_path);
        }

        Ok(ytdlp_tools_status(paths))
    }
}

fn bundled_ytdlp_path(paths: &AppPaths) -> std::path::PathBuf {
    let mut path = paths.tools_dir().join("yt-dlp").join("yt-dlp");
    if cfg!(windows) {
        path.set_extension("exe");
    }
    path
}

fn tool_version_first_line_with_arg(
    program: impl AsRef<std::ffi::OsStr>,
    arg: &str,
) -> Option<String> {
    let output = crate::cmd::command(program).arg(arg).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let first = text.lines().next()?.trim();
    if first.is_empty() {
        return None;
    }
    Some(first.to_string())
}

#[derive(Debug, Clone, Serialize)]
pub struct PythonToolchainStatus {
    pub base_available: bool,
    pub base_program: String,
    pub base_args: Vec<String>,
    pub base_version: Option<String>,

    pub venv_dir: String,
    pub venv_exists: bool,
    pub venv_python_path: String,
    pub venv_python_version: Option<String>,
    pub venv_pip_version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PortablePythonStatus {
    pub installed: bool,
    pub python_path: String,
    pub python_version: Option<String>,
    pub install_dir: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Phase2PackPlanItem {
    pub id: String,
    pub title: String,
    pub supported: bool,
    pub estimated_bytes: Option<u64>,
}

pub fn python_toolchain_status(paths: &AppPaths) -> PythonToolchainStatus {
    let resolved = resolve_base_python(paths);
    let base_available = resolved.is_some();
    let (base_program, base_args, base_version) = match &resolved {
        Some(r) => (
            r.program.to_string_lossy().to_string(),
            r.args.clone(),
            Some(r.version.clone()),
        ),
        None => (String::new(), Vec::new(), None),
    };

    let venv_dir = paths.python_venv_dir();
    let venv_exists = venv_dir.exists() && venv_dir.is_dir();
    let venv_python = venv_python_path(&venv_dir);
    let venv_python_version = python_version(&venv_python, &[]);
    let venv_pip_version = venv_python_version
        .as_ref()
        .and_then(|_| pip_version(&venv_python));

    PythonToolchainStatus {
        base_available,
        base_program,
        base_args,
        base_version,
        venv_dir: venv_dir.to_string_lossy().to_string(),
        venv_exists,
        venv_python_path: venv_python.to_string_lossy().to_string(),
        venv_python_version,
        venv_pip_version,
    }
}

pub fn phase2_packs_install_plan() -> Vec<Phase2PackPlanItem> {
    vec![
        Phase2PackPlanItem {
            id: "portable_python_win64".to_string(),
            title: "Portable Python (Windows x64)".to_string(),
            supported: cfg!(windows),
            estimated_bytes: None,
        },
        Phase2PackPlanItem {
            id: "python_toolchain".to_string(),
            title: "Python toolchain (venv)".to_string(),
            supported: true,
            estimated_bytes: None,
        },
        Phase2PackPlanItem {
            id: "spleeter".to_string(),
            title: "Spleeter separation (baseline)".to_string(),
            supported: true,
            estimated_bytes: None,
        },
        Phase2PackPlanItem {
            id: "diarization".to_string(),
            title: "Diarization (baseline)".to_string(),
            supported: true,
            estimated_bytes: None,
        },
        Phase2PackPlanItem {
            id: "tts_preview".to_string(),
            title: "TTS preview (system voices)".to_string(),
            supported: true,
            estimated_bytes: None,
        },
        Phase2PackPlanItem {
            id: "tts_neural_local_v1".to_string(),
            title: "Neural TTS local (Kokoro)".to_string(),
            supported: true,
            estimated_bytes: None,
        },
        Phase2PackPlanItem {
            id: "tts_voice_preserving_local_v1".to_string(),
            title: "Voice-preserving dub (OpenVoice V2)".to_string(),
            supported: true,
            estimated_bytes: None,
        },
    ]
}

#[derive(Debug, Clone, Serialize)]
pub struct PackIntegrityManifestStatus {
    pub exists: bool,
    pub manifest_path: String,
    pub generated_at_ms: Option<i64>,
}

pub fn pack_integrity_manifest_status(paths: &AppPaths) -> PackIntegrityManifestStatus {
    let path = pack_integrity_manifest_path(paths);
    let mut generated_at_ms: Option<i64> = None;
    if let Ok(bytes) = std::fs::read(&path) {
        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) {
            generated_at_ms = v
                .get("generated_at_ms")
                .and_then(|n| n.as_i64())
                .filter(|ms| *ms > 0);
        }
    }
    PackIntegrityManifestStatus {
        exists: path.exists(),
        manifest_path: path.to_string_lossy().to_string(),
        generated_at_ms,
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PackIntegrityManifestResult {
    pub out_path: String,
    pub file_bytes: u64,
    pub generated_at_ms: i64,
}

pub fn generate_pack_integrity_manifest(paths: &AppPaths) -> Result<PackIntegrityManifestResult> {
    paths.ensure_dirs()?;

    #[derive(Serialize)]
    struct PackIntegrityPacks {
        spleeter: SpleeterPackStatus,
        demucs: DemucsPackStatus,
        diarization: DiarizationPackStatus,
        tts_preview: TtsPreviewPackStatus,
        tts_neural_local_v1: TtsNeuralLocalV1PackStatus,
        tts_voice_preserving_local_v1: TtsVoicePreservingLocalV1PackStatus,
    }

    #[derive(Serialize)]
    struct PackIntegrityModelManifests {
        spleeter_2stems: Option<serde_json::Value>,
        openvoice_v2: Option<serde_json::Value>,
    }

    #[derive(Serialize)]
    struct PackIntegrityManifest {
        schema_version: u32,
        generated_at_ms: i64,
        portable_python: PortablePythonStatus,
        python_toolchain: PythonToolchainStatus,
        packs: PackIntegrityPacks,
        model_manifests: PackIntegrityModelManifests,
    }

    let generated_at_ms = now_ms();
    let out_path = pack_integrity_manifest_path(paths);
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let spleeter_manifest_path = paths
        .python_models_dir()
        .join("spleeter")
        .join("2stems")
        .join("voxvulgi_spleeter_manifest.json");
    let openvoice_manifest_path = paths
        .python_models_dir()
        .join("openvoice_v2")
        .join("voxvulgi_openvoicev2_manifest.json");

    let manifest = PackIntegrityManifest {
        schema_version: 1,
        generated_at_ms,
        portable_python: portable_python_status(paths),
        python_toolchain: python_toolchain_status(paths),
        packs: PackIntegrityPacks {
            spleeter: spleeter_pack_status(paths),
            demucs: demucs_pack_status(paths),
            diarization: diarization_pack_status(paths),
            tts_preview: tts_preview_pack_status(paths),
            tts_neural_local_v1: tts_neural_local_v1_pack_status(paths),
            tts_voice_preserving_local_v1: tts_voice_preserving_local_v1_pack_status(paths),
        },
        model_manifests: PackIntegrityModelManifests {
            spleeter_2stems: read_json_value_best_effort(&spleeter_manifest_path),
            openvoice_v2: read_json_value_best_effort(&openvoice_manifest_path),
        },
    };

    let json = serde_json::to_string_pretty(&manifest)?;
    std::fs::write(&out_path, format!("{json}\n"))?;
    let file_bytes = std::fs::metadata(&out_path).map(|m| m.len()).unwrap_or(0);

    Ok(PackIntegrityManifestResult {
        out_path: out_path.to_string_lossy().to_string(),
        file_bytes,
        generated_at_ms,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct PerformanceTierStatus {
    pub tier: String,
    pub gpu_names: Vec<String>,
    pub torch_cuda_available: Option<bool>,
    pub recommended_separation_backend: String,
    pub recommended_diarization_backend: String,
    pub recommended_tts_vc_device: String,
}

pub fn performance_tier_status(paths: &AppPaths) -> PerformanceTierStatus {
    let gpu_names = detect_gpu_names_best_effort();
    let torch_cuda_available = detect_torch_cuda_best_effort(paths);

    let tier = if torch_cuda_available.unwrap_or(false) || !gpu_names.is_empty() {
        "gpu".to_string()
    } else {
        "cpu".to_string()
    };

    // Defaults remain CPU-safe and deterministic.
    let recommended_separation_backend = if tier == "gpu" {
        "spleeter (baseline)".to_string()
    } else {
        "spleeter (baseline)".to_string()
    };

    let recommended_diarization_backend = "baseline".to_string();
    let recommended_tts_vc_device = if torch_cuda_available.unwrap_or(false) {
        "cuda (if supported by pack)".to_string()
    } else {
        "cpu".to_string()
    };

    PerformanceTierStatus {
        tier,
        gpu_names,
        torch_cuda_available,
        recommended_separation_backend,
        recommended_diarization_backend,
        recommended_tts_vc_device,
    }
}

fn detect_gpu_names_best_effort() -> Vec<String> {
    // Best-effort, cross-platform-ish detection.
    let mut out: Vec<String> = Vec::new();

    if let Ok(output) = crate::cmd::command("nvidia-smi")
        .args(["--query-gpu=name", "--format=csv,noheader"])
        .output()
    {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                let name = line.trim();
                if !name.is_empty() {
                    out.push(name.to_string());
                }
            }
        }
    }

    out
}

fn detect_torch_cuda_best_effort(paths: &AppPaths) -> Option<bool> {
    let venv_python = venv_python_path(&paths.python_venv_dir());
    if !venv_python.exists() {
        return None;
    }
    let output = crate::cmd::command(&venv_python)
        .args([
            "-c",
            "import json\ntry:\n import torch\n print(json.dumps({'cuda': bool(torch.cuda.is_available())}))\nexcept Exception as e:\n print(json.dumps({'error': str(e)}))\n",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let last = text.lines().rev().find(|l| !l.trim().is_empty())?.trim();
    let v: serde_json::Value = serde_json::from_str(last).ok()?;
    v.get("cuda").and_then(|b| b.as_bool())
}

pub fn portable_python_status(paths: &AppPaths) -> PortablePythonStatus {
    let exe = paths.python_portable_python_exe();
    let version = python_version(&exe, &[]);
    PortablePythonStatus {
        installed: version.is_some() && exe.exists(),
        python_path: exe.to_string_lossy().to_string(),
        python_version: version,
        install_dir: paths.python_portable_dir().to_string_lossy().to_string(),
    }
}

pub fn install_portable_python(paths: &AppPaths) -> Result<PortablePythonStatus> {
    #[cfg(not(windows))]
    {
        let _ = paths;
        return Err(EngineError::InstallFailed(
            "portable Python install is only supported on Windows for now".to_string(),
        ));
    }

    #[cfg(windows)]
    {
        const PYTHON_NUGET_VERSION: &str = "3.11.9";
        const PYTHON_NUGET_SHA256_HEX: &str =
            "9283876D58C017E0E846F95B490DA3BCA0FC0A6EE1134B2870677CFB7EEC3C67";
        const PYTHON_NUGET_URL: &str = "https://www.nuget.org/api/v2/package/python/3.11.9";

        paths.ensure_dirs()?;
        let install_dir = paths.python_portable_dir();
        std::fs::create_dir_all(&install_dir)?;

        let marker = install_dir.join(".probe");
        if marker.exists() {
            let status = portable_python_status(paths);
            if status.installed {
                return Ok(status);
            }
        }

        // Clean up any partial install.
        if install_dir.exists() {
            let _ = std::fs::remove_dir_all(&install_dir);
        }
        std::fs::create_dir_all(&install_dir)?;

        let download_tmp = install_dir.join(format!("python-nuget-{PYTHON_NUGET_VERSION}.nupkg.download"));
        let download_final = install_dir.join(format!("python-nuget-{PYTHON_NUGET_VERSION}.nupkg"));

        let resp = ureq::get(PYTHON_NUGET_URL).call().map_err(|e| {
            EngineError::InstallFailed(format!("portable Python download failed: {e}"))
        })?;
        let status = resp.status();
        if status.as_u16() >= 400 {
            return Err(EngineError::InstallFailed(format!(
                "portable Python download failed (status={status})"
            )));
        }

        {
            let mut reader = resp.into_body().into_reader();
            let mut file = std::fs::File::create(&download_tmp)?;
            std::io::copy(&mut reader, &mut file)?;
            file.flush()?;
        }

        let expected = hex::decode(PYTHON_NUGET_SHA256_HEX).map_err(|e| {
            EngineError::InstallFailed(format!("invalid embedded portable Python sha256: {e}"))
        })?;
        let got = sha256_file(&download_tmp)?;
        if got != expected {
            let _ = std::fs::remove_file(&download_tmp);
            return Err(EngineError::InstallFailed(
                "portable Python download hash mismatch".to_string(),
            ));
        }

        if download_final.exists() {
            let _ = std::fs::remove_file(&download_final);
        }
        if std::fs::rename(&download_tmp, &download_final).is_err() {
            std::fs::copy(&download_tmp, &download_final)?;
            let _ = std::fs::remove_file(&download_tmp);
        }

        extract_zip_strip_prefix(&download_final, &install_dir, "tools/")?;

        let exe = paths.python_portable_python_exe();
        let version = python_version(&exe, &[]).ok_or_else(|| {
            EngineError::InstallFailed("portable Python is not usable after install".to_string())
        })?;
        std::fs::write(
            &marker,
            format!(
                "OK\nversion={}\nsource=nuget:python:{}\nsha256={}\n",
                version.trim(),
                PYTHON_NUGET_VERSION,
                PYTHON_NUGET_SHA256_HEX
            ),
        )?;

        let _ = generate_pack_integrity_manifest(paths);
        Ok(portable_python_status(paths))
    }
}

pub fn install_python_toolchain(paths: &AppPaths) -> Result<PythonToolchainStatus> {
    paths.ensure_dirs()?;

    let resolved = resolve_base_python(paths).ok_or_else(|| {
        EngineError::InstallFailed(
            "Python was not found. Install Python 3 and ensure it is on PATH, install the optional portable Python in Diagnostics, or set a Python override in app config (config/python_exe.txt)."
                .to_string(),
        )
    })?;

    let venv_dir = paths.python_venv_dir();
    if !venv_dir.exists() {
        if let Some(parent) = venv_dir.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut cmd = crate::cmd::command(&resolved.program);
        for arg in &resolved.args {
            cmd.arg(arg);
        }
        let output = cmd
            .args(["-m", "venv"])
            .arg(&venv_dir)
            .output()
            .map_err(|e| EngineError::InstallFailed(format!("failed to create venv: {e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(EngineError::InstallFailed(format!(
                "python venv creation failed (code={:?}): {}",
                output.status.code(),
                stderr.trim()
            )));
        }
    }

    let venv_python = venv_python_path(&venv_dir);
    let venv_version = python_version(&venv_python, &[]).ok_or_else(|| {
        EngineError::InstallFailed("venv python is not available after install".to_string())
    })?;
    let pip = pip_version(&venv_python).ok_or_else(|| {
        EngineError::InstallFailed("venv pip is not available after install".to_string())
    })?;

    let _ = (venv_version, pip);
    let _ = generate_pack_integrity_manifest(paths);
    Ok(python_toolchain_status(paths))
}

pub fn python_venv_python_path(paths: &AppPaths) -> Result<std::path::PathBuf> {
    let venv_python = venv_python_path(&paths.python_venv_dir());
    if !venv_python.exists() {
        return Err(EngineError::ExternalToolMissing {
            tool: "python (venv)".to_string(),
        });
    }
    Ok(venv_python)
}

#[derive(Debug, Clone)]
struct ResolvedPython {
    program: std::path::PathBuf,
    args: Vec<String>,
    version: String,
}

fn resolve_base_python(paths: &AppPaths) -> Option<ResolvedPython> {
    if let Ok(Some(override_path)) = paths.python_exe_override() {
        if let Some(version) = python_version(&override_path, &[]) {
            return Some(ResolvedPython {
                program: override_path,
                args: Vec::new(),
                version,
            });
        }
    }

    let portable = paths.python_portable_python_exe();
    if let Some(version) = python_version(&portable, &[]) {
        return Some(ResolvedPython {
            program: portable,
            args: Vec::new(),
            version,
        });
    }

    let mut candidates: Vec<(std::path::PathBuf, Vec<String>)> = Vec::new();
    if cfg!(windows) {
        // Prefer explicit Python 3.11 on Windows first; it is the most compatible
        // version for the Phase 2 native Python packs we run in-app today.
        let preferred_versions = ["3.11", "3.10", "3.9", "3.8"];
        for version in preferred_versions {
            candidates.push((std::path::PathBuf::from("py"), vec![format!("-{version}")]));
        }
        candidates.push((std::path::PathBuf::from("python"), Vec::new()));
        candidates.push((std::path::PathBuf::from("py"), vec!["-3".to_string()]));
        candidates.push((std::path::PathBuf::from("python3"), Vec::new()));
    } else {
        candidates.push((std::path::PathBuf::from("python3"), Vec::new()));
        candidates.push((std::path::PathBuf::from("python"), Vec::new()));
    }

    for (program, args) in candidates {
        if let Some(version) = python_version(&program, &args) {
            return Some(ResolvedPython {
                program,
                args,
                version,
            });
        }
    }

    None
}

fn venv_python_path(venv_dir: &std::path::Path) -> std::path::PathBuf {
    if cfg!(windows) {
        venv_dir.join("Scripts").join("python.exe")
    } else {
        venv_dir.join("bin").join("python")
    }
}

fn sha256_file(path: &std::path::Path) -> Result<Vec<u8>> {
    use sha2::Digest;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = sha2::Sha256::new();
    let mut buf = vec![0_u8; 1024 * 1024];
    loop {
        let n = std::io::Read::read(&mut file, buf.as_mut_slice())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().to_vec())
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn pack_integrity_manifest_path(paths: &AppPaths) -> std::path::PathBuf {
    paths.python_toolchain_dir().join("pack_integrity_manifest.json")
}

fn read_json_value_best_effort(path: &std::path::Path) -> Option<serde_json::Value> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn extract_zip_strip_prefix(
    zip_path: &std::path::Path,
    out_dir: &std::path::Path,
    prefix: &str,
) -> Result<()> {
    use zip::result::ZipError;

    let file = std::fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| {
        EngineError::InstallFailed(format!(
            "failed to read zip archive {}: {e}",
            zip_path.to_string_lossy()
        ))
    })?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| match e {
            ZipError::FileNotFound => EngineError::InstallFailed("zip entry missing".to_string()),
            other => EngineError::InstallFailed(format!("zip read failed: {other}")),
        })?;

        let name = entry.name().replace('\\', "/");
        if !name.starts_with(prefix) {
            continue;
        }
        let rel = name[prefix.len()..].trim_start_matches('/');
        if rel.is_empty() {
            continue;
        }

        // Prevent directory traversal.
        let rel_path = std::path::Path::new(rel);
        if rel_path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir | std::path::Component::RootDir | std::path::Component::Prefix(_)))
        {
            return Err(EngineError::InstallFailed(format!(
                "unsafe zip path: {name}"
            )));
        }

        let out_path = out_dir.join(rel_path);
        if entry.is_dir() {
            std::fs::create_dir_all(&out_path)?;
            continue;
        }

        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out_file = std::fs::File::create(&out_path)?;
        std::io::copy(&mut entry, &mut out_file)?;
    }

    Ok(())
}

fn python_version(program: &std::path::Path, base_args: &[String]) -> Option<String> {
    let mut cmd = crate::cmd::command(program);
    for arg in base_args {
        cmd.arg(arg);
    }
    let output = cmd.arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let text = if !stdout.trim().is_empty() {
        stdout
    } else {
        stderr
    };

    let first = text.lines().next()?.trim();
    if first.is_empty() {
        return None;
    }
    Some(first.to_string())
}

fn pip_version(venv_python: &std::path::Path) -> Option<String> {
    let output = crate::cmd::command(venv_python)
        .args(["-m", "pip", "--version"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let first = text.lines().next()?.trim();
    if first.is_empty() {
        return None;
    }
    Some(first.to_string())
}

#[derive(Debug, Clone, Serialize)]
pub struct SpleeterPackStatus {
    pub installed: bool,
    pub version: Option<String>,
    pub models_dir: String,
    pub models_installed: bool,
}

pub fn spleeter_pack_status(paths: &AppPaths) -> SpleeterPackStatus {
    let models_dir = paths
        .python_models_dir()
        .join("spleeter")
        .to_string_lossy()
        .to_string();
    let models_path = std::path::PathBuf::from(&models_dir);
    let models_installed = models_path.join("2stems").join(".probe").exists();

    let venv_dir = paths.python_venv_dir();
    let venv_python = venv_python_path(&venv_dir);
    if !venv_python.exists() {
        return SpleeterPackStatus {
            installed: false,
            version: None,
            models_dir,
            models_installed: false,
        };
    }

    let version = python_module_version(&venv_python, "spleeter");
    SpleeterPackStatus {
        installed: version.is_some() && models_installed,
        version,
        models_dir,
        models_installed,
    }
}

pub fn install_spleeter_pack(paths: &AppPaths) -> Result<SpleeterPackStatus> {
    // Ensure venv exists first.
    let _ = install_python_toolchain(paths)?;
    let venv_python = python_venv_python_path(paths)?;
    let py_version = python_version(&venv_python, &[]).unwrap_or_else(|| "unknown".to_string());
    let candidates = spleeter_install_candidates(&py_version);
    let mut last_error: Option<String> = None;
    let models_dir = paths.python_models_dir().join("spleeter");
    std::fs::create_dir_all(&models_dir)?;

    let _ = run_python_checked(
        paths,
        &venv_python,
        &[
            "-m",
            "pip",
            "install",
            "--upgrade",
            "pip",
            "setuptools",
            "wheel",
        ],
        "pip bootstrap failed",
    );

    let _ = run_python_checked(
        paths,
        &venv_python,
        &[
            "-m",
            "pip",
            "install",
            "--only-binary=:all:",
            "numpy==1.26.4",
        ],
        "numpy bootstrap failed",
    );
    let _ = run_python_checked(
        paths,
        &venv_python,
        &["-m", "pip", "install", "h2"],
        "httpx HTTP/2 dependency bootstrap failed",
    );

    for spec in candidates.iter() {
        let attempts: Vec<Vec<&str>> = vec![
            vec![
                "-m",
                "pip",
                "install",
                "--only-binary=:all:",
                "--prefer-binary",
                spec,
            ],
            vec![
                "-m",
                "pip",
                "install",
                "--no-binary=:all:",
                "--no-build-isolation",
                spec,
            ],
            vec!["-m", "pip", "install", "--no-build-isolation", spec],
            vec!["-m", "pip", "install", spec],
        ];

        for args in attempts {
            let err = run_python_checked(
                paths,
                &venv_python,
                &args,
                &format!("pip install {spec} failed"),
            );
            if err.is_ok() && python_module_version(&venv_python, "spleeter").is_some() {
                // Download Spleeter models during explicit install (not during jobs).
                //
                // Avoid relying on Spleeter's httpx client implementation (redirect handling differs
                // across httpx versions). Use stdlib urllib which follows redirects by default.
                let model_download_code = format!(
                    r#"
import hashlib
import json
import os
import tarfile
import tempfile
import time
import urllib.request
 
MODEL_PATH = r"{model_path}"
os.makedirs(MODEL_PATH, exist_ok=True)

model_name = "2stems"
repo = "deezer/spleeter"
release = "v1.4.0"
base = f"https://github.com/{{repo}}/releases/download/{{release}}"
checksum_url = f"{{base}}/checksum.json"
archive_url = f"{{base}}/{{model_name}}.tar.gz"

with urllib.request.urlopen(checksum_url) as resp:
    index = json.loads(resp.read().decode("utf-8"))
expected = index.get(model_name)
if not expected:
    raise RuntimeError("checksum.json missing 2stems entry")

tmp = tempfile.NamedTemporaryFile(delete=False, suffix=".tar.gz")
tmp_path = tmp.name
tmp.close()
try:
    with urllib.request.urlopen(archive_url) as resp, open(tmp_path, "wb") as f:
        while True:
            chunk = resp.read(1024 * 1024)
            if not chunk:
                break
            f.write(chunk)

    h = hashlib.sha256()
    with open(tmp_path, "rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    got = h.hexdigest()
    if got != expected:
        raise RuntimeError(f"model archive checksum mismatch: expected={{expected}} got={{got}}")

    target_dir = os.path.join(MODEL_PATH, model_name)
    os.makedirs(target_dir, exist_ok=True)
    with tarfile.open(name=tmp_path, mode="r:gz") as tar:
        target_real = os.path.realpath(target_dir)
        for member in tar.getmembers():
            member_path = os.path.realpath(os.path.join(target_dir, member.name))
            if not member_path.startswith(target_real + os.sep) and member_path != target_real:
                raise RuntimeError("unsafe tar member path")
        tar.extractall(path=target_dir)

    with open(os.path.join(target_dir, ".probe"), "w", encoding="utf-8") as f:
        f.write("OK")

    manifest = {{
        "schema_version": 1,
        "repo": repo,
        "release": release,
        "model_name": model_name,
        "archive_url": archive_url,
        "checksum_url": checksum_url,
        "expected_archive_sha256": expected,
        "got_archive_sha256": got,
        "downloaded_at_ms": int(time.time() * 1000),
    }}
    with open(os.path.join(target_dir, "voxvulgi_spleeter_manifest.json"), "w", encoding="utf-8") as f:
        json.dump(manifest, f, ensure_ascii=False, indent=2)
finally:
    try:
        os.unlink(tmp_path)
    except Exception:
        pass

print("spleeter_model_download_ok")
"#,
                    model_path = models_dir.to_string_lossy()
                );
                run_python_checked(
                    paths,
                    &venv_python,
                    &["-c", &model_download_code],
                    "Spleeter model download failed",
                )?;

                // Best-effort warmup.
                let _ = run_python_checked(
                    paths,
                    &venv_python,
                    &[
                        "-c",
                        "from spleeter.separator import Separator; Separator('spleeter:2stems'); print('ok')",
                    ],
                    "spleeter warmup failed",
                );
                let status = spleeter_pack_status(paths);
                let _ = generate_pack_integrity_manifest(paths);
                return Ok(status);
            }
            if let Err(err) = err {
                let context = err.to_string();
            let guidance = explain_spleeter_install_failure(&context, spec, &py_version);
                let prior = last_error.get_or_insert_with(String::new);
                if !prior.is_empty() {
                    prior.push('\n');
                }
                prior.push_str(&context);
                if !prior.contains(&guidance) {
                    prior.push('\n');
                    prior.push_str(&format!("Guidance: {guidance}"));
                }
            } else {
                last_error = Some("spleeter installed but module detection failed".to_string());
            }
        }
    }

    // Deterministic fallback for environments where dependency resolution is blocked by
    // strict pinning (especially tensorflow-io-gcs-filesystem==0.32.0).
    for spec in candidates.iter() {
        let attempts: Vec<Vec<&str>> = vec![
            vec![
                "-m",
                "pip",
                "install",
                "--no-deps",
                "--only-binary=:all:",
                "--prefer-binary",
                spec,
            ],
            vec![
                "-m",
                "pip",
                "install",
                "--no-deps",
                "--no-binary=:all:",
                "--no-build-isolation",
                spec,
            ],
            vec!["-m", "pip", "install", "--no-deps", "--no-build-isolation", spec],
            vec!["-m", "pip", "install", "--no-deps", spec],
        ];

        for args in attempts {
            let err = run_python_checked(
                paths,
                &venv_python,
                &args,
                &format!("pip install {spec} (no-deps fallback) failed"),
            );
            if err.is_ok() {
                if let Err(err) = install_spleeter_fallback_dependencies(paths, &venv_python) {
                    let context = err.to_string();
                    let prior = last_error.get_or_insert_with(String::new);
                    if !prior.is_empty() {
                        prior.push('\n');
                    }
                    prior.push_str(&context);
                    continue;
                }

                if let Err(err) = run_python_checked(
                    paths,
                    &venv_python,
                    &[
                        "-c",
                        "from spleeter.separator import Separator; Separator('spleeter:2stems'); print('ok')",
                    ],
                    "spleeter warmup failed",
                ) {
                    let prior = last_error.get_or_insert_with(String::new);
                    if !prior.is_empty() {
                        prior.push('\n');
                    }
                    prior.push_str(&err.to_string());
                    continue;
                }

                if python_module_version(&venv_python, "spleeter").is_some() {
                    let model_download_code = format!(
                        r#"
import hashlib
import json
import os
import tarfile
import tempfile
import time
import urllib.request

MODEL_PATH = r"{model_path}"
os.makedirs(MODEL_PATH, exist_ok=True)

model_name = "2stems"
repo = "deezer/spleeter"
release = "v1.4.0"
base = f"https://github.com/{{repo}}/releases/download/{{release}}"
checksum_url = f"{{base}}/checksum.json"
archive_url = f"{{base}}/{{model_name}}.tar.gz"

with urllib.request.urlopen(checksum_url) as resp:
    index = json.loads(resp.read().decode("utf-8"))
expected = index.get(model_name)
if not expected:
    raise RuntimeError("checksum.json missing 2stems entry")

tmp = tempfile.NamedTemporaryFile(delete=False, suffix=".tar.gz")
tmp_path = tmp.name
tmp.close()
try:
    with urllib.request.urlopen(archive_url) as resp, open(tmp_path, "wb") as f:
        while True:
            chunk = resp.read(1024 * 1024)
            if not chunk:
                break
            f.write(chunk)

    h = hashlib.sha256()
    with open(tmp_path, "rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    got = h.hexdigest()
    if got != expected:
        raise RuntimeError(f"model archive checksum mismatch: expected={{expected}} got={{got}}")

    target_dir = os.path.join(MODEL_PATH, model_name)
    os.makedirs(target_dir, exist_ok=True)
    with tarfile.open(name=tmp_path, mode="r:gz") as tar:
        target_real = os.path.realpath(target_dir)
        for member in tar.getmembers():
            member_path = os.path.realpath(os.path.join(target_dir, member.name))
            if not member_path.startswith(target_real + os.sep) and member_path != target_real:
                raise RuntimeError("unsafe tar member path")
        tar.extractall(path=target_dir)

    with open(os.path.join(target_dir, ".probe"), "w", encoding="utf-8") as f:
        f.write("OK")

    manifest = {{
        "schema_version": 1,
        "repo": repo,
        "release": release,
        "model_name": model_name,
        "archive_url": archive_url,
        "checksum_url": checksum_url,
        "expected_archive_sha256": expected,
        "got_archive_sha256": got,
        "downloaded_at_ms": int(time.time() * 1000),
    }}
    with open(os.path.join(target_dir, "voxvulgi_spleeter_manifest.json"), "w", encoding="utf-8") as f:
        json.dump(manifest, f, ensure_ascii=False, indent=2)
finally:
    try:
        os.unlink(tmp_path)
    except Exception:
        pass

print("spleeter_model_download_ok")
"#,
                        model_path = models_dir.to_string_lossy()
                    );
                    run_python_checked(
                        paths,
                        &venv_python,
                        &["-c", &model_download_code],
                        "Spleeter model download failed",
                    )?;
                    let status = spleeter_pack_status(paths);
                    let _ = generate_pack_integrity_manifest(paths);
                    return Ok(status);
                }
                last_error = Some(
                    "spleeter installed with fallback strategy, but module detection failed".to_string(),
                );
                continue;
            }
            if let Err(err) = err {
                let context = err.to_string();
                let guidance = explain_spleeter_install_failure(&context, spec, &py_version);
                let prior = last_error.get_or_insert_with(String::new);
                if !prior.is_empty() {
                    prior.push('\n');
                }
                prior.push_str(&context);
                if !prior.contains(&guidance) {
                    prior.push('\n');
                    prior.push_str(&format!("Guidance: {guidance}"));
                }
            }
        }
    }

    Err(EngineError::InstallFailed(match last_error {
        Some(last_error) => format!(
            "spleeter install failed for python {py_version}: {last_error}"
        ),
        None => "spleeter install failed without a captured reason".to_string(),
    }))
}

fn explain_spleeter_install_failure(raw_error: &str, spec: &str, py_version: &str) -> String {
    let text = raw_error.to_lowercase();

    if text.contains("cannot import 'poetry.core.masonry.api'") {
        return format!(
            "{spec} failed due source-branch build-path metadata tooling on this environment. Prefer a compatible interpreter (Python 3.9/3.10) or ensure the active interpreter can use available wheels."
        );
    }

    if text.contains("metadata-generation-failed")
        && (text.contains("numpy") || text.contains("ccompiler") || text.contains("nameerror"))
    {
        return format!(
            "Build tools in this environment cannot complete numpy metadata generation for {spec}. Use Python 3.9/3.10 with binary wheels or rerun after cleaning the venv."
        );
    }

    if text.contains("resolutionimpossible") {
        return format!(
            "Dependency resolution conflict for {spec} on Python {py_version}. Use Python 3.9/3.10, then retry install."
        );
    }

    if text.contains("tensorflow-io-gcs-filesystem==0.32.0")
        || text.contains("no matching distribution found for tensorflow-io-gcs-filesystem==0.32.0")
    {
        return format!(
            "{spec} is currently blocked by the pinned tensorflow-io-gcs-filesystem==0.32.0 requirement on Python {py_version}. The installer now attempts a no-deps fallback and explicit dependency bootstrap (including tensorflow-io-gcs-filesystem==0.31.0). If this still fails, switch the app interpreter in config/python_exe.txt to Python 3.9/3.10 and retry."
        );
    }

    if text.contains("no matching distribution found for tensorflow==2.12.1")
        || text.contains("could not find a version that satisfies the requirement tensorflow==2.12.1")
    {
        return format!(
            "TensorFlow pinned by {spec} cannot be resolved on Python {py_version}. Install Python 3.9/3.10, set it in config/python_exe.txt, and run install again."
        );
    }

    if text.contains("building wheel") || text.contains("error: command '") || text.contains("msvc") {
        return "Build path failed during native extension compile. Install Microsoft C++ Build Tools (or choose an interpreter where Spleeter wheels are available).".to_string();
    }

    if text.contains("permission denied") {
        return "Installer write access failed. Ensure a writable app data/cache directory or run with a writable filesystem path.".to_string();
    }

    if text.contains("invalid requirement") || text.contains("invalid specifier")
    {
        return "Pip metadata resolution failed. We already auto-upgraded pip/setuptools/wheel; retry the install after a clean retry.".to_string();
    }

    "Use a compatible Python interpreter for this environment, or keep Spleeter disabled and continue with non-Spleeter workflows.".to_string()
}

fn install_spleeter_fallback_dependencies(
    paths: &AppPaths,
    venv_python: &std::path::Path,
) -> Result<()> {
    let deps: Vec<&str> = vec![
        "tensorflow==2.12.1",
        "tensorflow-io-gcs-filesystem==0.32.0",
        "h2",
        "ffmpeg-python==0.2.0",
        "httpx",
        "typer",
        "click>=8.1.7",
        "norbert==0.2.1",
        "pandas==1.5.3",
        "numpy==1.26.4",
    ];

    for dep in deps {
        if dep == "tensorflow-io-gcs-filesystem==0.32.0" {
            if let Err(err) = install_python_dependency_pin(paths, venv_python, dep) {
                let raw = err.to_string().to_lowercase();
                if raw.contains("no matching distribution found for tensorflow-io-gcs-filesystem==0.32.0")
                    || raw.contains(
                        "could not find a version that satisfies the requirement tensorflow-io-gcs-filesystem==0.32.0",
                    )
                {
                    install_python_dependency_pin(
                        paths,
                        venv_python,
                        "tensorflow-io-gcs-filesystem==0.31.0",
                    )?;
                    continue;
                }
                return Err(err);
            }
            continue;
        }
        install_python_dependency_pin(paths, venv_python, dep)?;
    }

    Ok(())
}

fn install_python_dependency_pin(
    paths: &AppPaths,
    venv_python: &std::path::Path,
    pin: &str,
) -> Result<()> {
    let attempts: Vec<Vec<&str>> = vec![
        vec![
            "-m",
            "pip",
            "install",
            "--upgrade",
            "--only-binary=:all:",
            "--prefer-binary",
            pin,
        ],
        vec![
            "-m",
            "pip",
            "install",
            "--no-binary=:all:",
            "--no-build-isolation",
            pin,
        ],
        vec!["-m", "pip", "install", "--no-build-isolation", pin],
        vec!["-m", "pip", "install", pin],
    ];

    let mut last_error: Option<String> = None;
    for args in attempts {
        if let Err(err) = run_python_checked(
            paths,
            venv_python,
            &args,
            &format!("dependency install {pin} failed"),
        ) {
            last_error = Some(err.to_string());
            continue;
        }
        return Ok(());
    }

    Err(EngineError::InstallFailed(
        last_error.unwrap_or_else(|| format!("dependency install {pin} failed")),
    ))
}

fn spleeter_install_candidates(py_version: &str) -> Vec<&'static str> {
    match parse_python_major_minor(py_version) {
        Some((3, minor)) if (8..=11).contains(&minor) => {
            vec!["spleeter==2.4.2", "spleeter"]
        }
        Some((3, minor)) if minor < 8 => vec!["spleeter==2.4.0", "spleeter"],
        Some((3, _)) => vec!["spleeter"],
        _ => vec!["spleeter==2.4.2", "spleeter"],
    }
}

fn parse_python_major_minor(version: &str) -> Option<(u32, u32)> {
    let normalized = version.trim().trim_start_matches("Python ");
    let mut parts = normalized.split('.');
    let major = parts.next()?.parse::<u32>().ok()?;
    let minor = parts.next()?.parse::<u32>().ok()?;
    Some((major, minor))
}

#[derive(Debug, Clone, Serialize)]
pub struct DemucsPackStatus {
    pub installed: bool,
    pub demucs_version: Option<String>,
}

pub fn demucs_pack_status(paths: &AppPaths) -> DemucsPackStatus {
    let venv_dir = paths.python_venv_dir();
    let venv_python = venv_python_path(&venv_dir);
    if !venv_python.exists() {
        return DemucsPackStatus {
            installed: false,
            demucs_version: None,
        };
    }

    let demucs_version = python_module_version(&venv_python, "demucs_infer");
    DemucsPackStatus {
        installed: demucs_version.is_some(),
        demucs_version,
    }
}

pub fn install_demucs_pack(paths: &AppPaths) -> Result<DemucsPackStatus> {
    // Ensure venv exists first.
    let _ = install_python_toolchain(paths)?;
    let venv_python = python_venv_python_path(paths)?;

    let _ = run_python_checked(
        paths,
        &venv_python,
        &[
            "-m",
            "pip",
            "install",
            "--upgrade",
            "pip",
            "setuptools",
            "wheel",
        ],
        "pip bootstrap failed",
    );

    // Prefer an inference-only distribution (smaller surface area than full training stack).
    let pinned = "demucs-infer==4.1.2";
    if let Err(err) = run_python_checked(
        paths,
        &venv_python,
        &["-m", "pip", "install", "--prefer-binary", pinned],
        "pip install demucs-infer failed (pinned)",
    ) {
        run_python_checked(
            paths,
            &venv_python,
            &["-m", "pip", "install", "--prefer-binary", "demucs-infer"],
            &format!("pip install demucs-infer failed (unpinned fallback): {err}"),
        )?;
    }

    let _ = run_python_checked(
        paths,
        &venv_python,
        &["-c", "import demucs_infer; print('ok')"],
        "demucs warmup failed",
    );

    let status = demucs_pack_status(paths);
    let _ = generate_pack_integrity_manifest(paths);
    Ok(status)
}

#[derive(Debug, Clone, Serialize)]
pub struct DiarizationPackStatus {
    pub installed: bool,
    pub resemblyzer_version: Option<String>,
    pub numpy_version: Option<String>,
    pub sklearn_version: Option<String>,
}

pub fn diarization_pack_status(paths: &AppPaths) -> DiarizationPackStatus {
    let venv_dir = paths.python_venv_dir();
    let venv_python = venv_python_path(&venv_dir);
    if !venv_python.exists() {
        return DiarizationPackStatus {
            installed: false,
            resemblyzer_version: None,
            numpy_version: None,
            sklearn_version: None,
        };
    }

    let resemblyzer_version = python_module_version(&venv_python, "resemblyzer");
    let numpy_version = python_module_version(&venv_python, "numpy");
    let sklearn_version = python_module_version(&venv_python, "sklearn");

    let installed = resemblyzer_version.is_some() && numpy_version.is_some();

    DiarizationPackStatus {
        installed,
        resemblyzer_version,
        numpy_version,
        sklearn_version,
    }
}

pub fn install_diarization_pack(paths: &AppPaths) -> Result<DiarizationPackStatus> {
    // Ensure venv exists first.
    let _ = install_python_toolchain(paths)?;
    let venv_python = python_venv_python_path(paths)?;

    let pinned = [
        "resemblyzer==0.1.4",
        "numpy==1.26.4",
        "scikit-learn==1.8.0",
        "webrtcvad==2.0.10",
        "soundfile==0.13.1",
    ];
    let install_err = run_python_checked(
        paths,
        &venv_python,
        &[
            "-m",
            "pip",
            "install",
            pinned[0],
            pinned[1],
            pinned[2],
            pinned[3],
            pinned[4],
        ],
        "pip install diarization dependencies failed (pinned)",
    );
    if let Err(err) = install_err {
        // Best-effort fallback when pinned wheels are unavailable.
        let _ = run_python_checked(
            paths,
            &venv_python,
            &[
                "-m",
                "pip",
                "install",
                "resemblyzer",
                "numpy",
                "scikit-learn",
                "webrtcvad",
                "soundfile",
            ],
            &format!("pip install diarization dependencies failed (unpinned fallback): {err}"),
        )?;
    }

    patch_webrtcvad_pkg_resources_import(&venv_python)?;

    // Best-effort warmup to ensure the embedding model weights shipped with the package are usable.
    let _ = run_python_checked(
        paths,
        &venv_python,
        &["-c", "from resemblyzer import VoiceEncoder; VoiceEncoder(); print('ok')"],
        "diarization warmup failed",
    );

    let status = diarization_pack_status(paths);
    let _ = generate_pack_integrity_manifest(paths);
    Ok(status)
}

fn patch_webrtcvad_pkg_resources_import(python: &std::path::Path) -> Result<()> {
    let venv_dir = python
        .parent()
        .and_then(|p| p.parent())
        .ok_or_else(|| EngineError::InstallFailed("invalid venv python path".to_string()))?;

    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if cfg!(windows) {
        candidates.push(venv_dir.join("Lib").join("site-packages").join("webrtcvad.py"));
    } else {
        let lib_dir = venv_dir.join("lib");
        if let Ok(entries) = std::fs::read_dir(&lib_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if !name.starts_with("python") {
                    continue;
                }
                candidates.push(path.join("site-packages").join("webrtcvad.py"));
            }
        }
    }

    let file_path = candidates
        .into_iter()
        .find(|p| p.is_file())
        .ok_or_else(|| EngineError::InstallFailed("webrtcvad.py not found".to_string()))?;
    let bytes = std::fs::read(&file_path).map_err(|e| {
        EngineError::InstallFailed(format!("failed to read webrtcvad.py: {e}"))
    })?;
    let mut text = String::from_utf8_lossy(&bytes).to_string();

    if text.contains("try:") && text.contains("import pkg_resources") {
        return Ok(());
    }
    if !text.contains("import pkg_resources") {
        return Ok(());
    }

    // Patch only the fragile version lookup; keep behavior identical otherwise.
    text = text.replace(
        "import pkg_resources\n\nimport _webrtcvad\n",
        "try:\n    import pkg_resources\nexcept Exception:  # pragma: no cover\n    pkg_resources = None\n\nimport _webrtcvad\n",
    );
    text = text.replace(
        "__version__ = pkg_resources.get_distribution('webrtcvad').version",
        "__version__ = (pkg_resources.get_distribution('webrtcvad').version if pkg_resources else 'installed')",
    );

    std::fs::write(&file_path, text).map_err(|e| {
        EngineError::InstallFailed(format!("failed to patch webrtcvad.py: {e}"))
    })?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct TtsPreviewPackStatus {
    pub installed: bool,
    pub pyttsx3_version: Option<String>,
}

pub fn tts_preview_pack_status(paths: &AppPaths) -> TtsPreviewPackStatus {
    let venv_dir = paths.python_venv_dir();
    let venv_python = venv_python_path(&venv_dir);
    if !venv_python.exists() {
        return TtsPreviewPackStatus {
            installed: false,
            pyttsx3_version: None,
        };
    }

    let pyttsx3_version = python_module_version(&venv_python, "pyttsx3");
    TtsPreviewPackStatus {
        installed: pyttsx3_version.is_some(),
        pyttsx3_version,
    }
}

pub fn install_tts_preview_pack(paths: &AppPaths) -> Result<TtsPreviewPackStatus> {
    // Ensure venv exists first.
    let _ = install_python_toolchain(paths)?;
    let venv_python = python_venv_python_path(paths)?;

    let pinned = "pyttsx3==2.90";
    if let Err(err) = run_python_checked(
        paths,
        &venv_python,
        &["-m", "pip", "install", pinned],
        "pip install pyttsx3 failed (pinned)",
    ) {
        run_python_checked(
            paths,
            &venv_python,
            &["-m", "pip", "install", "pyttsx3"],
            &format!("pip install pyttsx3 failed (unpinned fallback): {err}"),
        )?;
    }

    let _ = run_python_checked(
        paths,
        &venv_python,
        &["-c", "import pyttsx3; pyttsx3.init(); print('ok')"],
        "pyttsx3 warmup failed",
    );

    let status = tts_preview_pack_status(paths);
    let _ = generate_pack_integrity_manifest(paths);
    Ok(status)
}

#[derive(Debug, Clone, Serialize)]
pub struct TtsNeuralLocalV1PackStatus {
    pub installed: bool,
    pub package_version: Option<String>,
}

pub fn tts_neural_local_v1_pack_status(paths: &AppPaths) -> TtsNeuralLocalV1PackStatus {
    let venv_dir = paths.python_venv_dir();
    let venv_python = venv_python_path(&venv_dir);
    if !venv_python.exists() {
        return TtsNeuralLocalV1PackStatus {
            installed: false,
            package_version: None,
        };
    }

    let package_version = python_module_version(&venv_python, "kokoro");
    TtsNeuralLocalV1PackStatus {
        installed: package_version.is_some(),
        package_version,
    }
}

pub fn install_tts_neural_local_v1_pack(paths: &AppPaths) -> Result<TtsNeuralLocalV1PackStatus> {
    // Ensure venv exists first.
    let _ = install_python_toolchain(paths)?;
    let venv_python = python_venv_python_path(paths)?;

    let _ = run_python_checked(
        paths,
        &venv_python,
        &["-m", "pip", "install", "--upgrade", "setuptools", "wheel"],
        "pip bootstrap failed",
    );

    // Kokoro -> Misaki -> spaCy requires Click features that aren't present in old Click versions.
    // Ensure we don't get stuck with older Typer/Click pins from other packs.
    let _ = run_python_checked(
        paths,
        &venv_python,
        &[
            "-m",
            "pip",
            "install",
            "--upgrade",
            "click>=8.1.7",
            "typer>=0.12.0",
        ],
        "pip upgrade click/typer compatibility for neural TTS failed",
    );

    let pinned = ["kokoro==0.9.4", "numpy==1.26.4", "soundfile==0.13.1", "torch==2.10.0"];
    let install_err = run_python_checked(
        paths,
        &venv_python,
        &["-m", "pip", "install", pinned[0], pinned[1], pinned[2], pinned[3]],
        "pip install neural TTS dependencies failed (pinned)",
    );
    if let Err(err) = install_err {
        run_python_checked(
            paths,
            &venv_python,
            &["-m", "pip", "install", "kokoro", "numpy", "soundfile", "torch"],
            &format!("pip install neural TTS dependencies failed (unpinned fallback): {err}"),
        )?;
    }

    let _ = run_python_checked(
        paths,
        &venv_python,
        &[
            "-c",
            "from kokoro import KPipeline\n_ = KPipeline(lang_code='a')\nprint('ok')",
        ],
        "neural TTS warmup failed",
    );

    let status = tts_neural_local_v1_pack_status(paths);
    let _ = generate_pack_integrity_manifest(paths);
    Ok(status)
}

#[derive(Debug, Clone, Serialize)]
pub struct TtsVoicePreservingLocalV1PackStatus {
    pub installed: bool,
    pub kokoro_version: Option<String>,
    pub openvoice_version: Option<String>,
    pub cosyvoice_version: Option<String>,
    pub openvoice_models_dir: String,
    pub openvoice_models_installed: bool,
    pub openvoice_patch_applied: bool,
}

pub fn tts_voice_preserving_local_v1_pack_status(
    paths: &AppPaths,
) -> TtsVoicePreservingLocalV1PackStatus {
    let openvoice_models_dir = paths
        .python_models_dir()
        .join("openvoice_v2")
        .to_string_lossy()
        .to_string();

    let venv_dir = paths.python_venv_dir();
    let venv_python = venv_python_path(&venv_dir);
    if !venv_python.exists() {
        return TtsVoicePreservingLocalV1PackStatus {
            installed: false,
            kokoro_version: None,
            openvoice_version: None,
            cosyvoice_version: None,
            openvoice_models_dir,
            openvoice_models_installed: false,
            openvoice_patch_applied: false,
        };
    }

    let kokoro_version = python_module_version(&venv_python, "kokoro");
    let openvoice_version = python_module_version(&venv_python, "openvoice");
    let cosyvoice_version = python_module_version(&venv_python, "cosyvoice");
    let openvoice_patch_applied =
        openvoice_api_patch_applied(&venv_python).unwrap_or(false);
    let models_dir = std::path::PathBuf::from(&openvoice_models_dir);
    let openvoice_models_installed = models_dir.join("converter").join("config.json").exists()
        && models_dir
            .join("converter")
            .join("checkpoint.pth")
            .exists();

    TtsVoicePreservingLocalV1PackStatus {
        installed: kokoro_version.is_some()
            && openvoice_version.is_some()
            && openvoice_models_installed
            && openvoice_patch_applied,
        kokoro_version,
        openvoice_version,
        cosyvoice_version,
        openvoice_models_dir,
        openvoice_models_installed,
        openvoice_patch_applied,
    }
}

pub fn install_tts_voice_preserving_local_v1_pack(
    paths: &AppPaths,
) -> Result<TtsVoicePreservingLocalV1PackStatus> {
    let _ = install_python_toolchain(paths)?;
    let venv_python = python_venv_python_path(paths)?;

    let _ = run_python_checked(
        paths,
        &venv_python,
        &["-m", "pip", "install", "--upgrade", "setuptools", "wheel"],
        "pip bootstrap failed",
    );

    // Voice-preserving dubbing uses Kokoro as the baseline TTS stage and OpenVoice V2 as the
    // voice-conversion stage.
    let _ = install_tts_neural_local_v1_pack(paths)?;

    let mut status_error: Option<String> = None;
    let mut openvoice_installed = false;
    // Pin OpenVoice to a known-good commit for determinism.
    const OPENVOICE_GIT_PIN: &str = "git+https://github.com/myshell-ai/OpenVoice.git@74a1d147b17a8c3092dd5430504bd83ef6c7eb23";
    let attempts = vec![vec![
        "-m",
        "pip",
        "install",
        "--upgrade",
        "--no-deps",
        OPENVOICE_GIT_PIN,
    ]];
    for args in attempts {
        match run_python_checked(
            paths,
            &venv_python,
            &args,
            "pip install OpenVoice failed",
        ) {
            Ok(()) => {
                openvoice_installed = true;
                status_error = None;
                break;
            }
            Err(err) => status_error = Some(err.to_string()),
        }
    }

    if !openvoice_installed {
        return Err(EngineError::InstallFailed(
            status_error.unwrap_or_else(|| {
                "OpenVoice install failed without a captured error".to_string()
            }),
        ));
    }

    let pinned = [
        "huggingface_hub==1.4.1",
        "librosa==0.11.0",
        "soundfile==0.13.1",
        "inflect==7.5.0",
        "Unidecode==1.4.0",
        "eng_to_ipa==0.0.2",
        "pypinyin==0.55.0",
        "cn2an==0.5.23",
        "jieba==0.42.1",
    ];
    let deps_err = run_python_checked(
        paths,
        &venv_python,
        &[
            "-m",
            "pip",
            "install",
            "--upgrade",
            pinned[0],
            pinned[1],
            pinned[2],
            pinned[3],
            pinned[4],
            pinned[5],
            pinned[6],
            pinned[7],
            pinned[8],
        ],
        "pip install OpenVoice dependencies failed (pinned)",
    );
    if let Err(err) = deps_err {
        let _ = run_python_checked(
            paths,
            &venv_python,
            &[
                "-m",
                "pip",
                "install",
                "--upgrade",
                "huggingface_hub",
                "librosa",
                "soundfile",
                "inflect",
                "Unidecode",
                "eng_to_ipa",
                "pypinyin",
                "cn2an",
                "jieba",
            ],
            &format!("pip install OpenVoice dependencies failed (unpinned fallback): {err}"),
        )?;
    }

    // Patch OpenVoice so `ToneColorConverter(enable_watermark=False)` works without requiring
    // watermarking deps at runtime.
    let openvoice_patch_code = r#"
import pathlib
import sys

api_path = None
for entry in sys.path:
    try:
        candidate = pathlib.Path(entry) / "openvoice" / "api.py"
    except Exception:
        continue
    if candidate.is_file():
        api_path = candidate
        break

if api_path is None:
    raise RuntimeError("openvoice/api.py not found on sys.path")

text = api_path.read_text(encoding="utf-8", errors="ignore")

patched_marker = "kwargs.pop('enable_watermark'"
broken_newline = "enable_watermark = kwargs.pop('enable_watermark', True)\\\\n        super().__init__(*args, **kwargs)"

if broken_newline in text:
    text = text.replace(
        broken_newline,
        "enable_watermark = kwargs.pop('enable_watermark', True)\n        super().__init__(*args, **kwargs)",
        1,
    )
    if "if kwargs.get('enable_watermark', True):" in text:
        text = text.replace("if kwargs.get('enable_watermark', True):", "if enable_watermark:", 1)
    api_path.write_text(text, encoding="utf-8")
    print("openvoice_api_fixed_newline")
elif patched_marker in text:
    print("openvoice_api_already_patched")
else:
    if "super().__init__(*args, **kwargs)" not in text:
        raise RuntimeError("unexpected openvoice/api.py: missing super().__init__ call")
    if "if kwargs.get('enable_watermark', True):" not in text:
        raise RuntimeError("unexpected openvoice/api.py: missing enable_watermark condition")

    text = text.replace(
        "super().__init__(*args, **kwargs)",
        "enable_watermark = kwargs.pop('enable_watermark', True)\n        super().__init__(*args, **kwargs)",
        1,
    )
    text = text.replace("if kwargs.get('enable_watermark', True):", "if enable_watermark:", 1)
    api_path.write_text(text, encoding="utf-8")
    print("openvoice_api_patched")
"#;

    run_python_checked(
        paths,
        &venv_python,
        &["-c", openvoice_patch_code],
        "OpenVoice patch failed",
    )?;

    let models_dir = paths.python_models_dir().join("openvoice_v2");
    std::fs::create_dir_all(&models_dir)?;

    // Pin the OpenVoiceV2 weights snapshot + verify hashes.
    const OPENVOICEV2_REPO_ID: &str = "myshell-ai/OpenVoiceV2";
    const OPENVOICEV2_REVISION: &str = "f36e7edfe1684461a8343844af60babc2efbb727";
    const OPENVOICEV2_CONFIG_SHA256: &str =
        "9dfff60350b8c63f2c664efd92a61b2516efb22671466960f0e5dfebd881fa47";
    const OPENVOICEV2_CHECKPOINT_SHA256: &str =
        "9652c27e92b6b2a91632590ac9962ef7ae2b712e5c5b7f4c34ec55ee2b37ab9e";
    const OPENVOICEV2_BASE_SPEAKER_EN_DEFAULT_SHA256: &str =
        "e4139de3bc2ea162f45a5a5f9559b710686c9689749b5ab8945ee5e2a082d154";

    let download_code = format!(
        r#"
import hashlib
import json
import os
import time

from huggingface_hub import hf_hub_download

repo_id = "{repo_id}"
revision = "{revision}"
base_dir = r"{models_dir}"
os.makedirs(base_dir, exist_ok=True)

files = [
  {{"filename": "converter/config.json", "sha256": "{config_sha256}"}},
  {{"filename": "converter/checkpoint.pth", "sha256": "{checkpoint_sha256}"}},
  {{"filename": "base_speakers/ses/en-default.pth", "sha256": "{base_speaker_sha256}"}},
]

downloaded = []
for entry in files:
  filename = entry["filename"]
  expected = entry["sha256"].lower()
  path = hf_hub_download(
    repo_id=repo_id,
    filename=filename,
    revision=revision,
    local_dir=base_dir,
    local_dir_use_symlinks=False,
  )

  h = hashlib.sha256()
  with open(path, "rb") as f:
    for chunk in iter(lambda: f.read(1024 * 1024), b""):
      h.update(chunk)
  got = h.hexdigest().lower()
  if got != expected:
    raise RuntimeError("OpenVoiceV2 file sha256 mismatch for %s: expected=%s got=%s" % (filename, expected, got))

  downloaded.append({{"filename": filename, "path": path, "sha256": got, "bytes": os.path.getsize(path)}})

manifest = {{
  "repo_id": repo_id,
  "revision": revision,
  "downloaded": downloaded,
  "downloaded_at_ms": int(time.time() * 1000),
}}

with open(os.path.join(base_dir, "voxvulgi_openvoicev2_manifest.json"), "w", encoding="utf-8") as f:
  json.dump(manifest, f, ensure_ascii=False, indent=2)
print("openvoicev2_download_ok")
"#,
        repo_id = OPENVOICEV2_REPO_ID,
        revision = OPENVOICEV2_REVISION,
        config_sha256 = OPENVOICEV2_CONFIG_SHA256,
        checkpoint_sha256 = OPENVOICEV2_CHECKPOINT_SHA256,
        base_speaker_sha256 = OPENVOICEV2_BASE_SPEAKER_EN_DEFAULT_SHA256,
        models_dir = models_dir.to_string_lossy(),
    );

    run_python_checked(
        paths,
        &venv_python,
        &["-c", &download_code],
        "OpenVoiceV2 model download failed",
    )?;

    let warmup_code = format!(
        r#"
import os
import torch
from importlib import import_module

base_dir = r"{models_dir}"
config_path = os.path.join(base_dir, "converter", "config.json")
ckpt_path = os.path.join(base_dir, "converter", "checkpoint.pth")

api_mod = import_module("openvoice.api")
ToneColorConverter = getattr(api_mod, "ToneColorConverter")

try:
  converter = ToneColorConverter(config_path, device="cpu", enable_watermark=False)
except TypeError as e:
  raise RuntimeError("ToneColorConverter must support enable_watermark=False") from e

for attr in ("watermark_model", "watermark_detector"):
  if hasattr(converter, attr):
    try:
      setattr(converter, attr, None)
    except Exception:
      pass

if hasattr(converter, "load_ckpt"):
  converter.load_ckpt(ckpt_path)
else:
  raise RuntimeError("ToneColorConverter has no load_ckpt()")

print("openvoice_converter_warmup_ok")
"#,
        models_dir = models_dir.to_string_lossy()
    );

    run_python_checked(
        paths,
        &venv_python,
        &["-c", &warmup_code],
        "OpenVoice converter warmup failed",
    )?;

    let status = tts_voice_preserving_local_v1_pack_status(paths);
    if !status.installed {
        return Err(EngineError::InstallFailed(
            status_error.unwrap_or_else(|| {
                "voice-preserving pack installation completed but status check failed".to_string()
            }),
        ));
    }

    let _ = generate_pack_integrity_manifest(paths);
    Ok(status)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pyttsx3Voice {
    pub id: String,
    pub name: String,
}

pub fn tts_preview_pyttsx3_list_voices(paths: &AppPaths) -> Result<Vec<Pyttsx3Voice>> {
    let pack = tts_preview_pack_status(paths);
    if !pack.installed {
        return Err(EngineError::InstallFailed(
            "TTS preview pack is not installed. Open Diagnostics -> Tools -> Install TTS preview pack."
                .to_string(),
        ));
    }

    let venv_python = python_venv_python_path(paths).map_err(|_| {
        EngineError::InstallFailed(
            "Python toolchain is not set up. Open Diagnostics -> Tools -> Setup Python toolchain."
                .to_string(),
        )
    })?;

    // Emit a single JSON line so we can parse the final non-empty stdout line robustly.
    let code = r#"
import json
import pyttsx3

engine = pyttsx3.init()
voices = []
for v in (engine.getProperty("voices") or []):
    vid = getattr(v, "id", "") or ""
    name = getattr(v, "name", "") or ""
    vid = str(vid).strip()
    if not vid:
        continue
    name = (str(name).strip() if name else vid)
    voices.append({"id": vid, "name": name})

print(json.dumps(voices, ensure_ascii=False))
"#;

    let mut cmd = crate::cmd::command(&venv_python);
    cmd.args(["-c", code]);
    cmd.env("PYTHONNOUSERSITE", "1");
    cmd.env(
        "XDG_CACHE_HOME",
        paths.cache_dir().join("python").to_string_lossy().to_string(),
    );

    let output = cmd
        .output()
        .map_err(|e| EngineError::InstallFailed(format!("failed to list pyttsx3 voices: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(EngineError::InstallFailed(format!(
            "pyttsx3 voices script failed (code={:?}): {}",
            output.status.code(),
            stderr.trim()
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let last = stdout
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    if last.is_empty() {
        return Ok(Vec::new());
    }

    let voices: Vec<Pyttsx3Voice> = serde_json::from_str(last).map_err(|e| {
        EngineError::InstallFailed(format!("failed to parse pyttsx3 voices JSON: {e}"))
    })?;
    Ok(voices)
}

fn python_module_version(python: &std::path::Path, module: &str) -> Option<String> {
    let code = format!(
        "import importlib\nm=importlib.import_module({module:?})\nprint(getattr(m,'__version__', 'installed') or 'installed')\n"
    );
    let output = crate::cmd::command(python).args(["-c", &code]).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines().find_map(|line| {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn openvoice_api_patch_applied(python: &std::path::Path) -> Option<bool> {
    // Status checks should be fast, deterministic, and independent from Python import side-effects.
    // We derive the venv root from the venv python path and read `openvoice/api.py` directly.
    let venv_dir = python.parent()?.parent()?;

    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if cfg!(windows) {
        candidates.push(
            venv_dir
                .join("Lib")
                .join("site-packages")
                .join("openvoice")
                .join("api.py"),
        );
    } else {
        let lib_dir = venv_dir.join("lib");
        if let Ok(entries) = std::fs::read_dir(&lib_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if !name.starts_with("python") {
                    continue;
                }
                candidates.push(
                    path.join("site-packages")
                        .join("openvoice")
                        .join("api.py"),
                );
            }
        }
    }

    let api_path = candidates.into_iter().find(|p| p.is_file())?;
    let bytes = std::fs::read(api_path).ok()?;
    let text = String::from_utf8_lossy(&bytes);

    let has_pop = text.contains("kwargs.pop('enable_watermark'")
        || text.contains("kwargs.pop(\"enable_watermark\"");
    let has_if_enable = text.contains("if enable_watermark:");
    let broken = text.contains("kwargs.pop('enable_watermark', True)\\\\n        super().__init__(*args, **kwargs)")
        || text.contains("kwargs.pop(\"enable_watermark\", True)\\\\n        super().__init__(*args, **kwargs)")
        || text.contains("enable_watermark = kwargs.pop('enable_watermark', True)\\\\n        super().__init__(*args, **kwargs)")
        || text.contains("enable_watermark = kwargs.pop(\"enable_watermark\", True)\\\\n        super().__init__(*args, **kwargs)");

    Some(has_pop && has_if_enable && !broken)
}

fn run_python_checked(
    paths: &AppPaths,
    python: &std::path::Path,
    args: &[&str],
    error_prefix: &str,
) -> Result<()> {
    let mut cmd = crate::cmd::command(python);
    cmd.args(args);

    // Reduce surprise writes outside app-data.
    cmd.env("PYTHONNOUSERSITE", "1");
    cmd.env("PIP_DISABLE_PIP_VERSION_CHECK", "1");
    cmd.env("PIP_NO_INPUT", "1");
    cmd.env(
        "PIP_CACHE_DIR",
        paths.cache_dir().join("pip").to_string_lossy().to_string(),
    );
    cmd.env(
        "XDG_CACHE_HOME",
        paths.cache_dir().join("python").to_string_lossy().to_string(),
    );
    cmd.env(
        "HF_HOME",
        paths.cache_dir().join("huggingface").to_string_lossy().to_string(),
    );
    cmd.env(
        "HUGGINGFACE_HUB_CACHE",
        paths.cache_dir()
            .join("huggingface")
            .join("hub")
            .to_string_lossy()
            .to_string(),
    );

    let output = cmd
        .output()
        .map_err(|e| EngineError::InstallFailed(format!("{error_prefix}: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(EngineError::InstallFailed(format!(
            "{error_prefix} (code={:?}): {}",
            output.status.code(),
            stderr.trim()
        )));
    }
    Ok(())
}
