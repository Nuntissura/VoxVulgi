//! Disposable non-cfg(test) AppDatabase connection-lifetime probe.
//! Usage: --root <new-absolute-directory> [--seconds 20] [--database-copy <schema59-or60-backup>] [--workload meta|job_insert] [--reader-interval-ms 0..1000] [--writer-interval-ms 0..1000] [--reader-count 1..16] [--writer-count 1..4] [--connection-policy baseline|no_close_checkpoint|no_close_checkpoint_full]
//! No live backup, keeper connection, injected lock, or admission delay.
use rusqlite::{config::DbConfig, Connection, OpenFlags, TransactionBehavior};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};
use voxvulgi_engine::{db, paths::AppPaths};

type ProbeResult<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const KEY: &str = "wp0323_disposable_churn_counter";
const MAX_ITERATIONS: u64 = 100_000;
const CURRENT_PROBE_SCHEMA: u32 = 60;
const JOB_PREFIX: &str = "wp0323_churn_fixture_";
const READ_JOB: &str = "wp0323_churn_fixture_read";

#[derive(Clone, Copy, Debug, PartialEq)]
enum Workload { Meta, JobInsert }

#[derive(Clone, Copy, Debug, PartialEq)]
enum ConnectionPolicy { Baseline, NoCloseCheckpoint, NoCloseCheckpointFull }

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
    vfs_timings: Vec<serde_json::Value>,
    slow_vfs_operations: Vec<serde_json::Value>,
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

fn parse() -> ProbeResult<(PathBuf, u64, Option<PathBuf>, Workload, u64, u64, usize, usize, ConnectionPolicy)> {
    let mut root = None;
    let mut seconds = 20;
    let mut copy = None;
    let mut workload = Workload::Meta;
    let mut policy = ConnectionPolicy::Baseline;
    let mut reader_interval_ms: u64 = 0;
    let mut writer_interval_ms: u64 = 0;
    let mut reader_count: usize = 4;
    let mut writer_count: usize = 1;
    let mut args = std::env::args_os().skip(1);
    while let Some(argument) = args.next() {
        let value = args.next().ok_or("Every argument requires a value")?;
        match argument.to_str() {
            Some("--root") => root = Some(PathBuf::from(value)),
            Some("--seconds") => seconds = value.to_str().ok_or("Invalid seconds")?.parse()?,
            Some("--reader-count") => reader_count = value.to_str().ok_or("Invalid reader count")?.parse()?,
            Some("--writer-count") => writer_count = value.to_str().ok_or("Invalid writer count")?.parse()?,
            Some("--reader-interval-ms") => reader_interval_ms = value.to_str().ok_or("Invalid reader interval")?.parse()?,
            Some("--writer-interval-ms") => writer_interval_ms = value.to_str().ok_or("Invalid writer interval")?.parse()?,
            Some("--database-copy") => copy = Some(PathBuf::from(value)),
            Some("--connection-policy") => policy = match value.to_str() { Some("baseline") => ConnectionPolicy::Baseline, Some("no_close_checkpoint") => ConnectionPolicy::NoCloseCheckpoint, Some("no_close_checkpoint_full") => ConnectionPolicy::NoCloseCheckpointFull, _ => return Err("Unsupported connection-policy".into()) },
            Some("--workload") => workload = match value.to_str() { Some("meta") => Workload::Meta, Some("job_insert") => Workload::JobInsert, _ => return Err("workload must be meta or job_insert".into()) },
            _ => return Err("Unknown argument; see usage at source header".into()),
        }
    }
    if !(1..=30).contains(&seconds) { return Err("seconds must be1..30".into()); }
    if reader_interval_ms > 1000 || writer_interval_ms > 1000 { return Err("Intervals must be0..1000ms".into()); }
    if !(1..=16).contains(&reader_count) || !(1..=4).contains(&writer_count) { return Err("reader-count must be1..16 and writer-count1..4".into()); }
    if policy != ConnectionPolicy::Baseline && workload != Workload::JobInsert { return Err("Nonbaseline connection policy requires job_insert workload so synchronous is set before any transaction".into()); }
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
        if ![59, CURRENT_PROBE_SCHEMA].contains(&standalone_schema(&source)?) {
            return Err("database-copy must have schema59 or60; only the disposable destination may migrate".into());
        }
        return Ok((root, seconds, Some(source), workload, reader_interval_ms, writer_interval_ms, reader_count, writer_count, policy));
    }
    Ok((root, seconds, None, workload, reader_interval_ms, writer_interval_ms, reader_count, writer_count, policy))
}



// Counterfactual stays on disposable AppDatabase contexts; no environment or production defaults.
fn apply_policy(connection: &Connection, policy: ConnectionPolicy) -> voxvulgi_engine::Result<serde_json::Value> {
    let disabled = connection.set_db_config(DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true)?;
    if policy == ConnectionPolicy::NoCloseCheckpointFull {
        connection.pragma_update(None, "synchronous", "FULL")?;
    }
    let effective = connection.db_config(DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE)?;
    let synchronous: i64 = connection.pragma_query_value(None, "synchronous", |row|row.get(0))?;
    let autocheckpoint: i64 = connection.pragma_query_value(None, "wal_autocheckpoint", |row|row.get(0))?;
    if !disabled || !effective || autocheckpoint != 1000
        || (policy == ConnectionPolicy::NoCloseCheckpointFull && synchronous != 2) {
        return Err(voxvulgi_engine::EngineError::InstallFailed("Counterfactual connection policy did not match requested settings".into()));
    }
    Ok(json!({"no_checkpoint_on_close":effective,"synchronous":synchronous,"wal_autocheckpoint":autocheckpoint}))
}

// Exact enqueue SQL shape; synthetic IDs stay confined to the disposable root and no runner starts.
fn insert_job(connection: &Connection, id: &str, params_json: &str) -> rusqlite::Result<usize> {
    connection.execute(
        "INSERT INTO job(id,item_id,batch_id,type,status,progress,error,params_json,created_at_ms,started_at_ms,finished_at_ms,logs_path,lane,track,target_key,attempt_no) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,1)",
        rusqlite::params![id, Option::<String>::None, Option::<String>::None, "download_direct_url", "queued", 0.0_f32, Option::<String>::None, params_json, std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis().min(i64::MAX as u128) as i64, Option::<i64>::None, Option::<i64>::None, format!("{id}.jsonl"), "recurring", "youtube_recurring", Option::<String>::None])
}

fn worker(database: db::AppDatabase, barrier: Arc<Barrier>, deadline: Instant, index: usize, writer_count: usize, workload: Workload, job_params: Arc<String>, interval_ms: u64, policy: ConnectionPolicy) -> WorkerReport {
    let is_writer = index < writer_count;
    let lane = if is_writer { format!("churn_writer_{index}") } else { format!("churn_reader_{}", index-writer_count+1) };
    let mut report = WorkerReport { lane: lane.clone(), ..Default::default() };
    let mut vfs_totals = [(0_u64, 0_u64, 0_u64); 5];
    let mut vfs_names = [""; 5];
    barrier.wait();
    while Instant::now() < deadline && report.attempts < MAX_ITERATIONS {
        report.attempts += 1;
        let request_id = format!("{lane}-{}", report.attempts);
        let context = db::DatabaseOperationContext::new(&lane, if workload == Workload::Meta { "wp0323_short_meta_churn" } else { "wp0323_job_insert_churn" })
            .with_request_id(&request_id);
        db::AppDatabase::begin_vfs_timing_probe();
        let started = Instant::now();
        let mut query_ms = 0;
        let result = if workload == Workload::JobInsert && is_writer {
            // Same autocommit INSERT column/value shape as jobs.rs enqueue; no extra transaction.
            (|| {
                let connection = database.write_context(context)?;
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
                let query_started = Instant::now();
                let result = transaction.execute(
                    "UPDATE meta SET value=CAST(CAST(value AS INTEGER)+1 AS TEXT) WHERE key=?1", [KEY]);
                query_ms = query_started.elapsed().as_millis();
                if result? != 1 { return Err(voxvulgi_engine::EngineError::InstallFailed("Disposable counter missing".into())); }
                Ok(())
            })
        } else {
            database.read(context, |connection| {
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
    report.vfs_timings = vfs_totals.iter().enumerate().map(|(i, (calls, total_ns, max_ns))|
        json!({"callback":vfs_names[i],"calls":calls,"total_ns":total_ns,"max_single_call_ns":max_ns})).collect();
    report
}

fn main() -> ProbeResult<()> {
    let (root, seconds, source, workload, reader_interval_ms, writer_interval_ms, reader_count, writer_count, policy) = parse()?;
    std::fs::create_dir(&root)?;
    let root = root.canonicalize()?;
    reject_protected(&root)?;
    std::fs::write(root.join("wp0323_disposable_fixture.json"), serde_json::to_vec_pretty(&json!({"root":root,"source_backup":source,"seconds":seconds,"pid":std::process::id()}))?)?;
    let paths = AppPaths::new(root.clone());
    paths.ensure_dirs()?;
    let database_path = paths.db_dir().join("app.sqlite");
    let mut backup_provenance = None;
    if let Some(source) = source.as_ref() {
        reject_sidecars(source)?;
        let before = hash_file(source)?;
        let source_schema = standalone_schema(source)?;
        if ![59, CURRENT_PROBE_SCHEMA].contains(&source_schema) { return Err("Source backup schema changed".into()); }
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
    let setup = Connection::open(&database_path)?;
    setup.pragma_update(None, "journal_mode", "WAL")?;
    setup.pragma_update(None, "synchronous", "NORMAL")?;
    setup.pragma_update(None, "foreign_keys", "ON")?;
    let destination_schema_before_migration = db::schema_user_version(&setup)?;
    if destination_schema_before_migration != CURRENT_PROBE_SCHEMA { db::migrate(&setup)?; }
    let destination_schema_after_migration = db::schema_user_version(&setup)?;
    if destination_schema_after_migration != CURRENT_PROBE_SCHEMA { return Err("Disposable migration did not reach schema60".into()); }
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
    drop(setup);
    let database = db::AppDatabase::for_paths(&paths)?;
    let barrier = Arc::new(Barrier::new(reader_count+writer_count+1));
    let started = Instant::now();
    let deadline = started + Duration::from_secs(seconds);
    let workers = (0..reader_count+writer_count).map(|index| {
        let database = database.clone();
        let barrier = Arc::clone(&barrier);
        let job_params = Arc::clone(&job_params);
        std::thread::spawn(move || worker(database, barrier, deadline, index, writer_count, workload, job_params, if index < writer_count { writer_interval_ms } else { reader_interval_ms }, policy))
    }).collect::<Vec<_>>();
    println!("{}", json!({"event":"started","pid":std::process::id(),"root":root,"seconds":seconds,"writer_count":writer_count,"reader_count":reader_count,"keeper":false,"workload":format!("{workload:?}"),"connection_policy":format!("{policy:?}"),"reader_interval_ms":reader_interval_ms,"writer_interval_ms":writer_interval_ms,"source_job_count":job_count,"page_count":page_count,"page_size":page_size,"job_params_bytes":job_params.len(),"destination_schema_before_migration":destination_schema_before_migration,"destination_schema_after_migration":destination_schema_after_migration}));
    barrier.wait();
    let mut reports = Vec::new();
    let mut panics = 0;
    for worker in workers {
        match worker.join() { Ok(report) => reports.push(report), Err(_) => panics += 1 }
    }
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
    let writer_succeeded = Some(reports.iter().filter(|report| report.lane.starts_with("churn_writer_")).map(|report| report.succeeded).sum::<u64>());
    let counter = verification.as_ref().ok().and_then(|value| value.parse::<u64>().ok());
    let canonical_counter_matches = panics == 0 && writer_succeeded.is_some() && writer_succeeded == counter;
    let drain_started = Instant::now();
    let drain = database.shutdown_and_drain(db::SHUTDOWN_DRAIN_TIMEOUT);
    let snapshot = database.snapshot();
    let wal_path = PathBuf::from(format!("{}-wal", database_path.display()));
    let wal_bytes_after_workload = std::fs::metadata(&wal_path).map(|m|m.len()).unwrap_or(0);
    let wal_frames_after_workload = wal_bytes_after_workload.saturating_sub(32)/(page_size+24);
    let failures = reports.iter().map(|report| report.attempts-report.succeeded).sum::<u64>();
    let summary = json!({"event":"terminal","root":root,"source_backup":source,"backup_provenance":backup_provenance,"workload":format!("{workload:?}"),"connection_policy":format!("{policy:?}"),"reader_interval_ms":reader_interval_ms,"writer_interval_ms":writer_interval_ms,"writer_count":writer_count,"reader_count":reader_count,"page_count_before_workload":page_count,"page_size":page_size,"wal_bytes_after_workload":wal_bytes_after_workload,"wal_frames_after_workload":wal_frames_after_workload,"source_job_count":job_count,"destination_schema_before_migration":destination_schema_before_migration,"destination_schema_after_migration":destination_schema_after_migration,"elapsed_ms":started.elapsed().as_millis(),"workers":reports,"worker_panics":panics,"failures":failures,"canonical_counter_matches":canonical_counter_matches,"connection_policy_proven":connection_policy_proven,"counter":counter,"verification_error":verification.err().map(|error|error.to_string()),"shutdown":{"elapsed_ms":drain_started.elapsed().as_millis(),"error":drain.as_ref().err().map(ToString::to_string)},"runtime":snapshot,"limits":"Recent runtime receipts are bounded512 and do not represent all operations. Tight or cadenced churn is a hypothesis probe; no live symptom or causal attribution is claimed. VFS callback durations may overlap and are not a partition; unwrapped native SHM calls appear only within xShmUnmap. Runtime SQL/open/close bounds are unchanged; pathological close may prevent terminal output."});
    std::fs::write(root.join("churn_summary.json"), serde_json::to_vec_pretty(&summary)?)?;
    println!("{}", summary);
    if failures != 0 || panics != 0 || !canonical_counter_matches || !connection_policy_proven || drain.is_err() { return Err("Probe failed; inspect churn_summary.json".into()); }
    Ok(())
}
