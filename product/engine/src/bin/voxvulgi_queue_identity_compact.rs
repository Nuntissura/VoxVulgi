use serde::Serialize;
use std::env;
use std::path::PathBuf;
use voxvulgi_engine::jobs::{
    get_queue_control, set_queue_paused, youtube_queue_identity_reconcile,
    YoutubeQueueIdentityBackupReceipt, YoutubeQueueIdentityReconcileSummary,
};
use voxvulgi_engine::paths::AppPaths;

#[derive(Serialize)]
struct Receipt {
    base_dir: String,
    queue_was_paused: bool,
    queue_is_paused: bool,
    backup: Option<YoutubeQueueIdentityBackupReceipt>,
    preview: YoutubeQueueIdentityReconcileSummary,
    applied: Option<YoutubeQueueIdentityReconcileSummary>,
}

fn usage() -> &'static str {
    "Usage: voxvulgi_queue_identity_compact [--base-dir <app-data-dir>] [--apply]"
}

fn default_base_dir() -> Result<PathBuf, String> {
    let appdata = env::var_os("APPDATA")
        .ok_or_else(|| "APPDATA is unavailable; pass --base-dir explicitly".to_string())?;
    Ok(PathBuf::from(appdata).join("com.voxvulgi.voxvulgi"))
}

fn run() -> Result<Receipt, String> {
    let mut args = env::args().skip(1);
    let mut base_dir = None;
    let mut apply = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--base-dir" => {
                base_dir =
                    Some(PathBuf::from(args.next().ok_or_else(|| {
                        format!("missing value for --base-dir\n{}", usage())
                    })?));
            }
            "--apply" => apply = true,
            "--help" | "-h" => return Err(usage().to_string()),
            other => return Err(format!("unknown argument: {other}\n{}", usage())),
        }
    }
    let base_dir = base_dir.map_or_else(default_base_dir, Ok)?;
    let paths = AppPaths::new(base_dir.clone());
    let queue_was_paused = get_queue_control(&paths)
        .map_err(|error| format!("read queue pause state: {error}"))?
        .paused;
    if apply && !queue_was_paused {
        set_queue_paused(&paths, true)
            .map_err(|error| format!("pause queue before compaction: {error}"))?;
    }
    let queue_is_paused = get_queue_control(&paths)
        .map_err(|error| format!("verify queue pause state: {error}"))?
        .paused;
    let preview = youtube_queue_identity_reconcile(&paths, true, None, Some(500))
        .map_err(|error| format!("preview failed: {error}"))?;
    let applied = if apply {
        Some(
            youtube_queue_identity_reconcile(&paths, false, None, Some(500))
                .map_err(|error| format!("apply failed: {error}"))?,
        )
    } else {
        None
    };
    let backup = applied.as_ref().and_then(|summary| summary.backup.clone());
    Ok(Receipt {
        base_dir: base_dir.to_string_lossy().to_string(),
        queue_was_paused,
        queue_is_paused,
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
