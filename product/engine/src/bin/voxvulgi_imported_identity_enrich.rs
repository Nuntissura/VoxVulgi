use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use std::env;
use std::path::{Path, PathBuf};
use voxvulgi_engine::paths::AppPaths;
use voxvulgi_engine::subscriptions::{
    enrich_imported_youtube_identity_4kvdp, YoutubeImportedIdentityEnrichmentSummary,
};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct CanonicalCounts {
    library_items: usize,
    youtube_identities: usize,
    linked_youtube_identities: usize,
    memberships: usize,
    evidence_rows: usize,
    checkpoints: usize,
}

#[derive(Serialize)]
struct BackupVerification {
    path: String,
    quick_check: String,
    counts: CanonicalCounts,
}

#[derive(Serialize)]
struct Receipt {
    base_dir: String,
    source_sqlite: String,
    apply: bool,
    backup: Option<BackupVerification>,
    before: CanonicalCounts,
    summary: YoutubeImportedIdentityEnrichmentSummary,
    after: CanonicalCounts,
}

fn usage() -> &'static str {
    "Usage: voxvulgi_imported_identity_enrich [--base-dir <app-data-dir>] --sqlite-path <4kvdp.sqlite> [--max-items <n>] [--apply --backup <verified-backup.sqlite>]"
}

fn default_base_dir() -> Result<PathBuf, String> {
    let appdata = env::var_os("APPDATA")
        .ok_or_else(|| "APPDATA is unavailable; pass --base-dir explicitly".to_string())?;
    Ok(PathBuf::from(appdata).join("com.voxvulgi.voxvulgi"))
}

fn read_counts(conn: &Connection) -> Result<CanonicalCounts, String> {
    let count = |sql: &str| -> Result<usize, String> {
        conn.query_row(sql, [], |row| row.get::<_, i64>(0))
            .map(|value| value.max(0) as usize)
            .map_err(|error| format!("count query failed: {error}; sql={sql}"))
    };
    Ok(CanonicalCounts {
        library_items: count("SELECT COUNT(*) FROM library_item")?,
        youtube_identities: count(
            "SELECT COUNT(*) FROM media_source_identity WHERE service='youtube'",
        )?,
        linked_youtube_identities: count(
            "SELECT COUNT(*) FROM media_source_identity WHERE service='youtube' AND library_item_id IS NOT NULL",
        )?,
        memberships: count("SELECT COUNT(*) FROM media_source_membership")?,
        evidence_rows: count("SELECT COUNT(*) FROM media_import_evidence")?,
        checkpoints: count("SELECT COUNT(*) FROM media_import_enrichment_checkpoint")?,
    })
}

fn open_readonly(path: &Path) -> Result<Connection, String> {
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_FULL_MUTEX,
    )
    .map_err(|error| format!("open read-only database {}: {error}", path.display()))
}

fn verify_backup(
    backup_path: &Path,
    expected_counts: &CanonicalCounts,
) -> Result<BackupVerification, String> {
    if !backup_path.is_file() {
        return Err(format!(
            "backup path is not a file: {}",
            backup_path.display()
        ));
    }
    let conn = open_readonly(backup_path)?;
    let quick_check: String = conn
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .map_err(|error| format!("backup quick_check: {error}"))?;
    if quick_check != "ok" {
        return Err(format!("backup quick_check failed: {quick_check}"));
    }
    let counts = read_counts(&conn)?;
    if &counts != expected_counts {
        return Err(format!(
            "backup preimage counts differ from the live database: backup={counts:?}, live={expected_counts:?}"
        ));
    }
    Ok(BackupVerification {
        path: backup_path.to_string_lossy().to_string(),
        quick_check,
        counts,
    })
}

fn live_counts(base_dir: &Path) -> Result<CanonicalCounts, String> {
    let db_path = base_dir.join("db").join("app.sqlite");
    let conn = open_readonly(&db_path)?;
    read_counts(&conn)
}

fn run() -> Result<Receipt, String> {
    let mut args = env::args().skip(1);
    let mut base_dir = None;
    let mut sqlite_path = None;
    let mut max_items = None;
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
            "--sqlite-path" => {
                sqlite_path =
                    Some(PathBuf::from(args.next().ok_or_else(|| {
                        format!("missing value for --sqlite-path\n{}", usage())
                    })?));
            }
            "--max-items" => {
                let raw = args
                    .next()
                    .ok_or_else(|| format!("missing value for --max-items\n{}", usage()))?;
                max_items = Some(
                    raw.parse::<usize>()
                        .map_err(|_| format!("invalid --max-items value: {raw}"))?
                        .max(1),
                );
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
    let sqlite_path =
        sqlite_path.ok_or_else(|| format!("--sqlite-path is required\n{}", usage()))?;
    if !sqlite_path.is_file() {
        return Err(format!(
            "4K Video Downloader database is not a file: {}",
            sqlite_path.display()
        ));
    }
    let source_before = std::fs::metadata(&sqlite_path)
        .map_err(|error| format!("source metadata before: {error}"))?;
    let before = live_counts(&base_dir)?;
    let backup = if apply {
        let backup_path = backup_path
            .as_deref()
            .ok_or_else(|| "--apply requires --backup <verified-backup.sqlite>".to_string())?;
        Some(verify_backup(backup_path, &before)?)
    } else {
        None
    };

    let paths = AppPaths::new(base_dir.clone());
    let summary =
        enrich_imported_youtube_identity_4kvdp(&paths, Some(&sqlite_path), !apply, max_items)
            .map_err(|error| format!("enrichment failed: {error}"))?;
    let after = live_counts(&base_dir)?;
    let source_after = std::fs::metadata(&sqlite_path)
        .map_err(|error| format!("source metadata after: {error}"))?;
    if source_before.len() != source_after.len()
        || source_before.modified().ok() != source_after.modified().ok()
    {
        return Err("read-only 4K Video Downloader source changed during enrichment".to_string());
    }
    if !apply && before != after {
        return Err(format!(
            "dry-run changed canonical counts: before={before:?}, after={after:?}"
        ));
    }

    Ok(Receipt {
        base_dir: base_dir.to_string_lossy().to_string(),
        source_sqlite: sqlite_path.to_string_lossy().to_string(),
        apply,
        backup,
        before,
        summary,
        after,
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
