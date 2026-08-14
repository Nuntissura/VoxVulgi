use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
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
    file_bytes: u64,
}

#[derive(Debug, Serialize)]
struct RepairReceipt {
    base_dir: String,
    backup: BackupVerification,
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
    Ok(BackupVerification {
        path: backup_path.to_string_lossy().to_string(),
        quick_check,
        job_count: backup_jobs,
        download_job_count: backup_download_jobs,
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
    let backup = verify_backup(&paths.db_dir().join("app.sqlite"), backup_path)?;

    let mut pages_run = 0_usize;
    while pages_run < max_pages {
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
