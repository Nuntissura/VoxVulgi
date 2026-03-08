use crate::paths::AppPaths;
use crate::{db, voice_backends, voice_benchmarks, EngineError, Result};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ItemVoicePlan {
    pub item_id: String,
    pub goal: String,
    pub preferred_backend_id: Option<String>,
    pub fallback_backend_id: Option<String>,
    pub selected_candidate_id: Option<String>,
    pub selected_variant_label: Option<String>,
    pub notes: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ItemVoicePlanUpsert {
    pub goal: Option<String>,
    pub preferred_backend_id: Option<String>,
    pub fallback_backend_id: Option<String>,
    pub selected_candidate_id: Option<String>,
    pub selected_variant_label: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReusableVoicePlanDefault {
    pub goal: String,
    pub preferred_backend_id: Option<String>,
    pub fallback_backend_id: Option<String>,
    pub selected_variant_label: Option<String>,
    pub notes: Option<String>,
}

pub fn get_item_voice_plan(paths: &AppPaths, item_id: &str) -> Result<Option<ItemVoicePlan>> {
    let item_id = item_id.trim();
    if item_id.is_empty() {
        return Err(EngineError::InstallFailed("item_id is empty".to_string()));
    }
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    let mut stmt = conn.prepare(
        r#"
SELECT
  item_id,
  goal,
  preferred_backend_id,
  fallback_backend_id,
  selected_candidate_id,
  selected_variant_label,
  notes,
  created_at_ms,
  updated_at_ms
FROM item_voice_plan
WHERE item_id=?1
"#,
    )?;
    match stmt.query_row(params![item_id], map_plan_row) {
        Ok(value) => Ok(Some(value)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub fn upsert_item_voice_plan(
    paths: &AppPaths,
    item_id: &str,
    update: ItemVoicePlanUpsert,
) -> Result<ItemVoicePlan> {
    let item_id = item_id.trim();
    if item_id.is_empty() {
        return Err(EngineError::InstallFailed("item_id is empty".to_string()));
    }
    let goal = normalize_goal(update.goal.as_deref());
    let preferred_backend_id = normalize_optional_string(update.preferred_backend_id);
    let fallback_backend_id = normalize_optional_string(update.fallback_backend_id);
    let selected_candidate_id = normalize_optional_string(update.selected_candidate_id);
    let selected_variant_label = normalize_optional_string(update.selected_variant_label);
    let notes = normalize_optional_string(update.notes);

    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    let now = now_ms();
    conn.execute(
        r#"
INSERT INTO item_voice_plan (
  item_id,
  goal,
  preferred_backend_id,
  fallback_backend_id,
  selected_candidate_id,
  selected_variant_label,
  notes,
  created_at_ms,
  updated_at_ms
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
ON CONFLICT(item_id) DO UPDATE SET
  goal=excluded.goal,
  preferred_backend_id=excluded.preferred_backend_id,
  fallback_backend_id=excluded.fallback_backend_id,
  selected_candidate_id=excluded.selected_candidate_id,
  selected_variant_label=excluded.selected_variant_label,
  notes=excluded.notes,
  updated_at_ms=excluded.updated_at_ms
"#,
        params![
            item_id,
            goal,
            preferred_backend_id,
            fallback_backend_id,
            selected_candidate_id,
            selected_variant_label,
            notes,
            now,
            now
        ],
    )?;
    get_item_voice_plan(paths, item_id)?.ok_or_else(|| {
        EngineError::InstallFailed(format!("failed to reload item voice plan for {item_id}"))
    })
}

pub fn delete_item_voice_plan(paths: &AppPaths, item_id: &str) -> Result<()> {
    let item_id = item_id.trim();
    if item_id.is_empty() {
        return Err(EngineError::InstallFailed("item_id is empty".to_string()));
    }
    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    conn.execute("DELETE FROM item_voice_plan WHERE item_id=?1", params![item_id])?;
    Ok(())
}

pub fn promote_recommendation_to_item_voice_plan(
    paths: &AppPaths,
    item_id: &str,
    recommendation: voice_backends::VoiceBackendRecommendation,
) -> Result<ItemVoicePlan> {
    upsert_item_voice_plan(
        paths,
        item_id,
        ItemVoicePlanUpsert {
            goal: Some(recommendation.goal),
            preferred_backend_id: Some(recommendation.preferred_backend_id),
            fallback_backend_id: recommendation.fallback_backend_id,
            selected_candidate_id: None,
            selected_variant_label: None,
            notes: Some(format!(
                "Promoted from recommendation: {}",
                recommendation.rationale.join(" ")
            )),
        },
    )
}

pub fn promote_benchmark_candidate_to_item_voice_plan(
    paths: &AppPaths,
    item_id: &str,
    track_id: &str,
    goal: Option<&str>,
    candidate_id: &str,
) -> Result<ItemVoicePlan> {
    let report = voice_benchmarks::load_voice_benchmark_report(paths, item_id, track_id, goal)?
        .ok_or_else(|| {
            EngineError::InstallFailed(
                "voice benchmark report not found; generate the report first".to_string(),
            )
        })?;
    let candidate_id = candidate_id.trim();
    if candidate_id.is_empty() {
        return Err(EngineError::InstallFailed(
            "candidate_id is empty".to_string(),
        ));
    }
    let candidate = report
        .candidates
        .into_iter()
        .find(|value| value.candidate_id == candidate_id)
        .ok_or_else(|| {
            EngineError::InstallFailed(format!(
                "voice benchmark candidate not found: {candidate_id}"
            ))
        })?;

    upsert_item_voice_plan(
        paths,
        item_id,
        ItemVoicePlanUpsert {
            goal: Some(report.goal),
            preferred_backend_id: Some(candidate.backend_id),
            fallback_backend_id: None,
            selected_candidate_id: Some(candidate.candidate_id),
            selected_variant_label: candidate.variant_label,
            notes: Some(format!(
                "Promoted from benchmark candidate {} ({:.1}).",
                candidate.display_name, candidate.score
            )),
        },
    )
}

pub fn reusable_voice_plan_default_from_parts(
    goal: Option<String>,
    preferred_backend_id: Option<String>,
    fallback_backend_id: Option<String>,
    selected_variant_label: Option<String>,
    notes: Option<String>,
) -> Option<ReusableVoicePlanDefault> {
    let preferred_backend_id = normalize_optional_string(preferred_backend_id);
    let fallback_backend_id = normalize_optional_string(fallback_backend_id);
    let selected_variant_label = normalize_optional_string(selected_variant_label);
    let notes = normalize_optional_string(notes);
    let goal = goal
        .as_deref()
        .map(|value| normalize_goal(Some(value)))
        .or_else(|| {
            if preferred_backend_id.is_some()
                || fallback_backend_id.is_some()
                || selected_variant_label.is_some()
                || notes.is_some()
            {
                Some("balanced".to_string())
            } else {
                None
            }
        })?;

    Some(ReusableVoicePlanDefault {
        goal,
        preferred_backend_id,
        fallback_backend_id,
        selected_variant_label,
        notes,
    })
}

pub fn promote_benchmark_candidate_to_reusable_voice_plan_default(
    paths: &AppPaths,
    item_id: &str,
    track_id: &str,
    goal: Option<&str>,
    candidate_id: &str,
) -> Result<ReusableVoicePlanDefault> {
    let report = voice_benchmarks::load_voice_benchmark_report(paths, item_id, track_id, goal)?
        .ok_or_else(|| {
            EngineError::InstallFailed(
                "voice benchmark report not found; generate the report first".to_string(),
            )
        })?;
    let candidate_id = candidate_id.trim();
    if candidate_id.is_empty() {
        return Err(EngineError::InstallFailed(
            "candidate_id is empty".to_string(),
        ));
    }
    let candidate = report
        .candidates
        .into_iter()
        .find(|value| value.candidate_id == candidate_id)
        .ok_or_else(|| {
            EngineError::InstallFailed(format!(
                "voice benchmark candidate not found: {candidate_id}"
            ))
        })?;

    Ok(ReusableVoicePlanDefault {
        goal: report.goal,
        preferred_backend_id: Some(candidate.backend_id),
        fallback_backend_id: None,
        selected_variant_label: candidate.variant_label,
        notes: Some(format!(
            "Promoted from benchmark candidate {} ({:.1}).",
            candidate.display_name, candidate.score
        )),
    })
}

pub fn upsert_item_voice_plan_from_reusable_default(
    paths: &AppPaths,
    item_id: &str,
    default: &ReusableVoicePlanDefault,
    source_note: Option<&str>,
) -> Result<ItemVoicePlan> {
    upsert_item_voice_plan(
        paths,
        item_id,
        ItemVoicePlanUpsert {
            goal: Some(default.goal.clone()),
            preferred_backend_id: default.preferred_backend_id.clone(),
            fallback_backend_id: default.fallback_backend_id.clone(),
            selected_candidate_id: None,
            selected_variant_label: default.selected_variant_label.clone(),
            notes: combine_note_parts(source_note, default.notes.as_deref()),
        },
    )
}

fn map_plan_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ItemVoicePlan> {
    Ok(ItemVoicePlan {
        item_id: row.get(0)?,
        goal: row.get(1)?,
        preferred_backend_id: row.get(2)?,
        fallback_backend_id: row.get(3)?,
        selected_candidate_id: row.get(4)?,
        selected_variant_label: row.get(5)?,
        notes: row.get(6)?,
        created_at_ms: row.get(7)?,
        updated_at_ms: row.get(8)?,
    })
}

fn normalize_goal(raw: Option<&str>) -> String {
    match raw.unwrap_or("").trim() {
        "identity" => "identity".to_string(),
        "expressive" => "expressive".to_string(),
        "timing" => "timing".to_string(),
        "speed" => "speed".to_string(),
        _ => "balanced".to_string(),
    }
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

fn combine_note_parts(first: Option<&str>, second: Option<&str>) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    for value in [first, second] {
        let trimmed = value.unwrap_or("").trim();
        if !trimmed.is_empty() {
            parts.push(trimmed.to_string());
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::voice_benchmarks::{VoiceBenchmarkCandidate, VoiceBenchmarkReport, VoiceBenchmarkScoreTerm};
    use rusqlite::params;
    use tempfile::tempdir;

    #[test]
    fn upsert_and_delete_item_voice_plan_round_trip() {
        let dir = tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        seed_item(&paths, "item-1");

        let plan = upsert_item_voice_plan(
            &paths,
            "item-1",
            ItemVoicePlanUpsert {
                goal: Some("identity".to_string()),
                preferred_backend_id: Some("seed_vc".to_string()),
                fallback_backend_id: Some("openvoice_v2".to_string()),
                selected_candidate_id: Some("candidate-a".to_string()),
                selected_variant_label: Some("seed_try".to_string()),
                notes: Some("Test plan".to_string()),
            },
        )
        .expect("upsert");
        assert_eq!(plan.goal, "identity");
        assert_eq!(plan.selected_variant_label.as_deref(), Some("seed_try"));

        let loaded = get_item_voice_plan(&paths, "item-1")
            .expect("load")
            .expect("exists");
        assert_eq!(loaded.preferred_backend_id.as_deref(), Some("seed_vc"));

        delete_item_voice_plan(&paths, "item-1").expect("delete");
        assert!(get_item_voice_plan(&paths, "item-1").expect("load after delete").is_none());
    }

    #[test]
    fn promote_benchmark_candidate_to_item_voice_plan_picks_candidate_backend() {
        let dir = tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        seed_item(&paths, "item-1");
        let benchmark_dir = paths.derived_item_dir("item-1").join("voice_benchmark");
        std::fs::create_dir_all(&benchmark_dir).expect("benchmark dir");
        let report = VoiceBenchmarkReport {
            schema_version: 1,
            generated_at_ms: 0,
            item_id: "item-1".to_string(),
            track_id: "track-1".to_string(),
            goal: "expressive".to_string(),
            recommended_candidate_id: Some("cosyvoice_a".to_string()),
            candidate_count: 1,
            summary: vec!["ok".to_string()],
            json_path: benchmark_dir
                .join("voice_benchmark_v1_track-1_expressive.json")
                .to_string_lossy()
                .to_string(),
            markdown_path: benchmark_dir
                .join("voice_benchmark_v1_track-1_expressive.md")
                .to_string_lossy()
                .to_string(),
            candidates: vec![VoiceBenchmarkCandidate {
                candidate_id: "cosyvoice_a".to_string(),
                display_name: "CosyVoice (a)".to_string(),
                backend_id: "cosyvoice".to_string(),
                variant_label: Some("cosy_try".to_string()),
                manifest_path: "manifest.json".to_string(),
                expected_segments: 1,
                rendered_segments: 1,
                coverage_ratio: 1.0,
                timing_fit_ratio: 1.0,
                timing_overrun_segments: 0,
                timing_short_segments: 0,
                warn_count: 0,
                fail_count: 0,
                reference_warn_count: 0,
                reference_fail_count: 0,
                output_warn_count: 0,
                output_fail_count: 0,
                similarity_proxy: Some(0.9),
                converted_ratio: Some(0.9),
                final_mix_ready: true,
                export_pack_ready: true,
                score: 91.0,
                score_breakdown: vec![VoiceBenchmarkScoreTerm {
                    key: "coverage".to_string(),
                    label: "Coverage".to_string(),
                    weight: 1.0,
                    value: 0.9,
                    points: 90.0,
                }],
                strengths: vec![],
                concerns: vec![],
            }],
        };
        std::fs::write(
            &report.json_path,
            format!("{}\n", serde_json::to_string_pretty(&report).expect("json")),
        )
        .expect("write report");

        let plan = promote_benchmark_candidate_to_item_voice_plan(
            &paths,
            "item-1",
            "track-1",
            Some("expressive"),
            "cosyvoice_a",
        )
        .expect("promote");
        assert_eq!(plan.goal, "expressive");
        assert_eq!(plan.preferred_backend_id.as_deref(), Some("cosyvoice"));
        assert_eq!(plan.selected_variant_label.as_deref(), Some("cosy_try"));
    }

    fn seed_item(paths: &AppPaths, item_id: &str) {
        let conn = db::open(paths).expect("db open");
        db::migrate(&conn).expect("migrate");
        conn.execute(
            "INSERT INTO library_item (
                id, created_at_ms, source_type, source_uri, title, media_path
            ) VALUES (?1, 0, 'local', ?2, ?3, ?4)",
            params![item_id, format!("file:///{item_id}"), item_id, format!("{item_id}.mp4")],
        )
        .expect("seed item");
    }
}
