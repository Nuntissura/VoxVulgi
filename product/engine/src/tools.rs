use crate::pack_install_state;
use crate::paths::AppPaths;
use crate::python_lockfile::{self, PythonLockfile};
use crate::{pinned_dependency_manifest, vendor_patches};
use crate::{EngineError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const PYTHON_COMMAND_TIMEOUT_SECS: u64 = 30 * 60;
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
        .output()
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
    let mut result = run_python_checked(
        paths,
        python,
        &args,
        &format!("{error_prefix}: pip install --require-hashes failed for {pack_name}"),
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
        .output()
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
    let warmup_ready = kokoro_warmup_probe_path(paths).exists();
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
        "Kokoro is installed, but its local model warmup has not completed.".to_string()
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
    let kokoro_warmup_ready = kokoro_warmup_probe_path(paths).exists();
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
        "Kokoro base TTS is installed, but its local model warmup has not completed.".to_string()
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
    let output = crate::cmd::command(python)
        .args(["-c", &code])
        .output()
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

fn python_module_available(python: &std::path::Path, module: &str) -> bool {
    let code = format!(
        "import importlib.util\ntry:\n    found = importlib.util.find_spec({module:?}) is not None\nexcept Exception:\n    found = False\nraise SystemExit(0 if found else 1)\n"
    );
    crate::cmd::command(python)
        .args(["-c", &code])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn python_distribution_version(python: &std::path::Path, distribution: &str) -> Option<String> {
    let code = format!("import importlib.metadata as m\nprint(m.version({distribution:?}))\n");
    let output = crate::cmd::command(python)
        .args(["-c", &code])
        .output()
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

    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| EngineError::InstallFailed(format!("{error_prefix}: {e}")))?;
    let started = std::time::Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|e| EngineError::InstallFailed(format!("{error_prefix}: {e}")))?
        {
            let mut stderr = Vec::new();
            if let Some(mut pipe) = child.stderr.take() {
                let _ = pipe.read_to_end(&mut stderr);
            }
            if let Some(mut pipe) = child.stdout.take() {
                let mut stdout = Vec::new();
                let _ = pipe.read_to_end(&mut stdout);
            }
            if !status.success() {
                let stderr = String::from_utf8_lossy(&stderr);
                return Err(EngineError::InstallFailed(format!(
                    "{error_prefix} (code={:?}): {}",
                    status.code(),
                    stderr.trim()
                )));
            }
            return Ok(());
        }

        if started.elapsed() > std::time::Duration::from_secs(PYTHON_COMMAND_TIMEOUT_SECS) {
            let _ = child.kill();
            let output = child
                .wait_with_output()
                .map_err(|e| EngineError::InstallFailed(format!("{error_prefix}: {e}")))?;
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr = stderr.trim();
            return Err(EngineError::InstallFailed(format!(
                "{error_prefix} timed out after {PYTHON_COMMAND_TIMEOUT_SECS}s{}{}",
                if stderr.is_empty() { "" } else { ": " },
                stderr
            )));
        }

        std::thread::sleep(std::time::Duration::from_millis(500));
    }
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
    fn pack_lockfile_runtime_ready_allows_stale_receipt_when_versions_match() {
        assert!(pack_lockfile_runtime_ready(true, true));
        assert!(pack_lockfile_runtime_ready(false, true));
        assert!(!pack_lockfile_runtime_ready(false, false));
    }
}
