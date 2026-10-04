use super::{open_checkpoint_maintenance_raw, open_readonly_raw, open_runtime_write_raw, open_write_raw};
use crate::paths::AppPaths;
use crate::{EngineError, Result};
use rusqlite::{Connection, ErrorCode, TransactionBehavior};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex, OnceLock, Weak};
use std::thread::ThreadId;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const WRITER_QUEUE_CAPACITY: usize = 64;
pub const WRITER_ADMISSION_TIMEOUT: Duration = Duration::from_secs(5);
pub const READ_EXECUTOR_LIMIT: usize = 4;
pub const READ_ADMISSION_CAPACITY: usize = 64;
pub const READ_ADMISSION_TIMEOUT: Duration = Duration::from_secs(4);
pub const SHUTDOWN_DRAIN_TIMEOUT: Duration = Duration::from_secs(10);
pub const OPERATION_RECEIPT_CAPACITY: usize = 512;
pub const IDEMPOTENT_RETRY_LIMIT: u32 = 3;
/// WP-0324: an operation that made SQLite read at least this many bytes from the database, WAL
/// or journal files emits a `database_heavy_read` trace row naming its call site, so repeated
/// large scans can be attributed to the code that runs them.
pub const HEAVY_READ_TRACE_BYTES: u64 = 16 * 1024 * 1024;
pub const WRITER_BATCH_MAX_OPERATIONS: usize = 1;
pub const LONG_READER_WARNING_MS: u64 = 5_000;
pub const WRITER_FAIRNESS_POLICY: &str = "strict_fifo_no_priority_bypass";
pub const CHECKPOINT_POLICY: &str = "passive_maintenance_only_never_foreground";
pub const CHECKPOINT_INTERVAL: Duration = Duration::from_millis(500);
pub const CHECKPOINT_STALE_TIMEOUT: Duration = Duration::from_secs(10);
pub const CHECKPOINT_BACKLOG_WARNING: u64 = 64 * 1024 * 1024;
pub const CHECKPOINT_BACKLOG_LIMIT: u64 = 256 * 1024 * 1024;
pub const CANCELLATION_POLICY: &str =
    "cancellable_before_admission_then_raii_rollback_or_terminal_commit";

static NEXT_OPERATION_ID: AtomicU64 = AtomicU64::new(1);
static RUNTIMES: OnceLock<Mutex<HashMap<PathBuf, Arc<RuntimeInner>>>> = OnceLock::new();

fn runtime_map() -> &'static Mutex<HashMap<PathBuf, Arc<RuntimeInner>>> {
    RUNTIMES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DatabaseMode {
    Read,
    Write,
    Maintenance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DatabasePriority {
    Foreground,
    Background,
    Maintenance,
}

#[derive(Debug, Clone)]
pub struct DatabaseCancellation {
    cancelled: Arc<AtomicBool>,
}

impl Default for DatabaseCancellation {
    fn default() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl DatabaseCancellation {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

#[derive(Debug, Clone)]
pub struct DatabaseOperationContext {
    pub lane: String,
    pub operation: String,
    pub request_id: Option<String>,
    pub priority: DatabasePriority,
    pub cancellation: DatabaseCancellation,
    pub batch_identity: Option<String>,
}

impl DatabaseOperationContext {
    pub fn new(lane: impl Into<String>, operation: impl Into<String>) -> Self {
        Self {
            lane: lane.into(),
            operation: operation.into(),
            request_id: None,
            priority: DatabasePriority::Background,
            cancellation: DatabaseCancellation::default(),
            batch_identity: None,
        }
    }

    pub fn legacy(mode: DatabaseMode) -> Self {
        let thread = std::thread::current();
        let lane = thread.name().unwrap_or("unnamed").to_string();
        Self {
            lane,
            operation: match mode {
                DatabaseMode::Read => "legacy_read",
                DatabaseMode::Write => "legacy_write",
                DatabaseMode::Maintenance => "legacy_maintenance",
            }
            .to_string(),
            request_id: None,
            priority: DatabasePriority::Background,
            cancellation: DatabaseCancellation::default(),
            batch_identity: None,
        }
    }

    pub fn foreground(mut self) -> Self {
        self.priority = DatabasePriority::Foreground;
        self
    }

    pub fn maintenance(mut self) -> Self {
        self.priority = DatabasePriority::Maintenance;
        self
    }

    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = Some(request_id.into());
        self
    }

    pub fn with_batch_identity(mut self, batch_identity: impl Into<String>) -> Self {
        self.batch_identity = Some(batch_identity.into());
        self
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ActiveDatabaseOperation {
    pub operation_id: u64,
    pub lane: String,
    pub operation: String,
    pub request_id: Option<String>,
    pub mode: DatabaseMode,
    pub priority: DatabasePriority,
    pub enqueued_at_ms: u64,
    pub admitted_at_ms: Option<u64>,
    pub queue_wait_ms: Option<u64>,
    pub worker_id: Option<String>,
    pub batch_identity: Option<String>,
    pub transaction_behavior: Option<String>,
    pub phase_ms: BTreeMap<String, u64>,
    pub row_count: Option<u64>,
    /// Bytes SQLite read from the database/WAL/journal files on the operation's thread while its
    /// context was open (read-counting VFS); recorded when the context closes on that thread.
    pub file_bytes_read: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DatabaseOperationReceipt {
    pub operation_id: u64,
    pub lane: String,
    pub operation: String,
    pub request_id: Option<String>,
    pub mode: DatabaseMode,
    pub priority: DatabasePriority,
    pub enqueued_at_ms: u64,
    pub admitted_at_ms: Option<u64>,
    pub finished_at_ms: u64,
    pub queue_wait_ms: Option<u64>,
    pub execution_ms: Option<u64>,
    pub retry_count: u32,
    pub outcome: String,
    pub batch_identity: Option<String>,
    pub transaction_behavior: Option<String>,
    pub phase_ms: BTreeMap<String, u64>,
    pub row_count: Option<u64>,
    pub file_bytes_read: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DatabaseRuntimeSnapshot {
    pub checkpoint_maintenance: CheckpointMaintenanceHealth,
    pub database_path: PathBuf,
    pub sqlite_version: String,
    pub sqlite_version_number: i32,
    pub sqlite_source_id: String,
    pub writer_capacity: usize,
    pub waiting_writers: usize,
    pub writer_active: bool,
    pub read_executor_limit: usize,
    pub read_admission_capacity: usize,
    pub active_readers: usize,
    pub waiting_readers: usize,
    pub shutting_down: bool,
    pub active_operations: Vec<ActiveDatabaseOperation>,
    pub recent_receipts: Vec<DatabaseOperationReceipt>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WalHealth {
    pub checkpoint_maintenance: CheckpointMaintenanceHealth,
    pub database_path: PathBuf,
    pub wal_path: PathBuf,
    pub wal_bytes: u64,
    pub shm_bytes: u64,
    pub active_readers: usize,
    pub writer_active: bool,
    pub waiting_writers: usize,
    pub oldest_reader_age_ms: Option<u64>,
    pub long_reader_candidates: Vec<ActiveDatabaseOperation>,
    pub last_checkpoint: Option<WalCheckpointReceipt>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WalCheckpointReceipt {
    pub mode: String,
    pub busy: i64,
    pub log_frames: i64,
    pub checkpointed_frames: i64,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DatabaseContentionReceipt {
    pub classification: String,
    pub database_path: PathBuf,
    pub active_internal_candidates: Vec<ActiveDatabaseOperation>,
    pub recent_receipts: Vec<DatabaseOperationReceipt>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckpointMaintenanceHealth {
    pub enabled: bool,
    pub owner_state: String,
    pub last_cycle_at_ms: Option<u64>,
    pub cycle_age_ms: Option<u64>,
    pub consecutive_errors: u32,
    pub last_error: Option<String>,
    pub last_checkpoint: Option<WalCheckpointReceipt>,
    pub backlog_bytes: Option<u64>,
    pub backlog_warning: bool,
    pub writer_backpressure: Option<String>,
    pub physical_wal_bytes: u64,
    pub max_physical_wal_bytes: u64,
    pub cycles: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckpointMaintenanceShutdownReceipt {
    pub stop_requested: bool,
    pub joined: bool,
    pub elapsed_ms: u64,
    pub final_checkpoint: Option<WalCheckpointReceipt>,
    pub owner_close_error: Option<String>,
}

#[derive(Debug)]
struct MaintenanceState {
    ready: bool,
    state: &'static str,
    last_cycle: Instant,
    health: CheckpointMaintenanceHealth,
    final_receipt: Option<CheckpointMaintenanceShutdownReceipt>,
}

#[derive(Debug)]
struct ManualCheckpoint {
    runtime: Weak<RuntimeInner>,
    operation_id: u64,
    started: Instant,
    reply: mpsc::SyncSender<std::result::Result<WalCheckpointReceipt, String>>,
    replied: bool,
}

impl Drop for ManualCheckpoint {
    fn drop(&mut self) {
        if !self.replied {
            let outcome=if std::thread::panicking() {"maintenance_owner_panicked"}else{"maintenance_request_abandoned"};
            if let Some(runtime)=self.runtime.upgrade() {runtime.finish(self.operation_id,self.started,outcome,0);}
            let _=self.reply.try_send(Err(outcome.into()));
        }
    }
}

struct CheckpointCycleTerminal<'a> {
    runtime: &'a RuntimeInner,
    operation_id: u64,
    started: Instant,
    finished: bool,
}

impl Drop for CheckpointCycleTerminal<'_> {
    fn drop(&mut self) {
        if !self.finished {
            self.runtime.finish(self.operation_id,self.started,
                if std::thread::panicking(){"maintenance_owner_panicked"}else{"maintenance_cycle_abandoned"},0);
        }
    }
}

#[derive(Debug)]
struct MaintenanceShared {
    state: Mutex<MaintenanceState>,
    wake: Condvar,
    requests: Mutex<VecDeque<ManualCheckpoint>>,
    stop: AtomicBool,
    guards: AtomicUsize,
}

#[derive(Debug)]
struct MaintenanceOwner {
    shared: Arc<MaintenanceShared>,
    handle: Option<std::thread::JoinHandle<()>>,
}

#[derive(Debug)]
pub struct CheckpointMaintenanceGuard {
    runtime: Weak<RuntimeInner>,
    shared: Arc<MaintenanceShared>,
}

fn empty_maintenance_health() -> CheckpointMaintenanceHealth {
    CheckpointMaintenanceHealth { enabled:false,owner_state:"not_started".into(),last_cycle_at_ms:None,
        cycle_age_ms:None,consecutive_errors:0,last_error:None,last_checkpoint:None,backlog_bytes:None,
        backlog_warning:false,writer_backpressure:None,physical_wal_bytes:0,max_physical_wal_bytes:0,cycles:0 }
}

fn checkpoint_backlog(log: i64, checkpointed: i64, page_size: u64) -> Option<u64> {
    if log == -1 && checkpointed == -1 { return Some(0); }
    if log < 0 || checkpointed < 0 || checkpointed > log || page_size == 0 { return None; }
    Some((log as u64 - checkpointed as u64).saturating_mul(page_size))
}

fn maintenance_gate(state: &MaintenanceState) -> Option<&'static str> {
    if !state.ready || state.state != "running" || state.last_cycle.elapsed() >= CHECKPOINT_STALE_TIMEOUT
        || state.health.consecutive_errors >= 3 { return Some("maintenance_unavailable"); }
    if state.health.backlog_bytes.is_some_and(|bytes|bytes >= CHECKPOINT_BACKLOG_LIMIT) {
        return Some("maintenance_backlog_limit");
    }
    None
}

fn reject_manual_checkpoints(shared: &MaintenanceShared, reason: &str) {
    let requests: Vec<_> = shared.requests.lock().unwrap_or_else(|p|p.into_inner()).drain(..).collect();
    for mut request in requests {
        if let Some(runtime)=request.runtime.upgrade() {
            runtime.finish(request.operation_id,request.started,reason,0);
        }
        let _=request.reply.send(Err(reason.into()));
        request.replied=true;
    }
}

fn maintenance_cycle(runtime: &RuntimeInner, shared: &MaintenanceShared, connection: &Connection,
    page_size: u64, operation_id: u64, started: Instant) -> Result<WalCheckpointReceipt> {
    let mut terminal=CheckpointCycleTerminal{runtime,operation_id,started,finished:false};
    runtime.admitted(operation_id,started.elapsed());
    #[cfg(test)]
    {
        let manual=runtime.registry.lock().unwrap_or_else(|p|p.into_inner()).active.get(&operation_id)
            .is_some_and(|operation|operation.operation=="wal_checkpoint_passive");
        if manual && runtime.test_manual_checkpoint_panic.swap(false,Ordering::AcqRel) {
            panic!("owned manual checkpoint panic fixture");
        }
    }
    let result=connection.query_row("PRAGMA wal_checkpoint(PASSIVE)",[],|row| {
        Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?,row.get::<_,i64>(2)?))
    });
    runtime.operation_metadata(operation_id,Some(("checkpoint_sql",started.elapsed())),None,None);
    let physical=std::fs::metadata(runtime.database_path.with_extension("sqlite-wal")).map(|m|m.len()).unwrap_or(0);
    let mut state=shared.state.lock().unwrap_or_else(|p|p.into_inner());
    state.last_cycle=Instant::now();
    state.health.last_cycle_at_ms=Some(now_ms());
    state.health.cycles=state.health.cycles.saturating_add(1);
    state.health.physical_wal_bytes=physical;
    state.health.max_physical_wal_bytes=state.health.max_physical_wal_bytes.max(physical);
    match result {
        Ok((busy,log_frames,checkpointed_frames))=> {
            let receipt=WalCheckpointReceipt{mode:"passive".into(),busy,log_frames,checkpointed_frames,
                elapsed_ms:started.elapsed().as_millis().min(u64::MAX as u128) as u64};
            // A busy or malformed no-WAL result must never erase a known backlog.
            let backlog=if busy!=0 && (log_frames<0 || checkpointed_frames<0) {None}
                else {checkpoint_backlog(log_frames,checkpointed_frames,page_size)};
            if let Some(backlog)=backlog {state.health.backlog_bytes=Some(backlog);}
            state.health.backlog_warning=state.health.backlog_bytes.is_some_and(|b|b>=CHECKPOINT_BACKLOG_WARNING);
            state.health.consecutive_errors=0;
            state.health.last_error=None;
            state.health.last_checkpoint=Some(receipt.clone());
            let warning=state.health.backlog_warning;
            let backlog_bytes=state.health.backlog_bytes;
            drop(state);
            *runtime.last_checkpoint.lock().unwrap_or_else(|p|p.into_inner())=Some(receipt.clone());
            runtime.finish(operation_id,started,if busy!=0 {"checkpoint_busy"}
                else if log_frames>checkpointed_frames {"checkpoint_partial"}else{"checkpoint_completed"},0);
            terminal.finished=true;
            runtime.admission_changed.notify_all();
            if warning || busy!=0 || log_frames>checkpointed_frames {
                crate::diagnostics::emit_trace_event(&runtime.paths,"database_checkpoint_pressure","warn",
                    serde_json::json!({"operation_id":operation_id,"busy":busy,"log_frames":log_frames,
                        "checkpointed_frames":checkpointed_frames,"backlog_bytes":backlog_bytes,"physical_wal_bytes":physical}));
            }
            Ok(receipt)
        }
        Err(error)=> {
            state.health.consecutive_errors=state.health.consecutive_errors.saturating_add(1);
            state.health.last_error=Some(error.to_string());
            let consecutive_errors=state.health.consecutive_errors;
            drop(state);
            runtime.finish(operation_id,started,"checkpoint_failed",0);
            terminal.finished=true;
            runtime.admission_changed.notify_all();
            crate::diagnostics::emit_trace_event(&runtime.paths,"database_checkpoint_failed","warn",
                serde_json::json!({"operation_id":operation_id,"consecutive_errors":consecutive_errors,
                    "error":error.to_string().chars().take(4096).collect::<String>()}));
            Err(error.into())
        }
    }
}

fn maintenance_worker(weak: Weak<RuntimeInner>, shared: &MaintenanceShared) -> Result<CheckpointMaintenanceShutdownReceipt> {
    let runtime=weak.upgrade().ok_or_else(||EngineError::DatabaseRuntime("maintenance_runtime_gone".into()))?;
    let connection=open_checkpoint_maintenance_raw(&runtime.database_path)?;
    let page_size=connection.query_row("PRAGMA page_size",[],|row|row.get::<_,u64>(0))?;
    {
        let mut state=shared.state.lock().unwrap_or_else(|p|p.into_inner());
        state.ready=true;
        state.state="running";
        state.last_cycle=Instant::now();
    }
    shared.wake.notify_all();
    let mut next=Instant::now();
    while !shared.stop.load(Ordering::Acquire) {
        let request=shared.requests.lock().unwrap_or_else(|p|p.into_inner()).pop_front();
        if let Some(mut request)=request {
            let result=maintenance_cycle(&runtime,shared,&connection,page_size,request.operation_id,request.started);
            let _=request.reply.send(result.map_err(|error|error.to_string()));
            request.replied=true;
        } else if Instant::now()>=next {
            let (id,started)=runtime.register(&DatabaseOperationContext::new("database_maintenance","wal_checkpoint_background").maintenance(),DatabaseMode::Maintenance);
            let _=maintenance_cycle(&runtime,shared,&connection,page_size,id,started);
            next=Instant::now()+CHECKPOINT_INTERVAL;
        } else {
            let state=shared.state.lock().unwrap_or_else(|p|p.into_inner());
            if shared.stop.load(Ordering::Acquire) {break;}
            // Bounded wait also covers a notification arriving immediately before wait.
            drop(shared.wake.wait_timeout(state,next.saturating_duration_since(Instant::now()).min(Duration::from_millis(50)))
                .unwrap_or_else(|p|p.into_inner()));
        }
    }
    reject_manual_checkpoints(shared,"maintenance_stopped_before_admission");
    // RAII setup-error cleanup also waits for already admitted atomic operations.
    let mut admission=runtime.admission.lock().unwrap_or_else(|p|p.into_inner());
    while admission.writer_active || admission.active_readers>0 || !admission.waiting_writers.is_empty() || !admission.waiting_readers.is_empty() {
        admission=runtime.admission_changed.wait_timeout(admission,Duration::from_millis(50)).unwrap_or_else(|p|p.into_inner()).0;
    }
    drop(admission);
    let (id,started)=runtime.register(&DatabaseOperationContext::new("database_maintenance","wal_checkpoint_shutdown").maintenance(),DatabaseMode::Maintenance);
    let final_checkpoint=maintenance_cycle(&runtime,shared,&connection,page_size,id,started).ok();
    let owner_close_error=connection.close().err().map(|(_,error)|error.to_string());
    Ok(CheckpointMaintenanceShutdownReceipt{stop_requested:true,joined:false,elapsed_ms:0,final_checkpoint,owner_close_error})
}

impl MaintenanceShared {
    fn signal_stop(&self) {
        self.stop.store(true, Ordering::Release);
        self.wake.notify_all();
    }
}

impl Drop for CheckpointMaintenanceGuard {
    fn drop(&mut self) {
        if self.shared.guards.fetch_sub(1, Ordering::AcqRel) == 1 {
            if let Some(runtime) = self.runtime.upgrade() {
                runtime.admission.lock().unwrap_or_else(|p|p.into_inner()).shutting_down = true;
                runtime.admission_changed.notify_all();
            }
            self.shared.signal_stop();
        }
    }
}

impl CheckpointMaintenanceGuard {
    pub fn stop_and_join(&mut self, timeout: Duration) -> Result<CheckpointMaintenanceShutdownReceipt> {
        let started=Instant::now();
        let deadline=started+timeout;
        let runtime=self.runtime.upgrade().ok_or_else(||EngineError::DatabaseRuntime("maintenance_runtime_gone".into()))?;
        let mut admission=runtime.admission.lock().unwrap_or_else(|p|p.into_inner());
        admission.shutting_down=true;
        runtime.admission_changed.notify_all();
        while admission.writer_active || admission.active_readers>0 || !admission.waiting_writers.is_empty()
            || !admission.waiting_readers.is_empty() {
            if Instant::now()>=deadline {
                drop(admission);
                self.shared.signal_stop();
                return Err(EngineError::DatabaseRuntime("maintenance_quiesce_timeout_unjoined".into()));
            }
            admission=runtime.admission_changed.wait_timeout(admission,
                deadline.saturating_duration_since(Instant::now()).min(Duration::from_millis(50)))
                .unwrap_or_else(|p|p.into_inner()).0;
        }
        drop(admission);
        self.shared.signal_stop();
        let mut state=self.shared.state.lock().unwrap_or_else(|p|p.into_inner());
        while state.final_receipt.is_none() {
            if Instant::now()>=deadline {
                return Err(EngineError::DatabaseRuntime("maintenance_stop_timeout_unjoined_nonpreemptible_io".into()));
            }
            state=self.shared.wake.wait_timeout(state,deadline.saturating_duration_since(Instant::now()))
                .unwrap_or_else(|p|p.into_inner()).0;
        }
        let mut receipt=state.final_receipt.clone().expect("terminal owner receipt");
        drop(state);
        loop {
            let finished=runtime.maintenance.lock().unwrap_or_else(|p|p.into_inner()).as_ref()
                .and_then(|owner|owner.handle.as_ref()).is_none_or(|handle|handle.is_finished());
            if finished {break;}
            if Instant::now()>=deadline {
                return Err(EngineError::DatabaseRuntime("maintenance_stop_timeout_unjoined_nonpreemptible_io".into()));
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        let handle=runtime.maintenance.lock().unwrap_or_else(|p|p.into_inner()).as_mut().and_then(|o|o.handle.take());
        if let Some(handle)=handle {
            if handle.thread().id()==std::thread::current().id() {
                return Err(EngineError::DatabaseRuntime("maintenance_self_join_refused".into()));
            }
            if handle.join().is_err() { return Err(EngineError::DatabaseRuntime("maintenance_worker_panicked".into())); }
        }
        receipt.joined=true;
        receipt.elapsed_ms=started.elapsed().as_millis().min(u64::MAX as u128) as u64;
        Ok(receipt)
    }
}

#[derive(Debug)]
struct WaitingWriter {
    ticket: u64,
    operation_id: u64,
}

#[derive(Debug)]
struct WaitingReader {
    ticket: u64,
    operation_id: u64,
}

#[derive(Debug, Default)]
struct AdmissionState {
    next_ticket: u64,
    next_reader_ticket: u64,
    waiting_writers: VecDeque<WaitingWriter>,
    writer_active: bool,
    writer_thread: Option<ThreadId>,
    active_readers: usize,
    waiting_readers: VecDeque<WaitingReader>,
    shutting_down: bool,
}

#[derive(Debug, Default)]
struct RegistryState {
    active: HashMap<u64, ActiveDatabaseOperation>,
    receipts: VecDeque<DatabaseOperationReceipt>,
}

#[cfg(feature = "wp0333_connection_reuse_proof")]
#[derive(Debug, Default, Clone, Serialize)]
pub struct ConnectionReuseProof {
    pub enabled: bool,
    pub physical_writer_opens: u64,
    pub physical_reader_opens: u64,
    pub max_writer_owners: usize,
    pub max_reader_owners: usize,
    pub lease_returns: u64,
    pub quarantines: u64,
    pub physical_closes: u64,
    pub close_errors: u64,
    pub remaining_owners: usize,
    pub writer_owners: usize,
    pub reader_owners: usize,
    pub shutdown_joined: bool,
    pub physical_close_receipts: Vec<serde_json::Value>,
}

#[cfg(feature = "wp0333_connection_reuse_proof")]
#[derive(Debug)]
pub struct ConnectionReuseGuard { runtime: Arc<RuntimeInner> }

#[cfg(feature = "wp0333_connection_reuse_proof")]
impl Drop for ConnectionReuseGuard {
    fn drop(&mut self) {
        let joined=self.runtime.reuse.lock().unwrap_or_else(|p|p.into_inner()).as_ref().is_some_and(|p|p.proof.shutdown_joined);
        if joined {return;}
        let deadline=Instant::now()+SHUTDOWN_DRAIN_TIMEOUT;
        let result=(||->Result<()> {
            if let Some(shared)=self.runtime.maintenance_shared() {
                shared.guards.fetch_add(1,Ordering::AcqRel);
                let mut owner=CheckpointMaintenanceGuard{runtime:Arc::downgrade(&self.runtime),shared};
                owner.stop_and_join(deadline.saturating_duration_since(Instant::now()))?;
            }
            AppDatabase{inner:Arc::clone(&self.runtime)}.shutdown_and_drain(deadline.saturating_duration_since(Instant::now()))
        })();
        if let Err(error)=result {
            crate::diagnostics::emit_trace_event(&self.runtime.paths,"database_reuse_raii_cleanup_failed","error",serde_json::json!({"error":error.to_string(),"proof":AppDatabase{inner:Arc::clone(&self.runtime)}.connection_reuse_proof()}));
        }
    }
}

#[cfg(feature = "wp0333_connection_reuse_proof")]
#[derive(Debug, Default)]
struct ConnectionReuse {
    writer: Option<Connection>,
    readers: Vec<Connection>,
    poisoned: Vec<(Connection, bool)>,
    pending_close: Vec<(Connection, bool)>,
    proof: ConnectionReuseProof,
    closing: bool,
    closer_starting: bool,
    ownership_unreconciled: bool,
    closer: Option<std::thread::JoinHandle<()>>,
    close_result: Option<std::result::Result<(), String>>,
    session_baselines: HashMap<usize, BTreeMap<String,i64>>,
    #[cfg(test)]
    fail_reset: bool,
    #[cfg(test)]
    close_delay: Duration,
    #[cfg(test)]
    close_panic: bool,
}

#[cfg(feature = "wp0333_connection_reuse_proof")]
struct ReuseCloseOwner<'a> {
    runtime: &'a RuntimeInner,
    connection: Option<Connection>,
    write: bool,
    operation_id: u64,
    started: Instant,
    finished: bool,
}

#[cfg(feature = "wp0333_connection_reuse_proof")]
impl Drop for ReuseCloseOwner<'_> {
    fn drop(&mut self) {
        if self.finished {return;}
        let outcome=if let Some(connection)=self.connection.take() {
            self.runtime.reuse.lock().unwrap_or_else(|p|p.into_inner()).as_mut().unwrap().poisoned.push((connection,self.write));
            "reuse_close_panicked_owner_retained"
        } else {
            // Consuming native close did not return: ownership cannot be asserted.
            // Keep the outstanding count and fail closed rather than reporting a drain.
            self.runtime.reuse.lock().unwrap_or_else(|p|p.into_inner()).as_mut().unwrap().ownership_unreconciled=true;
            "reuse_close_panicked_ownership_unreconciled"
        };
        self.runtime.reuse.lock().unwrap_or_else(|p|p.into_inner()).as_mut().unwrap().proof.close_errors+=1;
        self.runtime.finish(self.operation_id,self.started,outcome,0);
    }
}

#[cfg(feature = "wp0333_connection_reuse_proof")]
fn reset_reusable(connection: &Connection, write: bool, maintained: bool, fresh_foreign_keys: bool) -> Result<bool> {
    let rolled_back = !connection.is_autocommit();
    if rolled_back { connection.execute_batch("ROLLBACK")?; }
    if !connection.is_autocommit() { return Err(EngineError::DatabaseRuntime("reuse_transaction_not_reset".into())); }
    connection.flush_prepared_statement_cache();
    if unsafe {!rusqlite::ffi::sqlite3_next_stmt(connection.handle(),std::ptr::null_mut()).is_null()} {
        return Err(EngineError::DatabaseRuntime("reuse_outstanding_statement_refused".into()));
    }
    let attached: Vec<String> = connection.prepare("PRAGMA database_list")?
        .query_map([], |r|r.get(1))?.collect::<rusqlite::Result<_>>()?;
    if attached.iter().any(|name|name != "main") {
        return Err(EngineError::DatabaseRuntime("reuse_session_attachment_or_temp_refused".into()));
    }
    connection.busy_timeout(if write {Duration::from_secs(10)} else {Duration::from_millis(super::READ_ONLY_BUSY_TIMEOUT_MS)})?;
    connection.pragma_update(None,"query_only",!write)?;
    connection.pragma_update(None,"foreign_keys",if write {true} else {fresh_foreign_keys})?;
    connection.set_db_config(rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE,true)?;
    if write && maintained { super::configure_maintained_writer(connection)?; }
    let query_only: bool=connection.pragma_query_value(None,"query_only",|r|r.get(0))?;
    let foreign_keys: bool=connection.pragma_query_value(None,"foreign_keys",|r|r.get(0))?;
    let busy: u64=connection.pragma_query_value(None,"busy_timeout",|r|r.get(0))?;
    let wal: String=connection.pragma_query_value(None,"journal_mode",|r|r.get(0))?;
    if query_only == write || (write && !foreign_keys) || (!write && foreign_keys!=fresh_foreign_keys)
        || busy != if write {10000} else {super::READ_ONLY_BUSY_TIMEOUT_MS}
        || !wal.eq_ignore_ascii_case("wal")
        || !connection.db_config(rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE)? {
        return Err(EngineError::DatabaseRuntime("reuse_role_policy_readback_failed".into()));
    }
    Ok(rolled_back)
}

#[derive(Debug)]
struct RuntimeInner {
    #[cfg(feature = "wp0333_connection_reuse_proof")]
    reuse: Mutex<Option<ConnectionReuse>>,
    #[cfg(test)]
    test_manual_checkpoint_panic: AtomicBool,
    #[cfg(test)]
    test_maintenance_unavailable: AtomicBool,
    maintenance: Mutex<Option<MaintenanceOwner>>,
    database_path: PathBuf,
    /// Paths of the first opener; used only to address the diagnostics trace.
    paths: AppPaths,
    admission: Mutex<AdmissionState>,
    admission_changed: Condvar,
    registry: Mutex<RegistryState>,
    last_checkpoint: Mutex<Option<WalCheckpointReceipt>>,
}

impl RuntimeInner {
    #[cfg(feature = "wp0333_connection_reuse_proof")]
    fn reuse_enabled(&self) -> bool { self.reuse.lock().unwrap_or_else(|p|p.into_inner()).is_some() }

    #[cfg(feature = "wp0333_connection_reuse_proof")]
    fn reuse_checkout(&self, write: bool) -> Result<Option<Connection>> {
        if !self.maintained_writer_policy() {return Err(EngineError::DatabaseRuntime("reuse_requires_ready_maintenance".into()));}
        let mut pool=self.reuse.lock().unwrap_or_else(|p|p.into_inner());
        let Some(pool)=pool.as_mut() else {return Ok(None)};
        if pool.ownership_unreconciled {return Err(EngineError::DatabaseRuntime("reuse_ownership_unreconciled".into()));}
        if pool.closing || !pool.poisoned.is_empty() {return Err(EngineError::DatabaseRuntime("reuse_owner_poisoned_or_closing".into()));}
        Ok(if write {pool.writer.take()} else {pool.readers.pop()})
    }

    #[cfg(feature = "wp0333_connection_reuse_proof")]
    fn reuse_opened(&self, write: bool, connection: &Connection) -> Result<()> {
        let mut baseline=BTreeMap::new();
        for name in ["foreign_keys","reverse_unordered_selects","read_uncommitted","recursive_triggers","ignore_check_constraints","defer_foreign_keys"] {
            baseline.insert(name.to_string(),connection.pragma_query_value(None,name,|r|r.get(0))?);
        }
        let mut pool=self.reuse.lock().unwrap_or_else(|p|p.into_inner());
        if let Some(pool)=pool.as_mut() {
            pool.session_baselines.insert(unsafe {connection.handle() as usize},baseline);
            if write {pool.proof.physical_writer_opens+=1;} else {pool.proof.physical_reader_opens+=1;}
            pool.proof.remaining_owners+=1;
            if write {pool.proof.writer_owners+=1;pool.proof.max_writer_owners=pool.proof.max_writer_owners.max(pool.proof.writer_owners);}
            else {pool.proof.reader_owners+=1;pool.proof.max_reader_owners=pool.proof.max_reader_owners.max(pool.proof.reader_owners);}
        }
        Ok(())
    }

    #[cfg(feature = "wp0333_connection_reuse_proof")]
    fn reuse_return(&self, connection: Connection, write: bool, operation_id: u64) -> bool {
        let started=Instant::now();
        let had_transaction=!connection.is_autocommit();
        #[cfg(test)]
        let forced=self.reuse.lock().unwrap_or_else(|p|p.into_inner()).as_ref().is_some_and(|p|p.fail_reset);
        #[cfg(not(test))]
        let forced=false;
        let result=if std::thread::panicking() || forced {Err(EngineError::DatabaseRuntime("reuse_panicked_or_reset_failed".into()))}
            else {(||->Result<bool> {
                let baseline=self.reuse.lock().unwrap_or_else(|p|p.into_inner()).as_ref().unwrap().session_baselines
                    .get(&(unsafe {connection.handle() as usize})).cloned().ok_or_else(||EngineError::DatabaseRuntime("reuse_missing_session_baseline".into()))?;
                let fresh_foreign_keys=*baseline.get("foreign_keys").ok_or_else(||EngineError::DatabaseRuntime("reuse_missing_foreign_keys_baseline".into()))?!=0;
                let rolled_back=reset_reusable(&connection,write,self.maintained_writer_policy(),fresh_foreign_keys)?;
                for (name,value) in baseline {
                    let actual:i64=connection.pragma_query_value(None,&name,|r|r.get(0))?;
                    if actual!=value {return Err(EngineError::DatabaseRuntime(format!("reuse_session_policy_changed:{name}")));}
                }
                Ok(rolled_back)
            })()};
        self.operation_metadata(operation_id,Some(("lease_return",started.elapsed())),None,None);
        if let Err(error)=result {
            let (id,time)=self.register(&DatabaseOperationContext::new("database_reuse","lease_cleanup_failed").maintenance(),DatabaseMode::Maintenance);
            self.admitted(id,Duration::ZERO);
            crate::diagnostics::emit_trace_event(&self.paths,"database_reuse_cleanup_failed","warn",
                serde_json::json!({"operation_id":operation_id,"cleanup_operation_id":id,"error":error.to_string(),"write":write}));
            self.finish(id,time,"reuse_cleanup_failed",0);
            {let mut p=self.reuse.lock().unwrap_or_else(|p|p.into_inner());p.as_mut().unwrap().proof.quarantines+=1;}
            self.reuse_close(connection,write,"quarantine_physical_close");
        } else {
            let mut pool=self.reuse.lock().unwrap_or_else(|p|p.into_inner());let pool=pool.as_mut().unwrap();
            pool.proof.lease_returns+=1;
            if write {pool.writer=Some(connection);} else {pool.readers.push(connection);}
        }
        had_transaction
    }

    #[cfg(feature = "wp0333_connection_reuse_proof")]
    fn reuse_close(&self, connection: Connection, write: bool, operation: &str) {
        let (id,time)=self.register(&DatabaseOperationContext::new("database_reuse",operation).maintenance(),DatabaseMode::Maintenance);
        self.admitted(id,Duration::ZERO);
        let mut owner=ReuseCloseOwner{runtime:self,connection:Some(connection),write,operation_id:id,started:time,finished:false};
        #[cfg(test)]
        if self.reuse.lock().unwrap_or_else(|p|p.into_inner()).as_ref().unwrap().close_panic {panic!("controlled reuse close panic");}
        let connection=owner.connection.as_ref().unwrap();
        let owner_id=unsafe {connection.handle() as usize};
        AppDatabase::begin_vfs_timing_probe();
        connection.flush_prepared_statement_cache();
        let outcome=match owner.connection.take().unwrap().close() {
            Ok(())=> {let mut p=self.reuse.lock().unwrap_or_else(|p|p.into_inner());let p=p.as_mut().unwrap();p.session_baselines.remove(&owner_id);p.proof.physical_closes+=1;p.proof.remaining_owners=p.proof.remaining_owners.saturating_sub(1);if write {p.proof.writer_owners=p.proof.writer_owners.saturating_sub(1);}else{p.proof.reader_owners=p.proof.reader_owners.saturating_sub(1);} "reuse_physical_closed"},
            Err((connection,error))=> {
                {let mut p=self.reuse.lock().unwrap_or_else(|p|p.into_inner());let p=p.as_mut().unwrap();p.proof.close_errors+=1;p.poisoned.push((connection,write));}
                crate::diagnostics::emit_trace_event(&self.paths,"database_reuse_close_failed","error",serde_json::json!({"operation_id":id,"error":error.to_string()}));
                "reuse_physical_close_failed"
            }
        };
        let mut native=AppDatabase::finish_vfs_timing_probe().to_vec();
        native.extend(AppDatabase::vfs_open_read_timing_probe());
        {let mut p=self.reuse.lock().unwrap_or_else(|p|p.into_inner());let p=p.as_mut().unwrap();
            if p.proof.physical_close_receipts.len()<32 {p.proof.physical_close_receipts.push(serde_json::json!({"operation_id":id,"owner_id":owner_id,"role":if write {"writer"}else{"reader"},"operation":operation,"outcome":outcome,"elapsed_ms":time.elapsed().as_millis() as u64,"native_callbacks":native}));}
        }
        self.operation_metadata(id,Some(("connection_close",time.elapsed())),None,None);
        self.finish(id,time,outcome,0);
        owner.finished=true;
    }

    #[cfg(feature = "wp0333_connection_reuse_proof")]
    fn shutdown_reuse(self: &Arc<Self>, deadline: Instant) -> Result<()> {
        let owners={
            let mut p=self.reuse.lock().unwrap_or_else(|p|p.into_inner());let Some(p)=p.as_mut() else {return Ok(())};
            if p.ownership_unreconciled {return Err(EngineError::DatabaseRuntime("reuse_ownership_unreconciled".into()));}
            if p.closer_starting {return Err(EngineError::DatabaseRuntime("reuse_close_start_in_progress".into()));}
            if p.closer.is_some() {false} else {
            let start=
            if !p.closing {
                p.closing=true;
                let mut owners: Vec<_>=std::mem::take(&mut p.readers).into_iter().map(|c|(c,false)).collect();owners.extend(p.writer.take().map(|c|(c,true)));owners.extend(std::mem::take(&mut p.poisoned));p.pending_close=owners;true
            } else if p.closer.is_none() && p.proof.remaining_owners>0 {
                p.pending_close.extend(std::mem::take(&mut p.poisoned));p.close_result=None;true
            } else {p.close_result.is_none()};
            if start {p.closer_starting=true;}
            start
            }
        };
        if owners {
            let runtime=Arc::clone(self);
            let handle=std::thread::Builder::new().name("wp0333-reuse-close".into()).spawn(move|| {
                #[cfg(test)]
                {let delay=runtime.reuse.lock().unwrap_or_else(|p|p.into_inner()).as_ref().unwrap().close_delay;std::thread::sleep(delay);}
                let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    loop {
                        let next=runtime.reuse.lock().unwrap_or_else(|p|p.into_inner()).as_mut().unwrap().pending_close.pop();
                        let Some((owner,write))=next else {break};
                        runtime.reuse_close(owner,write,"shutdown_physical_close");
                    }
                })).map_err(|_|"reuse_closer_panicked".to_string());
                let mut p=runtime.reuse.lock().unwrap_or_else(|p|p.into_inner());p.as_mut().unwrap().close_result=Some(result);
            });
            let mut p=self.reuse.lock().unwrap_or_else(|p|p.into_inner());let p=p.as_mut().unwrap();p.closer_starting=false;
            match handle {Ok(handle)=>p.closer=Some(handle),Err(error)=>return Err(error.into())}
        }
        loop {
            let finished=self.reuse.lock().unwrap_or_else(|p|p.into_inner()).as_ref().unwrap().closer.as_ref().is_none_or(|h|h.is_finished());
            if finished {break;}
            if Instant::now()>=deadline {return Err(EngineError::DatabaseRuntime("reuse_close_timeout_unjoined_nonpreemptible_io".into()));}
            std::thread::sleep(deadline.saturating_duration_since(Instant::now()).min(Duration::from_millis(10)));
        }
        let handle={let mut p=self.reuse.lock().unwrap_or_else(|p|p.into_inner());let p=p.as_mut().unwrap();
            if p.closer_starting {return Err(EngineError::DatabaseRuntime("reuse_close_join_in_progress".into()));}
            p.closer_starting=true;p.closer.take()};
        let join_failed=handle.is_some_and(|h|h.join().is_err());
        let mut p=self.reuse.lock().unwrap_or_else(|p|p.into_inner());let p=p.as_mut().unwrap();
        p.closer_starting=false;
        if join_failed {return Err(EngineError::DatabaseRuntime("reuse_closer_panicked".into()));}
        if p.ownership_unreconciled || p.close_result.as_ref().is_some_and(|r|r.is_err()) || p.proof.remaining_owners!=0 {
            return Err(EngineError::DatabaseRuntime("reuse_shutdown_owners_not_closed".into()));
        }
        p.proof.shutdown_joined=true;Ok(())
    }

    fn maintenance_shared(&self) -> Option<Arc<MaintenanceShared>> {
        self.maintenance.lock().unwrap_or_else(|p|p.into_inner()).as_ref().map(|o|Arc::clone(&o.shared))
    }

    fn maintenance_health(&self) -> CheckpointMaintenanceHealth {
        let Some(shared)=self.maintenance_shared() else { return empty_maintenance_health(); };
        let state=shared.state.lock().unwrap_or_else(|p|p.into_inner());
        let mut health=state.health.clone();
        health.owner_state=state.state.into();
        health.cycle_age_ms=Some(state.last_cycle.elapsed().as_millis().min(u64::MAX as u128) as u64);
        health.writer_backpressure=maintenance_gate(&state).map(str::to_string);
        if shared.stop.load(Ordering::Acquire) {health.writer_backpressure=Some("maintenance_unavailable".into());}
        #[cfg(test)]
        if self.test_maintenance_unavailable.load(Ordering::Acquire) {health.writer_backpressure=Some("maintenance_unavailable".into());}
        health
    }

    fn maintenance_writer_gate(&self) -> Option<&'static str> {
        #[cfg(test)]
        if self.test_maintenance_unavailable.load(Ordering::Acquire) {return Some("maintenance_unavailable");}
        let shared=self.maintenance_shared()?;
        if shared.stop.load(Ordering::Acquire) {return Some("maintenance_unavailable");}
        let state=shared.state.lock().unwrap_or_else(|p|p.into_inner());
        maintenance_gate(&state)
    }

    fn maintained_writer_policy(&self) -> bool {
        self.maintenance_shared().is_some_and(|s|s.state.lock().unwrap_or_else(|p|p.into_inner()).ready)
    }
    fn register(&self, context: &DatabaseOperationContext, mode: DatabaseMode) -> (u64, Instant) {
        let operation_id = NEXT_OPERATION_ID.fetch_add(1, Ordering::Relaxed);
        let enqueued_at_ms = now_ms();
        let operation = ActiveDatabaseOperation {
            operation_id,
            lane: context.lane.clone(),
            operation: context.operation.clone(),
            request_id: context.request_id.clone(),
            mode,
            priority: context.priority,
            enqueued_at_ms,
            admitted_at_ms: None,
            queue_wait_ms: None,
            worker_id: None,
            batch_identity: context.batch_identity.clone(),
            transaction_behavior: None,
            phase_ms: BTreeMap::new(),
            row_count: None,
            file_bytes_read: None,
        };
        self.registry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .active
            .insert(operation_id, operation);
        if context.request_id.is_some() {
            crate::diagnostics::emit_trace_event(&self.paths, "database_request_started", "info", serde_json::json!({
                "operation_id": operation_id, "request_id": context.request_id,
                "operation": context.operation, "mode": mode, "enqueued_at_ms": enqueued_at_ms,
            }));
        }
        (operation_id, Instant::now())
    }

    fn admitted(&self, operation_id: u64, wait: Duration) {
        if let Some(operation) = self
            .registry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .active
            .get_mut(&operation_id)
        {
            operation.admitted_at_ms = Some(now_ms());
            operation.queue_wait_ms = Some(wait.as_millis().min(u64::MAX as u128) as u64);
            operation.worker_id = Some(format!("{:?}", std::thread::current().id()));
        }
    }

    fn operation_metadata(
        &self,
        operation_id: u64,
        phase: Option<(&str, Duration)>,
        transaction_behavior: Option<&str>,
        row_count: Option<u64>,
    ) {
        if let Some(operation) = self
            .registry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .active
            .get_mut(&operation_id)
        {
            if let Some((name, duration)) = phase {
                operation.phase_ms.insert(
                    name.to_string(),
                    duration.as_millis().min(u64::MAX as u128) as u64,
                );
            }
            if let Some(behavior) = transaction_behavior {
                operation.transaction_behavior = Some(behavior.to_string());
            }
            if row_count.is_some() {
                operation.row_count = row_count;
            }
        }
    }

    fn finish(
        &self,
        operation_id: u64,
        started: Instant,
        outcome: impl Into<String>,
        retry_count: u32,
    ) {
        let mut registry = self
            .registry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(operation) = registry.active.remove(&operation_id) else {
            return;
        };
        let finished_at_ms = now_ms();
        let elapsed_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
        let execution_ms = operation
            .admitted_at_ms
            .map(|_| elapsed_ms.saturating_sub(operation.queue_wait_ms.unwrap_or(0)));
        let outcome: String = outcome.into();
        let request_trace = operation.request_id.as_ref().map(|_| serde_json::json!({
            "operation_id": operation_id, "request_id": operation.request_id,
            "operation": operation.operation, "mode": operation.mode,
            "enqueued_at_ms": operation.enqueued_at_ms, "admitted_at_ms": operation.admitted_at_ms,
            "finished_at_ms": finished_at_ms, "queue_wait_ms": operation.queue_wait_ms,
            "execution_ms": execution_ms, "phase_ms": operation.phase_ms, "outcome": outcome,
        }));
        let admission_failure = outcome.contains("admission_timeout").then(|| serde_json::json!({
            "operation_id": operation_id, "operation": operation.operation, "mode": operation.mode,
            "outcome": outcome, "elapsed_ms": elapsed_ms,
            "admitted_candidates": registry.active.values().filter(|candidate| candidate.admitted_at_ms.is_some())
                .take(16).map(|candidate| serde_json::json!({
                    "operation_id": candidate.operation_id, "operation": candidate.operation,
                    "mode": candidate.mode, "admitted_at_ms": candidate.admitted_at_ms,
                    "queue_wait_ms": candidate.queue_wait_ms, "phase_ms": candidate.phase_ms,
                    "execution_elapsed_ms": candidate.admitted_at_ms.map(|at| finished_at_ms.saturating_sub(at)),
                })).collect::<Vec<_>>(),
        }));
        let heavy_read = operation
            .file_bytes_read
            .filter(|bytes| *bytes >= HEAVY_READ_TRACE_BYTES)
            .map(|bytes| {
                serde_json::json!({
                    "operation_id": operation_id,
                    "operation": operation.operation,
                    "lane": operation.lane,
                    "mode": operation.mode,
                    "priority": operation.priority,
                    "file_bytes_read": bytes,
                    "execution_ms": execution_ms,
                    "row_count": operation.row_count,
                    "outcome": outcome,
                })
            });
        registry.receipts.push_back(DatabaseOperationReceipt {
            operation_id,
            lane: operation.lane,
            operation: operation.operation,
            request_id: operation.request_id,
            mode: operation.mode,
            priority: operation.priority,
            enqueued_at_ms: operation.enqueued_at_ms,
            admitted_at_ms: operation.admitted_at_ms,
            finished_at_ms,
            queue_wait_ms: operation.queue_wait_ms,
            execution_ms,
            retry_count,
            outcome,
            batch_identity: operation.batch_identity,
            transaction_behavior: operation.transaction_behavior,
            phase_ms: operation.phase_ms,
            row_count: operation.row_count,
            file_bytes_read: operation.file_bytes_read,
        });
        while registry.receipts.len() > OPERATION_RECEIPT_CAPACITY {
            registry.receipts.pop_front();
        }
        drop(registry);
        if let Some(details) = request_trace {
            crate::diagnostics::emit_trace_event(&self.paths, "database_request_completed", "info", details);
        }
        if let Some(details) = admission_failure {
            crate::diagnostics::emit_trace_event(&self.paths, "database_admission_failure", "warn", details);
        }
        if let Some(details) = heavy_read {
            crate::diagnostics::emit_trace_event(
                &self.paths,
                "database_heavy_read",
                "warn",
                details,
            );
        }
    }

    fn record_file_bytes_read(&self, operation_id: u64, bytes: u64) {
        if let Some(operation) = self
            .registry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .active
            .get_mut(&operation_id)
        {
            operation.file_bytes_read = Some(bytes);
        }
    }

    fn fail_before_admission(
        &self,
        operation_id: u64,
        started: Instant,
        outcome: &'static str,
    ) -> EngineError {
        self.finish(operation_id, started, outcome, 0);
        EngineError::DatabaseRuntime(format!(
            "{outcome}; database={}",
            self.database_path.display()
        ))
    }

    fn busy_outcome(&self, operation_id: u64, phase: &str) -> &'static str {
        let has_other_internal_candidate = self
            .registry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .active
            .values()
            .any(|candidate| {
                candidate.operation_id != operation_id && candidate.admitted_at_ms.is_some()
            });
        match (phase, has_other_internal_candidate) {
            ("begin", true) => "begin_busy_internal_candidates",
            ("execute", true) => "execute_busy_internal_candidates",
            ("commit", true) => "commit_busy_internal_candidates",
            ("begin", false) => "begin_busy_external_or_unknown",
            ("execute", false) => "execute_busy_external_or_unknown",
            ("commit", false) => "commit_busy_external_or_unknown",
            _ => "busy_external_or_unknown",
        }
    }

    fn acquire_writer(
        self: &Arc<Self>,
        context: &DatabaseOperationContext,
    ) -> Result<WriterPermit> {
        self.acquire_writer_with_timeout(context, WRITER_ADMISSION_TIMEOUT)
    }

    fn acquire_writer_with_timeout(
        self: &Arc<Self>,
        context: &DatabaseOperationContext,
        admission_timeout: Duration,
    ) -> Result<WriterPermit> {
        let (operation_id, started) = self.register(context, DatabaseMode::Write);
        if context.cancellation.is_cancelled() {
            return Err(self.fail_before_admission(
                operation_id,
                started,
                "cancelled_before_admission",
            ));
        }

        let current_thread = std::thread::current().id();
        let mut state = self
            .admission
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.writer_active && state.writer_thread == Some(current_thread) {
            drop(state);
            return Err(self.fail_before_admission(
                operation_id,
                started,
                "nested_writer_admission_rejected",
            ));
        }
        if state.shutting_down {
            drop(state);
            return Err(self.fail_before_admission(operation_id, started, "runtime_shutting_down"));
        }
        if let Some(reason)=self.maintenance_writer_gate() {
            drop(state);
            return Err(self.fail_before_admission(operation_id,started,reason));
        }
        if state.waiting_writers.len() + usize::from(state.writer_active) >= WRITER_QUEUE_CAPACITY {
            drop(state);
            return Err(self.fail_before_admission(
                operation_id,
                started,
                "writer_queue_overloaded",
            ));
        }

        let ticket = state.next_ticket;
        state.next_ticket = state.next_ticket.wrapping_add(1);
        state.waiting_writers.push_back(WaitingWriter {
            ticket,
            operation_id,
        });
        let deadline = Instant::now() + admission_timeout;
        loop {
            if state.shutting_down || context.cancellation.is_cancelled() {
                state
                    .waiting_writers
                    .retain(|waiter| waiter.operation_id != operation_id);
                let outcome = if state.shutting_down {
                    "runtime_shutting_down"
                } else {
                    "cancelled_before_admission"
                };
                drop(state);
                self.admission_changed.notify_all();
                return Err(self.fail_before_admission(operation_id, started, outcome));
            }
            let is_front = state
                .waiting_writers
                .front()
                .map(|waiter| waiter.ticket == ticket && waiter.operation_id == operation_id)
                .unwrap_or(false);
            if is_front && !state.writer_active {
                if let Some(reason)=self.maintenance_writer_gate() {
                    state.waiting_writers.retain(|w|w.operation_id!=operation_id);
                    drop(state);
                    self.admission_changed.notify_all();
                    return Err(self.fail_before_admission(operation_id,started,reason));
                }
                state.waiting_writers.pop_front();
                state.writer_active = true;
                state.writer_thread = Some(current_thread);
                drop(state);
                self.admitted(operation_id, started.elapsed());
                return Ok(WriterPermit {
                    runtime: Arc::clone(self),
                    operation_id,
                    started,
                    outcome: "completed_write_context",
                    retry_count: 0,
                });
            }
            let now = Instant::now();
            if now >= deadline {
                state
                    .waiting_writers
                    .retain(|waiter| waiter.operation_id != operation_id);
                drop(state);
                self.admission_changed.notify_all();
                return Err(self.fail_before_admission(
                    operation_id,
                    started,
                    "writer_admission_timeout",
                ));
            }
            let remaining = deadline.saturating_duration_since(now);
            let (next, _) = self
                .admission_changed
                .wait_timeout(state, remaining.min(Duration::from_millis(50)))
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state = next;
        }
    }

    fn acquire_reader(
        self: &Arc<Self>,
        context: &DatabaseOperationContext,
    ) -> Result<ReaderPermit> {
        let (operation_id, started) = self.register(context, DatabaseMode::Read);
        let deadline = Instant::now() + READ_ADMISSION_TIMEOUT;
        let mut state = self
            .admission
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.shutting_down {
            drop(state);
            return Err(self.fail_before_admission(operation_id, started, "runtime_shutting_down"));
        }
        if state.waiting_readers.len() >= READ_ADMISSION_CAPACITY {
            drop(state);
            return Err(self.fail_before_admission(
                operation_id,
                started,
                "read_admission_overloaded",
            ));
        }
        let ticket = state.next_reader_ticket;
        state.next_reader_ticket = state.next_reader_ticket.wrapping_add(1);
        state.waiting_readers.push_back(WaitingReader {
            ticket,
            operation_id,
        });
        loop {
            if state.shutting_down || context.cancellation.is_cancelled() {
                let outcome = if state.shutting_down {
                    "runtime_shutting_down"
                } else {
                    "cancelled_before_admission"
                };
                state
                    .waiting_readers
                    .retain(|waiter| waiter.operation_id != operation_id);
                drop(state);
                self.admission_changed.notify_all();
                return Err(self.fail_before_admission(operation_id, started, outcome));
            }
            let is_front = state
                .waiting_readers
                .front()
                .map(|waiter| waiter.ticket == ticket && waiter.operation_id == operation_id)
                .unwrap_or(false);
            if is_front && state.active_readers < READ_EXECUTOR_LIMIT {
                state.waiting_readers.pop_front();
                state.active_readers += 1;
                drop(state);
                self.admitted(operation_id, started.elapsed());
                return Ok(ReaderPermit {
                    runtime: Arc::clone(self),
                    operation_id,
                    started,
                    outcome: "completed_read_context",
                });
            }
            let now = Instant::now();
            if now >= deadline {
                state
                    .waiting_readers
                    .retain(|waiter| waiter.operation_id != operation_id);
                drop(state);
                self.admission_changed.notify_all();
                return Err(self.fail_before_admission(
                    operation_id,
                    started,
                    "read_admission_timeout",
                ));
            }
            let (next, _) = self
                .admission_changed
                .wait_timeout(
                    state,
                    deadline
                        .saturating_duration_since(now)
                        .min(Duration::from_millis(50)),
                )
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state = next;
        }
    }
}

struct WriterPermit {
    runtime: Arc<RuntimeInner>,
    operation_id: u64,
    started: Instant,
    outcome: &'static str,
    retry_count: u32,
}

impl WriterPermit {
    fn phase(&self, name: &str, duration: Duration) {
        self.runtime
            .operation_metadata(self.operation_id, Some((name, duration)), None, None);
    }

    fn transaction_behavior(&self, behavior: TransactionBehavior) {
        self.runtime.operation_metadata(
            self.operation_id,
            None,
            Some(match behavior {
                TransactionBehavior::Deferred => "deferred",
                TransactionBehavior::Immediate => "immediate",
                TransactionBehavior::Exclusive => "exclusive",
                _ => "unknown",
            }),
            None,
        );
    }
}

impl Drop for WriterPermit {
    fn drop(&mut self) {
        {
            let mut state = self
                .runtime
                .admission
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state.writer_active = false;
            state.writer_thread = None;
        }
        self.runtime.finish(
            self.operation_id,
            self.started,
            self.outcome,
            self.retry_count,
        );
        self.runtime.admission_changed.notify_all();
    }
}

struct ReaderPermit {
    runtime: Arc<RuntimeInner>,
    operation_id: u64,
    started: Instant,
    outcome: &'static str,
}

impl Drop for ReaderPermit {
    fn drop(&mut self) {
        {
            let mut state = self
                .runtime
                .admission
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state.active_readers = state.active_readers.saturating_sub(1);
        }
        self.runtime
            .finish(self.operation_id, self.started, self.outcome, 0);
        self.runtime.admission_changed.notify_all();
    }
}

pub struct DatabaseWriteContext {
    connection: Option<Connection>,
    permit: Option<WriterPermit>,
    initial_total_changes: u64,
    read_meter: ReadMeter,
    connection_use_started: Instant,
}

impl Deref for DatabaseWriteContext {
    type Target = Connection;

    fn deref(&self) -> &Self::Target {
        self.connection
            .as_ref()
            .expect("database connection present")
    }
}

impl DerefMut for DatabaseWriteContext {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.connection
            .as_mut()
            .expect("database connection present")
    }
}

// WP-0324 read-counting VFS. SQLite's page-cache counter (`SQLITE_DBSTATUS_CACHE_MISS`) misses
// overflow pages of large TEXT/BLOB values, which the bundled build reads straight from the file
// (`SQLITE_DIRECT_OVERFLOW_READ`). This shim counts every `xRead` instead. It copies the default
// VFS, overrides `xOpen` and delegates `xDelete`, and wraps each opened file so every I/O method forwards to the
// real file (same pattern as SQLite's ext/misc/appendvfs.c and vfsstat.c). Bytes are added to a
// per-thread counter; a runtime context records the delta on the thread that opened it.

const READ_COUNTING_VFS_NAME: &str = "voxvulgi_read_counting";
const READ_COUNTING_FILE_HEADER: usize = std::mem::size_of::<rusqlite::ffi::sqlite3_file>();
const SQLITE_IOERR_SHORT_READ: std::os::raw::c_int = rusqlite::ffi::SQLITE_IOERR | (2 << 8);

thread_local! {
    static THREAD_SQLITE_BYTES_READ: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

// Disposable example probes only; no allocation, logging or runtime mutex inside SQLite callbacks.
#[derive(Clone, Copy, Default)]
struct VfsTiming { calls: u64, total_ns: u64, max_ns: u64 }
const VFS_TIMING_NAMES: [&str; 16] = ["xClose", "xShmUnmap", "xShmLock", "xSync", "xDelete", "xSyncMainDb", "xSyncWal", "xSyncUnknown", "fileKindOverflow", "xCloseMainDb", "xCloseWal", "xCloseUnknown", "xCloseOk", "xCloseError", "xOpen", "xRead"];
#[derive(Clone, Copy, Default)]
struct VfsProbeFile { pointer: usize, open_flags: std::os::raw::c_int }
#[derive(Clone, Copy, Default, Serialize)]
struct ShmLockSlotReceipt {
    shared_lock_ok: u64, shared_unlock_ok: u64, unlock_error: u64,
    exclusive_busy: u64, other_error: u64,
}
#[derive(Clone, Copy, Default, Serialize)]
struct ShmLockFileReceipt {
    #[serde(skip)]
    pointer: usize,
    file_id: u64, main_db: bool, closed: bool, shared_mask: u8,
    slots: [ShmLockSlotReceipt; 8],
}
#[derive(Clone, Copy)]
struct ShmLockCapture {
    enabled: bool, files: [ShmLockFileReceipt; 64], used: usize,
    unknown: u64, overflow: u64, calls: u64,
}
impl Default for ShmLockCapture {
    fn default() -> Self { Self { enabled: false, files: [ShmLockFileReceipt::default(); 64], used: 0, unknown: 0, overflow: 0, calls: 0 } }
}
thread_local! {
    static SHM_LOCK_ENABLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static SHM_LOCK_BORROW_CONFLICTS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static SHM_LOCK_CAPTURE: std::cell::RefCell<ShmLockCapture> = std::cell::RefCell::new(ShmLockCapture::default());
}
fn with_shm_capture(update: impl FnOnce(&mut ShmLockCapture)) {
    if !SHM_LOCK_ENABLED.try_with(std::cell::Cell::get).unwrap_or(false) { return; }
    let _ = SHM_LOCK_CAPTURE.try_with(|capture| {
        if let Ok(mut state) = capture.try_borrow_mut() { update(&mut state); }
        else { let _ = SHM_LOCK_BORROW_CONFLICTS.try_with(|count| count.set(count.get().saturating_add(1))); }
    });
}
fn capture_shm_open(file: *mut rusqlite::ffi::sqlite3_file, flags: i32) {
    with_shm_capture(|state| {
        if state.used == 64 { state.overflow = state.overflow.saturating_add(1); }
        else {
            state.files[state.used] = ShmLockFileReceipt { pointer: file as usize,
                main_db: flags & rusqlite::ffi::SQLITE_OPEN_MAIN_DB != 0,
                file_id: state.used as u64 + 1, ..ShmLockFileReceipt::default() };
            state.used += 1;
        }
    });
}
fn capture_shm_close(file: *mut rusqlite::ffi::sqlite3_file) {
    with_shm_capture(|state| {
        if let Some(record) = state.files[..state.used].iter_mut().find(|record| record.pointer == file as usize && !record.closed) {
            record.closed = true;
        } else { state.unknown = state.unknown.saturating_add(1); }
    });
}
fn capture_shm_result(file: *mut rusqlite::ffi::sqlite3_file, offset: i32, count: i32, flags: i32, result: i32) {
    with_shm_capture(|state| {
        state.calls = state.calls.saturating_add(1);
        let valid_flags = matches!(flags, 5 | 6 | 9 | 10);
        let valid_range = offset >= 0 && count > 0 && offset < 8 && count <= 8 - offset
            && (flags & rusqlite::ffi::SQLITE_SHM_SHARED == 0 || count == 1);
        let index = state.files[..state.used].iter().position(|record| record.pointer == file as usize && !record.closed);
        if !valid_flags || !valid_range || index.is_none() { state.unknown = state.unknown.saturating_add(1); }
        else if let Some(index) = index {
            let record = &mut state.files[index];
            for slot in offset as usize..(offset + count) as usize {
                let outcome = &mut record.slots[slot];
                if result == rusqlite::ffi::SQLITE_OK {
                    if flags == 6 { outcome.shared_lock_ok = outcome.shared_lock_ok.saturating_add(1); record.shared_mask |= 1 << slot; }
                    if flags == 5 { outcome.shared_unlock_ok = outcome.shared_unlock_ok.saturating_add(1); record.shared_mask &= !(1 << slot); }
                } else if flags & rusqlite::ffi::SQLITE_SHM_UNLOCK != 0 {
                    outcome.unlock_error = outcome.unlock_error.saturating_add(1);
                } else if flags == 10 && result & 255 == rusqlite::ffi::SQLITE_BUSY {
                    outcome.exclusive_busy = outcome.exclusive_busy.saturating_add(1);
                } else { outcome.other_error = outcome.other_error.saturating_add(1); }
            }
        }
    });
}
thread_local! {
    static VFS_PROBE_ENABLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static VFS_PROBE_TIMINGS: std::cell::Cell<[VfsTiming; 16]> = const {
        std::cell::Cell::new([VfsTiming { calls: 0, total_ns: 0, max_ns: 0 }; 16])
    };
    static VFS_PROBE_FILES: std::cell::Cell<[VfsProbeFile; 64]> = const {
        std::cell::Cell::new([VfsProbeFile { pointer: 0, open_flags: 0 }; 64])
    };
}

fn record_vfs_timing(index: usize, elapsed: u64) {
    let _ = VFS_PROBE_TIMINGS.try_with(|metrics| {
        let mut current = metrics.get();
        current[index].calls = current[index].calls.saturating_add(1);
        current[index].total_ns = current[index].total_ns.saturating_add(elapsed);
        current[index].max_ns = current[index].max_ns.max(elapsed);
        metrics.set(current);
    });
}

fn probe_file_open(file: *mut rusqlite::ffi::sqlite3_file, flags: std::os::raw::c_int) {
    capture_shm_open(file, flags);
    if !VFS_PROBE_ENABLED.try_with(std::cell::Cell::get).unwrap_or(false) { return; }
    let _ = VFS_PROBE_FILES.try_with(|files| {
        let mut current = files.get();
        let index = current.iter().position(|slot| slot.pointer == file as usize)
            .or_else(|| current.iter().position(|slot| slot.pointer == 0));
        if let Some(index) = index {
            current[index] = VfsProbeFile { pointer: file as usize, open_flags: flags };
            files.set(current);
        } else { record_vfs_timing(8, 0); }
    });
}

fn probe_file_close(file: *mut rusqlite::ffi::sqlite3_file) {
    if !VFS_PROBE_ENABLED.try_with(std::cell::Cell::get).unwrap_or(false) { return; }
    let _ = VFS_PROBE_FILES.try_with(|files| {
        let mut current = files.get();
        if let Some(slot) = current.iter_mut().find(|slot| slot.pointer == file as usize) {
            *slot = VfsProbeFile::default();
            files.set(current);
        }
    });
}

fn probe_sync_kind(file: *mut rusqlite::ffi::sqlite3_file) -> usize {
    VFS_PROBE_FILES.try_with(|files| {
        let flags = files.get().iter().find(|slot| slot.pointer == file as usize).map(|slot|slot.open_flags);
        match flags {
            Some(flags) if flags & rusqlite::ffi::SQLITE_OPEN_MAIN_DB != 0 => 5,
            Some(flags) if flags & rusqlite::ffi::SQLITE_OPEN_WAL != 0 => 6,
            _ => 7,
        }
    }).unwrap_or(7)
}

fn timed_vfs_call(index: usize, call: impl FnOnce() -> std::os::raw::c_int) -> std::os::raw::c_int {
    if !VFS_PROBE_ENABLED.try_with(std::cell::Cell::get).unwrap_or(false) { return call(); }
    let started = Instant::now();
    let result = call();
    let elapsed = started.elapsed().as_nanos().min(u64::MAX as u128) as u64;
    record_vfs_timing(index, elapsed);
    result
}

fn timed_vfs_close(file: *mut rusqlite::ffi::sqlite3_file, delegate_present: bool, call: impl FnOnce() -> std::os::raw::c_int) -> std::os::raw::c_int {
    if !VFS_PROBE_ENABLED.try_with(std::cell::Cell::get).unwrap_or(false) { return call(); }
    // Capture flags before the delegate frees the real file. All three metrics use one sample.
    let kind = if delegate_present { probe_sync_kind(file) + 4 } else { 11 };
    let started = Instant::now();
    let result = call();
    let elapsed = started.elapsed().as_nanos().min(u64::MAX as u128) as u64;
    record_vfs_timing(0, elapsed);
    record_vfs_timing(kind, elapsed);
    record_vfs_timing(if result == rusqlite::ffi::SQLITE_OK { 12 } else { 13 }, elapsed);
    result
}

fn thread_sqlite_bytes_read() -> u64 {
    THREAD_SQLITE_BYTES_READ.with(std::cell::Cell::get)
}

static READ_COUNTING_BASE_VFS: OnceLock<usize> = OnceLock::new();

/// Opaque validated disposable-fixture identity; never installed by production code.
#[doc(hidden)]
#[derive(Clone)]
pub struct DisposableFilenameProbe { exact: PathBuf, plain: PathBuf }
thread_local! {
    static DISPOSABLE_FILENAME_PROBE: std::cell::RefCell<Option<DisposableFilenameProbe>> = const { std::cell::RefCell::new(None) };
}
fn short_extended_local_filename(path: &Path) -> Option<PathBuf> {
    let text = path.to_str()?;
    let bytes = text.as_bytes();
    if !bytes.starts_with(b"\\\\?\\") || !bytes.get(4).is_some_and(u8::is_ascii_alphabetic)
        || bytes.get(5) != Some(&b':') || bytes.get(6) != Some(&b'\\')
        || text.encode_utf16().count() >= 260 { return None; }
    Some(PathBuf::from(&text[4..]))
}

/// The real file lives directly after the wrapper's `sqlite3_file` header in the same
/// allocation (`szOsFile` = header + base `szOsFile`).
unsafe fn read_counting_real_file(
    file: *mut rusqlite::ffi::sqlite3_file,
) -> *mut rusqlite::ffi::sqlite3_file {
    file.cast::<u8>().add(READ_COUNTING_FILE_HEADER).cast()
}

macro_rules! forward_io {
    ($file:expr, $method:ident, $missing:expr $(, $arg:expr)*) => {{
        let real = read_counting_real_file($file);
        let methods = (*real).pMethods;
        match if methods.is_null() { None } else { (*methods).$method } {
            Some(function) => function(real $(, $arg)*),
            None => $missing,
        }
    }};
}

unsafe extern "C" fn counting_close(file: *mut rusqlite::ffi::sqlite3_file) -> std::os::raw::c_int {
    let result = if VFS_PROBE_ENABLED.try_with(std::cell::Cell::get).unwrap_or(false) {
        let real = read_counting_real_file(file);
        let methods = (*real).pMethods;
        let close = if methods.is_null() { None } else { (*methods).xClose };
        timed_vfs_close(file, close.is_some(), || match close {
            Some(function) => function(real),
            None => rusqlite::ffi::SQLITE_OK,
        })
    } else { forward_io!(file, xClose, rusqlite::ffi::SQLITE_OK) };
    if result == rusqlite::ffi::SQLITE_OK { capture_shm_close(file); }
    else { with_shm_capture(|state|state.unknown = state.unknown.saturating_add(1)); }
    probe_file_close(file);
    result
}

unsafe extern "C" fn counting_read(
    file: *mut rusqlite::ffi::sqlite3_file,
    buffer: *mut std::os::raw::c_void,
    amount: std::os::raw::c_int,
    offset: rusqlite::ffi::sqlite3_int64,
) -> std::os::raw::c_int {
    let rc = timed_vfs_call(15, || forward_io!(file, xRead, rusqlite::ffi::SQLITE_IOERR, buffer, amount, offset));
    if rc == rusqlite::ffi::SQLITE_OK || rc == SQLITE_IOERR_SHORT_READ {
        let amount = amount.max(0) as u64;
        THREAD_SQLITE_BYTES_READ.with(|total| total.set(total.get().saturating_add(amount)));
    }
    rc
}

unsafe extern "C" fn counting_write(
    file: *mut rusqlite::ffi::sqlite3_file,
    buffer: *const std::os::raw::c_void,
    amount: std::os::raw::c_int,
    offset: rusqlite::ffi::sqlite3_int64,
) -> std::os::raw::c_int {
    forward_io!(file, xWrite, rusqlite::ffi::SQLITE_IOERR, buffer, amount, offset)
}

unsafe extern "C" fn counting_truncate(
    file: *mut rusqlite::ffi::sqlite3_file,
    size: rusqlite::ffi::sqlite3_int64,
) -> std::os::raw::c_int {
    forward_io!(file, xTruncate, rusqlite::ffi::SQLITE_IOERR, size)
}

unsafe extern "C" fn counting_sync(
    file: *mut rusqlite::ffi::sqlite3_file,
    flags: std::os::raw::c_int,
) -> std::os::raw::c_int {
    if !VFS_PROBE_ENABLED.try_with(std::cell::Cell::get).unwrap_or(false) {
        return forward_io!(file, xSync, rusqlite::ffi::SQLITE_IOERR, flags);
    }
    let kind = probe_sync_kind(file);
    let started = Instant::now();
    let result = forward_io!(file, xSync, rusqlite::ffi::SQLITE_IOERR, flags);
    let elapsed = started.elapsed().as_nanos().min(u64::MAX as u128) as u64;
    record_vfs_timing(3, elapsed);
    record_vfs_timing(kind, elapsed);
    result
}

unsafe extern "C" fn counting_file_size(
    file: *mut rusqlite::ffi::sqlite3_file,
    size: *mut rusqlite::ffi::sqlite3_int64,
) -> std::os::raw::c_int {
    forward_io!(file, xFileSize, rusqlite::ffi::SQLITE_IOERR, size)
}

unsafe extern "C" fn counting_lock(
    file: *mut rusqlite::ffi::sqlite3_file,
    level: std::os::raw::c_int,
) -> std::os::raw::c_int {
    forward_io!(file, xLock, rusqlite::ffi::SQLITE_IOERR, level)
}

unsafe extern "C" fn counting_unlock(
    file: *mut rusqlite::ffi::sqlite3_file,
    level: std::os::raw::c_int,
) -> std::os::raw::c_int {
    forward_io!(file, xUnlock, rusqlite::ffi::SQLITE_IOERR, level)
}

unsafe extern "C" fn counting_check_reserved_lock(
    file: *mut rusqlite::ffi::sqlite3_file,
    result: *mut std::os::raw::c_int,
) -> std::os::raw::c_int {
    forward_io!(file, xCheckReservedLock, rusqlite::ffi::SQLITE_IOERR, result)
}

unsafe extern "C" fn counting_file_control(
    file: *mut rusqlite::ffi::sqlite3_file,
    op: std::os::raw::c_int,
    argument: *mut std::os::raw::c_void,
) -> std::os::raw::c_int {
    forward_io!(file, xFileControl, rusqlite::ffi::SQLITE_NOTFOUND, op, argument)
}

unsafe extern "C" fn counting_sector_size(
    file: *mut rusqlite::ffi::sqlite3_file,
) -> std::os::raw::c_int {
    forward_io!(file, xSectorSize, 0)
}

unsafe extern "C" fn counting_device_characteristics(
    file: *mut rusqlite::ffi::sqlite3_file,
) -> std::os::raw::c_int {
    forward_io!(file, xDeviceCharacteristics, 0)
}

unsafe extern "C" fn counting_shm_map(
    file: *mut rusqlite::ffi::sqlite3_file,
    region: std::os::raw::c_int,
    region_size: std::os::raw::c_int,
    extend: std::os::raw::c_int,
    mapped: *mut *mut std::os::raw::c_void,
) -> std::os::raw::c_int {
    forward_io!(
        file,
        xShmMap,
        rusqlite::ffi::SQLITE_IOERR,
        region,
        region_size,
        extend,
        mapped
    )
}

unsafe extern "C" fn counting_shm_lock(
    file: *mut rusqlite::ffi::sqlite3_file,
    offset: std::os::raw::c_int,
    count: std::os::raw::c_int,
    flags: std::os::raw::c_int,
) -> std::os::raw::c_int {
    let result = timed_vfs_call(2, || forward_io!(file, xShmLock, rusqlite::ffi::SQLITE_IOERR, offset, count, flags));
    capture_shm_result(file, offset, count, flags, result);
    result
}

unsafe extern "C" fn counting_shm_barrier(file: *mut rusqlite::ffi::sqlite3_file) {
    forward_io!(file, xShmBarrier, ())
}

unsafe extern "C" fn counting_shm_unmap(
    file: *mut rusqlite::ffi::sqlite3_file,
    delete: std::os::raw::c_int,
) -> std::os::raw::c_int {
    timed_vfs_call(1, || forward_io!(file, xShmUnmap, rusqlite::ffi::SQLITE_OK, delete))
}

unsafe extern "C" fn counting_fetch(
    file: *mut rusqlite::ffi::sqlite3_file,
    offset: rusqlite::ffi::sqlite3_int64,
    amount: std::os::raw::c_int,
    mapped: *mut *mut std::os::raw::c_void,
) -> std::os::raw::c_int {
    // Without a real xFetch, report "no mapping" so SQLite falls back to xRead.
    forward_io!(
        file,
        xFetch,
        {
            *mapped = std::ptr::null_mut();
            rusqlite::ffi::SQLITE_OK
        },
        offset,
        amount,
        mapped
    )
}

unsafe extern "C" fn counting_unfetch(
    file: *mut rusqlite::ffi::sqlite3_file,
    offset: rusqlite::ffi::sqlite3_int64,
    mapped: *mut std::os::raw::c_void,
) -> std::os::raw::c_int {
    forward_io!(file, xUnfetch, rusqlite::ffi::SQLITE_OK, offset, mapped)
}

static READ_COUNTING_IO_METHODS: rusqlite::ffi::sqlite3_io_methods =
    rusqlite::ffi::sqlite3_io_methods {
        iVersion: 3,
        xClose: Some(counting_close),
        xRead: Some(counting_read),
        xWrite: Some(counting_write),
        xTruncate: Some(counting_truncate),
        xSync: Some(counting_sync),
        xFileSize: Some(counting_file_size),
        xLock: Some(counting_lock),
        xUnlock: Some(counting_unlock),
        xCheckReservedLock: Some(counting_check_reserved_lock),
        xFileControl: Some(counting_file_control),
        xSectorSize: Some(counting_sector_size),
        xDeviceCharacteristics: Some(counting_device_characteristics),
        xShmMap: Some(counting_shm_map),
        xShmLock: Some(counting_shm_lock),
        xShmBarrier: Some(counting_shm_barrier),
        xShmUnmap: Some(counting_shm_unmap),
        xFetch: Some(counting_fetch),
        xUnfetch: Some(counting_unfetch),
    };

unsafe extern "C" fn counting_open(
    _vfs: *mut rusqlite::ffi::sqlite3_vfs,
    name: rusqlite::ffi::sqlite3_filename,
    file: *mut rusqlite::ffi::sqlite3_file,
    flags: std::os::raw::c_int,
    out_flags: *mut std::os::raw::c_int,
) -> std::os::raw::c_int {
    let Some(base) = READ_COUNTING_BASE_VFS
        .get()
        .map(|pointer| *pointer as *mut rusqlite::ffi::sqlite3_vfs)
    else {
        return rusqlite::ffi::SQLITE_ERROR;
    };
    let real = read_counting_real_file(file);
    let rc = timed_vfs_call(14, || match (*base).xOpen {
        Some(open) => open(base, name, real, flags, out_flags),
        None => rusqlite::ffi::SQLITE_ERROR,
    });
    // SQLite calls xClose whenever pMethods is non-null after xOpen, even on failure, so the
    // wrapper mirrors whether the real file needs closing.
    (*file).pMethods = if (*real).pMethods.is_null() {
        std::ptr::null()
    } else {
        probe_file_open(file, flags);
        &READ_COUNTING_IO_METHODS
    };
    rc
}

unsafe extern "C" fn counting_delete(
    _vfs: *mut rusqlite::ffi::sqlite3_vfs,
    name: *const std::os::raw::c_char,
    sync_dir: std::os::raw::c_int,
) -> std::os::raw::c_int {
    let Some(base) = READ_COUNTING_BASE_VFS.get().map(|p| *p as *mut rusqlite::ffi::sqlite3_vfs) else {
        return rusqlite::ffi::SQLITE_IOERR;
    };
    timed_vfs_call(4, || match (*base).xDelete {
        Some(delete) => delete(base, name, sync_dir),
        None => rusqlite::ffi::SQLITE_IOERR,
    })
}

/// Registers the read-counting VFS once (not as the process default). `None` means
/// registration failed and callers open with the default VFS, uncounted.
fn read_counting_vfs_name() -> Option<&'static str> {
    static REGISTERED: OnceLock<bool> = OnceLock::new();
    let registered = *REGISTERED.get_or_init(|| {
        // SAFETY: `sqlite3_vfs_find` auto-initializes SQLite and returns a VFS that lives for the
        // process. The copied VFS keeps the base's pAppData/mxPathname for its forwarded methods,
        // is leaked so it outlives every connection, and only its xOpen is replaced.
        unsafe {
            let base = rusqlite::ffi::sqlite3_vfs_find(std::ptr::null());
            if base.is_null() || READ_COUNTING_BASE_VFS.set(base as usize).is_err() {
                return false;
            }
            let mut vfs = *base;
            vfs.szOsFile = READ_COUNTING_FILE_HEADER as std::os::raw::c_int + (*base).szOsFile;
            vfs.pNext = std::ptr::null_mut();
            vfs.zName = c"voxvulgi_read_counting".as_ptr();
            vfs.xOpen = Some(counting_open);
            if (*base).xDelete.is_some() { vfs.xDelete = Some(counting_delete); }
            let vfs = Box::into_raw(Box::new(vfs));
            rusqlite::ffi::sqlite3_vfs_register(vfs, 0) == rusqlite::ffi::SQLITE_OK
        }
    });
    registered.then_some(READ_COUNTING_VFS_NAME)
}

/// Opens an application-database connection through the read-counting VFS when it is available.
pub(super) fn open_counted_connection(
    path: &Path,
    flags: rusqlite::OpenFlags,
) -> rusqlite::Result<Connection> {
    let path = AppDatabase::disposable_open_filename(path)?;
    let path = AppDatabase::sqlite_open_filename(path.as_ref());
    match read_counting_vfs_name() {
        Some(vfs) => Connection::open_with_flags_and_vfs(path, flags, vfs),
        None => Connection::open_with_flags(path, flags),
    }
}

/// Start point of an operation's file-read measurement; valid only on the opening thread.
#[derive(Debug, Clone, Copy)]
struct ReadMeter {
    thread: ThreadId,
    start: u64,
}

impl ReadMeter {
    fn start() -> Self {
        Self {
            thread: std::thread::current().id(),
            start: thread_sqlite_bytes_read(),
        }
    }

    /// `None` when the context is dropped on a different thread than it was opened on.
    fn bytes_read(&self) -> Option<u64> {
        (std::thread::current().id() == self.thread)
            .then(|| thread_sqlite_bytes_read().saturating_sub(self.start))
    }
}

impl Drop for DatabaseWriteContext {
    fn drop(&mut self) {
        if let Some(permit) = self.permit.as_ref() {
            permit.phase("connection_use", self.connection_use_started.elapsed());
        }
        if let (Some(connection), Some(permit)) = (self.connection.as_ref(), self.permit.as_ref()) {
            let row_count = connection
                .total_changes()
                .saturating_sub(self.initial_total_changes);
            permit
                .runtime
                .operation_metadata(permit.operation_id, None, None, Some(row_count));
            if let Some(bytes) = self.read_meter.bytes_read() {
                permit
                    .runtime
                    .record_file_bytes_read(permit.operation_id, bytes);
            }
        }
        let close_started = Instant::now();
        #[cfg(feature = "wp0333_connection_reuse_proof")]
        if let Some(permit)=self.permit.as_mut().filter(|p|p.runtime.reuse_enabled()) {
            if let Some(connection)=self.connection.take() {
                let rolled_back=permit.runtime.reuse_return(connection,true,permit.operation_id);
                if rolled_back && permit.outcome=="completed_write_context" {permit.outcome="manual_transaction_not_committed";}
            }
            // This is a logical lease return, not native xClose.
            self.permit.take();return;
        }
        if let Some(permit) = self.permit.as_ref() {permit.phase("connection_close",Duration::ZERO);}
        self.connection.take();
        if let Some(permit) = self.permit.as_ref() {
            permit.phase("connection_close", close_started.elapsed());
        }
        self.permit.take();
    }
}

pub struct DatabaseReadContext {
    connection: Option<Connection>,
    permit: Option<ReaderPermit>,
    read_meter: ReadMeter,
    connection_use_started: Instant,
}

impl DatabaseReadContext {
    pub fn record_phase(&self, name: &str, duration: Duration) {
        if let Some(permit) = self.permit.as_ref() {
            permit.runtime.operation_metadata(permit.operation_id, Some((name, duration)), None, None);
        }
    }
    pub(crate) fn mark_outcome(&mut self, outcome: &'static str) {
        if let Some(permit) = self.permit.as_mut() {
            permit.outcome = outcome;
        }
    }
}

impl Deref for DatabaseReadContext {
    type Target = Connection;

    fn deref(&self) -> &Self::Target {
        self.connection
            .as_ref()
            .expect("database connection present")
    }
}

impl DerefMut for DatabaseReadContext {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.connection
            .as_mut()
            .expect("database connection present")
    }
}

impl Drop for DatabaseReadContext {
    fn drop(&mut self) {
        self.record_phase("connection_use", self.connection_use_started.elapsed());
        if let (Some(_), Some(permit), Some(bytes)) = (
            self.connection.as_ref(),
            self.permit.as_ref(),
            self.read_meter.bytes_read(),
        ) {
            permit
                .runtime
                .record_file_bytes_read(permit.operation_id, bytes);
        }
        let close_started = Instant::now();
        #[cfg(feature = "wp0333_connection_reuse_proof")]
        if let Some(permit)=self.permit.as_ref().filter(|p|p.runtime.reuse_enabled()) {
            if let Some(connection)=self.connection.take() {permit.runtime.reuse_return(connection,false,permit.operation_id);}
            self.permit.take();return;
        }
        self.record_phase("connection_close",Duration::ZERO);
        self.connection.take();
        self.record_phase("connection_close", close_started.elapsed());
        self.permit.take();
    }
}

#[derive(Clone, Debug)]
pub struct AppDatabase {
    inner: Arc<RuntimeInner>,
}

pub type DatabaseRuntime = AppDatabase;

impl AppDatabase {
    #[cfg(feature = "wp0333_connection_reuse_proof")]
    pub fn connection_reuse_proof(&self) -> Option<ConnectionReuseProof> {
        self.inner.reuse.lock().unwrap_or_else(|p|p.into_inner()).as_ref().map(|p|p.proof.clone())
    }

    /// Diagnostic counterpart only; validates exact fresh fixture identity before opting in.
    #[cfg(feature = "wp0333_connection_reuse_proof")]
    pub fn enable_disposable_connection_reuse(&self, root: &Path, expected_source_sha256: &str) -> Result<ConnectionReuseGuard> {
        use sha2::{Digest,Sha256};
        let refuse=||EngineError::DatabaseRuntime("reuse_fixture_identity_refused".into());
        let root=root.canonicalize()?;
        if self.inner.database_path != absolute_key(&root.join("db/app.sqlite"))? {return Err(refuse());}
        let normalize=|path: &Path|path.to_string_lossy().replace('\\',"/").to_lowercase().trim_start_matches("//?/").trim_end_matches('/').to_string();
        for key in ["APPDATA","LOCALAPPDATA"] {
            let protected=PathBuf::from(std::env::var_os(key).ok_or_else(refuse)?).join("com.voxvulgi.voxvulgi");
            let protected=protected.canonicalize().unwrap_or(protected);
            let r=normalize(&root);let p=normalize(&protected);
            if r==p||r.starts_with(&(p.clone()+"/"))||p.starts_with(&(r+"/")) {return Err(refuse());}
        }
        let marker: serde_json::Value=serde_json::from_slice(&std::fs::read(root.join("wp0333_production_fixture.json"))?)?;
        if marker["pid"].as_u64()!=Some(std::process::id() as u64) || marker["schema"].as_u64()!=Some(61)
            || marker["job_density"].as_u64().is_none_or(|n|n<1000)
            || marker["root"].as_str().map(Path::new)!=Some(root.as_path())
            || marker["source_sha256"].as_str()!=Some(expected_source_sha256) {return Err(refuse());}
        let source=PathBuf::from(marker["source"].as_str().ok_or_else(refuse)?).canonicalize()?;
        for key in ["APPDATA","LOCALAPPDATA"] {
            let protected=PathBuf::from(std::env::var_os(key).ok_or_else(refuse)?).join("com.voxvulgi.voxvulgi");
            let protected=protected.canonicalize().unwrap_or(protected);let s=normalize(&source);let p=normalize(&protected);
            if s==p||s.starts_with(&(p.clone()+"/"))||p.starts_with(&(s+"/")) {return Err(refuse());}
        }
        if root.starts_with(source.parent().ok_or_else(refuse)?) || source.starts_with(&root) {return Err(refuse());}
        let hash=|path: &Path|->Result<String> {
            use std::io::Read;
            let mut file=std::fs::File::open(path)?;let mut digest=Sha256::new();let mut buf=[0;65536];
            loop {let n=file.read(&mut buf)?;if n==0 {break}digest.update(&buf[..n]);}Ok(hex::encode(digest.finalize()))
        };
        for path in [&source,&self.inner.database_path] {
            if hash(path)?!=expected_source_sha256 {return Err(refuse());}
            for suffix in ["-wal","-shm","-journal"] {if PathBuf::from(format!("{}{suffix}",path.display())).exists() {return Err(refuse());}}
        }
        self.read(DatabaseOperationContext::new("reuse_guard","fixture_identity"),|c| {
            let schema:u32=c.pragma_query_value(None,"user_version",|r|r.get(0))?;
            let density:u64=c.query_row("SELECT COUNT(*) FROM job",[],|r|r.get(0))?;
            if schema!=61 || Some(density)!=marker["job_density"].as_u64() {return Err(refuse());}Ok(())
        })?;
        if self.inner.maintenance_shared().is_some() {return Err(refuse());}
        let state=self.inner.admission.lock().unwrap_or_else(|p|p.into_inner());
        if state.shutting_down||state.writer_active||state.active_readers!=0||!state.waiting_readers.is_empty()||!state.waiting_writers.is_empty(){return Err(refuse());}
        let mut pool=self.inner.reuse.lock().unwrap_or_else(|p|p.into_inner());
        if pool.is_some(){return Err(refuse());}
        *pool=Some(ConnectionReuse{proof:ConnectionReuseProof{enabled:true,..Default::default()},..Default::default()});
        Ok(ConnectionReuseGuard{runtime:Arc::clone(&self.inner)})
    }

    /// SQLite filename spelling only; canonical runtime identity remains unchanged.
    /// Pure lexical checks: no filesystem work is performed inside admission.
    #[doc(hidden)]
    pub fn sqlite_open_filename(path: &Path) -> std::borrow::Cow<'_, Path> {
        #[cfg(windows)]
        {
            let Some(text) = path.to_str() else { return std::borrow::Cow::Borrowed(path); };
            let bytes = text.as_bytes();
            if !bytes.starts_with(b"\\\\?\\")
                || !bytes.get(4).is_some_and(u8::is_ascii_alphabetic)
                || bytes.get(5) != Some(&b':') || bytes.get(6) != Some(&b'\\') {
                return std::borrow::Cow::Borrowed(path);
            }
            let plain_units = text[4..].encode_utf16().count();
            if plain_units >= 248 || plain_units.checked_add(8).is_none_or(|units| units >= 260) {
                return std::borrow::Cow::Borrowed(path);
            }
            for component in text[7..].split('\\') {
                if component.is_empty() || component.ends_with(['.', ' '])
                    || component.chars().any(|character| character <= '\u{1f}'
                        || matches!(character, '<' | '>' | ':' | '"' | '/' | '|' | '?' | '*')) {
                    return std::borrow::Cow::Borrowed(path);
                }
                // DOS device recognition uses the first extension, not file_stem's last one.
                let base = component.split('.').next().unwrap_or("").trim_end_matches(['.', ' ']).to_ascii_uppercase();
                let numbered_device = base.strip_prefix("COM").or_else(|| base.strip_prefix("LPT"))
                    .is_some_and(|number| matches!(number, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"));
                if matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$") || numbered_device {
                    return std::borrow::Cow::Borrowed(path);
                }
            }
            return std::borrow::Cow::Borrowed(dunce::simplified(path));
        }
        #[cfg(not(windows))]
        std::borrow::Cow::Borrowed(path)
    }

    /// Validate once outside admission; only the harness marker's exact root/db/app.sqlite is eligible.
    #[doc(hidden)]
    pub fn prepare_disposable_filename_probe(root: &Path, path: &Path) -> rusqlite::Result<DisposableFilenameProbe> {
        let invalid = || rusqlite::Error::InvalidPath(path.to_path_buf());
        let root = root.canonicalize().map_err(|_|invalid())?;
        let parent = path.parent().ok_or_else(invalid)?.canonicalize().map_err(|_|invalid())?;
        match std::fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            _ => return Err(invalid()),
        }
        if parent != root.join("db") || path.file_name() != Some(std::ffi::OsStr::new("app.sqlite"))
            || !root.join("wp0323_disposable_fixture.json").is_file() { return Err(invalid()); }
        let marker: serde_json::Value = serde_json::from_slice(&std::fs::read(root.join("wp0323_disposable_fixture.json")).map_err(|_|invalid())?).map_err(|_|invalid())?;
        if marker["root"].as_str().map(Path::new) != Some(root.as_path())
            || marker["pid"].as_u64() != Some(std::process::id() as u64) { return Err(invalid()); }
        for variable in ["APPDATA", "LOCALAPPDATA"] {
            let base = std::env::var_os(variable).ok_or_else(invalid)?;
            let protected = PathBuf::from(base).join("com.voxvulgi.voxvulgi");
            let protected = protected.canonicalize().unwrap_or(protected);
            let normalize = |value: &Path|value.to_string_lossy().replace('\\', "/").to_lowercase().trim_start_matches("//?/").trim_end_matches('/').to_string();
            let candidate = normalize(&root);
            let protected = normalize(&protected);
            if candidate == protected || candidate.starts_with(&(protected.clone()+"/"))
                || protected.starts_with(&(candidate+"/")) { return Err(invalid()); }
        }
        let exact = parent.join("app.sqlite");
        if path != exact { return Err(invalid()); }
        let plain = short_extended_local_filename(&exact).ok_or_else(invalid)?;
        Ok(DisposableFilenameProbe { exact, plain })
    }
    #[doc(hidden)]
    pub fn enable_disposable_filename_probe(probe: DisposableFilenameProbe) {
        DISPOSABLE_FILENAME_PROBE.with(|current| *current.borrow_mut() = Some(probe));
    }
    #[doc(hidden)]
    pub fn end_disposable_filename_probe() { DISPOSABLE_FILENAME_PROBE.with(|current| *current.borrow_mut() = None); }
    #[doc(hidden)]
    pub fn disposable_filename_probe_enabled() -> bool { DISPOSABLE_FILENAME_PROBE.with(|current|current.borrow().is_some()) }
    /// Lexical exact-identity matching only; no filesystem work in admitted opens.
    #[doc(hidden)]
    pub fn disposable_open_filename(path: &Path) -> rusqlite::Result<std::borrow::Cow<'_, Path>> {
        DISPOSABLE_FILENAME_PROBE.with(|current| match current.borrow().as_ref() {
            None => Ok(std::borrow::Cow::Borrowed(path)),
            Some(probe) if path == probe.exact || path == probe.plain => Ok(std::borrow::Cow::Owned(probe.plain.clone())),
            Some(_) => Err(rusqlite::Error::InvalidPath(path.to_path_buf())),
        })
    }
    /// Harness-only capture on this thread; begin before any counted-VFS connection opens.
    #[doc(hidden)]
    pub fn begin_shm_lock_probe() {
        // Registration occurs outside callbacks; the harness still verifies exact availability.
        let _ = read_counting_vfs_name();
        SHM_LOCK_CAPTURE.with(|capture| *capture.borrow_mut() = ShmLockCapture { enabled: true, ..ShmLockCapture::default() });
        SHM_LOCK_BORROW_CONFLICTS.with(|count|count.set(0));
        SHM_LOCK_ENABLED.with(|enabled|enabled.set(true));
    }
    /// Snapshot allocates only outside native callbacks. Unknown/overflow rejects attribution.
    #[doc(hidden)]
    pub fn snapshot_shm_lock_probe() -> serde_json::Value {
        SHM_LOCK_CAPTURE.with(|capture| {
            let state = capture.borrow();
            let conflicts = SHM_LOCK_BORROW_CONFLICTS.with(std::cell::Cell::get);
            serde_json::json!({"enabled":state.enabled,"files":&state.files[..state.used],
                "unknown":state.unknown,"overflow":state.overflow,"borrow_conflicts":conflicts,
                "native_shm_calls":state.calls,
                "attribution_proven":state.enabled && state.calls > 0 && state.files[..state.used].iter().any(|file|file.main_db)
                    && state.unknown == 0 && state.overflow == 0 && conflicts == 0,
                "limits":"Counters reflect native delegate results; shared_mask is inferred from successful shared lock/unlock calls, not OS lock introspection. File IDs are capture-local generations. Slots0..7 are SQLite SHM lock offsets."})
        })
    }
    #[doc(hidden)]
    pub fn end_shm_lock_probe() {
        SHM_LOCK_ENABLED.with(|enabled|enabled.set(false));
        SHM_LOCK_CAPTURE.with(|capture|capture.borrow_mut().enabled = false);
    }
    /// Begin an example-only per-thread VFS probe. Never nest probes on one thread.
    #[doc(hidden)]
    pub fn begin_vfs_timing_probe() {
        VFS_PROBE_TIMINGS.with(|metrics| metrics.set([VfsTiming::default(); 16]));
        VFS_PROBE_FILES.with(|files|files.set([VfsProbeFile::default(); 64]));
        VFS_PROBE_ENABLED.with(|enabled| enabled.set(true));
    }

    /// (callback, calls, accumulated nanoseconds, maximum single-call nanoseconds).
    /// Metrics can overlap through nested forwarding; they are not a duration partition.
    /// Close result metrics are returned codes; a missing delegate is always an unknown close kind.
    #[doc(hidden)]
    pub fn finish_vfs_timing_probe() -> [(&'static str, u64, u64, u64); 14] {
        VFS_PROBE_ENABLED.with(|enabled| enabled.set(false));
        let metrics = VFS_PROBE_TIMINGS.with(std::cell::Cell::get);
        std::array::from_fn(|i| (VFS_TIMING_NAMES[i], metrics[i].calls, metrics[i].total_ns, metrics[i].max_ns))
    }

    /// Additional native open/read metrics from the last explicitly enabled thread probe.
    /// Read after finish_vfs_timing_probe; its existing fourteen fields remain unchanged.
    #[doc(hidden)]
    pub fn vfs_open_read_timing_probe() -> [(&'static str, u64, u64, u64); 2] {
        let metrics=VFS_PROBE_TIMINGS.with(std::cell::Cell::get);
        std::array::from_fn(|index| {let i=index+14;
            (VFS_TIMING_NAMES[i],metrics[i].calls,metrics[i].total_ns,metrics[i].max_ns)})
    }
    pub fn for_paths(paths: &AppPaths) -> Result<Self> {
        let database_path = paths.db_dir().join("app.sqlite");
        let key = absolute_key(&database_path)?;
        let mut runtimes = runtime_map()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(runtime) = runtimes.get(&key) {
            return Ok(Self {
                inner: Arc::clone(runtime),
            });
        }
        let runtime = Arc::new(RuntimeInner {
            #[cfg(feature = "wp0333_connection_reuse_proof")]
            reuse: Mutex::new(None),
            #[cfg(test)]
            test_manual_checkpoint_panic: AtomicBool::new(false),
            #[cfg(test)]
            test_maintenance_unavailable: AtomicBool::new(false),
            maintenance: Mutex::new(None),
            database_path: key.clone(),
            paths: paths.clone(),
            admission: Mutex::new(AdmissionState::default()),
            admission_changed: Condvar::new(),
            registry: Mutex::new(RegistryState::default()),
            last_checkpoint: Mutex::new(None),
        });
        runtimes.insert(key, Arc::clone(&runtime));
        Ok(Self { inner: runtime })
    }

    pub fn database_path(&self) -> &Path {
        &self.inner.database_path
    }

    /// Owner metadata only; opens no SQLite connection and reserves no admission slot.
    pub fn checkpoint_maintenance_health(&self) -> CheckpointMaintenanceHealth {
        self.inner.maintenance_health()
    }

    /// Explicit post-schema/default-library gate. Clones share one canonical owner.
    pub fn start_checkpoint_maintenance(&self) -> Result<CheckpointMaintenanceGuard> {
        if self.inner.admission.lock().unwrap_or_else(|p|p.into_inner()).shutting_down {
            return Err(EngineError::DatabaseRuntime("maintenance_start_after_shutdown_refused".into()));
        }
        let shared={
            let mut owner=self.inner.maintenance.lock().unwrap_or_else(|p|p.into_inner());
            if let Some(owner)=owner.as_ref() {
                owner.shared.guards.fetch_add(1,Ordering::AcqRel);
                Arc::clone(&owner.shared)
            } else {
                let mut health=empty_maintenance_health();
                health.enabled=true;
                health.owner_state="initializing".into();
                let shared=Arc::new(MaintenanceShared {
                    state:Mutex::new(MaintenanceState{ready:false,state:"initializing",last_cycle:Instant::now(),health,final_receipt:None}),
                    wake:Condvar::new(),requests:Mutex::new(VecDeque::new()),stop:AtomicBool::new(false),guards:AtomicUsize::new(1),
                });
                let weak=Arc::downgrade(&self.inner);
                let worker_shared=Arc::clone(&shared);
                let handle=std::thread::Builder::new().name("voxvulgi-passive-checkpoint".into()).spawn(move || {
                    let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||maintenance_worker(weak,&worker_shared)));
                    let receipt=match result {
                        Ok(Ok(receipt))=>receipt,
                        Ok(Err(error))=>CheckpointMaintenanceShutdownReceipt{stop_requested:worker_shared.stop.load(Ordering::Acquire),joined:false,
                            elapsed_ms:0,final_checkpoint:None,owner_close_error:Some(error.to_string())},
                        Err(_)=>CheckpointMaintenanceShutdownReceipt{stop_requested:worker_shared.stop.load(Ordering::Acquire),joined:false,
                            elapsed_ms:0,final_checkpoint:None,owner_close_error:Some("maintenance_worker_panicked".into())},
                    };
                    let mut state=worker_shared.state.lock().unwrap_or_else(|p|p.into_inner());
                    state.state=if receipt.owner_close_error.is_some(){"failed"}else{"stopped"};
                    state.health.last_error=receipt.owner_close_error.clone().or(state.health.last_error.clone());
                    state.final_receipt=Some(receipt);
                    drop(state);
                    reject_manual_checkpoints(&worker_shared,"maintenance_owner_exited");
                    worker_shared.wake.notify_all();
                })?;
                *owner=Some(MaintenanceOwner{shared:Arc::clone(&shared),handle:Some(handle)});
                shared
            }
        };
        let guard=CheckpointMaintenanceGuard{runtime:Arc::downgrade(&self.inner),shared};
        let deadline=Instant::now()+SHUTDOWN_DRAIN_TIMEOUT;
        let mut state=guard.shared.state.lock().unwrap_or_else(|p|p.into_inner());
        while !state.ready && state.final_receipt.is_none() {
            if Instant::now()>=deadline {
                drop(state);
                return Err(EngineError::DatabaseRuntime("maintenance_initialization_timeout_unjoined".into()));
            }
            state=guard.shared.wake.wait_timeout(state,deadline.saturating_duration_since(Instant::now()))
                .unwrap_or_else(|p|p.into_inner()).0;
        }
        if !state.ready || state.state!="running" || guard.shared.stop.load(Ordering::Acquire) {
            let error=state.health.last_error.clone().unwrap_or_else(||"maintenance_not_running".into());
            drop(state);
            return Err(EngineError::DatabaseRuntime(error));
        }
        drop(state);
        Ok(guard)
    }

    #[cfg(test)]
    pub(crate) fn set_test_maintenance_unavailable(&self, unavailable: bool) {
        self.inner.test_maintenance_unavailable.store(unavailable,Ordering::Release);
        self.inner.admission_changed.notify_all();
    }

    pub fn write_context(&self, context: DatabaseOperationContext) -> Result<DatabaseWriteContext> {
        #[cfg(test)]
        {
            #[cfg(feature = "wp0333_connection_reuse_proof")]
            let prepare_fixture=!self.inner.reuse_enabled();
            #[cfg(not(feature = "wp0333_connection_reuse_proof"))]
            let prepare_fixture=true;
            // Production startup owns schema creation/migration before any runtime admission.
            // Unit fixtures invoke post-ready write APIs directly, so give write fixtures the
            // same predecessor state without weakening read-only no-creation semantics or adding
            // migration to a production path.
            if prepare_fixture {
            if let Some(db_dir) = self.inner.database_path.parent() {
                std::fs::create_dir_all(db_dir)?;
            }
            let fixture_connection = open_write_raw(&self.inner.database_path)?;
            super::migrate(&fixture_connection)?;
            }
        }
        let permit = self.inner.acquire_writer(&context)?;
        let read_meter = ReadMeter::start();
        let open_started = Instant::now();
        #[cfg(feature = "wp0333_connection_reuse_proof")]
        let opened=if self.inner.reuse_enabled() {
            self.inner.reuse_checkout(true).and_then(|cached| match cached {
                Some(c)=>Ok(c),None=>open_runtime_write_raw(&self.inner.database_path,self.inner.maintained_writer_policy()).and_then(|c|{self.inner.reuse_opened(true,&c)?;Ok(c)})
            })
        } else {open_runtime_write_raw(&self.inner.database_path,self.inner.maintained_writer_policy())};
        #[cfg(not(feature = "wp0333_connection_reuse_proof"))]
        let opened=open_runtime_write_raw(&self.inner.database_path,self.inner.maintained_writer_policy());
        match opened {
            Ok(connection) => {
                permit.phase("open", open_started.elapsed());
                let initial_total_changes = connection.total_changes();
                Ok(DatabaseWriteContext {
                    connection: Some(connection),
                    permit: Some(permit),
                    initial_total_changes,
                    read_meter,
                    connection_use_started: Instant::now(),
                })
            }
            Err(error) => {
                let mut permit = permit;
                permit.outcome = if is_busy_error(&error) {
                    "open_busy_external_or_unknown"
                } else {
                    "open_failed"
                };
                Err(error)
            }
        }
    }

    pub fn read_context(&self, context: DatabaseOperationContext) -> Result<DatabaseReadContext> {
        let permit = self.inner.acquire_reader(&context)?;
        let read_meter = ReadMeter::start();
        let open_started = Instant::now();
        #[cfg(feature = "wp0333_connection_reuse_proof")]
        let opened=if self.inner.reuse_enabled() {
            self.inner.reuse_checkout(false).and_then(|cached| match cached {
                Some(c)=>Ok(c),None=>open_readonly_raw(&self.inner.database_path).and_then(|c|{self.inner.reuse_opened(false,&c)?;Ok(c)})
            })
        } else {open_readonly_raw(&self.inner.database_path)};
        #[cfg(not(feature = "wp0333_connection_reuse_proof"))]
        let opened=open_readonly_raw(&self.inner.database_path);
        match opened {
            Ok(connection) => {
                self.inner.operation_metadata(
                    permit.operation_id,
                    Some(("open", open_started.elapsed())),
                    None,
                    None,
                );
                Ok(DatabaseReadContext {
                    connection: Some(connection),
                    permit: Some(permit),
                    read_meter,
                    connection_use_started: Instant::now(),
                })
            }
            Err(error) => {
                let mut permit = permit;
                permit.outcome = if is_busy_error(&error) {
                    "open_busy_external_or_unknown"
                } else {
                    "open_failed"
                };
                Err(error)
            }
        }
    }

    pub fn read<T>(
        &self,
        context: DatabaseOperationContext,
        operation: impl FnOnce(&Connection) -> Result<T>,
    ) -> Result<T> {
        let mut database = self.read_context(context)?;
        let result = operation(&database);
        database.mark_outcome(match &result {
            Ok(_) => "completed",
            Err(error) if is_busy_error(error) => "busy_external_or_unknown",
            Err(_) => "failed",
        });
        result
    }

    pub fn write<T>(
        &self,
        context: DatabaseOperationContext,
        behavior: TransactionBehavior,
        operation: impl FnOnce(&rusqlite::Transaction<'_>) -> Result<T>,
    ) -> Result<T> {
        self.write_attempt(context, behavior, 0, operation)
    }

    fn write_attempt<T>(
        &self,
        context: DatabaseOperationContext,
        behavior: TransactionBehavior,
        retry_count: u32,
        operation: impl FnOnce(&rusqlite::Transaction<'_>) -> Result<T>,
    ) -> Result<T> {
        let mut database = self.write_context(context)?;
        if let Some(permit) = database.permit.as_mut() {
            permit.retry_count = retry_count;
        }
        let (connection, permit) = (&mut database.connection, &mut database.permit);
        if let Some(permit) = permit.as_ref() {
            permit.transaction_behavior(behavior);
        }
        let begin_started = Instant::now();
        let transaction = match connection
            .as_mut()
            .expect("database connection present")
            .transaction_with_behavior(behavior)
        {
            Ok(transaction) => {
                if let Some(permit) = permit.as_ref() {
                    permit.phase("begin", begin_started.elapsed());
                }
                transaction
            }
            Err(error) => {
                let error = EngineError::Database(error);
                if let Some(permit) = permit.as_mut() {
                    permit.outcome = if is_busy_error(&error) {
                        permit.runtime.busy_outcome(permit.operation_id, "begin")
                    } else {
                        "begin_failed"
                    };
                }
                return Err(error);
            }
        };
        let execute_started = Instant::now();
        let value = match operation(&transaction) {
            Ok(value) => value,
            Err(error) => {
                drop(transaction);
                if let Some(permit) = permit.as_mut() {
                    permit.outcome = if is_busy_error(&error) {
                        permit.runtime.busy_outcome(permit.operation_id, "execute")
                    } else {
                        "rolled_back"
                    };
                }
                return Err(error);
            }
        };
        if let Some(permit) = permit.as_ref() {
            permit.phase("execute", execute_started.elapsed());
        }
        let commit_started = Instant::now();
        match transaction.commit() {
            Ok(()) => {
                if let Some(permit) = permit.as_mut() {
                    permit.phase("commit", commit_started.elapsed());
                    permit.outcome = "committed";
                }
                Ok(value)
            }
            Err(error) => {
                let error = EngineError::Database(error);
                if let Some(permit) = permit.as_mut() {
                    permit.outcome = if is_busy_error(&error) {
                        permit.runtime.busy_outcome(permit.operation_id, "commit")
                    } else {
                        "commit_failed"
                    };
                }
                Err(error)
            }
        }
    }

    pub fn write_idempotent<T>(
        &self,
        context: DatabaseOperationContext,
        behavior: TransactionBehavior,
        mut operation: impl FnMut(&rusqlite::Transaction<'_>) -> Result<T>,
    ) -> Result<T> {
        let mut attempt = 0;
        loop {
            if context.cancellation.is_cancelled() {
                return Err(EngineError::DatabaseRuntime(format!(
                    "idempotent_retry_cancelled; operation={}; database={}",
                    context.operation,
                    self.database_path().display()
                )));
            }
            let result = self.write_attempt(context.clone(), behavior, attempt, |transaction| {
                operation(transaction)
            });
            match result {
                Err(error) if is_busy_error(&error) && attempt < IDEMPOTENT_RETRY_LIMIT => {
                    let delays = [20_u64, 50, 100];
                    let seed = context
                        .lane
                        .bytes()
                        .chain(context.operation.bytes())
                        .fold(attempt as u64, |value, byte| {
                            value.wrapping_mul(33).wrapping_add(u64::from(byte))
                        });
                    let delay =
                        Duration::from_millis(delays[attempt as usize].saturating_add(seed % 11));
                    let sleep_deadline = Instant::now() + delay;
                    while Instant::now() < sleep_deadline {
                        if context.cancellation.is_cancelled() {
                            return Err(EngineError::DatabaseRuntime(format!(
                                "idempotent_retry_cancelled; operation={}; database={}",
                                context.operation,
                                self.database_path().display()
                            )));
                        }
                        std::thread::sleep(
                            sleep_deadline
                                .saturating_duration_since(Instant::now())
                                .min(Duration::from_millis(10)),
                        );
                    }
                    attempt += 1;
                }
                result => return result,
            }
        }
    }

    pub fn snapshot(&self) -> DatabaseRuntimeSnapshot {
        let checkpoint_maintenance=self.inner.maintenance_health();
        // These static linked-library getters open no database and reserve no admission slot.
        let sqlite_version = rusqlite::version().to_owned();
        let sqlite_version_number = rusqlite::version_number();
        // SAFETY: SQLite guarantees a static NUL-terminated source identity string.
        let sqlite_source_id = unsafe {
            std::ffi::CStr::from_ptr(rusqlite::ffi::sqlite3_sourceid())
        }.to_string_lossy().into_owned();
        let admission = self
            .inner
            .admission
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let registry = self
            .inner
            .registry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        DatabaseRuntimeSnapshot {
            checkpoint_maintenance,
            database_path: self.inner.database_path.clone(),
            sqlite_version,
            sqlite_version_number,
            sqlite_source_id,
            writer_capacity: WRITER_QUEUE_CAPACITY,
            waiting_writers: admission.waiting_writers.len(),
            writer_active: admission.writer_active,
            read_executor_limit: READ_EXECUTOR_LIMIT,
            read_admission_capacity: READ_ADMISSION_CAPACITY,
            active_readers: admission.active_readers,
            waiting_readers: admission.waiting_readers.len(),
            shutting_down: admission.shutting_down,
            active_operations: registry.active.values().cloned().collect(),
            recent_receipts: registry.receipts.iter().cloned().collect(),
        }
    }

    pub fn wal_health(&self) -> WalHealth {
        let snapshot = self.snapshot();
        let observed_at_ms = now_ms();
        let mut admitted_readers = snapshot
            .active_operations
            .iter()
            .filter(|operation| {
                operation.mode == DatabaseMode::Read && operation.admitted_at_ms.is_some()
            })
            .cloned()
            .collect::<Vec<_>>();
        admitted_readers.sort_by_key(|operation| operation.admitted_at_ms);
        let oldest_reader_age_ms = admitted_readers
            .first()
            .and_then(|operation| operation.admitted_at_ms)
            .map(|admitted_at_ms| observed_at_ms.saturating_sub(admitted_at_ms));
        let long_reader_candidates = admitted_readers
            .into_iter()
            .filter(|operation| {
                operation
                    .admitted_at_ms
                    .map(|admitted_at_ms| {
                        observed_at_ms.saturating_sub(admitted_at_ms) >= LONG_READER_WARNING_MS
                    })
                    .unwrap_or(false)
            })
            .collect();
        let wal_path = PathBuf::from(format!("{}-wal", self.inner.database_path.display()));
        let shm_path = PathBuf::from(format!("{}-shm", self.inner.database_path.display()));
        WalHealth {
            checkpoint_maintenance: self.inner.maintenance_health(),
            database_path: self.inner.database_path.clone(),
            wal_path: wal_path.clone(),
            wal_bytes: std::fs::metadata(wal_path)
                .map(|metadata| metadata.len())
                .unwrap_or(0),
            shm_bytes: std::fs::metadata(shm_path)
                .map(|metadata| metadata.len())
                .unwrap_or(0),
            active_readers: snapshot.active_readers,
            writer_active: snapshot.writer_active,
            waiting_writers: snapshot.waiting_writers,
            oldest_reader_age_ms,
            long_reader_candidates,
            last_checkpoint: self
                .inner
                .last_checkpoint
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone(),
        }
    }

    /// Runs the only runtime checkpoint mode exposed by the service. Callers must schedule this
    /// as maintenance; foreground projections never invoke checkpoint work implicitly.
    pub fn checkpoint_passive(&self) -> Result<WalCheckpointReceipt> {
        let shared=self.inner.maintenance_shared().ok_or_else(||EngineError::DatabaseRuntime("maintenance_unavailable".into()))?;
        let (operation_id,started)=self.inner.register(&DatabaseOperationContext::new("database_maintenance","wal_checkpoint_passive").maintenance(),DatabaseMode::Maintenance);
        let (reply,receive)=mpsc::sync_channel(1);
        let mut requests=shared.requests.lock().unwrap_or_else(|p|p.into_inner());
        let running=shared.state.lock().unwrap_or_else(|p|p.into_inner()).state=="running";
        if shared.stop.load(Ordering::Acquire) || !running || requests.len()>=1 {
            drop(requests);
            self.inner.finish(operation_id,started,"maintenance_manual_unavailable_or_overload",0);
            return Err(EngineError::DatabaseRuntime("maintenance_manual_unavailable_or_overload".into()));
        }
        requests.push_back(ManualCheckpoint{runtime:Arc::downgrade(&self.inner),operation_id,started,reply,replied:false});
        drop(requests);
        shared.wake.notify_all();
        match receive.recv_timeout(SHUTDOWN_DRAIN_TIMEOUT) {
            Ok(Ok(receipt))=>Ok(receipt),
            Ok(Err(error))=>Err(EngineError::DatabaseRuntime(error)),
            Err(_)=>Err(EngineError::DatabaseRuntime("maintenance_manual_wait_timeout_or_owner_exited; terminal_receipt_pending".into())),
        }
    }
    pub fn contention_receipt(&self, error: &EngineError) -> Option<DatabaseContentionReceipt> {
        if !is_busy_error(error) {
            return None;
        }
        Some(self.contention_snapshot())
    }

    /// Snapshot the admitted internal operations that could plausibly participate in a busy or
    /// locked result. Queued operations are deliberately excluded: their presence cannot prove
    /// that VoxVulgi owns the SQLite lock.
    pub fn contention_snapshot(&self) -> DatabaseContentionReceipt {
        let snapshot = self.snapshot();
        let admitted_internal_candidates = snapshot
            .active_operations
            .into_iter()
            .filter(|operation| operation.admitted_at_ms.is_some())
            .collect::<Vec<_>>();
        DatabaseContentionReceipt {
            classification: if admitted_internal_candidates.is_empty() {
                "external_or_unknown"
            } else {
                "internal_candidates"
            }
            .to_string(),
            database_path: snapshot.database_path,
            active_internal_candidates: admitted_internal_candidates,
            recent_receipts: snapshot.recent_receipts,
        }
    }

    pub fn shutdown_and_drain(&self, timeout: Duration) -> Result<()> {
        if self.inner.maintenance.lock().unwrap_or_else(|p|p.into_inner()).as_ref()
            .is_some_and(|owner|owner.handle.is_some()) {
            return Err(EngineError::DatabaseRuntime("maintenance_owner_not_joined".into()));
        }
        let shutdown_context =
            DatabaseOperationContext::new("database_shutdown", "shutdown_and_drain_reconcile")
                .maintenance();
        let (operation_id, operation_started) = self
            .inner
            .register(&shutdown_context, DatabaseMode::Maintenance);
        self.inner.admitted(operation_id, Duration::ZERO);
        let deadline = Instant::now() + timeout;
        let mut state = self
            .inner
            .admission
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.shutting_down = true;
        self.inner.admission_changed.notify_all();
        while state.writer_active
            || state.active_readers > 0
            || !state.waiting_readers.is_empty()
            || !state.waiting_writers.is_empty()
        {
            let now = Instant::now();
            if now >= deadline {
                let detail = format!(
                    "shutdown_drain_timeout; writer_active={}; readers={}; waiting_readers={}; waiting_writers={}",
                    state.writer_active,
                    state.active_readers,
                    state.waiting_readers.len(),
                    state.waiting_writers.len()
                );
                drop(state);
                self.inner.finish(
                    operation_id,
                    operation_started,
                    "shutdown_drain_timeout_reconciled_snapshot",
                    0,
                );
                return Err(EngineError::DatabaseRuntime(detail));
            }
            let (next, _) = self
                .inner
                .admission_changed
                .wait_timeout(
                    state,
                    deadline
                        .saturating_duration_since(now)
                        .min(Duration::from_millis(50)),
                )
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state = next;
        }
        drop(state);
        #[cfg(feature = "wp0333_connection_reuse_proof")]
        if let Err(error)=self.inner.shutdown_reuse(deadline) {
            self.inner.finish(operation_id,operation_started,"shutdown_reuse_not_closed",0);
            return Err(error);
        }
        self.inner
            .finish(operation_id, operation_started, "shutdown_drained", 0);
        Ok(())
    }
}

fn absolute_key(database_path: &Path) -> Result<PathBuf> {
    let absolute = if database_path.is_absolute() {
        database_path.to_path_buf()
    } else {
        std::env::current_dir()?.join(database_path)
    };
    if absolute.exists() {
        return Ok(std::fs::canonicalize(absolute)?);
    }
    let parent = absolute.parent().ok_or_else(|| {
        EngineError::DatabaseRuntime(format!(
            "database path has no parent: {}",
            absolute.display()
        ))
    })?;
    let canonical_parent = std::fs::canonicalize(parent)?;
    let file_name = absolute.file_name().ok_or_else(|| {
        EngineError::DatabaseRuntime(format!(
            "database path has no file name: {}",
            absolute.display()
        ))
    })?;
    Ok(canonical_parent.join(file_name))
}

fn is_busy_error(error: &EngineError) -> bool {
    matches!(
        error,
        EngineError::Database(rusqlite::Error::SqliteFailure(sqlite, _))
            if matches!(sqlite.code, ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Barrier, MutexGuard};

    // Paste inside database_runtime::tests after supplying an ACTUAL guarded,
    // feature-enabled disposable fixture. These helpers never enable reuse or open
    // operator roots. Parent owns integration/compilation/execution.
    #[cfg(feature = "wp0333_connection_reuse_proof")]
    fn independent_reuse_committed_contamination(database: &AppDatabase) {
        let attached = tempfile::tempdir().expect("owned attached fixture");
        let attached_path = attached.path().join("attached.sqlite");
        {
            let fixture = Connection::open(&attached_path).expect("create owned attachment fixture");
            fixture.execute_batch("CREATE TABLE attached_fixture(value INTEGER);").unwrap();
        }
        let before = database.connection_reuse_proof().expect("guard enabled");
        {
            let writer = database.write_context(DatabaseOperationContext::new("independent", "commit_then_contaminate")).unwrap();
            writer.execute_batch("CREATE TABLE independent_reuse_probe(value INTEGER); INSERT INTO independent_reuse_probe VALUES(7);").unwrap();
            writer.busy_handler(Some(|_| false)).unwrap();
            writer.execute_batch("CREATE TEMP TABLE independent_temp(value INTEGER);").unwrap();
            writer.execute("ATTACH DATABASE ?1 AS independent_attached", [attached_path.to_str().unwrap()]).unwrap();
        }
        let quarantined = database.connection_reuse_proof().unwrap();
        assert_eq!(quarantined.quarantines, before.quarantines + 1);
        let writer = database.write_context(DatabaseOperationContext::new("independent", "fresh_after_contamination")).unwrap();
        assert_eq!(writer.query_row("SELECT value FROM independent_reuse_probe", [], |r| r.get::<_, i64>(0)).unwrap(), 7);
        assert_eq!(writer.query_row("PRAGMA busy_timeout", [], |r| r.get::<_, i64>(0)).unwrap(), 10000);
        let names: Vec<String> = writer.prepare("PRAGMA database_list").unwrap().query_map([], |r| r.get(1)).unwrap().collect::<rusqlite::Result<_>>().unwrap();
        assert_eq!(names, vec!["main"]);
        assert!(writer.prepare("SELECT * FROM independent_temp").is_err());
        assert!(writer.prepare("SELECT * FROM independent_attached.sqlite_schema").is_err());
        assert!(writer.is_autocommit());
        drop(writer);
    }

    #[cfg(feature = "wp0333_connection_reuse_proof")]
    fn independent_reuse_panic_and_checked_close(database: &AppDatabase, owner: &mut CheckpointMaintenanceGuard) {
        {
            let writer = database.write_context(DatabaseOperationContext::new("independent", "durable_before_panic")).unwrap();
            writer.execute_batch("CREATE TABLE independent_panic_probe(value INTEGER); INSERT INTO independent_panic_probe VALUES(7);").unwrap();
        }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let writer = database.write_context(DatabaseOperationContext::new("independent", "panic_manual_transaction")).unwrap();
            writer.execute_batch("BEGIN; UPDATE independent_panic_probe SET value=99;").unwrap();
            panic!("owned independent lease panic");
        }));
        assert!(result.is_err());
        let writer = database.write_context(DatabaseOperationContext::new("independent", "readback_and_unfinalized_statement")).unwrap();
        assert!(writer.is_autocommit());
        assert_eq!(writer.query_row("SELECT value FROM independent_panic_probe", [], |r| r.get::<_, i64>(0)).unwrap(), 7);
        // Deliberate C-level statement escape exercises SQLite checked-close refusal.
        // No Rust statement/cache lifetime can accidentally finalize this probe.
        let mut statement = std::ptr::null_mut();
        unsafe {
            assert_eq!(rusqlite::ffi::sqlite3_prepare_v2(writer.handle(), b"SELECT 1\0".as_ptr().cast(), -1, &mut statement, std::ptr::null_mut()), rusqlite::ffi::SQLITE_OK);
        }
        assert!(!statement.is_null());
        struct OwnedStatement(*mut rusqlite::ffi::sqlite3_stmt);
        impl Drop for OwnedStatement {
            fn drop(&mut self) {
                if !self.0.is_null() { unsafe { rusqlite::ffi::sqlite3_finalize(self.0); } }
            }
        }
        let mut statement_owner = OwnedStatement(statement);
        drop(writer);
        let joined = owner.stop_and_join(SHUTDOWN_DRAIN_TIMEOUT).unwrap();
        assert!(joined.joined);
        let failure = database.shutdown_and_drain(SHUTDOWN_DRAIN_TIMEOUT).expect_err("live statement cannot yield successful drain");
        let retained = database.connection_reuse_proof().unwrap();
        assert!(retained.close_errors >= 1, "{failure}");
        assert!(retained.remaining_owners >= 1);
        assert!(!retained.shutdown_joined);
        let statement = std::mem::replace(&mut statement_owner.0, std::ptr::null_mut());
        unsafe { assert_eq!(rusqlite::ffi::sqlite3_finalize(statement), rusqlite::ffi::SQLITE_OK); }
        // Owned failed-close retry must be supported/explicitly integrated before
        // executing this probe, so the deliberately retained owner cannot leak.
        database.shutdown_and_drain(SHUTDOWN_DRAIN_TIMEOUT).expect("finalized owner closes on explicit retry");
        let closed = database.connection_reuse_proof().unwrap();
        assert_eq!(closed.remaining_owners, 0);
        assert!(closed.shutdown_joined);
    }

    #[cfg(feature = "wp0333_connection_reuse_proof")]
    #[test]
    fn connection_reuse_independent_committed_session_quarantine() {
        let _serial=serial_test_guard();
        let (_dir,database,_guard,mut owner)=reuse_fixture();
        independent_reuse_committed_contamination(&database);
        reuse_finish(&database,&mut owner);
    }

    #[cfg(feature = "wp0333_connection_reuse_proof")]
    #[test]
    fn connection_reuse_independent_panic_and_unfinalized_owner() {
        let _serial=serial_test_guard();
        let (_dir,database,_guard,mut owner)=reuse_fixture();
        independent_reuse_panic_and_checked_close(&database,&mut owner);
    }

    #[cfg(feature = "wp0333_connection_reuse_proof")]
    fn reuse_fixture() -> (tempfile::TempDir,AppDatabase,ConnectionReuseGuard,CheckpointMaintenanceGuard) {
        use sha2::{Digest,Sha256};
        let directory=tempfile::tempdir().unwrap();let backup_dir=directory.path().join("source_backup");std::fs::create_dir(&backup_dir).unwrap();let source=backup_dir.join("source.sqlite");
        let mut c=open_write_raw(&source).unwrap();super::super::migrate(&c).unwrap();
        let tx=c.transaction().unwrap();
        for i in 0..1000 {tx.execute("INSERT INTO job(id,type,status,progress,params_json,created_at_ms,logs_path) VALUES(?1,'download_direct_url','queued',0,'{}',0,'fixture')",[format!("reuse-fixture-{i}")]).unwrap();}
        tx.commit().unwrap();drop(c);
        let root=directory.path().join("fixture");std::fs::create_dir(&root).unwrap();let root=root.canonicalize().unwrap();
        std::fs::create_dir(root.join("db")).unwrap();std::fs::copy(&source,root.join("db/app.sqlite")).unwrap();
        let hash=hex::encode(Sha256::digest(std::fs::read(&source).unwrap()));
        std::fs::write(root.join("wp0333_production_fixture.json"),serde_json::to_vec(&serde_json::json!({"root":root,"source":source.canonicalize().unwrap(),"source_sha256":hash,"schema":61,"job_density":1000,"pid":std::process::id()})).unwrap()).unwrap();
        let database=AppDatabase::for_paths(&AppPaths::new(root.clone())).unwrap();
        let guard=database.enable_disposable_connection_reuse(&root,&hash).unwrap();
        assert!(database.read_context(DatabaseOperationContext::new("reuse_test","before_owner")).is_err());
        let owner=database.start_checkpoint_maintenance().unwrap();(directory,database,guard,owner)
    }

    #[cfg(feature = "wp0333_connection_reuse_proof")]
    fn reuse_finish(database: &AppDatabase, owner: &mut CheckpointMaintenanceGuard) {
        let deadline=Instant::now()+SHUTDOWN_DRAIN_TIMEOUT;
        assert!(owner.stop_and_join(deadline.saturating_duration_since(Instant::now())).unwrap().joined);
        database.shutdown_and_drain(deadline.saturating_duration_since(Instant::now())).unwrap();
    }

    #[cfg(feature = "wp0333_connection_reuse_proof")]
    #[test]
    fn connection_reuse_actual_factory_bound_and_logical_returns() {
        let _serial=serial_test_guard();let (_dir,database,_guard,mut owner)=reuse_fixture();
        for _ in 0..5 {let c=database.write_context(DatabaseOperationContext::new("reuse_test","writer")).unwrap();assert_eq!(c.pragma_query_value(None,"synchronous",|r|r.get::<_,i64>(0)).unwrap(),2);drop(c);}
        let readers:Vec<_>=(0..READ_EXECUTOR_LIMIT).map(|_|database.read_context(DatabaseOperationContext::new("reuse_test","reader")).unwrap()).collect();
        for c in &readers {assert_eq!(c.pragma_query_value(None,"query_only",|r|r.get::<_,i64>(0)).unwrap(),1);assert_eq!(c.pragma_query_value(None,"foreign_keys",|r|r.get::<_,i64>(0)).unwrap(),1);assert_eq!(c.pragma_query_value(None,"busy_timeout",|r|r.get::<_,i64>(0)).unwrap(),4000);}
        drop(readers);
        let c=database.read_context(DatabaseOperationContext::new("reuse_test","reader_reused")).unwrap();assert_eq!(c.pragma_query_value(None,"foreign_keys",|r|r.get::<_,i64>(0)).unwrap(),1);drop(c);
        let p=database.connection_reuse_proof().unwrap();assert_eq!(p.physical_writer_opens,1);assert_eq!(p.physical_reader_opens,4);assert_eq!(p.max_writer_owners,1);assert_eq!(p.max_reader_owners,4);assert_eq!(p.physical_closes,0);
        for r in database.snapshot().recent_receipts.iter().filter(|r|r.lane=="reuse_test"&&r.outcome.starts_with("completed")) {assert!(r.phase_ms.contains_key("lease_return"));assert!(!r.phase_ms.contains_key("connection_close"));}
        reuse_finish(&database,&mut owner);let p=database.connection_reuse_proof().unwrap();assert_eq!(p.physical_closes,5);assert_eq!(p.remaining_owners,0);assert!(p.shutdown_joined);
    }

    #[cfg(feature = "wp0333_connection_reuse_proof")]
    #[test]
    fn connection_reuse_manual_transactions_never_become_committed_ack() {
        let _serial=serial_test_guard();let (_dir,database,_guard,mut owner)=reuse_fixture();
        {let c=database.write_context(DatabaseOperationContext::new("reuse_test","manual")).unwrap();c.execute_batch("BEGIN; INSERT INTO meta(key,value) VALUES('reuse_dirty','uncommitted')").unwrap();}
        let count=database.read(DatabaseOperationContext::new("reuse_test","readback"),|c|Ok(c.query_row("SELECT COUNT(*) FROM meta WHERE key='reuse_dirty'",[],|r|r.get::<_,i64>(0))?)).unwrap();assert_eq!(count,0);
        assert!(database.snapshot().recent_receipts.iter().any(|r|r.operation=="manual"&&r.outcome=="manual_transaction_not_committed"));
        {let c=database.read_context(DatabaseOperationContext::new("reuse_test","manual_reader")).unwrap();c.execute_batch("BEGIN").unwrap();c.pragma_update(None,"query_only",false).unwrap();}
        let c=database.read_context(DatabaseOperationContext::new("reuse_test","clean_reader")).unwrap();assert!(c.is_autocommit());assert_eq!(c.pragma_query_value(None,"query_only",|r|r.get::<_,i64>(0)).unwrap(),1);drop(c);reuse_finish(&database,&mut owner);
    }

    #[cfg(feature = "wp0333_connection_reuse_proof")]
    #[test]
    fn connection_reuse_cleanup_failure_preserves_committed_outcome() {
        let _serial=serial_test_guard();let (_dir,database,_guard,mut owner)=reuse_fixture();
        database.inner.reuse.lock().unwrap().as_mut().unwrap().fail_reset=true;
        database.write(DatabaseOperationContext::new("reuse_test","committed_then_reset_failure"),TransactionBehavior::Immediate,|tx|{tx.execute("INSERT INTO meta(key,value) VALUES('reuse_committed','yes')",[])?;Ok(())}).unwrap();
        database.inner.reuse.lock().unwrap().as_mut().unwrap().fail_reset=false;
        assert!(database.snapshot().recent_receipts.iter().any(|r|r.operation=="committed_then_reset_failure"&&r.outcome=="committed"));
        assert_eq!(database.read(DatabaseOperationContext::new("reuse_test","committed_readback"),|c|Ok(c.query_row("SELECT value FROM meta WHERE key='reuse_committed'",[],|r|r.get::<_,String>(0))?)).unwrap(),"yes");assert_eq!(database.connection_reuse_proof().unwrap().quarantines,1);reuse_finish(&database,&mut owner);
    }

    #[cfg(feature = "wp0333_connection_reuse_proof")]
    #[test]
    fn connection_reuse_late_close_retains_owned_join_until_actual_completion() {
        let _serial=serial_test_guard();let (_dir,database,_guard,mut owner)=reuse_fixture();
        drop(database.read_context(DatabaseOperationContext::new("reuse_test","close_late")).unwrap());owner.stop_and_join(SHUTDOWN_DRAIN_TIMEOUT).unwrap();
        database.inner.reuse.lock().unwrap().as_mut().unwrap().close_delay=Duration::from_millis(50);
        assert!(database.shutdown_and_drain(Duration::from_millis(1)).unwrap_err().to_string().contains("reuse_close_timeout_unjoined"));
        assert!(!database.connection_reuse_proof().unwrap().shutdown_joined);assert!(database.inner.reuse.lock().unwrap().as_ref().unwrap().closer.is_some());
        database.shutdown_and_drain(SHUTDOWN_DRAIN_TIMEOUT).unwrap();assert_eq!(database.connection_reuse_proof().unwrap().remaining_owners,0);
    }

    #[cfg(feature = "wp0333_connection_reuse_proof")]
    #[test]
    fn connection_reuse_concurrent_shutdown_reserves_one_owned_closer() {
        let _serial=serial_test_guard();let (_dir,database,_guard,mut owner)=reuse_fixture();
        drop(database.read_context(DatabaseOperationContext::new("reuse_test","concurrent_close")).unwrap());owner.stop_and_join(SHUTDOWN_DRAIN_TIMEOUT).unwrap();
        database.inner.reuse.lock().unwrap().as_mut().unwrap().close_delay=Duration::from_millis(25);
        let barrier=Arc::new(Barrier::new(3));let threads:Vec<_>=(0..2).map(|_|{
            let d=database.clone();let b=barrier.clone();std::thread::spawn(move||{b.wait();d.shutdown_and_drain(SHUTDOWN_DRAIN_TIMEOUT)})
        }).collect();barrier.wait();
        for thread in threads {if let Err(error)=thread.join().unwrap() {assert!(error.to_string().contains("in_progress"));}}
        database.shutdown_and_drain(SHUTDOWN_DRAIN_TIMEOUT).unwrap();let p=database.connection_reuse_proof().unwrap();
        assert_eq!(p.physical_closes,1);assert_eq!(p.remaining_owners,0);assert!(p.shutdown_joined);assert_eq!(p.physical_close_receipts.len(),1);
    }

    #[cfg(feature = "wp0333_connection_reuse_proof")]
    #[test]
    fn connection_reuse_close_panic_retains_exact_owner_and_reconciles_on_retry() {
        let _serial=serial_test_guard();let (_dir,database,_guard,mut owner)=reuse_fixture();
        drop(database.read_context(DatabaseOperationContext::new("reuse_test","panic_close")).unwrap());owner.stop_and_join(SHUTDOWN_DRAIN_TIMEOUT).unwrap();
        database.inner.reuse.lock().unwrap().as_mut().unwrap().close_panic=true;
        assert!(database.shutdown_and_drain(SHUTDOWN_DRAIN_TIMEOUT).is_err());
        {let p=database.inner.reuse.lock().unwrap_or_else(|p|p.into_inner());let p=p.as_ref().unwrap();assert_eq!(p.poisoned.len(),1);assert_eq!(p.proof.remaining_owners,1);assert!(!p.proof.shutdown_joined);}
        assert!(database.snapshot().active_operations.is_empty());
        assert!(database.snapshot().recent_receipts.iter().any(|r|r.outcome=="reuse_close_panicked_owner_retained"));
        database.inner.reuse.lock().unwrap_or_else(|p|p.into_inner()).as_mut().unwrap().close_panic=false;
        database.shutdown_and_drain(SHUTDOWN_DRAIN_TIMEOUT).unwrap();assert_eq!(database.connection_reuse_proof().unwrap().remaining_owners,0);
    }

    #[cfg(feature = "wp0333_connection_reuse_proof")]
    #[test]
    fn connection_reuse_unreconciled_ownership_permanently_refuses_replacement_and_drain() {
        let _serial=serial_test_guard();let (_dir,database,_guard,mut owner)=reuse_fixture();
        drop(database.read_context(DatabaseOperationContext::new("reuse_test","retained_owner")).unwrap());
        let before=database.connection_reuse_proof().unwrap();
        database.inner.reuse.lock().unwrap().as_mut().unwrap().ownership_unreconciled=true;
        assert!(database.read_context(DatabaseOperationContext::new("reuse_test","refused_reader")).err().unwrap().to_string().contains("ownership_unreconciled"));
        assert!(database.write_context(DatabaseOperationContext::new("reuse_test","refused_writer")).err().unwrap().to_string().contains("ownership_unreconciled"));
        assert_eq!(database.connection_reuse_proof().unwrap().physical_reader_opens,before.physical_reader_opens);
        assert_eq!(database.connection_reuse_proof().unwrap().physical_writer_opens,before.physical_writer_opens);
        owner.stop_and_join(SHUTDOWN_DRAIN_TIMEOUT).unwrap();
        assert!(database.shutdown_and_drain(SHUTDOWN_DRAIN_TIMEOUT).unwrap_err().to_string().contains("ownership_unreconciled"));
        assert!(!database.connection_reuse_proof().unwrap().shutdown_joined);
        // Only this test clears its simulated condition; real unreconciled ownership is permanent.
        database.inner.reuse.lock().unwrap().as_mut().unwrap().ownership_unreconciled=false;
        database.shutdown_and_drain(SHUTDOWN_DRAIN_TIMEOUT).unwrap();
    }

    #[cfg(not(feature = "wp0333_connection_reuse_proof"))]
    #[test]
    fn connection_reuse_feature_off_retains_physical_context_close() {
        let _serial=serial_test_guard();let (_dir,_paths,database)=fixture();
        AppDatabase::begin_vfs_timing_probe();drop(database.read_context(DatabaseOperationContext::new("reuse_default","fresh_close")).unwrap());
        let metrics=AppDatabase::finish_vfs_timing_probe();assert!(metrics.iter().any(|(name,count,_,_)|*name=="xClose"&&*count>0));
        assert!(database.snapshot().recent_receipts.iter().any(|r|r.operation=="fresh_close"&&r.phase_ms.contains_key("connection_close")));
    }

    #[test]
    fn checkpoint_maintenance_manual_cycle_panic_reconciles_exact_terminal_and_reply() {
        let _serial=serial_test_guard();
        let (_directory,_paths,database)=fixture();
        let mut owner=database.start_checkpoint_maintenance().unwrap();
        database.inner.test_manual_checkpoint_panic.store(true,Ordering::Release);
        let error=database.checkpoint_passive().expect_err("injected owner cycle panic");
        assert!(error.to_string().contains("maintenance_owner_panicked"),"{error}");
        let receipt=owner.stop_and_join(SHUTDOWN_DRAIN_TIMEOUT).unwrap();
        assert!(receipt.joined);
        assert!(receipt.owner_close_error.as_deref().unwrap().contains("maintenance_worker_panicked"));
        let snapshot=database.snapshot();
        assert!(snapshot.active_operations.iter().all(|operation|operation.mode!=DatabaseMode::Maintenance));
        let terminals:Vec<_>=snapshot.recent_receipts.iter().filter(|receipt|receipt.operation=="wal_checkpoint_passive").collect();
        assert_eq!(terminals.len(),1);
        assert_eq!(terminals[0].outcome,"maintenance_owner_panicked");
        assert!(terminals[0].admitted_at_ms.is_some());
    }

    #[test]
    fn checkpoint_maintenance_policy_owner_manual_and_shutdown() {
        let _serial=serial_test_guard();
        let (_directory,_paths,database)=fixture();
        let mut owner=database.start_checkpoint_maintenance().expect("owner");
        let second=database.start_checkpoint_maintenance().expect("same owner");
        assert!(Arc::ptr_eq(&owner.shared,&second.shared));
        drop(second);
        let writer=database.write_context(DatabaseOperationContext::new("test","full_policy")).expect("writer");
        assert_eq!(writer.query_row("PRAGMA synchronous",[],|r|r.get::<_,i64>(0)).unwrap(),2);
        assert_eq!(writer.query_row("PRAGMA wal_autocheckpoint",[],|r|r.get::<_,i64>(0)).unwrap(),0);
        assert!(writer.db_config(rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE).unwrap());
        // Manual PASSIVE is independent of the application writer lane, even while held.
        database.checkpoint_passive().expect("manual shared-owner request");
        assert!(database.snapshot().writer_active);
        drop(writer);
        let receipt=owner.stop_and_join(SHUTDOWN_DRAIN_TIMEOUT).expect("join");
        assert!(receipt.joined && receipt.stop_requested && receipt.owner_close_error.is_none());
        database.shutdown_and_drain(SHUTDOWN_DRAIN_TIMEOUT).expect("drain after owner");
    }

    #[test]
    fn checkpoint_maintenance_backlog_and_staleness_never_false_clear() {
        assert_eq!(checkpoint_backlog(-1,-1,4096),Some(0));
        assert_eq!(checkpoint_backlog(-2,0,4096),None);
        assert_eq!(checkpoint_backlog(1,2,4096),None);
        assert_eq!(checkpoint_backlog(i64::MAX,0,u64::MAX),Some(u64::MAX));
        let mut state=MaintenanceState{ready:true,state:"running",last_cycle:Instant::now(),health:empty_maintenance_health(),final_receipt:None};
        state.health.backlog_bytes=Some(CHECKPOINT_BACKLOG_LIMIT);
        assert_eq!(maintenance_gate(&state),Some("maintenance_backlog_limit"));
        state.health.consecutive_errors=3;
        assert_eq!(maintenance_gate(&state),Some("maintenance_unavailable"));
        state.health.consecutive_errors=0;
        state.health.backlog_bytes=Some(0);
        state.last_cycle=Instant::now()-CHECKPOINT_STALE_TIMEOUT;
        assert_eq!(maintenance_gate(&state),Some("maintenance_unavailable"));
    }

    #[test]
    fn checkpoint_maintenance_pinned_reader_partial_then_recovers() {
        let _serial=serial_test_guard();
        let (_directory,_paths,database)=fixture();
        let mut owner=database.start_checkpoint_maintenance().expect("owner");
        database.write_context(DatabaseOperationContext::new("test","seed"))
            .unwrap().execute("INSERT INTO meta(key,value) VALUES('seed','0')",[]).unwrap();
        let reader=database.read_context(DatabaseOperationContext::new("test","pin")).unwrap();
        reader.execute_batch("BEGIN").unwrap();
        reader.query_row("SELECT COUNT(*) FROM meta",[],|r|r.get::<_,i64>(0)).unwrap();
        for index in 0..8 {
            database.write_context(DatabaseOperationContext::new("test","write_while_pinned"))
                .unwrap().execute("INSERT INTO meta(key,value) VALUES(?1,?2)",rusqlite::params![format!("pin{index}"),"x".repeat(8192)]).unwrap();
        }
        let partial=database.checkpoint_passive().unwrap();
        assert!(partial.log_frames>partial.checkpointed_frames);
        assert!(database.snapshot().checkpoint_maintenance.backlog_bytes.unwrap()>0);
        reader.execute_batch("ROLLBACK").unwrap();
        drop(reader);
        let recovered=database.checkpoint_passive().unwrap();
        assert_eq!(recovered.log_frames,recovered.checkpointed_frames);
        assert_eq!(database.snapshot().checkpoint_maintenance.backlog_bytes,Some(0));
        owner.stop_and_join(SHUTDOWN_DRAIN_TIMEOUT).unwrap();
    }

    #[test]
    fn checkpoint_maintenance_shutdown_timeout_retains_owned_worker_until_quiescent() {
        let _serial=serial_test_guard();
        let (_directory,_paths,database)=fixture();
        let mut owner=database.start_checkpoint_maintenance().unwrap();
        let writer=database.write_context(DatabaseOperationContext::new("test","held_atomic" )).unwrap();
        let error=owner.stop_and_join(Duration::from_millis(2)).unwrap_err();
        assert!(error.to_string().contains("quiesce_timeout_unjoined"));
        assert!(database.shutdown_and_drain(Duration::from_millis(2)).unwrap_err().to_string().contains("owner_not_joined"));
        writer.execute("INSERT INTO meta(key,value) VALUES('finish_after_deadline','yes')",[]).unwrap();
        drop(writer);
        assert!(owner.stop_and_join(SHUTDOWN_DRAIN_TIMEOUT).unwrap().joined);
        database.shutdown_and_drain(SHUTDOWN_DRAIN_TIMEOUT).unwrap();
    }

    #[test]
    fn checkpoint_maintenance_rechecks_queued_writer_and_quiesces_without_force() {
        let _serial=serial_test_guard();
        let (_directory,_paths,database)=fixture();
        let mut owner=database.start_checkpoint_maintenance().expect("owner");
        let writer=database.write_context(DatabaseOperationContext::new("test","already_admitted")).expect("writer");
        let queued_db=database.clone();
        let queued=std::thread::spawn(move||queued_db.write_context(DatabaseOperationContext::new("test","queued_before_pressure")).map(|_|()));
        let deadline=Instant::now()+Duration::from_secs(2);
        while database.snapshot().waiting_writers==0 {assert!(Instant::now()<deadline);std::thread::yield_now();}
        // Prevent background refresh during this exact permit-acquisition boundary.
        owner.shared.signal_stop();
        writer.execute("INSERT INTO meta(key,value) VALUES('atomic_survives','yes')",[]).expect("admitted write remains valid");
        drop(writer);
        assert!(queued.join().unwrap().unwrap_err().to_string().contains("maintenance_unavailable"));
        owner.stop_and_join(SHUTDOWN_DRAIN_TIMEOUT).expect("quiesce and join");
        let readonly=open_readonly_raw(database.database_path()).expect("independent read");
        assert_eq!(readonly.query_row("SELECT value FROM meta WHERE key='atomic_survives'",[],|r|r.get::<_,String>(0)).unwrap(),"yes");
        assert!(readonly.db_config(rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE).unwrap());
    }

    fn serial_test_guard() -> MutexGuard<'static, ()> {
        static SERIAL: OnceLock<Mutex<()>> = OnceLock::new();
        SERIAL
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn fixture() -> (tempfile::TempDir, AppPaths, AppDatabase) {
        let directory = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(directory.path().join("owned_disposable_app_data"));
        paths.ensure_dirs().expect("ensure disposable dirs");
        let connection = open_write_raw(&paths.db_dir().join("app.sqlite")).expect("open raw");
        connection
            // Pin to the current schema version so the runtime applies no migrations to this
            // meta-only fixture (migrations touch tables the fixture intentionally lacks).
            .execute_batch(&format!(
                "CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL); PRAGMA user_version={};",
                crate::db::CURRENT_SCHEMA_VERSION
            ))
            .expect("minimal isolated runtime fixture");
        drop(connection);
        let database = AppDatabase::for_paths(&paths).expect("runtime");
        (directory, paths, database)
    }

    #[test]
    fn writer_lane_is_fifo_serial_and_reconciles_every_admitted_write() {
        let _serial = serial_test_guard();
        let (_directory, _paths, database) = fixture();
        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let admission_order = Arc::new(Mutex::new(Vec::new()));
        let held = database
            .inner
            .acquire_writer_with_timeout(
                &DatabaseOperationContext::new("test", "fifo_gate"),
                Duration::from_secs(30),
            )
            .expect("hold writer permit");
        let mut workers = Vec::new();
        for sequence in 0..8_i64 {
            let worker_database = database.clone();
            let active = Arc::clone(&active);
            let maximum = Arc::clone(&maximum);
            let admission_order = Arc::clone(&admission_order);
            workers.push(std::thread::spawn(move || {
                let permit = worker_database.inner.acquire_writer_with_timeout(
                    &DatabaseOperationContext::new(format!("lane_{sequence}"), "fifo_admission"),
                    Duration::from_secs(30),
                )?;
                let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                maximum.fetch_max(current, Ordering::SeqCst);
                admission_order
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .push(sequence);
                std::thread::sleep(Duration::from_millis(2));
                active.fetch_sub(1, Ordering::SeqCst);
                drop(permit);
                Ok::<(), EngineError>(())
            }));
            let deadline = Instant::now() + Duration::from_secs(2);
            while database.snapshot().waiting_writers < sequence as usize + 1 {
                assert!(Instant::now() < deadline, "writer did not enter FIFO queue");
                std::thread::yield_now();
            }
        }
        drop(held);
        for worker in workers {
            worker.join().expect("worker join").expect("writer result");
        }

        assert_eq!(maximum.load(Ordering::SeqCst), 1, "writers overlapped");
        assert_eq!(
            *admission_order
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
            (0..8_i64).collect::<Vec<_>>()
        );
        let terminal = database
            .snapshot()
            .recent_receipts
            .into_iter()
            .filter(|receipt| {
                receipt.operation == "fifo_admission"
                    && receipt.outcome == "completed_write_context"
            })
            .count();
        assert_eq!(terminal, 8, "every admitted write has a terminal receipt");
    }

    #[test]
    fn nested_writer_admission_is_rejected_without_waiting() {
        let _serial = serial_test_guard();
        let (_directory, _paths, database) = fixture();
        let _held = database
            .write_context(DatabaseOperationContext::new("test", "outer"))
            .expect("outer writer");
        let started = Instant::now();
        let error = match database.write_context(DatabaseOperationContext::new("test", "inner")) {
            Ok(_) => panic!("nested writer must fail"),
            Err(error) => error,
        };
        assert!(started.elapsed() < Duration::from_millis(250));
        assert!(error
            .to_string()
            .contains("nested_writer_admission_rejected"));
    }

    #[test]
    fn writer_queue_overload_and_pre_admission_cancellation_are_explicit() {
        let _serial = serial_test_guard();
        let (_directory, _paths, database) = fixture();
        let held = database
            .write_context(DatabaseOperationContext::new("test", "overload_gate"))
            .expect("hold writer");
        let barrier = Arc::new(Barrier::new(WRITER_QUEUE_CAPACITY));
        let mut cancellations = Vec::new();
        let mut workers = Vec::new();
        for index in 0..(WRITER_QUEUE_CAPACITY - 1) {
            let database = database.clone();
            let cancellation = DatabaseCancellation::default();
            cancellations.push(cancellation.clone());
            let barrier = Arc::clone(&barrier);
            workers.push(std::thread::spawn(move || {
                barrier.wait();
                database.write_context(DatabaseOperationContext {
                    lane: format!("saturated_{index}"),
                    operation: "queued".to_string(),
                    request_id: None,
                    priority: DatabasePriority::Background,
                    cancellation,
                    batch_identity: None,
                })
            }));
        }
        barrier.wait();
        let deadline = Instant::now() + Duration::from_secs(3);
        while database.snapshot().waiting_writers < WRITER_QUEUE_CAPACITY - 1 {
            assert!(Instant::now() < deadline, "writer queue did not saturate");
            std::thread::yield_now();
        }
        let overflow_database = database.clone();
        let error = match std::thread::spawn(move || {
            overflow_database.write_context(DatabaseOperationContext::new("test", "overflow"))
        })
        .join()
        .expect("overflow join")
        {
            Ok(_) => panic!("overflow admission must fail"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("writer_queue_overloaded"));
        for cancellation in cancellations {
            cancellation.cancel();
        }
        drop(held);
        for worker in workers {
            let error = match worker.join().expect("join") {
                Ok(_) => panic!("cancelled queued writer must fail"),
                Err(error) => error,
            };
            assert!(error.to_string().contains("cancelled_before_admission"));
        }
        assert_eq!(database.snapshot().waiting_writers, 0);
    }

    #[test]
    fn reader_executor_limit_is_bounded_and_cancellable() {
        let _serial = serial_test_guard();
        let (_directory, _paths, database) = fixture();
        let readers = (0..READ_EXECUTOR_LIMIT)
            .map(|index| {
                database
                    .read_context(DatabaseOperationContext::new(
                        format!("reader_{index}"),
                        "held_read",
                    ))
                    .expect("read slot")
            })
            .collect::<Vec<_>>();
        assert_eq!(database.snapshot().active_readers, READ_EXECUTOR_LIMIT);

        let cancellation = DatabaseCancellation::default();
        let worker_cancellation = cancellation.clone();
        let worker_database = database.clone();
        let worker = std::thread::spawn(move || {
            worker_database.read_context(DatabaseOperationContext {
                lane: "overflow_reader".to_string(),
                operation: "bounded_read".to_string(),
                request_id: None,
                priority: DatabasePriority::Foreground,
                cancellation: worker_cancellation,
                batch_identity: None,
            })
        });
        let deadline = Instant::now() + Duration::from_secs(1);
        while database.snapshot().active_operations.len() <= READ_EXECUTOR_LIMIT {
            assert!(Instant::now() < deadline, "reader did not wait for a slot");
            std::thread::yield_now();
        }
        cancellation.cancel();
        let error = match worker.join().expect("join") {
            Ok(_) => panic!("cancelled reader must fail"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("cancelled_before_admission"));
        drop(readers);
        assert_eq!(database.snapshot().active_readers, 0);
    }

    #[test]
    fn reader_admission_queue_is_bounded_and_overload_is_immediate() {
        let _serial = serial_test_guard();
        let (_directory, _paths, database) = fixture();
        let held_readers = (0..READ_EXECUTOR_LIMIT)
            .map(|index| {
                database
                    .read_context(DatabaseOperationContext::new(
                        format!("reader_capacity_gate_{index}"),
                        "held_read_for_admission_overload",
                    ))
                    .expect("hold reader executor slot")
            })
            .collect::<Vec<_>>();

        let barrier = Arc::new(Barrier::new(READ_ADMISSION_CAPACITY + 1));
        let mut cancellations = Vec::with_capacity(READ_ADMISSION_CAPACITY);
        let mut workers = Vec::with_capacity(READ_ADMISSION_CAPACITY);
        for index in 0..READ_ADMISSION_CAPACITY {
            let worker_database = database.clone();
            let cancellation = DatabaseCancellation::default();
            cancellations.push(cancellation.clone());
            let barrier = Arc::clone(&barrier);
            workers.push(std::thread::spawn(move || {
                barrier.wait();
                worker_database.read_context(DatabaseOperationContext {
                    lane: format!("queued_reader_{index}"),
                    operation: "read_admission_queue_fill".to_string(),
                    request_id: Some(format!("queued-reader-{index}")),
                    priority: DatabasePriority::Background,
                    cancellation,
                    batch_identity: None,
                })
            }));
        }
        barrier.wait();

        let deadline = Instant::now() + Duration::from_secs(2);
        while database.snapshot().waiting_readers < READ_ADMISSION_CAPACITY {
            assert!(
                Instant::now() < deadline,
                "reader admission queue did not reach its declared capacity"
            );
            std::thread::yield_now();
        }

        let started = Instant::now();
        let error = match database.read_context(DatabaseOperationContext::new(
            "test",
            "read_admission_overflow",
        )) {
            Ok(_) => panic!("reader admission beyond capacity must fail"),
            Err(error) => error,
        };
        assert!(
            started.elapsed() < Duration::from_millis(250),
            "overload must be rejected without waiting for the admission timeout"
        );
        assert!(error.to_string().contains("read_admission_overloaded"));
        assert_eq!(database.snapshot().waiting_readers, READ_ADMISSION_CAPACITY);
        let overload_receipts = database
            .snapshot()
            .recent_receipts
            .into_iter()
            .filter(|receipt| {
                receipt.operation == "read_admission_overflow"
                    && receipt.outcome == "read_admission_overloaded"
            })
            .collect::<Vec<_>>();
        assert_eq!(overload_receipts.len(), 1);
        assert_eq!(overload_receipts[0].admitted_at_ms, None);
        assert_eq!(overload_receipts[0].execution_ms, None);

        for cancellation in cancellations {
            cancellation.cancel();
        }
        for worker in workers {
            let error = match worker.join().expect("queued reader join") {
                Ok(_) => panic!("cancelled queued reader must fail"),
                Err(error) => error,
            };
            assert!(error.to_string().contains("cancelled_before_admission"));
        }
        drop(held_readers);
        let snapshot = database.snapshot();
        assert_eq!(snapshot.active_readers, 0);
        assert_eq!(snapshot.waiting_readers, 0);
    }

    #[test]
    fn canonical_database_path_aliases_share_exactly_one_runtime() {
        let _serial = serial_test_guard();
        let (_directory, paths, database) = fixture();
        let alias_segment = paths.base_dir.join("path_alias_segment");
        std::fs::create_dir_all(&alias_segment).expect("create alias segment");
        let alias_paths = AppPaths::new(alias_segment.join("..").to_path_buf());
        let alias_database = AppDatabase::for_paths(&alias_paths).expect("runtime through alias");

        assert_eq!(database.database_path(), alias_database.database_path());
        assert!(
            Arc::ptr_eq(&database.inner, &alias_database.inner),
            "canonical aliases must map to the same runtime instance"
        );

        let reader = database
            .read_context(DatabaseOperationContext::new("test", "alias_shared_state"))
            .expect("reader through canonical path");
        assert_eq!(
            alias_database.snapshot().active_readers,
            1,
            "alias must observe the canonical runtime's admission state"
        );
        drop(reader);
    }

    #[test]
    fn reader_lane_is_fifo_and_newcomers_cannot_bypass_waiters() {
        let _serial = serial_test_guard();
        let (_directory, _paths, database) = fixture();
        let held_readers = (0..READ_EXECUTOR_LIMIT)
            .map(|index| {
                database
                    .read_context(DatabaseOperationContext::new(
                        format!("reader_fifo_gate_{index}"),
                        "reader_fifo_gate",
                    ))
                    .expect("hold reader slot")
            })
            .collect::<Vec<_>>();
        let admission_order = Arc::new(Mutex::new(Vec::new()));
        let release = Arc::new(Barrier::new(5));
        let mut workers = Vec::new();
        for sequence in 0..4_u64 {
            let worker_database = database.clone();
            let admission_order = Arc::clone(&admission_order);
            let release = Arc::clone(&release);
            workers.push(std::thread::spawn(move || {
                let reader = worker_database.read_context(DatabaseOperationContext::new(
                    format!("reader_fifo_{sequence}"),
                    "reader_fifo_admission",
                ))?;
                admission_order
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .push(sequence);
                release.wait();
                drop(reader);
                Ok::<(), EngineError>(())
            }));
            let deadline = Instant::now() + Duration::from_secs(2);
            while database.snapshot().waiting_readers < sequence as usize + 1 {
                assert!(Instant::now() < deadline, "reader did not enter FIFO queue");
                std::thread::yield_now();
            }
        }
        for (expected_sequence, reader) in held_readers.into_iter().enumerate() {
            drop(reader);
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                let observed = admission_order
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .clone();
                if observed.len() > expected_sequence {
                    assert_eq!(observed[expected_sequence], expected_sequence as u64);
                    break;
                }
                assert!(Instant::now() < deadline, "queued reader was not admitted");
                std::thread::yield_now();
            }
        }
        release.wait();
        for worker in workers {
            worker.join().expect("reader join").expect("reader result");
        }
        assert_eq!(
            *admission_order
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
            vec![0, 1, 2, 3]
        );
    }

    #[test]
    fn linked_sqlite_identity_matches_sql_and_fixed_bundled_release() {
        let _serial = serial_test_guard();
        let (_directory, _paths, database) = fixture();
        let identity = database.snapshot();
        let sql_identity = database.read(DatabaseOperationContext::new("test", "sqlite_identity"), |connection| {
            Ok(connection.query_row("SELECT sqlite_version(), sqlite_source_id()", [], |row|
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?)
        }).expect("actual linked SQL identity");
        assert_eq!(identity.sqlite_version, sql_identity.0);
        assert_eq!(identity.sqlite_source_id, sql_identity.1);
        assert!(identity.sqlite_version_number >= 3_051_003, "WAL-reset fix is a hard prerequisite");
        assert_eq!(identity.sqlite_version_number, 3_053_002);
        assert_eq!(identity.sqlite_version, "3.53.2");
        assert_eq!(identity.sqlite_source_id, "2026-06-03 19:12:13 d6e03d8c777cfa2d35e3b60d8ec3e0187f3e9f99d8e2ee9cac695fd6fcdf1a24");
    }

    #[test]
    #[cfg(windows)]
    fn sqlite_open_filename_preserves_windows_namespace_hazards() {
        for text in [
            r"\\server\share\app.sqlite", r"\\?\UNC\server\share\app.sqlite",
            r"\\.\C:\app.sqlite", r"\\?\GLOBALROOT\Device\app.sqlite",
            r"\\?\Volume{a}\app.sqlite", r"C:\app.sqlite", r"\\?\C:relative",
            r"\\?\C:\", r"\\?\C:\dir\\app.sqlite", r"\\?\C:\dir\app.sqlite\",
            r"\\?\C:\dir\.\app.sqlite", r"\\?\C:\dir\..\app.sqlite",
            r"\\?\C:\dir/app.sqlite", r"\\?\C:\dir.\app.sqlite",
            r"\\?\C:\dir \app.sqlite", r"\\?\C:\dir\app.sqlite.",
            r"\\?\C:\dir\app.sqlite ", r"\\?\C:\dir\app.sqlite:stream",
            r"\\?\C:\dir\NUL.tar.gz", r"\\?\C:\con.any.more\app.sqlite",
            r"\\?\C:\Con .txt\app.sqlite", r"\\?\C:\dir\COM¹.txt",
            r"\\?\C:\COM1 .txt\app.sqlite", r"\\?\C:\CON..foo\app.sqlite",
            r"\\?\C:\LPT²\app.sqlite", r"\\?\C:\dir\CONIN$",
            r"\\?\C:\CONOUT$.txt\app.sqlite", r"\\?\C:\dir\app?.sqlite",
            r"\\?\C:\dir\app*.sqlite", r"\\?\C:\dir\app|.sqlite",
            r"\\?\C:\dir\app<.sqlite", r"\\?\C:\dir\app>.sqlite",
            "\\\\?\\C:\\dir\\app\".sqlite", "\\\\?\\C:\\dir\\app\u{1f}.sqlite",
            "\\\\?\\C:\\dir\\app\0.sqlite",
        ] {
            let path = Path::new(text);
            assert_eq!(AppDatabase::sqlite_open_filename(path).as_ref().as_os_str(), path.as_os_str(), "altered {text:?}");
        }
        for base in ["CON", "PRN", "AUX", "NUL", "COM1", "COM9", "LPT1", "LPT9", "COM²", "COM³", "LPT¹", "LPT³"] {
            let path = PathBuf::from(format!(r"\\?\C:\{base}.one.two\app.sqlite"));
            assert_eq!(AppDatabase::sqlite_open_filename(&path).as_ref().as_os_str(), path.as_os_str());
        }
        use std::os::windows::ffi::OsStringExt;
        let mut units: Vec<u16> = r"\\?\C:\dir\".encode_utf16().collect();
        units.extend([0xd800, b'x' as u16]);
        let path = PathBuf::from(std::ffi::OsString::from_wide(&units));
        assert_eq!(AppDatabase::sqlite_open_filename(&path).as_ref().as_os_str(), path.as_os_str());
    }

    #[test]
    #[cfg(windows)]
    fn sqlite_open_filename_short_unicode_and_sidecar_boundaries() {
        for text in [r"\\?\C:\fixture\db\app.sqlite", r"\\?\d:\Ilja Smets\日本語\😀\app.sqlite", r"\\?\C:\COM0\not.NUL\app.sqlite"] {
            assert_eq!(AppDatabase::sqlite_open_filename(Path::new(text)).as_ref(), Path::new(&text[4..]));
        }
        // Supplementary characters occupy two UTF16 units; UTF8 byte counts are irrelevant.
        let short = format!(r"\\?\C:\{}", "😀".repeat(122));
        let boundary = format!(r"\\?\C:\{}x", "😀".repeat(122));
        assert_eq!(short[4..].encode_utf16().count(), 247);
        assert_eq!(boundary[4..].encode_utf16().count(), 248);
        assert_eq!(AppDatabase::sqlite_open_filename(Path::new(&short)).as_ref(), Path::new(&short[4..]));
        for text in [boundary, format!(r"\\?\C:\{}", "x".repeat(260))] {
            assert_eq!(AppDatabase::sqlite_open_filename(Path::new(&text)).as_ref().as_os_str(), Path::new(&text).as_os_str());
        }
        for suffix in ["-journal", "-wal", "-shm"] {
            assert!(format!("{}{suffix}", &short[4..]).encode_utf16().count() < 260);
        }
    }

    #[test]
    #[cfg(not(windows))]
    fn sqlite_open_filename_non_windows_is_unchanged() {
        for text in ["/tmp/app.sqlite", r"\\?\C:\db\app.sqlite", r"\\server\share\app.sqlite"] {
            assert_eq!(AppDatabase::sqlite_open_filename(Path::new(text)).as_ref(), Path::new(text));
        }
    }

    #[cfg(windows)]
    fn independent_windows_file_id(path: &Path) -> (u64, [u8; 16]) {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{FileIdInfo, GetFileInformationByHandleEx, FILE_ID_INFO};
        let file = std::fs::File::open(path).expect("independent native file open");
        let mut id = std::mem::MaybeUninit::<FILE_ID_INFO>::zeroed();
        let ok = unsafe { GetFileInformationByHandleEx(file.as_raw_handle(), FileIdInfo, id.as_mut_ptr().cast(), std::mem::size_of::<FILE_ID_INFO>() as u32) };
        assert_ne!(ok, 0, "native FileIdInfo failed: {}", std::io::Error::last_os_error());
        let id = unsafe { id.assume_init() };
        (id.VolumeSerialNumber, id.FileId.Identifier)
    }

    #[test]
    #[cfg(windows)]
    fn sqlite_open_filename_preserved_special_and_long_paths_keep_native_identity() {
        let directory = tempfile::tempdir().expect("tempdir");
        let root = directory.path().canonicalize().expect("canonical disposable root");
        let long_parent = root.join("a".repeat(100)).join("b".repeat(100)).join("c".repeat(100));
        let trailing_parent = root.join("trailing.");
        std::fs::create_dir_all(&long_parent).expect("create long verbatim directories");
        std::fs::create_dir_all(&trailing_parent).expect("create special verbatim directories");
        for path in [long_parent.join("app.sqlite"), trailing_parent.join("app.sqlite"), root.join("NUL.tar.gz"), root.join("COM¹.txt")] {
            std::fs::write(&path, b"owned filename identity sentinel").expect("create exact verbatim file");
            let adapted = AppDatabase::sqlite_open_filename(&path);
            assert_eq!(adapted.as_ref().as_os_str(), path.as_os_str());
            assert_eq!(independent_windows_file_id(&path), independent_windows_file_id(adapted.as_ref()));
        }
    }

    #[test]
    #[cfg(windows)]
    fn sqlite_open_filename_actual_runtime_tls_off_keeps_registry_and_physical_identity() {
        let _serial = serial_test_guard();
        AppDatabase::end_disposable_filename_probe();
        let (_directory, paths, database) = fixture();
        assert!(!AppDatabase::disposable_filename_probe_enabled());
        let canonical = database.database_path().to_path_buf();
        let plain = AppDatabase::sqlite_open_filename(&canonical).into_owned();
        assert_ne!(canonical.as_os_str(), plain.as_os_str(), "fixture must exercise conversion");
        assert_eq!(database.database_path(), canonical.as_path());
        assert_eq!(independent_windows_file_id(&canonical), independent_windows_file_id(&plain));
        let alias_root = AppDatabase::sqlite_open_filename(&paths.base_dir).into_owned();
        let alias = AppDatabase::for_paths(&AppPaths::new(alias_root)).expect("plain alias runtime");
        assert!(Arc::ptr_eq(&database.inner, &alias.inner));
        let reader = database.read_context(DatabaseOperationContext::new("test", "production_filename_read")).expect("runtime reader");
        let filename = unsafe { std::ffi::CStr::from_ptr(rusqlite::ffi::sqlite3_db_filename(reader.handle(), c"main".as_ptr())) };
        assert_eq!(filename.to_bytes(), plain.to_str().expect("Unicode fixture").as_bytes());
        drop(reader);
        database.write(DatabaseOperationContext::new("test", "production_filename_write"), TransactionBehavior::Immediate, |transaction| {
            let filename = unsafe { std::ffi::CStr::from_ptr(rusqlite::ffi::sqlite3_db_filename(transaction.handle(), c"main".as_ptr())) };
            assert_eq!(filename.to_bytes(), plain.to_str().expect("Unicode fixture").as_bytes());
            transaction.execute("INSERT INTO meta(key,value) VALUES('filename_identity','acknowledged')", [])?;
            Ok(())
        }).expect("acknowledged write");
        database.shutdown_and_drain(SHUTDOWN_DRAIN_TIMEOUT).expect("drain");
        // Independent original-spelling connection does not call either filename adapter.
        let independent = Connection::open_with_flags(&canonical, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).expect("independent canonical reopen");
        let value: String = independent.query_row("SELECT value FROM meta WHERE key='filename_identity'", [], |row| row.get(0)).expect("reconcile acknowledgement");
        assert_eq!(value, "acknowledged");
        assert_eq!(independent_windows_file_id(&canonical), independent_windows_file_id(&plain));
        assert!(!AppDatabase::disposable_filename_probe_enabled());
    }

    #[test]
    fn disposable_filename_probe_lexical_classes_and_length() {
        assert_eq!(short_extended_local_filename(Path::new(r"\\?\C:\fixture\db\app.sqlite")), Some(PathBuf::from(r"C:\fixture\db\app.sqlite")));
        for path in [r"\\server\share\app.sqlite", r"\\?\UNC\server\share\app.sqlite", r"\\.\C:\app.sqlite", r"C:\app.sqlite", r"\\?\Volume{a}\app.sqlite", r"\\?\C:relative"] {
            assert!(short_extended_local_filename(Path::new(path)).is_none(), "unsupported path accepted");
        }
        let long = format!(r"\\?\C:\{}", "x".repeat(253));
        assert_eq!(long.encode_utf16().count(), 260);
        assert!(short_extended_local_filename(Path::new(&long)).is_none());
        AppDatabase::end_disposable_filename_probe();
        let original = Path::new(r"\\?\C:\fixture\db\app.sqlite");
        assert!(matches!(AppDatabase::disposable_open_filename(original).unwrap(), std::borrow::Cow::Borrowed(_)));
        AppDatabase::enable_disposable_filename_probe(DisposableFilenameProbe { exact: original.to_path_buf(), plain: PathBuf::from(r"C:\fixture\db\app.sqlite") });
        assert_eq!(AppDatabase::disposable_open_filename(original).unwrap().as_ref(), Path::new(r"C:\fixture\db\app.sqlite"));
        assert!(AppDatabase::disposable_open_filename(Path::new(r"\\?\C:\other\app.sqlite")).is_err());
        AppDatabase::end_disposable_filename_probe();
        assert_eq!(AppDatabase::disposable_open_filename(original).unwrap().as_ref(), original);
    }

    #[cfg(windows)]
    #[test]
    fn disposable_filename_probe_validated_fixture_native_identity() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        std::fs::create_dir(root.join("db")).unwrap();
        let path = root.join("db/app.sqlite");
        assert!(AppDatabase::prepare_disposable_filename_probe(&root, &path).is_err(), "missing marker accepted");
        std::fs::write(root.join("wp0323_disposable_fixture.json"), b"{}").unwrap();
        assert!(AppDatabase::prepare_disposable_filename_probe(&root, &path).is_err(), "unowned marker accepted");
        std::fs::write(root.join("wp0323_disposable_fixture.json"), serde_json::to_vec(&serde_json::json!({"root":root,"pid":std::process::id()})).unwrap()).unwrap();
        std::fs::write(&path, b"existing fixture").unwrap();
        assert!(AppDatabase::prepare_disposable_filename_probe(&root, &path).is_err(), "existing database accepted");
        assert_eq!(std::fs::read(&path).unwrap(), b"existing fixture");
        std::fs::remove_file(&path).unwrap();
        let probe = AppDatabase::prepare_disposable_filename_probe(&root, &path).expect("validated disposable fixture");
        AppDatabase::enable_disposable_filename_probe(probe);
        let connection = Connection::open(AppDatabase::disposable_open_filename(&path).unwrap()).unwrap();
        connection.execute_batch("CREATE TABLE proof(value); INSERT INTO proof VALUES(73)").unwrap();
        let native = unsafe { std::ffi::CStr::from_ptr(rusqlite::ffi::sqlite3_db_filename(connection.handle(), c"main".as_ptr())) }.to_bytes();
        assert!(!native.starts_with(b"\\\\"), "native filename retained UNC locking spelling");
        connection.close().unwrap();
        let counted = open_counted_connection(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        assert_eq!(counted.query_row("SELECT value FROM proof", [], |row|row.get::<_, i64>(0)).unwrap(), 73);
        let native = unsafe { std::ffi::CStr::from_ptr(rusqlite::ffi::sqlite3_db_filename(counted.handle(), c"main".as_ptr())) }.to_bytes();
        assert!(!native.starts_with(b"\\\\"), "counted native filename retained UNC spelling");
        counted.close().unwrap();
        AppDatabase::end_disposable_filename_probe();
        let exact = Connection::open(&path).unwrap();
        assert_eq!(exact.query_row("SELECT value FROM proof", [], |row|row.get::<_, i64>(0)).unwrap(), 73, "spelling changed database identity");
        assert!(AppDatabase::prepare_disposable_filename_probe(&root, &root.join("db/other.sqlite")).is_err());
    }

    #[test]
    fn shm_lock_probe_disabled_mapping_generation_and_bounds() {
        AppDatabase::begin_shm_lock_probe();
        AppDatabase::end_shm_lock_probe();
        let file = 123_usize as *mut rusqlite::ffi::sqlite3_file;
        capture_shm_open(file, rusqlite::ffi::SQLITE_OPEN_MAIN_DB);
        capture_shm_result(file, 3, 1, 6, rusqlite::ffi::SQLITE_OK);
        assert_eq!(AppDatabase::snapshot_shm_lock_probe()["files"].as_array().unwrap().len(), 0);
        AppDatabase::begin_shm_lock_probe();
        capture_shm_open(file, rusqlite::ffi::SQLITE_OPEN_MAIN_DB);
        capture_shm_result(file, 3, 1, 6, rusqlite::ffi::SQLITE_OK);
        capture_shm_result(file, 3, 1, 5, rusqlite::ffi::SQLITE_IOERR);
        assert_eq!(AppDatabase::snapshot_shm_lock_probe()["files"][0]["shared_mask"], 8);
        capture_shm_result(file, 3, 1, 5, rusqlite::ffi::SQLITE_OK);
        capture_shm_result(file, 3, 1, 10, rusqlite::ffi::SQLITE_BUSY);
        capture_shm_result(file, 3, 1, 6, rusqlite::ffi::SQLITE_IOERR);
        capture_shm_close(file);
        capture_shm_open(file, rusqlite::ffi::SQLITE_OPEN_MAIN_DB); // Reused native address retains the closed generation.
        let snapshot = AppDatabase::snapshot_shm_lock_probe();
        assert_eq!(snapshot["files"][0]["slots"][3]["unlock_error"], 1);
        assert_eq!(snapshot["files"][0]["slots"][3]["exclusive_busy"], 1);
        assert_eq!(snapshot["files"][0]["slots"][3]["other_error"], 1);
        assert_eq!(snapshot["files"][0]["shared_mask"], 0);
        assert_eq!(snapshot["files"][0]["closed"], true);
        assert_eq!(snapshot["files"][1]["file_id"], 2);
        assert!(!snapshot.to_string().contains("pointer"));
        capture_shm_result(file, 7, 2, 6, rusqlite::ffi::SQLITE_OK);
        assert_eq!(AppDatabase::snapshot_shm_lock_probe()["attribution_proven"], false);
        SHM_LOCK_CAPTURE.with(|capture| {
            let _held = capture.borrow();
            capture_shm_result(file, 3, 1, 6, rusqlite::ffi::SQLITE_OK);
        });
        assert_eq!(AppDatabase::snapshot_shm_lock_probe()["borrow_conflicts"], 1);
        AppDatabase::begin_shm_lock_probe();
        for pointer in 1..=65 { capture_shm_open(pointer as *mut rusqlite::ffi::sqlite3_file, rusqlite::ffi::SQLITE_OPEN_MAIN_DB); }
        assert_eq!(AppDatabase::snapshot_shm_lock_probe()["overflow"], 1);
        assert_eq!(AppDatabase::snapshot_shm_lock_probe()["files"].as_array().unwrap().len(), 64);
        AppDatabase::end_shm_lock_probe();
    }

    #[test]
    fn shm_lock_probe_actual_wal_read_rollback_and_close() {
        let _serial = serial_test_guard();
        AppDatabase::begin_shm_lock_probe();
        AppDatabase::end_shm_lock_probe();
        let (_directory, _paths, database) = fixture();
        let writer = database.write_context(DatabaseOperationContext::new("test", "shm_seed")).expect("writer");
        writer.execute("INSERT INTO meta(key,value) VALUES('shm_seed','actual')", []).expect("real WAL frame");
        let disabled = open_counted_connection(&database.inner.database_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_FULL_MUTEX).expect("disabled reader");
        let value: String = disabled.query_row("SELECT value FROM meta WHERE key='shm_seed'", [], |row|row.get(0)).expect("disabled passthrough");
        assert_eq!(value, "actual");
        disabled.close().expect("disabled close");
        assert!(AppDatabase::snapshot_shm_lock_probe()["files"].as_array().unwrap().is_empty());
        AppDatabase::begin_shm_lock_probe();
        let reader = open_counted_connection(&database.inner.database_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_FULL_MUTEX).expect("reader");
        reader.execute_batch("BEGIN").expect("begin");
        let _: String = reader.query_row("SELECT value FROM meta WHERE key='shm_seed'", [], |row|row.get(0)).expect("snapshot");
        let during = AppDatabase::snapshot_shm_lock_probe();
        assert!(during["files"].as_array().unwrap().iter().any(|file| file["shared_mask"].as_u64().unwrap() != 0));
        reader.execute_batch("ROLLBACK").expect("rollback");
        let after = AppDatabase::snapshot_shm_lock_probe();
        let unlocks = |snapshot: &serde_json::Value| snapshot["files"].as_array().unwrap().iter()
            .flat_map(|file|file["slots"].as_array().unwrap())
            .map(|slot|slot["shared_unlock_ok"].as_u64().unwrap()).sum::<u64>();
        assert!(unlocks(&after) > unlocks(&during), "ROLLBACK must expose its actual shared unlock callback");
        assert_eq!(after["attribution_proven"], true);
        reader.close().expect("checked close");
        assert!(AppDatabase::snapshot_shm_lock_probe()["files"].as_array().unwrap().iter().all(|file| file["closed"] == true));
        AppDatabase::end_shm_lock_probe();
        AppDatabase::begin_shm_lock_probe();
        assert!(AppDatabase::snapshot_shm_lock_probe()["files"].as_array().unwrap().is_empty());
        AppDatabase::end_shm_lock_probe();
        drop(writer);
    }

    #[test]
    fn vfs_close_probe_classifies_single_delegate_results_and_resets() {
        AppDatabase::finish_vfs_timing_probe();
        let calls = std::cell::Cell::new(0);
        let close = |result| { calls.set(calls.get() + 1); result };
        let main = 101_usize as *mut rusqlite::ffi::sqlite3_file;
        let wal = 102_usize as *mut rusqlite::ffi::sqlite3_file;
        let unknown = 103_usize as *mut rusqlite::ffi::sqlite3_file;
        assert_eq!(timed_vfs_close(main, true, || close(rusqlite::ffi::SQLITE_IOERR)), rusqlite::ffi::SQLITE_IOERR);
        assert_eq!(calls.get(), 1);
        assert!(AppDatabase::finish_vfs_timing_probe().iter().all(|(_, count, _, _)| *count == 0));
        AppDatabase::begin_vfs_timing_probe();
        probe_file_open(main, rusqlite::ffi::SQLITE_OPEN_MAIN_DB);
        probe_file_open(wal, rusqlite::ffi::SQLITE_OPEN_WAL);
        assert_eq!(timed_vfs_close(main, true, || close(rusqlite::ffi::SQLITE_OK)), rusqlite::ffi::SQLITE_OK);
        probe_file_close(main);
        assert_eq!(timed_vfs_close(wal, true, || close(rusqlite::ffi::SQLITE_IOERR)), rusqlite::ffi::SQLITE_IOERR);
        probe_file_close(wal);
        timed_vfs_close(unknown, true, || close(rusqlite::ffi::SQLITE_OK));
        probe_file_open(main, rusqlite::ffi::SQLITE_OPEN_MAIN_DB);
        timed_vfs_close(main, false, || close(rusqlite::ffi::SQLITE_OK));
        probe_file_close(main);
        let metrics = AppDatabase::finish_vfs_timing_probe();
        assert_eq!(calls.get(), 5, "call must run exactly once per close");
        assert_eq!([metrics[9].1, metrics[10].1, metrics[11].1], [1, 1, 2], "missing native close cannot prove known kind");
        assert_eq!([metrics[12].1, metrics[13].1], [3, 1]);
        assert_eq!(metrics[0].1, metrics[9].1 + metrics[10].1 + metrics[11].1);
        assert_eq!(metrics[0].2, metrics[9].2 + metrics[10].2 + metrics[11].2);
        assert_eq!(metrics[0].1, metrics[12].1 + metrics[13].1);
        assert_eq!(metrics[0].2, metrics[12].2 + metrics[13].2);
        assert_eq!(metrics[0].3, metrics[9].3.max(metrics[10].3).max(metrics[11].3));
        AppDatabase::begin_vfs_timing_probe();
        assert!(AppDatabase::finish_vfs_timing_probe().iter().all(|(_, count, total, max)| *count == 0 && *total == 0 && *max == 0));
    }

    #[test]
    fn vfs_probe_records_actual_callbacks_and_resets_between_operations() {
        let _serial = serial_test_guard();
        let (_directory, _paths, database) = fixture();
        AppDatabase::begin_vfs_timing_probe();
        database.read(DatabaseOperationContext::new("test", "vfs_probe"), |connection| {
            Ok(connection.query_row("SELECT COUNT(*) FROM meta", [], |row| row.get::<_, i64>(0))?)
        }).expect("actual counted VFS read and close");
        let metrics = AppDatabase::finish_vfs_timing_probe();
        let native = AppDatabase::vfs_open_read_timing_probe();
        assert_eq!([native[0].0,native[1].0],["xOpen","xRead"]);
        assert!(native.iter().all(|(_,calls,total,max)|*calls>0 && total>=max),"actual native open/read delegates must be observed");
        assert_eq!(metrics[0].0, "xClose");
        assert!(metrics[0].1 > 0, "actual close must be observed");
        assert!(metrics[9].1 > 0, "actual main DB close must be classified");
        assert_eq!(metrics[11].1, 0, "actual fixture closes must have known kinds/delegates");
        assert_eq!(metrics[13].1, 0, "actual fixture closes must succeed");
        assert_eq!(metrics[0].1, metrics[9].1 + metrics[10].1 + metrics[11].1);
        assert_eq!(metrics[0].2, metrics[9].2 + metrics[10].2 + metrics[11].2);
        assert_eq!(metrics[0].1, metrics[12].1 + metrics[13].1);
        assert!(metrics.iter().all(|(_, _, total, max)| total >= max));
        AppDatabase::begin_vfs_timing_probe();
        assert!(AppDatabase::vfs_open_read_timing_probe().iter().all(|(_,calls,total,max)|*calls==0 && *total==0 && *max==0));
        {
            let writer = database.write_context(DatabaseOperationContext::new("test", "vfs_sync_kind")).expect("writer");
            writer.pragma_update(None, "synchronous", "FULL").expect("FULL sync fixture");
            writer.execute("INSERT INTO meta(key,value) VALUES('vfs_sync','row')", []).expect("WAL write");
            let _: (i64, i64, i64) = writer.query_row("PRAGMA wal_checkpoint(PASSIVE)", [], |row|
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))).expect("actual main-file checkpoint");
        }
        let classified = AppDatabase::finish_vfs_timing_probe();
        assert!(classified[5].1 > 0, "actual main DB sync must be classified");
        assert!(classified[6].1 > 0, "actual WAL sync must be classified");
        assert_eq!(classified[7].1, 0, "actual fixture file kinds must be known");
        assert_eq!(classified[8].1, 0, "actual fixture map must not overflow");
        assert_eq!(classified[3].1, classified[5].1 + classified[6].1 + classified[7].1);
        assert_eq!(classified[3].2, classified[5].2 + classified[6].2 + classified[7].2);
        AppDatabase::begin_vfs_timing_probe();
        let reset = AppDatabase::finish_vfs_timing_probe();
        assert!(reset.iter().all(|(_, calls, total, max)| *calls == 0 && *total == 0 && *max == 0));
        database.read(DatabaseOperationContext::new("test", "vfs_probe_disabled"), |connection| {
            Ok(connection.query_row("SELECT COUNT(*) FROM meta", [], |row| row.get::<_, i64>(0))?)
        }).expect("unprobed read");
        assert!(AppDatabase::finish_vfs_timing_probe().iter().all(|(_, calls, _, _)| *calls == 0));
        AppDatabase::begin_vfs_timing_probe();
        for pointer in 1..=65 {
            // Opaque test keys only; no dereference or synthetic database operation.
            probe_file_open(pointer as *mut rusqlite::ffi::sqlite3_file, rusqlite::ffi::SQLITE_OPEN_MAIN_DB);
        }
        assert_eq!(probe_sync_kind(66 as *mut rusqlite::ffi::sqlite3_file), 7);
        assert_eq!(AppDatabase::finish_vfs_timing_probe()[8].1, 1, "overflow must be explicit");
    }

    #[test]
    fn compatibility_context_receipts_are_terminal_and_report_changed_rows() {
        let _serial = serial_test_guard();
        let (_directory, _paths, database) = fixture();
        {
            let writer = database
                .write_context(DatabaseOperationContext::new("test", "context_write"))
                .expect("writer context");
            writer
                .execute("INSERT INTO meta(key,value) VALUES('receipt','row')", [])
                .expect("write row");
        }
        {
            let reader = database
                .read_context(DatabaseOperationContext::new("test", "context_read"))
                .expect("reader context");
            let _: i64 = reader
                .query_row("SELECT COUNT(*) FROM meta", [], |row| row.get(0))
                .expect("read row");
        }
        let snapshot = database.snapshot();
        let write = snapshot
            .recent_receipts
            .iter()
            .find(|receipt| receipt.operation == "context_write")
            .expect("write receipt");
        assert_eq!(write.outcome, "completed_write_context");
        assert_eq!(write.row_count, Some(1));
        let read = snapshot
            .recent_receipts
            .iter()
            .find(|receipt| receipt.operation == "context_read")
            .expect("read receipt");
        assert_eq!(read.outcome, "completed_read_context");
        for receipt in [write, read] {
            assert!(receipt.phase_ms.contains_key("connection_use"));
            assert!(receipt.phase_ms.contains_key("connection_close"));
            assert_eq!(receipt.phase_ms.len(), 3);
        }
    }

    #[test]
    fn queued_operations_are_not_reported_as_internal_lock_holders() {
        let _serial = serial_test_guard();
        let (_directory, _paths, database) = fixture();
        let context = DatabaseOperationContext::new("queued_only", "not_admitted");
        let (operation_id, started) = database.inner.register(&context, DatabaseMode::Write);
        let contention = database.contention_snapshot();
        assert_eq!(contention.classification, "external_or_unknown");
        assert!(contention.active_internal_candidates.is_empty());
        database
            .inner
            .finish(operation_id, started, "test_cleanup", 0);
    }

    #[test]
    fn cancellation_after_writer_admission_does_not_abandon_the_transaction() {
        let _serial = serial_test_guard();
        let (_directory, _paths, database) = fixture();
        let cancellation = DatabaseCancellation::default();
        let context = DatabaseOperationContext {
            lane: "canonical_write".to_string(),
            operation: "cancel_boundary".to_string(),
            request_id: Some("request-1".to_string()),
            priority: DatabasePriority::Foreground,
            cancellation: cancellation.clone(),
            batch_identity: None,
        };
        let mut connection = database.write_context(context).expect("admitted writer");
        cancellation.cancel();
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .expect("transaction");
        transaction
            .execute(
                "INSERT INTO meta(key,value) VALUES('cancel_boundary','committed')",
                [],
            )
            .expect("insert");
        transaction.commit().expect("commit");
        drop(connection);
        let value = database
            .read(
                DatabaseOperationContext::new("test", "cancel_boundary_read"),
                |connection| {
                    Ok(connection.query_row(
                        "SELECT value FROM meta WHERE key='cancel_boundary'",
                        [],
                        |row| row.get::<_, String>(0),
                    )?)
                },
            )
            .expect("canonical reread");
        assert_eq!(value, "committed");
    }

    #[test]
    fn read_context_is_query_only_and_wal_health_has_bounded_runtime_state() {
        let _serial = serial_test_guard();
        let (_directory, _paths, database) = fixture();
        let reader = database
            .read_context(DatabaseOperationContext::new("test", "query_only"))
            .expect("reader");
        let error = reader
            .execute(
                "INSERT INTO meta(key,value) VALUES('forbidden','write')",
                [],
            )
            .expect_err("read connection must reject writes");
        assert!(matches!(
            error,
            rusqlite::Error::SqliteFailure(_, _) | rusqlite::Error::SqlInputError { .. }
        ));
        let health = database.wal_health();
        assert_eq!(health.active_readers, 1);
        assert_eq!(health.database_path, database.database_path());
        assert!(health.oldest_reader_age_ms.is_some());
        assert!(health.last_checkpoint.is_none());
        drop(reader);
        let mut owner=database.start_checkpoint_maintenance().expect("maintenance owner");
        let checkpoint = database.checkpoint_passive().expect("passive checkpoint");
        let health = database.wal_health();
        assert_eq!(
            health
                .last_checkpoint
                .as_ref()
                .map(|receipt| receipt.checkpointed_frames),
            Some(checkpoint.checkpointed_frames)
        );
        owner.stop_and_join(SHUTDOWN_DRAIN_TIMEOUT).expect("owner joined");
    }

    #[test]
    fn shutdown_waits_for_admitted_work_then_refuses_new_admission() {
        let _serial = serial_test_guard();
        let (_directory, _paths, database) = fixture();
        let reader = database
            .read_context(DatabaseOperationContext::new("test", "drain_reader"))
            .expect("reader");
        let shutdown_database = database.clone();
        let shutdown = std::thread::spawn(move || {
            shutdown_database.shutdown_and_drain(Duration::from_secs(2))
        });
        let deadline = Instant::now() + Duration::from_secs(1);
        while !database.snapshot().shutting_down {
            assert!(Instant::now() < deadline, "shutdown state not visible");
            std::thread::yield_now();
        }
        drop(reader);
        shutdown.join().expect("join").expect("drain");
        let error =
            match database.read_context(DatabaseOperationContext::new("test", "after_shutdown")) {
                Ok(_) => panic!("new admission after shutdown must fail"),
                Err(error) => error,
            };
        assert!(error.to_string().contains("runtime_shutting_down"));
        let terminal = database
            .snapshot()
            .recent_receipts
            .into_iter()
            .filter(|receipt| {
                receipt.operation == "after_shutdown" && receipt.outcome == "runtime_shutting_down"
            })
            .collect::<Vec<_>>();
        assert_eq!(
            terminal.len(),
            1,
            "every refused post-shutdown admission needs exactly one terminal receipt"
        );
        assert_eq!(terminal[0].admitted_at_ms, None);
        assert_eq!(terminal[0].execution_ms, None);
    }

    #[test]
    fn shutdown_drains_admitted_write_to_canonical_state_and_one_terminal_receipt() {
        let _serial = serial_test_guard();
        let (_directory, _paths, database) = fixture();
        let (transaction_started_tx, transaction_started_rx) = std::sync::mpsc::channel();
        let (allow_commit_tx, allow_commit_rx) = std::sync::mpsc::channel();
        let writer_database = database.clone();
        let writer = std::thread::spawn(move || {
            writer_database.write(
                DatabaseOperationContext::new("shutdown_test", "drain_committed_write")
                    .with_request_id("shutdown-write-request"),
                TransactionBehavior::Immediate,
                |transaction| {
                    transaction.execute(
                        "INSERT INTO meta(key,value) VALUES('shutdown_write','committed')",
                        [],
                    )?;
                    transaction_started_tx
                        .send(())
                        .expect("signal admitted transaction");
                    allow_commit_rx
                        .recv()
                        .expect("release admitted transaction");
                    Ok(())
                },
            )
        });
        transaction_started_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("writer admitted before shutdown");

        let shutdown_database = database.clone();
        let shutdown = std::thread::spawn(move || {
            shutdown_database.shutdown_and_drain(Duration::from_secs(10))
        });
        let deadline = Instant::now() + Duration::from_secs(1);
        while !database.snapshot().shutting_down {
            assert!(Instant::now() < deadline, "shutdown state not visible");
            std::thread::yield_now();
        }
        allow_commit_tx.send(()).expect("allow canonical commit");
        writer.join().expect("writer join").expect("writer commit");
        shutdown.join().expect("shutdown join").expect("drain");

        let canonical = open_readonly_raw(database.database_path())
            .expect("independent canonical read after runtime shutdown");
        let value: String = canonical
            .query_row(
                "SELECT value FROM meta WHERE key='shutdown_write'",
                [],
                |row| row.get(0),
            )
            .expect("committed canonical row");
        assert_eq!(value, "committed");

        let terminal = database
            .snapshot()
            .recent_receipts
            .into_iter()
            .filter(|receipt| receipt.operation == "drain_committed_write")
            .collect::<Vec<_>>();
        assert_eq!(
            terminal.len(),
            1,
            "admitted write needs one terminal receipt"
        );
        assert_eq!(terminal[0].outcome, "committed");
        assert!(terminal[0].admitted_at_ms.is_some());
        assert!(terminal[0].execution_ms.is_some());
    }

    #[test]
    fn shutdown_timeout_emits_exactly_one_reconciled_terminal_failure_receipt() {
        let _serial = serial_test_guard();
        let (_directory, _paths, database) = fixture();
        let reader = database
            .read_context(DatabaseOperationContext::new(
                "shutdown_test",
                "reader_held_past_shutdown_deadline",
            ))
            .expect("hold admitted reader");

        let error = database
            .shutdown_and_drain(Duration::from_millis(5))
            .expect_err("held admitted reader must exhaust the shutdown deadline");
        assert!(error.to_string().contains("shutdown_drain_timeout"));
        let terminal = database
            .snapshot()
            .recent_receipts
            .into_iter()
            .filter(|receipt| receipt.operation == "shutdown_and_drain_reconcile")
            .collect::<Vec<_>>();
        assert_eq!(
            terminal.len(),
            1,
            "shutdown timeout needs exactly one reconciled terminal receipt"
        );
        assert_eq!(
            terminal[0].outcome,
            "shutdown_drain_timeout_reconciled_snapshot"
        );
        assert!(terminal[0].admitted_at_ms.is_some());
        assert!(terminal[0].execution_ms.is_some());
        drop(reader);
    }

    #[test]
    fn receipt_registry_is_bounded_and_redacts_sql_values() {
        let _serial = serial_test_guard();
        let (_directory, _paths, database) = fixture();
        for index in 0..(OPERATION_RECEIPT_CAPACITY + 20) {
            database
                .read(
                    DatabaseOperationContext::new("diagnostics", format!("projection_{index}")),
                    |connection| {
                        let _: i64 = connection.query_row("SELECT 1", [], |row| row.get(0))?;
                        Ok(())
                    },
                )
                .expect("read");
        }
        let snapshot = database.snapshot();
        assert_eq!(snapshot.recent_receipts.len(), OPERATION_RECEIPT_CAPACITY);
        let serialized = serde_json::to_string(&snapshot).expect("serialize");
        assert!(
            !serialized.contains("SELECT 1"),
            "raw SQL must not enter receipts"
        );
    }

    #[test]
    fn wp0324_receipts_attribute_file_bytes_read_to_the_operation() {
        let _serial = serial_test_guard();
        let (_directory, paths, database) = fixture();
        let raw = open_write_raw(&paths.db_dir().join("app.sqlite")).expect("open raw");
        raw.execute_batch(
            "CREATE TABLE wp0324_big(b BLOB NOT NULL);
             WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 24)
             INSERT INTO wp0324_big(b) SELECT randomblob(1048576) FROM n;",
        )
        .expect("seed 24 MiB table");
        drop(raw);

        // hex() needs every overflow page, so the scan must read the whole table from the file.
        let hex_chars: i64 = database
            .read(
                DatabaseOperationContext::new("test", "wp0324_heavy_scan"),
                |connection| {
                    Ok(connection.query_row(
                        "SELECT sum(length(hex(b))) FROM wp0324_big",
                        [],
                        |row| row.get(0),
                    )?)
                },
            )
            .expect("heavy read");
        assert_eq!(hex_chars, 24 * 1_048_576 * 2);
        let small: i64 = database
            .read(
                DatabaseOperationContext::new("test", "wp0324_small_read"),
                |connection| Ok(connection.query_row("SELECT 1", [], |row| row.get(0))?),
            )
            .expect("small read");
        assert_eq!(small, 1);

        let receipts = database.snapshot().recent_receipts;
        let heavy = receipts
            .iter()
            .rev()
            .find(|receipt| receipt.operation == "wp0324_heavy_scan")
            .expect("heavy receipt");
        assert!(
            heavy
                .file_bytes_read
                .is_some_and(|bytes| bytes >= HEAVY_READ_TRACE_BYTES),
            "heavy scan must record at least the trace threshold: {:?}",
            heavy.file_bytes_read
        );
        let small = receipts
            .iter()
            .rev()
            .find(|receipt| receipt.operation == "wp0324_small_read")
            .expect("small receipt");
        assert!(
            small
                .file_bytes_read
                .unwrap_or(0)
                < HEAVY_READ_TRACE_BYTES,
            "a small read must stay below the trace threshold: {:?}",
            small.file_bytes_read
        );
    }
}
