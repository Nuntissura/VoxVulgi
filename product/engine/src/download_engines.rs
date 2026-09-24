use crate::paths::AppPaths;
use crate::{pinned_dependency_manifest, EngineError, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

const ENGINE_SCHEMA_VERSION: u32 = 1;
const PACKAGED_ENGINE_ID: &str = "packaged-yt-dlp";
const YT_DLP_CLI_PROTOCOL: &str = "yt_dlp_cli_v1";
const VERSION_PROBE_TIMEOUT: Duration = Duration::from_secs(120);
const COMPATIBILITY_PROBE_TIMEOUT: Duration = Duration::from_secs(120);
const UPDATE_CACHE_TTL_MS: i64 = 24 * 60 * 60 * 1_000;
const UPDATE_CACHE_FUTURE_SKEW_MS: i64 = 5 * 60 * 1_000;
const YT_DLP_LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DownloadEngineKind {
    PackagedYtDlp,
    ManagedYtDlp,
    CustomExecutable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct DownloadEngineManifest {
    schema_version: u32,
    engine_id: String,
    kind: DownloadEngineKind,
    protocol: String,
    executable: String,
    version: String,
    sha256_hex: String,
    file_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct DownloadEngineSelection {
    schema_version: u32,
    engine_id: String,
    manifest_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DownloadEngineStatus {
    pub selected_engine_id: String,
    pub kind: DownloadEngineKind,
    pub protocol: String,
    pub path: String,
    pub version: Option<String>,
    pub sha256_hex: Option<String>,
    pub file_bytes: Option<u64>,
    pub verified: bool,
    pub error: Option<String>,
    pub update_state: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DownloadEngineUpdateResult {
    pub checked: bool,
    pub updated: bool,
    pub latest_version: Option<String>,
    pub error: Option<String>,
    pub status: DownloadEngineStatus,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DownloadEngineCompatibilityReceipt {
    pub passed: bool,
    pub selected_engine_id: String,
    pub kind: DownloadEngineKind,
    pub protocol: String,
    pub path: String,
    pub version: String,
    pub sha256_hex: String,
    pub file_bytes: u64,
    pub version_probe_passed: bool,
    pub local_fixture_parse_passed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedDownloadEngine {
    pub engine_id: String,
    pub kind: DownloadEngineKind,
    pub protocol: String,
    pub program: PathBuf,
    pub version: String,
    pub sha256_hex: String,
    pub file_bytes: u64,
}

fn mutation_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn engine_root(paths: &AppPaths) -> PathBuf {
    paths.download_engines_dir()
}

fn generations_root(paths: &AppPaths) -> PathBuf {
    engine_root(paths).join("generations")
}

fn current_selection_path(paths: &AppPaths) -> PathBuf {
    engine_root(paths).join("current.json")
}

fn previous_selection_path(paths: &AppPaths) -> PathBuf {
    engine_root(paths).join("previous.json")
}

fn packaged_fallback_path(paths: &AppPaths) -> PathBuf {
    engine_root(paths).join("packaged_fallback.json")
}

fn update_cache_path(paths: &AppPaths) -> PathBuf {
    engine_root(paths).join("update_check.json")
}

fn packaged_executable(paths: &AppPaths) -> PathBuf {
    let mut path = paths.tools_dir().join("yt-dlp").join("yt-dlp");
    if cfg!(windows) {
        path.set_extension("exe");
    }
    path
}

fn default_selection() -> DownloadEngineSelection {
    DownloadEngineSelection {
        schema_version: ENGINE_SCHEMA_VERSION,
        engine_id: PACKAGED_ENGINE_ID.to_string(),
        manifest_sha256: None,
    }
}

fn read_selection(path: &Path) -> Result<DownloadEngineSelection> {
    let bytes = std::fs::read(path)?;
    let selection: DownloadEngineSelection = serde_json::from_slice(&bytes)?;
    validate_selection(&selection)?;
    Ok(selection)
}

fn current_selection(paths: &AppPaths) -> Result<DownloadEngineSelection> {
    let path = current_selection_path(paths);
    if path.exists() {
        read_selection(&path)
    } else {
        Ok(default_selection())
    }
}

fn validate_token(value: &str, label: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(EngineError::InstallFailed(format!(
            "download engine {label} is not a safe token"
        )));
    }
    Ok(())
}

fn validate_sha256(value: &str, label: &str) -> Result<()> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(EngineError::InstallFailed(format!(
            "download engine {label} is not a SHA-256 digest"
        )));
    }
    Ok(())
}

fn validate_selection(selection: &DownloadEngineSelection) -> Result<()> {
    if selection.schema_version != ENGINE_SCHEMA_VERSION {
        return Err(EngineError::InstallFailed(format!(
            "download engine selection schema {} is unsupported",
            selection.schema_version
        )));
    }
    validate_token(&selection.engine_id, "selection ID")?;
    if selection.engine_id == PACKAGED_ENGINE_ID {
        if selection.manifest_sha256.is_some() {
            return Err(EngineError::InstallFailed(
                "packaged download engine selection must not carry a manifest hash".to_string(),
            ));
        }
    } else {
        validate_sha256(
            selection.manifest_sha256.as_deref().unwrap_or_default(),
            "selection manifest hash",
        )?;
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode_upper(hasher.finalize()))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    hex::encode_upper(Sha256::digest(bytes))
}

fn executable_version(path: &Path) -> Result<String> {
    let output = crate::cmd::run_owned_output(
        crate::cmd::command(path).arg("--version"),
        VERSION_PROBE_TIMEOUT,
        || false,
    )
    .map_err(|error| {
        EngineError::InstallFailed(format!(
            "download engine version probe failed for {}: {error}",
            path.display()
        ))
    })?;
    if !output.status.success() {
        return Err(EngineError::InstallFailed(format!(
            "download engine version probe failed for {} (status={})",
            path.display(),
            output.status
        )));
    }
    let version = String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    if version.is_empty() || version.len() > 256 {
        return Err(EngineError::InstallFailed(format!(
            "download engine returned an invalid version from {}",
            path.display()
        )));
    }
    Ok(version)
}

fn compatibility_probe(path: &Path) -> Result<()> {
    let fixture_path = std::env::temp_dir().join(format!(
        "voxvulgi_engine_probe_{}.mp4",
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::write(&fixture_path, b"voxvulgi-probe")?;

    let probe_result = (|| -> Result<()> {
        let fixture_url = url::Url::from_file_path(&fixture_path)
            .map_err(|_| {
                EngineError::InstallFailed(format!(
                    "download engine compatibility fixture path is not an absolute file URL: {}",
                    fixture_path.display()
                ))
            })?
            .to_string();
        let output = crate::cmd::run_owned_output(
            crate::cmd::command(path).args([
                "--ignore-config",
                "--no-warnings",
                "--enable-file-urls",
                "--skip-download",
                "--dump-single-json",
                &fixture_url,
            ]),
            COMPATIBILITY_PROBE_TIMEOUT,
            || false,
        )
        .map_err(|error| {
            EngineError::InstallFailed(format!(
                "download engine local compatibility probe failed to run: {error}"
            ))
        })?;
        if !output.status.success() {
            return Err(EngineError::InstallFailed(format!(
                "download engine rejected the local compatibility fixture (status={}): {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        let receipt: serde_json::Value =
            serde_json::from_slice(&output.stdout).map_err(|error| {
                EngineError::InstallFailed(format!(
                    "download engine compatibility output was not JSON: {error}"
                ))
            })?;
        let observed_url = receipt
            .get("webpage_url")
            .or_else(|| receipt.get("original_url"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        if observed_url != fixture_url {
            return Err(EngineError::InstallFailed(
                "download engine compatibility receipt did not identify the local fixture"
                    .to_string(),
            ));
        }
        Ok(())
    })();

    let cleanup_result = std::fs::remove_file(&fixture_path);
    if probe_result.is_ok() {
        cleanup_result.map_err(|error| {
            EngineError::InstallFailed(format!(
                "download engine compatibility fixture cleanup failed for {}: {error}",
                fixture_path.display()
            ))
        })?;
    }
    probe_result
}

/// Verified (version, sha256, bytes) per executable, reused while the file's size and
/// modification time are unchanged. A `yt-dlp --version` launch takes 14-16 s on the operator's
/// machine (self-extracting executable + antivirus scan) and status/scheduler paths ask for the
/// identity repeatedly, which froze the UI for 26-38 s per refresh (WP-0322). Any replacement of
/// the executable changes its size or mtime and forces a full re-verification.
type VerifiedIdentityCache =
    Mutex<std::collections::HashMap<PathBuf, (u64, std::time::SystemTime, String, String)>>;

fn verified_identity_cache() -> &'static VerifiedIdentityCache {
    static CACHE: OnceLock<VerifiedIdentityCache> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

fn verified_identity(path: &Path) -> Result<(String, String, u64)> {
    let metadata = std::fs::metadata(path).map_err(|error| {
        EngineError::InstallFailed(format!(
            "download engine executable cannot be read at {}: {error}",
            path.display()
        ))
    })?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err(EngineError::InstallFailed(format!(
            "download engine executable is not a non-empty regular file: {}",
            path.display()
        )));
    }
    let modified = metadata.modified().ok();
    if let Some(modified) = modified {
        if let Some((len, cached_modified, version, hash)) = verified_identity_cache()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(path)
            .cloned()
        {
            if len == metadata.len() && cached_modified == modified {
                return Ok((version, hash, len));
            }
        }
    }
    let identity = verified_identity_uncached(path, &metadata)?;
    if let Some(modified) = modified {
        verified_identity_cache()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(
                path.to_path_buf(),
                (identity.2, modified, identity.0.clone(), identity.1.clone()),
            );
    }
    Ok(identity)
}

fn verified_identity_uncached(
    path: &Path,
    metadata: &std::fs::Metadata,
) -> Result<(String, String, u64)> {
    let hash_before = sha256_file(path)?;
    let version = executable_version(path)?;
    let metadata_after = std::fs::metadata(path)?;
    let hash_after = sha256_file(path)?;
    if metadata.len() != metadata_after.len() || hash_before != hash_after {
        return Err(EngineError::InstallFailed(format!(
            "download engine changed during verification: {}",
            path.display()
        )));
    }
    Ok((version, hash_after, metadata_after.len()))
}

fn manifest_path(paths: &AppPaths, engine_id: &str) -> PathBuf {
    generations_root(paths)
        .join(engine_id)
        .join("engine_manifest.json")
}

fn load_manifest(
    paths: &AppPaths,
    selection: &DownloadEngineSelection,
) -> Result<(DownloadEngineManifest, PathBuf)> {
    let manifest_path = manifest_path(paths, &selection.engine_id);
    let manifest_bytes = std::fs::read(&manifest_path).map_err(|error| {
        EngineError::InstallFailed(format!(
            "selected download engine manifest cannot be read at {}: {error}",
            manifest_path.display()
        ))
    })?;
    let observed_manifest_hash = sha256_bytes(&manifest_bytes);
    if !observed_manifest_hash
        .eq_ignore_ascii_case(selection.manifest_sha256.as_deref().unwrap_or_default())
    {
        return Err(EngineError::InstallFailed(
            "selected download engine manifest hash mismatch".to_string(),
        ));
    }
    let manifest: DownloadEngineManifest = serde_json::from_slice(&manifest_bytes)?;
    if manifest.schema_version != ENGINE_SCHEMA_VERSION
        || manifest.engine_id != selection.engine_id
        || manifest.protocol != YT_DLP_CLI_PROTOCOL
    {
        return Err(EngineError::InstallFailed(
            "selected download engine manifest is incompatible".to_string(),
        ));
    }
    validate_token(&manifest.engine_id, "manifest ID")?;
    validate_sha256(&manifest.sha256_hex, "executable hash")?;
    let executable_relative = Path::new(&manifest.executable);
    if executable_relative.is_absolute()
        || executable_relative.components().any(|part| {
            matches!(
                part,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(EngineError::InstallFailed(
            "download engine manifest executable escapes its generation".to_string(),
        ));
    }
    let generation = generations_root(paths).join(&manifest.engine_id);
    let executable = generation.join(executable_relative);
    let canonical_generation = std::fs::canonicalize(&generation)?;
    let canonical_executable = std::fs::canonicalize(&executable)?;
    if !canonical_executable.starts_with(&canonical_generation) {
        return Err(EngineError::InstallFailed(
            "download engine executable resolves outside its immutable generation".to_string(),
        ));
    }
    Ok((manifest, canonical_executable))
}

fn verify_packaged(paths: &AppPaths) -> Result<ResolvedDownloadEngine> {
    let program = packaged_executable(paths);
    let pin = &pinned_dependency_manifest::manifest().yt_dlp_windows;
    let (version, hash, bytes) = verified_identity(&program)?;
    if bytes != pin.file_bytes {
        return Err(EngineError::SizeMismatch {
            path: program,
            expected: pin.file_bytes,
            actual: bytes,
        });
    }
    if !hash.eq_ignore_ascii_case(&pin.sha256_hex) {
        return Err(EngineError::HashMismatch {
            path: program,
            expected: pin.sha256_hex.clone(),
            actual: hash,
        });
    }
    if version != pin.version {
        return Err(EngineError::InstallFailed(format!(
            "packaged yt-dlp version mismatch (expected={}, observed={version})",
            pin.version
        )));
    }
    Ok(ResolvedDownloadEngine {
        engine_id: PACKAGED_ENGINE_ID.to_string(),
        kind: DownloadEngineKind::PackagedYtDlp,
        protocol: YT_DLP_CLI_PROTOCOL.to_string(),
        program,
        version,
        sha256_hex: pin.sha256_hex.clone(),
        file_bytes: bytes,
    })
}

fn verify_generation(
    paths: &AppPaths,
    selection: &DownloadEngineSelection,
) -> Result<ResolvedDownloadEngine> {
    let (manifest, program) = load_manifest(paths, selection)?;
    let (version, hash, bytes) = verified_identity(&program)?;
    if version != manifest.version || bytes != manifest.file_bytes {
        return Err(EngineError::InstallFailed(
            "selected download engine identity no longer matches its manifest".to_string(),
        ));
    }
    if !hash.eq_ignore_ascii_case(&manifest.sha256_hex) {
        return Err(EngineError::HashMismatch {
            path: program,
            expected: manifest.sha256_hex,
            actual: hash,
        });
    }
    Ok(ResolvedDownloadEngine {
        engine_id: manifest.engine_id,
        kind: manifest.kind,
        protocol: manifest.protocol,
        program,
        version,
        sha256_hex: hash,
        file_bytes: bytes,
    })
}

fn resolve_selection(
    paths: &AppPaths,
    selection: &DownloadEngineSelection,
) -> Result<ResolvedDownloadEngine> {
    validate_selection(selection)?;
    if selection.engine_id == PACKAGED_ENGINE_ID {
        verify_packaged(paths)
    } else {
        verify_generation(paths, selection)
    }
}

pub fn resolve_selected_engine(paths: &AppPaths) -> Result<ResolvedDownloadEngine> {
    resolve_selection(paths, &current_selection(paths)?)
}

pub fn probe_selected_engine(paths: &AppPaths) -> Result<DownloadEngineCompatibilityReceipt> {
    let engine = resolve_selected_engine(paths)?;
    compatibility_probe(&engine.program)?;
    Ok(DownloadEngineCompatibilityReceipt {
        passed: true,
        selected_engine_id: engine.engine_id,
        kind: engine.kind,
        protocol: engine.protocol,
        path: engine.program.to_string_lossy().to_string(),
        version: engine.version,
        sha256_hex: engine.sha256_hex,
        file_bytes: engine.file_bytes,
        version_probe_passed: true,
        local_fixture_parse_passed: true,
    })
}

pub(crate) fn engine_identity_stamp_paths(paths: &AppPaths) -> Vec<PathBuf> {
    let mut paths_to_stamp = vec![
        current_selection_path(paths),
        packaged_fallback_path(paths),
        packaged_executable(paths),
    ];
    if let Ok(selection) = current_selection(paths) {
        if selection.engine_id != PACKAGED_ENGINE_ID {
            let manifest = manifest_path(paths, &selection.engine_id);
            paths_to_stamp.push(manifest);
            let mut executable = generations_root(paths)
                .join(selection.engine_id)
                .join("engine");
            if cfg!(windows) {
                executable.set_extension("exe");
            }
            paths_to_stamp.push(executable);
        }
    }
    paths_to_stamp
}

fn status_error(paths: &AppPaths, error: String) -> DownloadEngineStatus {
    let selection = current_selection(paths).unwrap_or_else(|_| default_selection());
    let (kind, path) = if selection.engine_id == PACKAGED_ENGINE_ID {
        (
            DownloadEngineKind::PackagedYtDlp,
            packaged_executable(paths).to_string_lossy().to_string(),
        )
    } else {
        (
            if selection.engine_id.starts_with("custom-") {
                DownloadEngineKind::CustomExecutable
            } else {
                DownloadEngineKind::ManagedYtDlp
            },
            generations_root(paths)
                .join(&selection.engine_id)
                .to_string_lossy()
                .to_string(),
        )
    };
    DownloadEngineStatus {
        selected_engine_id: selection.engine_id,
        kind,
        protocol: YT_DLP_CLI_PROTOCOL.to_string(),
        path,
        version: None,
        sha256_hex: None,
        file_bytes: None,
        verified: false,
        error: Some(error),
        update_state: "repair_required".to_string(),
    }
}

pub fn engine_status(paths: &AppPaths) -> DownloadEngineStatus {
    match resolve_selected_engine(paths) {
        Ok(engine) => DownloadEngineStatus {
            selected_engine_id: engine.engine_id,
            kind: engine.kind,
            protocol: engine.protocol,
            path: engine.program.to_string_lossy().to_string(),
            version: Some(engine.version),
            sha256_hex: Some(engine.sha256_hex),
            file_bytes: Some(engine.file_bytes),
            verified: true,
            error: None,
            update_state: packaged_update_state(paths),
        },
        Err(error) => status_error(paths, error.to_string()),
    }
}

pub fn packaged_update_state(paths: &AppPaths) -> String {
    let Ok(selected) = current_selection(paths) else {
        return "selection_invalid".to_string();
    };
    let cached_latest = std::fs::read(update_cache_path(paths))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<UpdateCheckCache>(&bytes).ok())
        .filter(|cache| validate_update_cache(cache).is_ok());
    if selected.engine_id == PACKAGED_ENGINE_ID {
        return match verify_packaged(paths) {
            Ok(engine)
                if cached_latest.as_ref().is_some_and(|latest| {
                    engine.version != latest.version
                        || engine.file_bytes != latest.file_bytes
                        || !engine.sha256_hex.eq_ignore_ascii_case(&latest.sha256_hex)
                }) =>
            {
                "update_available".to_string()
            }
            Ok(_) => "current".to_string(),
            Err(_) => "repair_required".to_string(),
        };
    }
    match verify_generation(paths, &selected) {
        Ok(engine) if engine.kind == DownloadEngineKind::ManagedYtDlp => {
            let expected = cached_latest.as_ref();
            if expected.is_none_or(|latest| {
                engine.version == latest.version
                    && engine.file_bytes == latest.file_bytes
                    && engine.sha256_hex.eq_ignore_ascii_case(&latest.sha256_hex)
            }) {
                "current".to_string()
            } else {
                "update_available".to_string()
            }
        }
        Ok(_) => "custom_selected".to_string(),
        Err(_) => "repair_required".to_string(),
    }
}

fn write_all_synced(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

#[cfg(windows)]
fn replace_file_atomically(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn replace_file_atomically(source: &Path, destination: &Path) -> std::io::Result<()> {
    std::fs::rename(source, destination)
}

fn write_json_atomically<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let parent = path.parent().ok_or_else(|| {
        EngineError::InstallFailed("download engine state path has no parent".to_string())
    })?;
    std::fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("state"),
        uuid::Uuid::new_v4()
    ));
    let bytes = serde_json::to_vec_pretty(value)?;
    write_all_synced(&temporary, &bytes)?;
    if let Err(error) = replace_file_atomically(&temporary, path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error.into());
    }
    Ok(())
}

fn activate_selection(paths: &AppPaths, selection: &DownloadEngineSelection) -> Result<()> {
    resolve_selection(paths, selection)?;
    let current_path = current_selection_path(paths);
    if let Ok(current) = current_selection(paths) {
        write_json_atomically(&previous_selection_path(paths), &current)?;
    }
    write_json_atomically(&current_path, selection)
}

/// Select the app-managed yt-dlp lane: its latest verified writable update when present,
/// otherwise the exact yt-dlp executable shipped in the selected runtime generation.
pub fn select_packaged_engine(paths: &AppPaths) -> Result<DownloadEngineStatus> {
    let _guard = mutation_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let fallback = packaged_fallback_path(paths);
    let selection = read_selection(&fallback)
        .ok()
        .filter(|candidate| {
            resolve_selection(paths, candidate)
                .is_ok_and(|engine| engine.kind == DownloadEngineKind::ManagedYtDlp)
        })
        .unwrap_or_else(default_selection);
    activate_selection(paths, &selection)?;
    Ok(engine_status(paths))
}

pub fn clear_custom_engine(paths: &AppPaths) -> Result<DownloadEngineStatus> {
    select_packaged_engine(paths)
}

pub fn select_engine(paths: &AppPaths, engine_id: &str) -> Result<DownloadEngineStatus> {
    let _guard = mutation_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let selection = if engine_id == PACKAGED_ENGINE_ID {
        default_selection()
    } else {
        validate_token(engine_id, "selection ID")?;
        let manifest_bytes = std::fs::read(manifest_path(paths, engine_id))?;
        DownloadEngineSelection {
            schema_version: ENGINE_SCHEMA_VERSION,
            engine_id: engine_id.to_string(),
            manifest_sha256: Some(sha256_bytes(&manifest_bytes)),
        }
    };
    activate_selection(paths, &selection)?;
    Ok(engine_status(paths))
}

fn stage_engine(
    paths: &AppPaths,
    source: &Path,
    kind: DownloadEngineKind,
    expected_version: Option<&str>,
    activate: bool,
) -> Result<(DownloadEngineStatus, DownloadEngineSelection)> {
    let canonical_source = std::fs::canonicalize(source).map_err(|error| {
        EngineError::InstallFailed(format!(
            "download engine candidate cannot be opened at {}: {error}",
            source.display()
        ))
    })?;
    let (source_version, source_hash, source_bytes) = verified_identity(&canonical_source)?;
    if let Some(expected) = expected_version {
        if source_version != expected {
            return Err(EngineError::InstallFailed(format!(
                "download engine candidate version mismatch (expected={expected}, observed={source_version})"
            )));
        }
    }
    let kind_label = match kind {
        DownloadEngineKind::ManagedYtDlp => "managed",
        DownloadEngineKind::CustomExecutable => "custom",
        DownloadEngineKind::PackagedYtDlp => {
            return Err(EngineError::InstallFailed(
                "packaged engines cannot be staged as a generation".to_string(),
            ))
        }
    };
    let engine_id = format!(
        "{kind_label}-{}",
        source_hash
            .chars()
            .take(24)
            .collect::<String>()
            .to_ascii_lowercase()
    );
    let roots = generations_root(paths);
    std::fs::create_dir_all(&roots)?;
    let generation = roots.join(&engine_id);
    let executable_name = if cfg!(windows) {
        "engine.exe"
    } else {
        "engine"
    };
    let manifest = DownloadEngineManifest {
        schema_version: ENGINE_SCHEMA_VERSION,
        engine_id: engine_id.clone(),
        kind,
        protocol: YT_DLP_CLI_PROTOCOL.to_string(),
        executable: executable_name.to_string(),
        version: source_version,
        sha256_hex: source_hash,
        file_bytes: source_bytes,
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
    let manifest_hash = sha256_bytes(&manifest_bytes);
    let selection = DownloadEngineSelection {
        schema_version: ENGINE_SCHEMA_VERSION,
        engine_id,
        manifest_sha256: Some(manifest_hash),
    };

    if generation.exists() {
        let existing = std::fs::read(generation.join("engine_manifest.json"))?;
        if existing != manifest_bytes {
            return Err(EngineError::InstallFailed(
                "immutable download engine generation already exists with different metadata"
                    .to_string(),
            ));
        }
    } else {
        let staging = roots.join(format!(".staging-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&staging)?;
        let staged_executable = staging.join(executable_name);
        let staged_manifest = staging.join("engine_manifest.json");
        let stage_result = (|| -> Result<()> {
            std::fs::copy(&canonical_source, &staged_executable)?;
            write_all_synced(&staged_manifest, &manifest_bytes)?;
            let (version, hash, bytes) = verified_identity(&staged_executable)?;
            if version != manifest.version
                || bytes != manifest.file_bytes
                || !hash.eq_ignore_ascii_case(&manifest.sha256_hex)
            {
                return Err(EngineError::InstallFailed(
                    "download engine changed while staging its immutable generation".to_string(),
                ));
            }
            compatibility_probe(&staged_executable)?;
            std::fs::rename(&staging, &generation)?;
            Ok(())
        })();
        if stage_result.is_err() && staging.exists() {
            let _ = std::fs::remove_dir_all(&staging);
        }
        stage_result?;
    }

    resolve_selection(paths, &selection)?;
    if kind == DownloadEngineKind::ManagedYtDlp {
        write_json_atomically(&packaged_fallback_path(paths), &selection)?;
    }
    if activate {
        activate_selection(paths, &selection)?;
    }
    Ok((engine_status(paths), selection))
}

pub fn stage_and_activate_ytdlp(
    paths: &AppPaths,
    source: &Path,
    expected_version: Option<&str>,
) -> Result<DownloadEngineStatus> {
    let _guard = mutation_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    stage_engine(
        paths,
        source,
        DownloadEngineKind::ManagedYtDlp,
        expected_version,
        true,
    )
    .map(|(status, _)| status)
}

pub fn set_custom_engine(paths: &AppPaths, source: &Path) -> Result<DownloadEngineStatus> {
    let _guard = mutation_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    stage_engine(
        paths,
        source,
        DownloadEngineKind::CustomExecutable,
        None,
        true,
    )
    .map(|(status, _)| status)
}

pub fn rollback_engine(paths: &AppPaths) -> Result<DownloadEngineStatus> {
    let _guard = mutation_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let previous = read_selection(&previous_selection_path(paths))?;
    resolve_selection(paths, &previous)?;
    let current = current_selection(paths)?;
    write_json_atomically(&current_selection_path(paths), &previous)?;
    write_json_atomically(&previous_selection_path(paths), &current)?;
    Ok(engine_status(paths))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UpdateCheckCache {
    checked_at_ms: i64,
    version: String,
    download_url: String,
    sha256_hex: String,
    file_bytes: u64,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    assets: Vec<GithubReleaseAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubReleaseAsset {
    name: String,
    browser_download_url: String,
    size: u64,
    digest: Option<String>,
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

fn http_agent(timeout: Duration) -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .build();
    ureq::Agent::new_with_config(config)
}

fn fetch_latest_release() -> Result<UpdateCheckCache> {
    let response = http_agent(Duration::from_secs(30))
        .get(YT_DLP_LATEST_RELEASE_URL)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "VoxVulgi")
        .call()
        .map_err(|error| {
            EngineError::InstallFailed(format!("yt-dlp update check failed: {error}"))
        })?;
    let release: GithubRelease = serde_json::from_reader(response.into_body().into_reader())?;
    let asset = release
        .assets
        .into_iter()
        .find(|asset| asset.name == "yt-dlp.exe")
        .ok_or_else(|| {
            EngineError::InstallFailed(
                "latest yt-dlp release does not contain yt-dlp.exe".to_string(),
            )
        })?;
    if !asset
        .browser_download_url
        .starts_with("https://github.com/yt-dlp/yt-dlp/releases/download/")
    {
        return Err(EngineError::InstallFailed(
            "latest yt-dlp asset URL is outside the official release origin".to_string(),
        ));
    }
    let digest = asset.digest.unwrap_or_default();
    let sha256_hex = digest
        .strip_prefix("sha256:")
        .unwrap_or_default()
        .to_ascii_uppercase();
    validate_sha256(&sha256_hex, "release asset digest")?;
    if asset.size == 0 {
        return Err(EngineError::InstallFailed(
            "latest yt-dlp asset has an invalid zero-byte size".to_string(),
        ));
    }
    let cache = UpdateCheckCache {
        checked_at_ms: now_ms(),
        version: release.tag_name.trim_start_matches('v').to_string(),
        download_url: asset.browser_download_url,
        sha256_hex,
        file_bytes: asset.size,
    };
    validate_update_cache(&cache)?;
    Ok(cache)
}

fn validate_update_cache(cache: &UpdateCheckCache) -> Result<()> {
    validate_token(&cache.version, "release version")?;
    validate_sha256(&cache.sha256_hex, "release asset digest")?;
    if cache.file_bytes == 0 {
        return Err(EngineError::InstallFailed(
            "yt-dlp release asset has an invalid zero-byte size".to_string(),
        ));
    }
    let expected_url = format!(
        "https://github.com/yt-dlp/yt-dlp/releases/download/{}/yt-dlp.exe",
        cache.version
    );
    let expected_v_url = format!(
        "https://github.com/yt-dlp/yt-dlp/releases/download/v{}/yt-dlp.exe",
        cache.version
    );
    if cache.download_url != expected_url && cache.download_url != expected_v_url {
        return Err(EngineError::InstallFailed(
            "yt-dlp release asset URL does not exactly match its official versioned release"
                .to_string(),
        ));
    }
    let current_time = now_ms();
    if cache.checked_at_ms <= 0
        || cache.checked_at_ms > current_time.saturating_add(UPDATE_CACHE_FUTURE_SKEW_MS)
    {
        return Err(EngineError::InstallFailed(
            "yt-dlp release cache timestamp is invalid or implausibly in the future".to_string(),
        ));
    }
    Ok(())
}

fn cached_or_latest_release(paths: &AppPaths, force: bool) -> Result<UpdateCheckCache> {
    let cache_path = update_cache_path(paths);
    if !force && cache_path.exists() {
        if let Ok(cache) = serde_json::from_slice::<UpdateCheckCache>(&std::fs::read(&cache_path)?)
        {
            if now_ms().saturating_sub(cache.checked_at_ms) < UPDATE_CACHE_TTL_MS
                && validate_update_cache(&cache).is_ok()
            {
                return Ok(cache);
            }
        }
    }
    let release = fetch_latest_release()?;
    write_json_atomically(&cache_path, &release)?;
    Ok(release)
}

fn download_release_candidate(paths: &AppPaths, release: &UpdateCheckCache) -> Result<PathBuf> {
    let download_dir = engine_root(paths).join("downloads");
    std::fs::create_dir_all(&download_dir)?;
    let candidate = download_dir.join(format!("yt-dlp-{}.download", uuid::Uuid::new_v4()));
    let result = (|| -> Result<()> {
        let response = http_agent(Duration::from_secs(10 * 60))
            .get(&release.download_url)
            .header("User-Agent", "VoxVulgi")
            .call()
            .map_err(|error| {
                EngineError::InstallFailed(format!("yt-dlp update download failed: {error}"))
            })?;
        let mut reader = response.into_body().into_reader();
        let mut file = File::create(&candidate)?;
        std::io::copy(&mut reader, &mut file)?;
        file.sync_all()?;
        let bytes = std::fs::metadata(&candidate)?.len();
        if bytes != release.file_bytes {
            return Err(EngineError::SizeMismatch {
                path: candidate.clone(),
                expected: release.file_bytes,
                actual: bytes,
            });
        }
        let hash = sha256_file(&candidate)?;
        if !hash.eq_ignore_ascii_case(&release.sha256_hex) {
            return Err(EngineError::HashMismatch {
                path: candidate.clone(),
                expected: release.sha256_hex.clone(),
                actual: hash,
            });
        }
        Ok(())
    })();
    if let Err(error) = result {
        let _ = std::fs::remove_file(&candidate);
        return Err(error);
    }
    Ok(candidate)
}

fn selected_is_custom(paths: &AppPaths) -> bool {
    let Ok(selection) = current_selection(paths) else {
        return false;
    };
    selection.engine_id.starts_with("custom-")
}

fn update_inner(paths: &AppPaths, force: bool) -> Result<(bool, String)> {
    let release = cached_or_latest_release(paths, force)?;
    let selected_custom = selected_is_custom(paths);
    if let Ok(engine) = verify_packaged(paths) {
        if engine.version == release.version
            && engine.file_bytes == release.file_bytes
            && engine.sha256_hex.eq_ignore_ascii_case(&release.sha256_hex)
        {
            return Ok((false, release.version));
        }
    }
    if let Ok(fallback) = read_selection(&packaged_fallback_path(paths)) {
        if let Ok(engine) = verify_generation(paths, &fallback) {
            if engine.kind == DownloadEngineKind::ManagedYtDlp
                && engine.version == release.version
                && engine.file_bytes == release.file_bytes
                && engine.sha256_hex.eq_ignore_ascii_case(&release.sha256_hex)
            {
                if !selected_custom {
                    let activation_changed = current_selection(paths)
                        .map(|current| current != fallback)
                        .unwrap_or(true);
                    if activation_changed {
                        activate_selection(paths, &fallback)?;
                    }
                    return Ok((activation_changed, release.version));
                }
                return Ok((false, release.version));
            }
        }
    }
    let candidate = download_release_candidate(paths, &release)?;
    let stage_result = stage_engine(
        paths,
        &candidate,
        DownloadEngineKind::ManagedYtDlp,
        Some(&release.version),
        !selected_custom,
    );
    let _ = std::fs::remove_file(&candidate);
    stage_result?;
    Ok((true, release.version))
}

/// Bounded startup-safe update check. Network, release, compatibility, and activation failures
/// are returned in the receipt and never make application startup fail.
pub fn check_and_update_packaged_ytdlp(
    paths: &AppPaths,
    force: bool,
) -> DownloadEngineUpdateResult {
    let _guard = mutation_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    match update_inner(paths, force) {
        Ok((updated, latest_version)) => DownloadEngineUpdateResult {
            checked: true,
            updated,
            latest_version: Some(latest_version),
            error: None,
            status: engine_status(paths),
        },
        Err(error) => DownloadEngineUpdateResult {
            checked: false,
            updated: false,
            latest_version: None,
            error: Some(error.to_string()),
            status: engine_status(paths),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_paths(temp: &TempDir) -> AppPaths {
        AppPaths::new(temp.path().join("app_data"))
    }

    fn fake_engine(temp: &TempDir, name: &str, version: &str) -> PathBuf {
        let source = temp.path().join(format!("{name}.rs"));
        let mut path = temp.path().join(name);
        if cfg!(windows) {
            path.set_extension("exe");
        }
        let literal = format!("{version:?}");
        std::fs::write(
            &source,
            format!(
                r#"fn main() {{
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|arg| arg == "--version") {{
        println!({literal});
        return;
    }}
    if !args.iter().any(|arg| arg == "--enable-file-urls") {{
        std::process::exit(2);
    }}
    let url = args.iter().find(|arg| arg.starts_with("file://")).cloned().unwrap_or_default();
    println!("{{{{\"webpage_url\":{{:?}}}}}}", url);
}}"#
            ),
        )
        .unwrap();
        let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
        let status = std::process::Command::new(rustc)
            .args([source.as_os_str(), "-o".as_ref(), path.as_os_str()])
            .status()
            .unwrap();
        assert!(status.success());
        path
    }

    #[test]
    fn custom_engine_is_snapshotted_and_reverified() {
        let temp = TempDir::new().unwrap();
        let paths = test_paths(&temp);
        let source = fake_engine(&temp, "custom_engine", "rust-downloader 0.1.0");
        let status = set_custom_engine(&paths, &source).unwrap();
        assert!(status.verified);
        assert_eq!(status.kind, DownloadEngineKind::CustomExecutable);
        let selected_path = PathBuf::from(&status.path);
        assert_ne!(selected_path, source);

        std::fs::write(&source, "changed source after selection").unwrap();
        let still_selected = resolve_selected_engine(&paths).unwrap();
        assert_eq!(still_selected.version, "rust-downloader 0.1.0");

        std::fs::write(&selected_path, "tampered generation").unwrap();
        assert!(!engine_status(&paths).verified);
    }

    #[test]
    fn activation_and_rollback_swap_verified_generations() {
        let temp = TempDir::new().unwrap();
        let paths = test_paths(&temp);
        let first = fake_engine(&temp, "first", "custom 1");
        let second = fake_engine(&temp, "second", "custom 2");
        let first_status = set_custom_engine(&paths, &first).unwrap();
        let second_status = set_custom_engine(&paths, &second).unwrap();
        assert_ne!(
            first_status.selected_engine_id,
            second_status.selected_engine_id
        );

        let rolled_back = rollback_engine(&paths).unwrap();
        assert_eq!(
            rolled_back.selected_engine_id,
            first_status.selected_engine_id
        );
        assert_eq!(rolled_back.version.as_deref(), Some("custom 1"));

        let rolled_forward = rollback_engine(&paths).unwrap();
        assert_eq!(
            rolled_forward.selected_engine_id,
            second_status.selected_engine_id
        );
    }

    #[test]
    fn managed_update_does_not_displace_a_custom_selection() {
        let temp = TempDir::new().unwrap();
        let paths = test_paths(&temp);
        let custom = fake_engine(&temp, "custom_selected", "rust-downloader 0.2.0");
        let custom_status = set_custom_engine(&paths, &custom).unwrap();
        let managed = fake_engine(&temp, "managed_update", "2026.08.19");
        let (status_after_stage, fallback) = stage_engine(
            &paths,
            &managed,
            DownloadEngineKind::ManagedYtDlp,
            Some("2026.08.19"),
            !selected_is_custom(&paths),
        )
        .unwrap();

        assert_eq!(
            status_after_stage.selected_engine_id,
            custom_status.selected_engine_id
        );
        assert_eq!(
            resolve_selection(&paths, &fallback).unwrap().kind,
            DownloadEngineKind::ManagedYtDlp
        );
    }

    #[test]
    fn invalid_selection_does_not_fall_back_silently() {
        let temp = TempDir::new().unwrap();
        let paths = test_paths(&temp);
        std::fs::create_dir_all(engine_root(&paths)).unwrap();
        std::fs::write(current_selection_path(&paths), b"not json").unwrap();
        let status = engine_status(&paths);
        assert!(!status.verified);
        assert!(status.error.unwrap().contains("json error"));
    }

    #[test]
    fn explicit_selection_repairs_corrupt_current_state() {
        let temp = TempDir::new().unwrap();
        let paths = test_paths(&temp);
        let custom = fake_engine(&temp, "recover_selection", "custom recovery 1");
        let selected = set_custom_engine(&paths, &custom).unwrap();
        std::fs::write(current_selection_path(&paths), b"not json").unwrap();

        let repaired = select_engine(&paths, &selected.selected_engine_id).unwrap();
        assert!(repaired.verified);
        assert_eq!(repaired.selected_engine_id, selected.selected_engine_id);
    }

    #[test]
    fn forged_cached_release_url_is_rejected_before_download() {
        let cache = UpdateCheckCache {
            checked_at_ms: now_ms(),
            version: "2026.08.19".to_string(),
            download_url: "https://attacker.invalid/yt-dlp.exe".to_string(),
            sha256_hex: "A".repeat(64),
            file_bytes: 1024,
        };
        let error = validate_update_cache(&cache).unwrap_err();
        assert!(error
            .to_string()
            .contains("does not exactly match its official versioned release"));
    }

    #[test]
    fn cached_release_rejects_version_url_mismatch_and_future_timestamp() {
        let mut cache = UpdateCheckCache {
            checked_at_ms: now_ms(),
            version: "2026.08.19".to_string(),
            download_url:
                "https://github.com/yt-dlp/yt-dlp/releases/download/2026.07.04/yt-dlp.exe"
                    .to_string(),
            sha256_hex: "A".repeat(64),
            file_bytes: 1024,
        };
        assert!(validate_update_cache(&cache).is_err());
        cache.download_url =
            "https://github.com/yt-dlp/yt-dlp/releases/download/2026.08.19/yt-dlp.exe".to_string();
        cache.checked_at_ms = now_ms().saturating_add(UPDATE_CACHE_FUTURE_SKEW_MS + 1);
        assert!(validate_update_cache(&cache).is_err());
    }

    #[test]
    fn verified_identity_reuses_cache_until_size_or_mtime_changes() {
        // WP-0322: the identity of an unchanged executable must not re-launch it; any change to
        // size or modification time must force a real re-verification.
        let dir = tempfile::tempdir().expect("tempdir");
        let exe = dir.path().join("not-really-yt-dlp.exe");
        std::fs::write(&exe, b"fixture bytes").expect("write fixture");
        let metadata = std::fs::metadata(&exe).expect("metadata");
        verified_identity_cache()
            .lock()
            .unwrap()
            .insert(
                exe.clone(),
                (
                    metadata.len(),
                    metadata.modified().expect("mtime"),
                    "2099.01.01".to_string(),
                    "ABC".to_string(),
                ),
            );
        let cached = verified_identity(&exe).expect("unchanged file served from cache");
        assert_eq!(cached, ("2099.01.01".to_string(), "ABC".to_string(), metadata.len()));

        std::fs::write(&exe, b"fixture bytes, changed").expect("change fixture");
        assert!(
            verified_identity(&exe).is_err(),
            "a changed file must be re-verified (this fixture is not a runnable engine)"
        );
    }
}
