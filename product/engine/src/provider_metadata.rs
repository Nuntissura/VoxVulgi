use crate::db;
use crate::paths::AppPaths;
use crate::{EngineError, Result};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderMetadataQuality {
    Unknown,
    Filename,
    Imported,
    RemotePartial,
    RemoteCanonical,
}

impl ProviderMetadataQuality {
    pub fn rank(self) -> i64 {
        match self {
            Self::Unknown => 0,
            Self::Filename => 10,
            Self::Imported => 20,
            Self::RemotePartial => 30,
            Self::RemoteCanonical => 40,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Filename => "filename",
            Self::Imported => "imported",
            Self::RemotePartial => "remote_partial",
            Self::RemoteCanonical => "remote_canonical",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderMetadataObservation {
    pub service: String,
    pub media_id: String,
    pub raw_title: Option<String>,
    pub uploader_id: Option<String>,
    pub uploader_name: Option<String>,
    pub canonical_url: Option<String>,
    pub source_url: Option<String>,
    pub published_at_ms: Option<i64>,
    pub thumbnail_url: Option<String>,
    pub provider_name: String,
    pub provider_version: Option<String>,
    #[serde(default)]
    pub capability_epoch: i64,
    pub quality: ProviderMetadataQuality,
    pub source_operation: String,
    pub source_job_id: Option<String>,
    pub source_subscription_id: Option<String>,
    pub observed_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderMetadataUpsertReceipt {
    pub service: String,
    pub media_id: String,
    pub observation_id: String,
    pub accepted: bool,
    pub decision_reason: String,
    pub quality_class: String,
    pub quality_rank: i64,
    pub observed_at_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisplayTitleProvenance {
    OperatorOverride,
    CanonicalRemote,
    ImportedOrFile,
    StableProviderId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedDisplayTitle {
    pub value: String,
    pub provenance: DisplayTitleProvenance,
    pub damaged: bool,
    pub placeholder: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderTitleRepairClass {
    AlreadyCanonical,
    Missing,
    Placeholder,
    Damaged,
    FilenameOnly,
    ValidSnapshot,
    Conflict,
    IdentityMissing,
    Unavailable,
}

impl ProviderTitleRepairClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AlreadyCanonical => "already_canonical",
            Self::Missing => "missing",
            Self::Placeholder => "placeholder",
            Self::Damaged => "damaged",
            Self::FilenameOnly => "filename_only",
            Self::ValidSnapshot => "valid_snapshot",
            Self::Conflict => "conflict",
            Self::IdentityMissing => "identity_missing",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderTitleRepairPageReceipt {
    pub state: String,
    pub page_scanned: usize,
    pub page_repaired: usize,
    pub cumulative_scanned: i64,
    pub cumulative_repaired: i64,
    pub cumulative_conflicts: i64,
    pub cumulative_unavailable: i64,
    pub classifications: BTreeMap<String, usize>,
    pub after_job_created_at_ms: Option<i64>,
    pub after_job_id: Option<String>,
    pub completed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderTitleRepairStatus {
    pub state: String,
    pub scanned: i64,
    pub repaired: i64,
    pub conflicts: i64,
    pub unavailable: i64,
    pub total_candidates: i64,
    pub remaining_candidates: i64,
    pub canonical_identities: i64,
    pub canonical_titles: i64,
    pub observation_receipts: i64,
    pub repair_change_receipts: i64,
}

#[derive(Debug, Clone, Default)]
struct RepairCandidate {
    override_title: Option<String>,
    remote_title: Option<String>,
    imported_title: Option<String>,
}

#[derive(Debug, Clone)]
struct RepairRequest {
    job_id: String,
    service: String,
    media_id: String,
    source_url: String,
    current_title: Option<String>,
}

fn valid_candidate_title(value: Option<String>, service: &str, media_id: &str) -> Option<String> {
    normalized_optional(value).filter(|title| {
        !title_contains_encoding_damage(title)
            && !title_is_provider_placeholder(service, media_id, title)
    })
}

fn classify_repair_title(
    service: &str,
    media_id: &str,
    current: Option<&str>,
    candidate: &RepairCandidate,
) -> ProviderTitleRepairClass {
    let current = current.map(str::trim).filter(|value| !value.is_empty());
    let Some(current) = current else {
        return ProviderTitleRepairClass::Missing;
    };
    if title_contains_encoding_damage(current) {
        return ProviderTitleRepairClass::Damaged;
    }
    if title_is_provider_placeholder(service, media_id, current) {
        return ProviderTitleRepairClass::Placeholder;
    }
    if candidate.override_title.as_deref() == Some(current)
        || candidate.remote_title.as_deref() == Some(current)
    {
        return ProviderTitleRepairClass::AlreadyCanonical;
    }
    if candidate.imported_title.as_deref() == Some(current) {
        return ProviderTitleRepairClass::FilenameOnly;
    }
    if candidate.override_title.is_some() || candidate.remote_title.is_some() {
        return ProviderTitleRepairClass::Conflict;
    }
    ProviderTitleRepairClass::ValidSnapshot
}

fn repair_candidates_batch(
    conn: &rusqlite::Connection,
    requests: &[RepairRequest],
) -> Result<BTreeMap<String, RepairCandidate>> {
    let mut candidates = BTreeMap::new();
    for chunk in requests.chunks(150) {
        if chunk.is_empty() {
            continue;
        }
        let value_rows = std::iter::repeat_n("(?,?,?,?)", chunk.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            r#"
WITH requested(job_id,service,media_id,source_url) AS (VALUES {value_rows})
SELECT requested.job_id,requested.service,requested.media_id,
       title_override.title,metadata.raw_title,
       COALESCE(linked_library.title,direct_library.title)
FROM requested
LEFT JOIN media_title_override title_override
  ON title_override.service=requested.service AND title_override.media_id=requested.media_id
LEFT JOIN media_provider_metadata metadata
  ON metadata.service=requested.service AND metadata.media_id=requested.media_id
LEFT JOIN media_source_identity identity
  ON identity.service=requested.service AND identity.media_id=requested.media_id
LEFT JOIN library_item linked_library ON linked_library.id=identity.library_item_id
LEFT JOIN library_item direct_library ON direct_library.id=(
  SELECT item.id FROM library_item item
  WHERE item.source_uri=requested.source_url OR item.source_uri=identity.canonical_url
  ORDER BY item.created_at_ms DESC,item.id DESC LIMIT 1
)
"#
        );
        let mut values = Vec::with_capacity(chunk.len() * 4);
        for request in chunk {
            values.push(rusqlite::types::Value::Text(request.job_id.clone()));
            values.push(rusqlite::types::Value::Text(request.service.clone()));
            values.push(rusqlite::types::Value::Text(request.media_id.clone()));
            values.push(rusqlite::types::Value::Text(request.source_url.clone()));
        }
        let mut statement = conn.prepare(&sql)?;
        let rows = statement.query_map(rusqlite::params_from_iter(values.iter()), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })?;
        for row in rows {
            let (job_id, service, media_id, override_title, remote_title, imported_title) = row?;
            candidates.insert(
                job_id,
                RepairCandidate {
                    override_title: valid_candidate_title(override_title, &service, &media_id),
                    remote_title: valid_candidate_title(remote_title, &service, &media_id),
                    imported_title: valid_candidate_title(imported_title, &service, &media_id),
                },
            );
        }
    }
    Ok(candidates)
}

pub fn repair_provider_titles_page(
    paths: &AppPaths,
    requested_limit: usize,
) -> Result<ProviderTitleRepairPageReceipt> {
    let limit = requested_limit.clamp(1, 500);
    let mut conn = db::write_context(paths)?;
    let checkpoint = conn.query_row(
        "SELECT state,after_job_created_at_ms,after_job_id,scanned_count,repaired_count,conflict_count,unavailable_count FROM media_provider_metadata_repair_checkpoint WHERE singleton=1",
        [],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<i64>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
            ))
        },
    )?;
    if checkpoint.0 == "completed" {
        return Ok(ProviderTitleRepairPageReceipt {
            state: checkpoint.0,
            page_scanned: 0,
            page_repaired: 0,
            cumulative_scanned: checkpoint.3,
            cumulative_repaired: checkpoint.4,
            cumulative_conflicts: checkpoint.5,
            cumulative_unavailable: checkpoint.6,
            classifications: BTreeMap::new(),
            after_job_created_at_ms: checkpoint.1,
            after_job_id: checkpoint.2,
            completed: true,
        });
    }

    let mut statement = conn.prepare(
        "SELECT id,created_at_ms,target_title,params_json FROM job
         WHERE type='download_direct_url'
           AND (?1 IS NULL OR created_at_ms>?1 OR (created_at_ms=?1 AND id>COALESCE(?2,'')))
         ORDER BY created_at_ms,id LIMIT ?3",
    )?;
    let rows = statement
        .query_map(
            params![checkpoint.1, checkpoint.2, (limit + 1) as i64],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);
    let has_more = rows.len() > limit;
    let page = &rows[..rows.len().min(limit)];
    let mut classifications = BTreeMap::new();
    let mut repairs = Vec::new();
    let mut repair_requests = Vec::new();
    let mut page_conflicts = 0_i64;
    let mut page_unavailable = 0_i64;

    for (job_id, _, current_title, params_json) in page {
        let source_url = serde_json::from_str::<serde_json::Value>(params_json)
            .ok()
            .and_then(|value| {
                value
                    .get("url")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
            });
        let Some(source_url) = source_url else {
            *classifications
                .entry(
                    ProviderTitleRepairClass::IdentityMissing
                        .as_str()
                        .to_string(),
                )
                .or_insert(0) += 1;
            page_unavailable += 1;
            continue;
        };
        let Some(source) = crate::library::canonical_media_source(&source_url) else {
            *classifications
                .entry(
                    ProviderTitleRepairClass::IdentityMissing
                        .as_str()
                        .to_string(),
                )
                .or_insert(0) += 1;
            page_unavailable += 1;
            continue;
        };
        repair_requests.push(RepairRequest {
            job_id: job_id.clone(),
            service: source.service,
            media_id: source.media_id,
            source_url,
            current_title: current_title.clone(),
        });
    }

    let candidates = repair_candidates_batch(&conn, &repair_requests)?;
    for request in repair_requests {
        let candidate = candidates.get(&request.job_id).cloned().unwrap_or_default();
        let class = classify_repair_title(
            &request.service,
            &request.media_id,
            request.current_title.as_deref(),
            &candidate,
        );
        *classifications
            .entry(class.as_str().to_string())
            .or_insert(0) += 1;
        if class == ProviderTitleRepairClass::Conflict {
            page_conflicts += 1;
        }
        if matches!(
            class,
            ProviderTitleRepairClass::Missing
                | ProviderTitleRepairClass::Placeholder
                | ProviderTitleRepairClass::Damaged
        ) {
            let selected = candidate
                .override_title
                .as_ref()
                .map(|title| (title.clone(), "operator_override"))
                .or_else(|| {
                    candidate
                        .remote_title
                        .as_ref()
                        .map(|title| (title.clone(), "canonical_remote"))
                })
                .or_else(|| {
                    candidate
                        .imported_title
                        .as_ref()
                        .map(|title| (title.clone(), "imported_file"))
                });
            if let Some((after_title, provenance)) = selected {
                repairs.push((
                    request.job_id,
                    request.service,
                    request.media_id,
                    class,
                    request.current_title,
                    after_title,
                    provenance,
                ));
            } else {
                page_unavailable += 1;
                *classifications
                    .entry(ProviderTitleRepairClass::Unavailable.as_str().to_string())
                    .or_insert(0) += 1;
            }
        }
    }

    let after = page
        .last()
        .map(|(job_id, created_at_ms, _, _)| (*created_at_ms, job_id.clone()));
    let completed = !has_more;
    let now = now_ms();
    let tx = conn.transaction()?;
    let mut page_repaired = 0_usize;
    for (job_id, service, media_id, class, before_title, after_title, provenance) in repairs {
        let changed = tx.execute(
            "UPDATE job SET target_title=?1 WHERE id=?2 AND target_title IS ?3",
            params![after_title, job_id, before_title],
        )?;
        if changed == 1 {
            page_repaired += 1;
            tx.execute(
                "INSERT OR IGNORE INTO media_provider_metadata_repair_change(
                   change_id,job_id,service,media_id,classification,before_title,after_title,
                   title_provenance,changed_at_ms
                 ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![
                    Uuid::new_v4().to_string(),
                    job_id,
                    service,
                    media_id,
                    class.as_str(),
                    before_title,
                    after_title,
                    provenance,
                    now,
                ],
            )?;
        }
    }
    let cumulative_scanned = checkpoint.3 + page.len() as i64;
    let cumulative_repaired = checkpoint.4 + page_repaired as i64;
    let cumulative_conflicts = checkpoint.5 + page_conflicts;
    let cumulative_unavailable = checkpoint.6 + page_unavailable;
    let (after_created_at, after_job_id) = after
        .map(|(created_at, id)| (Some(created_at), Some(id)))
        .unwrap_or((checkpoint.1, checkpoint.2));
    let state = if completed { "completed" } else { "running" };
    tx.execute(
        "UPDATE media_provider_metadata_repair_checkpoint SET
           state=?1,after_job_created_at_ms=?2,after_job_id=?3,scanned_count=?4,
           repaired_count=?5,conflict_count=?6,unavailable_count=?7,updated_at_ms=?8,
           last_error=NULL WHERE singleton=1",
        params![
            state,
            after_created_at,
            after_job_id,
            cumulative_scanned,
            cumulative_repaired,
            cumulative_conflicts,
            cumulative_unavailable,
            now,
        ],
    )?;
    tx.commit()?;
    Ok(ProviderTitleRepairPageReceipt {
        state: state.to_string(),
        page_scanned: page.len(),
        page_repaired,
        cumulative_scanned,
        cumulative_repaired,
        cumulative_conflicts,
        cumulative_unavailable,
        classifications,
        after_job_created_at_ms: after_created_at,
        after_job_id,
        completed,
    })
}

pub fn reset_provider_title_repair_checkpoint(paths: &AppPaths, now_ms: i64) -> Result<()> {
    let conn = db::write_context(paths)?;
    conn.execute(
        "UPDATE media_provider_metadata_repair_checkpoint SET
           state='idle',after_job_created_at_ms=NULL,after_job_id=NULL,scanned_count=0,
           repaired_count=0,conflict_count=0,unavailable_count=0,updated_at_ms=?1,
           last_error=NULL WHERE singleton=1",
        [now_ms],
    )?;
    Ok(())
}

pub fn provider_title_repair_status(paths: &AppPaths) -> Result<ProviderTitleRepairStatus> {
    let conn = db::open_readonly(paths)?;
    let checkpoint = conn.query_row(
        "SELECT state,after_job_created_at_ms,after_job_id,scanned_count,repaired_count,conflict_count,unavailable_count FROM media_provider_metadata_repair_checkpoint WHERE singleton=1",
        [],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<i64>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
            ))
        },
    )?;
    let total_candidates = conn.query_row(
        "SELECT COUNT(*) FROM job WHERE type='download_direct_url'",
        [],
        |row| row.get(0),
    )?;
    let remaining_candidates = conn.query_row(
        "SELECT COUNT(*) FROM job WHERE type='download_direct_url' AND (?1 IS NULL OR created_at_ms>?1 OR (created_at_ms=?1 AND id>COALESCE(?2,'')))",
        params![checkpoint.1, checkpoint.2],
        |row| row.get(0),
    )?;
    let (canonical_identities, canonical_titles, observation_receipts, repair_change_receipts) =
        conn.query_row(
            r#"
SELECT
  (SELECT COUNT(*) FROM media_provider_metadata),
  (SELECT COUNT(*) FROM media_provider_metadata WHERE raw_title IS NOT NULL),
  (SELECT COUNT(*) FROM media_provider_metadata_observation),
  (SELECT COUNT(*) FROM media_provider_metadata_repair_change)
"#,
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
    Ok(ProviderTitleRepairStatus {
        state: checkpoint.0,
        scanned: checkpoint.3,
        repaired: checkpoint.4,
        conflicts: checkpoint.5,
        unavailable: checkpoint.6,
        total_candidates,
        remaining_candidates,
        canonical_identities,
        canonical_titles,
        observation_receipts,
        repair_change_receipts,
    })
}

fn required_trimmed(value: &str, label: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(EngineError::InstallFailed(format!(
            "provider metadata {label} is required"
        )));
    }
    Ok(trimmed.to_string())
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

fn normalized_optional(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

pub fn normalize_title_for_search(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

pub fn title_contains_encoding_damage(value: &str) -> bool {
    value.contains('\u{fffd}')
}

pub fn title_is_provider_placeholder(service: &str, media_id: &str, value: &str) -> bool {
    let normalized = normalize_title_for_search(value);
    let media_id = normalize_title_for_search(media_id);
    let expected = match service.trim().to_ascii_lowercase().as_str() {
        "youtube" => format!("youtube video {media_id}"),
        "instagram" => format!("instagram media {media_id}"),
        "tiktok" => format!("tiktok video {media_id}"),
        _ => return false,
    };
    normalized == expected
}

pub fn parse_provider_json_line(bytes: &[u8]) -> Result<ProviderMetadataObservation> {
    // serde_json rejects malformed UTF-8. Never use from_utf8_lossy at this provider boundary:
    // replacement characters would become durable title corruption with no recoverable bytes.
    Ok(serde_json::from_slice(bytes)?)
}

fn normalize_provider_metadata_observation(
    mut observation: ProviderMetadataObservation,
) -> Result<ProviderMetadataObservation> {
    observation.service = required_trimmed(&observation.service, "service")?.to_ascii_lowercase();
    observation.media_id = required_trimmed(&observation.media_id, "media_id")?;
    observation.provider_name = required_trimmed(&observation.provider_name, "provider_name")?;
    observation.source_operation =
        required_trimmed(&observation.source_operation, "source_operation")?;
    if observation.observed_at_ms < 0 || observation.capability_epoch < 0 {
        return Err(EngineError::InstallFailed(
            "provider metadata timestamps and capability epoch must be non-negative".to_string(),
        ));
    }
    observation.raw_title = normalized_optional(observation.raw_title);
    observation.uploader_id = normalized_optional(observation.uploader_id);
    observation.uploader_name = normalized_optional(observation.uploader_name);
    observation.canonical_url = normalized_optional(observation.canonical_url);
    observation.source_url = normalized_optional(observation.source_url);
    observation.thumbnail_url = normalized_optional(observation.thumbnail_url);
    observation.provider_version = normalized_optional(observation.provider_version);
    observation.source_job_id = normalized_optional(observation.source_job_id);
    observation.source_subscription_id = normalized_optional(observation.source_subscription_id);
    Ok(observation)
}

fn upsert_provider_metadata_tx(
    tx: &rusqlite::Transaction<'_>,
    observation: ProviderMetadataObservation,
) -> Result<ProviderMetadataUpsertReceipt> {
    let quality_rank = observation.quality.rank();
    let normalized_title = observation
        .raw_title
        .as_deref()
        .map(normalize_title_for_search);
    let existing = tx
        .query_row(
            "SELECT quality_rank,observed_at_ms FROM media_provider_metadata WHERE service=?1 AND media_id=?2",
            params![observation.service, observation.media_id],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()?;
    let (accepted, decision_reason) = match existing {
        None => (true, "new_identity".to_string()),
        Some((existing_rank, _)) if quality_rank > existing_rank => {
            (true, "higher_quality".to_string())
        }
        Some((existing_rank, existing_observed_at_ms))
            if quality_rank == existing_rank
                && observation.observed_at_ms > existing_observed_at_ms =>
        {
            (true, "same_quality_newer".to_string())
        }
        Some((existing_rank, existing_observed_at_ms))
            if quality_rank == existing_rank
                && observation.observed_at_ms == existing_observed_at_ms =>
        {
            (false, "same_quality_equal_timestamp_rejected".to_string())
        }
        Some((existing_rank, _)) if quality_rank < existing_rank => {
            (false, "lower_quality_rejected".to_string())
        }
        Some(_) => (false, "older_observation_rejected".to_string()),
    };

    let observation_id = Uuid::new_v4().to_string();
    let payload_json = serde_json::to_string(&observation)?;
    tx.execute(
        "INSERT INTO media_provider_metadata_observation(
           observation_id,service,media_id,observed_at_ms,provider_name,provider_version,
           capability_epoch,quality_class,quality_rank,source_operation,source_job_id,
           source_subscription_id,payload_json,accepted,decision_reason
         ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
        params![
            observation_id,
            observation.service,
            observation.media_id,
            observation.observed_at_ms,
            observation.provider_name,
            observation.provider_version,
            observation.capability_epoch,
            observation.quality.as_str(),
            quality_rank,
            observation.source_operation,
            observation.source_job_id,
            observation.source_subscription_id,
            payload_json,
            i64::from(accepted),
            decision_reason,
        ],
    )?;

    if accepted {
        tx.execute(
            "INSERT INTO media_provider_metadata(
               service,media_id,raw_title,normalized_title,uploader_id,uploader_name,
               canonical_url,source_url,published_at_ms,thumbnail_url,provider_name,
               provider_version,capability_epoch,quality_class,quality_rank,source_operation,
               source_job_id,source_subscription_id,observed_at_ms,updated_at_ms
             ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?19)
             ON CONFLICT(service,media_id) DO UPDATE SET
               raw_title=COALESCE(excluded.raw_title,media_provider_metadata.raw_title),
               normalized_title=COALESCE(excluded.normalized_title,media_provider_metadata.normalized_title),
               uploader_id=COALESCE(excluded.uploader_id,media_provider_metadata.uploader_id),
               uploader_name=COALESCE(excluded.uploader_name,media_provider_metadata.uploader_name),
               canonical_url=COALESCE(excluded.canonical_url,media_provider_metadata.canonical_url),
               source_url=COALESCE(excluded.source_url,media_provider_metadata.source_url),
               published_at_ms=COALESCE(excluded.published_at_ms,media_provider_metadata.published_at_ms),
               thumbnail_url=COALESCE(excluded.thumbnail_url,media_provider_metadata.thumbnail_url),
               provider_name=excluded.provider_name,
               provider_version=COALESCE(excluded.provider_version,media_provider_metadata.provider_version),
               capability_epoch=excluded.capability_epoch,
               quality_class=excluded.quality_class,
               quality_rank=excluded.quality_rank,
               source_operation=excluded.source_operation,
               source_job_id=COALESCE(excluded.source_job_id,media_provider_metadata.source_job_id),
               source_subscription_id=COALESCE(excluded.source_subscription_id,media_provider_metadata.source_subscription_id),
               observed_at_ms=excluded.observed_at_ms,
               updated_at_ms=excluded.updated_at_ms",
            params![
                observation.service,
                observation.media_id,
                observation.raw_title,
                normalized_title,
                observation.uploader_id,
                observation.uploader_name,
                observation.canonical_url,
                observation.source_url,
                observation.published_at_ms,
                observation.thumbnail_url,
                observation.provider_name,
                observation.provider_version,
                observation.capability_epoch,
                observation.quality.as_str(),
                quality_rank,
                observation.source_operation,
                observation.source_job_id,
                observation.source_subscription_id,
                observation.observed_at_ms,
            ],
        )?;
    }
    Ok(ProviderMetadataUpsertReceipt {
        service: observation.service,
        media_id: observation.media_id,
        observation_id,
        accepted,
        decision_reason,
        quality_class: observation.quality.as_str().to_string(),
        quality_rank,
        observed_at_ms: observation.observed_at_ms,
    })
}

pub fn upsert_provider_metadata(
    paths: &AppPaths,
    observation: ProviderMetadataObservation,
) -> Result<ProviderMetadataUpsertReceipt> {
    let observation = normalize_provider_metadata_observation(observation)?;
    let mut conn = db::write_context(paths)?;
    let tx = conn.transaction()?;
    let receipt = upsert_provider_metadata_tx(&tx, observation)?;
    tx.commit()?;
    Ok(receipt)
}

pub fn upsert_provider_metadata_batch(
    paths: &AppPaths,
    observations: Vec<ProviderMetadataObservation>,
) -> Result<Vec<ProviderMetadataUpsertReceipt>> {
    if observations.is_empty() {
        return Ok(Vec::new());
    }
    let observations = observations
        .into_iter()
        .map(normalize_provider_metadata_observation)
        .collect::<Result<Vec<_>>>()?;
    let mut conn = db::write_context(paths)?;
    let tx = conn.transaction()?;
    let mut receipts = Vec::with_capacity(observations.len());
    for observation in observations {
        receipts.push(upsert_provider_metadata_tx(&tx, observation)?);
    }
    tx.commit()?;
    Ok(receipts)
}

pub fn set_operator_title_override(
    paths: &AppPaths,
    service: &str,
    media_id: &str,
    title: &str,
    attribution: &str,
    now_ms: i64,
) -> Result<()> {
    let service = required_trimmed(service, "service")?.to_ascii_lowercase();
    let media_id = required_trimmed(media_id, "media_id")?;
    let title = required_trimmed(title, "operator title")?;
    let attribution = required_trimmed(attribution, "override attribution")?;
    let conn = db::write_context(paths)?;
    conn.execute(
        "INSERT INTO media_title_override(service,media_id,title,attribution,created_at_ms,updated_at_ms)
         VALUES(?1,?2,?3,?4,?5,?5)
         ON CONFLICT(service,media_id) DO UPDATE SET
           title=excluded.title,attribution=excluded.attribution,updated_at_ms=excluded.updated_at_ms",
        params![service, media_id, title, attribution, now_ms],
    )?;
    Ok(())
}

pub fn resolve_display_title(
    paths: &AppPaths,
    service: &str,
    media_id: &str,
    imported_or_file_title: Option<&str>,
) -> Result<ResolvedDisplayTitle> {
    let service = required_trimmed(service, "service")?.to_ascii_lowercase();
    let media_id = required_trimmed(media_id, "media_id")?;
    let conn = db::open_readonly(paths)?;
    let (override_title, remote_title) = conn.query_row(
        r#"
SELECT title_override.title,metadata.raw_title
FROM (SELECT ?1 AS service,?2 AS media_id) requested
LEFT JOIN media_title_override title_override
  ON title_override.service=requested.service AND title_override.media_id=requested.media_id
LEFT JOIN media_provider_metadata metadata
  ON metadata.service=requested.service AND metadata.media_id=requested.media_id
"#,
        params![service, media_id],
        |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(1)?,
            ))
        },
    )?;
    Ok(resolve_display_title_values(
        &service,
        &media_id,
        override_title,
        remote_title,
        imported_or_file_title.map(str::to_string),
    ))
}

pub fn resolve_library_display_titles(
    paths: &AppPaths,
    items: &[(String, String)],
) -> Result<BTreeMap<String, ResolvedDisplayTitle>> {
    let conn = db::open_readonly(paths)?;
    let imported_by_item = items.iter().cloned().collect::<BTreeMap<_, _>>();
    let mut resolved = BTreeMap::new();
    for chunk in items.chunks(300) {
        if chunk.is_empty() {
            continue;
        }
        let placeholders = std::iter::repeat_n("?", chunk.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            r#"
SELECT identity.library_item_id,identity.service,identity.media_id,
       title_override.title,metadata.raw_title
FROM media_source_identity identity
LEFT JOIN library_download_lineage lineage ON lineage.item_id=identity.library_item_id
LEFT JOIN media_title_override title_override
  ON title_override.service=identity.service AND title_override.media_id=identity.media_id
LEFT JOIN media_provider_metadata metadata
  ON metadata.service=identity.service AND metadata.media_id=identity.media_id
WHERE identity.library_item_id IN ({placeholders})
ORDER BY identity.library_item_id,
  CASE WHEN lineage.service IS NOT NULL AND identity.service=lineage.service THEN 0 ELSE 1 END,
  CASE identity.service WHEN 'youtube' THEN 0 WHEN 'instagram' THEN 1 WHEN 'tiktok' THEN 2 ELSE 3 END,
  identity.media_id
"#
        );
        let mut statement = conn.prepare(&sql)?;
        let rows = statement.query_map(
            rusqlite::params_from_iter(chunk.iter().map(|(item_id, _)| item_id.as_str())),
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            },
        )?;
        for row in rows {
            let (item_id, service, media_id, override_title, remote_title) = row?;
            if resolved.contains_key(&item_id) {
                continue;
            }
            let imported = imported_by_item.get(&item_id).cloned();
            resolved.insert(
                item_id,
                resolve_display_title_values(
                    &service,
                    &media_id,
                    override_title,
                    remote_title,
                    imported,
                ),
            );
        }
    }
    Ok(resolved)
}

fn resolve_display_title_values(
    service: &str,
    media_id: &str,
    override_title: Option<String>,
    remote_title: Option<String>,
    imported_or_file_title: Option<String>,
) -> ResolvedDisplayTitle {
    if let Some(title) = normalized_optional(override_title) {
        return resolved_title(
            title,
            DisplayTitleProvenance::OperatorOverride,
            service,
            media_id,
        );
    }
    if let Some(title) = valid_candidate_title(remote_title, service, media_id) {
        return resolved_title(
            title,
            DisplayTitleProvenance::CanonicalRemote,
            service,
            media_id,
        );
    }
    if let Some(title) = normalized_optional(imported_or_file_title) {
        return resolved_title(
            title,
            DisplayTitleProvenance::ImportedOrFile,
            service,
            media_id,
        );
    }
    resolved_title(
        format!("{} {}", service_display_name(service), media_id),
        DisplayTitleProvenance::StableProviderId,
        service,
        media_id,
    )
}

fn service_display_name(service: &str) -> String {
    let mut chars = service.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => "Provider".to_string(),
    }
}

fn resolved_title(
    value: String,
    provenance: DisplayTitleProvenance,
    service: &str,
    media_id: &str,
) -> ResolvedDisplayTitle {
    ResolvedDisplayTitle {
        damaged: title_contains_encoding_damage(&value),
        placeholder: title_is_provider_placeholder(service, media_id, &value),
        value,
        provenance,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(
        title: &str,
        quality: ProviderMetadataQuality,
        observed_at_ms: i64,
    ) -> ProviderMetadataObservation {
        ProviderMetadataObservation {
            service: "YouTube".to_string(),
            media_id: "abc123".to_string(),
            raw_title: Some(title.to_string()),
            uploader_id: Some("channel-id".to_string()),
            uploader_name: Some("채널 日本語".to_string()),
            canonical_url: Some("https://www.youtube.com/watch?v=abc123".to_string()),
            source_url: Some("https://youtu.be/abc123".to_string()),
            published_at_ms: Some(123),
            thumbnail_url: Some("https://i.example/thumb.jpg".to_string()),
            provider_name: "yt-dlp".to_string(),
            provider_version: Some("2026.07.04".to_string()),
            capability_epoch: 7,
            quality,
            source_operation: "single_metadata".to_string(),
            source_job_id: Some("job-1".to_string()),
            source_subscription_id: None,
            observed_at_ms,
        }
    }

    #[test]
    fn structured_utf8_metadata_round_trips_without_lossy_replacement() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app"));
        db::ensure_schema(&paths).expect("schema");
        let title = "한국어 日本語 😀 العربية\tline\n two";
        let bytes = serde_json::to_vec(&fixture(
            title,
            ProviderMetadataQuality::RemoteCanonical,
            100,
        ))
        .expect("json");
        let parsed = parse_provider_json_line(&bytes).expect("strict JSON parse");
        upsert_provider_metadata(&paths, parsed).expect("upsert");
        let conn = db::open_readonly(&paths).expect("readonly");
        let stored: (String, String) = conn
            .query_row(
                "SELECT raw_title,normalized_title FROM media_provider_metadata WHERE service='youtube' AND media_id='abc123'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("metadata row");
        assert_eq!(stored.0, title);
        assert_eq!(stored.1, "한국어 日本語 😀 العربية line two");
        assert!(parse_provider_json_line(b"{\"raw_title\":\"bad\xff\"}").is_err());
    }

    #[test]
    fn batch_upsert_is_atomic_and_preserves_each_observation_receipt() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app"));
        db::ensure_schema(&paths).expect("schema");
        let mut second = fixture(
            "두 번째 제목",
            ProviderMetadataQuality::RemoteCanonical,
            101,
        );
        second.media_id = "second-id".to_string();
        let receipts = upsert_provider_metadata_batch(
            &paths,
            vec![
                fixture(
                    "첫 번째 제목",
                    ProviderMetadataQuality::RemoteCanonical,
                    100,
                ),
                second,
            ],
        )
        .expect("batch upsert");
        assert_eq!(receipts.len(), 2);
        assert!(receipts.iter().all(|receipt| receipt.accepted));

        let mut invalid = fixture(
            "invalid batch member",
            ProviderMetadataQuality::RemoteCanonical,
            102,
        );
        invalid.media_id = " ".to_string();
        let mut would_be_valid = fixture(
            "must roll back",
            ProviderMetadataQuality::RemoteCanonical,
            102,
        );
        would_be_valid.media_id = "rollback-id".to_string();
        assert!(upsert_provider_metadata_batch(&paths, vec![would_be_valid, invalid]).is_err());

        let conn = db::open_readonly(&paths).expect("readonly");
        let rollback_rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM media_provider_metadata WHERE media_id='rollback-id'",
                [],
                |row| row.get(0),
            )
            .expect("rollback count");
        assert_eq!(rollback_rows, 0);
    }

    #[test]
    fn quality_and_time_precedence_reject_regression_but_keeps_history() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app"));
        db::ensure_schema(&paths).expect("schema");
        assert!(
            upsert_provider_metadata(
                &paths,
                fixture(
                    "Canonical title",
                    ProviderMetadataQuality::RemoteCanonical,
                    200
                ),
            )
            .expect("canonical")
            .accepted
        );
        let lower = upsert_provider_metadata(
            &paths,
            fixture("filename_abc123", ProviderMetadataQuality::Filename, 300),
        )
        .expect("lower quality");
        assert!(!lower.accepted);
        let older = upsert_provider_metadata(
            &paths,
            fixture("Older title", ProviderMetadataQuality::RemoteCanonical, 100),
        )
        .expect("older");
        assert!(!older.accepted);
        let equal_timestamp = upsert_provider_metadata(
            &paths,
            fixture(
                "Conflicting equal-time title",
                ProviderMetadataQuality::RemoteCanonical,
                200,
            ),
        )
        .expect("equal timestamp");
        assert!(!equal_timestamp.accepted);
        assert_eq!(
            equal_timestamp.decision_reason,
            "same_quality_equal_timestamp_rejected"
        );
        let conn = db::open_readonly(&paths).expect("readonly");
        let stored: String = conn
            .query_row(
                "SELECT raw_title FROM media_provider_metadata WHERE service='youtube' AND media_id='abc123'",
                [],
                |row| row.get(0),
            )
            .expect("title");
        assert_eq!(stored, "Canonical title");
        let observations: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM media_provider_metadata_observation WHERE service='youtube' AND media_id='abc123'",
                [],
                |row| row.get(0),
            )
            .expect("history count");
        assert_eq!(observations, 4);
    }

    #[test]
    fn resolver_preserves_operator_override_and_reports_provenance() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app"));
        db::ensure_schema(&paths).expect("schema");
        upsert_provider_metadata(
            &paths,
            fixture(
                "Remote title",
                ProviderMetadataQuality::RemoteCanonical,
                100,
            ),
        )
        .expect("metadata");
        let remote = resolve_display_title(&paths, "youtube", "abc123", Some("file title"))
            .expect("remote title");
        assert_eq!(remote.value, "Remote title");
        assert_eq!(remote.provenance, DisplayTitleProvenance::CanonicalRemote);

        set_operator_title_override(
            &paths,
            "youtube",
            "abc123",
            "Operator title",
            "operator",
            200,
        )
        .expect("override");
        let overridden = resolve_display_title(&paths, "youtube", "abc123", Some("file title"))
            .expect("override title");
        assert_eq!(overridden.value, "Operator title");
        assert_eq!(
            overridden.provenance,
            DisplayTitleProvenance::OperatorOverride
        );
    }

    #[test]
    fn shared_library_resolver_is_bounded_and_rejects_damaged_remote_titles() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app"));
        db::ensure_schema(&paths).expect("schema");
        let conn = db::write_context(&paths).expect("open");
        conn.execute(
            "INSERT INTO library_item(id,created_at_ms,source_type,source_uri,title,media_path) VALUES('item-1',1,'local_file','C:/media/file.mkv','Imported clean title','C:/media/file.mkv')",
            [],
        )
        .expect("library item");
        conn.execute(
            "INSERT INTO media_source_identity(service,media_id,canonical_url,library_item_id,created_at_ms,updated_at_ms) VALUES('youtube','abc123','https://www.youtube.com/watch?v=abc123','item-1',1,1)",
            [],
        )
        .expect("identity");
        conn.execute(
            "INSERT INTO media_source_identity(service,media_id,canonical_url,library_item_id,created_at_ms,updated_at_ms) VALUES('instagram','post:ig123','https://www.instagram.com/p/ig123/','item-1',1,1)",
            [],
        )
        .expect("secondary identity");
        drop(conn);

        upsert_provider_metadata(
            &paths,
            fixture("COMPL�XITY", ProviderMetadataQuality::RemoteCanonical, 100),
        )
        .expect("damaged metadata receipt");
        let resolved = resolve_library_display_titles(
            &paths,
            &[("item-1".to_string(), "Imported clean title".to_string())],
        )
        .expect("bulk resolver");
        let item = resolved.get("item-1").expect("resolved item");
        assert_eq!(item.value, "Imported clean title");
        assert_eq!(item.provenance, DisplayTitleProvenance::ImportedOrFile);

        set_operator_title_override(
            &paths,
            "youtube",
            "abc123",
            "Operator title",
            "operator",
            200,
        )
        .expect("override");
        let resolved = resolve_library_display_titles(
            &paths,
            &[("item-1".to_string(), "Imported clean title".to_string())],
        )
        .expect("bulk resolver with override");
        let item = resolved.get("item-1").expect("resolved item");
        assert_eq!(item.value, "Operator title");
        assert_eq!(item.provenance, DisplayTitleProvenance::OperatorOverride);

        let conn = db::write_context(&paths).expect("open");
        conn.execute(
            "INSERT INTO library_download_lineage(item_id,source_job_id,service,origin_kind,work_track,item_created_at_ms,recorded_at_ms) VALUES('item-1','job-ig','instagram','single','instagram',1,1)",
            [],
        )
        .expect("instagram lineage");
        drop(conn);
        let mut instagram = fixture(
            "Instagram canonical title",
            ProviderMetadataQuality::RemoteCanonical,
            300,
        );
        instagram.service = "instagram".to_string();
        instagram.media_id = "post:ig123".to_string();
        instagram.canonical_url = Some("https://www.instagram.com/p/ig123/".to_string());
        upsert_provider_metadata(&paths, instagram).expect("instagram metadata");
        let resolved = resolve_library_display_titles(
            &paths,
            &[("item-1".to_string(), "Imported clean title".to_string())],
        )
        .expect("lineage-aware bulk resolver");
        let item = resolved.get("item-1").expect("resolved item");
        assert_eq!(item.value, "Instagram canonical title");
        assert_eq!(item.provenance, DisplayTitleProvenance::CanonicalRemote);
    }

    #[test]
    fn placeholder_and_damage_classification_is_provider_and_identity_specific() {
        assert!(title_is_provider_placeholder(
            "youtube",
            "abc123",
            "YouTube video abc123"
        ));
        assert!(!title_is_provider_placeholder(
            "youtube",
            "different",
            "YouTube video abc123"
        ));
        assert!(title_contains_encoding_damage("COMPL�XITY"));
        assert!(!title_contains_encoding_damage("COMPLEXITY"));
        assert!(
            valid_candidate_title(Some("COMPL�XITY".to_string()), "youtube", "abc123").is_none()
        );
        assert!(valid_candidate_title(
            Some("YouTube video abc123".to_string()),
            "youtube",
            "abc123"
        )
        .is_none());
    }

    #[test]
    fn bounded_repair_resumes_without_overwriting_valid_conflicts() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app"));
        db::ensure_schema(&paths).expect("schema");
        for (index, media_id, title) in [
            (1_i64, "repair1", None),
            (2, "repair2", Some("YouTube video repair2")),
            (3, "repair3", Some("COMPL�XITY")),
            (4, "repair4", Some("Valid historical snapshot")),
        ] {
            let mut metadata = fixture(
                &format!("Canonical {media_id}"),
                ProviderMetadataQuality::RemoteCanonical,
                100 + index,
            );
            metadata.media_id = media_id.to_string();
            metadata.canonical_url = Some(format!("https://www.youtube.com/watch?v={media_id}"));
            upsert_provider_metadata(&paths, metadata).expect("metadata");
            let conn = db::write_context(&paths).expect("open");
            conn.execute(
                "INSERT INTO job(id,type,status,progress,params_json,created_at_ms,logs_path,target_title)
                 VALUES(?1,'download_direct_url','failed',0,?2,?3,?4,?5)",
                params![
                    format!("job-{index}"),
                    serde_json::json!({"url": format!("https://www.youtube.com/watch?v={media_id}")}).to_string(),
                    index,
                    format!("job-{index}.jsonl"),
                    title,
                ],
            )
            .expect("job");
        }
        let conn = db::write_context(&paths).expect("open");
        conn.execute(
            "INSERT INTO job(id,type,status,progress,params_json,created_at_ms,logs_path,target_title)
             VALUES('job-5','download_direct_url','failed',0,'{}',5,'job-5.jsonl',NULL)",
            [],
        )
        .expect("identity-missing job");
        drop(conn);

        let first = repair_provider_titles_page(&paths, 2).expect("first page");
        assert_eq!(first.page_scanned, 2);
        assert_eq!(first.page_repaired, 2);
        assert!(!first.completed);
        let second = repair_provider_titles_page(&paths, 2).expect("second page");
        assert_eq!(second.page_repaired, 1);
        assert_eq!(second.classifications.get("conflict"), Some(&1));
        let third = repair_provider_titles_page(&paths, 2).expect("third page");
        assert!(third.completed);
        assert_eq!(third.cumulative_scanned, 5);
        assert_eq!(third.cumulative_repaired, 3);
        assert_eq!(third.cumulative_conflicts, 1);
        assert_eq!(third.cumulative_unavailable, 1);

        let conn = db::open_readonly(&paths).expect("readonly");
        let mut statement = conn
            .prepare("SELECT id,target_title FROM job ORDER BY created_at_ms,id")
            .expect("prepare");
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
            })
            .expect("query")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("collect");
        drop(statement);
        assert_eq!(rows[0].1.as_deref(), Some("Canonical repair1"));
        assert_eq!(rows[1].1.as_deref(), Some("Canonical repair2"));
        assert_eq!(rows[2].1.as_deref(), Some("Canonical repair3"));
        assert_eq!(rows[3].1.as_deref(), Some("Valid historical snapshot"));
        let changes: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM media_provider_metadata_repair_change",
                [],
                |row| row.get(0),
            )
            .expect("changes");
        assert_eq!(changes, 3);
        drop(conn);

        reset_provider_title_repair_checkpoint(&paths, 999).expect("reset checkpoint");
        loop {
            let receipt = repair_provider_titles_page(&paths, 2).expect("idempotent page");
            if receipt.completed {
                assert_eq!(receipt.cumulative_repaired, 0);
                break;
            }
        }
        let conn = db::open_readonly(&paths).expect("readonly");
        let changes_after: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM media_provider_metadata_repair_change",
                [],
                |row| row.get(0),
            )
            .expect("changes after");
        assert_eq!(changes_after, 3);
    }
}
