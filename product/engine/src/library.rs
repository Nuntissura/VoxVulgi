use crate::ffmpeg;
use crate::paths::AppPaths;
use crate::root_rebind;
use crate::{db, EngineError, Result};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use url::Url;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CanonicalMediaSource {
    pub service: String,
    pub media_id: String,
    pub canonical_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadPreflightRow {
    pub input_index: usize,
    pub url: String,
    pub status: String,
    pub service: Option<String>,
    pub media_id: Option<String>,
    pub library_item_id: Option<String>,
    pub library_title: Option<String>,
    pub media_path: Option<String>,
    pub active_job_id: Option<String>,
    pub failed_url: Option<String>,
    pub last_error: Option<String>,
    pub observation_state: Option<String>,
    pub observation_observed_at_ms: Option<i64>,
    pub observation_source: Option<String>,
    pub observation_duration_ms: Option<i64>,
    pub observation_age_ms: Option<i64>,
    pub observation_refresh_in_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadSourceClaim {
    Claimed,
    Active(String),
    Present(String),
    Missing(String),
    OperatorDeleted(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MediaPathObservation {
    Present,
    Missing,
    Unreachable,
    Slow,
}

#[derive(Debug, Clone, Default)]
pub struct MediaProbeCausalEnvelope {
    pub request_id: Option<String>,
    pub span_id: Option<String>,
    pub incident_id: Option<String>,
}

const MEDIA_PATH_OBSERVATION_TTL: Duration = Duration::from_secs(30);
const MEDIA_PATH_OBSERVATION_TIMEOUT: Duration = Duration::from_millis(1_500);
const MEDIA_PATH_RECONCILE_QUEUE_CAPACITY: usize = 128;
const MEDIA_PATH_PROBE_QUEUE_CAPACITY: usize = 64;
const MEDIA_PATH_PROBE_WORKERS: usize = 4;
const MEDIA_PATH_OBSERVATION_CACHE_CAPACITY: usize = 4_096;
static MEDIA_PATH_OBSERVATIONS: OnceLock<Mutex<HashMap<String, (Instant, MediaPathObservation)>>> =
    OnceLock::new();
static MEDIA_PATH_OBSERVATION_GENERATIONS: OnceLock<Mutex<HashMap<String, (u64, Instant)>>> =
    OnceLock::new();
static MEDIA_PATH_RECONCILER: OnceLock<
    mpsc::SyncSender<(AppPaths, String, Option<MediaProbeCausalEnvelope>)>,
> = OnceLock::new();
static MEDIA_PATH_RECONCILE_PENDING: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

/// Resolve a historical stored media identity to its verified active filesystem location.
/// The stored DB value remains unchanged so identity/dedupe and rollback stay stable.
pub fn resolve_media_path(paths: &AppPaths, stored_path: &str) -> Result<PathBuf> {
    root_rebind::resolve_active_alias_path(paths, Path::new(stored_path.trim()), false)
}
static MEDIA_PATH_PROBE_POOL: OnceLock<
    mpsc::SyncSender<(PathBuf, mpsc::SyncSender<MediaPathObservation>)>,
> = OnceLock::new();

fn media_path_probe_pool(
) -> &'static mpsc::SyncSender<(PathBuf, mpsc::SyncSender<MediaPathObservation>)> {
    MEDIA_PATH_PROBE_POOL.get_or_init(|| {
        let (sender, receiver) = mpsc::sync_channel::<(
            PathBuf,
            mpsc::SyncSender<MediaPathObservation>,
        )>(MEDIA_PATH_PROBE_QUEUE_CAPACITY);
        let receiver = std::sync::Arc::new(Mutex::new(receiver));
        for index in 0..MEDIA_PATH_PROBE_WORKERS {
            let receiver = receiver.clone();
            std::thread::Builder::new()
                .name(format!("voxvulgi-media-probe-{index}"))
                .spawn(move || loop {
                    let request = receiver.lock().unwrap_or_else(|p| p.into_inner()).recv();
                    let Ok((candidate, reply)) = request else {
                        break;
                    };
                    let observation = match candidate.try_exists() {
                        Ok(true) if candidate.is_file() => MediaPathObservation::Present,
                        Ok(_) => MediaPathObservation::Missing,
                        Err(_) => MediaPathObservation::Unreachable,
                    };
                    let _ = reply.try_send(observation);
                })
                .expect("media probe worker must start");
        }
        sender
    })
}

fn media_path_observations() -> &'static Mutex<HashMap<String, (Instant, MediaPathObservation)>> {
    MEDIA_PATH_OBSERVATIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn media_path_observation_generations() -> &'static Mutex<HashMap<String, (u64, Instant)>> {
    MEDIA_PATH_OBSERVATION_GENERATIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn media_path_observation_generation(path: &str) -> u64 {
    let mut generations = media_path_observation_generations()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some((generation, touched_at)) = generations.get_mut(path) else {
        return 0;
    };
    *touched_at = Instant::now();
    *generation
}

fn bound_media_path_observation_generations(
    generations: &mut HashMap<String, (u64, Instant)>,
    incoming_path: &str,
) {
    if !generations.contains_key(incoming_path)
        && generations.len() >= MEDIA_PATH_OBSERVATION_CACHE_CAPACITY
    {
        if let Some(oldest) = generations
            .iter()
            .min_by_key(|(_, (_, touched_at))| *touched_at)
            .map(|(path, _)| path.clone())
        {
            generations.remove(&oldest);
        }
    }
}

fn cache_media_path_observation(path: &str, observation: MediaPathObservation) {
    let mut cache = media_path_observations()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !cache.contains_key(path) && cache.len() >= MEDIA_PATH_OBSERVATION_CACHE_CAPACITY {
        if let Some(oldest) = cache
            .iter()
            .min_by_key(|(_, (observed_at, _))| *observed_at)
            .map(|(key, _)| key.clone())
        {
            cache.remove(&oldest);
            media_path_observation_generations()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .remove(&oldest);
        }
    }
    cache.insert(path.to_string(), (Instant::now(), observation));
}

/// A disconnected or heavily contended NAS must not pin a Tauri worker indefinitely. Cache a
/// bounded observation briefly, and distinguish timeout/error from an authoritative missing file.
pub(crate) fn observe_media_path(paths: &AppPaths, path: &str) -> MediaPathObservation {
    if let Ok(conn) = db::open_readonly(paths) {
        return observe_media_path_with_conn(paths, &conn, path, None, false);
    }
    queue_media_path_reconcile(paths, path, None);
    MediaPathObservation::Unreachable
}

fn observe_media_path_with_conn(
    paths: &AppPaths,
    conn: &rusqlite::Connection,
    path: &str,
    causal: Option<&MediaProbeCausalEnvelope>,
    fresh_if_stale: bool,
) -> MediaPathObservation {
    let now = Instant::now();
    if let Some((observed_at, observation)) = media_path_observations()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(path)
        .copied()
    {
        if now.duration_since(observed_at) <= MEDIA_PATH_OBSERVATION_TTL {
            return observation;
        }
    }

    let persisted = conn
        .query_row(
            "SELECT state, next_refresh_at_ms, invalidated_at_ms FROM media_availability_observation WHERE path=?1",
            [path],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, Option<i64>>(2)?)),
        )
        .optional()
        .ok()
        .flatten();
    if let Some((state, next_refresh_at_ms, invalidated_at_ms)) = persisted {
        let observation = media_path_observation_from_str(&state);
        if invalidated_at_ms.is_none() && next_refresh_at_ms > now_ms() {
            cache_media_path_observation(path, observation);
            return observation;
        }
        if fresh_if_stale {
            return observe_media_path_fresh_with_causal(paths, path, causal);
        }
        queue_media_path_reconcile(paths, path, causal.cloned());
        return observation;
    }
    if fresh_if_stale {
        return observe_media_path_fresh_with_causal(paths, path, causal);
    }
    queue_media_path_reconcile(paths, path, causal.cloned());
    MediaPathObservation::Unreachable
}

/// Execution-boundary callers must not reuse the short-lived observation cache: a cleanup,
/// relocation, or restore can change a canonical path while an old job remains queued.
/// The actual filesystem probe stays timeout-bounded and must be called outside a DB
/// transaction.
pub(crate) fn observe_media_path_fresh(paths: &AppPaths, path: &str) -> MediaPathObservation {
    observe_media_path_fresh_with_causal(paths, path, None)
}

fn observe_media_path_fresh_with_causal(
    paths: &AppPaths,
    path: &str,
    causal: Option<&MediaProbeCausalEnvelope>,
) -> MediaPathObservation {
    let now = Instant::now();
    let probe_started_at_ms = now_ms();
    let generation = media_path_observation_generation(path);
    crate::diagnostics::emit_trace_event(
        paths,
        "media_path_probe_started",
        "info",
        media_path_probe_details(causal, generation, "nas_bounded_worker_pool", None, None),
    );
    // Keep historical identity paths immutable; resolve a verified alias only for this bounded
    // filesystem probe so a rebind stays reversible and cleanup inventory remains truthful.
    let candidate = match root_rebind::resolve_active_alias_path(paths, Path::new(path), false) {
        Ok(candidate) => candidate,
        Err(_) => {
            let observation = MediaPathObservation::Unreachable;
            commit_media_path_observation_if_current(
                paths,
                path,
                observation,
                "root_alias_resolution_failed",
                now.elapsed(),
                probe_started_at_ms,
                generation,
            );
            emit_media_path_probe_completed(paths, causal, generation, observation, "root_alias_resolution_failed", now.elapsed());
            return observation;
        }
    };
    let (sender, receiver) = mpsc::sync_channel(1);
    let observation = match media_path_probe_pool().try_send((candidate, sender)) {
        Ok(()) => media_path_observation_from_probe_receive(
            receiver.recv_timeout(MEDIA_PATH_OBSERVATION_TIMEOUT),
        ),
        // Fixed-capacity saturation is a distinct latency signal, not evidence that storage is
        // unreachable. The caller remains bounded and the occupied worker slots stay accounted.
        Err(mpsc::TrySendError::Full(_)) => MediaPathObservation::Slow,
        Err(mpsc::TrySendError::Disconnected(_)) => MediaPathObservation::Unreachable,
    };
    commit_media_path_observation_if_current(
        paths,
        path,
        observation,
        "fresh_probe",
        now.elapsed(),
        probe_started_at_ms,
        generation,
    );
    emit_media_path_probe_completed(paths, causal, generation, observation, "nas_bounded_worker_pool", now.elapsed());
    observation
}

fn media_path_observation_from_probe_receive(
    result: std::result::Result<MediaPathObservation, mpsc::RecvTimeoutError>,
) -> MediaPathObservation {
    match result {
        Ok(observation) => observation,
        Err(mpsc::RecvTimeoutError::Timeout) => MediaPathObservation::Slow,
        Err(mpsc::RecvTimeoutError::Disconnected) => MediaPathObservation::Unreachable,
    }
}

fn emit_media_path_probe_completed(
    paths: &AppPaths,
    causal: Option<&MediaProbeCausalEnvelope>,
    generation: u64,
    observation: MediaPathObservation,
    source: &str,
    duration: Duration,
) {
    let result = match observation {
        MediaPathObservation::Present => "present",
        MediaPathObservation::Missing => "missing",
        MediaPathObservation::Unreachable => "unreachable",
        MediaPathObservation::Slow => "slow",
    };
    crate::diagnostics::emit_trace_event(
        paths,
        "media_path_probe_completed",
        if matches!(observation, MediaPathObservation::Unreachable | MediaPathObservation::Slow) { "warn" } else { "info" },
        media_path_probe_details(causal, generation, source, Some(duration), Some(result)),
    );
}

fn media_path_probe_details(
    causal: Option<&MediaProbeCausalEnvelope>,
    generation: u64,
    source: &str,
    duration: Option<Duration>,
    result: Option<&str>,
) -> serde_json::Value {
    serde_json::json!({
        "request_id": causal.and_then(|value| value.request_id.clone()),
        "span_id": causal.and_then(|value| value.span_id.clone()),
        "incident_id": causal.and_then(|value| value.incident_id.clone()),
        "duration_ms": duration.map(|value| value.as_millis() as u64),
        "source": source,
        "generation": generation,
        "result": result,
    })
}

fn invalidate_media_path_observation_memory(path: &str) {
    media_path_observations()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(path);
    let mut generations = media_path_observation_generations()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    bound_media_path_observation_generations(&mut generations, path);
    let generation = generations
        .entry(path.to_string())
        .or_insert_with(|| (0, Instant::now()));
    generation.0 = generation.0.saturating_add(1);
    generation.1 = Instant::now();
}

fn persist_media_path_observation_invalidation(
    conn: &rusqlite::Connection,
    path: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO media_availability_observation(path,state,observed_at_ms,source,duration_ms,next_refresh_at_ms,invalidated_at_ms) VALUES(?1,'unreachable',0,'invalidated',0,0,?2) ON CONFLICT(path) DO UPDATE SET invalidated_at_ms=excluded.invalidated_at_ms,next_refresh_at_ms=0",
        params![path, now_ms()],
    )?;
    Ok(())
}

pub(crate) fn persist_media_path_observation_rewrite_invalidation(
    conn: &rusqlite::Connection,
    old_path: &str,
    new_path: &str,
) -> Result<()> {
    persist_media_path_observation_invalidation(conn, old_path)?;
    if !old_path.eq_ignore_ascii_case(new_path) {
        persist_media_path_observation_invalidation(conn, new_path)?;
    }
    Ok(())
}

pub(crate) fn invalidate_media_path_observation_rewrite_memory(
    old_path: &str,
    new_path: &str,
) {
    invalidate_media_path_observation_memory(old_path);
    if !old_path.eq_ignore_ascii_case(new_path) {
        invalidate_media_path_observation_memory(new_path);
    }
}

fn invalidate_media_path_observation(paths: &AppPaths, path: &str) -> Result<()> {
    let mut conn = db::open(paths)?;
    db::migrate(&conn)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    persist_media_path_observation_invalidation(&tx, path)?;
    tx.commit()?;
    invalidate_media_path_observation_memory(path);
    Ok(())
}

pub(crate) fn invalidate_media_path_observation_rewrite(
    paths: &AppPaths,
    old_path: &str,
    new_path: &str,
) -> Result<()> {
    let mut conn = db::open(paths)?;
    db::migrate(&conn)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    persist_media_path_observation_rewrite_invalidation(&tx, old_path, new_path)?;
    tx.commit()?;
    invalidate_media_path_observation_rewrite_memory(old_path, new_path);
    Ok(())
}

fn media_path_observation_from_str(value: &str) -> MediaPathObservation {
    match value {
        "present" => MediaPathObservation::Present,
        "missing" => MediaPathObservation::Missing,
        "slow" => MediaPathObservation::Slow,
        _ => MediaPathObservation::Unreachable,
    }
}

fn commit_media_path_observation_if_current(
    paths: &AppPaths,
    path: &str,
    observation: MediaPathObservation,
    source: &str,
    duration: Duration,
    probe_started_at_ms: i64,
    expected_generation: u64,
) {
    if media_path_observation_generation(path) != expected_generation {
        return;
    }
    let state = match observation {
        MediaPathObservation::Present => "present",
        MediaPathObservation::Missing => "missing",
        MediaPathObservation::Unreachable => "unreachable",
        MediaPathObservation::Slow => "slow",
    };
    let observed_at_ms = now_ms();
    let refresh_after_ms = match observation {
        MediaPathObservation::Present => 10 * 60 * 1000,
        MediaPathObservation::Missing => 2 * 60 * 1000,
        MediaPathObservation::Unreachable => 30 * 1000,
        MediaPathObservation::Slow => 15 * 1000,
    };
    if let Ok(conn) = db::open(paths) {
        let _ = db::migrate(&conn);
        let committed = conn.execute(
            "INSERT INTO media_availability_observation(path,state,observed_at_ms,source,duration_ms,next_refresh_at_ms,invalidated_at_ms) VALUES(?1,?2,?3,?4,?5,?6,NULL) ON CONFLICT(path) DO UPDATE SET state=excluded.state, observed_at_ms=excluded.observed_at_ms, source=excluded.source, duration_ms=excluded.duration_ms, next_refresh_at_ms=excluded.next_refresh_at_ms, invalidated_at_ms=NULL WHERE (media_availability_observation.invalidated_at_ms IS NULL OR media_availability_observation.invalidated_at_ms < ?7) AND media_availability_observation.observed_at_ms <= ?7",
            params![path, state, observed_at_ms, source, duration.as_millis() as i64, observed_at_ms + refresh_after_ms, probe_started_at_ms],
        ).unwrap_or(0);
        if committed > 0 && media_path_observation_generation(path) == expected_generation {
            cache_media_path_observation(path, observation);
        }
    }
}

fn queue_media_path_reconcile(
    paths: &AppPaths,
    path: &str,
    causal: Option<MediaProbeCausalEnvelope>,
) {
    let pending = MEDIA_PATH_RECONCILE_PENDING.get_or_init(|| Mutex::new(HashSet::new()));
    if !pending
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(path.to_string())
    {
        return;
    }
    let sender = MEDIA_PATH_RECONCILER.get_or_init(|| {
        let (sender, receiver) =
            mpsc::sync_channel::<(AppPaths, String, Option<MediaProbeCausalEnvelope>)>(
                MEDIA_PATH_RECONCILE_QUEUE_CAPACITY,
            );
        let _ = std::thread::Builder::new()
            .name("voxvulgi-media-reconciler".to_string())
            .spawn(move || {
                while let Ok((paths, path, causal)) = receiver.recv() {
                    let _ = observe_media_path_fresh_with_causal(&paths, &path, causal.as_ref());
                    MEDIA_PATH_RECONCILE_PENDING
                        .get_or_init(|| Mutex::new(HashSet::new()))
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .remove(&path);
                }
            });
        sender
    });
    if sender
        .try_send((paths.clone(), path.to_string(), causal))
        .is_err()
    {
        pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(path);
    }
}

pub fn canonical_media_source(raw_url: &str) -> Option<CanonicalMediaSource> {
    let trimmed = raw_url.trim();
    if let Some(media_id) = crate::subscriptions::youtube_video_id_from_url(trimmed) {
        return Some(CanonicalMediaSource {
            service: "youtube".to_string(),
            canonical_url: format!("https://www.youtube.com/watch?v={media_id}"),
            media_id,
        });
    }
    let mut parsed = Url::parse(trimmed).ok()?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return None;
    }
    parsed.set_fragment(None);
    let host = parsed
        .host_str()?
        .trim_start_matches("www.")
        .to_ascii_lowercase();
    parsed.set_host(Some(&host)).ok()?;
    if host == "instagram.com" || host.ends_with(".instagram.com") {
        let segments = parsed
            .path_segments()
            .map(|parts| parts.filter(|part| !part.is_empty()).collect::<Vec<_>>())
            .unwrap_or_default();
        if segments.len() >= 2
            && matches!(
                segments[0].to_ascii_lowercase().as_str(),
                "p" | "reel" | "reels" | "tv"
            )
        {
            let shortcode = segments[1].to_string();
            return Some(CanonicalMediaSource {
                service: "instagram".to_string(),
                media_id: format!("post:{shortcode}"),
                canonical_url: format!("https://www.instagram.com/p/{shortcode}/"),
            });
        }
        if segments.len() >= 3 && segments[0].eq_ignore_ascii_case("stories") {
            let story_id = segments[2].to_string();
            return Some(CanonicalMediaSource {
                service: "instagram".to_string(),
                media_id: format!("story:{story_id}"),
                canonical_url: format!(
                    "https://www.instagram.com/stories/{}/{story_id}/",
                    segments[1]
                ),
            });
        }
        if let Some(profile) = segments.first() {
            let profile = profile.to_ascii_lowercase();
            return Some(CanonicalMediaSource {
                service: "instagram".to_string(),
                media_id: format!("profile:{profile}"),
                canonical_url: format!("https://www.instagram.com/{profile}/"),
            });
        }
    }
    let canonical_url = parsed.to_string();
    Some(CanonicalMediaSource {
        service: "web".to_string(),
        media_id: canonical_url.clone(),
        canonical_url,
    })
}

fn ensure_source_identity_row_conn(
    conn: &rusqlite::Connection,
    source: &CanonicalMediaSource,
    source_url: &str,
) -> Result<()> {
    let now = now_ms();
    conn.execute(
        r#"
INSERT INTO media_source_identity (
  service, media_id, canonical_url, repair_state, created_at_ms, updated_at_ms
) VALUES (?1, ?2, ?3, 'ready', ?4, ?4)
ON CONFLICT(service, media_id) DO UPDATE SET
  canonical_url=excluded.canonical_url
"#,
        params![source.service, source.media_id, source.canonical_url, now],
    )?;
    conn.execute(
        "INSERT OR IGNORE INTO media_source_alias (service, media_id, source_url, created_at_ms) VALUES (?1, ?2, ?3, ?4)",
        params![source.service, source.media_id, source_url.trim(), now],
    )?;
    Ok(())
}

fn ensure_source_identity_conn(
    conn: &rusqlite::Connection,
    source: &CanonicalMediaSource,
    source_url: &str,
) -> Result<()> {
    ensure_source_identity_row_conn(conn, source, source_url)?;
    let now = now_ms();
    let linked: Option<String> = conn
        .query_row(
            "SELECT library_item_id FROM media_source_identity WHERE service=?1 AND media_id=?2",
            params![source.service, source.media_id],
            |row| row.get(0),
        )
        .optional()?
        .flatten();
    if linked.is_some() {
        return Ok(());
    }

    // Preservation-first lazy backfill. Candidate SQL stays indexed for exact URLs; the bounded
    // YouTube LIKE fallback is verified again through the canonical parser before linking.
    let like = format!(
        "%{}%",
        source.media_id.replace('%', "\\%").replace('_', "\\_")
    );
    let mut stmt = conn.prepare(
        r#"
SELECT id, source_uri
FROM library_item
WHERE source_uri=?1 OR source_uri LIKE ?2 ESCAPE '\'
ORDER BY created_at_ms DESC
LIMIT 50
"#,
    )?;
    let candidates = stmt
        .query_map(params![source_url.trim(), like], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (item_id, candidate_url) in candidates {
        let Some(candidate) = canonical_media_source(&candidate_url) else {
            continue;
        };
        if candidate.service == source.service && candidate.media_id == source.media_id {
            conn.execute(
                "UPDATE media_source_identity SET library_item_id=?1, repair_state='ready', updated_at_ms=?2 WHERE service=?3 AND media_id=?4 AND library_item_id IS NULL",
                params![item_id, now, source.service, source.media_id],
            )?;
            break;
        }
    }
    Ok(())
}

fn upsert_source_membership_conn(
    conn: &rusqlite::Connection,
    source: &CanonicalMediaSource,
    source_subscription_id: Option<&str>,
    evidence_kind: &str,
) -> Result<()> {
    if source.service != "youtube" {
        return Ok(());
    }
    let Some(subscription_id) = source_subscription_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };
    let subscription = conn
        .query_row(
            "SELECT source_url, title FROM youtube_subscription WHERE id=?1",
            [subscription_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    let Some((source_url, source_title)) = subscription else {
        return Ok(());
    };
    let lower = source_url.trim().to_ascii_lowercase();
    let source_kind = if lower.contains("/playlist") || lower.contains("list=") {
        "playlist"
    } else if lower.trim_end_matches('/').ends_with("/shorts") {
        "shorts_page"
    } else if lower.trim_end_matches('/').ends_with("/videos") {
        "videos_page"
    } else if canonical_media_source(&source_url)
        .map(|value| value.service == "youtube" && value.media_id == source.media_id)
        .unwrap_or(false)
    {
        "direct_video"
    } else {
        "channel_page"
    };
    let now = now_ms();
    let inserted = conn.execute(
        r#"
INSERT OR IGNORE INTO media_source_membership (
  service, media_id, source_subscription_id, source_kind, source_url_snapshot,
  source_title_snapshot, evidence_kind, created_at_ms, updated_at_ms
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
"#,
        params![
            source.service,
            source.media_id,
            subscription_id,
            source_kind,
            source_url,
            source_title,
            evidence_kind,
            now
        ],
    )?;
    if inserted == 0 {
        conn.execute(
            r#"
UPDATE media_source_membership SET
  source_kind=?1, source_url_snapshot=?2, source_title_snapshot=?3,
  evidence_kind=?4, updated_at_ms=?5
WHERE service=?6 AND media_id=?7 AND source_subscription_id=?8
"#,
            params![
                source_kind,
                source_url,
                source_title,
                evidence_kind,
                now,
                source.service,
                source.media_id,
                subscription_id
            ],
        )?;
    }
    Ok(())
}

fn source_membership_kind(source_url: &str, source: &CanonicalMediaSource) -> &'static str {
    let lower = source_url.trim().to_ascii_lowercase();
    if lower.contains("/playlist") || lower.contains("list=") {
        "playlist"
    } else if lower.trim_end_matches('/').ends_with("/shorts") {
        "shorts_page"
    } else if lower.trim_end_matches('/').ends_with("/videos") {
        "videos_page"
    } else if canonical_media_source(source_url)
        .map(|value| value.service == source.service && value.media_id == source.media_id)
        .unwrap_or(false)
    {
        "direct_video"
    } else {
        "channel_page"
    }
}

/// Preserve the source context of a durable queued attempt inside its caller's transaction.
/// Current subscription rows remain authoritative; enqueue-time snapshots are used only when a
/// historical subscription row is unavailable.
pub(crate) fn record_queued_download_source_context_conn(
    conn: &rusqlite::Connection,
    source_url: &str,
    origin_kind: &str,
    source_subscription_id: Option<&str>,
    source_job_id: &str,
    source_page_url_snapshot: Option<&str>,
    source_title_snapshot: Option<&str>,
    evidence_kind: &str,
) -> Result<CanonicalMediaSource> {
    record_queued_download_source_context_inner_conn(
        conn,
        source_url,
        origin_kind,
        source_subscription_id,
        source_job_id,
        source_page_url_snapshot,
        source_title_snapshot,
        evidence_kind,
        true,
    )
}

/// Bulk queue compaction already performs a separate authoritative path reconciliation. Avoid
/// the legacy per-job `library_item.source_uri LIKE` backfill here so the full canonical queue
/// remains O(queue) instead of O(queue × library).
pub(crate) fn record_queued_download_source_context_without_library_backfill_conn(
    conn: &rusqlite::Connection,
    source_url: &str,
    origin_kind: &str,
    source_subscription_id: Option<&str>,
    source_job_id: &str,
    source_page_url_snapshot: Option<&str>,
    source_title_snapshot: Option<&str>,
    evidence_kind: &str,
) -> Result<CanonicalMediaSource> {
    record_queued_download_source_context_inner_conn(
        conn,
        source_url,
        origin_kind,
        source_subscription_id,
        source_job_id,
        source_page_url_snapshot,
        source_title_snapshot,
        evidence_kind,
        false,
    )
}

fn record_queued_download_source_context_inner_conn(
    conn: &rusqlite::Connection,
    source_url: &str,
    origin_kind: &str,
    source_subscription_id: Option<&str>,
    source_job_id: &str,
    source_page_url_snapshot: Option<&str>,
    source_title_snapshot: Option<&str>,
    evidence_kind: &str,
    allow_library_backfill: bool,
) -> Result<CanonicalMediaSource> {
    let source = canonical_media_source(source_url).ok_or_else(|| {
        EngineError::InstallFailed("download URL has no canonical media identity".to_string())
    })?;
    if allow_library_backfill {
        ensure_source_identity_conn(conn, &source, source_url)?;
    } else {
        ensure_source_identity_row_conn(conn, &source, source_url)?;
    }
    conn.execute(
        "INSERT OR IGNORE INTO media_source_association (id, service, media_id, origin_kind, source_subscription_id, source_job_id, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            Uuid::new_v4().to_string(),
            source.service,
            source.media_id,
            origin_kind,
            source_subscription_id,
            source_job_id,
            now_ms()
        ],
    )?;
    let Some(subscription_id) = source_subscription_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(source);
    };
    let membership_exists = conn
        .query_row(
            "SELECT 1 FROM media_source_membership WHERE service=?1 AND media_id=?2 AND source_subscription_id=?3",
            params![source.service, source.media_id, subscription_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if membership_exists {
        return Ok(source);
    }

    let subscription_snapshot: Option<(String, String)> = conn
        .query_row(
            "SELECT source_url, title FROM youtube_subscription WHERE id=?1",
            [subscription_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let snapshot_url = subscription_snapshot
        .as_ref()
        .map(|value| value.0.as_str())
        .or_else(|| {
            source_page_url_snapshot
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
        .unwrap_or(source_url);
    let snapshot_title = subscription_snapshot
        .as_ref()
        .map(|value| value.1.as_str())
        .or_else(|| {
            source_title_snapshot
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
        .unwrap_or(subscription_id);
    let now = now_ms();
    conn.execute(
        r#"
INSERT OR IGNORE INTO media_source_membership (
  service, media_id, source_subscription_id, source_kind, source_url_snapshot,
  source_title_snapshot, evidence_kind, created_at_ms, updated_at_ms
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
"#,
        params![
            source.service,
            source.media_id,
            subscription_id,
            source_membership_kind(snapshot_url, &source),
            snapshot_url,
            snapshot_title,
            evidence_kind,
            now
        ],
    )?;
    Ok(source)
}

fn clear_stale_source_claim_conn(
    conn: &rusqlite::Connection,
    source: &CanonicalMediaSource,
) -> Result<Option<String>> {
    let active_claim: Option<(String, i64)> = conn
        .query_row(
            "SELECT active_job_id, updated_at_ms FROM media_source_identity WHERE service=?1 AND media_id=?2 AND active_job_id IS NOT NULL",
            params![source.service, source.media_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((active_job_id, claimed_at_ms)) = active_claim else {
        return Ok(None);
    };
    let active = conn
        .query_row(
            "SELECT 1 FROM job WHERE id=?1 AND status IN ('queued','running') LIMIT 1",
            [&active_job_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if active {
        return Ok(Some(active_job_id));
    }
    // Claims are intentionally written immediately before their durable job row. Preserve that
    // small atomic hand-off window so two concurrent enqueue calls cannot both clear and acquire
    // the same identity. A crashed pre-insert claimant self-heals after this grace period.
    if now_ms().saturating_sub(claimed_at_ms) < 60_000 {
        return Ok(Some(active_job_id));
    }
    conn.execute(
        "UPDATE media_source_identity SET active_job_id=NULL, updated_at_ms=?1 WHERE service=?2 AND media_id=?3 AND active_job_id=?4",
        params![now_ms(), source.service, source.media_id, active_job_id],
    )?;
    Ok(None)
}

pub fn preflight_download_urls(
    paths: &AppPaths,
    urls: &[String],
) -> Result<Vec<DownloadPreflightRow>> {
    preflight_download_urls_with_causal(paths, urls, None)
}

pub fn preflight_download_urls_with_causal(
    paths: &AppPaths,
    urls: &[String],
    causal: Option<&MediaProbeCausalEnvelope>,
) -> Result<Vec<DownloadPreflightRow>> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    let mut seen = HashSet::new();
    let mut out = Vec::with_capacity(urls.len());
    for (input_index, url) in urls.iter().enumerate() {
        let Some(source) = canonical_media_source(url) else {
            out.push(DownloadPreflightRow {
                input_index,
                url: url.clone(),
                status: "invalid".to_string(),
                service: None,
                media_id: None,
                library_item_id: None,
                library_title: None,
                media_path: None,
                active_job_id: None,
                failed_url: None,
                last_error: None,
                observation_state: None,
                observation_observed_at_ms: None,
                observation_source: None,
                observation_duration_ms: None,
                observation_age_ms: None,
                observation_refresh_in_ms: None,
            });
            continue;
        };
        let key = format!("{}\n{}", source.service, source.media_id);
        if !seen.insert(key) {
            out.push(DownloadPreflightRow {
                input_index,
                url: url.clone(),
                status: "duplicate_input".to_string(),
                service: Some(source.service),
                media_id: Some(source.media_id),
                library_item_id: None,
                library_title: None,
                media_path: None,
                active_job_id: None,
                failed_url: None,
                last_error: None,
                observation_state: None,
                observation_observed_at_ms: None,
                observation_source: None,
                observation_duration_ms: None,
                observation_age_ms: None,
                observation_refresh_in_ms: None,
            });
            continue;
        }
        ensure_source_identity_conn(&conn, &source, url)?;
        let active_job_id = clear_stale_source_claim_conn(&conn, &source)?;
        let row = conn.query_row(
            r#"
SELECT i.library_item_id, li.title, li.media_path, i.last_failed_url, i.last_error, li.file_status
FROM media_source_identity i
LEFT JOIN library_item li ON li.id=i.library_item_id
WHERE i.service=?1 AND i.media_id=?2
"#,
            params![source.service, source.media_id],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                ))
            },
        )?;
        let status = if row
            .5
            .as_deref()
            .is_some_and(|value| matches!(value, "operator_deleted" | "delete_pending"))
        {
            "operator_deleted"
        } else if active_job_id.is_some() {
            "active"
        } else if let Some(path) = row.2.as_deref() {
            // Download preflight controls whether an existing canonical item is suppressed or
            // offered for repair. It is an execution-correctness boundary rather than a rendering
            // poll, so an absent/invalid observation receives one bounded exact probe.
            match observe_media_path_with_conn(paths, &conn, path, causal, true) {
                MediaPathObservation::Present => "present",
                MediaPathObservation::Missing => "missing",
                MediaPathObservation::Unreachable => "storage_unreachable",
                MediaPathObservation::Slow => "storage_slow",
            }
        } else {
            "ready"
        };
        let observation = row.2.as_deref().and_then(|path| conn.query_row(
            "SELECT state, observed_at_ms, source, duration_ms, next_refresh_at_ms FROM media_availability_observation WHERE path=?1",
            [path], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?, r.get::<_, i64>(4)?))
        ).optional().ok().flatten());
        let observed_now = now_ms();
        out.push(DownloadPreflightRow {
            input_index,
            url: url.clone(),
            status: status.to_string(),
            service: Some(source.service),
            media_id: Some(source.media_id),
            library_item_id: row.0,
            library_title: row.1,
            media_path: row.2,
            active_job_id,
            failed_url: row.3,
            observation_state: observation.as_ref().map(|v| v.0.clone()),
            observation_observed_at_ms: observation.as_ref().map(|v| v.1),
            observation_source: observation.as_ref().map(|v| v.2.clone()),
            observation_duration_ms: observation.as_ref().map(|v| v.3),
            observation_age_ms: observation
                .as_ref()
                .map(|v| observed_now.saturating_sub(v.1)),
            observation_refresh_in_ms: observation
                .as_ref()
                .map(|v| v.4.saturating_sub(observed_now).max(0)),
            last_error: row.4,
        });
    }
    Ok(out)
}

pub fn claim_download_source(
    paths: &AppPaths,
    source_url: &str,
    job_id: &str,
    allow_missing: bool,
    allow_operator_deleted: bool,
    origin_kind: &str,
    source_subscription_id: Option<&str>,
) -> Result<DownloadSourceClaim> {
    let source = canonical_media_source(source_url).ok_or_else(|| {
        EngineError::InstallFailed("download URL has no canonical media identity".to_string())
    })?;
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    ensure_source_identity_conn(&conn, &source, source_url)?;
    // Preserve every ingress association, even when this request correctly resolves to an
    // already-present or already-active canonical item and therefore creates no new job.
    conn.execute(
        "INSERT OR IGNORE INTO media_source_association (id, service, media_id, origin_kind, source_subscription_id, source_job_id, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![Uuid::new_v4().to_string(), source.service, source.media_id, origin_kind, source_subscription_id, job_id, now_ms()],
    )?;
    upsert_source_membership_conn(&conn, &source, source_subscription_id, "voxvulgi_discovery")?;
    let item: Option<(String, String, String)> = conn
        .query_row(
            r#"
SELECT li.id, li.media_path, li.file_status
FROM media_source_identity i
JOIN library_item li ON li.id=i.library_item_id
WHERE i.service=?1 AND i.media_id=?2
"#,
            params![source.service, source.media_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let operator_deleted_item_id = item.as_ref().and_then(|(item_id, _, file_status)| {
        matches!(file_status.as_str(), "operator_deleted" | "delete_pending")
            .then(|| item_id.clone())
    });
    if let Some(item_id) = operator_deleted_item_id.as_ref() {
        if !allow_operator_deleted {
            return Ok(DownloadSourceClaim::OperatorDeleted(item_id.clone()));
        }
    }
    if let Some(active) = clear_stale_source_claim_conn(&conn, &source)? {
        // Keep an existing exact redownload authorization intact. Replacing it here would
        // authorize a job ID that was never enqueued while invalidating the active job.
        return Ok(DownloadSourceClaim::Active(active));
    }
    if let Some(item_id) = operator_deleted_item_id {
        conn.execute(
            "UPDATE library_item SET file_redownload_authorized_job_id=?1, \
             file_status_changed_at_ms=?2, file_status_change_source='operator_manual_redownload' \
             WHERE id=?3 AND file_status IN ('operator_deleted','delete_pending')",
            params![job_id, now_ms(), item_id],
        )?;
    }
    if let Some((item_id, media_path, file_status)) = item {
        if matches!(file_status.as_str(), "operator_deleted" | "delete_pending")
            && allow_operator_deleted
        {
            // An explicit manual redownload treats the tombstoned path as intentionally absent.
            // A file that reappeared out of band remains protected by the normal present check.
        }
        match observe_media_path_fresh(paths, &media_path) {
            MediaPathObservation::Present => return Ok(DownloadSourceClaim::Present(item_id)),
            MediaPathObservation::Unreachable => {
                return Err(EngineError::InstallFailed(format!(
                    "storage is unreachable while checking canonical media: {media_path}"
                )))
            }
            MediaPathObservation::Slow => {
                return Err(EngineError::InstallFailed(format!(
                    "storage probe was too slow while checking canonical media: {media_path}"
                )))
            }
            MediaPathObservation::Missing if !allow_missing => {
                return Ok(DownloadSourceClaim::Missing(item_id))
            }
            MediaPathObservation::Missing => {}
        }
    }
    let changed = conn.execute(
        "UPDATE media_source_identity SET active_job_id=?1, repair_state=?2, updated_at_ms=?3 WHERE service=?4 AND media_id=?5 AND active_job_id IS NULL",
        params![
            job_id,
            if allow_missing { "redownloading" } else { "downloading" },
            now_ms(),
            source.service,
            source.media_id
        ],
    )?;
    if changed == 0 {
        let active: Option<String> = conn.query_row(
            "SELECT active_job_id FROM media_source_identity WHERE service=?1 AND media_id=?2",
            params![source.service, source.media_id],
            |row| row.get(0),
        )?;
        return Ok(DownloadSourceClaim::Active(active.unwrap_or_default()));
    }
    Ok(DownloadSourceClaim::Claimed)
}

pub fn record_source_association(
    paths: &AppPaths,
    source_url: &str,
    origin_kind: &str,
    source_subscription_id: Option<&str>,
    source_job_id: Option<&str>,
) -> Result<()> {
    let source = canonical_media_source(source_url).ok_or_else(|| {
        EngineError::InstallFailed("source URL has no canonical media identity".to_string())
    })?;
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    ensure_source_identity_conn(&conn, &source, source_url)?;
    conn.execute(
        "INSERT OR IGNORE INTO media_source_association (id, service, media_id, origin_kind, source_subscription_id, source_job_id, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![Uuid::new_v4().to_string(), source.service, source.media_id, origin_kind, source_subscription_id, source_job_id, now_ms()],
    )?;
    upsert_source_membership_conn(&conn, &source, source_subscription_id, "voxvulgi_discovery")?;
    Ok(())
}

pub fn release_download_source_claim(
    paths: &AppPaths,
    job_id: &str,
    failed_url: Option<&str>,
    error: Option<&str>,
) -> Result<()> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    conn.execute(
        r#"
UPDATE media_source_identity
SET active_job_id=NULL,
    repair_state=CASE WHEN library_item_id IS NULL THEN 'ready' ELSE 'source_failed' END,
    last_failed_url=COALESCE(?1, last_failed_url),
    last_error=COALESCE(?2, last_error),
    updated_at_ms=?3
WHERE active_job_id=?4
"#,
        params![
            failed_url,
            error.map(|value| value.chars().take(2000).collect::<String>()),
            now_ms(),
            job_id
        ],
    )?;
    conn.execute(
        "UPDATE library_item SET file_redownload_authorized_job_id=NULL \
         WHERE file_redownload_authorized_job_id=?1 \
           AND file_status IN ('operator_deleted','delete_pending')",
        [job_id],
    )?;
    Ok(())
}

pub fn relocate_canonical_media(
    paths: &AppPaths,
    item_id: &str,
    new_path: &Path,
) -> Result<LibraryItem> {
    let canonical = new_path.canonicalize()?;
    if !canonical.is_file() {
        return Err(EngineError::InstallFailed(
            "the selected relocation target is not a file".to_string(),
        ));
    }
    let canonical_text = canonical.to_string_lossy().to_string();
    let mut conn = db::open(paths)?;
    db::migrate(&conn)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let previous_path = tx
        .query_row(
            "SELECT media_path FROM library_item WHERE id=?1",
            [item_id.trim()],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let changed = tx.execute(
        "UPDATE library_item SET media_path=?1 WHERE id=?2",
        params![canonical_text, item_id.trim()],
    )?;
    if changed == 0 {
        return Err(EngineError::InstallFailed(
            "library item not found".to_string(),
        ));
    }
    if let Some(previous_path) = previous_path {
        persist_media_path_observation_rewrite_invalidation(
            &tx,
            &previous_path,
            &canonical_text,
        )?;
        tx.execute(
            "UPDATE media_source_identity SET repair_state='ready', last_error=NULL, updated_at_ms=?1 WHERE library_item_id=?2",
            params![now_ms(), item_id.trim()],
        )?;
        tx.commit()?;
        invalidate_media_path_observation_rewrite_memory(&previous_path, &canonical_text);
    } else {
        return Err(EngineError::InstallFailed(
            "library item had no canonical media path".to_string(),
        ));
    }
    get_item_by_id(paths, item_id)
}

pub fn replace_canonical_source_url(
    paths: &AppPaths,
    service: &str,
    media_id: &str,
    new_url: &str,
) -> Result<()> {
    let replacement = canonical_media_source(new_url)
        .ok_or_else(|| EngineError::InstallFailed("replacement URL is invalid".to_string()))?;
    if replacement.service != service || replacement.media_id != media_id {
        return Err(EngineError::InstallFailed(
            "replacement URL points to a different source video".to_string(),
        ));
    }
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    ensure_source_identity_conn(&conn, &replacement, new_url)?;
    conn.execute(
        "UPDATE media_source_identity SET canonical_url=?1, last_failed_url=NULL, last_error=NULL, repair_state=CASE WHEN library_item_id IS NULL THEN 'ready' ELSE 'missing' END, updated_at_ms=?2 WHERE service=?3 AND media_id=?4",
        params![new_url.trim(), now_ms(), service, media_id],
    )?;
    Ok(())
}

pub fn remove_canonical_library_record(paths: &AppPaths, item_id: &str) -> Result<bool> {
    let mut conn = db::open(paths)?;
    db::migrate(&conn)?;
    let tx = conn.transaction()?;
    tx.execute(
        "UPDATE media_source_identity SET library_item_id=NULL, repair_state='record_removed', updated_at_ms=?1 WHERE library_item_id=?2",
        params![now_ms(), item_id.trim()],
    )?;
    let removed = tx.execute("DELETE FROM library_item WHERE id=?1", [item_id.trim()])? > 0;
    tx.commit()?;
    Ok(removed)
}

pub const LIBRARY_FILE_STATUS_AVAILABLE: &str = "available";
pub const LIBRARY_FILE_STATUS_DELETE_PENDING: &str = "delete_pending";
pub const LIBRARY_FILE_STATUS_OPERATOR_DELETED: &str = "operator_deleted";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryFileDeleteResult {
    pub item_id: String,
    pub title: Option<String>,
    pub media_path: Option<String>,
    pub outcome: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryFileDeleteReceipt {
    pub mode: String,
    pub requested: usize,
    pub deleted: usize,
    pub already_deleted: usize,
    pub failed: usize,
    pub results: Vec<LibraryFileDeleteResult>,
}

#[derive(Debug, Clone)]
pub struct OperatorDeletedRedownloadTarget {
    pub item_id: String,
    pub title: String,
    pub source_url: String,
    pub output_dir: String,
}

const MAX_LIBRARY_FILE_ACTION_ITEMS: usize = 500;

/// WP-0284 explicit file deletion. Metadata, identity, membership, and history rows are retained.
/// `delete_pending` is written before the filesystem handoff so an interrupted operation cannot
/// reopen the item to subscription refresh or generic retry.
pub fn delete_library_item_files(
    paths: &AppPaths,
    item_ids: &[String],
    mode: &str,
    change_source: &str,
) -> Result<LibraryFileDeleteReceipt> {
    let mode = match mode.trim() {
        "trash" => "trash",
        "permanent" => "permanent",
        other => {
            return Err(EngineError::InstallFailed(format!(
                "unsupported file deletion mode: {other}"
            )))
        }
    };
    let change_source = match change_source.trim() {
        "operator" => "operator",
        "assistant" => "assistant",
        other => {
            return Err(EngineError::InstallFailed(format!(
                "unsupported file deletion source: {other}"
            )))
        }
    };
    let mut seen = HashSet::new();
    let ids = item_ids
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .filter(|value| seen.insert((*value).to_string()))
        .take(MAX_LIBRARY_FILE_ACTION_ITEMS + 1)
        .map(str::to_string)
        .collect::<Vec<_>>();
    if ids.is_empty() {
        return Err(EngineError::InstallFailed(
            "select at least one library item".to_string(),
        ));
    }
    if ids.len() > MAX_LIBRARY_FILE_ACTION_ITEMS {
        return Err(EngineError::InstallFailed(format!(
            "too many selected library items: {} (max {})",
            ids.len(),
            MAX_LIBRARY_FILE_ACTION_ITEMS
        )));
    }

    let mut receipt = LibraryFileDeleteReceipt {
        mode: mode.to_string(),
        requested: ids.len(),
        deleted: 0,
        already_deleted: 0,
        failed: 0,
        results: Vec::with_capacity(ids.len()),
    };
    for item_id in ids {
        let result = delete_library_item_file(paths, &item_id, mode, change_source);
        match result.outcome.as_str() {
            "deleted" => receipt.deleted += 1,
            "already_deleted" => receipt.already_deleted += 1,
            _ => receipt.failed += 1,
        }
        receipt.results.push(result);
    }
    Ok(receipt)
}

fn delete_library_item_file(
    paths: &AppPaths,
    item_id: &str,
    mode: &str,
    change_source: &str,
) -> LibraryFileDeleteResult {
    let mut conn = match db::open(paths).and_then(|conn| {
        db::migrate(&conn)?;
        Ok(conn)
    }) {
        Ok(conn) => conn,
        Err(err) => {
            return LibraryFileDeleteResult {
                item_id: item_id.to_string(),
                title: None,
                media_path: None,
                outcome: "failed".to_string(),
                message: err.to_string(),
            }
        }
    };
    let row: Option<(String, String, String)> = match conn
        .query_row(
            "SELECT li.title, li.media_path, li.file_status \
             FROM library_item li \
             WHERE li.id=?1 AND EXISTS ( \
               SELECT 1 FROM media_source_identity i \
               WHERE i.library_item_id=li.id AND i.service='youtube' \
             )",
            [item_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
    {
        Ok(row) => row,
        Err(err) => {
            return LibraryFileDeleteResult {
                item_id: item_id.to_string(),
                title: None,
                media_path: None,
                outcome: "failed".to_string(),
                message: err.to_string(),
            }
        }
    };
    let Some((title, media_path, initial_status)) = row else {
        return LibraryFileDeleteResult {
            item_id: item_id.to_string(),
            title: None,
            media_path: None,
            outcome: "failed".to_string(),
            message: "library item not found".to_string(),
        };
    };
    if initial_status == LIBRARY_FILE_STATUS_OPERATOR_DELETED {
        return LibraryFileDeleteResult {
            item_id: item_id.to_string(),
            title: Some(title),
            media_path: Some(media_path),
            outcome: "already_deleted".to_string(),
            message: "video is already marked deleted".to_string(),
        };
    }

    let was_pending = initial_status == LIBRARY_FILE_STATUS_DELETE_PENDING;
    if !matches!(
        initial_status.as_str(),
        LIBRARY_FILE_STATUS_AVAILABLE | LIBRARY_FILE_STATUS_DELETE_PENDING
    ) {
        return LibraryFileDeleteResult {
            item_id: item_id.to_string(),
            title: Some(title),
            media_path: Some(media_path),
            outcome: "failed".to_string(),
            message: format!("unsupported current file status: {initial_status}"),
        };
    }

    let begin_result: Result<()> = (|| {
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let running_count: i64 = tx.query_row(
            r#"
SELECT COUNT(*)
FROM media_source_identity i
JOIN job j ON j.id=i.active_job_id
WHERE i.library_item_id=?1 AND j.status='running'
"#,
            [item_id],
            |row| row.get(0),
        )?;
        if running_count > 0 {
            return Err(EngineError::InstallFailed(
                "video is currently downloading; cancel it before deleting the file".to_string(),
            ));
        }
        tx.execute(
            r#"
UPDATE job
SET status='canceled',
    error='[operator-delete-wp0284] selected canonical video was deleted by the operator',
    finished_at_ms=?1
WHERE status='queued'
  AND id IN (
    SELECT active_job_id
    FROM media_source_identity
    WHERE library_item_id=?2 AND active_job_id IS NOT NULL
  )
"#,
            params![now_ms(), item_id],
        )?;
        tx.execute(
            "UPDATE media_source_identity SET active_job_id=NULL, repair_state='operator_deleted', \
             updated_at_ms=?1 WHERE library_item_id=?2",
            params![now_ms(), item_id],
        )?;
        tx.execute(
            "UPDATE library_item SET file_status='delete_pending', \
             file_status_changed_at_ms=?1, file_status_change_source=?2, \
             file_delete_method=?3, file_redownload_authorized_job_id=NULL \
             WHERE id=?4 AND file_status IN ('available','delete_pending')",
            params![now_ms(), change_source, mode, item_id],
        )?;
        tx.commit()?;
        Ok(())
    })();
    if let Err(err) = begin_result {
        return LibraryFileDeleteResult {
            item_id: item_id.to_string(),
            title: Some(title),
            media_path: Some(media_path),
            outcome: "failed".to_string(),
            message: err.to_string(),
        };
    }

    let physical_media_path = resolve_media_path(paths, &media_path);
    let physical_media_text = physical_media_path
        .as_ref()
        .ok()
        .map(|path| path.to_string_lossy().to_string());
    let observation = if physical_media_path.is_ok() {
        observe_media_path_fresh(paths, &media_path)
    } else {
        MediaPathObservation::Unreachable
    };
    let filesystem_result = match (observation, physical_media_path.as_ref()) {
        (_, Err(error)) => Err(format!(
            "verified media root alias could not be resolved: {error}"
        )),
        (MediaPathObservation::Present, Ok(path)) if mode == "trash" => {
            trash::delete(path).map_err(|err| err.to_string())
        }
        (MediaPathObservation::Present, Ok(path)) => {
            std::fs::remove_file(path).map_err(|err| err.to_string())
        }
        (MediaPathObservation::Missing, Ok(_)) => Ok(()),
        (MediaPathObservation::Unreachable, Ok(_)) => {
            Err("storage is unreachable; file was not marked deleted".to_string())
        }
        (MediaPathObservation::Slow, Ok(_)) => {
            Err("storage probe was too slow; file was not marked deleted".to_string())
        }
    };
    if let Err(message) = filesystem_result {
        if !was_pending {
            let _ = conn.execute(
                "UPDATE library_item SET file_status='available', \
                 file_status_changed_at_ms=?1, file_status_change_source=?2, \
                 file_delete_method=NULL WHERE id=?3 AND file_status='delete_pending'",
                params![now_ms(), change_source, item_id],
            );
            let _ = conn.execute(
                "UPDATE media_source_identity SET repair_state='ready', updated_at_ms=?1 \
                 WHERE library_item_id=?2",
                params![now_ms(), item_id],
            );
        }
        return LibraryFileDeleteResult {
            item_id: item_id.to_string(),
            title: Some(title),
            media_path: Some(media_path),
            outcome: "failed".to_string(),
            message,
        };
    }
    let physical_observation_path = physical_media_text.filter(|path| path != &media_path);
    let finalization: Result<bool> = (|| {
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        persist_media_path_observation_invalidation(&tx, &media_path)?;
        if let Some(physical_path) = physical_observation_path.as_deref() {
            persist_media_path_observation_invalidation(&tx, physical_path)?;
        }
        let changed = tx.execute(
            "UPDATE library_item SET file_status='operator_deleted', \
             file_status_changed_at_ms=?1, file_status_change_source=?2, file_delete_method=?3 \
             WHERE id=?4 AND file_status='delete_pending'",
            params![now_ms(), change_source, mode, item_id],
        )?;
        tx.commit()?;
        if changed == 1 {
            invalidate_media_path_observation_memory(&media_path);
            if let Some(physical_path) = physical_observation_path.as_deref() {
                invalidate_media_path_observation_memory(physical_path);
            }
        }
        Ok(changed == 1)
    })();
    match finalization {
        Ok(true) => LibraryFileDeleteResult {
            item_id: item_id.to_string(),
            title: Some(title),
            media_path: Some(media_path),
            outcome: "deleted".to_string(),
            message: if observation == MediaPathObservation::Missing {
                "file was already absent; canonical item is now marked deleted".to_string()
            } else if mode == "trash" {
                "file moved to the OS Recycle Bin and marked deleted".to_string()
            } else {
                "file permanently removed and marked deleted".to_string()
            },
        },
        Ok(false) => LibraryFileDeleteResult {
            item_id: item_id.to_string(),
            title: Some(title),
            media_path: Some(media_path),
            outcome: "failed".to_string(),
            message: "file was removed, but lifecycle finalization did not update the item"
                .to_string(),
        },
        Err(err) => LibraryFileDeleteResult {
            item_id: item_id.to_string(),
            title: Some(title),
            media_path: Some(media_path),
            outcome: "failed".to_string(),
            message: format!("file was removed, but lifecycle finalization failed: {err}"),
        },
    }
}

pub fn operator_deleted_redownload_target(
    paths: &AppPaths,
    item_id: &str,
) -> Result<OperatorDeletedRedownloadTarget> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    let target = conn
        .query_row(
            r#"
SELECT li.id, li.title, i.canonical_url, li.media_path
FROM library_item li
JOIN media_source_identity i ON i.library_item_id=li.id
WHERE li.id=?1
  AND li.file_status IN ('operator_deleted','delete_pending')
  AND i.service='youtube'
ORDER BY i.updated_at_ms DESC
LIMIT 1
"#,
            [item_id.trim()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .optional()?;
    let Some((item_id, title, source_url, media_path)) = target else {
        return Err(EngineError::InstallFailed(
            "selected item is not an operator-deleted canonical YouTube video".to_string(),
        ));
    };
    let resolved_media_path = resolve_media_path(paths, &media_path)?;
    let output_dir = resolved_media_path
        .parent()
        .ok_or_else(|| {
            EngineError::InstallFailed("deleted video path has no parent folder".to_string())
        })?
        .to_string_lossy()
        .to_string();
    Ok(OperatorDeletedRedownloadTarget {
        item_id,
        title,
        source_url,
        output_dir,
    })
}

const THUMB_CACHE_MAX_BYTES: u64 = 512 * 1024 * 1024;
const THUMB_CACHE_MAX_AGE_DAYS: i64 = 45;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryItem {
    pub id: String,
    pub created_at_ms: i64,
    pub source_type: String,
    pub source_uri: String,
    pub title: String,
    pub media_path: String,
    pub duration_ms: Option<i64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub container: Option<String>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub thumbnail_path: Option<String>,
    #[serde(default = "default_library_file_status")]
    pub file_status: String,
    pub file_status_changed_at_ms: Option<i64>,
    pub file_status_change_source: Option<String>,
    pub file_delete_method: Option<String>,
    pub file_redownload_authorized_job_id: Option<String>,
    /// Durable direct-download classification. `None` is intentionally unknown, never inferred
    /// from a media path or source URL by library consumers.
    pub lineage_service: Option<String>,
    pub lineage_origin_kind: Option<String>,
    pub lineage_work_track: Option<String>,
    /// Service used by the canonical Media Library source filter. Download lineage wins, then an
    /// exact linked source identity. `None` means local/unclassified; it is never inferred from a
    /// storage-folder name.
    #[serde(default)]
    pub canonical_service: Option<String>,
}

fn default_library_file_status() -> String {
    "available".to_string()
}

/// Canonical classification captured at the direct-download execution boundary.
///
/// These values are stored with the resulting library item because terminal job rows are cleaned
/// up over time. Consumers must treat a missing lineage row as unknown, not as a single video.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DownloadLineageClassification {
    pub service: String,
    pub origin_kind: String,
    pub work_track: String,
}

#[derive(Debug, Clone)]
pub struct DownloadLineageInput {
    pub item_id: String,
    pub source_job_id: String,
    pub source_batch_id: Option<String>,
    pub source_subscription_id: Option<String>,
    pub classification: DownloadLineageClassification,
    pub item_created_at_ms: i64,
}

/// Source context required when a completed download first enters the library. The item ID and
/// creation timestamp are generated by the import itself, then the library item, ingest
/// provenance, source-job link, and canonical lineage are committed in one SQLite transaction.
#[derive(Debug, Clone)]
pub struct DownloadedFileLineageInput {
    pub source_job_id: String,
    pub source_batch_id: Option<String>,
    pub source_subscription_id: Option<String>,
    pub classification: DownloadLineageClassification,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadLineageBackfillState {
    pub complete: bool,
    pub has_more: bool,
    pub cursor_job_rowid: i64,
    pub remaining_candidates: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoutubeSingleHistoryPage {
    pub canonical_total: usize,
    /// Canonical items after the supplied plain-text search has been applied.
    pub filtered_total: usize,
    /// Loaded independently because proving this diagnostic count requires a full legacy-library
    /// scan. `None` keeps the primary canonical history page bounded and immediately usable.
    pub unclassified_total: Option<usize>,
    pub items: Vec<LibraryItem>,
    pub backfill: DownloadLineageBackfillState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryPage {
    /// Number of canonical rows matching the supplied backend predicates, independent of page size.
    pub filtered_total: usize,
    pub items: Vec<LibraryItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryItemTransferSummary {
    pub source_library_id: String,
    pub target_library_id: String,
    pub mode: String,
    pub items_matched: usize,
    pub items_copied: usize,
    pub items_moved: usize,
}

fn library_item_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<LibraryItem> {
    Ok(LibraryItem {
        id: row.get(0)?,
        created_at_ms: row.get(1)?,
        source_type: row.get(2)?,
        source_uri: row.get(3)?,
        title: row.get(4)?,
        media_path: row.get(5)?,
        duration_ms: row.get(6)?,
        width: row.get(7)?,
        height: row.get(8)?,
        container: row.get(9)?,
        video_codec: row.get(10)?,
        audio_codec: row.get(11)?,
        thumbnail_path: row.get(12)?,
        file_status: default_library_file_status(),
        file_status_changed_at_ms: None,
        file_status_change_source: None,
        file_delete_method: None,
        file_redownload_authorized_job_id: None,
        lineage_service: None,
        lineage_origin_kind: None,
        lineage_work_track: None,
        canonical_service: None,
    })
}

fn library_item_from_lifecycle_lineage_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<LibraryItem> {
    let mut item = library_item_from_row(row)?;
    item.lineage_service = row.get(13)?;
    item.lineage_origin_kind = row.get(14)?;
    item.lineage_work_track = row.get(15)?;
    item.file_status = row.get(16)?;
    item.file_status_changed_at_ms = row.get(17)?;
    item.file_status_change_source = row.get(18)?;
    item.file_delete_method = row.get(19)?;
    item.file_redownload_authorized_job_id = row.get(20)?;
    Ok(item)
}

fn library_item_from_library_page_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<LibraryItem> {
    let mut item = library_item_from_lifecycle_lineage_row(row)?;
    item.canonical_service = row.get(21)?;
    Ok(item)
}

fn library_item_from_lifecycle_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<LibraryItem> {
    let mut item = library_item_from_row(row)?;
    item.file_status = row.get(13)?;
    item.file_status_changed_at_ms = row.get(14)?;
    item.file_status_change_source = row.get(15)?;
    item.file_delete_method = row.get(16)?;
    item.file_redownload_authorized_job_id = row.get(17)?;
    Ok(item)
}

fn library_item_from_lineage_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<LibraryItem> {
    let mut item = library_item_from_row(row)?;
    item.lineage_service = row.get(13)?;
    item.lineage_origin_kind = row.get(14)?;
    item.lineage_work_track = row.get(15)?;
    Ok(item)
}

fn path_key(value: &str) -> String {
    let mut normalized = value
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_lowercase();
    if let Some(stripped) = normalized.strip_prefix("//?/") {
        normalized = stripped.to_string();
    }
    normalized
}

fn path_is_under_root(path: &str, root: &str) -> bool {
    let path = path_key(path);
    let root = path_key(root);
    path == root || path.starts_with(&format!("{root}/"))
}

fn replace_root_prefix(path: &str, source_root: &str, target_root: &str) -> String {
    let normalized_path = path.replace('\\', "/");
    let normalized_source = source_root
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string();
    let normalized_target = target_root
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string();
    let path_key = normalized_path.to_lowercase();
    let source_key = normalized_source.to_lowercase();
    if path_key == source_key {
        return normalized_target;
    }
    if let Some(relative) = normalized_path.get(normalized_source.len()..) {
        return format!("{}{}", normalized_target, relative);
    }
    normalized_path
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThumbnailCacheStatus {
    pub cache_dir: String,
    pub total_bytes: u64,
    pub total_files: usize,
    pub max_bytes: u64,
    pub max_age_days: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThumbnailCacheClearSummary {
    pub removed_entries: usize,
    pub removed_bytes: u64,
}

fn thumbnail_cache_path(paths: &AppPaths, item_id: &str) -> PathBuf {
    paths
        .thumbnail_cache_dir()
        .join(thumbnail_cache_file_name(item_id))
}

fn thumbnail_timestamp_seconds(duration_ms: Option<i64>) -> f64 {
    match duration_ms {
        Some(ms) if ms > 0 => {
            let dur_s = (ms as f64) / 1000.0;
            (dur_s * 0.10).min(5.0).max(0.0)
        }
        _ => 0.0,
    }
}

fn set_item_thumbnail_path(
    paths: &AppPaths,
    item_id: &str,
    thumbnail_path: Option<&Path>,
) -> Result<()> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    let stored = thumbnail_path.map(|value| value.to_string_lossy().to_string());
    conn.execute(
        "UPDATE library_item SET thumbnail_path=?1 WHERE id=?2",
        params![stored, item_id],
    )?;
    Ok(())
}

pub fn ensure_thumbnail_path(paths: &AppPaths, item_id: &str) -> Result<Option<PathBuf>> {
    paths.ensure_dirs()?;
    let item = get_item_by_id(paths, item_id)?;

    if let Some(existing) = item
        .thumbnail_path
        .as_deref()
        .map(PathBuf::from)
        .filter(|path| path.is_file())
    {
        return Ok(Some(existing));
    }

    let thumbnail_path = thumbnail_cache_path(paths, item_id);
    if thumbnail_path.is_file() {
        set_item_thumbnail_path(paths, item_id, Some(&thumbnail_path))?;
        return Ok(Some(thumbnail_path));
    }

    let media_path = resolve_media_path(paths, item.media_path.trim())?;
    if !media_path.is_file() {
        if item.thumbnail_path.is_some() {
            set_item_thumbnail_path(paths, item_id, None)?;
        }
        return Ok(None);
    }

    match ffmpeg::generate_thumbnail(
        paths,
        &media_path,
        &thumbnail_path,
        thumbnail_timestamp_seconds(item.duration_ms),
    ) {
        Ok(()) => {
            set_item_thumbnail_path(paths, item_id, Some(&thumbnail_path))?;
            prune_thumbnail_cache(paths, THUMB_CACHE_MAX_BYTES, THUMB_CACHE_MAX_AGE_DAYS);
            Ok(Some(thumbnail_path))
        }
        Err(crate::EngineError::ExternalToolMissing { .. })
        | Err(crate::EngineError::ExternalToolFailed { .. }) => {
            if thumbnail_path.exists() {
                let _ = std::fs::remove_file(&thumbnail_path);
            }
            if item.thumbnail_path.is_some() {
                set_item_thumbnail_path(paths, item_id, None)?;
            }
            Ok(None)
        }
        Err(_) => {
            if thumbnail_path.exists() {
                let _ = std::fs::remove_file(&thumbnail_path);
            }
            if item.thumbnail_path.is_some() {
                set_item_thumbnail_path(paths, item_id, None)?;
            }
            Ok(None)
        }
    }
}

pub fn list_items(paths: &AppPaths, limit: usize, offset: usize) -> Result<Vec<LibraryItem>> {
    list_items_by_file_status(paths, limit, offset, Some("available"))
}

pub fn list_items_by_file_status(
    paths: &AppPaths,
    limit: usize,
    offset: usize,
    file_status: Option<&str>,
) -> Result<Vec<LibraryItem>> {
    // WP-0224: read-only connection bypasses the job-runner write queue so
    // Library page mount stops blocking behind running jobs. Schema is
    // already migrated by `db::ensure_schema` at startup, so we skip the
    // per-call `migrate()` (it requires a writer anyway).
    let conn = db::open_readonly(paths)?;

    let normalized_status = match file_status.unwrap_or("available").trim() {
        "available" => "available",
        "operator_deleted" | "deleted" => "operator_deleted",
        "all" => "all",
        other => {
            return Err(EngineError::InstallFailed(format!(
                "unsupported library file status filter: {other}"
            )))
        }
    };
    let mut stmt = conn.prepare(
        r#"
SELECT
  library_item.id,
  library_item.created_at_ms,
  library_item.source_type,
  library_item.source_uri,
  library_item.title,
  library_item.media_path,
  library_item.duration_ms,
  library_item.width,
  library_item.height,
  library_item.container,
  library_item.video_codec,
  library_item.audio_codec,
  library_item.thumbnail_path,
  library_download_lineage.service,
  library_download_lineage.origin_kind,
  library_download_lineage.work_track,
  library_item.file_status,
  library_item.file_status_changed_at_ms,
  library_item.file_status_change_source,
  library_item.file_delete_method,
  library_item.file_redownload_authorized_job_id
FROM library_item
LEFT JOIN library_download_lineage ON library_download_lineage.item_id = library_item.id
WHERE
  (?1 = 'all')
  OR (?1 = 'available' AND library_item.file_status = 'available')
  OR (?1 = 'operator_deleted' AND library_item.file_status IN ('operator_deleted', 'delete_pending'))
ORDER BY
  CASE WHEN library_item.file_status = 'available' THEN 0 ELSE 1 END ASC,
  library_item.created_at_ms DESC
LIMIT ?2 OFFSET ?3
"#,
    )?;

    let items = stmt
        .query_map(
            params![normalized_status, limit as i64, offset as i64],
            library_item_from_lifecycle_lineage_row,
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(items)
}

pub fn query_items_page(
    paths: &AppPaths,
    limit: usize,
    offset: usize,
    file_status: Option<&str>,
    query: Option<&str>,
    media_type: Option<&str>,
    source: Option<&str>,
    single_video_only: bool,
    sort_by: Option<&str>,
    direction: Option<&str>,
) -> Result<LibraryPage> {
    let normalized_status = match file_status.unwrap_or("available").trim() {
        "available" => "available",
        "operator_deleted" | "deleted" => "operator_deleted",
        "all" => "all",
        other => {
            return Err(EngineError::InstallFailed(format!(
                "unsupported library file status filter: {other}"
            )))
        }
    };
    let normalized_media_type = match media_type.unwrap_or("all").trim() {
        "all" => "all",
        "video" => "video",
        "image" => "image",
        "audio" => "audio",
        "other" => "other",
        other => {
            return Err(EngineError::InstallFailed(format!(
                "unsupported library media type filter: {other}"
            )))
        }
    };
    let normalized_source = match source.unwrap_or("all").trim() {
        "all" => "all",
        "youtube" => "youtube",
        "instagram" => "instagram",
        "local" => "local",
        other => {
            return Err(EngineError::InstallFailed(format!(
                "unsupported library source filter: {other}"
            )))
        }
    };
    let normalized_sort = match sort_by.unwrap_or("date").trim() {
        "date" => "date",
        "title" => "title",
        other => {
            return Err(EngineError::InstallFailed(format!(
                "unsupported library sort: {other}"
            )))
        }
    };
    let normalized_direction = match direction.unwrap_or("desc").trim() {
        "asc" => "ASC",
        "desc" => "DESC",
        other => {
            return Err(EngineError::InstallFailed(format!(
                "unsupported library sort direction: {other}"
            )))
        }
    };
    let normalized_query = query.unwrap_or_default().trim().to_lowercase();
    let bounded_limit = limit.clamp(1, 1_000);

    // The canonical CTE resolves one service per item without joining membership rows, so a video
    // with many source memberships is still returned exactly once. Structured direct-source
    // fallback is intentionally limited to non-local imports; local/4KVDP items become YouTube only
    // after exact identity enrichment.
    let canonical_cte = r#"
WITH identity_service AS (
  SELECT
    library_item_id,
    CASE
      WHEN SUM(CASE WHEN service='youtube' THEN 1 ELSE 0 END) > 0 THEN 'youtube'
      WHEN SUM(CASE WHEN service='instagram' THEN 1 ELSE 0 END) > 0 THEN 'instagram'
      ELSE MIN(service)
    END AS service
  FROM media_source_identity
  WHERE library_item_id IS NOT NULL
  GROUP BY library_item_id
),
canonical AS (
  SELECT
    li.id,
    li.created_at_ms,
    li.source_type,
    li.source_uri,
    li.title,
    li.media_path,
    li.duration_ms,
    li.width,
    li.height,
    li.container,
    li.video_codec,
    li.audio_codec,
    li.thumbnail_path,
    lineage.service AS lineage_service,
    lineage.origin_kind AS lineage_origin_kind,
    lineage.work_track AS lineage_work_track,
    li.file_status,
    li.file_status_changed_at_ms,
    li.file_status_change_source,
    li.file_delete_method,
    li.file_redownload_authorized_job_id,
    COALESCE(
      lineage.service,
      identity_service.service,
      CASE
        WHEN lower(li.source_type) NOT IN ('local_file', 'import', '4kvdp_import')
          AND (
            instr(lower(li.source_uri), 'youtube.com') > 0
            OR instr(lower(li.source_uri), 'youtu.be') > 0
            OR instr(lower(li.source_type), 'youtube') > 0
          )
          THEN 'youtube'
        WHEN lower(li.source_type) NOT IN ('local_file', 'import', '4kvdp_import')
          AND (
            instr(lower(li.source_uri), 'instagram.com') > 0
            OR instr(lower(li.source_type), 'instagram') > 0
          )
          THEN 'instagram'
        ELSE NULL
      END
    ) AS canonical_service,
    CASE
      WHEN lower(li.media_path) GLOB '*.jpg'
        OR lower(li.media_path) GLOB '*.jpeg'
        OR lower(li.media_path) GLOB '*.png'
        OR lower(li.media_path) GLOB '*.webp'
        OR lower(li.media_path) GLOB '*.gif'
        OR lower(li.media_path) GLOB '*.bmp'
        THEN 'image'
      WHEN lower(li.media_path) GLOB '*.mp3'
        OR lower(li.media_path) GLOB '*.wav'
        OR lower(li.media_path) GLOB '*.flac'
        OR lower(li.media_path) GLOB '*.aac'
        OR lower(li.media_path) GLOB '*.m4a'
        OR lower(li.media_path) GLOB '*.ogg'
        THEN 'audio'
      WHEN li.width IS NOT NULL
        OR li.height IS NOT NULL
        OR COALESCE(li.video_codec, '') <> ''
        OR lower(li.media_path) GLOB '*.mp4'
        OR lower(li.media_path) GLOB '*.mkv'
        OR lower(li.media_path) GLOB '*.webm'
        OR lower(li.media_path) GLOB '*.mov'
        OR lower(li.media_path) GLOB '*.avi'
        THEN 'video'
      WHEN COALESCE(li.audio_codec, '') <> '' THEN 'audio'
      ELSE 'other'
    END AS media_kind
  FROM library_item li
  LEFT JOIN library_download_lineage lineage ON lineage.item_id=li.id
  LEFT JOIN identity_service ON identity_service.library_item_id=li.id
)
"#;
    let where_sql = r#"
WHERE
  (
    (?1='all')
    OR (?1='available' AND file_status='available')
    OR (?1='operator_deleted' AND file_status IN ('operator_deleted', 'delete_pending'))
  )
  AND (
    ?2=''
    OR instr(lower(COALESCE(title, '')), ?2) > 0
    OR instr(lower(COALESCE(media_path, '')), ?2) > 0
    OR instr(lower(COALESCE(source_uri, '')), ?2) > 0
    OR instr(lower(COALESCE(video_codec, '')), ?2) > 0
    OR instr(lower(COALESCE(audio_codec, '')), ?2) > 0
  )
  AND (?3='all' OR media_kind=?3)
  AND (
    ?4='all'
    OR (?4='local' AND canonical_service IS NULL)
    OR canonical_service=?4
  )
  AND (?5=0 OR lineage_origin_kind='single')
"#;
    let order_expression = if normalized_sort == "title" {
        format!("title COLLATE NOCASE {normalized_direction}, id {normalized_direction}")
    } else {
        format!("created_at_ms {normalized_direction}, id {normalized_direction}")
    };
    let count_sql = format!("{canonical_cte} SELECT COUNT(*) FROM canonical {where_sql}");
    let page_sql = format!(
        r#"
{canonical_cte}
SELECT
  id, created_at_ms, source_type, source_uri, title, media_path,
  duration_ms, width, height, container, video_codec, audio_codec, thumbnail_path,
  lineage_service, lineage_origin_kind, lineage_work_track,
  file_status, file_status_changed_at_ms, file_status_change_source,
  file_delete_method, file_redownload_authorized_job_id, canonical_service
FROM canonical
{where_sql}
ORDER BY
  CASE WHEN ?1='all' AND file_status='available' THEN 0
       WHEN ?1='all' THEN 1
       ELSE 0
  END ASC,
  {order_expression}
LIMIT ?6 OFFSET ?7
"#
    );

    let conn = db::open_readonly(paths)?;
    let tx = conn.unchecked_transaction()?;
    let predicate_params = params![
        normalized_status,
        normalized_query,
        normalized_media_type,
        normalized_source,
        if single_video_only { 1_i64 } else { 0_i64 },
    ];
    let filtered_total: i64 = tx.query_row(&count_sql, predicate_params, |row| row.get(0))?;
    let mut stmt = tx.prepare(&page_sql)?;
    let items = stmt
        .query_map(
            params![
                normalized_status,
                normalized_query,
                normalized_media_type,
                normalized_source,
                if single_video_only { 1_i64 } else { 0_i64 },
                bounded_limit as i64,
                offset as i64,
            ],
            library_item_from_library_page_row,
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    tx.commit()?;

    Ok(LibraryPage {
        filtered_total: usize::try_from(filtered_total).unwrap_or(usize::MAX),
        items,
    })
}

pub fn list_subscription_items_by_file_status(
    paths: &AppPaths,
    subscription_id: &str,
    file_status: &str,
    limit: usize,
) -> Result<Vec<LibraryItem>> {
    let normalized_status = match file_status.trim() {
        "available" => "available",
        "operator_deleted" | "deleted" => "operator_deleted",
        other => {
            return Err(EngineError::InstallFailed(format!(
                "unsupported subscription video status filter: {other}"
            )))
        }
    };
    let conn = db::open_readonly(paths)?;
    let mut stmt = conn.prepare(
        r#"
SELECT
  li.id,
  li.created_at_ms,
  li.source_type,
  li.source_uri,
  li.title,
  li.media_path,
  li.duration_ms,
  li.width,
  li.height,
  li.container,
  li.video_codec,
  li.audio_codec,
  li.thumbnail_path,
  lineage.service,
  lineage.origin_kind,
  lineage.work_track,
  li.file_status,
  li.file_status_changed_at_ms,
  li.file_status_change_source,
  li.file_delete_method,
  li.file_redownload_authorized_job_id
FROM media_source_membership membership
JOIN media_source_identity identity
  ON identity.service=membership.service AND identity.media_id=membership.media_id
JOIN library_item li ON li.id=identity.library_item_id
LEFT JOIN library_download_lineage lineage ON lineage.item_id=li.id
WHERE membership.source_subscription_id=?1
  AND (
    (?2='available' AND li.file_status='available')
    OR
    (?2='operator_deleted' AND li.file_status IN ('operator_deleted','delete_pending'))
  )
GROUP BY li.id
ORDER BY li.created_at_ms DESC
LIMIT ?3
"#,
    )?;
    let items = stmt
        .query_map(
            params![subscription_id.trim(), normalized_status, limit as i64],
            library_item_from_lifecycle_lineage_row,
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(items)
}

pub fn list_items_under_roots(paths: &AppPaths, roots: &[String]) -> Result<Vec<LibraryItem>> {
    if roots.is_empty() {
        return Ok(Vec::new());
    }
    let conn = db::open_readonly(paths)?;
    let mut stmt = conn.prepare(
        r#"
SELECT
  id,
  created_at_ms,
  source_type,
  source_uri,
  title,
  media_path,
  duration_ms,
  width,
  height,
  container,
  video_codec,
  audio_codec,
  thumbnail_path
FROM library_item
ORDER BY created_at_ms DESC
"#,
    )?;
    let rows = stmt
        .query_map([], library_item_from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows
        .into_iter()
        .filter(|item| {
            roots
                .iter()
                .any(|root| path_is_under_root(&item.media_path, root))
        })
        .collect())
}

/// WP-0257: bounded, READ-ONLY listing of library items whose `media_path` sits under `dir`.
///
/// Powers the subscription detail pane's "downloaded" list for a single subscription WITHOUT
/// materializing the entire (100k+ row) library the way [`list_items_under_roots`] does. Uses
/// [`db::open_readonly`], a hard `LIMIT`, and never writes or migrates, so it cannot lock the
/// database or block the job runner.
///
/// Path-form handling mirrors [`path_is_under_root`]. Downloaded media is stored canonicalized
/// (Windows verbatim `\\?\C:\...`, UNC as `\\?\UNC\server\share\...`), while a resolved
/// subscription output dir is usually NOT verbatim. We build a `LIKE ... ESCAPE '|'` prefix for
/// each equivalent stored form (plain, forward-slash, `\\?\`, `\\?\UNC\`) with a REQUIRED
/// trailing separator so `.../Foo` can never match sibling `.../Foobar` (which keeps the SQL
/// `LIMIT` honest — every matched row is a genuine child, so the limit is never filled by
/// siblings that would be discarded afterwards). LIKE wildcards are escaped because sanitized
/// folder names contain `_`, and SQLite `LIKE` is ASCII case-insensitive, matching the
/// case-folding `path_is_under_root` performs. Every returned row is still re-verified with
/// `path_is_under_root`, so the SQL prefix can only ever over-select, never return a
/// wrong-folder item.
pub fn list_items_under_dir_bounded(
    paths: &AppPaths,
    dir: &str,
    limit: usize,
) -> Result<Vec<LibraryItem>> {
    let dir_trimmed = dir.trim();
    if dir_trimmed.is_empty() || limit == 0 {
        return Ok(Vec::new());
    }

    let patterns = like_prefix_patterns(dir_trimmed);
    if patterns.is_empty() {
        return Ok(Vec::new());
    }

    let conn = db::open_readonly(paths)?;
    let where_clause = (1..=patterns.len())
        .map(|i| format!("media_path LIKE ?{i} ESCAPE '|'"))
        .collect::<Vec<_>>()
        .join(" OR ");
    let sql = format!(
        r#"
SELECT
  id,
  created_at_ms,
  source_type,
  source_uri,
  title,
  media_path,
  duration_ms,
  width,
  height,
  container,
  video_codec,
  audio_codec,
  thumbnail_path
FROM library_item
WHERE {where_clause}
ORDER BY created_at_ms DESC
LIMIT ?{}
"#,
        patterns.len() + 1
    );

    let mut bind: Vec<rusqlite::types::Value> = patterns
        .into_iter()
        .map(rusqlite::types::Value::Text)
        .collect();
    bind.push(rusqlite::types::Value::Integer(limit as i64));

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(bind), library_item_from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);

    Ok(rows
        .into_iter()
        .filter(|item| path_is_under_root(&item.media_path, dir_trimmed))
        .collect())
}

/// Build SQLite `LIKE ... ESCAPE '|'` prefix patterns matching every equivalent stored form of
/// `dir` followed by a path separator. See [`list_items_under_dir_bounded`] for why each form is
/// needed. The prefix is escaped for LIKE wildcards; the trailing separator is intentionally NOT
/// escaped (it must match a real separator character in the stored path).
fn like_prefix_patterns(dir: &str) -> Vec<String> {
    let base = dir.trim_end_matches(|c| c == '\\' || c == '/');
    if base.is_empty() {
        return Vec::new();
    }

    let backslash = base.replace('/', "\\");
    let forward = base.replace('\\', "/");

    let mut forms: Vec<String> = vec![backslash.clone(), forward];
    // Windows verbatim (extended-length) forms produced by `std::fs::canonicalize`.
    if !backslash.starts_with("\\\\?\\") {
        if let Some(unc_tail) = backslash.strip_prefix("\\\\") {
            // UNC share `\\server\share\...` canonicalizes to `\\?\UNC\server\share\...`.
            forms.push(format!("\\\\?\\UNC\\{unc_tail}"));
        } else {
            // Drive path `C:\...` canonicalizes to `\\?\C:\...`.
            forms.push(format!("\\\\?\\{backslash}"));
        }
    }

    let mut patterns: Vec<String> = Vec::new();
    for form in &forms {
        for sep in ['\\', '/'] {
            let pattern = format!("{}{}%", escape_like_pipe(form), sep);
            if !patterns.contains(&pattern) {
                patterns.push(pattern);
            }
        }
    }
    patterns
}

pub fn list_youtube_video_candidates(
    paths: &AppPaths,
    limit: usize,
    offset: usize,
) -> Result<Vec<LibraryItem>> {
    let conn = db::open_readonly(paths)?;

    let mut stmt = conn.prepare(
        r#"
SELECT
  id,
  created_at_ms,
  source_type,
  source_uri,
  title,
  media_path,
  duration_ms,
  width,
  height,
  container,
  video_codec,
  audio_codec,
  thumbnail_path
FROM library_item
WHERE
  (
    lower(source_uri) LIKE '%youtube.com%'
    OR lower(source_uri) LIKE '%youtu.be%'
    OR lower(source_type) LIKE '%youtube%'
  )
  AND (
    width IS NOT NULL
    OR height IS NOT NULL
    OR video_codec IS NOT NULL
    OR lower(media_path) LIKE '%.mp4'
    OR lower(media_path) LIKE '%.mkv'
    OR lower(media_path) LIKE '%.webm'
    OR lower(media_path) LIKE '%.mov'
  )
ORDER BY created_at_ms DESC
LIMIT ?1 OFFSET ?2
"#,
    )?;

    // WP-0253 Item 2b: page the candidates (was an unbounded 122k-row scan). ORDER BY
    // created_at_ms DESC walks idx_library_item_created newest-first and stops at the
    // page limit, so each page is bounded instead of materializing the whole library.
    let items = stmt
        .query_map(params![limit as i64, offset as i64], library_item_from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(items)
}

const DOWNLOAD_LINEAGE_BACKFILL_CURSOR_KEY: &str =
    "library_download_lineage_backfill_v1_last_job_rowid";
const MAX_DOWNLOAD_LINEAGE_BACKFILL_BATCH: usize = 500;
const MAX_YOUTUBE_SINGLE_HISTORY_PAGE: usize = 500;

#[derive(Debug)]
struct DownloadLineageBackfillCandidate {
    rowid: i64,
    job_id: String,
    item_id: String,
    batch_id: Option<String>,
    params_json: String,
    item_created_at_ms: i64,
}

#[derive(Debug, Deserialize)]
struct StoredDownloadDirectUrlParams {
    url: String,
    #[serde(default)]
    subscription_id: Option<String>,
}

/// Classify a direct-download input from its execution context. This deliberately does not look
/// at destination folders, library titles, or prior UI projections: those are derived state and
/// caused the original single-video history leak.
pub fn classify_direct_download_execution(
    url: &str,
    source_subscription_id: Option<&str>,
) -> Option<DownloadLineageClassification> {
    let parsed = url.trim().parse::<ureq::http::Uri>().ok()?;
    let host = parsed.authority()?.host().to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    let path = parsed.path().to_ascii_lowercase();
    let query = parsed.query().unwrap_or_default().to_ascii_lowercase();
    let is_subscription = source_subscription_id.is_some_and(|id| !id.trim().is_empty());
    let is_youtube = host == "youtube.com"
        || host == "www.youtube.com"
        || host == "m.youtube.com"
        || host == "music.youtube.com"
        || host == "youtu.be"
        || host.ends_with(".youtube.com");

    if is_youtube {
        let origin_kind = if is_subscription {
            "subscription"
        } else if host == "youtu.be"
            || path.starts_with("/watch")
            || path.starts_with("/shorts/")
            || path.starts_with("/live/")
        {
            // A watch URL with a `list` parameter remains a submitted individual video. This
            // mirrors the downloader's existing single-file routing rather than silently
            // converting it to a playlist based on display/query heuristics.
            "single"
        } else if path.starts_with("/playlist")
            || query.split('&').any(|part| part.starts_with("list="))
        {
            "playlist"
        } else if path.starts_with("/@")
            || path.starts_with("/channel/")
            || path.starts_with("/c/")
            || path.starts_with("/user/")
        {
            "channel"
        } else {
            "other"
        };
        return Some(DownloadLineageClassification {
            service: "youtube".to_string(),
            origin_kind: origin_kind.to_string(),
            // Foreground playlist/channel submissions are intentionally still part of the
            // YouTube single-download track. Only `origin_kind=single` enters single history.
            work_track: if is_subscription {
                "youtube_recurring"
            } else {
                "youtube_single"
            }
            .to_string(),
        });
    }

    let is_instagram =
        host == "instagram.com" || host == "www.instagram.com" || host.ends_with(".instagram.com");
    if is_instagram {
        let origin_kind = if is_subscription {
            "subscription"
        } else if path.starts_with("/p/")
            || path.starts_with("/reel/")
            || path.starts_with("/reels/")
            || path.starts_with("/tv/")
        {
            "single"
        } else {
            "profile"
        };
        return Some(DownloadLineageClassification {
            service: "instagram".to_string(),
            origin_kind: origin_kind.to_string(),
            work_track: "instagram".to_string(),
        });
    }

    Some(DownloadLineageClassification {
        service: "other_video".to_string(),
        origin_kind: if is_subscription {
            "subscription"
        } else {
            "single"
        }
        .to_string(),
        work_track: "other_video".to_string(),
    })
}

/// Atomically link a successful direct-download job to its imported library item and capture the
/// execution classification. The lineage insert never overwrites an existing row: first durable
/// evidence wins deterministically, which makes retry/recovery idempotent.
pub fn record_download_lineage(paths: &AppPaths, input: DownloadLineageInput) -> Result<()> {
    let mut conn = db::open(paths)?;
    db::migrate(&conn)?;
    let tx = conn.transaction()?;
    record_download_lineage_in_transaction(&tx, input)?;
    tx.commit()?;
    Ok(())
}

fn record_download_lineage_in_transaction(
    tx: &rusqlite::Transaction<'_>,
    input: DownloadLineageInput,
) -> Result<()> {
    let existing_job_batch: Option<Option<String>> = tx
        .query_row(
            "SELECT batch_id FROM job WHERE id=?1",
            params![&input.source_job_id],
            |row| row.get(0),
        )
        .optional()?;
    let existing_batch_id = existing_job_batch.ok_or_else(|| {
        crate::EngineError::InstallFailed(format!(
            "direct-download source job not found: {}",
            input.source_job_id
        ))
    })?;

    tx.execute(
        "UPDATE job SET item_id=?1 WHERE id=?2",
        params![&input.item_id, &input.source_job_id],
    )?;
    let source_batch_id = input.source_batch_id.or(existing_batch_id);
    tx.execute(
        r#"
INSERT INTO library_download_lineage (
  item_id,
  source_job_id,
  source_batch_id,
  source_subscription_id,
  service,
  origin_kind,
  work_track,
  item_created_at_ms,
  recorded_at_ms
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
ON CONFLICT(item_id) DO NOTHING
"#,
        params![
            &input.item_id,
            &input.source_job_id,
            source_batch_id,
            input.source_subscription_id,
            input.classification.service,
            input.classification.origin_kind,
            input.classification.work_track,
            input.item_created_at_ms,
            now_ms(),
        ],
    )?;
    Ok(())
}

fn download_lineage_backfill_cursor(conn: &rusqlite::Connection) -> Result<i64> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM meta WHERE key=?1",
            params![DOWNLOAD_LINEAGE_BACKFILL_CURSOR_KEY],
            |row| row.get(0),
        )
        .optional()?;
    Ok(raw
        .as_deref()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0)
        .max(0))
}

fn write_download_lineage_backfill_cursor_in_transaction(
    tx: &rusqlite::Transaction<'_>,
    cursor_job_rowid: i64,
) -> Result<()> {
    tx.execute(
        "INSERT INTO meta(key, value) VALUES(?1, ?2) \
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![
            DOWNLOAD_LINEAGE_BACKFILL_CURSOR_KEY,
            cursor_job_rowid.to_string()
        ],
    )?;
    Ok(())
}

fn download_lineage_backfill_state(
    conn: &rusqlite::Connection,
) -> Result<DownloadLineageBackfillState> {
    let cursor_job_rowid = download_lineage_backfill_cursor(conn)?;
    let remaining_candidates: i64 = conn.query_row(
        r#"
SELECT COUNT(*)
FROM job
WHERE rowid > ?1
  AND type='download_direct_url'
  AND status='succeeded'
  AND item_id IS NOT NULL
"#,
        params![cursor_job_rowid],
        |row| row.get(0),
    )?;
    let remaining_candidates = remaining_candidates.max(0) as usize;
    Ok(DownloadLineageBackfillState {
        complete: remaining_candidates == 0,
        has_more: remaining_candidates > 0,
        cursor_job_rowid,
        remaining_candidates,
    })
}

/// Run one bounded, resumable legacy-evidence scan. This is deliberately outside schema
/// migration: it only trusts successful direct-download job rows with a linked library item and
/// structured URL/subscription params; anything else remains explicitly unclassified.
pub fn backfill_download_lineage_batch(
    paths: &AppPaths,
    requested_limit: usize,
) -> Result<DownloadLineageBackfillState> {
    let limit = requested_limit.clamp(1, MAX_DOWNLOAD_LINEAGE_BACKFILL_BATCH);
    let mut conn = db::open(paths)?;
    db::migrate(&conn)?;
    let cursor = download_lineage_backfill_cursor(&conn)?;
    let mut stmt = conn.prepare(
        r#"
SELECT
  job.rowid,
  job.id,
  job.item_id,
  job.batch_id,
  job.params_json,
  library_item.created_at_ms
FROM job
JOIN library_item ON library_item.id = job.item_id
WHERE job.rowid > ?1
  AND job.type='download_direct_url'
  AND job.status='succeeded'
  AND job.item_id IS NOT NULL
ORDER BY job.rowid ASC
LIMIT ?2
"#,
    )?;
    let candidates = stmt
        .query_map(params![cursor, limit as i64], |row| {
            Ok(DownloadLineageBackfillCandidate {
                rowid: row.get(0)?,
                job_id: row.get(1)?,
                item_id: row.get(2)?,
                batch_id: row.get(3)?,
                params_json: row.get(4)?,
                item_created_at_ms: row.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);

    // A single transaction per bounded step prevents a large historical scan from opening and
    // migrating one connection per item. The lineage writes and resume cursor commit together,
    // so a failed step can be retried without a partially advanced cursor.
    let tx = conn.transaction()?;
    let mut advanced_cursor = cursor;
    for candidate in candidates {
        advanced_cursor = candidate.rowid;
        let parsed: StoredDownloadDirectUrlParams =
            match serde_json::from_str(&candidate.params_json) {
                Ok(value) => value,
                Err(_) => continue,
            };
        let Some(classification) =
            classify_direct_download_execution(&parsed.url, parsed.subscription_id.as_deref())
        else {
            continue;
        };
        record_download_lineage_in_transaction(
            &tx,
            DownloadLineageInput {
                item_id: candidate.item_id,
                source_job_id: candidate.job_id,
                source_batch_id: candidate.batch_id,
                source_subscription_id: parsed.subscription_id,
                classification,
                item_created_at_ms: candidate.item_created_at_ms,
            },
        )?;
    }
    if advanced_cursor != cursor {
        write_download_lineage_backfill_cursor_in_transaction(&tx, advanced_cursor)?;
    };
    tx.commit()?;
    download_lineage_backfill_state(&conn)
}

/// Exact count of older YouTube-looking video items that have no durable lineage row yet.
/// This is intentionally separate from the primary history page because the legacy predicates
/// require a full `library_item` scan on existing databases.
pub fn count_youtube_single_unclassified(paths: &AppPaths) -> Result<usize> {
    let conn = db::open_readonly(paths)?;
    let count: i64 = conn.query_row(
        r#"
SELECT COUNT(*)
FROM library_item
WHERE
  (
    lower(source_uri) LIKE '%youtube.com%'
    OR lower(source_uri) LIKE '%youtu.be%'
    OR lower(source_type) LIKE '%youtube%'
  )
  AND (
    width IS NOT NULL
    OR height IS NOT NULL
    OR video_codec IS NOT NULL
    OR lower(media_path) LIKE '%.mp4'
    OR lower(media_path) LIKE '%.mkv'
    OR lower(media_path) LIKE '%.webm'
    OR lower(media_path) LIKE '%.mov'
  )
  AND NOT EXISTS (
    SELECT 1
    FROM library_download_lineage
    WHERE library_download_lineage.item_id = library_item.id
  )
"#,
        [],
        |row| row.get(0),
    )?;
    Ok(count.max(0) as usize)
}

/// Canonical, paged YouTube single-video history. Only the durable lineage row decides whether an
/// item belongs here; subscription, playlist, channel, and unknown legacy items are excluded.
pub fn list_youtube_single_history(
    paths: &AppPaths,
    limit: usize,
    offset: usize,
    query: Option<&str>,
    direction: Option<&str>,
) -> Result<YoutubeSingleHistoryPage> {
    let conn = db::open_readonly(paths)?;
    let search_pattern = query
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("%{}%", escape_like_pipe(&value.to_ascii_lowercase())));
    let order_direction = match direction.map(str::trim) {
        Some(value) if value.eq_ignore_ascii_case("asc") => "ASC",
        _ => "DESC",
    };
    let canonical_total: i64 = conn.query_row(
        r#"
SELECT COUNT(*)
FROM library_download_lineage
WHERE service='youtube' AND origin_kind='single' AND work_track='youtube_single'
"#,
        [],
        |row| row.get(0),
    )?;
    // The lineage predicate is first and is backed by
    // idx_library_download_lineage_service_origin_item. Plain search is intentionally applied
    // only to that canonical candidate set; it is not fuzzy and never reclassifies a row.
    let filtered_total: i64 = conn.query_row(
        r#"
SELECT COUNT(*)
FROM library_download_lineage
JOIN library_item ON library_item.id = library_download_lineage.item_id
WHERE library_download_lineage.service='youtube'
  AND library_download_lineage.origin_kind='single'
  AND library_download_lineage.work_track='youtube_single'
  AND (
    ?1 IS NULL
    OR lower(library_item.title) LIKE ?1 ESCAPE '|'
    OR lower(library_item.source_uri) LIKE ?1 ESCAPE '|'
    OR lower(library_item.media_path) LIKE ?1 ESCAPE '|'
  )
"#,
        params![search_pattern.as_deref()],
        |row| row.get(0),
    )?;
    let page_limit = limit.min(MAX_YOUTUBE_SINGLE_HISTORY_PAGE);
    let page_sql = format!(
        r#"
SELECT
  library_item.id,
  library_item.created_at_ms,
  library_item.source_type,
  library_item.source_uri,
  library_item.title,
  library_item.media_path,
  library_item.duration_ms,
  library_item.width,
  library_item.height,
  library_item.container,
  library_item.video_codec,
  library_item.audio_codec,
  library_item.thumbnail_path,
  library_download_lineage.service,
  library_download_lineage.origin_kind,
  library_download_lineage.work_track
FROM library_download_lineage
JOIN library_item ON library_item.id = library_download_lineage.item_id
WHERE library_download_lineage.service='youtube'
  AND library_download_lineage.origin_kind='single'
  AND library_download_lineage.work_track='youtube_single'
  AND (
    ?1 IS NULL
    OR lower(library_item.title) LIKE ?1 ESCAPE '|'
    OR lower(library_item.source_uri) LIKE ?1 ESCAPE '|'
    OR lower(library_item.media_path) LIKE ?1 ESCAPE '|'
  )
ORDER BY library_download_lineage.item_created_at_ms {order_direction}, library_item.id {order_direction}
LIMIT ?2 OFFSET ?3
"#
    );
    let mut stmt = conn.prepare(&page_sql)?;
    let items = stmt
        .query_map(
            params![search_pattern.as_deref(), page_limit as i64, offset as i64],
            library_item_from_lineage_row,
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(YoutubeSingleHistoryPage {
        canonical_total: canonical_total.max(0) as usize,
        filtered_total: filtered_total.max(0) as usize,
        unclassified_total: None,
        items,
        backfill: download_lineage_backfill_state(&conn)?,
    })
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct FallbackResyncReport {
    pub configured_root: String,
    pub configured_reachable: bool,
    pub considered: usize,
    pub moved: usize,
    pub skipped_existing_target: usize,
    pub skipped_missing_source: usize,
    pub errors: usize,
    pub manifest_path: Option<String>,
}

fn sha256_of_file(path: &Path) -> std::io::Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    };
    Ok(format!("{:x}", hasher.finalize()))
}

/// Escape a literal string for use in a SQLite `LIKE ... ESCAPE '|'` pattern. `|` is a
/// reserved Windows path char so it can never appear in a real media path.
fn escape_like_pipe(value: &str) -> String {
    value
        .replace('|', "||")
        .replace('%', "|%")
        .replace('_', "|_")
}

/// WP-0253 Item 2d: when the configured download root (e.g. a NAS share) is reachable
/// again, move items saved to the local fallback during an outage back onto it. STRICTLY
/// SAFE order: copy -> verify (size + sha256) -> relink the DB -> delete the local copy
/// ONLY after a verified copy + relink. Never overwrites an existing file on the root;
/// writes a timestamped manifest of every action. No-op when the root is still unreachable
/// or nothing fell back, so it is safe to call on every startup.
pub fn resync_local_fallback_downloads(paths: &AppPaths) -> Result<FallbackResyncReport> {
    let mut report = FallbackResyncReport::default();

    let fallback_root = match paths.local_fallback_download_dir().canonicalize() {
        Ok(p) => p,
        Err(_) => return Ok(report), // no fallback dir => nothing was ever saved locally
    };
    let configured = paths
        .effective_download_dir()
        .map_err(|e| crate::EngineError::InstallFailed(e.to_string()))?;
    report.configured_root = configured.to_string_lossy().to_string();

    // Only act when the configured root is actually reachable (NAS back up).
    if !crate::paths::download_root_reachable(&configured, std::time::Duration::from_secs(5)) {
        return Ok(report);
    }
    report.configured_reachable = true;

    let mut conn = db::open(paths)?;
    db::migrate(&conn)?;

    let prefix = fallback_root.to_string_lossy().to_string();
    let pattern = format!("{}%", escape_like_pipe(&prefix));
    let rows: Vec<(String, String)> = {
        let mut stmt = conn.prepare(
            "SELECT id, media_path FROM library_item WHERE media_path LIKE ?1 ESCAPE '|'",
        )?;
        let mapped = stmt.query_map(params![pattern], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        mapped.collect::<rusqlite::Result<Vec<_>>>()?
    };
    report.considered = rows.len();
    if rows.is_empty() {
        return Ok(report);
    }

    let mut manifest = Vec::<String>::new();
    for (id, media_path) in rows {
        let src = PathBuf::from(&media_path);
        if !src.is_file() {
            report.skipped_missing_source += 1;
            manifest.push(format!("MISSING_SOURCE\t{id}\t{media_path}"));
            continue;
        }
        let rel = match src.strip_prefix(&fallback_root) {
            Ok(rel) => rel.to_path_buf(),
            Err(_) => {
                report.errors += 1;
                manifest.push(format!("REL_FAIL\t{id}\t{media_path}"));
                continue;
            }
        };
        let target = configured.join(&rel);
        if target.exists() {
            // Never overwrite an existing file on the configured root.
            report.skipped_existing_target += 1;
            manifest.push(format!("TARGET_EXISTS\t{id}\t{}", target.to_string_lossy()));
            continue;
        }

        let copy_result: std::io::Result<()> = (|| {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let tmp = target.with_extension("vvresynctmp");
            let _ = std::fs::remove_file(&tmp);
            std::fs::copy(&src, &tmp)?;
            let src_len = std::fs::metadata(&src)?.len();
            let tmp_len = std::fs::metadata(&tmp)?.len();
            if src_len != tmp_len {
                let _ = std::fs::remove_file(&tmp);
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "size mismatch after copy",
                ));
            }
            if sha256_of_file(&src)? != sha256_of_file(&tmp)? {
                let _ = std::fs::remove_file(&tmp);
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "sha256 mismatch after copy",
                ));
            }
            std::fs::rename(&tmp, &target)?;
            Ok(())
        })();

        match copy_result {
            Ok(()) => {
                let target_str = target.to_string_lossy().to_string();
                // Relink the DB BEFORE deleting the local copy, so the item always points
                // at the verified copy on the configured root even if the delete fails.
                let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
                tx.execute(
                    "UPDATE library_item SET media_path=?1 WHERE id=?2",
                    params![&target_str, id],
                )?;
                persist_media_path_observation_rewrite_invalidation(
                    &tx,
                    &media_path,
                    &target_str,
                )?;
                tx.commit()?;
                invalidate_media_path_observation_rewrite_memory(&media_path, &target_str);
                let _ = std::fs::remove_file(&src);
                report.moved += 1;
                manifest.push(format!("MOVED\t{id}\t{media_path}\t->\t{target_str}"));
            }
            Err(e) => {
                report.errors += 1;
                manifest.push(format!("ERROR\t{id}\t{media_path}\t{e}"));
            }
        }
    }

    // Timestamped manifest of every action (kept regardless of success for audit).
    let manifest_dir = paths.cache_dir().join("fallback_resync");
    if std::fs::create_dir_all(&manifest_dir).is_ok() {
        let manifest_path = manifest_dir.join(format!("resync_{}.log", now_ms()));
        if std::fs::write(&manifest_path, manifest.join("\n")).is_ok() {
            report.manifest_path = Some(manifest_path.to_string_lossy().to_string());
        }
    }

    Ok(report)
}

pub fn upsert_item_metadata(paths: &AppPaths, item: &LibraryItem) -> Result<()> {
    let mut conn = db::open(paths)?;
    db::migrate(&conn)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let previous_path = tx
        .query_row(
            "SELECT media_path FROM library_item WHERE id=?1",
            [&item.id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    tx.execute(
        r#"
INSERT INTO library_item (
  id,
  created_at_ms,
  source_type,
  source_uri,
  title,
  media_path,
  duration_ms,
  width,
  height,
  container,
  video_codec,
  audio_codec,
  thumbnail_path
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
ON CONFLICT(id) DO UPDATE SET
  created_at_ms = excluded.created_at_ms,
  source_type = excluded.source_type,
  source_uri = excluded.source_uri,
  title = excluded.title,
  media_path = excluded.media_path,
  duration_ms = excluded.duration_ms,
  width = excluded.width,
  height = excluded.height,
  container = excluded.container,
  video_codec = excluded.video_codec,
  audio_codec = excluded.audio_codec,
  thumbnail_path = excluded.thumbnail_path
"#,
        params![
            &item.id,
            item.created_at_ms,
            &item.source_type,
            &item.source_uri,
            &item.title,
            &item.media_path,
            item.duration_ms,
            item.width,
            item.height,
            &item.container,
            &item.video_codec,
            &item.audio_codec,
            &item.thumbnail_path,
        ],
    )?;
    match previous_path.as_deref() {
        Some(previous_path) => persist_media_path_observation_rewrite_invalidation(
            &tx,
            previous_path,
            &item.media_path,
        )?,
        None => persist_media_path_observation_invalidation(&tx, &item.media_path)?,
    };
    tx.commit()?;
    match previous_path {
        Some(previous_path) => invalidate_media_path_observation_rewrite_memory(
            &previous_path,
            &item.media_path,
        ),
        None => invalidate_media_path_observation_memory(&item.media_path),
    };
    Ok(())
}

pub fn transfer_item_metadata_between_roots(
    paths: &AppPaths,
    source_library_id: &str,
    source_root: &str,
    target_library_id: &str,
    target_root: &str,
    copy: bool,
) -> Result<LibraryItemTransferSummary> {
    let items = list_items_under_roots(paths, &[source_root.to_string()])?;
    let mut conn = db::open(paths)?;
    db::migrate(&conn)?;
    let mut copied = 0_usize;
    let mut moved = 0_usize;
    for item in &items {
        let target_path = replace_root_prefix(&item.media_path, source_root, target_root);
        if copy {
            let mut copied_item = item.clone();
            copied_item.id = Uuid::new_v4().to_string();
            copied_item.media_path = target_path;
            copied_item.thumbnail_path = None;
            upsert_item_metadata(paths, &copied_item)?;
            copied = copied.saturating_add(1);
        } else {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            tx.execute(
                "UPDATE library_item SET media_path = ?1, thumbnail_path = NULL WHERE id = ?2",
                params![&target_path, &item.id],
            )?;
            persist_media_path_observation_rewrite_invalidation(
                &tx,
                &item.media_path,
                &target_path,
            )?;
            tx.commit()?;
            invalidate_media_path_observation_rewrite_memory(&item.media_path, &target_path);
            moved = moved.saturating_add(1);
        }
    }

    Ok(LibraryItemTransferSummary {
        source_library_id: source_library_id.to_string(),
        target_library_id: target_library_id.to_string(),
        mode: if copy { "copy" } else { "move" }.to_string(),
        items_matched: items.len(),
        items_copied: copied,
        items_moved: moved,
    })
}

pub fn list_localization_workspace_items(
    paths: &AppPaths,
    limit: usize,
    offset: usize,
) -> Result<Vec<LibraryItem>> {
    // WP-0224: read-only connection (see list_items above).
    let conn = db::open_readonly(paths)?;

    let mut stmt = conn.prepare(
        r#"
SELECT
  library_item.id,
  library_item.created_at_ms,
  library_item.source_type,
  library_item.source_uri,
  library_item.title,
  library_item.media_path,
  library_item.duration_ms,
  library_item.width,
  library_item.height,
  library_item.container,
  library_item.video_codec,
  library_item.audio_codec,
  library_item.thumbnail_path
FROM localization_workspace_item
JOIN library_item ON library_item.id = localization_workspace_item.item_id
ORDER BY localization_workspace_item.selected_at_ms DESC, library_item.created_at_ms DESC
LIMIT ?1 OFFSET ?2
"#,
    )?;

    let items = stmt
        .query_map(params![limit as i64, offset as i64], library_item_from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(items)
}

pub fn get_item_by_id(paths: &AppPaths, item_id: &str) -> Result<LibraryItem> {
    // WP-0226: read-only connection bypasses job-runner write queue.
    let conn = db::open_readonly(paths)?;

    conn.query_row(
        r#"
SELECT
  id,
  created_at_ms,
  source_type,
  source_uri,
  title,
  media_path,
  duration_ms,
  width,
  height,
  container,
  video_codec,
  audio_codec,
  thumbnail_path,
  file_status,
  file_status_changed_at_ms,
  file_status_change_source,
  file_delete_method,
  file_redownload_authorized_job_id
FROM library_item
WHERE id=?1
"#,
        params![item_id],
        library_item_from_lifecycle_row,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => {
            crate::EngineError::InstallFailed(format!("library item not found: {item_id}"))
        }
        other => crate::EngineError::Database(other),
    })
}

/// WP-0245: batched read for the Jobs page and other panels that previously
/// fanned out per-item `get_item_by_id` calls. One read-only connection, one
/// `SELECT … WHERE id IN (...)`. Order of the returned vec is not guaranteed
/// to match input order; callers should index by `LibraryItem.id`. Missing
/// ids are silently skipped (no error). Caller is expected to bound the input
/// length; we still defensively cap at 500 to stay clear of SQLite's default
/// 999 bound-parameter limit.
pub fn list_items_by_ids(paths: &AppPaths, ids: &[&str]) -> Result<Vec<LibraryItem>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    const MAX_IDS: usize = 500;
    let ids = if ids.len() > MAX_IDS {
        &ids[..MAX_IDS]
    } else {
        ids
    };

    let conn = db::open_readonly(paths)?;
    let placeholders = (0..ids.len()).map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT id, created_at_ms, source_type, source_uri, title, media_path, \
         duration_ms, width, height, container, video_codec, audio_codec, thumbnail_path \
         FROM library_item WHERE id IN ({placeholders})"
    );
    let mut stmt = conn.prepare(&sql)?;
    let items = stmt
        .query_map(
            rusqlite::params_from_iter(ids.iter()),
            library_item_from_row,
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(items)
}

pub fn get_item_by_canonical_media_path(
    paths: &AppPaths,
    media_path: &Path,
) -> Result<Option<LibraryItem>> {
    let canonical = media_path.canonicalize()?;
    let media_path_str = canonical.to_string_lossy().to_string();
    let supplied_path_str = media_path.to_string_lossy().to_string();

    let conn = db::open(paths)?;
    db::migrate(&conn)?;

    let mut statement = conn.prepare_cached(
        r#"
SELECT
  id,
  created_at_ms,
  source_type,
  source_uri,
  title,
  media_path,
  duration_ms,
  width,
  height,
  container,
  video_codec,
  audio_codec,
  thumbnail_path
FROM library_item
WHERE media_path=?1 COLLATE NOCASE
ORDER BY created_at_ms DESC
LIMIT 1
"#,
    )?;
    let mut physical_candidates = vec![media_path_str.clone()];
    if !media_path_str.eq_ignore_ascii_case(&supplied_path_str) {
        physical_candidates.push(supplied_path_str);
    }
    for candidate in &physical_candidates {
        if let Some(item) = statement
            .query_row([candidate], library_item_from_row)
            .optional()?
        {
            return Ok(Some(item));
        }
    }

    // A direct-root rebind intentionally preserves historical DB identity. Invert the bounded
    // one-hop alias set and perform case-insensitive exact lookups through
    // idx_library_item_media_path. Never enumerate/canonicalize the full library: on a large NAS
    // that turned one dedupe check into 140k filesystem probes.
    let aliases = crate::root_rebind::load_root_aliases(paths)?;
    if aliases.aliases.is_empty() {
        return Ok(None);
    }
    let mut historical_candidates = Vec::new();
    for candidate in &physical_candidates {
        historical_candidates.extend(crate::root_rebind::historical_alias_candidates(
            &aliases.aliases,
            candidate,
        )?);
    }
    historical_candidates.sort_by_key(|value| value.to_ascii_lowercase());
    historical_candidates.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    for candidate in historical_candidates {
        if let Some(item) = statement
            .query_row([candidate], library_item_from_row)
            .optional()?
        {
            return Ok(Some(item));
        }
    }

    Ok(None)
}

pub fn add_item_to_localization_workspace(
    paths: &AppPaths,
    item_id: &str,
    selection_source: &str,
    selection_path: Option<&str>,
) -> Result<()> {
    let item_id = item_id.trim();
    let selection_source = selection_source.trim();
    if item_id.is_empty() {
        return Err(crate::EngineError::InstallFailed(
            "item_id is required for localization workspace".to_string(),
        ));
    }
    if selection_source.is_empty() {
        return Err(crate::EngineError::InstallFailed(
            "selection_source is required for localization workspace".to_string(),
        ));
    }

    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    conn.execute(
        r#"
INSERT INTO localization_workspace_item (
  item_id,
  selected_at_ms,
  selection_source,
  selection_path
) VALUES (?1, ?2, ?3, ?4)
ON CONFLICT(item_id) DO UPDATE SET
  selected_at_ms=excluded.selected_at_ms,
  selection_source=excluded.selection_source,
  selection_path=excluded.selection_path
"#,
        params![
            item_id,
            now_ms(),
            selection_source,
            selection_path
                .map(|value| value.trim())
                .filter(|value| !value.is_empty()),
        ],
    )?;
    Ok(())
}

pub fn import_local_file(paths: &AppPaths, input_path: &Path) -> Result<LibraryItem> {
    let input_path = input_path.canonicalize()?;
    let source_uri = input_path.to_string_lossy().to_string();
    import_media_file(paths, &input_path, "local_file", &source_uri, None)
}

/// Import a completed direct download and publish every database-side identity/provenance row as
/// one atomic handoff. A crash or SQLite error cannot leave a visible library item without the
/// canonical lineage required by single-video history and scheduler-track consumers.
pub fn import_downloaded_file_with_lineage(
    paths: &AppPaths,
    downloaded_path: &Path,
    source_url: &str,
    rights_note: &str,
    provider: &str,
    attested_at_ms: i64,
    lineage: DownloadedFileLineageInput,
) -> Result<LibraryItem> {
    let downloaded_path = downloaded_path.canonicalize()?;
    let source_url = source_url.trim();
    let rights_note = rights_note.trim();
    let provider = provider.trim();
    let mut item = prepare_media_item(paths, &downloaded_path, "url_direct", source_url, None)?;

    let mut conn = db::open(paths)?;
    db::migrate(&conn)?;
    let canonical_source = canonical_media_source(source_url);
    if let Some(source) = canonical_source.as_ref() {
        ensure_source_identity_conn(&conn, source, source_url)?;
        if let Some((existing_id, existing_created_at)) = conn
            .query_row(
                r#"
SELECT li.id, li.created_at_ms
FROM media_source_identity i
JOIN library_item li ON li.id=i.library_item_id
WHERE i.service=?1 AND i.media_id=?2
"#,
                params![source.service, source.media_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?
        {
            item.id = existing_id;
            item.created_at_ms = existing_created_at;
        }
    }
    let tx = conn.transaction()?;
    let previous_media_path = tx
        .query_row(
            "SELECT media_path FROM library_item WHERE id=?1",
            [&item.id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if previous_media_path.is_some() {
        tx.execute(
            r#"
UPDATE library_item SET
  source_type=?1, source_uri=?2, title=?3, media_path=?4, duration_ms=?5,
  width=?6, height=?7, container=?8, video_codec=?9, audio_codec=?10,
  thumbnail_path=COALESCE(?11, thumbnail_path)
WHERE id=?12
"#,
            params![
                item.source_type,
                item.source_uri,
                item.title,
                item.media_path,
                item.duration_ms,
                item.width,
                item.height,
                item.container,
                item.video_codec,
                item.audio_codec,
                item.thumbnail_path,
                item.id,
            ],
        )?;
    } else {
        insert_library_item(&tx, &item)?;
    }
    let restored_operator_deleted = tx.execute(
        "UPDATE library_item SET file_status='available', file_status_changed_at_ms=?1, \
         file_status_change_source='authorized_redownload_completed', file_delete_method=NULL, \
         file_redownload_authorized_job_id=NULL \
         WHERE id=?2 AND file_status IN ('operator_deleted','delete_pending') \
           AND file_redownload_authorized_job_id=?3",
        params![now_ms(), item.id, lineage.source_job_id],
    )? == 1;
    tx.execute(
        r#"
INSERT INTO ingest_provenance (
  item_id,
  provider,
  source_url,
  rights_note,
  attested_at_ms,
  created_at_ms
) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
ON CONFLICT(item_id) DO UPDATE SET
  provider=excluded.provider,
  source_url=excluded.source_url,
  rights_note=excluded.rights_note,
  attested_at_ms=excluded.attested_at_ms,
  created_at_ms=excluded.created_at_ms
"#,
        params![
            &item.id,
            provider,
            source_url,
            rights_note,
            attested_at_ms,
            now_ms(),
        ],
    )?;

    record_download_lineage_in_transaction(
        &tx,
        DownloadLineageInput {
            item_id: item.id.clone(),
            source_job_id: lineage.source_job_id.clone(),
            source_batch_id: lineage.source_batch_id,
            source_subscription_id: lineage.source_subscription_id.clone(),
            classification: lineage.classification.clone(),
            item_created_at_ms: item.created_at_ms,
        },
    )?;
    if let Some(source) = canonical_source.as_ref() {
        tx.execute(
            r#"
UPDATE media_source_identity SET
  library_item_id=?1, active_job_id=NULL, repair_state='ready',
  canonical_url=?2, last_failed_url=NULL, last_error=NULL, updated_at_ms=?3
WHERE service=?4 AND media_id=?5
"#,
            params![
                item.id,
                source_url,
                now_ms(),
                source.service,
                source.media_id
            ],
        )?;
        tx.execute(
            "INSERT OR IGNORE INTO media_source_association (id, service, media_id, origin_kind, source_subscription_id, source_job_id, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                Uuid::new_v4().to_string(),
                source.service,
                source.media_id,
                lineage.classification.origin_kind,
                lineage.source_subscription_id,
                lineage.source_job_id,
                now_ms(),
            ],
        )?;
        upsert_source_membership_conn(
            &tx,
            source,
            lineage.source_subscription_id.as_deref(),
            "voxvulgi_download",
        )?;
    }
    match previous_media_path.as_deref() {
        Some(previous_path) => persist_media_path_observation_rewrite_invalidation(
            &tx,
            previous_path,
            &item.media_path,
        )?,
        None => persist_media_path_observation_invalidation(&tx, &item.media_path)?,
    };
    tx.commit()?;

    match previous_media_path {
        Some(previous_path) => invalidate_media_path_observation_rewrite_memory(
            &previous_path,
            &item.media_path,
        ),
        None => invalidate_media_path_observation_memory(&item.media_path),
    };

    item.lineage_service = Some(lineage.classification.service);
    item.lineage_origin_kind = Some(lineage.classification.origin_kind);
    item.lineage_work_track = Some(lineage.classification.work_track);
    if restored_operator_deleted {
        item.file_status = LIBRARY_FILE_STATUS_AVAILABLE.to_string();
        item.file_status_changed_at_ms = Some(now_ms());
        item.file_status_change_source = Some("authorized_redownload_completed".to_string());
        item.file_delete_method = None;
        item.file_redownload_authorized_job_id = None;
    }

    Ok(item)
}

fn import_media_file(
    paths: &AppPaths,
    media_path: &Path,
    source_type: &str,
    source_uri: &str,
    title_hint: Option<&str>,
) -> Result<LibraryItem> {
    let item = prepare_media_item(paths, media_path, source_type, source_uri, title_hint)?;
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    insert_library_item(&conn, &item)?;
    Ok(item)
}

fn prepare_media_item(
    paths: &AppPaths,
    media_path: &Path,
    source_type: &str,
    source_uri: &str,
    title_hint: Option<&str>,
) -> Result<LibraryItem> {
    let id = Uuid::new_v4().to_string();
    let created_at_ms = now_ms();
    let title = title_hint
        .and_then(|s| {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .or_else(|| {
            media_path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "Untitled".to_string());
    let media_path_str = media_path.to_string_lossy().to_string();

    let derived_dir = paths.derived_item_dir(&id);
    std::fs::create_dir_all(&derived_dir)?;

    // Import should remain possible even when ffmpeg/ffprobe is not installed. Metadata and
    // thumbnails are best-effort.
    let probe = match ffmpeg::probe(paths, media_path) {
        Ok(v) => v,
        Err(crate::EngineError::ExternalToolMissing { .. }) => ffmpeg::MediaProbe {
            duration_ms: None,
            container: None,
            video_codec: None,
            audio_codec: None,
            width: None,
            height: None,
            video_stream_count: 0,
            audio_stream_count: 0,
            audio_streams: Vec::new(),
            subtitle_streams: Vec::new(),
        },
        Err(crate::EngineError::ExternalToolFailed { .. }) => ffmpeg::MediaProbe {
            duration_ms: None,
            container: None,
            video_codec: None,
            audio_codec: None,
            width: None,
            height: None,
            video_stream_count: 0,
            audio_stream_count: 0,
            audio_streams: Vec::new(),
            subtitle_streams: Vec::new(),
        },
        Err(e) => return Err(e),
    };

    let thumbnail_path = thumbnail_cache_path(paths, &id);
    let timestamp_seconds = thumbnail_timestamp_seconds(probe.duration_ms);

    let thumbnail_path_str =
        match ffmpeg::generate_thumbnail(paths, media_path, &thumbnail_path, timestamp_seconds) {
            Ok(()) => Some(thumbnail_path.to_string_lossy().to_string()),
            Err(crate::EngineError::ExternalToolMissing { .. }) => None,
            Err(crate::EngineError::ExternalToolFailed { .. }) => None,
            Err(_) => None,
        };
    prune_thumbnail_cache(paths, THUMB_CACHE_MAX_BYTES, THUMB_CACHE_MAX_AGE_DAYS);

    Ok(LibraryItem {
        id,
        created_at_ms,
        source_type: source_type.to_string(),
        source_uri: source_uri.to_string(),
        title,
        media_path: media_path_str,
        duration_ms: probe.duration_ms,
        width: probe.width,
        height: probe.height,
        container: probe.container,
        video_codec: probe.video_codec,
        audio_codec: probe.audio_codec,
        thumbnail_path: thumbnail_path_str,
        file_status: default_library_file_status(),
        file_status_changed_at_ms: None,
        file_status_change_source: None,
        file_delete_method: None,
        file_redownload_authorized_job_id: None,
        lineage_service: None,
        lineage_origin_kind: None,
        lineage_work_track: None,
        canonical_service: None,
    })
}

fn insert_library_item(conn: &rusqlite::Connection, item: &LibraryItem) -> Result<()> {
    // WP-0253 Item 2c: stamp the unified-library columns at insert so new items are
    // identical in shape to the backfilled legacy/new ones (single library going forward).
    let origin = if item.source_type == "url_direct" {
        "voxvulgi_download"
    } else {
        "local_import"
    };
    conn.execute(
        r#"
INSERT INTO library_item (
  id,
  created_at_ms,
  source_type,
  source_uri,
  title,
  media_path,
  duration_ms,
  width,
  height,
  container,
  video_codec,
  audio_codec,
  thumbnail_path,
  origin,
  library_id
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
  (SELECT id FROM video_library WHERE kind='default' LIMIT 1))
"#,
        params![
            &item.id,
            item.created_at_ms,
            &item.source_type,
            &item.source_uri,
            &item.title,
            &item.media_path,
            item.duration_ms,
            item.width,
            item.height,
            &item.container,
            &item.video_codec,
            &item.audio_codec,
            &item.thumbnail_path,
            origin,
        ],
    )?;
    Ok(())
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

pub fn derived_dir_for_item(paths: &AppPaths, item_id: &str) -> PathBuf {
    paths.derived_item_dir(item_id)
}

pub fn thumbnail_cache_status(paths: &AppPaths) -> Result<ThumbnailCacheStatus> {
    paths.ensure_dirs()?;
    let cache_dir = paths.thumbnail_cache_dir();
    let mut total_bytes = 0_u64;
    let mut total_files = 0_usize;

    if cache_dir.exists() {
        let entries = std::fs::read_dir(&cache_dir)?;
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_file() {
                continue;
            }
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            total_files += 1;
            total_bytes = total_bytes.saturating_add(meta.len());
        }
    }

    Ok(ThumbnailCacheStatus {
        cache_dir: cache_dir.to_string_lossy().to_string(),
        total_bytes,
        total_files,
        max_bytes: THUMB_CACHE_MAX_BYTES,
        max_age_days: THUMB_CACHE_MAX_AGE_DAYS,
    })
}

pub fn clear_thumbnail_cache(paths: &AppPaths) -> Result<ThumbnailCacheClearSummary> {
    paths.ensure_dirs()?;
    let cache_dir = paths.thumbnail_cache_dir();
    if !cache_dir.exists() {
        return Ok(ThumbnailCacheClearSummary {
            removed_entries: 0,
            removed_bytes: 0,
        });
    }

    let mut removed_entries = 0_usize;
    let mut removed_bytes = 0_u64;
    let entries = std::fs::read_dir(&cache_dir)?;
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_file() {
            continue;
        }
        let bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
        if std::fs::remove_file(&path).is_ok() {
            removed_entries += 1;
            removed_bytes = removed_bytes.saturating_add(bytes);
        }
    }

    Ok(ThumbnailCacheClearSummary {
        removed_entries,
        removed_bytes,
    })
}

fn thumbnail_cache_file_name(item_id: &str) -> String {
    let mut out = String::with_capacity(item_id.len());
    for ch in item_id.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    let mut trimmed = out.trim_matches('_').to_string();
    if trimmed.is_empty() {
        trimmed = "item".to_string();
    }
    if trimmed.len() > 80 {
        trimmed.truncate(80);
    }
    format!("{trimmed}.jpg")
}

fn prune_thumbnail_cache(paths: &AppPaths, max_bytes: u64, max_age_days: i64) {
    let cache_dir = paths.thumbnail_cache_dir();
    if !cache_dir.exists() {
        return;
    }

    let now = SystemTime::now();
    let max_age_secs = (max_age_days.max(1) as u64)
        .saturating_mul(24)
        .saturating_mul(60)
        .saturating_mul(60);

    struct Entry {
        path: PathBuf,
        bytes: u64,
        modified: SystemTime,
    }

    let mut entries: Vec<Entry> = Vec::new();
    let mut total_bytes = 0_u64;

    let Ok(iter) = std::fs::read_dir(&cache_dir) else {
        return;
    };
    for entry in iter.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_file() {
            continue;
        }
        let path = entry.path();
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        let modified = meta.modified().unwrap_or(UNIX_EPOCH);
        let age_secs = now
            .duration_since(modified)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if age_secs > max_age_secs {
            let _ = std::fs::remove_file(&path);
            continue;
        }
        let bytes = meta.len();
        total_bytes = total_bytes.saturating_add(bytes);
        entries.push(Entry {
            path,
            bytes,
            modified,
        });
    }

    if total_bytes <= max_bytes {
        return;
    }

    entries.sort_by_key(|entry| entry.modified);
    for entry in entries {
        if total_bytes <= max_bytes {
            break;
        }
        if std::fs::remove_file(&entry.path).is_ok() {
            total_bytes = total_bytes.saturating_sub(entry.bytes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use filetime::{set_file_mtime, FileTime};
    use rusqlite::params;

    fn seed_present_observation(paths: &AppPaths, path: &str) {
        let conn = db::open(paths).expect("observation db");
        conn.execute(
            "INSERT INTO media_availability_observation(path,state,observed_at_ms,source,duration_ms,next_refresh_at_ms,invalidated_at_ms) VALUES(?1,'present',1,'fixture',1,9999999999999,NULL) ON CONFLICT(path) DO UPDATE SET state='present',observed_at_ms=1,source='fixture',duration_ms=1,next_refresh_at_ms=9999999999999,invalidated_at_ms=NULL",
            [path],
        )
        .expect("seed observation");
        cache_media_path_observation(path, MediaPathObservation::Present);
    }

    fn assert_observation_invalidated(paths: &AppPaths, path: &str) {
        assert!(
            !media_path_observations()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .contains_key(path),
            "in-memory observation remained eligible for {path}"
        );
        let conn = db::open_readonly(paths).expect("readonly observation db");
        let invalidated_at_ms = conn
            .query_row(
                "SELECT invalidated_at_ms FROM media_availability_observation WHERE path=?1",
                [path],
                |row| row.get::<_, Option<i64>>(0),
            )
            .expect("stored observation");
        assert!(
            invalidated_at_ms.is_some(),
            "persistent observation remained eligible for {path}"
        );
    }

    fn seed_lineage_item(
        paths: &AppPaths,
        id: &str,
        created_at_ms: i64,
        source_uri: &str,
        title: &str,
        media_path: &str,
    ) {
        let conn = db::open(paths).expect("open db");
        db::migrate(&conn).expect("migrate");
        conn.execute(
            r#"
INSERT INTO library_item (
  id, created_at_ms, source_type, source_uri, title, media_path,
  duration_ms, width, height, container, video_codec, audio_codec, thumbnail_path
) VALUES (?1, ?2, 'url_direct', ?3, ?4, ?5, NULL, 1920, 1080, 'mp4', 'h264', NULL, NULL)
"#,
            params![id, created_at_ms, source_uri, title, media_path],
        )
        .expect("seed library item");
    }

    fn seed_direct_success_job(
        paths: &AppPaths,
        id: &str,
        item_id: Option<&str>,
        batch_id: Option<&str>,
        url: &str,
        subscription_id: Option<&str>,
    ) {
        let params_json = serde_json::json!({
            "url": url,
            "subscription_id": subscription_id,
        })
        .to_string();
        let conn = db::open(paths).expect("open db");
        db::migrate(&conn).expect("migrate");
        conn.execute(
            r#"
INSERT INTO job (
  id, item_id, batch_id, type, status, progress, error, params_json,
  created_at_ms, started_at_ms, finished_at_ms, logs_path
) VALUES (?1, ?2, ?3, 'download_direct_url', 'succeeded', 1.0, NULL, ?4, 1, 1, 2, ?5)
"#,
            params![
                id,
                item_id,
                batch_id,
                params_json,
                paths
                    .job_logs_dir()
                    .join(format!("{id}.jsonl"))
                    .to_string_lossy()
                    .to_string(),
            ],
        )
        .expect("seed direct success job");
    }

    #[test]
    fn direct_download_classification_keeps_foreground_collections_out_of_single_history() {
        let single =
            classify_direct_download_execution("https://www.youtube.com/shorts/short-id", None)
                .expect("shorts classification");
        assert_eq!(single.service, "youtube");
        assert_eq!(single.origin_kind, "single");
        assert_eq!(single.work_track, "youtube_single");

        let playlist =
            classify_direct_download_execution("https://www.youtube.com/playlist?list=PL123", None)
                .expect("playlist classification");
        assert_eq!(playlist.origin_kind, "playlist");
        assert_eq!(playlist.work_track, "youtube_single");

        let channel = classify_direct_download_execution("https://www.youtube.com/@voxvulgi", None)
            .expect("channel classification");
        assert_eq!(channel.origin_kind, "channel");
        assert_eq!(channel.work_track, "youtube_single");

        let subscription = classify_direct_download_execution(
            "https://www.youtube.com/watch?v=sub-id",
            Some("subscription-id"),
        )
        .expect("subscription classification");
        assert_eq!(subscription.origin_kind, "subscription");
        assert_eq!(subscription.work_track, "youtube_recurring");

        let instagram =
            classify_direct_download_execution("https://www.instagram.com/reel/ABC123/", None)
                .expect("instagram classification");
        assert_eq!(instagram.service, "instagram");
        assert_eq!(instagram.work_track, "instagram");
    }

    #[test]
    fn lineage_backfill_is_bounded_resumable_and_preserves_canonical_history_after_job_cleanup() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        db::ensure_schema(&paths).expect("schema");

        // The mapped NAS path is deliberately not usable as a classifier. Only successful job
        // context decides the result.
        seed_lineage_item(
            &paths,
            "single-item",
            30,
            "https://www.youtube.com/watch?v=single-id",
            "Wanted single title",
            r"\\?\UNC\nas\media\wanted_single.mp4",
        );
        seed_lineage_item(
            &paths,
            "subscription-item",
            20,
            "https://www.youtube.com/shorts/sub-id",
            "Subscription short",
            r"\\?\UNC\nas\media\subscription_short.mp4",
        );
        seed_lineage_item(
            &paths,
            "playlist-item",
            10,
            "https://www.youtube.com/playlist?list=PL123",
            "Playlist result",
            r"\\?\UNC\nas\media\playlist_result.mp4",
        );
        seed_lineage_item(
            &paths,
            "legacy-unknown",
            5,
            "https://www.youtube.com/watch?v=legacy-id",
            "Legacy unknown",
            r"\\?\UNC\nas\media\legacy_unknown.mp4",
        );
        seed_direct_success_job(
            &paths,
            "single-job",
            Some("single-item"),
            Some("batch-single"),
            "https://www.youtube.com/watch?v=single-id",
            None,
        );
        seed_direct_success_job(
            &paths,
            "subscription-job",
            Some("subscription-item"),
            Some("batch-subscription"),
            "https://www.youtube.com/shorts/sub-id",
            Some("subscription-id"),
        );
        seed_direct_success_job(
            &paths,
            "playlist-job",
            Some("playlist-item"),
            Some("batch-playlist"),
            "https://www.youtube.com/playlist?list=PL123",
            None,
        );

        let first = backfill_download_lineage_batch(&paths, 2).expect("first bounded backfill");
        assert!(
            first.has_more,
            "first step must not scan beyond its requested bound"
        );
        let completed = backfill_download_lineage_batch(&paths, 2).expect("second backfill");
        assert!(completed.complete);
        assert_eq!(completed.remaining_candidates, 0);

        let page = list_youtube_single_history(&paths, 50, 0, None, None).expect("history");
        assert_eq!(page.canonical_total, 1);
        assert_eq!(page.filtered_total, 1);
        assert_eq!(page.unclassified_total, None);
        assert_eq!(
            count_youtube_single_unclassified(&paths).expect("unclassified count"),
            1
        );
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].id, "single-item");
        assert_eq!(page.items[0].lineage_service.as_deref(), Some("youtube"));
        assert_eq!(page.items[0].lineage_origin_kind.as_deref(), Some("single"));
        assert_eq!(
            page.items[0].lineage_work_track.as_deref(),
            Some("youtube_single")
        );

        let filtered =
            list_youtube_single_history(&paths, 50, 0, Some("wanted_single"), Some("asc"))
                .expect("filtered history");
        assert_eq!(filtered.canonical_total, 1);
        assert_eq!(filtered.filtered_total, 1);

        let conn = db::open(&paths).expect("open db");
        conn.execute(
            "DELETE FROM job WHERE id IN ('single-job', 'subscription-job', 'playlist-job')",
            [],
        )
        .expect("simulate terminal job cleanup");
        let after_cleanup =
            list_youtube_single_history(&paths, 50, 0, None, None).expect("history after cleanup");
        assert_eq!(after_cleanup.canonical_total, 1);
        assert_eq!(after_cleanup.items[0].id, "single-item");

        let normal_list = list_items(&paths, 50, 0).expect("normal library list");
        let normal_single = normal_list
            .iter()
            .find(|item| item.id == "single-item")
            .expect("single in normal list");
        assert_eq!(normal_single.lineage_origin_kind.as_deref(), Some("single"));
        let normal_unknown = normal_list
            .iter()
            .find(|item| item.id == "legacy-unknown")
            .expect("legacy item in normal list");
        assert!(normal_unknown.lineage_origin_kind.is_none());
    }

    #[test]
    fn downloaded_item_provenance_and_lineage_commit_or_roll_back_together() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_data"));
        db::ensure_schema(&paths).expect("schema");

        let success_url = "https://www.youtube.com/watch?v=atomic-success";
        seed_direct_success_job(
            &paths,
            "atomic-success-job",
            None,
            Some("atomic-batch"),
            success_url,
            None,
        );
        let success_media = dir.path().join("atomic-success.mp4");
        std::fs::write(&success_media, b"not-real-media").expect("write media fixture");
        let classification =
            classify_direct_download_execution(success_url, None).expect("classification");
        let imported = import_downloaded_file_with_lineage(
            &paths,
            &success_media,
            success_url,
            "unspecified",
            "youtube",
            10,
            DownloadedFileLineageInput {
                source_job_id: "atomic-success-job".to_string(),
                source_batch_id: None,
                source_subscription_id: None,
                classification,
            },
        )
        .expect("atomic import");
        assert_eq!(imported.lineage_service.as_deref(), Some("youtube"));
        assert_eq!(imported.lineage_origin_kind.as_deref(), Some("single"));
        assert_eq!(
            imported.lineage_work_track.as_deref(),
            Some("youtube_single")
        );

        let page = list_youtube_single_history(&paths, 20, 0, None, None)
            .expect("canonical history immediately after commit");
        assert_eq!(page.canonical_total, 1);
        assert_eq!(page.items[0].id, imported.id);

        let failure_url = "https://www.youtube.com/watch?v=atomic-failure";
        seed_direct_success_job(
            &paths,
            "atomic-failure-job",
            None,
            Some("atomic-failure-batch"),
            failure_url,
            None,
        );
        let failure_media = dir.path().join("atomic-failure.mp4");
        std::fs::write(&failure_media, b"not-real-media").expect("write failure fixture");
        let conn = db::open(&paths).expect("open db");
        conn.execute_batch(
            "CREATE TRIGGER fail_atomic_lineage BEFORE INSERT ON library_download_lineage \
             WHEN NEW.source_job_id='atomic-failure-job' \
             BEGIN SELECT RAISE(ABORT, 'forced lineage failure'); END;",
        )
        .expect("install failure trigger");
        let failed = import_downloaded_file_with_lineage(
            &paths,
            &failure_media,
            failure_url,
            "unspecified",
            "youtube",
            20,
            DownloadedFileLineageInput {
                source_job_id: "atomic-failure-job".to_string(),
                source_batch_id: None,
                source_subscription_id: None,
                classification: classify_direct_download_execution(failure_url, None)
                    .expect("failure classification"),
            },
        );
        assert!(
            failed.is_err(),
            "forced lineage failure must abort the handoff"
        );

        let library_rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM library_item WHERE source_uri=?1",
                [failure_url],
                |row| row.get(0),
            )
            .expect("count rolled-back library rows");
        let provenance_rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM ingest_provenance WHERE source_url=?1",
                [failure_url],
                |row| row.get(0),
            )
            .expect("count rolled-back provenance rows");
        let failed_job_item_id: Option<String> = conn
            .query_row(
                "SELECT item_id FROM job WHERE id='atomic-failure-job'",
                [],
                |row| row.get(0),
            )
            .expect("read failed source job");
        assert_eq!(
            library_rows, 0,
            "library insert must roll back with lineage"
        );
        assert_eq!(
            provenance_rows, 0,
            "provenance insert must roll back with lineage"
        );
        assert!(
            failed_job_item_id.is_none(),
            "source-job item link must roll back with lineage"
        );
    }

    #[test]
    fn direct_lineage_write_links_job_and_keeps_first_evidence_on_retry() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        db::ensure_schema(&paths).expect("schema");
        seed_lineage_item(
            &paths,
            "retry-item",
            50,
            "https://www.youtube.com/watch?v=retry-id",
            "Retry item",
            r"\\?\UNC\nas\media\retry.mp4",
        );
        seed_direct_success_job(
            &paths,
            "retry-first-job",
            None,
            Some("batch-first"),
            "https://www.youtube.com/watch?v=retry-id",
            None,
        );
        seed_direct_success_job(
            &paths,
            "retry-second-job",
            None,
            Some("batch-second"),
            "https://www.youtube.com/playlist?list=PL-retry",
            None,
        );
        let single =
            classify_direct_download_execution("https://www.youtube.com/watch?v=retry-id", None)
                .expect("single classification");
        record_download_lineage(
            &paths,
            DownloadLineageInput {
                item_id: "retry-item".to_string(),
                source_job_id: "retry-first-job".to_string(),
                source_batch_id: None,
                source_subscription_id: None,
                classification: single,
                item_created_at_ms: 50,
            },
        )
        .expect("first lineage");
        let playlist = classify_direct_download_execution(
            "https://www.youtube.com/playlist?list=PL-retry",
            None,
        )
        .expect("playlist classification");
        record_download_lineage(
            &paths,
            DownloadLineageInput {
                item_id: "retry-item".to_string(),
                source_job_id: "retry-second-job".to_string(),
                source_batch_id: None,
                source_subscription_id: None,
                classification: playlist,
                item_created_at_ms: 50,
            },
        )
        .expect("second lineage no overwrite");

        let conn = db::open(&paths).expect("open db");
        let stored: (String, String, String, Option<String>) = conn
            .query_row(
                "SELECT service, origin_kind, work_track, source_batch_id \
                 FROM library_download_lineage WHERE item_id='retry-item'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("stored lineage");
        assert_eq!(stored.0, "youtube");
        assert_eq!(stored.1, "single");
        assert_eq!(stored.2, "youtube_single");
        assert_eq!(stored.3.as_deref(), Some("batch-first"));
        let first_linked_item: Option<String> = conn
            .query_row(
                "SELECT item_id FROM job WHERE id='retry-first-job'",
                [],
                |row| row.get(0),
            )
            .expect("first job link");
        assert_eq!(first_linked_item.as_deref(), Some("retry-item"));
    }

    #[test]
    fn thumbnail_cache_file_name_is_sanitized() {
        let key = thumbnail_cache_file_name("  ab/cd:ef?gh  ");
        assert_eq!(key, "ab_cd_ef_gh.jpg");
    }

    #[test]
    fn prune_thumbnail_cache_evicts_oldest_first() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        paths.ensure_dirs().expect("dirs");
        let cache = paths.thumbnail_cache_dir();

        let old = cache.join("old.jpg");
        let mid = cache.join("mid.jpg");
        let fresh = cache.join("fresh.jpg");
        std::fs::write(&old, vec![1_u8; 60]).expect("old");
        std::fs::write(&mid, vec![2_u8; 60]).expect("mid");
        std::fs::write(&fresh, vec![3_u8; 60]).expect("fresh");

        let now = std::time::SystemTime::now();
        set_file_mtime(
            &old,
            FileTime::from_system_time(
                now.checked_sub(std::time::Duration::from_secs(300))
                    .expect("old ts"),
            ),
        )
        .expect("set old");
        set_file_mtime(
            &mid,
            FileTime::from_system_time(
                now.checked_sub(std::time::Duration::from_secs(200))
                    .expect("mid ts"),
            ),
        )
        .expect("set mid");
        set_file_mtime(
            &fresh,
            FileTime::from_system_time(
                now.checked_sub(std::time::Duration::from_secs(100))
                    .expect("fresh ts"),
            ),
        )
        .expect("set fresh");

        prune_thumbnail_cache(&paths, 120, 3650);

        assert!(
            !old.exists(),
            "oldest file should be evicted first when over cache budget"
        );
        assert!(mid.exists(), "newer file should remain");
        assert!(fresh.exists(), "newest file should remain");
    }

    #[test]
    fn ensure_thumbnail_path_reuses_cached_file_and_updates_db() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        paths.ensure_dirs().expect("dirs");
        db::ensure_schema(&paths).expect("schema");

        let item_id = "item-thumb-cache";
        let media_path = dir.path().join("sample.mp4");
        std::fs::write(&media_path, b"not-a-real-video").expect("media");
        let cached_thumb = thumbnail_cache_path(&paths, item_id);
        std::fs::write(&cached_thumb, b"jpeg").expect("thumb");

        let conn = db::open(&paths).expect("db");
        db::migrate(&conn).expect("migrate");
        conn.execute(
            r#"
INSERT INTO library_item (
  id, created_at_ms, source_type, source_uri, title, media_path,
  duration_ms, width, height, container, video_codec, audio_codec, thumbnail_path
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, NULL, NULL, NULL, NULL, NULL, NULL)
"#,
            params![
                item_id,
                1_i64,
                "local_file",
                media_path.to_string_lossy().to_string(),
                "Sample",
                media_path.to_string_lossy().to_string(),
            ],
        )
        .expect("insert");

        let resolved = ensure_thumbnail_path(&paths, item_id)
            .expect("resolve")
            .expect("thumbnail");
        assert_eq!(resolved, cached_thumb);

        let stored: Option<String> = conn
            .query_row(
                "SELECT thumbnail_path FROM library_item WHERE id=?1",
                [item_id],
                |row| row.get(0),
            )
            .expect("stored path");
        assert_eq!(
            stored.as_deref(),
            Some(cached_thumb.to_string_lossy().as_ref())
        );
    }

    #[test]
    fn ensure_thumbnail_path_clears_stale_reference_when_media_missing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        paths.ensure_dirs().expect("dirs");
        db::ensure_schema(&paths).expect("schema");

        let item_id = "item-thumb-missing";
        let missing_media = dir.path().join("missing.mp4");
        let stale_thumb = dir.path().join("stale.jpg");

        let conn = db::open(&paths).expect("db");
        db::migrate(&conn).expect("migrate");
        conn.execute(
            r#"
INSERT INTO library_item (
  id, created_at_ms, source_type, source_uri, title, media_path,
  duration_ms, width, height, container, video_codec, audio_codec, thumbnail_path
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, NULL, NULL, NULL, NULL, NULL, ?7)
"#,
            params![
                item_id,
                1_i64,
                "local_file",
                missing_media.to_string_lossy().to_string(),
                "Missing",
                missing_media.to_string_lossy().to_string(),
                stale_thumb.to_string_lossy().to_string(),
            ],
        )
        .expect("insert");

        let resolved = ensure_thumbnail_path(&paths, item_id).expect("resolve");
        assert!(
            resolved.is_none(),
            "missing media should not yield a thumbnail"
        );

        let stored: Option<String> = conn
            .query_row(
                "SELECT thumbnail_path FROM library_item WHERE id=?1",
                [item_id],
                |row| row.get(0),
            )
            .expect("stored path");
        assert!(
            stored.is_none(),
            "stale thumbnail reference should be cleared"
        );
    }

    #[test]
    fn list_items_by_ids_returns_empty_for_empty_input() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        paths.ensure_dirs().expect("dirs");
        db::ensure_schema(&paths).expect("schema");

        let items = list_items_by_ids(&paths, &[]).expect("list");
        assert!(items.is_empty());
    }

    #[test]
    fn list_items_by_ids_skips_missing_ids_and_returns_present() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        paths.ensure_dirs().expect("dirs");
        db::ensure_schema(&paths).expect("schema");

        let conn = db::open(&paths).expect("db");
        db::migrate(&conn).expect("migrate");
        for (id, title) in [("aaa", "Alpha"), ("bbb", "Beta"), ("ccc", "Gamma")] {
            conn.execute(
                r#"
INSERT INTO library_item (
  id, created_at_ms, source_type, source_uri, title, media_path,
  duration_ms, width, height, container, video_codec, audio_codec, thumbnail_path
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, NULL, NULL, NULL, NULL, NULL, NULL)
"#,
                params![id, 1_i64, "local_file", "uri", title, "media"],
            )
            .expect("insert");
        }

        let items = list_items_by_ids(&paths, &["aaa", "missing", "ccc"]).expect("list");
        assert_eq!(items.len(), 2, "should skip the missing id silently");
        let mut titles: Vec<String> = items.iter().map(|i| i.title.clone()).collect();
        titles.sort();
        assert_eq!(titles, vec!["Alpha".to_string(), "Gamma".to_string()]);
    }

    #[test]
    fn canonical_preflight_claim_relocate_and_metadata_removal_are_preservation_first() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_data"));
        db::ensure_schema(&paths).expect("schema");
        let source_url = "https://www.youtube.com/watch?v=canon123456";
        let alias_url = "https://youtu.be/canon123456?t=42";
        let original = dir.path().join("canonical-original.mp4");
        std::fs::write(&original, b"fixture").expect("write original");
        seed_lineage_item(
            &paths,
            "canonical-item",
            10,
            source_url,
            "Canonical title",
            &original.to_string_lossy(),
        );
        let conn = db::open(&paths).expect("subscription db");
        conn.execute(
            "INSERT INTO youtube_subscription (id, title, source_url, folder_map, created_at_ms, updated_at_ms) VALUES ('sub-1', 'Creator shorts', 'https://www.youtube.com/@creator/shorts', 'creator_shorts', 1, 1)",
            [],
        )
        .expect("seed source membership");
        drop(conn);

        let present =
            preflight_download_urls(&paths, &[alias_url.to_string()]).expect("present preflight");
        assert_eq!(present[0].status, "present");
        assert_eq!(
            present[0].library_item_id.as_deref(),
            Some("canonical-item")
        );

        std::fs::remove_file(&original).expect("simulate missing media");
        invalidate_media_path_observation(&paths, &original.to_string_lossy())
            .expect("invalidate original observation");
        let missing =
            preflight_download_urls(&paths, &[source_url.to_string()]).expect("missing preflight");
        assert_eq!(missing[0].status, "missing");
        assert!(matches!(
            claim_download_source(&paths, source_url, "unapproved", false, false, "single", None)
                .expect("unapproved claim"),
            DownloadSourceClaim::Missing(ref id) if id == "canonical-item"
        ));
        assert_eq!(
            claim_download_source(
                &paths,
                source_url,
                "repair-job",
                true,
                false,
                "single",
                None
            )
            .expect("approved repair claim"),
            DownloadSourceClaim::Claimed
        );
        assert_eq!(
            claim_download_source(
                &paths,
                alias_url,
                "racing-job",
                true,
                false,
                "subscription",
                Some("sub-1")
            )
            .expect("racing claim"),
            DownloadSourceClaim::Active("repair-job".to_string())
        );
        let conn = db::open_readonly(&paths).expect("membership db");
        let membership_kind: String = conn
            .query_row(
                "SELECT source_kind FROM media_source_membership WHERE service='youtube' AND media_id='canon123456' AND source_subscription_id='sub-1'",
                [],
                |row| row.get(0),
            )
            .expect("active claim membership");
        assert_eq!(membership_kind, "shorts_page");
        drop(conn);
        release_download_source_claim(
            &paths,
            "repair-job",
            Some(source_url),
            Some("upstream link failed"),
        )
        .expect("release failed repair");
        let failed = preflight_download_urls(&paths, &[source_url.to_string()])
            .expect("failed repair preflight");
        assert_eq!(failed[0].failed_url.as_deref(), Some(source_url));
        assert_eq!(
            failed[0].last_error.as_deref(),
            Some("upstream link failed")
        );

        let relocated = dir.path().join("canonical-relocated.mp4");
        std::fs::write(&relocated, b"relocated fixture").expect("write relocation");
        let item = relocate_canonical_media(&paths, "canonical-item", &relocated)
            .expect("relocate canonical item");
        assert_eq!(
            PathBuf::from(item.media_path),
            relocated.canonicalize().expect("canonical path")
        );
        let removed = remove_canonical_library_record(&paths, "canonical-item")
            .expect("remove metadata only");
        assert!(removed);
        assert!(
            relocated.is_file(),
            "metadata removal must never delete media"
        );
        let ready = preflight_download_urls(&paths, &[source_url.to_string()])
            .expect("post-removal preflight");
        assert_eq!(ready[0].status, "ready");
        assert!(ready[0].library_item_id.is_none());
    }

    #[test]
    fn operator_deleted_video_is_removed_preserved_and_requires_exact_manual_authorization() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_data"));
        db::ensure_schema(&paths).expect("schema");
        let source_url = "https://www.youtube.com/watch?v=deleted12345";
        let media_path = dir.path().join("operator-delete-fixture.mp4");
        std::fs::write(&media_path, b"fixture").expect("write fixture");
        seed_lineage_item(
            &paths,
            "deleted-item",
            10,
            source_url,
            "Deleted fixture",
            &media_path.to_string_lossy(),
        );
        let conn = db::open(&paths).expect("db");
        conn.execute(
            "INSERT INTO youtube_subscription (id, title, source_url, folder_map, created_at_ms, updated_at_ms) VALUES ('sub-delete', 'Delete source', 'https://www.youtube.com/@delete/videos', 'delete', 1, 1)",
            [],
        )
        .expect("subscription");
        drop(conn);
        record_source_association(&paths, source_url, "subscription", Some("sub-delete"), None)
            .expect("membership");

        let receipt = delete_library_item_files(
            &paths,
            &["deleted-item".to_string()],
            "permanent",
            "operator",
        )
        .expect("delete receipt");
        assert_eq!(receipt.deleted, 1);
        assert_eq!(receipt.failed, 0);
        assert!(!media_path.exists(), "selected file must be removed");
        assert!(
            list_items(&paths, 20, 0)
                .expect("available list")
                .is_empty(),
            "normal library projection must hide deleted items"
        );
        let deleted =
            list_items_by_file_status(&paths, 20, 0, Some("operator_deleted")).expect("deleted");
        assert_eq!(deleted.len(), 1);
        assert_eq!(deleted[0].file_status, "operator_deleted");
        let preflight =
            preflight_download_urls(&paths, &[source_url.to_string()]).expect("preflight");
        assert_eq!(preflight[0].status, "operator_deleted");
        assert!(matches!(
            claim_download_source(
                &paths,
                source_url,
                "generic-retry",
                true,
                false,
                "subscription",
                Some("sub-delete")
            )
            .expect("generic claim"),
            DownloadSourceClaim::OperatorDeleted(ref id) if id == "deleted-item"
        ));
        assert_eq!(
            claim_download_source(
                &paths,
                source_url,
                "exact-manual-job",
                true,
                true,
                "subscription",
                Some("sub-delete")
            )
            .expect("manual claim"),
            DownloadSourceClaim::Claimed
        );
        assert_eq!(
            claim_download_source(
                &paths,
                source_url,
                "replacement-manual-job",
                true,
                true,
                "subscription",
                Some("sub-delete")
            )
            .expect("repeat manual claim"),
            DownloadSourceClaim::Active("exact-manual-job".to_string()),
            "a second request must not replace an active exact authorization"
        );
        let conn = db::open_readonly(&paths).expect("readonly");
        let state: (String, Option<String>, i64) = conn
            .query_row(
                r#"
SELECT li.file_status, li.file_redownload_authorized_job_id,
       (SELECT COUNT(*) FROM media_source_membership WHERE service='youtube' AND media_id='deleted12345' AND source_subscription_id='sub-delete')
FROM library_item li WHERE li.id='deleted-item'
"#,
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("state");
        assert_eq!(state.0, "operator_deleted");
        assert_eq!(state.1.as_deref(), Some("exact-manual-job"));
        assert_eq!(state.2, 1, "source membership must survive file deletion");
    }

    #[test]
    fn file_delete_receipt_is_partial_and_unreachable_never_becomes_deleted() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_data"));
        db::ensure_schema(&paths).expect("schema");
        let removable_url = "https://www.youtube.com/watch?v=partial0001";
        let unreachable_url = "https://www.youtube.com/watch?v=partial0002";
        let removable_path = dir.path().join("partial-removable.mp4");
        std::fs::write(&removable_path, b"fixture").expect("write removable fixture");
        seed_lineage_item(
            &paths,
            "partial-removable",
            10,
            removable_url,
            "Partial removable",
            &removable_path.to_string_lossy(),
        );
        let unreachable_path = "invalid\0windows-path.mp4";
        assert_eq!(
            observe_media_path_fresh(&paths, unreachable_path),
            MediaPathObservation::Unreachable,
            "the fixture must exercise the unreachable/error branch"
        );
        seed_lineage_item(
            &paths,
            "partial-unreachable",
            11,
            unreachable_url,
            "Partial unreachable",
            unreachable_path,
        );
        preflight_download_urls(
            &paths,
            &[removable_url.to_string(), unreachable_url.to_string()],
        )
        .expect("bind canonical identities");

        let receipt = delete_library_item_files(
            &paths,
            &[
                "partial-removable".to_string(),
                "partial-unreachable".to_string(),
            ],
            "permanent",
            "operator",
        )
        .expect("partial receipt");
        assert_eq!(receipt.requested, 2);
        assert_eq!(receipt.deleted, 1);
        assert_eq!(receipt.failed, 1);
        assert!(!removable_path.exists(), "successful item must be removed");
        let removable = get_item_by_id(&paths, "partial-removable").expect("removable item");
        let unreachable = get_item_by_id(&paths, "partial-unreachable").expect("unreachable item");
        assert_eq!(removable.file_status, LIBRARY_FILE_STATUS_OPERATOR_DELETED);
        assert_eq!(unreachable.file_status, LIBRARY_FILE_STATUS_AVAILABLE);
        assert!(
            receipt
                .results
                .iter()
                .find(|row| row.item_id == "partial-unreachable")
                .is_some_and(|row| {
                    row.outcome == "failed" && row.message.contains("storage is unreachable")
                }),
            "unreachable failure must remain explicit in the per-item receipt"
        );
    }

    #[test]
    fn trash_mode_marks_an_already_absent_file_without_dropping_canonical_metadata() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_data"));
        db::ensure_schema(&paths).expect("schema");
        let source_url = "https://www.youtube.com/watch?v=trashmod001";
        let absent_path = dir.path().join("already-absent.mp4");
        seed_lineage_item(
            &paths,
            "trash-mode-item",
            10,
            source_url,
            "Trash mode fixture",
            &absent_path.to_string_lossy(),
        );
        preflight_download_urls(&paths, &[source_url.to_string()]).expect("bind identity");

        let receipt = delete_library_item_files(
            &paths,
            &["trash-mode-item".to_string()],
            "trash",
            "operator",
        )
        .expect("trash receipt");
        assert_eq!(receipt.mode, "trash");
        assert_eq!(receipt.deleted, 1);
        assert_eq!(receipt.failed, 0);
        assert_eq!(
            receipt.results[0].message,
            "file was already absent; canonical item is now marked deleted"
        );
        let item = get_item_by_id(&paths, "trash-mode-item").expect("preserved item");
        assert_eq!(item.file_status, LIBRARY_FILE_STATUS_OPERATOR_DELETED);
        assert_eq!(item.file_delete_method.as_deref(), Some("trash"));
        let preflight =
            preflight_download_urls(&paths, &[source_url.to_string()]).expect("preflight");
        assert_eq!(preflight[0].status, "operator_deleted");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn trash_mode_moves_a_present_file_to_the_windows_recycle_bin() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_data"));
        db::ensure_schema(&paths).expect("schema");
        let source_url = "https://www.youtube.com/watch?v=trashwin001";
        let file_name = format!("wp0284-trash-{}.mp4", Uuid::new_v4());
        let media_path = dir.path().join(&file_name);
        std::fs::write(&media_path, b"fixture").expect("write trash fixture");
        seed_lineage_item(
            &paths,
            "trash-windows-item",
            10,
            source_url,
            "Windows trash fixture",
            &media_path.to_string_lossy(),
        );
        preflight_download_urls(&paths, &[source_url.to_string()]).expect("bind identity");

        let receipt = delete_library_item_files(
            &paths,
            &["trash-windows-item".to_string()],
            "trash",
            "operator",
        )
        .expect("trash receipt");
        assert_eq!(receipt.deleted, 1);
        assert!(!media_path.exists(), "file must leave its original path");

        let mut matching = trash::os_limited::list()
            .expect("list Recycle Bin")
            .into_iter()
            .filter(|entry| entry.name == std::ffi::OsString::from(&file_name))
            .collect::<Vec<_>>();
        let matching_count = matching.len();
        if !matching.is_empty() {
            trash::os_limited::restore_all(std::mem::take(&mut matching))
                .expect("restore test fixture from Recycle Bin");
        }
        assert_eq!(
            matching_count, 1,
            "the exact uniquely named fixture must be present in the Recycle Bin"
        );
        assert!(
            media_path.is_file(),
            "the verification fixture must be restored so the test leaves no Recycle Bin artifact"
        );
    }

    #[test]
    fn authorized_redownload_clears_tombstone_only_after_successful_import() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_data"));
        db::ensure_schema(&paths).expect("schema");
        let source_url = "https://www.youtube.com/watch?v=restore12345";
        let old_path = dir.path().join("removed-original.mp4");
        seed_lineage_item(
            &paths,
            "restore-item",
            10,
            source_url,
            "Restore fixture",
            &old_path.to_string_lossy(),
        );
        preflight_download_urls(&paths, &[source_url.to_string()]).expect("bind identity");
        let conn = db::open(&paths).expect("db");
        conn.execute(
            "UPDATE library_item SET file_status='operator_deleted', \
             file_redownload_authorized_job_id='manual-restore-job' WHERE id='restore-item'",
            [],
        )
        .expect("tombstone");
        drop(conn);
        seed_direct_success_job(
            &paths,
            "manual-restore-job",
            None,
            Some("restore-batch"),
            source_url,
            None,
        );
        let replacement = dir.path().join("replacement.mp4");
        std::fs::write(&replacement, b"replacement").expect("replacement");
        let old_text = old_path.to_string_lossy().to_string();
        let replacement_text = replacement
            .canonicalize()
            .expect("canonical replacement")
            .to_string_lossy()
            .to_string();
        seed_present_observation(&paths, &old_text);
        seed_present_observation(&paths, &replacement_text);
        let item = import_downloaded_file_with_lineage(
            &paths,
            &replacement,
            source_url,
            "unspecified",
            "youtube_yt_dlp_v1",
            20,
            DownloadedFileLineageInput {
                source_job_id: "manual-restore-job".to_string(),
                source_batch_id: Some("restore-batch".to_string()),
                source_subscription_id: None,
                classification: DownloadLineageClassification {
                    service: "youtube".to_string(),
                    origin_kind: "single".to_string(),
                    work_track: "youtube_single".to_string(),
                },
            },
        )
        .expect("authorized import");
        assert_eq!(item.id, "restore-item");
        assert_eq!(item.file_status, "available");
        let stored = get_item_by_id(&paths, "restore-item").expect("stored item");
        assert_eq!(stored.file_status, "available");
        assert_eq!(
            stored.media_path,
            replacement.canonicalize().unwrap().to_string_lossy()
        );
        assert!(stored.file_redownload_authorized_job_id.is_none());
        assert_observation_invalidated(&paths, &old_text);
        assert_observation_invalidated(&paths, &replacement_text);
    }

    #[test]
    fn missing_canonical_redownload_reuses_the_existing_library_item() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_data"));
        db::ensure_schema(&paths).expect("schema");
        let source_url = "https://www.youtube.com/watch?v=reuse1234567";
        let missing_path = dir.path().join("missing-before-repair.mp4");
        seed_lineage_item(
            &paths,
            "reuse-item",
            11,
            source_url,
            "Old title",
            &missing_path.to_string_lossy(),
        );
        let before =
            preflight_download_urls(&paths, &[source_url.to_string()]).expect("link legacy item");
        assert_eq!(before[0].status, "missing");
        seed_direct_success_job(
            &paths,
            "reuse-job",
            None,
            Some("reuse-batch"),
            source_url,
            None,
        );
        let repaired_path = dir.path().join("repaired-download.mp4");
        std::fs::write(&repaired_path, b"repair fixture").expect("write repair");
        let imported = import_downloaded_file_with_lineage(
            &paths,
            &repaired_path,
            source_url,
            "unspecified",
            "youtube",
            12,
            DownloadedFileLineageInput {
                source_job_id: "reuse-job".to_string(),
                source_batch_id: Some("reuse-batch".to_string()),
                source_subscription_id: None,
                classification: classify_direct_download_execution(source_url, None)
                    .expect("classification"),
            },
        )
        .expect("repair import");
        assert_eq!(imported.id, "reuse-item");
        let conn = db::open_readonly(&paths).expect("readonly db");
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM library_item WHERE source_uri=?1",
                [source_url],
                |row| row.get(0),
            )
            .expect("count canonical item");
        assert_eq!(count, 1, "redownload must update, not duplicate, the item");
    }

    #[test]
    fn media_library_query_filters_the_canonical_set_before_pagination() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_data"));
        db::ensure_schema(&paths).expect("schema");
        let conn = db::open(&paths).expect("db");
        db::migrate(&conn).expect("migrate");

        // Seed more than the UI page size with newer local rows. The wanted imported YouTube item
        // is deliberately older than all of them, reproducing the v0.1.132 failure where React
        // filtered only the newest 200 rows and displayed an empty YouTube result.
        for index in 0..205_i64 {
            conn.execute(
                r#"
INSERT INTO library_item (
  id, created_at_ms, source_type, source_uri, title, media_path,
  duration_ms, width, height, container, video_codec, audio_codec, thumbnail_path
) VALUES (?1, ?2, 'local_file', ?3, ?4, ?3, NULL, 1920, 1080, 'mp4', 'h264', 'aac', NULL)
"#,
                params![
                    format!("new-local-{index:03}"),
                    1_000 + index,
                    format!(r"C:\media\new-local-{index:03}.mp4"),
                    format!("New local {index:03}"),
                ],
            )
            .expect("local item");
        }
        conn.execute(
            r#"
INSERT INTO library_item (
  id, created_at_ms, source_type, source_uri, title, media_path,
  duration_ms, width, height, container, video_codec, audio_codec, thumbnail_path
) VALUES ('older-imported-youtube', 1, 'local_file', ?1, 'Needle imported video', ?1,
          NULL, 3840, 2160, 'mp4', 'av1', 'opus', NULL)
"#,
            [r"\\MIR\home\Video\legacy\needle.mp4"],
        )
        .expect("imported item");
        conn.execute(
            r#"
INSERT INTO media_source_identity (
  service, media_id, canonical_url, library_item_id, created_at_ms, updated_at_ms
) VALUES ('youtube', 'exact1234567', 'https://www.youtube.com/watch?v=exact1234567',
          'older-imported-youtube', 1, 1)
"#,
            [],
        )
        .expect("exact imported identity");
        // A second identity linked to the same item must not duplicate the returned library row.
        conn.execute(
            r#"
INSERT INTO media_source_identity (
  service, media_id, canonical_url, library_item_id, created_at_ms, updated_at_ms
) VALUES ('other', 'other1234567', 'https://example.test/other1234567',
          'older-imported-youtube', 1, 1)
"#,
            [],
        )
        .expect("secondary identity");
        drop(conn);

        let youtube = query_items_page(
            &paths,
            20,
            0,
            Some("available"),
            None,
            Some("video"),
            Some("youtube"),
            false,
            Some("date"),
            Some("desc"),
        )
        .expect("youtube page");
        assert_eq!(youtube.filtered_total, 1);
        assert_eq!(youtube.items.len(), 1);
        assert_eq!(youtube.items[0].id, "older-imported-youtube");
        assert_eq!(
            youtube.items[0].canonical_service.as_deref(),
            Some("youtube")
        );

        let searched = query_items_page(
            &paths,
            1,
            0,
            Some("available"),
            Some("needle"),
            Some("all"),
            Some("all"),
            false,
            Some("title"),
            Some("asc"),
        )
        .expect("searched page");
        assert_eq!(searched.filtered_total, 1);
        assert_eq!(searched.items[0].id, "older-imported-youtube");

        let local = query_items_page(
            &paths,
            10,
            0,
            Some("available"),
            None,
            Some("video"),
            Some("local"),
            false,
            Some("date"),
            Some("desc"),
        )
        .expect("local page");
        assert_eq!(local.filtered_total, 205);
        assert_eq!(local.items.len(), 10);
        assert!(
            local
                .items
                .iter()
                .all(|item| item.canonical_service.is_none()),
            "unlinked local rows must remain local/unclassified"
        );

        let singles = query_items_page(
            &paths,
            20,
            0,
            Some("available"),
            None,
            Some("video"),
            Some("youtube"),
            true,
            Some("date"),
            Some("desc"),
        )
        .expect("single page");
        assert_eq!(
            singles.filtered_total, 0,
            "exact imported identity must not invent single-video lineage"
        );
    }

    #[test]
    fn relocate_and_root_transfer_invalidate_both_old_and_new_observations() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_data"));
        db::ensure_schema(&paths).expect("schema");

        let old = dir.path().join("relocate-old.mkv");
        let relocated = dir.path().join("relocate-new.mkv");
        std::fs::write(&old, b"old").expect("old fixture");
        std::fs::write(&relocated, b"new").expect("new fixture");
        let old_text = old.canonicalize().unwrap().to_string_lossy().to_string();
        let relocated_text = relocated
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        seed_lineage_item(
            &paths,
            "relocate-observation-item",
            1,
            "file://relocate",
            "Relocate observation",
            &old_text,
        );
        seed_present_observation(&paths, &old_text);
        seed_present_observation(&paths, &relocated_text);
        relocate_canonical_media(&paths, "relocate-observation-item", &relocated)
            .expect("relocate");
        assert_observation_invalidated(&paths, &old_text);
        assert_observation_invalidated(&paths, &relocated_text);

        let source_root = dir.path().join("source_root");
        let target_root = dir.path().join("target_root");
        std::fs::create_dir_all(&source_root).unwrap();
        std::fs::create_dir_all(&target_root).unwrap();
        let source_path = source_root.join("transfer.mkv").to_string_lossy().to_string();
        let target_path = replace_root_prefix(
            &source_path,
            &source_root.to_string_lossy(),
            &target_root.to_string_lossy(),
        );
        seed_lineage_item(
            &paths,
            "transfer-observation-item",
            2,
            "file://transfer",
            "Transfer observation",
            &source_path,
        );
        seed_present_observation(&paths, &source_path);
        seed_present_observation(&paths, &target_path);
        let summary = transfer_item_metadata_between_roots(
            &paths,
            "source-library",
            &source_root.to_string_lossy(),
            "target-library",
            &target_root.to_string_lossy(),
            false,
        )
        .expect("root transfer");
        assert_eq!(summary.items_moved, 1);
        assert_observation_invalidated(&paths, &source_path);
        assert_observation_invalidated(&paths, &target_path);
    }

    #[test]
    fn fallback_resync_invalidates_source_and_destination_observations() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_data"));
        paths.ensure_dirs().expect("app dirs");
        db::ensure_schema(&paths).expect("schema");
        let configured = dir.path().join("configured_root");
        std::fs::create_dir_all(&configured).expect("configured root");
        paths
            .set_download_dir_override(&configured)
            .expect("download override");
        let fallback = paths.local_fallback_download_dir();
        std::fs::create_dir_all(&fallback).expect("fallback root");
        let source = fallback.join("channel").join("fallback.mkv");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(&source, b"fallback bytes").expect("fallback fixture");
        let source_text = source.canonicalize().unwrap().to_string_lossy().to_string();
        let destination = configured.join("channel").join("fallback.mkv");
        let destination_text = destination.to_string_lossy().to_string();
        seed_lineage_item(
            &paths,
            "fallback-observation-item",
            3,
            "file://fallback",
            "Fallback observation",
            &source_text,
        );
        seed_present_observation(&paths, &source_text);
        seed_present_observation(&paths, &destination_text);

        let report = resync_local_fallback_downloads(&paths).expect("fallback resync");
        assert_eq!(report.moved, 1);
        assert!(destination.is_file());
        assert_observation_invalidated(&paths, &source_text);
        assert_observation_invalidated(&paths, &destination_text);
    }
}
#[test]
fn media_availability_observation_persists_and_invalidation_forces_refresh() {
    let dir = tempfile::tempdir().expect("tempdir");
    let paths = AppPaths::new(dir.path().join("app_data"));
    db::ensure_schema(&paths).expect("schema");
    let media = dir.path().join("observation_fixture.mkv");
    std::fs::write(&media, b"fixture").expect("write fixture");
    let media_text = media.to_string_lossy().to_string();

    assert_eq!(
        observe_media_path_fresh(&paths, &media_text),
        MediaPathObservation::Present
    );
    media_path_observations()
        .lock()
        .expect("cache lock")
        .remove(&media_text);
    std::fs::remove_file(&media).expect("remove fixture");
    assert_eq!(
        observe_media_path(&paths, &media_text),
        MediaPathObservation::Present,
        "fresh persisted observation should survive an in-memory cache reset"
    );

    invalidate_media_path_observation(&paths, &media_text).expect("invalidate observation");
    assert_eq!(
        observe_media_path_fresh(&paths, &media_text),
        MediaPathObservation::Missing,
        "an invalidated correctness boundary must perform a fresh probe"
    );
    let conn = db::open_readonly(&paths).expect("readonly db");
    let row: (String, String, i64, Option<i64>) = conn
            .query_row(
                "SELECT state,source,duration_ms,invalidated_at_ms FROM media_availability_observation WHERE path=?1",
                [&media_text],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("persistent observation");
    assert_eq!(row.0, "missing");
    assert_eq!(row.1, "fresh_probe");
    assert!(row.2 >= 0);
    assert!(row.3.is_none());
}

#[test]
fn media_observation_invalidation_failure_is_truthful_and_keeps_memory_generation() {
    let dir = tempfile::tempdir().expect("tempdir");
    let paths = AppPaths::new(dir.path().join("app_data"));
    db::ensure_schema(&paths).expect("schema");
    let media_text = dir.path().join("durable-invalidation.mkv").to_string_lossy().to_string();
    let conn = db::open(&paths).expect("observation db");
    conn.execute(
        "INSERT INTO media_availability_observation(path,state,observed_at_ms,source,duration_ms,next_refresh_at_ms,invalidated_at_ms) VALUES(?1,'present',1,'fixture',1,9999999999999,NULL) ON CONFLICT(path) DO UPDATE SET state='present',observed_at_ms=1,source='fixture',duration_ms=1,next_refresh_at_ms=9999999999999,invalidated_at_ms=NULL",
        [&media_text],
    )
    .expect("seed observation");
    drop(conn);
    cache_media_path_observation(&media_text, MediaPathObservation::Present);
    let generation_before = media_path_observation_generation(&media_text);
    let conn = db::open(&paths).expect("db");
    conn.execute("DROP TABLE media_availability_observation", [])
        .expect("inject durable invalidation failure");
    drop(conn);

    assert!(invalidate_media_path_observation(&paths, &media_text).is_err());
    assert_eq!(
        media_path_observation_generation(&media_text),
        generation_before,
        "memory invalidation must not claim success before durable invalidation commits"
    );
    assert_eq!(
        media_path_observations()
            .lock()
            .expect("cache")
            .get(&media_text)
            .map(|entry| entry.1),
        Some(MediaPathObservation::Present)
    );
}

#[test]
fn bounded_probe_timeout_persists_as_slow_not_unreachable() {
    assert_eq!(
        media_path_observation_from_probe_receive(Err(mpsc::RecvTimeoutError::Timeout)),
        MediaPathObservation::Slow
    );
    assert_eq!(
        media_path_observation_from_probe_receive(Err(mpsc::RecvTimeoutError::Disconnected)),
        MediaPathObservation::Unreachable
    );
    let dir = tempfile::tempdir().expect("tempdir");
    let paths = AppPaths::new(dir.path().join("app_data"));
    db::ensure_schema(&paths).expect("schema");
    commit_media_path_observation_if_current(
        &paths,
        "slow-fixture",
        MediaPathObservation::Slow,
        "bounded_timeout",
        MEDIA_PATH_OBSERVATION_TIMEOUT,
        now_ms(),
        media_path_observation_generation("slow-fixture"),
    );
    assert_eq!(
        db::open_readonly(&paths)
            .expect("readonly")
            .query_row(
                "SELECT state FROM media_availability_observation WHERE path='slow-fixture'",
                [],
                |row| row.get::<_, String>(0),
            )
            .expect("slow row"),
        "slow"
    );
}

#[test]
fn media_observation_cache_is_bounded_and_evicts_oldest_entry() {
    let mut cache = media_path_observations()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    cache.clear();
    drop(cache);
    for index in 0..=MEDIA_PATH_OBSERVATION_CACHE_CAPACITY {
        cache_media_path_observation(
            &format!("bounded-observation-{index}"),
            MediaPathObservation::Missing,
        );
    }
    let cache = media_path_observations()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    assert_eq!(cache.len(), MEDIA_PATH_OBSERVATION_CACHE_CAPACITY);
    assert!(!cache.contains_key("bounded-observation-0"));
    assert!(cache.contains_key(&format!(
        "bounded-observation-{MEDIA_PATH_OBSERVATION_CACHE_CAPACITY}"
    )));
}

#[test]
fn media_observation_generation_map_is_bounded_beyond_4096_paths() {
    let mut generations = media_path_observation_generations()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    generations.clear();
    for index in 0..=(MEDIA_PATH_OBSERVATION_CACHE_CAPACITY + 64) {
        let path = format!("bounded-generation-{index}");
        bound_media_path_observation_generations(&mut generations, &path);
        generations.insert(path, (index as u64, Instant::now()));
    }
    assert_eq!(generations.len(), MEDIA_PATH_OBSERVATION_CACHE_CAPACITY);
    assert!(!generations.contains_key("bounded-generation-0"));
    assert!(generations.contains_key(&format!(
        "bounded-generation-{}",
        MEDIA_PATH_OBSERVATION_CACHE_CAPACITY + 64
    )));
}

#[test]
fn nas_probe_receipt_preserves_causal_envelope_generation_source_duration_and_result() {
    let causal = MediaProbeCausalEnvelope {
        request_id: Some("request-fixture".into()),
        span_id: Some("span-fixture".into()),
        incident_id: Some("incident-fixture".into()),
    };
    let details = media_path_probe_details(
        Some(&causal),
        17,
        "nas_bounded_worker_pool",
        Some(Duration::from_millis(23)),
        Some("unreachable"),
    );
    assert_eq!(details["request_id"], "request-fixture");
    assert_eq!(details["span_id"], "span-fixture");
    assert_eq!(details["incident_id"], "incident-fixture");
    assert_eq!(details["generation"], 17);
    assert_eq!(details["duration_ms"], 23);
    assert_eq!(details["source"], "nas_bounded_worker_pool");
    assert_eq!(details["result"], "unreachable");
}

#[test]
fn invalidation_generation_rejects_late_probe_commit() {
    let dir = tempfile::tempdir().expect("tempdir");
    let paths = AppPaths::new(dir.path().join("app"));
    db::ensure_schema(&paths).expect("schema");
    let path = dir
        .path()
        .join("late-probe.mkv")
        .to_string_lossy()
        .to_string();
    let initial_generation = media_path_observation_generation(&path);
    commit_media_path_observation_if_current(
        &paths,
        &path,
        MediaPathObservation::Missing,
        "initial_fixture",
        Duration::from_millis(1),
        now_ms(),
        initial_generation,
    );
    let stale_generation = media_path_observation_generation(&path);
    let probe_started_at_ms = now_ms();
    invalidate_media_path_observation(&paths, &path).expect("invalidate observation");
    commit_media_path_observation_if_current(
        &paths,
        &path,
        MediaPathObservation::Present,
        "late_probe_fixture",
        Duration::from_millis(1),
        probe_started_at_ms,
        stale_generation,
    );
    assert!(!media_path_observations()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .contains_key(&path));
    let conn = db::open_readonly(&paths).expect("readonly");
    let invalidated: Option<i64> = conn
        .query_row(
            "SELECT invalidated_at_ms FROM media_availability_observation WHERE path=?1",
            [&path],
            |row| row.get(0),
        )
        .optional()
        .expect("query")
        .flatten();
    assert!(
        invalidated.is_some(),
        "late probe must not clear invalidation"
    );
}

#[test]
fn canonical_mp4_dedupe_resolves_verified_root_alias_without_rewriting_identity() {
    let dir = tempfile::tempdir().expect("tempdir");
    let paths = AppPaths::new(dir.path().join("app_data"));
    db::ensure_schema(&paths).expect("schema");
    let target_root = dir.path().join("direct_archive");
    std::fs::create_dir_all(&target_root).expect("target root");
    let current_mp4 = target_root.join("legacy.mp4");
    std::fs::write(&current_mp4, b"historical mp4").expect("historical file");
    let stored_mp4 = r"C:\old_archive\legacy.mp4";
    let conn = db::open(&paths).expect("db");
    conn.execute(
        "INSERT INTO library_item(id,created_at_ms,source_type,source_uri,title,media_path) VALUES('legacy-item',1,'local_file',?1,'Legacy MP4',?1)",
        [stored_mp4],
    )
    .expect("stored historical identity");
    drop(conn);
    let aliases = crate::root_rebind::RootAliasesConfig {
        schema_version: 1,
        aliases: vec![crate::root_rebind::RootAliasRecord {
            id: "test-alias".to_string(),
            from_root: r"C:\old_archive".to_string(),
            to_root: target_root
                .canonicalize()
                .expect("canonical alias target")
                .to_string_lossy()
                .to_string(),
            verified_at_ms: 1,
            status: "active".to_string(),
            receipt_path: "test-receipt.json".to_string(),
        }],
    };
    crate::persistence::atomic_write_text(
        &paths.root_aliases_config_path(),
        &format!("{}\n", serde_json::to_string_pretty(&aliases).unwrap()),
    )
    .expect("alias config");

    let found = get_item_by_canonical_media_path(&paths, &current_mp4)
        .expect("alias-aware dedupe")
        .expect("historical item found");
    assert_eq!(found.id, "legacy-item");
    assert_eq!(
        found.media_path, stored_mp4,
        "database identity stays historical"
    );
}

#[test]
fn historical_mp4_behavior_matrix_covers_alias_availability_dedupe_delete_retry_membership_and_nonconversion(
) {
    let dir = tempfile::tempdir().expect("tempdir");
    let paths = AppPaths::new(dir.path().join("app_data"));
    db::ensure_schema(&paths).expect("schema");
    let target_root = dir.path().join("direct_archive");
    std::fs::create_dir_all(&target_root).expect("target root");
    let current_mp4 = target_root.join("legacy-delete.mp4");
    std::fs::write(&current_mp4, b"historical mp4 delete fixture").expect("historical file");
    let canonical_target_root = target_root.canonicalize().expect("canonical alias target");
    let stored_mp4 = r"C:\old_archive\legacy-delete.mp4";
    let conn = db::open(&paths).expect("db");
    conn.execute(
        "INSERT INTO library_item(id,created_at_ms,source_type,source_uri,title,media_path,container) VALUES('legacy-delete-item',1,'url_direct','https://www.youtube.com/watch?v=legacydelete','Legacy MP4 delete',?1,'mp4')",
        [stored_mp4],
    )
    .expect("stored historical identity");
    conn.execute(
        "INSERT INTO media_source_identity(service,media_id,canonical_url,library_item_id,repair_state,created_at_ms,updated_at_ms) VALUES('youtube','legacydelete','https://www.youtube.com/watch?v=legacydelete','legacy-delete-item','ready',1,1)",
        [],
    )
    .expect("canonical identity");
    conn.execute(
        "INSERT INTO media_source_membership(service,media_id,source_subscription_id,source_kind,source_url_snapshot,source_title_snapshot,evidence_kind,created_at_ms,updated_at_ms) VALUES('youtube','legacydelete','legacy-subscription','subscription','https://www.youtube.com/@legacy','Legacy subscription','runtime',1,1)",
        [],
    )
    .expect("historical subscription membership");
    drop(conn);
    let aliases = crate::root_rebind::RootAliasesConfig {
        schema_version: 1,
        aliases: vec![crate::root_rebind::RootAliasRecord {
            id: "test-delete-alias".to_string(),
            from_root: r"C:\old_archive".to_string(),
            to_root: canonical_target_root.to_string_lossy().to_string(),
            verified_at_ms: 1,
            status: "active".to_string(),
            receipt_path: "test-receipt.json".to_string(),
        }],
    };
    crate::persistence::atomic_write_text(
        &paths.root_aliases_config_path(),
        &format!("{}\n", serde_json::to_string_pretty(&aliases).unwrap()),
    )
    .expect("alias config");

    assert_eq!(
        observe_media_path_fresh(&paths, stored_mp4),
        MediaPathObservation::Present,
        "historical stored identity must resolve to an available physical MP4"
    );
    let deduped = get_item_by_canonical_media_path(&paths, &current_mp4)
        .expect("alias-aware canonical lookup")
        .expect("dedupe identity");
    assert_eq!(deduped.id, "legacy-delete-item");
    assert_eq!(deduped.media_path, stored_mp4);

    let receipt = delete_library_item_files(
        &paths,
        &["legacy-delete-item".to_string()],
        "permanent",
        "operator",
    )
    .expect("delete receipt");
    assert_eq!(receipt.deleted, 1);
    assert_eq!(receipt.failed, 0);
    assert!(
        !current_mp4.exists(),
        "resolved physical MP4 must be deleted"
    );
    let item = get_item_by_id(&paths, "legacy-delete-item").expect("preserved item");
    assert_eq!(
        item.media_path, stored_mp4,
        "historical DB identity stays unchanged"
    );
    assert_eq!(item.container.as_deref(), Some("mp4"));
    assert_eq!(item.file_status, LIBRARY_FILE_STATUS_OPERATOR_DELETED);
    let conn = db::open_readonly(&paths).expect("matrix state");
    let membership_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM media_source_membership WHERE service='youtube' AND media_id='legacydelete' AND source_subscription_id='legacy-subscription'",
            [],
            |row| row.get(0),
        )
        .expect("membership count");
    assert_eq!(
        membership_count, 1,
        "file action must preserve source membership"
    );
    drop(conn);
    let redownload = operator_deleted_redownload_target(&paths, "legacy-delete-item")
        .expect("alias-aware retry target");
    assert_eq!(
        PathBuf::from(redownload.output_dir),
        canonical_target_root,
        "retry destination follows the verified physical root while lineage remains historical"
    );
    assert!(
        !target_root.join("legacy-delete.mkv").exists(),
        "historical compatibility must not silently convert the MP4"
    );
}

#[test]
fn disconnected_alias_restores_historical_mp4_read_open_reveal_availability_and_dedupe() {
    let dir = tempfile::tempdir().expect("tempdir");
    let paths = AppPaths::new(dir.path().join("app_data"));
    db::ensure_schema(&paths).expect("schema");
    let old_root = dir.path().join("restored_old_root");
    let target_root = dir.path().join("disconnected_direct_root");
    std::fs::create_dir_all(old_root.join("channel")).expect("old root");
    std::fs::create_dir_all(target_root.join("channel")).expect("initial target root");
    let old_mp4 = old_root.join("channel").join("legacy-restored.mp4");
    let canonical_target_root = target_root.canonicalize().expect("canonical alias target");
    let target_mp4 = canonical_target_root
        .join("channel")
        .join("legacy-restored.mp4");
    std::fs::write(&old_mp4, b"historical restored mp4").expect("historical file");
    std::fs::write(&target_mp4, b"direct target mp4").expect("target file");
    let stored_mp4 = old_mp4.to_string_lossy().to_string();
    let conn = db::open(&paths).expect("db");
    conn.execute(
        "INSERT INTO library_item(id,created_at_ms,source_type,source_uri,title,media_path,container) VALUES('legacy-restored-item',1,'local_file','https://www.youtube.com/watch?v=legacyrestored','Restored Legacy MP4',?1,'mp4')",
        [&stored_mp4],
    )
    .expect("historical row");
    drop(conn);
    let aliases = crate::root_rebind::RootAliasesConfig {
        schema_version: 1,
        aliases: vec![crate::root_rebind::RootAliasRecord {
            id: "disconnected-alias".to_string(),
            from_root: old_root.to_string_lossy().to_string(),
            to_root: canonical_target_root.to_string_lossy().to_string(),
            verified_at_ms: 1,
            status: "active".to_string(),
            receipt_path: "test-receipt.json".to_string(),
        }],
    };
    crate::persistence::atomic_write_text(
        &paths.root_aliases_config_path(),
        &format!("{}\n", serde_json::to_string_pretty(&aliases).unwrap()),
    )
    .expect("alias config");

    assert_eq!(
        resolve_media_path(&paths, &stored_mp4).expect("connected alias resolver"),
        target_mp4,
        "connected target remains the active physical read location"
    );
    std::fs::remove_dir_all(&target_root).expect("simulate direct target disconnect");
    std::thread::sleep(crate::root_rebind::ALIAS_TARGET_CACHE_TTL + Duration::from_millis(25));

    let resolved = resolve_media_path(&paths, &stored_mp4).expect("historical read resolver");
    assert_eq!(
        resolved, old_mp4,
        "open/reveal source must fall back to restored old root"
    );
    assert_eq!(
        observe_media_path_fresh(&paths, &stored_mp4),
        MediaPathObservation::Present,
        "availability must observe the restored historical file"
    );
    let deduped = get_item_by_canonical_media_path(&paths, &old_mp4)
        .expect("canonical lookup")
        .expect("historical row");
    assert_eq!(deduped.id, "legacy-restored-item");
    assert_eq!(deduped.container.as_deref(), Some("mp4"));
    assert_eq!(deduped.media_path, stored_mp4);

    let search = query_items_page(
        &paths,
        20,
        0,
        Some("available"),
        Some("restored legacy"),
        Some("video"),
        Some("all"),
        false,
        Some("title"),
        Some("asc"),
    )
    .expect("historical MP4 search");
    assert_eq!(search.filtered_total, 1);
    assert_eq!(search.items[0].id, "legacy-restored-item");
    let scan = list_youtube_video_candidates(&paths, 20, 0).expect("legacy candidate scan");
    assert!(scan.iter().any(|item| item.id == "legacy-restored-item"));

    let imported_path = old_root.join("imported-legacy.mp4");
    std::fs::write(&imported_path, b"legacy import fixture").expect("import fixture");
    let imported = import_local_file(&paths, &imported_path).expect("historical MP4 import");
    assert!(imported.media_path.to_ascii_lowercase().ends_with(".mp4"));
    assert!(
        imported_path.exists(),
        "import must not convert historical MP4"
    );

    let conn = db::open(&paths).expect("migration connection");
    db::migrate(&conn).expect("schema migration preserves legacy rows");
    drop(conn);
    assert_eq!(
        get_item_by_id(&paths, "legacy-restored-item")
            .expect("post-migration item")
            .media_path,
        stored_mp4,
        "schema migration must not rewrite historical MP4 identity"
    );

    let repaired_path = old_root.join("channel").join("legacy-repaired.mp4");
    std::fs::write(&repaired_path, b"historical repaired mp4").expect("repair target");
    let repaired = relocate_canonical_media(&paths, "legacy-restored-item", &repaired_path)
        .expect("repair relocation accepts MP4");
    assert!(repaired.media_path.to_ascii_lowercase().ends_with(".mp4"));
    assert!(
        repaired_path.exists(),
        "repair must not convert historical MP4"
    );
}

#[test]
fn alias_aware_dedupe_uses_indexed_bounded_candidates_without_library_scan() {
    let dir = tempfile::tempdir().expect("tempdir");
    let paths = AppPaths::new(dir.path().join("app_data"));
    db::ensure_schema(&paths).expect("schema");
    let target_root = dir.path().join("target");
    std::fs::create_dir_all(&target_root).expect("target");
    let current = target_root.join("wanted.mp4");
    std::fs::write(&current, b"historical").expect("media");
    let conn = db::open(&paths).expect("db");
    let tx = conn.unchecked_transaction().expect("transaction");
    for index in 0..2_000 {
        tx.execute(
            "INSERT INTO library_item(id,created_at_ms,source_type,source_uri,title,media_path) VALUES(?1,?2,'local_file',?3,?4,?5)",
            rusqlite::params![
                format!("unrelated-{index}"),
                index as i64,
                format!("file://unrelated/{index}"),
                format!("Unrelated {index}"),
                format!(r"Q:\offline\unrelated-{index}.mp4")
            ],
        )
        .expect("unrelated row");
    }
    tx.execute(
        "INSERT INTO library_item(id,created_at_ms,source_type,source_uri,title,media_path) VALUES('wanted',3000,'local_file','file://wanted','Wanted',?1)",
        [r"C:\old\wanted.mp4"],
    )
    .expect("wanted row");
    tx.commit().expect("commit");
    let plan = conn
        .prepare(
            "EXPLAIN QUERY PLAN SELECT id FROM library_item WHERE media_path=?1 COLLATE NOCASE ORDER BY created_at_ms DESC LIMIT 1",
        )
        .expect("plan")
        .query_map([r"C:\old\wanted.mp4"], |row| row.get::<_, String>(3))
        .expect("plan rows")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("plan details")
        .join(" ");
    assert!(
        plan.contains("idx_library_item_media_path"),
        "exact alias candidate lookup must use the NOCASE media-path index: {plan}"
    );
    drop(conn);
    let aliases = crate::root_rebind::RootAliasesConfig {
        schema_version: 1,
        aliases: vec![crate::root_rebind::RootAliasRecord {
            id: "bounded-candidate".to_string(),
            from_root: r"C:\old".to_string(),
            to_root: target_root
                .canonicalize()
                .expect("canonical alias target")
                .to_string_lossy()
                .to_string(),
            verified_at_ms: 1,
            status: "active".to_string(),
            receipt_path: "test.json".to_string(),
        }],
    };
    crate::persistence::atomic_write_text(
        &paths.root_aliases_config_path(),
        &format!("{}\n", serde_json::to_string_pretty(&aliases).unwrap()),
    )
    .expect("aliases");
    let candidates = crate::root_rebind::historical_alias_candidates(
        &aliases.aliases,
        &current
            .canonicalize()
            .expect("canonical current")
            .to_string_lossy(),
    )
    .expect("historical candidates");
    assert_eq!(
        candidates,
        vec![r"C:\old\wanted.mp4".to_string()],
        "target={} canonical={}",
        target_root.to_string_lossy(),
        current.canonicalize().unwrap().to_string_lossy()
    );
    let found = get_item_by_canonical_media_path(&paths, &current)
        .expect("indexed lookup")
        .expect("wanted row");
    assert_eq!(found.id, "wanted");
}
