use rusqlite::types::ValueRef;
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::env;
use std::path::{Path, PathBuf};
use voxvulgi_engine::paths::AppPaths;
use voxvulgi_engine::provider_metadata::{
    provider_title_repair_status, repair_provider_titles_page, ProviderTitleRepairStatus,
};

#[derive(Debug, Serialize)]
struct BackupVerification {
    path: String,
    quick_check: String,
    job_count: i64,
    download_job_count: i64,
    logical_sha256: String,
    table_count: usize,
    file_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct MutationGuardVerification {
    bridge_pid: Option<u32>,
    bridge_pid_alive: bool,
    wal_bytes: u64,
}

#[derive(Debug, Serialize)]
struct RepairReceipt {
    base_dir: String,
    backup: BackupVerification,
    mutation_guard: MutationGuardVerification,
    pages_run: usize,
    page_size: usize,
    status: ProviderTitleRepairStatus,
}

fn usage() -> &'static str {
    "Usage: voxvulgi_provider_title_repair --apply --backup <verified-backup.sqlite> [--base-dir <app-data-dir>] [--page-size <1..500>] [--max-pages <n>]"
}

fn default_base_dir() -> Result<PathBuf, String> {
    let appdata = env::var_os("APPDATA")
        .ok_or_else(|| "APPDATA is unavailable; pass --base-dir explicitly".to_string())?;
    Ok(PathBuf::from(appdata).join("com.voxvulgi.voxvulgi"))
}

fn readonly_counts(path: &Path) -> Result<(i64, i64), String> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_FULL_MUTEX,
    )
    .map_err(|error| format!("open {}: {error}", path.display()))?;
    let job_count = conn
        .query_row("SELECT COUNT(*) FROM job", [], |row| row.get(0))
        .map_err(|error| format!("job count for {}: {error}", path.display()))?;
    let download_job_count = conn
        .query_row(
            "SELECT COUNT(*) FROM job WHERE type='download_direct_url'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| format!("download job count for {}: {error}", path.display()))?;
    Ok((job_count, download_job_count))
}

fn quote_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn hash_framed(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

fn hash_sql_value(hasher: &mut Sha256, value: ValueRef<'_>) {
    match value {
        ValueRef::Null => hasher.update([0]),
        ValueRef::Integer(value) => {
            hasher.update([1]);
            hasher.update(value.to_le_bytes());
        }
        ValueRef::Real(value) => {
            hasher.update([2]);
            hasher.update(value.to_bits().to_le_bytes());
        }
        ValueRef::Text(value) => {
            hasher.update([3]);
            hash_framed(hasher, value);
        }
        ValueRef::Blob(value) => {
            hasher.update([4]);
            hash_framed(hasher, value);
        }
    }
}

fn readonly_logical_fingerprint(path: &Path) -> Result<(String, usize), String> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_FULL_MUTEX,
    )
    .map_err(|error| format!("open {} for logical fingerprint: {error}", path.display()))?;
    let tables = {
        let mut stmt = conn
            .prepare("SELECT name, COALESCE(sql, '') FROM sqlite_schema WHERE type='table' ORDER BY name")
            .map_err(|error| format!("prepare schema fingerprint for {}: {error}", path.display()))?;
        stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| format!("query schema fingerprint for {}: {error}", path.display()))?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| format!("read schema fingerprint for {}: {error}", path.display()))?
    };
    let mut hasher = Sha256::new();
    hash_framed(&mut hasher, b"voxvulgi-logical-sqlite-v1");
    for (table_name, schema_sql) in &tables {
        hash_framed(&mut hasher, table_name.as_bytes());
        hash_framed(&mut hasher, schema_sql.as_bytes());
        let columns = {
            let mut stmt = conn
                .prepare("SELECT name, pk FROM pragma_table_info(?1) ORDER BY cid")
                .map_err(|error| format!("prepare columns for {table_name}: {error}"))?;
            stmt.query_map([table_name], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(|error| format!("query columns for {table_name}: {error}"))?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| format!("read columns for {table_name}: {error}"))?
        };
        if columns.is_empty() {
            return Err(format!(
                "cannot fingerprint table with no visible columns: {table_name}"
            ));
        }
        for (column, primary_key_order) in &columns {
            hash_framed(&mut hasher, column.as_bytes());
            hasher.update(primary_key_order.to_le_bytes());
        }
        let lower_columns = columns
            .iter()
            .map(|(name, _)| name.to_ascii_lowercase())
            .collect::<HashSet<_>>();
        let without_rowid = schema_sql.to_ascii_uppercase().contains("WITHOUT ROWID");
        let rowid_alias = if without_rowid {
            None
        } else {
            ["rowid", "_rowid_", "oid"]
                .into_iter()
                .find(|candidate| !lower_columns.contains(*candidate))
        };
        let quoted_columns = columns
            .iter()
            .map(|(name, _)| quote_identifier(name))
            .collect::<Vec<_>>();
        let order_columns = if let Some(alias) = rowid_alias {
            vec![quote_identifier(alias)]
        } else {
            let mut primary_keys = columns
                .iter()
                .filter(|(_, order)| *order > 0)
                .map(|(name, order)| (*order, quote_identifier(name)))
                .collect::<Vec<_>>();
            primary_keys.sort_by_key(|(order, _)| *order);
            if primary_keys.is_empty() {
                quoted_columns.clone()
            } else {
                primary_keys.into_iter().map(|(_, name)| name).collect()
            }
        };
        let mut selected_columns = quoted_columns.clone();
        if let Some(alias) = rowid_alias {
            selected_columns.insert(0, quote_identifier(alias));
        }
        let sql = format!(
            "SELECT {} FROM {} ORDER BY {}",
            selected_columns.join(","),
            quote_identifier(table_name),
            order_columns.join(",")
        );
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|error| format!("prepare logical fingerprint for {table_name}: {error}"))?;
        let column_count = selected_columns.len();
        let mut rows = stmt
            .query([])
            .map_err(|error| format!("query logical fingerprint for {table_name}: {error}"))?;
        while let Some(row) = rows
            .next()
            .map_err(|error| format!("read logical fingerprint row for {table_name}: {error}"))?
        {
            hasher.update([0x52]);
            for index in 0..column_count {
                hash_sql_value(
                    &mut hasher,
                    row.get_ref(index).map_err(|error| {
                        format!("read logical fingerprint value for {table_name}: {error}")
                    })?,
                );
            }
        }
    }
    Ok((hex::encode_upper(hasher.finalize()), tables.len()))
}

#[cfg(windows)]
fn process_is_alive(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return false;
    }
    unsafe {
        CloseHandle(handle);
    }
    true
}

#[cfg(not(windows))]
fn process_is_alive(_pid: u32) -> bool {
    false
}

fn verify_mutation_guard(base_dir: &Path) -> Result<MutationGuardVerification, String> {
    let bridge_path = base_dir.join("agent_bridge.json");
    let bridge_pid = if bridge_path.is_file() {
        let bytes = std::fs::read(&bridge_path)
            .map_err(|error| format!("read {}: {error}", bridge_path.display()))?;
        serde_json::from_slice::<serde_json::Value>(&bytes)
            .map_err(|error| format!("parse {}: {error}", bridge_path.display()))?
            .get("pid")
            .and_then(serde_json::Value::as_u64)
            .and_then(|pid| u32::try_from(pid).ok())
    } else {
        None
    };
    let bridge_pid_alive = bridge_pid.is_some_and(process_is_alive);
    if bridge_pid_alive {
        return Err(format!(
            "refusing live database mutation while VoxVulgi bridge pid {} is alive",
            bridge_pid.expect("alive pid")
        ));
    }
    let wal_path = base_dir.join("db").join("app.sqlite-wal");
    let wal_bytes = std::fs::metadata(&wal_path)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    if wal_bytes != 0 {
        return Err(format!(
            "refusing database mutation while WAL is non-empty: {} bytes at {}",
            wal_bytes,
            wal_path.display()
        ));
    }
    Ok(MutationGuardVerification {
        bridge_pid,
        bridge_pid_alive,
        wal_bytes,
    })
}

fn verify_backup(live_path: &Path, backup_path: &Path) -> Result<BackupVerification, String> {
    if !backup_path.is_file() {
        return Err(format!(
            "backup path is not a file: {}",
            backup_path.display()
        ));
    }
    let conn = Connection::open_with_flags(
        backup_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_FULL_MUTEX,
    )
    .map_err(|error| format!("open backup: {error}"))?;
    let quick_check: String = conn
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .map_err(|error| format!("backup quick_check: {error}"))?;
    if quick_check != "ok" {
        return Err(format!("backup quick_check failed: {quick_check}"));
    }
    drop(conn);
    let (live_jobs, live_download_jobs) = readonly_counts(live_path)?;
    let (backup_jobs, backup_download_jobs) = readonly_counts(backup_path)?;
    if (live_jobs, live_download_jobs) != (backup_jobs, backup_download_jobs) {
        return Err(format!(
            "backup preimage mismatch: live jobs/downloads={live_jobs}/{live_download_jobs}, backup={backup_jobs}/{backup_download_jobs}"
        ));
    }
    let (live_logical_sha256, live_table_count) = readonly_logical_fingerprint(live_path)?;
    let (backup_logical_sha256, backup_table_count) = readonly_logical_fingerprint(backup_path)?;
    if (live_table_count, &live_logical_sha256) != (backup_table_count, &backup_logical_sha256) {
        return Err(format!(
            "backup logical preimage mismatch: live tables/sha256={live_table_count}/{live_logical_sha256}, backup={backup_table_count}/{backup_logical_sha256}"
        ));
    }
    Ok(BackupVerification {
        path: backup_path.to_string_lossy().to_string(),
        quick_check,
        job_count: backup_jobs,
        download_job_count: backup_download_jobs,
        logical_sha256: backup_logical_sha256,
        table_count: backup_table_count,
        file_bytes: std::fs::metadata(backup_path)
            .map_err(|error| format!("backup metadata: {error}"))?
            .len(),
    })
}

fn run() -> Result<RepairReceipt, String> {
    let mut args = env::args().skip(1);
    let mut base_dir = None;
    let mut backup_path = None;
    let mut apply = false;
    let mut page_size = 500_usize;
    let mut max_pages = usize::MAX;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--base-dir" => {
                base_dir =
                    Some(PathBuf::from(args.next().ok_or_else(|| {
                        format!("missing value for --base-dir\n{}", usage())
                    })?));
            }
            "--backup" => {
                backup_path =
                    Some(PathBuf::from(args.next().ok_or_else(|| {
                        format!("missing value for --backup\n{}", usage())
                    })?));
            }
            "--page-size" => {
                page_size = args
                    .next()
                    .ok_or_else(|| format!("missing value for --page-size\n{}", usage()))?
                    .parse::<usize>()
                    .map_err(|error| format!("invalid --page-size: {error}"))?
                    .clamp(1, 500);
            }
            "--max-pages" => {
                max_pages = args
                    .next()
                    .ok_or_else(|| format!("missing value for --max-pages\n{}", usage()))?
                    .parse::<usize>()
                    .map_err(|error| format!("invalid --max-pages: {error}"))?;
            }
            "--apply" => apply = true,
            "--help" | "-h" => return Err(usage().to_string()),
            other => return Err(format!("unknown argument: {other}\n{}", usage())),
        }
    }
    if !apply {
        return Err(format!(
            "repair is mutation-gated; pass --apply\n{}",
            usage()
        ));
    }
    let base_dir = base_dir.map_or_else(default_base_dir, Ok)?;
    let paths = AppPaths::new(base_dir.clone());
    let backup_path = backup_path
        .as_deref()
        .ok_or_else(|| "--apply requires --backup <verified-backup.sqlite>".to_string())?;
    let mutation_guard = verify_mutation_guard(&base_dir)?;
    let backup = verify_backup(&paths.db_dir().join("app.sqlite"), backup_path)?;
    if verify_mutation_guard(&base_dir)? != mutation_guard {
        return Err("VoxVulgi app/bridge state changed during backup verification".to_string());
    }

    let mut pages_run = 0_usize;
    while pages_run < max_pages {
        verify_mutation_guard(&base_dir)?;
        let page = repair_provider_titles_page(&paths, page_size)
            .map_err(|error| format!("repair page {} failed: {error}", pages_run + 1))?;
        pages_run += 1;
        if pages_run == 1 || pages_run % 25 == 0 || page.completed {
            eprintln!(
                "provider-title-repair page={} scanned={} repaired={} conflicts={} unavailable={} state={}",
                pages_run,
                page.cumulative_scanned,
                page.cumulative_repaired,
                page.cumulative_conflicts,
                page.cumulative_unavailable,
                page.state,
            );
        }
        if page.completed {
            break;
        }
    }
    let status = provider_title_repair_status(&paths)
        .map_err(|error| format!("load final repair status: {error}"))?;
    Ok(RepairReceipt {
        base_dir: base_dir.to_string_lossy().to_string(),
        backup,
        mutation_guard,
        pages_run,
        page_size,
        status,
    })
}

fn main() {
    match run() {
        Ok(receipt) => println!(
            "{}",
            serde_json::to_string_pretty(&receipt).expect("serialize receipt")
        ),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod backup_tests {
    use super::*;

    fn create_test_database(path: &Path, title: &str) {
        let conn = Connection::open(path).expect("open test database");
        conn.execute_batch(
            "CREATE TABLE job(id TEXT PRIMARY KEY, type TEXT NOT NULL, title TEXT);\n\
             CREATE TABLE preference(key TEXT PRIMARY KEY, value TEXT) WITHOUT ROWID;",
        )
        .expect("create test schema");
        conn.execute(
            "INSERT INTO job(id,type,title) VALUES('job-1','download_direct_url',?1)",
            [title],
        )
        .expect("insert test job");
        conn.execute(
            "INSERT INTO preference(key,value) VALUES('theme','dark')",
            [],
        )
        .expect("insert preference");
    }

    #[test]
    fn backup_verification_rejects_same_counts_with_different_content() {
        let temp = tempfile::tempdir().expect("tempdir");
        let live = temp.path().join("live.sqlite");
        let backup = temp.path().join("backup.sqlite");
        create_test_database(&live, "canonical title");
        create_test_database(&backup, "wrong stale title");

        let error = verify_backup(&live, &backup).expect_err("different content must fail");
        assert!(error.contains("logical preimage mismatch"), "{error}");
    }

    #[test]
    fn backup_verification_accepts_logically_identical_databases() {
        let temp = tempfile::tempdir().expect("tempdir");
        let live = temp.path().join("live.sqlite");
        let backup = temp.path().join("backup.sqlite");
        create_test_database(&live, "같은 제목");
        create_test_database(&backup, "같은 제목");

        let verification = verify_backup(&live, &backup).expect("matching backup");
        assert_eq!(verification.table_count, 2);
        assert_eq!(verification.logical_sha256.len(), 64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn mutation_guard_rejects_a_live_bridge_pid() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("db")).expect("db dir");
        std::fs::write(
            dir.path().join("agent_bridge.json"),
            serde_json::json!({"pid": std::process::id(), "port": 1}).to_string(),
        )
        .expect("bridge metadata");
        let error = verify_mutation_guard(dir.path()).expect_err("live bridge must block");
        assert!(error.contains("bridge pid"));
    }

    #[test]
    fn mutation_guard_rejects_a_nonempty_wal() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("db")).expect("db dir");
        std::fs::write(dir.path().join("db").join("app.sqlite-wal"), b"active")
            .expect("wal fixture");
        let error = verify_mutation_guard(dir.path()).expect_err("nonempty WAL must block");
        assert!(error.contains("WAL is non-empty"));
    }
}
