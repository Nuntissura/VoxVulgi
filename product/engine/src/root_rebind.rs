use crate::config::{self, FeatureStorageRootsConfig};
use crate::db;
use crate::paths::AppPaths;
use crate::{persistence, EngineError, Result};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc::{sync_channel, Receiver, SyncSender, TrySendError},
    Arc, Mutex, OnceLock,
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

const ROOT_ALIAS_SCHEMA_VERSION: u32 = 1;
const ROOT_REBIND_RECEIPT_SCHEMA_VERSION: u32 = 3;
const MAX_IDENTITY_EVIDENCE: usize = 64;
const MAX_ACTIVE_ROOT_ALIASES: usize = 64;
const CANONICAL_IDENTITY_SAMPLE_COUNT: usize = 3;
const MAX_CANONICAL_IDENTITY_CANDIDATE_PROBES: usize = 48;
const ROOT_REBIND_METADATA_IO_TIMEOUT: Duration = Duration::from_secs(3);
const ROOT_REBIND_IDENTITY_IO_TIMEOUT: Duration = Duration::from_secs(10);
const ROOT_REBIND_IO_POLL: Duration = Duration::from_millis(50);
const ALIAS_TARGET_PROBE_TIMEOUT: Duration = Duration::from_millis(300);
pub(crate) const ALIAS_TARGET_CACHE_TTL: Duration = Duration::from_secs(1);
static ALIAS_TARGET_AVAILABILITY: OnceLock<Mutex<HashMap<String, (Instant, bool)>>> =
    OnceLock::new();
static ROOT_ALIAS_WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static ROOT_REBIND_OPERATION_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
const ROOT_REBIND_WORKER_COUNT: usize = 2;
const ROOT_REBIND_RECOVERY_WORKER_COUNT: usize = 1;
const ROOT_REBIND_QUEUE_CAPACITY: usize = 8;
const ROOT_REBIND_TASK_RETENTION: usize = 128;
const ROOT_REBIND_IO_WORKER_COUNT: usize = 2;
const ROOT_REBIND_RECOVERY_IO_WORKER_COUNT: usize = 1;
const ROOT_REBIND_IO_QUEUE_CAPACITY: usize = 8;
static ROOT_REBIND_TASK_QUEUE: OnceLock<SyncSender<RootRebindWorkRequest>> = OnceLock::new();
static ROOT_REBIND_RECOVERY_TASK_QUEUE: OnceLock<SyncSender<RootRebindWorkRequest>> =
    OnceLock::new();
static ROOT_REBIND_TASKS: OnceLock<Mutex<HashMap<String, RootRebindTaskStatus>>> = OnceLock::new();
static ROOT_REBIND_TASK_CANCELLATIONS: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> =
    OnceLock::new();
static ROOT_REBIND_WORKERS_STARTED: AtomicU64 = AtomicU64::new(0);
static ROOT_REBIND_RECOVERY_WORKERS_STARTED: AtomicU64 = AtomicU64::new(0);
static ROOT_REBIND_IO_EXECUTOR: OnceLock<RootRebindIoExecutor> = OnceLock::new();
static ROOT_REBIND_RECOVERY_IO_EXECUTOR: OnceLock<RootRebindIoExecutor> = OnceLock::new();

thread_local! {
    static ROOT_REBIND_RECOVERY_IO_LANE: Cell<bool> = const { Cell::new(false) };
}

fn alias_target_available(target_root: &Path, fresh: bool) -> bool {
    let key = target_root.to_string_lossy().to_string();
    let cache = ALIAS_TARGET_AVAILABILITY.get_or_init(|| Mutex::new(HashMap::new()));
    if !fresh {
        if let Some((observed_at, available)) = cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&key)
            .copied()
        {
            if observed_at.elapsed() <= ALIAS_TARGET_CACHE_TTL {
                return available;
            }
        }
    }
    let available = crate::paths::path_is_dir_bounded(target_root, ALIAS_TARGET_PROBE_TIMEOUT);
    cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(key, (Instant::now(), available));
    available
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RootAliasRecord {
    pub id: String,
    pub from_root: String,
    pub to_root: String,
    pub verified_at_ms: i64,
    pub status: String,
    pub receipt_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RootAliasesConfig {
    pub schema_version: u32,
    #[serde(default)]
    pub aliases: Vec<RootAliasRecord>,
}

impl Default for RootAliasesConfig {
    fn default() -> Self {
        Self {
            schema_version: ROOT_ALIAS_SCHEMA_VERSION,
            aliases: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RootIdentityEvidence {
    pub relative_path: String,
    pub expected_bytes: Option<u64>,
    pub observed_bytes: u64,
    #[serde(default)]
    pub expected_sample_sha256: Option<String>,
    #[serde(default)]
    pub observed_sample_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RootRebindRowChange {
    pub surface: String,
    pub row_id: String,
    pub field: String,
    pub original_value: String,
    pub rebound_value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct RootRebindDryRun {
    pub feature_root_matches: u64,
    pub video_library_root_matches: u64,
    pub youtube_subscription_override_matches: u64,
    pub instagram_subscription_override_matches: u64,
    pub queued_destination_matches: u64,
    #[serde(default)]
    pub running_destination_matches: u64,
    #[serde(default)]
    pub running_destination_job_ids: Vec<String>,
    pub historical_library_path_matches: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RootRebindBackupReference {
    pub sqlite_path: String,
    pub sqlite_integrity: String,
    pub feature_config_path: Option<String>,
    pub feature_config_verified: bool,
    #[serde(default)]
    pub aliases_config_path: String,
    #[serde(default)]
    pub aliases_config_sha256: String,
    #[serde(default)]
    pub aliases_config_source_existed: bool,
    #[serde(default)]
    pub aliases_config_verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RootRebindReceipt {
    pub schema_version: u32,
    pub id: String,
    pub from_root: String,
    pub to_root: String,
    pub target_verified_at_ms: i64,
    pub status: String,
    pub phase: String,
    pub identity_evidence: Vec<RootIdentityEvidence>,
    pub dry_run: RootRebindDryRun,
    pub backup: RootRebindBackupReference,
    #[serde(default)]
    pub affected_rows: Vec<RootRebindRowChange>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RootRebindTaskStatus {
    pub task_id: String,
    pub operation: String,
    pub state: String,
    pub submitted_at_ms: i64,
    pub started_at_ms: Option<i64>,
    pub finished_at_ms: Option<i64>,
    pub result: Option<Value>,
    pub error: Option<String>,
}

type RootRebindWork = Box<dyn FnOnce(Arc<AtomicBool>) -> Result<Value> + Send + 'static>;

struct RootRebindWorkRequest {
    task_id: String,
    work: RootRebindWork,
}

fn root_rebind_task_registry() -> &'static Mutex<HashMap<String, RootRebindTaskStatus>> {
    ROOT_REBIND_TASKS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn root_rebind_task_cancellations() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    ROOT_REBIND_TASK_CANCELLATIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn root_rebind_task_sender(recovery: bool) -> &'static SyncSender<RootRebindWorkRequest> {
    let queue = if recovery {
        &ROOT_REBIND_RECOVERY_TASK_QUEUE
    } else {
        &ROOT_REBIND_TASK_QUEUE
    };
    queue.get_or_init(|| {
        let (sender, receiver) = sync_channel::<RootRebindWorkRequest>(ROOT_REBIND_QUEUE_CAPACITY);
        let receiver = Arc::new(Mutex::new(receiver));
        let worker_count = if recovery {
            ROOT_REBIND_RECOVERY_WORKER_COUNT
        } else {
            ROOT_REBIND_WORKER_COUNT
        };
        for index in 0..worker_count {
            let receiver = Arc::clone(&receiver);
            let worker_name = if recovery {
                format!("root-rebind-recovery-{index}")
            } else {
                format!("root-rebind-{index}")
            };
            std::thread::Builder::new()
                .name(worker_name)
                .spawn(move || root_rebind_worker(receiver, recovery))
                .expect("root rebind fixed worker must start");
            if recovery {
                ROOT_REBIND_RECOVERY_WORKERS_STARTED.fetch_add(1, Ordering::Relaxed);
            } else {
                ROOT_REBIND_WORKERS_STARTED.fetch_add(1, Ordering::Relaxed);
            }
        }
        sender
    })
}

fn root_rebind_worker(receiver: Arc<Mutex<Receiver<RootRebindWorkRequest>>>, recovery: bool) {
    ROOT_REBIND_RECOVERY_IO_LANE.with(|lane| lane.set(recovery));
    loop {
        let request = {
            let locked = receiver
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            locked.recv()
        };
        let Ok(request) = request else {
            return;
        };
        {
            let mut tasks = root_rebind_task_registry()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some(status) = tasks.get_mut(&request.task_id) {
                status.state = "running".to_string();
                status.started_at_ms = Some(now_ms());
            }
        }
        let cancellation = root_rebind_task_cancellations()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&request.task_id)
            .cloned()
            .unwrap_or_else(|| Arc::new(AtomicBool::new(false)));
        let outcome = if cancellation.load(Ordering::Relaxed) {
            Ok(Err(invalid("root rebind task canceled before execution")))
        } else {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                (request.work)(cancellation)
            }))
        };
        let mut tasks = root_rebind_task_registry()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(status) = tasks.get_mut(&request.task_id) {
            status.finished_at_ms = Some(now_ms());
            match outcome {
                Ok(Ok(value)) => {
                    status.state = "completed".to_string();
                    status.result = Some(value);
                }
                Ok(Err(error)) => {
                    status.state = "failed".to_string();
                    status.error = Some(error.to_string());
                }
                Err(_) => {
                    status.state = "failed".to_string();
                    status.error = Some("root rebind worker panicked".to_string());
                }
            }
        }
        root_rebind_task_cancellations()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&request.task_id);
    }
}

/// Queue potentially blocking root identity, metadata, hashing, backup, or recovery work on a
/// fixed executor. The returned ticket is intentionally pending; callers poll the typed status
/// instead of tying the Tauri/startup thread to a slow or disconnected storage root.
pub fn submit_root_rebind_task<T, F>(operation: &str, work: F) -> Result<RootRebindTaskStatus>
where
    T: Serialize + Send + 'static,
    F: FnOnce() -> Result<T> + Send + 'static,
{
    submit_root_rebind_task_cancellable(operation, move |_| work())
}

pub fn submit_root_rebind_task_cancellable<T, F>(
    operation: &str,
    work: F,
) -> Result<RootRebindTaskStatus>
where
    T: Serialize + Send + 'static,
    F: FnOnce(Arc<AtomicBool>) -> Result<T> + Send + 'static,
{
    let task_id = format!("root-rebind-task-{}", Uuid::new_v4());
    let status = RootRebindTaskStatus {
        task_id: task_id.clone(),
        operation: operation.to_string(),
        state: "queued".to_string(),
        submitted_at_ms: now_ms(),
        started_at_ms: None,
        finished_at_ms: None,
        result: None,
        error: None,
    };
    {
        let mut tasks = root_rebind_task_registry()
            .lock()
            .map_err(|_| invalid("root rebind task registry lock is poisoned"))?;
        if tasks.len() >= ROOT_REBIND_TASK_RETENTION {
            let mut completed = tasks
                .values()
                .filter(|task| matches!(task.state.as_str(), "completed" | "failed"))
                .map(|task| {
                    (
                        task.finished_at_ms.unwrap_or(i64::MIN),
                        task.task_id.clone(),
                    )
                })
                .collect::<Vec<_>>();
            completed.sort();
            let remove_count = tasks.len() + 1 - ROOT_REBIND_TASK_RETENTION;
            for (_, id) in completed.into_iter().take(remove_count) {
                tasks.remove(&id);
            }
            if tasks.len() >= ROOT_REBIND_TASK_RETENTION {
                return Err(invalid("root rebind task registry is saturated"));
            }
        }
        tasks.insert(task_id.clone(), status.clone());
    }
    let cancellation = Arc::new(AtomicBool::new(false));
    root_rebind_task_cancellations()
        .lock()
        .map_err(|_| invalid("root rebind cancellation registry lock is poisoned"))?
        .insert(task_id.clone(), Arc::clone(&cancellation));
    let request = RootRebindWorkRequest {
        task_id: task_id.clone(),
        work: Box::new(move |cancellation| Ok(serde_json::to_value(work(cancellation)?)?)),
    };
    let recovery = matches!(
        operation,
        "apply" | "rollback" | "recover" | "reconcile" | "startup_recover"
    );
    match root_rebind_task_sender(recovery).try_send(request) {
        Ok(()) => Ok(status),
        Err(TrySendError::Full(_)) => {
            let mut tasks = root_rebind_task_registry()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            tasks.remove(&task_id);
            root_rebind_task_cancellations()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .remove(&task_id);
            Err(invalid("root rebind bounded work queue is saturated"))
        }
        Err(TrySendError::Disconnected(_)) => {
            let mut tasks = root_rebind_task_registry()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            tasks.remove(&task_id);
            root_rebind_task_cancellations()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .remove(&task_id);
            Err(invalid("root rebind bounded work queue is unavailable"))
        }
    }
}

pub fn cancel_root_rebind_task(task_id: &str) -> Result<RootRebindTaskStatus> {
    let cancellation = root_rebind_task_cancellations()
        .lock()
        .map_err(|_| invalid("root rebind cancellation registry lock is poisoned"))?
        .get(task_id)
        .cloned()
        .ok_or_else(|| invalid(format!("root rebind task is not cancellable: {task_id}")))?;
    cancellation.store(true, Ordering::Relaxed);
    root_rebind_task_status(task_id, None)
}

pub fn root_rebind_task_status(
    task_id: &str,
    _wait_timeout_ms: Option<u64>,
) -> Result<RootRebindTaskStatus> {
    root_rebind_task_registry()
        .lock()
        .map_err(|_| invalid("root rebind task registry lock is poisoned"))?
        .get(task_id)
        .cloned()
        .ok_or_else(|| invalid(format!("root rebind task not found: {task_id}")))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootRebindStopAfter {
    Database,
    FeatureConfig,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn invalid(message: impl Into<String>) -> EngineError {
    EngineError::InstallFailed(message.into())
}

type RootRebindIoWork = Box<dyn FnOnce() + Send + 'static>;

struct RootRebindIoRequest {
    work: RootRebindIoWork,
}

struct RootRebindIoExecutor {
    sender: SyncSender<RootRebindIoRequest>,
    workers_started: Arc<AtomicU64>,
}

impl RootRebindIoExecutor {
    fn new(worker_prefix: &str, worker_count: usize, queue_capacity: usize) -> Self {
        let (sender, receiver) = sync_channel::<RootRebindIoRequest>(queue_capacity);
        let receiver = Arc::new(Mutex::new(receiver));
        let workers_started = Arc::new(AtomicU64::new(0));
        for index in 0..worker_count {
            let receiver = Arc::clone(&receiver);
            std::thread::Builder::new()
                .name(format!("{worker_prefix}-{index}"))
                .spawn(move || {
                    loop {
                        let request = {
                            let locked = receiver
                                .lock()
                                .unwrap_or_else(|poisoned| poisoned.into_inner());
                            locked.recv()
                        };
                        let Ok(request) = request else {
                            return;
                        };
                        // A filesystem API may panic, time out at the caller, or remain blocked
                        // inside the OS. Keep that work on this fixed worker until it really exits;
                        // never replace it with a detached per-probe thread.
                        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            (request.work)()
                        }));
                    }
                })
                .expect("root rebind fixed I/O worker must start");
            workers_started.fetch_add(1, Ordering::Relaxed);
        }
        Self {
            sender,
            workers_started,
        }
    }

    fn workers_started(&self) -> u64 {
        self.workers_started.load(Ordering::Relaxed)
    }
}

fn root_rebind_io_executor(recovery: bool) -> &'static RootRebindIoExecutor {
    if recovery {
        ROOT_REBIND_RECOVERY_IO_EXECUTOR.get_or_init(|| {
            RootRebindIoExecutor::new(
                "root-rebind-recovery-io",
                ROOT_REBIND_RECOVERY_IO_WORKER_COUNT,
                ROOT_REBIND_IO_QUEUE_CAPACITY,
            )
        })
    } else {
        ROOT_REBIND_IO_EXECUTOR.get_or_init(|| {
            RootRebindIoExecutor::new(
                "root-rebind-io",
                ROOT_REBIND_IO_WORKER_COUNT,
                ROOT_REBIND_IO_QUEUE_CAPACITY,
            )
        })
    }
}

fn run_bounded_rebind_io_on<T, F>(
    executor: &RootRebindIoExecutor,
    label: &str,
    cancellation: &AtomicBool,
    timeout: Duration,
    work: F,
) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T> + Send + 'static,
{
    if cancellation.load(Ordering::Relaxed) {
        return Err(invalid(format!("root rebind canceled before {label}")));
    }
    let (result_sender, receiver) = sync_channel(1);
    let request = RootRebindIoRequest {
        work: Box::new(move || {
            let _ = result_sender.send(work());
        }),
    };
    match executor.sender.try_send(request) {
        Ok(()) => {}
        Err(TrySendError::Full(_)) => {
            return Err(invalid(format!(
                "root rebind bounded filesystem I/O capacity is saturated before {label}"
            )))
        }
        Err(TrySendError::Disconnected(_)) => {
            return Err(invalid(format!(
                "root rebind bounded filesystem I/O capacity is unavailable before {label}"
            )))
        }
    }
    let started = Instant::now();
    loop {
        if cancellation.load(Ordering::Relaxed) {
            return Err(invalid(format!("root rebind canceled during {label}")));
        }
        let remaining = timeout.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            return Err(invalid(format!("root rebind timed out during {label}")));
        }
        match receiver.recv_timeout(remaining.min(ROOT_REBIND_IO_POLL)) {
            Ok(result) => return result,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return Err(invalid(format!(
                    "root rebind bounded I/O worker disconnected during {label}"
                )))
            }
        }
    }
}

fn run_bounded_rebind_io<T, F>(
    label: &str,
    cancellation: &AtomicBool,
    timeout: Duration,
    work: F,
) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T> + Send + 'static,
{
    let recovery = ROOT_REBIND_RECOVERY_IO_LANE.with(Cell::get);
    run_bounded_rebind_io_on(
        root_rebind_io_executor(recovery),
        label,
        cancellation,
        timeout,
        work,
    )
}

fn root_components(raw: &str) -> Result<Vec<String>> {
    let mut normalized = raw.trim().replace('/', "\\");
    const DEVICE_UNC_PREFIX: &str = "\\\\?\\UNC\\";
    if normalized
        .get(..DEVICE_UNC_PREFIX.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(DEVICE_UNC_PREFIX))
    {
        normalized = format!("\\\\{}", &normalized[DEVICE_UNC_PREFIX.len()..]);
    } else if let Some(rest) = normalized.strip_prefix("\\\\?\\") {
        normalized = rest.to_string();
    }
    while normalized.len() > 3 && normalized.ends_with('\\') {
        normalized.pop();
    }
    let is_unc = normalized.starts_with("\\\\");
    let bytes = normalized.as_bytes();
    let is_drive =
        bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\';
    if !is_unc && !is_drive {
        return Err(invalid(format!("root path must be absolute: {raw}")));
    }
    let components = normalized
        .split('\\')
        .filter(|component| !component.is_empty() && *component != ".")
        .map(str::to_string)
        .collect::<Vec<_>>();
    if (is_unc && components.len() < 2)
        || components
            .iter()
            .any(|component| component.eq_ignore_ascii_case(".."))
        || components.is_empty()
    {
        return Err(invalid(format!("root path is not canonicalizable: {raw}")));
    }
    Ok(components)
}

fn canonical_root_components(raw: &str) -> Result<Vec<String>> {
    Ok(root_components(raw)?
        .into_iter()
        .map(|component| component.to_ascii_lowercase())
        .collect())
}

fn validate_rebind_root_relation(
    from_root: &str,
    to_root: &str,
    canonical_old_root: &Path,
    canonical_target: &Path,
) -> Result<bool> {
    if canonical_root_components(from_root)? == canonical_root_components(to_root)? {
        return Err(invalid(
            "root rebind source and target are the same normalized logical root",
        ));
    }
    Ok(canonical_old_root == canonical_target)
}

fn descendant_remainder(path: &str, root: &str) -> Result<Option<Vec<String>>> {
    let original_path = root_components(path)?;
    let path = original_path
        .iter()
        .map(|component| component.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let root = canonical_root_components(root)?;
    if path.len() < root.len() || path[..root.len()] != root {
        return Ok(None);
    }
    Ok(Some(original_path[root.len()..].to_vec()))
}

fn join_root(to_root: &str, remainder: &[String]) -> String {
    let separator = if to_root.contains('\\') { '\\' } else { '/' };
    let mut result = to_root.trim_end_matches(['\\', '/']).to_string();
    for component in remainder {
        result.push(separator);
        result.push_str(component);
    }
    result
}

pub fn load_root_aliases(paths: &AppPaths) -> Result<RootAliasesConfig> {
    let path = paths.root_aliases_config_path();
    if !path.exists() {
        return Ok(RootAliasesConfig::default());
    }
    let parsed: RootAliasesConfig = serde_json::from_slice(&std::fs::read(&path)?)?;
    validate_aliases(&parsed.aliases)?;
    Ok(parsed)
}

fn save_root_aliases(paths: &AppPaths, aliases: &RootAliasesConfig) -> Result<()> {
    validate_aliases(&aliases.aliases)?;
    let text = format!("{}\n", serde_json::to_string_pretty(aliases)?);
    persistence::atomic_write_text(&paths.root_aliases_config_path(), &text)?;
    Ok(())
}

fn update_root_aliases<F>(paths: &AppPaths, update: F) -> Result<RootAliasesConfig>
where
    F: FnOnce(&mut RootAliasesConfig) -> Result<()>,
{
    let _guard = ROOT_ALIAS_WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| invalid("root alias writer lock is poisoned"))?;
    let mut aliases = load_root_aliases(paths)?;
    update(&mut aliases)?;
    validate_aliases(&aliases.aliases)?;
    save_root_aliases(paths, &aliases)?;
    let persisted = load_root_aliases(paths)?;
    if persisted != aliases {
        return Err(invalid("root alias write readback did not match"));
    }
    Ok(persisted)
}

pub fn validate_aliases(aliases: &[RootAliasRecord]) -> Result<()> {
    if aliases
        .iter()
        .filter(|alias| alias.status == "active")
        .count()
        > MAX_ACTIVE_ROOT_ALIASES
    {
        return Err(invalid(format!(
            "active root aliases exceed bounded limit {MAX_ACTIVE_ROOT_ALIASES}"
        )));
    }
    let mut from_roots: Vec<Vec<String>> = Vec::new();
    let mut to_roots = HashSet::new();
    for alias in aliases.iter().filter(|alias| alias.status == "active") {
        let from = canonical_root_components(&alias.from_root)?;
        let to = canonical_root_components(&alias.to_root)?;
        if from == to {
            return Err(invalid("root alias source and target are identical"));
        }
        if !to_roots.insert(to.clone()) {
            return Err(invalid("multiple active aliases target the same root"));
        }
        for previous in &from_roots {
            let common = previous.len().min(from.len());
            if previous[..common] == from[..common] {
                return Err(invalid("active root aliases overlap ambiguously"));
            }
        }
        from_roots.push(from);
    }
    let edges = aliases
        .iter()
        .filter(|alias| alias.status == "active")
        .map(|alias| {
            Ok((
                canonical_root_components(&alias.from_root)?,
                canonical_root_components(&alias.to_root)?,
            ))
        })
        .collect::<Result<HashMap<_, _>>>()?;
    // Keep resolution one-hop and idempotent. Targets may not land inside any active source
    // (including their own source), and a source may not land inside a target. That rejects
    // exact and nested A->B->C chains rather than making every consumer implement transitive
    // resolution with a different stopping rule.
    for (from, to) in &edges {
        for other_from in edges.keys() {
            let common = to.len().min(other_from.len());
            if to[..common] == other_from[..common] {
                return Err(invalid(
                    "active root alias target forms a nested alias chain",
                ));
            }
        }
        let common = from.len().min(to.len());
        if from[..common] == to[..common] {
            return Err(invalid(
                "root alias source and target may not contain each other",
            ));
        }
    }
    for start in edges.keys() {
        let mut seen = HashSet::new();
        let mut cursor = start;
        while let Some(next) = edges.get(cursor) {
            if !seen.insert(cursor.clone()) {
                return Err(invalid("root alias cycle detected"));
            }
            cursor = next;
        }
    }
    Ok(())
}

pub fn resolve_alias_path(aliases: &[RootAliasRecord], input: &str) -> Result<Option<String>> {
    validate_aliases(aliases)?;
    let mut matched = Vec::new();
    for alias in aliases.iter().filter(|alias| alias.status == "active") {
        if let Some(remainder) = descendant_remainder(input, &alias.from_root)? {
            matched.push((alias, remainder));
        }
    }
    match matched.as_slice() {
        [] => Ok(None),
        [(alias, remainder)] => Ok(Some(join_root(&alias.to_root, remainder))),
        _ => Err(invalid("path matches multiple active root aliases")),
    }
}

/// Return the bounded historical stored identities that could resolve to a current physical
/// path. This is the inverse of one-hop alias resolution and lets indexed DB lookups avoid
/// opening/canonicalizing every historical library row.
pub fn historical_alias_candidates(
    aliases: &[RootAliasRecord],
    current_path: &str,
) -> Result<Vec<String>> {
    validate_aliases(aliases)?;
    let mut candidates = Vec::new();
    for alias in aliases.iter().filter(|alias| alias.status == "active") {
        if let Some(remainder) = descendant_remainder(current_path, &alias.to_root)? {
            candidates.push(join_root(&alias.from_root, &remainder));
        }
    }
    candidates.sort_by_key(|value| value.to_ascii_lowercase());
    candidates.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    Ok(candidates)
}

pub fn resolve_active_alias_path(
    paths: &AppPaths,
    input: &Path,
    require_available: bool,
) -> Result<PathBuf> {
    let input_text = input.to_string_lossy();
    let aliases = load_root_aliases(paths)?;
    let Some(mapped) = resolve_alias_path(&aliases.aliases, &input_text)? else {
        return Ok(input.to_path_buf());
    };
    let mapped = PathBuf::from(mapped);
    let matched_alias = aliases
        .aliases
        .iter()
        .filter(|alias| alias.status == "active")
        .find(|alias| {
            descendant_remainder(&input_text, &alias.from_root)
                .ok()
                .flatten()
                .is_some()
        })
        .ok_or_else(|| invalid("resolved root alias no longer has a unique active owner"))?;
    let target_root = PathBuf::from(&matched_alias.to_root);
    let target_available = alias_target_available(&target_root, require_available);
    if require_available {
        if !target_available
            || !crate::paths::path_is_dir_bounded(&mapped, ALIAS_TARGET_PROBE_TIMEOUT)
        {
            return Err(invalid(format!(
                "verified root alias target is currently unavailable: {}",
                mapped.to_string_lossy()
            )));
        }
        return Ok(mapped);
    }
    // A rebind alias is a reversible physical-location projection, not a destructive rewrite of
    // historical identity. If the direct/NAS target is disconnected, reads must fall back to the
    // stored old-root path so restoring that root immediately restores open/reveal/availability
    // and dedupe behavior. New writes take the fail-closed branch above and never fall back.
    if !target_available {
        return Ok(input.to_path_buf());
    }
    Ok(mapped)
}

fn table_exists(conn: &Connection, table: &str) -> Result<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1",
            [table],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

fn count_path_column(conn: &Connection, table: &str, column: &str, from: &str) -> Result<u64> {
    if !table_exists(conn, table)? {
        return Ok(0);
    }
    let sql = format!("SELECT {column} FROM {table} WHERE {column} IS NOT NULL");
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
    let mut count = 0;
    for row in rows {
        if descendant_remainder(&row?, from)?.is_some() {
            count += 1;
        }
    }
    Ok(count)
}

fn map_queued_destination_fields(value: &mut Value, from: &str, to: &str) -> Result<bool> {
    let Some(object) = value.as_object_mut() else {
        return Ok(false);
    };
    // Queued download parameter structs use `output_dir` as their sole absolute destination.
    // URLs, import `path`, source media, templates, titles, cookies and nested pipeline inputs
    // are intentionally never traversed or rewritten.
    let Some(output_dir) = object
        .get("output_dir")
        .and_then(Value::as_str)
        .map(str::to_string)
    else {
        return Ok(false);
    };
    let Some(remainder) = descendant_remainder(&output_dir, from)? else {
        return Ok(false);
    };
    object.insert(
        "output_dir".to_string(),
        Value::String(join_root(to, &remainder)),
    );
    Ok(true)
}

fn count_queued_destination_matches(conn: &Connection, from: &str) -> Result<u64> {
    if !table_exists(conn, "job")? {
        return Ok(0);
    }
    let mut statement =
        conn.prepare("SELECT params_json FROM job WHERE status='queued' ORDER BY created_at_ms")?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
    let mut count = 0;
    for row in rows {
        let mut value: Value = serde_json::from_str(&row?)?;
        if map_queued_destination_fields(&mut value, from, from)? {
            count += 1;
        }
    }
    Ok(count)
}

const MAX_REBIND_IN_FLIGHT_JOB_IDS: usize = 256;

fn affected_running_destination_job_ids(
    conn: &Connection,
    from: &str,
    to: Option<&str>,
) -> Result<Vec<String>> {
    if !table_exists(conn, "job")? {
        return Ok(Vec::new());
    }
    let mut statement = conn.prepare(
        "SELECT id, type, params_json FROM job \
         WHERE status='running' ORDER BY created_at_ms,id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    let mut affected = Vec::new();
    for row in rows {
        let (job_id, job_type, params_json) = row?;
        let value: Value = serde_json::from_str(&params_json).map_err(|_| {
            invalid(format!(
                "root rebind cannot prove the destination of running job {job_id}"
            ))
        })?;
        let output_dir = value
            .as_object()
            .and_then(|object| object.get("output_dir"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let matches_rebind = match output_dir {
            // These download runners resolve a missing explicit destination through mutable
            // configured roots. Other job families without output_dir are unrelated; those
            // with an explicit output_dir are checked below regardless of type.
            None => matches!(
                job_type.as_str(),
                "download_direct_url" | "youtube_subscription_refresh_v1"
            ),
            Some(output_dir) => {
                descendant_remainder(output_dir, from)?.is_some()
                    || to
                        .map(|target| descendant_remainder(output_dir, target))
                        .transpose()?
                        .flatten()
                        .is_some()
            }
        };
        if matches_rebind {
            affected.push(job_id);
            if affected.len() > MAX_REBIND_IN_FLIGHT_JOB_IDS {
                return Err(invalid(format!(
                    "root rebind affected running-job evidence exceeds bounded limit {MAX_REBIND_IN_FLIGHT_JOB_IDS}"
                )));
            }
        }
    }
    Ok(affected)
}

fn refuse_affected_running_jobs(
    conn: &Connection,
    from: &str,
    to: Option<&str>,
    operation: &str,
) -> Result<Vec<String>> {
    let affected = affected_running_destination_job_ids(conn, from, to)?;
    if !affected.is_empty() {
        return Err(invalid(format!(
            "root rebind {operation} refused while affected download jobs are running: {}. Pause the queue, wait for these jobs to finish or cancel them, then retry",
            affected.join(",")
        )));
    }
    Ok(affected)
}

pub fn root_rebind_dry_run(paths: &AppPaths, from_root: &str) -> Result<RootRebindDryRun> {
    canonical_root_components(from_root)?;
    let feature = config::load_feature_storage_roots_config(paths)?;
    let feature_root_matches = feature
        .video_root
        .as_deref()
        .map(|path| descendant_remainder(path, from_root))
        .transpose()?
        .flatten()
        .is_some() as u64;
    let conn = db::open_readonly(paths)?;
    let running_destination_job_ids = affected_running_destination_job_ids(&conn, from_root, None)?;
    Ok(RootRebindDryRun {
        feature_root_matches,
        video_library_root_matches: count_path_column(
            &conn,
            "video_library",
            "root_path",
            from_root,
        )?,
        youtube_subscription_override_matches: count_path_column(
            &conn,
            "youtube_subscription",
            "output_dir_override",
            from_root,
        )?,
        instagram_subscription_override_matches: count_path_column(
            &conn,
            "instagram_subscription",
            "output_dir_override",
            from_root,
        )?,
        queued_destination_matches: count_queued_destination_matches(&conn, from_root)?,
        running_destination_matches: running_destination_job_ids.len() as u64,
        running_destination_job_ids,
        historical_library_path_matches: count_path_column(
            &conn,
            "library_item",
            "media_path",
            from_root,
        )?,
    })
}

fn verify_identity_evidence(
    target: &Path,
    evidence: &[RootIdentityEvidence],
    cancellation: &AtomicBool,
) -> Result<Vec<RootIdentityEvidence>> {
    if evidence.is_empty() || evidence.len() > MAX_IDENTITY_EVIDENCE {
        return Err(invalid(format!(
            "root identity evidence must contain 1..={MAX_IDENTITY_EVIDENCE} entries"
        )));
    }
    let target_owned = target.to_path_buf();
    let canonical_target = run_bounded_rebind_io(
        "target root canonicalization",
        cancellation,
        ROOT_REBIND_METADATA_IO_TIMEOUT,
        move || {
            std::fs::canonicalize(&target_owned).map_err(|error| {
                invalid(format!(
                    "root identity target is not canonicalizable at {}: {error}",
                    target_owned.to_string_lossy()
                ))
            })
        },
    )?;
    let mut seen = HashSet::new();
    let mut verified = Vec::new();
    for expected in evidence {
        let relative = Path::new(&expected.relative_path);
        if relative.is_absolute()
            || relative.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err(invalid(
                "root identity evidence path must be safe and relative",
            ));
        }
        let identity_key = expected
            .relative_path
            .replace('/', "\\")
            .trim_start_matches(".\\")
            .to_ascii_lowercase();
        if !seen.insert(identity_key) {
            return Err(invalid(
                "root identity evidence contains a repeated canonical path",
            ));
        }
        let candidate = target.join(relative);
        let candidate_for_io = candidate.clone();
        let (canonical_candidate, metadata, observed_sample_sha256) = run_bounded_rebind_io(
            "target identity evidence probe",
            cancellation,
            ROOT_REBIND_IDENTITY_IO_TIMEOUT,
            move || {
                let canonical_candidate =
                    std::fs::canonicalize(&candidate_for_io).map_err(|error| {
                        invalid(format!(
                            "root identity evidence is missing at {}: {error}",
                            candidate_for_io.to_string_lossy()
                        ))
                    })?;
                let metadata = std::fs::metadata(&canonical_candidate)?;
                let hash = sampled_content_sha256(&canonical_candidate)?;
                Ok((canonical_candidate, metadata, hash))
            },
        )?;
        if !canonical_candidate.starts_with(&canonical_target) {
            return Err(invalid(
                "root identity evidence resolves outside the target root",
            ));
        }
        if !metadata.is_file() {
            return Err(invalid("root identity evidence must identify a file"));
        }
        if expected
            .expected_bytes
            .is_some_and(|bytes| bytes != metadata.len())
        {
            return Err(invalid(format!(
                "root identity evidence size mismatch for {}",
                expected.relative_path
            )));
        }
        let expected_sample_sha256 = expected
            .expected_sample_sha256
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                invalid(format!(
                    "root identity evidence requires a sampled content hash for {}",
                    expected.relative_path
                ))
            })?;
        if !expected_sample_sha256.eq_ignore_ascii_case(&observed_sample_sha256) {
            return Err(invalid(format!(
                "root identity evidence sampled content mismatch for {}",
                expected.relative_path
            )));
        }
        verified.push(RootIdentityEvidence {
            relative_path: expected.relative_path.clone(),
            // Pin the observed size even when the caller omitted it. Apply/resume must prove the
            // same target evidence rather than accepting any same-named replacement file.
            expected_bytes: Some(expected.expected_bytes.unwrap_or(metadata.len())),
            observed_bytes: metadata.len(),
            expected_sample_sha256: Some(observed_sample_sha256.clone()),
            observed_sample_sha256,
        });
    }
    Ok(verified)
}

fn canonical_identity_evidence_paths(
    paths: &AppPaths,
    old_root: &Path,
    cancellation: &AtomicBool,
) -> Result<(Vec<(String, PathBuf)>, usize)> {
    let old_root_owned = old_root.to_path_buf();
    let canonical_old_root = run_bounded_rebind_io(
        "old root canonicalization",
        cancellation,
        ROOT_REBIND_METADATA_IO_TIMEOUT,
        move || {
            std::fs::canonicalize(&old_root_owned).map_err(|error| {
                invalid(format!(
                    "root rebind requires readable canonical old-root identity evidence at {}: {error}",
                    old_root_owned.to_string_lossy()
                ))
            })
        },
    )?;
    let conn = db::open_readonly(paths)?;
    if !table_exists(&conn, "library_item")? {
        return Err(invalid(
            "root rebind has no canonical library metadata from which to select identity evidence",
        ));
    }
    let mut statement = conn.prepare(
        "SELECT media_path FROM library_item WHERE media_path IS NOT NULL AND trim(media_path)<>'' ORDER BY lower(media_path),media_path",
    )?;
    let stored_paths = statement.query_map([], |row| row.get::<_, String>(0))?;
    let old_root_text = old_root.to_string_lossy();
    let mut seen = HashSet::new();
    let mut candidate_paths = Vec::new();
    for stored_path in stored_paths {
        if cancellation.load(Ordering::Relaxed) {
            return Err(invalid(
                "root rebind canceled while selecting identity evidence",
            ));
        }
        let stored_path = stored_path?;
        let Some(remainder) = descendant_remainder(&stored_path, &old_root_text)? else {
            continue;
        };
        if remainder.is_empty() {
            continue;
        }
        let mut relative = PathBuf::new();
        for component in remainder {
            relative.push(component);
        }
        let relative_text = relative.to_string_lossy().to_string();
        let identity_key = relative_text.replace('/', "\\").to_ascii_lowercase();
        if seen.insert(identity_key) {
            candidate_paths.push((relative_text, old_root.join(&relative)));
            if candidate_paths.len() >= MAX_CANONICAL_IDENTITY_CANDIDATE_PROBES {
                break;
            }
        }
    }
    let mut candidates = Vec::new();
    let candidate_probe_count = candidate_paths.len();
    for (relative_text, source) in candidate_paths {
        let source_for_io = source.clone();
        let canonical_old_root_for_io = canonical_old_root.clone();
        let canonical_source = run_bounded_rebind_io(
            "old root identity evidence probe",
            cancellation,
            ROOT_REBIND_IDENTITY_IO_TIMEOUT,
            move || {
                let Ok(canonical_source) = std::fs::canonicalize(&source_for_io) else {
                    return Ok(None);
                };
                let Ok(metadata) = std::fs::metadata(&canonical_source) else {
                    return Ok(None);
                };
                if !metadata.is_file() || !canonical_source.starts_with(&canonical_old_root_for_io)
                {
                    return Ok(None);
                }
                Ok(Some(canonical_source))
            },
        )?;
        if let Some(canonical_source) = canonical_source {
            candidates.push((relative_text, canonical_source));
        }
    }
    candidates.sort_by(|left, right| {
        left.0
            .to_ascii_lowercase()
            .cmp(&right.0.to_ascii_lowercase())
            .then_with(|| left.0.cmp(&right.0))
    });
    if candidates.is_empty() {
        return Err(invalid(
            "root rebind has no readable old-root files backed by canonical library metadata",
        ));
    }

    let indexes = if candidates.len() <= CANONICAL_IDENTITY_SAMPLE_COUNT {
        (0..candidates.len()).collect::<Vec<_>>()
    } else {
        vec![0, candidates.len() / 2, candidates.len() - 1]
    };
    Ok((
        indexes
            .into_iter()
            .map(|index| candidates[index].clone())
            .collect(),
        candidate_probe_count,
    ))
}

fn derive_trusted_identity_evidence(
    paths: &AppPaths,
    old_root: &Path,
    target: &Path,
    requested: &[RootIdentityEvidence],
    cancellation: &AtomicBool,
) -> Result<Vec<RootIdentityEvidence>> {
    if requested.len() > MAX_IDENTITY_EVIDENCE {
        return Err(invalid(format!(
            "root identity evidence must contain at most {MAX_IDENTITY_EVIDENCE} canonical entries"
        )));
    }
    let (selected, _candidate_probe_count) =
        canonical_identity_evidence_paths(paths, old_root, cancellation)?;
    let selected_keys = selected
        .iter()
        .map(|(relative, _)| relative.replace('/', "\\").to_ascii_lowercase())
        .collect::<HashSet<_>>();
    if !requested.is_empty() {
        let mut requested_keys = HashSet::new();
        for request in requested {
            let relative = Path::new(&request.relative_path);
            if relative.is_absolute()
                || relative.components().any(|component| {
                    matches!(
                        component,
                        Component::ParentDir | Component::RootDir | Component::Prefix(_)
                    )
                })
            {
                return Err(invalid(
                    "root identity evidence path must be safe and relative",
                ));
            }
            let key = request
                .relative_path
                .replace('/', "\\")
                .trim_start_matches(".\\")
                .to_ascii_lowercase();
            if !requested_keys.insert(key) {
                return Err(invalid(
                    "root identity evidence contains a repeated canonical path",
                ));
            }
        }
        if requested_keys != selected_keys {
            return Err(invalid(format!(
                "root identity evidence must exactly cover the {} deterministic canonical library sample(s)",
                selected.len()
            )));
        }
    }

    let mut trusted = Vec::with_capacity(selected.len());
    for (relative_path, canonical_file) in selected {
        let file_for_io = canonical_file.clone();
        let (bytes, sample_hash) = run_bounded_rebind_io(
            "old root identity evidence hashing",
            cancellation,
            ROOT_REBIND_IDENTITY_IO_TIMEOUT,
            move || {
                let metadata = std::fs::metadata(&file_for_io)?;
                Ok((metadata.len(), sampled_content_sha256(&file_for_io)?))
            },
        )?;
        trusted.push(RootIdentityEvidence {
            relative_path,
            expected_bytes: Some(bytes),
            observed_bytes: 0,
            expected_sample_sha256: Some(sample_hash),
            observed_sample_sha256: String::new(),
        });
    }
    verify_identity_evidence(target, &trusted, cancellation)
}

fn sampled_content_sha256(path: &Path) -> Result<String> {
    const SAMPLE_BYTES: u64 = 64 * 1024;
    let metadata = std::fs::metadata(path)?;
    let bytes = metadata.len();
    let mut offsets = vec![0_u64];
    if bytes > SAMPLE_BYTES {
        offsets.push(bytes.saturating_sub(SAMPLE_BYTES) / 2);
        offsets.push(bytes.saturating_sub(SAMPLE_BYTES));
    }
    offsets.sort_unstable();
    offsets.dedup();
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    hasher.update(bytes.to_le_bytes());
    for offset in offsets {
        file.seek(SeekFrom::Start(offset))?;
        let mut sample = vec![0_u8; SAMPLE_BYTES.min(bytes.saturating_sub(offset)) as usize];
        file.read_exact(&mut sample)?;
        hasher.update(offset.to_le_bytes());
        hasher.update(sample);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn write_receipt(paths: &AppPaths, receipt: &RootRebindReceipt) -> Result<PathBuf> {
    let path = validated_receipt_path(paths, &receipt.id)?;
    let text = format!("{}\n", serde_json::to_string_pretty(receipt)?);
    persistence::atomic_write_text(&path, &text)?;
    Ok(path)
}

pub fn root_rebind_receipt_status(paths: &AppPaths, id: &str) -> Result<RootRebindReceipt> {
    let path = validated_receipt_path(paths, id)?;
    let receipt: RootRebindReceipt = serde_json::from_slice(&std::fs::read(&path)?)?;
    if receipt.id != id {
        return Err(invalid(format!(
            "root rebind receipt content id does not match requested id: {}",
            path.to_string_lossy()
        )));
    }
    Ok(receipt)
}

fn validated_receipt_path(paths: &AppPaths, id: &str) -> Result<PathBuf> {
    let uuid = id
        .strip_prefix("root-rebind-")
        .and_then(|value| Uuid::parse_str(value).ok())
        .filter(|value| format!("root-rebind-{value}") == id)
        .ok_or_else(|| invalid("root rebind receipt id must be root-rebind-<canonical UUID>"))?;
    let dir = paths.root_rebind_receipts_dir();
    let path = dir.join(format!("root-rebind-{uuid}.json"));
    if path.parent() != Some(dir.as_path()) {
        return Err(invalid(
            "root rebind receipt path escaped its owned directory",
        ));
    }
    Ok(path)
}

pub fn list_root_rebind_receipts(paths: &AppPaths) -> Result<Vec<RootRebindReceipt>> {
    let dir = paths.root_rebind_receipts_dir();
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut receipts = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let path = entry?.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let id = path
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or_else(|| invalid("root rebind receipt filename was not valid UTF-8"))?;
        receipts.push(root_rebind_receipt_status(paths, id)?);
    }
    receipts.sort_by(|left, right| {
        right
            .updated_at_ms
            .cmp(&left.updated_at_ms)
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(receipts)
}

fn create_verified_backup(paths: &AppPaths, id: &str) -> Result<RootRebindBackupReference> {
    let backup_dir = paths.root_rebind_backups_dir().join(id);
    std::fs::create_dir_all(&backup_dir)?;
    let sqlite_path = backup_dir.join("app.sqlite");
    if sqlite_path.exists() {
        std::fs::remove_file(&sqlite_path)?;
    }
    let source = db::open_readonly(paths)?;
    let mut destination = Connection::open(&sqlite_path)?;
    {
        let backup = rusqlite::backup::Backup::new(&source, &mut destination)?;
        backup.run_to_completion(256, Duration::from_millis(10), None)?;
    }
    drop(destination);
    let reopened =
        Connection::open_with_flags(&sqlite_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let integrity: String = reopened.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if integrity != "ok" {
        return Err(invalid(format!(
            "SQLite backup integrity failed: {integrity}"
        )));
    }

    let source_config = paths.feature_storage_roots_config_path();
    let (config_path, config_verified) = if source_config.is_file() {
        let destination = backup_dir.join("feature_storage_roots.json");
        std::fs::copy(&source_config, &destination)?;
        let _: FeatureStorageRootsConfig = serde_json::from_slice(&std::fs::read(&destination)?)?;
        (Some(destination.to_string_lossy().to_string()), true)
    } else {
        (None, true)
    };

    // Alias activation is itself one of the rebind side effects, so its prior state needs the
    // same independently reopened proof as SQLite/config. Back up a canonical empty config when
    // the source did not yet exist; source_existed preserves the distinction for recovery tools.
    let source_aliases = paths.root_aliases_config_path();
    let aliases_source_existed = source_aliases.is_file();
    let aliases_bytes = if aliases_source_existed {
        let bytes = std::fs::read(&source_aliases)?;
        let parsed: RootAliasesConfig = serde_json::from_slice(&bytes)?;
        validate_aliases(&parsed.aliases)?;
        bytes
    } else {
        format!(
            "{}\n",
            serde_json::to_string_pretty(&RootAliasesConfig::default())?
        )
        .into_bytes()
    };
    let aliases_source_sha256 = hex::encode(Sha256::digest(&aliases_bytes));
    let aliases_path = backup_dir.join("root_aliases.json");
    persistence::atomic_write_text(
        &aliases_path,
        std::str::from_utf8(&aliases_bytes)
            .map_err(|_| invalid("root aliases config is not valid UTF-8"))?,
    )?;
    let reopened_aliases_bytes = std::fs::read(&aliases_path)?;
    let reopened_aliases: RootAliasesConfig = serde_json::from_slice(&reopened_aliases_bytes)?;
    validate_aliases(&reopened_aliases.aliases)?;
    let reopened_aliases_sha256 = hex::encode(Sha256::digest(&reopened_aliases_bytes));
    if reopened_aliases_sha256 != aliases_source_sha256 {
        return Err(invalid("root aliases backup hash verification failed"));
    }
    Ok(RootRebindBackupReference {
        sqlite_path: sqlite_path.to_string_lossy().to_string(),
        sqlite_integrity: integrity,
        feature_config_path: config_path,
        feature_config_verified: config_verified,
        aliases_config_path: aliases_path.to_string_lossy().to_string(),
        aliases_config_sha256: aliases_source_sha256,
        aliases_config_source_existed: aliases_source_existed,
        aliases_config_verified: true,
    })
}

fn verify_aliases_backup(backup: &RootRebindBackupReference) -> Result<()> {
    if !backup.aliases_config_verified
        || backup.aliases_config_path.trim().is_empty()
        || backup.aliases_config_sha256.trim().is_empty()
    {
        return Err(invalid("root aliases backup is not verified"));
    }
    let bytes = std::fs::read(&backup.aliases_config_path)?;
    let parsed: RootAliasesConfig = serde_json::from_slice(&bytes)?;
    validate_aliases(&parsed.aliases)?;
    let observed = hex::encode(Sha256::digest(&bytes));
    if observed != backup.aliases_config_sha256 {
        return Err(invalid("root aliases backup hash changed after prepare"));
    }
    Ok(())
}

fn collect_path_changes(
    conn: &Connection,
    surface: &str,
    table: &str,
    id_column: &str,
    path_column: &str,
    from: &str,
    to: &str,
) -> Result<Vec<RootRebindRowChange>> {
    if !table_exists(conn, table)? {
        return Ok(Vec::new());
    }
    let query =
        format!("SELECT {id_column}, {path_column} FROM {table} WHERE {path_column} IS NOT NULL");
    let mut statement = conn.prepare(&query)?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut changes = Vec::new();
    for (row_id, original_value) in rows {
        if let Some(remainder) = descendant_remainder(&original_value, from)? {
            changes.push(RootRebindRowChange {
                surface: surface.to_string(),
                row_id,
                field: path_column.to_string(),
                rebound_value: join_root(to, &remainder),
                original_value,
            });
        }
    }
    Ok(changes)
}

fn collect_queued_destination_changes(
    conn: &Connection,
    from: &str,
    to: &str,
) -> Result<Vec<RootRebindRowChange>> {
    if !table_exists(conn, "job")? {
        return Ok(Vec::new());
    }
    let mut statement = conn
        .prepare("SELECT id, params_json FROM job WHERE status='queued' ORDER BY created_at_ms")?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut changes = Vec::new();
    for (row_id, original_value) in rows {
        let mut value: Value = serde_json::from_str(&original_value)?;
        if map_queued_destination_fields(&mut value, from, to)? {
            changes.push(RootRebindRowChange {
                surface: "job".to_string(),
                row_id,
                field: "params_json".to_string(),
                original_value,
                rebound_value: serde_json::to_string(&value)?,
            });
        }
    }
    Ok(changes)
}

fn collect_database_rebind_changes(
    conn: &Connection,
    from: &str,
    to: &str,
) -> Result<Vec<RootRebindRowChange>> {
    let mut changes = Vec::new();
    changes.extend(collect_path_changes(
        conn,
        "video_library",
        "video_library",
        "id",
        "root_path",
        from,
        to,
    )?);
    changes.extend(collect_path_changes(
        conn,
        "youtube_subscription",
        "youtube_subscription",
        "id",
        "output_dir_override",
        from,
        to,
    )?);
    changes.extend(collect_path_changes(
        conn,
        "instagram_subscription",
        "instagram_subscription",
        "id",
        "output_dir_override",
        from,
        to,
    )?);
    changes.extend(collect_queued_destination_changes(conn, from, to)?);
    changes.sort_by(|left, right| {
        (&left.surface, &left.row_id, &left.field).cmp(&(
            &right.surface,
            &right.row_id,
            &right.field,
        ))
    });
    Ok(changes)
}

fn reject_unrecorded_database_matches(
    conn: &Connection,
    receipt: &RootRebindReceipt,
) -> Result<()> {
    let recorded = receipt
        .affected_rows
        .iter()
        .filter(|change| change.surface != "feature_storage_roots")
        .map(|change| {
            (
                (
                    change.surface.as_str(),
                    change.row_id.as_str(),
                    change.field.as_str(),
                ),
                change,
            )
        })
        .collect::<HashMap<_, _>>();
    for current in collect_database_rebind_changes(conn, &receipt.from_root, &receipt.to_root)? {
        let key = (
            current.surface.as_str(),
            current.row_id.as_str(),
            current.field.as_str(),
        );
        let Some(expected) = recorded.get(&key) else {
            return Err(invalid(format!(
                "root rebind prepared snapshot is stale; new matching row requires a new prepare: {}:{}",
                current.surface, current.row_id
            )));
        };
        if expected.original_value != current.original_value
            || expected.rebound_value != current.rebound_value
        {
            return Err(invalid(format!(
                "root rebind prepared snapshot no longer matches row {}:{}",
                current.surface, current.row_id
            )));
        }
    }
    Ok(())
}

fn collect_rebind_changes(
    paths: &AppPaths,
    from: &str,
    to: &str,
) -> Result<Vec<RootRebindRowChange>> {
    let conn = db::open_readonly(paths)?;
    let mut changes = collect_database_rebind_changes(&conn, from, to)?;
    let feature = config::load_feature_storage_roots_config(paths)?;
    if let Some(original_value) = feature.video_root {
        if let Some(remainder) = descendant_remainder(&original_value, from)? {
            changes.push(RootRebindRowChange {
                surface: "feature_storage_roots".to_string(),
                row_id: "video_root".to_string(),
                field: "video_root".to_string(),
                rebound_value: join_root(to, &remainder),
                original_value,
            });
        }
    }
    changes.sort_by(|left, right| {
        (&left.surface, &left.row_id, &left.field).cmp(&(
            &right.surface,
            &right.row_id,
            &right.field,
        ))
    });
    Ok(changes)
}

pub fn prepare_root_rebind(
    paths: &AppPaths,
    from_root: &str,
    to_root: &Path,
    evidence: &[RootIdentityEvidence],
) -> Result<RootRebindReceipt> {
    let cancellation = AtomicBool::new(false);
    prepare_root_rebind_cancellable(paths, from_root, to_root, evidence, &cancellation)
}

pub fn prepare_root_rebind_cancellable(
    paths: &AppPaths,
    from_root: &str,
    to_root: &Path,
    evidence: &[RootIdentityEvidence],
    cancellation: &AtomicBool,
) -> Result<RootRebindReceipt> {
    canonical_root_components(from_root)?;
    let to_root_text = to_root.to_string_lossy().to_string();
    canonical_root_components(&to_root_text)?;
    {
        let conn = db::open_readonly(paths)?;
        refuse_affected_running_jobs(&conn, from_root, Some(&to_root_text), "prepare")?;
    }
    let mut proposed_aliases = load_root_aliases(paths)?.aliases;
    proposed_aliases.push(RootAliasRecord {
        id: "prepare-validation".to_string(),
        from_root: from_root.to_string(),
        to_root: to_root.to_string_lossy().to_string(),
        verified_at_ms: 0,
        status: "active".to_string(),
        receipt_path: String::new(),
    });
    validate_aliases(&proposed_aliases)?;
    let target_for_probe = to_root.to_path_buf();
    let canonical_target = run_bounded_rebind_io(
        "target root availability probe",
        cancellation,
        ROOT_REBIND_METADATA_IO_TIMEOUT,
        move || {
            if !target_for_probe.is_dir() {
                return Err(invalid(format!(
                    "root rebind target is unavailable: {}",
                    target_for_probe.to_string_lossy()
                )));
            }
            Ok(std::fs::canonicalize(&target_for_probe)?)
        },
    )?;
    if cancellation.load(Ordering::Relaxed) {
        return Err(invalid(format!(
            "root rebind canceled before identity comparison: {}",
            to_root.to_string_lossy(),
        )));
    }
    let old_root_for_probe = PathBuf::from(from_root);
    let canonical_old_root = run_bounded_rebind_io(
        "old root availability probe",
        cancellation,
        ROOT_REBIND_METADATA_IO_TIMEOUT,
        move || {
            std::fs::canonicalize(&old_root_for_probe).map_err(|error| {
                invalid(format!(
                    "root rebind old root is unavailable at {}: {error}",
                    old_root_for_probe.to_string_lossy()
                ))
            })
        },
    )?;
    // Distinct logical roots are intentionally allowed to resolve to the same physical tree.
    // This is the safe direct-attach/mapped-drive case: the identity samples below still prove
    // the exact media tree, while the rebind updates the durable logical destination spelling.
    let _same_physical_tree = validate_rebind_root_relation(
        from_root,
        &to_root.to_string_lossy(),
        &canonical_old_root,
        &canonical_target,
    )?;
    let verified_evidence = derive_trusted_identity_evidence(
        paths,
        Path::new(from_root),
        to_root,
        evidence,
        cancellation,
    )?;
    let id = format!("root-rebind-{}", Uuid::new_v4());
    let affected_rows = collect_rebind_changes(paths, from_root, &to_root_text)?;
    let backup = create_verified_backup(paths, &id)?;
    let dry_run = root_rebind_dry_run(paths, from_root)?;
    let now = now_ms();
    let receipt = RootRebindReceipt {
        schema_version: ROOT_REBIND_RECEIPT_SCHEMA_VERSION,
        id,
        from_root: from_root.to_string(),
        to_root: to_root_text,
        target_verified_at_ms: now,
        status: "prepared".to_string(),
        phase: "backups_verified".to_string(),
        identity_evidence: verified_evidence,
        dry_run,
        backup,
        affected_rows,
        created_at_ms: now,
        updated_at_ms: now,
    };
    if let Err(error) = write_receipt(paths, &receipt) {
        let backup_dir = paths.root_rebind_backups_dir().join(&receipt.id);
        return match std::fs::remove_dir_all(&backup_dir) {
            Ok(()) => Err(error),
            Err(cleanup_error) => Err(invalid(format!(
                "root rebind receipt persistence failed ({error}); orphan backup cleanup also failed at {} ({cleanup_error})",
                backup_dir.to_string_lossy()
            ))),
        };
    }
    Ok(receipt)
}

fn apply_recorded_database_changes(
    tx: &rusqlite::Transaction<'_>,
    changes: &[RootRebindRowChange],
    forward: bool,
) -> Result<()> {
    for change in changes
        .iter()
        .filter(|change| change.surface != "feature_storage_roots")
    {
        let (table, id_column, field) = match (change.surface.as_str(), change.field.as_str()) {
            ("video_library", "root_path") => ("video_library", "id", "root_path"),
            ("youtube_subscription", "output_dir_override") => {
                ("youtube_subscription", "id", "output_dir_override")
            }
            ("instagram_subscription", "output_dir_override") => {
                ("instagram_subscription", "id", "output_dir_override")
            }
            ("job", "params_json") => ("job", "id", "params_json"),
            _ => {
                return Err(invalid(
                    "root rebind receipt contains an unsupported database surface",
                ))
            }
        };
        let (expected, replacement) = if forward {
            (&change.original_value, &change.rebound_value)
        } else {
            (&change.rebound_value, &change.original_value)
        };
        let query = format!("SELECT {field} FROM {table} WHERE {id_column}=?1");
        let current = tx
            .query_row(&query, [&change.row_id], |row| row.get::<_, String>(0))
            .optional()?;
        match current.as_deref() {
            Some(value) if value == replacement => continue,
            Some(value) if value == expected => {
                let update =
                    format!("UPDATE {table} SET {field}=?1 WHERE {id_column}=?2 AND {field}=?3");
                if tx.execute(&update, params![replacement, change.row_id, expected])? != 1 {
                    return Err(invalid(format!(
                        "root rebind row changed concurrently: {}:{}",
                        change.surface, change.row_id
                    )));
                }
            }
            Some(_) => {
                return Err(invalid(format!(
                    "root rebind refused to overwrite a changed row: {}:{}",
                    change.surface, change.row_id
                )))
            }
            None => {
                return Err(invalid(format!(
                    "root rebind receipt row is missing: {}:{}",
                    change.surface, change.row_id
                )))
            }
        }
    }
    Ok(())
}

fn apply_recorded_feature_change(
    paths: &AppPaths,
    changes: &[RootRebindRowChange],
    forward: bool,
) -> Result<()> {
    let Some(change) = changes
        .iter()
        .find(|change| change.surface == "feature_storage_roots" && change.field == "video_root")
    else {
        return Ok(());
    };
    let (expected, replacement) = if forward {
        (&change.original_value, &change.rebound_value)
    } else {
        (&change.rebound_value, &change.original_value)
    };
    config::compare_exchange_feature_storage_video_root(paths, expected, replacement)?;
    Ok(())
}

fn reject_unrecorded_feature_match(paths: &AppPaths, receipt: &RootRebindReceipt) -> Result<()> {
    let current = config::load_feature_storage_roots_config(paths)?;
    reject_unrecorded_feature_match_config(&current, receipt)
}

fn reject_unrecorded_feature_match_config(
    current_config: &FeatureStorageRootsConfig,
    receipt: &RootRebindReceipt,
) -> Result<()> {
    let current = current_config.video_root.as_deref();
    let matching_current = current
        .map(|value| descendant_remainder(value, &receipt.from_root))
        .transpose()?
        .flatten();
    let recorded = receipt
        .affected_rows
        .iter()
        .find(|change| change.surface == "feature_storage_roots" && change.field == "video_root");
    match recorded {
        None if matching_current.is_some() => Err(invalid(
            "root rebind prepared snapshot is stale; feature video_root now matches the old root",
        )),
        None => Ok(()),
        Some(change) if current == Some(change.rebound_value.as_str()) => Ok(()),
        Some(change) if current == Some(change.original_value.as_str()) => {
            let remainder = matching_current.ok_or_else(|| {
                invalid("recorded feature video_root no longer belongs to the old root")
            })?;
            if change.rebound_value == join_root(&receipt.to_root, &remainder) {
                Ok(())
            } else {
                Err(invalid(
                    "root rebind prepared snapshot no longer maps feature video_root",
                ))
            }
        }
        Some(_) => Err(invalid(
            "root rebind prepared snapshot no longer matches feature video_root",
        )),
    }
}

fn verify_recorded_database_state(
    paths: &AppPaths,
    changes: &[RootRebindRowChange],
    forward: bool,
) -> Result<()> {
    let conn = db::open_readonly(paths)?;
    for change in changes
        .iter()
        .filter(|change| change.surface != "feature_storage_roots")
    {
        let (table, id_column, field) = match (change.surface.as_str(), change.field.as_str()) {
            ("video_library", "root_path") => ("video_library", "id", "root_path"),
            ("youtube_subscription", "output_dir_override") => {
                ("youtube_subscription", "id", "output_dir_override")
            }
            ("instagram_subscription", "output_dir_override") => {
                ("instagram_subscription", "id", "output_dir_override")
            }
            ("job", "params_json") => ("job", "id", "params_json"),
            _ => {
                return Err(invalid(
                    "root rebind receipt contains an unsupported database surface",
                ))
            }
        };
        let expected = if forward {
            &change.rebound_value
        } else {
            &change.original_value
        };
        let query = format!("SELECT {field} FROM {table} WHERE {id_column}=?1");
        let current = conn
            .query_row(&query, [&change.row_id], |row| row.get::<_, String>(0))
            .optional()?;
        if current.as_deref() != Some(expected.as_str()) {
            return Err(invalid(format!(
                "root rebind state verification failed for {}:{}",
                change.surface, change.row_id
            )));
        }
    }
    Ok(())
}

fn verify_recorded_feature_state(
    paths: &AppPaths,
    changes: &[RootRebindRowChange],
    forward: bool,
) -> Result<()> {
    let Some(change) = changes
        .iter()
        .find(|change| change.surface == "feature_storage_roots" && change.field == "video_root")
    else {
        return Ok(());
    };
    let expected = if forward {
        &change.rebound_value
    } else {
        &change.original_value
    };
    let feature = config::load_feature_storage_roots_config(paths)?;
    if feature.video_root.as_deref() != Some(expected.as_str()) {
        return Err(invalid("root rebind feature config verification failed"));
    }
    Ok(())
}

pub fn apply_prepared_root_rebind(
    paths: &AppPaths,
    receipt_id: &str,
    stop_after: Option<RootRebindStopAfter>,
) -> Result<RootRebindReceipt> {
    let cancellation = AtomicBool::new(false);
    apply_prepared_root_rebind_cancellable(paths, receipt_id, stop_after, &cancellation)
}

pub fn apply_prepared_root_rebind_cancellable(
    paths: &AppPaths,
    receipt_id: &str,
    stop_after: Option<RootRebindStopAfter>,
    cancellation: &AtomicBool,
) -> Result<RootRebindReceipt> {
    let _operation_guard = ROOT_REBIND_OPERATION_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| invalid("root rebind operation lock is poisoned"))?;
    let mut receipt = root_rebind_receipt_status(paths, receipt_id)?;
    if receipt.schema_version != ROOT_REBIND_RECEIPT_SCHEMA_VERSION {
        return Err(invalid(
            "root rebind receipt schema is stale; prepare a new dry run before applying",
        ));
    }
    if !matches!(receipt.status.as_str(), "prepared" | "applying" | "applied") {
        return Err(invalid("root rebind receipt is not applicable"));
    }
    {
        let conn = db::open_readonly(paths)?;
        refuse_affected_running_jobs(&conn, &receipt.from_root, Some(&receipt.to_root), "apply")?;
    }
    let target = PathBuf::from(&receipt.to_root);
    verify_identity_evidence(&target, &receipt.identity_evidence, cancellation)?;
    if cancellation.load(Ordering::Relaxed) {
        return Err(invalid("root rebind canceled before mutation"));
    }
    if !Path::new(&receipt.backup.sqlite_path).is_file()
        || receipt.backup.sqlite_integrity != "ok"
        || !receipt.backup.feature_config_verified
    {
        return Err(invalid(
            "root rebind backups are not independently verified",
        ));
    }
    verify_aliases_backup(&receipt.backup)?;
    reject_unrecorded_feature_match(paths, &receipt)?;
    receipt.status = "applying".to_string();
    receipt.updated_at_ms = now_ms();
    write_receipt(paths, &receipt)?;

    if receipt.phase == "backups_verified" {
        receipt.phase = "database_applying".to_string();
        receipt.updated_at_ms = now_ms();
        write_receipt(paths, &receipt)?;
    }
    if receipt.phase == "database_applying" {
        let mut conn = db::open(paths)?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        refuse_affected_running_jobs(&tx, &receipt.from_root, Some(&receipt.to_root), "apply")?;
        reject_unrecorded_database_matches(&tx, &receipt)?;
        apply_recorded_database_changes(&tx, &receipt.affected_rows, true)?;
        tx.commit()?;
        verify_recorded_database_state(paths, &receipt.affected_rows, true)?;
        receipt.phase = "database_applied".to_string();
        receipt.updated_at_ms = now_ms();
        write_receipt(paths, &receipt)?;
    }
    if receipt.phase == "database_applied" && stop_after == Some(RootRebindStopAfter::Database) {
        return Ok(receipt);
    }

    if receipt.phase == "database_applied" {
        receipt.phase = "feature_config_applying".to_string();
        receipt.updated_at_ms = now_ms();
        write_receipt(paths, &receipt)?;
    }
    if receipt.phase == "feature_config_applying" {
        // Reserve the job database while checking the running set and publishing the
        // feature-root change. A runner cannot transition a newly claimed job to running
        // between this check and the config write.
        let mut conn = db::open(paths)?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        refuse_affected_running_jobs(&tx, &receipt.from_root, Some(&receipt.to_root), "apply")?;
        reject_unrecorded_feature_match(paths, &receipt)?;
        apply_recorded_feature_change(paths, &receipt.affected_rows, true)?;
        verify_recorded_feature_state(paths, &receipt.affected_rows, true)?;
        tx.commit()?;
        receipt.phase = "feature_config_applied".to_string();
        receipt.updated_at_ms = now_ms();
        write_receipt(paths, &receipt)?;
    }
    if receipt.phase == "feature_config_applied"
        && stop_after == Some(RootRebindStopAfter::FeatureConfig)
    {
        return Ok(receipt);
    }

    if receipt.phase == "feature_config_applied" {
        receipt.phase = "alias_activating".to_string();
        receipt.updated_at_ms = now_ms();
        write_receipt(paths, &receipt)?;
    }
    if receipt.phase == "alias_activating" || receipt.phase == "alias_activated" {
        // Hold a SQLite write reservation from the last stale-snapshot check through alias and
        // receipt publication. Rows created after prepare cannot slip into the old-root gap
        // between validation and the externally visible applied state.
        let mut conn = db::open(paths)?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        refuse_affected_running_jobs(&tx, &receipt.from_root, Some(&receipt.to_root), "apply")?;
        reject_unrecorded_database_matches(&tx, &receipt)?;
        config::with_feature_storage_roots_config_lock(paths, |feature_config| {
            reject_unrecorded_feature_match_config(feature_config, &receipt)?;
            let receipt_path = paths
                .root_rebind_receipts_dir()
                .join(format!("{}.json", receipt.id));
            let aliases = update_root_aliases(paths, |aliases| {
                aliases.aliases.retain(|alias| alias.id != receipt.id);
                aliases.aliases.push(RootAliasRecord {
                    id: receipt.id.clone(),
                    from_root: receipt.from_root.clone(),
                    to_root: receipt.to_root.clone(),
                    verified_at_ms: receipt.target_verified_at_ms,
                    status: "active".to_string(),
                    receipt_path: receipt_path.to_string_lossy().to_string(),
                });
                Ok(())
            })?;
            let active = aliases.aliases.into_iter().any(|alias| {
                alias.id == receipt.id
                    && alias.status == "active"
                    && alias.from_root == receipt.from_root
                    && alias.to_root == receipt.to_root
            });
            if !active {
                return Err(invalid("root rebind alias activation verification failed"));
            }
            receipt.phase = "alias_activated".to_string();
            receipt.status = "applied".to_string();
            receipt.updated_at_ms = now_ms();
            write_receipt(paths, &receipt)?;
            Ok(())
        })?;
        tx.commit()?;
    }
    Ok(receipt)
}

pub fn rollback_root_rebind(paths: &AppPaths, receipt_id: &str) -> Result<RootRebindReceipt> {
    let _operation_guard = ROOT_REBIND_OPERATION_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| invalid("root rebind operation lock is poisoned"))?;
    let mut receipt = root_rebind_receipt_status(paths, receipt_id)?;
    if receipt.schema_version != ROOT_REBIND_RECEIPT_SCHEMA_VERSION {
        return Err(invalid(
            "root rebind receipt schema is stale; automatic rollback is unsafe",
        ));
    }
    if !matches!(
        receipt.status.as_str(),
        "prepared" | "applying" | "applied" | "rolling_back" | "rolled_back"
    ) {
        return Err(invalid("root rebind receipt is not rollback-capable"));
    }
    {
        let conn = db::open_readonly(paths)?;
        refuse_affected_running_jobs(
            &conn,
            &receipt.from_root,
            Some(&receipt.to_root),
            "rollback",
        )?;
    }

    // Persist rollback intent before touching either store. Every side effect below is
    // idempotent and reconciles actual values, never the possibly stale receipt phase.
    receipt.status = "rolling_back".to_string();
    receipt.phase = "rolling_back".to_string();
    receipt.updated_at_ms = now_ms();
    write_receipt(paths, &receipt)?;

    let mut conn = db::open(paths)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    refuse_affected_running_jobs(&tx, &receipt.from_root, Some(&receipt.to_root), "rollback")?;
    apply_recorded_database_changes(&tx, &receipt.affected_rows, false)?;
    tx.commit()?;
    verify_recorded_database_state(paths, &receipt.affected_rows, false)?;

    // A second immediate reservation spans the non-database rollback surfaces. If a
    // runner claimed a job after the database rollback committed, this check refuses the
    // continuation; otherwise no job can become running until feature config and aliases
    // both point back to the old root.
    let mut conn = db::open(paths)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    refuse_affected_running_jobs(&tx, &receipt.from_root, Some(&receipt.to_root), "rollback")?;
    apply_recorded_feature_change(paths, &receipt.affected_rows, false)?;
    verify_recorded_feature_state(paths, &receipt.affected_rows, false)?;

    let aliases = update_root_aliases(paths, |aliases| {
        aliases.aliases.retain(|alias| alias.id != receipt.id);
        Ok(())
    })?;
    if aliases
        .aliases
        .iter()
        .any(|alias| alias.id == receipt.id && alias.status == "active")
    {
        return Err(invalid("root rebind rollback alias verification failed"));
    }
    tx.commit()?;
    receipt.status = "rolled_back".to_string();
    receipt.phase = "rolled_back".to_string();
    receipt.updated_at_ms = now_ms();
    write_receipt(paths, &receipt)?;
    Ok(receipt)
}

/// Resume only receipts that had crossed into `applying`. A merely prepared receipt remains
/// inert and still requires an explicit guarded apply call.
pub fn reconcile_incomplete_root_rebinds(paths: &AppPaths) -> Result<Vec<RootRebindReceipt>> {
    let mut reconciled = Vec::new();
    for receipt in list_root_rebind_receipts(paths)? {
        if receipt.status == "applying" {
            reconciled.push(apply_prepared_root_rebind(paths, &receipt.id, None)?);
        } else if receipt.status == "applied" {
            // Idempotently reassert the alias if a crash occurred between alias persistence and
            // the final receipt write.
            reconciled.push(apply_prepared_root_rebind(paths, &receipt.id, None)?);
        } else if receipt.status == "rolling_back" {
            reconciled.push(rollback_root_rebind(paths, &receipt.id)?);
        }
    }
    Ok(reconciled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, paths::AppPaths};

    fn requested_evidence(relative_path: &str) -> RootIdentityEvidence {
        RootIdentityEvidence {
            relative_path: relative_path.to_string(),
            expected_bytes: None,
            observed_bytes: 0,
            expected_sample_sha256: None,
            observed_sample_sha256: String::new(),
        }
    }

    fn alias(id: &str, from: &str, to: &str) -> RootAliasRecord {
        RootAliasRecord {
            id: id.to_string(),
            from_root: from.to_string(),
            to_root: to.to_string(),
            verified_at_ms: 1,
            status: "active".to_string(),
            receipt_path: "receipt.json".to_string(),
        }
    }

    #[test]
    fn resolver_handles_device_unc_case_and_component_boundaries() {
        let aliases = vec![alias("a", r"\\?\unc\MIR\home\Video", r"X:\archive")];
        assert_eq!(
            resolve_alias_path(&aliases, r"\\mir\HOME\video\Channel\clip.mp4").unwrap(),
            Some(r"X:\archive\Channel\clip.mp4".to_string())
        );
        assert_eq!(
            resolve_alias_path(&aliases, r"\\?\UnC\mir\HOME\video\Channel\clip.mp4").unwrap(),
            Some(r"X:\archive\Channel\clip.mp4".to_string())
        );
        assert_eq!(
            historical_alias_candidates(&aliases, r"x:\ARCHIVE\Channel\clip.mp4").unwrap(),
            vec![r"\\?\unc\MIR\home\Video\Channel\clip.mp4".to_string()]
        );
        assert_eq!(
            resolve_alias_path(&aliases, r"\\mir\HOME\video-old\clip.mp4").unwrap(),
            None
        );
    }

    #[test]
    fn aliases_reject_cycles_overlap_and_duplicate_targets() {
        assert!(validate_aliases(&[
            alias("a", r"C:\old", r"D:\new"),
            alias("b", r"D:\new", r"C:\old"),
        ])
        .is_err());
        let too_many = (0..=MAX_ACTIVE_ROOT_ALIASES)
            .map(|index| {
                alias(
                    &format!("bounded-{index}"),
                    &format!(r"C:\source-{index}"),
                    &format!(r"D:\target-{index}"),
                )
            })
            .collect::<Vec<_>>();
        assert!(
            validate_aliases(&too_many).is_err(),
            "alias resolution must remain bounded"
        );
        assert!(validate_aliases(&[
            alias("a", r"C:\old", r"D:\one"),
            alias("b", r"C:\old\nested", r"E:\two"),
        ])
        .is_err());
        assert!(validate_aliases(&[
            alias("a", r"C:\one", r"D:\same"),
            alias("b", r"C:\two", r"D:\same"),
        ])
        .is_err());
    }

    #[test]
    fn disconnected_alias_target_falls_back_for_reads_but_new_writes_fail_closed() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("app"));
        paths.ensure_dirs().unwrap();
        let old_root = dir.path().join("old_root");
        let target_root = dir.path().join("disconnected_target");
        std::fs::create_dir_all(old_root.join("channel")).unwrap();
        let old_file = old_root.join("channel").join("legacy.mp4");
        std::fs::write(&old_file, b"legacy").unwrap();
        let config = RootAliasesConfig {
            schema_version: ROOT_ALIAS_SCHEMA_VERSION,
            aliases: vec![alias(
                "disconnect",
                &old_root.to_string_lossy(),
                &target_root.to_string_lossy(),
            )],
        };
        persistence::atomic_write_text(
            &paths.root_aliases_config_path(),
            &format!("{}\n", serde_json::to_string_pretty(&config).unwrap()),
        )
        .unwrap();

        assert_eq!(
            resolve_active_alias_path(&paths, &old_file, false).unwrap(),
            old_file,
            "historical reads must recover through the restored old root"
        );
        assert!(
            resolve_active_alias_path(&paths, &old_root.join("new-output"), true).is_err(),
            "new writes must never fall back to the historical root"
        );
    }

    #[test]
    fn queued_destination_mapping_changes_only_typed_output_dir() {
        let mut value = serde_json::json!({
            "output_dir": r"\\?\uNc\mir\home\Video\channel",
            "nested": [r"\\mir\home\video\other", "title"],
            "path": r"\\mir\home\video\input.mp4",
            "url": "https://example.invalid/watch?path=archive",
            "similar": r"\\mir\home\video-old\untouched"
        });
        assert!(
            map_queued_destination_fields(&mut value, r"\\mir\home\video", r"Y:\archive").unwrap()
        );
        assert_eq!(value["output_dir"], r"Y:\archive\channel");
        assert_eq!(value["nested"][0], r"\\mir\home\video\other");
        assert_eq!(value["path"], r"\\mir\home\video\input.mp4");
        assert_eq!(value["similar"], r"\\mir\home\video-old\untouched");
    }

    #[test]
    fn aliases_reject_nested_self_targets_and_one_hop_chains() {
        assert!(validate_aliases(&[alias("self", r"C:\archive", r"C:\archive\moved")]).is_err());
        assert!(validate_aliases(&[
            alias("first", r"C:\old", r"D:\new"),
            alias("second", r"D:\new", r"E:\final"),
        ])
        .is_err());
        assert!(validate_aliases(&[
            alias("first", r"C:\old", r"D:\new\nested"),
            alias("second", r"D:\new", r"E:\final"),
        ])
        .is_err());
    }

    #[test]
    fn queued_destination_dry_run_counts_jobs_not_matching_input_strings() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("app"));
        let conn = db::open(&paths).unwrap();
        db::migrate(&conn).unwrap();
        let insert = |id: &str, status: &str, params_json: Value| {
            conn.execute(
                "INSERT INTO job(id,type,status,progress,params_json,created_at_ms,logs_path) VALUES(?1,'download_direct_url',?2,0,?3,1,'log')",
                params![id, status, serde_json::to_string(&params_json).unwrap()],
            )
            .unwrap();
        };
        insert(
            "destination-match",
            "queued",
            serde_json::json!({
                "output_dir": r"C:\old\downloads",
                "path": r"C:\old\input.mp4"
            }),
        );
        insert(
            "input-only",
            "queued",
            serde_json::json!({
                "output_dir": r"E:\other",
                "path": r"C:\old\input.mp4",
                "pipeline": { "source_path": r"C:\old\nested.mp4" }
            }),
        );
        insert(
            "not-queued",
            "running",
            serde_json::json!({ "output_dir": r"C:\old\downloads" }),
        );
        drop(conn);

        let dry_run = root_rebind_dry_run(&paths, r"C:\old").unwrap();
        assert_eq!(dry_run.queued_destination_matches, 1);
        assert_eq!(dry_run.running_destination_matches, 1);
        assert_eq!(
            dry_run.running_destination_job_ids,
            vec!["not-queued".to_string()]
        );
    }

    #[test]
    fn apply_and_rollback_refuse_affected_running_jobs_without_partial_publication() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("app"));
        paths.ensure_dirs().unwrap();
        let conn = db::open(&paths).unwrap();
        db::migrate(&conn).unwrap();
        drop(conn);
        let old_root = dir.path().join("old");
        let target_root = dir.path().join("target");
        std::fs::create_dir_all(&old_root).unwrap();
        std::fs::create_dir_all(&target_root).unwrap();
        std::fs::write(old_root.join("identity.bin"), b"same").unwrap();
        std::fs::write(target_root.join("identity.bin"), b"same").unwrap();
        let old_root_text = old_root.to_string_lossy().to_string();
        let target_root_text = target_root.to_string_lossy().to_string();
        db::open(&paths)
            .unwrap()
            .execute(
                "INSERT INTO library_item(id,created_at_ms,source_type,source_uri,title,media_path) VALUES('root-rebind-identity',1,'local_file','file://identity','Identity',?1)",
                [old_root.join("identity.bin").to_string_lossy().as_ref()],
            )
            .unwrap();
        let receipt = prepare_root_rebind(
            &paths,
            &old_root_text,
            &target_root,
            &[requested_evidence("identity.bin")],
        )
        .unwrap();

        let insert_running = |id: &str, output_dir: &str| {
            let conn = db::open(&paths).unwrap();
            conn.execute(
                "INSERT INTO job(id,type,status,progress,params_json,created_at_ms,logs_path) VALUES(?1,'download_direct_url','running',0,?2,1,'log')",
                params![
                    id,
                    serde_json::to_string(&serde_json::json!({ "output_dir": output_dir }))
                        .unwrap()
                ],
            )
            .unwrap();
        };
        let finish = |id: &str| {
            db::open(&paths)
                .unwrap()
                .execute("UPDATE job SET status='succeeded' WHERE id=?1", [id])
                .unwrap();
        };

        insert_running("running-before-apply", &old_root_text);
        let error = apply_prepared_root_rebind(&paths, &receipt.id, None)
            .expect_err("apply must refuse an affected in-flight writer");
        assert!(
            error.to_string().contains("running-before-apply"),
            "{error}"
        );
        assert!(load_root_aliases(&paths)
            .unwrap()
            .aliases
            .iter()
            .all(|alias| alias.id != receipt.id));
        assert_eq!(
            root_rebind_receipt_status(&paths, &receipt.id)
                .unwrap()
                .status,
            "prepared"
        );

        finish("running-before-apply");
        let applied = apply_prepared_root_rebind(&paths, &receipt.id, None).unwrap();
        assert_eq!(applied.status, "applied");

        insert_running("running-before-rollback", &target_root_text);
        let error = rollback_root_rebind(&paths, &receipt.id)
            .expect_err("rollback must refuse an affected in-flight writer");
        assert!(
            error.to_string().contains("running-before-rollback"),
            "{error}"
        );
        assert!(load_root_aliases(&paths)
            .unwrap()
            .aliases
            .iter()
            .any(|alias| alias.id == receipt.id && alias.status == "active"));
        assert_eq!(
            root_rebind_receipt_status(&paths, &receipt.id)
                .unwrap()
                .status,
            "applied"
        );

        finish("running-before-rollback");
        let rolled_back = rollback_root_rebind(&paths, &receipt.id).unwrap();
        assert_eq!(rolled_back.status, "rolled_back");
    }

    #[test]
    fn interrupted_rebind_resumes_without_rewriting_historical_library_paths() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("app"));
        let old_root = dir.path().join("old");
        std::fs::create_dir_all(&old_root).unwrap();
        let old_root_text = old_root.to_string_lossy().to_string();
        let legacy_path = old_root.join("legacy.mp4").to_string_lossy().to_string();
        std::fs::write(old_root.join("legacy.mp4"), b"identity").unwrap();
        let conn = db::open(&paths).unwrap();
        db::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO video_library(id,name,root_path,active,kind,created_at_ms,updated_at_ms) VALUES('lib','Library',?1,1,'custom',1,1)",
            [&old_root_text],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO library_item(id,created_at_ms,source_type,source_uri,title,media_path) VALUES('item',1,'file','file://legacy','Legacy',?1)",
            [&legacy_path],
        )
        .unwrap();
        drop(conn);
        config::save_feature_storage_roots_config(
            &paths,
            &FeatureStorageRootsConfig {
                video_root: Some(old_root_text.clone()),
                ..FeatureStorageRootsConfig::default()
            },
        )
        .unwrap();
        let target = dir.path().join("target");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("legacy.mp4"), b"identity").unwrap();
        let receipt = prepare_root_rebind(
            &paths,
            &old_root_text,
            &target,
            &[requested_evidence("legacy.mp4")],
        )
        .unwrap();
        // Crash failpoint: the DB commit happened after durable intent but before the phase
        // completion receipt. Resume must inspect/idempotently accept that actual state.
        let mut crash_receipt = receipt.clone();
        crash_receipt.status = "applying".to_string();
        crash_receipt.phase = "database_applying".to_string();
        write_receipt(&paths, &crash_receipt).unwrap();
        let mut conn = db::open(&paths).unwrap();
        let tx = conn.transaction().unwrap();
        apply_recorded_database_changes(&tx, &receipt.affected_rows, true).unwrap();
        tx.commit().unwrap();
        let partial =
            apply_prepared_root_rebind(&paths, &receipt.id, Some(RootRebindStopAfter::Database))
                .unwrap();
        assert_eq!(partial.phase, "database_applied");

        // Second crash failpoint: config was atomically written, but its phase receipt was not.
        let mut config_crash_receipt = partial.clone();
        config_crash_receipt.status = "applying".to_string();
        config_crash_receipt.phase = "feature_config_applying".to_string();
        write_receipt(&paths, &config_crash_receipt).unwrap();
        apply_recorded_feature_change(&paths, &receipt.affected_rows, true).unwrap();
        let reconciled = reconcile_incomplete_root_rebinds(&paths).unwrap();
        assert_eq!(reconciled.len(), 1);
        let done = reconciled.into_iter().next().unwrap();
        assert_eq!(done.status, "applied");
        let conn = db::open_readonly(&paths).unwrap();
        let media_path: String = conn
            .query_row(
                "SELECT media_path FROM library_item WHERE id='item'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(media_path, legacy_path);

        let rolled_back = rollback_root_rebind(&paths, &receipt.id).unwrap();
        assert_eq!(rolled_back.status, "rolled_back");
        let rolled_back_again = rollback_root_rebind(&paths, &receipt.id).unwrap();
        assert_eq!(rolled_back_again.status, "rolled_back");
        assert!(load_root_aliases(&paths).unwrap().aliases.is_empty());
        let conn = db::open_readonly(&paths).unwrap();
        let library_root: String = conn
            .query_row(
                "SELECT root_path FROM video_library WHERE id='lib'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(library_root, old_root_text);
    }

    #[test]
    fn identity_evidence_rejects_wrong_same_size_tree() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("app"));
        db::ensure_schema(&paths).unwrap();
        let old_root = dir.path().join("old");
        let target = dir.path().join("target");
        std::fs::create_dir_all(&old_root).unwrap();
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(old_root.join("identity.bin"), b"expected").unwrap();
        std::fs::write(target.join("identity.bin"), b"wrongone").unwrap();
        let conn = db::open(&paths).unwrap();
        conn.execute(
            "INSERT INTO library_item(id,created_at_ms,source_type,source_uri,title,media_path) VALUES('identity-item',1,'local_file','file://identity','Identity',?1)",
            [old_root.join("identity.bin").to_string_lossy().to_string()],
        )
        .unwrap();
        drop(conn);
        let error = prepare_root_rebind(
            &paths,
            &old_root.to_string_lossy(),
            &target,
            &[requested_evidence("identity.bin")],
        )
        .expect_err("same-size wrong content must not prove root identity");
        assert!(error.to_string().contains("sampled content mismatch"));
    }

    #[test]
    fn identity_evidence_rejects_target_self_attestation_without_readable_old_root() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("app"));
        db::ensure_schema(&paths).unwrap();
        let missing_old = dir.path().join("missing-old-root");
        let target = dir.path().join("target");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("identity.bin"), b"identity").unwrap();
        let target_hash = sampled_content_sha256(&target.join("identity.bin")).unwrap();
        let error = prepare_root_rebind(
            &paths,
            &missing_old.to_string_lossy(),
            &target,
            &[RootIdentityEvidence {
                expected_sample_sha256: Some(target_hash.clone()),
                observed_sample_sha256: target_hash,
                ..requested_evidence("identity.bin")
            }],
        )
        .expect_err("target self-attestation must not replace canonical old-root evidence");
        assert!(error.to_string().contains("old root is unavailable"));
    }

    #[test]
    fn identity_evidence_requires_unique_deterministic_bounded_library_coverage() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("app"));
        db::ensure_schema(&paths).unwrap();
        let old_root = dir.path().join("old");
        let target = dir.path().join("target");
        std::fs::create_dir_all(&old_root).unwrap();
        std::fs::create_dir_all(&target).unwrap();
        let conn = db::open(&paths).unwrap();
        for (index, name) in ["a.mp4", "b.mp4", "c.mp4", "d.mp4"].iter().enumerate() {
            let bytes = format!("canonical-{name}");
            std::fs::write(old_root.join(name), &bytes).unwrap();
            std::fs::write(target.join(name), &bytes).unwrap();
            conn.execute(
                "INSERT INTO library_item(id,created_at_ms,source_type,source_uri,title,media_path) VALUES(?1,1,'local_file',?2,?3,?4)",
                params![
                    format!("item-{index}"),
                    format!("file://{name}"),
                    *name,
                    old_root.join(name).to_string_lossy().to_string()
                ],
            )
            .unwrap();
        }
        drop(conn);

        let arbitrary = prepare_root_rebind(
            &paths,
            &old_root.to_string_lossy(),
            &target,
            &[requested_evidence("b.mp4")],
        )
        .expect_err("one caller-selected file cannot replace deterministic coverage");
        assert!(arbitrary.to_string().contains("exactly cover"));

        let repeated = prepare_root_rebind(
            &paths,
            &old_root.to_string_lossy(),
            &target,
            &[
                requested_evidence("a.mp4"),
                requested_evidence("a.mp4"),
                requested_evidence("d.mp4"),
            ],
        )
        .expect_err("repeated evidence must be refused");
        assert!(repeated.to_string().contains("repeated canonical path"));

        let receipt = prepare_root_rebind(&paths, &old_root.to_string_lossy(), &target, &[])
            .expect("engine-selected first/middle/last canonical coverage");
        assert_eq!(
            receipt
                .identity_evidence
                .iter()
                .map(|entry| entry.relative_path.as_str())
                .collect::<Vec<_>>(),
            vec!["a.mp4", "c.mp4", "d.mp4"]
        );
    }

    #[test]
    fn rollback_restores_only_receipted_rows_and_preserves_preexisting_destination_rows() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("app"));
        let old_root = dir.path().join("old");
        let target = dir.path().join("target");
        std::fs::create_dir_all(&old_root).unwrap();
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(old_root.join("identity.bin"), b"identity").unwrap();
        std::fs::write(target.join("identity.bin"), b"identity").unwrap();
        let old_downloads = old_root.join("downloads").to_string_lossy().to_string();
        let old_source = old_root.join("source.mp4").to_string_lossy().to_string();
        let old_untouched = old_root
            .join("must-not-be-rewritten.mp4")
            .to_string_lossy()
            .to_string();
        let moved_destination = target.join("downloads").to_string_lossy().to_string();
        let conn = db::open(&paths).unwrap();
        db::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO library_item(id,created_at_ms,source_type,source_uri,title,media_path) VALUES('identity-item',1,'local_file','file://identity','Identity',?1)",
            [old_root.join("identity.bin").to_string_lossy().to_string()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO job(id,type,status,progress,params_json,created_at_ms,logs_path) VALUES('move-me','download_direct_url','queued',0,?1,1,'move.log')",
            [serde_json::json!({"output_dir": old_downloads, "input_path": old_source}).to_string()],
        ).unwrap();
        conn.execute(
            "INSERT INTO job(id,type,status,progress,params_json,created_at_ms,logs_path) VALUES('already-there','download_direct_url','queued',0,?1,1,'existing.log')",
            [serde_json::json!({"output_dir": moved_destination, "input_path": old_untouched}).to_string()],
        ).unwrap();
        drop(conn);
        let receipt = prepare_root_rebind(
            &paths,
            &old_root.to_string_lossy(),
            &target,
            &[requested_evidence("identity.bin")],
        )
        .unwrap();
        assert!(receipt
            .affected_rows
            .iter()
            .any(|row| row.row_id == "move-me"));
        assert!(!receipt
            .affected_rows
            .iter()
            .any(|row| row.row_id == "already-there"));
        apply_prepared_root_rebind(&paths, &receipt.id, None).unwrap();
        rollback_root_rebind(&paths, &receipt.id).unwrap();
        let conn = db::open_readonly(&paths).unwrap();
        let moved: String = conn
            .query_row(
                "SELECT params_json FROM job WHERE id='move-me'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let existing: String = conn
            .query_row(
                "SELECT params_json FROM job WHERE id='already-there'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let moved: serde_json::Value = serde_json::from_str(&moved).unwrap();
        let existing: serde_json::Value = serde_json::from_str(&existing).unwrap();
        assert_eq!(
            moved["output_dir"],
            old_root.join("downloads").to_string_lossy().as_ref()
        );
        assert_eq!(
            moved["input_path"],
            old_root.join("source.mp4").to_string_lossy().as_ref()
        );
        assert_eq!(
            existing["output_dir"],
            target.join("downloads").to_string_lossy().as_ref()
        );
        assert_eq!(
            existing["input_path"],
            old_root
                .join("must-not-be-rewritten.mp4")
                .to_string_lossy()
                .as_ref()
        );
    }

    #[test]
    fn rollback_reconciles_side_effects_that_precede_receipt_updates_and_rechecks_rolled_back_state(
    ) {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("app"));
        let old_root = dir.path().join("old");
        let target = dir.path().join("target");
        std::fs::create_dir_all(&old_root).unwrap();
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(old_root.join("canonical.mp4"), b"canonical-media").unwrap();
        std::fs::write(target.join("canonical.mp4"), b"canonical-media").unwrap();
        let old_root_text = old_root.to_string_lossy().to_string();
        let target_text = target.to_string_lossy().to_string();
        let conn = db::open(&paths).unwrap();
        db::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO video_library(id,name,root_path,active,kind,created_at_ms,updated_at_ms) VALUES('lib','Library',?1,1,'custom',1,1)",
            [&old_root_text],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO library_item(id,created_at_ms,source_type,source_uri,title,media_path) VALUES('canonical-item',1,'local_file','file://canonical','Canonical',?1)",
            [old_root.join("canonical.mp4").to_string_lossy().to_string()],
        )
        .unwrap();
        drop(conn);
        config::save_feature_storage_roots_config(
            &paths,
            &FeatureStorageRootsConfig {
                video_root: Some(old_root_text.clone()),
                ..FeatureStorageRootsConfig::default()
            },
        )
        .unwrap();

        let receipt = prepare_root_rebind(
            &paths,
            &old_root_text,
            &target,
            &[requested_evidence("canonical.mp4")],
        )
        .unwrap();

        // Deterministic crash failpoint: every side effect has committed, while the durable
        // receipt still describes only the pre-side-effect state.
        let mut conn = db::open(&paths).unwrap();
        let tx = conn.transaction().unwrap();
        apply_recorded_database_changes(&tx, &receipt.affected_rows, true).unwrap();
        tx.commit().unwrap();
        apply_recorded_feature_change(&paths, &receipt.affected_rows, true).unwrap();
        let mut aliases = load_root_aliases(&paths).unwrap();
        aliases
            .aliases
            .push(alias(&receipt.id, &old_root_text, &target_text));
        save_root_aliases(&paths, &aliases).unwrap();

        let rolled_back = rollback_root_rebind(&paths, &receipt.id).unwrap();
        assert_eq!(rolled_back.status, "rolled_back");
        verify_recorded_database_state(&paths, &receipt.affected_rows, false).unwrap();
        verify_recorded_feature_state(&paths, &receipt.affected_rows, false).unwrap();
        assert!(load_root_aliases(&paths)
            .unwrap()
            .aliases
            .iter()
            .all(|entry| entry.id != receipt.id));

        // A stale external rebound after the receipt said rolled_back must be repaired; the old
        // early-return behavior would have left state rebound while claiming rollback success.
        let mut conn = db::open(&paths).unwrap();
        let tx = conn.transaction().unwrap();
        apply_recorded_database_changes(&tx, &receipt.affected_rows, true).unwrap();
        tx.commit().unwrap();
        let rolled_back_again = rollback_root_rebind(&paths, &receipt.id).unwrap();
        assert_eq!(rolled_back_again.status, "rolled_back");
        verify_recorded_database_state(&paths, &receipt.affected_rows, false).unwrap();
    }

    #[test]
    fn apply_rejects_database_rows_created_after_prepare_without_publishing_alias() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("app"));
        let old_root = dir.path().join("old");
        let target = dir.path().join("target");
        std::fs::create_dir_all(&old_root).unwrap();
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(old_root.join("identity.mkv"), b"identity").unwrap();
        std::fs::write(target.join("identity.mkv"), b"identity").unwrap();
        let old_root_text = old_root.to_string_lossy().to_string();
        let conn = db::open(&paths).unwrap();
        db::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO library_item(id,created_at_ms,source_type,source_uri,title,media_path) VALUES('identity',1,'local_file','file://identity','Identity',?1)",
            [old_root.join("identity.mkv").to_string_lossy().to_string()],
        )
        .unwrap();
        drop(conn);
        let receipt = prepare_root_rebind(&paths, &old_root_text, &target, &[]).unwrap();

        let conn = db::open(&paths).unwrap();
        conn.execute(
            "INSERT INTO video_library(id,name,root_path,active,kind,created_at_ms,updated_at_ms) VALUES('late-library','Late',?1,1,'custom',1,1)",
            [&old_root_text],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO job(id,type,status,progress,params_json,created_at_ms,logs_path) VALUES('late-job','download_direct_url','queued',0,?1,1,'late.log')",
            [serde_json::json!({"output_dir": old_root.join("late")}).to_string()],
        )
        .unwrap();
        drop(conn);

        let error = apply_prepared_root_rebind(&paths, &receipt.id, None)
            .expect_err("a prepared snapshot must not omit later matching destinations");
        assert!(error.to_string().contains("prepared snapshot is stale"));
        assert!(load_root_aliases(&paths).unwrap().aliases.is_empty());
        let persisted = root_rebind_receipt_status(&paths, &receipt.id).unwrap();
        assert_ne!(persisted.status, "applied");
        let conn = db::open_readonly(&paths).unwrap();
        let root: String = conn
            .query_row(
                "SELECT root_path FROM video_library WHERE id='late-library'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(root, old_root_text);
    }

    #[test]
    fn apply_rejects_feature_root_created_after_prepare() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("app"));
        let old_root = dir.path().join("old");
        let target = dir.path().join("target");
        std::fs::create_dir_all(&old_root).unwrap();
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(old_root.join("identity.mkv"), b"identity").unwrap();
        std::fs::write(target.join("identity.mkv"), b"identity").unwrap();
        let old_root_text = old_root.to_string_lossy().to_string();
        let conn = db::open(&paths).unwrap();
        db::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO library_item(id,created_at_ms,source_type,source_uri,title,media_path) VALUES('identity',1,'local_file','file://identity','Identity',?1)",
            [old_root.join("identity.mkv").to_string_lossy().to_string()],
        )
        .unwrap();
        drop(conn);
        let receipt = prepare_root_rebind(&paths, &old_root_text, &target, &[]).unwrap();
        config::save_feature_storage_roots_config(
            &paths,
            &FeatureStorageRootsConfig {
                video_root: Some(old_root_text.clone()),
                ..FeatureStorageRootsConfig::default()
            },
        )
        .unwrap();

        let error = apply_prepared_root_rebind(&paths, &receipt.id, None)
            .expect_err("a new matching feature root invalidates prepare");
        assert!(error.to_string().contains("feature video_root now matches"));
        assert!(load_root_aliases(&paths).unwrap().aliases.is_empty());
        assert_eq!(
            config::load_feature_storage_roots_config(&paths)
                .unwrap()
                .video_root,
            Some(old_root_text)
        );
    }

    #[test]
    fn apply_reopens_and_hashes_alias_backup_before_mutation() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("app"));
        let old_root = dir.path().join("old");
        let target = dir.path().join("target");
        std::fs::create_dir_all(&old_root).unwrap();
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(old_root.join("identity.mkv"), b"identity").unwrap();
        std::fs::write(target.join("identity.mkv"), b"identity").unwrap();
        let conn = db::open(&paths).unwrap();
        db::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO library_item(id,created_at_ms,source_type,source_uri,title,media_path) VALUES('identity',1,'local_file','file://identity','Identity',?1)",
            [old_root.join("identity.mkv").to_string_lossy().to_string()],
        )
        .unwrap();
        drop(conn);
        let receipt = prepare_root_rebind(&paths, &old_root.to_string_lossy(), &target, &[])
            .expect("prepare with alias backup");
        assert!(receipt.backup.aliases_config_verified);
        assert!(!receipt.backup.aliases_config_sha256.is_empty());
        let backed_up: RootAliasesConfig = serde_json::from_slice(
            &std::fs::read(&receipt.backup.aliases_config_path).expect("alias backup"),
        )
        .expect("reopened alias backup");
        assert!(backed_up.aliases.is_empty());

        std::fs::write(
            &receipt.backup.aliases_config_path,
            b"{\"schema_version\":1,\"aliases\":[]}",
        )
        .expect("tamper alias backup");
        let error = apply_prepared_root_rebind(&paths, &receipt.id, None)
            .expect_err("tampered backup must stop before alias activation");
        assert!(error.to_string().contains("backup hash changed"));
        assert!(load_root_aliases(&paths).unwrap().aliases.is_empty());
        assert_eq!(
            root_rebind_receipt_status(&paths, &receipt.id)
                .unwrap()
                .status,
            "prepared"
        );
    }

    #[test]
    fn concurrent_alias_updates_preserve_both_non_overlapping_records() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("app"));
        paths.ensure_dirs().unwrap();
        let barrier = Arc::new(std::sync::Barrier::new(3));
        let mut handles = Vec::new();
        for entry in [
            alias("race-a", r"C:\old-a", r"D:\new-a"),
            alias("race-b", r"E:\old-b", r"F:\new-b"),
        ] {
            let paths = paths.clone();
            let barrier = Arc::clone(&barrier);
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                update_root_aliases(&paths, |aliases| {
                    aliases.aliases.push(entry);
                    Ok(())
                })
                .unwrap();
            }));
        }
        barrier.wait();
        for handle in handles {
            handle.join().unwrap();
        }
        let aliases = load_root_aliases(&paths).unwrap();
        assert_eq!(aliases.aliases.len(), 2);
        assert!(aliases.aliases.iter().any(|entry| entry.id == "race-a"));
        assert!(aliases.aliases.iter().any(|entry| entry.id == "race-b"));
    }

    #[test]
    fn bounded_executor_returns_pending_ticket_then_typed_completion() {
        let ticket = submit_root_rebind_task("test_pending", || {
            std::thread::sleep(Duration::from_millis(150));
            Ok(serde_json::json!({"receipt_count": 2}))
        })
        .unwrap();
        assert_eq!(ticket.state, "queued");
        let command_started = Instant::now();
        let pending = root_rebind_task_status(&ticket.task_id, Some(2_000)).unwrap();
        assert!(
            command_started.elapsed() < Duration::from_millis(50),
            "status returns a nonblocking snapshot even when passed a wait hint"
        );
        assert!(matches!(pending.state.as_str(), "queued" | "running"));
        let completed = loop {
            let status = root_rebind_task_status(&ticket.task_id, None).unwrap();
            if matches!(status.state.as_str(), "completed" | "failed") {
                break status;
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(completed.state, "completed");
        assert_eq!(completed.result.unwrap()["receipt_count"], 2);
        assert_eq!(
            ROOT_REBIND_WORKERS_STARTED.load(Ordering::Relaxed),
            ROOT_REBIND_WORKER_COUNT as u64,
            "executor initialization is fixed-size rather than per request"
        );
    }

    #[test]
    fn root_parser_rejects_drive_relative_and_incomplete_unc_paths() {
        for invalid_path in [r"C:relative", r"C:", r"\\server", r"1:\not-a-drive"] {
            assert!(
                root_components(invalid_path).is_err(),
                "must reject non-rooted or incomplete path: {invalid_path}"
            );
        }
        assert!(root_components(r"C:\archive").is_ok());
        assert!(root_components(r"\\server\share\archive").is_ok());
        assert!(root_components(r"\\?\UNC\server\share\archive").is_ok());
    }

    #[test]
    fn mapped_drive_and_unc_are_distinct_logical_roots_even_for_one_physical_tree() {
        let same_physical = Path::new(r"\\?\UNC\MIR\home\Video\archive");
        assert!(validate_rebind_root_relation(
            r"\\?\UNC\MIR\home\Video\archive",
            r"Z:\Video\archive",
            same_physical,
            same_physical,
        )
        .unwrap());
        assert!(validate_rebind_root_relation(
            r"\\?\UNC\MIR\home\Video\archive",
            r"\\MIR\home\Video\archive\",
            same_physical,
            same_physical,
        )
        .is_err());
        assert!(validate_rebind_root_relation(
            r"Z:\Video\archive",
            r"z:/video/archive\",
            same_physical,
            same_physical,
        )
        .is_err());
    }

    #[test]
    fn receipt_id_rejects_traversal_and_binds_filename_to_content_id() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("app"));
        paths.ensure_dirs().unwrap();
        for invalid_id in [
            r"..\crafted",
            r"C:\temp\crafted",
            "root-rebind-not-a-uuid",
            "ROOT-REBIND-00000000-0000-0000-0000-000000000000",
        ] {
            assert!(validated_receipt_path(&paths, invalid_id).is_err());
        }

        let requested_id = format!("root-rebind-{}", Uuid::new_v4());
        let different_id = format!("root-rebind-{}", Uuid::new_v4());
        let receipt = RootRebindReceipt {
            schema_version: ROOT_REBIND_RECEIPT_SCHEMA_VERSION,
            id: different_id,
            from_root: r"C:\old".to_string(),
            to_root: r"D:\new".to_string(),
            target_verified_at_ms: 1,
            status: "prepared".to_string(),
            phase: "backups_verified".to_string(),
            identity_evidence: Vec::new(),
            dry_run: RootRebindDryRun::default(),
            backup: RootRebindBackupReference {
                sqlite_path: String::new(),
                sqlite_integrity: "ok".to_string(),
                feature_config_path: None,
                feature_config_verified: true,
                aliases_config_path: String::new(),
                aliases_config_sha256: String::new(),
                aliases_config_source_existed: false,
                aliases_config_verified: true,
            },
            affected_rows: Vec::new(),
            created_at_ms: 1,
            updated_at_ms: 1,
        };
        let path = validated_receipt_path(&paths, &requested_id).unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, serde_json::to_vec(&receipt).unwrap()).unwrap();
        assert!(root_rebind_receipt_status(&paths, &requested_id)
            .unwrap_err()
            .to_string()
            .contains("content id does not match"));
    }

    #[test]
    fn six_figure_identity_selection_caps_filesystem_probes() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("app"));
        db::ensure_schema(&paths).unwrap();
        let old_root = dir.path().join("old");
        std::fs::create_dir_all(&old_root).unwrap();
        for index in 0..MAX_CANONICAL_IDENTITY_CANDIDATE_PROBES {
            std::fs::write(old_root.join(format!("item-{index:06}.mkv")), b"identity").unwrap();
        }
        let mut conn = db::open(&paths).unwrap();
        let tx = conn.transaction().unwrap();
        {
            let mut insert = tx
                .prepare("INSERT INTO library_item(id,created_at_ms,source_type,source_uri,title,media_path) VALUES(?1,1,'local_file',?2,?3,?4)")
                .unwrap();
            for index in 0..140_000_u32 {
                let name = format!("item-{index:06}.mkv");
                insert
                    .execute(params![
                        format!("item-{index}"),
                        format!("file://{name}"),
                        name,
                        old_root
                            .join(format!("item-{index:06}.mkv"))
                            .to_string_lossy()
                            .to_string()
                    ])
                    .unwrap();
            }
        }
        tx.commit().unwrap();
        drop(conn);
        let cancellation = AtomicBool::new(false);
        let (selected, probe_count) =
            canonical_identity_evidence_paths(&paths, &old_root, &cancellation).unwrap();
        assert_eq!(selected.len(), CANONICAL_IDENTITY_SAMPLE_COUNT);
        assert_eq!(probe_count, MAX_CANONICAL_IDENTITY_CANDIDATE_PROBES);
    }

    #[test]
    fn reserved_recovery_worker_survives_two_stalled_general_probes() {
        let stalled = || {
            submit_root_rebind_task_cancellable("prepare", move |cancellation| {
                run_bounded_rebind_io(
                    "synthetic stalled NAS probe",
                    &cancellation,
                    Duration::from_millis(25),
                    || {
                        std::thread::sleep(Duration::from_millis(100));
                        Ok(())
                    },
                )
            })
            .unwrap()
        };
        let first = stalled();
        let second = stalled();
        let recovery = submit_root_rebind_task("recover", || {
            let cancellation = AtomicBool::new(false);
            run_bounded_rebind_io(
                "synthetic reserved recovery probe",
                &cancellation,
                Duration::from_millis(250),
                || Ok("recovered"),
            )
        })
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let status = root_rebind_task_status(&recovery.task_id, None).unwrap();
            if status.state == "completed" {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "reserved recovery worker was starved"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        for task_id in [&first.task_id, &second.task_id] {
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                let status = root_rebind_task_status(task_id, None).unwrap();
                if status.state == "failed" {
                    assert!(status.error.unwrap().contains("timed out"));
                    break;
                }
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        assert_eq!(
            ROOT_REBIND_RECOVERY_WORKERS_STARTED.load(Ordering::Relaxed),
            ROOT_REBIND_RECOVERY_WORKER_COUNT as u64
        );
        assert_eq!(
            root_rebind_io_executor(true).workers_started(),
            ROOT_REBIND_RECOVERY_IO_WORKER_COUNT as u64,
            "recovery work must use one fixed reserved I/O worker"
        );
    }

    #[test]
    fn timed_out_io_remains_accounted_and_repeated_hangs_cannot_grow_threads() {
        const WORKERS: usize = 2;
        const QUEUE: usize = 2;
        let executor = RootRebindIoExecutor::new("root-rebind-test-io", WORKERS, QUEUE);
        let entered = Arc::new(AtomicU64::new(0));
        let release = Arc::new(AtomicBool::new(false));
        let cancellation = AtomicBool::new(false);

        for index in 0..WORKERS {
            let entered_for_work = Arc::clone(&entered);
            let release = Arc::clone(&release);
            let error = run_bounded_rebind_io_on(
                &executor,
                &format!("hung probe {index}"),
                &cancellation,
                Duration::from_millis(15),
                move || {
                    entered_for_work.fetch_add(1, Ordering::SeqCst);
                    let deadline = Instant::now() + Duration::from_secs(2);
                    while !release.load(Ordering::SeqCst) && Instant::now() < deadline {
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    Ok(())
                },
            )
            .unwrap_err();
            assert!(error.to_string().contains("timed out"));
            let deadline = Instant::now() + Duration::from_millis(250);
            while entered.load(Ordering::SeqCst) < (index + 1) as u64 {
                assert!(
                    Instant::now() < deadline,
                    "fixed I/O worker did not enter probe"
                );
                std::thread::sleep(Duration::from_millis(2));
            }
        }

        // Both timed-out calls are still occupying the only workers. Fill the bounded queue;
        // the next request must fail closed instead of spawning another OS thread.
        for index in 0..QUEUE {
            let error = run_bounded_rebind_io_on(
                &executor,
                &format!("queued probe {index}"),
                &cancellation,
                Duration::from_millis(15),
                || Ok(()),
            )
            .unwrap_err();
            assert!(error.to_string().contains("timed out"));
        }
        let saturated = run_bounded_rebind_io_on(
            &executor,
            "overflow probe",
            &cancellation,
            Duration::from_millis(15),
            || Ok(()),
        )
        .unwrap_err();
        assert!(saturated.to_string().contains("capacity is saturated"));
        assert_eq!(
            executor.workers_started(),
            WORKERS as u64,
            "timed-out and saturated probes must not create replacement threads"
        );
        release.store(true, Ordering::SeqCst);
    }

    #[test]
    fn running_bounded_probe_observes_task_cancellation() {
        let ticket = submit_root_rebind_task_cancellable("prepare", move |cancellation| {
            run_bounded_rebind_io(
                "synthetic cancellable NAS probe",
                &cancellation,
                Duration::from_secs(2),
                || {
                    std::thread::sleep(Duration::from_millis(500));
                    Ok(())
                },
            )
        })
        .unwrap();
        std::thread::sleep(Duration::from_millis(30));
        cancel_root_rebind_task(&ticket.task_id).unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            let status = root_rebind_task_status(&ticket.task_id, None).unwrap();
            if status.state == "failed" {
                assert!(status.error.unwrap().contains("canceled"));
                break;
            }
            assert!(
                Instant::now() < deadline,
                "cancellation was not observed promptly"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
