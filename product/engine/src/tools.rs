use crate::pack_install_state;
use crate::paths::AppPaths;
use crate::python_lockfile::{self, PythonLockfile};
use crate::{pinned_dependency_manifest, vendor_patches};
use crate::{EngineError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex, OnceLock};

trait OwnedCommandOutputExt {
    fn owned_output(&mut self) -> std::io::Result<std::process::Output>;
}

impl OwnedCommandOutputExt for std::process::Command {
    fn owned_output(&mut self) -> std::io::Result<std::process::Output> {
        crate::cmd::run_owned_output(
            self,
            std::time::Duration::from_secs(3600),
            crate::jobs::external_command_cancel_requested,
        )
    }
}

const PYTHON_COMMAND_TIMEOUT_SECS: u64 = 30 * 60;
// Hashed pack repairs can force-reinstall a full Torch-backed lock after a failed
// or interrupted attempt (WP-0234). On a throttled connection that legitimately
// exceeded the generic 30-minute command budget, even though pip had continued
// making progress. Keep the normal command bound tight while allowing the
// deterministic, hash-verified pack transaction enough time to finish.
const PYTHON_LOCKFILE_INSTALL_TIMEOUT_SECS: u64 = 60 * 60;
const PYTHON_POST_INSTALL_VERSION_CHECK_RETRIES: usize = 10;
const PYTHON_POST_INSTALL_VERSION_CHECK_DELAY_MS: u64 = 2_000;

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
        match ffmpeg_sidecar::download::download_ffmpeg_package(download_url, &destination) {
            Ok(path) => path,
            Err(primary_err) => download_ffmpeg_package_with_curl(download_url, &destination)
                .map_err(|fallback_err| {
                    EngineError::InstallFailed(format!(
                    "ffmpeg download failed: {primary_err}; curl fallback failed: {fallback_err}"
                ))
                })?,
        };
    ffmpeg_sidecar::download::unpack_ffmpeg(&archive_path, &destination)
        .map_err(|e| EngineError::InstallFailed(e.to_string()))?;

    Ok(ffmpeg_tools_status(paths))
}

fn download_ffmpeg_package_with_curl(url: &str, download_dir: &Path) -> Result<PathBuf> {
    let filename = Path::new(url).file_name().ok_or_else(|| {
        EngineError::InstallFailed("could not derive ffmpeg filename".to_string())
    })?;
    let archive_path = download_dir.join(filename);

    download_url_to_file_with_curl(url, &archive_path, "ffmpeg archive")?;

    Ok(archive_path)
}

fn download_url_to_file_with_curl(url: &str, output_path: &Path, label: &str) -> Result<()> {
    let _ = std::fs::remove_file(output_path);

    let curl_program = if cfg!(windows) { "curl.exe" } else { "curl" };
    let output = crate::cmd::command(curl_program)
        .arg("-L")
        .arg("--fail")
        .arg("--silent")
        .arg("--show-error")
        .arg("--retry")
        .arg("5")
        .arg("--retry-all-errors")
        .arg("--retry-delay")
        .arg("2")
        .arg("--connect-timeout")
        .arg("30")
        .arg("--speed-limit")
        .arg("1024")
        .arg("--speed-time")
        .arg("60")
        .arg("--output")
        .arg(output_path)
        .arg(url)
        .owned_output()
        .map_err(|e| EngineError::InstallFailed(format!("could not launch curl: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(EngineError::InstallFailed(format!(
            "curl exited with status {}{}",
            output.status,
            if stderr.is_empty() {
                String::new()
            } else {
                format!(": {stderr}")
            }
        )));
    }

    if !output_path.exists() {
        return Err(EngineError::InstallFailed(format!(
            "curl did not create {label}: {}",
            output_path.to_string_lossy()
        )));
    }

    let downloaded_size = std::fs::metadata(output_path).map(|m| m.len()).unwrap_or(0);
    if downloaded_size == 0 {
        let _ = std::fs::remove_file(output_path);
        return Err(EngineError::InstallFailed(format!(
            "curl created empty {label}: {}",
            output_path.to_string_lossy()
        )));
    }

    Ok(())
}

fn tool_version_first_line(program: impl AsRef<std::ffi::OsStr>) -> Option<String> {
    let output = crate::cmd::command(program)
        .arg("-version")
        .owned_output()
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
pub struct YtDlpToolsStatus {
    pub available: bool,
    pub bundled_installed: bool,
    pub bundled_path: String,
    pub ytdlp_path: String,
    pub ytdlp_version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct JsRuntimeToolsStatus {
    pub available: bool,
    pub preferred_runtime: String,
    pub preferred_path: String,
    pub preferred_version: Option<String>,
    pub bundled_deno_installed: bool,
    pub bundled_deno_path: String,
    pub bundled_deno_version: Option<String>,
    pub deno_on_path: bool,
    pub deno_path: String,
    pub deno_version: Option<String>,
    pub node_on_path: bool,
    pub node_path: String,
    pub node_version: Option<String>,
}

#[derive(Debug, Clone)]
struct ResolvedJsRuntime {
    runtime_id: &'static str,
    spec: String,
    path: String,
    version: String,
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
        let pin = &pinned_dependency_manifest::manifest().yt_dlp_windows;

        let destination = bundled_ytdlp_path(paths);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let tmp_path = destination.with_extension("download");

        let primary_download = (|| -> Result<()> {
            let resp = ureq::get(&pin.url)
                .call()
                .map_err(|e| EngineError::InstallFailed(format!("yt-dlp download failed: {e}")))?;
            let status = resp.status();
            if status.as_u16() >= 400 {
                return Err(EngineError::InstallFailed(format!(
                    "yt-dlp download failed (status={status})"
                )));
            }

            let mut reader = resp.into_body().into_reader();
            let mut file = std::fs::File::create(&tmp_path)?;
            std::io::copy(&mut reader, &mut file)?;
            file.flush()?;
            Ok(())
        })();
        if let Err(primary_err) = primary_download {
            download_url_to_file_with_curl(&pin.url, &tmp_path, "yt-dlp executable").map_err(
                |fallback_err| {
                    EngineError::InstallFailed(format!(
                        "yt-dlp download failed: {primary_err}; curl fallback failed: {fallback_err}"
                    ))
                },
            )?;
        }

        let downloaded_size = std::fs::metadata(&tmp_path).map(|m| m.len()).unwrap_or(0);
        if downloaded_size != pin.file_bytes {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(EngineError::SizeMismatch {
                path: tmp_path.clone(),
                expected: pin.file_bytes,
                actual: downloaded_size,
            });
        }

        let expected = hex::decode(&pin.sha256_hex)
            .map_err(|e| EngineError::InstallFailed(format!("invalid yt-dlp sha256 pin: {e}")))?;
        let got = sha256_file(&tmp_path)?;
        if got != expected {
            let actual = hex::encode_upper(got);
            let _ = std::fs::remove_file(&tmp_path);
            return Err(EngineError::HashMismatch {
                path: tmp_path.clone(),
                expected: pin.sha256_hex.clone(),
                actual,
            });
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

fn bundled_deno_path(paths: &AppPaths) -> std::path::PathBuf {
    paths.deno_exe()
}

fn resolve_js_runtime_candidate(
    runtime_id: &'static str,
    candidate: std::path::PathBuf,
    prefer_explicit_path: bool,
) -> Option<ResolvedJsRuntime> {
    let version = tool_version_first_line_with_arg(&candidate, "--version")?;
    let spec = if prefer_explicit_path || candidate.is_absolute() {
        format!("{runtime_id}:{}", candidate.to_string_lossy())
    } else {
        runtime_id.to_string()
    };
    Some(ResolvedJsRuntime {
        runtime_id,
        spec,
        path: candidate.to_string_lossy().to_string(),
        version,
    })
}

fn preferred_ytdlp_js_runtime(paths: &AppPaths) -> Option<ResolvedJsRuntime> {
    let bundled_deno = bundled_deno_path(paths);
    if bundled_deno.exists() {
        if let Some(runtime) = resolve_js_runtime_candidate("deno", bundled_deno, true) {
            return Some(runtime);
        }
    }

    if let Some(runtime) =
        resolve_js_runtime_candidate("deno", std::path::PathBuf::from("deno"), false)
    {
        return Some(runtime);
    }

    resolve_js_runtime_candidate("node", std::path::PathBuf::from("node"), false)
}

pub fn preferred_ytdlp_js_runtime_arg(paths: &AppPaths) -> Option<String> {
    preferred_ytdlp_js_runtime(paths).map(|runtime| runtime.spec)
}

pub fn js_runtime_tools_status(paths: &AppPaths) -> JsRuntimeToolsStatus {
    let bundled_deno = bundled_deno_path(paths);
    let bundled_deno_version = if bundled_deno.exists() {
        tool_version_first_line_with_arg(&bundled_deno, "--version")
    } else {
        None
    };

    let deno_resolution =
        resolve_js_runtime_candidate("deno", std::path::PathBuf::from("deno"), false);
    let node_resolution =
        resolve_js_runtime_candidate("node", std::path::PathBuf::from("node"), false);
    let preferred = preferred_ytdlp_js_runtime(paths);

    JsRuntimeToolsStatus {
        available: preferred.is_some(),
        preferred_runtime: preferred
            .as_ref()
            .map(|runtime| runtime.runtime_id.to_string())
            .unwrap_or_default(),
        preferred_path: preferred
            .as_ref()
            .map(|runtime| runtime.path.clone())
            .unwrap_or_default(),
        preferred_version: preferred.as_ref().map(|runtime| runtime.version.clone()),
        bundled_deno_installed: bundled_deno.exists() && bundled_deno_version.is_some(),
        bundled_deno_path: bundled_deno.to_string_lossy().to_string(),
        bundled_deno_version,
        deno_on_path: deno_resolution.is_some(),
        deno_path: deno_resolution
            .as_ref()
            .map(|runtime| runtime.path.clone())
            .unwrap_or_default(),
        deno_version: deno_resolution
            .as_ref()
            .map(|runtime| runtime.version.clone()),
        node_on_path: node_resolution.is_some(),
        node_path: node_resolution
            .as_ref()
            .map(|runtime| runtime.path.clone())
            .unwrap_or_default(),
        node_version: node_resolution
            .as_ref()
            .map(|runtime| runtime.version.clone()),
    }
}

pub fn install_js_runtime_tools(paths: &AppPaths) -> Result<JsRuntimeToolsStatus> {
    #[cfg(not(windows))]
    {
        let _ = paths;
        return Err(EngineError::InstallFailed(
            "automatic Deno install is only supported on Windows for now".to_string(),
        ));
    }

    #[cfg(windows)]
    {
        let pin = &pinned_dependency_manifest::manifest().deno_windows;

        paths.ensure_dirs()?;
        let install_dir = paths.deno_dir();
        std::fs::create_dir_all(&install_dir)?;

        let marker = install_dir.join(".probe");
        if marker.exists() {
            let status = js_runtime_tools_status(paths);
            if status.bundled_deno_installed {
                return Ok(status);
            }
        }

        if install_dir.exists() {
            let _ = std::fs::remove_dir_all(&install_dir);
        }
        std::fs::create_dir_all(&install_dir)?;

        let download_tmp = install_dir.join(format!("deno-{}.zip.download", pin.version));
        let download_final = install_dir.join(format!("deno-{}.zip", pin.version));

        let resp = ureq::get(&pin.url)
            .call()
            .map_err(|e| EngineError::InstallFailed(format!("Deno download failed: {e}")))?;
        let status = resp.status();
        if status.as_u16() >= 400 {
            return Err(EngineError::InstallFailed(format!(
                "Deno download failed (status={status})"
            )));
        }

        {
            let mut reader = resp.into_body().into_reader();
            let mut file = std::fs::File::create(&download_tmp)?;
            std::io::copy(&mut reader, &mut file)?;
            file.flush()?;
        }

        let downloaded_size = std::fs::metadata(&download_tmp)
            .map(|m| m.len())
            .unwrap_or(0);
        if downloaded_size != pin.file_bytes {
            let _ = std::fs::remove_file(&download_tmp);
            return Err(EngineError::SizeMismatch {
                path: download_tmp.clone(),
                expected: pin.file_bytes,
                actual: downloaded_size,
            });
        }

        let expected = hex::decode(&pin.sha256_hex)
            .map_err(|e| EngineError::InstallFailed(format!("invalid Deno sha256 pin: {e}")))?;
        let got = sha256_file(&download_tmp)?;
        if got != expected {
            let actual = hex::encode_upper(got);
            let _ = std::fs::remove_file(&download_tmp);
            return Err(EngineError::HashMismatch {
                path: download_tmp.clone(),
                expected: pin.sha256_hex.clone(),
                actual,
            });
        }

        if download_final.exists() {
            let _ = std::fs::remove_file(&download_final);
        }
        if std::fs::rename(&download_tmp, &download_final).is_err() {
            std::fs::copy(&download_tmp, &download_final)?;
            let _ = std::fs::remove_file(&download_tmp);
        }

        extract_zip_strip_prefix(&download_final, &install_dir, "")?;

        let exe = paths.deno_exe();
        let version = tool_version_first_line_with_arg(&exe, "--version").ok_or_else(|| {
            EngineError::InstallFailed("Deno is not usable after install".to_string())
        })?;
        crate::persistence::atomic_write_text(
            &marker,
            format!(
                "OK\nversion={}\nsource={}\nsha256={}\n",
                version.trim(),
                pin.source_label,
                pin.sha256_hex
            )
            .as_str(),
        )?;

        let _ = generate_pack_integrity_manifest(paths);
        Ok(js_runtime_tools_status(paths))
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct InstagramProfileProviderStatus {
    pub installed: bool,
    pub enumerator_ready: bool,
    pub version: String,
    pub executable_path: String,
    pub enumerator_script_path: String,
    pub readiness_error: Option<String>,
}

const INSTAGRAM_PROFILE_ENUMERATOR_SCRIPT: &[u8] =
    include_bytes!("../resources/tooling/instagram_profile_enumerator.py");

fn instagram_profile_enumerator_version(paths: &AppPaths) -> Option<String> {
    let python = python_venv_python_path(paths).ok()?;
    let output = crate::cmd::command(python)
        .args(["-c", "import instaloader; print(instaloader.__version__)"])
        .owned_output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()
        .map(|value| value.trim().to_string())
}

pub fn instagram_profile_provider_status(paths: &AppPaths) -> InstagramProfileProviderStatus {
    let pin = &pinned_dependency_manifest::manifest().instagram_profile_provider;
    let executable = paths.instagram_profile_provider_exe();
    let size_matches = std::fs::metadata(&executable)
        .map(|metadata| metadata.len() == pin.executable_bytes)
        .unwrap_or(false);
    let hash_matches = size_matches
        && file_sha256_hex(&executable)
            .is_some_and(|hash| hash.eq_ignore_ascii_case(&pin.executable_sha256_hex));
    let version_matches = hash_matches
        && tool_version_first_line_with_arg(&executable, "--version")
            .is_some_and(|version| version.trim() == pin.version);
    let script = paths.instagram_profile_enumerator_script();
    let script_matches = std::fs::read(&script)
        .map(|bytes| bytes == INSTAGRAM_PROFILE_ENUMERATOR_SCRIPT)
        .unwrap_or(false);
    let enumerator_ready = script_matches
        && instagram_profile_enumerator_version(paths)
            .is_some_and(|version| version == pin.version);
    InstagramProfileProviderStatus {
        installed: version_matches,
        enumerator_ready,
        version: pin.version.clone(),
        executable_path: executable.to_string_lossy().to_string(),
        enumerator_script_path: script.to_string_lossy().to_string(),
        readiness_error: if version_matches && enumerator_ready {
            None
        } else if !version_matches {
            Some("the pinned Instagram profile provider is missing or failed byte/version verification; rebuild or repair the offline tooling payload".to_string())
        } else {
            Some("the pinned Instagram profile enumerator module/script is missing from the bundled Python environment; rebuild or repair the offline tooling payload".to_string())
        },
    }
}

#[cfg(windows)]
pub fn install_instagram_profile_provider(
    paths: &AppPaths,
) -> Result<InstagramProfileProviderStatus> {
    let current = instagram_profile_provider_status(paths);
    if current.installed
        && std::fs::read(paths.instagram_profile_enumerator_script())
            .map(|bytes| bytes == INSTAGRAM_PROFILE_ENUMERATOR_SCRIPT)
            .unwrap_or(false)
    {
        return Ok(current);
    }
    let pin = &pinned_dependency_manifest::manifest().instagram_profile_provider;
    let install_dir = paths.instagram_profile_provider_dir();
    std::fs::create_dir_all(&install_dir)?;
    let archive = install_dir.join(format!("instaloader-{}.zip", pin.version));
    download_verified_file(
        &pin.url,
        &archive,
        pin.file_bytes,
        &pin.sha256_hex,
        "Instagram profile provider",
    )?;
    extract_zip_strip_prefix(&archive, &install_dir, "")?;
    crate::persistence::atomic_write_bytes(
        &paths.instagram_profile_enumerator_script(),
        INSTAGRAM_PROFILE_ENUMERATOR_SCRIPT,
    )?;
    let status = instagram_profile_provider_status(paths);
    if !status.installed {
        return Err(EngineError::InstallFailed(
            status.readiness_error.clone().unwrap_or_else(|| {
                "Instagram profile provider verification failed after extraction".to_string()
            }),
        ));
    }
    crate::persistence::atomic_write_text(
        &install_dir.join(".probe"),
        &format!(
            "OK\nversion={}\nsource={}\narchive_sha256={}\nexecutable_sha256={}\n",
            pin.version, pin.source_label, pin.sha256_hex, pin.executable_sha256_hex
        ),
    )?;
    let _ = generate_pack_integrity_manifest(paths);
    Ok(status)
}

#[cfg(windows)]
pub fn install_instagram_profile_enumerator(
    paths: &AppPaths,
) -> Result<InstagramProfileProviderStatus> {
    let pin = &pinned_dependency_manifest::manifest().instagram_profile_enumerator;
    let python = python_venv_python_path(paths)?;
    let install_dir = paths.instagram_profile_provider_dir();
    std::fs::create_dir_all(&install_dir)?;
    crate::persistence::atomic_write_bytes(
        &paths.instagram_profile_enumerator_script(),
        INSTAGRAM_PROFILE_ENUMERATOR_SCRIPT,
    )?;
    let wheel = install_dir.join(format!("instaloader-{}-py3-none-any.whl", pin.version));
    download_verified_file(
        &pin.url,
        &wheel,
        pin.file_bytes,
        &pin.sha256_hex,
        "Instagram profile enumerator wheel",
    )?;
    run_python_checked(
        paths,
        &python,
        &[
            "-m",
            "pip",
            "install",
            "--no-deps",
            "--force-reinstall",
            wheel.to_string_lossy().as_ref(),
        ],
        "Instagram profile enumerator wheel install failed",
    )?;
    let status = instagram_profile_provider_status(paths);
    if !status.installed || !status.enumerator_ready {
        return Err(EngineError::InstallFailed(
            status.readiness_error.clone().unwrap_or_else(|| {
                "Instagram profile enumerator verification failed after install".to_string()
            }),
        ));
    }
    let _ = generate_pack_integrity_manifest(paths);
    Ok(status)
}

#[cfg(not(windows))]
pub fn install_instagram_profile_provider(
    _paths: &AppPaths,
) -> Result<InstagramProfileProviderStatus> {
    Err(EngineError::InstallFailed(
        "the managed Instagram profile provider is currently packaged for Windows".to_string(),
    ))
}

pub fn ensure_instagram_profile_provider(paths: &AppPaths) -> Result<std::path::PathBuf> {
    let status = instagram_profile_provider_status(paths);
    if status.installed && status.enumerator_ready {
        return Ok(paths.instagram_profile_provider_exe());
    }
    Err(EngineError::InstallFailed(
        status
            .readiness_error
            .unwrap_or_else(|| "Instagram profile provider is not ready".to_string()),
    ))
}

#[derive(Debug, Clone, Serialize)]
pub struct YoutubePoProviderInstallStatus {
    pub installed: bool,
    pub provider_version: String,
    pub node_version: Option<String>,
    pub npm_version: Option<String>,
    pub node_exe_sha256_hex: Option<String>,
    pub npm_cmd_sha256_hex: Option<String>,
    pub plugin_sha256_hex: Option<String>,
    pub plugin_tree_sha256_hex: Option<String>,
    pub server_entrypoint_sha256_hex: Option<String>,
    pub derived_lock_sha256_hex: Option<String>,
    pub node_modules_tree_sha256_hex: Option<String>,
    pub node_modules_integrity_verifying: bool,
    pub node_modules_integrity_state: String,
    pub node_modules_verified_at_ms: Option<i64>,
    pub security_audit_passed: bool,
    pub readiness_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
struct ProviderNodeModulesIntegrityReceipt {
    schema_version: u32,
    install_generation: String,
    tree_sha256_hex: String,
    verified_at_ms: i64,
}

#[derive(Debug, Clone)]
struct ProviderNodeModulesProcessAttestation {
    install_generation: String,
    tree_sha256_hex: String,
    verified_at_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderVerificationProgress {
    pub schema_version: u32,
    pub semantic_key: String,
    pub source_identity: String,
    pub phase: String,
    pub state: String,
    pub revision: u64,
    pub files_completed: u64,
    pub files_planned: Option<u64>,
    pub bytes_completed: u64,
    pub bytes_planned: Option<u64>,
    pub scan_count: u64,
    pub started_at_ms: i64,
    pub updated_at_ms: i64,
    pub finished_at_ms: Option<i64>,
    pub error: Option<String>,
    pub foreground_pressure_active: bool,
    pub held_reason: Option<String>,
    pub resource_policy: String,
}

const PROVIDER_VERIFICATION_FOREGROUND_LEASE_MS: i64 = 5_000;
const PROVIDER_VERIFICATION_BACKGROUND_POLICY: &str =
    "single_flight_32_file_yield_256_file_1ms_checkpoint";
const PROVIDER_VERIFICATION_FOREGROUND_POLICY: &str =
    "foreground_checkpoint_4_file_yield_16_file_2ms_sleep";

#[derive(Debug, Clone)]
struct ProviderVerificationForegroundLease {
    generation: u64,
    expires_at_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderVerificationForegroundDemand {
    pub active: bool,
    pub active_consumers: usize,
    pub generation: u64,
    pub expires_at_ms: Option<i64>,
    pub held_reason: Option<String>,
    pub resource_policy: String,
}

fn provider_verification_foreground_slots(
) -> &'static Mutex<HashMap<PathBuf, HashMap<String, ProviderVerificationForegroundLease>>> {
    static PRESSURE: OnceLock<
        Mutex<HashMap<PathBuf, HashMap<String, ProviderVerificationForegroundLease>>>,
    > = OnceLock::new();
    PRESSURE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn provider_verification_resource_policy(active: bool) -> &'static str {
    if active {
        PROVIDER_VERIFICATION_FOREGROUND_POLICY
    } else {
        PROVIDER_VERIFICATION_BACKGROUND_POLICY
    }
}

fn provider_verification_foreground_demand_for_key(
    server_dir: &Path,
    generation: u64,
) -> ProviderVerificationForegroundDemand {
    let now = now_ms();
    let mut slots = provider_verification_foreground_slots()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(leases) = slots.get_mut(server_dir) else {
        return ProviderVerificationForegroundDemand {
            active: false,
            active_consumers: 0,
            generation,
            expires_at_ms: None,
            held_reason: None,
            resource_policy: PROVIDER_VERIFICATION_BACKGROUND_POLICY.to_string(),
        };
    };
    leases.retain(|_, lease| lease.expires_at_ms > now);
    let active_consumers = leases.len();
    let active = active_consumers > 0;
    let expires_at_ms = leases.values().map(|lease| lease.expires_at_ms).max();
    if !active {
        slots.remove(server_dir);
    }
    ProviderVerificationForegroundDemand {
        active,
        active_consumers,
        generation,
        expires_at_ms,
        held_reason: active.then(|| "foreground_navigation_job_or_probe_demand".to_string()),
        resource_policy: provider_verification_resource_policy(active).to_string(),
    }
}

/// Register or clear a short foreground-demand lease for navigation, job start, or probes.
/// The lease expires automatically if the WebView disappears, and stale clears cannot
/// release a newer generation. This changes only provider-verification checkpoints.
pub fn set_youtube_po_provider_verification_foreground_demand(
    paths: &AppPaths,
    consumer_id: &str,
    generation: u64,
    active: bool,
) -> ProviderVerificationForegroundDemand {
    let server_dir = paths.youtube_po_provider_server_dir();
    let now = now_ms();
    let consumer_id = consumer_id.trim();
    if !consumer_id.is_empty() {
        let mut slots = provider_verification_foreground_slots()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let leases = slots.entry(server_dir.clone()).or_default();
        leases.retain(|_, lease| lease.expires_at_ms > now);
        let current_generation = leases
            .get(consumer_id)
            .map(|lease| lease.generation)
            .unwrap_or(0);
        if generation >= current_generation {
            if active {
                leases.insert(
                    consumer_id.to_string(),
                    ProviderVerificationForegroundLease {
                        generation,
                        expires_at_ms: now
                            .saturating_add(PROVIDER_VERIFICATION_FOREGROUND_LEASE_MS),
                    },
                );
            } else {
                leases.remove(consumer_id);
            }
        }
        if leases.is_empty() {
            slots.remove(&server_dir);
        }
    }
    provider_verification_foreground_demand_for_key(&server_dir, generation)
}

fn provider_verification_progress_slots(
) -> &'static Mutex<HashMap<PathBuf, ProviderVerificationProgress>> {
    static PROGRESS: OnceLock<Mutex<HashMap<PathBuf, ProviderVerificationProgress>>> =
        OnceLock::new();
    PROGRESS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn youtube_po_provider_verification_progress(
    paths: &AppPaths,
) -> Option<ProviderVerificationProgress> {
    provider_verification_progress_slots()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&paths.youtube_po_provider_server_dir())
        .cloned()
}

fn begin_provider_verification_progress(paths: &AppPaths) {
    let now = now_ms();
    let server_dir = paths.youtube_po_provider_server_dir();
    let previous = provider_verification_progress_slots()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&server_dir)
        .cloned();
    let previous_revision = previous
        .as_ref()
        .map(|progress| progress.revision)
        .unwrap_or(0);
    let scan_count = previous
        .as_ref()
        .map(|progress| progress.scan_count)
        .unwrap_or(0);
    let installed_identity = load_provider_installed_identity(paths).ok().flatten();
    let source_identity = format!(
        "generation={}|root={}|directory={}|commit={}",
        provider_install_generation(),
        paths.youtube_po_provider_dir().to_string_lossy(),
        provider_directory_identity(&paths.youtube_po_provider_dir())
            .unwrap_or_else(|_| "unavailable".to_string()),
        installed_identity
            .as_ref()
            .map(|identity| identity.commit_nonce.as_str())
            .unwrap_or("unbound"),
    );
    provider_verification_progress_slots()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(
            server_dir,
            ProviderVerificationProgress {
                schema_version: 1,
                semantic_key: "youtube_po_provider_tree_verify".to_string(),
                source_identity,
                phase: "provider_manifest_load".to_string(),
                state: "running".to_string(),
                revision: previous_revision.saturating_add(1),
                files_completed: 0,
                files_planned: None,
                bytes_completed: 0,
                bytes_planned: None,
                scan_count,
                started_at_ms: now,
                updated_at_ms: now,
                finished_at_ms: None,
                error: None,
                foreground_pressure_active: false,
                held_reason: None,
                resource_policy: PROVIDER_VERIFICATION_BACKGROUND_POLICY.to_string(),
            },
        );
}

fn mark_provider_verification_scan_started(server_dir: &Path) {
    let mut slots = provider_verification_progress_slots()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(progress) = slots.get_mut(server_dir) {
        progress.scan_count = progress.scan_count.saturating_add(1);
        progress.revision = progress.revision.saturating_add(1);
        progress.updated_at_ms = now_ms();
    }
}

fn update_provider_verification_progress(
    server_dir: &Path,
    phase: &str,
    files_completed: u64,
    bytes_completed: u64,
) {
    let demand = provider_verification_foreground_demand_for_key(server_dir, 0);
    let mut slots = provider_verification_progress_slots()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(progress) = slots.get_mut(server_dir) {
        progress.phase = phase.to_string();
        progress.files_completed = files_completed;
        progress.bytes_completed = bytes_completed;
        progress.updated_at_ms = now_ms();
        progress.revision = progress.revision.saturating_add(1);
        progress.foreground_pressure_active = demand.active;
        progress.held_reason = demand.held_reason;
        progress.resource_policy = demand.resource_policy;
    }
}

fn finish_provider_verification_progress(paths: &AppPaths, error: Option<String>) {
    let server_dir = paths.youtube_po_provider_server_dir();
    let installed_identity = load_provider_installed_identity(paths).ok().flatten();
    let terminal_source_identity = format!(
        "generation={}|root={}|directory={}|commit={}",
        provider_install_generation(),
        paths.youtube_po_provider_dir().to_string_lossy(),
        provider_directory_identity(&paths.youtube_po_provider_dir())
            .unwrap_or_else(|_| "unavailable".to_string()),
        installed_identity
            .as_ref()
            .map(|identity| identity.commit_nonce.as_str())
            .unwrap_or("unbound"),
    );
    let mut slots = provider_verification_progress_slots()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(progress) = slots.get_mut(&server_dir) {
        let now = now_ms();
        progress.phase = "provider_attestation_publish".to_string();
        let succeeded = error.is_none();
        progress.source_identity = terminal_source_identity;
        progress.state = if succeeded { "ready" } else { "error" }.to_string();
        progress.error = error;
        progress.updated_at_ms = now;
        progress.finished_at_ms = Some(now);
        // A single-pass tree walk does not know its final totals before it finishes. Only a
        // successful terminal scan may promote the observed totals to exact planned totals. An
        // interrupted or early-failing scan must remain visibly incomplete/unknown.
        if succeeded {
            progress.files_planned = Some(progress.files_completed);
            progress.bytes_planned = Some(progress.bytes_completed);
        }
        progress.revision = progress.revision.saturating_add(1);
        progress.foreground_pressure_active = false;
        progress.held_reason = None;
        progress.resource_policy = "verification_complete".to_string();
    }
}

fn provider_node_modules_process_attestations(
) -> &'static std::sync::Mutex<HashMap<PathBuf, ProviderNodeModulesProcessAttestation>> {
    static ATTESTATIONS: std::sync::OnceLock<
        std::sync::Mutex<HashMap<PathBuf, ProviderNodeModulesProcessAttestation>>,
    > = std::sync::OnceLock::new();
    ATTESTATIONS.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

fn provider_node_modules_process_invalidations(
) -> &'static std::sync::Mutex<HashMap<PathBuf, String>> {
    static INVALIDATIONS: std::sync::OnceLock<std::sync::Mutex<HashMap<PathBuf, String>>> =
        std::sync::OnceLock::new();
    INVALIDATIONS.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

fn provider_verification_terminal_errors(
) -> &'static std::sync::Mutex<HashMap<PathBuf, (String, String)>> {
    static ERRORS: std::sync::OnceLock<std::sync::Mutex<HashMap<PathBuf, (String, String)>>> =
        std::sync::OnceLock::new();
    ERRORS.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

fn provider_node_modules_integrity_receipt_path(server_dir: &Path) -> PathBuf {
    server_dir.join(".node_modules_integrity.json")
}

fn provider_install_generation() -> String {
    use sha2::Digest;
    let manifest = pinned_dependency_manifest::manifest();
    let provider = &manifest.youtube_po_provider;
    let node = &manifest.node_windows;
    let payload = format!(
        "node={}|node_sha={}|npm={}|npm_sha={}|node_tree={}|plugin={}|plugin_tree={}|server={}|lock={}|node_modules={}|provider_tree={}",
        node.version,
        node.node_exe_sha256_hex,
        node.npm_version,
        node.npm_cmd_sha256_hex,
        node.complete_tree_sha256_hex,
        provider.plugin_sha256_hex,
        provider.plugin_tree_sha256_hex,
        provider.server_entrypoint_sha256_hex,
        provider.derived_lock_sha256_hex,
        provider.node_modules_tree_sha256_hex,
        provider.application_complete_tree_sha256_hex,
    );
    hex::encode_upper(sha2::Sha256::digest(payload.as_bytes()))
}

fn read_provider_node_modules_integrity_receipt(
    server_dir: &Path,
) -> Option<ProviderNodeModulesIntegrityReceipt> {
    let bytes = std::fs::read(provider_node_modules_integrity_receipt_path(server_dir)).ok()?;
    let receipt: ProviderNodeModulesIntegrityReceipt = serde_json::from_slice(&bytes).ok()?;
    (receipt.schema_version == 1 && receipt.install_generation == provider_install_generation())
        .then_some(receipt)
}

fn provider_node_modules_process_attestation(
    server_dir: &Path,
) -> Option<ProviderNodeModulesProcessAttestation> {
    provider_node_modules_process_attestations()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(server_dir)
        .filter(|attestation| attestation.install_generation == provider_install_generation())
        .cloned()
}

fn clear_provider_node_modules_process_attestation(server_dir: &Path) {
    provider_node_modules_process_attestations()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .remove(server_dir);
}

fn attest_provider_node_modules_tree(
    server_dir: &Path,
    tree_sha256_hex: &str,
) -> Result<ProviderNodeModulesProcessAttestation> {
    let expected = &pinned_dependency_manifest::manifest()
        .youtube_po_provider
        .node_modules_tree_sha256_hex;
    if !tree_sha256_hex.eq_ignore_ascii_case(expected) {
        return Err(EngineError::HashMismatch {
            path: server_dir.join("node_modules"),
            expected: expected.clone(),
            actual: tree_sha256_hex.to_string(),
        });
    }
    let attestation = ProviderNodeModulesProcessAttestation {
        install_generation: provider_install_generation(),
        tree_sha256_hex: tree_sha256_hex.to_ascii_uppercase(),
        verified_at_ms: now_ms(),
    };
    write_provider_node_modules_integrity_receipt(server_dir, &attestation.tree_sha256_hex)?;
    provider_node_modules_process_attestations()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(server_dir.to_path_buf(), attestation.clone());
    provider_node_modules_process_invalidations()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .remove(server_dir);
    Ok(attestation)
}

fn write_provider_node_modules_integrity_receipt(
    server_dir: &Path,
    tree_sha256_hex: &str,
) -> Result<()> {
    let receipt = ProviderNodeModulesIntegrityReceipt {
        schema_version: 1,
        install_generation: provider_install_generation(),
        tree_sha256_hex: tree_sha256_hex.to_ascii_uppercase(),
        verified_at_ms: now_ms(),
    };
    Ok(crate::persistence::atomic_write_text(
        &provider_node_modules_integrity_receipt_path(server_dir),
        &serde_json::to_string_pretty(&receipt)?,
    )?)
}

fn provider_node_modules_integrity_verifying() -> &'static std::sync::atomic::AtomicBool {
    static VERIFYING: std::sync::OnceLock<std::sync::atomic::AtomicBool> =
        std::sync::OnceLock::new();
    VERIFYING.get_or_init(|| std::sync::atomic::AtomicBool::new(false))
}

struct ProviderIntegrityVerificationGuard;

impl Drop for ProviderIntegrityVerificationGuard {
    fn drop(&mut self) {
        provider_node_modules_integrity_verifying()
            .store(false, std::sync::atomic::Ordering::Release);
    }
}

/// Performs the expensive authoritative installed-byte verification. Hot status polling only
/// reads the current process's in-memory attestation; startup/offline hydration and explicit
/// capability probes call this function on a background/blocking worker.
pub fn verify_youtube_po_provider_node_modules(
    paths: &AppPaths,
) -> Result<YoutubePoProviderInstallStatus> {
    paths.ensure_dirs()?;
    let server_dir = paths.youtube_po_provider_server_dir();
    let lifecycle = youtube_po_provider_lifecycle_lock();
    let (_lifecycle_guard, waited_for_active_flight) = match lifecycle.try_lock() {
        Ok(guard) => (guard, false),
        Err(std::sync::TryLockError::WouldBlock) => (
            lifecycle
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            true,
        ),
        Err(std::sync::TryLockError::Poisoned(error)) => (error.into_inner(), false),
    };
    verify_youtube_po_provider_node_modules_single_flight_locked(
        paths,
        &server_dir,
        waited_for_active_flight,
    )
}

fn verify_youtube_po_provider_node_modules_single_flight_locked(
    paths: &AppPaths,
    server_dir: &Path,
    waited_for_active_flight: bool,
) -> Result<YoutubePoProviderInstallStatus> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        verify_youtube_po_provider_node_modules_single_flight_inner(
            paths,
            server_dir,
            waited_for_active_flight,
        )
    })) {
        Ok(result) => result,
        Err(_) => {
            let shared_error = remember_provider_verification_terminal_error(
                server_dir,
                provider_install_generation(),
                EngineError::InstallFailed(
                    "provider verification worker panicked before producing a terminal receipt"
                        .to_string(),
                ),
            );
            finish_provider_verification_progress(paths, Some(shared_error.to_string()));
            Err(shared_error)
        }
    }
}

fn verify_youtube_po_provider_node_modules_single_flight_inner(
    paths: &AppPaths,
    server_dir: &Path,
    waited_for_active_flight: bool,
) -> Result<YoutubePoProviderInstallStatus> {
    let generation = provider_install_generation();
    if waited_for_active_flight {
        if provider_node_modules_process_attestation(server_dir).is_some() {
            // The producer owns the canonical progress receipt. A waiter consumes the same
            // successful terminal without publishing a synthetic second scan.
            return Ok(youtube_po_provider_install_status(paths));
        }
        if let Some((terminal_generation, error)) = provider_verification_terminal_errors()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(server_dir)
            .cloned()
        {
            if terminal_generation == generation {
                return Err(EngineError::InstallFailed(error));
            }
        }
    }
    provider_verification_terminal_errors()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .remove(server_dir);
    let _interprocess_guard = match acquire_youtube_po_provider_install_interprocess_lock(
        paths,
        YOUTUBE_PO_PROVIDER_INSTALL_LOCK_TIMEOUT_MS,
    ) {
        Ok(guard) => guard,
        Err(error) => {
            return Err(remember_provider_verification_terminal_error(
                server_dir, generation, error,
            ));
        }
    };
    match verify_youtube_po_provider_node_modules_locked(paths) {
        Ok(status) => Ok(status),
        Err(error) => Err(remember_provider_verification_terminal_error(
            server_dir, generation, error,
        )),
    }
}

#[cfg(test)]
fn provider_verification_injected_panic() -> &'static std::sync::atomic::AtomicBool {
    static PANIC_ONCE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    &PANIC_ONCE
}

fn remember_provider_verification_terminal_error(
    server_dir: &Path,
    generation: String,
    error: EngineError,
) -> EngineError {
    let shared_error = match error {
        EngineError::InstallFailed(message) => message,
        other => other.to_string(),
    };
    provider_verification_terminal_errors()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(server_dir.to_path_buf(), (generation, shared_error.clone()));
    EngineError::InstallFailed(shared_error)
}

fn require_exact_committed_provider_identity_lineage(
    paths: &AppPaths,
    identity: &ProviderInstalledIdentity,
) -> Result<()> {
    if identity.lineage_attempt_id.is_empty() || identity.commit_nonce.is_empty() {
        return Err(EngineError::InstallFailed(
            "provider installed identity is not bound to v48 committed lineage".to_string(),
        ));
    }
    let conn = crate::db::open_readonly(paths)?;
    let bound: i64 = conn.query_row(
        "SELECT COUNT(*) FROM provider_install_lineage lineage
         WHERE lineage.attempt_id=?1 AND lineage.commit_nonce=?2
           AND lineage.phase='committed' AND lineage.install_generation=?3
           AND lineage.node_directory_identity=?4
           AND lineage.provider_directory_identity=?5
           AND lineage.node_tree_sha256=?6 AND lineage.provider_tree_sha256=?7",
        rusqlite::params![
            identity.lineage_attempt_id,
            identity.commit_nonce,
            identity.install_generation,
            identity.node_directory_identity,
            identity.provider_directory_identity,
            identity.node_tree_sha256,
            identity.provider_tree_sha256,
        ],
        |row| row.get(0),
    )?;
    if bound != 1 {
        return Err(EngineError::InstallFailed(
            "provider installed identity has no exact committed v48 lineage".to_string(),
        ));
    }
    Ok(())
}

fn authenticate_stored_managed_provider_identity_at(
    paths: &AppPaths,
    identity: &ProviderInstalledIdentity,
    node_root: &Path,
    provider_root: &Path,
) -> Result<()> {
    require_exact_committed_provider_identity_lineage(paths, identity)?;
    verify_published_directory_lineage(
        node_root,
        &identity.node_directory_identity,
        &identity.node_tree_sha256,
        canonical_provider_node_tree_sha256_hex,
        "managed Node",
    )?;
    verify_published_directory_lineage(
        provider_root,
        &identity.provider_directory_identity,
        &identity.provider_tree_sha256,
        canonical_provider_application_tree_sha256_hex,
        "managed provider",
    )
}

fn authenticate_authoritative_installed_provider_identity(
    paths: &AppPaths,
    progress_key: Option<&Path>,
) -> Result<()> {
    let identity = load_provider_installed_identity(paths)?.ok_or_else(|| {
        EngineError::InstallFailed(
            "provider payload has no authoritative committed complete-tree identity".to_string(),
        )
    })?;
    require_exact_committed_provider_identity_lineage(paths, &identity)?;
    if identity.install_generation != provider_install_generation() {
        return Err(EngineError::InstallFailed(
            "provider committed identity belongs to a different pinned install generation"
                .to_string(),
        ));
    }
    let manifest = pinned_dependency_manifest::manifest();
    if !identity
        .node_tree_sha256
        .eq_ignore_ascii_case(&manifest.node_windows.complete_tree_sha256_hex)
        || !identity.provider_tree_sha256.eq_ignore_ascii_case(
            &manifest
                .youtube_po_provider
                .application_complete_tree_sha256_hex,
        )
    {
        return Err(EngineError::InstallFailed(
            "provider committed identity does not match the executable-pinned complete trees"
                .to_string(),
        ));
    }
    verify_published_directory_lineage(
        &paths.node_runtime_dir(),
        &identity.node_directory_identity,
        &identity.node_tree_sha256,
        canonical_provider_node_tree_sha256_hex,
        "Node",
    )?;
    let provider_root = paths.youtube_po_provider_dir();
    let actual_directory_identity = provider_directory_identity(&provider_root)?;
    if actual_directory_identity != identity.provider_directory_identity {
        return Err(EngineError::InstallFailed(
            "provider published directory is a different filesystem object than the sealed staging directory".to_string(),
        ));
    }
    let actual_tree = match progress_key {
        Some(key) => {
            canonical_provider_application_tree_sha256_hex_with_progress(&provider_root, key)
        }
        None => canonical_provider_application_tree_sha256_hex(&provider_root),
    }
    .ok_or_else(|| {
        EngineError::InstallFailed(
            "provider complete published tree could not be authenticated".to_string(),
        )
    })?;
    if !actual_tree.eq_ignore_ascii_case(&identity.provider_tree_sha256) {
        return Err(EngineError::HashMismatch {
            path: provider_root,
            expected: identity.provider_tree_sha256,
            actual: actual_tree,
        });
    }
    Ok(())
}

fn authenticate_embedded_complete_provider_payload(
    paths: &AppPaths,
) -> Result<ProviderInstalledIdentity> {
    authenticate_embedded_complete_provider_payload_with_progress(paths, None)
}

fn authenticate_embedded_complete_provider_payload_with_progress(
    paths: &AppPaths,
    progress_key: Option<&Path>,
) -> Result<ProviderInstalledIdentity> {
    authenticate_published_node_payload(paths)?;
    authenticate_published_provider_payload(paths)?;
    let manifest = pinned_dependency_manifest::manifest();
    authenticate_complete_provider_trees_against_with_progress(
        paths,
        &manifest.node_windows.complete_tree_sha256_hex,
        &manifest
            .youtube_po_provider
            .application_complete_tree_sha256_hex,
        progress_key,
    )
}

fn authenticate_complete_provider_trees_against(
    paths: &AppPaths,
    expected_node_tree_sha256: &str,
    expected_provider_tree_sha256: &str,
) -> Result<ProviderInstalledIdentity> {
    authenticate_complete_provider_trees_against_with_progress(
        paths,
        expected_node_tree_sha256,
        expected_provider_tree_sha256,
        None,
    )
}

fn authenticate_complete_provider_trees_against_with_progress(
    paths: &AppPaths,
    expected_node_tree_sha256: &str,
    expected_provider_tree_sha256: &str,
    progress_key: Option<&Path>,
) -> Result<ProviderInstalledIdentity> {
    let node_tree_sha256 = canonical_provider_node_tree_sha256_hex(&paths.node_runtime_dir())
        .ok_or_else(|| {
            EngineError::InstallFailed(
                "offline provider Node tree could not be completely authenticated".to_string(),
            )
        })?;
    if !node_tree_sha256.eq_ignore_ascii_case(expected_node_tree_sha256) {
        return Err(EngineError::HashMismatch {
            path: paths.node_runtime_dir(),
            expected: expected_node_tree_sha256.to_string(),
            actual: node_tree_sha256,
        });
    }
    let provider_root = paths.youtube_po_provider_dir();
    let provider_tree_sha256 = match progress_key {
        Some(key) => {
            canonical_provider_application_tree_sha256_hex_with_progress(&provider_root, key)
        }
        None => canonical_provider_application_tree_sha256_hex(&provider_root),
    }
    .ok_or_else(|| {
        EngineError::InstallFailed(
            "offline provider application tree could not be completely authenticated".to_string(),
        )
    })?;
    if !provider_tree_sha256.eq_ignore_ascii_case(expected_provider_tree_sha256) {
        return Err(EngineError::HashMismatch {
            path: paths.youtube_po_provider_dir(),
            expected: expected_provider_tree_sha256.to_string(),
            actual: provider_tree_sha256,
        });
    }
    Ok(ProviderInstalledIdentity {
        lineage_attempt_id: String::new(),
        commit_nonce: String::new(),
        install_generation: provider_install_generation(),
        node_directory_identity: provider_directory_identity(&paths.node_runtime_dir())?,
        provider_directory_identity: provider_directory_identity(&paths.youtube_po_provider_dir())?,
        node_tree_sha256: expected_node_tree_sha256.to_ascii_uppercase(),
        provider_tree_sha256: expected_provider_tree_sha256.to_ascii_uppercase(),
    })
}

fn provider_portable_attestation_path(paths: &AppPaths) -> PathBuf {
    paths
        .tools_dir()
        .join(".youtube_po_provider_portable_attestation.json")
}

/// Writes an audit-only carrier after authenticating the final trees against roots compiled into
/// the executable. Runtime adoption never consumes this editable JSON as authority.
pub fn write_youtube_po_provider_portable_attestation(paths: &AppPaths) -> Result<()> {
    let verified = authenticate_embedded_complete_provider_payload(paths)?;
    let carrier = ProviderPortableAttestation {
        schema_version: 1,
        install_generation: verified.install_generation,
        node_complete_tree_sha256: verified.node_tree_sha256,
        provider_complete_tree_sha256: verified.provider_tree_sha256,
    };
    crate::persistence::atomic_write_text(
        &provider_portable_attestation_path(paths),
        &format!("{}\n", serde_json::to_string_pretty(&carrier)?),
    )?;
    Ok(())
}

fn adopt_embedded_complete_provider_payload(paths: &AppPaths) -> Result<()> {
    adopt_embedded_complete_provider_payload_with_progress(paths, None)
}

fn adopt_embedded_complete_provider_payload_with_progress(
    paths: &AppPaths,
    progress_key: Option<&Path>,
) -> Result<()> {
    let verified =
        authenticate_embedded_complete_provider_payload_with_progress(paths, progress_key)?;
    commit_adopted_provider_identity(paths, verified)
}

fn commit_adopted_provider_identity(
    paths: &AppPaths,
    verified: ProviderInstalledIdentity,
) -> Result<()> {
    #[cfg(test)]
    crate::db::ensure_schema(paths)?;
    if let Some(existing) = load_provider_installed_identity(paths)? {
        let legacy_unbound =
            existing.lineage_attempt_id.is_empty() && existing.commit_nonce.is_empty();
        if !legacy_unbound {
            if existing.install_generation == verified.install_generation
                && existing.node_directory_identity == verified.node_directory_identity
                && existing.provider_directory_identity == verified.provider_directory_identity
                && existing
                    .node_tree_sha256
                    .eq_ignore_ascii_case(&verified.node_tree_sha256)
                && existing
                    .provider_tree_sha256
                    .eq_ignore_ascii_case(&verified.provider_tree_sha256)
            {
                return Ok(());
            }
            return Err(EngineError::InstallFailed(
                "provider installed identity conflicts with the authenticated destination bytes"
                    .to_string(),
            ));
        }
        if existing.install_generation != verified.install_generation
            || existing.node_directory_identity != verified.node_directory_identity
            || existing.provider_directory_identity != verified.provider_directory_identity
            || !existing
                .node_tree_sha256
                .eq_ignore_ascii_case(&verified.node_tree_sha256)
            || !existing
                .provider_tree_sha256
                .eq_ignore_ascii_case(&verified.provider_tree_sha256)
        {
            return Err(EngineError::InstallFailed(
                "legacy provider identity does not match the executable-pinned destination bytes"
                    .to_string(),
            ));
        }
    }

    let attempt_id = uuid::Uuid::new_v4().to_string();
    let commit_nonce = random_provider_authority_nonce();
    let ownership_token_digest = provider_ownership_token_digest(&format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    ));
    let stage_root = paths
        .tools_dir()
        .join(format!("youtube_po_provider_stage_{attempt_id}"));
    let current_pid = std::process::id();
    let current_process_identity = provider_process_identity(current_pid).ok_or_else(|| {
        EngineError::InstallFailed(
            "could not establish offline provider adoption process identity".to_string(),
        )
    })?;
    let mut conn = crate::db::write_context(paths)?;
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let owners: i64 = tx.query_row("SELECT COUNT(*) FROM provider_install_owner", [], |row| {
        row.get(0)
    })?;
    let unresolved: i64 = tx.query_row(
        "SELECT COUNT(*) FROM provider_install_lineage lineage
         WHERE NOT EXISTS(
           SELECT 1 FROM provider_installed_identity identity
           WHERE identity.singleton=1
             AND identity.lineage_attempt_id=lineage.attempt_id
             AND identity.commit_nonce=lineage.commit_nonce
             AND lineage.phase='committed'
         )",
        [],
        |row| row.get(0),
    )?;
    if owners != 0 || unresolved != 0 {
        return Err(EngineError::InstallFailed(
            "offline provider adoption refused active or unresolved install lineage".to_string(),
        ));
    }
    let timestamp = now_ms();
    tx.execute(
        "INSERT INTO provider_install_lineage(
           attempt_id,stage_root,phase,updated_at_ms,ownership_token_digest,commit_nonce,install_generation
         ) VALUES(?1,?2,'prepared',?3,?4,?5,?6)",
        rusqlite::params![
            attempt_id,
            stage_root.to_string_lossy(),
            timestamp,
            ownership_token_digest,
            commit_nonce,
            verified.install_generation,
        ],
    )?;
    tx.execute(
        "INSERT INTO provider_install_owner(
           singleton,attempt_id,acquired_at_ms,updated_at_ms,owner_pid,owner_process_identity,commit_nonce
         ) VALUES(1,?1,?2,?2,?3,?4,?5)",
        rusqlite::params![
            attempt_id,
            timestamp,
            current_pid,
            current_process_identity,
            commit_nonce,
        ],
    )?;
    tx.execute(
        "UPDATE provider_install_lineage SET
           node_directory_identity=?1,provider_directory_identity=?2,
           node_tree_sha256=?3,provider_tree_sha256=?4
         WHERE attempt_id=?5 AND phase='prepared'",
        rusqlite::params![
            verified.node_directory_identity,
            verified.provider_directory_identity,
            verified.node_tree_sha256,
            verified.provider_tree_sha256,
            attempt_id,
        ],
    )?;
    for (before, after) in [
        ("prepared", "node_publish_intent"),
        ("node_publish_intent", "node_published"),
        ("node_published", "provider_publish_intent"),
        ("provider_publish_intent", "provider_published"),
        ("provider_published", "committed"),
    ] {
        let changed = tx.execute(
            "UPDATE provider_install_lineage SET phase=?1,updated_at_ms=?2
             WHERE attempt_id=?3 AND phase=?4",
            rusqlite::params![after, now_ms(), attempt_id, before],
        )?;
        if changed != 1 {
            return Err(EngineError::InstallFailed(
                "offline provider adoption lost its legal lineage transition".to_string(),
            ));
        }
    }
    tx.execute(
        "INSERT INTO provider_installed_identity(
           singleton,lineage_attempt_id,commit_nonce,install_generation,
           node_directory_identity,provider_directory_identity,node_tree_sha256,
           provider_tree_sha256,committed_at_ms
         ) VALUES(1,?1,?2,?3,?4,?5,?6,?7,?8)
         ON CONFLICT(singleton) DO UPDATE SET
           lineage_attempt_id=excluded.lineage_attempt_id,
           commit_nonce=excluded.commit_nonce,
           install_generation=excluded.install_generation,
           node_directory_identity=excluded.node_directory_identity,
           provider_directory_identity=excluded.provider_directory_identity,
           node_tree_sha256=excluded.node_tree_sha256,
           provider_tree_sha256=excluded.provider_tree_sha256,
           committed_at_ms=excluded.committed_at_ms",
        rusqlite::params![
            attempt_id,
            commit_nonce,
            verified.install_generation,
            verified.node_directory_identity,
            verified.provider_directory_identity,
            verified.node_tree_sha256,
            verified.provider_tree_sha256,
            now_ms(),
        ],
    )?;
    tx.execute(
        "DELETE FROM provider_install_owner
         WHERE singleton=1 AND attempt_id=?1 AND commit_nonce=?2",
        rusqlite::params![attempt_id, commit_nonce],
    )?;
    tx.execute(
        "DELETE FROM provider_install_lineage
         WHERE phase='committed' AND attempt_id<>?1
           AND NOT EXISTS(
             SELECT 1 FROM provider_installed_identity identity
             WHERE identity.lineage_attempt_id=provider_install_lineage.attempt_id
               AND identity.commit_nonce=provider_install_lineage.commit_nonce
           )",
        [attempt_id.as_str()],
    )?;
    tx.commit()?;
    Ok(())
}

fn reconcile_provider_lineage_before_verification(paths: &AppPaths) -> Result<()> {
    if let Some(lineage) = load_provider_install_lineage(paths)? {
        let owner_is_live = provider_process_identity(lineage.owner_pid).as_deref()
            == Some(lineage.owner_process_identity.as_str());
        if owner_is_live {
            return Err(EngineError::InstallFailed(
                "provider verification refused while an install owner is still alive".to_string(),
            ));
        }
        if lineage.phase != "committed" {
            return Err(EngineError::InstallFailed(format!(
                "provider payload is not launchable while install lineage is in {} phase",
                lineage.phase
            )));
        }
        reconcile_interrupted_provider_install(paths)?;
    }
    if load_provider_install_lineage(paths)?.is_some() {
        return Err(EngineError::InstallFailed(
            "provider install lineage was not authoritatively cleared".to_string(),
        ));
    }
    let progress_key = paths.youtube_po_provider_server_dir();
    match authenticate_authoritative_installed_provider_identity(paths, Some(&progress_key)) {
        Ok(()) => Ok(()),
        Err(error) => {
            let legacy_or_absent =
                load_provider_installed_identity(paths)?.is_none_or(|identity| {
                    identity.lineage_attempt_id.is_empty() && identity.commit_nonce.is_empty()
                });
            if !legacy_or_absent {
                return Err(error);
            }
            // The adoption transaction commits the exact complete-tree identity authenticated by
            // the single progress-aware pass above. Rewalking the same provider bytes here would
            // make first-run verification two full scans and invalidate the producer receipt.
            adopt_embedded_complete_provider_payload_with_progress(paths, Some(&progress_key))
        }
    }
}

fn verify_youtube_po_provider_node_modules_locked(
    paths: &AppPaths,
) -> Result<YoutubePoProviderInstallStatus> {
    begin_provider_verification_progress(paths);
    #[cfg(test)]
    if provider_verification_injected_panic().swap(false, std::sync::atomic::Ordering::SeqCst) {
        mark_provider_verification_scan_started(&paths.youtube_po_provider_server_dir());
        std::thread::sleep(std::time::Duration::from_millis(100));
        panic!("injected provider verification panic after progress started");
    }
    let result = verify_youtube_po_provider_node_modules_inner(paths);
    finish_provider_verification_progress(
        paths,
        result.as_ref().err().map(std::string::ToString::to_string),
    );
    result
}

fn verify_youtube_po_provider_node_modules_inner(
    paths: &AppPaths,
) -> Result<YoutubePoProviderInstallStatus> {
    let server_dir = paths.youtube_po_provider_server_dir();
    clear_provider_node_modules_process_attestation(&server_dir);
    update_provider_verification_progress(&server_dir, "provider_manifest_load", 0, 0);
    if let Err(error) = reconcile_provider_lineage_before_verification(paths) {
        let _ = std::fs::remove_file(provider_node_modules_integrity_receipt_path(&server_dir));
        provider_node_modules_process_invalidations()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(server_dir, error.to_string());
        return Err(error);
    }
    if provider_node_modules_integrity_verifying()
        .compare_exchange(
            false,
            true,
            std::sync::atomic::Ordering::AcqRel,
            std::sync::atomic::Ordering::Acquire,
        )
        .is_err()
    {
        return Err(EngineError::InstallFailed(
            "provider dependency integrity verification is already running".to_string(),
        ));
    }
    let _guard = ProviderIntegrityVerificationGuard;
    let expected = &pinned_dependency_manifest::manifest()
        .youtube_po_provider
        .node_modules_tree_sha256_hex;
    // The complete provider-tree authentication above includes every server/node_modules byte.
    // Rewalking that child tree would double the dominant startup I/O. A successful match to the
    // pinned complete-tree digest therefore publishes its pinned child digest directly.
    attest_provider_node_modules_tree(&server_dir, expected)?;
    Ok(youtube_po_provider_install_status(paths))
}

fn provider_plugin_tree_sha256_hex(
    root: &Path,
    expected_files: &std::collections::BTreeMap<String, String>,
) -> Option<String> {
    use sha2::Digest;
    let mut actual = std::collections::BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).ok()? {
            let entry = entry.ok()?;
            let path = entry.path();
            let metadata = std::fs::symlink_metadata(&path).ok()?;
            if metadata.file_type().is_symlink() || provider_metadata_is_reparse_point(&metadata) {
                return None;
            }
            if metadata.is_dir() {
                stack.push(path);
                continue;
            }
            if !metadata.is_file() {
                return None;
            }
            let relative = path
                .strip_prefix(root)
                .ok()?
                .to_string_lossy()
                .replace('\\', "/");
            if relative == ".plugin_archive_sha256" {
                continue;
            }
            actual.insert(relative, file_sha256_hex(&path)?);
        }
    }
    if actual.len() != expected_files.len()
        || actual.iter().any(|(path, hash)| {
            expected_files
                .get(path)
                .map(|expected| !hash.eq_ignore_ascii_case(expected))
                .unwrap_or(true)
        })
    {
        return None;
    }
    let mut hasher = sha2::Sha256::new();
    for (path, hash) in actual {
        hasher.update(path.as_bytes());
        hasher.update([0]);
        hasher.update(hash.as_bytes());
        hasher.update(b"\n");
    }
    Some(hex::encode_upper(hasher.finalize()))
}

fn file_sha256_hex(path: &Path) -> Option<String> {
    sha256_file(path).ok().map(hex::encode_upper)
}

/// Hash an installed dependency tree by normalized relative path and complete file bytes.
/// File metadata is deliberately excluded so restores and reproducible installs remain stable,
/// while same-size byte replacement is still detected. Symlinks/reparse-style file links are
/// rejected rather than followed because the production provider must be self-contained.
const PROVIDER_NODE_TREE_EXCLUSIONS: &[&str] = &[".voxvulgi_provider_install_attempt"];
const PROVIDER_APPLICATION_TREE_EXCLUSIONS: &[&str] = &[
    ".voxvulgi_provider_install_attempt",
    "server/.node_modules_integrity.json",
];

fn canonical_directory_tree_sha256_hex_with_exclusions(
    root: &Path,
    exact_excluded_files: &[&str],
) -> Option<String> {
    canonical_directory_tree_sha256_hex_with_exclusions_and_progress(
        root,
        exact_excluded_files,
        None,
    )
}

fn canonical_directory_tree_sha256_hex_with_exclusions_and_progress(
    root: &Path,
    exact_excluded_files: &[&str],
    mut progress: Option<&mut dyn FnMut(u64, u64)>,
) -> Option<String> {
    use sha2::Digest;
    const MAX_FILES: usize = 12_000;
    const MAX_TOTAL_BYTES: u64 = 512 * 1024 * 1024;
    const MAX_FILE_BYTES: u64 = 128 * 1024 * 1024;
    const MAX_DEPTH: usize = 32;
    const MAX_ELAPSED: std::time::Duration = std::time::Duration::from_secs(10 * 60);
    let started = std::time::Instant::now();
    let mut total_bytes = 0_u64;
    let mut files = std::collections::BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        for entry in std::fs::read_dir(&directory).ok()? {
            if started.elapsed() > MAX_ELAPSED {
                return None;
            }
            let entry = entry.ok()?;
            let path = entry.path();
            let metadata = std::fs::symlink_metadata(&path).ok()?;
            if metadata.file_type().is_symlink() || provider_metadata_is_reparse_point(&metadata) {
                return None;
            }
            if metadata.is_dir() {
                stack.push(path);
                continue;
            }
            if !metadata.is_file() {
                return None;
            }
            let relative = path
                .strip_prefix(root)
                .ok()?
                .to_string_lossy()
                .replace('\\', "/");
            if exact_excluded_files.contains(&relative.as_str()) {
                continue;
            }
            if relative.split('/').count() > MAX_DEPTH
                || metadata.len() > MAX_FILE_BYTES
                || files.len() >= MAX_FILES
            {
                return None;
            }
            total_bytes = total_bytes.checked_add(metadata.len())?;
            if total_bytes > MAX_TOTAL_BYTES {
                return None;
            }
            files.insert(relative, file_sha256_hex(&path)?);
            let files_completed = files.len() as u64;
            if let Some(callback) = progress.as_deref_mut() {
                callback(files_completed, total_bytes);
            }
            // Bound continuous filesystem/AV pressure without weakening complete-byte hashing.
            // The current process remains interactive and the single-flight retains ownership.
            if files_completed % 32 == 0 {
                std::thread::yield_now();
            }
            if files_completed % 256 == 0 {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }
    }
    if files.is_empty() {
        return None;
    }
    let mut hasher = sha2::Sha256::new();
    for (path, hash) in files {
        hasher.update(path.as_bytes());
        hasher.update([0]);
        hasher.update(hash.as_bytes());
        hasher.update(b"\n");
    }
    Some(hex::encode_upper(hasher.finalize()))
}

#[cfg(windows)]
fn provider_metadata_is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn provider_metadata_is_reparse_point(_metadata: &std::fs::Metadata) -> bool {
    false
}

fn canonical_directory_tree_sha256_hex(root: &Path) -> Option<String> {
    canonical_directory_tree_sha256_hex_with_exclusions(root, &[])
}

fn canonical_provider_node_tree_sha256_hex(root: &Path) -> Option<String> {
    canonical_directory_tree_sha256_hex_with_exclusions(root, PROVIDER_NODE_TREE_EXCLUSIONS)
}

fn canonical_provider_application_tree_sha256_hex(root: &Path) -> Option<String> {
    canonical_directory_tree_sha256_hex_with_exclusions(root, PROVIDER_APPLICATION_TREE_EXCLUSIONS)
}

fn canonical_provider_application_tree_sha256_hex_with_progress(
    root: &Path,
    progress_key: &Path,
) -> Option<String> {
    mark_provider_verification_scan_started(progress_key);
    let mut last_revision_files = 0_u64;
    let mut demand = provider_verification_foreground_demand_for_key(progress_key, 0);
    let mut observer = |files_completed: u64, bytes_completed: u64| {
        if files_completed == 1 || files_completed % 4 == 0 {
            demand = provider_verification_foreground_demand_for_key(progress_key, 0);
        }
        if files_completed == 1 || files_completed.saturating_sub(last_revision_files) >= 16 {
            update_provider_verification_progress(
                progress_key,
                "provider_tree_verify",
                files_completed,
                bytes_completed,
            );
            last_revision_files = files_completed;
        }
        if demand.active {
            if files_completed % 4 == 0 {
                std::thread::yield_now();
            }
            if files_completed % 16 == 0 {
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }
    };
    canonical_directory_tree_sha256_hex_with_exclusions_and_progress(
        root,
        PROVIDER_APPLICATION_TREE_EXCLUSIONS,
        Some(&mut observer),
    )
}

fn authenticate_provider_node_modules_tree(root: &Path, expected: &str) -> Result<String> {
    authenticate_provider_node_modules_tree_impl(root, expected, None)
}

fn authenticate_provider_node_modules_tree_with_progress(
    root: &Path,
    expected: &str,
    progress_key: &Path,
) -> Result<String> {
    let mut last_revision_files = 0_u64;
    let mut demand = provider_verification_foreground_demand_for_key(progress_key, 0);
    let mut observer = |files_completed: u64, bytes_completed: u64| {
        if files_completed == 1 || files_completed % 4 == 0 {
            demand = provider_verification_foreground_demand_for_key(progress_key, 0);
        }
        if files_completed == 1 || files_completed.saturating_sub(last_revision_files) >= 16 {
            update_provider_verification_progress(
                progress_key,
                "provider_tree_verify",
                files_completed,
                bytes_completed,
            );
            last_revision_files = files_completed;
        }
        if demand.active {
            if files_completed % 4 == 0 {
                std::thread::yield_now();
            }
            if files_completed % 16 == 0 {
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }
    };
    authenticate_provider_node_modules_tree_impl(root, expected, Some(&mut observer))
}

fn authenticate_provider_node_modules_tree_impl(
    root: &Path,
    expected: &str,
    progress: Option<&mut dyn FnMut(u64, u64)>,
) -> Result<String> {
    let actual =
        canonical_directory_tree_sha256_hex_with_exclusions_and_progress(root, &[], progress)
            .ok_or_else(|| {
                EngineError::InstallFailed(
                    "installed provider production dependency tree could not be authenticated"
                        .to_string(),
                )
            })?;
    if !actual.eq_ignore_ascii_case(expected) {
        #[cfg(test)]
        if let Some(capture_root) =
            std::env::var_os("VOXVULGI_CAPTURE_PROVIDER_NODE_MODULES_DIR").map(PathBuf::from)
        {
            if !capture_root.exists() {
                if let Some(parent) = capture_root.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::rename(root, &capture_root);
            }
        }
        return Err(EngineError::HashMismatch {
            path: root.to_path_buf(),
            expected: expected.to_string(),
            actual,
        });
    }
    Ok(actual)
}

fn youtube_po_plugin_entrypoint(paths: &AppPaths) -> Option<PathBuf> {
    let root = paths.youtube_po_provider_plugin_dir();
    let candidates = [
        root.join("yt_dlp_plugins")
            .join("extractor")
            .join("getpot_bgutil.py"),
        root.join("yt_dlp_plugins")
            .join("extractor")
            .join("youtube_pot_bgutil.py"),
    ];
    candidates
        .into_iter()
        .find(|path| path.exists())
        .or_else(|| {
            let mut stack = vec![root];
            while let Some(dir) = stack.pop() {
                for entry in std::fs::read_dir(dir).ok()? {
                    let entry = entry.ok()?;
                    let path = entry.path();
                    if path.is_dir() {
                        stack.push(path);
                    } else if path.extension().and_then(|value| value.to_str()) == Some("py") {
                        return Some(path);
                    }
                }
            }
            None
        })
}

pub fn youtube_po_provider_install_status(paths: &AppPaths) -> YoutubePoProviderInstallStatus {
    let pin = &pinned_dependency_manifest::manifest().youtube_po_provider;
    let node_version = if paths.node_exe().exists() {
        tool_version_first_line_with_arg(&paths.node_exe(), "--version")
    } else {
        None
    };
    let npm_version = if paths.node_npm_cmd().exists() {
        tool_version_first_line_with_arg(&paths.node_npm_cmd(), "--version")
    } else {
        None
    };
    let node_exe_sha256_hex = file_sha256_hex(&paths.node_exe());
    let npm_cmd_sha256_hex = file_sha256_hex(&paths.node_npm_cmd());
    let plugin_marker = paths
        .youtube_po_provider_plugin_dir()
        .join(".plugin_archive_sha256");
    let plugin_sha256_hex = youtube_po_plugin_entrypoint(paths).and_then(|_| {
        std::fs::read_to_string(plugin_marker)
            .ok()
            .map(|value| value.trim().to_ascii_uppercase())
    });
    let plugin_tree_sha256_hex = provider_plugin_tree_sha256_hex(
        &paths.youtube_po_provider_plugin_dir(),
        &pin.plugin_files_sha256,
    );
    let server_entrypoint_sha256_hex = file_sha256_hex(&paths.youtube_po_provider_entrypoint());
    let derived_lock_sha256_hex = file_sha256_hex(
        &paths
            .youtube_po_provider_server_dir()
            .join("package-lock.json"),
    );
    // The JSON receipt is an audit/history artifact only. It is deliberately not an executable
    // trust root: a local file can be copied, stale, or forged. Readiness requires a full-byte
    // verification completed by this process and recorded in the in-memory attestation map.
    let node_modules_process_attestation =
        provider_node_modules_process_attestation(&paths.youtube_po_provider_server_dir());
    let node_modules_tree_sha256_hex = node_modules_process_attestation
        .as_ref()
        .map(|attestation| attestation.tree_sha256_hex.clone());
    let security_audit_marker = paths
        .youtube_po_provider_server_dir()
        .join(".production_audit_zero");
    let security_audit_passed = security_audit_marker.exists()
        && std::fs::read_to_string(&security_audit_marker)
            .ok()
            .map(|value| value.trim() == pin.derived_lock_sha256_hex)
            .unwrap_or(false);
    let lifecycle_allowlist_valid =
        std::fs::read(paths.youtube_po_provider_server_dir().join("package.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .map(|package| provider_lifecycle_allowlist_is_exact(&package))
            .unwrap_or(false);
    let version_matches = node_version
        .as_deref()
        .map(|version| {
            version.trim_start_matches('v')
                == pinned_dependency_manifest::manifest().node_windows.version
        })
        .unwrap_or(false);
    let npm_version_matches = npm_version.as_deref()
        == Some(
            pinned_dependency_manifest::manifest()
                .node_windows
                .npm_version
                .as_str(),
        );
    let node_bytes_match = node_exe_sha256_hex.as_deref().is_some_and(|hash| {
        hash.eq_ignore_ascii_case(
            &pinned_dependency_manifest::manifest()
                .node_windows
                .node_exe_sha256_hex,
        )
    });
    let npm_bytes_match = npm_cmd_sha256_hex.as_deref().is_some_and(|hash| {
        hash.eq_ignore_ascii_case(
            &pinned_dependency_manifest::manifest()
                .node_windows
                .npm_cmd_sha256_hex,
        )
    });
    let lock_matches = derived_lock_sha256_hex
        .as_deref()
        .map(|hash| hash.eq_ignore_ascii_case(&pin.derived_lock_sha256_hex))
        .unwrap_or(false);
    let node_modules_tree_matches = node_modules_tree_sha256_hex
        .as_deref()
        .is_some_and(|hash| hash.eq_ignore_ascii_case(&pin.node_modules_tree_sha256_hex));
    let plugin_matches = plugin_sha256_hex
        .as_deref()
        .map(|hash| hash.eq_ignore_ascii_case(&pin.plugin_sha256_hex))
        .unwrap_or(false);
    let plugin_tree_matches = plugin_tree_sha256_hex
        .as_deref()
        .map(|hash| hash.eq_ignore_ascii_case(&pin.plugin_tree_sha256_hex))
        .unwrap_or(false);
    let server_present = server_entrypoint_sha256_hex.is_some();
    let server_matches = server_entrypoint_sha256_hex
        .as_deref()
        .is_some_and(|hash| hash.eq_ignore_ascii_case(&pin.server_entrypoint_sha256_hex));
    let installed = version_matches
        && npm_version_matches
        && node_bytes_match
        && npm_bytes_match
        && plugin_matches
        && plugin_tree_matches
        && server_matches
        && lock_matches
        && node_modules_tree_matches
        && security_audit_passed
        && lifecycle_allowlist_valid;
    let integrity_verifying =
        provider_node_modules_integrity_verifying().load(std::sync::atomic::Ordering::Acquire);
    let integrity_invalid = provider_node_modules_process_invalidations()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .contains_key(&paths.youtube_po_provider_server_dir());
    let node_modules_integrity_state = if integrity_verifying {
        "verifying"
    } else if node_modules_process_attestation.is_some() {
        "verified"
    } else if integrity_invalid {
        "invalid"
    } else {
        "unverified"
    }
    .to_string();
    YoutubePoProviderInstallStatus {
        installed,
        provider_version: pin.version.clone(),
        node_version,
        npm_version,
        node_exe_sha256_hex,
        npm_cmd_sha256_hex,
        plugin_sha256_hex,
        plugin_tree_sha256_hex,
        server_entrypoint_sha256_hex,
        derived_lock_sha256_hex,
        node_modules_tree_sha256_hex,
        node_modules_integrity_verifying: integrity_verifying,
        node_modules_integrity_state,
        node_modules_verified_at_ms: node_modules_process_attestation
            .as_ref()
            .map(|attestation| attestation.verified_at_ms),
        security_audit_passed,
        readiness_error: if installed {
            None
        } else {
            Some(format!(
                "pinned localhost PO provider payload failed integrity validation (node_version={version_matches}, npm_version={npm_version_matches}, node_bytes={node_bytes_match}, npm_bytes={npm_bytes_match}, plugin_archive={plugin_matches}, plugin_tree={plugin_tree_matches}, server_present={server_present}, server_bytes={server_matches}, lock={lock_matches}, node_modules_tree={node_modules_tree_matches}, audit={security_audit_passed}, lifecycle={lifecycle_allowlist_valid})",
            ))
        },
    }
}

fn download_verified_file(
    url: &str,
    destination: &Path,
    expected_bytes: u64,
    expected_sha256_hex: &str,
    label: &str,
) -> Result<()> {
    let response = ureq::get(url)
        .call()
        .map_err(|error| EngineError::InstallFailed(format!("{label} download failed: {error}")))?;
    if response.status().as_u16() >= 400 {
        return Err(EngineError::InstallFailed(format!(
            "{label} download failed (status={})",
            response.status()
        )));
    }
    let mut reader = response.into_body().into_reader();
    let mut output = std::fs::File::create(destination)?;
    std::io::copy(&mut reader, &mut output)?;
    output.flush()?;
    let actual_bytes = std::fs::metadata(destination)?.len();
    if actual_bytes != expected_bytes {
        return Err(EngineError::SizeMismatch {
            path: destination.to_path_buf(),
            expected: expected_bytes,
            actual: actual_bytes,
        });
    }
    let actual_hash = file_sha256_hex(destination).unwrap_or_default();
    if !actual_hash.eq_ignore_ascii_case(expected_sha256_hex) {
        return Err(EngineError::HashMismatch {
            path: destination.to_path_buf(),
            expected: expected_sha256_hex.to_string(),
            actual: actual_hash,
        });
    }
    Ok(())
}

fn patch_po_provider_for_localhost(server_dir: &Path) -> Result<()> {
    let package_path = server_dir.join("package.json");
    let mut package: serde_json::Value = serde_json::from_slice(&std::fs::read(&package_path)?)?;
    package["allowScripts"] = serde_json::json!({
        "canvas@3.2.3": true,
        "@swc/core@1.15.47": false,
    });
    std::fs::write(
        &package_path,
        format!("{}\n", serde_json::to_string_pretty(&package)?),
    )?;

    let main_path = server_dir.join("src").join("main.ts");
    let source = std::fs::read_to_string(&main_path)?;
    let block_start = source
        .find("const httpServer = express();")
        .ok_or_else(|| {
            EngineError::InstallFailed(
                "provider server entrypoint shape changed before localhost patch".to_string(),
            )
        })?;
    let listen_start = source[block_start..]
        .find("httpServer\n    .listen(")
        .map(|index| block_start + index)
        .ok_or_else(|| EngineError::InstallFailed("provider listen block missing".to_string()))?;
    let route_start = source[listen_start..]
        .find("\nconst sessionManager")
        .map(|index| listen_start + index)
        .ok_or_else(|| EngineError::InstallFailed("provider route boundary missing".to_string()))?;
    let mut patched = String::with_capacity(source.len());
    patched.push_str(&source[..listen_start]);
    patched.push_str(
        "httpServer.listen(\n    { host: \"127.0.0.1\", port: PORT_NUMBER },\n    (err) => {\n        if (err) throw err;\n        console.log(`Started POT server (v${VERSION}) on address 127.0.0.1:${PORT_NUMBER}`);\n    },\n);\n",
    );
    patched.push_str(&source[route_start..]);
    if patched.contains("host: \"::\"") || patched.contains("host: \"0.0.0.0\"") {
        return Err(EngineError::InstallFailed(
            "provider localhost hardening left a wildcard bind".to_string(),
        ));
    }
    std::fs::write(main_path, patched)?;
    if !provider_lifecycle_allowlist_is_exact(&package) {
        return Err(EngineError::InstallFailed(
            "provider lifecycle-script allowlist is not exact".to_string(),
        ));
    }
    Ok(())
}

fn provider_lifecycle_allowlist_is_exact(package: &serde_json::Value) -> bool {
    let Some(entries) = package
        .get("allowScripts")
        .and_then(serde_json::Value::as_object)
    else {
        return false;
    };
    entries.len() == 2
        && entries
            .get("canvas@3.2.3")
            .and_then(serde_json::Value::as_bool)
            == Some(true)
        && entries
            .get("@swc/core@1.15.47")
            .and_then(serde_json::Value::as_bool)
            == Some(false)
}

fn provider_lock_matches_manifest(lock: &[u8], expected_sha256_hex: &str) -> bool {
    use sha2::Digest;
    let normalized = String::from_utf8_lossy(lock).replace("\r\n", "\n");
    hex::encode_upper(sha2::Sha256::digest(normalized.as_bytes()))
        .eq_ignore_ascii_case(expected_sha256_hex)
}

fn provider_npm_ci_args() -> [&'static str; 2] {
    ["ci", "--ignore-scripts"]
}

fn provider_lock_lifecycle_packages_are_exact(lock: &[u8]) -> bool {
    let Ok(lock): std::result::Result<serde_json::Value, _> = serde_json::from_slice(lock) else {
        return false;
    };
    let Some(packages) = lock.get("packages").and_then(serde_json::Value::as_object) else {
        return false;
    };
    let mut lifecycle = packages
        .iter()
        .filter(|(_, value)| {
            value
                .get("hasInstallScript")
                .and_then(serde_json::Value::as_bool)
                == Some(true)
        })
        .map(|(path, value)| {
            format!(
                "{}@{}",
                path.trim_start_matches("node_modules/"),
                value
                    .get("version")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("missing")
            )
        })
        .collect::<Vec<_>>();
    lifecycle.sort();
    lifecycle == ["@swc/core@1.15.47", "canvas@3.2.3"]
}

fn installed_provider_package_version(server_dir: &Path, name: &str) -> Option<String> {
    let package_path = name
        .split('/')
        .fold(server_dir.join("node_modules"), |path, part| {
            path.join(part)
        })
        .join("package.json");
    std::fs::read(package_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|package| {
            package
                .get("version")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        })
}

fn installed_provider_build_lifecycle_packages_are_exact(server_dir: &Path) -> bool {
    installed_provider_package_version(server_dir, "canvas").as_deref() == Some("3.2.3")
        && installed_provider_package_version(server_dir, "@swc/core").as_deref() == Some("1.15.47")
}

fn installed_provider_runtime_lifecycle_packages_are_exact(server_dir: &Path) -> bool {
    let swc_dir = server_dir.join("node_modules").join("@swc").join("core");
    installed_provider_package_version(server_dir, "canvas").as_deref() == Some("3.2.3")
        && !swc_dir.exists()
}

fn remove_provider_build_only_artifacts(server_dir: &Path) -> Result<()> {
    let npm_cache = server_dir.join(".npm_cache");
    if npm_cache.exists() {
        std::fs::remove_dir_all(&npm_cache).map_err(|error| {
            EngineError::InstallFailed(format!(
                "provider npm build cache cleanup failed before application sealing: {error}"
            ))
        })?;
    }
    let typescript_build_info = server_dir.join("tsconfig.tsbuildinfo");
    if typescript_build_info.exists() {
        std::fs::remove_file(&typescript_build_info).map_err(|error| {
            EngineError::InstallFailed(format!(
                "provider TypeScript build-info cleanup failed before application sealing: {error}"
            ))
        })?;
    }
    if npm_cache.exists() || typescript_build_info.exists() {
        return Err(EngineError::InstallFailed(
            "provider build-only artifacts remained after cleanup; application tree was not sealed"
                .to_string(),
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn run_provider_npm(
    node_dir: &Path,
    server_dir: &Path,
    args: &[&str],
) -> Result<std::process::Output> {
    let existing_path = std::env::var_os("PATH").unwrap_or_default();
    let joined_path = std::env::join_paths(
        std::iter::once(node_dir.to_path_buf()).chain(std::env::split_paths(&existing_path)),
    )
    .map_err(|error| EngineError::InstallFailed(format!("could not compose Node PATH: {error}")))?;
    let controlled_config = server_dir.join(".voxvulgi_npmrc");
    let controlled_global_config = server_dir.join(".voxvulgi_global_npmrc");
    let controlled_config_text =
        b"registry=https://registry.npmjs.org/\nignore-scripts=true\naudit=true\nfund=false\n";
    std::fs::write(&controlled_config, controlled_config_text)?;
    std::fs::write(&controlled_global_config, controlled_config_text)?;
    // npm also reads a project-local .npmrc after the user/global files. Replace the untrusted
    // upstream project config inside this attempt-owned staging tree with the same reviewed
    // policy so it cannot re-enable scripts or redirect the registry.
    std::fs::write(server_dir.join(".npmrc"), controlled_config_text)?;
    let mut command = crate::cmd::command(node_dir.join("npm.cmd"));
    command
        .args(args)
        .arg("--userconfig")
        .arg(&controlled_config)
        .arg("--globalconfig")
        .arg(&controlled_global_config)
        .arg("--cache")
        .arg(server_dir.join(".npm_cache"))
        .arg("--registry=https://registry.npmjs.org/")
        .current_dir(server_dir)
        .env("PATH", joined_path);
    for (name, _) in std::env::vars_os() {
        if name
            .to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("NPM_CONFIG_")
        {
            command.env_remove(name);
        }
    }
    command.owned_output().map_err(|error| {
        EngineError::InstallFailed(format!("provider npm command failed to start: {error}"))
    })
}

fn require_success(output: std::process::Output, label: &str) -> Result<std::process::Output> {
    if output.status.success() {
        return Ok(output);
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    Err(EngineError::InstallFailed(format!(
        "{label} failed (code={:?}): {}{}",
        output.status.code(),
        stdout.trim(),
        if stderr.trim().is_empty() {
            String::new()
        } else {
            format!("; {}", stderr.trim())
        }
    )))
}

struct AttemptDirectoryGuard {
    path: PathBuf,
    armed: bool,
}

struct ProviderInstallOperationGuard {
    paths: AppPaths,
    attempt_id: String,
    armed: bool,
}

struct ManagedProviderReplacementGuard {
    final_node: PathBuf,
    final_provider: PathBuf,
    archived_node: PathBuf,
    archived_provider: PathBuf,
    armed: bool,
}

impl ManagedProviderReplacementGuard {
    fn preserve_archive(mut self) {
        self.armed = false;
    }
}

impl Drop for ManagedProviderReplacementGuard {
    fn drop(&mut self) {
        if !self.armed || self.final_node.exists() || self.final_provider.exists() {
            return;
        }
        if self.archived_node.exists() && self.archived_provider.exists() {
            if std::fs::rename(&self.archived_node, &self.final_node).is_ok()
                && std::fs::rename(&self.archived_provider, &self.final_provider).is_err()
            {
                // Do not leave a half-restored managed runtime. Returning Node to its
                // authenticated archive preserves the all-or-nothing recovery boundary.
                let _ = std::fs::rename(&self.final_node, &self.archived_node);
            }
        }
    }
}

impl ProviderInstallOperationGuard {
    fn new(paths: &AppPaths, attempt_id: &str) -> Self {
        Self {
            paths: paths.clone(),
            attempt_id: attempt_id.to_string(),
            armed: true,
        }
    }

    fn preserve_for_crash_recovery(&mut self) {
        self.armed = false;
    }
}

fn enter_durable_provider_publication<F>(
    operation_guard: &mut ProviderInstallOperationGuard,
    persist_node_publish_intent: F,
) -> Result<()>
where
    F: FnOnce() -> Result<()>,
{
    persist_node_publish_intent()?;
    operation_guard.preserve_for_crash_recovery();
    Ok(())
}

impl Drop for ProviderInstallOperationGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = abort_prepublication_provider_install(&self.paths, &self.attempt_id);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProviderInstallAttemptReceipt {
    attempt_id: String,
    stage_root: PathBuf,
    phase: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProviderInstallOwnershipMarker {
    schema_version: u32,
    attempt_id: String,
    ownership_token: String,
}

#[derive(Debug, Clone)]
struct ProviderInstallLineage {
    attempt_id: String,
    stage_root: PathBuf,
    phase: String,
    install_generation: String,
    ownership_token_digest: String,
    node_directory_identity: String,
    provider_directory_identity: String,
    node_tree_sha256: String,
    provider_tree_sha256: String,
    commit_nonce: String,
    owner_pid: u32,
    owner_process_identity: String,
}

#[derive(Debug, Clone)]
struct ProviderInstalledIdentity {
    lineage_attempt_id: String,
    commit_nonce: String,
    install_generation: String,
    node_directory_identity: String,
    provider_directory_identity: String,
    node_tree_sha256: String,
    provider_tree_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProviderPortableAttestation {
    schema_version: u32,
    install_generation: String,
    node_complete_tree_sha256: String,
    provider_complete_tree_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProviderIdentityCommitOutcome {
    committed: bool,
    receipt_written: bool,
}

fn random_provider_authority_nonce() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

const PROVIDER_INSTALL_PHASES: &[&str] = &[
    "prepared",
    "node_publish_intent",
    "node_published",
    "provider_publish_intent",
    "provider_published",
    "committed",
];

fn valid_provider_attempt_id(attempt_id: &str) -> bool {
    uuid::Uuid::parse_str(attempt_id)
        .ok()
        .is_some_and(|parsed| parsed.hyphenated().to_string() == attempt_id)
}

fn provider_ownership_token_digest(token: &str) -> String {
    use sha2::Digest;
    hex::encode_upper(sha2::Sha256::digest(token.as_bytes()))
}

#[cfg(windows)]
fn provider_directory_identity(path: &Path) -> Result<String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FileIdInfo, GetFileInformationByHandleEx, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_ID_INFO, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    let wide = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(EngineError::InstallFailed(format!(
            "provider directory identity is unavailable for {}: {}",
            path.display(),
            std::io::Error::last_os_error()
        )));
    }
    let mut info = std::mem::MaybeUninit::<FILE_ID_INFO>::zeroed();
    let ok = unsafe {
        GetFileInformationByHandleEx(
            handle,
            FileIdInfo,
            info.as_mut_ptr().cast(),
            std::mem::size_of::<FILE_ID_INFO>() as u32,
        )
    };
    unsafe {
        let _ = CloseHandle(handle);
    }
    if ok == 0 {
        return Err(EngineError::InstallFailed(format!(
            "provider directory file ID is unavailable for {}: {}",
            path.display(),
            std::io::Error::last_os_error()
        )));
    }
    let info = unsafe { info.assume_init() };
    Ok(format!(
        "windows:{:016X}:{}",
        info.VolumeSerialNumber,
        hex::encode_upper(info.FileId.Identifier)
    ))
}

#[cfg(unix)]
fn provider_directory_identity(path: &Path) -> Result<String> {
    use std::os::unix::fs::MetadataExt;
    let metadata = std::fs::metadata(path)?;
    Ok(format!("unix:{}:{}", metadata.dev(), metadata.ino()))
}

#[cfg(not(any(windows, unix)))]
fn provider_directory_identity(path: &Path) -> Result<String> {
    Err(EngineError::InstallFailed(format!(
        "provider directory identity is unsupported for {}",
        path.display()
    )))
}

#[cfg(windows)]
fn provider_process_identity(pid: u32) -> Option<String> {
    use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, STILL_ACTIVE};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return None;
    }
    let mut exit_code = 0u32;
    let process_is_active = unsafe { GetExitCodeProcess(handle, &mut exit_code) } != 0
        && exit_code == STILL_ACTIVE as u32;
    if !process_is_active {
        unsafe {
            let _ = CloseHandle(handle);
        }
        return None;
    }
    let mut created = std::mem::MaybeUninit::<FILETIME>::zeroed();
    let mut exited = std::mem::MaybeUninit::<FILETIME>::zeroed();
    let mut kernel = std::mem::MaybeUninit::<FILETIME>::zeroed();
    let mut user = std::mem::MaybeUninit::<FILETIME>::zeroed();
    let ok = unsafe {
        GetProcessTimes(
            handle,
            created.as_mut_ptr(),
            exited.as_mut_ptr(),
            kernel.as_mut_ptr(),
            user.as_mut_ptr(),
        )
    };
    unsafe {
        let _ = CloseHandle(handle);
    }
    if ok == 0 {
        return None;
    }
    let created = unsafe { created.assume_init() };
    let ticks = (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime);
    Some(format!("windows:{pid}:{ticks:016X}"))
}

#[cfg(unix)]
fn provider_process_identity(pid: u32) -> Option<String> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let after_name = stat.rsplit_once(") ")?.1;
    let start_ticks = after_name.split_whitespace().nth(19)?;
    Some(format!("unix:{pid}:{start_ticks}"))
}

#[cfg(not(any(windows, unix)))]
fn provider_process_identity(pid: u32) -> Option<String> {
    (pid == std::process::id()).then(|| format!("process:{pid}"))
}

fn validated_provider_stage_root(
    paths: &AppPaths,
    attempt_id: &str,
    claimed: &Path,
) -> Result<PathBuf> {
    if !valid_provider_attempt_id(attempt_id) {
        return Err(EngineError::InstallFailed(
            "provider install lineage has an invalid attempt id".to_string(),
        ));
    }
    let tools_root = paths.tools_dir();
    std::fs::create_dir_all(&tools_root)?;
    let expected = tools_root.join(format!("youtube_po_provider_stage_{attempt_id}"));
    if claimed != expected {
        return Err(EngineError::InstallFailed(
            "provider install lineage has an invalid staging root".to_string(),
        ));
    }
    let canonical_tools = std::fs::canonicalize(&tools_root)?;
    let containment_probe = if expected.exists() {
        std::fs::canonicalize(&expected)?
    } else {
        canonical_tools.join(format!("youtube_po_provider_stage_{attempt_id}"))
    };
    if containment_probe.parent() != Some(canonical_tools.as_path()) {
        return Err(EngineError::InstallFailed(
            "provider install staging root escaped the managed tools directory".to_string(),
        ));
    }
    Ok(expected)
}

fn provider_phase_transition_allowed(before: Option<&str>, after: &str) -> bool {
    matches!(
        (before, after),
        (None, "prepared")
            | (Some("prepared"), "prepared" | "node_publish_intent")
            | (
                Some("node_publish_intent"),
                "node_publish_intent" | "node_published"
            )
            | (
                Some("node_published"),
                "node_published" | "provider_publish_intent"
            )
            | (
                Some("provider_publish_intent"),
                "provider_publish_intent" | "provider_published"
            )
            | (
                Some("provider_published"),
                "provider_published" | "committed"
            )
            | (Some("committed"), "committed")
    )
}

fn claim_provider_install_owner(
    paths: &AppPaths,
    attempt_id: &str,
    stage_root: &Path,
    ownership_token_digest: &str,
    commit_nonce: &str,
) -> Result<()> {
    if commit_nonce.len() < 32 {
        return Err(EngineError::InstallFailed(
            "provider install commit nonce is invalid".to_string(),
        ));
    }
    let stage_root = validated_provider_stage_root(paths, attempt_id, stage_root)?;
    let mut conn = crate::db::write_context(paths)?;
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let owner = match tx.query_row(
        "SELECT attempt_id,owner_pid,owner_process_identity,commit_nonce FROM provider_install_owner WHERE singleton=1",
        [],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, u32>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        },
    ) {
        Ok(value) => Some(value),
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(error) => return Err(error.into()),
    };
    let current_pid = std::process::id();
    let current_process_identity = provider_process_identity(current_pid).ok_or_else(|| {
        EngineError::InstallFailed(
            "could not establish the provider install owner process identity".to_string(),
        )
    })?;
    if let Some((owner, owner_pid, owner_process_identity, owner_commit_nonce)) = owner.as_ref() {
        if owner != attempt_id {
            return Err(EngineError::InstallFailed(format!(
                "provider install is already owned by active attempt {owner}; explicit recovery is required"
            )));
        }
        if *owner_pid != current_pid || owner_process_identity != &current_process_identity {
            return Err(EngineError::InstallFailed(
                "provider install owner process identity does not match this process".to_string(),
            ));
        }
        let (existing_root, existing_phase, existing_token_digest) = tx.query_row(
            "SELECT stage_root,phase,ownership_token_digest FROM provider_install_lineage WHERE attempt_id=?1",
            [attempt_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )?;
        if Path::new(&existing_root) != stage_root
            || existing_phase != "prepared"
            || existing_token_digest != ownership_token_digest
            || owner_commit_nonce != commit_nonce
        {
            return Err(EngineError::InstallFailed(
                "provider install owner cannot be re-claimed after publication began".to_string(),
            ));
        }
        tx.execute(
            "UPDATE provider_install_owner SET updated_at_ms=?1 WHERE singleton=1 AND attempt_id=?2",
            rusqlite::params![now_ms(), attempt_id],
        )?;
        tx.commit()?;
        let _ = write_provider_install_attempt_receipt(paths, attempt_id, &stage_root, "prepared");
        return Ok(());
    }

    let existing_lineages: i64 = tx.query_row(
        "SELECT COUNT(*) FROM provider_install_lineage lineage
         WHERE NOT EXISTS(
           SELECT 1 FROM provider_installed_identity identity
           WHERE identity.singleton=1
             AND identity.lineage_attempt_id=lineage.attempt_id
             AND identity.commit_nonce=lineage.commit_nonce
             AND lineage.phase='committed'
         )",
        [],
        |row| row.get(0),
    )?;
    if existing_lineages != 0 {
        return Err(EngineError::InstallFailed(
            "unowned provider install lineage requires explicit recovery".to_string(),
        ));
    }
    let timestamp = now_ms();
    tx.execute(
        "INSERT INTO provider_install_lineage(attempt_id,stage_root,phase,updated_at_ms,ownership_token_digest,commit_nonce,install_generation) VALUES(?1,?2,'prepared',?3,?4,?5,?6)",
        rusqlite::params![attempt_id, stage_root.to_string_lossy(), timestamp, ownership_token_digest, commit_nonce, provider_install_generation()],
    )?;
    tx.execute(
        "INSERT INTO provider_install_owner(singleton,attempt_id,acquired_at_ms,updated_at_ms,owner_pid,owner_process_identity,commit_nonce) VALUES(1,?1,?2,?2,?3,?4,?5)",
        rusqlite::params![attempt_id, timestamp, current_pid, current_process_identity, commit_nonce],
    )?;
    tx.commit()?;
    let _ = write_provider_install_attempt_receipt(paths, attempt_id, &stage_root, "prepared");
    Ok(())
}

fn persist_provider_install_lineage(
    paths: &AppPaths,
    attempt_id: &str,
    stage_root: &Path,
    phase: &str,
) -> Result<()> {
    if !PROVIDER_INSTALL_PHASES.contains(&phase) {
        return Err(EngineError::InstallFailed(
            "provider install lineage has an invalid phase".to_string(),
        ));
    }
    let stage_root = validated_provider_stage_root(paths, attempt_id, stage_root)?;
    let mut conn = crate::db::write_context(paths)?;
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let owner = match tx.query_row(
        "SELECT attempt_id,commit_nonce FROM provider_install_owner WHERE singleton=1",
        [],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    ) {
        Ok(value) => value,
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            return Err(EngineError::InstallFailed(
                "provider install lineage has no authoritative owner".to_string(),
            ))
        }
        Err(error) => return Err(error.into()),
    };
    if owner.0 != attempt_id {
        return Err(EngineError::InstallFailed(
            "provider install lineage update was rejected for a non-owner attempt".to_string(),
        ));
    }
    let existing = match tx.query_row(
        "SELECT stage_root,phase,install_generation,ownership_token_digest,node_directory_identity,provider_directory_identity,node_tree_sha256,provider_tree_sha256,commit_nonce FROM provider_install_lineage WHERE attempt_id=?1",
        [attempt_id],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
            ))
        },
    ) {
        Ok(value) => Some(value),
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(error) => return Err(error.into()),
    };
    if let Some((
        existing_root,
        existing_phase,
        install_generation,
        ownership_token_digest,
        node_directory_identity,
        provider_directory_identity,
        node_tree_sha256,
        provider_tree_sha256,
        commit_nonce,
    )) = existing.as_ref()
    {
        if Path::new(existing_root) != stage_root
            || !provider_phase_transition_allowed(Some(existing_phase), phase)
            || commit_nonce != &owner.1
        {
            return Err(EngineError::InstallFailed(
                "provider install lineage phase/root transition was rejected".to_string(),
            ));
        }
        if phase != "prepared"
            && [
                install_generation,
                ownership_token_digest,
                node_directory_identity,
                provider_directory_identity,
                node_tree_sha256,
                provider_tree_sha256,
            ]
            .iter()
            .any(|value| value.is_empty())
        {
            return Err(EngineError::InstallFailed(
                "provider publication requires generation, sealed token, directory, and complete-tree identities"
                    .to_string(),
            ));
        }
    } else if !provider_phase_transition_allowed(None, phase) {
        return Err(EngineError::InstallFailed(
            "provider install lineage must begin in prepared phase".to_string(),
        ));
    } else {
        return Err(EngineError::InstallFailed(
            "provider install owner references a missing lineage".to_string(),
        ));
    }
    let updated = tx.execute(
        "UPDATE provider_install_lineage SET phase=?1,updated_at_ms=?2 WHERE attempt_id=?3 AND stage_root=?4",
        rusqlite::params![phase, now_ms(), attempt_id, stage_root.to_string_lossy()],
    )?;
    if updated != 1 {
        return Err(EngineError::InstallFailed(
            "provider install lineage update did not affect its exact authoritative row"
                .to_string(),
        ));
    }
    tx.commit()?;
    let _ = write_provider_install_attempt_receipt(paths, attempt_id, &stage_root, phase);
    Ok(())
}

fn seal_provider_install_lineage(
    paths: &AppPaths,
    attempt_id: &str,
    stage_root: &Path,
    ownership_token_digest: &str,
    node_directory_identity: &str,
    provider_directory_identity: &str,
    node_tree_sha256: &str,
    provider_tree_sha256: &str,
) -> Result<()> {
    let stage_root = validated_provider_stage_root(paths, attempt_id, stage_root)?;
    let mut conn = crate::db::write_context(paths)?;
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let changed = tx.execute(
        "UPDATE provider_install_lineage
         SET ownership_token_digest=?1,node_directory_identity=?2,
             provider_directory_identity=?3,node_tree_sha256=?4,
             provider_tree_sha256=?5,updated_at_ms=?6
         WHERE attempt_id=?7 AND stage_root=?8 AND phase='prepared'
           AND EXISTS(SELECT 1 FROM provider_install_owner owner
                      WHERE owner.singleton=1 AND owner.attempt_id=?7
                        AND owner.commit_nonce=provider_install_lineage.commit_nonce)",
        rusqlite::params![
            ownership_token_digest,
            node_directory_identity,
            provider_directory_identity,
            node_tree_sha256,
            provider_tree_sha256,
            now_ms(),
            attempt_id,
            stage_root.to_string_lossy(),
        ],
    )?;
    if changed != 1 {
        return Err(EngineError::InstallFailed(
            "provider install lineage could not be sealed before publication".to_string(),
        ));
    }
    tx.commit()?;
    Ok(())
}

fn commit_provider_installed_identity(
    paths: &AppPaths,
    attempt_id: &str,
    stage_root: &Path,
) -> Result<ProviderIdentityCommitOutcome> {
    commit_provider_installed_identity_with_receipt(
        paths,
        attempt_id,
        stage_root,
        |paths, attempt, root| {
            write_provider_install_attempt_receipt(paths, attempt, root, "committed")
        },
    )
}

fn commit_provider_installed_identity_with_receipt<W>(
    paths: &AppPaths,
    attempt_id: &str,
    stage_root: &Path,
    write_receipt: W,
) -> Result<ProviderIdentityCommitOutcome>
where
    W: FnOnce(&AppPaths, &str, &Path) -> Result<()>,
{
    let stage_root = validated_provider_stage_root(paths, attempt_id, stage_root)?;
    let mut conn = crate::db::write_context(paths)?;
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let identity = tx.query_row(
        "SELECT node_directory_identity,provider_directory_identity,node_tree_sha256,provider_tree_sha256,commit_nonce
         FROM provider_install_lineage
         WHERE attempt_id=?1 AND stage_root=?2 AND phase='provider_published'
           AND EXISTS(SELECT 1 FROM provider_install_owner owner
                      WHERE owner.singleton=1 AND owner.attempt_id=?1)",
        rusqlite::params![attempt_id, stage_root.to_string_lossy()],
        |row| {
            Ok(ProviderInstalledIdentity {
                lineage_attempt_id: attempt_id.to_string(),
                commit_nonce: row.get(4)?,
                install_generation: provider_install_generation(),
                node_directory_identity: row.get(0)?,
                provider_directory_identity: row.get(1)?,
                node_tree_sha256: row.get(2)?,
                provider_tree_sha256: row.get(3)?,
            })
        },
    )?;
    let changed = tx.execute(
        "UPDATE provider_install_lineage SET phase='committed',updated_at_ms=?1
         WHERE attempt_id=?2 AND phase='provider_published'",
        rusqlite::params![now_ms(), attempt_id],
    )?;
    if changed != 1 {
        return Err(EngineError::InstallFailed(
            "provider installed identity commit lost its publication lineage".to_string(),
        ));
    }
    tx.execute(
        "INSERT INTO provider_installed_identity(
           singleton,lineage_attempt_id,commit_nonce,install_generation,
           node_directory_identity,provider_directory_identity,
           node_tree_sha256,provider_tree_sha256,committed_at_ms
         ) VALUES(1,?1,?2,?3,?4,?5,?6,?7,?8)
         ON CONFLICT(singleton) DO UPDATE SET
           lineage_attempt_id=excluded.lineage_attempt_id,
           commit_nonce=excluded.commit_nonce,
           install_generation=excluded.install_generation,
           node_directory_identity=excluded.node_directory_identity,
           provider_directory_identity=excluded.provider_directory_identity,
           node_tree_sha256=excluded.node_tree_sha256,
           provider_tree_sha256=excluded.provider_tree_sha256,
           committed_at_ms=excluded.committed_at_ms",
        rusqlite::params![
            identity.lineage_attempt_id,
            identity.commit_nonce,
            identity.install_generation,
            identity.node_directory_identity,
            identity.provider_directory_identity,
            identity.node_tree_sha256,
            identity.provider_tree_sha256,
            now_ms(),
        ],
    )?;
    tx.execute(
        "DELETE FROM provider_install_lineage
         WHERE phase='committed' AND attempt_id<>?1
           AND NOT EXISTS(
             SELECT 1 FROM provider_installed_identity identity
             WHERE identity.lineage_attempt_id=provider_install_lineage.attempt_id
               AND identity.commit_nonce=provider_install_lineage.commit_nonce
           )",
        [attempt_id],
    )?;
    tx.commit()?;
    let receipt_written = write_receipt(paths, attempt_id, &stage_root).is_ok();
    Ok(ProviderIdentityCommitOutcome {
        committed: true,
        receipt_written,
    })
}

fn load_provider_installed_identity(paths: &AppPaths) -> Result<Option<ProviderInstalledIdentity>> {
    let conn = crate::db::open_readonly(paths)?;
    match conn.query_row(
        "SELECT lineage_attempt_id,commit_nonce,install_generation,
                node_directory_identity,provider_directory_identity,
                node_tree_sha256,provider_tree_sha256
         FROM provider_installed_identity WHERE singleton=1",
        [],
        |row| {
            Ok(ProviderInstalledIdentity {
                lineage_attempt_id: row.get(0)?,
                commit_nonce: row.get(1)?,
                install_generation: row.get(2)?,
                node_directory_identity: row.get(3)?,
                provider_directory_identity: row.get(4)?,
                node_tree_sha256: row.get(5)?,
                provider_tree_sha256: row.get(6)?,
            })
        },
    ) {
        Ok(identity) => Ok(Some(identity)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn managed_provider_replacement_archive_root(
    paths: &AppPaths,
    attempt_id: &str,
) -> Result<PathBuf> {
    if !valid_provider_attempt_id(attempt_id) {
        return Err(EngineError::InstallFailed(
            "provider replacement archive rejected an invalid attempt ID".to_string(),
        ));
    }
    Ok(paths
        .tools_dir()
        .join(format!("youtube_po_provider_previous_{attempt_id}")))
}

fn prepare_governed_provider_replacement(
    paths: &AppPaths,
    attempt_id: &str,
) -> Result<Option<ManagedProviderReplacementGuard>> {
    let final_node = paths.node_runtime_dir();
    let final_provider = paths.youtube_po_provider_dir();
    if !final_node.exists() && !final_provider.exists() {
        return Ok(None);
    }
    if !final_node.is_dir() || !final_provider.is_dir() {
        return Err(EngineError::InstallFailed(
            "provider replacement requires both exact managed final directories".to_string(),
        ));
    }
    let identity = load_provider_installed_identity(paths)?.ok_or_else(|| {
        EngineError::InstallFailed(
            "provider replacement refused final directories without installed identity".to_string(),
        )
    })?;
    authenticate_stored_managed_provider_identity_at(
        paths,
        &identity,
        &final_node,
        &final_provider,
    )?;
    let archive_root = managed_provider_replacement_archive_root(paths, attempt_id)?;
    if archive_root.exists() {
        return Err(EngineError::InstallFailed(
            "provider replacement archive already exists for this attempt".to_string(),
        ));
    }
    std::fs::create_dir(&archive_root)?;
    let archived_node = archive_root.join("node");
    let archived_provider = archive_root.join("provider");
    std::fs::rename(&final_node, &archived_node)?;
    if let Err(error) = std::fs::rename(&final_provider, &archived_provider) {
        let _ = std::fs::rename(&archived_node, &final_node);
        let _ = std::fs::remove_dir(&archive_root);
        return Err(error.into());
    }
    if let Err(error) = authenticate_stored_managed_provider_identity_at(
        paths,
        &identity,
        &archived_node,
        &archived_provider,
    ) {
        let _ = std::fs::rename(&archived_provider, &final_provider);
        let _ = std::fs::rename(&archived_node, &final_node);
        let _ = std::fs::remove_dir(&archive_root);
        return Err(error);
    }
    Ok(Some(ManagedProviderReplacementGuard {
        final_node,
        final_provider,
        archived_node,
        archived_provider,
        armed: true,
    }))
}

fn restore_governed_provider_replacement_if_present(
    paths: &AppPaths,
    attempt_id: &str,
) -> Result<()> {
    let archive_root = managed_provider_replacement_archive_root(paths, attempt_id)?;
    if !archive_root.exists() {
        return Ok(());
    }
    let final_node = paths.node_runtime_dir();
    let final_provider = paths.youtube_po_provider_dir();
    if final_node.exists() || final_provider.exists() {
        return Err(EngineError::InstallFailed(
            "provider replacement recovery refused to overwrite a managed final".to_string(),
        ));
    }
    let archived_node = archive_root.join("node");
    let archived_provider = archive_root.join("provider");
    let identity = load_provider_installed_identity(paths)?.ok_or_else(|| {
        EngineError::InstallFailed(
            "provider replacement recovery has no prior installed identity".to_string(),
        )
    })?;
    authenticate_stored_managed_provider_identity_at(
        paths,
        &identity,
        &archived_node,
        &archived_provider,
    )?;
    std::fs::rename(&archived_node, &final_node)?;
    if let Err(error) = std::fs::rename(&archived_provider, &final_provider) {
        let _ = std::fs::rename(&final_node, &archived_node);
        return Err(error.into());
    }
    let _ = std::fs::remove_dir(archive_root);
    Ok(())
}

fn load_provider_install_lineage(paths: &AppPaths) -> Result<Option<ProviderInstallLineage>> {
    let conn = crate::db::open_readonly(paths)?;
    let unresolved_lineage_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM provider_install_lineage lineage
         WHERE NOT EXISTS(
           SELECT 1 FROM provider_installed_identity identity
           WHERE identity.singleton=1
             AND identity.lineage_attempt_id=lineage.attempt_id
             AND identity.commit_nonce=lineage.commit_nonce
             AND lineage.phase='committed'
         )",
        [],
        |row| row.get(0),
    )?;
    let mut stmt = conn.prepare(
        "SELECT lineage.attempt_id,lineage.stage_root,lineage.phase,
                lineage.install_generation,lineage.ownership_token_digest,lineage.node_directory_identity,
                lineage.provider_directory_identity,lineage.node_tree_sha256,
                lineage.provider_tree_sha256,lineage.commit_nonce,
                owner.owner_pid,owner.owner_process_identity
         FROM provider_install_owner owner
         JOIN provider_install_lineage lineage ON lineage.attempt_id=owner.attempt_id
         WHERE owner.singleton=1",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(ProviderInstallLineage {
                attempt_id: row.get(0)?,
                stage_root: PathBuf::from(row.get::<_, String>(1)?),
                phase: row.get(2)?,
                install_generation: row.get(3)?,
                ownership_token_digest: row.get(4)?,
                node_directory_identity: row.get(5)?,
                provider_directory_identity: row.get(6)?,
                node_tree_sha256: row.get(7)?,
                provider_tree_sha256: row.get(8)?,
                commit_nonce: row.get(9)?,
                owner_pid: row.get(10)?,
                owner_process_identity: row.get(11)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let single_settled_committed_owner =
        rows.len() == 1 && unresolved_lineage_count == 0 && rows[0].phase == "committed";
    if rows.len() > 1
        || (rows.is_empty() && unresolved_lineage_count != 0)
        || (!rows.is_empty() && unresolved_lineage_count != 1 && !single_settled_committed_owner)
    {
        return Err(EngineError::InstallFailed(
            "provider install lineage has no unambiguous authoritative owner; explicit recovery is required"
                .to_string(),
        ));
    }
    Ok(rows.into_iter().next())
}

fn delete_provider_install_lineage(paths: &AppPaths, attempt_id: &str) -> Result<()> {
    let conn = crate::db::write_context(paths)?;
    conn.execute(
        "DELETE FROM provider_install_lineage WHERE attempt_id=?1",
        [attempt_id],
    )?;
    Ok(())
}

fn release_provider_install_owner(paths: &AppPaths, attempt_id: &str) -> Result<()> {
    let mut conn = crate::db::write_context(paths)?;
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let changed = tx.execute(
        "DELETE FROM provider_install_owner
         WHERE singleton=1 AND attempt_id=?1
           AND EXISTS(
             SELECT 1 FROM provider_install_lineage lineage
             WHERE lineage.attempt_id=?1 AND lineage.phase='committed'
               AND lineage.commit_nonce=provider_install_owner.commit_nonce
           )",
        [attempt_id],
    )?;
    if changed != 1 {
        return Err(EngineError::InstallFailed(
            "provider committed owner release lost its exact lineage".to_string(),
        ));
    }
    tx.commit()?;
    Ok(())
}

fn abort_prepublication_provider_install(paths: &AppPaths, attempt_id: &str) -> Result<()> {
    let mut conn = crate::db::write_context(paths)?;
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let state = match tx.query_row(
        "SELECT lineage.stage_root,lineage.phase,owner.owner_pid,owner.owner_process_identity
         FROM provider_install_owner owner
         JOIN provider_install_lineage lineage ON lineage.attempt_id=owner.attempt_id
         WHERE owner.singleton=1 AND owner.attempt_id=?1
           AND owner.commit_nonce=lineage.commit_nonce",
        [attempt_id],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, u32>(2)?,
                row.get::<_, String>(3)?,
            ))
        },
    ) {
        Ok(value) => Some(value),
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(error) => return Err(error.into()),
    };
    let Some((stage_root, phase, owner_pid, owner_process_identity)) = state else {
        return Ok(());
    };
    if phase != "prepared"
        || owner_pid != std::process::id()
        || provider_process_identity(owner_pid).as_deref() != Some(owner_process_identity.as_str())
    {
        return Err(EngineError::InstallFailed(
            "provider prepublication cleanup refused non-prepared or foreign owner".to_string(),
        ));
    }
    tx.execute(
        "DELETE FROM provider_install_owner WHERE singleton=1 AND attempt_id=?1",
        [attempt_id],
    )?;
    tx.execute(
        "DELETE FROM provider_install_lineage WHERE attempt_id=?1 AND phase='prepared'",
        [attempt_id],
    )?;
    tx.commit()?;
    drop(conn);
    let _ = std::fs::remove_file(provider_install_attempt_receipt_path(paths));
    let stage_root = PathBuf::from(stage_root);
    if stage_root.exists() {
        let _ = std::fs::remove_dir_all(stage_root);
    }
    Ok(())
}

fn abort_owned_provider_install_after_complete_rollback(
    paths: &AppPaths,
    attempt_id: &str,
) -> Result<()> {
    let lineage = load_provider_install_lineage(paths)?.ok_or_else(|| {
        EngineError::InstallFailed(
            "provider rollback cleanup could not find its durable lineage".to_string(),
        )
    })?;
    if lineage.attempt_id != attempt_id
        || lineage.phase == "committed"
        || lineage.owner_pid != std::process::id()
        || provider_process_identity(lineage.owner_pid).as_deref()
            != Some(lineage.owner_process_identity.as_str())
    {
        return Err(EngineError::InstallFailed(
            "provider rollback cleanup refused committed or foreign ownership".to_string(),
        ));
    }
    if paths.node_runtime_dir().exists() || paths.youtube_po_provider_dir().exists() {
        return Err(EngineError::InstallFailed(
            "provider rollback cleanup refused while a published destination remains".to_string(),
        ));
    }
    let stage_root = validated_provider_stage_root(paths, attempt_id, &lineage.stage_root)?;
    verify_published_directory_lineage(
        &stage_root.join("node"),
        &lineage.node_directory_identity,
        &lineage.node_tree_sha256,
        canonical_provider_node_tree_sha256_hex,
        "rolled-back Node",
    )?;
    verify_published_directory_lineage(
        &stage_root.join("provider"),
        &lineage.provider_directory_identity,
        &lineage.provider_tree_sha256,
        canonical_provider_application_tree_sha256_hex,
        "rolled-back provider",
    )?;

    let mut conn = crate::db::write_context(paths)?;
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let owner_deleted = tx.execute(
        "DELETE FROM provider_install_owner
         WHERE singleton=1 AND attempt_id=?1 AND commit_nonce=?2",
        rusqlite::params![attempt_id, lineage.commit_nonce],
    )?;
    let lineage_deleted = tx.execute(
        "DELETE FROM provider_install_lineage
         WHERE attempt_id=?1 AND commit_nonce=?2 AND phase<>'committed'",
        rusqlite::params![attempt_id, lineage.commit_nonce],
    )?;
    if owner_deleted != 1 || lineage_deleted != 1 {
        return Err(EngineError::InstallFailed(
            "provider rollback cleanup lost its exact owner or lineage".to_string(),
        ));
    }
    tx.commit()?;
    drop(conn);
    let _ = std::fs::remove_file(provider_install_attempt_receipt_path(paths));
    std::fs::remove_dir_all(stage_root)?;
    Ok(())
}

fn provider_install_attempt_receipt_path(paths: &AppPaths) -> PathBuf {
    paths
        .tools_dir()
        .join(".youtube_po_provider_install_attempt.json")
}

fn quarantine_or_remove_provider_install_receipt(paths: &AppPaths) {
    let receipt = provider_install_attempt_receipt_path(paths);
    if !receipt.exists() {
        return;
    }
    let quarantine = paths.tools_dir().join(format!(
        ".youtube_po_provider_install_attempt.orphan.{}.json",
        uuid::Uuid::new_v4().simple()
    ));
    if std::fs::rename(&receipt, quarantine).is_err() {
        let _ = std::fs::remove_file(receipt);
    }
}

fn provider_attempt_marker(dir: &Path) -> PathBuf {
    dir.join(".voxvulgi_provider_install_attempt")
}

fn write_provider_install_attempt_receipt(
    paths: &AppPaths,
    attempt_id: &str,
    stage_root: &Path,
    phase: &str,
) -> Result<()> {
    crate::persistence::atomic_write_text(
        &provider_install_attempt_receipt_path(paths),
        &format!(
            "{}\n",
            serde_json::to_string_pretty(&ProviderInstallAttemptReceipt {
                attempt_id: attempt_id.to_string(),
                stage_root: stage_root.to_path_buf(),
                phase: phase.to_string(),
            })?
        ),
    )?;
    Ok(())
}

fn write_provider_ownership_marker(
    dir: &Path,
    attempt_id: &str,
    ownership_token: &str,
) -> Result<()> {
    crate::persistence::atomic_write_text(
        &provider_attempt_marker(dir),
        &serde_json::to_string(&ProviderInstallOwnershipMarker {
            schema_version: 2,
            attempt_id: attempt_id.to_string(),
            ownership_token: ownership_token.to_string(),
        })?,
    )?;
    Ok(())
}

fn attempt_marker_matches(dir: &Path, lineage: &ProviderInstallLineage) -> bool {
    std::fs::read(provider_attempt_marker(dir))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<ProviderInstallOwnershipMarker>(&bytes).ok())
        .is_some_and(|marker| {
            marker.schema_version == 2
                && marker.attempt_id == lineage.attempt_id
                && !marker.ownership_token.is_empty()
                && provider_ownership_token_digest(&marker.ownership_token)
                    == lineage.ownership_token_digest
        })
}

fn authenticate_published_node_payload_against(
    paths: &AppPaths,
    expected_node_sha256: &str,
    expected_npm_sha256: &str,
) -> Result<()> {
    for (path, expected) in [
        (paths.node_exe(), expected_node_sha256),
        (paths.node_npm_cmd(), expected_npm_sha256),
    ] {
        let actual = file_sha256_hex(&path).ok_or_else(|| {
            EngineError::InstallFailed(format!(
                "published provider Node payload could not be authenticated: {}",
                path.display()
            ))
        })?;
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(EngineError::HashMismatch {
                path,
                expected: expected.to_string(),
                actual,
            });
        }
    }
    Ok(())
}

fn authenticate_published_node_payload(paths: &AppPaths) -> Result<()> {
    let pin = &pinned_dependency_manifest::manifest().node_windows;
    authenticate_published_node_payload_against(
        paths,
        &pin.node_exe_sha256_hex,
        &pin.npm_cmd_sha256_hex,
    )
}

struct PublishedProviderIdentity<'a> {
    plugin_archive_sha256: &'a str,
    plugin_files_sha256: &'a std::collections::BTreeMap<String, String>,
    plugin_tree_sha256: &'a str,
    server_entrypoint_sha256: &'a str,
    derived_lock_sha256: &'a str,
    node_modules_tree_sha256: &'a str,
}

fn authenticate_published_provider_payload_against(
    paths: &AppPaths,
    expected: &PublishedProviderIdentity<'_>,
) -> Result<String> {
    let plugin_dir = paths.youtube_po_provider_plugin_dir();
    let server_dir = paths.youtube_po_provider_server_dir();

    let plugin_marker = std::fs::read_to_string(plugin_dir.join(".plugin_archive_sha256"))
        .map_err(|_| {
            EngineError::InstallFailed(
                "published provider plugin archive identity is missing".to_string(),
            )
        })?;
    if !plugin_marker
        .trim()
        .eq_ignore_ascii_case(expected.plugin_archive_sha256)
    {
        return Err(EngineError::HashMismatch {
            path: plugin_dir.join(".plugin_archive_sha256"),
            expected: expected.plugin_archive_sha256.to_string(),
            actual: plugin_marker.trim().to_string(),
        });
    }
    let plugin_tree = provider_plugin_tree_sha256_hex(&plugin_dir, expected.plugin_files_sha256)
        .ok_or_else(|| {
            EngineError::InstallFailed(
                "published provider plugin tree could not be authenticated".to_string(),
            )
        })?;
    if !plugin_tree.eq_ignore_ascii_case(expected.plugin_tree_sha256) {
        return Err(EngineError::HashMismatch {
            path: plugin_dir,
            expected: expected.plugin_tree_sha256.to_string(),
            actual: plugin_tree,
        });
    }

    for (path, expected) in [
        (
            paths.youtube_po_provider_entrypoint(),
            expected.server_entrypoint_sha256,
        ),
        (
            server_dir.join("package-lock.json"),
            expected.derived_lock_sha256,
        ),
    ] {
        let actual = file_sha256_hex(&path).ok_or_else(|| {
            EngineError::InstallFailed(format!(
                "published provider payload could not be authenticated: {}",
                path.display()
            ))
        })?;
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(EngineError::HashMismatch {
                path,
                expected: expected.to_string(),
                actual,
            });
        }
    }

    let audit_marker =
        std::fs::read_to_string(server_dir.join(".production_audit_zero")).map_err(|_| {
            EngineError::InstallFailed(
                "published provider security-audit identity is missing".to_string(),
            )
        })?;
    if audit_marker.trim() != expected.derived_lock_sha256 {
        return Err(EngineError::InstallFailed(
            "published provider security-audit identity does not match the reviewed lock"
                .to_string(),
        ));
    }
    let package = std::fs::read(server_dir.join("package.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .ok_or_else(|| {
            EngineError::InstallFailed(
                "published provider package manifest could not be authenticated".to_string(),
            )
        })?;
    if !provider_lifecycle_allowlist_is_exact(&package)
        || !installed_provider_runtime_lifecycle_packages_are_exact(&server_dir)
    {
        return Err(EngineError::InstallFailed(
            "published provider lifecycle-package identities are not the reviewed set".to_string(),
        ));
    }
    authenticate_provider_node_modules_tree(
        &server_dir.join("node_modules"),
        expected.node_modules_tree_sha256,
    )
}

fn authenticate_published_provider_payload(paths: &AppPaths) -> Result<String> {
    let pin = &pinned_dependency_manifest::manifest().youtube_po_provider;
    authenticate_published_provider_payload_against(
        paths,
        &PublishedProviderIdentity {
            plugin_archive_sha256: &pin.plugin_sha256_hex,
            plugin_files_sha256: &pin.plugin_files_sha256,
            plugin_tree_sha256: &pin.plugin_tree_sha256_hex,
            server_entrypoint_sha256: &pin.server_entrypoint_sha256_hex,
            derived_lock_sha256: &pin.derived_lock_sha256_hex,
            node_modules_tree_sha256: &pin.node_modules_tree_sha256_hex,
        },
    )
}

fn verify_published_directory_lineage(
    path: &Path,
    expected_directory_identity: &str,
    expected_tree_sha256: &str,
    tree_hash: fn(&Path) -> Option<String>,
    label: &str,
) -> Result<()> {
    if expected_directory_identity.is_empty() || expected_tree_sha256.is_empty() {
        return Err(EngineError::InstallFailed(format!(
            "{label} recovery lineage is not sealed with directory and complete-tree identity"
        )));
    }
    let actual_directory_identity = provider_directory_identity(path)?;
    if actual_directory_identity != expected_directory_identity {
        return Err(EngineError::InstallFailed(format!(
            "{label} published directory is a different filesystem object than the sealed staging directory"
        )));
    }
    let actual_tree = tree_hash(path).ok_or_else(|| {
        EngineError::InstallFailed(format!(
            "{label} complete published tree could not be authenticated"
        ))
    })?;
    if !actual_tree.eq_ignore_ascii_case(expected_tree_sha256) {
        return Err(EngineError::HashMismatch {
            path: path.to_path_buf(),
            expected: expected_tree_sha256.to_string(),
            actual: actual_tree,
        });
    }
    Ok(())
}

fn authenticate_committed_provider_install(
    paths: &AppPaths,
    lineage: &ProviderInstallLineage,
) -> Result<()> {
    verify_published_directory_lineage(
        &paths.node_runtime_dir(),
        &lineage.node_directory_identity,
        &lineage.node_tree_sha256,
        canonical_provider_node_tree_sha256_hex,
        "Node",
    )?;
    verify_published_directory_lineage(
        &paths.youtube_po_provider_dir(),
        &lineage.provider_directory_identity,
        &lineage.provider_tree_sha256,
        canonical_provider_application_tree_sha256_hex,
        "provider",
    )?;
    let installed_identity = load_provider_installed_identity(paths)?.ok_or_else(|| {
        EngineError::InstallFailed(
            "committed provider lineage has no authoritative installed identity".to_string(),
        )
    })?;
    if lineage.install_generation != provider_install_generation()
        || installed_identity.install_generation != lineage.install_generation
        || installed_identity.lineage_attempt_id != lineage.attempt_id
        || installed_identity.commit_nonce != lineage.commit_nonce
        || installed_identity.node_directory_identity != lineage.node_directory_identity
        || installed_identity.provider_directory_identity != lineage.provider_directory_identity
        || installed_identity.node_tree_sha256 != lineage.node_tree_sha256
        || installed_identity.provider_tree_sha256 != lineage.provider_tree_sha256
    {
        return Err(EngineError::InstallFailed(
            "committed provider installed identity does not match its exact lineage".to_string(),
        ));
    }
    authenticate_published_node_payload(paths)?;
    let server_dir = paths.youtube_po_provider_server_dir();
    let actual = authenticate_published_provider_payload(paths)?;
    attest_provider_node_modules_tree(&server_dir, &actual)?;
    let status = youtube_po_provider_install_status(paths);
    if status.installed {
        Ok(())
    } else {
        clear_provider_node_modules_process_attestation(&server_dir);
        Err(EngineError::InstallFailed(
            status.readiness_error.unwrap_or_else(|| {
                "committed provider payload failed full authoritative readiness validation"
                    .to_string()
            }),
        ))
    }
}

fn reconcile_interrupted_provider_install_with_checks<N, P, C>(
    paths: &AppPaths,
    authenticate_node: N,
    authenticate_provider: P,
    authenticate_committed: C,
    owner_is_live: impl Fn(&ProviderInstallLineage) -> bool,
) -> Result<()>
where
    N: Fn(&AppPaths) -> Result<()>,
    P: Fn(&AppPaths) -> Result<()>,
    C: Fn(&AppPaths, &ProviderInstallLineage) -> Result<()>,
{
    let receipt_path = provider_install_attempt_receipt_path(paths);
    let Some(lineage) = load_provider_install_lineage(paths)? else {
        if receipt_path.exists() {
            quarantine_or_remove_provider_install_receipt(paths);
        }
        return Ok(());
    };
    let stage_root =
        validated_provider_stage_root(paths, &lineage.attempt_id, &lineage.stage_root)?;
    if !PROVIDER_INSTALL_PHASES.contains(&lineage.phase.as_str()) {
        return Err(EngineError::InstallFailed(
            "provider install database lineage has an invalid phase".to_string(),
        ));
    }
    if owner_is_live(&lineage) {
        return Err(EngineError::InstallFailed(
            "provider install recovery refused because the durable owner process is still alive"
                .to_string(),
        ));
    }
    if receipt_path.exists() {
        let receipt_matches = std::fs::read(&receipt_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<ProviderInstallAttemptReceipt>(&bytes).ok())
            .is_some_and(|receipt| {
                receipt.attempt_id == lineage.attempt_id
                    && receipt.stage_root == stage_root
                    && PROVIDER_INSTALL_PHASES.contains(&receipt.phase.as_str())
            });
        if !receipt_matches {
            quarantine_or_remove_provider_install_receipt(paths);
        }
    }
    if lineage.phase == "committed" {
        authenticate_committed(paths, &lineage)?;
        let _ = std::fs::remove_file(provider_attempt_marker(&paths.node_runtime_dir()));
        let _ = std::fs::remove_file(provider_attempt_marker(&paths.youtube_po_provider_dir()));
        let _ = std::fs::remove_file(&receipt_path);
        if stage_root.exists() {
            std::fs::remove_dir_all(&stage_root)?;
        }
        release_provider_install_owner(paths, &lineage.attempt_id)?;
        return Ok(());
    }
    let replacement_archive =
        managed_provider_replacement_archive_root(paths, &lineage.attempt_id)?;
    if lineage.phase == "prepared"
        && !stage_root.exists()
        && !replacement_archive.exists()
        && paths.node_runtime_dir().is_dir()
        && paths.youtube_po_provider_dir().is_dir()
    {
        // The owner can die after claiming the durable singleton but before the existing,
        // authenticated generation is moved into its replacement archive. In that exact state,
        // the managed finals still belong to the previously committed identity; authenticate
        // both trees before clearing only the never-started replacement attempt.
        let installed_identity = load_provider_installed_identity(paths)?.ok_or_else(|| {
            EngineError::InstallFailed(
                "prepared provider recovery found managed finals without an installed identity"
                    .to_string(),
            )
        })?;
        authenticate_stored_managed_provider_identity_at(
            paths,
            &installed_identity,
            &paths.node_runtime_dir(),
            &paths.youtube_po_provider_dir(),
        )?;
        if receipt_path.exists() {
            std::fs::remove_file(&receipt_path)?;
        }
        delete_provider_install_lineage(paths, &lineage.attempt_id)?;
        return Ok(());
    }
    let node_authorized = matches!(
        lineage.phase.as_str(),
        "node_publish_intent" | "node_published" | "provider_publish_intent" | "provider_published"
    );
    let provider_authorized = matches!(
        lineage.phase.as_str(),
        "provider_publish_intent" | "provider_published"
    );
    let publications = [
        (
            paths.node_runtime_dir(),
            stage_root.join("node"),
            "Node",
            node_authorized,
        ),
        (
            paths.youtube_po_provider_dir(),
            stage_root.join("provider"),
            "provider",
            provider_authorized,
        ),
    ];
    // Validate every published candidate before moving any bytes. A later invalid/ambiguous
    // directory must not leave an earlier valid directory partially rolled back.
    for (published, staged, label, phase_authorized) in &publications {
        if !published.exists() {
            continue;
        }
        if !phase_authorized {
            return Err(EngineError::InstallFailed(format!(
                "refusing to recover {label}: phase {} does not authorize this published directory",
                lineage.phase
            )));
        }
        if !attempt_marker_matches(&published, &lineage) {
            return Err(EngineError::InstallFailed(format!(
                "refusing to recover {label}: published directory is not owned by attempt {}",
                lineage.attempt_id
            )));
        }
        match *label {
            "Node" => {
                verify_published_directory_lineage(
                    published,
                    &lineage.node_directory_identity,
                    &lineage.node_tree_sha256,
                    canonical_provider_node_tree_sha256_hex,
                    "Node",
                )?;
                authenticate_node(paths)?;
            }
            "provider" => {
                verify_published_directory_lineage(
                    published,
                    &lineage.provider_directory_identity,
                    &lineage.provider_tree_sha256,
                    canonical_provider_application_tree_sha256_hex,
                    "provider",
                )?;
                authenticate_provider(paths)?;
            }
            _ => unreachable!("fixed provider publication label"),
        }
        if staged.exists() {
            return Err(EngineError::InstallFailed(format!(
                "refusing to recover {label}: both staged and published attempt directories exist"
            )));
        }
    }
    std::fs::create_dir_all(&stage_root)?;
    for (published, staged, _, _) in &publications {
        if !published.exists() {
            continue;
        }
        std::fs::rename(&published, &staged)?;
    }
    restore_governed_provider_replacement_if_present(paths, &lineage.attempt_id)?;
    std::fs::remove_dir_all(&stage_root)?;
    if receipt_path.exists() {
        std::fs::remove_file(receipt_path)?;
    }
    delete_provider_install_lineage(paths, &lineage.attempt_id)?;
    Ok(())
}

fn reconcile_interrupted_provider_install(paths: &AppPaths) -> Result<()> {
    reconcile_interrupted_provider_install_with_checks(
        paths,
        authenticate_published_node_payload,
        |paths| authenticate_published_provider_payload(paths).map(|_| ()),
        authenticate_committed_provider_install,
        |lineage| {
            provider_process_identity(lineage.owner_pid).as_deref()
                == Some(lineage.owner_process_identity.as_str())
        },
    )
}

impl AttemptDirectoryGuard {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    fn finish(mut self) -> Result<()> {
        if self.path.exists() {
            std::fs::remove_dir_all(&self.path)?;
        }
        self.armed = false;
        Ok(())
    }
}

impl Drop for AttemptDirectoryGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

fn rollback_attempt_owned_publish(
    final_node: &Path,
    node_stage: &Path,
    final_provider: &Path,
    provider_stage: &Path,
) -> Result<()> {
    let mut failures = Vec::new();
    for (published, staged, label) in [
        (final_provider, provider_stage, "provider"),
        (final_node, node_stage, "Node"),
    ] {
        if !published.exists() {
            continue;
        }
        if staged.exists() {
            failures.push(format!("{label} staging destination unexpectedly exists"));
            continue;
        }
        if let Err(error) = std::fs::rename(published, staged) {
            failures.push(format!("could not roll back {label}: {error}"));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(EngineError::InstallFailed(failures.join("; ")))
    }
}

fn publish_provider_pair_with_checks<B, F, R>(
    node_stage: &Path,
    provider_stage: &Path,
    final_node: &Path,
    final_provider: &Path,
    before_node_publish: B,
    before_provider_publish: F,
    readiness: R,
) -> Result<()>
where
    B: FnOnce() -> Result<()>,
    F: FnOnce() -> Result<()>,
    R: FnOnce() -> Result<()>,
{
    if final_node.exists() || final_provider.exists() {
        return Err(EngineError::InstallFailed(
            "refusing to overwrite an existing provider payload; prepare into a fresh offline staging root"
                .to_string(),
        ));
    }
    before_node_publish()?;
    std::fs::rename(node_stage, final_node)?;
    let publish_result = before_provider_publish()
        .and_then(|_| std::fs::rename(provider_stage, final_provider).map_err(EngineError::from));
    if let Err(error) = publish_result {
        let rollback =
            rollback_attempt_owned_publish(final_node, node_stage, final_provider, provider_stage);
        return match rollback {
            Ok(()) => Err(error),
            Err(rollback_error) => Err(EngineError::InstallFailed(format!(
                "provider pair publication failed: {error}; rollback also failed: {rollback_error}"
            ))),
        };
    }
    if let Err(error) = readiness() {
        let rollback =
            rollback_attempt_owned_publish(final_node, node_stage, final_provider, provider_stage);
        return match rollback {
            Ok(()) => Err(error),
            Err(rollback_error) => Err(EngineError::InstallFailed(format!(
                "provider readiness failed: {error}; rollback also failed: {rollback_error}"
            ))),
        };
    }
    Ok(())
}

#[cfg(windows)]
pub fn install_youtube_po_provider(paths: &AppPaths) -> Result<YoutubePoProviderInstallStatus> {
    let _lifecycle_guard = youtube_po_provider_lifecycle_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    paths.ensure_dirs()?;
    // This guard covers recovery as well as claim/download/publication. Without it, a second
    // process can observe the first process's durable owner and incorrectly "recover" a live
    // attempt by deleting its staging tree before the singleton claim is reached.
    let _interprocess_guard = acquire_youtube_po_provider_install_interprocess_lock(
        paths,
        YOUTUBE_PO_PROVIDER_INSTALL_LOCK_TIMEOUT_MS,
    )?;
    reconcile_interrupted_provider_install(paths)?;
    if paths.node_runtime_dir().exists() || paths.youtube_po_provider_dir().exists() {
        let legacy_or_absent = load_provider_installed_identity(paths)?.is_none_or(|identity| {
            identity.lineage_attempt_id.is_empty() && identity.commit_nonce.is_empty()
        });
        if legacy_or_absent {
            // A v47/fresh-profile identity is never accepted in place. Exact executable-pinned
            // final bytes are first bound through the legal v48 transaction, after which the
            // ordinary governed replacement protocol can preserve them during a reinstall.
            adopt_embedded_complete_provider_payload(paths)?;
        }
    }
    clear_provider_node_modules_process_attestation(&paths.youtube_po_provider_server_dir());
    let manifest = pinned_dependency_manifest::manifest();
    let node_pin = &manifest.node_windows;
    let provider_pin = &manifest.youtube_po_provider;
    let attempt_id = uuid::Uuid::new_v4().to_string();
    let ownership_token = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let ownership_token_digest = provider_ownership_token_digest(&ownership_token);
    let commit_nonce = random_provider_authority_nonce();
    let stage_root = paths
        .tools_dir()
        .join(format!("youtube_po_provider_stage_{attempt_id}"));
    // The SQLite singleton is acquired before staging or network work and remains the durable
    // ownership record. The named mutex above prevents another live process from misclassifying
    // that owner as an interrupted attempt while this installation is still running.
    claim_provider_install_owner(
        paths,
        &attempt_id,
        &stage_root,
        &ownership_token_digest,
        &commit_nonce,
    )?;
    let mut operation_guard = ProviderInstallOperationGuard::new(paths, &attempt_id);
    let replacement_guard = prepare_governed_provider_replacement(paths, &attempt_id)?;
    let node_stage = stage_root.join("node");
    let provider_stage = stage_root.join("provider");
    let plugin_stage = provider_stage.join("plugin");
    let server_stage = provider_stage.join("server");
    std::fs::create_dir_all(&stage_root)?;
    let stage_guard = AttemptDirectoryGuard::new(stage_root.clone());

    let node_zip = stage_root.join("node.zip");
    download_verified_file(
        &node_pin.url,
        &node_zip,
        node_pin.file_bytes,
        &node_pin.sha256_hex,
        "Node LTS",
    )?;
    extract_zip_strip_prefix(
        &node_zip,
        &node_stage,
        &format!("node-v{}-win-x64/", node_pin.version),
    )?;
    for (path, expected) in [
        (
            node_stage.join("node.exe"),
            node_pin.node_exe_sha256_hex.as_str(),
        ),
        (
            node_stage.join("npm.cmd"),
            node_pin.npm_cmd_sha256_hex.as_str(),
        ),
    ] {
        let actual = file_sha256_hex(&path).unwrap_or_default();
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(EngineError::HashMismatch {
                path,
                expected: expected.to_string(),
                actual,
            });
        }
    }
    let node_version = tool_version_first_line_with_arg(&node_stage.join("node.exe"), "--version")
        .ok_or_else(|| {
            EngineError::InstallFailed("pinned Node executable is not runnable".to_string())
        })?;
    if node_version.trim_start_matches('v') != node_pin.version {
        return Err(EngineError::InstallFailed(format!(
            "pinned Node version mismatch: expected {}, got {}",
            node_pin.version, node_version
        )));
    }
    let npm_version = tool_version_first_line_with_arg(&node_stage.join("npm.cmd"), "--version")
        .ok_or_else(|| {
            EngineError::InstallFailed("pinned npm executable is not runnable".to_string())
        })?;
    if npm_version != node_pin.npm_version {
        return Err(EngineError::InstallFailed(format!(
            "pinned npm version mismatch: expected {}, got {npm_version}",
            node_pin.npm_version
        )));
    }

    let plugin_zip = stage_root.join("provider_plugin.zip");
    download_verified_file(
        &provider_pin.plugin_url,
        &plugin_zip,
        provider_pin.plugin_file_bytes,
        &provider_pin.plugin_sha256_hex,
        "yt-dlp PO provider plugin",
    )?;
    std::fs::create_dir_all(&plugin_stage)?;
    extract_zip_strip_prefix(&plugin_zip, &plugin_stage, "")?;
    let extracted_plugin_tree =
        provider_plugin_tree_sha256_hex(&plugin_stage, &provider_pin.plugin_files_sha256)
            .ok_or_else(|| {
                EngineError::InstallFailed(
                    "extracted PO provider plugin tree does not match the exact reviewed file set"
                        .to_string(),
                )
            })?;
    if !extracted_plugin_tree.eq_ignore_ascii_case(&provider_pin.plugin_tree_sha256_hex) {
        return Err(EngineError::HashMismatch {
            path: plugin_stage.clone(),
            expected: provider_pin.plugin_tree_sha256_hex.clone(),
            actual: extracted_plugin_tree,
        });
    }

    let source_zip = stage_root.join("provider_source.zip");
    download_verified_file(
        &provider_pin.source_url,
        &source_zip,
        provider_pin.source_file_bytes,
        &provider_pin.source_sha256_hex,
        "PO provider source",
    )?;
    std::fs::create_dir_all(&server_stage)?;
    extract_zip_strip_prefix(
        &source_zip,
        &server_stage,
        &format!(
            "bgutil-ytdlp-pot-provider-{}/server/",
            provider_pin.source_commit
        ),
    )?;
    patch_po_provider_for_localhost(&server_stage)?;

    let lock_path = server_stage.join("package-lock.json");
    let embedded_lock = pinned_dependency_manifest::YOUTUBE_PO_PROVIDER_DERIVED_LOCK;
    if !provider_lock_matches_manifest(
        embedded_lock.as_bytes(),
        &provider_pin.derived_lock_sha256_hex,
    ) || !provider_lock_lifecycle_packages_are_exact(embedded_lock.as_bytes())
    {
        return Err(EngineError::InstallFailed(
            "embedded provider dependency lock failed its manifest hash or exact lifecycle-package policy"
                .to_string(),
        ));
    }
    let normalized_embedded_lock = embedded_lock.replace("\r\n", "\n");
    crate::persistence::atomic_write_text(&lock_path, &normalized_embedded_lock)?;
    let lock_hash = file_sha256_hex(&lock_path).unwrap_or_default();
    if !lock_hash.eq_ignore_ascii_case(&provider_pin.derived_lock_sha256_hex) {
        return Err(EngineError::HashMismatch {
            path: lock_path,
            expected: provider_pin.derived_lock_sha256_hex.clone(),
            actual: lock_hash,
        });
    }
    let audit = require_success(
        run_provider_npm(
            &node_stage,
            &server_stage,
            &["audit", "--omit=dev", "--json"],
        )?,
        "provider production dependency audit",
    )?;
    let audit_json: serde_json::Value = serde_json::from_slice(&audit.stdout).map_err(|error| {
        EngineError::InstallFailed(format!("provider audit returned invalid JSON: {error}"))
    })?;
    let vulnerability_total = audit_json
        .pointer("/metadata/vulnerabilities/total")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(u64::MAX);
    if vulnerability_total != 0 {
        return Err(EngineError::InstallFailed(format!(
            "provider production audit reported {vulnerability_total} vulnerabilities"
        )));
    }
    require_success(
        run_provider_npm(&node_stage, &server_stage, &provider_npm_ci_args())?,
        "provider reproducible npm install",
    )?;
    if !installed_provider_build_lifecycle_packages_are_exact(&server_stage) {
        return Err(EngineError::InstallFailed(
            "installed provider lifecycle-package identities do not match the reviewed lock"
                .to_string(),
        ));
    }
    require_success(
        run_provider_npm(
            &node_stage,
            &server_stage,
            &["rebuild", "canvas", "--ignore-scripts=false"],
        )?,
        "reviewed canvas lifecycle build",
    )?;
    require_success(
        crate::cmd::command(
            server_stage
                .join("node_modules")
                .join(".bin")
                .join("tsc.cmd"),
        )
        .current_dir(&server_stage)
        .owned_output()
        .map_err(|error| {
            EngineError::InstallFailed(format!(
                "provider TypeScript compiler failed to start: {error}"
            ))
        })?,
        "provider TypeScript build",
    )?;
    require_success(
        run_provider_npm(
            &node_stage,
            &server_stage,
            &["prune", "--omit=dev", "--ignore-scripts"],
        )?,
        "provider production dependency prune",
    )?;
    // npm is allowed to normalize package-lock.json during prune. Restore the same reviewed,
    // LF-canonical lock bytes used by the manifest hash so a CRLF checkout cannot publish a
    // different durable payload.
    crate::persistence::atomic_write_text(&lock_path, &normalized_embedded_lock)?;
    let restored_lock_hash = file_sha256_hex(&lock_path).unwrap_or_default();
    if !restored_lock_hash.eq_ignore_ascii_case(&provider_pin.derived_lock_sha256_hex) {
        return Err(EngineError::HashMismatch {
            path: lock_path.clone(),
            expected: provider_pin.derived_lock_sha256_hex.clone(),
            actual: restored_lock_hash,
        });
    }
    let installed_audit = require_success(
        run_provider_npm(
            &node_stage,
            &server_stage,
            &["audit", "--omit=dev", "--json"],
        )?,
        "installed provider production dependency audit",
    )?;
    let installed_audit_json: serde_json::Value = serde_json::from_slice(&installed_audit.stdout)
        .map_err(|error| {
        EngineError::InstallFailed(format!(
            "installed provider audit returned invalid JSON: {error}"
        ))
    })?;
    let installed_vulnerability_total = installed_audit_json
        .pointer("/metadata/vulnerabilities/total")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(u64::MAX);
    if installed_vulnerability_total != 0 {
        return Err(EngineError::InstallFailed(format!(
            "installed provider production audit reported {installed_vulnerability_total} vulnerabilities"
        )));
    }
    // npm's cache indexes/logs and TypeScript's incremental-build index contain run-specific
    // timestamps, staging paths, and ambient parent-workspace declarations. They are build inputs,
    // not runtime dependencies, and would make the sealed application tree location-dependent.
    remove_provider_build_only_artifacts(&server_stage)?;
    let installed_node_modules_tree = authenticate_provider_node_modules_tree(
        &server_stage.join("node_modules"),
        &provider_pin.node_modules_tree_sha256_hex,
    )?;
    if !server_stage.join("build").join("main.js").exists() {
        return Err(EngineError::InstallFailed(
            "provider build did not produce build/main.js".to_string(),
        ));
    }
    let built_entrypoint = server_stage.join("build").join("main.js");
    let built_entrypoint_hash = file_sha256_hex(&built_entrypoint).unwrap_or_default();
    if !built_entrypoint_hash.eq_ignore_ascii_case(&provider_pin.server_entrypoint_sha256_hex) {
        return Err(EngineError::HashMismatch {
            path: built_entrypoint,
            expected: provider_pin.server_entrypoint_sha256_hex.clone(),
            actual: built_entrypoint_hash,
        });
    }
    require_success(
        crate::cmd::command(node_stage.join("node.exe"))
            .args([
                "-e",
                "const c=require('canvas'); if(typeof c.createCanvas!=='function') process.exit(2); c.createCanvas(1,1).toBuffer();",
            ])
            .current_dir(&server_stage)
            .owned_output()
            .map_err(|error| {
                EngineError::InstallFailed(format!(
                    "provider canvas smoke probe failed to start: {error}"
                ))
            })?,
        "provider canvas smoke probe",
    )?;
    let provider_version_probe = require_success(
        crate::cmd::command(node_stage.join("node.exe"))
            .arg(server_stage.join("build").join("generate_once.js"))
            .arg("--version")
            .current_dir(&server_stage)
            .owned_output()
            .map_err(|error| {
                EngineError::InstallFailed(format!(
                    "provider version probe failed to start: {error}"
                ))
            })?,
        "provider generator version probe",
    )?;
    if String::from_utf8_lossy(&provider_version_probe.stdout).trim() != provider_pin.version {
        return Err(EngineError::InstallFailed(
            "provider executable version did not match the pinned source".to_string(),
        ));
    }
    crate::persistence::atomic_write_text(
        &server_stage.join(".production_audit_zero"),
        &format!("{}\n", provider_pin.derived_lock_sha256_hex),
    )?;
    crate::persistence::atomic_write_text(
        &plugin_stage.join(".plugin_archive_sha256"),
        &format!("{}\n", provider_pin.plugin_sha256_hex),
    )?;
    write_provider_ownership_marker(&node_stage, &attempt_id, &ownership_token)?;
    write_provider_ownership_marker(&provider_stage, &attempt_id, &ownership_token)?;
    let node_directory_identity = provider_directory_identity(&node_stage)?;
    let provider_directory_identity = provider_directory_identity(&provider_stage)?;
    let node_tree_sha256 =
        canonical_provider_node_tree_sha256_hex(&node_stage).ok_or_else(|| {
            EngineError::InstallFailed(
                "complete pinned-derived Node distribution tree could not be sealed".to_string(),
            )
        })?;
    let provider_tree_sha256 = canonical_provider_application_tree_sha256_hex(&provider_stage)
        .ok_or_else(|| {
            EngineError::InstallFailed(
                "complete pinned-derived provider application tree could not be sealed".to_string(),
            )
        })?;
    seal_provider_install_lineage(
        paths,
        &attempt_id,
        &stage_root,
        &ownership_token_digest,
        &node_directory_identity,
        &provider_directory_identity,
        &node_tree_sha256,
        &provider_tree_sha256,
    )?;
    persist_provider_install_lineage(paths, &attempt_id, &stage_root, "prepared")?;

    let final_node = paths.node_runtime_dir();
    let final_provider = paths.youtube_po_provider_dir();
    if let Some(parent) = final_node.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            EngineError::InstallFailed(format!(
                "could not prepare the managed Node publication parent {}: {error}",
                parent.display()
            ))
        })?;
    }
    // The operation guard remains armed until the first publication intent is durably committed.
    // A DB/callback failure here is still prepublication and must release the owner for immediate
    // same-process retry instead of manufacturing a live prepared owner that recovery refuses.
    enter_durable_provider_publication(&mut operation_guard, || {
        persist_provider_install_lineage(paths, &attempt_id, &stage_root, "node_publish_intent")
    })?;
    let publish_result = publish_provider_pair_with_checks(
        &node_stage,
        &provider_stage,
        &final_node,
        &final_provider,
        || Ok(()),
        || {
            persist_provider_install_lineage(paths, &attempt_id, &stage_root, "node_published")?;
            persist_provider_install_lineage(
                paths,
                &attempt_id,
                &stage_root,
                "provider_publish_intent",
            )
        },
        || {
            persist_provider_install_lineage(
                paths,
                &attempt_id,
                &stage_root,
                "provider_published",
            )?;
            // Publication changes the executable identity boundary. Re-authenticate the final
            // bytes after both attempt-owned directories are in place; only this exact result
            // may seed the current-process attestation used by readiness and launch.
            let final_tree = authenticate_provider_node_modules_tree(
                &paths.youtube_po_provider_server_dir().join("node_modules"),
                &installed_node_modules_tree,
            )?;
            attest_provider_node_modules_tree(
                &paths.youtube_po_provider_server_dir(),
                &final_tree,
            )?;
            let status = youtube_po_provider_install_status(paths);
            if status.installed {
                verify_published_directory_lineage(
                    &paths.node_runtime_dir(),
                    &node_directory_identity,
                    &node_tree_sha256,
                    canonical_provider_node_tree_sha256_hex,
                    "Node",
                )?;
                verify_published_directory_lineage(
                    &paths.youtube_po_provider_dir(),
                    &provider_directory_identity,
                    &provider_tree_sha256,
                    canonical_provider_application_tree_sha256_hex,
                    "provider",
                )?;
                let committed =
                    commit_provider_installed_identity(paths, &attempt_id, &stage_root)?;
                if !committed.committed {
                    return Err(EngineError::InstallFailed(
                        "provider identity commit did not reach its terminal DB state".to_string(),
                    ));
                }
                Ok(())
            } else {
                Err(EngineError::InstallFailed(
                    status
                        .readiness_error
                        .unwrap_or_else(|| "provider readiness failed after install".to_string()),
                ))
            }
        },
    );
    if let Err(error) = publish_result {
        clear_provider_node_modules_process_attestation(&paths.youtube_po_provider_server_dir());
        let _ = std::fs::remove_file(provider_install_attempt_receipt_path(paths));
        if !final_node.exists() && !final_provider.exists() {
            if let Err(cleanup_error) =
                abort_owned_provider_install_after_complete_rollback(paths, &attempt_id)
            {
                return Err(EngineError::InstallFailed(format!(
                    "provider publication failed: {error}; durable retry cleanup also failed: {cleanup_error}"
                )));
            }
        }
        return Err(error);
    }
    let status = youtube_po_provider_install_status(paths);
    let _ = std::fs::remove_file(provider_attempt_marker(&final_node));
    let _ = std::fs::remove_file(provider_attempt_marker(&final_provider));
    let _ = std::fs::remove_file(provider_install_attempt_receipt_path(paths));
    release_provider_install_owner(paths, &attempt_id)?;
    stage_guard.finish()?;
    if let Some(guard) = replacement_guard {
        guard.preserve_archive();
    }
    Ok(status)
}

#[cfg(not(windows))]
pub fn install_youtube_po_provider(_paths: &AppPaths) -> Result<YoutubePoProviderInstallStatus> {
    Err(EngineError::InstallFailed(
        "automatic YouTube PO provider install is only supported on Windows".to_string(),
    ))
}

#[derive(Debug, Clone, Serialize)]
pub struct YoutubePoProviderRuntimeStatus {
    pub installed: bool,
    pub running: bool,
    pub healthy: bool,
    pub provider_version: String,
    pub port: Option<u16>,
    pub process_id: Option<u32>,
    pub startup_ms: Option<u64>,
    pub error: Option<String>,
}

struct ManagedYoutubePoProvider {
    child: std::process::Child,
    server_dir: PathBuf,
    port: u16,
    provider_version: String,
    install_identity: String,
    startup_ms: u64,
    #[cfg(windows)]
    job_handle: isize,
}

impl Drop for ManagedYoutubePoProvider {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            if self.job_handle != 0 {
                let _ = windows_sys::Win32::Foundation::CloseHandle(self.job_handle as _);
                self.job_handle = 0;
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn youtube_po_provider_slot() -> &'static std::sync::Mutex<Option<ManagedYoutubePoProvider>> {
    static SLOT: std::sync::OnceLock<std::sync::Mutex<Option<ManagedYoutubePoProvider>>> =
        std::sync::OnceLock::new();
    SLOT.get_or_init(|| std::sync::Mutex::new(None))
}

fn youtube_po_provider_lifecycle_lock() -> &'static std::sync::Mutex<()> {
    static LIFECYCLE_LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LIFECYCLE_LOCK.get_or_init(|| std::sync::Mutex::new(()))
}

const YOUTUBE_PO_PROVIDER_INSTALL_LOCK_TIMEOUT_MS: u32 = 5_000;

#[cfg(windows)]
struct YoutubePoProviderInstallInterprocessGuard {
    handle: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(windows)]
impl Drop for YoutubePoProviderInstallInterprocessGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = windows_sys::Win32::System::Threading::ReleaseMutex(self.handle);
            let _ = windows_sys::Win32::Foundation::CloseHandle(self.handle);
        }
    }
}

#[cfg(not(windows))]
struct YoutubePoProviderInstallInterprocessGuard;

#[cfg(not(windows))]
fn acquire_youtube_po_provider_install_interprocess_lock(
    _paths: &AppPaths,
    _timeout_ms: u32,
) -> Result<YoutubePoProviderInstallInterprocessGuard> {
    Ok(YoutubePoProviderInstallInterprocessGuard)
}

#[cfg(windows)]
fn youtube_po_provider_install_interprocess_lock_name(paths: &AppPaths) -> String {
    use sha2::Digest;
    let tools_root = std::fs::canonicalize(paths.tools_dir()).unwrap_or_else(|_| paths.tools_dir());
    let identity = hex::encode_upper(sha2::Sha256::digest(
        tools_root.to_string_lossy().to_ascii_lowercase().as_bytes(),
    ));
    format!(
        "Global\\VoxVulgiYoutubePoProviderInstall-{}",
        &identity[..32]
    )
}

#[cfg(windows)]
fn acquire_youtube_po_provider_install_interprocess_lock(
    paths: &AppPaths,
    timeout_ms: u32,
) -> Result<YoutubePoProviderInstallInterprocessGuard> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::{CreateMutexW, WaitForSingleObject};

    // Global spans Windows Terminal Services sessions. Null SECURITY_ATTRIBUTES applies the
    // creating operator token's default DACL: same-user console/RDP processes coordinate while
    // unrelated user SIDs do not receive an intentionally permissive ACL.
    let lock_name =
        std::ffi::OsStr::new(&youtube_po_provider_install_interprocess_lock_name(paths))
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
    let handle = unsafe { CreateMutexW(std::ptr::null(), 0, lock_name.as_ptr()) };
    if handle.is_null() {
        return Err(EngineError::InstallFailed(format!(
            "could not create the YouTube provider install lock: {}",
            std::io::Error::last_os_error()
        )));
    }
    let wait = unsafe { WaitForSingleObject(handle, timeout_ms) };
    if wait == WAIT_OBJECT_0 || wait == WAIT_ABANDONED {
        return Ok(YoutubePoProviderInstallInterprocessGuard { handle });
    }
    unsafe {
        let _ = windows_sys::Win32::Foundation::CloseHandle(handle);
    }
    if wait == WAIT_TIMEOUT {
        return Err(EngineError::InstallFailed(
            "another VoxVulgi process is installing or recovering the YouTube provider; retry after it finishes"
                .to_string(),
        ));
    }
    Err(EngineError::InstallFailed(format!(
        "could not acquire the YouTube provider install lock: {}",
        std::io::Error::last_os_error()
    )))
}

#[cfg(windows)]
fn assign_kill_on_parent_exit(child: &std::process::Child) -> Result<isize> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            return Err(EngineError::InstallFailed(
                "could not create the provider lifecycle job".to_string(),
            ));
        }
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let configured = SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const std::ffi::c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        );
        let assigned = if configured != 0 {
            AssignProcessToJobObject(job, child.as_raw_handle() as _)
        } else {
            0
        };
        if configured == 0 || assigned == 0 {
            let _ = windows_sys::Win32::Foundation::CloseHandle(job);
            return Err(EngineError::InstallFailed(
                "could not bind the provider process to the app lifecycle".to_string(),
            ));
        }
        Ok(job as isize)
    }
}

fn ping_youtube_po_provider(port: u16) -> Option<String> {
    let mut config = ureq::Agent::config_builder();
    config = config.timeout_global(Some(std::time::Duration::from_secs(2)));
    let agent: ureq::Agent = config.build().into();
    let mut response = agent
        .get(&format!("http://127.0.0.1:{port}/ping"))
        .call()
        .ok()?;
    let body = response.body_mut().read_to_string().ok()?;
    let payload: serde_json::Value = serde_json::from_str(&body).ok()?;
    payload
        .get("version")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

fn provider_install_identity(status: &YoutubePoProviderInstallStatus) -> String {
    format!(
        "node={}|node_sha={}|npm_sha={}|plugin={}|server={}|lock={}|node_modules={}",
        status.node_version.as_deref().unwrap_or("missing"),
        status.node_exe_sha256_hex.as_deref().unwrap_or("missing"),
        status.npm_cmd_sha256_hex.as_deref().unwrap_or("missing"),
        status
            .plugin_tree_sha256_hex
            .as_deref()
            .unwrap_or("missing"),
        status
            .server_entrypoint_sha256_hex
            .as_deref()
            .unwrap_or("missing"),
        status
            .derived_lock_sha256_hex
            .as_deref()
            .unwrap_or("missing"),
        status
            .node_modules_tree_sha256_hex
            .as_deref()
            .unwrap_or("missing"),
    )
}

pub fn youtube_po_provider_runtime_status(paths: &AppPaths) -> YoutubePoProviderRuntimeStatus {
    let installed = youtube_po_provider_install_status(paths);
    let identity = provider_install_identity(&installed);
    let mut slot = youtube_po_provider_slot()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut clear = false;
    let status = if let Some(managed) = slot.as_mut() {
        let running = managed.child.try_wait().ok().flatten().is_none();
        let health_version = running
            .then(|| ping_youtube_po_provider(managed.port))
            .flatten();
        let healthy = managed.server_dir == paths.youtube_po_provider_server_dir()
            && health_version.as_deref() == Some(managed.provider_version.as_str())
            && managed.install_identity == identity;
        clear = !running
            || managed.server_dir != paths.youtube_po_provider_server_dir()
            || managed.install_identity != identity;
        YoutubePoProviderRuntimeStatus {
            installed: installed.installed,
            running,
            healthy,
            provider_version: managed.provider_version.clone(),
            port: Some(managed.port),
            process_id: Some(managed.child.id()),
            startup_ms: Some(managed.startup_ms),
            error: if healthy {
                None
            } else {
                Some("localhost provider health check failed".to_string())
            },
        }
    } else {
        YoutubePoProviderRuntimeStatus {
            installed: installed.installed,
            running: false,
            healthy: false,
            provider_version: installed.provider_version,
            port: None,
            process_id: None,
            startup_ms: None,
            error: installed
                .readiness_error
                .or_else(|| Some("localhost provider is stopped".to_string())),
        }
    };
    if clear {
        *slot = None;
    }
    status
}

pub fn ensure_youtube_po_provider(paths: &AppPaths) -> Result<YoutubePoProviderRuntimeStatus> {
    paths.ensure_dirs()?;
    let server_dir = paths.youtube_po_provider_server_dir();
    let lifecycle = youtube_po_provider_lifecycle_lock();
    let (_lifecycle_guard, waited_for_active_flight) = match lifecycle.try_lock() {
        Ok(guard) => (guard, false),
        Err(std::sync::TryLockError::WouldBlock) => (
            lifecycle
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            true,
        ),
        Err(std::sync::TryLockError::Poisoned(error)) => (error.into_inner(), false),
    };
    if provider_node_modules_integrity_verifying().load(std::sync::atomic::Ordering::Acquire) {
        return Err(EngineError::InstallFailed(
            "provider dependency integrity verification is still in progress".to_string(),
        ));
    }

    // Every execution gate re-authenticates the complete authoritative trees while both
    // lifecycle locks are held. A healthy child cannot authorize later lazy module loads after
    // same-process filesystem tamper. A failed scan also tears down and reaps the owned child.
    if let Err(error) = verify_youtube_po_provider_node_modules_single_flight_locked(
        paths,
        &server_dir,
        waited_for_active_flight,
    ) {
        *youtube_po_provider_slot()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        return Err(error);
    }
    let installed = youtube_po_provider_install_status(paths);
    if !installed.installed {
        *youtube_po_provider_slot()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        clear_provider_node_modules_process_attestation(&server_dir);
        let readiness_error = installed
            .readiness_error
            .unwrap_or_else(|| "PO provider payload is unavailable".to_string());
        provider_node_modules_process_invalidations()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(server_dir, readiness_error.clone());
        return Err(EngineError::InstallFailed(readiness_error));
    }
    let identity = provider_install_identity(&installed);
    let mut slot = youtube_po_provider_slot()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(managed) = slot.as_mut() {
        if managed.server_dir == server_dir
            && managed.install_identity == identity
            && managed.child.try_wait()?.is_none()
            && ping_youtube_po_provider(managed.port).as_deref()
                == Some(managed.provider_version.as_str())
        {
            return Ok(YoutubePoProviderRuntimeStatus {
                installed: true,
                running: true,
                healthy: true,
                provider_version: managed.provider_version.clone(),
                port: Some(managed.port),
                process_id: Some(managed.child.id()),
                startup_ms: Some(managed.startup_ms),
                error: None,
            });
        }
        *slot = None;
    }
    drop(slot);

    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    let port = listener.local_addr()?.port();
    drop(listener);
    let log_dir = paths.install_logs_dir().join("youtube_po_provider");
    std::fs::create_dir_all(&log_dir)?;
    let stdout = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_dir.join("stdout.log"))?;
    let stderr = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_dir.join("stderr.log"))?;
    let mut child = crate::cmd::command(paths.node_exe())
        .arg(paths.youtube_po_provider_entrypoint())
        .arg("--port")
        .arg(port.to_string())
        .current_dir(paths.youtube_po_provider_server_dir())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::from(stdout))
        .stderr(std::process::Stdio::from(stderr))
        .spawn()
        .map_err(|error| {
            EngineError::InstallFailed(format!("could not start localhost PO provider: {error}"))
        })?;
    #[cfg(windows)]
    let job_handle = match assign_kill_on_parent_exit(&child) {
        Ok(handle) => handle,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    };
    let started = std::time::Instant::now();
    let deadline = started + std::time::Duration::from_secs(60);
    loop {
        if let Some(exit) = child.try_wait()? {
            return Err(EngineError::InstallFailed(format!(
                "localhost PO provider exited during startup (code={:?})",
                exit.code()
            )));
        }
        if ping_youtube_po_provider(port).as_deref() == Some(installed.provider_version.as_str()) {
            break;
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(EngineError::InstallFailed(
                "localhost PO provider did not become healthy within 60 seconds".to_string(),
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    let startup_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    let status = YoutubePoProviderRuntimeStatus {
        installed: true,
        running: true,
        healthy: true,
        provider_version: installed.provider_version.clone(),
        port: Some(port),
        process_id: Some(child.id()),
        startup_ms: Some(startup_ms),
        error: None,
    };
    let mut slot = youtube_po_provider_slot()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    *slot = Some(ManagedYoutubePoProvider {
        child,
        server_dir,
        port,
        provider_version: installed.provider_version,
        install_identity: identity,
        startup_ms,
        #[cfg(windows)]
        job_handle,
    });
    Ok(status)
}

pub fn request_youtube_po_provider_start(paths: &AppPaths) -> bool {
    use std::sync::atomic::{AtomicBool, Ordering};
    static START_REQUESTED: AtomicBool = AtomicBool::new(false);
    if youtube_po_provider_runtime_status(paths).healthy {
        return false;
    }
    if START_REQUESTED
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return false;
    }
    let paths = paths.clone();
    std::thread::Builder::new()
        .name("youtube-po-provider-start".to_string())
        .spawn(move || {
            let _ = ensure_youtube_po_provider(&paths);
            START_REQUESTED.store(false, Ordering::Release);
        })
        .map(|_| true)
        .unwrap_or_else(|_| {
            START_REQUESTED.store(false, Ordering::Release);
            false
        })
}

pub fn shutdown_youtube_po_provider() {
    let _lifecycle_guard = youtube_po_provider_lifecycle_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut slot = youtube_po_provider_slot()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    *slot = None;
    provider_node_modules_process_attestations()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clear();
    provider_node_modules_process_invalidations()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clear();
}

fn unpinned_fallback_disabled_error(context: &str, pinned_err: &EngineError) -> EngineError {
    EngineError::InstallFailed(format!(
        "{context} failed after pinned install error: {pinned_err}. Mutable fallback installs are disabled by default. Set {}=1 to opt in for local recovery runs.",
        pinned_dependency_manifest::allow_unpinned_fallback_env_name()
    ))
}

fn pip_install_args<'a>(prefix: &[&'a str], packages: &'a [String]) -> Vec<&'a str> {
    let mut args = prefix.to_vec();
    args.extend(packages.iter().map(String::as_str));
    args
}

/// WP-0232 (shipping fix): return the bundled lockfile content for the given pack.
///
/// The lockfile JSON is baked into the engine binary at compile time via
/// `include_str!` in `python_lockfile.rs`, so this works on every end-user install
/// regardless of where the exe lives or whether the source tree exists. Returns
/// `None` for packs without a bundled lockfile (currently only spleeter — its pin set
/// is unbuildable on Py 3.11 per the WP-0232 manifest-defect note).
fn locate_pack_lockfile(pack_name: &str) -> Option<&'static str> {
    python_lockfile::bundled_lockfile_for_pack(pack_name)
}

/// WP-0232: install a Python pack from its hashed lockfile.
/// WP-0234: also journals install state and promotes `--upgrade` to `--force-reinstall`
/// when the prior install attempt is recorded as crashed (in_progress without finish) or
/// failed. Recovers the venv from bad state without an operator-visible "Repair" click.
///
/// Renders the lockfile to a `requirements.txt` and runs
/// `pip install --require-hashes --no-deps {--upgrade|--force-reinstall} -r <file>`.
/// The resolver is bypassed entirely; pip just downloads each pinned URL and verifies
/// the sha256. Any wheel that fails the hash check causes pip to exit non-zero
/// immediately.
fn install_pack_from_lockfile(
    paths: &AppPaths,
    python: &Path,
    pack_name: &str,
    lockfile_json: &str,
    error_prefix: &str,
) -> Result<()> {
    let lockfile = PythonLockfile::from_bundled_str(pack_name, lockfile_json).map_err(|e| {
        EngineError::InstallFailed(format!(
            "{error_prefix}: failed to parse bundled lockfile for {pack_name}: {e}"
        ))
    })?;
    let rendered = lockfile.render_hashed_requirements().map_err(|e| {
        EngineError::InstallFailed(format!(
            "{error_prefix}: failed to render lockfile for {pack_name}: {e}"
        ))
    })?;

    // Write the rendered requirements to a temp file inside the venv tooling dir so the
    // path is short and stable; pip on Windows occasionally barfs on TEMP paths with
    // spaces in them, and APPDATA paths often have spaces.
    let req_dir = paths.python_models_dir().join(".lockfile_requirements");
    std::fs::create_dir_all(&req_dir).map_err(|e| {
        EngineError::InstallFailed(format!(
            "{error_prefix}: failed to create requirements dir {}: {e}",
            req_dir.display()
        ))
    })?;
    let req_path = req_dir.join(format!("{pack_name}.requirements.txt"));
    std::fs::write(&req_path, rendered.as_bytes()).map_err(|e| {
        EngineError::InstallFailed(format!(
            "{error_prefix}: failed to write requirements file {}: {e}",
            req_path.display()
        ))
    })?;

    // WP-0234: decide upgrade mode based on prior install state.
    let lockfile_sha = pack_install_state::lockfile_sha_of(&rendered);
    let prior = pack_install_state::load(paths, pack_name);
    let version_drift_before_install =
        !lockfile_source_pin_mismatches(python, pack_name).is_empty();
    let force = prior.last_outcome.requires_force_reinstall() || version_drift_before_install;
    let install_mode_flag = if force {
        "--force-reinstall"
    } else {
        "--upgrade"
    };
    if force {
        cleanup_stale_distribution_metadata(python, pack_name);
    }
    let _ = pack_install_state::mark_started(paths, pack_name, &lockfile_sha);

    let req_path_str = req_path.to_string_lossy().to_string();
    let args: [&str; 8] = [
        "-m",
        "pip",
        "install",
        "--require-hashes",
        "--no-deps",
        install_mode_flag,
        "-r",
        req_path_str.as_str(),
    ];
    let mut result = run_python_checked_with_timeout(
        paths,
        python,
        &args,
        &format!("{error_prefix}: pip install --require-hashes failed for {pack_name}"),
        PYTHON_LOCKFILE_INSTALL_TIMEOUT_SECS,
    );
    if result.is_ok() {
        let mismatches = lockfile_source_pin_mismatches_with_retries(python, pack_name);
        if let Some(first) = mismatches.first() {
            result = Err(EngineError::InstallFailed(format!(
                "{error_prefix}: post-install version check failed for {pack_name}: {} expected {}, installed {}",
                first.package,
                first.expected,
                first.installed.as_deref().unwrap_or("missing")
            )));
        }
    }

    match &result {
        Ok(()) => {
            let _ = pack_install_state::mark_completed(paths, pack_name, &lockfile_sha);
        }
        Err(err) => {
            let _ = pack_install_state::mark_failed(paths, pack_name, &err.to_string());
        }
    }
    result
}

fn current_lockfile_sha(pack_name: &str) -> Option<String> {
    let lockfile_json = locate_pack_lockfile(pack_name)?;
    let lockfile = PythonLockfile::from_bundled_str(pack_name, lockfile_json).ok()?;
    let rendered = lockfile.render_hashed_requirements().ok()?;
    Some(pack_install_state::lockfile_sha_of(&rendered))
}

fn pack_install_satisfied(paths: &AppPaths, pack_name: &str) -> bool {
    let Some(expected_sha) = current_lockfile_sha(pack_name) else {
        return false;
    };
    pack_install_state::load(paths, pack_name).is_completed_with_lockfile(&expected_sha)
}

fn pack_lockfile_runtime_ready(lockfile_ready: bool, versions_ready: bool) -> bool {
    lockfile_ready || versions_ready
}

fn pack_install_state_shas(paths: &AppPaths, pack_name: &str) -> (Option<String>, Option<String>) {
    let expected_lockfile_sha = current_lockfile_sha(pack_name);
    let state = pack_install_state::load(paths, pack_name);
    let installed_lockfile_sha = if state.lockfile_sha.is_empty() {
        None
    } else {
        Some(state.lockfile_sha)
    };
    (expected_lockfile_sha, installed_lockfile_sha)
}

#[derive(Debug, Clone, Serialize)]
pub struct PythonPackageVersionMismatch {
    pub package: String,
    pub expected: String,
    pub installed: Option<String>,
}

fn lockfile_source_pin_versions(pack_name: &str) -> Vec<(String, String)> {
    let Some(lockfile_json) = locate_pack_lockfile(pack_name) else {
        return Vec::new();
    };
    let Ok(lockfile) = PythonLockfile::from_bundled_str(pack_name, lockfile_json) else {
        return Vec::new();
    };

    lockfile
        .source_pins
        .iter()
        .filter_map(|pin| {
            let (name, version) = pin.split_once("==")?;
            Some((name.trim().to_string(), version.trim().to_string()))
        })
        .collect()
}

fn normalize_python_package_name(name: &str) -> String {
    name.to_ascii_lowercase()
        .replace('_', "-")
        .replace('.', "-")
}

fn status_pin_names(pack_name: &str) -> Option<&'static [&'static str]> {
    match pack_name {
        "tts_neural_local_v1" => Some(&[
            "kokoro",
            "numpy",
            "torch",
            "transformers",
            "huggingface-hub",
        ]),
        "tts_voice_preserving_local_v1" => Some(&[
            "huggingface-hub",
            "numpy",
            "librosa",
            "soundfile",
            "inflect",
            "unidecode",
            "eng-to-ipa",
            "pypinyin",
            "cn2an",
            "jieba",
        ]),
        "diarization" => Some(&[
            "resemblyzer",
            "numpy",
            "scikit-learn",
            "librosa",
            "numba",
            "llvmlite",
            "webrtcvad",
            "soundfile",
        ]),
        _ => None,
    }
}

fn python_site_packages_dir(python: &std::path::Path) -> Option<PathBuf> {
    let scripts_dir = python.parent()?;
    let venv_dir = scripts_dir.parent()?;
    let site_packages = venv_dir.join("Lib").join("site-packages");
    if site_packages.is_dir() {
        Some(site_packages)
    } else {
        None
    }
}

fn dist_info_distribution_name(file_name: &str) -> Option<String> {
    dist_info_distribution_name_and_version(file_name).map(|(name, _)| name)
}

fn dist_info_distribution_name_and_version(file_name: &str) -> Option<(String, String)> {
    let stem = file_name.strip_suffix(".dist-info")?;
    let (name, _) = stem.rsplit_once('-')?;
    let version = stem.rsplit_once('-')?.1.to_string();
    Some((normalize_python_package_name(name), version))
}

fn dist_info_name_matches_package(dist_name: &str, package_name: &str) -> bool {
    dist_name == package_name
        || dist_name
            .strip_prefix('~')
            .map(|backup_name| package_name.ends_with(backup_name))
            .unwrap_or(false)
}

fn quarantine_python_artifact(path: &std::path::Path, quarantine_dir: &std::path::Path) {
    let _ = std::fs::create_dir_all(quarantine_dir);
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "python_artifact".to_string());
    let mut dest = quarantine_dir.join(format!("{}_{}", now_ms(), file_name));
    let mut counter = 0;
    while dest.exists() {
        counter += 1;
        dest = quarantine_dir.join(format!("{}_{}_{}", now_ms(), counter, file_name));
    }
    if std::fs::rename(path, &dest).is_err() {
        let _ = std::fs::remove_dir_all(path);
    }
}

fn cleanup_stale_distribution_metadata(python: &std::path::Path, pack_name: &str) {
    let Some(site_packages) = python_site_packages_dir(python) else {
        return;
    };
    let mismatches = lockfile_source_pin_mismatches(python, pack_name);
    let mut packages = if mismatches.is_empty() {
        lockfile_source_pin_versions(pack_name)
            .into_iter()
            .map(|(name, _)| name)
            .collect::<Vec<_>>()
    } else {
        mismatches
            .into_iter()
            .map(|mismatch| mismatch.package)
            .collect::<Vec<_>>()
    };
    if let Some(status_names) = status_pin_names(pack_name) {
        let normalized_status_names = status_names
            .iter()
            .map(|name| normalize_python_package_name(name))
            .collect::<Vec<_>>();
        packages.retain(|name| {
            let normalized = normalize_python_package_name(name);
            normalized_status_names
                .iter()
                .any(|status_name| status_name == &normalized)
        });
    }
    let normalized_packages = packages
        .iter()
        .map(|name| normalize_python_package_name(name))
        .collect::<Vec<_>>();
    let Ok(entries) = std::fs::read_dir(site_packages) else {
        return;
    };
    let quarantine_dir = python_site_packages_dir(python)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("_voxvulgi_stale_python_artifacts");
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let file_name = entry.file_name().to_string_lossy().to_string();
        let normalized_file_name = normalize_python_package_name(&file_name);
        let dist_name = dist_info_distribution_name(&file_name);
        let matches_target = normalized_packages.iter().any(|package_name| {
            dist_name
                .as_ref()
                .map(|dist_name| dist_info_name_matches_package(dist_name, package_name))
                .unwrap_or(false)
                || normalized_file_name == *package_name
                || normalized_file_name
                    .strip_prefix('~')
                    .map(|backup_name| package_name.ends_with(backup_name))
                    .unwrap_or(false)
        });
        if matches_target {
            quarantine_python_artifact(&entry.path(), &quarantine_dir);
        }
    }
}

fn python_distribution_versions(
    python: &std::path::Path,
    distributions: &[String],
) -> HashMap<String, Option<String>> {
    if distributions.is_empty() {
        return HashMap::new();
    }
    let names_json = serde_json::to_string(distributions).unwrap_or_else(|_| "[]".to_string());
    let code = format!(
        "import importlib.metadata as m, json\n\
         names = {names_json}\n\
         out = {{}}\n\
         for name in names:\n\
             try:\n\
                 out[name] = m.version(name)\n\
             except Exception:\n\
                 out[name] = None\n\
         print(json.dumps(out))\n"
    );
    let output = match crate::cmd::command(python)
        .args(["-c", &code])
        .env("PYTHONNOUSERSITE", "1")
        .owned_output()
    {
        Ok(output) => output,
        Err(_) => {
            return distributions
                .iter()
                .map(|name| {
                    (
                        name.clone(),
                        python_distribution_version_from_site_packages(python, name),
                    )
                })
                .collect()
        }
    };
    if !output.status.success() {
        return distributions
            .iter()
            .map(|name| {
                (
                    name.clone(),
                    python_distribution_version_from_site_packages(python, name),
                )
            })
            .collect();
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut parsed = serde_json::from_str::<HashMap<String, Option<String>>>(&text)
        .unwrap_or_else(|_| HashMap::new());
    for name in distributions {
        if parsed.get(name).and_then(|value| value.as_ref()).is_none() {
            if let Some(version) = python_distribution_version_from_site_packages(python, name) {
                parsed.insert(name.clone(), Some(version));
            }
        }
    }
    parsed
}

fn python_distribution_version_from_site_packages(
    python: &std::path::Path,
    distribution: &str,
) -> Option<String> {
    let site_packages = python_site_packages_dir(python)?;
    let wanted = normalize_python_package_name(distribution);
    let entries = std::fs::read_dir(site_packages).ok()?;
    for entry in entries.flatten() {
        let file_name = entry.file_name().to_string_lossy().to_string();
        let Some((dist_name, folder_version)) = dist_info_distribution_name_and_version(&file_name)
        else {
            continue;
        };
        if !dist_info_name_matches_package(&dist_name, &wanted) {
            continue;
        }
        let metadata_path = entry.path().join("METADATA");
        if let Ok(metadata) = std::fs::read_to_string(metadata_path) {
            for line in metadata.lines() {
                if let Some(version) = line.strip_prefix("Version:") {
                    let trimmed = version.trim();
                    if !trimmed.is_empty() {
                        return Some(trimmed.to_string());
                    }
                }
            }
        }
        if !folder_version.trim().is_empty() {
            return Some(folder_version);
        }
    }
    None
}

fn lockfile_source_pin_mismatches(
    python: &std::path::Path,
    pack_name: &str,
) -> Vec<PythonPackageVersionMismatch> {
    let mut pins = lockfile_source_pin_versions(pack_name);
    if let Some(status_names) = status_pin_names(pack_name) {
        let normalized_status_names = status_names
            .iter()
            .map(|name| normalize_python_package_name(name))
            .collect::<Vec<_>>();
        pins.retain(|(name, _)| {
            let normalized = normalize_python_package_name(name);
            normalized_status_names
                .iter()
                .any(|status_name| status_name == &normalized)
        });
    }
    let names = pins
        .iter()
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    let installed_versions = python_distribution_versions(python, &names);

    pins.into_iter()
        .filter_map(|(package, expected)| {
            let installed = installed_versions.get(&package).cloned().flatten();
            if installed.as_deref() == Some(expected.as_str()) {
                None
            } else {
                Some(PythonPackageVersionMismatch {
                    package,
                    expected,
                    installed,
                })
            }
        })
        .collect()
}

fn lockfile_source_pin_mismatches_with_retries(
    python: &std::path::Path,
    pack_name: &str,
) -> Vec<PythonPackageVersionMismatch> {
    let mut mismatches = lockfile_source_pin_mismatches(python, pack_name);
    for _ in 0..PYTHON_POST_INSTALL_VERSION_CHECK_RETRIES {
        if mismatches.is_empty() {
            return mismatches;
        }
        std::thread::sleep(std::time::Duration::from_millis(
            PYTHON_POST_INSTALL_VERSION_CHECK_DELAY_MS,
        ));
        mismatches = lockfile_source_pin_mismatches(python, pack_name);
    }
    mismatches
}

fn tool_version_first_line_with_arg(
    program: impl AsRef<std::ffi::OsStr>,
    arg: &str,
) -> Option<String> {
    let output = crate::cmd::command(program).arg(arg).owned_output().ok()?;
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Phase2PacksSetupEstimate {
    pub schema_version: u32,
    pub download_bytes: u64,
    pub reference_mbps: u32,
    pub min_minutes: u32,
    pub max_minutes: u32,
    pub basis: String,
}

pub fn phase2_packs_setup_estimate() -> Phase2PacksSetupEstimate {
    static ESTIMATE: OnceLock<Phase2PacksSetupEstimate> = OnceLock::new();
    ESTIMATE
        .get_or_init(|| {
            serde_json::from_str(include_str!(
                "../resources/tooling/phase2_setup_estimate.json"
            ))
            .expect("Phase 2 setup estimate manifest must parse")
        })
        .clone()
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
        Phase2PackPlanItem {
            id: "voice_clone_cosyvoice_v1".to_string(),
            title: "Voice-preserving dub (CosyVoice 2)".to_string(),
            supported: true,
            estimated_bytes: None,
        },
    ]
}

pub fn phase2_pack_step_satisfied(paths: &AppPaths, step_id: &str) -> bool {
    match step_id {
        "portable_python_win64" => python_toolchain_status(paths).base_available,
        "python_toolchain" => python_toolchain_status(paths).venv_python_version.is_some(),
        "spleeter" => spleeter_pack_status(paths).installed,
        "demucs" => demucs_pack_status(paths).installed,
        "diarization" => diarization_pack_status(paths).installed,
        "tts_preview" => tts_preview_pack_status(paths).installed,
        "tts_neural_local_v1" => tts_neural_local_v1_pack_status(paths).installed,
        "tts_voice_preserving_local_v1" => {
            tts_voice_preserving_local_v1_pack_status(paths).installed
        }
        "voice_clone_cosyvoice_v1" => cosyvoice_pack_status(paths).installed,
        _ => false,
    }
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
        pinned_dependency_manifest: serde_json::Value,
        allow_unpinned_fallback_env: String,
        allow_unpinned_fallback_enabled: bool,
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
        pinned_dependency_manifest: pinned_dependency_manifest::manifest_json_value(),
        allow_unpinned_fallback_env: pinned_dependency_manifest::allow_unpinned_fallback_env_name()
            .to_string(),
        allow_unpinned_fallback_enabled: pinned_dependency_manifest::allow_unpinned_fallback(),
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
    crate::persistence::atomic_write_text(&out_path, &format!("{json}\n"))?;
    let file_bytes = std::fs::metadata(&out_path).map(|m| m.len()).unwrap_or(0);

    Ok(PackIntegrityManifestResult {
        out_path: out_path.to_string_lossy().to_string(),
        file_bytes,
        generated_at_ms,
    })
}

const CAPABILITY_PROBE_CACHE_TTL_MS: i64 = 30_000;
const CAPABILITY_PROBE_WAITER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);
const CAPABILITY_PROBE_CHILD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(90);

#[derive(Debug, Clone)]
struct SemanticProbeCache<T> {
    source_identity: String,
    verified_at_ms: i64,
    value: T,
}

#[derive(Debug)]
struct SemanticProbeState<T> {
    running: bool,
    epoch: u64,
    next_flight_id: u64,
    active_flight_id: Option<u64>,
    active_waiters: usize,
    cache: Option<SemanticProbeCache<T>>,
    terminal_by_flight: HashMap<u64, SharedSemanticProbeTerminal<T>>,
}

#[derive(Debug, Clone)]
struct SharedSemanticProbeTerminal<T> {
    source_identity: String,
    remaining_waiters: usize,
    outcome: SemanticProbeOutcome<T>,
}

#[derive(Debug)]
struct SemanticProbeSlot<T> {
    state: Mutex<SemanticProbeState<T>>,
    wake: Condvar,
}

#[derive(Debug, Clone)]
struct SemanticProbeOutcome<T> {
    value: Option<T>,
    verified_at_ms: i64,
    source_identity: String,
    freshness: &'static str,
    shared_flight: bool,
    probe_state: &'static str,
    error: Option<String>,
}

impl<T: Clone> SemanticProbeSlot<T> {
    fn new() -> Self {
        Self {
            state: Mutex::new(SemanticProbeState {
                running: false,
                epoch: 0,
                next_flight_id: 0,
                active_flight_id: None,
                active_waiters: 0,
                cache: None,
                terminal_by_flight: HashMap::new(),
            }),
            wake: Condvar::new(),
        }
    }

    fn run<F>(&self, source_identity: String, compute: F) -> SemanticProbeOutcome<T>
    where
        F: FnOnce() -> std::result::Result<T, String>,
    {
        let mut waited_for_shared_flight = false;
        let mut joined_flight_id: Option<u64> = None;
        let mut compute = Some(compute);
        loop {
            let now = now_ms();
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(flight_id) = joined_flight_id {
                let terminal = state.terminal_by_flight.get(&flight_id).cloned();
                if let Some(mut terminal) = terminal {
                    let remove_terminal =
                        if let Some(stored) = state.terminal_by_flight.get_mut(&flight_id) {
                            stored.remaining_waiters = stored.remaining_waiters.saturating_sub(1);
                            stored.remaining_waiters == 0
                        } else {
                            false
                        };
                    if remove_terminal {
                        state.terminal_by_flight.remove(&flight_id);
                    }
                    joined_flight_id = None;
                    if terminal.source_identity == source_identity {
                        terminal.outcome.shared_flight = true;
                        if terminal.outcome.probe_state == "verified" {
                            terminal.outcome.freshness = "shared_flight";
                        }
                        return terminal.outcome;
                    }
                }
            }
            if let Some(cache) = state.cache.as_ref().filter(|cache| {
                cache.source_identity == source_identity
                    && now.saturating_sub(cache.verified_at_ms) <= CAPABILITY_PROBE_CACHE_TTL_MS
            }) {
                return SemanticProbeOutcome {
                    value: Some(cache.value.clone()),
                    verified_at_ms: cache.verified_at_ms,
                    source_identity,
                    freshness: if waited_for_shared_flight {
                        "shared_flight"
                    } else {
                        "cached"
                    },
                    shared_flight: waited_for_shared_flight,
                    probe_state: "verified",
                    error: None,
                };
            }
            if state.running {
                if joined_flight_id.is_none() {
                    joined_flight_id = state.active_flight_id;
                    state.active_waiters = state.active_waiters.saturating_add(1);
                }
                let (next, timeout) = self
                    .wake
                    .wait_timeout(state, CAPABILITY_PROBE_WAITER_TIMEOUT)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                state = next;
                if timeout.timed_out() && state.running {
                    if state.active_flight_id == joined_flight_id {
                        state.active_waiters = state.active_waiters.saturating_sub(1);
                    }
                    let stale = state
                        .cache
                        .as_ref()
                        .filter(|cache| cache.source_identity == source_identity)
                        .cloned();
                    drop(state);
                    return SemanticProbeOutcome {
                        value: stale.as_ref().map(|cache| cache.value.clone()),
                        verified_at_ms: stale.as_ref().map(|cache| cache.verified_at_ms).unwrap_or(0),
                        source_identity: source_identity.clone(),
                        freshness: if stale.is_some() { "stale_timeout" } else { "timeout" },
                        shared_flight: true,
                        probe_state: "timeout",
                        error: Some("semantic probe waiter timed out before the shared native probe completed".to_string()),
                    };
                }
                waited_for_shared_flight = true;
                continue;
            }
            state.running = true;
            state.next_flight_id = state.next_flight_id.wrapping_add(1);
            let flight_id = state.next_flight_id;
            state.active_flight_id = Some(flight_id);
            state.active_waiters = 0;
            let flight_epoch = state.epoch;
            drop(state);

            let computed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                compute
                    .take()
                    .expect("probe computation starts exactly once"),
            ));
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.running = false;
            state.active_flight_id = None;
            let joined_waiters = std::mem::take(&mut state.active_waiters);
            match computed {
                Ok(Ok(value)) if state.epoch == flight_epoch => {
                    let verified_at_ms = now_ms();
                    state.cache = Some(SemanticProbeCache {
                        source_identity: source_identity.clone(),
                        verified_at_ms,
                        value: value.clone(),
                    });
                    let outcome = SemanticProbeOutcome {
                        value: Some(value),
                        verified_at_ms,
                        source_identity: source_identity.clone(),
                        freshness: "verified",
                        shared_flight: false,
                        probe_state: "verified",
                        error: None,
                    };
                    if joined_waiters > 0 {
                        state.terminal_by_flight.insert(
                            flight_id,
                            SharedSemanticProbeTerminal {
                                source_identity: source_identity.clone(),
                                remaining_waiters: joined_waiters,
                                outcome: outcome.clone(),
                            },
                        );
                    }
                    self.wake.notify_all();
                    return outcome;
                }
                Ok(Ok(_)) => {
                    let outcome = SemanticProbeOutcome {
                        value: None,
                        verified_at_ms: 0,
                        source_identity: source_identity.clone(),
                        freshness: "superseded",
                        shared_flight: false,
                        probe_state: "superseded",
                        error: Some("probe result was discarded because its source changed during execution".to_string()),
                    };
                    if joined_waiters > 0 {
                        state.terminal_by_flight.insert(
                            flight_id,
                            SharedSemanticProbeTerminal {
                                source_identity: source_identity.clone(),
                                remaining_waiters: joined_waiters,
                                outcome: outcome.clone(),
                            },
                        );
                    }
                    self.wake.notify_all();
                    return outcome;
                }
                Ok(Err(error)) => {
                    let stale = state
                        .cache
                        .as_ref()
                        .filter(|cache| cache.source_identity == source_identity)
                        .cloned();
                    let outcome = SemanticProbeOutcome {
                        value: stale.as_ref().map(|cache| cache.value.clone()),
                        verified_at_ms: stale
                            .as_ref()
                            .map(|cache| cache.verified_at_ms)
                            .unwrap_or(0),
                        source_identity: source_identity.clone(),
                        freshness: if stale.is_some() {
                            "stale_failed"
                        } else {
                            "failed"
                        },
                        shared_flight: false,
                        probe_state: "failed",
                        error: Some(error),
                    };
                    if joined_waiters > 0 {
                        state.terminal_by_flight.insert(
                            flight_id,
                            SharedSemanticProbeTerminal {
                                source_identity: source_identity.clone(),
                                remaining_waiters: joined_waiters,
                                outcome: outcome.clone(),
                            },
                        );
                    }
                    self.wake.notify_all();
                    return outcome;
                }
                Err(payload) => {
                    if joined_waiters > 0 {
                        state.terminal_by_flight.insert(
                            flight_id,
                            SharedSemanticProbeTerminal {
                                source_identity: source_identity.clone(),
                                remaining_waiters: joined_waiters,
                                outcome: SemanticProbeOutcome {
                                    value: None,
                                    verified_at_ms: 0,
                                    source_identity: source_identity.clone(),
                                    freshness: "failed",
                                    shared_flight: true,
                                    probe_state: "failed",
                                    error: Some("semantic probe computation panicked".to_string()),
                                },
                            },
                        );
                    }
                    self.wake.notify_all();
                    drop(state);
                    std::panic::resume_unwind(payload);
                }
            }
        }
    }

    fn invalidate(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.epoch = state.epoch.wrapping_add(1);
        state.cache = None;
        drop(state);
        self.wake.notify_all();
    }
}

fn performance_tier_probe_slot(
) -> &'static SemanticProbeSlot<(Vec<String>, Option<bool>, Option<u32>)> {
    static SLOT: OnceLock<SemanticProbeSlot<(Vec<String>, Option<bool>, Option<u32>)>> =
        OnceLock::new();
    SLOT.get_or_init(SemanticProbeSlot::new)
}

fn demucs_status_probe_slot() -> &'static SemanticProbeSlot<(Option<String>, Option<u32>)> {
    static SLOT: OnceLock<SemanticProbeSlot<(Option<String>, Option<u32>)>> = OnceLock::new();
    SLOT.get_or_init(SemanticProbeSlot::new)
}

fn capability_probe_source_identity(paths: &AppPaths, semantic_key: &str) -> String {
    use sha2::Digest;
    let python = venv_python_path(&paths.python_venv_dir());
    let python_metadata = std::fs::metadata(&python).ok();
    let site_packages = paths.python_venv_dir().join("Lib").join("site-packages");
    let site_metadata = std::fs::metadata(&site_packages).ok();
    let modified = |metadata: &Option<std::fs::Metadata>| {
        metadata
            .as_ref()
            .and_then(|value| value.modified().ok())
            .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|value| value.as_nanos())
            .unwrap_or(0)
    };
    let payload = format!(
        "semantic={semantic_key}|python={}|python_len={}|python_mtime={}|site_len={}|site_mtime={}|cuda_visible={}",
        python.to_string_lossy(),
        python_metadata.as_ref().map(|value| value.len()).unwrap_or(0),
        modified(&python_metadata),
        site_metadata.as_ref().map(|value| value.len()).unwrap_or(0),
        modified(&site_metadata),
        std::env::var("CUDA_VISIBLE_DEVICES").unwrap_or_default(),
    );
    hex::encode_upper(sha2::Sha256::digest(payload.as_bytes()))
}

/// Invalidates the current-process Diagnostics/Options capability-probe caches after a repair,
/// payload promotion, Python override, or managed-pack mutation. Production inference/jobs are
/// deliberately outside this cache and its two-probe admission domain.
pub fn invalidate_capability_probe_cache() {
    performance_tier_probe_slot().invalidate();
    demucs_status_probe_slot().invalidate();
}

struct CapabilityProbeInvalidationGuard;

impl CapabilityProbeInvalidationGuard {
    fn new() -> Self {
        invalidate_capability_probe_cache();
        Self
    }
}

impl Drop for CapabilityProbeInvalidationGuard {
    fn drop(&mut self) {
        invalidate_capability_probe_cache();
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PerformanceTierStatus {
    pub tier: String,
    pub gpu_names: Vec<String>,
    pub torch_cuda_available: Option<bool>,
    pub recommended_separation_backend: String,
    pub recommended_diarization_backend: String,
    pub recommended_tts_vc_device: String,
    pub verified_at_ms: i64,
    pub source_identity: String,
    pub freshness: String,
    pub shared_flight: bool,
    /// PID of the one Python/Torch probe computation that produced this result.
    /// Cached and shared consumers receive the same PID as provenance; missing runtime
    /// and waiter-timeout fallbacks report `None`.
    pub child_pid: Option<u32>,
    pub probe_state: String,
    pub probe_error: Option<String>,
}

pub fn performance_tier_status(paths: &AppPaths) -> PerformanceTierStatus {
    let source_identity = capability_probe_source_identity(paths, "performance_tier_torch_cuda");
    if !venv_python_path(&paths.python_venv_dir()).exists() {
        return PerformanceTierStatus {
            tier: "cpu".to_string(),
            gpu_names: Vec::new(),
            torch_cuda_available: None,
            recommended_separation_backend: "spleeter (baseline)".to_string(),
            recommended_diarization_backend: "baseline".to_string(),
            recommended_tts_vc_device: "cpu".to_string(),
            verified_at_ms: now_ms(),
            source_identity,
            freshness: "verified_missing_runtime".to_string(),
            shared_flight: false,
            child_pid: None,
            probe_state: "missing_runtime".to_string(),
            probe_error: None,
        };
    }
    let outcome = performance_tier_probe_slot().run(source_identity, || {
        detect_torch_cuda(paths).map(|(torch_cuda_available, child_pid)| {
            (
                detect_gpu_names_best_effort(),
                torch_cuda_available,
                child_pid,
            )
        })
    });
    let (gpu_names, torch_cuda_available, computed_child_pid) =
        outcome.value.unwrap_or_else(|| (Vec::new(), None, None));
    let child_pid = computed_child_pid.or_else(|| {
        outcome
            .error
            .as_deref()
            .and_then(capability_probe_pid_from_error)
    });

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
        verified_at_ms: outcome.verified_at_ms,
        source_identity: outcome.source_identity,
        freshness: outcome.freshness.to_string(),
        shared_flight: outcome.shared_flight,
        child_pid,
        probe_state: outcome.probe_state.to_string(),
        probe_error: outcome.error,
    }
}

fn detect_gpu_names_best_effort() -> Vec<String> {
    // Best-effort, cross-platform-ish detection.
    let mut out: Vec<String> = Vec::new();

    let mut command = crate::cmd::command("nvidia-smi");
    command.args(["--query-gpu=name", "--format=csv,noheader"]);
    // nvidia-smi can hang behind a wedged driver. It is only a best-effort
    // supplement to the bounded Torch probe, so give it the same owned-child
    // timeout/kill/reap contract instead of blocking a semantic flight forever.
    if let Ok((output, _child_pid)) = wait_for_owned_capability_probe(
        &mut command,
        "nvidia_smi_gpu_name",
        std::time::Duration::from_secs(10),
    ) {
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

fn capability_probe_pid_from_error(error: &str) -> Option<u32> {
    let marker = " pid ";
    let start = error.find(marker)? + marker.len();
    let digits = error[start..]
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>();
    (!digits.is_empty())
        .then(|| digits.parse::<u32>().ok())
        .flatten()
}

fn wait_for_owned_capability_probe(
    command: &mut std::process::Command,
    probe_label: &str,
    timeout: std::time::Duration,
) -> std::result::Result<(std::process::Output, u32), String> {
    crate::cmd::run_owned_output_with_pid(
        command,
        timeout,
        crate::jobs::external_command_cancel_requested,
    )
    .map_err(|error| format!("{probe_label} failed: {error}"))
}

fn detect_torch_cuda(paths: &AppPaths) -> std::result::Result<(Option<bool>, Option<u32>), String> {
    let venv_python = venv_python_path(&paths.python_venv_dir());
    if !venv_python.exists() {
        return Err("managed Python runtime is missing".to_string());
    }
    let mut command = crate::cmd::command(&venv_python);
    command.args([
            "-c",
            "import json\ntry:\n import torch\n print(json.dumps({'cuda': bool(torch.cuda.is_available())}))\nexcept Exception as e:\n print(json.dumps({'error': str(e)}))\n",
        ]);
    let (output, child_pid) = wait_for_owned_capability_probe(
        &mut command,
        "Torch capability probe",
        CAPABILITY_PROBE_CHILD_TIMEOUT,
    )?;
    if !output.status.success() {
        return Err(format!(
            "Torch capability probe pid {child_pid} exited unsuccessfully: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let Some(last) = text.lines().rev().find(|l| !l.trim().is_empty()) else {
        return Err(format!(
            "Torch capability probe pid {child_pid} produced no JSON result"
        ));
    };
    let parsed = serde_json::from_str::<serde_json::Value>(last.trim()).map_err(|error| {
        format!("Torch capability probe pid {child_pid} returned malformed JSON: {error}")
    })?;
    if let Some(error) = parsed.get("error").and_then(|value| value.as_str()) {
        return Err(format!(
            "Torch import failed in probe pid {child_pid}: {error}"
        ));
    }
    let value = parsed
        .get("cuda")
        .and_then(|cuda| cuda.as_bool())
        .ok_or_else(|| {
            format!("Torch capability probe pid {child_pid} omitted boolean cuda state")
        })?;
    Ok((Some(value), Some(child_pid)))
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
    let _probe_invalidation = CapabilityProbeInvalidationGuard::new();
    #[cfg(not(windows))]
    {
        let _ = paths;
        return Err(EngineError::InstallFailed(
            "portable Python install is only supported on Windows for now".to_string(),
        ));
    }

    #[cfg(windows)]
    {
        let pin = &pinned_dependency_manifest::manifest().portable_python_windows;

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

        let download_tmp = install_dir.join(format!("python-nuget-{}.nupkg.download", pin.version));
        let download_final = install_dir.join(format!("python-nuget-{}.nupkg", pin.version));

        let resp = ureq::get(&pin.url).call().map_err(|e| {
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

        let expected = hex::decode(&pin.sha256_hex).map_err(|e| {
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
        crate::persistence::atomic_write_text(
            &marker,
            format!(
                "OK\nversion={}\nsource={}\nsha256={}\n",
                version.trim(),
                pin.source_label,
                pin.sha256_hex
            )
            .as_str(),
        )?;

        let _ = generate_pack_integrity_manifest(paths);
        Ok(portable_python_status(paths))
    }
}

pub fn install_python_toolchain(paths: &AppPaths) -> Result<PythonToolchainStatus> {
    let _probe_invalidation = CapabilityProbeInvalidationGuard::new();
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
        cmd.args(["-m", "venv"]).arg(&venv_dir);
        let output = crate::cmd::run_owned_output(
            &mut cmd,
            std::time::Duration::from_secs(600),
            crate::jobs::external_command_cancel_requested,
        )
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

/// Interpreter for the isolated CosyVoice venv (torch 2.3.1 stack), kept separate
/// from the main venv because their dependency pins conflict.
pub fn cosyvoice_venv_python_path(paths: &AppPaths) -> Result<std::path::PathBuf> {
    let venv_python = venv_python_path(&paths.python_cosyvoice_venv_dir());
    if !venv_python.exists() {
        return Err(EngineError::ExternalToolMissing {
            tool: "python (cosyvoice venv)".to_string(),
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
    paths
        .python_toolchain_dir()
        .join("pack_integrity_manifest.json")
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
        if rel_path.components().any(|c| {
            matches!(
                c,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        }) {
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
    let output = cmd.arg("--version").owned_output().ok()?;
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
        .owned_output()
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
    let _probe_invalidation = CapabilityProbeInvalidationGuard::new();
    // Ensure venv exists first.
    let _ = install_python_toolchain(paths)?;
    let venv_python = python_venv_python_path(paths)?;
    let py_version = python_version(&venv_python, &[]).unwrap_or_else(|| "unknown".to_string());
    let candidates = spleeter_install_candidates(&py_version);
    let pin = &pinned_dependency_manifest::manifest().spleeter;
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

    let bootstrap_args = pip_install_args(
        &["-m", "pip", "install", "--only-binary=:all:"],
        &pin.bootstrap_packages,
    );
    let _ = run_python_checked(
        paths,
        &venv_python,
        &bootstrap_args,
        "spleeter bootstrap failed",
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

model_name = "{model_name}"
repo = "{repo}"
release = "{release}"
base = f"https://github.com/{{repo}}/releases/download/{{release}}"
checksum_url = f"{{base}}/checksum.json"
archive_url = f"{{base}}/{{model_name}}.tar.gz"

def sleep_before_retry(attempt):
    time.sleep(min(2 ** attempt, 20))

def read_url(url, label, attempts=5):
    last_exc = None
    for attempt in range(1, attempts + 1):
        try:
            with urllib.request.urlopen(url, timeout=60) as resp:
                return resp.read()
        except Exception as exc:
            last_exc = exc
            if attempt < attempts:
                sleep_before_retry(attempt)
    raise RuntimeError("%s download failed after %s attempts: %s" % (label, attempts, last_exc))

def download_url_to_file(url, path, label, attempts=5):
    last_exc = None
    for attempt in range(1, attempts + 1):
        try:
            with urllib.request.urlopen(url, timeout=60) as resp, open(path, "wb") as f:
                while True:
                    chunk = resp.read(1024 * 1024)
                    if not chunk:
                        break
                    f.write(chunk)
            return
        except Exception as exc:
            last_exc = exc
            try:
                os.unlink(path)
            except Exception:
                pass
            if attempt < attempts:
                sleep_before_retry(attempt)
    raise RuntimeError("%s download failed after %s attempts: %s" % (label, attempts, last_exc))

index = json.loads(read_url(checksum_url, "checksum").decode("utf-8"))
expected = index.get(model_name)
if not expected:
    raise RuntimeError("checksum.json missing 2stems entry")

tmp = tempfile.NamedTemporaryFile(delete=False, suffix=".tar.gz")
tmp_path = tmp.name
tmp.close()
try:
    got = None
    last_exc = None
    for attempt in range(1, 6):
        try:
            download_url_to_file(archive_url, tmp_path, "model archive")
            h = hashlib.sha256()
            with open(tmp_path, "rb") as f:
                for chunk in iter(lambda: f.read(1024 * 1024), b""):
                    h.update(chunk)
            got = h.hexdigest()
            if got == expected:
                break
            last_exc = RuntimeError("model archive checksum mismatch: expected=%s got=%s" % (expected, got))
        except Exception as exc:
            last_exc = exc
        try:
            os.unlink(tmp_path)
        except Exception:
            pass
        if attempt < 5:
            sleep_before_retry(attempt)
    if got != expected:
        raise last_exc

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
                    model_path = models_dir.to_string_lossy(),
                    model_name = pin.model.model_name,
                    repo = pin.model.repo,
                    release = pin.model.release,
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
            vec![
                "-m",
                "pip",
                "install",
                "--no-deps",
                "--no-build-isolation",
                spec,
            ],
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

model_name = "{model_name}"
repo = "{repo}"
release = "{release}"
base = f"https://github.com/{{repo}}/releases/download/{{release}}"
checksum_url = f"{{base}}/checksum.json"
archive_url = f"{{base}}/{{model_name}}.tar.gz"

def sleep_before_retry(attempt):
    time.sleep(min(2 ** attempt, 20))

def read_url(url, label, attempts=5):
    last_exc = None
    for attempt in range(1, attempts + 1):
        try:
            with urllib.request.urlopen(url, timeout=60) as resp:
                return resp.read()
        except Exception as exc:
            last_exc = exc
            if attempt < attempts:
                sleep_before_retry(attempt)
    raise RuntimeError("%s download failed after %s attempts: %s" % (label, attempts, last_exc))

def download_url_to_file(url, path, label, attempts=5):
    last_exc = None
    for attempt in range(1, attempts + 1):
        try:
            with urllib.request.urlopen(url, timeout=60) as resp, open(path, "wb") as f:
                while True:
                    chunk = resp.read(1024 * 1024)
                    if not chunk:
                        break
                    f.write(chunk)
            return
        except Exception as exc:
            last_exc = exc
            try:
                os.unlink(path)
            except Exception:
                pass
            if attempt < attempts:
                sleep_before_retry(attempt)
    raise RuntimeError("%s download failed after %s attempts: %s" % (label, attempts, last_exc))

index = json.loads(read_url(checksum_url, "checksum").decode("utf-8"))
expected = index.get(model_name)
if not expected:
    raise RuntimeError("checksum.json missing 2stems entry")

tmp = tempfile.NamedTemporaryFile(delete=False, suffix=".tar.gz")
tmp_path = tmp.name
tmp.close()
try:
    got = None
    last_exc = None
    for attempt in range(1, 6):
        try:
            download_url_to_file(archive_url, tmp_path, "model archive")
            h = hashlib.sha256()
            with open(tmp_path, "rb") as f:
                for chunk in iter(lambda: f.read(1024 * 1024), b""):
                    h.update(chunk)
            got = h.hexdigest()
            if got == expected:
                break
            last_exc = RuntimeError("model archive checksum mismatch: expected=%s got=%s" % (expected, got))
        except Exception as exc:
            last_exc = exc
        try:
            os.unlink(tmp_path)
        except Exception:
            pass
        if attempt < 5:
            sleep_before_retry(attempt)
    if got != expected:
        raise last_exc

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
                        model_path = models_dir.to_string_lossy(),
                        model_name = pin.model.model_name,
                        repo = pin.model.repo,
                        release = pin.model.release,
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
                    "spleeter installed with fallback strategy, but module detection failed"
                        .to_string(),
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
        Some(last_error) => {
            format!("spleeter install failed for python {py_version}: {last_error}")
        }
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
        || text
            .contains("could not find a version that satisfies the requirement tensorflow==2.12.1")
    {
        return format!(
            "TensorFlow pinned by {spec} cannot be resolved on Python {py_version}. Install Python 3.9/3.10, set it in config/python_exe.txt, and run install again."
        );
    }

    if text.contains("building wheel") || text.contains("error: command '") || text.contains("msvc")
    {
        return "Build path failed during native extension compile. Install Microsoft C++ Build Tools (or choose an interpreter where Spleeter wheels are available).".to_string();
    }

    if text.contains("permission denied") {
        return "Installer write access failed. Ensure a writable app data/cache directory or run with a writable filesystem path.".to_string();
    }

    if text.contains("invalid requirement") || text.contains("invalid specifier") {
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

    Err(EngineError::InstallFailed(last_error.unwrap_or_else(
        || format!("dependency install {pin} failed"),
    )))
}

fn spleeter_install_candidates(py_version: &str) -> Vec<String> {
    let pin = &pinned_dependency_manifest::manifest().spleeter;
    let mut candidates = match parse_python_major_minor(py_version) {
        Some((3, minor)) if (8..=11).contains(&minor) => {
            vec![pin.candidate_pins.py38_to_py311.clone()]
        }
        Some((3, minor)) if minor < 8 => vec![pin.candidate_pins.py_lt_38.clone()],
        _ => vec![pin.candidate_pins.default_pinned.clone()],
    };
    if pinned_dependency_manifest::allow_unpinned_fallback() {
        candidates.push(pin.unpinned_fallback_spec.clone());
    }
    candidates
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
    pub verified_at_ms: i64,
    pub source_identity: String,
    pub freshness: String,
    pub shared_flight: bool,
    pub child_pid: Option<u32>,
    pub probe_state: String,
    pub probe_error: Option<String>,
}

/// WP-0229: production Phase2 installs avoid rerunning pip and model warmup when
/// the canonical pack status already proves the Spleeter pack is present.
pub fn install_spleeter_pack_if_needed(paths: &AppPaths) -> Result<SpleeterPackStatus> {
    let status = spleeter_pack_status(paths);
    if status.installed {
        return Ok(status);
    }
    install_spleeter_pack(paths)
}

pub fn demucs_pack_status(paths: &AppPaths) -> DemucsPackStatus {
    let venv_dir = paths.python_venv_dir();
    let venv_python = venv_python_path(&venv_dir);
    let source_identity = capability_probe_source_identity(paths, "demucs_module_status");
    if !venv_python.exists() {
        return DemucsPackStatus {
            installed: false,
            demucs_version: None,
            verified_at_ms: now_ms(),
            source_identity,
            freshness: "verified_missing_runtime".to_string(),
            shared_flight: false,
            child_pid: None,
            probe_state: "missing_runtime".to_string(),
            probe_error: None,
        };
    }

    let outcome = demucs_status_probe_slot().run(source_identity, || {
        python_module_version_with_pid(&venv_python, "demucs_infer")
    });
    let (demucs_version, computed_child_pid) = outcome.value.unwrap_or((None, None));
    let child_pid = computed_child_pid.or_else(|| {
        outcome
            .error
            .as_deref()
            .and_then(capability_probe_pid_from_error)
    });
    DemucsPackStatus {
        installed: demucs_version.is_some(),
        demucs_version,
        verified_at_ms: outcome.verified_at_ms,
        source_identity: outcome.source_identity,
        freshness: outcome.freshness.to_string(),
        shared_flight: outcome.shared_flight,
        child_pid,
        probe_state: outcome.probe_state.to_string(),
        probe_error: outcome.error,
    }
}

pub fn install_demucs_pack(paths: &AppPaths) -> Result<DemucsPackStatus> {
    let _probe_invalidation = CapabilityProbeInvalidationGuard::new();
    // Ensure venv exists first.
    let _ = install_python_toolchain(paths)?;
    let venv_python = python_venv_python_path(paths)?;
    let pin = &pinned_dependency_manifest::manifest().demucs;

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

    // WP-0232: prefer the hashed lockfile if bundled. Legacy pinned-spec path retained
    // as a fallback when no lockfile is on disk.
    let pinned_install = match locate_pack_lockfile("demucs") {
        Some(lockfile_path) => install_pack_from_lockfile(
            paths,
            &venv_python,
            "demucs",
            lockfile_path,
            "demucs-infer install",
        ),
        None => run_python_checked(
            paths,
            &venv_python,
            &[
                "-m",
                "pip",
                "install",
                "--prefer-binary",
                pin.pinned_spec.as_str(),
            ],
            "pip install demucs-infer failed (pinned, legacy path; no lockfile bundled)",
        ),
    };

    // Prefer an inference-only distribution (smaller surface area than full training stack).
    if let Err(err) = pinned_install {
        if !pinned_dependency_manifest::allow_unpinned_fallback() {
            return Err(unpinned_fallback_disabled_error(
                "demucs-infer install",
                &err,
            ));
        }
        run_python_checked(
            paths,
            &venv_python,
            &[
                "-m",
                "pip",
                "install",
                "--prefer-binary",
                pin.unpinned_fallback_spec.as_str(),
            ],
            &format!("pip install demucs-infer failed (unpinned fallback): {err}"),
        )?;
    }

    let _ = run_python_checked(
        paths,
        &venv_python,
        &["-c", "import demucs_infer; print('ok')"],
        "demucs warmup failed",
    );

    invalidate_capability_probe_cache();
    let status = demucs_pack_status(paths);
    let _ = generate_pack_integrity_manifest(paths);
    Ok(status)
}

#[derive(Debug, Clone, Serialize)]
pub struct DiarizationPackStatus {
    pub installed: bool,
    pub state: String,
    pub repair_required: bool,
    pub status_detail: String,
    pub resemblyzer_version: Option<String>,
    pub numpy_version: Option<String>,
    pub sklearn_version: Option<String>,
    pub librosa_version: Option<String>,
    pub numba_version: Option<String>,
    pub llvmlite_version: Option<String>,
    pub webrtcvad_version: Option<String>,
    pub soundfile_version: Option<String>,
    pub runtime_validation_error: Option<String>,
}

fn diarization_runtime_validation_code() -> &'static str {
    r#"
import numpy
import soundfile as sf
import sklearn.cluster
import librosa
import numba
import llvmlite
import webrtcvad
from resemblyzer import VoiceEncoder, preprocess_wav
VoiceEncoder()
print("ok")
"#
}

fn validate_diarization_runtime(paths: &AppPaths, venv_python: &std::path::Path) -> Result<()> {
    run_python_checked(
        paths,
        venv_python,
        &["-c", diarization_runtime_validation_code()],
        "diarization runtime validation failed",
    )
}

fn diarization_pack_is_installed(
    all_required_present: bool,
    lockfile_runtime_ready: bool,
    versions_ready: bool,
) -> bool {
    all_required_present && lockfile_runtime_ready && versions_ready
}

pub fn diarization_pack_status(paths: &AppPaths) -> DiarizationPackStatus {
    let venv_dir = paths.python_venv_dir();
    let venv_python = venv_python_path(&venv_dir);
    if !venv_python.exists() {
        return DiarizationPackStatus {
            installed: false,
            state: "not_installed".to_string(),
            repair_required: false,
            status_detail: "Python venv is not prepared.".to_string(),
            resemblyzer_version: None,
            numpy_version: None,
            sklearn_version: None,
            librosa_version: None,
            numba_version: None,
            llvmlite_version: None,
            webrtcvad_version: None,
            soundfile_version: None,
            runtime_validation_error: None,
        };
    }

    let distribution_names = [
        "Resemblyzer",
        "numpy",
        "scikit-learn",
        "librosa",
        "numba",
        "llvmlite",
        "webrtcvad",
        "soundfile",
    ]
    .iter()
    .map(|name| (*name).to_string())
    .collect::<Vec<_>>();
    let versions = python_distribution_versions(&venv_python, &distribution_names);
    let version_for = |name: &str| versions.get(name).cloned().flatten();

    let resemblyzer_version = version_for("Resemblyzer");
    let numpy_version = version_for("numpy");
    let sklearn_version = version_for("scikit-learn");
    let librosa_version = version_for("librosa");
    let numba_version = version_for("numba");
    let llvmlite_version = version_for("llvmlite");
    let webrtcvad_version = version_for("webrtcvad");
    let soundfile_version = version_for("soundfile");

    let package_presence = [
        resemblyzer_version.as_ref(),
        numpy_version.as_ref(),
        sklearn_version.as_ref(),
        librosa_version.as_ref(),
        numba_version.as_ref(),
        llvmlite_version.as_ref(),
        webrtcvad_version.as_ref(),
        soundfile_version.as_ref(),
    ];
    let any_present = package_presence.iter().any(|value| value.is_some());
    let all_required_present = package_presence.iter().all(|value| value.is_some());
    let lockfile_ready = pack_install_satisfied(paths, "diarization");
    let version_mismatches = lockfile_source_pin_mismatches(&venv_python, "diarization");
    let versions_ready = version_mismatches.is_empty();
    let lockfile_runtime_ready = pack_lockfile_runtime_ready(lockfile_ready, versions_ready);
    let (_, installed_lockfile_sha) = pack_install_state_shas(paths, "diarization");
    let receipt_stale = !lockfile_ready && versions_ready && installed_lockfile_sha.is_some();

    let installed = all_required_present;
    let repair_required = if installed {
        !lockfile_runtime_ready || !versions_ready
    } else {
        any_present || installed_lockfile_sha.is_some()
    };
    let (state, repair_required, status_detail) = if installed {
        (
            "installed".to_string(),
            repair_required,
            if !versions_ready {
                let first = &version_mismatches[0];
                format!(
                    "Diarization packages are present, but installed package versions do not match the bundled lockfile. First mismatch: {} requires {}, installed {}. Run Install/Repair to refresh the pack; startup remains unblocked because package metadata is present.",
                    first.package,
                    first.expected,
                    first.installed.as_deref().unwrap_or("missing")
                )
            } else if receipt_stale {
                "Diarization is ready; the install receipt journal is stale but installed package versions match the bundled dependency lockfile. Full runtime validation runs during install/repair and diarization jobs.".to_string()
            } else {
                "Diarization package metadata matches the bundled dependency lockfile. Full runtime validation runs during install/repair and diarization jobs.".to_string()
            },
        )
    } else if any_present {
        (
            "broken".to_string(),
            repair_required,
            if !all_required_present {
                "One or more required diarization packages are missing.".to_string()
            } else if !versions_ready {
                let first = &version_mismatches[0];
                format!(
                    "Installed package versions do not match the Diarization lockfile. First mismatch: {} requires {}, installed {}. Run Install/Repair to refresh the pack.",
                    first.package,
                    first.expected,
                    first.installed.as_deref().unwrap_or("missing")
                )
            } else if !lockfile_runtime_ready {
                "Installed packages do not match the current Diarization lockfile. Run Install/Repair to refresh the pack.".to_string()
            } else {
                "Diarization packages are not ready.".to_string()
            },
        )
    } else {
        (
            "not_installed".to_string(),
            repair_required,
            "Diarization packages are not installed in the managed venv.".to_string(),
        )
    };
    let runtime_validation_error = if installed {
        None
    } else {
        Some(status_detail.clone())
    };

    DiarizationPackStatus {
        installed,
        state,
        repair_required,
        status_detail,
        resemblyzer_version,
        numpy_version,
        sklearn_version,
        librosa_version,
        numba_version,
        llvmlite_version,
        webrtcvad_version,
        soundfile_version,
        runtime_validation_error,
    }
}

pub fn install_diarization_pack(paths: &AppPaths) -> Result<DiarizationPackStatus> {
    let _probe_invalidation = CapabilityProbeInvalidationGuard::new();
    // Ensure venv exists first.
    let _ = install_python_toolchain(paths)?;
    let venv_python = python_venv_python_path(paths)?;
    let pin = &pinned_dependency_manifest::manifest().diarization;

    let binary_repair_pins = pin
        .pinned
        .iter()
        .filter(|spec| spec.starts_with("numba==") || spec.starts_with("llvmlite=="))
        .cloned()
        .collect::<Vec<_>>();
    if !binary_repair_pins.is_empty() {
        let binary_repair_args = pip_install_args(
            &["-m", "pip", "install", "--upgrade", "--only-binary=:all:"],
            &binary_repair_pins,
        );
        let _ = run_python_checked(
            paths,
            &venv_python,
            &binary_repair_args,
            "pip repair diarization numba/llvmlite pair failed",
        );
    }

    // WP-0232: prefer the hashed lockfile; legacy fallback path retained.
    let install_err = match locate_pack_lockfile("diarization") {
        Some(lockfile_path) => install_pack_from_lockfile(
            paths,
            &venv_python,
            "diarization",
            lockfile_path,
            "diarization dependency install",
        ),
        None => {
            let pinned_args = pip_install_args(&["-m", "pip", "install", "--upgrade"], &pin.pinned);
            run_python_checked(
                paths,
                &venv_python,
                &pinned_args,
                "pip install diarization dependencies failed (pinned, legacy path; no lockfile bundled)",
            )
        }
    };
    if let Err(err) = install_err {
        if !pinned_dependency_manifest::allow_unpinned_fallback() {
            return Err(unpinned_fallback_disabled_error(
                "diarization dependency install",
                &err,
            ));
        }
        let fallback_args = pip_install_args(
            &["-m", "pip", "install", "--upgrade"],
            &pin.unpinned_fallback,
        );
        // Best-effort fallback when pinned wheels are unavailable.
        let _ = run_python_checked(
            paths,
            &venv_python,
            &fallback_args,
            &format!("pip install diarization dependencies failed (unpinned fallback): {err}"),
        )?;
    }

    vendor_patches::patch_webrtcvad_pkg_resources_import(&venv_python)?;

    validate_diarization_runtime(paths, &venv_python)?;

    let status = diarization_pack_status(paths);
    let _ = generate_pack_integrity_manifest(paths);
    Ok(status)
}

#[derive(Debug, Clone, Serialize)]
pub struct TtsPreviewPackStatus {
    pub installed: bool,
    pub pyttsx3_version: Option<String>,
}

/// WP-0229: preserve the bare installer for explicit repair while normal
/// Phase2 runs return immediately when the canonical status is satisfied.
pub fn install_diarization_pack_if_needed(paths: &AppPaths) -> Result<DiarizationPackStatus> {
    let status = diarization_pack_status(paths);
    if status.installed {
        return Ok(status);
    }
    install_diarization_pack(paths)
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
    let _probe_invalidation = CapabilityProbeInvalidationGuard::new();
    // Ensure venv exists first.
    let _ = install_python_toolchain(paths)?;
    let venv_python = python_venv_python_path(paths)?;
    let pin = &pinned_dependency_manifest::manifest().tts_preview;

    // WP-0232: prefer the hashed lockfile; legacy fallback path retained.
    let pinned_install = match locate_pack_lockfile("tts_preview") {
        Some(lockfile_path) => install_pack_from_lockfile(
            paths,
            &venv_python,
            "tts_preview",
            lockfile_path,
            "pyttsx3 install",
        ),
        None => run_python_checked(
            paths,
            &venv_python,
            &["-m", "pip", "install", pin.pinned[0].as_str()],
            "pip install pyttsx3 failed (pinned, legacy path; no lockfile bundled)",
        ),
    };
    if let Err(err) = pinned_install {
        if !pinned_dependency_manifest::allow_unpinned_fallback() {
            return Err(unpinned_fallback_disabled_error("pyttsx3 install", &err));
        }
        run_python_checked(
            paths,
            &venv_python,
            &["-m", "pip", "install", pin.unpinned_fallback[0].as_str()],
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
    pub repair_required: bool,
    pub status_detail: String,
    pub package_version: Option<String>,
    pub transformers_version: Option<String>,
    pub huggingface_hub_version: Option<String>,
    pub expected_lockfile_sha: Option<String>,
    pub installed_lockfile_sha: Option<String>,
    pub version_mismatches: Vec<PythonPackageVersionMismatch>,
}

fn kokoro_warmup_probe_path(paths: &AppPaths) -> std::path::PathBuf {
    paths.python_models_dir().join("kokoro").join(".warmup_ok")
}

fn dir_has_file_with_extension(dir: &std::path::Path, ext: &str) -> bool {
    std::fs::read_dir(dir)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .any(|entry| {
            entry
                .path()
                .extension()
                .and_then(|value| value.to_str())
                .map(|value| value.eq_ignore_ascii_case(ext))
                .unwrap_or(false)
        })
}

/// The voice-preserving dub job loads Kokoro through `KPipeline`, which resolves
/// `hexgrad/Kokoro-82M` from the Hugging Face cache (`HF_HOME = <cache>/huggingface`)
/// with `HF_HUB_OFFLINE=1` at job time. A `.warmup_ok` marker file is NOT sufficient
/// proof that the model is reachable there: an older build warmed Kokoro into the
/// default *user* cache and left a marker, which made later builds skip
/// re-provisioning, so the offline job could never find the weights in the
/// *app-local* cache and failed on the very first synth call. This verifies the
/// snapshot the offline job actually needs (config + model weights + the default
/// `af_heart` voice) is present in the app-local cache the job reads, so the gate
/// cannot be satisfied by a stale marker alone.
fn kokoro_app_cache_ready(paths: &AppPaths) -> bool {
    let repo = paths
        .cache_dir()
        .join("huggingface")
        .join("hub")
        .join("models--hexgrad--Kokoro-82M");
    // huggingface_hub resolves the `main` revision through `refs/main` -> commit sha,
    // then loads `snapshots/<sha>/<file>`. Mirror that resolution exactly so this gate
    // matches what the OFFLINE job can actually load: a partial payload hydration that
    // drops `refs/main` or the snapshot files (the real-world failure shape) must read
    // as not-ready even if some stray files exist. `is_file()` follows the symlinks the
    // HF cache uses into `blobs/`, so a dangling snapshot entry (missing blob) also
    // correctly reads as not ready.
    let sha = match std::fs::read_to_string(repo.join("refs").join("main")) {
        Ok(value) => value.trim().to_string(),
        Err(_) => return false,
    };
    if sha.is_empty() {
        return false;
    }
    let snapshot = repo.join("snapshots").join(&sha);
    let has_config = snapshot.join("config.json").is_file();
    let has_weights =
        snapshot.join("kokoro-v1_0.pth").is_file() || dir_has_file_with_extension(&snapshot, "pth");
    let has_default_voice = snapshot.join("voices").join("af_heart.pt").is_file();
    has_config && has_weights && has_default_voice
}

#[derive(Debug, Clone, Serialize)]
pub struct CosyVoicePackStatus {
    pub installed: bool,
    pub status_detail: String,
    pub venv_python_present: bool,
    pub model_present: bool,
    pub matcha_present: bool,
    pub render_script_present: bool,
    pub render_script_current: bool,
    pub wetext_assets_present: bool,
}

fn cosyvoice_wetext_assets_complete(model_dir: &std::path::Path) -> bool {
    [
        "en/tn/tagger.fst",
        "en/tn/verbalizer.fst",
        "zh/tn/tagger.fst",
        "zh/tn/verbalizer.fst",
    ]
    .iter()
    .all(|relative| file_is_nonempty(&model_dir.join(relative)))
}

fn file_is_nonempty(path: &std::path::Path) -> bool {
    std::fs::metadata(path)
        .map(|metadata| metadata.is_file() && metadata.len() > 0)
        .unwrap_or(false)
}

/// WP-0229: pyttsx3 setup is a no-op when the canonical preview-pack status is
/// already installed; the bare function remains the force-repair path.
pub fn install_tts_preview_pack_if_needed(paths: &AppPaths) -> Result<TtsPreviewPackStatus> {
    let status = tts_preview_pack_status(paths);
    if status.installed {
        return Ok(status);
    }
    install_tts_preview_pack(paths)
}

/// Readiness for the CosyVoice 2 cross-lingual clone backend. Applies the same
/// honest-gate lesson as the Kokoro fix: verify the isolated venv python and the
/// concrete local model files the offline render actually loads are present, rather
/// than trusting a marker. CosyVoice is offline-by-design (loads from a local
/// `model_dir`). The wetext frontend is a separate ModelScope asset graph, so its
/// exact app-local FST inputs are part of readiness too; an external user cache does
/// not satisfy the managed offline contract.
pub fn cosyvoice_pack_status(paths: &AppPaths) -> CosyVoicePackStatus {
    let venv_python_present =
        file_is_nonempty(&venv_python_path(&paths.python_cosyvoice_venv_dir()));

    let model_dir = paths.cosyvoice_model_parent_dir().join("CosyVoice2-0.5B");
    let model_present = file_is_nonempty(&model_dir.join("cosyvoice2.yaml"))
        && file_is_nonempty(&model_dir.join("llm.pt"))
        && file_is_nonempty(&model_dir.join("flow.pt"))
        && file_is_nonempty(&model_dir.join("hift.pt"))
        && file_is_nonempty(
            &model_dir
                .join("CosyVoice-BlankEN")
                .join("model.safetensors"),
        );

    let backend_dir = paths.cosyvoice_backend_dir();
    let matcha_present = backend_dir
        .join("third_party")
        .join("Matcha-TTS")
        .join("matcha")
        .is_dir();
    let render_script_path = backend_dir.join("voxvulgi_cosyvoice_render.py");
    let render_script_present = file_is_nonempty(&render_script_path);
    let render_script_current = std::fs::read(&render_script_path)
        .map(|bytes| bytes == COSYVOICE_RENDER_WRAPPER.as_bytes())
        .unwrap_or(false);
    let wetext_assets_present = cosyvoice_wetext_assets_complete(&backend_dir.join("wetext"));

    let installed = venv_python_present
        && model_present
        && matcha_present
        && render_script_present
        && render_script_current
        && wetext_assets_present;
    let status_detail = if installed {
        "CosyVoice 2 voice cloning is ready (isolated venv + model + app-local wetext assets)."
            .to_string()
    } else if !venv_python_present {
        "CosyVoice isolated Python environment is not installed.".to_string()
    } else if !model_present {
        "CosyVoice2-0.5B model files are missing from the local model directory.".to_string()
    } else if !matcha_present {
        "CosyVoice dependency Matcha-TTS is missing (third_party/Matcha-TTS).".to_string()
    } else if !render_script_present {
        "CosyVoice render wrapper script is missing.".to_string()
    } else if !render_script_current {
        "CosyVoice render wrapper is stale for this VoxVulgi build; repair the managed pack."
            .to_string()
    } else {
        "CosyVoice wetext normalizer assets are missing from the managed app-local pack."
            .to_string()
    };

    CosyVoicePackStatus {
        installed,
        status_detail,
        venv_python_present,
        model_present,
        matcha_present,
        render_script_present,
        render_script_current,
        wetext_assets_present,
    }
}

// Bundled into the binary so a fresh install always has the exact pinned deps + the
// render wrapper that matches this engine build (no reliance on the on-disk checkout).
const COSYVOICE_REQUIREMENTS: &str =
    include_str!("../resources/tooling/requirements.cosyvoice.txt");
const COSYVOICE_RENDER_WRAPPER: &str =
    include_str!("../resources/tooling/voxvulgi_cosyvoice_render.py");
// torch 2.3.1 stack + a 4.86 GB model on a throttled connection can exceed the default
// 30-minute command timeout, so the CosyVoice install steps get a 90-minute budget.
const COSYVOICE_INSTALL_TIMEOUT_SECS: u64 = 90 * 60;

fn cosyvoice_model_complete(model_dir: &std::path::Path) -> bool {
    model_dir.join("cosyvoice2.yaml").is_file()
        && model_dir.join("llm.pt").is_file()
        && model_dir.join("flow.pt").is_file()
        && model_dir.join("hift.pt").is_file()
        && model_dir
            .join("CosyVoice-BlankEN")
            .join("model.safetensors")
            .is_file()
}

fn py_path(path: &std::path::Path) -> String {
    // Forward slashes are valid on Windows and avoid backslash-escaping in embedded code.
    path.to_string_lossy().replace('\\', "/")
}

fn cosyvoice_model_download_code(model_dir: &std::path::Path) -> String {
    format!(
        "from huggingface_hub import snapshot_download\n\
         snapshot_download('FunAudioLLM/CosyVoice2-0.5B', local_dir=r'{}')\n\
         print('cosyvoice_model_downloaded')\n",
        py_path(model_dir)
    )
}

fn cosyvoice_wetext_download_code(model_dir: &std::path::Path) -> String {
    format!(
        "from modelscope import snapshot_download\n\
         snapshot_download('pengzhendong/wetext', local_dir=r'{}')\n\
         print('cosyvoice_wetext_downloaded')\n",
        py_path(model_dir)
    )
}

/// WP-0262: install-time warmup ceiling. The CosyVoice class import
/// (`from cosyvoice.cli.cosyvoice import ...`) has been observed to take >150 s on a
/// cold venv; the render wrapper's `--warmup` mode enforces a bounded, instrumented
/// import (default 300 s hard limit via `IMPORT_HARD_LIMIT_SECS`) that fails LOUDLY
/// with the stall location instead of silently consuming the outer command timeout.
/// This outer budget is a generous belt-and-suspenders ceiling around that.
const COSYVOICE_WARMUP_TIMEOUT_SECS: u64 = 12 * 60;

/// WP-0262: run the bounded, instrumented warmup through the render wrapper's
/// `--warmup` mode (a single canonical import+model-load+synth path shared with the
/// job-time renderer). A slow/hung `from cosyvoice.cli.cosyvoice import ...` now
/// surfaces as an explicit, loud error identifying WHERE it stalled rather than a
/// mystery job-timeout with no audio. This also warms the wetext text-frontend cache
/// (its only first-run network use) so later dub jobs need no model download.
fn cosyvoice_warmup_args(
    repo_dir: &std::path::Path,
    model_parent: &std::path::Path,
) -> Vec<String> {
    let wrapper = repo_dir.join("voxvulgi_cosyvoice_render.py");
    vec![
        py_path(&wrapper),
        "--warmup".to_string(),
        "--model-dir".to_string(),
        py_path(model_parent),
    ]
}

/// Provision the isolated CosyVoice 2 voice-clone pack: write the engine-pinned render
/// wrapper, create the second venv, install the pinned deps (validated recipe), download
/// the model and app-local wetext assets if absent, warm the model, and verify with the
/// honest gate.
/// The CosyVoice repo code + Matcha-TTS ship via the offline payload (too large to embed,
/// too fragile to git-clone at runtime); the venv + 4.86 GB model are downloaded here.
pub fn install_voice_clone_cosyvoice_v1_pack(paths: &AppPaths) -> Result<CosyVoicePackStatus> {
    let _probe_invalidation = CapabilityProbeInvalidationGuard::new();
    paths.ensure_dirs()?;
    let backend_dir = paths.cosyvoice_backend_dir();

    // The CosyVoice python package must be present (shipped via the offline payload). We
    // do not git-clone at runtime; fail with guidance instead.
    if !backend_dir
        .join("cosyvoice")
        .join("cli")
        .join("cosyvoice.py")
        .is_file()
    {
        return Err(EngineError::InstallFailed(format!(
            "CosyVoice backend code is missing under {}. It ships with the installer offline payload; reinstall VoxVulgi to restore it.",
            backend_dir.display()
        )));
    }
    if !backend_dir
        .join("third_party")
        .join("Matcha-TTS")
        .join("matcha")
        .is_dir()
    {
        return Err(EngineError::InstallFailed(format!(
            "CosyVoice dependency Matcha-TTS is missing under {}. It ships with the offline payload; reinstall VoxVulgi.",
            backend_dir.display()
        )));
    }

    // Always (re)write the render wrapper from the engine-pinned copy so it matches this build.
    std::fs::write(
        backend_dir.join("voxvulgi_cosyvoice_render.py"),
        COSYVOICE_RENDER_WRAPPER,
    )?;

    // 1) Isolated venv (torch 2.3.1 conflicts with the main venv's torch 2.10).
    let venv_dir = paths.python_cosyvoice_venv_dir();
    if !venv_python_path(&venv_dir).exists() {
        let resolved = resolve_base_python(paths).ok_or_else(|| {
            EngineError::InstallFailed(
                "Python was not found to create the CosyVoice venv. Install the portable Python in Diagnostics first."
                    .to_string(),
            )
        })?;
        if let Some(parent) = venv_dir.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut cmd = crate::cmd::command(&resolved.program);
        for arg in &resolved.args {
            cmd.arg(arg);
        }
        cmd.args(["-m", "venv"]).arg(&venv_dir);
        let output = crate::cmd::run_owned_output(
            &mut cmd,
            std::time::Duration::from_secs(600),
            crate::jobs::external_command_cancel_requested,
        )
        .map_err(|e| EngineError::InstallFailed(format!("failed to create CosyVoice venv: {e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(EngineError::InstallFailed(format!(
                "CosyVoice venv creation failed (code={:?}): {}",
                output.status.code(),
                stderr.trim()
            )));
        }
    }
    let venv_python = venv_python_path(&venv_dir);

    // 2) Pinned dependency install (validated recipe). setuptools<80 still ships
    //    pkg_resources, which openai-whisper's legacy build needs; --no-build-isolation
    //    then builds it against the venv's setuptools.
    let _ = run_python_checked(
        paths,
        &venv_python,
        &["-m", "pip", "install", "--upgrade", "pip"],
        "CosyVoice pip bootstrap",
    );
    run_python_checked(
        paths,
        &venv_python,
        &["-m", "pip", "install", "setuptools<80", "wheel"],
        "CosyVoice setuptools/wheel install failed",
    )?;
    let req_path = paths
        .python_models_dir()
        .join(".cosyvoice_requirements.txt");
    if let Some(parent) = req_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&req_path, COSYVOICE_REQUIREMENTS)?;
    let req_arg = req_path.to_string_lossy().to_string();
    run_python_checked_with_timeout(
        paths,
        &venv_python,
        &[
            "-m",
            "pip",
            "install",
            "--no-build-isolation",
            "-r",
            &req_arg,
        ],
        "CosyVoice dependency install failed",
        COSYVOICE_INSTALL_TIMEOUT_SECS,
    )?;

    // 3) Download the model into the local model dir if not already complete.
    let model_dir = paths.cosyvoice_model_parent_dir().join("CosyVoice2-0.5B");
    if !cosyvoice_model_complete(&model_dir) {
        std::fs::create_dir_all(&model_dir)?;
        let code = cosyvoice_model_download_code(&model_dir);
        run_python_checked_with_timeout(
            paths,
            &venv_python,
            &["-c", &code],
            "CosyVoice2-0.5B model download failed",
            COSYVOICE_INSTALL_TIMEOUT_SECS,
        )?;
    }

    // 4) Download the exact text-normalizer graph into the managed backend directory.
    //    The render wrapper resolves this directory directly and refuses unexpected
    //    ModelScope lookups, so runtime readiness never depends on a user-profile cache.
    let wetext_dir = backend_dir.join("wetext");
    if !cosyvoice_wetext_assets_complete(&wetext_dir) {
        std::fs::create_dir_all(&wetext_dir)?;
        let code = cosyvoice_wetext_download_code(&wetext_dir);
        run_python_checked_with_timeout(
            paths,
            &venv_python,
            &["-c", &code],
            "CosyVoice wetext asset download failed",
            COSYVOICE_INSTALL_TIMEOUT_SECS,
        )?;
    }

    // 5) Warm the model (verifies inference works through the offline local-asset path).
    //    WP-0262: routed through the render wrapper's bounded/instrumented `--warmup`
    //    mode so a slow/hung CosyVoice class import fails LOUDLY with the stall location
    //    instead of silently exceeding the timeout.
    let warmup_args = cosyvoice_warmup_args(&backend_dir, &paths.cosyvoice_model_parent_dir());
    let warmup_args_ref: Vec<&str> = warmup_args.iter().map(String::as_str).collect();
    run_python_checked_with_timeout(
        paths,
        &venv_python,
        &warmup_args_ref,
        "CosyVoice warmup failed",
        COSYVOICE_WARMUP_TIMEOUT_SECS,
    )?;

    // 6) Honest gate.
    let status = cosyvoice_pack_status(paths);
    if !status.installed {
        return Err(EngineError::InstallFailed(format!(
            "CosyVoice install completed but the readiness check failed: {}",
            status.status_detail
        )));
    }
    let _ = generate_pack_integrity_manifest(paths);
    Ok(status)
}

/// WP-0229 current-scope extension: CosyVoice joined the Phase2 plan after the
/// original packet was authored and must obey the same no-op/force contract.
pub fn install_voice_clone_cosyvoice_v1_pack_if_needed(
    paths: &AppPaths,
) -> Result<CosyVoicePackStatus> {
    let status = cosyvoice_pack_status(paths);
    if status.installed {
        return Ok(status);
    }
    install_voice_clone_cosyvoice_v1_pack(paths)
}

pub fn tts_neural_local_v1_pack_status(paths: &AppPaths) -> TtsNeuralLocalV1PackStatus {
    let venv_dir = paths.python_venv_dir();
    let venv_python = venv_python_path(&venv_dir);
    let (expected_lockfile_sha, installed_lockfile_sha) =
        pack_install_state_shas(paths, "tts_neural_local_v1");
    if !venv_python.exists() {
        return TtsNeuralLocalV1PackStatus {
            installed: false,
            repair_required: false,
            status_detail: "Python environment is not installed yet.".to_string(),
            package_version: None,
            transformers_version: None,
            huggingface_hub_version: None,
            expected_lockfile_sha,
            installed_lockfile_sha,
            version_mismatches: Vec::new(),
        };
    }

    let package_version = python_distribution_version(&venv_python, "kokoro");
    let transformers_version = python_distribution_version(&venv_python, "transformers");
    let huggingface_hub_version = python_distribution_version(&venv_python, "huggingface-hub")
        .or_else(|| python_distribution_version(&venv_python, "huggingface_hub"));
    let warmup_ready = kokoro_warmup_probe_path(paths).exists() && kokoro_app_cache_ready(paths);
    let lockfile_ready = pack_install_satisfied(paths, "tts_neural_local_v1");
    let version_mismatches = lockfile_source_pin_mismatches(&venv_python, "tts_neural_local_v1");
    let versions_ready = version_mismatches.is_empty();
    let lockfile_runtime_ready = pack_lockfile_runtime_ready(lockfile_ready, versions_ready);
    let receipt_stale = !lockfile_ready && versions_ready && installed_lockfile_sha.is_some();
    let installed =
        package_version.is_some() && warmup_ready && lockfile_runtime_ready && versions_ready;
    let repair_required =
        !installed && (package_version.is_some() || installed_lockfile_sha.is_some());
    let status_detail = if installed {
        if receipt_stale {
            "Neural TTS is ready; the install receipt journal is stale but installed package versions match the bundled dependency lockfile.".to_string()
        } else {
            "Neural TTS is ready and matches the bundled dependency lockfile.".to_string()
        }
    } else if package_version.is_none() {
        "Kokoro is not installed in the managed Python environment.".to_string()
    } else if !warmup_ready {
        "Kokoro is installed, but its model is missing from the app-local cache the dub job reads. Run Install/Repair to provision it.".to_string()
    } else if !lockfile_runtime_ready {
        "Installed packages do not match the current Neural TTS lockfile. Run Install/Repair to refresh the pack.".to_string()
    } else if !versions_ready {
        let first = &version_mismatches[0];
        format!(
            "Installed package versions do not match the Neural TTS lockfile. First mismatch: {} requires {}, installed {}. Run Install/Repair to refresh the pack.",
            first.package,
            first.expected,
            first.installed.as_deref().unwrap_or("missing")
        )
    } else {
        "Neural TTS is not ready.".to_string()
    };

    TtsNeuralLocalV1PackStatus {
        installed,
        repair_required,
        status_detail,
        package_version,
        transformers_version,
        huggingface_hub_version,
        expected_lockfile_sha,
        installed_lockfile_sha,
        version_mismatches,
    }
}

pub fn install_tts_neural_local_v1_pack(paths: &AppPaths) -> Result<TtsNeuralLocalV1PackStatus> {
    let _probe_invalidation = CapabilityProbeInvalidationGuard::new();
    // Ensure venv exists first.
    let _ = install_python_toolchain(paths)?;
    let venv_python = python_venv_python_path(paths)?;
    let pin = &pinned_dependency_manifest::manifest().tts_neural_local_v1;

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
        &pip_install_args(
            &["-m", "pip", "install", "--upgrade"],
            &pin.compatibility_upgrades,
        ),
        "pip upgrade click/typer compatibility for neural TTS failed",
    );

    // WP-0232: prefer the hashed lockfile path when present. The lockfile resolves the
    // entire dep tree at build time; pip just downloads exact wheels and verifies sha256.
    // Eliminates the WP-0231 class of resolver-drift bug.
    //
    // WP-0231 fallback: if no lockfile is bundled (older offline payload, dev tree),
    // fall back to the legacy `pip install --upgrade <pinned list>` path so the install
    // is never silently bypassed.
    let install_err = match locate_pack_lockfile("tts_neural_local_v1") {
        Some(lockfile_path) => install_pack_from_lockfile(
            paths,
            &venv_python,
            "tts_neural_local_v1",
            lockfile_path,
            "neural TTS dependency install",
        ),
        None => {
            let pinned_args = pip_install_args(&["-m", "pip", "install", "--upgrade"], &pin.pinned);
            run_python_checked(
                paths,
                &venv_python,
                &pinned_args,
                "pip install neural TTS dependencies failed (pinned, legacy path; no lockfile bundled)",
            )
        }
    };
    if let Err(err) = install_err {
        if !pinned_dependency_manifest::allow_unpinned_fallback() {
            return Err(unpinned_fallback_disabled_error(
                "neural TTS dependency install",
                &err,
            ));
        }
        let fallback_args = pip_install_args(
            &["-m", "pip", "install", "--upgrade"],
            &pin.unpinned_fallback,
        );
        run_python_checked(
            paths,
            &venv_python,
            &fallback_args,
            &format!("pip install neural TTS dependencies failed (unpinned fallback): {err}"),
        )?;
    }

    let warmup_args: [&str; 2] = [
        "-c",
        concat!(
            "from kokoro import KPipeline; ",
            "pipeline = KPipeline(lang_code='a'); ",
            "result = next(iter(pipeline('warmup', voice='af_heart'))); ",
            "audio = getattr(result, 'audio', None); ",
            "nested = getattr(result, 'output', None) if audio is None else None; ",
            "audio = getattr(nested, 'audio', None) if audio is None and nested is not None else audio; ",
            "assert audio is not None, 'kokoro warmup produced no audio'; ",
            "print('ok')",
        ),
    ];

    let warmup_result = run_python_checked_with_retries(
        paths,
        &venv_python,
        &warmup_args,
        "neural TTS warmup failed",
        2,
    );

    if let Err(initial_err) = warmup_result {
        // WP-0231: one-shot self-heal. If the warmup probe still fails after the retry loop,
        // it almost always means the venv has a coherent-looking pip resolve but a stale
        // package version on disk (transformers / huggingface_hub / kokoro). Force-reinstall
        // only those three so unrelated installed packs (Spleeter, diarization, TTS preview)
        // are not disturbed, then retry the warmup once.
        if !pin.warmup_recovery_force_reinstall.is_empty() {
            let recovery_args = pip_install_args(
                &["-m", "pip", "install", "--force-reinstall", "--no-deps"],
                &pin.warmup_recovery_force_reinstall,
            );
            let recovery_install = run_python_checked(
                paths,
                &venv_python,
                &recovery_args,
                "neural TTS warmup recovery reinstall failed",
            );
            if let Err(recovery_err) = recovery_install {
                return Err(EngineError::InstallFailed(format!(
                    "{initial_err} (recovery reinstall also failed: {recovery_err})"
                )));
            }
            run_python_checked(
                paths,
                &venv_python,
                &warmup_args,
                &format!(
                    "neural TTS warmup still failing after recovery reinstall ({initial_err})"
                ),
            )?;
        } else {
            return Err(initial_err);
        }
    }

    // Only write the readiness marker once the model is actually present in the
    // app-local HF cache the OFFLINE dub job reads. This prevents the stale-marker
    // class of bug where the warmup populated a different cache (e.g. the default
    // user cache) yet the marker still claimed the offline job was ready.
    if !kokoro_app_cache_ready(paths) {
        return Err(EngineError::InstallFailed(
            "Kokoro warmup completed but the Kokoro-82M snapshot is missing from the \
             app-local Hugging Face cache the offline dub job reads (HF_HOME). The dub \
             job would fail at the first synth call; aborting instead of marking the \
             pack ready."
                .to_string(),
        ));
    }
    let warmup_probe = kokoro_warmup_probe_path(paths);
    if let Some(parent) = warmup_probe.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&warmup_probe, "ok\n")?;

    let status = tts_neural_local_v1_pack_status(paths);
    if !status.installed {
        let _ =
            pack_install_state::mark_failed(paths, "tts_neural_local_v1", &status.status_detail);
        return Err(EngineError::InstallFailed(format!(
            "neural TTS installation completed but status check failed: {}",
            status.status_detail
        )));
    }
    let _ = generate_pack_integrity_manifest(paths);
    Ok(status)
}

#[derive(Debug, Clone, Serialize)]
pub struct TtsVoicePreservingLocalV1PackStatus {
    pub installed: bool,
    pub repair_required: bool,
    pub status_detail: String,
    pub kokoro_version: Option<String>,
    pub openvoice_version: Option<String>,
    pub cosyvoice_version: Option<String>,
    pub openvoice_models_dir: String,
    pub openvoice_models_installed: bool,
    pub openvoice_patch_applied: bool,
    pub expected_lockfile_sha: Option<String>,
    pub installed_lockfile_sha: Option<String>,
    pub version_mismatches: Vec<PythonPackageVersionMismatch>,
}

/// WP-0229: skip dependency resolution and Kokoro warmup only when the
/// canonical neural-pack status is fully installed.
pub fn install_tts_neural_local_v1_pack_if_needed(
    paths: &AppPaths,
) -> Result<TtsNeuralLocalV1PackStatus> {
    let status = tts_neural_local_v1_pack_status(paths);
    if status.installed {
        return Ok(status);
    }
    install_tts_neural_local_v1_pack(paths)
}

pub fn tts_voice_preserving_local_v1_pack_status(
    paths: &AppPaths,
) -> TtsVoicePreservingLocalV1PackStatus {
    let openvoice_models_dir = paths
        .python_models_dir()
        .join("openvoice_v2")
        .to_string_lossy()
        .to_string();
    let (expected_lockfile_sha, installed_lockfile_sha) =
        pack_install_state_shas(paths, "tts_voice_preserving_local_v1");

    let venv_dir = paths.python_venv_dir();
    let venv_python = venv_python_path(&venv_dir);
    if !venv_python.exists() {
        return TtsVoicePreservingLocalV1PackStatus {
            installed: false,
            repair_required: false,
            status_detail: "Python environment is not installed yet.".to_string(),
            kokoro_version: None,
            openvoice_version: None,
            cosyvoice_version: None,
            openvoice_models_dir,
            openvoice_models_installed: false,
            openvoice_patch_applied: false,
            expected_lockfile_sha,
            installed_lockfile_sha,
            version_mismatches: Vec::new(),
        };
    }

    let kokoro_version = python_distribution_version(&venv_python, "kokoro");
    let openvoice_runtime_available = python_module_available(&venv_python, "openvoice.api");
    let cosyvoice_runtime_available = python_module_available(&venv_python, "cosyvoice");
    let openvoice_version = python_distribution_version(&venv_python, "MyShell-OpenVoice")
        .or_else(|| python_distribution_version(&venv_python, "openvoice"))
        .or_else(|| openvoice_runtime_available.then(|| "installed (module only)".to_string()));
    let cosyvoice_version = python_distribution_version(&venv_python, "cosyvoice")
        .or_else(|| cosyvoice_runtime_available.then(|| "installed (module only)".to_string()));
    let openvoice_patch_applied =
        vendor_patches::openvoice_api_patch_applied(&venv_python).unwrap_or(false);
    let kokoro_warmup_ready =
        kokoro_warmup_probe_path(paths).exists() && kokoro_app_cache_ready(paths);
    let kokoro_lockfile_ready = pack_install_satisfied(paths, "tts_neural_local_v1");
    let neural_base_status = tts_neural_local_v1_pack_status(paths);
    let lockfile_ready = pack_install_satisfied(paths, "tts_voice_preserving_local_v1");
    let version_mismatches =
        lockfile_source_pin_mismatches(&venv_python, "tts_voice_preserving_local_v1");
    let versions_ready = version_mismatches.is_empty();
    let lockfile_runtime_ready = pack_lockfile_runtime_ready(lockfile_ready, versions_ready);
    let receipt_stale = !lockfile_ready && versions_ready && installed_lockfile_sha.is_some();
    let models_dir = std::path::PathBuf::from(&openvoice_models_dir);
    let openvoice_models_installed = models_dir.join("converter").join("config.json").exists()
        && models_dir.join("converter").join("checkpoint.pth").exists();
    let installed = kokoro_version.is_some()
        && kokoro_warmup_ready
        && kokoro_lockfile_ready
        && neural_base_status.installed
        && openvoice_runtime_available
        && openvoice_version.is_some()
        && openvoice_models_installed
        && openvoice_patch_applied
        && lockfile_runtime_ready
        && versions_ready;
    let repair_required = !installed
        && (kokoro_version.is_some()
            || openvoice_version.is_some()
            || installed_lockfile_sha.is_some());
    let status_detail = if installed {
        if receipt_stale {
            "Voice-preserving dubbing is ready; the install receipt journal is stale but installed package versions match the bundled dependency lockfiles.".to_string()
        } else {
            "Voice-preserving dubbing is ready and matches the bundled dependency lockfiles."
                .to_string()
        }
    } else if kokoro_version.is_none() {
        "Kokoro base TTS is not installed; install Neural TTS first or run the full voice pack install.".to_string()
    } else if !kokoro_warmup_ready {
        "Kokoro base TTS is installed, but its model is missing from the app-local cache the dub job reads. Run Install/Repair to provision it.".to_string()
    } else if !kokoro_lockfile_ready {
        "Kokoro base TTS does not match the current Neural TTS lockfile. Run Install/Repair to refresh the pack.".to_string()
    } else if !neural_base_status.installed {
        format!(
            "Kokoro base TTS needs repair before voice preservation can run: {}",
            neural_base_status.status_detail
        )
    } else if !openvoice_runtime_available {
        "OpenVoice runtime module openvoice.api is missing from the managed Python environment."
            .to_string()
    } else if openvoice_version.is_none() {
        "OpenVoice is not installed in the managed Python environment.".to_string()
    } else if !openvoice_models_installed {
        "OpenVoice converter model files are missing.".to_string()
    } else if !openvoice_patch_applied {
        "OpenVoice runtime patch is missing.".to_string()
    } else if !lockfile_runtime_ready {
        "Installed packages do not match the current OpenVoice lockfile. Run Install/Repair to refresh the pack.".to_string()
    } else if !versions_ready {
        let first = &version_mismatches[0];
        format!(
            "Installed package versions do not match the OpenVoice lockfile. First mismatch: {} requires {}, installed {}. Run Install/Repair to refresh the pack.",
            first.package,
            first.expected,
            first.installed.as_deref().unwrap_or("missing")
        )
    } else {
        "Voice-preserving dubbing is not ready.".to_string()
    };

    TtsVoicePreservingLocalV1PackStatus {
        installed,
        repair_required,
        status_detail,
        kokoro_version,
        openvoice_version,
        cosyvoice_version,
        openvoice_models_dir,
        openvoice_models_installed,
        openvoice_patch_applied,
        expected_lockfile_sha,
        installed_lockfile_sha,
        version_mismatches,
    }
}

pub fn install_tts_voice_preserving_local_v1_pack(
    paths: &AppPaths,
) -> Result<TtsVoicePreservingLocalV1PackStatus> {
    let _probe_invalidation = CapabilityProbeInvalidationGuard::new();
    let _ = install_python_toolchain(paths)?;
    let venv_python = python_venv_python_path(paths)?;
    let pin = &pinned_dependency_manifest::manifest().tts_voice_preserving_local_v1;

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
    let attempts = vec![vec![
        "-m",
        "pip",
        "install",
        "--upgrade",
        "--no-deps",
        pin.openvoice_git_spec.as_str(),
    ]];
    for args in attempts {
        match run_python_checked(paths, &venv_python, &args, "pip install OpenVoice failed") {
            Ok(()) => {
                openvoice_installed = true;
                status_error = None;
                break;
            }
            Err(err) => status_error = Some(err.to_string()),
        }
    }

    if !openvoice_installed {
        return Err(EngineError::InstallFailed(status_error.unwrap_or_else(
            || "OpenVoice install failed without a captured error".to_string(),
        )));
    }

    // WP-0232: prefer the hashed lockfile for OpenVoice's pinned deps. The OpenVoice
    // git+ install above runs with `--no-deps` so it does not appear in this lockfile;
    // this step only installs the pinned_dependencies list.
    let deps_err = match locate_pack_lockfile("tts_voice_preserving_local_v1") {
        Some(lockfile_path) => install_pack_from_lockfile(
            paths,
            &venv_python,
            "tts_voice_preserving_local_v1",
            lockfile_path,
            "OpenVoice dependency install",
        ),
        None => {
            let pinned_args = pip_install_args(
                &["-m", "pip", "install", "--upgrade"],
                &pin.pinned_dependencies,
            );
            run_python_checked(
                paths,
                &venv_python,
                &pinned_args,
                "pip install OpenVoice dependencies failed (pinned, legacy path; no lockfile bundled)",
            )
        }
    };
    if let Err(err) = deps_err {
        if !pinned_dependency_manifest::allow_unpinned_fallback() {
            return Err(unpinned_fallback_disabled_error(
                "OpenVoice dependency install",
                &err,
            ));
        }
        let fallback_args = pip_install_args(
            &["-m", "pip", "install", "--upgrade"],
            &pin.unpinned_fallback_dependencies,
        );
        let _ = run_python_checked(
            paths,
            &venv_python,
            &fallback_args,
            &format!("pip install OpenVoice dependencies failed (unpinned fallback): {err}"),
        )?;
    }

    vendor_patches::patch_openvoice_api_enable_watermark(&venv_python)?;

    let models_dir = paths.python_models_dir().join("openvoice_v2");
    std::fs::create_dir_all(&models_dir)?;

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

files = {files_json}

downloaded = []
for entry in files:
  filename = entry["filename"]
  expected = entry["sha256_hex"].lower()
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
        repo_id = pin.openvoice_v2.repo_id,
        revision = pin.openvoice_v2.revision,
        files_json = serde_json::to_string_pretty(&pin.openvoice_v2.files)
            .unwrap_or_else(|_| "[]".to_string()),
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
        let _ = pack_install_state::mark_failed(
            paths,
            "tts_voice_preserving_local_v1",
            &status.status_detail,
        );
        return Err(EngineError::InstallFailed(status_error.unwrap_or_else(
            || {
                format!(
                    "voice-preserving pack installation completed but status check failed: {}",
                    status.status_detail
                )
            },
        )));
    }

    let _ = generate_pack_integrity_manifest(paths);
    Ok(status)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pyttsx3Voice {
    pub id: String,
    pub name: String,
}

/// WP-0229: the multi-condition voice-preserving status is the canonical
/// short-circuit; explicit force repair continues to call the bare installer.
pub fn install_tts_voice_preserving_local_v1_pack_if_needed(
    paths: &AppPaths,
) -> Result<TtsVoicePreservingLocalV1PackStatus> {
    let status = tts_voice_preserving_local_v1_pack_status(paths);
    if status.installed {
        return Ok(status);
    }
    install_tts_voice_preserving_local_v1_pack(paths)
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
        paths
            .cache_dir()
            .join("python")
            .to_string_lossy()
            .to_string(),
    );

    let output = cmd
        .owned_output()
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
    let output = crate::cmd::command(python)
        .args(["-c", &code])
        .owned_output()
        .ok()?;
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

fn python_module_version_with_pid(
    python: &std::path::Path,
    module: &str,
) -> std::result::Result<(Option<String>, Option<u32>), String> {
    let code = format!(
        "import importlib, json\ntry:\n m=importlib.import_module({module:?})\n print(json.dumps({{'state':'installed','version':getattr(m,'__version__', 'installed') or 'installed'}}))\nexcept ModuleNotFoundError as e:\n if e.name == {module:?}: print(json.dumps({{'state':'missing'}}))\n else: print(json.dumps({{'state':'failed','error':str(e)}}))\nexcept Exception as e:\n print(json.dumps({{'state':'failed','error':str(e)}}))\n"
    );
    let mut command = crate::cmd::command(python);
    command.args(["-c", &code]);
    let (output, child_pid) = wait_for_owned_capability_probe(
        &mut command,
        "Python module probe",
        CAPABILITY_PROBE_CHILD_TIMEOUT,
    )?;
    if !output.status.success() {
        return Err(format!(
            "Python module probe pid {child_pid} exited unsuccessfully: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .ok_or_else(|| format!("Python module probe pid {child_pid} produced no JSON result"))?;
    let parsed: serde_json::Value = serde_json::from_str(line.trim()).map_err(|error| {
        format!("Python module probe pid {child_pid} returned malformed JSON: {error}")
    })?;
    match parsed.get("state").and_then(|value| value.as_str()) {
        Some("installed") => Ok((
            parsed
                .get("version")
                .and_then(|value| value.as_str())
                .map(str::to_string),
            Some(child_pid),
        )),
        Some("missing") => Ok((None, Some(child_pid))),
        Some("failed") => Err(format!(
            "Python module import failed in probe pid {child_pid}: {}",
            parsed
                .get("error")
                .and_then(|value| value.as_str())
                .unwrap_or("unknown error")
        )),
        _ => Err(format!(
            "Python module probe pid {child_pid} returned an unknown state"
        )),
    }
}

fn python_module_available(python: &std::path::Path, module: &str) -> bool {
    let code = format!(
        "import importlib.util\ntry:\n    found = importlib.util.find_spec({module:?}) is not None\nexcept Exception:\n    found = False\nraise SystemExit(0 if found else 1)\n"
    );
    let mut command = crate::cmd::command(python);
    command.args(["-c", &code]);
    command
        .owned_output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn python_distribution_version(python: &std::path::Path, distribution: &str) -> Option<String> {
    let code = format!("import importlib.metadata as m\nprint(m.version({distribution:?}))\n");
    let output = crate::cmd::command(python)
        .args(["-c", &code])
        .owned_output()
        .ok()?;
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

fn run_python_checked(
    paths: &AppPaths,
    python: &std::path::Path,
    args: &[&str],
    error_prefix: &str,
) -> Result<()> {
    run_python_checked_with_timeout(
        paths,
        python,
        args,
        error_prefix,
        PYTHON_COMMAND_TIMEOUT_SECS,
    )
}

/// Like `run_python_checked` but with a caller-chosen timeout. The CosyVoice install
/// (torch 2.3.1 stack + a multi-GB model on a throttled connection) can exceed the
/// default 30-minute command timeout, so it passes a longer budget.
fn run_python_checked_with_timeout(
    paths: &AppPaths,
    python: &std::path::Path,
    args: &[&str],
    error_prefix: &str,
    timeout_secs: u64,
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
        paths
            .cache_dir()
            .join("python")
            .to_string_lossy()
            .to_string(),
    );
    cmd.env(
        "HF_HOME",
        paths
            .cache_dir()
            .join("huggingface")
            .to_string_lossy()
            .to_string(),
    );
    cmd.env(
        "HUGGINGFACE_HUB_CACHE",
        paths
            .cache_dir()
            .join("huggingface")
            .join("hub")
            .to_string_lossy()
            .to_string(),
    );
    cmd.env("HF_HUB_DISABLE_XET", "1");
    cmd.env("HF_HUB_DOWNLOAD_TIMEOUT", "300");
    cmd.env("HF_HUB_ETAG_TIMEOUT", "30");

    let output = crate::cmd::run_owned_output(
        &mut cmd,
        std::time::Duration::from_secs(timeout_secs),
        crate::jobs::external_command_cancel_requested,
    )
    .map_err(|error| EngineError::InstallFailed(format!("{error_prefix}: {error}")))?;
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

fn run_python_checked_with_retries(
    paths: &AppPaths,
    python: &std::path::Path,
    args: &[&str],
    error_prefix: &str,
    attempts: usize,
) -> Result<()> {
    let attempts = attempts.max(1);
    let mut last_error: Option<String> = None;
    for attempt in 1..=attempts {
        match run_python_checked(paths, python, args, error_prefix) {
            Ok(()) => return Ok(()),
            Err(err) => {
                last_error = Some(err.to_string());
                if attempt < attempts {
                    std::thread::sleep(std::time::Duration::from_secs((1_u64 << attempt).min(20)));
                }
            }
        }
    }
    Err(EngineError::InstallFailed(format!(
        "{error_prefix} after {attempts} attempts: {}",
        last_error.unwrap_or_else(|| "unknown error".to_string())
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_verification_foreground_pressure_is_generation_safe_and_observable() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app"));

        let active =
            set_youtube_po_provider_verification_foreground_demand(&paths, "diagnostics", 2, true);
        assert!(active.active);
        assert_eq!(active.active_consumers, 1);
        assert_eq!(
            active.held_reason.as_deref(),
            Some("foreground_navigation_job_or_probe_demand")
        );
        assert_eq!(
            active.resource_policy,
            PROVIDER_VERIFICATION_FOREGROUND_POLICY
        );

        let stale_clear =
            set_youtube_po_provider_verification_foreground_demand(&paths, "diagnostics", 1, false);
        assert!(
            stale_clear.active,
            "a stale generation cannot clear a newer lease"
        );

        begin_provider_verification_progress(&paths);
        update_provider_verification_progress(
            &paths.youtube_po_provider_server_dir(),
            "provider_tree_verify",
            16,
            4096,
        );
        let progress = youtube_po_provider_verification_progress(&paths).expect("progress");
        assert!(progress.foreground_pressure_active);
        assert_eq!(progress.held_reason, active.held_reason);
        assert_eq!(progress.resource_policy, active.resource_policy);

        let cleared =
            set_youtube_po_provider_verification_foreground_demand(&paths, "diagnostics", 2, false);
        assert!(!cleared.active);
        assert_eq!(
            cleared.resource_policy,
            PROVIDER_VERIFICATION_BACKGROUND_POLICY
        );
    }

    #[test]
    fn provider_verification_checkpoint_policies_keep_foreground_chunks_smaller() {
        assert_eq!(
            provider_verification_resource_policy(false),
            "single_flight_32_file_yield_256_file_1ms_checkpoint"
        );
        assert_eq!(
            provider_verification_resource_policy(true),
            "foreground_checkpoint_4_file_yield_16_file_2ms_sleep"
        );
    }

    #[test]
    fn provider_verification_failure_keeps_unknown_planned_totals_truthful() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app"));
        paths.ensure_dirs().expect("ensure app dirs");
        begin_provider_verification_progress(&paths);
        update_provider_verification_progress(
            &paths.youtube_po_provider_server_dir(),
            "provider_tree_verify",
            2,
            1024,
        );
        finish_provider_verification_progress(&paths, Some("interrupted".to_string()));

        let progress = youtube_po_provider_verification_progress(&paths).expect("progress");
        assert_eq!(progress.state, "error");
        assert_eq!(progress.files_completed, 2);
        assert_eq!(progress.bytes_completed, 1024);
        assert_eq!(progress.files_planned, None);
        assert_eq!(progress.bytes_planned, None);
    }

    #[test]
    fn concurrent_provider_ensure_shares_injected_panic_terminal() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app"));
        paths.ensure_dirs().expect("ensure app dirs");
        provider_verification_injected_panic().store(true, std::sync::atomic::Ordering::SeqCst);
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let calls = (0..2)
            .map(|_| {
                let paths = paths.clone();
                let barrier = std::sync::Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    ensure_youtube_po_provider(&paths)
                        .expect_err("injected verifier panic must fail closed")
                        .to_string()
                })
            })
            .collect::<Vec<_>>();
        barrier.wait();
        let errors = calls
            .into_iter()
            .map(|call| call.join().expect("ensure caller join"))
            .collect::<Vec<_>>();
        assert_eq!(errors[0], errors[1]);
        assert!(errors[0].contains("panicked before producing a terminal receipt"));
        assert!(youtube_po_provider_slot()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_none());
        let progress = youtube_po_provider_verification_progress(&paths)
            .expect("panic after progress start must leave a terminal receipt");
        assert_eq!(progress.state, "error");
        assert!(progress.finished_at_ms.is_some());
        assert_eq!(progress.error.as_deref(), Some(errors[0].as_str()));
        assert_eq!(progress.scan_count, 1);
    }

    #[test]
    fn progress_aware_complete_provider_authentication_is_one_counted_scan() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app"));
        paths.ensure_dirs().expect("ensure app dirs");
        std::fs::create_dir_all(paths.node_runtime_dir()).expect("node dir");
        std::fs::create_dir_all(paths.youtube_po_provider_dir()).expect("provider dir");
        std::fs::write(paths.node_runtime_dir().join("node_fixture"), b"node").expect("node");
        std::fs::write(
            paths.youtube_po_provider_dir().join("provider_fixture"),
            b"provider",
        )
        .expect("provider");
        let expected_node =
            canonical_provider_node_tree_sha256_hex(&paths.node_runtime_dir()).expect("node root");
        let expected_provider =
            canonical_provider_application_tree_sha256_hex(&paths.youtube_po_provider_dir())
                .expect("provider root");

        begin_provider_verification_progress(&paths);
        let progress_key = paths.youtube_po_provider_server_dir();
        authenticate_complete_provider_trees_against_with_progress(
            &paths,
            &expected_node,
            &expected_provider,
            Some(&progress_key),
        )
        .expect("single-pass authentication");
        finish_provider_verification_progress(&paths, None);

        let progress = youtube_po_provider_verification_progress(&paths).expect("progress");
        assert_eq!(progress.state, "ready");
        assert_eq!(progress.scan_count, 1);
        assert_eq!(progress.files_planned, Some(progress.files_completed));
        assert_eq!(progress.bytes_planned, Some(progress.bytes_completed));
        assert!(progress.files_completed >= 1);
        assert!(progress.bytes_completed >= 8);
    }

    #[test]
    fn semantic_probe_single_flight_shares_one_computation_and_invalidates() {
        let slot = std::sync::Arc::new(SemanticProbeSlot::<u32>::new());
        let starts = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let gate = std::sync::Arc::new(std::sync::Barrier::new(8));
        let mut workers = Vec::new();
        for _ in 0..8 {
            let slot = slot.clone();
            let starts = starts.clone();
            let gate = gate.clone();
            workers.push(std::thread::spawn(move || {
                gate.wait();
                slot.run("same-runtime".to_string(), || {
                    starts.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    std::thread::sleep(std::time::Duration::from_millis(40));
                    Ok(42)
                })
            }));
        }
        let outcomes = workers
            .into_iter()
            .map(|worker| worker.join().expect("probe waiter"))
            .collect::<Vec<_>>();
        assert!(outcomes.iter().all(|outcome| outcome.value == Some(42)));
        assert_eq!(starts.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert!(outcomes.iter().any(|outcome| outcome.shared_flight));

        slot.invalidate();
        let rerun = slot.run("same-runtime".to_string(), || {
            starts.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(43)
        });
        assert_eq!(rerun.value, Some(43));
        assert_eq!(starts.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[test]
    fn semantic_probe_failure_is_shared_once_then_a_later_request_can_retry() {
        let slot = std::sync::Arc::new(SemanticProbeSlot::<u32>::new());
        let starts = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let gate = std::sync::Arc::new(std::sync::Barrier::new(8));
        let mut workers = Vec::new();
        for _ in 0..8 {
            let slot = slot.clone();
            let starts = starts.clone();
            let gate = gate.clone();
            workers.push(std::thread::spawn(move || {
                gate.wait();
                slot.run("failed-runtime".to_string(), || {
                    starts.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    Err("shared injected failure from probe pid 4242".to_string())
                })
            }));
        }
        let outcomes = workers
            .into_iter()
            .map(|worker| worker.join().expect("failed probe waiter"))
            .collect::<Vec<_>>();
        assert_eq!(starts.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert!(outcomes
            .iter()
            .all(|outcome| outcome.probe_state == "failed"));
        assert!(outcomes.iter().all(|outcome| {
            outcome.error.as_deref() == Some("shared injected failure from probe pid 4242")
        }));
        assert_eq!(
            outcomes
                .iter()
                .filter(|outcome| outcome.shared_flight)
                .count(),
            7,
            "every joined consumer must receive the owner's one terminal failure",
        );

        let retry = slot.run("failed-runtime".to_string(), || {
            starts.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(9)
        });
        assert_eq!(retry.value, Some(9));
        assert_eq!(starts.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[test]
    fn owned_capability_probe_timeout_terminates_and_reaps_the_child() {
        #[cfg(windows)]
        let mut command = {
            let mut command = crate::cmd::command("ping.exe");
            command.args(["-n", "30", "127.0.0.1"]);
            command
        };
        #[cfg(not(windows))]
        let mut command = {
            let mut command = crate::cmd::command("sh");
            command.args(["-c", "sleep 30"]);
            command
        };
        let started = std::time::Instant::now();
        let error = wait_for_owned_capability_probe(
            &mut command,
            "bounded capability fixture",
            std::time::Duration::from_millis(100),
        )
        .expect_err("sleeping owned child must time out");
        assert!(error.contains("pid "));
        assert!(error.contains("was terminated"));
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
    }

    #[test]
    fn semantic_probe_panic_releases_flight_for_retry() {
        let slot = SemanticProbeSlot::<u32>::new();
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = slot.run("runtime".to_string(), || panic!("injected"));
        }));
        assert!(panicked.is_err());
        let retry = slot.run("runtime".to_string(), || Ok(7));
        assert_eq!(retry.value, Some(7));
        assert_eq!(retry.freshness, "verified");
    }

    #[test]
    fn semantic_probe_invalidation_discards_in_flight_completion() {
        let slot = std::sync::Arc::new(SemanticProbeSlot::<u32>::new());
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (resume_tx, resume_rx) = std::sync::mpsc::channel();
        let worker_slot = slot.clone();
        let worker = std::thread::spawn(move || {
            worker_slot.run("runtime-a".to_string(), || {
                started_tx.send(()).expect("signal probe start");
                resume_rx.recv().expect("resume probe");
                Ok(11)
            })
        });
        started_rx.recv().expect("probe started");
        slot.invalidate();
        resume_tx.send(()).expect("release probe");
        let superseded = worker.join().expect("probe worker");
        assert_eq!(superseded.probe_state, "superseded");
        assert_eq!(superseded.value, None);

        let fresh = slot.run("runtime-b".to_string(), || Ok(12));
        assert_eq!(fresh.value, Some(12));
        assert_eq!(fresh.probe_state, "verified");
    }

    #[test]
    fn semantic_probe_waiter_with_new_identity_runs_once_after_old_terminal() {
        let slot = std::sync::Arc::new(SemanticProbeSlot::<u32>::new());
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (resume_tx, resume_rx) = std::sync::mpsc::channel();
        let owner_slot = slot.clone();
        let owner = std::thread::spawn(move || {
            owner_slot.run("runtime-a".to_string(), || {
                started_tx.send(()).expect("signal old probe start");
                resume_rx.recv().expect("resume old probe");
                Ok(11)
            })
        });
        started_rx.recv().expect("old probe started");

        let new_starts = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let waiter_slot = slot.clone();
        let waiter_starts = new_starts.clone();
        let waiter = std::thread::spawn(move || {
            waiter_slot.run("runtime-b".to_string(), || {
                waiter_starts.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(12)
            })
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            let joined = slot
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .active_waiters;
            if joined == 1 {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "new identity did not join old flight"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        slot.invalidate();
        resume_tx.send(()).expect("release old probe");
        assert_eq!(owner.join().expect("old owner").probe_state, "superseded");
        let fresh = waiter.join().expect("new identity waiter");
        assert_eq!(fresh.value, Some(12));
        assert_eq!(new_starts.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert!(
            slot.state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .terminal_by_flight
                .is_empty(),
            "a waiter gated behind a different source identity must consume the old terminal slot",
        );
    }

    #[cfg(windows)]
    #[test]
    fn provider_process_identity_rejects_exited_process_with_retained_handle() {
        let mut child = std::process::Command::new("cmd")
            .args(["/d", "/c", "exit", "0"])
            .spawn()
            .expect("spawn short-lived process");
        let pid = child.id();
        assert!(provider_process_identity(pid).is_some());
        child.wait().expect("wait for short-lived process");

        assert!(
            provider_process_identity(pid).is_none(),
            "a retained handle must not make an exited process look active"
        );
    }

    #[test]
    fn phase2_plan_includes_managed_cosyvoice_pack() {
        let plan = phase2_packs_install_plan();
        assert!(plan
            .iter()
            .any(|item| item.id == "voice_clone_cosyvoice_v1" && item.supported));
    }

    #[test]
    fn phase2_setup_estimate_is_manifest_owned_and_coherent() {
        let estimate = phase2_packs_setup_estimate();
        assert_eq!(estimate.download_bytes, 3_000_000_000);
        assert_eq!(estimate.reference_mbps, 100);
        assert!(estimate.min_minutes > 0);
        assert!(estimate.max_minutes >= estimate.min_minutes);
        assert!(!estimate.basis.trim().is_empty());
    }

    #[test]
    fn cosyvoice_readiness_requires_app_local_nonempty_wetext_assets() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app"));
        let python = venv_python_path(&paths.python_cosyvoice_venv_dir());
        std::fs::create_dir_all(python.parent().expect("python parent")).expect("python dir");
        std::fs::write(&python, b"python").expect("python fixture");

        let backend = paths.cosyvoice_backend_dir();
        std::fs::create_dir_all(
            backend
                .join("third_party")
                .join("Matcha-TTS")
                .join("matcha"),
        )
        .expect("matcha fixture");
        std::fs::write(
            backend.join("voxvulgi_cosyvoice_render.py"),
            COSYVOICE_RENDER_WRAPPER,
        )
        .expect("wrapper fixture");
        let model = backend.join("pretrained_models").join("CosyVoice2-0.5B");
        for relative in [
            "cosyvoice2.yaml",
            "llm.pt",
            "flow.pt",
            "hift.pt",
            "CosyVoice-BlankEN/model.safetensors",
        ] {
            let path = model.join(relative);
            std::fs::create_dir_all(path.parent().expect("model parent")).expect("model dir");
            std::fs::write(path, b"model").expect("model fixture");
        }

        let incomplete = cosyvoice_pack_status(&paths);
        assert!(!incomplete.installed);
        assert!(!incomplete.wetext_assets_present);

        for relative in [
            "en/tn/tagger.fst",
            "en/tn/verbalizer.fst",
            "zh/tn/tagger.fst",
            "zh/tn/verbalizer.fst",
        ] {
            let path = backend.join("wetext").join(relative);
            std::fs::create_dir_all(path.parent().expect("wetext parent")).expect("wetext dir");
            std::fs::write(path, b"fst").expect("wetext fixture");
        }
        let complete = cosyvoice_pack_status(&paths);
        assert!(complete.installed);
        assert!(complete.wetext_assets_present);

        std::fs::write(
            backend.join("voxvulgi_cosyvoice_render.py"),
            b"stale wrapper",
        )
        .expect("stale wrapper fixture");
        let stale = cosyvoice_pack_status(&paths);
        assert!(!stale.installed);
        assert!(stale.render_script_present);
        assert!(!stale.render_script_current);
    }

    static PROVIDER_INTEGRITY_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn provider_test_ownership_token() -> String {
        format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        )
    }

    #[test]
    fn diarization_runtime_validation_exercises_runtime_dependency_chain() {
        let code = diarization_runtime_validation_code();
        for required in [
            "resemblyzer",
            "VoiceEncoder",
            "librosa",
            "numba",
            "llvmlite",
            "webrtcvad",
            "soundfile",
            "sklearn.cluster",
        ] {
            assert!(
                code.contains(required),
                "validation script should exercise {required}"
            );
        }
    }

    #[test]
    fn diarization_pack_never_reports_installed_for_lockfile_version_drift() {
        assert!(diarization_pack_is_installed(true, true, true));
        assert!(!diarization_pack_is_installed(true, false, false));
        assert!(!diarization_pack_is_installed(true, true, false));
        assert!(!diarization_pack_is_installed(false, true, true));
    }

    #[test]
    fn pack_lockfile_runtime_ready_allows_stale_receipt_when_versions_match() {
        assert!(pack_lockfile_runtime_ready(true, true));
        assert!(pack_lockfile_runtime_ready(false, true));
        assert!(!pack_lockfile_runtime_ready(false, false));
    }

    #[test]
    fn provider_lifecycle_allowlist_rejects_missing_wildcard_or_extra_approvals() {
        let exact = serde_json::json!({
            "allowScripts": {
                "canvas@3.2.3": true,
                "@swc/core@1.15.47": false,
            }
        });
        assert!(provider_lifecycle_allowlist_is_exact(&exact));
        for invalid in [
            serde_json::json!({}),
            serde_json::json!({"allowScripts": {"canvas": true, "@swc/core@1.15.47": false}}),
            serde_json::json!({"allowScripts": {"canvas@3.2.3": true, "@swc/core@1.15.47": true}}),
            serde_json::json!({"allowScripts": {"canvas@3.2.3": true, "@swc/core@1.15.47": false, "prebuild-install": true}}),
        ] {
            assert!(!provider_lifecycle_allowlist_is_exact(&invalid));
        }
        assert_eq!(
            provider_npm_ci_args(),
            ["ci", "--ignore-scripts"],
            "pinned npm 11.17 must suppress every lifecycle script before the exact canvas rebuild"
        );
        assert_eq!(
            pinned_dependency_manifest::manifest()
                .node_windows
                .npm_version,
            "11.17.0",
            "the executable contract must match npm bundled in the exact Node archive"
        );
    }

    #[test]
    fn embedded_provider_lock_hash_rejects_tampering() {
        let pin = &pinned_dependency_manifest::manifest().youtube_po_provider;
        let lock = pinned_dependency_manifest::YOUTUBE_PO_PROVIDER_DERIVED_LOCK.as_bytes();
        assert!(provider_lock_matches_manifest(
            lock,
            &pin.derived_lock_sha256_hex
        ));
        let mut tampered = lock.to_vec();
        tampered[0] ^= 1;
        assert!(!provider_lock_matches_manifest(
            &tampered,
            &pin.derived_lock_sha256_hex
        ));
        assert!(provider_lock_lifecycle_packages_are_exact(lock));
        let unexpected = br#"{"packages":{"node_modules/canvas":{"version":"3.2.3","hasInstallScript":true},"node_modules/evil":{"version":"1.0.0","hasInstallScript":true}}}"#;
        assert!(!provider_lock_lifecycle_packages_are_exact(unexpected));
    }

    #[test]
    fn provider_lifecycle_packages_distinguish_build_and_pruned_runtime_sets() {
        let base = std::env::temp_dir().join(format!(
            "voxvulgi_provider_lifecycle_sets_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let server = base.join("server");
        for (name, version) in [("canvas", "3.2.3"), ("@swc/core", "1.15.47")] {
            let package = name
                .split('/')
                .fold(server.join("node_modules"), |path, part| path.join(part))
                .join("package.json");
            std::fs::create_dir_all(package.parent().unwrap()).unwrap();
            std::fs::write(
                package,
                serde_json::to_vec(&serde_json::json!({"version": version})).unwrap(),
            )
            .unwrap();
        }
        assert!(installed_provider_build_lifecycle_packages_are_exact(
            &server
        ));
        assert!(!installed_provider_runtime_lifecycle_packages_are_exact(
            &server
        ));

        std::fs::remove_dir_all(server.join("node_modules").join("@swc").join("core")).unwrap();
        assert!(!installed_provider_build_lifecycle_packages_are_exact(
            &server
        ));
        assert!(installed_provider_runtime_lifecycle_packages_are_exact(
            &server
        ));

        std::fs::write(
            server
                .join("node_modules")
                .join("canvas")
                .join("package.json"),
            br#"{"version":"3.2.2"}"#,
        )
        .unwrap();
        assert!(!installed_provider_runtime_lifecycle_packages_are_exact(
            &server
        ));
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn provider_sealing_removes_location_dependent_build_only_artifacts() {
        let base = std::env::temp_dir().join(format!(
            "voxvulgi_provider_build_artifacts_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let server = base.join("server");
        std::fs::create_dir_all(server.join(".npm_cache").join("logs")).unwrap();
        std::fs::write(
            server.join(".npm_cache").join("logs").join("run.log"),
            b"run",
        )
        .unwrap();
        std::fs::write(
            server.join("tsconfig.tsbuildinfo"),
            br#"{"fileNames":["../../ambient/node_modules/@types/react/index.d.ts"]}"#,
        )
        .unwrap();

        remove_provider_build_only_artifacts(&server).unwrap();
        assert!(!server.join(".npm_cache").exists());
        assert!(!server.join("tsconfig.tsbuildinfo").exists());
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn provider_plugin_tree_rejects_same_size_tamper_with_intact_archive_marker() {
        let base = std::env::temp_dir().join(format!(
            "voxvulgi_plugin_tamper_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let plugin = base.join("plugin");
        let script = plugin
            .join("yt_dlp_plugins")
            .join("extractor")
            .join("provider.py");
        std::fs::create_dir_all(script.parent().unwrap()).unwrap();
        std::fs::write(&script, b"trusted-bytes").unwrap();
        std::fs::write(
            plugin.join(".plugin_archive_sha256"),
            b"INTACT_ARCHIVE_MARKER\n",
        )
        .unwrap();
        let mut expected = std::collections::BTreeMap::new();
        expected.insert(
            "yt_dlp_plugins/extractor/provider.py".to_string(),
            file_sha256_hex(&script).unwrap(),
        );
        assert!(provider_plugin_tree_sha256_hex(&plugin, &expected).is_some());
        std::fs::write(&script, b"tamperd-bytes").unwrap();
        assert_eq!(std::fs::metadata(&script).unwrap().len(), 13);
        assert!(provider_plugin_tree_sha256_hex(&plugin, &expected).is_none());
        assert_eq!(
            std::fs::read_to_string(plugin.join(".plugin_archive_sha256"))
                .unwrap()
                .trim(),
            "INTACT_ARCHIVE_MARKER"
        );
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn published_node_and_provider_identity_reject_non_node_same_size_tamper() {
        let base = std::env::temp_dir().join(format!(
            "voxvulgi_provider_published_identity_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let paths = AppPaths::new(base.clone());
        paths.ensure_dirs().unwrap();

        std::fs::create_dir_all(paths.node_runtime_dir()).unwrap();
        std::fs::write(paths.node_exe(), b"fixture-node").unwrap();
        std::fs::write(paths.node_npm_cmd(), b"fixture-npm").unwrap();
        let node_hash = file_sha256_hex(&paths.node_exe()).unwrap();
        let npm_hash = file_sha256_hex(&paths.node_npm_cmd()).unwrap();
        authenticate_published_node_payload_against(&paths, &node_hash, &npm_hash).unwrap();

        let plugin_dir = paths.youtube_po_provider_plugin_dir();
        let plugin_script = plugin_dir
            .join("yt_dlp_plugins")
            .join("extractor")
            .join("provider.py");
        std::fs::create_dir_all(plugin_script.parent().unwrap()).unwrap();
        std::fs::write(&plugin_script, b"trusted-plugin").unwrap();
        std::fs::write(
            plugin_dir.join(".plugin_archive_sha256"),
            b"FIXTURE-ARCHIVE\n",
        )
        .unwrap();
        let mut plugin_files = std::collections::BTreeMap::new();
        plugin_files.insert(
            "yt_dlp_plugins/extractor/provider.py".to_string(),
            file_sha256_hex(&plugin_script).unwrap(),
        );
        let plugin_tree = provider_plugin_tree_sha256_hex(&plugin_dir, &plugin_files).unwrap();

        let server_dir = paths.youtube_po_provider_server_dir();
        let entrypoint = paths.youtube_po_provider_entrypoint();
        std::fs::create_dir_all(entrypoint.parent().unwrap()).unwrap();
        std::fs::write(&entrypoint, b"trusted-server").unwrap();
        let lock = server_dir.join("package-lock.json");
        std::fs::write(&lock, b"trusted-lock").unwrap();
        let lock_hash = file_sha256_hex(&lock).unwrap();
        std::fs::write(
            server_dir.join(".production_audit_zero"),
            format!("{lock_hash}\n"),
        )
        .unwrap();
        std::fs::write(
            server_dir.join("package.json"),
            serde_json::to_vec(&serde_json::json!({
                "allowScripts": {
                    "canvas@3.2.3": true,
                    "@swc/core@1.15.47": false
                }
            }))
            .unwrap(),
        )
        .unwrap();
        for (name, version) in [("canvas", "3.2.3")] {
            let package = name
                .split('/')
                .fold(server_dir.join("node_modules"), |path, part| {
                    path.join(part)
                })
                .join("package.json");
            std::fs::create_dir_all(package.parent().unwrap()).unwrap();
            std::fs::write(
                package,
                serde_json::to_vec(&serde_json::json!({"version": version})).unwrap(),
            )
            .unwrap();
        }
        let node_modules_tree =
            canonical_directory_tree_sha256_hex(&server_dir.join("node_modules")).unwrap();
        let entrypoint_hash = file_sha256_hex(&entrypoint).unwrap();
        let expected = PublishedProviderIdentity {
            plugin_archive_sha256: "FIXTURE-ARCHIVE",
            plugin_files_sha256: &plugin_files,
            plugin_tree_sha256: &plugin_tree,
            server_entrypoint_sha256: &entrypoint_hash,
            derived_lock_sha256: &lock_hash,
            node_modules_tree_sha256: &node_modules_tree,
        };
        authenticate_published_provider_payload_against(&paths, &expected).unwrap();

        std::fs::write(&entrypoint, b"tamperd-server").unwrap();
        assert_eq!(std::fs::metadata(&entrypoint).unwrap().len(), 14);
        assert!(authenticate_published_provider_payload_against(&paths, &expected).is_err());
        std::fs::write(&entrypoint, b"trusted-server").unwrap();
        std::fs::write(&plugin_script, b"tamperd-plugin").unwrap();
        assert_eq!(std::fs::metadata(&plugin_script).unwrap().len(), 14);
        assert!(authenticate_published_provider_payload_against(&paths, &expected).is_err());
        assert!(paths.node_runtime_dir().exists());
        assert!(paths.youtube_po_provider_dir().exists());
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn provider_node_modules_tree_hash_is_content_addressed_and_metadata_independent() {
        let base = std::env::temp_dir().join(format!(
            "voxvulgi_provider_node_modules_hash_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let nested = base.join("package");
        std::fs::create_dir_all(&nested).unwrap();
        let first = nested.join("index.js");
        let second = base.join("package.json");
        std::fs::write(&first, b"trusted-byte").unwrap();
        std::fs::write(&second, b"{}").unwrap();
        let trusted = canonical_directory_tree_sha256_hex(&base).expect("tree hash");
        let original_modified = std::fs::metadata(&first).unwrap().modified().unwrap();
        std::fs::write(&first, b"tampered-byt").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&first)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(original_modified))
            .unwrap();
        assert_eq!(std::fs::metadata(&first).unwrap().len(), 12);
        assert!(
            authenticate_provider_node_modules_tree(&base, &trusted).is_err(),
            "same-size replacement with restored mtime must fail authoritative byte verification"
        );
        std::fs::write(&first, b"trusted-byte").unwrap();
        assert_eq!(canonical_directory_tree_sha256_hex(&base).unwrap(), trusted);
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn persisted_audit_receipt_can_be_stale_but_authoritative_verification_catches_tamper() {
        let base = std::env::temp_dir().join(format!(
            "voxvulgi_provider_receipt_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let server = base.join("server");
        let node_modules = server.join("node_modules");
        std::fs::create_dir_all(&node_modules).unwrap();
        let module = node_modules.join("index.js");
        std::fs::write(&module, b"trusted-byte").unwrap();
        let trusted = canonical_directory_tree_sha256_hex(&node_modules).unwrap();
        write_provider_node_modules_integrity_receipt(&server, &trusted).unwrap();
        let projected = read_provider_node_modules_integrity_receipt(&server).unwrap();

        let original_modified = std::fs::metadata(&module).unwrap().modified().unwrap();
        std::fs::write(&module, b"tampered-byt").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&module)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(original_modified))
            .unwrap();
        assert_eq!(
            read_provider_node_modules_integrity_receipt(&server)
                .unwrap()
                .tree_sha256_hex,
            projected.tree_sha256_hex,
            "the persisted receipt is history and does not self-authenticate later bytes"
        );
        assert!(authenticate_provider_node_modules_tree(&node_modules, &trusted).is_err());
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn forged_or_stale_receipt_never_becomes_current_process_launch_attestation() {
        let _guard = PROVIDER_INTEGRITY_TEST_LOCK.lock().unwrap();
        let base = std::env::temp_dir().join(format!(
            "voxvulgi_provider_forged_receipt_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let paths = AppPaths::new(base.clone());
        let server = paths.youtube_po_provider_server_dir();
        std::fs::create_dir_all(&server).unwrap();
        let expected = pinned_dependency_manifest::manifest()
            .youtube_po_provider
            .node_modules_tree_sha256_hex
            .clone();
        write_provider_node_modules_integrity_receipt(&server, &expected).unwrap();
        assert!(read_provider_node_modules_integrity_receipt(&server).is_some());
        assert!(provider_node_modules_process_attestation(&server).is_none());
        let status = youtube_po_provider_install_status(&paths);
        assert_eq!(status.node_modules_tree_sha256_hex, None);
        assert!(
            !status.installed,
            "receipt-only trust must never authorize execution"
        );
        assert!(
            ensure_youtube_po_provider(&paths).is_err(),
            "launch must force full verification instead of consuming the forged receipt"
        );
        assert!(provider_node_modules_process_attestation(&server).is_none());
        clear_provider_node_modules_process_attestation(&server);
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn forced_tamper_verification_clears_prior_process_attestation_and_receipt() {
        let _guard = PROVIDER_INTEGRITY_TEST_LOCK.lock().unwrap();
        let base = std::env::temp_dir().join(format!(
            "voxvulgi_provider_forced_tamper_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let paths = AppPaths::new(base.clone());
        let server = paths.youtube_po_provider_server_dir();
        let node_modules = server.join("node_modules");
        std::fs::create_dir_all(&node_modules).unwrap();
        std::fs::write(node_modules.join("index.js"), b"tampered-runtime").unwrap();
        let expected = pinned_dependency_manifest::manifest()
            .youtube_po_provider
            .node_modules_tree_sha256_hex
            .clone();
        // Seed the precondition directly: this represents a prior successful in-process
        // verification followed by a same-process disk mutation.
        attest_provider_node_modules_tree(&server, &expected).unwrap();
        assert!(provider_node_modules_process_attestation(&server).is_some());
        assert!(verify_youtube_po_provider_node_modules(&paths).is_err());
        assert!(provider_node_modules_process_attestation(&server).is_none());
        assert!(!provider_node_modules_integrity_receipt_path(&server).exists());
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn startup_verification_race_never_projects_forged_receipt_as_installed() {
        let _guard = PROVIDER_INTEGRITY_TEST_LOCK.lock().unwrap();
        let base = std::env::temp_dir().join(format!(
            "voxvulgi_provider_startup_race_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let paths = AppPaths::new(base.clone());
        let server = paths.youtube_po_provider_server_dir();
        std::fs::create_dir_all(&server).unwrap();
        let expected = pinned_dependency_manifest::manifest()
            .youtube_po_provider
            .node_modules_tree_sha256_hex
            .clone();
        write_provider_node_modules_integrity_receipt(&server, &expected).unwrap();
        clear_provider_node_modules_process_attestation(&server);
        provider_node_modules_integrity_verifying()
            .store(true, std::sync::atomic::Ordering::Release);
        let status = youtube_po_provider_install_status(&paths);
        let launch = ensure_youtube_po_provider(&paths);
        provider_node_modules_integrity_verifying()
            .store(false, std::sync::atomic::Ordering::Release);
        assert!(status.node_modules_integrity_verifying);
        assert_eq!(status.node_modules_tree_sha256_hex, None);
        assert!(!status.installed);
        assert!(
            launch.is_err(),
            "launch must remain held while verification is active"
        );
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn provider_node_modules_authentication_rejects_unbounded_depth() {
        let base = std::env::temp_dir().join(format!(
            "voxvulgi_provider_depth_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let mut nested = base.clone();
        for index in 0..33 {
            nested.push(format!("d{index}"));
        }
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("index.js"), b"bounded").unwrap();
        assert!(canonical_directory_tree_sha256_hex(&base).is_none());
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn exact_provider_node_modules_tree_hash_probe_when_available() {
        let Some(root) = std::env::var_os("VOXVULGI_PROVIDER_NODE_MODULES_DIR").map(PathBuf::from)
        else {
            return;
        };
        let hash = canonical_directory_tree_sha256_hex(&root).expect("canonical node_modules hash");
        eprintln!("provider_node_modules_tree_sha256={hash}");
    }

    #[test]
    fn compare_provider_node_modules_files_when_requested() {
        let (Some(baseline), Some(candidate)) = (
            std::env::var_os("VOXVULGI_PROVIDER_NODE_MODULES_BASELINE").map(PathBuf::from),
            std::env::var_os("VOXVULGI_PROVIDER_NODE_MODULES_CANDIDATE").map(PathBuf::from),
        ) else {
            return;
        };
        fn hashes(root: &Path) -> std::collections::BTreeMap<String, String> {
            let mut result = std::collections::BTreeMap::new();
            let mut stack = vec![root.to_path_buf()];
            while let Some(directory) = stack.pop() {
                for entry in std::fs::read_dir(&directory).expect("read provider tree") {
                    let entry = entry.expect("provider tree entry");
                    let path = entry.path();
                    let metadata = std::fs::symlink_metadata(&path).expect("provider metadata");
                    assert!(!metadata.file_type().is_symlink());
                    assert!(!provider_metadata_is_reparse_point(&metadata));
                    if metadata.is_dir() {
                        stack.push(path);
                    } else if metadata.is_file() {
                        let relative = path
                            .strip_prefix(root)
                            .expect("provider relative path")
                            .to_string_lossy()
                            .replace('\\', "/");
                        result.insert(
                            relative,
                            file_sha256_hex(&path).expect("provider file hash"),
                        );
                    } else {
                        panic!("unexpected provider tree entry: {}", path.display());
                    }
                }
            }
            result
        }
        let baseline_hashes = hashes(&baseline);
        let candidate_hashes = hashes(&candidate);
        let paths = baseline_hashes
            .keys()
            .chain(candidate_hashes.keys())
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        let differences = paths
            .into_iter()
            .filter_map(|path| {
                let before = baseline_hashes.get(&path);
                let after = candidate_hashes.get(&path);
                (before != after).then(|| (path, before.cloned(), after.cloned()))
            })
            .collect::<Vec<_>>();
        let baseline_only = differences
            .iter()
            .filter(|(_, before, after)| before.is_some() && after.is_none())
            .count();
        let candidate_only = differences
            .iter()
            .filter(|(_, before, after)| before.is_none() && after.is_some())
            .count();
        let changed = differences
            .iter()
            .filter(|(_, before, after)| before.is_some() && after.is_some())
            .count();
        eprintln!(
            "PROVIDER_NODE_MODULES_FILE_COUNTS baseline={} candidate={} baseline_only={baseline_only} candidate_only={candidate_only} changed={changed}",
            baseline_hashes.len(),
            candidate_hashes.len()
        );
        eprintln!(
            "PROVIDER_NODE_MODULES_FILE_DIFFERENCES={}",
            differences.len()
        );
        for (path, before, after) in differences {
            if before.is_none() || (before.is_some() && after.is_some()) {
                eprintln!(
                    "PROVIDER_NODE_MODULES_DIFF path={path} baseline={} candidate={}",
                    before.as_deref().unwrap_or("<missing>"),
                    after.as_deref().unwrap_or("<missing>")
                );
            }
        }
    }

    fn synthetic_provider_destination(paths: &AppPaths) -> ProviderInstalledIdentity {
        paths.ensure_dirs().unwrap();
        std::fs::create_dir_all(paths.node_runtime_dir()).unwrap();
        std::fs::create_dir_all(paths.youtube_po_provider_dir()).unwrap();
        std::fs::write(paths.node_runtime_dir().join("node_fixture"), b"node").unwrap();
        std::fs::write(
            paths.youtube_po_provider_dir().join("provider_fixture"),
            b"provider",
        )
        .unwrap();
        let node_root = canonical_provider_node_tree_sha256_hex(&paths.node_runtime_dir()).unwrap();
        let provider_root =
            canonical_provider_application_tree_sha256_hex(&paths.youtube_po_provider_dir())
                .unwrap();
        authenticate_complete_provider_trees_against(paths, &node_root, &provider_root).unwrap()
    }

    #[test]
    fn fresh_offline_adoption_is_atomic_idempotent_and_carrier_independent() {
        let base = std::env::temp_dir().join(format!(
            "voxvulgi_provider_adoption_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let paths = AppPaths::new(base.clone());
        let verified = synthetic_provider_destination(&paths);
        std::fs::write(
            provider_portable_attestation_path(&paths),
            br#"{"schema_version":1,"node_complete_tree_sha256":"FORGED","provider_complete_tree_sha256":"FORGED"}"#,
        )
        .unwrap();
        let independently_verified = authenticate_complete_provider_trees_against(
            &paths,
            &verified.node_tree_sha256,
            &verified.provider_tree_sha256,
        )
        .unwrap();
        commit_adopted_provider_identity(&paths, independently_verified.clone()).unwrap();
        commit_adopted_provider_identity(&paths, independently_verified.clone()).unwrap();
        let identity = load_provider_installed_identity(&paths).unwrap().unwrap();
        assert!(!identity.lineage_attempt_id.is_empty());
        assert!(identity.commit_nonce.len() >= 32);
        let conn = crate::db::open(&paths).unwrap();
        crate::db::migrate(&conn).unwrap();
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM provider_install_owner", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM provider_install_lineage WHERE phase='committed'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            1
        );
        drop(conn);
        std::fs::write(
            paths.youtube_po_provider_dir().join("tampered_extra"),
            b"tampered",
        )
        .unwrap();
        assert!(authenticate_complete_provider_trees_against(
            &paths,
            &verified.node_tree_sha256,
            &verified.provider_tree_sha256,
        )
        .is_err());
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn governed_install_replacement_preserves_pin_n_and_commits_pin_n_plus_one() {
        let base = std::env::temp_dir().join(format!(
            "voxvulgi_provider_generation_replacement_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let paths = AppPaths::new(base.clone());
        let mut old_identity = synthetic_provider_destination(&paths);
        old_identity.install_generation = "A".repeat(64);
        commit_adopted_provider_identity(&paths, old_identity.clone()).unwrap();
        let old_identity = load_provider_installed_identity(&paths).unwrap().unwrap();

        let attempt_id = uuid::Uuid::new_v4().to_string();
        let stage_root = paths
            .tools_dir()
            .join(format!("youtube_po_provider_stage_{attempt_id}"));
        let token = provider_test_ownership_token();
        claim_provider_install_owner(
            &paths,
            &attempt_id,
            &stage_root,
            &provider_ownership_token_digest(&token),
            &random_provider_authority_nonce(),
        )
        .unwrap();
        let replacement = prepare_governed_provider_replacement(&paths, &attempt_id)
            .unwrap()
            .expect("old managed finals must enter the governed archive");
        assert!(!paths.node_runtime_dir().exists());
        assert!(!paths.youtube_po_provider_dir().exists());

        let node_stage = stage_root.join("node");
        let provider_stage = stage_root.join("provider");
        std::fs::create_dir_all(&node_stage).unwrap();
        std::fs::create_dir_all(&provider_stage).unwrap();
        std::fs::write(node_stage.join("node_fixture"), b"node-n-plus-one").unwrap();
        std::fs::write(
            provider_stage.join("provider_fixture"),
            b"provider-n-plus-one",
        )
        .unwrap();
        write_provider_ownership_marker(&node_stage, &attempt_id, &token).unwrap();
        write_provider_ownership_marker(&provider_stage, &attempt_id, &token).unwrap();
        seal_provider_install_lineage(
            &paths,
            &attempt_id,
            &stage_root,
            &provider_ownership_token_digest(&token),
            &provider_directory_identity(&node_stage).unwrap(),
            &provider_directory_identity(&provider_stage).unwrap(),
            &canonical_provider_node_tree_sha256_hex(&node_stage).unwrap(),
            &canonical_provider_application_tree_sha256_hex(&provider_stage).unwrap(),
        )
        .unwrap();
        persist_provider_install_lineage(&paths, &attempt_id, &stage_root, "node_publish_intent")
            .unwrap();
        std::fs::rename(&node_stage, paths.node_runtime_dir()).unwrap();
        persist_provider_install_lineage(&paths, &attempt_id, &stage_root, "node_published")
            .unwrap();
        persist_provider_install_lineage(
            &paths,
            &attempt_id,
            &stage_root,
            "provider_publish_intent",
        )
        .unwrap();
        std::fs::rename(&provider_stage, paths.youtube_po_provider_dir()).unwrap();
        persist_provider_install_lineage(&paths, &attempt_id, &stage_root, "provider_published")
            .unwrap();
        commit_provider_installed_identity(&paths, &attempt_id, &stage_root).unwrap();
        release_provider_install_owner(&paths, &attempt_id).unwrap();
        replacement.preserve_archive();

        let current = load_provider_installed_identity(&paths).unwrap().unwrap();
        assert_eq!(current.install_generation, provider_install_generation());
        assert_ne!(current.lineage_attempt_id, old_identity.lineage_attempt_id);
        let archive_root = managed_provider_replacement_archive_root(&paths, &attempt_id).unwrap();
        let archived_node = archive_root.join("node");
        let archived_provider = archive_root.join("provider");
        assert_eq!(
            provider_directory_identity(&archived_node).unwrap(),
            old_identity.node_directory_identity
        );
        assert_eq!(
            provider_directory_identity(&archived_provider).unwrap(),
            old_identity.provider_directory_identity
        );
        assert_eq!(
            canonical_provider_node_tree_sha256_hex(&archived_node).unwrap(),
            old_identity.node_tree_sha256
        );
        assert_eq!(
            canonical_provider_application_tree_sha256_hex(&archived_provider).unwrap(),
            old_identity.provider_tree_sha256
        );
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn interrupted_governed_replacement_restores_authenticated_pin_n_without_data_loss() {
        let base = std::env::temp_dir().join(format!(
            "vv_grr_{}_{}",
            std::process::id(),
            &uuid::Uuid::new_v4().simple().to_string()[..8]
        ));
        let paths = AppPaths::new(base.clone());
        let mut old_identity = synthetic_provider_destination(&paths);
        old_identity.install_generation = "A".repeat(64);
        commit_adopted_provider_identity(&paths, old_identity.clone()).unwrap();
        let old_identity = load_provider_installed_identity(&paths).unwrap().unwrap();

        let attempt_id = uuid::Uuid::new_v4().to_string();
        let stage_root = paths
            .tools_dir()
            .join(format!("youtube_po_provider_stage_{attempt_id}"));
        let token = provider_test_ownership_token();
        let token_digest = provider_ownership_token_digest(&token);
        claim_provider_install_owner(
            &paths,
            &attempt_id,
            &stage_root,
            &token_digest,
            &random_provider_authority_nonce(),
        )
        .unwrap();
        let replacement = prepare_governed_provider_replacement(&paths, &attempt_id)
            .unwrap()
            .expect("old managed finals must enter the governed archive");

        let node_stage = stage_root.join("node");
        let provider_stage = stage_root.join("provider");
        std::fs::create_dir_all(&node_stage).unwrap();
        std::fs::create_dir_all(&provider_stage).unwrap();
        std::fs::write(node_stage.join("node_fixture"), b"node-n-plus-one").unwrap();
        std::fs::write(
            provider_stage.join("provider_fixture"),
            b"provider-n-plus-one",
        )
        .unwrap();
        write_provider_ownership_marker(&node_stage, &attempt_id, &token).unwrap();
        write_provider_ownership_marker(&provider_stage, &attempt_id, &token).unwrap();
        seal_provider_install_lineage(
            &paths,
            &attempt_id,
            &stage_root,
            &token_digest,
            &provider_directory_identity(&node_stage).unwrap(),
            &provider_directory_identity(&provider_stage).unwrap(),
            &canonical_provider_node_tree_sha256_hex(&node_stage).unwrap(),
            &canonical_provider_application_tree_sha256_hex(&provider_stage).unwrap(),
        )
        .unwrap();
        persist_provider_install_lineage(&paths, &attempt_id, &stage_root, "prepared").unwrap();
        persist_provider_install_lineage(&paths, &attempt_id, &stage_root, "node_publish_intent")
            .unwrap();
        std::fs::rename(&node_stage, paths.node_runtime_dir()).unwrap();
        persist_provider_install_lineage(&paths, &attempt_id, &stage_root, "node_published")
            .unwrap();

        // Simulate process death: the in-process rollback guard never runs. Durable recovery
        // must roll the new attempt back and restore only the authenticated archived generation.
        std::mem::forget(replacement);
        reconcile_interrupted_provider_install_with_checks(
            &paths,
            |_| Ok(()),
            |_| Ok(()),
            |_, _| Ok(()),
            |_| false,
        )
        .unwrap();

        let restored = load_provider_installed_identity(&paths).unwrap().unwrap();
        assert_eq!(restored.lineage_attempt_id, old_identity.lineage_attempt_id);
        assert_eq!(restored.commit_nonce, old_identity.commit_nonce);
        assert_eq!(restored.install_generation, old_identity.install_generation);
        assert_eq!(
            restored.node_directory_identity,
            old_identity.node_directory_identity
        );
        assert_eq!(
            restored.provider_directory_identity,
            old_identity.provider_directory_identity
        );
        assert_eq!(restored.node_tree_sha256, old_identity.node_tree_sha256);
        assert_eq!(
            restored.provider_tree_sha256,
            old_identity.provider_tree_sha256
        );
        authenticate_stored_managed_provider_identity_at(
            &paths,
            &restored,
            &paths.node_runtime_dir(),
            &paths.youtube_po_provider_dir(),
        )
        .unwrap();
        assert!(!stage_root.exists());
        assert!(
            !managed_provider_replacement_archive_root(&paths, &attempt_id)
                .unwrap()
                .exists()
        );
        let conn = crate::db::open(&paths).unwrap();
        crate::db::migrate(&conn).unwrap();
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM provider_install_owner WHERE attempt_id=?1",
                rusqlite::params![attempt_id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            0
        );
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn dead_prepared_owner_before_replacement_preserves_authenticated_installed_generation() {
        let base = std::env::temp_dir().join(format!(
            "vv_pre_replace_{}_{}",
            std::process::id(),
            &uuid::Uuid::new_v4().simple().to_string()[..8]
        ));
        let paths = AppPaths::new(base.clone());
        let mut old_identity = synthetic_provider_destination(&paths);
        old_identity.install_generation = "A".repeat(64);
        commit_adopted_provider_identity(&paths, old_identity).unwrap();
        let installed_before = load_provider_installed_identity(&paths).unwrap().unwrap();

        let attempt_id = uuid::Uuid::new_v4().to_string();
        let stage_root = paths
            .tools_dir()
            .join(format!("youtube_po_provider_stage_{attempt_id}"));
        let token = provider_test_ownership_token();
        claim_provider_install_owner(
            &paths,
            &attempt_id,
            &stage_root,
            &provider_ownership_token_digest(&token),
            &random_provider_authority_nonce(),
        )
        .unwrap();

        reconcile_interrupted_provider_install_with_checks(
            &paths,
            |_| panic!("the previously installed Node tree is authenticated by its identity"),
            |_| panic!("the previously installed provider tree is authenticated by its identity"),
            |_, _| panic!("the replacement attempt was never committed"),
            |_| false,
        )
        .unwrap();

        let installed_after = load_provider_installed_identity(&paths).unwrap().unwrap();
        assert_eq!(
            installed_after.lineage_attempt_id,
            installed_before.lineage_attempt_id
        );
        assert_eq!(installed_after.commit_nonce, installed_before.commit_nonce);
        authenticate_stored_managed_provider_identity_at(
            &paths,
            &installed_after,
            &paths.node_runtime_dir(),
            &paths.youtube_po_provider_dir(),
        )
        .unwrap();
        assert!(!stage_root.exists());
        let conn = crate::db::open(&paths).unwrap();
        crate::db::migrate(&conn).unwrap();
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM provider_install_owner WHERE attempt_id=?1",
                rusqlite::params![attempt_id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            0
        );
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM provider_install_lineage WHERE attempt_id=?1",
                rusqlite::params![attempt_id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            0
        );
        let _ = std::fs::remove_dir_all(base);
    }

    fn seed_v47_unbound_provider_identity(
        paths: &AppPaths,
        identity: &ProviderInstalledIdentity,
        node_tree_override: Option<&str>,
    ) {
        paths.ensure_dirs().unwrap();
        let conn = rusqlite::Connection::open(paths.db_dir().join("app.sqlite")).unwrap();
        conn.execute_batch(
            "CREATE TABLE meta(key TEXT PRIMARY KEY,value TEXT NOT NULL);
             INSERT INTO meta(key,value) VALUES('schema_version','47');
             CREATE TABLE provider_install_lineage(
               attempt_id TEXT PRIMARY KEY,stage_root TEXT NOT NULL,phase TEXT NOT NULL,
               updated_at_ms INTEGER NOT NULL,ownership_token_digest TEXT NOT NULL DEFAULT '',
               node_directory_identity TEXT NOT NULL DEFAULT '',provider_directory_identity TEXT NOT NULL DEFAULT '',
               node_tree_sha256 TEXT NOT NULL DEFAULT '',provider_tree_sha256 TEXT NOT NULL DEFAULT ''
             );
             CREATE TABLE provider_install_owner(
               singleton INTEGER PRIMARY KEY,attempt_id TEXT NOT NULL UNIQUE,
               acquired_at_ms INTEGER NOT NULL,updated_at_ms INTEGER NOT NULL,
               owner_pid INTEGER NOT NULL DEFAULT 0,owner_process_identity TEXT NOT NULL DEFAULT ''
             );
             CREATE TABLE provider_installed_identity(
               singleton INTEGER PRIMARY KEY,install_generation TEXT NOT NULL,
               node_directory_identity TEXT NOT NULL,provider_directory_identity TEXT NOT NULL,
               node_tree_sha256 TEXT NOT NULL,provider_tree_sha256 TEXT NOT NULL,
               committed_at_ms INTEGER NOT NULL
             );
             PRAGMA user_version=47;",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO provider_installed_identity(
               singleton,install_generation,node_directory_identity,provider_directory_identity,
               node_tree_sha256,provider_tree_sha256,committed_at_ms
             ) VALUES(1,?1,?2,?3,?4,?5,1)",
            rusqlite::params![
                identity.install_generation,
                identity.node_directory_identity,
                identity.provider_directory_identity,
                node_tree_override.unwrap_or(&identity.node_tree_sha256),
                identity.provider_tree_sha256,
            ],
        )
        .unwrap();
    }

    #[test]
    fn v47_unbound_identity_binds_only_to_exact_authenticated_destination() {
        for mismatch in [false, true] {
            let base = std::env::temp_dir().join(format!(
                "voxvulgi_provider_v47_adoption_{}_{}_{}",
                mismatch,
                std::process::id(),
                uuid::Uuid::new_v4()
            ));
            let paths = AppPaths::new(base.clone());
            let verified = synthetic_provider_destination(&paths);
            seed_v47_unbound_provider_identity(
                &paths,
                &verified,
                mismatch.then_some("BAD_LEGACY_ROOT"),
            );
            assert!(
                authenticate_authoritative_installed_provider_identity(&paths, None).is_err(),
                "a parseable v47 row must never be accepted before transactional binding"
            );
            let result = commit_adopted_provider_identity(&paths, verified);
            assert_eq!(result.is_err(), mismatch, "adoption result: {result:?}");
            let identity = load_provider_installed_identity(&paths).unwrap().unwrap();
            assert_eq!(identity.lineage_attempt_id.is_empty(), mismatch);
            assert_eq!(identity.commit_nonce.is_empty(), mismatch);
            if !mismatch {
                require_exact_committed_provider_identity_lineage(&paths, &identity).unwrap();
            }
            let _ = std::fs::remove_dir_all(base);
        }
    }

    fn seed_provider_published_lineage(
        paths: &AppPaths,
        verified: &ProviderInstalledIdentity,
    ) -> (String, PathBuf) {
        let attempt_id = uuid::Uuid::new_v4().to_string();
        let stage_root = paths
            .tools_dir()
            .join(format!("youtube_po_provider_stage_{attempt_id}"));
        let token = provider_test_ownership_token();
        let token_digest = provider_ownership_token_digest(&token);
        let nonce = random_provider_authority_nonce();
        claim_provider_install_owner(paths, &attempt_id, &stage_root, &token_digest, &nonce)
            .unwrap();
        seal_provider_install_lineage(
            paths,
            &attempt_id,
            &stage_root,
            &token_digest,
            &verified.node_directory_identity,
            &verified.provider_directory_identity,
            &verified.node_tree_sha256,
            &verified.provider_tree_sha256,
        )
        .unwrap();
        persist_provider_install_lineage(paths, &attempt_id, &stage_root, "prepared").unwrap();
        for phase in [
            "node_publish_intent",
            "node_published",
            "provider_publish_intent",
            "provider_published",
        ] {
            persist_provider_install_lineage(paths, &attempt_id, &stage_root, phase).unwrap();
        }
        (attempt_id, stage_root)
    }

    #[test]
    fn postcommit_receipt_failure_retains_finals_and_authoritative_identity() {
        let base = std::env::temp_dir().join(format!(
            "voxvulgi_provider_receipt_commit_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let paths = AppPaths::new(base.clone());
        let verified = synthetic_provider_destination(&paths);
        let (attempt_id, stage_root) = seed_provider_published_lineage(&paths, &verified);
        let outcome = commit_provider_installed_identity_with_receipt(
            &paths,
            &attempt_id,
            &stage_root,
            |_paths, _attempt, _root| {
                Err(EngineError::InstallFailed(
                    "injected postcommit receipt failure".to_string(),
                ))
            },
        )
        .unwrap();
        assert_eq!(
            outcome,
            ProviderIdentityCommitOutcome {
                committed: true,
                receipt_written: false,
            }
        );
        assert!(paths.node_runtime_dir().exists());
        assert!(paths.youtube_po_provider_dir().exists());
        let identity = load_provider_installed_identity(&paths).unwrap().unwrap();
        assert_eq!(identity.lineage_attempt_id, attempt_id);
        let conn = crate::db::open(&paths).unwrap();
        crate::db::migrate(&conn).unwrap();
        assert_eq!(
            conn.query_row(
                "SELECT phase FROM provider_install_lineage WHERE attempt_id=?1",
                [&attempt_id],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
            "committed"
        );
        drop(conn);
        release_provider_install_owner(&paths, &attempt_id).unwrap();
        std::fs::write(provider_install_attempt_receipt_path(&paths), b"malformed").unwrap();
        reconcile_interrupted_provider_install(&paths).unwrap();
        assert!(!provider_install_attempt_receipt_path(&paths).exists());
        assert!(load_provider_installed_identity(&paths).unwrap().is_some());
        assert!(paths.node_runtime_dir().exists());
        assert!(paths.youtube_po_provider_dir().exists());
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn prepublication_failures_release_durable_owner_for_same_process_retry() {
        let base = std::env::temp_dir().join(format!(
            "voxvulgi_provider_prepublication_retry_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let paths = AppPaths::new(base.clone());
        paths.ensure_dirs().unwrap();
        for boundary in ["download", "extract", "npm", "audit", "seal"] {
            let attempt_id = uuid::Uuid::new_v4().to_string();
            let stage_root = paths
                .tools_dir()
                .join(format!("youtube_po_provider_stage_{attempt_id}"));
            let token = provider_test_ownership_token();
            let nonce = random_provider_authority_nonce();
            claim_provider_install_owner(
                &paths,
                &attempt_id,
                &stage_root,
                &provider_ownership_token_digest(&token),
                &nonce,
            )
            .unwrap();
            std::fs::create_dir_all(&stage_root).unwrap();
            std::fs::write(stage_root.join(format!("{boundary}_partial")), boundary).unwrap();
            drop(ProviderInstallOperationGuard::new(&paths, &attempt_id));
            let conn = crate::db::open(&paths).unwrap();
            crate::db::migrate(&conn).unwrap();
            assert_eq!(
                conn.query_row("SELECT COUNT(*) FROM provider_install_owner", [], |row| row
                    .get::<_, i64>(0))
                    .unwrap(),
                0,
                "owner leaked after {boundary}"
            );
            assert_eq!(
                conn.query_row("SELECT COUNT(*) FROM provider_install_lineage", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
                0,
                "lineage leaked after {boundary}"
            );
            drop(conn);
            assert!(!stage_root.exists(), "stage leaked after {boundary}");
        }
        let retry_id = uuid::Uuid::new_v4().to_string();
        let retry_stage = paths
            .tools_dir()
            .join(format!("youtube_po_provider_stage_{retry_id}"));
        let retry_token = provider_test_ownership_token();
        claim_provider_install_owner(
            &paths,
            &retry_id,
            &retry_stage,
            &provider_ownership_token_digest(&retry_token),
            &random_provider_authority_nonce(),
        )
        .unwrap();
        abort_prepublication_provider_install(&paths, &retry_id).unwrap();
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn every_publication_persistence_failure_releases_owner_for_same_process_retry() {
        for boundary in [
            "node_publish_intent",
            "node_published",
            "provider_publish_intent",
            "provider_published",
        ] {
            let base = std::env::temp_dir().join(format!(
                "vv_cb_{}_{}",
                std::process::id(),
                &uuid::Uuid::new_v4().simple().to_string()[..8]
            ));
            let paths = AppPaths::new(base.clone());
            let (attempt_id, _token, stage_root) = seed_provider_recovery_phase(&paths, "prepared");
            let node_stage = stage_root.join("node");
            let provider_stage = stage_root.join("provider");
            let final_node = paths.node_runtime_dir();
            let final_provider = paths.youtube_po_provider_dir();
            let mut operation_guard = ProviderInstallOperationGuard::new(&paths, &attempt_id);

            if boundary == "node_publish_intent" {
                let result = enter_durable_provider_publication(&mut operation_guard, || {
                    Err(EngineError::InstallFailed(
                        "injected node_publish_intent persistence failure".to_string(),
                    ))
                });
                assert!(result.is_err());
                drop(operation_guard);
            } else {
                enter_durable_provider_publication(&mut operation_guard, || {
                    persist_provider_install_lineage(
                        &paths,
                        &attempt_id,
                        &stage_root,
                        "node_publish_intent",
                    )
                })
                .unwrap();
                let result = publish_provider_pair_with_checks(
                    &node_stage,
                    &provider_stage,
                    &final_node,
                    &final_provider,
                    || Ok(()),
                    || {
                        if boundary == "node_published" {
                            return Err(EngineError::InstallFailed(
                                "injected node_published persistence failure".to_string(),
                            ));
                        }
                        persist_provider_install_lineage(
                            &paths,
                            &attempt_id,
                            &stage_root,
                            "node_published",
                        )?;
                        if boundary == "provider_publish_intent" {
                            return Err(EngineError::InstallFailed(
                                "injected provider_publish_intent persistence failure".to_string(),
                            ));
                        }
                        persist_provider_install_lineage(
                            &paths,
                            &attempt_id,
                            &stage_root,
                            "provider_publish_intent",
                        )
                    },
                    || {
                        Err(EngineError::InstallFailed(
                            "injected provider_published persistence failure".to_string(),
                        ))
                    },
                );
                assert!(result.is_err());
                assert!(!final_node.exists() && !final_provider.exists());
                abort_owned_provider_install_after_complete_rollback(&paths, &attempt_id).unwrap();
                drop(operation_guard);
            }

            let conn = crate::db::open(&paths).unwrap();
            crate::db::migrate(&conn).unwrap();
            let owner_count: i64 = conn
                .query_row("SELECT COUNT(*) FROM provider_install_owner", [], |row| {
                    row.get(0)
                })
                .unwrap();
            let lineage_count: i64 = conn
                .query_row("SELECT COUNT(*) FROM provider_install_lineage", [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(owner_count, 0, "owner leaked at {boundary}");
            assert_eq!(lineage_count, 0, "lineage leaked at {boundary}");
            drop(conn);

            let retry_id = uuid::Uuid::new_v4().to_string();
            let retry_stage = paths
                .tools_dir()
                .join(format!("youtube_po_provider_stage_{retry_id}"));
            claim_provider_install_owner(
                &paths,
                &retry_id,
                &retry_stage,
                &provider_ownership_token_digest(&provider_test_ownership_token()),
                &random_provider_authority_nonce(),
            )
            .unwrap();
            abort_prepublication_provider_install(&paths, &retry_id).unwrap();
            let _ = std::fs::remove_dir_all(base);
        }
    }

    #[cfg(windows)]
    #[test]
    fn locked_orphan_receipt_never_blocks_and_is_removed_after_restart_retry() {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Foundation::{CloseHandle, GENERIC_READ, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::Storage::FileSystem::{CreateFileW, OPEN_EXISTING};

        let base = std::env::temp_dir().join(format!(
            "voxvulgi_provider_locked_receipt_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let paths = AppPaths::new(base.clone());
        let verified = synthetic_provider_destination(&paths);
        commit_adopted_provider_identity(&paths, verified).unwrap();
        let receipt = provider_install_attempt_receipt_path(&paths);
        std::fs::write(&receipt, b"orphan").unwrap();
        let wide = receipt
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                GENERIC_READ,
                0,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        assert_ne!(handle, INVALID_HANDLE_VALUE);
        reconcile_interrupted_provider_install(&paths).unwrap();
        assert!(
            receipt.exists(),
            "locked audit receipt should be left for retry"
        );
        unsafe {
            let _ = CloseHandle(handle);
        }
        reconcile_interrupted_provider_install(&paths).unwrap();
        assert!(
            !receipt.exists(),
            "fresh retry should remove orphan receipt"
        );
        assert!(load_provider_installed_identity(&paths).unwrap().is_some());
        let _ = std::fs::remove_dir_all(base);
    }

    #[cfg(windows)]
    #[test]
    fn derive_fresh_pinned_provider_complete_roots_when_requested() {
        if std::env::var_os("VOXVULGI_DERIVE_PROVIDER_COMPLETE_ROOTS").is_none() {
            return;
        }
        let base = std::env::temp_dir().join(format!(
            "vv_roots_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));
        let paths = AppPaths::new(base.clone());
        let status = install_youtube_po_provider(&paths).expect("fresh pinned provider install");
        assert!(status.installed && status.security_audit_passed);
        let node_root = canonical_provider_node_tree_sha256_hex(&paths.node_runtime_dir())
            .expect("complete Node root");
        let provider_root =
            canonical_provider_application_tree_sha256_hex(&paths.youtube_po_provider_dir())
                .expect("complete provider root");
        eprintln!("PINNED_PROVIDER_NODE_COMPLETE_ROOT={node_root}");
        eprintln!("PINNED_PROVIDER_APPLICATION_COMPLETE_ROOT={provider_root}");
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn stale_node_published_receipt_recovers_only_attempt_owned_payload() {
        let base = std::env::temp_dir().join(format!(
            "voxvulgi_provider_recovery_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let paths = AppPaths::new(base.clone());
        paths.ensure_dirs().unwrap();
        let attempt_id = uuid::Uuid::new_v4().to_string();
        let ownership_token = provider_test_ownership_token();
        let commit_nonce = random_provider_authority_nonce();
        let stage_root = paths
            .tools_dir()
            .join(format!("youtube_po_provider_stage_{attempt_id}"));
        let node_stage = stage_root.join("node");
        let provider_stage = stage_root.join("provider");
        std::fs::create_dir_all(&node_stage).unwrap();
        std::fs::create_dir_all(&provider_stage).unwrap();
        std::fs::write(node_stage.join("node_fixture"), b"node").unwrap();
        std::fs::write(provider_stage.join("provider_fixture"), b"provider").unwrap();
        write_provider_ownership_marker(&node_stage, &attempt_id, &ownership_token).unwrap();
        write_provider_ownership_marker(&provider_stage, &attempt_id, &ownership_token).unwrap();
        claim_provider_install_owner(
            &paths,
            &attempt_id,
            &stage_root,
            &provider_ownership_token_digest(&ownership_token),
            &commit_nonce,
        )
        .unwrap();
        seal_provider_install_lineage(
            &paths,
            &attempt_id,
            &stage_root,
            &provider_ownership_token_digest(&ownership_token),
            &provider_directory_identity(&node_stage).unwrap(),
            &provider_directory_identity(&provider_stage).unwrap(),
            &canonical_provider_node_tree_sha256_hex(&node_stage).unwrap(),
            &canonical_provider_application_tree_sha256_hex(&provider_stage).unwrap(),
        )
        .unwrap();
        persist_provider_install_lineage(&paths, &attempt_id, &stage_root, "prepared").unwrap();
        persist_provider_install_lineage(&paths, &attempt_id, &stage_root, "node_publish_intent")
            .unwrap();
        std::fs::create_dir_all(paths.node_runtime_dir().parent().unwrap()).unwrap();
        std::fs::rename(&node_stage, paths.node_runtime_dir()).unwrap();
        persist_provider_install_lineage(&paths, &attempt_id, &stage_root, "node_published")
            .unwrap();

        reconcile_interrupted_provider_install_with_checks(
            &paths,
            |_| Ok(()),
            |_| Ok(()),
            |_, _| Ok(()),
            |_| false,
        )
        .unwrap();
        assert!(!paths.node_runtime_dir().exists());
        assert!(!paths.youtube_po_provider_dir().exists());
        assert!(!stage_root.exists());
        assert!(!provider_install_attempt_receipt_path(&paths).exists());
        let conn = crate::db::open(&paths).unwrap();
        crate::db::migrate(&conn).unwrap();
        let owners: i64 = conn
            .query_row("SELECT COUNT(*) FROM provider_install_owner", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(owners, 0, "stale owner must be released by recovery");
        drop(conn);
        let _ = std::fs::remove_dir_all(base);
    }

    fn seed_provider_recovery_phase(paths: &AppPaths, phase: &str) -> (String, String, PathBuf) {
        paths.ensure_dirs().unwrap();
        let attempt_id = uuid::Uuid::new_v4().to_string();
        let ownership_token = provider_test_ownership_token();
        let commit_nonce = random_provider_authority_nonce();
        let stage_root = paths
            .tools_dir()
            .join(format!("youtube_po_provider_stage_{attempt_id}"));
        let node_stage = stage_root.join("node");
        let provider_stage = stage_root.join("provider");
        std::fs::create_dir_all(&node_stage).unwrap();
        std::fs::create_dir_all(&provider_stage).unwrap();
        std::fs::write(node_stage.join("node_fixture"), b"node").unwrap();
        std::fs::write(provider_stage.join("provider_fixture"), b"provider").unwrap();
        write_provider_ownership_marker(&node_stage, &attempt_id, &ownership_token).unwrap();
        write_provider_ownership_marker(&provider_stage, &attempt_id, &ownership_token).unwrap();
        claim_provider_install_owner(
            paths,
            &attempt_id,
            &stage_root,
            &provider_ownership_token_digest(&ownership_token),
            &commit_nonce,
        )
        .unwrap();
        seal_provider_install_lineage(
            paths,
            &attempt_id,
            &stage_root,
            &provider_ownership_token_digest(&ownership_token),
            &provider_directory_identity(&node_stage).unwrap(),
            &provider_directory_identity(&provider_stage).unwrap(),
            &canonical_provider_node_tree_sha256_hex(&node_stage).unwrap(),
            &canonical_provider_application_tree_sha256_hex(&provider_stage).unwrap(),
        )
        .unwrap();

        if phase != "prepared" {
            persist_provider_install_lineage(paths, &attempt_id, &stage_root, "prepared").unwrap();
            persist_provider_install_lineage(
                paths,
                &attempt_id,
                &stage_root,
                "node_publish_intent",
            )
            .unwrap();
            std::fs::create_dir_all(paths.node_runtime_dir().parent().unwrap()).unwrap();
            std::fs::rename(&node_stage, paths.node_runtime_dir()).unwrap();
            if phase != "node_publish_intent" {
                persist_provider_install_lineage(paths, &attempt_id, &stage_root, "node_published")
                    .unwrap();
                if phase != "node_published" {
                    persist_provider_install_lineage(
                        paths,
                        &attempt_id,
                        &stage_root,
                        "provider_publish_intent",
                    )
                    .unwrap();
                    std::fs::rename(&provider_stage, paths.youtube_po_provider_dir()).unwrap();
                    if phase != "provider_publish_intent" {
                        persist_provider_install_lineage(
                            paths,
                            &attempt_id,
                            &stage_root,
                            "provider_published",
                        )
                        .unwrap();
                        if phase == "committed" {
                            commit_provider_installed_identity(paths, &attempt_id, &stage_root)
                                .unwrap();
                        }
                    }
                }
            }
        }
        (attempt_id, ownership_token, stage_root)
    }

    #[test]
    fn provider_directory_identity_survives_rename_but_rejects_identical_copy() {
        let base = tempfile::tempdir().unwrap();
        let staged = base.path().join("staged");
        let published = base.path().join("published");
        let copied = base.path().join("copied");
        std::fs::create_dir_all(&staged).unwrap();
        std::fs::write(staged.join("payload"), b"identical").unwrap();
        let staged_identity = provider_directory_identity(&staged).unwrap();
        std::fs::rename(&staged, &published).unwrap();
        assert_eq!(
            provider_directory_identity(&published).unwrap(),
            staged_identity,
            "same-parent publication must preserve the staged filesystem object identity"
        );
        std::fs::create_dir_all(&copied).unwrap();
        std::fs::copy(published.join("payload"), copied.join("payload")).unwrap();
        assert_ne!(
            provider_directory_identity(&copied).unwrap(),
            staged_identity,
            "an identical-byte copied directory must not inherit destructive recovery authority"
        );
    }

    #[test]
    fn copied_valid_marker_and_bytes_in_authorized_phase_preserve_both_objects() {
        let base = std::env::temp_dir().join(format!(
            "voxvulgi_provider_copied_identity_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let paths = AppPaths::new(base.clone());
        let (_attempt_id, _ownership_token, stage_root) =
            seed_provider_recovery_phase(&paths, "node_published");
        let published = paths.node_runtime_dir();
        let original = paths.tools_dir().join("original_node_object");
        std::fs::rename(&published, &original).unwrap();
        std::fs::create_dir_all(&published).unwrap();
        for entry in std::fs::read_dir(&original).unwrap() {
            let entry = entry.unwrap();
            assert!(entry.file_type().unwrap().is_file());
            std::fs::copy(entry.path(), published.join(entry.file_name())).unwrap();
        }
        assert_eq!(
            canonical_provider_node_tree_sha256_hex(&published),
            canonical_provider_node_tree_sha256_hex(&original)
        );
        assert!(reconcile_interrupted_provider_install_with_checks(
            &paths,
            |_| Ok(()),
            |_| Ok(()),
            |_, _| Ok(()),
            |_| false,
        )
        .is_err());
        assert!(
            published.exists(),
            "copied final must be preserved on ambiguity"
        );
        assert!(
            original.exists(),
            "original sealed object must remain preserved"
        );
        assert!(
            stage_root.exists(),
            "recovery evidence must remain available"
        );
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn live_owner_identity_refuses_recovery_before_any_cleanup() {
        let base = std::env::temp_dir().join(format!(
            "voxvulgi_provider_live_owner_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let paths = AppPaths::new(base.clone());
        let (_attempt_id, _ownership_token, stage_root) =
            seed_provider_recovery_phase(&paths, "prepared");
        assert!(reconcile_interrupted_provider_install(&paths).is_err());
        assert!(stage_root.exists());
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn provider_published_lineage_is_never_verifiable_or_launchable() {
        let base = std::env::temp_dir().join(format!(
            "voxvulgi_provider_uncommitted_launch_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let paths = AppPaths::new(base.clone());
        let (_attempt_id, _ownership_token, _stage_root) =
            seed_provider_recovery_phase(&paths, "provider_published");
        let conn = crate::db::open(&paths).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn.execute(
            "UPDATE provider_install_owner SET owner_pid=0,owner_process_identity='dead-process' WHERE singleton=1",
            [],
        )
        .unwrap();
        drop(conn);
        assert!(verify_youtube_po_provider_node_modules(&paths).is_err());
        assert!(ensure_youtube_po_provider(&paths).is_err());
        assert!(
            provider_node_modules_process_attestation(&paths.youtube_po_provider_server_dir())
                .is_none()
        );
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn complete_provider_tree_roots_detect_unlisted_same_size_tamper() {
        let base = tempfile::tempdir().unwrap();
        let node = base.path().join("node");
        let provider = base.path().join("provider");
        std::fs::create_dir_all(node.join("node_modules/npm/lib")).unwrap();
        std::fs::create_dir_all(provider.join("server/build")).unwrap();
        let npm_impl = node.join("node_modules/npm/lib/cli.js");
        let provider_extra = provider.join("server/build/worker.js");
        std::fs::write(node.join("node.exe"), b"node").unwrap();
        std::fs::write(&npm_impl, b"trusted-npm").unwrap();
        std::fs::write(provider.join("server/build/main.js"), b"main").unwrap();
        std::fs::write(&provider_extra, b"trusted-app").unwrap();
        let node_root = canonical_provider_node_tree_sha256_hex(&node).unwrap();
        let provider_root = canonical_provider_application_tree_sha256_hex(&provider).unwrap();

        let npm_mtime = std::fs::metadata(&npm_impl).unwrap().modified().unwrap();
        std::fs::write(&npm_impl, b"tamperd-npm").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&npm_impl)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(npm_mtime))
            .unwrap();
        assert_ne!(
            canonical_provider_node_tree_sha256_hex(&node).unwrap(),
            node_root
        );

        let app_mtime = std::fs::metadata(&provider_extra)
            .unwrap()
            .modified()
            .unwrap();
        std::fs::write(&provider_extra, b"tamperd-app").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&provider_extra)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(app_mtime))
            .unwrap();
        assert_ne!(
            canonical_provider_application_tree_sha256_hex(&provider).unwrap(),
            provider_root
        );
        assert_eq!(
            PROVIDER_NODE_TREE_EXCLUSIONS,
            [".voxvulgi_provider_install_attempt"]
        );
        assert_eq!(
            PROVIDER_APPLICATION_TREE_EXCLUSIONS,
            [
                ".voxvulgi_provider_install_attempt",
                "server/.node_modules_integrity.json"
            ]
        );
    }

    #[cfg(windows)]
    #[test]
    fn provider_interprocess_mutex_is_global_and_tree_rejects_reparse_escape() {
        let base = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(base.path().to_path_buf());
        paths.ensure_dirs().unwrap();
        assert!(youtube_po_provider_install_interprocess_lock_name(&paths)
            .starts_with("Global\\VoxVulgiYoutubePoProviderInstall-"));

        let tree = base.path().join("tree");
        let outside = base.path().join("outside");
        std::fs::create_dir_all(&tree).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("payload"), b"outside").unwrap();
        match std::os::windows::fs::symlink_dir(&outside, tree.join("junction_like")) {
            Ok(()) => assert!(canonical_provider_application_tree_sha256_hex(&tree).is_none()),
            Err(error) => eprintln!("reparse creation unavailable in this test context: {error}"),
        }
    }

    #[test]
    fn provider_recovery_phase_matrix_only_rolls_back_phase_authorized_valid_payloads() {
        for (phase, expected_node_checks, expected_provider_checks, expected_committed_checks) in [
            ("prepared", 0, 0, 0),
            ("node_publish_intent", 1, 0, 0),
            ("node_published", 1, 0, 0),
            ("provider_publish_intent", 1, 1, 0),
            ("provider_published", 1, 1, 0),
            ("committed", 0, 0, 1),
        ] {
            let base = std::env::temp_dir().join(format!(
                "vv_ppm_{}_{}",
                std::process::id(),
                uuid::Uuid::new_v4()
            ));
            let paths = AppPaths::new(base.clone());
            let (_attempt_id, _ownership_token, stage_root) =
                seed_provider_recovery_phase(&paths, phase);
            let node_checks = std::cell::Cell::new(0);
            let provider_checks = std::cell::Cell::new(0);
            let committed_checks = std::cell::Cell::new(0);
            reconcile_interrupted_provider_install_with_checks(
                &paths,
                |_| {
                    node_checks.set(node_checks.get() + 1);
                    Ok(())
                },
                |_| {
                    provider_checks.set(provider_checks.get() + 1);
                    Ok(())
                },
                |_, _| {
                    committed_checks.set(committed_checks.get() + 1);
                    Ok(())
                },
                |_| false,
            )
            .unwrap_or_else(|error| panic!("phase={phase}: {error}"));
            assert_eq!(node_checks.get(), expected_node_checks, "phase={phase}");
            assert_eq!(
                provider_checks.get(),
                expected_provider_checks,
                "phase={phase}"
            );
            assert_eq!(
                committed_checks.get(),
                expected_committed_checks,
                "phase={phase}"
            );
            assert_eq!(
                paths.node_runtime_dir().exists(),
                phase == "committed",
                "phase={phase}"
            );
            assert_eq!(
                paths.youtube_po_provider_dir().exists(),
                phase == "committed",
                "phase={phase}"
            );
            assert!(!stage_root.exists(), "phase={phase}");
            let _ = std::fs::remove_dir_all(base);
        }
    }

    #[test]
    fn committed_recovery_authenticates_finals_before_owner_release() {
        for mutation in ["valid", "tampered", "missing"] {
            let base = std::env::temp_dir().join(format!(
                "vv_pcr_{}_{}_{}",
                &mutation[..1],
                std::process::id(),
                uuid::Uuid::new_v4()
            ));
            let paths = AppPaths::new(base.clone());
            let (_attempt_id, _token, _stage_root) =
                seed_provider_recovery_phase(&paths, "committed");
            if mutation == "tampered" {
                std::fs::write(paths.node_runtime_dir().join("node_fixture"), b"changed").unwrap();
            } else if mutation == "missing" {
                std::fs::remove_dir_all(paths.youtube_po_provider_dir()).unwrap();
            }
            let result = reconcile_interrupted_provider_install_with_checks(
                &paths,
                |_| Ok(()),
                |_| Ok(()),
                |paths, lineage| {
                    verify_published_directory_lineage(
                        &paths.node_runtime_dir(),
                        &lineage.node_directory_identity,
                        &lineage.node_tree_sha256,
                        canonical_provider_node_tree_sha256_hex,
                        "Node",
                    )?;
                    verify_published_directory_lineage(
                        &paths.youtube_po_provider_dir(),
                        &lineage.provider_directory_identity,
                        &lineage.provider_tree_sha256,
                        canonical_provider_application_tree_sha256_hex,
                        "provider",
                    )
                },
                |_| false,
            );
            assert_eq!(result.is_err(), mutation != "valid", "mutation={mutation}");
            let conn = crate::db::open(&paths).unwrap();
            crate::db::migrate(&conn).unwrap();
            let owners = conn
                .query_row("SELECT COUNT(*) FROM provider_install_owner", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap();
            assert_eq!(
                owners,
                i64::from(mutation != "valid"),
                "mutation={mutation}"
            );
            assert!(load_provider_installed_identity(&paths).unwrap().is_some());
            let _ = std::fs::remove_dir_all(base);
        }
    }

    #[test]
    fn prepared_lineage_with_copied_markers_preserves_prior_installed_payloads() {
        let base = std::env::temp_dir().join(format!(
            "voxvulgi_provider_prepared_preserve_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let paths = AppPaths::new(base.clone());
        let (attempt_id, ownership_token, stage_root) =
            seed_provider_recovery_phase(&paths, "prepared");
        for final_dir in [paths.node_runtime_dir(), paths.youtube_po_provider_dir()] {
            std::fs::create_dir_all(&final_dir).unwrap();
            write_provider_ownership_marker(&final_dir, &attempt_id, &ownership_token).unwrap();
            std::fs::write(final_dir.join("prior_payload"), b"preserve-me").unwrap();
        }
        assert!(reconcile_interrupted_provider_install_with_checks(
            &paths,
            |_| panic!("prepared phase must not authenticate Node final"),
            |_| panic!("prepared phase must not authenticate provider final"),
            |_, _| panic!("prepared phase is not committed"),
            |_| false,
        )
        .is_err());
        assert!(paths.node_runtime_dir().join("prior_payload").exists());
        assert!(paths
            .youtube_po_provider_dir()
            .join("prior_payload")
            .exists());
        assert!(
            stage_root.exists(),
            "ambiguous recovery must retain staging evidence"
        );
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn node_phase_never_rolls_back_or_deletes_a_provider_final() {
        let base = std::env::temp_dir().join(format!(
            "voxvulgi_provider_node_phase_preserve_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let paths = AppPaths::new(base.clone());
        let (attempt_id, ownership_token, stage_root) =
            seed_provider_recovery_phase(&paths, "node_published");
        let provider_final = paths.youtube_po_provider_dir();
        std::fs::create_dir_all(&provider_final).unwrap();
        write_provider_ownership_marker(&provider_final, &attempt_id, &ownership_token).unwrap();
        std::fs::write(provider_final.join("prior_payload"), b"preserve-me").unwrap();
        assert!(reconcile_interrupted_provider_install_with_checks(
            &paths,
            |_| Ok(()),
            |_| panic!("node phase must reject provider before authentication"),
            |_, _| panic!("node phase is not committed"),
            |_| false,
        )
        .is_err());
        assert!(paths.node_runtime_dir().exists());
        assert!(provider_final.join("prior_payload").exists());
        assert!(stage_root.exists());
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn forged_marker_and_committed_non_node_tamper_preserve_all_final_payloads() {
        for (phase, committed_tamper) in [
            ("node_publish_intent", false),
            ("node_published", false),
            ("provider_publish_intent", false),
            ("provider_published", false),
            ("committed", true),
        ] {
            let base = std::env::temp_dir().join(format!(
                "vv_fpf_{phase}_{}_{}",
                std::process::id(),
                uuid::Uuid::new_v4()
            ));
            let paths = AppPaths::new(base.clone());
            let (_attempt_id, _ownership_token, stage_root) =
                seed_provider_recovery_phase(&paths, phase);
            if !committed_tamper {
                let forged = if phase.starts_with("provider") {
                    paths.youtube_po_provider_dir()
                } else {
                    paths.node_runtime_dir()
                };
                std::fs::write(provider_attempt_marker(&forged), "copied-wrong-attempt").unwrap();
            }
            let result = reconcile_interrupted_provider_install_with_checks(
                &paths,
                |_| Ok(()),
                |_| Ok(()),
                |_, _| {
                    Err(EngineError::InstallFailed(
                        "simulated plugin/server tamper".to_string(),
                    ))
                },
                |_| false,
            );
            assert!(result.is_err(), "phase={phase}");
            if phase != "prepared" {
                assert!(paths.node_runtime_dir().exists(), "phase={phase}");
            }
            if phase.starts_with("provider") || phase == "committed" {
                assert!(paths.youtube_po_provider_dir().exists(), "phase={phase}");
            }
            assert!(stage_root.exists(), "phase={phase}");
            let _ = std::fs::remove_dir_all(base);
        }
    }

    #[test]
    fn provider_install_owner_is_singleton_across_independent_connections() {
        let base = std::env::temp_dir().join(format!(
            "voxvulgi_provider_owner_race_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let paths = AppPaths::new(base.clone());
        paths.ensure_dirs().unwrap();
        crate::db::ensure_schema(&paths).unwrap();
        let attempts = [
            uuid::Uuid::new_v4().to_string(),
            uuid::Uuid::new_v4().to_string(),
        ];
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let mut joins = Vec::new();
        for attempt_id in attempts.clone() {
            let paths = paths.clone();
            let barrier = std::sync::Arc::clone(&barrier);
            let ownership_token = provider_test_ownership_token();
            let commit_nonce = random_provider_authority_nonce();
            joins.push(std::thread::spawn(move || {
                let stage_root = paths
                    .tools_dir()
                    .join(format!("youtube_po_provider_stage_{attempt_id}"));
                barrier.wait();
                claim_provider_install_owner(
                    &paths,
                    &attempt_id,
                    &stage_root,
                    &provider_ownership_token_digest(&ownership_token),
                    &commit_nonce,
                )
                .map(|_| attempt_id)
            }));
        }
        barrier.wait();
        let results = joins
            .into_iter()
            .map(|join| join.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(results.iter().filter(|result| result.is_err()).count(), 1);
        let conn = crate::db::open(&paths).unwrap();
        crate::db::migrate(&conn).unwrap();
        let owner_and_lineage: (i64, i64) = (
            conn.query_row("SELECT COUNT(*) FROM provider_install_owner", [], |row| {
                row.get(0)
            })
            .unwrap(),
            conn.query_row("SELECT COUNT(*) FROM provider_install_lineage", [], |row| {
                row.get(0)
            })
            .unwrap(),
        );
        assert_eq!(owner_and_lineage, (1, 1));
        drop(conn);
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn provider_install_owner_child_process_probe() {
        let Some(base) = std::env::var_os("VOXVULGI_PROVIDER_OWNER_PROBE_ROOT").map(PathBuf::from)
        else {
            return;
        };
        let attempt_id =
            std::env::var("VOXVULGI_PROVIDER_OWNER_PROBE_ATTEMPT").expect("child attempt id");
        let ownership_token =
            std::env::var("VOXVULGI_PROVIDER_OWNER_PROBE_TOKEN").expect("child ownership token");
        let commit_nonce =
            std::env::var("VOXVULGI_PROVIDER_OWNER_PROBE_NONCE").expect("child commit nonce");
        let paths = AppPaths::new(base);
        let stage_root = paths
            .tools_dir()
            .join(format!("youtube_po_provider_stage_{attempt_id}"));
        if claim_provider_install_owner(
            &paths,
            &attempt_id,
            &stage_root,
            &provider_ownership_token_digest(&ownership_token),
            &commit_nonce,
        )
        .is_err()
        {
            // This helper runs only in a spawned test process. Exit distinctly without a panic
            // so the parent can assert that exactly one cross-process claimant was rejected.
            std::process::exit(42);
        }
    }

    #[cfg(windows)]
    #[test]
    fn provider_install_interprocess_lock_child_probe() {
        let Some(base) =
            std::env::var_os("VOXVULGI_PROVIDER_INSTALL_LOCK_PROBE_ROOT").map(PathBuf::from)
        else {
            return;
        };
        let timeout_ms = std::env::var("VOXVULGI_PROVIDER_INSTALL_LOCK_PROBE_TIMEOUT_MS")
            .ok()
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(250);
        let paths = AppPaths::new(base);
        paths.ensure_dirs().unwrap();
        if acquire_youtube_po_provider_install_interprocess_lock(&paths, timeout_ms).is_err() {
            std::process::exit(42);
        }
    }

    #[cfg(windows)]
    #[test]
    fn live_provider_install_cannot_be_recovered_by_a_second_process() {
        let base = std::env::temp_dir().join(format!(
            "voxvulgi_provider_install_lock_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let paths = AppPaths::new(base.clone());
        paths.ensure_dirs().unwrap();
        let test_exe = std::env::current_exe().unwrap();

        let guard = acquire_youtube_po_provider_install_interprocess_lock(&paths, 500).unwrap();
        let blocked = std::process::Command::new(&test_exe)
            .args([
                "--exact",
                "tools::tests::provider_install_interprocess_lock_child_probe",
            ])
            .env("VOXVULGI_PROVIDER_INSTALL_LOCK_PROBE_ROOT", &base)
            .env("VOXVULGI_PROVIDER_INSTALL_LOCK_PROBE_TIMEOUT_MS", "100")
            .status()
            .unwrap();
        assert_eq!(blocked.code(), Some(42));

        drop(guard);
        let acquired_after_release = std::process::Command::new(&test_exe)
            .args([
                "--exact",
                "tools::tests::provider_install_interprocess_lock_child_probe",
            ])
            .env("VOXVULGI_PROVIDER_INSTALL_LOCK_PROBE_ROOT", &base)
            .env("VOXVULGI_PROVIDER_INSTALL_LOCK_PROBE_TIMEOUT_MS", "500")
            .status()
            .unwrap();
        assert!(acquired_after_release.success());
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn provider_install_owner_rejects_a_second_process_attempt() {
        let base = std::env::temp_dir().join(format!(
            "voxvulgi_provider_owner_process_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let paths = AppPaths::new(base.clone());
        paths.ensure_dirs().unwrap();
        crate::db::ensure_schema(&paths).unwrap();
        let test_exe = std::env::current_exe().expect("current Rust test executable");
        let mut children = Vec::new();
        for _ in 0..2 {
            let attempt_id = uuid::Uuid::new_v4().to_string();
            let ownership_token = provider_test_ownership_token();
            let commit_nonce = random_provider_authority_nonce();
            children.push(
                std::process::Command::new(&test_exe)
                    .args([
                        "--exact",
                        "tools::tests::provider_install_owner_child_process_probe",
                    ])
                    .env("VOXVULGI_PROVIDER_OWNER_PROBE_ROOT", &base)
                    .env("VOXVULGI_PROVIDER_OWNER_PROBE_ATTEMPT", attempt_id)
                    .env("VOXVULGI_PROVIDER_OWNER_PROBE_TOKEN", ownership_token)
                    .env("VOXVULGI_PROVIDER_OWNER_PROBE_NONCE", commit_nonce)
                    .spawn()
                    .expect("spawn independent provider owner claimant"),
            );
        }
        let statuses = children
            .into_iter()
            .map(|mut child| child.wait().expect("provider claimant exit"))
            .collect::<Vec<_>>();
        assert_eq!(statuses.iter().filter(|status| status.success()).count(), 1);
        let conn = crate::db::open(&paths).unwrap();
        crate::db::migrate(&conn).unwrap();
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM provider_install_owner", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM provider_install_lineage", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
            1
        );
        let stored_nonces = conn
            .query_row(
                "SELECT owner.commit_nonce,lineage.commit_nonce
                 FROM provider_install_owner owner
                 JOIN provider_install_lineage lineage ON lineage.attempt_id=owner.attempt_id
                 WHERE owner.singleton=1",
                [],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .unwrap();
        assert!(stored_nonces.0.len() >= 32);
        assert_eq!(stored_nonces.0, stored_nonces.1);
        drop(conn);
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn provider_install_owner_same_attempt_claim_is_idempotent() {
        let base = std::env::temp_dir().join(format!(
            "voxvulgi_provider_owner_idempotent_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let paths = AppPaths::new(base.clone());
        paths.ensure_dirs().unwrap();
        let attempt_id = uuid::Uuid::new_v4().to_string();
        let ownership_token = provider_test_ownership_token();
        let stage_root = paths
            .tools_dir()
            .join(format!("youtube_po_provider_stage_{attempt_id}"));
        let token_digest = provider_ownership_token_digest(&ownership_token);
        let commit_nonce = random_provider_authority_nonce();
        claim_provider_install_owner(
            &paths,
            &attempt_id,
            &stage_root,
            &token_digest,
            &commit_nonce,
        )
        .unwrap();
        claim_provider_install_owner(
            &paths,
            &attempt_id,
            &stage_root,
            &token_digest,
            &commit_nonce,
        )
        .unwrap();
        let conn = crate::db::open(&paths).unwrap();
        crate::db::migrate(&conn).unwrap();
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM provider_install_owner", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM provider_install_lineage", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
            1
        );
        let stored_nonces = conn
            .query_row(
                "SELECT owner.commit_nonce,lineage.commit_nonce
                 FROM provider_install_owner owner
                 JOIN provider_install_lineage lineage ON lineage.attempt_id=owner.attempt_id
                 WHERE owner.singleton=1",
                [],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .unwrap();
        assert!(stored_nonces.0.len() >= 32);
        assert_eq!(stored_nonces.0, commit_nonce);
        assert_eq!(stored_nonces.1, commit_nonce);
        drop(conn);
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn provider_recovery_rejects_traversal_and_receipt_only_forged_markers() {
        let base = std::env::temp_dir().join(format!(
            "voxvulgi_provider_recovery_forgery_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let paths = AppPaths::new(base.clone());
        paths.ensure_dirs().unwrap();
        let conn = crate::db::open(&paths).unwrap();
        crate::db::migrate(&conn).unwrap();
        drop(conn);
        let traversal_id = format!("{}\\..\\..\\outside", uuid::Uuid::new_v4());
        let traversal_root = paths
            .tools_dir()
            .join(format!("youtube_po_provider_stage_{traversal_id}"));
        assert!(validated_provider_stage_root(&paths, &traversal_id, &traversal_root).is_err());

        let forged_id = uuid::Uuid::new_v4().to_string();
        let forged_stage = paths
            .tools_dir()
            .join(format!("youtube_po_provider_stage_{forged_id}"));
        std::fs::create_dir_all(paths.node_runtime_dir()).unwrap();
        std::fs::create_dir_all(paths.youtube_po_provider_dir()).unwrap();
        std::fs::write(
            provider_attempt_marker(&paths.node_runtime_dir()),
            &forged_id,
        )
        .unwrap();
        std::fs::write(
            provider_attempt_marker(&paths.youtube_po_provider_dir()),
            &forged_id,
        )
        .unwrap();
        write_provider_install_attempt_receipt(
            &paths,
            &forged_id,
            &forged_stage,
            "provider_published",
        )
        .unwrap();
        assert!(reconcile_interrupted_provider_install(&paths).is_ok());
        assert!(
            !provider_install_attempt_receipt_path(&paths).exists(),
            "a receipt without trusted lineage is only an orphan carrier"
        );
        assert!(paths.node_runtime_dir().exists());
        assert!(paths.youtube_po_provider_dir().exists());
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn launch_rejects_an_incomplete_attestation_without_spawning() {
        let _guard = PROVIDER_INTEGRITY_TEST_LOCK.lock().unwrap();
        let base = std::env::temp_dir().join(format!(
            "voxvulgi_provider_launch_revalidation_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let paths = AppPaths::new(base.clone());
        paths.ensure_dirs().unwrap();
        let server = paths.youtube_po_provider_server_dir();
        let dependency_dir = server
            .join("node_modules")
            .join("fixture_dependency")
            .join("dist");
        std::fs::create_dir_all(&dependency_dir).unwrap();
        let nested_dependency = dependency_dir.join("index.js");
        std::fs::write(&nested_dependency, b"trusted-byte").unwrap();
        let expected = pinned_dependency_manifest::manifest()
            .youtube_po_provider
            .node_modules_tree_sha256_hex
            .clone();
        attest_provider_node_modules_tree(&server, &expected).unwrap();
        assert!(provider_node_modules_process_attestation(&server).is_some());
        let modified = std::fs::metadata(&nested_dependency)
            .unwrap()
            .modified()
            .unwrap();
        std::fs::write(&nested_dependency, b"tampered-byt").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&nested_dependency)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(modified))
            .unwrap();
        let scans_before = youtube_po_provider_verification_progress(&paths)
            .map(|progress| progress.scan_count)
            .unwrap_or(0);
        assert!(ensure_youtube_po_provider(&paths).is_err());
        let progress = youtube_po_provider_verification_progress(&paths)
            .expect("launch reauthentication progress");
        assert_eq!(
            progress.scan_count, scans_before,
            "an incomplete fixture must fail lineage authentication before a tree scan"
        );
        assert!(provider_node_modules_process_attestation(&server).is_none());
        assert!(youtube_po_provider_slot().lock().unwrap().is_none());
        assert_eq!(
            youtube_po_provider_install_status(&paths).node_modules_integrity_state,
            "invalid"
        );
        let _ = std::fs::remove_dir_all(base);
    }

    #[cfg(windows)]
    #[test]
    fn exact_provider_launch_rejects_same_size_nested_tamper_before_spawn() {
        let Some(fixture_base) =
            std::env::var_os("VOXVULGI_EXACT_PROVIDER_FIXTURE_BASE_DIR").map(PathBuf::from)
        else {
            eprintln!(
                "VOXVULGI_EXACT_PROVIDER_FIXTURE_BASE_DIR not set; exact provider launch tamper proof skipped"
            );
            return;
        };
        if std::env::var("VOXVULGI_EXACT_PROVIDER_FIXTURE_MUTATION_APPROVED").as_deref() != Ok("1")
        {
            eprintln!(
                "VOXVULGI_EXACT_PROVIDER_FIXTURE_MUTATION_APPROVED=1 not set; mutation proof skipped"
            );
            return;
        }
        let _guard = PROVIDER_INTEGRITY_TEST_LOCK.lock().unwrap();
        let paths = AppPaths::new(fixture_base);
        let normal_base = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .map(|root| root.join("com.voxvulgi.voxvulgi"));
        assert_ne!(
            normal_base
                .as_deref()
                .and_then(|path| path.canonicalize().ok()),
            paths.base_dir.canonicalize().ok(),
            "the exact mutation fixture must never be the operator's normal app-data root"
        );

        let baseline = ensure_youtube_po_provider(&paths)
            .expect("the supplied exact fixture must start from a verified baseline");
        assert!(baseline.healthy, "exact fixture baseline must be running");
        let baseline_pid = baseline.process_id.expect("baseline provider pid");
        let server = paths.youtube_po_provider_server_dir();
        let mut stack = vec![server.join("node_modules")];
        let mut nested_dependency = None;
        while let Some(directory) = stack.pop() {
            for entry in std::fs::read_dir(&directory).expect("walk exact node_modules fixture") {
                let entry = entry.expect("read exact node_modules entry");
                let path = entry.path();
                let metadata = entry.metadata().expect("read exact dependency metadata");
                if metadata.is_dir() {
                    stack.push(path);
                } else if metadata.is_file() && metadata.len() > 0 {
                    nested_dependency = Some(path);
                    break;
                }
            }
            if nested_dependency.is_some() {
                break;
            }
        }
        let nested_dependency = nested_dependency.expect("exact fixture nested dependency file");
        let mut bytes = std::fs::read(&nested_dependency).expect("read exact dependency bytes");
        let original_mtime = filetime::FileTime::from_last_modification_time(
            &std::fs::metadata(&nested_dependency).expect("read exact dependency timestamp"),
        );
        bytes[0] ^= 0x01;
        std::fs::write(&nested_dependency, &bytes).expect("write same-size nested tamper");
        filetime::set_file_mtime(&nested_dependency, original_mtime)
            .expect("restore nested dependency timestamp");

        let scans_before = youtube_po_provider_verification_progress(&paths)
            .expect("baseline provider progress")
            .scan_count;
        let launch_result = ensure_youtube_po_provider(&paths);
        let progress = youtube_po_provider_verification_progress(&paths)
            .expect("tamper verification progress");
        let slot_empty = youtube_po_provider_slot().lock().unwrap().is_none();
        let integrity_state =
            youtube_po_provider_install_status(&paths).node_modules_integrity_state;
        bytes[0] ^= 0x01;
        std::fs::write(&nested_dependency, &bytes).expect("restore exact dependency bytes");
        filetime::set_file_mtime(&nested_dependency, original_mtime)
            .expect("restore exact dependency timestamp after proof");

        let error = launch_result.expect_err("same-size nested tamper must fail before spawn");
        let error_text = error.to_string();
        assert!(
            error_text.contains("integrity")
                || error_text.contains("tree")
                || error_text.contains("hash mismatch"),
            "unexpected tamper failure: {error_text}"
        );
        assert_eq!(progress.scan_count, scans_before + 1);
        assert!(slot_empty);
        assert_eq!(integrity_state, "invalid");
        assert!(
            provider_process_identity(baseline_pid).is_none(),
            "tamper rejection must terminate and reap the previously healthy provider child"
        );

        verify_youtube_po_provider_node_modules(&paths)
            .expect("restored exact fixture must re-attest before shared-failure proof");
        let original_bytes = std::fs::read(&nested_dependency).expect("read restored dependency");
        let restored_mtime = filetime::FileTime::from_last_modification_time(
            &std::fs::metadata(&nested_dependency).expect("read restored dependency timestamp"),
        );
        let mut second_tamper = original_bytes.clone();
        second_tamper[0] ^= 0x01;
        std::fs::write(&nested_dependency, &second_tamper)
            .expect("write concurrent same-size tamper");
        filetime::set_file_mtime(&nested_dependency, restored_mtime)
            .expect("restore concurrent tamper timestamp");
        let shared_scans_before = youtube_po_provider_verification_progress(&paths)
            .expect("restored baseline progress")
            .scan_count;
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(6));
        let mut callers = Vec::new();
        for _ in 0..6 {
            let caller_paths = paths.clone();
            let caller_barrier = std::sync::Arc::clone(&barrier);
            callers.push(std::thread::spawn(move || {
                caller_barrier.wait();
                verify_youtube_po_provider_node_modules(&caller_paths)
                    .expect_err("all exact tamper waiters must receive one failure")
                    .to_string()
            }));
        }
        let errors = callers
            .into_iter()
            .map(|caller| caller.join().expect("provider verification waiter"))
            .collect::<Vec<_>>();
        std::fs::write(&nested_dependency, &original_bytes)
            .expect("restore exact dependency after concurrent proof");
        filetime::set_file_mtime(&nested_dependency, restored_mtime)
            .expect("restore exact dependency timestamp after concurrent proof");
        assert!(errors.windows(2).all(|pair| pair[0] == pair[1]));
        assert_eq!(
            youtube_po_provider_verification_progress(&paths)
                .expect("shared failure progress")
                .scan_count,
            shared_scans_before + 1,
            "concurrent callers for one tampered generation must share one failed scan"
        );
    }

    #[cfg(windows)]
    #[test]
    fn pinned_npm_ignore_scripts_blocks_an_unexpected_lifecycle_package() {
        let Some(node_dir) = std::env::var_os("VOXVULGI_PINNED_NODE_DIR").map(PathBuf::from) else {
            eprintln!("VOXVULGI_PINNED_NODE_DIR not set; exact archive executable probe skipped");
            return;
        };
        let npm = node_dir.join("npm.cmd");
        let expected_npm = &pinned_dependency_manifest::manifest()
            .node_windows
            .npm_version;
        assert_eq!(
            tool_version_first_line_with_arg(&npm, "--version").as_deref(),
            Some(expected_npm.as_str())
        );
        let base = std::env::temp_dir().join(format!(
            "voxvulgi_npm_negative_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let dependency = base.join("unexpected_dependency");
        let app = base.join("app");
        std::fs::create_dir_all(&dependency).unwrap();
        std::fs::create_dir_all(&app).unwrap();
        std::fs::write(
            dependency.join("package.json"),
            r#"{"name":"unexpected-lifecycle","version":"1.0.0","scripts":{"postinstall":"node postinstall.js"}}"#,
        )
        .unwrap();
        std::fs::write(
            dependency.join("postinstall.js"),
            "require('fs').writeFileSync('../unexpected_lifecycle_ran','bad')",
        )
        .unwrap();
        std::fs::write(
            app.join("package.json"),
            r#"{"name":"fixture","version":"1.0.0","dependencies":{"unexpected-lifecycle":"file:../unexpected_dependency"}}"#,
        )
        .unwrap();
        let lock = crate::cmd::command(&npm)
            .args([
                "install",
                "--package-lock-only",
                "--ignore-scripts",
                "--no-audit",
                "--registry=https://registry.npmjs.org/",
            ])
            .current_dir(&app)
            .owned_output()
            .unwrap();
        assert!(lock.status.success(), "fixture lock generation failed");
        std::fs::write(
            app.join(".npmrc"),
            b"ignore-scripts=false\nregistry=http://127.0.0.1:9/\n",
        )
        .unwrap();
        let install =
            run_provider_npm(&node_dir, &app, &["ci", "--ignore-scripts", "--no-audit"]).unwrap();
        assert!(install.status.success(), "fixture npm ci failed");
        assert!(
            !app.join("unexpected_lifecycle_ran").exists()
                && !base.join("unexpected_lifecycle_ran").exists(),
            "an unexpected dependency lifecycle script executed"
        );
        let effective_project_config = std::fs::read_to_string(app.join(".npmrc")).unwrap();
        assert!(effective_project_config.contains("ignore-scripts=true"));
        assert!(effective_project_config.contains("registry=https://registry.npmjs.org/"));
        let _ = std::fs::remove_dir_all(base);
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "WP-0299 exact pinned provider executable install proof"]
    fn exact_pinned_provider_install_is_audited_built_and_runnable() {
        let root = std::env::var_os("VOXVULGI_PROVIDER_INSTALL_PROBE_ROOT")
            .map(PathBuf::from)
            .expect("set VOXVULGI_PROVIDER_INSTALL_PROBE_ROOT to an empty disposable path");
        assert!(!root.exists(), "probe root must be fresh");
        let paths = AppPaths::new(root.clone());
        let status = install_youtube_po_provider(&paths).expect("exact provider install");
        assert!(status.installed);
        assert!(status.security_audit_passed);
        assert_eq!(status.node_version.as_deref(), Some("v24.19.0"));
        assert_eq!(status.npm_version.as_deref(), Some("11.17.0"));
        assert_eq!(status.provider_version, "1.3.1");
        eprintln!(
            "provider_server_entrypoint_sha256={}",
            status
                .server_entrypoint_sha256_hex
                .as_deref()
                .expect("built entrypoint hash")
        );
        let runtime = ensure_youtube_po_provider(&paths).expect("localhost provider runtime");
        assert!(runtime.healthy);
        assert!(!paths
            .youtube_po_provider_server_dir()
            .join("tsconfig.tsbuildinfo")
            .exists());
        assert!(!paths
            .youtube_po_provider_server_dir()
            .join(".npm_cache")
            .exists());
        shutdown_youtube_po_provider();
        if std::env::var_os("VOXVULGI_KEEP_PROVIDER_PROBE").is_none() {
            let _ = std::fs::remove_dir_all(root);
        }
    }

    #[cfg(windows)]
    #[test]
    fn existing_exact_provider_status_rejects_same_size_server_tamper() {
        let Some(root) =
            std::env::var_os("VOXVULGI_PROVIDER_EXISTING_PROBE_ROOT").map(PathBuf::from)
        else {
            eprintln!(
                "VOXVULGI_PROVIDER_EXISTING_PROBE_ROOT not set; existing provider probe skipped"
            );
            return;
        };
        let paths = AppPaths::new(root);
        let verified = verify_youtube_po_provider_node_modules(&paths)
            .expect("fresh process must establish authoritative node_modules trust");
        assert!(
            verified.installed,
            "authoritative fresh-process verification must make the exact provider ready: {:?}",
            verified.readiness_error
        );
        let before = youtube_po_provider_install_status(&paths);
        assert!(
            before.installed,
            "existing exact provider must begin ready: {:?}",
            before.readiness_error
        );
        assert_eq!(
            before.server_entrypoint_sha256_hex.as_deref(),
            Some(
                pinned_dependency_manifest::manifest()
                    .youtube_po_provider
                    .server_entrypoint_sha256_hex
                    .as_str()
            )
        );
        let entrypoint = paths.youtube_po_provider_entrypoint();
        let original = std::fs::read(&entrypoint).unwrap();
        let mut tampered = original.clone();
        let index = tampered.len() / 2;
        tampered[index] ^= 1;
        std::fs::write(&entrypoint, &tampered).unwrap();
        assert_eq!(
            std::fs::metadata(&entrypoint).unwrap().len(),
            original.len() as u64
        );
        let rejected = youtube_po_provider_install_status(&paths);
        std::fs::write(&entrypoint, &original).unwrap();
        assert!(!rejected.installed);
        assert!(rejected
            .readiness_error
            .as_deref()
            .unwrap_or_default()
            .contains("server_bytes=false"));
        assert!(youtube_po_provider_install_status(&paths).installed);
    }

    #[test]
    fn provider_pair_publication_rolls_back_second_publish_and_readiness_failures() {
        for fail_readiness in [false, true] {
            let base = std::env::temp_dir().join(format!(
                "voxvulgi_provider_publish_{}_{}_{}",
                std::process::id(),
                fail_readiness,
                uuid::Uuid::new_v4()
            ));
            let stage_root = base.join("attempt");
            let node_stage = stage_root.join("node");
            let provider_stage = stage_root.join("provider");
            let final_node = base.join("final_node");
            let final_provider = base.join("final_provider");
            std::fs::create_dir_all(&node_stage).unwrap();
            std::fs::create_dir_all(&provider_stage).unwrap();
            std::fs::write(node_stage.join("owned.txt"), b"node").unwrap();
            std::fs::write(provider_stage.join("owned.txt"), b"provider").unwrap();
            let guard = AttemptDirectoryGuard::new(stage_root.clone());
            let result = publish_provider_pair_with_checks(
                &node_stage,
                &provider_stage,
                &final_node,
                &final_provider,
                || Ok(()),
                || {
                    if fail_readiness {
                        Ok(())
                    } else {
                        Err(EngineError::InstallFailed(
                            "injected second publication failure".to_string(),
                        ))
                    }
                },
                || {
                    if fail_readiness {
                        Err(EngineError::InstallFailed(
                            "injected readiness failure".to_string(),
                        ))
                    } else {
                        Ok(())
                    }
                },
            );
            assert!(result.is_err());
            assert!(
                !final_node.exists(),
                "attempt-owned Node must not be orphaned"
            );
            assert!(
                !final_provider.exists(),
                "attempt-owned provider must not be orphaned"
            );
            assert!(node_stage.exists(), "Node must return to guarded staging");
            assert!(
                provider_stage.exists(),
                "provider must return to guarded staging"
            );
            drop(guard);
            assert!(
                !stage_root.exists(),
                "failure guard must remove staging bytes"
            );
            let _ = std::fs::remove_dir_all(base);
        }
    }

    #[test]
    fn kokoro_app_cache_ready_requires_snapshot_files_not_just_marker() {
        use std::fs;
        let base =
            std::env::temp_dir().join(format!("voxvulgi_kokoro_gate_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let paths = AppPaths::new(base.clone());

        // The stale-marker bug scenario: nothing in the app-local HF cache yet.
        assert!(
            !kokoro_app_cache_ready(&paths),
            "empty app cache must not read as ready"
        );

        let repo = paths
            .cache_dir()
            .join("huggingface")
            .join("hub")
            .join("models--hexgrad--Kokoro-82M");
        let sha = "f3ff3571791e39611d31c381e3a41a3af07b4987";
        let snapshot = repo.join("snapshots").join(sha);
        fs::create_dir_all(snapshot.join("voices")).unwrap();
        fs::write(snapshot.join("config.json"), b"{}").unwrap();
        fs::write(snapshot.join("kokoro-v1_0.pth"), b"weights").unwrap();
        fs::write(snapshot.join("voices").join("af_heart.pt"), b"voice").unwrap();

        // Snapshot files present but no `refs/main` -> the offline resolver cannot map
        // `main` -> sha, so the job would still fail. Must read as not ready.
        assert!(
            !kokoro_app_cache_ready(&paths),
            "snapshot files without refs/main must not read as ready"
        );

        fs::create_dir_all(repo.join("refs")).unwrap();
        fs::write(repo.join("refs").join("main"), sha).unwrap();
        assert!(
            kokoro_app_cache_ready(&paths),
            "refs/main + config + weights + default voice present must read as ready"
        );

        // Missing the default voice -> the offline job would fail loading it.
        fs::remove_file(snapshot.join("voices").join("af_heart.pt")).unwrap();
        assert!(
            !kokoro_app_cache_ready(&paths),
            "missing default voice must not read as ready"
        );

        let _ = fs::remove_dir_all(&base);
    }
}
