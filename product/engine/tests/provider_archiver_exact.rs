use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};
use voxvulgi_engine::db;
use voxvulgi_engine::instagram_subscriptions::{self, InstagramSubscriptionUpsert};
use voxvulgi_engine::jobs::{self, JobRow, JobStatus};
use voxvulgi_engine::library;
use voxvulgi_engine::paths::AppPaths;
use voxvulgi_engine::provider_metadata::DisplayTitleProvenance;
use voxvulgi_engine::tiktok_subscriptions::{self, TiktokSubscriptionUpsert};
use voxvulgi_engine::tools;

fn required_env(name: &str) -> String {
    std::env::var(name)
        .unwrap_or_else(|_| panic!("{name} must be set for the exact provider probe"))
}

fn prepare_python_venv(paths: &AppPaths) {
    let python = paths.python_venv_dir().join("Scripts").join("python.exe");
    if !python.exists() {
        std::fs::create_dir_all(paths.python_venv_dir().parent().unwrap()).unwrap();
        let status = Command::new("python")
            .args(["-m", "venv"])
            .arg(paths.python_venv_dir())
            .status()
            .expect("system Python must be launchable to create the isolated proof venv");
        assert!(status.success(), "isolated proof venv creation failed");
    }
    let import_status = Command::new(&python)
        .args(["-c", "import requests"])
        .status()
        .expect("isolated proof Python must launch");
    if !import_status.success() {
        let install_status = Command::new(&python)
            .args(["-m", "pip", "install", "requests==2.34.2"])
            .status()
            .expect("pinned requests install must launch");
        assert!(install_status.success(), "pinned requests install failed");
    }
}

fn terminal(status: &JobStatus) -> bool {
    matches!(
        status,
        JobStatus::Succeeded | JobStatus::Failed | JobStatus::Canceled
    )
}

fn wait_for_jobs(paths: &AppPaths, ids: &[String], timeout: Duration) -> Vec<JobRow> {
    let started = Instant::now();
    loop {
        let jobs = jobs::list_jobs(paths, 2_000, 0).unwrap();
        let selected = ids
            .iter()
            .filter_map(|id| jobs.iter().find(|job| &job.id == id).cloned())
            .collect::<Vec<_>>();
        if selected.len() == ids.len() && selected.iter().all(|job| terminal(&job.status)) {
            return selected;
        }
        assert!(
            started.elapsed() < timeout,
            "timed out waiting for jobs {ids:?}"
        );
        thread::sleep(Duration::from_millis(400));
    }
}

fn wait_for_quiescence(paths: &AppPaths, timeout: Duration) -> Vec<JobRow> {
    let started = Instant::now();
    loop {
        let jobs = jobs::list_jobs(paths, 2_000, 0).unwrap();
        if jobs.iter().all(|job| terminal(&job.status)) {
            return jobs;
        }
        assert!(
            started.elapsed() < timeout,
            "timed out waiting for provider queue quiescence"
        );
        thread::sleep(Duration::from_millis(400));
    }
}

fn assert_succeeded(rows: &[JobRow], context: &str) {
    let failed = rows
        .iter()
        .filter(|job| job.status != JobStatus::Succeeded)
        .map(|job| json!({"id": job.id, "type": job.job_type, "status": job.status, "error": job.error}))
        .collect::<Vec<_>>();
    assert!(failed.is_empty(), "{context} failed: {failed:#?}");
}

fn install_exact_tools(paths: &AppPaths, instagram: bool) {
    paths.ensure_dirs().unwrap();
    tools::install_ytdlp_tools(paths).expect("pinned yt-dlp install must succeed");
    if instagram {
        prepare_python_venv(paths);
        tools::install_instagram_profile_provider(paths)
            .expect("pinned Instaloader executable install must succeed");
        tools::install_instagram_profile_enumerator(paths)
            .expect("pinned Instaloader enumerator install must succeed");
    }
}

fn discover_instagram_post(paths: &AppPaths, profile: &str) -> String {
    let output = Command::new(paths.python_venv_dir().join("Scripts").join("python.exe"))
        .arg(paths.instagram_profile_enumerator_script())
        .args(["--profile", profile, "--max-items", "2", "--include-posts"])
        .output()
        .expect("Instagram exact fixture enumeration must launch");
    assert!(
        output.status.success(),
        "Instagram exact fixture enumeration failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    value["items"][0]["source_url"]
        .as_str()
        .expect("Instagram profile must expose an exact post fixture")
        .to_string()
}

fn discover_tiktok_video(paths: &AppPaths, profile_url: &str) -> String {
    let ytdlp = Path::new(&tools::ytdlp_tools_status(paths).bundled_path).to_path_buf();
    let output = Command::new(ytdlp)
        .args([
            "--ignore-config",
            "--flat-playlist",
            "--playlist-end",
            "2",
            "--dump-single-json",
            profile_url,
        ])
        .output()
        .expect("TikTok exact fixture enumeration must launch");
    assert!(
        output.status.success(),
        "TikTok exact fixture enumeration failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    let entry = &value["entries"][0];
    entry["webpage_url"]
        .as_str()
        .or_else(|| entry["url"].as_str())
        .expect("TikTok profile must expose an exact video fixture")
        .to_string()
}

fn app_paths(env_name: &str) -> AppPaths {
    let base = PathBuf::from(required_env(env_name));
    AppPaths::new(base)
}

fn transfer_job_count(paths: &AppPaths, service: &str) -> i64 {
    let conn = db::open_readonly(paths).expect("open proof database");
    conn.query_row(
        "SELECT COUNT(*) FROM job WHERE type='download_direct_url' AND track IN (?1,?2)",
        [format!("{service}_single"), format!("{service}_recurring")],
        |row| row.get(0),
    )
    .expect("count provider transfer jobs")
}

fn assert_provider_contract(paths: &AppPaths, service: &str) -> Value {
    let conn = db::open_readonly(paths).expect("open proof database");
    let identity_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM media_source_identity WHERE service=?1",
            [service],
            |row| row.get(0),
        )
        .expect("identity count");
    let linked_identity_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM media_source_identity WHERE service=?1 AND library_item_id IS NOT NULL",
            [service],
            |row| row.get(0),
        )
        .expect("linked identity count");
    let linked_item_ids = conn
        .prepare(
            "SELECT library_item_id FROM media_source_identity WHERE service=?1 AND library_item_id IS NOT NULL",
        )
        .expect("prepare linked identities")
        .query_map([service], |row| row.get::<_, String>(0))
        .expect("query linked identities")
        .collect::<rusqlite::Result<HashSet<_>>>()
        .expect("collect linked identities");
    let metadata_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM media_provider_metadata WHERE service=?1",
            [service],
            |row| row.get(0),
        )
        .expect("metadata count");
    let orphan_metadata_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM media_provider_metadata metadata LEFT JOIN media_source_identity identity ON identity.service=metadata.service AND identity.media_id=metadata.media_id WHERE metadata.service=?1 AND identity.media_id IS NULL",
            [service],
            |row| row.get(0),
        )
        .expect("orphan metadata count");
    let membership_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM media_source_membership WHERE service=?1",
            [service],
            |row| row.get(0),
        )
        .expect("membership count");
    let lineage_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM library_download_lineage WHERE service=?1",
            [service],
            |row| row.get(0),
        )
        .expect("lineage count");
    let noncanonical_tiktok_metadata: i64 = if service == "tiktok" {
        conn.query_row(
            "SELECT COUNT(*) FROM media_provider_metadata WHERE service='tiktok' AND media_id NOT LIKE 'video:%'",
            [],
            |row| row.get(0),
        )
        .expect("TikTok metadata namespace")
    } else {
        0
    };
    drop(conn);

    let provider_items = library::list_items(paths, 2_000, 0)
        .expect("hydrated library")
        .into_iter()
        .filter(|item| linked_item_ids.contains(&item.id))
        .collect::<Vec<_>>();
    assert!(
        identity_count > 0,
        "{service} must persist canonical identities"
    );
    assert_eq!(
        linked_identity_count, identity_count,
        "every {service} identity must link to exactly one materialized library item"
    );
    assert!(
        metadata_count > 0,
        "{service} must persist canonical metadata"
    );
    assert_eq!(
        orphan_metadata_count, 0,
        "metadata keys must match canonical identities"
    );
    assert!(membership_count > 0, "profile membership must be durable");
    assert!(
        lineage_count > 0,
        "Jobs and Media Library require durable lineage"
    );
    assert_eq!(noncanonical_tiktok_metadata, 0);
    assert_eq!(provider_items.len() as i64, linked_identity_count);
    for item in &provider_items {
        assert!(!item.title.trim().is_empty());
        assert!(
            !item.title.starts_with("download_"),
            "placeholder title leaked: {}",
            item.title
        );
        assert!(
            matches!(
                item.title_provenance,
                Some(DisplayTitleProvenance::CanonicalRemote)
                    | Some(DisplayTitleProvenance::OperatorOverride)
            ),
            "provider title provenance was not canonical for {}: {:?}",
            item.id,
            item.title_provenance
        );
        assert!(
            Path::new(&item.media_path).is_file(),
            "missing final artifact: {}",
            item.media_path
        );
        if service == "tiktok" {
            assert_eq!(
                Path::new(&item.media_path)
                    .extension()
                    .and_then(|value| value.to_str()),
                Some("mkv")
            );
        }
    }
    json!({
        "canonical_identities": identity_count,
        "linked_identities": linked_identity_count,
        "metadata_rows": metadata_count,
        "orphan_metadata_rows": orphan_metadata_count,
        "memberships": membership_count,
        "lineage_rows": lineage_count,
        "hydrated_library_items": provider_items.len(),
        "transfer_jobs": transfer_job_count(paths, service),
    })
}

#[test]
#[ignore = "networked exact-provider acceptance probe; requires VOXVULGI_IG_PROBE_BASE and VOXVULGI_IG_PROFILE"]
fn exact_instagram_single_profile_second_refresh_and_restart() {
    let paths = app_paths("VOXVULGI_IG_PROBE_BASE");
    let profile = required_env("VOXVULGI_IG_PROFILE");
    let browser_cookie_source = std::env::var("VOXVULGI_IG_BROWSER").ok();
    install_exact_tools(&paths, true);
    let post_url = discover_instagram_post(&paths, &profile);
    let output_dir = paths.base_dir.join("proof_downloads");

    let runner = jobs::start_runner(paths.clone()).unwrap();
    let single = jobs::enqueue_download_instagram_batch(
        &paths,
        vec![post_url.clone()],
        None,
        Some(output_dir.to_string_lossy().to_string()),
        Some(browser_cookie_source.is_some()),
        browser_cookie_source.clone(),
    )
    .unwrap();
    let single_ids = single.iter().map(|job| job.id.clone()).collect::<Vec<_>>();
    assert_succeeded(
        &wait_for_jobs(&paths, &single_ids, Duration::from_secs(180)),
        "Instagram single lane",
    );

    let subscription = instagram_subscriptions::upsert_instagram_subscription(
        &paths,
        InstagramSubscriptionUpsert {
            id: None,
            title: profile.clone(),
            source_url: format!("https://www.instagram.com/{profile}/"),
            folder_map: Some(profile.clone()),
            output_dir_override: Some(output_dir.to_string_lossy().to_string()),
            use_browser_cookies: browser_cookie_source.is_some(),
            browser_cookie_source: browser_cookie_source.clone(),
            auth_session_input: None,
            clear_auth_session: false,
            active: true,
            refresh_interval_minutes: Some(180),
            max_items_per_refresh: Some(2),
            include_posts: true,
            include_reels: true,
            include_stories: true,
        },
    )
    .unwrap();

    let first_refresh =
        instagram_subscriptions::queue_instagram_subscription(&paths, &subscription.id).unwrap();
    let first_ids = first_refresh
        .iter()
        .map(|job| job.id.clone())
        .collect::<Vec<_>>();
    assert_succeeded(
        &wait_for_jobs(&paths, &first_ids, Duration::from_secs(180)),
        "Instagram first profile refresh",
    );
    let first_quiescent = wait_for_quiescence(&paths, Duration::from_secs(240));
    assert_succeeded(&first_quiescent, "Instagram first refresh child transfers");
    let first_item_count = library::list_items(&paths, 2_000, 0).unwrap().len();
    let first_transfer_count = transfer_job_count(&paths, "instagram");

    let second_refresh =
        instagram_subscriptions::queue_instagram_subscription(&paths, &subscription.id).unwrap();
    let second_ids = second_refresh
        .iter()
        .map(|job| job.id.clone())
        .collect::<Vec<_>>();
    assert_succeeded(
        &wait_for_jobs(&paths, &second_ids, Duration::from_secs(180)),
        "Instagram second profile refresh",
    );
    assert_succeeded(
        &wait_for_quiescence(&paths, Duration::from_secs(180)),
        "Instagram second refresh queue",
    );
    let second_item_count = library::list_items(&paths, 2_000, 0).unwrap().len();
    assert_eq!(
        first_item_count, second_item_count,
        "second refresh must not duplicate archived Instagram media"
    );
    assert_eq!(
        first_transfer_count,
        transfer_job_count(&paths, "instagram"),
        "second refresh must not enqueue a canonical Instagram transfer again"
    );

    runner.stop();
    thread::sleep(Duration::from_secs(2));
    let restarted_runner = jobs::start_runner(paths.clone()).unwrap();
    let restart_refresh =
        instagram_subscriptions::queue_instagram_subscription(&paths, &subscription.id).unwrap();
    let restart_ids = restart_refresh
        .iter()
        .map(|job| job.id.clone())
        .collect::<Vec<_>>();
    assert_succeeded(
        &wait_for_jobs(&paths, &restart_ids, Duration::from_secs(180)),
        "Instagram post-restart refresh",
    );
    assert_succeeded(
        &wait_for_quiescence(&paths, Duration::from_secs(180)),
        "Instagram post-restart queue",
    );
    restarted_runner.stop();
    assert_eq!(
        first_transfer_count,
        transfer_job_count(&paths, "instagram"),
        "post-restart refresh must not enqueue a canonical Instagram transfer again"
    );

    let persisted = instagram_subscriptions::list_instagram_subscriptions(&paths).unwrap();
    let row = persisted
        .iter()
        .find(|row| row.id == subscription.id)
        .unwrap();
    assert!(
        row.last_error.is_none()
            || row.hold_reason.is_some()
            || row.next_allowed_refresh_at_ms.is_some(),
        "a partial Instagram capability failure must be held or backed off"
    );
    let contract = assert_provider_contract(&paths, "instagram");
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "provider": "instagram",
            "profile": profile,
            "single_url": post_url,
            "single_job_ids": single_ids,
            "first_refresh_job_ids": first_ids,
            "second_refresh_job_ids": second_ids,
            "restart_refresh_job_ids": restart_ids,
            "library_items_after_first": first_item_count,
            "library_items_after_second": second_item_count,
            "provider_contract": contract,
            "subscription": row,
        }))
        .unwrap()
    );
}

#[test]
#[ignore = "networked exact-provider acceptance probe; requires VOXVULGI_TIKTOK_PROBE_BASE and VOXVULGI_TIKTOK_PROFILE"]
fn exact_tiktok_single_profile_second_refresh_and_restart() {
    let paths = app_paths("VOXVULGI_TIKTOK_PROBE_BASE");
    let profile_url = required_env("VOXVULGI_TIKTOK_PROFILE");
    install_exact_tools(&paths, false);
    let video_url = discover_tiktok_video(&paths, &profile_url);
    let output_dir = paths.base_dir.join("proof_downloads");

    let runner = jobs::start_runner(paths.clone()).unwrap();
    let single = jobs::enqueue_download_tiktok_batch(
        &paths,
        vec![video_url.clone()],
        None,
        Some(output_dir.to_string_lossy().to_string()),
        Some(false),
        None,
    )
    .unwrap();
    let single_ids = single.iter().map(|job| job.id.clone()).collect::<Vec<_>>();
    assert_succeeded(
        &wait_for_jobs(&paths, &single_ids, Duration::from_secs(300)),
        "TikTok single lane",
    );

    let subscription = tiktok_subscriptions::upsert_tiktok_subscription(
        &paths,
        TiktokSubscriptionUpsert {
            id: None,
            title: "TikTok exact profile".to_string(),
            source_url: profile_url.clone(),
            folder_map: Some("tiktok_exact_profile".to_string()),
            output_dir_override: Some(output_dir.to_string_lossy().to_string()),
            use_browser_cookies: false,
            browser_cookie_source: None,
            active: true,
            refresh_interval_minutes: Some(180),
            max_items_per_refresh: Some(2),
        },
    )
    .unwrap();

    let first_refresh =
        tiktok_subscriptions::queue_tiktok_subscription(&paths, &subscription.id).unwrap();
    let first_ids = first_refresh
        .iter()
        .map(|job| job.id.clone())
        .collect::<Vec<_>>();
    assert_succeeded(
        &wait_for_jobs(&paths, &first_ids, Duration::from_secs(180)),
        "TikTok first profile refresh",
    );
    assert_succeeded(
        &wait_for_quiescence(&paths, Duration::from_secs(300)),
        "TikTok first refresh child transfers",
    );
    let first_item_count = library::list_items(&paths, 2_000, 0).unwrap().len();
    let first_transfer_count = transfer_job_count(&paths, "tiktok");

    let second_refresh =
        tiktok_subscriptions::queue_tiktok_subscription(&paths, &subscription.id).unwrap();
    let second_ids = second_refresh
        .iter()
        .map(|job| job.id.clone())
        .collect::<Vec<_>>();
    assert_succeeded(
        &wait_for_jobs(&paths, &second_ids, Duration::from_secs(180)),
        "TikTok second profile refresh",
    );
    assert_succeeded(
        &wait_for_quiescence(&paths, Duration::from_secs(180)),
        "TikTok second refresh queue",
    );
    let second_item_count = library::list_items(&paths, 2_000, 0).unwrap().len();
    assert_eq!(
        first_item_count, second_item_count,
        "second refresh must not duplicate archived TikTok media"
    );
    assert_eq!(
        first_transfer_count,
        transfer_job_count(&paths, "tiktok"),
        "second refresh must not enqueue a canonical TikTok transfer again"
    );

    runner.stop();
    thread::sleep(Duration::from_secs(2));
    let restarted_runner = jobs::start_runner(paths.clone()).unwrap();
    let restart_refresh =
        tiktok_subscriptions::queue_tiktok_subscription(&paths, &subscription.id).unwrap();
    let restart_ids = restart_refresh
        .iter()
        .map(|job| job.id.clone())
        .collect::<Vec<_>>();
    assert_succeeded(
        &wait_for_jobs(&paths, &restart_ids, Duration::from_secs(180)),
        "TikTok post-restart refresh",
    );
    assert_succeeded(
        &wait_for_quiescence(&paths, Duration::from_secs(180)),
        "TikTok post-restart queue",
    );
    restarted_runner.stop();
    assert_eq!(
        first_transfer_count,
        transfer_job_count(&paths, "tiktok"),
        "post-restart refresh must not enqueue a canonical TikTok transfer again"
    );

    let persisted = tiktok_subscriptions::list_tiktok_subscriptions(&paths).unwrap();
    let row = persisted
        .iter()
        .find(|row| row.id == subscription.id)
        .unwrap();
    let contract = assert_provider_contract(&paths, "tiktok");
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "provider": "tiktok",
            "profile": profile_url,
            "single_url": video_url,
            "single_job_ids": single_ids,
            "first_refresh_job_ids": first_ids,
            "second_refresh_job_ids": second_ids,
            "restart_refresh_job_ids": restart_ids,
            "library_items_after_first": first_item_count,
            "library_items_after_second": second_item_count,
            "provider_contract": contract,
            "subscription": row,
        }))
        .unwrap()
    );
}
