use std::path::PathBuf;

use serde::Serialize;
use voxvulgi_engine::{jobs, paths::AppPaths, EngineError, Result};

const CONFIRM_FLAG: &str = "--confirm-live-enqueue";

#[derive(Serialize)]
struct ProbeReport {
    wp: &'static str,
    enqueued_count: usize,
    jobs: Vec<jobs::JobRow>,
}

fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let base_dir = args.next().map(PathBuf::from).ok_or_else(usage_error)?;
    let url = args
        .next()
        .and_then(|value| value.into_string().ok())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(usage_error)?;
    let confirmation = args
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or_else(usage_error)?;
    if confirmation != CONFIRM_FLAG || args.next().is_some() {
        return Err(usage_error());
    }

    let paths = AppPaths::new(base_dir);
    let rows =
        jobs::enqueue_download_direct_url_batch(&paths, vec![url], None, None, None, None, None)?;
    let report = ProbeReport {
        wp: "WP-0269",
        enqueued_count: rows.len(),
        jobs: rows,
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&report).map_err(|error| {
            EngineError::InstallFailed(format!("failed to serialize probe report: {error}"))
        })?
    );
    Ok(())
}

fn usage_error() -> EngineError {
    EngineError::InstallFailed(format!(
        "usage: wp0269_live_foreground_probe <app_data_dir> <url> {CONFIRM_FLAG}"
    ))
}
