use std::path::PathBuf;

use rusqlite::OptionalExtension;
use serde::Serialize;
use voxvulgi_engine::{db, library, paths::AppPaths, EngineError, Result};

#[derive(Serialize)]
struct ProbeReport {
    schema_version: i64,
    backfill_batches: usize,
    canonical_youtube_single_total: usize,
    unclassified_youtube_total: usize,
    subscription_rows_in_single_history: i64,
    mapped_subscription_rows_excluded_from_single_history: i64,
    sample_single_item_ids: Vec<String>,
}

fn main() -> Result<()> {
    let base_dir = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or_else(|| {
            EngineError::InstallFailed(
                "usage: wp0268_lineage_probe <isolated_app_data_copy>".to_string(),
            )
        })?;
    let paths = AppPaths::new(base_dir);
    let conn = db::open(&paths)?;
    db::migrate(&conn)?;
    drop(conn);

    let mut backfill_batches = 0_usize;
    loop {
        let state = library::backfill_download_lineage_batch(&paths, 500)?;
        backfill_batches = backfill_batches.saturating_add(1);
        if state.complete {
            break;
        }
        if backfill_batches > 10_000 {
            return Err(EngineError::InstallFailed(
                "lineage backfill did not converge within 10000 batches".to_string(),
            ));
        }
    }

    let history = library::list_youtube_single_history(&paths, 10, 0, None, Some("desc"))?;
    let unclassified_youtube_total = library::count_youtube_single_unclassified(&paths)?;
    let conn = db::open_readonly(&paths)?;
    let schema_version = conn
        .query_row(
            "SELECT value FROM meta WHERE key='schema_version'",
            [],
            |row| row.get::<_, String>(0),
        )?
        .parse::<i64>()
        .map_err(|error| EngineError::InstallFailed(format!("invalid schema_version: {error}")))?;
    let subscription_rows_in_single_history = conn.query_row(
        r#"
SELECT COUNT(*)
FROM library_download_lineage
WHERE service='youtube'
  AND origin_kind='single'
  AND work_track='youtube_single'
  AND source_subscription_id IS NOT NULL
"#,
        [],
        |row| row.get(0),
    )?;
    let mapped_subscription_rows_excluded_from_single_history = conn.query_row(
        r#"
SELECT COUNT(*)
FROM library_download_lineage lineage
JOIN library_item item ON item.id=lineage.item_id
WHERE lineage.service='youtube'
  AND lineage.origin_kind='subscription'
  AND lineage.work_track='youtube_recurring'
  AND lineage.source_subscription_id IS NOT NULL
  AND lower(item.media_path) NOT LIKE '%subscriptions%'
"#,
        [],
        |row| row.get(0),
    )?;
    let cursor: Option<String> = conn
        .query_row(
            "SELECT value FROM meta WHERE key='library_download_lineage_backfill_v1_last_job_rowid'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if cursor.is_none() {
        return Err(EngineError::InstallFailed(
            "lineage backfill cursor was not persisted".to_string(),
        ));
    }

    let report = ProbeReport {
        schema_version,
        backfill_batches,
        canonical_youtube_single_total: history.canonical_total,
        unclassified_youtube_total,
        subscription_rows_in_single_history,
        mapped_subscription_rows_excluded_from_single_history,
        sample_single_item_ids: history.items.into_iter().map(|item| item.id).collect(),
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&report).map_err(|error| {
            EngineError::InstallFailed(format!("failed to serialize probe report: {error}"))
        })?
    );
    Ok(())
}
