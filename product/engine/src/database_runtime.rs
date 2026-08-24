use super::{open_readonly_raw, open_write_raw};
use crate::paths::AppPaths;
use crate::{EngineError, Result};
use rusqlite::{Connection, ErrorCode, TransactionBehavior};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
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
pub const WRITER_BATCH_MAX_OPERATIONS: usize = 1;
pub const LONG_READER_WARNING_MS: u64 = 5_000;
pub const WRITER_FAIRNESS_POLICY: &str = "strict_fifo_no_priority_bypass";
pub const CHECKPOINT_POLICY: &str = "passive_maintenance_only_never_foreground";
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
}

#[derive(Debug, Clone, Serialize)]
pub struct DatabaseRuntimeSnapshot {
    pub database_path: PathBuf,
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

#[derive(Debug)]
struct RuntimeInner {
    database_path: PathBuf,
    admission: Mutex<AdmissionState>,
    admission_changed: Condvar,
    registry: Mutex<RegistryState>,
    last_checkpoint: Mutex<Option<WalCheckpointReceipt>>,
}

impl RuntimeInner {
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
        };
        self.registry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .active
            .insert(operation_id, operation);
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
            outcome: outcome.into(),
            batch_identity: operation.batch_identity,
            transaction_behavior: operation.transaction_behavior,
            phase_ms: operation.phase_ms,
            row_count: operation.row_count,
        });
        while registry.receipts.len() > OPERATION_RECEIPT_CAPACITY {
            registry.receipts.pop_front();
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

impl Drop for DatabaseWriteContext {
    fn drop(&mut self) {
        if let (Some(connection), Some(permit)) = (self.connection.as_ref(), self.permit.as_ref()) {
            let row_count = connection
                .total_changes()
                .saturating_sub(self.initial_total_changes);
            permit
                .runtime
                .operation_metadata(permit.operation_id, None, None, Some(row_count));
        }
        self.connection.take();
        self.permit.take();
    }
}

pub struct DatabaseReadContext {
    connection: Option<Connection>,
    permit: Option<ReaderPermit>,
}

impl DatabaseReadContext {
    fn mark_outcome(&mut self, outcome: &'static str) {
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
        self.connection.take();
        self.permit.take();
    }
}

#[derive(Clone, Debug)]
pub struct AppDatabase {
    inner: Arc<RuntimeInner>,
}

pub type DatabaseRuntime = AppDatabase;

impl AppDatabase {
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
            database_path: key.clone(),
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

    pub fn write_context(&self, context: DatabaseOperationContext) -> Result<DatabaseWriteContext> {
        #[cfg(test)]
        {
            // Production startup owns schema creation/migration before any runtime admission.
            // Unit fixtures invoke post-ready write APIs directly, so give write fixtures the
            // same predecessor state without weakening read-only no-creation semantics or adding
            // migration to a production path.
            if let Some(db_dir) = self.inner.database_path.parent() {
                std::fs::create_dir_all(db_dir)?;
            }
            let fixture_connection = open_write_raw(&self.inner.database_path)?;
            super::migrate(&fixture_connection)?;
        }
        let permit = self.inner.acquire_writer(&context)?;
        let open_started = Instant::now();
        match open_write_raw(&self.inner.database_path) {
            Ok(connection) => {
                permit.phase("open", open_started.elapsed());
                let initial_total_changes = connection.total_changes();
                Ok(DatabaseWriteContext {
                    connection: Some(connection),
                    permit: Some(permit),
                    initial_total_changes,
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
        let open_started = Instant::now();
        match open_readonly_raw(&self.inner.database_path) {
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
            database_path: self.inner.database_path.clone(),
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
        let started = Instant::now();
        let mut database = self.write_context(
            DatabaseOperationContext::new("database_maintenance", "wal_checkpoint_passive")
                .maintenance(),
        )?;
        let result = database.query_row("PRAGMA wal_checkpoint(PASSIVE)", [], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        });
        match result {
            Ok((busy, log_frames, checkpointed_frames)) => {
                if let Some(permit) = database.permit.as_mut() {
                    permit.outcome = if busy == 0 {
                        "checkpoint_completed"
                    } else {
                        "checkpoint_busy"
                    };
                }
                let receipt = WalCheckpointReceipt {
                    mode: "passive".to_string(),
                    busy,
                    log_frames,
                    checkpointed_frames,
                    elapsed_ms: started.elapsed().as_millis().min(u64::MAX as u128) as u64,
                };
                *self
                    .inner
                    .last_checkpoint
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(receipt.clone());
                Ok(receipt)
            }
            Err(error) => {
                if let Some(permit) = database.permit.as_mut() {
                    permit.outcome = "checkpoint_failed";
                }
                Err(error.into())
            }
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
            .execute_batch(
                "CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL); PRAGMA user_version=54;",
            )
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
        let checkpoint = database.checkpoint_passive().expect("passive checkpoint");
        let health = database.wal_health();
        assert_eq!(
            health
                .last_checkpoint
                .as_ref()
                .map(|receipt| receipt.checkpointed_frames),
            Some(checkpoint.checkpointed_frames)
        );
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
}
