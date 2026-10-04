//! Disposable non-cfg(test) AppDatabase connection-lifetime probe.
//! Usage: --root <new-absolute-directory> [--seconds 20] [--database-copy <schema59-through61-backup>] [--workload meta|job_insert] [--reader-interval-ms 0..1000] [--writer-interval-ms 0..1000] [--reader-count 1..16] [--writer-count 1..4] [--connection-policy baseline|no_close_checkpoint|no_close_checkpoint_full|no_close_checkpoint_full_no_auto_checkpoint]
//! Baseline uses no live backup, keeper connection, injected lock, or admission delay.
//! Opt-in maintenance scenario: --maintenance-interval-ms 500 --reader-pin-ms 5000.
//! This deliberately pins an isolated WAL reader; it is not baseline causal evidence.
//! --maintenance-reopen-owner 1 opts into a fresh owner before every checkpoint.
//! --maintenance-recovery-retries 0..3 permits only delayed BUSY_RECOVERY preparation retries.
//! --maintenance-keeper 1 retains an idle mapped disposable connection through both joins.
//! --maintenance-lock-probe 1 captures counted native SHM outcomes without changing proof gates.
//! --maintenance-plain-local-probe 1 maps only the validated short disposable fixture filename.
use rusqlite::{config::DbConfig, Connection, OpenFlags, TransactionBehavior};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use voxvulgi_engine::{db, paths::AppPaths};

type ProbeResult<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const KEY: &str = "wp0323_disposable_churn_counter";
const MAX_ITERATIONS: u64 = 100_000;
const CURRENT_PROBE_SCHEMA: u32 = 61;
const JOB_PREFIX: &str = "wp0323_churn_fixture_";
const READ_JOB: &str = "wp0323_churn_fixture_read";

#[derive(Debug)]
struct StageFailure(serde_json::Value);
impl std::fmt::Display for StageFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(formatter, "{}", self.0) }
}
impl std::error::Error for StageFailure {}

#[derive(Default)]
struct StageRecorder { receipts: Vec<serde_json::Value> }
impl StageRecorder {
    fn run<T>(&mut self, name: &str, operation: impl FnOnce() -> ProbeResult<T>) -> ProbeResult<T> {
        let started = Instant::now();
        let result = operation();
        let elapsed_ms = started.elapsed().as_millis();
        match result {
            Ok(value) => {
                if self.receipts.len() < 64 { self.receipts.push(json!({"stage":name,"elapsed_ms":elapsed_ms,"completed":true})); }
                Ok(value)
            }
            Err(error) => {
                let sqlite = error.downcast_ref::<rusqlite::Error>().or_else(||
                    error.downcast_ref::<voxvulgi_engine::EngineError>().and_then(|error|
                        if let voxvulgi_engine::EngineError::Database(sqlite) = error { Some(sqlite) } else { None }));
                let codes = match sqlite {
                    Some(rusqlite::Error::SqliteFailure(code, _)) => json!({
                        "primary":code.extended_code & 255,"extended":code.extended_code,"classification":format!("{:?}",code.code)}),
                    _ => serde_json::Value::Null,
                };
                Err(Box::new(StageFailure(json!({"failed_stage":name,"failed_stage_elapsed_ms":elapsed_ms,
                    "completed_stages":self.receipts,"sqlite_error_codes":codes,
                    "error_class":"stage_failed_no_SQL_or_secret_text"}))))
            }
        }
    }
}
macro_rules! stage {
    ($recorder:expr, $name:expr, $operation:expr) => {
        $recorder.run($name, || Ok($operation?))?
    };
}

fn staged_maintenance_policy(connection: &Connection, stages: &mut StageRecorder, prefix: &str) -> ProbeResult<serde_json::Value> {
    let disabled = stage!(stages, &format!("{prefix}no_checkpoint_on_close_set"), connection.set_db_config(DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true));
    stage!(stages, &format!("{prefix}synchronous_set"), connection.pragma_update(None, "synchronous", "FULL"));
    stage!(stages, &format!("{prefix}wal_autocheckpoint_set"), connection.pragma_update(None, "wal_autocheckpoint", 0));
    let effective = stage!(stages, &format!("{prefix}no_checkpoint_on_close_get"), connection.db_config(DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE));
    let synchronous: i64 = stage!(stages, &format!("{prefix}synchronous_get"), connection.pragma_query_value(None, "synchronous", |row|row.get(0)));
    let autocheckpoint: i64 = stage!(stages, &format!("{prefix}wal_autocheckpoint_get"), connection.pragma_query_value(None, "wal_autocheckpoint", |row|row.get(0)));
    stages.run(&format!("{prefix}policy_verify"), || {
        if !disabled || !effective || synchronous != 2 || autocheckpoint != 0 {
            return Err("Counterfactual policy readback mismatch".into());
        }
        Ok(json!({"requested_policy":"NoCloseCheckpointFullNoAutoCheckpoint",
            "no_checkpoint_on_close":effective,"synchronous":synchronous,"wal_autocheckpoint":autocheckpoint}))
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Workload { Meta, JobInsert }

#[derive(Clone, Copy, Debug, PartialEq)]
enum ConnectionPolicy { Baseline, NoCloseCheckpoint, NoCloseCheckpointFull, NoCloseCheckpointFullNoAutoCheckpoint }

fn hash_file(path: &Path) -> ProbeResult<(String, u64)> {
    let mut file = std::fs::File::open(path)?;
    let initial_size = file.metadata()?.len();
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut bytes = 0_u64;
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 { break; }
        hasher.update(&buffer[..count]);
        bytes = bytes.checked_add(count as u64).ok_or("Backup size overflow")?;
    }
    if bytes != initial_size || file.metadata()?.len() != initial_size {
        return Err("Backup size changed while hashing".into());
    }
    Ok((hex::encode(hasher.finalize()), bytes))
}

fn standalone_schema(path: &Path) -> ProbeResult<u32> {
    // Immutable inspection cannot create WAL/SHM beside the completed backup.
    let mut source_uri = url::Url::from_file_path(path).map_err(|_| "Invalid backup file URI")?;
    source_uri.query_pairs_mut().append_pair("mode", "ro").append_pair("immutable", "1");
    let connection = Connection::open_with_flags(source_uri.as_str(),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI)?;
    Ok(db::schema_user_version(&connection)?)
}

fn reject_sidecars(path: &Path) -> ProbeResult<()> {
    for suffix in ["-wal", "-shm", "-journal"] {
        let sidecar = PathBuf::from(format!("{}{suffix}", path.display()));
        if sidecar.exists() { return Err("Refuse backup with SQLite sidecars; require completed standalone backup".into()); }
    }
    Ok(())
}

#[derive(Default, Serialize)]
struct WorkerReport {
    lane: String,
    attempts: u64,
    succeeded: u64,
    total_operation_ms: u128,
    max_operation_ms: u128,
    total_query_ms: u128,
    max_query_ms: u128,
    failures_by_kind: BTreeMap<String, u64>,
    first_failures: Vec<serde_json::Value>,
    policy_observations: Vec<serde_json::Value>,
    first_native_filename: Option<NativeFilenameObservation>,
    first_mmap_size_bytes: Option<i64>,
    lock_summary: WorkerLockSummary,
    vfs_timings: Vec<serde_json::Value>,
    slow_vfs_operations: Vec<serde_json::Value>,
    vfs_file_kind_proven: bool,
}

impl WorkerReport {
    fn observe_mmap_size(&mut self, connection: &Connection) -> rusqlite::Result<()> {
        if self.first_mmap_size_bytes.is_none() {
            self.first_mmap_size_bytes = Some(connection.pragma_query_value(None, "mmap_size", |row| row.get(0))?);
        }
        Ok(())
    }
}

#[derive(Serialize)]
struct NativeFilenameObservation { prefix_class: &'static str, byte_length: usize, utf8: bool, coverage_proven: bool, sha256: Option<String> }
fn native_filename_observation(connection: &Connection) -> NativeFilenameObservation {
    let filename = unsafe { rusqlite::ffi::sqlite3_db_filename(connection.handle(), c"main".as_ptr()) };
    if filename.is_null() { return NativeFilenameObservation { prefix_class: "null", byte_length: 0, utf8: false, coverage_proven: false, sha256: None }; }
    let bytes = unsafe { std::ffi::CStr::from_ptr(filename) }.to_bytes();
    let prefix_class = if bytes.get(..8).is_some_and(|prefix|prefix.eq_ignore_ascii_case(b"\\\\?\\UNC\\")) { "extended_UNC" }
        else if bytes.starts_with(b"\\\\?\\") && bytes.get(4).is_some_and(u8::is_ascii_alphabetic) && bytes.get(5) == Some(&b':') { "extended_local" }
        else if bytes.starts_with(b"\\\\?\\") { "unknown" }
        else if bytes.starts_with(b"\\\\") { "UNC" }
        else if bytes.get(0).is_some_and(u8::is_ascii_alphabetic) && bytes.get(1) == Some(&b':') { "plain_local" }
        else { "unknown" };
    let utf8 = std::str::from_utf8(bytes).is_ok();
    NativeFilenameObservation { prefix_class, byte_length: bytes.len(), utf8, sha256: Some(hex::encode(Sha256::digest(bytes))),
        coverage_proven: utf8 && prefix_class != "unknown" && (!db::AppDatabase::disposable_filename_probe_enabled() || prefix_class == "plain_local") }
}

#[derive(Default, Serialize)]
struct WorkerLockSummary {
    enabled: bool, attribution_proven: bool, native_shm_calls: u64,
    operations: u64, coverage_failures: u64, unknown: u64, overflow: u64, borrow_conflicts: u64,
    closed_main_files: u64, positive_historical_shared_mask_files: u64, unlock_error_files: u64,
    shared_lock_ok: [u64; 8], shared_unlock_ok: [u64; 8], unlock_error: [u64; 8],
    exclusive_busy: [u64; 8], other_error: [u64; 8],
    anomalous_operations: u64, anomaly_receipts_omitted: u64,
    first_anomalies: Vec<serde_json::Value>,
}
impl WorkerLockSummary {
    fn add(&mut self, request_id: &str, snapshot: serde_json::Value) {
        let value = |key: &str|snapshot[key].as_u64().unwrap_or(0);
        self.operations += 1;
        self.native_shm_calls = self.native_shm_calls.saturating_add(value("native_shm_calls"));
        let coverage_failed = snapshot["attribution_proven"].as_bool() != Some(true);
        self.coverage_failures += u64::from(coverage_failed);
        self.unknown = self.unknown.saturating_add(value("unknown"));
        self.overflow = self.overflow.saturating_add(value("overflow"));
        self.borrow_conflicts = self.borrow_conflicts.saturating_add(value("borrow_conflicts"));
        let mut anomalous = coverage_failed;
        if let Some(files) = snapshot["files"].as_array() {
            for file in files.iter().filter(|file|file["main_db"].as_bool() == Some(true) && file["closed"].as_bool() == Some(true)) {
                self.closed_main_files += 1;
                let positive = file["shared_mask"].as_u64().unwrap_or(0) != 0;
                self.positive_historical_shared_mask_files += u64::from(positive);
                let mut unlock_error = false;
                for index in 0..8 {
                    let slot = &file["slots"][index];
                    for (key, target) in [("shared_lock_ok", &mut self.shared_lock_ok), ("shared_unlock_ok", &mut self.shared_unlock_ok),
                        ("unlock_error", &mut self.unlock_error), ("exclusive_busy", &mut self.exclusive_busy), ("other_error", &mut self.other_error)] {
                        target[index] = target[index].saturating_add(slot[key].as_u64().unwrap_or(0));
                    }
                    unlock_error |= slot["unlock_error"].as_u64().unwrap_or(0) != 0;
                }
                self.unlock_error_files += u64::from(unlock_error);
                anomalous |= positive || unlock_error;
            }
        }
        if anomalous {
            self.anomalous_operations += 1;
            if self.first_anomalies.len() < 32 {
                self.first_anomalies.push(json!({"request_id":request_id,"capture":snapshot,
                    "limits":"Positive historical inferred shared masks are native callback observations, not OS lock leaks; unmap may release OS locks without xShmLock unlock callbacks. Serialization follows the measured operation; instrumentation still has observer CPU/scheduling effects."}));
            } else { self.anomaly_receipts_omitted += 1; }
        }
    }
}

fn normalized(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/").to_lowercase()
        .trim_start_matches("//?/").trim_end_matches('/').to_string()
}

fn reject_protected(path: &Path) -> ProbeResult<()> {
    let candidate = normalized(path);
    for variable in ["APPDATA", "LOCALAPPDATA"] {
        let base = std::env::var_os(variable).ok_or("APPDATA/LOCALAPPDATA required for safety guard")?;
        let protected = PathBuf::from(base).join("com.voxvulgi.voxvulgi");
        let resolved = if protected.exists() { protected.canonicalize()? } else { protected };
        let protected = normalized(&resolved);
        if candidate == protected || candidate.starts_with(&(protected.clone() + "/"))
            || protected.starts_with(&(candidate.clone() + "/")) {
            return Err("Refuse production app-data path or its ancestor".into());
        }
    }
    Ok(())
}

fn parse() -> ProbeResult<(PathBuf, u64, Option<PathBuf>, Workload, u64, u64, usize, usize, ConnectionPolicy, u64, u64, bool, u32, bool, bool, bool)> {
    let mut root = None;
    let mut seconds = 20;
    let mut copy = None;
    let mut workload = Workload::Meta;
    let mut policy = ConnectionPolicy::Baseline;
    let mut reader_interval_ms: u64 = 0;
    let mut writer_interval_ms: u64 = 0;
    let mut reader_count: usize = 4;
    let mut writer_count: usize = 1;
    let mut maintenance_interval_ms: u64 = 0;
    let mut reader_pin_ms: u64 = 0;
    let mut reopen_owner = false;
    let mut recovery_retries: u32 = 0;
    let mut recovery_retries_requested = false;
    let mut keeper_enabled = false;
    let mut lock_probe = false;
    let mut plain_local_probe = false;
    let mut args = std::env::args_os().skip(1);
    while let Some(argument) = args.next() {
        let value = args.next().ok_or("Every argument requires a value")?;
        match argument.to_str() {
            Some("--root") => root = Some(PathBuf::from(value)),
            Some("--seconds") => seconds = value.to_str().ok_or("Invalid seconds")?.parse()?,
            Some("--reader-count") => reader_count = value.to_str().ok_or("Invalid reader count")?.parse()?,
            Some("--writer-count") => writer_count = value.to_str().ok_or("Invalid writer count")?.parse()?,
            Some("--maintenance-interval-ms") => maintenance_interval_ms = value.to_str().ok_or("Invalid maintenance interval")?.parse()?,
            Some("--reader-pin-ms") => reader_pin_ms = value.to_str().ok_or("Invalid reader pin")?.parse()?,
            Some("--maintenance-reopen-owner") => reopen_owner = match value.to_str() {
                Some("0") => false, Some("1") => true,
                _ => return Err("maintenance-reopen-owner must be0 or1".into()),
            },
            Some("--maintenance-recovery-retries") => {
                recovery_retries_requested = true;
                recovery_retries = value.to_str().ok_or("Invalid recovery retries")?.parse()?;
            },
            Some("--maintenance-lock-probe") => lock_probe = match value.to_str() {
                Some("0") => false, Some("1") => true,
                _ => return Err("maintenance-lock-probe must be0 or1".into()),
            },
            Some("--maintenance-plain-local-probe") => plain_local_probe = match value.to_str() {
                Some("0") => false, Some("1") => true,
                _ => return Err("maintenance-plain-local-probe must be0 or1".into()),
            },
            Some("--maintenance-keeper") => keeper_enabled = match value.to_str() {
                Some("0") => false, Some("1") => true,
                _ => return Err("maintenance-keeper must be0 or1".into()),
            },
            Some("--reader-interval-ms") => reader_interval_ms = value.to_str().ok_or("Invalid reader interval")?.parse()?,
            Some("--writer-interval-ms") => writer_interval_ms = value.to_str().ok_or("Invalid writer interval")?.parse()?,
            Some("--database-copy") => copy = Some(PathBuf::from(value)),
            Some("--connection-policy") => policy = match value.to_str() { Some("baseline") => ConnectionPolicy::Baseline, Some("no_close_checkpoint") => ConnectionPolicy::NoCloseCheckpoint, Some("no_close_checkpoint_full") => ConnectionPolicy::NoCloseCheckpointFull, Some("no_close_checkpoint_full_no_auto_checkpoint") => ConnectionPolicy::NoCloseCheckpointFullNoAutoCheckpoint, _ => return Err("Unsupported connection-policy".into()) },
            Some("--workload") => workload = match value.to_str() { Some("meta") => Workload::Meta, Some("job_insert") => Workload::JobInsert, _ => return Err("workload must be meta or job_insert".into()) },
            _ => return Err("Unknown argument; see usage at source header".into()),
        }
    }
    if !(1..=30).contains(&seconds) { return Err("seconds must be1..30".into()); }
    if reader_interval_ms > 1000 || writer_interval_ms > 1000 { return Err("Intervals must be0..1000ms".into()); }
    if !(1..=16).contains(&reader_count) || !(1..=4).contains(&writer_count) { return Err("reader-count must be1..16 and writer-count1..4".into()); }
    if policy != ConnectionPolicy::Baseline && workload != Workload::JobInsert { return Err("Nonbaseline connection policy requires job_insert workload so synchronous is set before any transaction".into()); }
    if maintenance_interval_ms != 0 && (!(100..=1000).contains(&maintenance_interval_ms)
        || policy != ConnectionPolicy::NoCloseCheckpointFullNoAutoCheckpoint || seconds < 4
        || reader_pin_ms == 0 || reader_pin_ms >= seconds * 500) {
        return Err("Maintenance requires FULL/no-close-checkpoint/auto0, interval100..1000ms, seconds>=4 and a positive pin shorter than half the workload".into());
    }
    if maintenance_interval_ms == 0 && reader_pin_ms != 0 { return Err("Reader pin requires maintenance".into()); }
    if maintenance_interval_ms == 0 && reopen_owner { return Err("Owner reopen requires maintenance".into()); }
    if recovery_retries > 3 || (recovery_retries_requested && (maintenance_interval_ms == 0 || !reopen_owner)) {
        return Err("Recovery retries must be0..3 and require maintenance plus owner reopen".into());
    }
    if keeper_enabled && (maintenance_interval_ms == 0 || !reopen_owner) {
        return Err("Keeper requires maintenance plus owner reopen".into());
    }
    if lock_probe && !keeper_enabled { return Err("Lock probe requires keeper plus maintenance and owner reopen".into()); }
    if plain_local_probe && !lock_probe { return Err("Plain local probe requires lock probe, keeper, maintenance and owner reopen".into()); }
    let root = root.ok_or("--root is required")?;
    if !root.is_absolute() || root.exists() { return Err("root must be an absent absolute directory".into()); }
    let parent = root.parent().ok_or("Root parent missing")?.canonicalize()?;
    let root = parent.join(root.file_name().ok_or("Root filename missing")?);
    reject_protected(&root)?;
    if let Some(source) = copy.as_ref() {
        let source = source.canonicalize()?;
        reject_protected(&source)?;
        if !source.is_file() { return Err("database-copy must be a regular backup file".into()); }
        reject_sidecars(&source)?;
        if ![59, 60, CURRENT_PROBE_SCHEMA].contains(&standalone_schema(&source)?) {
            return Err("database-copy must have schema59 through61; only the disposable destination may migrate".into());
        }
        return Ok((root, seconds, Some(source), workload, reader_interval_ms, writer_interval_ms, reader_count, writer_count, policy, maintenance_interval_ms, reader_pin_ms, reopen_owner, recovery_retries, keeper_enabled, lock_probe, plain_local_probe));
    }
    Ok((root, seconds, None, workload, reader_interval_ms, writer_interval_ms, reader_count, writer_count, policy, maintenance_interval_ms, reader_pin_ms, reopen_owner, recovery_retries, keeper_enabled, lock_probe, plain_local_probe))
}

struct FilenameProbeGuard;
impl Drop for FilenameProbeGuard { fn drop(&mut self) { db::AppDatabase::end_disposable_filename_probe(); } }
fn plain_native_filename_coverage(value: &serde_json::Value) -> (u64, bool) {
    match value {
        serde_json::Value::Object(object) if object.contains_key("prefix_class") && object.contains_key("byte_length") =>
            (1, value["prefix_class"].as_str() == Some("plain_local") && value["coverage_proven"].as_bool() == Some(true)),
        serde_json::Value::Object(object) => object.values().map(plain_native_filename_coverage).fold((0,true), |(n,ok),(m,valid)|(n+m,ok&&valid)),
        serde_json::Value::Array(array) => array.iter().map(plain_native_filename_coverage).fold((0,true), |(n,ok),(m,valid)|(n+m,ok&&valid)),
        _ => (0,true),
    }
}

fn native_filename_identity_coverage(value: &serde_json::Value, expected_sha256: &str) -> (u64, bool) {
    match value {
        serde_json::Value::Object(object) if object.contains_key("prefix_class") && object.contains_key("byte_length") =>
            (1, value["sha256"].as_str() == Some(expected_sha256) && value["coverage_proven"].as_bool() == Some(true)),
        serde_json::Value::Object(object) => object.values().map(|value| native_filename_identity_coverage(value, expected_sha256))
            .fold((0,true), |(n,ok),(m,valid)|(n+m,ok&&valid)),
        serde_json::Value::Array(array) => array.iter().map(|value| native_filename_identity_coverage(value, expected_sha256))
            .fold((0,true), |(n,ok),(m,valid)|(n+m,ok&&valid)),
        _ => (0,true),
    }
}

struct LockCapture(bool);
impl LockCapture {
    fn begin(enabled: bool) -> Self { if enabled { db::AppDatabase::begin_shm_lock_probe(); } Self(enabled) }
    fn snapshot(&self) -> serde_json::Value { if self.0 { db::AppDatabase::snapshot_shm_lock_probe() } else { serde_json::Value::Null } }
}
impl Drop for LockCapture { fn drop(&mut self) { if self.0 { db::AppDatabase::end_shm_lock_probe(); } } }

fn idle_owner_verified(state: &serde_json::Value) -> bool {
    state["is_autocommit"].as_bool() == Some(true) && state["main_txn_state"].as_i64() == Some(0)
        && state["busy_statement_count"].as_u64() == Some(0)
        && state["statement_count_overflow"].as_bool() == Some(false)
}

fn prepare_keeper(path: &Path, initial_identity: &serde_json::Value, lock_probe: bool) -> ProbeResult<(Connection, serde_json::Value)> {
    // Fixed staged helper opens only the already guarded disposable database, without CREATE.
    let (connection, owner_identity) = reopen_maintenance_owner_with_vfs(path, lock_probe)?;
    let mut stages = StageRecorder { receipts: owner_identity["stage_receipts"].as_array().cloned().unwrap_or_default() };
    stages.run("keeper_fixed_identity_verify", || {
        if owner_identity["sqlite_version"] != initial_identity["sqlite_version"]
            || owner_identity["sqlite_source_id"] != initial_identity["sqlite_source_id"] {
            return Err("Keeper identity differs".into());
        }
        Ok(())
    })?;
    stage!(stages, "keeper_query_only_set", connection.pragma_update(None, "query_only", "ON"));
    let query_only: i64 = stage!(stages, "keeper_query_only_get", connection.pragma_query_value(None, "query_only", |row|row.get(0)));
    let busy_timeout: i64 = stage!(stages, "keeper_busy_timeout_get", connection.pragma_query_value(None, "busy_timeout", |row|row.get(0)));
    stages.run("keeper_settings_verify", || if query_only == 1 && busy_timeout == 0 { Ok(()) }
        else { Err("Keeper settings differ".into()) })?;
    stage!(stages, "keeper_begin", connection.execute_batch("BEGIN"));
    let _: String = stage!(stages, "keeper_meta_snapshot_read", connection.query_row(
        "SELECT value FROM meta WHERE key=?1", [KEY], |row| row.get(0)));
    let mapped_state = native_owner_state(&connection);
    let locks_after_read = if lock_probe { db::AppDatabase::snapshot_shm_lock_probe() } else { serde_json::Value::Null };
    stages.run("keeper_read_snapshot_verify", || {
        if mapped_state["main_txn_state"].as_i64() != Some(1)
            || mapped_state["is_autocommit"].as_bool() != Some(false) {
            return Err("Keeper mapping read transaction not observed".into());
        }
        Ok(())
    })?;
    stage!(stages, "keeper_rollback", connection.execute_batch("ROLLBACK"));
    let idle_state = native_owner_state(&connection);
    let locks_after_rollback = if lock_probe { db::AppDatabase::snapshot_shm_lock_probe() } else { serde_json::Value::Null };
    stages.run("keeper_idle_verify", || if idle_owner_verified(&idle_state) { Ok(()) }
        else { Err("Keeper retained active SQLite state".into()) })?;
    Ok((connection, json!({"owner_identity":owner_identity,"mapped_read_state":mapped_state,
        "query_only":query_only,"busy_timeout_ms":busy_timeout,
        "idle_state":idle_state,"stage_receipts":stages.receipts,"prepared_before_threads":true,
        "locks_after_begin_and_meta_read":locks_after_read,"locks_after_rollback":locks_after_rollback,
        "limits":"Idle connection mapping is a harness counterfactual, not a proven live cause or production solution."})))
}

fn reopen_maintenance_owner(path: &Path) -> ProbeResult<(Connection, serde_json::Value)> {
    reopen_maintenance_owner_with_vfs(path, false)
}
fn reopen_maintenance_owner_with_vfs(path: &Path, counted: bool) -> ProbeResult<(Connection, serde_json::Value)> {
    reject_protected(path)?;
    let mut stages = StageRecorder::default();
    if counted && unsafe { rusqlite::ffi::sqlite3_vfs_find(c"voxvulgi_read_counting".as_ptr()) }.is_null() {
        return Err("Exact counted VFS is not registered; no fallback".into());
    }
    let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_FULL_MUTEX;
    let connection = stage!(stages, "native_connection_open", if counted {
        Connection::open_with_flags_and_vfs(db::AppDatabase::sqlite_open_filename(&db::AppDatabase::disposable_open_filename(path)?), flags, "voxvulgi_read_counting")
    } else { Connection::open_with_flags(db::AppDatabase::sqlite_open_filename(&db::AppDatabase::disposable_open_filename(path)?), flags) });
    stage!(stages, "busy_timeout", connection.busy_timeout(Duration::ZERO));
    let policy = staged_maintenance_policy(&connection, &mut stages, "")?;
    let version: String = stage!(stages, "sqlite_version", connection.query_row("SELECT sqlite_version()", [], |row| row.get(0)));
    let source_id: String = stage!(stages, "sqlite_source_id", connection.query_row("SELECT sqlite_source_id()", [], |row| row.get(0)));
    stages.run("fixed_version_verify", || {
        let parts = version.split('.').map(str::parse::<u32>).collect::<std::result::Result<Vec<_>, _>>()?;
        if parts.len() != 3 || parts.as_slice() < [3,51,3].as_slice() { return Err("Fixed SQLite required".into()); }
        Ok(())
    })?;
    let mode: String = stage!(stages, "journal_mode_get", connection.pragma_query_value(None, "journal_mode", |row| row.get(0)));
    stages.run("journal_mode_verify", || if mode.eq_ignore_ascii_case("wal") { Ok(()) } else { Err("WAL required".into()) })?;
    let state = stages.run("native_owner_state", || Ok(native_owner_state(&connection)))?;
    let identity = json!({"sqlite_version":version,"sqlite_source_id":source_id,"policy":policy,
        "journal_mode":mode,"state":state,"busy_timeout_ms":0,"create":false,"stage_receipts":stages.receipts});
    Ok((connection, identity))
}

// Harness-only independent maintenance owner. Never called by production runtime code.
// Connections are prepared before spawning/barrier so setup failure cannot strand worker joins.
fn prepare_maintenance(path: &Path) -> ProbeResult<(Connection, Connection, serde_json::Value)> {
    reject_protected(path)?;
    let mut stages = StageRecorder::default();
    let connection = stage!(stages, "native_connection_open", Connection::open_with_flags(db::AppDatabase::sqlite_open_filename(&db::AppDatabase::disposable_open_filename(path)?),
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_FULL_MUTEX));
    stage!(stages, "busy_timeout", connection.busy_timeout(Duration::ZERO));
    let policy = staged_maintenance_policy(&connection, &mut stages, "")?;
    let version: String = stage!(stages, "sqlite_version", connection.query_row("SELECT sqlite_version()", [], |row| row.get(0)));
    let source_id: String = stage!(stages, "sqlite_source_id", connection.query_row("SELECT sqlite_source_id()", [], |row| row.get(0)));
    stages.run("fixed_version_verify", || {
        let parts = version.split('.').map(str::parse::<u32>).collect::<std::result::Result<Vec<_>, _>>()?;
        if parts.len() != 3 || parts.as_slice() < [3,51,3].as_slice() { return Err("Fixed SQLite required".into()); }
        Ok(())
    })?;
    let journal_mode: String = stage!(stages, "journal_mode_get", connection.pragma_query_value(None, "journal_mode", |row| row.get(0)));
    stages.run("journal_mode_verify", || if journal_mode.eq_ignore_ascii_case("wal") { Ok(()) } else { Err("WAL required".into()) })?;
    // Create a real committed WAL frame before establishing the read snapshot.
    let seed_before: String = stage!(stages, "seed_value_read", connection.query_row("SELECT value FROM meta WHERE key=?1", [KEY], |row| row.get(0)));
    let seed_after = format!("{seed_before}|wp0333_maintenance_seed");
    let seed_rows = stage!(stages, "seed_value_write", connection.execute("UPDATE meta SET value=?2 WHERE key=?1", rusqlite::params![KEY, seed_after]));
    stages.run("seed_rows_verify", || if seed_rows == 1 { Ok(()) } else { Err("Seed missing".into()) })?;
    let seed_observed: String = stage!(stages, "seed_value_readback", connection.query_row("SELECT value FROM meta WHERE key=?1", [KEY], |row| row.get(0)));
    stages.run("seed_value_verify", || if seed_observed == seed_after && seed_observed != seed_before { Ok(()) } else { Err("Seed unchanged".into()) })?;
    let seed_wal_bytes = stages.run("seed_wal_size", || Ok(std::fs::metadata(PathBuf::from(format!("{}-wal", path.display())))?.len()))?;
    stages.run("seed_wal_verify", || if seed_wal_bytes > 32 { Ok(()) } else { Err("Committed frame missing".into()) })?;
    reject_protected(path)?;
    let pin = stage!(stages, "pin_native_connection_open", Connection::open_with_flags(db::AppDatabase::sqlite_open_filename(&db::AppDatabase::disposable_open_filename(path)?),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_FULL_MUTEX));
    stage!(stages, "pin_busy_timeout", pin.busy_timeout(Duration::ZERO));
    stage!(stages, "pin_query_only_set", pin.pragma_update(None, "query_only", "ON"));
    let pin_policy = staged_maintenance_policy(&pin, &mut stages, "pin.")?;
    let pin_version: String = stage!(stages, "pin_sqlite_version", pin.query_row("SELECT sqlite_version()", [], |row| row.get(0)));
    stages.run("pin_fixed_version_verify", || if pin_version == version { Ok(()) } else { Err("Pin identity differs".into()) })?;
    stage!(stages, "pin_begin", pin.execute_batch("BEGIN"));
    let _: String = stage!(stages, "pin_snapshot_read", pin.query_row("SELECT value FROM meta WHERE key=?1", [KEY], |row| row.get(0)));
    let owner_state = stages.run("native_owner_state", || Ok(native_owner_state(&connection)))?;
    let pin_state = stages.run("pin_native_owner_state", || Ok(native_owner_state(&pin)))?;
    let identity = json!({"sqlite_version":version,"sqlite_source_id":source_id,
        "journal_mode":journal_mode,"maintenance_policy":policy,"pin_policy":pin_policy,"pin_sqlite_version":pin_version,
        "owner_initial_state":owner_state,"pin_snapshot_state":pin_state,
        "stage_receipts":stages.receipts,
        "database_path":path,"seed_changed_and_verified":true,"seed_wal_bytes":seed_wal_bytes,
        "ownership":"guarded_disposable_harness_only_not_runtime_admission"});
    Ok((connection, pin, identity))
}

fn native_owner_state(connection: &Connection) -> serde_json::Value {
    // Inspect only this thread's owned connection. Never disclose SQL text.
    unsafe {
        let handle = connection.handle();
        let mut statement = rusqlite::ffi::sqlite3_next_stmt(handle, std::ptr::null_mut());
        let mut statements = 0_u32;
        let mut busy = 0_u32;
        while !statement.is_null() && statements < 64 {
            statements += 1;
            busy += u32::from(rusqlite::ffi::sqlite3_stmt_busy(statement) != 0);
            statement = rusqlite::ffi::sqlite3_next_stmt(handle, statement);
        }
        let mut name: *mut std::ffi::c_char = std::ptr::null_mut();
        let vfs_rc = rusqlite::ffi::sqlite3_file_control(handle, c"main".as_ptr(),
            rusqlite::ffi::SQLITE_FCNTL_VFSNAME, (&mut name as *mut *mut std::ffi::c_char).cast());
        let vfs = if name.is_null() { None } else {
            let value = std::ffi::CStr::from_ptr(name).to_string_lossy().into_owned();
            rusqlite::ffi::sqlite3_free(name.cast());
            Some(value)
        };
        json!({"is_autocommit":connection.is_autocommit(),
            "native_filename":native_filename_observation(connection),
            "main_txn_state":rusqlite::ffi::sqlite3_txn_state(handle, c"main".as_ptr()),
            "statement_count":statements,"busy_statement_count":busy,
            "statement_count_overflow":!statement.is_null(),"actual_vfs":vfs,"vfs_name_rc":vfs_rc})
    }
}

fn fresh_checkpoint_diagnostic(path: &Path, requested_vfs: Option<&str>, lock_probe: bool) -> serde_json::Value {
    let started = Instant::now();
    let capture = LockCapture::begin(lock_probe && requested_vfs == Some("voxvulgi_read_counting"));
    let result = (|| -> ProbeResult<serde_json::Value> {
        reject_protected(path)?;
        if let Some(name) = requested_vfs {
            let name = std::ffi::CString::new(name)?;
            if unsafe { rusqlite::ffi::sqlite3_vfs_find(name.as_ptr()) }.is_null() {
                return Err("Requested exact VFS is not registered; no fallback".into());
            }
        }
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_FULL_MUTEX;
        let connection = match requested_vfs {
            Some(name) => Connection::open_with_flags_and_vfs(db::AppDatabase::sqlite_open_filename(&db::AppDatabase::disposable_open_filename(path)?), flags, name)?,
            None => Connection::open_with_flags(db::AppDatabase::sqlite_open_filename(&db::AppDatabase::disposable_open_filename(path)?), flags)?,
        };
        connection.busy_timeout(Duration::ZERO)?;
        let policy = apply_policy(&connection, ConnectionPolicy::NoCloseCheckpointFullNoAutoCheckpoint)?;
        let version: String = connection.query_row("SELECT sqlite_version()", [], |row| row.get(0))?;
        let source_id: String = connection.query_row("SELECT sqlite_source_id()", [], |row| row.get(0))?;
        let parts = version.split('.').map(str::parse::<u32>).collect::<std::result::Result<Vec<_>, _>>()?;
        if parts.len() != 3 || parts.as_slice() < [3,51,3].as_slice() {
            return Err("Diagnostic requires actually linked fixed SQLite>=3.51.3".into());
        }
        let state = native_owner_state(&connection);
        let locks_before_checkpoint = capture.snapshot();
        let checkpoint_started = Instant::now();
        let checkpoint = connection.query_row("PRAGMA wal_checkpoint(PASSIVE)", [], |row|
            Ok((row.get::<_, i64>(0)?,row.get::<_, i64>(1)?,row.get::<_, i64>(2)?)));
        let checkpoint_ms = checkpoint_started.elapsed().as_millis();
        let locks_after_checkpoint = capture.snapshot();
        let checkpoint = match checkpoint {
            Ok((busy,log,checkpointed)) => json!({"busy":busy,"log_frames":log,"checkpointed_frames":checkpointed}),
            Err(error) => json!({"error":error.to_string()}),
        };
        let close_started = Instant::now();
        let close = match connection.close() {
            Ok(()) => None,
            Err((owned,error)) => { drop(owned); Some(error.to_string()) }
        };
        Ok(json!({"requested_vfs":requested_vfs,"policy":policy,"sqlite_version":version,
            "sqlite_source_id":source_id,"before_checkpoint":state,"checkpoint":checkpoint,
            "locks_before_checkpoint":locks_before_checkpoint,"locks_after_checkpoint":locks_after_checkpoint,
            "checkpoint_ms":checkpoint_ms,"close_ms":close_started.elapsed().as_millis(),"close_error":close}))
    })();
    let final_locks = capture.snapshot();
    match result {
        Ok(mut value) => { value["locks_after_checked_close"] = final_locks; value["elapsed_ms"] = json!(started.elapsed().as_millis()); value },
        Err(error) => json!({"requested_vfs":requested_vfs,"error":error.to_string(),"locks_on_error":final_locks,"elapsed_ms":started.elapsed().as_millis()}),
    }
}

fn maintenance_worker(connection: Connection, pin: Connection, identity: serde_json::Value,
    barrier: Arc<Barrier>, stop: Arc<AtomicBool>, path: PathBuf, page_size: u64,
    interval_ms: u64, pin_ms: u64, reopen_owner: bool, recovery_retries: u32, lock_probe: bool) -> serde_json::Value {
    let wal_path = PathBuf::from(format!("{}-wal", path.display()));
    let mut pin = Some(pin); // RAII rollback/close also releases on unwinding.
    let mut connection = Some(connection);
    let mut receipts = Vec::new();
    let mut overflow = 0_u64;
    let mut errors = 0_u64;
    let mut partial_during_pin = false;
    let mut recovered_after_pin = false;
    let mut final_checkpoint_complete = false;
    let mut max_wal_bytes = 0_u64;
    let mut release_error = None;
    let mut released_at_ms = None;
    let mut before_final_checkpoint = None;
    let mut stop_observed = false;
    let mut error_exit = false;
    let mut recovery_retries_used = 0_u32;
    barrier.wait();
    let started = Instant::now();
    loop {
        let final_attempt = stop.load(Ordering::Acquire);
        stop_observed |= final_attempt;
        if pin.is_some() && (started.elapsed() >= Duration::from_millis(pin_ms) || final_attempt) {
            let owned = pin.take().expect("pin present");
            release_error = owned.execute_batch("ROLLBACK").err().map(|error|error.to_string());
            drop(owned); // Even a failed rollback cannot retain the owned snapshot.
            released_at_ms = Some(started.elapsed().as_millis());
        }
        let mut reopen_receipt = None;
        if reopen_owner {
            let owned = connection.take();
            let had_previous_owner = owned.is_some();
            let prior_state = owned.as_ref().map(native_owner_state);
            let close_started = Instant::now();
            let close_error = match owned.map(Connection::close) {
                Some(Ok(())) | None => None,
                Some(Err((owned,error))) => { drop(owned); Some(error.to_string()) },
            };
            let close_ms = close_started.elapsed().as_millis();
            let open_started = Instant::now();
            let reopened = if close_error.is_none() { reopen_maintenance_owner(&path) }
                else { Err("Previous owned maintenance close failed".into()) };
            match reopened {
                Ok((fresh,reopened_identity)) => {
                    if reopened_identity["sqlite_version"] != identity["sqlite_version"]
                        || reopened_identity["sqlite_source_id"] != identity["sqlite_source_id"] {
                        errors += 1;
                        error_exit = true;
                        drop(fresh);
                        if receipts.len() < 128 { receipts.push(json!({"final_after_stop":final_attempt,
                            "owner_reopen_error":"Fixed native identity differs from initial owner",
                            "had_previous_owner":had_previous_owner,
                            "identity":reopened_identity,"close_ms":close_ms,
                            "open_ms":open_started.elapsed().as_millis()})); } else { overflow += 1; }
                        break;
                    }
                    connection = Some(fresh);
                    reopen_receipt = Some(json!({"prior_state":prior_state,"close_ms":close_ms,
                        "close_error":close_error,"had_previous_owner":had_previous_owner,
                        "recovery_retries_used":recovery_retries_used,
                        "open_ms":open_started.elapsed().as_millis(),"identity":reopened_identity}));
                }
                Err(error) => {
                    let stage_failure = error.downcast_ref::<StageFailure>().map(|failure| &failure.0);
                    let busy_recovery = stage_failure.is_some_and(|failure|
                        failure["sqlite_error_codes"]["extended"].as_i64() == Some(261)
                        && failure["sqlite_error_codes"]["primary"].as_i64() == Some(5));
                    let retry = close_error.is_none() && busy_recovery && recovery_retries_used < recovery_retries;
                    if retry { recovery_retries_used += 1; }
                    else { errors += 1; error_exit = true; }
                    if receipts.len() < 128 { receipts.push(json!({"final_after_stop":final_attempt,
                        "at_ms":started.elapsed().as_millis(),"owner_reopen_error":error.to_string(),
                        "owner_reopen_stage_failure":stage_failure,"checkpoint_skipped":true,
                        "retry_scheduled":retry,"recovery_retries_used":recovery_retries_used,
                        "recovery_retry_budget":recovery_retries,"had_previous_owner":had_previous_owner,
                        "incomplete_owner_returned":false,"retry_interval_ms":if retry { Some(interval_ms) } else { None },
                        "prior_state":prior_state,"close_ms":close_ms,"close_error":close_error,
                        "open_ms":open_started.elapsed().as_millis()})); } else { overflow += 1; }
                    if retry {
                        // Full explicit cadence even after stop; no hot-loop final retry.
                        std::thread::sleep(Duration::from_millis(interval_ms));
                        continue;
                    }
                    break;
                }
            }
        }
        let owner = connection.as_ref().expect("owned checkpoint connection");
        if final_attempt { before_final_checkpoint = Some(native_owner_state(owner)); }
        let checkpoint_started = Instant::now();
        let result = owner.query_row("PRAGMA wal_checkpoint(PASSIVE)", [], |row|
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?)));
        let checkpoint_ms = checkpoint_started.elapsed().as_millis();
        let wal_bytes = std::fs::metadata(&wal_path).map(|m|m.len()).unwrap_or(0);
        max_wal_bytes = max_wal_bytes.max(wal_bytes);
        let mut receipt = match result {
            Ok((busy, log, checkpointed)) => {
                if pin.is_some() && log > checkpointed { partial_during_pin = true; }
                if pin.is_none() && busy == 0 && log > 0 && log == checkpointed { recovered_after_pin = true; }
                if final_attempt {
                    final_checkpoint_complete = pin.is_none() && busy == 0 && log == checkpointed
                        && (log >= 0 || log == -1);
                }
                json!({"at_ms":started.elapsed().as_millis(),"checkpoint_ms":checkpoint_ms,
                    "busy":busy,"log_frames":log,"checkpointed_frames":checkpointed,
                    "pin_active":pin.is_some(),"final_after_stop":final_attempt,
                    "wal_bytes":wal_bytes,"physical_wal_frames":wal_bytes.saturating_sub(32)/(page_size+24)})
            }
            Err(error) => {
                errors += 1;
                json!({"at_ms":started.elapsed().as_millis(),"checkpoint_ms":checkpoint_ms,
                    "error":error.to_string(),"pin_active":pin.is_some(),"final_after_stop":final_attempt,"wal_bytes":wal_bytes})
            }
        };
        receipt["owner_reopen"] = json!(reopen_receipt);
        if receipts.len() < 128 { receipts.push(receipt); } else { overflow += 1; }
        if final_attempt { break; }
        // Poll only between native calls; PASSIVE/xSync is not claimed timeout bounded.
        let until = Instant::now() + Duration::from_millis(interval_ms);
        while !stop.load(Ordering::Acquire) && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(10).min(until.saturating_duration_since(Instant::now())));
        }
    }
    if let Some(owned) = pin.take() {
        release_error = owned.execute_batch("ROLLBACK").err().map(|error|error.to_string());
        drop(owned);
        released_at_ms = Some(started.elapsed().as_millis());
    }
    let close_started = Instant::now();
    let owner_close_error = match connection.map(Connection::close) {
        Some(Ok(())) => None,
        Some(Err((owned,error))) => { drop(owned); Some(error.to_string()) },
        None => Some("No owner after failed reopen".to_string()),
    };
    let owner_close_ms = close_started.elapsed().as_millis();
    let fresh_diagnostics = if !final_checkpoint_complete {
        vec![fresh_checkpoint_diagnostic(&path, None, lock_probe),
            fresh_checkpoint_diagnostic(&path, Some("voxvulgi_read_counting"), lock_probe)]
    } else { Vec::new() };
    json!({"identity":identity,"receipts":receipts,"receipt_overflow":overflow,"errors":errors,
        "partial_during_pin":partial_during_pin,"recovered_after_pin":recovered_after_pin,
        "final_checkpoint_complete":final_checkpoint_complete,
        "pin_released_at_ms":released_at_ms,"pin_release_error":release_error,
        "max_sampled_wal_bytes":max_wal_bytes,"connection_close_ms":owner_close_ms,
        "owner_close_error":owner_close_error,"before_final_checkpoint":before_final_checkpoint,
        "post_failure_fresh_diagnostics":fresh_diagnostics,
        "fresh_diagnostics_do_not_override_original_verdict":true,
        "fresh_diagnostic_order":"default PASSIVE then exact registered VFS PASSIVE; the first can change checkpoint state seen by the second",
        "reopen_owner":reopen_owner,
        "recovery_retry_budget":recovery_retries,"recovery_retries_used":recovery_retries_used,
        "owner_lifetime":"Opt-in closes previous owner immediately before each checkpoint; native close may change last-connection state. Default retains one owner.",
        "elapsed_ms":started.elapsed().as_millis(),"stop_observed":stop_observed,"error_exit":error_exit,
        "limits":"WAL samples are not an absolute growth bound. PASSIVE has no native fsync wall-clock deadline. Reader pin is an intentional isolated starvation scenario, not a claimed live cause."})
}



// Counterfactual stays on disposable AppDatabase contexts; no environment or production defaults.
fn apply_policy(connection: &Connection, policy: ConnectionPolicy) -> voxvulgi_engine::Result<serde_json::Value> {
    let disabled = connection.set_db_config(DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true)?;
    let full = matches!(policy, ConnectionPolicy::NoCloseCheckpointFull | ConnectionPolicy::NoCloseCheckpointFullNoAutoCheckpoint);
    let expected_autocheckpoint = if policy == ConnectionPolicy::NoCloseCheckpointFullNoAutoCheckpoint { 0 } else { 1000 };
    if full {
        connection.pragma_update(None, "synchronous", "FULL")?;
    }
    if expected_autocheckpoint == 0 {
        connection.pragma_update(None, "wal_autocheckpoint", 0)?;
    }
    let effective = connection.db_config(DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE)?;
    let synchronous: i64 = connection.pragma_query_value(None, "synchronous", |row|row.get(0))?;
    let autocheckpoint: i64 = connection.pragma_query_value(None, "wal_autocheckpoint", |row|row.get(0))?;
    if !disabled || !effective || autocheckpoint != expected_autocheckpoint
        || (full && synchronous != 2) {
        return Err(voxvulgi_engine::EngineError::InstallFailed("Counterfactual connection policy did not match requested settings".into()));
    }
    Ok(json!({"requested_policy":format!("{policy:?}"),"no_checkpoint_on_close":effective,"synchronous":synchronous,"wal_autocheckpoint":autocheckpoint}))
}

// Exact enqueue SQL shape; synthetic IDs stay confined to the disposable root and no runner starts.
fn insert_job(connection: &Connection, id: &str, params_json: &str) -> rusqlite::Result<usize> {
    connection.execute(
        "INSERT INTO job(id,item_id,batch_id,type,status,progress,error,params_json,created_at_ms,started_at_ms,finished_at_ms,logs_path,lane,track,target_key,attempt_no) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,1)",
        rusqlite::params![id, Option::<String>::None, Option::<String>::None, "download_direct_url", "queued", 0.0_f32, Option::<String>::None, params_json, std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis().min(i64::MAX as u128) as i64, Option::<i64>::None, Option::<i64>::None, format!("{id}.jsonl"), "recurring", "youtube_recurring", Option::<String>::None])
}

fn worker(database: db::AppDatabase, barrier: Arc<Barrier>, deadline: Instant, index: usize, writer_count: usize, workload: Workload, job_params: Arc<String>, interval_ms: u64, policy: ConnectionPolicy, lock_probe: bool) -> WorkerReport {
    let is_writer = index < writer_count;
    let lane = if is_writer { format!("churn_writer_{index}") } else { format!("churn_reader_{}", index-writer_count+1) };
    let mut report = WorkerReport { lane: lane.clone(), ..Default::default() };
    let mut vfs_totals = [(0_u64, 0_u64, 0_u64); 14];
    let mut vfs_names = [""; 14];
    barrier.wait();
    while Instant::now() < deadline && report.attempts < MAX_ITERATIONS {
        report.attempts += 1;
        let request_id = format!("{lane}-{}", report.attempts);
        let context = db::DatabaseOperationContext::new(&lane, if workload == Workload::Meta { "wp0323_short_meta_churn" } else { "wp0323_job_insert_churn" })
            .with_request_id(&request_id);
        db::AppDatabase::begin_vfs_timing_probe();
        let lock_capture = LockCapture::begin(lock_probe);
        let started = Instant::now();
        let mut query_ms = 0;
        let result = if workload == Workload::JobInsert && is_writer {
            // Same autocommit INSERT column/value shape as jobs.rs enqueue; no extra transaction.
            (|| {
                let connection = database.write_context(context)?;
                report.observe_mmap_size(&connection)?;
                if lock_probe && report.first_native_filename.is_none() { report.first_native_filename = Some(native_filename_observation(&connection)); }
                if policy != ConnectionPolicy::Baseline {
                    let effective = apply_policy(&connection, policy)?;
                    if report.policy_observations.is_empty() { report.policy_observations.push(effective); }
                }
                let query_started = Instant::now();
                let id = format!("{JOB_PREFIX}{index}_{}", report.attempts);
                let result = insert_job(&connection, &id, &job_params);
                query_ms = query_started.elapsed().as_millis();
                result?;
                drop(connection);
                Ok::<(), voxvulgi_engine::EngineError>(())
            })()
        } else if workload == Workload::JobInsert {
            database.read(context, |connection| {
                report.observe_mmap_size(connection)?;
                if lock_probe && report.first_native_filename.is_none() { report.first_native_filename = Some(native_filename_observation(connection)); }
                if policy != ConnectionPolicy::Baseline {
                    let effective = apply_policy(connection, policy)?;
                    if report.policy_observations.is_empty() { report.policy_observations.push(effective); }
                }
                let query_started = Instant::now();
                let result = if index == writer_count {
                    connection.query_row("SELECT value FROM meta WHERE key=?1", [KEY], |row| row.get::<_, String>(0))
                } else {
                    connection.query_row("SELECT status FROM job WHERE id=?1", [READ_JOB], |row| row.get::<_, String>(0))
                };
                query_ms = query_started.elapsed().as_millis();
                result?;
                Ok(())
            })
        } else if is_writer {
            database.write(context, TransactionBehavior::Immediate, |transaction| {
                report.observe_mmap_size(transaction)?;
                if lock_probe && report.first_native_filename.is_none() { report.first_native_filename = Some(native_filename_observation(transaction)); }
                let query_started = Instant::now();
                let result = transaction.execute(
                    "UPDATE meta SET value=CAST(CAST(value AS INTEGER)+1 AS TEXT) WHERE key=?1", [KEY]);
                query_ms = query_started.elapsed().as_millis();
                if result? != 1 { return Err(voxvulgi_engine::EngineError::InstallFailed("Disposable counter missing".into())); }
                Ok(())
            })
        } else {
            database.read(context, |connection| {
                report.observe_mmap_size(connection)?;
                if lock_probe && report.first_native_filename.is_none() { report.first_native_filename = Some(native_filename_observation(connection)); }
                if policy != ConnectionPolicy::Baseline {
                    let effective = apply_policy(connection, policy)?;
                    if report.policy_observations.is_empty() { report.policy_observations.push(effective); }
                }
                let query_started = Instant::now();
                let result = connection.query_row("SELECT value FROM meta WHERE key=?1", [KEY], |row| row.get::<_, String>(0));
                query_ms = query_started.elapsed().as_millis();
                result?;
                Ok(())
            })
        };
        let elapsed = started.elapsed().as_millis();
        let metrics = db::AppDatabase::finish_vfs_timing_probe();
        // Every operation context has dropped; serialization/aggregation is outside measured IO.
        if lock_probe { report.lock_summary.add(&request_id, lock_capture.snapshot()); }
        drop(lock_capture);
        for (i, (name, calls, total_ns, max_ns)) in metrics.iter().copied().enumerate() {
            vfs_names[i] = name;
            vfs_totals[i].0 = vfs_totals[i].0.saturating_add(calls);
            vfs_totals[i].1 = vfs_totals[i].1.saturating_add(total_ns);
            vfs_totals[i].2 = vfs_totals[i].2.max(max_ns);
        }
        if elapsed >= 1000 && report.slow_vfs_operations.len() < 32 {
            report.slow_vfs_operations.push(json!({"request_id":request_id,"elapsed_ms":elapsed,"query_ms":query_ms,"callbacks":metrics}));
        }
        report.total_operation_ms += elapsed;
        report.max_operation_ms = report.max_operation_ms.max(elapsed);
        report.total_query_ms += query_ms;
        report.max_query_ms = report.max_query_ms.max(query_ms);
        match result {
            Ok(()) => report.succeeded += 1,
            Err(error) => {
                let message = error.to_string();
                let kind = ["read_admission_timeout", "writer_admission_timeout", "read_admission_overloaded", "writer_queue_overloaded", "database is locked"]
                    .into_iter().find(|kind| message.contains(kind)).unwrap_or("other");
                *report.failures_by_kind.entry(kind.into()).or_default() += 1;
                if report.first_failures.len() < 32 {
                    report.first_failures.push(json!({"request_id":request_id,"error":message,"elapsed_ms":elapsed}));
                }
            }
        }
        // Realistic polling/dispatch cadence only after contexts, connections and permits drop.
        let remaining = deadline.saturating_duration_since(Instant::now());
        if interval_ms != 0 && !remaining.is_zero() {
            std::thread::sleep(Duration::from_millis(interval_ms).min(remaining));
        } else {
            std::thread::yield_now();
        }
    }
    report.vfs_file_kind_proven = report.attempts != 0 && vfs_totals[0].0 != 0
        && vfs_totals[7].0 == 0 && vfs_totals[8].0 == 0
        && vfs_totals[11].0 == 0 && vfs_totals[13].0 == 0
        && vfs_totals[0].0 == vfs_totals[9].0 + vfs_totals[10].0 + vfs_totals[11].0
        && vfs_totals[0].0 == vfs_totals[12].0 + vfs_totals[13].0;
    report.lock_summary.enabled = lock_probe;
    report.lock_summary.attribution_proven = lock_probe && report.lock_summary.operations > 0
        && report.lock_summary.coverage_failures == 0 && report.lock_summary.native_shm_calls > 0
        && report.first_native_filename.as_ref().is_some_and(|observation|observation.coverage_proven);
    report.vfs_timings = vfs_totals.iter().enumerate().map(|(i, (calls, total_ns, max_ns))|
        json!({"callback":vfs_names[i],"calls":calls,"total_ns":total_ns,"max_single_call_ns":max_ns})).collect();
    report
}

fn main() -> ProbeResult<()> {
    let (root, seconds, source, workload, reader_interval_ms, writer_interval_ms, reader_count, writer_count, policy, maintenance_interval_ms, reader_pin_ms, reopen_owner, recovery_retries, keeper_enabled, lock_probe, plain_local_probe) = parse()?;
    std::fs::create_dir(&root)?;
    let root = root.canonicalize()?;
    reject_protected(&root)?;
    std::fs::write(root.join("wp0323_disposable_fixture.json"), serde_json::to_vec_pretty(&json!({"root":root,"source_backup":source,"seconds":seconds,"pid":std::process::id()}))?)?;
    let paths = AppPaths::new(root.clone());
    paths.ensure_dirs()?;
    let database_path = paths.db_dir().join("app.sqlite");
    let filename_probe = if plain_local_probe { Some(db::AppDatabase::prepare_disposable_filename_probe(&root, &database_path)?) } else { None };
    if let Some(probe) = filename_probe.clone() { db::AppDatabase::enable_disposable_filename_probe(probe); }
    let _filename_probe_guard = FilenameProbeGuard;
    let mut backup_provenance = None;
    if let Some(source) = source.as_ref() {
        reject_sidecars(source)?;
        let before = hash_file(source)?;
        let source_schema = standalone_schema(source)?;
        if ![59, 60, CURRENT_PROBE_SCHEMA].contains(&source_schema) { return Err("Source backup schema changed".into()); }
        std::fs::copy(source, &database_path)?;
        let after = hash_file(source)?;
        let destination = hash_file(&database_path)?;
        reject_sidecars(source)?;
        reject_sidecars(&database_path)?;
        let destination_schema = standalone_schema(&database_path)?;
        let matches = before == after && before == destination && destination_schema == source_schema;
        backup_provenance = Some(json!({"source":source,"destination":database_path,
            "source_sha256_before_copy":before.0,"source_sha256_after_copy":after.0,
            "destination_sha256_before_setup":destination.0,"size_bytes":before.1,
            "source_schema":source_schema,"destination_schema_before_setup":destination_schema,
            "source_size_bytes_before_copy":before.1,"source_size_bytes_after_copy":after.1,
            "destination_size_bytes_before_setup":destination.1,"hash_size_schema_match":matches}));
        std::fs::write(root.join("backup_copy_provenance.json"), serde_json::to_vec_pretty(&backup_provenance)?)?;
        if !matches {
            return Err("Source/destination backup hash, size or schema mismatch; workload not started".into());
        }
    }
    // Startup-only disposable schema initialization, before runtime admission/workers.
    let setup = Connection::open(db::AppDatabase::sqlite_open_filename(&db::AppDatabase::disposable_open_filename(&database_path)?))?;
    setup.pragma_update(None, "journal_mode", "WAL")?;
    setup.pragma_update(None, "synchronous", "NORMAL")?;
    setup.pragma_update(None, "foreign_keys", "ON")?;
    let destination_schema_before_migration = db::schema_user_version(&setup)?;
    if destination_schema_before_migration != CURRENT_PROBE_SCHEMA { db::migrate(&setup)?; }
    let destination_schema_after_migration = db::schema_user_version(&setup)?;
    if destination_schema_after_migration != CURRENT_PROBE_SCHEMA { return Err("Disposable migration did not reach schema61".into()); }
    let page_count: u64 = setup.pragma_query_value(None, "page_count", |row|row.get(0))?;
    let page_size: u64 = setup.pragma_query_value(None, "page_size", |row|row.get(0))?;
    let job_count: u64 = setup.query_row("SELECT COUNT(*) FROM job", [], |row|row.get(0))?;
    let job_params: String = setup.query_row("SELECT params_json FROM job WHERE type='download_direct_url' AND status='queued' LIMIT 1", [], |row|row.get(0)).unwrap_or_else(|_| "{\"url\":\"https://example.invalid/wp0323-disposable\"}".into());
    if workload == Workload::JobInsert {
        let existing: u64 = setup.query_row("SELECT COUNT(*) FROM job WHERE id GLOB 'wp0323_churn_fixture_*'", [], |row|row.get(0))?;
        if existing != 0 { return Err("Fixture ID prefix already exists; refusing to reuse copied rows".into()); }
        insert_job(&setup, READ_JOB, &job_params)?;
    }
    let job_params = Arc::new(job_params);
    setup.execute("INSERT INTO meta(key,value) VALUES(?1,'0') ON CONFLICT(key) DO UPDATE SET value='0'", [KEY])?;
    let setup_native_state = native_owner_state(&setup);
    let setup_mmap_size_bytes: i64 = setup.pragma_query_value(None, "mmap_size", |row|row.get(0))?;
    drop(setup);
    let database = db::AppDatabase::for_paths(&paths)?;
    let prepared_maintenance = if maintenance_interval_ms != 0 { Some(prepare_maintenance(&database_path)?) } else { None };
    let keeper_capture = LockCapture::begin(lock_probe);
    let locks_before_keeper_open = keeper_capture.snapshot();
    let keeper = if keeper_enabled {
        Some(prepare_keeper(&database_path, &prepared_maintenance.as_ref().expect("maintenance required").2, lock_probe)?)
    } else { None };
    let barrier = Arc::new(Barrier::new(reader_count+writer_count+1+usize::from(prepared_maintenance.is_some())));
    let maintenance_stop = Arc::new(AtomicBool::new(false));
    let started = Instant::now();
    let deadline = started + Duration::from_secs(seconds);
    let maintenance = prepared_maintenance.map(|(connection, pin, identity)| {
        let barrier = Arc::clone(&barrier);
        let stop = Arc::clone(&maintenance_stop);
        let path = database_path.clone();
        let probe = filename_probe.clone();
        std::thread::spawn(move || {
            if let Some(probe) = probe { db::AppDatabase::enable_disposable_filename_probe(probe); }
            let _guard = FilenameProbeGuard;
            maintenance_worker(connection, pin, identity, barrier, stop,
                path, page_size, maintenance_interval_ms, reader_pin_ms, reopen_owner, recovery_retries, lock_probe)
        })
    });
    let workers = (0..reader_count+writer_count).map(|index| {
        let database = database.clone();
        let barrier = Arc::clone(&barrier);
        let job_params = Arc::clone(&job_params);
        let probe = filename_probe.clone();
        std::thread::spawn(move || {
            if let Some(probe) = probe { db::AppDatabase::enable_disposable_filename_probe(probe); }
            let _guard = FilenameProbeGuard;
            worker(database, barrier, deadline, index, writer_count, workload, job_params, if index < writer_count { writer_interval_ms } else { reader_interval_ms }, policy, lock_probe)
        })
    }).collect::<Vec<_>>();
    println!("{}", json!({"event":"started","pid":std::process::id(),"root":root,"seconds":seconds,"writer_count":writer_count,"reader_count":reader_count,"keeper":keeper_enabled,"workload":format!("{workload:?}"),"connection_policy":format!("{policy:?}"),"reader_interval_ms":reader_interval_ms,"writer_interval_ms":writer_interval_ms,"source_job_count":job_count,"page_count":page_count,"page_size":page_size,"job_params_bytes":job_params.len(),"destination_schema_before_migration":destination_schema_before_migration,"destination_schema_after_migration":destination_schema_after_migration}));
    barrier.wait();
    let mut reports = Vec::new();
    let mut panics = 0;
    for worker in workers {
        match worker.join() { Ok(report) => reports.push(report), Err(_) => panics += 1 }
    }
    let after_workers = database.snapshot();
    let runtime_active_after_workers = json!({"active_operations":after_workers.active_operations,
        "active_readers":after_workers.active_readers,"writer_active":after_workers.writer_active,
        "waiting_readers":after_workers.waiting_readers,"waiting_writers":after_workers.waiting_writers});
    let maintenance_stop_requested_at_ms = started.elapsed().as_millis();
    maintenance_stop.store(true, Ordering::Release);
    let maintenance_join_started = Instant::now();
    let maintenance_report = maintenance.map(|worker| match worker.join() {
        Ok(report) => report,
        Err(_) => { panics += 1; json!({"panic":true}) }
    });
    let maintenance_join_ms = maintenance_join_started.elapsed().as_millis();
    let maintenance_join_finished_at_ms = started.elapsed().as_millis();
    // Keeper outlives both owned joins, then closes before canonical read and runtime drain.
    let keeper_close_requested_at_ms = started.elapsed().as_millis();
    let keeper_report = keeper.map(|(connection, identity)| {
        let locks_before_close = keeper_capture.snapshot();
        let before_close = native_owner_state(&connection);
        let idle = idle_owner_verified(&before_close);
        let close_started = Instant::now();
        let close_error = match connection.close() {
            Ok(()) => None,
            Err((owned,error)) => { drop(owned); Some(error.to_string()) },
        };
        json!({"identity":identity,"before_close":before_close,"idle_verified":idle,
            "close_error":close_error,"close_ms":close_started.elapsed().as_millis(),
            "close_requested_at_ms":keeper_close_requested_at_ms,
            "maintenance_join_finished_at_ms":maintenance_join_finished_at_ms,
            "locks_before_open":locks_before_keeper_open,"locks_before_close":locks_before_close,
            "locks_after_checked_close":keeper_capture.snapshot(),
            "closed_after_workers_and_maintenance_join":true,"closed_before_canonical_read_and_database_drain":true})
    });
    drop(keeper_capture);
    let keeper_proven = keeper_report.as_ref().map(|report|
        report["idle_verified"].as_bool() == Some(true) && report["close_error"].is_null()).unwrap_or(true);
    let maintenance_proven = maintenance_report.as_ref().map(|report|
        report["errors"].as_u64() == Some(0) && report["receipt_overflow"].as_u64() == Some(0)
        && report["partial_during_pin"].as_bool() == Some(true)
        && report["recovered_after_pin"].as_bool() == Some(true)
        && report["final_checkpoint_complete"].as_bool() == Some(true)
        && report["owner_close_error"].is_null()
        && report["pin_release_error"].is_null() && report["stop_observed"].as_bool() == Some(true)
    ).unwrap_or(true);
    let post_keeper_close_diagnostics = if keeper_enabled
        && keeper_report.as_ref().is_some_and(|report| report["close_error"].is_null())
        && maintenance_report.as_ref().is_some_and(|report|
            report["final_checkpoint_complete"].as_bool() == Some(false))
    {
        let diagnostic_started_at_ms = started.elapsed().as_millis();
        let default = fresh_checkpoint_diagnostic(&database_path, None, lock_probe);
        let counted = fresh_checkpoint_diagnostic(&database_path, Some("voxvulgi_read_counting"), lock_probe);
        json!({"started_at_ms":diagnostic_started_at_ms,
            "maintenance_join_finished_at_ms":maintenance_join_finished_at_ms,
            "keeper_close_requested_at_ms":keeper_close_requested_at_ms,
            "workers_and_maintenance_joined":true,"keeper_checked_close_succeeded":true,
            "before_canonical_read_and_database_drain":true,
            "results":[default,counted],"original_verdict_not_overridden":true,
            "limits":"Sequential diagnostics: the default-VFS checkpoint and close may change the state observed by the subsequent exact registered-VFS checkpoint. These results do not alter the original final checkpoint or maintenance verdict; SQL transaction NONE does not establish absence of a WAL read lock."})
    } else { serde_json::Value::Null };
    let verification = database.read(db::DatabaseOperationContext::new("probe_verify", "disposable_counter_read"), |connection| {
        if policy != ConnectionPolicy::Baseline { apply_policy(connection, policy)?; }
        if workload == Workload::Meta {
            Ok(connection.query_row("SELECT value FROM meta WHERE key=?1", [KEY], |row| row.get::<_, String>(0))?)
        } else {
            let count: u64 = connection.query_row("SELECT COUNT(*) FROM job WHERE id GLOB 'wp0323_churn_fixture_*' AND id<>?1", [READ_JOB], |row|row.get(0))?;
            Ok(count.to_string())
        }
    });
    let connection_policy_proven = policy == ConnectionPolicy::Baseline || (reports.len() == reader_count+writer_count && reports.iter().all(|report| !report.policy_observations.is_empty()));
    let vfs_file_kind_proven = reports.len() == reader_count+writer_count && reports.iter().all(|report| report.vfs_file_kind_proven);
    let writer_succeeded = Some(reports.iter().filter(|report| report.lane.starts_with("churn_writer_")).map(|report| report.succeeded).sum::<u64>());
    let counter = verification.as_ref().ok().and_then(|value| value.parse::<u64>().ok());
    let canonical_counter_matches = panics == 0 && writer_succeeded.is_some() && writer_succeeded == counter;
    let maintenance_nonempty_commits = maintenance_report.is_none()
        || (writer_succeeded.unwrap_or(0) > 0 && counter.unwrap_or(0) > 0);
    let maintenance_proven = maintenance_proven && maintenance_nonempty_commits && keeper_proven;
    let drain_started = Instant::now();
    let drain = database.shutdown_and_drain(db::SHUTDOWN_DRAIN_TIMEOUT);
    let snapshot = database.snapshot();
    let wal_path = PathBuf::from(format!("{}-wal", database_path.display()));
    let wal_bytes_after_workload = std::fs::metadata(&wal_path).map(|m|m.len()).unwrap_or(0);
    let wal_frames_after_workload = wal_bytes_after_workload.saturating_sub(32)/(page_size+24);
    let failures = reports.iter().map(|report| report.attempts-report.succeeded).sum::<u64>();
    let summary = json!({"event":"terminal","root":root,"source_backup":source,"backup_provenance":backup_provenance,"workload":format!("{workload:?}"),"connection_policy":format!("{policy:?}"),"reader_interval_ms":reader_interval_ms,"writer_interval_ms":writer_interval_ms,"writer_count":writer_count,"reader_count":reader_count,"page_count_before_workload":page_count,"page_size":page_size,"wal_bytes_after_workload":wal_bytes_after_workload,"wal_frames_after_workload":wal_frames_after_workload,"source_job_count":job_count,"destination_schema_before_migration":destination_schema_before_migration,"destination_schema_after_migration":destination_schema_after_migration,"elapsed_ms":started.elapsed().as_millis(),"workers":reports,"worker_panics":panics,"failures":failures,"canonical_counter_matches":canonical_counter_matches,"connection_policy_proven":connection_policy_proven,"vfs_file_kind_proven":vfs_file_kind_proven,"counter":counter,"verification_error":verification.err().map(|error|error.to_string()),"shutdown":{"elapsed_ms":drain_started.elapsed().as_millis(),"error":drain.as_ref().err().map(ToString::to_string)},"runtime":snapshot,"limits":"Recent runtime receipts are bounded512 and do not represent all operations. Tight or cadenced churn is a hypothesis probe; no live symptom or causal attribution is claimed. VFS aggregate/category durations overlap and are not a partition; xSyncUnknown or fileKindOverflow invalidate file-kind attribution; unwrapped native SHM calls appear only within xShmUnmap. Disabling automatic checkpoints is a harness-only counterfactual; sustained WAL growth and production checkpoint maintenance are unproven. Runtime SQL/open/close bounds are unchanged; pathological close may prevent terminal output."});
    let mut summary = summary;
    summary["maintenance"] = json!({"enabled":maintenance_interval_ms != 0,
        "interval_ms":maintenance_interval_ms,"reader_pin_ms":reader_pin_ms,
        "reopen_owner":reopen_owner,
        "lock_probe_enabled":lock_probe,
        "plain_local_filename_probe_enabled":plain_local_probe,"setup_native_state":setup_native_state,
        "setup_mmap_size_bytes":setup_mmap_size_bytes,
        "mmap_readback_limits":"Read-only setup and first-worker-connection observations before the job SQL timer; no mmap setting changes. First-worker readback adds observer work to operation elapsed time. Getter errors use existing failure retention.",
        "worker_lock_capture_limits":"All operation captures include context close; snapshots and aggregation follow operation elapsed/VFS timing capture. Native filename inspection and callbacks still have observer CPU/scheduling effects. Historical inferred masks are not physical OS lock-state proof; worker diagnostics never alter the original verdict.",
        "recovery_retry_budget":recovery_retries,
        "keeper_enabled":keeper_enabled,"keeper_proven":keeper_proven,"keeper_report":keeper_report,
        "post_keeper_close_diagnostics":post_keeper_close_diagnostics,
        "stop_requested_at_ms":maintenance_stop_requested_at_ms,"join_ms":maintenance_join_ms,
        "proven":maintenance_proven,"report":maintenance_report,
        "nonempty_committed_workload":maintenance_nonempty_commits,
        "runtime_active_after_workers_join":runtime_active_after_workers,
        "canonical_reconciliation_after_maintenance_join":canonical_counter_matches});
    let (native_filename_observations, all_plain) = plain_native_filename_coverage(&summary);
    let worker_plain = summary["workers"].as_array().is_some_and(|workers|
        workers.len() == reader_count + writer_count && workers.iter().all(|worker|
            worker["first_native_filename"]["prefix_class"].as_str() == Some("plain_local")
                && worker["first_native_filename"]["coverage_proven"].as_bool() == Some(true)));
    let filename_proven = !plain_local_probe || (native_filename_observations > (reader_count+writer_count) as u64 && all_plain && worker_plain);
    summary["filename_counterfactual"] = json!({"enabled":plain_local_probe,
        "native_observations":native_filename_observations,"all_plain_local":all_plain,
        "every_worker_plain_local":worker_plain,"proven":filename_proven,
        "limits":"Exact validated disposable identity only; immutable standalone inspection URIs are excluded. Native filename coverage does not select a production path or policy solution."});
    let production_filename = db::AppDatabase::sqlite_open_filename(&database_path);
    let expected_filename_sha256 = production_filename.to_str().map(|text|hex::encode(Sha256::digest(text.as_bytes())));
    let (owner_observations, all_owner_names_agree) = expected_filename_sha256.as_deref()
        .map(|expected|native_filename_identity_coverage(&summary, expected)).unwrap_or((0,false));
    summary["production_filename_policy"] = json!({"active":true,"fixture_tls_enabled":plain_local_probe,
        "expected_native_filename_sha256":expected_filename_sha256,"native_owner_observations":owner_observations,
        "all_observed_owner_names_agree":all_owner_names_agree,
        "tls_off_boundary_proven":!plain_local_probe && lock_probe && owner_observations > (reader_count+writer_count) as u64
            && all_owner_names_agree && all_plain && worker_plain,
        "limits":"Additional filename observation only; original worker/maintenance/acknowledgement/drain verdict is unchanged. Require lock-probe coverage with fixture TLS off for a natural production-path boundary. Immutable standalone backup URI is excluded."});
    std::fs::write(root.join("churn_summary.json"), serde_json::to_vec_pretty(&summary)?)?;
    println!("{}", summary);
    if failures != 0 || panics != 0 || !canonical_counter_matches || !connection_policy_proven || !vfs_file_kind_proven || !maintenance_proven || !filename_proven || drain.is_err() { return Err("Probe failed; inspect churn_summary.json".into()); }
    Ok(())
}
