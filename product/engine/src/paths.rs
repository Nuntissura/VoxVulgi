use crate::persistence;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::time::Duration;

const BOUNDED_PATH_PROBE_WORKERS: usize = 4;
const BOUNDED_PATH_PROBE_QUEUE_CAPACITY: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundedPathKind {
    File,
    Directory,
    Missing,
    Unreachable,
}

struct BoundedPathProbeRequest {
    path: PathBuf,
    reply: mpsc::SyncSender<BoundedPathKind>,
    #[cfg(test)]
    artificial_delay: Option<Duration>,
}

static BOUNDED_PATH_PROBE_POOL: OnceLock<mpsc::SyncSender<BoundedPathProbeRequest>> =
    OnceLock::new();
#[cfg(test)]
static BOUNDED_PATH_PROBE_THREADS_STARTED: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

fn bounded_path_probe_pool() -> &'static mpsc::SyncSender<BoundedPathProbeRequest> {
    BOUNDED_PATH_PROBE_POOL.get_or_init(|| {
        let (sender, receiver) =
            mpsc::sync_channel::<BoundedPathProbeRequest>(BOUNDED_PATH_PROBE_QUEUE_CAPACITY);
        let receiver = Arc::new(Mutex::new(receiver));
        for index in 0..BOUNDED_PATH_PROBE_WORKERS {
            let receiver = Arc::clone(&receiver);
            std::thread::Builder::new()
                .name(format!("voxvulgi-path-probe-{index}"))
                .spawn(move || {
                    #[cfg(test)]
                    BOUNDED_PATH_PROBE_THREADS_STARTED
                        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    loop {
                        let request = receiver
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            .recv();
                        let Ok(request) = request else {
                            break;
                        };
                        #[cfg(test)]
                        if let Some(delay) = request.artificial_delay {
                            std::thread::sleep(delay);
                        }
                        let kind = match std::fs::metadata(&request.path) {
                            Ok(metadata) if metadata.is_dir() => BoundedPathKind::Directory,
                            Ok(metadata) if metadata.is_file() => BoundedPathKind::File,
                            Ok(_) => BoundedPathKind::Missing,
                            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                                BoundedPathKind::Missing
                            }
                            Err(_) => BoundedPathKind::Unreachable,
                        };
                        let _ = request.reply.try_send(kind);
                    }
                })
                .expect("bounded path probe worker must start");
        }
        sender
    })
}

fn probe_path_bounded_internal(
    path: &Path,
    timeout: Duration,
    #[cfg(test)] artificial_delay: Option<Duration>,
) -> BoundedPathKind {
    let (reply, receiver) = mpsc::sync_channel(1);
    let request = BoundedPathProbeRequest {
        path: path.to_path_buf(),
        reply,
        #[cfg(test)]
        artificial_delay,
    };
    if bounded_path_probe_pool().try_send(request).is_err() {
        return BoundedPathKind::Unreachable;
    }
    receiver
        .recv_timeout(timeout)
        .unwrap_or(BoundedPathKind::Unreachable)
}

pub fn probe_path_bounded(path: &Path, timeout: Duration) -> BoundedPathKind {
    probe_path_bounded_internal(
        path,
        timeout,
        #[cfg(test)]
        None,
    )
}

pub const MANAGED_RUNTIME_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeMode {
    Isolated,
    LegacyAppData,
    ManagedOffline,
    ManagedOfflineInvalid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeActivationPointer {
    pub schema_version: u32,
    pub runtime_id: String,
    pub manifest_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeComponentManifest {
    pub root: String,
    pub archive_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeRequiredFile {
    pub path: String,
    #[serde(default)]
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManagedRuntimeManifest {
    pub schema_version: u32,
    pub runtime_id: String,
    pub compatible_app_versions: Vec<String>,
    pub qualification_id: String,
    pub components: BTreeMap<String, RuntimeComponentManifest>,
    pub required_files: Vec<RuntimeRequiredFile>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeProvenance {
    pub mode: RuntimeMode,
    pub user_data_root: String,
    pub runtime_root: String,
    pub generation_root: String,
    pub runtime_id: Option<String>,
    pub manifest_sha256: Option<String>,
    pub manifest_status: String,
    pub fallback_policy: String,
}

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub base_dir: PathBuf,
    runtime_root: PathBuf,
    runtime_payload_dir: PathBuf,
    runtime_mode: RuntimeMode,
    runtime_id: Option<String>,
    runtime_manifest_sha256: Option<String>,
    runtime_manifest_status: String,
}

impl AppPaths {
    pub fn new(base_dir: PathBuf) -> Self {
        Self {
            runtime_root: base_dir.clone(),
            runtime_payload_dir: base_dir.clone(),
            base_dir,
            runtime_mode: RuntimeMode::Isolated,
            runtime_id: None,
            runtime_manifest_sha256: None,
            runtime_manifest_status: "isolated".to_string(),
        }
    }

    pub fn installed(
        base_dir: PathBuf,
        runtime_root: PathBuf,
        app_version: &str,
    ) -> Result<Self, String> {
        let pointer_path = runtime_root.join("current.json");
        if !pointer_path.exists() {
            return Ok(Self {
                runtime_payload_dir: base_dir.clone(),
                base_dir,
                runtime_root,
                runtime_mode: RuntimeMode::LegacyAppData,
                runtime_id: None,
                runtime_manifest_sha256: None,
                runtime_manifest_status: "legacy_no_pointer".to_string(),
            });
        }

        let pointer: RuntimeActivationPointer = serde_json::from_slice(
            &std::fs::read(&pointer_path)
                .map_err(|error| format!("failed to read managed runtime pointer: {error}"))?,
        )
        .map_err(|error| format!("managed runtime pointer is invalid JSON: {error}"))?;
        if pointer.schema_version != MANAGED_RUNTIME_SCHEMA_VERSION {
            return Err(format!(
                "managed runtime pointer schema {} is unsupported",
                pointer.schema_version
            ));
        }
        validate_runtime_token(&pointer.runtime_id, "runtime_id")?;
        validate_sha256(&pointer.manifest_sha256, "manifest_sha256")?;

        let generation_root = runtime_root.join("generations").join(&pointer.runtime_id);
        let manifest_path = generation_root.join("runtime_manifest.json");
        let manifest_bytes = std::fs::read(&manifest_path).map_err(|error| {
            format!(
                "selected managed runtime manifest cannot be read at {}: {error}",
                manifest_path.display()
            )
        })?;
        let observed_manifest_sha256 = hex::encode(Sha256::digest(&manifest_bytes));
        if !observed_manifest_sha256.eq_ignore_ascii_case(&pointer.manifest_sha256) {
            return Err(format!(
                "selected managed runtime manifest hash mismatch (expected={}, observed={})",
                pointer.manifest_sha256, observed_manifest_sha256
            ));
        }
        let manifest: ManagedRuntimeManifest = serde_json::from_slice(&manifest_bytes)
            .map_err(|error| format!("managed runtime manifest is invalid JSON: {error}"))?;
        validate_managed_runtime_manifest(
            &manifest,
            &pointer.runtime_id,
            app_version,
            &generation_root,
        )?;

        Ok(Self {
            base_dir,
            runtime_root,
            runtime_payload_dir: generation_root,
            runtime_mode: RuntimeMode::ManagedOffline,
            runtime_id: Some(pointer.runtime_id),
            runtime_manifest_sha256: Some(observed_manifest_sha256),
            runtime_manifest_status: "verified".to_string(),
        })
    }

    #[cfg(test)]
    pub(crate) fn managed_for_test(base_dir: PathBuf, generation_root: PathBuf) -> Self {
        let runtime_root = generation_root
            .parent()
            .and_then(Path::parent)
            .unwrap_or(&generation_root)
            .to_path_buf();
        Self {
            base_dir,
            runtime_root,
            runtime_payload_dir: generation_root,
            runtime_mode: RuntimeMode::ManagedOffline,
            runtime_id: Some("test-runtime".to_string()),
            runtime_manifest_sha256: Some("a".repeat(64)),
            runtime_manifest_status: "verified_test_fixture".to_string(),
        }
    }

    pub fn runtime_root(&self) -> &Path {
        &self.runtime_root
    }

    pub fn runtime_generation_root(&self) -> &Path {
        &self.runtime_payload_dir
    }

    /// Writable, app-managed download-engine generations. This intentionally lives with the
    /// installed runtime rather than retained user data (database/preferences).
    pub fn download_engines_dir(&self) -> PathBuf {
        self.runtime_root.join("download_engines")
    }

    pub fn runtime_mode(&self) -> RuntimeMode {
        self.runtime_mode
    }

    pub fn managed_offline(&self) -> bool {
        matches!(
            self.runtime_mode,
            RuntimeMode::ManagedOffline | RuntimeMode::ManagedOfflineInvalid
        )
    }

    pub fn invalid_managed(base_dir: PathBuf, runtime_root: PathBuf, error: String) -> Self {
        Self {
            base_dir,
            runtime_payload_dir: runtime_root.join("generations").join("_invalid"),
            runtime_root,
            runtime_mode: RuntimeMode::ManagedOfflineInvalid,
            runtime_id: None,
            runtime_manifest_sha256: None,
            runtime_manifest_status: format!("invalid: {error}"),
        }
    }

    pub fn runtime_provenance(&self) -> RuntimeProvenance {
        RuntimeProvenance {
            mode: self.runtime_mode,
            user_data_root: self.base_dir.to_string_lossy().to_string(),
            runtime_root: self.runtime_root.to_string_lossy().to_string(),
            generation_root: self.runtime_payload_dir.to_string_lossy().to_string(),
            runtime_id: self.runtime_id.clone(),
            manifest_sha256: self.runtime_manifest_sha256.clone(),
            manifest_status: self.runtime_manifest_status.clone(),
            fallback_policy: if self.managed_offline() {
                "managed_generation_only".to_string()
            } else {
                "legacy_developer_fallbacks_allowed".to_string()
            },
        }
    }

    pub fn config_dir(&self) -> PathBuf {
        self.base_dir.join("config")
    }

    pub fn glossary_path(&self) -> PathBuf {
        self.config_dir().join("glossary.json")
    }

    pub fn item_glossary_path(&self, item_id: &str) -> PathBuf {
        self.derived_item_dir(item_id).join("glossary.json")
    }

    pub fn item_translation_style_path(&self, item_id: &str) -> PathBuf {
        self.derived_item_dir(item_id)
            .join("translation_style.json")
    }

    pub fn item_localization_pipeline_preset_path(&self, item_id: &str) -> PathBuf {
        self.derived_item_dir(item_id)
            .join("localization_pipeline_preset.json")
    }

    pub fn voice_backend_adapters_path(&self) -> PathBuf {
        self.config_dir().join("voice_backend_adapters.json")
    }

    pub fn voice_backend_adapter_probes_path(&self) -> PathBuf {
        self.config_dir().join("voice_backend_adapter_probes.json")
    }

    pub fn library_dir(&self) -> PathBuf {
        self.base_dir.join("library")
    }

    pub fn derived_dir(&self) -> PathBuf {
        self.base_dir.join("derived")
    }

    pub fn voice_templates_dir(&self) -> PathBuf {
        self.base_dir.join("voice_templates")
    }

    pub fn voice_library_dir(&self) -> PathBuf {
        self.base_dir.join("voice_library")
    }

    pub fn voice_library_profile_dir(&self, profile_id: &str) -> PathBuf {
        self.voice_library_dir().join(profile_id)
    }

    pub fn voice_library_profile_refs_dir(&self, profile_id: &str) -> PathBuf {
        self.voice_library_profile_dir(profile_id)
            .join("references")
    }

    pub fn voice_template_dir(&self, template_id: &str) -> PathBuf {
        self.voice_templates_dir().join(template_id)
    }

    pub fn voice_template_profiles_dir(&self, template_id: &str) -> PathBuf {
        self.voice_template_dir(template_id).join("profiles")
    }

    pub fn derived_items_dir(&self) -> PathBuf {
        self.derived_dir().join("items")
    }

    pub fn derived_jobs_dir(&self) -> PathBuf {
        self.derived_dir().join("jobs")
    }

    pub fn derived_item_dir(&self, item_id: &str) -> PathBuf {
        self.derived_items_dir().join(item_id)
    }

    pub fn derived_item_voice_dir(&self, item_id: &str) -> PathBuf {
        self.derived_item_dir(item_id).join("voice")
    }

    pub fn job_artifacts_dir(&self, job_id: &str) -> PathBuf {
        self.derived_jobs_dir().join(job_id)
    }

    pub fn db_dir(&self) -> PathBuf {
        self.base_dir.join("db")
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.base_dir.join("logs")
    }

    pub fn job_logs_dir(&self) -> PathBuf {
        self.logs_dir().join("jobs")
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.base_dir.join("cache")
    }

    pub fn thumbnail_cache_dir(&self) -> PathBuf {
        self.cache_dir().join("thumbs")
    }

    pub fn secrets_dir(&self) -> PathBuf {
        self.base_dir.join("secrets")
    }

    pub fn job_secrets_dir(&self) -> PathBuf {
        self.secrets_dir().join("jobs")
    }

    pub fn job_cookie_secret_path(&self, job_id: &str) -> PathBuf {
        self.job_secrets_dir().join(format!("{job_id}.cookie.txt"))
    }

    pub fn subscription_secrets_dir(&self) -> PathBuf {
        self.secrets_dir().join("subscriptions")
    }

    pub fn youtube_subscription_secrets_dir(&self) -> PathBuf {
        self.subscription_secrets_dir().join("youtube")
    }

    pub fn subscription_state_dir(&self) -> PathBuf {
        self.library_dir().join("subscriptions")
    }

    pub fn youtube_subscription_state_dir(&self) -> PathBuf {
        self.subscription_state_dir().join("youtube")
    }

    pub fn youtube_subscription_state_item_dir(&self, subscription_id: &str) -> PathBuf {
        self.youtube_subscription_state_dir().join(subscription_id)
    }

    pub fn youtube_subscription_archive_state_path(&self, subscription_id: &str) -> PathBuf {
        self.youtube_subscription_state_item_dir(subscription_id)
            .join("voxvulgi_youtube_archive.txt")
    }

    pub fn instagram_subscription_secrets_dir(&self) -> PathBuf {
        self.subscription_secrets_dir().join("instagram")
    }

    pub fn youtube_subscription_cookie_secret_path(&self, subscription_id: &str) -> PathBuf {
        self.youtube_subscription_secrets_dir()
            .join(format!("{subscription_id}.cookie.txt"))
    }

    pub fn instagram_subscription_cookie_secret_path(&self, subscription_id: &str) -> PathBuf {
        self.instagram_subscription_secrets_dir()
            .join(format!("{subscription_id}.cookie.txt"))
    }

    /// WP-0263: the app-global Instagram login pasted once in Options and used for every
    /// Instagram operation (single download, subscription refresh, batch). Mirrors the
    /// YouTube global auth (`youtube_auth_config_path`), but is stored as a plain cookie
    /// secret file under the Instagram secrets dir so it reuses the existing
    /// `read_auth_cookie_secret_path` / `write_auth_cookie_secret_path` handling.
    pub fn instagram_global_auth_cookie_secret_path(&self) -> PathBuf {
        self.instagram_subscription_secrets_dir()
            .join("_global.cookie.txt")
    }

    pub fn download_dir_override_path(&self) -> PathBuf {
        self.config_dir().join("download_dir.txt")
    }

    /// WP-0322 A: default subscription export folder, disk-agnostic (relative to `base_dir`,
    /// same as every other app-data-relative path here).
    pub fn default_subscriptions_export_dir(&self) -> PathBuf {
        self.base_dir.join("exports").join("subscriptions")
    }

    pub fn subscriptions_export_dir_override_path(&self) -> PathBuf {
        self.config_dir().join("subscriptions_export_dir.txt")
    }

    /// WP-0322 A: last export time/path/count/error, so Options can show them without
    /// re-scanning the export folder.
    pub fn subscriptions_export_state_path(&self) -> PathBuf {
        self.config_dir().join("subscriptions_export_state.json")
    }

    pub fn subscriptions_export_dir_override(&self) -> std::io::Result<Option<PathBuf>> {
        let path = self.subscriptions_export_dir_override_path();
        if !path.exists() {
            return Ok(None);
        }
        let raw = std::fs::read_to_string(path)?;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        Ok(Some(PathBuf::from(trimmed)))
    }

    pub fn effective_subscriptions_export_dir(&self) -> std::io::Result<PathBuf> {
        if let Some(override_dir) = self.subscriptions_export_dir_override()? {
            return Ok(override_dir);
        }
        Ok(self.default_subscriptions_export_dir())
    }

    pub fn set_subscriptions_export_dir_override(&self, dir: &Path) -> std::io::Result<()> {
        let path = self.subscriptions_export_dir_override_path();
        let text = format!("{}\n", dir.to_string_lossy());
        persistence::atomic_write_text(&path, &text)?;
        Ok(())
    }

    pub fn clear_subscriptions_export_dir_override(&self) -> std::io::Result<()> {
        let path = self.subscriptions_export_dir_override_path();
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    pub fn python_exe_override_path(&self) -> PathBuf {
        self.config_dir().join("python_exe.txt")
    }

    pub fn diagnostics_trace_dir_override_path(&self) -> PathBuf {
        self.config_dir().join("diagnostics_trace_dir.txt")
    }

    pub fn legacy_diagnostics_trace_override_path(&self) -> PathBuf {
        self.config_dir().join("codex_diagnostics_dir.txt")
    }

    pub fn default_diagnostics_trace_dir(&self) -> PathBuf {
        self.base_dir.join("diagnostics").join("traces")
    }

    pub fn diagnostics_trace_dir_override(&self) -> std::io::Result<Option<PathBuf>> {
        for path in [
            self.diagnostics_trace_dir_override_path(),
            self.legacy_diagnostics_trace_override_path(),
        ] {
            if !path.exists() {
                continue;
            }
            let raw = std::fs::read_to_string(path)?;
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                continue;
            }
            return Ok(Some(PathBuf::from(trimmed)));
        }
        Ok(None)
    }

    pub fn effective_diagnostics_trace_dir(&self) -> std::io::Result<PathBuf> {
        if let Some(override_dir) = self.diagnostics_trace_dir_override()? {
            return Ok(override_dir);
        }
        Ok(self.default_diagnostics_trace_dir())
    }

    pub fn set_diagnostics_trace_dir_override(&self, dir: &Path) -> std::io::Result<()> {
        let path = self.diagnostics_trace_dir_override_path();
        let text = format!("{}\n", dir.to_string_lossy());
        persistence::atomic_write_text(&path, &text)?;
        let legacy = self.legacy_diagnostics_trace_override_path();
        if legacy.exists() {
            std::fs::remove_file(legacy)?;
        }
        Ok(())
    }

    pub fn clear_diagnostics_trace_dir_override(&self) -> std::io::Result<()> {
        for path in [
            self.diagnostics_trace_dir_override_path(),
            self.legacy_diagnostics_trace_override_path(),
        ] {
            if path.exists() {
                std::fs::remove_file(path)?;
            }
        }
        Ok(())
    }

    pub fn python_exe_override(&self) -> std::io::Result<Option<PathBuf>> {
        let path = self.python_exe_override_path();
        if !path.exists() {
            return Ok(None);
        }

        let raw = std::fs::read_to_string(path)?;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }

        Ok(Some(PathBuf::from(trimmed)))
    }

    pub fn set_python_exe_override(&self, exe_path: &Path) -> std::io::Result<()> {
        let path = self.python_exe_override_path();
        let text = format!("{}\n", exe_path.to_string_lossy());
        persistence::atomic_write_text(&path, &text)?;
        Ok(())
    }

    pub fn clear_python_exe_override(&self) -> std::io::Result<()> {
        let path = self.python_exe_override_path();
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    /// Default ASR model id used when no `config/asr_model_id.txt` override is set.
    /// Korean/Japanese-first: large-v3 q5_0 is the quality default (tiny is unusable
    /// for Korean). Swappable at runtime by writing a model id into the override file
    /// (e.g. `whispercpp-large-v3` for max quality, `whispercpp-tiny` to revert fast).
    pub const DEFAULT_ASR_MODEL_ID: &'static str = "whispercpp-large-v3-q5_0";

    pub fn asr_model_id_override_path(&self) -> PathBuf {
        self.config_dir().join("asr_model_id.txt")
    }

    pub fn asr_model_id_override(&self) -> std::io::Result<Option<String>> {
        let path = self.asr_model_id_override_path();
        if !path.exists() {
            return Ok(None);
        }
        let raw = std::fs::read_to_string(path)?;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        Ok(Some(trimmed.to_string()))
    }

    /// The ASR model id every transcription/translation enqueue site should use, so the
    /// model is selected in exactly one place and is operator-swappable without a rebuild.
    pub fn effective_asr_model_id(&self) -> String {
        match self.asr_model_id_override() {
            Ok(Some(id)) => id,
            _ => Self::DEFAULT_ASR_MODEL_ID.to_string(),
        }
    }

    pub fn set_asr_model_id_override(&self, model_id: &str) -> std::io::Result<()> {
        let path = self.asr_model_id_override_path();
        let text = format!("{}\n", model_id.trim());
        persistence::atomic_write_text(&path, &text)?;
        Ok(())
    }

    pub fn clear_asr_model_id_override(&self) -> std::io::Result<()> {
        let path = self.asr_model_id_override_path();
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    /// Operator override for the default voice-clone dub backend (e.g. `cosyvoice` or
    /// `openvoice_v2`). When unset, the engine prefers CosyVoice when its pack is
    /// installed and falls back to Kokoro+OpenVoice. Swappable without a rebuild.
    pub fn dub_backend_id_override_path(&self) -> PathBuf {
        self.config_dir().join("dub_backend_id.txt")
    }

    pub fn dub_backend_id_override(&self) -> std::io::Result<Option<String>> {
        let path = self.dub_backend_id_override_path();
        if !path.exists() {
            return Ok(None);
        }
        let raw = std::fs::read_to_string(path)?;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        Ok(Some(trimmed.to_string()))
    }

    pub fn set_dub_backend_id_override(&self, backend_id: &str) -> std::io::Result<()> {
        let path = self.dub_backend_id_override_path();
        let text = format!("{}\n", backend_id.trim());
        persistence::atomic_write_text(&path, &text)?;
        Ok(())
    }

    pub fn clear_dub_backend_id_override(&self) -> std::io::Result<()> {
        let path = self.dub_backend_id_override_path();
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    pub fn default_download_dir(&self) -> PathBuf {
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(parent) = exe_path.parent() {
                return parent.join("downloads");
            }
        }
        self.library_dir().join("downloads")
    }

    pub fn download_dir_override(&self) -> std::io::Result<Option<PathBuf>> {
        let path = self.download_dir_override_path();
        if !path.exists() {
            return Ok(None);
        }

        let raw = std::fs::read_to_string(path)?;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }

        Ok(Some(PathBuf::from(trimmed)))
    }

    pub fn effective_download_dir(&self) -> std::io::Result<PathBuf> {
        if let Some(override_dir) = self.download_dir_override()? {
            return Ok(override_dir);
        }
        Ok(self.default_download_dir())
    }

    pub fn set_download_dir_override(&self, dir: &Path) -> std::io::Result<()> {
        let path = self.download_dir_override_path();
        let text = format!("{}\n", dir.to_string_lossy());
        persistence::atomic_write_text(&path, &text)?;
        Ok(())
    }

    pub fn clear_download_dir_override(&self) -> std::io::Result<()> {
        let path = self.download_dir_override_path();
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    /// WP-0252 Item 2d: a stable, ALWAYS-local download folder used when the configured
    /// destination (e.g. a NAS share) is unreachable. Lives under app-data so it exists
    /// regardless of where the app is installed, and the user's NAS library is never lost
    /// — fallback items simply live here until the operator moves them back.
    pub fn local_fallback_download_dir(&self) -> PathBuf {
        self.base_dir.join("local_fallback_downloads")
    }

    /// The download destination to use right now: the configured dir if its root is
    /// reachable within a short bounded probe, otherwise the local fallback. The bool is
    /// `true` when the fallback was used (so callers can record it for later resync).
    /// Additive and non-destructive: never moves or deletes existing files.
    pub fn effective_download_dir_with_fallback(&self) -> std::io::Result<(PathBuf, bool)> {
        let configured = self.effective_download_dir()?;
        if download_root_reachable(&configured, std::time::Duration::from_secs(3)) {
            return Ok((configured, false));
        }
        let fallback = self.local_fallback_download_dir();
        std::fs::create_dir_all(&fallback)?;
        Ok((fallback, true))
    }

    pub fn models_dir(&self) -> PathBuf {
        self.runtime_payload_dir.join("models")
    }

    pub fn tools_dir(&self) -> PathBuf {
        self.runtime_payload_dir.join("tools")
    }

    pub fn huggingface_cache_dir(&self) -> PathBuf {
        if self.managed_offline() {
            self.runtime_payload_dir.join("cache").join("huggingface")
        } else {
            self.cache_dir().join("huggingface")
        }
    }

    pub fn instagram_profile_provider_dir(&self) -> PathBuf {
        self.tools_dir().join("instagram_profile_provider")
    }

    pub fn instagram_profile_provider_exe(&self) -> PathBuf {
        let mut path = self.instagram_profile_provider_dir().join("instaloader");
        if cfg!(windows) {
            path.set_extension("exe");
        }
        path
    }

    pub fn instagram_profile_enumerator_script(&self) -> PathBuf {
        self.instagram_profile_provider_dir()
            .join("instagram_profile_enumerator.py")
    }

    pub fn python_toolchain_dir(&self) -> PathBuf {
        self.tools_dir().join("python")
    }

    pub fn js_runtime_dir(&self) -> PathBuf {
        self.tools_dir().join("js_runtime")
    }

    pub fn deno_dir(&self) -> PathBuf {
        self.js_runtime_dir().join("deno")
    }

    pub fn deno_exe(&self) -> PathBuf {
        let mut path = self.deno_dir().join("deno");
        if cfg!(windows) {
            path.set_extension("exe");
        }
        path
    }

    pub fn node_runtime_dir(&self) -> PathBuf {
        self.js_runtime_dir().join("node")
    }

    pub fn node_exe(&self) -> PathBuf {
        let mut path = self.node_runtime_dir().join("node");
        if cfg!(windows) {
            path.set_extension("exe");
        }
        path
    }

    pub fn node_npm_cmd(&self) -> PathBuf {
        let mut path = self.node_runtime_dir().join("npm");
        if cfg!(windows) {
            path.set_extension("cmd");
        }
        path
    }

    pub fn youtube_po_provider_dir(&self) -> PathBuf {
        self.tools_dir().join("youtube_po_provider")
    }

    pub fn youtube_po_provider_plugin_dir(&self) -> PathBuf {
        self.youtube_po_provider_dir().join("plugin")
    }

    pub fn youtube_po_provider_server_dir(&self) -> PathBuf {
        self.youtube_po_provider_dir().join("server")
    }

    pub fn youtube_po_provider_entrypoint(&self) -> PathBuf {
        self.youtube_po_provider_server_dir()
            .join("build")
            .join("main.js")
    }

    pub fn python_portable_dir(&self) -> PathBuf {
        self.python_toolchain_dir().join(if self.managed_offline() {
            "runtime_main"
        } else {
            "portable"
        })
    }

    pub fn python_portable_python_exe(&self) -> PathBuf {
        let mut path = self.python_portable_dir().join("python");
        if cfg!(windows) {
            path.set_extension("exe");
        }
        path
    }

    pub fn python_venv_dir(&self) -> PathBuf {
        self.python_toolchain_dir().join(if self.managed_offline() {
            "runtime_main"
        } else {
            "venv"
        })
    }

    pub fn python_models_dir(&self) -> PathBuf {
        self.python_toolchain_dir().join("models")
    }

    /// Isolated Python venv dedicated to CosyVoice (its torch==2.3.1 / numpy<2 stack
    /// conflicts with the main venv's torch 2.10, so it must live separately).
    pub fn python_cosyvoice_venv_dir(&self) -> PathBuf {
        self.python_toolchain_dir().join(if self.managed_offline() {
            "runtime_cosyvoice"
        } else {
            "venv_cosyvoice"
        })
    }

    pub fn voice_backends_dir(&self) -> PathBuf {
        self.runtime_payload_dir.join("voice_backends")
    }

    /// The vendored CosyVoice repo (provides `cosyvoice.*` on sys.path + the render wrapper).
    pub fn cosyvoice_backend_dir(&self) -> PathBuf {
        self.voice_backends_dir().join("cosyvoice")
    }

    /// Parent dir holding `CosyVoice2-0.5B/` (the offline-by-design model the render loads).
    pub fn cosyvoice_model_parent_dir(&self) -> PathBuf {
        self.cosyvoice_backend_dir().join("pretrained_models")
    }

    /// WP-0234: per-pack install-state journal directory.
    /// One JSON file per pack, recording the most recent install outcome so the next
    /// install can detect mid-crash or failed states and force a clean reinstall.
    pub fn python_install_state_dir(&self) -> PathBuf {
        self.python_toolchain_dir().join("install_state")
    }

    pub fn batch_on_import_rules_path(&self) -> PathBuf {
        self.config_dir().join("batch_on_import_rules.json")
    }

    pub fn localization_pipeline_presets_path(&self) -> PathBuf {
        self.config_dir().join("localization_pipeline_presets.json")
    }

    pub fn safe_mode_config_path(&self) -> PathBuf {
        self.config_dir().join("safe_mode.json")
    }

    pub fn download_presets_config_path(&self) -> PathBuf {
        self.config_dir().join("download_presets.json")
    }

    pub fn provider_transfer_settings_path(&self) -> PathBuf {
        self.config_dir().join("provider_transfer_settings.json")
    }

    pub fn feature_storage_roots_config_path(&self) -> PathBuf {
        self.config_dir().join("feature_storage_roots.json")
    }

    pub fn root_aliases_config_path(&self) -> PathBuf {
        self.config_dir().join("root_aliases.json")
    }

    pub fn root_rebind_receipts_dir(&self) -> PathBuf {
        self.config_dir().join("root_rebind_receipts")
    }

    pub fn root_rebind_backups_dir(&self) -> PathBuf {
        self.config_dir().join("root_rebind_backups")
    }

    pub fn queue_identity_backups_dir(&self) -> PathBuf {
        self.config_dir().join("queue_identity_backups")
    }

    pub fn youtube_auth_config_path(&self) -> PathBuf {
        self.config_dir().join("youtube_auth.json")
    }

    pub fn diarization_optional_backend_config_path(&self) -> PathBuf {
        self.config_dir().join("diarization_optional_backend.json")
    }

    pub fn diarization_optional_backend_token_path(&self) -> PathBuf {
        self.secrets_dir()
            .join("diarization_optional_backend_token.txt")
    }

    pub fn install_logs_dir(&self) -> PathBuf {
        self.logs_dir().join("install")
    }

    pub fn ffmpeg_dir(&self) -> PathBuf {
        self.tools_dir().join("ffmpeg")
    }

    pub fn ffmpeg_bin_path(&self) -> PathBuf {
        let mut path = self.ffmpeg_dir().join("ffmpeg");
        if cfg!(windows) {
            path.set_extension("exe");
        }
        if self.managed_offline() && !path.exists() {
            let mut nested = self.ffmpeg_dir().join("bin").join("ffmpeg");
            if cfg!(windows) {
                nested.set_extension("exe");
            }
            if nested.exists() {
                return nested;
            }
        }
        path
    }

    pub fn ffprobe_bin_path(&self) -> PathBuf {
        let mut path = self.ffmpeg_dir().join("ffprobe");
        if cfg!(windows) {
            path.set_extension("exe");
        }
        if self.managed_offline() && !path.exists() {
            let mut nested = self.ffmpeg_dir().join("bin").join("ffprobe");
            if cfg!(windows) {
                nested.set_extension("exe");
            }
            if nested.exists() {
                return nested;
            }
        }
        path
    }

    pub fn ffmpeg_cmd(&self) -> PathBuf {
        let path = self.ffmpeg_bin_path();
        if path.exists() || self.managed_offline() {
            path
        } else {
            PathBuf::from("ffmpeg")
        }
    }

    pub fn ffprobe_cmd(&self) -> PathBuf {
        let path = self.ffprobe_bin_path();
        if path.exists() || self.managed_offline() {
            path
        } else {
            PathBuf::from("ffprobe")
        }
    }

    pub fn model_install_dir(&self, model_id: &str, version: &str) -> PathBuf {
        self.models_dir().join(model_id).join(version)
    }

    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(self.config_dir())?;
        std::fs::create_dir_all(self.library_dir())?;
        std::fs::create_dir_all(self.derived_items_dir())?;
        std::fs::create_dir_all(self.derived_jobs_dir())?;
        std::fs::create_dir_all(self.voice_templates_dir())?;
        std::fs::create_dir_all(self.voice_library_dir())?;
        std::fs::create_dir_all(self.db_dir())?;
        std::fs::create_dir_all(self.logs_dir())?;
        std::fs::create_dir_all(self.job_logs_dir())?;
        std::fs::create_dir_all(self.youtube_subscription_state_dir())?;
        std::fs::create_dir_all(self.default_diagnostics_trace_dir())?;
        std::fs::create_dir_all(self.cache_dir())?;
        std::fs::create_dir_all(self.thumbnail_cache_dir())?;
        std::fs::create_dir_all(self.job_secrets_dir())?;
        if !self.managed_offline() {
            std::fs::create_dir_all(self.models_dir())?;
            std::fs::create_dir_all(self.ffmpeg_dir())?;
        }
        Ok(())
    }

    pub fn normalize_base_dir(base_dir: &Path) -> PathBuf {
        // Keep it simple for now; callers should provide an app-specific directory.
        base_dir.to_path_buf()
    }
}

fn validate_runtime_token(value: &str, field: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(format!("managed runtime {field} is not a safe token"));
    }
    Ok(())
}

fn validate_sha256(value: &str, field: &str) -> Result<(), String> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!(
            "managed runtime {field} is not a SHA-256 hex digest"
        ));
    }
    Ok(())
}

fn validate_relative_runtime_path(value: &str, field: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(value);
    if value.trim().is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(format!("managed runtime {field} escapes its generation"));
    }
    Ok(path)
}

fn sha256_file(path: &Path) -> Result<String, String> {
    use std::io::Read as _;
    let mut file = std::fs::File::open(path)
        .map_err(|error| format!("failed to open {} for hashing: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("failed to read {} for hashing: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn validate_managed_runtime_manifest(
    manifest: &ManagedRuntimeManifest,
    selected_runtime_id: &str,
    app_version: &str,
    generation_root: &Path,
) -> Result<(), String> {
    if manifest.schema_version != MANAGED_RUNTIME_SCHEMA_VERSION {
        return Err(format!(
            "managed runtime manifest schema {} is unsupported",
            manifest.schema_version
        ));
    }
    if manifest.runtime_id != selected_runtime_id {
        return Err("managed runtime manifest ID does not match current.json".to_string());
    }
    validate_runtime_token(&manifest.runtime_id, "manifest runtime_id")?;
    validate_runtime_token(&manifest.qualification_id, "qualification_id")?;
    if !manifest
        .compatible_app_versions
        .iter()
        .any(|version| version == "*" || version == app_version)
    {
        return Err(format!(
            "managed runtime {} is not compatible with app version {}",
            manifest.runtime_id, app_version
        ));
    }

    let required_components = [
        ("tools", "tools"),
        ("models", "models"),
        ("huggingface", "cache/huggingface"),
        ("voice_backends", "voice_backends"),
    ];
    for (component_id, expected_root) in required_components {
        let component = manifest.components.get(component_id).ok_or_else(|| {
            format!("managed runtime manifest is missing component {component_id}")
        })?;
        if component.root.replace('\\', "/") != expected_root {
            return Err(format!(
                "managed runtime component {component_id} must use root {expected_root}"
            ));
        }
        validate_sha256(
            &component.archive_sha256,
            &format!("component {component_id} archive_sha256"),
        )?;
    }

    let canonical_generation = std::fs::canonicalize(generation_root).map_err(|error| {
        format!(
            "selected managed runtime generation cannot be opened at {}: {error}",
            generation_root.display()
        )
    })?;
    for required in &manifest.required_files {
        let relative = validate_relative_runtime_path(&required.path, "required file path")?;
        let candidate = generation_root.join(relative);
        if !candidate.is_file() {
            return Err(format!(
                "managed runtime required file is missing: {}",
                required.path
            ));
        }
        let canonical_candidate = std::fs::canonicalize(&candidate).map_err(|error| {
            format!(
                "managed runtime required file cannot be opened at {}: {error}",
                candidate.display()
            )
        })?;
        if !canonical_candidate.starts_with(&canonical_generation) {
            return Err(format!(
                "managed runtime required file escapes its generation: {}",
                required.path
            ));
        }
        if let Some(expected) = &required.sha256 {
            validate_sha256(expected, "required file sha256")?;
            let observed = sha256_file(&candidate)?;
            if !observed.eq_ignore_ascii_case(expected) {
                return Err(format!(
                    "managed runtime required file hash mismatch: {}",
                    required.path
                ));
            }
        }
    }
    Ok(())
}

pub fn activate_managed_runtime(
    runtime_root: &Path,
    pointer: &RuntimeActivationPointer,
) -> Result<(), String> {
    if pointer.schema_version != MANAGED_RUNTIME_SCHEMA_VERSION {
        return Err("cannot activate an unsupported managed runtime pointer".to_string());
    }
    validate_runtime_token(&pointer.runtime_id, "runtime_id")?;
    validate_sha256(&pointer.manifest_sha256, "manifest_sha256")?;
    std::fs::create_dir_all(runtime_root)
        .map_err(|error| format!("failed to create managed runtime root: {error}"))?;
    let current = runtime_root.join("current.json");
    if current.exists() {
        let previous = std::fs::read(&current)
            .map_err(|error| format!("failed to preserve prior runtime pointer: {error}"))?;
        persistence::atomic_write_bytes(&runtime_root.join("previous.json"), &previous)
            .map_err(|error| format!("failed to write prior runtime pointer: {error}"))?;
    }
    let mut bytes = serde_json::to_vec_pretty(pointer)
        .map_err(|error| format!("failed to serialize runtime pointer: {error}"))?;
    bytes.push(b'\n');
    persistence::atomic_write_bytes(&current, &bytes)
        .map_err(|error| format!("failed to activate managed runtime: {error}"))
}

/// Bounded reachability probe for a download/library root. Returns false if neither the
/// path nor its nearest existing ancestor can be stat-ed within `timeout`. A dropped NAS
/// share (UNC path) can block `metadata()` for the full OS SMB timeout, so the probe runs
/// on a worker thread and the caller gives up after `timeout` — converting a multi-second
/// hang into a quick "use local fallback" decision. (WP-0252 Item 2d.)
pub fn download_root_reachable(dir: &Path, timeout: std::time::Duration) -> bool {
    let probe = dir.to_path_buf();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut current = probe.as_path();
        let reachable = loop {
            if std::fs::metadata(current).is_ok() {
                break true;
            }
            match current.parent() {
                Some(parent) if parent != current => current = parent,
                _ => break false,
            }
        };
        let _ = tx.send(reachable);
    });
    matches!(rx.recv_timeout(timeout), Ok(true))
}

// WP-0255/WP-0306: bounded `is_dir`. A dropped NAS/UNC share makes a plain `Path::is_dir()`
// hang on the Windows SMB timeout (tens of seconds); doing that inside a list query froze the UI
// during a NAS blip. All callers share the fixed four-worker executor above: timeouts return
// false, queue saturation fails closed, and repeated stalls cannot create detached probe threads.
pub fn path_is_dir_bounded(dir: &Path, timeout: std::time::Duration) -> bool {
    probe_path_bounded(dir, timeout) == BoundedPathKind::Directory
}

#[cfg(test)]
mod managed_runtime_tests {
    use super::*;

    fn create_managed_runtime(
        root: &Path,
        runtime_id: &str,
        app_version: &str,
    ) -> RuntimeActivationPointer {
        let generation = root.join("generations").join(runtime_id);
        for relative in ["tools", "models", "cache/huggingface", "voice_backends"] {
            std::fs::create_dir_all(generation.join(relative)).expect("component dir");
        }
        std::fs::write(generation.join("tools").join("required.bin"), b"runtime")
            .expect("required file");
        let components = [
            ("tools", "tools"),
            ("models", "models"),
            ("huggingface", "cache/huggingface"),
            ("voice_backends", "voice_backends"),
        ]
        .into_iter()
        .map(|(id, component_root)| {
            (
                id.to_string(),
                RuntimeComponentManifest {
                    root: component_root.to_string(),
                    archive_sha256: "a".repeat(64),
                },
            )
        })
        .collect();
        let manifest = ManagedRuntimeManifest {
            schema_version: MANAGED_RUNTIME_SCHEMA_VERSION,
            runtime_id: runtime_id.to_string(),
            compatible_app_versions: vec![app_version.to_string()],
            qualification_id: "qualification-1".to_string(),
            components,
            required_files: vec![RuntimeRequiredFile {
                path: "tools/required.bin".to_string(),
                sha256: Some(hex::encode(Sha256::digest(b"runtime"))),
            }],
        };
        let mut bytes = serde_json::to_vec_pretty(&manifest).expect("manifest json");
        bytes.push(b'\n');
        std::fs::write(generation.join("runtime_manifest.json"), &bytes).expect("manifest write");
        RuntimeActivationPointer {
            schema_version: MANAGED_RUNTIME_SCHEMA_VERSION,
            runtime_id: runtime_id.to_string(),
            manifest_sha256: hex::encode(Sha256::digest(&bytes)),
        }
    }

    #[test]
    fn installed_paths_keep_user_data_separate_and_select_verified_generation() {
        let dir = tempfile::tempdir().expect("tempdir");
        let user_data = dir.path().join("roaming");
        let runtime_root = dir.path().join("local").join("runtime");
        let pointer = create_managed_runtime(&runtime_root, "runtime-1", "1.2.3");
        activate_managed_runtime(&runtime_root, &pointer).expect("activate");

        let paths = AppPaths::installed(user_data.clone(), runtime_root.clone(), "1.2.3")
            .expect("installed paths");
        assert_eq!(paths.runtime_mode(), RuntimeMode::ManagedOffline);
        assert_eq!(paths.db_dir(), user_data.join("db"));
        assert_eq!(
            paths.tools_dir(),
            runtime_root.join("generations/runtime-1/tools")
        );
        assert_eq!(
            paths.huggingface_cache_dir(),
            runtime_root.join("generations/runtime-1/cache/huggingface")
        );
        assert_eq!(
            paths.python_venv_dir(),
            runtime_root.join("generations/runtime-1/tools/python/runtime_main")
        );
        assert_eq!(
            paths.python_cosyvoice_venv_dir(),
            runtime_root.join("generations/runtime-1/tools/python/runtime_cosyvoice")
        );
        assert_eq!(
            paths.runtime_provenance().fallback_policy,
            "managed_generation_only"
        );
        assert_eq!(
            paths.ffmpeg_cmd(),
            runtime_root.join(if cfg!(windows) {
                "generations/runtime-1/tools/ffmpeg/ffmpeg.exe"
            } else {
                "generations/runtime-1/tools/ffmpeg/ffmpeg"
            }),
            "a missing managed FFmpeg must stay an absolute generation path instead of falling back to PATH"
        );
    }

    #[test]
    fn installed_paths_fail_closed_for_corrupt_selected_manifest() {
        let dir = tempfile::tempdir().expect("tempdir");
        let runtime_root = dir.path().join("runtime");
        let pointer = create_managed_runtime(&runtime_root, "runtime-1", "1.2.3");
        activate_managed_runtime(&runtime_root, &pointer).expect("activate");
        std::fs::write(
            runtime_root.join("generations/runtime-1/runtime_manifest.json"),
            b"{}",
        )
        .expect("corrupt manifest");
        let error = AppPaths::installed(dir.path().join("user"), runtime_root, "1.2.3")
            .expect_err("corrupt selected generation must fail");
        assert!(error.contains("manifest hash mismatch"));
    }

    #[test]
    fn installed_paths_without_pointer_use_explicit_legacy_mode() {
        let dir = tempfile::tempdir().expect("tempdir");
        let user_data = dir.path().join("roaming");
        let runtime_root = dir.path().join("local/runtime");
        let paths =
            AppPaths::installed(user_data.clone(), runtime_root, "1.2.3").expect("legacy paths");
        assert_eq!(paths.runtime_mode(), RuntimeMode::LegacyAppData);
        assert_eq!(paths.tools_dir(), user_data.join("tools"));
        assert_eq!(
            paths.runtime_provenance().manifest_status,
            "legacy_no_pointer"
        );
    }

    #[test]
    fn invalid_managed_paths_remain_managed_and_fail_closed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let user_data = dir.path().join("roaming");
        let runtime_root = dir.path().join("local/runtime");
        let paths = AppPaths::invalid_managed(
            user_data.clone(),
            runtime_root.clone(),
            "manifest hash mismatch".to_string(),
        );

        assert_eq!(paths.runtime_mode(), RuntimeMode::ManagedOfflineInvalid);
        assert!(paths.managed_offline());
        assert_eq!(paths.db_dir(), user_data.join("db"));
        assert_eq!(
            paths.ffmpeg_cmd(),
            runtime_root.join(if cfg!(windows) {
                "generations/_invalid/tools/ffmpeg/ffmpeg.exe"
            } else {
                "generations/_invalid/tools/ffmpeg/ffmpeg"
            }),
            "an invalid managed runtime must not fall back to PATH"
        );
        assert_eq!(
            paths.runtime_provenance().fallback_policy,
            "managed_generation_only"
        );
        assert!(paths
            .runtime_provenance()
            .manifest_status
            .contains("manifest hash mismatch"));
    }

    #[test]
    fn managed_runtime_accepts_the_existing_nested_ffmpeg_layout_without_path_fallback() {
        let dir = tempfile::tempdir().expect("tempdir");
        let runtime_root = dir.path().join("runtime");
        let pointer = create_managed_runtime(&runtime_root, "runtime-1", "1.2.3");
        let ffmpeg_bin = runtime_root.join("generations/runtime-1/tools/ffmpeg/bin");
        std::fs::create_dir_all(&ffmpeg_bin).expect("ffmpeg bin");
        let mut ffmpeg = ffmpeg_bin.join("ffmpeg");
        let mut ffprobe = ffmpeg_bin.join("ffprobe");
        if cfg!(windows) {
            ffmpeg.set_extension("exe");
            ffprobe.set_extension("exe");
        }
        std::fs::write(&ffmpeg, b"ffmpeg").expect("ffmpeg fixture");
        std::fs::write(&ffprobe, b"ffprobe").expect("ffprobe fixture");
        activate_managed_runtime(&runtime_root, &pointer).expect("activate");

        let paths = AppPaths::installed(dir.path().join("user"), runtime_root, "1.2.3")
            .expect("installed paths");
        assert_eq!(paths.ffmpeg_cmd(), ffmpeg);
        assert_eq!(paths.ffprobe_cmd(), ffprobe);
    }

    #[test]
    fn activation_preserves_the_previous_pointer() {
        let dir = tempfile::tempdir().expect("tempdir");
        let first = RuntimeActivationPointer {
            schema_version: MANAGED_RUNTIME_SCHEMA_VERSION,
            runtime_id: "runtime-1".to_string(),
            manifest_sha256: "a".repeat(64),
        };
        let second = RuntimeActivationPointer {
            schema_version: MANAGED_RUNTIME_SCHEMA_VERSION,
            runtime_id: "runtime-2".to_string(),
            manifest_sha256: "b".repeat(64),
        };
        activate_managed_runtime(dir.path(), &first).expect("first activation");
        activate_managed_runtime(dir.path(), &second).expect("second activation");
        let previous: RuntimeActivationPointer = serde_json::from_slice(
            &std::fs::read(dir.path().join("previous.json")).expect("previous"),
        )
        .expect("previous json");
        assert_eq!(previous, first);
    }
}

#[cfg(test)]
mod bounded_probe_tests {
    use super::*;
    use std::sync::atomic::Ordering;
    use std::time::Instant;
    static PROBE_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn stalled_path_probes_are_latency_bounded_and_never_grow_the_worker_pool() {
        let _serial = PROBE_TEST_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().expect("tempdir");
        let timeout = Duration::from_millis(20);
        for index in 0..24 {
            let started = Instant::now();
            let observed = probe_path_bounded_internal(
                &dir.path().join(format!("stalled-{index}")),
                timeout,
                Some(Duration::from_millis(150)),
            );
            assert_eq!(observed, BoundedPathKind::Unreachable);
            assert!(started.elapsed() < Duration::from_millis(100));
        }
        assert_eq!(
            BOUNDED_PATH_PROBE_THREADS_STARTED.load(Ordering::SeqCst),
            BOUNDED_PATH_PROBE_WORKERS,
            "timeouts and queue saturation must not spawn detached replacement threads"
        );
        std::thread::sleep(Duration::from_secs(1));
    }

    #[test]
    fn bounded_path_probe_distinguishes_files_directories_and_missing_paths() {
        let _serial = PROBE_TEST_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("file.mp4");
        std::fs::write(&file, b"legacy").expect("file");
        assert_eq!(
            probe_path_bounded(dir.path(), Duration::from_secs(1)),
            BoundedPathKind::Directory
        );
        assert_eq!(
            probe_path_bounded(&file, Duration::from_secs(1)),
            BoundedPathKind::File
        );
        assert_eq!(
            probe_path_bounded(&dir.path().join("missing"), Duration::from_secs(1)),
            BoundedPathKind::Missing
        );
    }
}
