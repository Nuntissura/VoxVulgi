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

/// WP-0321 S4: a single durable policy-state row (keyed by `operation=POLICY_STATE_OPERATION`) is
/// shared between download and enumeration outcomes for the same (provider, auth_fingerprint,
/// runtime_epoch) identity — one YouTube block pauses both. Raw evidence (`downloader_outcome`)
/// and rollups still keep the caller's own `operation` for per-surface history/diagnostics.
pub const POLICY_STATE_OPERATION: &str = OPERATION_DOWNLOAD;

const RAW_RETENTION_MS: i64 = 90 * 24 * 60 * 60_000;
const RAW_RETENTION_BATCH_SIZE: usize = 1_000;
const CANARY_LEASE_MS: i64 = 15 * 60_000;
const COOLDOWN_META_KEY: &str = "youtube_cooldown_v1";
const LEGACY_TUNING_META_KEY: &str = "youtube_protection_tuning_v1";
const LEGACY_ANTIBOT_RECURRING_MIN_SLEEP_META_KEY: &str =
    "antibot_recurring_download_min_sleep_secs";
const LEGACY_ANTIBOT_RECURRING_MAX_SLEEP_META_KEY: &str =
    "antibot_recurring_download_max_sleep_secs";
const LEGACY_ANTIBOT_ENUMERATION_SLEEP_REQUESTS_META_KEY: &str =
    "antibot_enumeration_sleep_requests";
const LEGACY_ANTIBOT_ADAPTIVE_PROTECTION_ENABLED_META_KEY: &str =
    "antibot_adaptive_protection_enabled";
static COOLDOWN_WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static HISTORY_RESET_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// WP-0321 S4: replaces the five-tier tuning/ladder model. A YouTube block enters cooldown for
/// `base_wait_secs`; each further failed controlled canary doubles the wait, capped at
/// `max_wait_secs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct YoutubeCooldownSettings {
    pub base_wait_secs: u64,
    pub max_wait_secs: u64,
}

impl Default for YoutubeCooldownSettings {
    fn default() -> Self {
        Self {
            base_wait_secs: 3_600,
            max_wait_secs: 21_600,
        }
    }
}

impl YoutubeCooldownSettings {
    fn normalized(mut self) -> Self {
        self.base_wait_secs = self.base_wait_secs.clamp(600, 86_400);
        self.max_wait_secs = self
            .max_wait_secs
            .clamp(self.base_wait_secs, 1_209_600);
        self
    }

    fn base_wait_ms(&self) -> i64 {
        self.base_wait_secs.saturating_mul(1_000).min(i64::MAX as u64) as i64
    }

    fn max_wait_ms(&self) -> i64 {
        self.max_wait_secs.saturating_mul(1_000).min(i64::MAX as u64) as i64
    }

    /// `base * 2^failed_probe_count`, capped at `max_wait_secs`. `failed_probe_count` is clamped
    /// before shifting so an unbounded count can never overflow or panic.
    fn escalating_wait_ms(&self, failed_probe_count: u32) -> i64 {
        let base_ms = self.base_wait_ms();
        let cap_ms = self.max_wait_ms();
        let multiplier = 1_i64
            .checked_shl(failed_probe_count.min(61))
            .unwrap_or(i64::MAX);
        base_ms.saturating_mul(multiplier).min(cap_ms)
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

/// WP-0321 S4: the five-mode ladder (normal/cautious/conservative/cooldown/hold) collapsed to two
/// live modes. `Legacy` is a deserialization-only landing spot for old persisted JSON/DB rows that
/// still say "cautious"/"conservative"/"hold" and must never be produced going forward; readers
/// treat it as `Normal`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloaderPolicyMode {
    Normal,
    Cooldown,
    #[serde(other)]
    Legacy,
}

impl DownloaderPolicyMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Cooldown => "cooldown",
            // Never produced; a defensive projection for a value that should not exist.
            Self::Legacy => "normal",
        }
    }

    fn parse(value: &str) -> Self {
        match value {
            "cooldown" => Self::Cooldown,
            _ => Self::Normal,
        }
    }

    fn normalized(self) -> Self {
        match self {
            Self::Legacy => Self::Normal,
            other => other,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloaderBaselinePolicy {
    pub concurrent_fragments: u32,
    pub sleep_interval_secs: u32,
    /// WP-0321 S4: additional random jitter added on top of `sleep_interval_secs`. Defaulted so a
    /// baseline serialized before this field existed still loads.
    #[serde(default)]
    pub sleep_jitter_secs: u32,
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
    #[serde(default)]
    pub sleep_jitter_secs: u32,
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
    /// Retained for schema/API stability; the corroboration mechanism that used to write this is
    /// removed. Always 0.
    pub corroboration_count: u32,
    /// Retained for schema/API stability; the sustained-recovery ladder that used to write this is
    /// removed. Always 0.
    pub success_streak: u32,
    pub entered_at_ms: i64,
    pub last_evidence_at_ms: Option<i64>,
    pub next_eligible_probe_at_ms: Option<i64>,
    /// WP-0320/S4: consecutive failed cooldown canaries since the current cooldown cycle began (or
    /// since the last `controlled_canary_success` / `return_to_baseline`). Drives the escalating
    /// retry wait; `mode != cooldown` implies this is stale/irrelevant.
    pub cooldown_failed_probe_count: u32,
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
    let mut stamps = crate::download_engines::engine_identity_stamp_paths(paths)
        .into_iter()
        .map(capability_file_stamp)
        .collect::<Vec<_>>();
    stamps.extend([
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
    ]);
    stamps
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

    let download_engine = crate::download_engines::engine_status(paths);
    let (yt_dlp_available, yt_dlp_version, yt_dlp_sha256_hex) = if download_engine.verified {
        (true, download_engine.version, download_engine.sha256_hex)
    } else {
        (false, None, None)
    };
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
    classify_youtube_outcome_with_cookie_rejection(error_text, false)
}

/// WP-0321 S4: `is_saved_cookie_rejection` should be computed the same way jobs.rs's
/// `is_youtube_saved_cookie_rejection(url, message)` decides it (the target URL is a YouTube URL
/// and the bot-check message names a supplied/rejected cookie identity) — the identity itself is
/// rejected, which is an auth problem, not a pacing problem, and must not enter cooldown. Any
/// other "Sign in to confirm you're not a bot" occurrence (including anonymous/no-cookie
/// requests) is the anti-bot pacing block itself (operator direction 2026-09-23: "even if the
/// passive gets blocked, it should reattempt after a cooldown"), so it is classified as a
/// rate-limit block to drive the same cooldown machinery as an HTTP 429.
pub fn classify_youtube_outcome_with_cookie_rejection(
    error_text: Option<&str>,
    is_saved_cookie_rejection: bool,
) -> DownloaderOutcomeClass {
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
    if lower.contains("sign in to confirm") && lower.contains("bot") {
        return if is_saved_cookie_rejection {
            DownloaderOutcomeClass::AuthenticationRequiredOrInvalid
        } else {
            DownloaderOutcomeClass::RateLimited
        };
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

fn load_cooldown_settings_conn(conn: &rusqlite::Connection) -> Result<YoutubeCooldownSettings> {
    let value = conn
        .query_row(
            "SELECT value FROM meta WHERE key=?1",
            [COOLDOWN_META_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    Ok(value
        .and_then(|raw| serde_json::from_str::<YoutubeCooldownSettings>(&raw).ok())
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

pub fn get_cooldown_settings(paths: &AppPaths) -> Result<YoutubeCooldownSettings> {
    let conn = db::open_readonly(paths)?;
    load_cooldown_settings_conn(&conn)
}

pub fn set_cooldown_settings_with_generation(
    paths: &AppPaths,
    settings: YoutubeCooldownSettings,
    mutation_generation: u64,
) -> Result<YoutubeCooldownSettings> {
    let _write_guard = COOLDOWN_WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut conn = db::write_context(paths)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    claim_mutation_generation_conn(&tx, "cooldown_settings", mutation_generation, false)?;
    let settings = settings.normalized();
    tx.execute(
        "INSERT INTO meta(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![COOLDOWN_META_KEY, serde_json::to_string(&settings)?],
    )?;
    tx.commit()?;
    Ok(settings)
}

pub fn reset_cooldown_settings_with_generation(
    paths: &AppPaths,
    mutation_generation: u64,
) -> Result<YoutubeCooldownSettings> {
    set_cooldown_settings_with_generation(
        paths,
        YoutubeCooldownSettings::default(),
        mutation_generation,
    )
}

/// The lane's effective policy. Fragments/sleep/jitter/requests/limit/throttled-rate are always
/// the baseline verbatim in both modes (VV-0321-POLICY: adaptive protection never rewrites
/// operator bandwidth policy); only eligibility and the canary gate change.
pub fn effective_policy(
    baseline: &DownloaderBaselinePolicy,
    state: &DownloaderPolicySnapshot,
    now_ms: i64,
) -> DownloaderEffectivePolicy {
    let mode = state.mode.normalized();
    let (eligible, canary_only) = match mode {
        DownloaderPolicyMode::Normal => (true, false),
        DownloaderPolicyMode::Cooldown => {
            let probe_ready = state
                .next_eligible_probe_at_ms
                .map(|at| now_ms >= at)
                .unwrap_or(false);
            (probe_ready, probe_ready)
        }
        DownloaderPolicyMode::Legacy => unreachable!("DownloaderPolicyMode::normalized never returns Legacy"),
    };
    DownloaderEffectivePolicy {
        mode,
        concurrent_fragments: baseline.concurrent_fragments.max(1),
        sleep_interval_secs: baseline.sleep_interval_secs,
        sleep_jitter_secs: baseline.sleep_jitter_secs,
        max_sleep_interval_secs: baseline
            .sleep_interval_secs
            .saturating_add(baseline.sleep_jitter_secs),
        sleep_requests_secs: baseline.sleep_requests_secs,
        aggregate_start_interval_secs: baseline.sleep_interval_secs,
        update_tranche_size: baseline.update_tranche_size.max(1),
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
    let _ = operation;
    let mut conn = db::write_context(paths)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute(
        "DELETE FROM downloader_canary_lease WHERE provider=?1 AND operation=?2 \
         AND auth_fingerprint=?3 AND runtime_epoch=?4 AND expires_at_ms<=?5",
        params![
            provider,
            POLICY_STATE_OPERATION,
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
                POLICY_STATE_OPERATION,
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
            params![
                provider,
                POLICY_STATE_OPERATION,
                auth_fingerprint,
                runtime_epoch,
                job_id
            ],
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
            POLICY_STATE_OPERATION,
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
    let _ = operation;
    let row = conn
        .query_row(
            "SELECT mode, corroboration_count, success_streak, entered_at_ms, last_evidence_at_ms, next_eligible_probe_at_ms, version, cooldown_failed_probe_count FROM downloader_policy_state WHERE provider=?1 AND operation=?2 AND auth_fingerprint=?3 AND runtime_epoch=?4",
            params![provider, POLICY_STATE_OPERATION, auth_fingerprint, runtime_epoch],
            |row| {
                let mode: String = row.get(0)?;
                Ok(DownloaderPolicySnapshot {
                    provider: provider.to_string(),
                    operation: POLICY_STATE_OPERATION.to_string(),
                    auth_fingerprint: auth_fingerprint.to_string(),
                    runtime_epoch: runtime_epoch.to_string(),
                    mode: DownloaderPolicyMode::parse(&mode),
                    corroboration_count: row.get::<_, i64>(1)?.max(0) as u32,
                    success_streak: row.get::<_, i64>(2)?.max(0) as u32,
                    entered_at_ms: row.get(3)?,
                    last_evidence_at_ms: row.get(4)?,
                    next_eligible_probe_at_ms: row.get(5)?,
                    version: row.get::<_, i64>(6)?.max(1) as u64,
                    cooldown_failed_probe_count: row.get::<_, i64>(7)?.max(0) as u32,
                })
            },
        )
        .optional()?;
    Ok(row.unwrap_or_else(|| DownloaderPolicySnapshot {
        provider: provider.to_string(),
        operation: POLICY_STATE_OPERATION.to_string(),
        auth_fingerprint: auth_fingerprint.to_string(),
        runtime_epoch: runtime_epoch.to_string(),
        mode: DownloaderPolicyMode::Normal,
        corroboration_count: 0,
        success_streak: 0,
        entered_at_ms: now_ms(),
        last_evidence_at_ms: None,
        next_eligible_probe_at_ms: None,
        version: 1,
        cooldown_failed_probe_count: 0,
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

/// WP-0321 S4: two-mode state machine. A block (`RateLimited` — including a reclassified bot-check
/// that is not a saved-cookie rejection) while `Normal` enters `Cooldown` for `base_wait_secs`. Any
/// outcome recorded for the single controlled canary attempt (`effective.canary_only`) while in
/// `Cooldown` either returns to `Normal` (success) or escalates the wait (anything else). Any other
/// outcome, in either mode, never changes the mode. Auth/PO-token outcomes never change mode.
pub fn record_outcome(
    paths: &AppPaths,
    input: RecordDownloaderOutcome<'_>,
) -> Result<DownloaderPolicySnapshot> {
    let mut conn = db::write_context(paths)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let cooldown = load_cooldown_settings_conn(&tx)?;
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
        POLICY_STATE_OPERATION,
        input.auth_fingerprint,
        input.runtime_epoch,
    )?;
    if let Some(job_id) = input.lease_owner_job_id {
        tx.execute(
            "DELETE FROM downloader_canary_lease WHERE provider=?1 AND operation=?2 \
             AND auth_fingerprint=?3 AND runtime_epoch=?4 AND job_id=?5",
            params![
                input.provider,
                POLICY_STATE_OPERATION,
                input.auth_fingerprint,
                input.runtime_epoch,
                job_id,
            ],
        )?;
    }
    let before = state.mode.normalized();
    state.mode = before;
    let mut reason = None;
    let is_block = matches!(input.outcome, DownloaderOutcomeClass::RateLimited);
    let is_canary_attempt = before == DownloaderPolicyMode::Cooldown && input.effective.canary_only;

    if before == DownloaderPolicyMode::Normal && is_block {
        state.mode = DownloaderPolicyMode::Cooldown;
        state.cooldown_failed_probe_count = 0;
        state.next_eligible_probe_at_ms =
            Some(input.occurred_at_ms.saturating_add(cooldown.base_wait_ms()));
        reason = Some("rate_limited_block");
    } else if is_canary_attempt && input.outcome == DownloaderOutcomeClass::Success {
        state.mode = DownloaderPolicyMode::Normal;
        state.cooldown_failed_probe_count = 0;
        state.next_eligible_probe_at_ms = None;
        reason = Some("controlled_canary_success");
    } else if is_canary_attempt {
        // Any other outcome on the controlled canary escalates the retry wait. The mode itself
        // stays Cooldown ("Other outcomes during cooldown change nothing" beyond the wait).
        state.cooldown_failed_probe_count = state.cooldown_failed_probe_count.saturating_add(1);
        state.next_eligible_probe_at_ms = Some(
            input
                .occurred_at_ms
                .saturating_add(cooldown.escalating_wait_ms(state.cooldown_failed_probe_count)),
        );
    }
    // Everything else (including Auth/PO-token outcomes, and any non-canary outcome recorded
    // while already in Cooldown) leaves the mode and probe untouched.

    state.corroboration_count = 0;
    state.success_streak = 0;
    state.last_evidence_at_ms = Some(input.occurred_at_ms);
    if state.mode != before {
        state.entered_at_ms = input.occurred_at_ms;
        state.version = state.version.saturating_add(1);
        let evidence_ids = vec![outcome_id.clone()];
        let evidence_snapshot = transition_evidence_snapshot_conn(&tx, &evidence_ids)?;
        tx.execute(
            "INSERT INTO downloader_policy_transition(id,provider,operation,auth_fingerprint,runtime_epoch,before_mode,after_mode,reason,evidence_ids_json,evidence_snapshot_json,occurred_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![
                Uuid::new_v4().to_string(),
                input.provider,
                POLICY_STATE_OPERATION,
                input.auth_fingerprint,
                input.runtime_epoch,
                before.as_str(),
                state.mode.as_str(),
                reason.unwrap_or("classified_outcome"),
                serde_json::to_string(&evidence_ids)?,
                serde_json::to_string(&evidence_snapshot)?,
                input.occurred_at_ms,
            ],
        )?;
    }
    tx.execute(
        "INSERT INTO downloader_policy_state(provider,operation,auth_fingerprint,runtime_epoch,mode,corroboration_count,success_streak,entered_at_ms,last_evidence_at_ms,next_eligible_probe_at_ms,version,cooldown_failed_probe_count) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12) ON CONFLICT(provider,operation,auth_fingerprint,runtime_epoch) DO UPDATE SET mode=excluded.mode,corroboration_count=excluded.corroboration_count,success_streak=excluded.success_streak,entered_at_ms=excluded.entered_at_ms,last_evidence_at_ms=excluded.last_evidence_at_ms,next_eligible_probe_at_ms=excluded.next_eligible_probe_at_ms,version=excluded.version,cooldown_failed_probe_count=excluded.cooldown_failed_probe_count",
        params![
            state.provider,
            POLICY_STATE_OPERATION,
            state.auth_fingerprint,
            state.runtime_epoch,
            state.mode.as_str(),
            state.corroboration_count,
            state.success_streak,
            state.entered_at_ms,
            state.last_evidence_at_ms,
            state.next_eligible_probe_at_ms,
            state.version,
            state.cooldown_failed_probe_count,
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
        input.occurred_at_ms.saturating_sub(RAW_RETENTION_MS),
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
    let cutoff_ms = now_ms.saturating_sub(RAW_RETENTION_MS);
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

/// Explicit operator probe of the existing cooldown, without opening the full queue. VV-0319-
/// POLICY-008: the earliest probe is always at least 5 minutes after the last evidence/entry.
pub fn request_controlled_download_probe(
    paths: &AppPaths,
    auth_fingerprint: &str,
    runtime_epoch: &str,
) -> Result<DownloaderPolicySnapshot> {
    request_controlled_download_probe_at(paths, auth_fingerprint, runtime_epoch, now_ms())
}

fn request_controlled_download_probe_at(
    paths: &AppPaths,
    auth_fingerprint: &str,
    runtime_epoch: &str,
    now: i64,
) -> Result<DownloaderPolicySnapshot> {
    let mut conn = db::write_context(paths)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut state = load_policy_state_conn(&tx, PROVIDER_YOUTUBE, OPERATION_DOWNLOAD, auth_fingerprint, runtime_epoch)?;
    if state.mode != DownloaderPolicyMode::Cooldown {
        return Err(EngineError::InstallFailed("A controlled retry requires a download cooldown; other holds are not changed".into()));
    }
    let earliest = now.max(state.last_evidence_at_ms.unwrap_or(state.entered_at_ms).saturating_add(5 * 60_000));
    if state.next_eligible_probe_at_ms.is_none_or(|at| at > earliest) {
        state.next_eligible_probe_at_ms = Some(earliest);
        state.version = state.version.saturating_add(1);
        tx.execute(
            "UPDATE downloader_policy_state SET next_eligible_probe_at_ms=?1,version=?2 WHERE provider=?3 AND operation=?4 AND auth_fingerprint=?5 AND runtime_epoch=?6",
            params![earliest, state.version, PROVIDER_YOUTUBE, OPERATION_DOWNLOAD, auth_fingerprint, runtime_epoch],
        )?;
        tx.execute(
            "INSERT INTO downloader_policy_transition(id,provider,operation,auth_fingerprint,runtime_epoch,before_mode,after_mode,reason,evidence_ids_json,occurred_at_ms) VALUES(?1,?2,?3,?4,?5,'cooldown','cooldown','operator_controlled_probe','[]',?6)",
            params![Uuid::new_v4().to_string(), PROVIDER_YOUTUBE, OPERATION_DOWNLOAD, auth_fingerprint, runtime_epoch, now],
        )?;
    }
    // Never remove or renew a live lease: concurrent workers still share one probe.
    tx.commit()?;
    Ok(state)
}

/// Returns to `Normal` (the old "baseline" concept). Kept name for API stability.
pub fn return_to_baseline(
    paths: &AppPaths,
    provider: &str,
    operation: &str,
    auth_fingerprint: &str,
    runtime_epoch: &str,
) -> Result<DownloaderPolicySnapshot> {
    let _ = operation;
    let mut conn = db::write_context(paths)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut state = load_policy_state_conn(
        &tx,
        provider,
        POLICY_STATE_OPERATION,
        auth_fingerprint,
        runtime_epoch,
    )?;
    let before = state.mode;
    let now = now_ms();
    state.mode = DownloaderPolicyMode::Normal;
    state.corroboration_count = 0;
    state.success_streak = 0;
    state.entered_at_ms = now;
    state.last_evidence_at_ms = Some(now);
    state.next_eligible_probe_at_ms = None;
    state.cooldown_failed_probe_count = 0;
    state.version = state.version.saturating_add(1);
    tx.execute(
        "INSERT INTO downloader_policy_transition(id,provider,operation,auth_fingerprint,runtime_epoch,before_mode,after_mode,reason,evidence_ids_json,occurred_at_ms) VALUES(?1,?2,?3,?4,?5,?6,'normal','operator_return_to_baseline','[]',?7)",
        params![Uuid::new_v4().to_string(), provider, POLICY_STATE_OPERATION, auth_fingerprint, runtime_epoch, before.as_str(), now],
    )?;
    tx.execute(
        "INSERT INTO downloader_policy_state(provider,operation,auth_fingerprint,runtime_epoch,mode,corroboration_count,success_streak,entered_at_ms,last_evidence_at_ms,next_eligible_probe_at_ms,version,cooldown_failed_probe_count) VALUES(?1,?2,?3,?4,'normal',0,0,?5,?5,NULL,?6,0) ON CONFLICT(provider,operation,auth_fingerprint,runtime_epoch) DO UPDATE SET mode='normal',corroboration_count=0,success_streak=0,entered_at_ms=excluded.entered_at_ms,last_evidence_at_ms=excluded.last_evidence_at_ms,next_eligible_probe_at_ms=NULL,version=excluded.version,cooldown_failed_probe_count=0",
        params![provider, POLICY_STATE_OPERATION, auth_fingerprint, runtime_epoch, now, state.version],
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
                outcome_summary_from_row,
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
            let evidence_optional = matches!(
                reason.as_str(),
                "operator_return_to_baseline" | "wp0321_simplified_modes"
            );
            if !evidence_optional
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

/// WP-0321 S4: offline replay of the two-mode machine over a `DownloaderPolicyHistory` snapshot
/// (used by the DiagnosticsPage receipt). `Normal` + a `RateLimited` block enters `Cooldown`; while
/// `Cooldown`, only an outcome tagged as the controlled canary (`effective_policy.canary_only`)
/// can change the mode, and only a `Success` canary returns to `Normal`. Everything else leaves
/// the mode unchanged.
pub fn replay_policy_history(history: &DownloaderPolicyHistory) -> DownloaderPolicyReplayReceipt {
    let mut ordered = history.outcomes.clone();
    ordered.sort_by_key(|event| event.occurred_at_ms);
    let mut mode = DownloaderPolicyMode::Normal;
    let mut mode_path = vec![mode];
    let mut unknown_events = 0_usize;
    for event in &ordered {
        let class = parse_outcome_class(&event.outcome_class);
        let before = mode;
        if class == DownloaderOutcomeClass::Unknown {
            unknown_events += 1;
        }
        match mode {
            DownloaderPolicyMode::Normal if class == DownloaderOutcomeClass::RateLimited => {
                mode = DownloaderPolicyMode::Cooldown;
            }
            DownloaderPolicyMode::Cooldown if event.effective_policy.canary_only => {
                if class == DownloaderOutcomeClass::Success {
                    mode = DownloaderPolicyMode::Normal;
                }
                // Any other canary outcome escalates the wait but leaves the mode at Cooldown.
            }
            _ => {}
        }
        if mode != before {
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

/// WP-0321 S4: one-time settings migration from the pre-simplification legacy antibot/tuning meta
/// keys to the per-lane `ProviderTransferPolicy` (schema 2) and `YoutubeCooldownSettings`. Reads
/// the legacy antibot meta keys directly by their string names (the corresponding Rust
/// fields/constants are removed from jobs.rs as part of this same work packet). Idempotent: if the
/// on-disk `provider_transfer_settings.json` already declares schema 2, this is a no-op.
pub fn migrate_legacy_youtube_pacing(paths: &AppPaths) -> Result<Option<PacingMigrationReceipt>> {
    let settings_path = paths.provider_transfer_settings_path();
    if settings_path.exists() {
        if let Ok(raw) = std::fs::read(&settings_path) {
            if let Ok(existing) = serde_json::from_slice::<serde_json::Value>(&raw) {
                if existing.get("schema_version").and_then(|v| v.as_u64()) == Some(2) {
                    return Ok(None);
                }
            }
        }
    }

    let (recurring_min, recurring_max, enumeration_sleep_requests, legacy_tuning_json) = {
        let conn = db::open_readonly(paths)?;
        let read_i64 = |key: &str| -> Option<i64> {
            conn.query_row(
                "SELECT value FROM meta WHERE key=?1",
                [key],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .ok()
            .flatten()
            .and_then(|raw| raw.trim().parse::<i64>().ok())
        };
        let legacy_tuning_json: Option<String> = conn
            .query_row(
                "SELECT value FROM meta WHERE key=?1",
                [LEGACY_TUNING_META_KEY],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        (
            read_i64(LEGACY_ANTIBOT_RECURRING_MIN_SLEEP_META_KEY),
            read_i64(LEGACY_ANTIBOT_RECURRING_MAX_SLEEP_META_KEY),
            read_i64(LEGACY_ANTIBOT_ENUMERATION_SLEEP_REQUESTS_META_KEY),
            legacy_tuning_json,
        )
    };

    let current = crate::config::load_provider_transfer_settings(paths)?;
    let presets = crate::config::load_download_presets_config(paths)?;
    let default_preset = presets
        .default_preset_id
        .as_deref()
        .and_then(|id| presets.presets.iter().find(|preset| preset.id == id))
        .or_else(|| presets.presets.first());
    let preset_sleep = default_preset.map_or(0, |preset| preset.yt_dlp_sleep_interval);
    let preset_requests = default_preset.map_or(0, |preset| preset.yt_dlp_sleep_requests);
    let preset_fragments = default_preset.map_or(1, |preset| preset.yt_dlp_concurrent_fragments);
    let preset_limit = default_preset.and_then(|preset| preset.yt_dlp_limit_rate.clone());

    let antibot_min = recurring_min.unwrap_or(0).max(0) as u32;
    let antibot_max = recurring_max.unwrap_or(0).max(0) as u32;
    let jitter = antibot_max.saturating_sub(antibot_min).max(5);
    let enumeration_requests = enumeration_sleep_requests.unwrap_or(0).max(0) as u32;

    let single_old = current.youtube_single.clone();
    let recurring_old = current.youtube_recurring.clone();
    let single_new = crate::config::ProviderTransferPolicy {
        sleep_interval_secs: preset_sleep
            .max(single_old.sleep_interval_secs)
            .max(antibot_min),
        sleep_jitter_secs: jitter,
        sleep_requests_secs: preset_requests.max(single_old.sleep_requests_secs),
        concurrent_fragments: preset_fragments.min(single_old.concurrent_fragments).min(1),
        limit_rate: preset_limit.clone().or_else(|| single_old.limit_rate.clone()),
    };
    let recurring_new = crate::config::ProviderTransferPolicy {
        sleep_interval_secs: preset_sleep
            .max(recurring_old.sleep_interval_secs)
            .max(antibot_min),
        sleep_jitter_secs: jitter,
        sleep_requests_secs: preset_requests
            .max(recurring_old.sleep_requests_secs)
            .max(enumeration_requests),
        concurrent_fragments: preset_fragments.min(recurring_old.concurrent_fragments).min(1),
        limit_rate: preset_limit.or_else(|| recurring_old.limit_rate.clone()),
    };

    let mut next = current;
    next.schema_version = crate::config::PROVIDER_TRANSFER_SETTINGS_SCHEMA_VERSION;
    next.youtube_single = single_new.clone();
    next.youtube_recurring = recurring_new.clone();
    crate::config::save_provider_transfer_settings(paths, &next)?;

    let cooldown = legacy_tuning_json
        .as_deref()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
        .map(|value| YoutubeCooldownSettings {
            base_wait_secs: value
                .get("cooldown_base_dwell_secs")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(3_600),
            max_wait_secs: value
                .get("cooldown_dwell_secs")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(21_600),
        })
        .unwrap_or_default()
        .normalized();
    let cooldown_json = serde_json::to_string(&cooldown)?;

    db::AppDatabase::for_paths(paths)?.write(
        db::DatabaseOperationContext::new("youtube_protection", "migrate_legacy_pacing")
            .foreground(),
        TransactionBehavior::Immediate,
        |transaction| {
            transaction.execute(
                "INSERT INTO meta(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                params![COOLDOWN_META_KEY, cooldown_json],
            )?;
            for key in [
                LEGACY_ANTIBOT_RECURRING_MIN_SLEEP_META_KEY,
                LEGACY_ANTIBOT_RECURRING_MAX_SLEEP_META_KEY,
                LEGACY_ANTIBOT_ENUMERATION_SLEEP_REQUESTS_META_KEY,
                LEGACY_ANTIBOT_ADAPTIVE_PROTECTION_ENABLED_META_KEY,
                LEGACY_TUNING_META_KEY,
            ] {
                transaction.execute("DELETE FROM meta WHERE key=?1", [key])?;
            }
            Ok(())
        },
    )?;

    Ok(Some(PacingMigrationReceipt {
        youtube_single: PacingMigrationLaneReceipt {
            old: single_old,
            new: single_new,
        },
        youtube_recurring: PacingMigrationLaneReceipt {
            old: recurring_old,
            new: recurring_new,
        },
        cooldown,
    }))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PacingMigrationLaneReceipt {
    pub old: crate::config::ProviderTransferPolicy,
    pub new: crate::config::ProviderTransferPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PacingMigrationReceipt {
    pub youtube_single: PacingMigrationLaneReceipt,
    pub youtube_recurring: PacingMigrationLaneReceipt,
    pub cooldown: YoutubeCooldownSettings,
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
            sleep_jitter_secs: 10,
            sleep_requests_secs: 0,
            update_tranche_size: 25,
            limit_rate: Some("4M".to_string()),
            throttled_rate: Some("100K".to_string()),
        }
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
    fn classifier_keeps_failure_domains_distinct() {
        assert_eq!(
            classify_youtube_outcome(Some("HTTP Error 429: Too Many Requests")),
            DownloaderOutcomeClass::RateLimited
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
        assert_eq!(
            classify_youtube_outcome(Some("login required to view this content")),
            DownloaderOutcomeClass::AuthenticationRequiredOrInvalid
        );
    }

    /// WP-0321 S4: the anti-bot block message is a rate-limit block unless it is specifically a
    /// saved-cookie identity rejection (which is an auth problem, not a pacing problem).
    #[test]
    fn bot_check_without_cookie_rejection_classifies_as_block() {
        assert_eq!(
            classify_youtube_outcome_with_cookie_rejection(
                Some("Sign in to confirm you're not a bot"),
                false,
            ),
            DownloaderOutcomeClass::RateLimited
        );
        assert_eq!(
            classify_youtube_outcome_with_cookie_rejection(
                Some("Sign in to confirm you're not a bot"),
                true,
            ),
            DownloaderOutcomeClass::AuthenticationRequiredOrInvalid
        );
        assert_eq!(
            classify_youtube_outcome(Some("Sign in to confirm you're not a bot")),
            DownloaderOutcomeClass::RateLimited,
            "the plain classifier defaults to not-a-cookie-rejection"
        );
    }

    #[test]
    fn single_block_enters_cooldown_with_base_wait() {
        let (_dir, paths) = paths();
        let settings = get_cooldown_settings(&paths).expect("default cooldown settings");
        assert_eq!(settings.base_wait_secs, 3_600);
        assert_eq!(settings.max_wait_secs, 21_600);

        let state = record(&paths, "video-a", DownloaderOutcomeClass::RateLimited, 1_000_000);
        assert_eq!(state.mode, DownloaderPolicyMode::Cooldown);
        assert_eq!(state.cooldown_failed_probe_count, 0);
        assert_eq!(
            state.next_eligible_probe_at_ms,
            Some(1_000_000 + settings.base_wait_ms())
        );

        let history = policy_history(
            &paths,
            PROVIDER_YOUTUBE,
            OPERATION_DOWNLOAD,
            "auth-a",
            "epoch-a",
            10,
        )
        .expect("history");
        assert_eq!(history.transitions.len(), 1);
        assert_eq!(history.transitions[0].reason, "rate_limited_block");
    }

    #[test]
    fn outcomes_during_cooldown_do_not_change_wait() {
        let (_dir, paths) = paths();
        let entered = record(&paths, "video-a", DownloaderOutcomeClass::RateLimited, 1_000_000);
        assert_eq!(entered.mode, DownloaderPolicyMode::Cooldown);
        let probe_at = entered.next_eligible_probe_at_ms.expect("probe");

        // A non-canary outcome recorded while still in Cooldown (not the eligible probe window)
        // must not move the mode or the probe time.
        let unaffected = record(&paths, "video-b", DownloaderOutcomeClass::Unknown, probe_at - 1);
        assert_eq!(unaffected.mode, DownloaderPolicyMode::Cooldown);
        assert_eq!(unaffected.next_eligible_probe_at_ms, Some(probe_at));
    }

    #[test]
    fn cooldown_canary_success_returns_to_normal() {
        let (_dir, paths) = paths();
        let entered = record(&paths, "video-a", DownloaderOutcomeClass::RateLimited, 1_000_000);
        let probe_at = entered.next_eligible_probe_at_ms.expect("probe");
        let cooldown_state = load_policy_state(
            &paths,
            PROVIDER_YOUTUBE,
            OPERATION_DOWNLOAD,
            "auth-a",
            "epoch-a",
        )
        .expect("cooldown state");
        let base = baseline();
        let effective = effective_policy(&base, &cooldown_state, probe_at);
        assert!(effective.eligible);
        assert!(effective.canary_only);

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
                lease_owner_job_id: None,
                duration_ms: Some(100),
                occurred_at_ms: probe_at + 1,
            },
        )
        .expect("canary success");
        assert_eq!(reopened.mode, DownloaderPolicyMode::Normal);
        assert_eq!(reopened.cooldown_failed_probe_count, 0);
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

    /// WP-0320/S4: a failed canary doubles the wait (base -> 2x -> 4x -> cap at max), and a
    /// success resets the failed-probe counter and returns to Normal.
    #[test]
    fn cooldown_dwell_escalates_on_repeated_canary_failure_and_resets_on_success() {
        let (_dir, paths) = paths();
        let entered = record(&paths, "video-a", DownloaderOutcomeClass::RateLimited, 1_000_000);
        assert_eq!(entered.mode, DownloaderPolicyMode::Cooldown);
        let settings = get_cooldown_settings(&paths).expect("cooldown settings");
        let first_wait_ms = entered.next_eligible_probe_at_ms.expect("probe") - entered.entered_at_ms;
        assert_eq!(first_wait_ms, settings.base_wait_ms(), "fresh cooldown entry waits the base");

        let expected_waits_ms = [
            settings.base_wait_ms() * 2,
            settings.base_wait_ms() * 4,
            settings.max_wait_ms(),
        ];
        let mut probe_at = entered.next_eligible_probe_at_ms.expect("probe");
        let base = baseline();
        for (attempt, expected_wait_ms) in expected_waits_ms.into_iter().enumerate() {
            let state = load_policy_state(
                &paths,
                PROVIDER_YOUTUBE,
                OPERATION_DOWNLOAD,
                "auth-a",
                "epoch-a",
            )
            .expect("state");
            let effective = effective_policy(&base, &state, probe_at);
            assert!(effective.canary_only, "attempt {attempt}");
            let after = record_outcome(
                &paths,
                RecordDownloaderOutcome {
                    provider: PROVIDER_YOUTUBE,
                    operation: OPERATION_DOWNLOAD,
                    canonical_target: "video-canary",
                    auth_fingerprint: "auth-a",
                    runtime_epoch: "epoch-a",
                    baseline: &base,
                    effective: &effective,
                    outcome: DownloaderOutcomeClass::RateLimited,
                    error_text: Some("HTTP Error 429"),
                    incident_id: None,
                    lease_owner_job_id: None,
                    duration_ms: Some(100),
                    occurred_at_ms: probe_at,
                },
            )
            .expect("failed canary");
            assert_eq!(after.mode, DownloaderPolicyMode::Cooldown, "attempt {attempt}");
            assert_eq!(after.cooldown_failed_probe_count, attempt as u32 + 1);
            let wait_ms = after.next_eligible_probe_at_ms.expect("probe") - probe_at;
            assert_eq!(wait_ms, expected_wait_ms, "attempt {attempt}");
            probe_at = after.next_eligible_probe_at_ms.expect("probe");
        }

        let recovered = record(&paths, "video-canary", DownloaderOutcomeClass::Success, probe_at);
        assert_eq!(recovered.mode, DownloaderPolicyMode::Normal);
        assert_eq!(recovered.cooldown_failed_probe_count, 0);
    }

    /// WP-0321 S4: the shared state row is keyed by `POLICY_STATE_OPERATION`, so an enumeration
    /// outcome that trips a block also pauses downloads for the same identity.
    #[test]
    fn enumeration_block_pauses_downloads() {
        let (_dir, paths) = paths();
        let current = load_policy_state(
            &paths,
            PROVIDER_YOUTUBE,
            OPERATION_ENUMERATION,
            "auth-a",
            "epoch-a",
        )
        .expect("state");
        let base = baseline();
        let effective = effective_policy(&base, &current, 1_000_000);
        record_outcome(
            &paths,
            RecordDownloaderOutcome {
                provider: PROVIDER_YOUTUBE,
                operation: OPERATION_ENUMERATION,
                canonical_target: "https://youtube.test/channel",
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
        .expect("enumeration block");

        let download_state = load_policy_state(
            &paths,
            PROVIDER_YOUTUBE,
            OPERATION_DOWNLOAD,
            "auth-a",
            "epoch-a",
        )
        .expect("download state reads the shared row");
        assert_eq!(download_state.mode, DownloaderPolicyMode::Cooldown);
        let download_effective = effective_policy(&base, &download_state, 1_000_000);
        assert!(!download_effective.eligible, "downloads are paused too");
    }

    #[test]
    fn auth_and_po_token_outcomes_never_change_mode() {
        for outcome in [
            DownloaderOutcomeClass::AuthenticationRequiredOrInvalid,
            DownloaderOutcomeClass::PoTokenOrClientCapability,
        ] {
            let (_dir, paths) = paths();
            let state = record(&paths, "video-a", outcome, 2_000_000);
            assert_eq!(state.mode, DownloaderPolicyMode::Normal);
            assert_eq!(state.corroboration_count, 0);
        }
    }

    #[test]
    fn effective_policy_preserves_baseline_bandwidth_verbatim_in_both_modes() {
        let baseline = baseline();
        let normal_state = DownloaderPolicySnapshot {
            provider: PROVIDER_YOUTUBE.to_string(),
            operation: POLICY_STATE_OPERATION.to_string(),
            auth_fingerprint: "a".to_string(),
            runtime_epoch: "e".to_string(),
            mode: DownloaderPolicyMode::Normal,
            corroboration_count: 0,
            success_streak: 0,
            entered_at_ms: 0,
            last_evidence_at_ms: None,
            next_eligible_probe_at_ms: None,
            version: 1,
            cooldown_failed_probe_count: 0,
        };
        let normal_effective = effective_policy(&baseline, &normal_state, 10);
        assert_eq!(normal_effective.concurrent_fragments, baseline.concurrent_fragments);
        assert_eq!(normal_effective.sleep_interval_secs, baseline.sleep_interval_secs);
        assert_eq!(normal_effective.sleep_jitter_secs, baseline.sleep_jitter_secs);
        assert_eq!(normal_effective.limit_rate, baseline.limit_rate);
        assert_eq!(normal_effective.throttled_rate, baseline.throttled_rate);

        let cooldown_state = DownloaderPolicySnapshot {
            mode: DownloaderPolicyMode::Cooldown,
            next_eligible_probe_at_ms: Some(5),
            ..normal_state
        };
        let cooldown_effective = effective_policy(&baseline, &cooldown_state, 10);
        assert_eq!(cooldown_effective.concurrent_fragments, baseline.concurrent_fragments);
        assert_eq!(cooldown_effective.sleep_interval_secs, baseline.sleep_interval_secs);
        assert_eq!(cooldown_effective.limit_rate, baseline.limit_rate);
        assert!(cooldown_effective.eligible);
        assert!(cooldown_effective.canary_only);
    }

    #[test]
    fn legacy_mode_outcome_json_still_loads() {
        let baseline_json = serde_json::json!({
            "concurrent_fragments": 1,
            "sleep_interval_secs": 8,
            "sleep_requests_secs": 6,
            "update_tranche_size": 1,
            "limit_rate": null,
            "throttled_rate": null,
        });
        let baseline: DownloaderBaselinePolicy =
            serde_json::from_value(baseline_json).expect("legacy baseline loads");
        assert_eq!(baseline.sleep_jitter_secs, 0);

        let effective_json = serde_json::json!({
            "mode": "conservative",
            "concurrent_fragments": 1,
            "sleep_interval_secs": 20,
            "max_sleep_interval_secs": 40,
            "sleep_requests_secs": 2,
            "aggregate_start_interval_secs": 20,
            "update_tranche_size": 5,
            "limit_rate": null,
            "throttled_rate": null,
            "eligible": true,
            "canary_only": false,
        });
        let effective: DownloaderEffectivePolicy =
            serde_json::from_value(effective_json).expect("legacy effective loads");
        assert_eq!(effective.mode, DownloaderPolicyMode::Legacy);
        assert_eq!(effective.mode.normalized(), DownloaderPolicyMode::Normal);
        assert_eq!(effective.sleep_jitter_secs, 0);
    }

    #[test]
    fn controlled_probe_preserves_cooldown_floor_and_exclusive_lease() {
        let (_dir, paths) = paths();
        let conn = db::open(&paths).unwrap();
        db::migrate(&conn).unwrap();
        conn.execute("INSERT INTO downloader_policy_state(provider,operation,auth_fingerprint,runtime_epoch,mode,entered_at_ms,last_evidence_at_ms,next_eligible_probe_at_ms,version) VALUES('youtube','download','auth','epoch','cooldown',1000,1000,21601000,1)", []).unwrap();
        drop(conn);
        let first = request_controlled_download_probe_at(&paths,"auth","epoch",2000).unwrap();
        assert_eq!(first.mode, DownloaderPolicyMode::Cooldown);
        assert_eq!(first.next_eligible_probe_at_ms,Some(301000));
        assert_eq!(first.last_evidence_at_ms,Some(1000));
        assert!(claim_cooldown_canary(&paths,PROVIDER_YOUTUBE,OPERATION_DOWNLOAD,"auth","epoch","early",300000).unwrap().is_none());
        assert!(claim_cooldown_canary(&paths,PROVIDER_YOUTUBE,OPERATION_DOWNLOAD,"auth","epoch","one",301001).unwrap().is_some());
        request_controlled_download_probe_at(&paths,"auth","epoch",301002).unwrap();
        assert!(claim_cooldown_canary(&paths,PROVIDER_YOUTUBE,OPERATION_DOWNLOAD,"auth","epoch","two",301002).unwrap().is_none());
        let conn = db::write_context(&paths).unwrap();
        conn.execute("UPDATE downloader_policy_state SET mode='normal'",[]).unwrap();
        drop(conn);
        assert!(request_controlled_download_probe_at(&paths,"auth","epoch",999999).is_err());
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
        let state = load_policy_state(&paths, PROVIDER_YOUTUBE, OPERATION_DOWNLOAD, "auth", "epoch")
            .expect("state");
        let non_canary_effective = effective_policy(&baseline, &state, 0);
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
            serde_json::to_string(&effective_policy(&baseline(), &load_policy_state_conn(&conn, PROVIDER_YOUTUBE, OPERATION_DOWNLOAD, "auth", "epoch").unwrap(), 0)).unwrap();
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
    fn durable_mutation_generation_survives_reload_and_rolls_back_atomically() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        db::ensure_schema(&paths).expect("schema");
        let saved =
            set_cooldown_settings_with_generation(&paths, YoutubeCooldownSettings::default(), 100)
                .expect("first durable generation");
        drop(saved);
        assert!(set_cooldown_settings_with_generation(
            &paths,
            YoutubeCooldownSettings::default(),
            99
        )
        .is_err());

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
        assert!(set_cooldown_settings_with_generation(
            &paths,
            YoutubeCooldownSettings::default(),
            101
        )
        .is_ok());
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

    /// WP-0321 S4: replay over the durable transition log lands on the same two-mode result as
    /// replay over the raw-outcome snapshot.
    #[test]
    fn replay_uses_two_mode_machine() {
        let (_dir, paths) = paths();
        record(&paths, "video-a", DownloaderOutcomeClass::RateLimited, 1_000_000);
        let history = policy_history(
            &paths,
            PROVIDER_YOUTUBE,
            OPERATION_DOWNLOAD,
            "auth-a",
            "epoch-a",
            50,
        )
        .expect("history");
        let replay = replay_policy_history(&history);
        assert_eq!(replay.final_mode, DownloaderPolicyMode::Cooldown);
        assert_eq!(replay.mode_path, vec![DownloaderPolicyMode::Normal, DownloaderPolicyMode::Cooldown]);

        let from_store = replay_policy_history_from_store(
            &paths,
            PROVIDER_YOUTUBE,
            OPERATION_DOWNLOAD,
            "auth-a",
            "epoch-a",
        )
        .expect("replay from store");
        assert_eq!(from_store.final_mode, DownloaderPolicyMode::Cooldown);
    }

    #[test]
    fn legacy_pacing_migration_preserves_effective_values() {
        let (_dir, paths) = paths();
        let conn = db::write_context(&paths).expect("write context");
        conn.execute(
            "INSERT INTO meta(key,value) VALUES(?1,?2)",
            params![LEGACY_ANTIBOT_RECURRING_MIN_SLEEP_META_KEY, "5"],
        )
        .expect("seed antibot min");
        conn.execute(
            "INSERT INTO meta(key,value) VALUES(?1,?2)",
            params![LEGACY_ANTIBOT_RECURRING_MAX_SLEEP_META_KEY, "10"],
        )
        .expect("seed antibot max");
        conn.execute(
            "INSERT INTO meta(key,value) VALUES(?1,?2)",
            params![LEGACY_ANTIBOT_ENUMERATION_SLEEP_REQUESTS_META_KEY, "2"],
        )
        .expect("seed enumeration requests");
        conn.execute(
            "INSERT INTO meta(key,value) VALUES(?1,?2)",
            params![LEGACY_ANTIBOT_ADAPTIVE_PROTECTION_ENABLED_META_KEY, "1"],
        )
        .expect("seed toggle");
        conn.execute(
            "INSERT INTO meta(key,value) VALUES(?1,?2)",
            params![
                LEGACY_TUNING_META_KEY,
                serde_json::json!({"cooldown_base_dwell_secs": 1800, "cooldown_dwell_secs": 7200})
                    .to_string()
            ],
        )
        .expect("seed legacy tuning");
        drop(conn);

        // Default preset (Safest-equivalent for this test) supplies sleep=8/requests=6/fragments=1.
        let presets = crate::config::load_download_presets_config(&paths).expect("presets");
        let mut updated = presets.clone();
        {
            let default_id = updated.default_preset_id.clone().expect("default preset");
            let preset = updated
                .presets
                .iter_mut()
                .find(|preset| preset.id == default_id)
                .expect("default preset row");
            preset.yt_dlp_sleep_interval = 8;
            preset.yt_dlp_sleep_requests = 6;
            preset.yt_dlp_concurrent_fragments = 1;
        }
        crate::config::save_download_presets_config(&paths, &updated).expect("save presets");

        let receipt = migrate_legacy_youtube_pacing(&paths)
            .expect("migration runs")
            .expect("migration receipt");
        assert_eq!(receipt.youtube_single.new.sleep_interval_secs, 8);
        assert_eq!(receipt.youtube_single.new.sleep_jitter_secs, 5);
        assert_eq!(receipt.youtube_single.new.sleep_requests_secs, 6);
        assert_eq!(receipt.youtube_single.new.concurrent_fragments, 1);
        assert_eq!(receipt.youtube_recurring.new.sleep_interval_secs, 10);
        assert_eq!(receipt.youtube_recurring.new.sleep_jitter_secs, 5);
        assert_eq!(receipt.youtube_recurring.new.sleep_requests_secs, 6);
        assert_eq!(receipt.youtube_recurring.new.concurrent_fragments, 1);
        assert_eq!(receipt.cooldown.base_wait_secs, 1800);
        assert_eq!(receipt.cooldown.max_wait_secs, 7200);

        let saved = crate::config::load_provider_transfer_settings(&paths).expect("saved settings");
        assert_eq!(saved.youtube_single.sleep_interval_secs, 8);
        assert_eq!(saved.youtube_recurring.sleep_interval_secs, 10);

        let cooldown = get_cooldown_settings(&paths).expect("migrated cooldown settings");
        assert_eq!(cooldown.base_wait_secs, 1800);
        assert_eq!(cooldown.max_wait_secs, 7200);

        let conn = db::open_readonly(&paths).expect("read meta");
        for key in [
            LEGACY_ANTIBOT_RECURRING_MIN_SLEEP_META_KEY,
            LEGACY_ANTIBOT_RECURRING_MAX_SLEEP_META_KEY,
            LEGACY_ANTIBOT_ENUMERATION_SLEEP_REQUESTS_META_KEY,
            LEGACY_ANTIBOT_ADAPTIVE_PROTECTION_ENABLED_META_KEY,
            LEGACY_TUNING_META_KEY,
        ] {
            let remaining: Option<String> = conn
                .query_row("SELECT value FROM meta WHERE key=?1", [key], |row| row.get(0))
                .optional()
                .expect("query meta");
            assert!(remaining.is_none(), "{key} must be deleted");
        }
    }

    #[test]
    fn legacy_pacing_migration_is_idempotent() {
        let (_dir, paths) = paths();
        let first = migrate_legacy_youtube_pacing(&paths).expect("first migration");
        assert!(first.is_some());
        let second = migrate_legacy_youtube_pacing(&paths).expect("second migration is a no-op");
        assert!(second.is_none());
    }

    #[test]
    fn provider_transfer_v1_without_jitter_loads() {
        // Covered directly in config.rs (owns ProviderTransferSettings); this asserts the
        // youtube_protection-side baseline built from such a policy still carries jitter 0.
        let baseline = DownloaderBaselinePolicy {
            concurrent_fragments: 1,
            sleep_interval_secs: 5,
            sleep_jitter_secs: 0,
            sleep_requests_secs: 2,
            update_tranche_size: 1,
            limit_rate: None,
            throttled_rate: None,
        };
        let state = DownloaderPolicySnapshot {
            provider: PROVIDER_YOUTUBE.to_string(),
            operation: POLICY_STATE_OPERATION.to_string(),
            auth_fingerprint: "a".to_string(),
            runtime_epoch: "e".to_string(),
            mode: DownloaderPolicyMode::Normal,
            corroboration_count: 0,
            success_streak: 0,
            entered_at_ms: 0,
            last_evidence_at_ms: None,
            next_eligible_probe_at_ms: None,
            version: 1,
            cooldown_failed_probe_count: 0,
        };
        let effective = effective_policy(&baseline, &state, 0);
        assert_eq!(effective.max_sleep_interval_secs, 5);
    }
}
