use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use std::env;
use std::path::{Path, PathBuf};
use voxvulgi_engine::jobs::{
    youtube_queue_identity_reconcile, YoutubeQueueIdentityReconcileSummary,
};
use voxvulgi_engine::paths::AppPaths;

#[derive(Serialize)]
struct BackupVerification {
    path: String,
    quick_check: String,
    schema_version: u32,
    queue_paused: bool,
    queued_direct_jobs: usize,
    running_direct_jobs: usize,
}

#[derive(Serialize)]
struct Receipt {
    base_dir: String,
    backup: Option<BackupVerification>,
    preview: YoutubeQueueIdentityReconcileSummary,
    applied: Option<YoutubeQueueIdentityReconcileSummary>,
}

fn usage() -> &'static str {
    "Usage: voxvulgi_queue_identity_compact [--base-dir <app-data-dir>] [--apply --backup <verified-backup.sqlite>]"
}

fn default_base_dir() -> Result<PathBuf, String> {
    let appdata = env::var_os("APPDATA")
        .ok_or_else(|| "APPDATA is unavailable; pass --base-dir explicitly".to_string())?;
    Ok(PathBuf::from(appdata).join("com.voxvulgi.voxvulgi"))
}

fn verify_backup(
    backup_path: &Path,
    expected_queued_direct_jobs: usize,
) -> Result<BackupVerification, String> {
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
    let schema_version: u32 = conn
        .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
        .map_err(|error| format!("backup schema version: {error}"))?
        .max(0) as u32;
    let queue_paused = conn
        .query_row(
            "SELECT value='1' FROM meta WHERE key='jobs_queue_paused'",
            [],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|error| format!("backup queue pause state: {error}"))?;
    let queued_direct_jobs = conn
        .query_row(
            "SELECT COUNT(*) FROM job WHERE status='queued' AND type='download_direct_url'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| format!("backup queued direct count: {error}"))?
        .max(0) as usize;
    let running_direct_jobs = conn
        .query_row(
            "SELECT COUNT(*) FROM job WHERE status='running' AND type='download_direct_url'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| format!("backup running direct count: {error}"))?
        .max(0) as usize;
    if !queue_paused
        || running_direct_jobs != 0
        || queued_direct_jobs != expected_queued_direct_jobs
    {
        return Err(format!(
            "backup preimage mismatch: paused={queue_paused}, queued_direct={queued_direct_jobs}, running_direct={running_direct_jobs}, expected_queued_direct={expected_queued_direct_jobs}"
        ));
    }
    Ok(BackupVerification {
        path: backup_path.to_string_lossy().to_string(),
        quick_check,
        schema_version,
        queue_paused,
        queued_direct_jobs,
        running_direct_jobs,
    })
}

fn run() -> Result<Receipt, String> {
    let mut args = env::args().skip(1);
    let mut base_dir = None;
    let mut apply = false;
    let mut backup_path = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--base-dir" => {
                base_dir =
                    Some(PathBuf::from(args.next().ok_or_else(|| {
                        format!("missing value for --base-dir\n{}", usage())
                    })?));
            }
            "--apply" => apply = true,
            "--backup" => {
                backup_path =
                    Some(PathBuf::from(args.next().ok_or_else(|| {
                        format!("missing value for --backup\n{}", usage())
                    })?));
            }
            "--help" | "-h" => return Err(usage().to_string()),
            other => return Err(format!("unknown argument: {other}\n{}", usage())),
        }
    }
    let base_dir = base_dir.map_or_else(default_base_dir, Ok)?;
    let paths = AppPaths::new(base_dir.clone());
    let preview = youtube_queue_identity_reconcile(&paths, true, None, Some(500))
        .map_err(|error| format!("preview failed: {error}"))?;
    let backup = if apply {
        let backup_path = backup_path
            .as_deref()
            .ok_or_else(|| "--apply requires --backup <verified-backup.sqlite>".to_string())?;
        Some(verify_backup(backup_path, preview.scanned_queued_jobs)?)
    } else {
        None
    };
    let applied = if apply {
        Some(
            youtube_queue_identity_reconcile(&paths, false, None, Some(500))
                .map_err(|error| format!("apply failed: {error}"))?,
        )
    } else {
        None
    };
    Ok(Receipt {
        base_dir: base_dir.to_string_lossy().to_string(),
        backup,
        preview,
        applied,
    })
}

fn main() {
    match run() {
        Ok(receipt) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&receipt).expect("serialize receipt")
            );
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
