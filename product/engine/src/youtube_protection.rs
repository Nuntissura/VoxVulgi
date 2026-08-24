use crate::{db, paths::AppPaths, EngineError, Result};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

pub const PROVIDER_YOUTUBE: &str = "youtube";
pub const OPERATION_DOWNLOAD: &str = "download";
pub const OPERATION_ENUMERATION: &str = "enumeration";
pub const ANONYMOUS_AUTH_FINGERPRINT: &str = "anonymous";

const CORROBORATION_MIN_SEPARATION_MS: i64 = 60_000;
const CORROBORATION_WINDOW_MS: i64 = 24 * 60 * 60_000;
const CAUTIOUS_DWELL_MS: i64 = 15 * 60_000;
const CONSERVATIVE_DWELL_MS: i64 = 60 * 60_000;
const COOLDOWN_DWELL_MS: i64 = 6 * 60 * 60_000;
const RECOVERY_SUCCESS_THRESHOLD: u32 = 3;
const RAW_RETENTION_MS: i64 = 90 * 24 * 60 * 60_000;
const RAW_RETENTION_BATCH_SIZE: usize = 1_000;
const CANARY_LEASE_MS: i64 = 15 * 60_000;
const TUNING_META_KEY: &str = "youtube_protection_tuning_v1";
static TUNING_WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static HISTORY_RESET_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct YoutubeProtectionTuning {
    pub corroboration_min_separation_secs: u32,
    pub corroboration_window_secs: u32,
    pub cautious_dwell_secs: u32,
    pub conservative_dwell_secs: u32,
    pub cooldown_dwell_secs: u32,
    pub recovery_success_threshold: u32,
    pub raw_retention_days: u32,
    pub cautious_max_fragments: u32,
    pub cautious_min_sleep_secs: u32,
    pub conservative_min_sleep_secs: u32,
    pub cooldown_min_sleep_secs: u32,
    pub cautious_start_interval_secs: u32,
    pub conservative_start_interval_secs: u32,
    pub cooldown_start_interval_secs: u32,
    pub canary_tranche_size: u32,
}

impl Default for YoutubeProtectionTuning {
    fn default() -> Self {
        Self {
            corroboration_min_separation_secs: (CORROBORATION_MIN_SEPARATION_MS / 1_000) as u32,
            corroboration_window_secs: (CORROBORATION_WINDOW_MS / 1_000) as u32,
            cautious_dwell_secs: (CAUTIOUS_DWELL_MS / 1_000) as u32,
            conservative_dwell_secs: (CONSERVATIVE_DWELL_MS / 1_000) as u32,
            cooldown_dwell_secs: (COOLDOWN_DWELL_MS / 1_000) as u32,
            recovery_success_threshold: RECOVERY_SUCCESS_THRESHOLD,
            raw_retention_days: (RAW_RETENTION_MS / (24 * 60 * 60_000)) as u32,
            cautious_max_fragments: 2,
            cautious_min_sleep_secs: 10,
            conservative_min_sleep_secs: 20,
            cooldown_min_sleep_secs: 30,
            cautious_start_interval_secs: 10,
            conservative_start_interval_secs: 20,
            cooldown_start_interval_secs: 30,
            canary_tranche_size: 1,
        }
    }
}

impl YoutubeProtectionTuning {
    fn normalized(mut self) -> Self {
        self.corroboration_min_separation_secs =
            self.corroboration_min_separation_secs.clamp(10, 3_600);
        self.corroboration_window_secs = self
            .corroboration_window_secs
            .clamp(self.corroboration_min_separation_secs, 7 * 24 * 3_600);
        self.cautious_dwell_secs = self.cautious_dwell_secs.clamp(60, 24 * 3_600);
        self.conservative_dwell_secs = self
            .conservative_dwell_secs
            .clamp(self.cautious_dwell_secs, 7 * 24 * 3_600);
        self.cooldown_dwell_secs = self
            .cooldown_dwell_secs
            .clamp(self.conservative_dwell_secs, 14 * 24 * 3_600);
        self.recovery_success_threshold = self.recovery_success_threshold.clamp(1, 20);
        self.raw_retention_days = self.raw_retention_days.clamp(7, 365);
        self.cautious_max_fragments = self.cautious_max_fragments.clamp(1, 8);
        self.cautious_min_sleep_secs = self.cautious_min_sleep_secs.clamp(5, 300);
        self.conservative_min_sleep_secs = self
            .conservative_min_sleep_secs
            .clamp(self.cautious_min_sleep_secs, 600);
        self.cooldown_min_sleep_secs = self
            .cooldown_min_sleep_secs
            .clamp(self.conservative_min_sleep_secs, 900);
        self.cautious_start_interval_secs = self.cautious_start_interval_secs.clamp(5, 300);
        self.conservative_start_interval_secs = self
            .conservative_start_interval_secs
            .clamp(self.cautious_start_interval_secs, 600);
        self.cooldown_start_interval_secs = self
            .cooldown_start_interval_secs
            .clamp(self.conservative_start_interval_secs, 900);
        // A cooldown probe is deliberately one item. Keeping this configurable above one made
        // the UI promise a canary while permitting a small batch at the execution boundary.
        self.canary_tranche_size = 1;
        self
    }

    fn corroboration_min_separation_ms(&self) -> i64 {
        i64::from(self.corroboration_min_separation_secs).saturating_mul(1_000)
    }

    fn corroboration_window_ms(&self) -> i64 {
        i64::from(self.corroboration_window_secs).saturating_mul(1_000)
    }

    fn cautious_dwell_ms(&self) -> i64 {
        i64::from(self.cautious_dwell_secs).saturating_mul(1_000)
    }

    fn conservative_dwell_ms(&self) -> i64 {
        i64::from(self.conservative_dwell_secs).saturating_mul(1_000)
    }

    fn cooldown_dwell_ms(&self) -> i64 {
        i64::from(self.cooldown_dwell_secs).saturating_mul(1_000)
    }

    fn raw_retention_ms(&self) -> i64 {
        i64::from(self.raw_retention_days).saturating_mul(24 * 60 * 60_000)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloaderOutcomeClass {
    RateLimited,
    PoTokenOrClientCapability,
    AuthenticationRequiredOrInvalid,
    ContentUnavailableOrPrivate,
    NetworkTransient,
    StorageOrLocalTool,
    Success,
    Unknown,
}

impl DownloaderOutcomeClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RateLimited => "rate_limited",
            Self::PoTokenOrClientCapability => "po_token_or_client_capability",
            Self::AuthenticationRequiredOrInvalid => "authentication_required_or_invalid",
            Self::ContentUnavailableOrPrivate => "content_unavailable_or_private",
            Self::NetworkTransient => "network_transient",
            Self::StorageOrLocalTool => "storage_or_local_tool",
            Self::Success => "success",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloaderPolicyMode {
    Normal,
    Cautious,
    Conservative,
    Cooldown,
    Hold,
}

impl DownloaderPolicyMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Cautious => "cautious",
            Self::Conservative => "conservative",
            Self::Cooldown => "cooldown",
            Self::Hold => "hold",
        }
    }

    fn parse(value: &str) -> Self {
        match value {
            "cautious" => Self::Cautious,
            "conservative" => Self::Conservative,
            "cooldown" => Self::Cooldown,
            "hold" => Self::Hold,
            _ => Self::Normal,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloaderBaselinePolicy {
    pub concurrent_fragments: u32,
    pub sleep_interval_secs: u32,
    pub sleep_requests_secs: u32,
    pub update_tranche_size: u32,
    pub limit_rate: Option<String>,
    pub throttled_rate: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloaderEffectivePolicy {
    pub mode: DownloaderPolicyMode,
    pub concurrent_fragments: u32,
    pub sleep_interval_secs: u32,
    pub max_sleep_interval_secs: u32,
    pub sleep_requests_secs: u32,
    pub aggregate_start_interval_secs: u32,
    pub update_tranche_size: u32,
    pub limit_rate: Option<String>,
    pub throttled_rate: Option<String>,
    pub eligible: bool,
    pub canary_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloaderPolicySnapshot {
    pub provider: String,
    pub operation: String,
    pub auth_fingerprint: String,
    pub runtime_epoch: String,
    pub mode: DownloaderPolicyMode,
    pub corroboration_count: u32,
    pub success_streak: u32,
    pub entered_at_ms: i64,
    pub last_evidence_at_ms: Option<i64>,
    pub next_eligible_probe_at_ms: Option<i64>,
    pub version: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloaderOutcomeSummary {
    pub id: String,
    pub target_fingerprint: String,
    pub occurred_at_ms: i64,
    pub outcome_class: String,
    pub error_signature: Option<String>,
    pub incident_id: Option<String>,
    pub duration_ms: Option<i64>,
    pub baseline_policy: DownloaderBaselinePolicy,
    pub effective_policy: DownloaderEffectivePolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloaderTransitionSummary {
    pub id: String,
    pub before_mode: String,
    pub after_mode: String,
    pub reason: String,
    pub evidence_ids: Vec<String>,
    pub evidence_snapshot: Vec<DownloaderTransitionEvidenceSnapshot>,
    pub occurred_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloaderTransitionEvidenceSnapshot {
    pub id: String,
    pub target_fingerprint: String,
    pub occurred_at_ms: i64,
    pub outcome_class: String,
    pub auth_fingerprint: String,
    pub runtime_epoch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloaderPolicyHistory {
    pub outcomes: Vec<DownloaderOutcomeSummary>,
    pub transitions: Vec<DownloaderTransitionSummary>,
    pub raw_total: u64,
    pub transition_total: u64,
    pub rollup_event_total: u64,
    pub unknown_total: u64,
    pub class_totals: Vec<DownloaderOutcomeClassTotal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloaderOutcomeClassTotal {
    pub outcome_class: String,
    pub event_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloaderPolicyReplayReceipt {
    pub events_replayed: usize,
    pub unknown_events: usize,
    pub final_mode: DownloaderPolicyMode,
    pub mode_path: Vec<DownloaderPolicyMode>,
    pub mode_path_truncated: bool,
    pub transitions_replayed: u64,
    pub complete: bool,
    pub truncated: bool,
    pub retained_raw_total: u64,
    pub durable_rollup_total: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloaderHistoryCursor {
    pub occurred_at_ms: i64,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloaderOutcomePage {
    pub outcomes: Vec<DownloaderOutcomeSummary>,
    pub next_cursor: Option<DownloaderHistoryCursor>,
    pub has_more: bool,
    pub raw_total: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloaderTransitionPage {
    pub transitions: Vec<DownloaderTransitionSummary>,
    pub next_cursor: Option<DownloaderHistoryCursor>,
    pub has_more: bool,
    pub transition_total: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloaderRetentionReceipt {
    pub deleted: u64,
    pub has_more: bool,
    pub cutoff_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloaderRetentionDrainReceipt {
    pub batches: usize,
    pub deleted: u64,
    pub complete: bool,
    pub has_more: bool,
    pub cutoff_ms: i64,
    pub elapsed_ms: u64,
    pub budget_exhausted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DownloaderRetentionContinuation {
    pub pending: bool,
    pub consecutive_failures: u32,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloaderHistoryResetReceipt {
    pub reset_id: String,
    pub complete: bool,
    pub has_more: bool,
    pub outcomes_deleted: u64,
    pub transitions_deleted: u64,
    pub rollups_deleted: u64,
    pub states_deleted: u64,
    pub leases_deleted: u64,
}

#[derive(Debug, Clone)]
pub struct RecordDownloaderOutcome<'a> {
    pub provider: &'a str,
    pub operation: &'a str,
    pub canonical_target: &'a str,
    pub auth_fingerprint: &'a str,
    pub runtime_epoch: &'a str,
    pub baseline: &'a DownloaderBaselinePolicy,
    pub effective: &'a DownloaderEffectivePolicy,
    pub outcome: DownloaderOutcomeClass,
    pub error_text: Option<&'a str>,
    pub incident_id: Option<&'a str>,
    pub lease_owner_job_id: Option<&'a str>,
    pub duration_ms: Option<i64>,
    pub occurred_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloaderRuntimeCapabilities {
    pub epoch: String,
    pub yt_dlp_available: bool,
    pub yt_dlp_version: Option<String>,
    pub yt_dlp_sha256_hex: Option<String>,
    pub node_version: Option<String>,
    pub npm_version: Option<String>,
    pub node_exe_sha256_hex: Option<String>,
    pub npm_cmd_sha256_hex: Option<String>,
    pub provider_version: String,
    pub provider_installed: bool,
    pub provider_running: bool,
    pub provider_healthy: bool,
    pub provider_plugin_sha256_hex: Option<String>,
    pub provider_server_sha256_hex: Option<String>,
    pub provider_lock_sha256_hex: Option<String>,
    pub provider_node_modules_sha256_hex: Option<String>,
    pub provider_node_modules_verified_at_ms: Option<i64>,
    pub provider_node_modules_integrity_verifying: bool,
    pub provider_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloaderRuntimeIdentity {
    pub epoch: String,
    pub yt_dlp_available: bool,
    pub yt_dlp_version: Option<String>,
    pub yt_dlp_sha256_hex: Option<String>,
    pub node_version: Option<String>,
    pub npm_version: Option<String>,
    pub node_exe_sha256_hex: Option<String>,
    pub npm_cmd_sha256_hex: Option<String>,
    pub provider_version: String,
    pub provider_installed: bool,
    pub provider_plugin_sha256_hex: Option<String>,
    pub provider_server_sha256_hex: Option<String>,
    pub provider_lock_sha256_hex: Option<String>,
    pub provider_node_modules_sha256_hex: Option<String>,
}

fn sha256_path(path: &Path) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let read = std::io::Read::read(&mut file, &mut buffer).ok()?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Some(hex::encode_upper(hasher.finalize()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CapabilityFileStamp {
    path: PathBuf,
    len: Option<u64>,
    modified_ns: Option<u128>,
}

fn capability_file_stamp(path: PathBuf) -> CapabilityFileStamp {
    let metadata = std::fs::metadata(&path).ok();
    CapabilityFileStamp {
        path,
        len: metadata.as_ref().map(std::fs::Metadata::len),
        modified_ns: metadata
            .and_then(|metadata| metadata.modified().ok())
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_nanos()),
    }
}

fn capability_file_stamps(paths: &AppPaths) -> Vec<CapabilityFileStamp> {
    let mut yt_dlp = paths.tools_dir().join("yt-dlp").join("yt-dlp");
    if cfg!(windows) {
        yt_dlp.set_extension("exe");
    }
    vec![
        capability_file_stamp(yt_dlp),
        capability_file_stamp(paths.node_exe()),
        capability_file_stamp(paths.youtube_po_provider_entrypoint()),
        capability_file_stamp(
            paths
                .youtube_po_provider_server_dir()
                .join("package-lock.json"),
        ),
        capability_file_stamp(paths.youtube_po_provider_server_dir().join("package.json")),
        capability_file_stamp(
            paths
                .youtube_po_provider_server_dir()
                .join(".production_audit_zero"),
        ),
        capability_file_stamp(
            paths
                .youtube_po_provider_server_dir()
                .join(".node_modules_integrity.json"),
        ),
        capability_file_stamp(
            paths
                .youtube_po_provider_plugin_dir()
                .join(".plugin_archive_sha256"),
        ),
    ]
}

#[derive(Debug, Clone)]
struct CachedRuntimeIdentity {
    stamps: Vec<CapabilityFileStamp>,
    verified_at: std::time::Instant,
    epoch: String,
    yt_dlp_available: bool,
    yt_dlp_version: Option<String>,
    yt_dlp_sha256_hex: Option<String>,
    node_version: Option<String>,
    npm_version: Option<String>,
    node_exe_sha256_hex: Option<String>,
    npm_cmd_sha256_hex: Option<String>,
    provider_version: String,
    provider_installed: bool,
    provider_plugin_sha256_hex: Option<String>,
    provider_server_sha256_hex: Option<String>,
    provider_lock_sha256_hex: Option<String>,
    provider_node_modules_sha256_hex: Option<String>,
    provider_node_modules_verified_at_ms: Option<i64>,
    provider_node_modules_integrity_verifying: bool,
    provider_error: Option<String>,
}

fn runtime_identity_cache() -> &'static Mutex<HashMap<PathBuf, CachedRuntimeIdentity>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, CachedRuntimeIdentity>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn immutable_runtime_epoch_payload(
    yt_dlp_available: bool,
    yt_dlp_version: Option<&str>,
    yt_dlp_sha256_hex: Option<&str>,
    node_version: Option<&str>,
    npm_version: Option<&str>,
    node_exe_sha256_hex: Option<&str>,
    npm_cmd_sha256_hex: Option<&str>,
    provider_version: &str,
    provider_installed: bool,
    provider_plugin_sha256_hex: Option<&str>,
    provider_server_sha256_hex: Option<&str>,
    provider_lock_sha256_hex: Option<&str>,
    provider_node_modules_sha256_hex: Option<&str>,
) -> serde_json::Value {
    serde_json::json!({
        "yt_dlp_available": yt_dlp_available,
        "yt_dlp_version": yt_dlp_version,
        "yt_dlp_sha256_hex": yt_dlp_sha256_hex,
        "node_version": node_version,
        "npm_version": npm_version,
        "node_exe_sha256_hex": node_exe_sha256_hex,
        "npm_cmd_sha256_hex": npm_cmd_sha256_hex,
        "provider_version": provider_version,
        "provider_installed": provider_installed,
        "provider_plugin_sha256_hex": provider_plugin_sha256_hex,
        "provider_server_sha256_hex": provider_server_sha256_hex,
        "provider_lock_sha256_hex": provider_lock_sha256_hex,
        "provider_node_modules_sha256_hex": provider_node_modules_sha256_hex,
    })
}

fn verified_bundled_ytdlp_identity(
    status: &crate::tools::YtDlpToolsStatus,
    bundled_sha256_hex: Option<String>,
    bundled_bytes: Option<u64>,
) -> (bool, Option<String>, Option<String>) {
    let pin = &crate::pinned_dependency_manifest::manifest().yt_dlp_windows;
    let verified = status.available
        && status.bundled_installed
        && status.ytdlp_path == status.bundled_path
        && status.ytdlp_version.as_deref() == Some(pin.version.as_str())
        && bundled_bytes == Some(pin.file_bytes)
        && bundled_sha256_hex
            .as_deref()
            .is_some_and(|actual| actual.eq_ignore_ascii_case(&pin.sha256_hex));
    if verified {
        (true, Some(pin.version.clone()), bundled_sha256_hex)
    } else {
        (false, None, None)
    }
}

fn load_runtime_identity(paths: &AppPaths) -> CachedRuntimeIdentity {
    let stamps = capability_file_stamps(paths);
    let key = paths.base_dir.clone();
    if let Some(cached) = runtime_identity_cache()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&key)
        .filter(|cached| {
            cached.stamps == stamps
                && cached.verified_at.elapsed() < std::time::Duration::from_secs(5)
        })
        .cloned()
    {
        return cached;
    }

    #[cfg(test)]
    {
        let mut misses = runtime_identity_cache_misses()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *misses.entry(key.clone()).or_insert(0) += 1;
    }

    let yt_dlp = crate::tools::ytdlp_tools_status(paths);
    let bundled_yt_dlp_sha256_hex = if yt_dlp.bundled_installed {
        sha256_path(Path::new(&yt_dlp.bundled_path))
    } else {
        None
    };
    let bundled_yt_dlp_bytes = yt_dlp
        .bundled_installed
        .then(|| {
            std::fs::metadata(&yt_dlp.bundled_path)
                .ok()
                .map(|value| value.len())
        })
        .flatten();
    let (yt_dlp_available, yt_dlp_version, yt_dlp_sha256_hex) =
        verified_bundled_ytdlp_identity(&yt_dlp, bundled_yt_dlp_sha256_hex, bundled_yt_dlp_bytes);
    let provider = crate::tools::youtube_po_provider_install_status(paths);
    let epoch_payload = immutable_runtime_epoch_payload(
        yt_dlp_available,
        yt_dlp_version.as_deref(),
        yt_dlp_sha256_hex.as_deref(),
        provider.node_version.as_deref(),
        provider.npm_version.as_deref(),
        provider.node_exe_sha256_hex.as_deref(),
        provider.npm_cmd_sha256_hex.as_deref(),
        &provider.provider_version,
        provider.installed,
        provider.plugin_tree_sha256_hex.as_deref(),
        provider.server_entrypoint_sha256_hex.as_deref(),
        provider.derived_lock_sha256_hex.as_deref(),
        provider.node_modules_tree_sha256_hex.as_deref(),
    );
    let identity = CachedRuntimeIdentity {
        stamps,
        verified_at: std::time::Instant::now(),
        epoch: fingerprint(&serde_json::to_string(&epoch_payload).unwrap_or_default()),
        yt_dlp_available,
        yt_dlp_version,
        yt_dlp_sha256_hex,
        node_version: provider.node_version,
        npm_version: provider.npm_version,
        node_exe_sha256_hex: provider.node_exe_sha256_hex,
        npm_cmd_sha256_hex: provider.npm_cmd_sha256_hex,
        provider_version: provider.provider_version,
        provider_installed: provider.installed,
        provider_plugin_sha256_hex: provider.plugin_tree_sha256_hex,
        provider_server_sha256_hex: provider.server_entrypoint_sha256_hex,
        provider_lock_sha256_hex: provider.derived_lock_sha256_hex,
        provider_node_modules_sha256_hex: provider.node_modules_tree_sha256_hex,
        provider_node_modules_verified_at_ms: provider.node_modules_verified_at_ms,
        provider_node_modules_integrity_verifying: provider.node_modules_integrity_verifying,
        provider_error: provider.readiness_error,
    };
    runtime_identity_cache()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(key, identity.clone());
    identity
}

#[cfg(test)]
fn runtime_identity_cache_misses() -> &'static Mutex<HashMap<PathBuf, usize>> {
    static MISSES: OnceLock<Mutex<HashMap<PathBuf, usize>>> = OnceLock::new();
    MISSES.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn runtime_capabilities(paths: &AppPaths) -> DownloaderRuntimeCapabilities {
    let identity = load_runtime_identity(paths);
    let provider_runtime = crate::tools::youtube_po_provider_runtime_status(paths);
    DownloaderRuntimeCapabilities {
        epoch: identity.epoch,
        yt_dlp_available: identity.yt_dlp_available,
        yt_dlp_version: identity.yt_dlp_version,
        yt_dlp_sha256_hex: identity.yt_dlp_sha256_hex,
        node_version: identity.node_version,
        npm_version: identity.npm_version,
        node_exe_sha256_hex: identity.node_exe_sha256_hex,
        npm_cmd_sha256_hex: identity.npm_cmd_sha256_hex,
        provider_version: identity.provider_version,
        provider_installed: identity.provider_installed,
        provider_running: provider_runtime.running,
        provider_healthy: provider_runtime.healthy,
        provider_plugin_sha256_hex: identity.provider_plugin_sha256_hex,
        provider_server_sha256_hex: identity.provider_server_sha256_hex,
        provider_lock_sha256_hex: identity.provider_lock_sha256_hex,
        provider_node_modules_sha256_hex: identity.provider_node_modules_sha256_hex,
        provider_node_modules_verified_at_ms: identity.provider_node_modules_verified_at_ms,
        provider_node_modules_integrity_verifying: identity
            .provider_node_modules_integrity_verifying,
        provider_error: provider_runtime.error.or(identity.provider_error),
    }
}

pub fn runtime_epoch_for_paths(paths: &AppPaths) -> String {
    load_runtime_identity(paths).epoch
}

pub fn runtime_identity_for_paths(paths: &AppPaths) -> DownloaderRuntimeIdentity {
    let identity = load_runtime_identity(paths);
    DownloaderRuntimeIdentity {
        epoch: identity.epoch,
        yt_dlp_available: identity.yt_dlp_available,
        yt_dlp_version: identity.yt_dlp_version,
        yt_dlp_sha256_hex: identity.yt_dlp_sha256_hex,
        node_version: identity.node_version,
        npm_version: identity.npm_version,
        node_exe_sha256_hex: identity.node_exe_sha256_hex,
        npm_cmd_sha256_hex: identity.npm_cmd_sha256_hex,
        provider_version: identity.provider_version,
        provider_installed: identity.provider_installed,
        provider_plugin_sha256_hex: identity.provider_plugin_sha256_hex,
        provider_server_sha256_hex: identity.provider_server_sha256_hex,
        provider_lock_sha256_hex: identity.provider_lock_sha256_hex,
        provider_node_modules_sha256_hex: identity.provider_node_modules_sha256_hex,
    }
}

pub fn runtime_epoch() -> String {
    let manifest = crate::pinned_dependency_manifest::manifest();
    format!(
        "yt-dlp:{}|deno:{}|po-provider:none",
        manifest.yt_dlp_windows.version, manifest.deno_windows.version
    )
}

pub fn classify_youtube_outcome(error_text: Option<&str>) -> DownloaderOutcomeClass {
    let Some(raw) = error_text else {
        return DownloaderOutcomeClass::Success;
    };
    let lower = raw.to_ascii_lowercase();
    if lower.contains("po token")
        || lower.contains("potoken")
        || lower.contains("player client") && lower.contains("required")
    {
        return DownloaderOutcomeClass::PoTokenOrClientCapability;
    }
    if lower.contains("sign in to confirm")
        || lower.contains("login required")
        || lower.contains("cookies") && lower.contains("rejected")
    {
        return DownloaderOutcomeClass::AuthenticationRequiredOrInvalid;
    }
    if lower.contains("private video")
        || lower.contains("video unavailable")
        || lower.contains("copyright")
        || lower.contains("members-only")
        || lower.contains("content isn't available")
    {
        return DownloaderOutcomeClass::ContentUnavailableOrPrivate;
    }
    if lower.contains("timed out")
        || lower.contains("connection reset")
        || lower.contains("temporary failure")
        || lower.contains("dns")
    {
        return DownloaderOutcomeClass::NetworkTransient;
    }
    if lower.contains("no space left")
        || lower.contains("access is denied")
        || lower.contains("permission denied")
        || lower.contains("ffmpeg")
        || lower.contains("ffprobe")
        || lower.contains("not found") && lower.contains("yt-dlp")
    {
        return DownloaderOutcomeClass::StorageOrLocalTool;
    }
    // Only explicit remote HTTP throttling signatures train pacing. Generic mentions such as a
    // local "rate limit setting" are intentionally unknown and cannot reduce throughput.
    if lower.contains("http error 429")
        || lower.contains("http status 429")
        || lower.contains("status code: 429")
        || lower.contains("status code 429")
        || lower.contains("too many requests")
            && (lower.contains("[youtube]")
                || lower.contains("http error")
                || lower.contains("server returned"))
    {
        return DownloaderOutcomeClass::RateLimited;
    }
    DownloaderOutcomeClass::Unknown
}

pub(crate) fn load_tuning_conn(conn: &rusqlite::Connection) -> Result<YoutubeProtectionTuning> {
    let value = conn
        .query_row(
            "SELECT value FROM meta WHERE key=?1",
            [TUNING_META_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    Ok(value
        .and_then(|raw| serde_json::from_str::<YoutubeProtectionTuning>(&raw).ok())
        .unwrap_or_default()
        .normalized())
}

pub(crate) fn claim_mutation_generation_conn(
    conn: &rusqlite::Connection,
    operation: &str,
    generation: u64,
    allow_equal_continuation: bool,
) -> Result<()> {
    if generation == 0 || generation > i64::MAX as u64 {
        return Err(EngineError::InstallFailed(
            "YouTube protection mutation generation is outside the durable SQLite range"
                .to_string(),
        ));
    }
    let latest = conn
        .query_row(
            "SELECT generation FROM youtube_protection_mutation_generation WHERE operation=?1",
            [operation],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .unwrap_or(0);
    let generation = generation as i64;
    if generation < latest || (generation == latest && !allow_equal_continuation) {
        return Err(EngineError::InstallFailed(format!(
            "stale YouTube protection mutation generation {generation}; durable latest is {latest}"
        )));
    }
    conn.execute(
        "INSERT INTO youtube_protection_mutation_generation(operation,generation,updated_at_ms) VALUES(?1,?2,?3) ON CONFLICT(operation) DO UPDATE SET generation=excluded.generation,updated_at_ms=excluded.updated_at_ms",
        params![operation, generation, now_ms()],
    )?;
    Ok(())
}

pub fn get_tuning(paths: &AppPaths) -> Result<YoutubeProtectionTuning> {
    let conn = db::open_readonly(paths)?;
    load_tuning_conn(&conn)
}

pub fn set_tuning(
    paths: &AppPaths,
    tuning: YoutubeProtectionTuning,
) -> Result<YoutubeProtectionTuning> {
    let _write_guard = TUNING_WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let tuning = tuning.normalized();
    let tuning_json = serde_json::to_string(&tuning)?;
    db::AppDatabase::for_paths(paths)?.write(
        db::DatabaseOperationContext::new("youtube_protection", "set_tuning").foreground(),
        TransactionBehavior::Immediate,
        |transaction| {
            transaction.execute(
                "INSERT INTO meta(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                params![TUNING_META_KEY, tuning_json],
            )?;
            Ok(())
        },
    )?;
    Ok(tuning)
}

pub fn set_tuning_with_generation(
    paths: &AppPaths,
    tuning: YoutubeProtectionTuning,
    mutation_generation: u64,
) -> Result<YoutubeProtectionTuning> {
    let _write_guard = TUNING_WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut conn = db::write_context(paths)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    claim_mutation_generation_conn(&tx, "tuning", mutation_generation, false)?;
    let tuning = tuning.normalized();
    tx.execute(
        "INSERT INTO meta(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![TUNING_META_KEY, serde_json::to_string(&tuning)?],
    )?;
    tx.commit()?;
    Ok(tuning)
}

pub fn reset_tuning_with_generation(
    paths: &AppPaths,
    mutation_generation: u64,
) -> Result<YoutubeProtectionTuning> {
    set_tuning_with_generation(
        paths,
        YoutubeProtectionTuning::default(),
        mutation_generation,
    )
}

pub fn reset_tuning(paths: &AppPaths) -> Result<YoutubeProtectionTuning> {
    set_tuning(paths, YoutubeProtectionTuning::default())
}

pub fn effective_policy(
    baseline: &DownloaderBaselinePolicy,
    state: &DownloaderPolicySnapshot,
    now_ms: i64,
) -> DownloaderEffectivePolicy {
    effective_policy_with_tuning(baseline, state, now_ms, &YoutubeProtectionTuning::default())
}

pub fn baseline_effective_policy(baseline: &DownloaderBaselinePolicy) -> DownloaderEffectivePolicy {
    DownloaderEffectivePolicy {
        mode: DownloaderPolicyMode::Normal,
        concurrent_fragments: baseline.concurrent_fragments.max(1),
        sleep_interval_secs: baseline.sleep_interval_secs,
        max_sleep_interval_secs: baseline.sleep_interval_secs,
        sleep_requests_secs: baseline.sleep_requests_secs,
        aggregate_start_interval_secs: baseline.sleep_interval_secs,
        update_tranche_size: baseline.update_tranche_size.max(1),
        limit_rate: baseline.limit_rate.clone(),
        throttled_rate: baseline.throttled_rate.clone(),
        eligible: true,
        canary_only: false,
    }
}

pub fn effective_policy_with_tuning(
    baseline: &DownloaderBaselinePolicy,
    state: &DownloaderPolicySnapshot,
    now_ms: i64,
    tuning: &YoutubeProtectionTuning,
) -> DownloaderEffectivePolicy {
    let tuning = tuning.clone().normalized();
    let fragments = baseline.concurrent_fragments.max(1);
    let tranche = baseline.update_tranche_size.max(1);
    let (
        concurrent_fragments,
        sleep_interval_secs,
        max_sleep_interval_secs,
        sleep_requests_secs,
        aggregate_start_interval_secs,
        update_tranche_size,
        eligible,
        canary_only,
    ) = match state.mode {
        DownloaderPolicyMode::Normal => (
            fragments,
            baseline.sleep_interval_secs,
            baseline.sleep_interval_secs.saturating_add(5),
            baseline.sleep_requests_secs,
            baseline.sleep_interval_secs,
            tranche,
            true,
            false,
        ),
        DownloaderPolicyMode::Cautious => (
            fragments.min(tuning.cautious_max_fragments),
            baseline
                .sleep_interval_secs
                .max(tuning.cautious_min_sleep_secs),
            baseline
                .sleep_interval_secs
                .max(tuning.cautious_min_sleep_secs.saturating_mul(2)),
            baseline.sleep_requests_secs.max(1),
            tuning.cautious_start_interval_secs,
            tranche.min(15),
            true,
            false,
        ),
        DownloaderPolicyMode::Conservative => (
            1,
            baseline
                .sleep_interval_secs
                .max(tuning.conservative_min_sleep_secs),
            baseline
                .sleep_interval_secs
                .max(tuning.conservative_min_sleep_secs.saturating_mul(2)),
            baseline.sleep_requests_secs.max(2),
            tuning.conservative_start_interval_secs,
            tranche.min(5),
            true,
            false,
        ),
        DownloaderPolicyMode::Cooldown => {
            let probe_ready = state
                .next_eligible_probe_at_ms
                .map(|at| now_ms >= at)
                .unwrap_or(false);
            (
                1,
                baseline
                    .sleep_interval_secs
                    .max(tuning.cooldown_min_sleep_secs),
                baseline
                    .sleep_interval_secs
                    .max(tuning.cooldown_min_sleep_secs.saturating_mul(2)),
                baseline.sleep_requests_secs.max(3),
                tuning.cooldown_start_interval_secs,
                tuning.canary_tranche_size.min(tranche),
                probe_ready,
                probe_ready,
            )
        }
        DownloaderPolicyMode::Hold => (
            1,
            baseline.sleep_interval_secs.max(30),
            baseline.sleep_interval_secs.max(60),
            baseline.sleep_requests_secs.max(3),
            30,
            1,
            false,
            false,
        ),
    };
    DownloaderEffectivePolicy {
        mode: state.mode,
        concurrent_fragments,
        sleep_interval_secs,
        max_sleep_interval_secs,
        sleep_requests_secs,
        aggregate_start_interval_secs,
        update_tranche_size,
        // Adaptive protection never changes bandwidth policy: these two yt-dlp concepts remain
        // distinct and the operator baseline stays byte-for-byte intact.
        limit_rate: baseline.limit_rate.clone(),
        throttled_rate: baseline.throttled_rate.clone(),
        eligible,
        canary_only,
    }
}

pub fn load_policy_state(
    paths: &AppPaths,
    provider: &str,
    operation: &str,
    auth_fingerprint: &str,
    runtime_epoch: &str,
) -> Result<DownloaderPolicySnapshot> {
    let conn = db::open_readonly(paths)?;
    load_policy_state_conn(&conn, provider, operation, auth_fingerprint, runtime_epoch)
}

/// Atomically reserves the single controlled probe allowed when a cooldown expires.
///
/// Callers must first compute an effective policy whose `canary_only` flag is true. The
/// reservation is a short durable lease, so a second worker cannot probe the same lane while the
/// first is active. A crashed/abandoned worker does not suppress the lane for a full cooldown:
/// the lease expires and the next scheduler pass can reclaim it. Any observed canary outcome
/// releases the lease transactionally with the policy update.
pub fn claim_cooldown_canary(
    paths: &AppPaths,
    provider: &str,
    operation: &str,
    auth_fingerprint: &str,
    runtime_epoch: &str,
    job_id: &str,
    claimed_at_ms: i64,
) -> Result<Option<String>> {
    let mut conn = db::write_context(paths)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute(
        "DELETE FROM downloader_canary_lease WHERE provider=?1 AND operation=?2 \
         AND auth_fingerprint=?3 AND runtime_epoch=?4 AND expires_at_ms<=?5",
        params![
            provider,
            operation,
            auth_fingerprint,
            runtime_epoch,
            claimed_at_ms,
        ],
    )?;
    let eligible = tx
        .query_row(
            "SELECT 1 FROM downloader_policy_state WHERE provider=?1 AND operation=?2 \
             AND auth_fingerprint=?3 AND runtime_epoch=?4 AND mode='cooldown' \
             AND next_eligible_probe_at_ms IS NOT NULL AND next_eligible_probe_at_ms<=?5",
            params![
                provider,
                operation,
                auth_fingerprint,
                runtime_epoch,
                claimed_at_ms
            ],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !eligible {
        tx.commit()?;
        return Ok(None);
    }
    if let Some(existing) = tx
        .query_row(
            "SELECT lease_id FROM downloader_canary_lease WHERE provider=?1 AND operation=?2 \
             AND auth_fingerprint=?3 AND runtime_epoch=?4 AND job_id=?5",
            params![provider, operation, auth_fingerprint, runtime_epoch, job_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
    {
        tx.commit()?;
        return Ok(Some(existing));
    }
    let lease_id = Uuid::new_v4().to_string();
    let inserted = tx.execute(
        "INSERT OR IGNORE INTO downloader_canary_lease(lease_id,job_id,provider,operation,auth_fingerprint,runtime_epoch,claimed_at_ms,expires_at_ms) \
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
        params![
            lease_id,
            job_id,
            provider,
            operation,
            auth_fingerprint,
            runtime_epoch,
            claimed_at_ms,
            claimed_at_ms.saturating_add(CANARY_LEASE_MS),
        ],
    )?;
    tx.commit()?;
    Ok((inserted == 1).then_some(lease_id))
}

pub fn release_cooldown_canary_for_job(paths: &AppPaths, job_id: &str) -> Result<u64> {
    db::AppDatabase::for_paths(paths)?.write(
        db::DatabaseOperationContext::new("youtube_download", "release_cooldown_canary"),
        TransactionBehavior::Immediate,
        |transaction| {
            Ok(transaction.execute(
                "DELETE FROM downloader_canary_lease WHERE job_id=?1",
                [job_id],
            )? as u64)
        },
    )
}

pub(crate) fn load_policy_state_conn(
    conn: &rusqlite::Connection,
    provider: &str,
    operation: &str,
    auth_fingerprint: &str,
    runtime_epoch: &str,
) -> Result<DownloaderPolicySnapshot> {
    let row = conn
        .query_row(
            "SELECT mode, corroboration_count, success_streak, entered_at_ms, last_evidence_at_ms, next_eligible_probe_at_ms, version FROM downloader_policy_state WHERE provider=?1 AND operation=?2 AND auth_fingerprint=?3 AND runtime_epoch=?4",
            params![provider, operation, auth_fingerprint, runtime_epoch],
            |row| {
                let mode: String = row.get(0)?;
                Ok(DownloaderPolicySnapshot {
                    provider: provider.to_string(),
                    operation: operation.to_string(),
                    auth_fingerprint: auth_fingerprint.to_string(),
                    runtime_epoch: runtime_epoch.to_string(),
                    mode: DownloaderPolicyMode::parse(&mode),
                    corroboration_count: row.get::<_, i64>(1)?.max(0) as u32,
                    success_streak: row.get::<_, i64>(2)?.max(0) as u32,
                    entered_at_ms: row.get(3)?,
                    last_evidence_at_ms: row.get(4)?,
                    next_eligible_probe_at_ms: row.get(5)?,
                    version: row.get::<_, i64>(6)?.max(1) as u64,
                })
            },
        )
        .optional()?;
    Ok(row.unwrap_or_else(|| DownloaderPolicySnapshot {
        provider: provider.to_string(),
        operation: operation.to_string(),
        auth_fingerprint: auth_fingerprint.to_string(),
        runtime_epoch: runtime_epoch.to_string(),
        mode: DownloaderPolicyMode::Normal,
        corroboration_count: 0,
        success_streak: 0,
        entered_at_ms: now_ms(),
        last_evidence_at_ms: None,
        next_eligible_probe_at_ms: None,
        version: 1,
    }))
}

fn transition_evidence_snapshot_conn(
    conn: &rusqlite::Connection,
    evidence_ids: &[String],
) -> Result<Vec<DownloaderTransitionEvidenceSnapshot>> {
    let mut snapshots = Vec::with_capacity(evidence_ids.len().min(4));
    let mut statement = conn.prepare(
        "SELECT id,target_fingerprint,occurred_at_ms,outcome_class,auth_fingerprint,runtime_epoch \
         FROM downloader_outcome WHERE id=?1",
    )?;
    for id in evidence_ids.iter().take(4) {
        if let Some(snapshot) = statement
            .query_row([id], |row| {
                Ok(DownloaderTransitionEvidenceSnapshot {
                    id: row.get(0)?,
                    target_fingerprint: row.get(1)?,
                    occurred_at_ms: row.get(2)?,
                    outcome_class: row.get(3)?,
                    auth_fingerprint: row.get(4)?,
                    runtime_epoch: row.get(5)?,
                })
            })
            .optional()?
        {
            snapshots.push(snapshot);
        }
    }
    Ok(snapshots)
}

pub fn record_outcome(
    paths: &AppPaths,
    input: RecordDownloaderOutcome<'_>,
) -> Result<DownloaderPolicySnapshot> {
    let mut conn = db::write_context(paths)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let tuning = load_tuning_conn(&tx)?;
    let outcome_id = Uuid::new_v4().to_string();
    let target_fingerprint = fingerprint(input.canonical_target);
    let error_signature = input.error_text.map(redacted_error_signature);
    tx.execute(
        "INSERT INTO downloader_outcome(id,provider,operation,target_fingerprint,auth_fingerprint,runtime_epoch,baseline_policy_json,effective_policy_json,occurred_at_ms,outcome_class,error_signature,incident_id,duration_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
        params![
            outcome_id,
            input.provider,
            input.operation,
            target_fingerprint,
            input.auth_fingerprint,
            input.runtime_epoch,
            serde_json::to_string(input.baseline)?,
            serde_json::to_string(input.effective)?,
            input.occurred_at_ms,
            input.outcome.as_str(),
            error_signature,
            input.incident_id,
            input.duration_ms,
        ],
    )?;

    let mut state = load_policy_state_conn(
        &tx,
        input.provider,
        input.operation,
        input.auth_fingerprint,
        input.runtime_epoch,
    )?;
    if let Some(job_id) = input.lease_owner_job_id {
        tx.execute(
            "DELETE FROM downloader_canary_lease WHERE provider=?1 AND operation=?2 \
             AND auth_fingerprint=?3 AND runtime_epoch=?4 AND job_id=?5",
            params![
                input.provider,
                input.operation,
                input.auth_fingerprint,
                input.runtime_epoch,
                job_id,
            ],
        )?;
    }
    let before = state.mode;
    let mut reason = None;
    let mut transition_evidence_ids = vec![outcome_id.clone()];
    match input.outcome {
        DownloaderOutcomeClass::AuthenticationRequiredOrInvalid
        | DownloaderOutcomeClass::PoTokenOrClientCapability => {
            state.mode = DownloaderPolicyMode::Hold;
            state.success_streak = 0;
            state.next_eligible_probe_at_ms = None;
            reason = Some(input.outcome.as_str());
        }
        DownloaderOutcomeClass::RateLimited => {
            let corroborated = tx
                .query_row(
                    "SELECT id FROM downloader_outcome WHERE provider=?1 AND operation=?2 AND auth_fingerprint=?3 AND runtime_epoch=?4 AND outcome_class='rate_limited' AND target_fingerprint<>?5 AND occurred_at_ms<=?6 AND occurred_at_ms>=?7 ORDER BY occurred_at_ms DESC LIMIT 1",
                    params![
                        input.provider,
                        input.operation,
                        input.auth_fingerprint,
                        input.runtime_epoch,
                        target_fingerprint,
                        input.occurred_at_ms - tuning.corroboration_min_separation_ms(),
                        input.occurred_at_ms - tuning.corroboration_window_ms(),
                    ],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            state.success_streak = 0;
            state.corroboration_count = state.corroboration_count.saturating_add(1);
            if let Some(corroborated_id) = corroborated {
                transition_evidence_ids.insert(0, corroborated_id);
                state.mode = match state.mode {
                    DownloaderPolicyMode::Normal => DownloaderPolicyMode::Cautious,
                    DownloaderPolicyMode::Cautious => DownloaderPolicyMode::Conservative,
                    DownloaderPolicyMode::Conservative | DownloaderPolicyMode::Cooldown => {
                        DownloaderPolicyMode::Cooldown
                    }
                    DownloaderPolicyMode::Hold => DownloaderPolicyMode::Hold,
                };
                state.next_eligible_probe_at_ms = match state.mode {
                    DownloaderPolicyMode::Cooldown => Some(
                        input
                            .occurred_at_ms
                            .saturating_add(tuning.cooldown_dwell_ms()),
                    ),
                    _ => None,
                };
                reason = Some("corroborated_rate_limited");
            }
        }
        DownloaderOutcomeClass::Success => {
            state.corroboration_count = 0;
            if state.mode == DownloaderPolicyMode::Cooldown && input.effective.canary_only {
                // One atomically claimed low-impact probe is the cooldown exit gate. Sustained
                // successes are still required for the later conservative -> cautious -> normal
                // recovery steps, so a single success never restores full baseline throughput.
                state.mode = DownloaderPolicyMode::Conservative;
                state.success_streak = 0;
                state.next_eligible_probe_at_ms = None;
                reason = Some("controlled_canary_success");
            }
            let dwell = match state.mode {
                DownloaderPolicyMode::Cautious => tuning.cautious_dwell_ms(),
                DownloaderPolicyMode::Conservative => tuning.conservative_dwell_ms(),
                DownloaderPolicyMode::Cooldown => tuning.cooldown_dwell_ms(),
                _ => 0,
            };
            if reason.is_none()
                && state.mode != DownloaderPolicyMode::Hold
                && input.occurred_at_ms.saturating_sub(state.entered_at_ms) >= dwell
            {
                state.success_streak = state.success_streak.saturating_add(1);
                if state.success_streak >= tuning.recovery_success_threshold {
                    state.mode = match state.mode {
                        DownloaderPolicyMode::Cooldown => DownloaderPolicyMode::Conservative,
                        DownloaderPolicyMode::Conservative => DownloaderPolicyMode::Cautious,
                        DownloaderPolicyMode::Cautious => DownloaderPolicyMode::Normal,
                        value => value,
                    };
                    state.success_streak = 0;
                    state.next_eligible_probe_at_ms = None;
                    reason = Some("sustained_success_recovery");
                }
            }
        }
        DownloaderOutcomeClass::ContentUnavailableOrPrivate
        | DownloaderOutcomeClass::NetworkTransient
        | DownloaderOutcomeClass::StorageOrLocalTool
        | DownloaderOutcomeClass::Unknown => {
            // Persist for review/rollup only. These classes must never train pacing.
        }
    }
    if before == DownloaderPolicyMode::Cooldown
        && input.effective.canary_only
        && input.outcome != DownloaderOutcomeClass::Success
    {
        state.next_eligible_probe_at_ms = Some(
            input
                .occurred_at_ms
                .saturating_add(tuning.cooldown_dwell_ms()),
        );
    }
    state.last_evidence_at_ms = Some(input.occurred_at_ms);
    if state.mode != before {
        state.entered_at_ms = input.occurred_at_ms;
        state.version = state.version.saturating_add(1);
        let evidence_snapshot = transition_evidence_snapshot_conn(&tx, &transition_evidence_ids)?;
        tx.execute(
            "INSERT INTO downloader_policy_transition(id,provider,operation,auth_fingerprint,runtime_epoch,before_mode,after_mode,reason,evidence_ids_json,evidence_snapshot_json,occurred_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![
                Uuid::new_v4().to_string(),
                input.provider,
                input.operation,
                input.auth_fingerprint,
                input.runtime_epoch,
                before.as_str(),
                state.mode.as_str(),
                reason.unwrap_or("classified_outcome"),
                serde_json::to_string(&transition_evidence_ids)?,
                serde_json::to_string(&evidence_snapshot)?,
                input.occurred_at_ms,
            ],
        )?;
    }
    tx.execute(
        "INSERT INTO downloader_policy_state(provider,operation,auth_fingerprint,runtime_epoch,mode,corroboration_count,success_streak,entered_at_ms,last_evidence_at_ms,next_eligible_probe_at_ms,version) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11) ON CONFLICT(provider,operation,auth_fingerprint,runtime_epoch) DO UPDATE SET mode=excluded.mode,corroboration_count=excluded.corroboration_count,success_streak=excluded.success_streak,entered_at_ms=excluded.entered_at_ms,last_evidence_at_ms=excluded.last_evidence_at_ms,next_eligible_probe_at_ms=excluded.next_eligible_probe_at_ms,version=excluded.version",
        params![
            state.provider,
            state.operation,
            state.auth_fingerprint,
            state.runtime_epoch,
            state.mode.as_str(),
            state.corroboration_count,
            state.success_streak,
            state.entered_at_ms,
            state.last_evidence_at_ms,
            state.next_eligible_probe_at_ms,
            state.version,
        ],
    )?;
    let day_utc = utc_day(input.occurred_at_ms);
    tx.execute(
        "INSERT INTO downloader_outcome_rollup(day_utc,provider,operation,auth_fingerprint,runtime_epoch,policy_mode,outcome_class,event_count,duration_ms_total,updated_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,1,?8,?9) ON CONFLICT(day_utc,provider,operation,auth_fingerprint,runtime_epoch,policy_mode,outcome_class) DO UPDATE SET event_count=event_count+1,duration_ms_total=duration_ms_total+excluded.duration_ms_total,updated_at_ms=excluded.updated_at_ms",
        params![
            day_utc,
            input.provider,
            input.operation,
            input.auth_fingerprint,
            input.runtime_epoch,
            input.effective.mode.as_str(),
            input.outcome.as_str(),
            input.duration_ms.unwrap_or(0).max(0),
            input.occurred_at_ms,
        ],
    )?;
    compact_outcomes_batch_conn(
        &tx,
        input
            .occurred_at_ms
            .saturating_sub(tuning.raw_retention_ms()),
        RAW_RETENTION_BATCH_SIZE,
    )?;
    tx.commit()?;
    Ok(state)
}

/// Persist classified evidence, duration, and durable rollups while automatic policy mutation is
/// disabled. The canonical policy state and transition history are intentionally untouched.
pub fn record_observation(
    paths: &AppPaths,
    input: RecordDownloaderOutcome<'_>,
) -> Result<DownloaderPolicySnapshot> {
    let mut conn = db::write_context(paths)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let tuning = load_tuning_conn(&tx)?;
    let state = load_policy_state_conn(
        &tx,
        input.provider,
        input.operation,
        input.auth_fingerprint,
        input.runtime_epoch,
    )?;
    let target_fingerprint = fingerprint(input.canonical_target);
    let error_signature = input.error_text.map(redacted_error_signature);
    tx.execute(
        "INSERT INTO downloader_outcome(id,provider,operation,target_fingerprint,auth_fingerprint,runtime_epoch,baseline_policy_json,effective_policy_json,occurred_at_ms,outcome_class,error_signature,incident_id,duration_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
        params![
            Uuid::new_v4().to_string(), input.provider, input.operation, target_fingerprint,
            input.auth_fingerprint, input.runtime_epoch, serde_json::to_string(input.baseline)?,
            serde_json::to_string(input.effective)?, input.occurred_at_ms, input.outcome.as_str(),
            error_signature, input.incident_id, input.duration_ms,
        ],
    )?;
    tx.execute(
        "INSERT INTO downloader_outcome_rollup(day_utc,provider,operation,auth_fingerprint,runtime_epoch,policy_mode,outcome_class,event_count,duration_ms_total,updated_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,1,?8,?9) ON CONFLICT(day_utc,provider,operation,auth_fingerprint,runtime_epoch,policy_mode,outcome_class) DO UPDATE SET event_count=event_count+1,duration_ms_total=duration_ms_total+excluded.duration_ms_total,updated_at_ms=excluded.updated_at_ms",
        params![utc_day(input.occurred_at_ms), input.provider, input.operation, input.auth_fingerprint,
            input.runtime_epoch, input.effective.mode.as_str(), input.outcome.as_str(),
            input.duration_ms.unwrap_or(0).max(0), input.occurred_at_ms],
    )?;
    compact_outcomes_batch_conn(
        &tx,
        input
            .occurred_at_ms
            .saturating_sub(tuning.raw_retention_ms()),
        RAW_RETENTION_BATCH_SIZE,
    )?;
    tx.commit()?;
    Ok(state)
}

fn compact_outcomes_batch_conn(
    conn: &rusqlite::Connection,
    cutoff_ms: i64,
    batch_size: usize,
) -> Result<DownloaderRetentionReceipt> {
    let batch_size = batch_size.clamp(1, 10_000) as i64;
    let deleted = conn.execute(
        "DELETE FROM downloader_outcome WHERE id IN (\
           SELECT id FROM downloader_outcome WHERE occurred_at_ms<?1 \
           ORDER BY occurred_at_ms,id LIMIT ?2\
         )",
        params![cutoff_ms, batch_size],
    )? as u64;
    let has_more = conn
        .query_row(
            "SELECT 1 FROM downloader_outcome WHERE occurred_at_ms<?1 LIMIT 1",
            [cutoff_ms],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    Ok(DownloaderRetentionReceipt {
        deleted,
        has_more,
        cutoff_ms,
    })
}

/// Performs one bounded, resumable raw-evidence retention batch. Durable rollups and transitions
/// are unaffected; callers may repeat while `has_more` is true after interruption.
pub fn compact_outcomes_batch(
    paths: &AppPaths,
    cutoff_ms: i64,
    batch_size: usize,
) -> Result<DownloaderRetentionReceipt> {
    let mut conn = db::write_context(paths)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let receipt = compact_outcomes_batch_conn(&tx, cutoff_ms, batch_size)?;
    tx.commit()?;
    Ok(receipt)
}

/// Drains expired raw evidence through short independent transactions. A non-zero delay is
/// required between continuation batches so startup maintenance cannot hot-loop or monopolize
/// the database. Every invocation has both a batch and wall-time budget; a background owner may
/// reschedule another invocation when `has_more` is true because the remaining rows are durable.
pub fn drain_expired_outcomes(
    paths: &AppPaths,
    now_ms: i64,
    inter_batch_delay_ms: u64,
    max_batches: usize,
    max_elapsed_ms: u64,
) -> Result<DownloaderRetentionDrainReceipt> {
    let tuning = get_tuning(paths)?;
    let cutoff_ms = now_ms.saturating_sub(tuning.raw_retention_ms());
    let batch_limit = max_batches.clamp(1, 10_000);
    let elapsed_budget = std::time::Duration::from_millis(max_elapsed_ms.clamp(25, 60_000));
    let started = std::time::Instant::now();
    let mut batches = 0usize;
    let mut deleted = 0u64;
    let mut has_more = true;
    while has_more && batches < batch_limit && (batches == 0 || started.elapsed() < elapsed_budget)
    {
        let receipt = compact_outcomes_batch(paths, cutoff_ms, RAW_RETENTION_BATCH_SIZE)?;
        batches = batches.saturating_add(1);
        deleted = deleted.saturating_add(receipt.deleted);
        has_more = receipt.has_more;
        if has_more && batches < batch_limit {
            std::thread::sleep(std::time::Duration::from_millis(
                inter_batch_delay_ms.max(25),
            ));
        }
    }
    Ok(DownloaderRetentionDrainReceipt {
        batches,
        deleted,
        complete: !has_more,
        has_more,
        cutoff_ms,
        elapsed_ms: started.elapsed().as_millis().min(u64::MAX as u128) as u64,
        budget_exhausted: has_more,
    })
}

pub fn retention_continuation(paths: &AppPaths) -> Result<DownloaderRetentionContinuation> {
    let conn = db::open_readonly(paths)?;
    conn.query_row(
        "SELECT pending,consecutive_failures,updated_at_ms FROM youtube_retention_continuation WHERE singleton=1",
        [],
        |row| {
            Ok(DownloaderRetentionContinuation {
                pending: row.get::<_, i64>(0)? != 0,
                consecutive_failures: row.get::<_, i64>(1)?.max(0) as u32,
                updated_at_ms: row.get(2)?,
            })
        },
    )
    .map_err(Into::into)
}

pub fn persist_retention_continuation(
    paths: &AppPaths,
    pending: bool,
    consecutive_failures: u32,
) -> Result<DownloaderRetentionContinuation> {
    let updated_at_ms = now_ms();
    db::AppDatabase::for_paths(paths)?.write(
        db::DatabaseOperationContext::new("youtube_retention", "persist_continuation"),
        TransactionBehavior::Immediate,
        |transaction| {
            transaction.execute(
                "INSERT INTO youtube_retention_continuation(singleton,pending,consecutive_failures,updated_at_ms) VALUES(1,?1,?2,?3) ON CONFLICT(singleton) DO UPDATE SET pending=excluded.pending,consecutive_failures=excluded.consecutive_failures,updated_at_ms=excluded.updated_at_ms",
                params![if pending { 1 } else { 0 }, consecutive_failures, updated_at_ms],
            )?;
            Ok(())
        },
    )?;
    Ok(DownloaderRetentionContinuation {
        pending,
        consecutive_failures,
        updated_at_ms,
    })
}

pub fn return_to_baseline(
    paths: &AppPaths,
    provider: &str,
    operation: &str,
    auth_fingerprint: &str,
    runtime_epoch: &str,
) -> Result<DownloaderPolicySnapshot> {
    let mut conn = db::write_context(paths)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut state =
        load_policy_state_conn(&tx, provider, operation, auth_fingerprint, runtime_epoch)?;
    let before = state.mode;
    let now = now_ms();
    state.mode = DownloaderPolicyMode::Normal;
    state.corroboration_count = 0;
    state.success_streak = 0;
    state.entered_at_ms = now;
    state.last_evidence_at_ms = Some(now);
    state.next_eligible_probe_at_ms = None;
    state.version = state.version.saturating_add(1);
    tx.execute(
        "INSERT INTO downloader_policy_transition(id,provider,operation,auth_fingerprint,runtime_epoch,before_mode,after_mode,reason,evidence_ids_json,occurred_at_ms) VALUES(?1,?2,?3,?4,?5,?6,'normal','operator_return_to_baseline','[]',?7)",
        params![Uuid::new_v4().to_string(), provider, operation, auth_fingerprint, runtime_epoch, before.as_str(), now],
    )?;
    tx.execute(
        "INSERT INTO downloader_policy_state(provider,operation,auth_fingerprint,runtime_epoch,mode,corroboration_count,success_streak,entered_at_ms,last_evidence_at_ms,next_eligible_probe_at_ms,version) VALUES(?1,?2,?3,?4,'normal',0,0,?5,?5,NULL,?6) ON CONFLICT(provider,operation,auth_fingerprint,runtime_epoch) DO UPDATE SET mode='normal',corroboration_count=0,success_streak=0,entered_at_ms=excluded.entered_at_ms,last_evidence_at_ms=excluded.last_evidence_at_ms,next_eligible_probe_at_ms=NULL,version=excluded.version",
        params![provider, operation, auth_fingerprint, runtime_epoch, now, state.version],
    )?;
    tx.commit()?;
    Ok(state)
}

pub fn policy_history(
    paths: &AppPaths,
    provider: &str,
    operation: &str,
    auth_fingerprint: &str,
    runtime_epoch: &str,
    limit: usize,
) -> Result<DownloaderPolicyHistory> {
    let conn = db::open_readonly(paths)?;
    policy_history_conn(
        &conn,
        provider,
        operation,
        auth_fingerprint,
        runtime_epoch,
        limit,
    )
}

pub(crate) fn policy_history_conn(
    conn: &rusqlite::Connection,
    provider: &str,
    operation: &str,
    auth_fingerprint: &str,
    runtime_epoch: &str,
    limit: usize,
) -> Result<DownloaderPolicyHistory> {
    let limit = limit.clamp(1, 500) as i64;
    let outcomes = {
        let mut statement = conn.prepare(
            "SELECT id,target_fingerprint,occurred_at_ms,outcome_class,error_signature,incident_id,duration_ms,baseline_policy_json,effective_policy_json FROM downloader_outcome WHERE provider=?1 AND operation=?2 AND auth_fingerprint=?3 AND runtime_epoch=?4 ORDER BY occurred_at_ms DESC,id DESC LIMIT ?5",
        )?;
        let rows = statement
            .query_map(
                params![provider, operation, auth_fingerprint, runtime_epoch, limit],
                |row| {
                    let baseline_json: String = row.get(7)?;
                    let effective_json: String = row.get(8)?;
                    let baseline_policy =
                        serde_json::from_str(&baseline_json).map_err(|error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                7,
                                rusqlite::types::Type::Text,
                                Box::new(error),
                            )
                        })?;
                    let effective_policy =
                        serde_json::from_str(&effective_json).map_err(|error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                8,
                                rusqlite::types::Type::Text,
                                Box::new(error),
                            )
                        })?;
                    Ok(DownloaderOutcomeSummary {
                        id: row.get(0)?,
                        target_fingerprint: row.get(1)?,
                        occurred_at_ms: row.get(2)?,
                        outcome_class: row.get(3)?,
                        error_signature: row.get(4)?,
                        incident_id: row.get(5)?,
                        duration_ms: row.get(6)?,
                        baseline_policy,
                        effective_policy,
                    })
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    let transitions = {
        let mut statement = conn.prepare(
            "SELECT id,before_mode,after_mode,reason,evidence_ids_json,evidence_snapshot_json,occurred_at_ms FROM downloader_policy_transition WHERE provider=?1 AND operation=?2 AND auth_fingerprint=?3 AND runtime_epoch=?4 ORDER BY occurred_at_ms DESC,id DESC LIMIT ?5",
        )?;
        let rows = statement
            .query_map(
                params![provider, operation, auth_fingerprint, runtime_epoch, limit],
                |row| {
                    let evidence_json: String = row.get(4)?;
                    let evidence_snapshot_json: String = row.get(5)?;
                    Ok(DownloaderTransitionSummary {
                        id: row.get(0)?,
                        before_mode: row.get(1)?,
                        after_mode: row.get(2)?,
                        reason: row.get(3)?,
                        evidence_ids: serde_json::from_str(&evidence_json).unwrap_or_default(),
                        evidence_snapshot: serde_json::from_str(&evidence_snapshot_json)
                            .unwrap_or_default(),
                        occurred_at_ms: row.get(6)?,
                    })
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    let raw_total = conn.query_row(
        "SELECT COUNT(*) FROM downloader_outcome WHERE provider=?1 AND operation=?2 AND auth_fingerprint=?3 AND runtime_epoch=?4",
        params![provider, operation, auth_fingerprint, runtime_epoch],
        |row| row.get::<_, i64>(0),
    )?.max(0) as u64;
    let transition_total = conn.query_row(
        "SELECT COUNT(*) FROM downloader_policy_transition WHERE provider=?1 AND operation=?2 AND auth_fingerprint=?3 AND runtime_epoch=?4",
        params![provider, operation, auth_fingerprint, runtime_epoch],
        |row| row.get::<_, i64>(0),
    )?.max(0) as u64;
    let rollup_event_total = conn.query_row(
        "SELECT COALESCE(SUM(event_count),0) FROM downloader_outcome_rollup WHERE provider=?1 AND operation=?2 AND auth_fingerprint=?3 AND runtime_epoch=?4",
        params![provider, operation, auth_fingerprint, runtime_epoch],
        |row| row.get::<_, i64>(0),
    )?.max(0) as u64;
    let unknown_total = conn.query_row(
        "SELECT COALESCE(SUM(event_count),0) FROM downloader_outcome_rollup WHERE provider=?1 AND operation=?2 AND auth_fingerprint=?3 AND runtime_epoch=?4 AND outcome_class='unknown'",
        params![provider, operation, auth_fingerprint, runtime_epoch],
        |row| row.get::<_, i64>(0),
    )?.max(0) as u64;
    let class_totals = {
        let mut statement = conn.prepare(
            "SELECT outcome_class, SUM(event_count) FROM downloader_outcome_rollup WHERE provider=?1 AND operation=?2 AND auth_fingerprint=?3 AND runtime_epoch=?4 GROUP BY outcome_class ORDER BY outcome_class",
        )?;
        let rows = statement
            .query_map(
                params![provider, operation, auth_fingerprint, runtime_epoch],
                |row| {
                    Ok(DownloaderOutcomeClassTotal {
                        outcome_class: row.get(0)?,
                        event_count: row.get::<_, i64>(1)?.max(0) as u64,
                    })
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    Ok(DownloaderPolicyHistory {
        outcomes,
        transitions,
        raw_total,
        transition_total,
        rollup_event_total,
        unknown_total,
        class_totals,
    })
}

fn outcome_summary_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<DownloaderOutcomeSummary> {
    let baseline_json: String = row.get(7)?;
    let effective_json: String = row.get(8)?;
    let baseline_policy = serde_json::from_str(&baseline_json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(7, rusqlite::types::Type::Text, Box::new(error))
    })?;
    let effective_policy = serde_json::from_str(&effective_json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(8, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(DownloaderOutcomeSummary {
        id: row.get(0)?,
        target_fingerprint: row.get(1)?,
        occurred_at_ms: row.get(2)?,
        outcome_class: row.get(3)?,
        error_signature: row.get(4)?,
        incident_id: row.get(5)?,
        duration_ms: row.get(6)?,
        baseline_policy,
        effective_policy,
    })
}

/// Returns a stable keyset-paginated slice of retained raw outcomes. The cursor includes the
/// UUID tie-breaker so equal timestamps cannot duplicate or skip rows between pages.
pub fn policy_outcomes_page(
    paths: &AppPaths,
    provider: &str,
    operation: &str,
    auth_fingerprint: &str,
    runtime_epoch: &str,
    cursor: Option<&DownloaderHistoryCursor>,
    limit: usize,
) -> Result<DownloaderOutcomePage> {
    let conn = db::open_readonly(paths)?;
    let page_size = limit.clamp(1, 1_000);
    let query_limit = page_size.saturating_add(1) as i64;
    let select = "SELECT id,target_fingerprint,occurred_at_ms,outcome_class,error_signature,incident_id,duration_ms,baseline_policy_json,effective_policy_json FROM downloader_outcome";
    let mut outcomes = if let Some(cursor) = cursor {
        let mut statement = conn.prepare(&format!(
            "{select} WHERE provider=?1 AND operation=?2 AND auth_fingerprint=?3 AND runtime_epoch=?4 \
             AND (occurred_at_ms<?5 OR (occurred_at_ms=?5 AND id<?6)) \
             ORDER BY occurred_at_ms DESC,id DESC LIMIT ?7"
        ))?;
        let rows = statement
            .query_map(
                params![
                    provider,
                    operation,
                    auth_fingerprint,
                    runtime_epoch,
                    cursor.occurred_at_ms,
                    cursor.id,
                    query_limit,
                ],
                outcome_summary_from_row,
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    } else {
        let mut statement = conn.prepare(&format!(
            "{select} WHERE provider=?1 AND operation=?2 AND auth_fingerprint=?3 AND runtime_epoch=?4 \
             ORDER BY occurred_at_ms DESC,id DESC LIMIT ?5"
        ))?;
        let rows = statement
            .query_map(
                params![
                    provider,
                    operation,
                    auth_fingerprint,
                    runtime_epoch,
                    query_limit
                ],
                outcome_summary_from_row,
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    let has_more = outcomes.len() > page_size;
    outcomes.truncate(page_size);
    let next_cursor = if has_more {
        outcomes.last().map(|row| DownloaderHistoryCursor {
            occurred_at_ms: row.occurred_at_ms,
            id: row.id.clone(),
        })
    } else {
        None
    };
    let raw_total = conn
        .query_row(
            "SELECT COUNT(*) FROM downloader_outcome WHERE provider=?1 AND operation=?2 AND auth_fingerprint=?3 AND runtime_epoch=?4",
            params![provider, operation, auth_fingerprint, runtime_epoch],
            |row| row.get::<_, i64>(0),
        )?
        .max(0) as u64;
    Ok(DownloaderOutcomePage {
        outcomes,
        next_cursor,
        has_more,
        raw_total,
    })
}

#[cfg(test)]
fn policy_outcomes_all(
    paths: &AppPaths,
    provider: &str,
    operation: &str,
    auth_fingerprint: &str,
    runtime_epoch: &str,
) -> Result<Vec<DownloaderOutcomeSummary>> {
    let mut outcomes = Vec::new();
    let mut cursor = None;
    loop {
        let page = policy_outcomes_page(
            paths,
            provider,
            operation,
            auth_fingerprint,
            runtime_epoch,
            cursor.as_ref(),
            1_000,
        )?;
        outcomes.extend(page.outcomes);
        if !page.has_more {
            break;
        }
        cursor = page.next_cursor;
    }
    Ok(outcomes)
}

pub fn policy_transitions_page(
    paths: &AppPaths,
    provider: &str,
    operation: &str,
    auth_fingerprint: &str,
    runtime_epoch: &str,
    cursor: Option<&DownloaderHistoryCursor>,
    limit: usize,
) -> Result<DownloaderTransitionPage> {
    let conn = db::open_readonly(paths)?;
    let page_size = limit.clamp(1, 1_000);
    let query_limit = page_size.saturating_add(1) as i64;
    let select = "SELECT id,before_mode,after_mode,reason,evidence_ids_json,evidence_snapshot_json,occurred_at_ms FROM downloader_policy_transition";
    let collect = |row: &rusqlite::Row<'_>| -> rusqlite::Result<DownloaderTransitionSummary> {
        let evidence_json: String = row.get(4)?;
        let snapshot_json: String = row.get(5)?;
        Ok(DownloaderTransitionSummary {
            id: row.get(0)?,
            before_mode: row.get(1)?,
            after_mode: row.get(2)?,
            reason: row.get(3)?,
            evidence_ids: serde_json::from_str(&evidence_json).unwrap_or_default(),
            evidence_snapshot: serde_json::from_str(&snapshot_json).unwrap_or_default(),
            occurred_at_ms: row.get(6)?,
        })
    };
    let mut transitions = if let Some(cursor) = cursor {
        let mut statement = conn.prepare(&format!(
            "{select} WHERE provider=?1 AND operation=?2 AND auth_fingerprint=?3 AND runtime_epoch=?4 \
             AND (occurred_at_ms<?5 OR (occurred_at_ms=?5 AND id<?6)) \
             ORDER BY occurred_at_ms DESC,id DESC LIMIT ?7"
        ))?;
        let rows = statement
            .query_map(
                params![
                    provider,
                    operation,
                    auth_fingerprint,
                    runtime_epoch,
                    cursor.occurred_at_ms,
                    cursor.id,
                    query_limit
                ],
                collect,
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    } else {
        let mut statement = conn.prepare(&format!(
            "{select} WHERE provider=?1 AND operation=?2 AND auth_fingerprint=?3 AND runtime_epoch=?4 \
             ORDER BY occurred_at_ms DESC,id DESC LIMIT ?5"
        ))?;
        let rows = statement
            .query_map(
                params![
                    provider,
                    operation,
                    auth_fingerprint,
                    runtime_epoch,
                    query_limit
                ],
                collect,
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    let has_more = transitions.len() > page_size;
    transitions.truncate(page_size);
    let next_cursor =
        has_more
            .then(|| transitions.last())
            .flatten()
            .map(|row| DownloaderHistoryCursor {
                occurred_at_ms: row.occurred_at_ms,
                id: row.id.clone(),
            });
    let transition_total = conn.query_row(
        "SELECT COUNT(*) FROM downloader_policy_transition WHERE provider=?1 AND operation=?2 AND auth_fingerprint=?3 AND runtime_epoch=?4",
        params![provider, operation, auth_fingerprint, runtime_epoch],
        |row| row.get::<_, i64>(0),
    )?.max(0) as u64;
    Ok(DownloaderTransitionPage {
        transitions,
        next_cursor,
        has_more,
        transition_total,
    })
}

pub fn replay_policy_history_from_store(
    paths: &AppPaths,
    provider: &str,
    operation: &str,
    auth_fingerprint: &str,
    runtime_epoch: &str,
) -> Result<DownloaderPolicyReplayReceipt> {
    let history = policy_history(
        paths,
        provider,
        operation,
        auth_fingerprint,
        runtime_epoch,
        1,
    )?;
    let conn = db::open_readonly(paths)?;
    let mut mode = DownloaderPolicyMode::Normal;
    let mut mode_path = vec![mode];
    let mut mode_path_truncated = false;
    let mut transitions_replayed = 0_u64;
    let mut evidence_complete = true;
    let mut cursor: Option<(i64, String)> = None;
    loop {
        let mut statement = if cursor.is_some() {
            conn.prepare(
                "SELECT id,after_mode,reason,evidence_snapshot_json,occurred_at_ms \
                 FROM downloader_policy_transition WHERE provider=?1 AND operation=?2 \
                   AND auth_fingerprint=?3 AND runtime_epoch=?4 \
                   AND (occurred_at_ms>?5 OR (occurred_at_ms=?5 AND id>?6)) \
                 ORDER BY occurred_at_ms ASC,id ASC LIMIT 1000",
            )?
        } else {
            conn.prepare(
                "SELECT id,after_mode,reason,evidence_snapshot_json,occurred_at_ms \
                 FROM downloader_policy_transition WHERE provider=?1 AND operation=?2 \
                   AND auth_fingerprint=?3 AND runtime_epoch=?4 \
                 ORDER BY occurred_at_ms ASC,id ASC LIMIT 1000",
            )?
        };
        let rows = if let Some((occurred_at_ms, id)) = cursor.as_ref() {
            statement
                .query_map(
                    params![
                        provider,
                        operation,
                        auth_fingerprint,
                        runtime_epoch,
                        occurred_at_ms,
                        id
                    ],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, i64>(4)?,
                        ))
                    },
                )?
                .collect::<rusqlite::Result<Vec<_>>>()?
        } else {
            statement
                .query_map(
                    params![provider, operation, auth_fingerprint, runtime_epoch],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, i64>(4)?,
                        ))
                    },
                )?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        if rows.is_empty() {
            break;
        }
        for (id, after_mode, reason, evidence_json, occurred_at_ms) in &rows {
            let snapshots: Vec<DownloaderTransitionEvidenceSnapshot> =
                serde_json::from_str(evidence_json).unwrap_or_default();
            if reason != "operator_return_to_baseline"
                && (snapshots.is_empty()
                    || snapshots.iter().any(|snapshot| {
                        snapshot.auth_fingerprint != auth_fingerprint
                            || snapshot.runtime_epoch != runtime_epoch
                    }))
            {
                evidence_complete = false;
            }
            mode = DownloaderPolicyMode::parse(after_mode);
            transitions_replayed = transitions_replayed.saturating_add(1);
            if mode_path.len() == 256 {
                mode_path.remove(0);
                mode_path_truncated = true;
            }
            mode_path.push(mode);
            cursor = Some((*occurred_at_ms, id.clone()));
        }
        if rows.len() < 1_000 {
            break;
        }
    }
    Ok(DownloaderPolicyReplayReceipt {
        events_replayed: history.rollup_event_total.min(usize::MAX as u64) as usize,
        unknown_events: history.unknown_total.min(usize::MAX as u64) as usize,
        final_mode: mode,
        mode_path,
        mode_path_truncated,
        transitions_replayed,
        complete: evidence_complete && history.rollup_event_total >= history.raw_total,
        truncated: !evidence_complete,
        retained_raw_total: history.raw_total,
        durable_rollup_total: history.rollup_event_total,
    })
}

pub fn reset_policy_history(
    paths: &AppPaths,
    provider: &str,
    operation: &str,
    auth_fingerprint: &str,
    runtime_epoch: &str,
) -> Result<DownloaderHistoryResetReceipt> {
    reset_policy_history_internal(
        paths,
        provider,
        operation,
        auth_fingerprint,
        runtime_epoch,
        None,
    )
}

fn reset_policy_history_internal(
    paths: &AppPaths,
    provider: &str,
    operation: &str,
    auth_fingerprint: &str,
    runtime_epoch: &str,
    mutation_generation: Option<u64>,
) -> Result<DownloaderHistoryResetReceipt> {
    let _reset_guard = HISTORY_RESET_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut conn = db::write_context(paths)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if let Some(generation) = mutation_generation {
        let continuation_active = tx
            .query_row(
                "SELECT 1 FROM downloader_history_reset WHERE provider=?1 AND operation=?2 AND auth_fingerprint=?3 AND runtime_epoch=?4 LIMIT 1",
                params![provider, operation, auth_fingerprint, runtime_epoch],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        claim_mutation_generation_conn(
            &tx,
            &format!("history_reset:{operation}"),
            generation,
            continuation_active,
        )?;
    }
    let existing = tx
        .query_row(
            "SELECT reset_id,outcome_max_rowid,transition_max_rowid,outcomes_deleted,transitions_deleted,rollups_deleted,states_deleted,leases_deleted \
             FROM downloader_history_reset WHERE provider=?1 AND operation=?2 AND auth_fingerprint=?3 AND runtime_epoch=?4",
            params![provider, operation, auth_fingerprint, runtime_epoch],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?, row.get::<_, i64>(3)?, row.get::<_, i64>(4)?, row.get::<_, i64>(5)?, row.get::<_, i64>(6)?, row.get::<_, i64>(7)?)),
        )
        .optional()?;
    let (
        reset_id,
        outcome_max_rowid,
        transition_max_rowid,
        mut outcomes_deleted,
        mut transitions_deleted,
        rollups_deleted,
        states_deleted,
        leases_deleted,
    ) = if let Some(row) = existing {
        row
    } else {
        let reset_id = Uuid::new_v4().to_string();
        let outcome_max_rowid = tx.query_row(
            "SELECT COALESCE(MAX(rowid),0) FROM downloader_outcome WHERE provider=?1 AND operation=?2 AND auth_fingerprint=?3 AND runtime_epoch=?4",
            params![provider, operation, auth_fingerprint, runtime_epoch], |row| row.get(0))?;
        let transition_max_rowid = tx.query_row(
            "SELECT COALESCE(MAX(rowid),0) FROM downloader_policy_transition WHERE provider=?1 AND operation=?2 AND auth_fingerprint=?3 AND runtime_epoch=?4",
            params![provider, operation, auth_fingerprint, runtime_epoch], |row| row.get(0))?;
        let rollups_deleted = tx.execute(
            "DELETE FROM downloader_outcome_rollup WHERE provider=?1 AND operation=?2 AND auth_fingerprint=?3 AND runtime_epoch=?4",
            params![provider, operation, auth_fingerprint, runtime_epoch])? as i64;
        let states_deleted = tx.execute(
            "DELETE FROM downloader_policy_state WHERE provider=?1 AND operation=?2 AND auth_fingerprint=?3 AND runtime_epoch=?4",
            params![provider, operation, auth_fingerprint, runtime_epoch])? as i64;
        let leases_deleted = tx.execute(
            "DELETE FROM downloader_canary_lease WHERE provider=?1 AND operation=?2 AND auth_fingerprint=?3 AND runtime_epoch=?4",
            params![provider, operation, auth_fingerprint, runtime_epoch])? as i64;
        tx.execute(
            "INSERT INTO downloader_history_reset(reset_id,provider,operation,auth_fingerprint,runtime_epoch,outcome_max_rowid,transition_max_rowid,rollups_deleted,states_deleted,leases_deleted) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![reset_id, provider, operation, auth_fingerprint, runtime_epoch, outcome_max_rowid, transition_max_rowid, rollups_deleted, states_deleted, leases_deleted])?;
        (
            reset_id,
            outcome_max_rowid,
            transition_max_rowid,
            0,
            0,
            rollups_deleted,
            states_deleted,
            leases_deleted,
        )
    };
    outcomes_deleted += tx.execute(
        "DELETE FROM downloader_outcome WHERE rowid IN (SELECT rowid FROM downloader_outcome WHERE provider=?1 AND operation=?2 AND auth_fingerprint=?3 AND runtime_epoch=?4 AND rowid<=?5 ORDER BY rowid LIMIT 1000)",
        params![provider, operation, auth_fingerprint, runtime_epoch, outcome_max_rowid])? as i64;
    transitions_deleted += tx.execute(
        "DELETE FROM downloader_policy_transition WHERE rowid IN (SELECT rowid FROM downloader_policy_transition WHERE provider=?1 AND operation=?2 AND auth_fingerprint=?3 AND runtime_epoch=?4 AND rowid<=?5 ORDER BY rowid LIMIT 1000)",
        params![provider, operation, auth_fingerprint, runtime_epoch, transition_max_rowid])? as i64;
    let outcomes_more = tx.query_row(
        "SELECT 1 FROM downloader_outcome WHERE provider=?1 AND operation=?2 AND auth_fingerprint=?3 AND runtime_epoch=?4 AND rowid<=?5 LIMIT 1",
        params![provider, operation, auth_fingerprint, runtime_epoch, outcome_max_rowid], |_| Ok(())).optional()?.is_some();
    let transitions_more = tx.query_row(
        "SELECT 1 FROM downloader_policy_transition WHERE provider=?1 AND operation=?2 AND auth_fingerprint=?3 AND runtime_epoch=?4 AND rowid<=?5 LIMIT 1",
        params![provider, operation, auth_fingerprint, runtime_epoch, transition_max_rowid], |_| Ok(())).optional()?.is_some();
    let has_more = outcomes_more || transitions_more;
    if has_more {
        tx.execute(
            "UPDATE downloader_history_reset SET outcomes_deleted=?2,transitions_deleted=?3 WHERE reset_id=?1",
            params![reset_id, outcomes_deleted, transitions_deleted])?;
    } else {
        tx.execute(
            "DELETE FROM downloader_history_reset WHERE reset_id=?1",
            [&reset_id],
        )?;
    }
    tx.commit()?;
    Ok(DownloaderHistoryResetReceipt {
        reset_id,
        complete: !has_more,
        has_more,
        outcomes_deleted: outcomes_deleted.max(0) as u64,
        transitions_deleted: transitions_deleted.max(0) as u64,
        rollups_deleted: rollups_deleted.max(0) as u64,
        states_deleted: states_deleted.max(0) as u64,
        leases_deleted: leases_deleted.max(0) as u64,
    })
}

pub fn reset_policy_history_with_generation(
    paths: &AppPaths,
    provider: &str,
    operation: &str,
    auth_fingerprint: &str,
    runtime_epoch: &str,
    mutation_generation: u64,
) -> Result<DownloaderHistoryResetReceipt> {
    reset_policy_history_internal(
        paths,
        provider,
        operation,
        auth_fingerprint,
        runtime_epoch,
        Some(mutation_generation),
    )
}

pub fn replay_policy_history(history: &DownloaderPolicyHistory) -> DownloaderPolicyReplayReceipt {
    replay_policy_history_with_tuning(history, &YoutubeProtectionTuning::default())
}

pub fn replay_policy_history_with_tuning(
    history: &DownloaderPolicyHistory,
    tuning: &YoutubeProtectionTuning,
) -> DownloaderPolicyReplayReceipt {
    let tuning = tuning.clone().normalized();
    let mut ordered = history.outcomes.clone();
    ordered.sort_by_key(|event| event.occurred_at_ms);
    let mut mode = DownloaderPolicyMode::Normal;
    let mut entered_at_ms = ordered
        .first()
        .map(|event| event.occurred_at_ms)
        .unwrap_or(0);
    let mut success_streak = 0_u32;
    let mut prior_rate_events: Vec<(String, i64)> = Vec::new();
    let mut mode_path = vec![mode];
    let mut unknown_events = 0;
    for event in &ordered {
        let class = parse_outcome_class(&event.outcome_class);
        let before = mode;
        match class {
            DownloaderOutcomeClass::AuthenticationRequiredOrInvalid
            | DownloaderOutcomeClass::PoTokenOrClientCapability => {
                mode = DownloaderPolicyMode::Hold;
                success_streak = 0;
            }
            DownloaderOutcomeClass::RateLimited => {
                let corroborated = prior_rate_events.iter().any(|(target, at)| {
                    target != &event.target_fingerprint
                        && event.occurred_at_ms.saturating_sub(*at)
                            >= tuning.corroboration_min_separation_ms()
                        && event.occurred_at_ms.saturating_sub(*at)
                            <= tuning.corroboration_window_ms()
                });
                prior_rate_events.push((event.target_fingerprint.clone(), event.occurred_at_ms));
                success_streak = 0;
                if corroborated {
                    mode = match mode {
                        DownloaderPolicyMode::Normal => DownloaderPolicyMode::Cautious,
                        DownloaderPolicyMode::Cautious => DownloaderPolicyMode::Conservative,
                        DownloaderPolicyMode::Conservative | DownloaderPolicyMode::Cooldown => {
                            DownloaderPolicyMode::Cooldown
                        }
                        DownloaderPolicyMode::Hold => DownloaderPolicyMode::Hold,
                    };
                }
            }
            DownloaderOutcomeClass::Success if mode != DownloaderPolicyMode::Hold => {
                if mode == DownloaderPolicyMode::Cooldown && event.effective_policy.canary_only {
                    mode = DownloaderPolicyMode::Conservative;
                    success_streak = 0;
                    if mode != before {
                        entered_at_ms = event.occurred_at_ms;
                        mode_path.push(mode);
                    }
                    continue;
                }
                let dwell = match mode {
                    DownloaderPolicyMode::Cautious => tuning.cautious_dwell_ms(),
                    DownloaderPolicyMode::Conservative => tuning.conservative_dwell_ms(),
                    DownloaderPolicyMode::Cooldown => tuning.cooldown_dwell_ms(),
                    _ => 0,
                };
                if event.occurred_at_ms.saturating_sub(entered_at_ms) >= dwell {
                    success_streak = success_streak.saturating_add(1);
                    if success_streak >= tuning.recovery_success_threshold {
                        mode = match mode {
                            DownloaderPolicyMode::Cooldown => DownloaderPolicyMode::Conservative,
                            DownloaderPolicyMode::Conservative => DownloaderPolicyMode::Cautious,
                            DownloaderPolicyMode::Cautious => DownloaderPolicyMode::Normal,
                            value => value,
                        };
                        success_streak = 0;
                    }
                }
            }
            DownloaderOutcomeClass::Unknown => unknown_events += 1,
            _ => {}
        }
        if mode != before {
            entered_at_ms = event.occurred_at_ms;
            mode_path.push(mode);
        }
    }
    DownloaderPolicyReplayReceipt {
        events_replayed: ordered.len(),
        unknown_events,
        final_mode: mode,
        mode_path,
        mode_path_truncated: false,
        transitions_replayed: history.transition_total,
        complete: history.outcomes.len() == history.raw_total as usize
            && history.raw_total == history.rollup_event_total,
        truncated: history.outcomes.len() < history.raw_total as usize
            || history.raw_total < history.rollup_event_total,
        retained_raw_total: history.raw_total,
        durable_rollup_total: history.rollup_event_total,
    }
}

fn parse_outcome_class(value: &str) -> DownloaderOutcomeClass {
    match value {
        "rate_limited" => DownloaderOutcomeClass::RateLimited,
        "po_token_or_client_capability" => DownloaderOutcomeClass::PoTokenOrClientCapability,
        "authentication_required_or_invalid" => {
            DownloaderOutcomeClass::AuthenticationRequiredOrInvalid
        }
        "content_unavailable_or_private" => DownloaderOutcomeClass::ContentUnavailableOrPrivate,
        "network_transient" => DownloaderOutcomeClass::NetworkTransient,
        "storage_or_local_tool" => DownloaderOutcomeClass::StorageOrLocalTool,
        "success" => DownloaderOutcomeClass::Success,
        _ => DownloaderOutcomeClass::Unknown,
    }
}

fn fingerprint(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.trim().as_bytes());
    hex::encode(hasher.finalize())
}

fn redacted_error_signature(value: &str) -> String {
    let normalized = value
        .split_whitespace()
        .take(24)
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    fingerprint(&normalized)
}

fn utc_day(timestamp_ms: i64) -> String {
    // Day number is locale-independent and sufficient as a compact rollup key. The `utc-`
    // prefix prevents consumers from mistaking it for local calendar time.
    format!("utc-{}", timestamp_ms.div_euclid(86_400_000))
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn paths() -> (TempDir, AppPaths) {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("appdata"));
        paths.ensure_dirs().expect("test app-data directories");
        let conn = crate::db::open(&paths).expect("test database");
        crate::db::migrate(&conn).expect("test schema");
        drop(conn);
        (dir, paths)
    }

    fn baseline() -> DownloaderBaselinePolicy {
        DownloaderBaselinePolicy {
            concurrent_fragments: 8,
            sleep_interval_secs: 5,
            sleep_requests_secs: 0,
            update_tranche_size: 25,
            limit_rate: Some("4M".to_string()),
            throttled_rate: Some("100K".to_string()),
        }
    }

    #[test]
    fn runtime_epoch_excludes_health_and_caches_hash_work_until_install_metadata_changes() {
        let payload = immutable_runtime_epoch_payload(
            true,
            Some("2026.07.04"),
            Some("YT_HASH"),
            Some("v24.19.0"),
            Some("11.17.0"),
            Some("NODE_HASH"),
            Some("NPM_HASH"),
            "1.3.1",
            true,
            Some("PLUGIN_HASH"),
            Some("SERVER_HASH"),
            Some("LOCK_HASH"),
            Some("NODE_MODULES_HASH"),
        );
        assert!(payload.get("provider_running").is_none());
        assert!(payload.get("provider_healthy").is_none());
        let restart_epoch = fingerprint(&serde_json::to_string(&payload).unwrap());
        let health_flap_epoch = fingerprint(&serde_json::to_string(&payload).unwrap());
        assert_eq!(restart_epoch, health_flap_epoch);

        let (_dir, paths) = paths();
        let cache_misses = || {
            runtime_identity_cache_misses()
                .lock()
                .unwrap()
                .get(&paths.base_dir)
                .copied()
                .unwrap_or(0)
        };
        let before = cache_misses();
        let first = runtime_epoch_for_paths(&paths);
        let after_first = cache_misses();
        let second = runtime_epoch_for_paths(&paths);
        let after_second = cache_misses();
        assert_eq!(first, second);
        assert_eq!(after_first, before + 1);
        assert_eq!(
            after_second, after_first,
            "unchanged identity must not rehash"
        );

        let yt_dlp_dir = paths.tools_dir().join("yt-dlp");
        std::fs::create_dir_all(&yt_dlp_dir).unwrap();
        let mut yt_dlp = yt_dlp_dir.join("yt-dlp");
        if cfg!(windows) {
            yt_dlp.set_extension("exe");
        }
        std::fs::write(yt_dlp, b"changed install bytes").unwrap();
        let _ = runtime_epoch_for_paths(&paths);
        assert_eq!(
            cache_misses(),
            after_second + 1,
            "install metadata changes must invalidate the immutable identity cache"
        );

        // Simulate an attacker restoring the file length and mtime after a
        // same-size byte replacement: the cached metadata stamps appear
        // current, but the bounded integrity TTL has elapsed. The next read
        // must perform a fresh byte verification instead of trusting stamps
        // for the process lifetime.
        {
            let current_stamps = capability_file_stamps(&paths);
            let mut cache = runtime_identity_cache().lock().unwrap();
            let cached = cache.get_mut(&paths.base_dir).expect("cached identity");
            cached.stamps = current_stamps;
            cached.verified_at = std::time::Instant::now() - std::time::Duration::from_secs(6);
        }
        let after_metadata_change = cache_misses();
        let _ = runtime_epoch_for_paths(&paths);
        assert_eq!(
            cache_misses(),
            after_metadata_change + 1,
            "same-size/restored-mtime replacement must be reverified after the bounded TTL"
        );
    }

    #[test]
    fn runtime_identity_rejects_unpinned_path_fallback_and_requires_exact_bundled_bytes() {
        let pin = &crate::pinned_dependency_manifest::manifest().yt_dlp_windows;
        let path_fallback = crate::tools::YtDlpToolsStatus {
            available: true,
            bundled_installed: false,
            bundled_path: "C:/isolated/tools/yt-dlp/yt-dlp.exe".to_string(),
            ytdlp_path: "yt-dlp".to_string(),
            ytdlp_version: Some("2026.05.16.233954".to_string()),
        };
        assert_eq!(
            verified_bundled_ytdlp_identity(&path_fallback, None, None),
            (false, None, None),
            "an executable found on PATH is never the protected bundled runtime",
        );

        let bundled = crate::tools::YtDlpToolsStatus {
            available: true,
            bundled_installed: true,
            bundled_path: "C:/isolated/tools/yt-dlp/yt-dlp.exe".to_string(),
            ytdlp_path: "C:/isolated/tools/yt-dlp/yt-dlp.exe".to_string(),
            ytdlp_version: Some(pin.version.clone()),
        };
        assert_eq!(
            verified_bundled_ytdlp_identity(
                &bundled,
                Some(pin.sha256_hex.to_ascii_lowercase()),
                Some(pin.file_bytes),
            ),
            (
                true,
                Some(pin.version.clone()),
                Some(pin.sha256_hex.to_ascii_lowercase())
            ),
        );
        assert!(!verified_bundled_ytdlp_identity(
            &bundled,
            Some("00".repeat(32)),
            Some(pin.file_bytes),
        )
        .0);
        assert!(
            !verified_bundled_ytdlp_identity(
                &bundled,
                Some(pin.sha256_hex.clone()),
                Some(pin.file_bytes.saturating_sub(1)),
            )
            .0
        );
    }

    #[test]
    fn advanced_tuning_is_bounded_persisted_and_drives_effective_overlay() {
        let (_dir, paths) = paths();
        let saved = set_tuning(
            &paths,
            YoutubeProtectionTuning {
                corroboration_min_separation_secs: 1,
                corroboration_window_secs: 2,
                cautious_dwell_secs: 1,
                conservative_dwell_secs: 2,
                cooldown_dwell_secs: 3,
                recovery_success_threshold: 0,
                raw_retention_days: 1,
                cautious_max_fragments: 99,
                cautious_min_sleep_secs: 17,
                conservative_min_sleep_secs: 29,
                cooldown_min_sleep_secs: 41,
                cautious_start_interval_secs: 13,
                conservative_start_interval_secs: 27,
                cooldown_start_interval_secs: 55,
                canary_tranche_size: 99,
            },
        )
        .expect("save tuning");
        assert_eq!(saved.corroboration_min_separation_secs, 10);
        assert_eq!(saved.corroboration_window_secs, 10);
        assert_eq!(saved.recovery_success_threshold, 1);
        assert_eq!(saved.raw_retention_days, 7);
        assert_eq!(saved.cautious_max_fragments, 8);
        assert_eq!(saved.canary_tranche_size, 1);
        assert_eq!(get_tuning(&paths).expect("reload tuning"), saved);

        let mut state = load_policy_state(
            &paths,
            PROVIDER_YOUTUBE,
            OPERATION_DOWNLOAD,
            ANONYMOUS_AUTH_FINGERPRINT,
            "test-epoch",
        )
        .expect("state");
        state.mode = DownloaderPolicyMode::Conservative;
        let effective = effective_policy_with_tuning(&baseline(), &state, 0, &saved);
        assert_eq!(effective.sleep_interval_secs, 29);
        assert_eq!(effective.aggregate_start_interval_secs, 27);
        assert_eq!(effective.concurrent_fragments, 1);
    }

    fn record(
        paths: &AppPaths,
        target: &str,
        outcome: DownloaderOutcomeClass,
        at: i64,
    ) -> DownloaderPolicySnapshot {
        let current = load_policy_state(
            paths,
            PROVIDER_YOUTUBE,
            OPERATION_DOWNLOAD,
            "auth-a",
            "epoch-a",
        )
        .expect("state");
        let baseline = baseline();
        let effective = effective_policy(&baseline, &current, at);
        record_outcome(
            paths,
            RecordDownloaderOutcome {
                provider: PROVIDER_YOUTUBE,
                operation: OPERATION_DOWNLOAD,
                canonical_target: target,
                auth_fingerprint: "auth-a",
                runtime_epoch: "epoch-a",
                baseline: &baseline,
                effective: &effective,
                outcome,
                error_text: Some(outcome.as_str()),
                incident_id: None,
                lease_owner_job_id: None,
                duration_ms: Some(100),
                occurred_at_ms: at,
            },
        )
        .expect("record")
    }

    #[test]
    fn disabled_adaptation_still_records_classified_duration_without_mutating_policy() {
        let (_dir, paths) = paths();
        let baseline = baseline();
        let before = load_policy_state(
            &paths,
            PROVIDER_YOUTUBE,
            OPERATION_DOWNLOAD,
            "auth-passive",
            "epoch-passive",
        )
        .expect("initial state");
        let effective = baseline_effective_policy(&baseline);
        for (target, outcome, duration_ms, at) in [
            (
                "video-a",
                DownloaderOutcomeClass::RateLimited,
                125_i64,
                1_000_i64,
            ),
            (
                "video-b",
                DownloaderOutcomeClass::Success,
                375_i64,
                2_000_i64,
            ),
        ] {
            let observed = record_observation(
                &paths,
                RecordDownloaderOutcome {
                    provider: PROVIDER_YOUTUBE,
                    operation: OPERATION_DOWNLOAD,
                    canonical_target: target,
                    auth_fingerprint: "auth-passive",
                    runtime_epoch: "epoch-passive",
                    baseline: &baseline,
                    effective: &effective,
                    outcome,
                    error_text: Some(outcome.as_str()),
                    incident_id: Some("incident-passive"),
                    lease_owner_job_id: None,
                    duration_ms: Some(duration_ms),
                    occurred_at_ms: at,
                },
            )
            .expect("passive observation");
            assert_eq!(observed.mode, before.mode);
            assert_eq!(observed.version, before.version);
        }

        let after = load_policy_state(
            &paths,
            PROVIDER_YOUTUBE,
            OPERATION_DOWNLOAD,
            "auth-passive",
            "epoch-passive",
        )
        .expect("state after passive observations");
        assert_eq!(after.mode, DownloaderPolicyMode::Normal);
        assert_eq!(after.version, before.version);
        assert_eq!(after.last_evidence_at_ms, None);
        let history = policy_history(
            &paths,
            PROVIDER_YOUTUBE,
            OPERATION_DOWNLOAD,
            "auth-passive",
            "epoch-passive",
            10,
        )
        .expect("passive history");
        assert_eq!(history.raw_total, 2);
        assert_eq!(history.rollup_event_total, 2);
        assert_eq!(history.transition_total, 0);
        let conn = db::open(&paths).expect("rollup db");
        let duration_total: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(duration_ms_total),0) FROM downloader_outcome_rollup \
                 WHERE provider=?1 AND operation=?2 AND auth_fingerprint=?3 AND runtime_epoch=?4",
                params![
                    PROVIDER_YOUTUBE,
                    OPERATION_DOWNLOAD,
                    "auth-passive",
                    "epoch-passive"
                ],
                |row| row.get(0),
            )
            .expect("duration rollup");
        assert_eq!(duration_total, 500);
    }

    #[test]
    fn active_epoch_history_reset_removes_state_raw_rollups_and_transitions_atomically() {
        let (_dir, paths) = paths();
        let first_at = 1_000_000;
        let second_at = first_at + CORROBORATION_MIN_SEPARATION_MS + 1;
        record(
            &paths,
            "https://youtube.test/a",
            DownloaderOutcomeClass::RateLimited,
            first_at,
        );
        record(
            &paths,
            "https://youtube.test/b",
            DownloaderOutcomeClass::RateLimited,
            second_at,
        );
        let before = policy_history(
            &paths,
            PROVIDER_YOUTUBE,
            OPERATION_DOWNLOAD,
            "auth-a",
            "epoch-a",
            500,
        )
        .expect("history before reset");
        assert_eq!(before.raw_total, 2);
        assert!(before.rollup_event_total >= 2);
        assert!(!before.transitions.is_empty());

        let receipt = reset_policy_history(
            &paths,
            PROVIDER_YOUTUBE,
            OPERATION_DOWNLOAD,
            "auth-a",
            "epoch-a",
        )
        .expect("reset history");
        assert_eq!(receipt.outcomes_deleted, 2);
        assert!(receipt.rollups_deleted >= 1);
        assert!(receipt.transitions_deleted >= 1);
        assert_eq!(receipt.states_deleted, 1);
        let after = policy_history(
            &paths,
            PROVIDER_YOUTUBE,
            OPERATION_DOWNLOAD,
            "auth-a",
            "epoch-a",
            500,
        )
        .expect("history after reset");
        assert_eq!(after.raw_total, 0);
        assert_eq!(after.rollup_event_total, 0);
        assert_eq!(after.transition_total, 0);
    }

    #[test]
    fn classifier_keeps_failure_domains_distinct() {
        assert_eq!(
            classify_youtube_outcome(Some("HTTP Error 429: Too Many Requests")),
            DownloaderOutcomeClass::RateLimited
        );
        assert_eq!(
            classify_youtube_outcome(Some(
                "Sign in to confirm you are not a bot; cookies were rejected"
            )),
            DownloaderOutcomeClass::AuthenticationRequiredOrInvalid
        );
        assert_eq!(
            classify_youtube_outcome(Some("PO Token is required for player client mweb")),
            DownloaderOutcomeClass::PoTokenOrClientCapability
        );
        assert_eq!(
            classify_youtube_outcome(Some("Private video")),
            DownloaderOutcomeClass::ContentUnavailableOrPrivate
        );
        assert_eq!(
            classify_youtube_outcome(Some("connection reset by peer")),
            DownloaderOutcomeClass::NetworkTransient
        );
        assert_eq!(
            classify_youtube_outcome(Some("ffmpeg not found")),
            DownloaderOutcomeClass::StorageOrLocalTool
        );
        assert_eq!(
            classify_youtube_outcome(Some("weird extractor failure")),
            DownloaderOutcomeClass::Unknown
        );
        assert_eq!(
            classify_youtube_outcome(Some("local proxy rate limit setting invalid")),
            DownloaderOutcomeClass::Unknown,
            "a generic local rate-limit phrase must never train remote pacing"
        );
        assert_eq!(
            classify_youtube_outcome(Some("This content isn't available, try again later")),
            DownloaderOutcomeClass::ContentUnavailableOrPrivate
        );
        assert_eq!(
            classify_youtube_outcome(Some("HTTP 500: Too Many Requests was quoted in help text")),
            DownloaderOutcomeClass::Unknown,
            "quoted text with a contradictory status must not train pacing"
        );
        assert_eq!(
            classify_youtube_outcome(Some("ERROR: [youtube] abc: Too Many Requests")),
            DownloaderOutcomeClass::RateLimited
        );
    }

    #[test]
    fn abandoned_canary_lease_expires_without_extending_full_cooldown() {
        let (_dir, paths) = paths();
        let conn = db::open(&paths).expect("db");
        db::migrate(&conn).expect("migrate");
        conn.execute(
            "INSERT INTO downloader_policy_state(provider,operation,auth_fingerprint,runtime_epoch,mode,entered_at_ms,next_eligible_probe_at_ms,version) VALUES(?1,?2,?3,?4,'cooldown',?5,?5,1)",
            params![PROVIDER_YOUTUBE, OPERATION_DOWNLOAD, "auth", "epoch", 1_000_i64],
        )
        .expect("seed cooldown");
        drop(conn);
        assert!(claim_cooldown_canary(
            &paths,
            PROVIDER_YOUTUBE,
            OPERATION_DOWNLOAD,
            "auth",
            "epoch",
            "job-a",
            1_000,
        )
        .expect("claim")
        .is_some());
        assert!(claim_cooldown_canary(
            &paths,
            PROVIDER_YOUTUBE,
            OPERATION_DOWNLOAD,
            "auth",
            "epoch",
            "job-b",
            1_001,
        )
        .expect("concurrent claim refused")
        .is_none());
        assert!(claim_cooldown_canary(
            &paths,
            PROVIDER_YOUTUBE,
            OPERATION_DOWNLOAD,
            "auth",
            "epoch",
            "job-b",
            1_000 + CANARY_LEASE_MS + 1,
        )
        .expect("expired lease reclaimed")
        .is_some());
        let state = load_policy_state(
            &paths,
            PROVIDER_YOUTUBE,
            OPERATION_DOWNLOAD,
            "auth",
            "epoch",
        )
        .expect("state");
        assert_eq!(state.next_eligible_probe_at_ms, Some(1_000));
    }

    #[test]
    fn unrelated_late_outcome_cannot_release_an_active_canary_owner() {
        let (_dir, paths) = paths();
        let conn = db::open(&paths).expect("db");
        db::migrate(&conn).expect("migrate");
        conn.execute(
            "INSERT INTO downloader_policy_state(provider,operation,auth_fingerprint,runtime_epoch,mode,entered_at_ms,next_eligible_probe_at_ms,version) VALUES(?1,?2,?3,?4,'cooldown',?5,?5,1)",
            params![PROVIDER_YOUTUBE, OPERATION_DOWNLOAD, "auth", "epoch", 1_000_i64],
        )
        .expect("seed cooldown");
        drop(conn);
        let owner_lease = claim_cooldown_canary(
            &paths,
            PROVIDER_YOUTUBE,
            OPERATION_DOWNLOAD,
            "auth",
            "epoch",
            "canary-job",
            1_000,
        )
        .expect("owner claim")
        .expect("owner lease");

        let baseline = baseline();
        let non_canary_effective = baseline_effective_policy(&baseline);
        record_outcome(
            &paths,
            RecordDownloaderOutcome {
                provider: PROVIDER_YOUTUBE,
                operation: OPERATION_DOWNLOAD,
                canonical_target: "late-pre-cooldown-job",
                auth_fingerprint: "auth",
                runtime_epoch: "epoch",
                baseline: &baseline,
                effective: &non_canary_effective,
                outcome: DownloaderOutcomeClass::Unknown,
                error_text: Some("unrelated late outcome"),
                incident_id: None,
                lease_owner_job_id: Some("late-job"),
                duration_ms: Some(10),
                occurred_at_ms: 1_001,
            },
        )
        .expect("late outcome");

        let conn = db::open(&paths).expect("lease db");
        let retained: (String, String) = conn
            .query_row(
                "SELECT lease_id,job_id FROM downloader_canary_lease WHERE provider=?1 AND operation=?2 AND auth_fingerprint=?3 AND runtime_epoch=?4",
                params![PROVIDER_YOUTUBE, OPERATION_DOWNLOAD, "auth", "epoch"],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("active lease retained");
        assert_eq!(retained, (owner_lease, "canary-job".to_string()));
        drop(conn);
        assert!(claim_cooldown_canary(
            &paths,
            PROVIDER_YOUTUBE,
            OPERATION_DOWNLOAD,
            "auth",
            "epoch",
            "other-job",
            1_002,
        )
        .expect("second dispatch")
        .is_none());
        assert_eq!(
            release_cooldown_canary_for_job(&paths, "late-job").expect("unrelated release"),
            0
        );
        assert_eq!(
            release_cooldown_canary_for_job(&paths, "canary-job").expect("owner release"),
            1
        );
        assert!(claim_cooldown_canary(
            &paths,
            PROVIDER_YOUTUBE,
            OPERATION_DOWNLOAD,
            "auth",
            "epoch",
            "other-job",
            1_003,
        )
        .expect("claim after exact release")
        .is_some());
    }

    #[test]
    fn paged_history_is_exhaustive_and_retention_batches_are_bounded_resumable() {
        let (_dir, paths) = paths();
        for index in 0..5 {
            record(
                &paths,
                &format!("video-{index}"),
                DownloaderOutcomeClass::Unknown,
                10_000 + index,
            );
        }
        let first = policy_outcomes_page(
            &paths,
            PROVIDER_YOUTUBE,
            OPERATION_DOWNLOAD,
            "auth-a",
            "epoch-a",
            None,
            2,
        )
        .expect("first page");
        assert_eq!(first.outcomes.len(), 2);
        assert!(first.has_more);
        let second = policy_outcomes_page(
            &paths,
            PROVIDER_YOUTUBE,
            OPERATION_DOWNLOAD,
            "auth-a",
            "epoch-a",
            first.next_cursor.as_ref(),
            2,
        )
        .expect("second page");
        assert_eq!(second.outcomes.len(), 2);
        assert!(second.has_more);
        assert_eq!(
            policy_outcomes_all(
                &paths,
                PROVIDER_YOUTUBE,
                OPERATION_DOWNLOAD,
                "auth-a",
                "epoch-a",
            )
            .expect("all outcomes")
            .len(),
            5
        );

        let first_delete = compact_outcomes_batch(&paths, i64::MAX, 2).expect("batch one");
        assert_eq!(first_delete.deleted, 2);
        assert!(first_delete.has_more);
        let second_delete = compact_outcomes_batch(&paths, i64::MAX, 2).expect("batch two");
        assert_eq!(second_delete.deleted, 2);
        assert!(second_delete.has_more);
        let final_delete = compact_outcomes_batch(&paths, i64::MAX, 2).expect("batch three");
        assert_eq!(final_delete.deleted, 1);
        assert!(!final_delete.has_more);
    }

    #[test]
    fn startup_retention_drain_resumes_after_a_bounded_interruption() {
        let (_dir, paths) = paths();
        let mut conn = db::open(&paths).expect("db");
        db::migrate(&conn).expect("migrate");
        let baseline_json = serde_json::to_string(&baseline()).unwrap();
        let effective_json =
            serde_json::to_string(&baseline_effective_policy(&baseline())).unwrap();
        let tx = conn.transaction().expect("seed transaction");
        {
            let mut statement = tx.prepare(
                "INSERT INTO downloader_outcome(id,provider,operation,target_fingerprint,auth_fingerprint,runtime_epoch,baseline_policy_json,effective_policy_json,occurred_at_ms,outcome_class) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            ).expect("seed statement");
            for index in 0..=RAW_RETENTION_BATCH_SIZE {
                statement
                    .execute(params![
                        format!("expired-{index:04}"),
                        PROVIDER_YOUTUBE,
                        OPERATION_DOWNLOAD,
                        format!("target-{index:04}"),
                        "auth",
                        "epoch",
                        baseline_json,
                        effective_json,
                        1_i64,
                        DownloaderOutcomeClass::Unknown.as_str(),
                    ])
                    .expect("seed outcome");
            }
        }
        tx.commit().expect("seed commit");

        let interrupted =
            drain_expired_outcomes(&paths, i64::MAX, 25, 1, 1_000).expect("bounded drain");
        assert_eq!(interrupted.batches, 1);
        assert_eq!(interrupted.deleted, RAW_RETENTION_BATCH_SIZE as u64);
        assert!(interrupted.has_more && !interrupted.complete);

        let resumed =
            drain_expired_outcomes(&paths, i64::MAX, 25, 8, 1_000).expect("resumed drain");
        assert_eq!(resumed.deleted, 1);
        assert!(resumed.complete && !resumed.has_more);
    }

    #[test]
    fn large_retention_backlog_yields_at_each_batch_budget_and_resumes_durably() {
        let (_dir, paths) = paths();
        let mut conn = db::open(&paths).expect("db");
        db::migrate(&conn).expect("migrate");
        let baseline_json = serde_json::to_string(&baseline()).unwrap();
        let effective_json =
            serde_json::to_string(&baseline_effective_policy(&baseline())).unwrap();
        let tx = conn.transaction().expect("seed transaction");
        {
            let mut statement = tx.prepare(
                "INSERT INTO downloader_outcome(id,provider,operation,target_fingerprint,auth_fingerprint,runtime_epoch,baseline_policy_json,effective_policy_json,occurred_at_ms,outcome_class) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,1,'unknown')",
            ).unwrap();
            for index in 0..(RAW_RETENTION_BATCH_SIZE * 3 + 1) {
                statement
                    .execute(params![
                        format!("bounded-{index:05}"),
                        PROVIDER_YOUTUBE,
                        OPERATION_DOWNLOAD,
                        format!("target-{index:05}"),
                        "auth",
                        "epoch",
                        baseline_json,
                        effective_json
                    ])
                    .unwrap();
            }
        }
        tx.commit().unwrap();

        let first = drain_expired_outcomes(&paths, i64::MAX, 25, 2, 10_000).unwrap();
        assert_eq!(first.batches, 2);
        assert_eq!(first.deleted, (RAW_RETENTION_BATCH_SIZE * 2) as u64);
        assert!(first.has_more && first.budget_exhausted);
        let second = drain_expired_outcomes(&paths, i64::MAX, 25, 2, 10_000).unwrap();
        assert_eq!(second.deleted, (RAW_RETENTION_BATCH_SIZE + 1) as u64);
        assert!(second.complete && !second.budget_exhausted);
    }

    #[test]
    fn retention_wall_time_budget_yields_with_durable_backlog_remaining() {
        let (_dir, paths) = paths();
        let mut conn = db::open(&paths).unwrap();
        db::migrate(&conn).unwrap();
        let tx = conn.transaction().unwrap();
        {
            let mut statement = tx.prepare(
                "INSERT INTO downloader_outcome(id,provider,operation,target_fingerprint,auth_fingerprint,runtime_epoch,baseline_policy_json,effective_policy_json,occurred_at_ms,outcome_class) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,1,'unknown')",
            ).unwrap();
            let baseline_json = serde_json::to_string(&baseline()).unwrap();
            let effective_json =
                serde_json::to_string(&baseline_effective_policy(&baseline())).unwrap();
            for index in 0..(RAW_RETENTION_BATCH_SIZE + 1) {
                statement
                    .execute(params![
                        format!("time-budget-{index:05}"),
                        PROVIDER_YOUTUBE,
                        OPERATION_DOWNLOAD,
                        format!("target-{index:05}"),
                        "auth",
                        "epoch",
                        baseline_json,
                        effective_json
                    ])
                    .unwrap();
            }
        }
        tx.commit().unwrap();

        let receipt = drain_expired_outcomes(&paths, i64::MAX, 25, 100, 25).unwrap();
        assert_eq!(receipt.batches, 1);
        assert_eq!(receipt.deleted, RAW_RETENTION_BATCH_SIZE as u64);
        assert!(receipt.has_more && receipt.budget_exhausted);
        assert!(receipt.elapsed_ms >= 25);
    }

    #[test]
    fn repeated_distinct_time_separated_rate_limits_transition_but_unknown_does_not() {
        let (_dir, paths) = paths();
        let first = record(
            &paths,
            "video-a",
            DownloaderOutcomeClass::RateLimited,
            1_000_000,
        );
        assert_eq!(first.mode, DownloaderPolicyMode::Normal);
        let unknown = record(
            &paths,
            "video-b",
            DownloaderOutcomeClass::Unknown,
            1_030_000,
        );
        assert_eq!(unknown.mode, DownloaderPolicyMode::Normal);
        let second = record(
            &paths,
            "video-b",
            DownloaderOutcomeClass::RateLimited,
            1_061_000,
        );
        assert_eq!(second.mode, DownloaderPolicyMode::Cautious);
        let history = policy_history(
            &paths,
            PROVIDER_YOUTUBE,
            OPERATION_DOWNLOAD,
            "auth-a",
            "epoch-a",
            20,
        )
        .expect("history");
        assert_eq!(history.transitions.len(), 1);
        assert_eq!(history.transitions[0].evidence_ids.len(), 2);
        let replay = replay_policy_history(&history);
        assert_eq!(replay.final_mode, DownloaderPolicyMode::Cautious);
        assert_eq!(replay.unknown_events, 1);
    }

    #[test]
    fn same_target_or_too_close_rate_limits_never_corroborate() {
        let (_dir, app_paths) = paths();
        assert_eq!(
            record(
                &app_paths,
                "video-a",
                DownloaderOutcomeClass::RateLimited,
                1_000_000
            )
            .mode,
            DownloaderPolicyMode::Normal
        );
        assert_eq!(
            record(
                &app_paths,
                "video-b",
                DownloaderOutcomeClass::RateLimited,
                1_030_000
            )
            .mode,
            DownloaderPolicyMode::Normal
        );
        assert_eq!(
            record(
                &app_paths,
                "video-a",
                DownloaderOutcomeClass::RateLimited,
                1_061_000
            )
            .mode,
            DownloaderPolicyMode::Normal
        );
        assert_eq!(
            record(
                &app_paths,
                "video-c",
                DownloaderOutcomeClass::RateLimited,
                1_091_000
            )
            .mode,
            DownloaderPolicyMode::Cautious,
            "video-a is sufficiently separated and distinct from video-c"
        );

        let (_dir, same_paths) = paths();
        assert_eq!(
            record(
                &same_paths,
                "video-a",
                DownloaderOutcomeClass::RateLimited,
                2_000_000
            )
            .mode,
            DownloaderPolicyMode::Normal
        );
        assert_eq!(
            record(
                &same_paths,
                "video-a",
                DownloaderOutcomeClass::RateLimited,
                2_500_000
            )
            .mode,
            DownloaderPolicyMode::Normal
        );
    }

    #[test]
    fn auth_and_capability_hold_without_training_pacing() {
        for outcome in [
            DownloaderOutcomeClass::AuthenticationRequiredOrInvalid,
            DownloaderOutcomeClass::PoTokenOrClientCapability,
        ] {
            let (_dir, paths) = paths();
            let state = record(&paths, "video-a", outcome, 2_000_000);
            assert_eq!(state.mode, DownloaderPolicyMode::Hold);
            assert_eq!(state.corroboration_count, 0);
        }
    }

    #[test]
    fn overlay_preserves_baseline_bandwidth_semantics_and_never_increases_concurrency() {
        let baseline = baseline();
        let state = DownloaderPolicySnapshot {
            provider: PROVIDER_YOUTUBE.to_string(),
            operation: OPERATION_DOWNLOAD.to_string(),
            auth_fingerprint: "a".to_string(),
            runtime_epoch: "e".to_string(),
            mode: DownloaderPolicyMode::Conservative,
            corroboration_count: 2,
            success_streak: 0,
            entered_at_ms: 0,
            last_evidence_at_ms: None,
            next_eligible_probe_at_ms: None,
            version: 2,
        };
        let effective = effective_policy(&baseline, &state, 10);
        assert_eq!(effective.concurrent_fragments, 1);
        assert_eq!(effective.limit_rate, baseline.limit_rate);
        assert_eq!(effective.throttled_rate, baseline.throttled_rate);
        assert_eq!(baseline.concurrent_fragments, 8);
    }

    #[test]
    fn runtime_epoch_prevents_old_evidence_from_controlling_new_runtime() {
        let (_dir, paths) = paths();
        let _ = record(
            &paths,
            "video-a",
            DownloaderOutcomeClass::AuthenticationRequiredOrInvalid,
            2_000_000,
        );
        let next = load_policy_state(
            &paths,
            PROVIDER_YOUTUBE,
            OPERATION_DOWNLOAD,
            "auth-a",
            "epoch-b",
        )
        .expect("new epoch");
        assert_eq!(next.mode, DownloaderPolicyMode::Normal);
    }

    #[test]
    fn cooldown_allows_exactly_one_atomic_canary_and_success_reopens_conservatively() {
        let (_dir, paths) = paths();
        let base_at = 10_000_000;
        for (index, target) in ["video-a", "video-b", "video-c", "video-d"]
            .into_iter()
            .enumerate()
        {
            let state = record(
                &paths,
                target,
                DownloaderOutcomeClass::RateLimited,
                base_at + (index as i64 * 61_000),
            );
            if index == 3 {
                assert_eq!(state.mode, DownloaderPolicyMode::Cooldown);
            }
        }
        let cooldown = load_policy_state(
            &paths,
            PROVIDER_YOUTUBE,
            OPERATION_DOWNLOAD,
            "auth-a",
            "epoch-a",
        )
        .expect("cooldown state");
        let probe_at = cooldown.next_eligible_probe_at_ms.expect("probe time");
        let base = baseline();
        let effective = effective_policy(&base, &cooldown, probe_at);
        assert!(effective.eligible);
        assert!(effective.canary_only);
        assert!(claim_cooldown_canary(
            &paths,
            PROVIDER_YOUTUBE,
            OPERATION_DOWNLOAD,
            "auth-a",
            "epoch-a",
            "canary-job",
            probe_at,
        )
        .expect("first claim")
        .is_some());
        assert!(claim_cooldown_canary(
            &paths,
            PROVIDER_YOUTUBE,
            OPERATION_DOWNLOAD,
            "auth-a",
            "epoch-a",
            "other-job",
            probe_at,
        )
        .expect("second claim")
        .is_none());

        let reopened = record_outcome(
            &paths,
            RecordDownloaderOutcome {
                provider: PROVIDER_YOUTUBE,
                operation: OPERATION_DOWNLOAD,
                canonical_target: "video-canary",
                auth_fingerprint: "auth-a",
                runtime_epoch: "epoch-a",
                baseline: &base,
                effective: &effective,
                outcome: DownloaderOutcomeClass::Success,
                error_text: None,
                incident_id: None,
                lease_owner_job_id: Some("canary-job"),
                duration_ms: Some(100),
                occurred_at_ms: probe_at + 1,
            },
        )
        .expect("canary outcome");
        assert_eq!(reopened.mode, DownloaderPolicyMode::Conservative);
        assert_eq!(reopened.success_streak, 0);
        assert_eq!(reopened.next_eligible_probe_at_ms, None);
        let history = policy_history(
            &paths,
            PROVIDER_YOUTUBE,
            OPERATION_DOWNLOAD,
            "auth-a",
            "epoch-a",
            50,
        )
        .expect("history");
        assert!(history
            .transitions
            .iter()
            .any(|row| row.reason == "controlled_canary_success"));
    }

    #[test]
    fn raw_retention_prunes_old_rows_but_preserves_durable_rollups() {
        let (_dir, paths) = paths();
        let old_at = 1_000_000;
        let recent_at = old_at + RAW_RETENTION_MS + 1;
        let _ = record(&paths, "video-old", DownloaderOutcomeClass::Unknown, old_at);
        let _ = record(
            &paths,
            "video-recent",
            DownloaderOutcomeClass::Success,
            recent_at,
        );
        let conn = db::open(&paths).expect("db");
        let raw_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM downloader_outcome", [], |row| {
                row.get(0)
            })
            .expect("raw count");
        let rollup_count: i64 = conn
            .query_row(
                "SELECT SUM(event_count) FROM downloader_outcome_rollup",
                [],
                |row| row.get(0),
            )
            .expect("rollup count");
        assert_eq!(raw_count, 1);
        assert_eq!(rollup_count, 2);
    }

    #[test]
    fn failed_state_write_rolls_back_outcome_rollup_and_transition_atomically() {
        let (_dir, paths) = paths();
        let conn = db::open(&paths).expect("db");
        db::migrate(&conn).expect("migrate");
        conn.execute_batch(
            "CREATE TRIGGER wp0299_fail_policy_state BEFORE INSERT ON downloader_policy_state \
             BEGIN SELECT RAISE(ABORT, 'wp0299 injected state failure'); END;",
        )
        .expect("install failure trigger");
        drop(conn);

        let current = load_policy_state(
            &paths,
            PROVIDER_YOUTUBE,
            OPERATION_DOWNLOAD,
            "auth-a",
            "epoch-a",
        )
        .expect("initial state");
        let base = baseline();
        let effective = effective_policy(&base, &current, 1_000_000);
        let failure = record_outcome(
            &paths,
            RecordDownloaderOutcome {
                provider: PROVIDER_YOUTUBE,
                operation: OPERATION_DOWNLOAD,
                canonical_target: "video-a",
                auth_fingerprint: "auth-a",
                runtime_epoch: "epoch-a",
                baseline: &base,
                effective: &effective,
                outcome: DownloaderOutcomeClass::RateLimited,
                error_text: Some("HTTP Error 429"),
                incident_id: None,
                lease_owner_job_id: None,
                duration_ms: Some(100),
                occurred_at_ms: 1_000_000,
            },
        )
        .expect_err("trigger must abort transaction");
        assert!(failure
            .to_string()
            .contains("wp0299 injected state failure"));

        let conn = db::open(&paths).expect("db after failure");
        for table in [
            "downloader_outcome",
            "downloader_outcome_rollup",
            "downloader_policy_transition",
            "downloader_policy_state",
        ] {
            let count: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .expect("count after rollback");
            assert_eq!(count, 0, "{table} must not retain a partial transaction");
        }
        conn.execute_batch("DROP TRIGGER wp0299_fail_policy_state;")
            .expect("remove failure trigger");
        drop(conn);
        let recovered = record(
            &paths,
            "video-a",
            DownloaderOutcomeClass::Success,
            1_000_001,
        );
        assert_eq!(recovered.mode, DownloaderPolicyMode::Normal);
    }

    #[test]
    #[ignore = "WP-0299 explicit million-event scale gate"]
    fn million_event_history_query_and_retention_use_bounded_indexed_paths() {
        let (_dir, paths) = paths();
        let conn = db::open(&paths).expect("db");
        db::migrate(&conn).expect("migrate");
        conn.execute_batch(
            "BEGIN IMMEDIATE; \
             WITH RECURSIVE seq(value) AS (SELECT 1 UNION ALL SELECT value+1 FROM seq WHERE value<1000000) \
             INSERT INTO downloader_outcome( \
               id,provider,operation,target_fingerprint,auth_fingerprint,runtime_epoch, \
               baseline_policy_json,effective_policy_json,occurred_at_ms,outcome_class \
             ) \
             SELECT printf('scale-%07d',value),'youtube','download',printf('target-%07d',value), \
                    'auth-a','epoch-a','{}','{}',1,'unknown' FROM seq; \
             COMMIT;",
        )
        .expect("seed one million outcomes");
        let pagination_plan: Vec<String> = {
            let mut statement = conn
                .prepare(
                    "EXPLAIN QUERY PLAN SELECT id FROM downloader_outcome \
                     WHERE provider='youtube' AND operation='download' AND auth_fingerprint='auth-a' \
                       AND runtime_epoch='epoch-a' \
                       AND (occurred_at_ms<1 OR (occurred_at_ms=1 AND id<'scale-0900000')) \
                     ORDER BY occurred_at_ms DESC,id DESC LIMIT 1000",
                )
                .expect("query plan");
            statement
                .query_map([], |row| row.get::<_, String>(3))
                .expect("plan rows")
                .collect::<rusqlite::Result<Vec<_>>>()
                .expect("collect plan")
        };
        assert!(
            pagination_plan.iter().any(|detail| {
                detail.contains("idx_downloader_outcome_policy_evidence")
                    || detail.contains("idx_downloader_outcome_history")
            }),
            "pagination must stay index-backed: {pagination_plan:?}"
        );
        assert!(
            pagination_plan
                .iter()
                .all(|detail| !detail.contains("TEMP B-TREE")),
            "identical timestamps must not force a pagination sort: {pagination_plan:?}"
        );
        let retention_plan: Vec<String> = {
            let mut statement = conn
                .prepare(
                    "EXPLAIN QUERY PLAN SELECT id FROM downloader_outcome \
                     WHERE occurred_at_ms<2 ORDER BY occurred_at_ms,id LIMIT 1000",
                )
                .expect("retention plan");
            statement
                .query_map([], |row| row.get::<_, String>(3))
                .expect("retention plan rows")
                .collect::<rusqlite::Result<Vec<_>>>()
                .expect("collect retention plan")
        };
        assert!(
            retention_plan
                .iter()
                .any(|detail| detail.contains("idx_downloader_outcome_retention")),
            "retention must stay index-backed: {retention_plan:?}"
        );
        assert!(
            retention_plan
                .iter()
                .all(|detail| !detail.contains("TEMP B-TREE")),
            "retention must not sort a million-row tie: {retention_plan:?}"
        );

        let first_started = std::time::Instant::now();
        let first = compact_outcomes_batch_conn(&conn, 2, RAW_RETENTION_BATCH_SIZE)
            .expect("first bounded compaction");
        let first_elapsed = first_started.elapsed();
        assert_eq!(first.deleted, RAW_RETENTION_BATCH_SIZE as u64);
        assert!(first.has_more);
        assert!(
            first_elapsed < std::time::Duration::from_secs(10),
            "one bounded retention batch exceeded the 10s gate: {first_elapsed:?}"
        );
        drop(conn);

        // Reopen after an interruption and resume. A bounded trigger must never
        // attempt to erase the remaining 999k rows in one transaction.
        let conn = db::open(&paths).expect("resume db");
        let second_started = std::time::Instant::now();
        let second = compact_outcomes_batch_conn(&conn, 2, RAW_RETENTION_BATCH_SIZE)
            .expect("resumed bounded compaction");
        let second_elapsed = second_started.elapsed();
        assert_eq!(second.deleted, RAW_RETENTION_BATCH_SIZE as u64);
        assert!(second.has_more);
        assert!(second_elapsed < std::time::Duration::from_secs(10));
        let retained: i64 = conn
            .query_row("SELECT COUNT(*) FROM downloader_outcome", [], |row| {
                row.get(0)
            })
            .expect("retained count");
        assert_eq!(
            retained,
            1_000_000 - (RAW_RETENTION_BATCH_SIZE as i64 * 2),
            "two interrupted/resumed triggers may delete only two bounded batches"
        );
        eprintln!(
            "WP-0299 million-event retention batch timings: first={first_elapsed:?}, resumed={second_elapsed:?}"
        );
    }

    #[test]
    fn durable_mutation_generation_survives_reload_and_rolls_back_atomically() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        db::ensure_schema(&paths).expect("schema");
        let saved = set_tuning_with_generation(&paths, YoutubeProtectionTuning::default(), 100)
            .expect("first durable generation");
        drop(saved);
        assert!(
            set_tuning_with_generation(&paths, YoutubeProtectionTuning::default(), 99).is_err()
        );

        let mut conn = db::open(&paths).expect("open rollback probe");
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .expect("transaction");
        claim_mutation_generation_conn(&tx, "rollback_probe", 200, false)
            .expect("claim in transaction");
        tx.rollback().expect("rollback");
        let persisted: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM youtube_protection_mutation_generation WHERE operation='rollback_probe'",
                [],
                |row| row.get(0),
            )
            .expect("rollback count");
        assert_eq!(
            persisted, 0,
            "failed/rolled-back mutations cannot consume generation"
        );
        assert!(
            set_tuning_with_generation(&paths, YoutubeProtectionTuning::default(), 101).is_ok()
        );
    }

    #[test]
    fn completed_history_reset_rejects_duplicate_generation_but_other_operation_is_independent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        db::ensure_schema(&paths).expect("schema");
        let download = reset_policy_history_with_generation(
            &paths,
            PROVIDER_YOUTUBE,
            OPERATION_DOWNLOAD,
            ANONYMOUS_AUTH_FINGERPRINT,
            "runtime",
            700,
        )
        .expect("download reset");
        assert!(download.complete);
        assert!(reset_policy_history_with_generation(
            &paths,
            PROVIDER_YOUTUBE,
            OPERATION_DOWNLOAD,
            ANONYMOUS_AUTH_FINGERPRINT,
            "runtime",
            700,
        )
        .is_err());
        assert!(reset_policy_history_with_generation(
            &paths,
            PROVIDER_YOUTUBE,
            OPERATION_ENUMERATION,
            ANONYMOUS_AUTH_FINGERPRINT,
            "runtime",
            700,
        )
        .is_ok());
    }

    #[test]
    fn retention_continuation_is_durable_and_truthful() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        db::ensure_schema(&paths).expect("schema");
        persist_retention_continuation(&paths, true, 3).expect("persist pending");
        let reopened = retention_continuation(&paths).expect("reopen pending");
        assert!(reopened.pending);
        assert_eq!(reopened.consecutive_failures, 3);
        persist_retention_continuation(&paths, false, 0).expect("complete");
        assert!(
            !retention_continuation(&paths)
                .expect("reopen complete")
                .pending
        );
    }
}
