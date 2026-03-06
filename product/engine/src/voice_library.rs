use crate::paths::AppPaths;
use crate::{db, speakers, EngineError, Result};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceLibraryProfile {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub description: Option<String>,
    pub display_name: Option<String>,
    pub tts_voice_id: Option<String>,
    pub tts_voice_profile_path: Option<String>,
    #[serde(default)]
    pub tts_voice_profile_paths: Vec<String>,
    pub style_preset: Option<String>,
    pub prosody_preset: Option<String>,
    pub pronunciation_overrides: Option<String>,
    pub render_mode: Option<String>,
    pub subtitle_prosody_mode: Option<String>,
    pub dir_path: String,
    pub reference_count: usize,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceLibraryReference {
    pub profile_id: String,
    pub reference_id: String,
    pub label: Option<String>,
    pub path: String,
    pub sort_order: i64,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceLibraryProfileDetail {
    pub profile: VoiceLibraryProfile,
    pub references: Vec<VoiceLibraryReference>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceLibraryProfileUpdate {
    pub name: Option<String>,
    pub description: Option<String>,
    pub display_name: Option<String>,
    pub tts_voice_id: Option<String>,
    pub style_preset: Option<String>,
    pub prosody_preset: Option<String>,
    pub pronunciation_overrides: Option<String>,
    pub render_mode: Option<String>,
    pub subtitle_prosody_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceLibrarySuggestion {
    pub item_speaker_key: String,
    pub current_display_name: Option<String>,
    pub profile_id: String,
    pub profile_kind: String,
    pub profile_name: String,
    pub profile_display_name: Option<String>,
    pub score: i64,
    pub match_reason: String,
}

pub fn list_voice_library_profiles(
    paths: &AppPaths,
    kind: Option<&str>,
) -> Result<Vec<VoiceLibraryProfile>> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;

    let kind = normalize_profile_kind_opt(kind)?;
    let sql = if kind.is_some() {
        r#"
SELECT
  p.id,
  p.kind,
  p.name,
  p.description,
  p.display_name,
  p.tts_voice_id,
  p.tts_voice_profile_path,
  p.tts_voice_profile_paths_json,
  p.style_preset,
  p.prosody_preset,
  p.pronunciation_overrides,
  p.render_mode,
  p.subtitle_prosody_mode,
  p.created_at_ms,
  p.updated_at_ms,
  COUNT(r.reference_id) AS reference_count
FROM voice_library_profile p
LEFT JOIN voice_library_reference r ON r.profile_id = p.id
WHERE p.kind=?1
GROUP BY
  p.id, p.kind, p.name, p.description, p.display_name, p.tts_voice_id,
  p.tts_voice_profile_path, p.tts_voice_profile_paths_json, p.style_preset,
  p.prosody_preset, p.pronunciation_overrides, p.render_mode,
  p.subtitle_prosody_mode, p.created_at_ms, p.updated_at_ms
ORDER BY p.updated_at_ms DESC, p.name COLLATE NOCASE ASC
"#
    } else {
        r#"
SELECT
  p.id,
  p.kind,
  p.name,
  p.description,
  p.display_name,
  p.tts_voice_id,
  p.tts_voice_profile_path,
  p.tts_voice_profile_paths_json,
  p.style_preset,
  p.prosody_preset,
  p.pronunciation_overrides,
  p.render_mode,
  p.subtitle_prosody_mode,
  p.created_at_ms,
  p.updated_at_ms,
  COUNT(r.reference_id) AS reference_count
FROM voice_library_profile p
LEFT JOIN voice_library_reference r ON r.profile_id = p.id
GROUP BY
  p.id, p.kind, p.name, p.description, p.display_name, p.tts_voice_id,
  p.tts_voice_profile_path, p.tts_voice_profile_paths_json, p.style_preset,
  p.prosody_preset, p.pronunciation_overrides, p.render_mode,
  p.subtitle_prosody_mode, p.created_at_ms, p.updated_at_ms
ORDER BY p.updated_at_ms DESC, p.name COLLATE NOCASE ASC
"#
    };

    let mut stmt = conn.prepare(sql)?;
    let map_row = |row: &rusqlite::Row<'_>| {
        let id: String = row.get(0)?;
        let single_profile_path: Option<String> = row.get(6)?;
        Ok(VoiceLibraryProfile {
            dir_path: paths
                .voice_library_profile_dir(&id)
                .to_string_lossy()
                .to_string(),
            id,
            kind: row.get(1)?,
            name: row.get(2)?,
            description: row.get(3)?,
            display_name: row.get(4)?,
            tts_voice_id: row.get(5)?,
            tts_voice_profile_path: single_profile_path.clone(),
            tts_voice_profile_paths: decode_profile_paths(
                row.get::<_, Option<String>>(7)?,
                single_profile_path,
            ),
            style_preset: row.get(8)?,
            prosody_preset: row.get(9)?,
            pronunciation_overrides: row.get(10)?,
            render_mode: row.get(11)?,
            subtitle_prosody_mode: row.get(12)?,
            created_at_ms: row.get(13)?,
            updated_at_ms: row.get(14)?,
            reference_count: row.get::<_, i64>(15)? as usize,
        })
    };

    let rows = if let Some(kind) = kind {
        stmt.query_map(params![kind], map_row)?
    } else {
        stmt.query_map([], map_row)?
    };
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn get_voice_library_profile(
    paths: &AppPaths,
    profile_id: &str,
) -> Result<VoiceLibraryProfileDetail> {
    let profile_id = profile_id.trim();
    if profile_id.is_empty() {
        return Err(EngineError::InstallFailed(
            "profile_id is empty".to_string(),
        ));
    }

    let conn = db::open(paths)?;
    db::migrate(&conn)?;
    let references = list_references(&conn, profile_id)?;
    let profile = conn.query_row(
        r#"
SELECT
  id,
  kind,
  name,
  description,
  display_name,
  tts_voice_id,
  tts_voice_profile_path,
  tts_voice_profile_paths_json,
  style_preset,
  prosody_preset,
  pronunciation_overrides,
  render_mode,
  subtitle_prosody_mode,
  created_at_ms,
  updated_at_ms
FROM voice_library_profile
WHERE id=?1
"#,
        params![profile_id],
        |row| {
            let id: String = row.get(0)?;
            let stored_single_path: Option<String> = row.get(6)?;
            let profile_paths = if references.is_empty() {
                decode_profile_paths(row.get::<_, Option<String>>(7)?, stored_single_path.clone())
            } else {
                references
                    .iter()
                    .map(|reference| reference.path.clone())
                    .collect::<Vec<_>>()
            };
            Ok(VoiceLibraryProfile {
                dir_path: paths
                    .voice_library_profile_dir(&id)
                    .to_string_lossy()
                    .to_string(),
                id,
                kind: row.get(1)?,
                name: row.get(2)?,
                description: row.get(3)?,
                display_name: row.get(4)?,
                tts_voice_id: row.get(5)?,
                tts_voice_profile_path: profile_paths.first().cloned().or(stored_single_path),
                tts_voice_profile_paths: profile_paths,
                style_preset: row.get(8)?,
                prosody_preset: row.get(9)?,
                pronunciation_overrides: row.get(10)?,
                render_mode: row.get(11)?,
                subtitle_prosody_mode: row.get(12)?,
                created_at_ms: row.get(13)?,
                updated_at_ms: row.get(14)?,
                reference_count: references.len(),
            })
        },
    )?;

    Ok(VoiceLibraryProfileDetail {
        profile,
        references,
    })
}

pub fn create_voice_library_profile(
    paths: &AppPaths,
    kind: &str,
    name: &str,
    description: Option<String>,
) -> Result<VoiceLibraryProfileDetail> {
    let kind = normalize_profile_kind(kind)?;
    let name = normalize_non_empty(Some(name.to_string()))
        .ok_or_else(|| EngineError::InstallFailed("profile name is empty".to_string()))?;
    let profile_id = Uuid::new_v4().to_string();
    std::fs::create_dir_all(paths.voice_library_profile_refs_dir(&profile_id))?;

    let mut conn = db::open(paths)?;
    db::migrate(&conn)?;
    let now = now_ms();
    let tx = conn.transaction()?;
    tx.execute(
        r#"
INSERT INTO voice_library_profile (
  id,
  kind,
  name,
  description,
  display_name,
  tts_voice_id,
  tts_voice_profile_path,
  tts_voice_profile_paths_json,
  style_preset,
  prosody_preset,
  pronunciation_overrides,
  render_mode,
  subtitle_prosody_mode,
  created_at_ms,
  updated_at_ms
) VALUES (?1, ?2, ?3, ?4, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, ?5, ?6)
"#,
        params![
            profile_id,
            kind,
            name,
            normalize_non_empty(description),
            now,
            now
        ],
    )?;
    tx.commit()?;

    get_voice_library_profile(paths, &profile_id)
}

pub fn create_voice_library_profile_from_item_speaker(
    paths: &AppPaths,
    item_id: &str,
    speaker_key: &str,
    kind: &str,
    name: &str,
    description: Option<String>,
) -> Result<VoiceLibraryProfileDetail> {
    let item_id = item_id.trim();
    let speaker_key = speaker_key.trim();
    if item_id.is_empty() || speaker_key.is_empty() {
        return Err(EngineError::InstallFailed(
            "item_id or speaker_key is empty".to_string(),
        ));
    }
    let kind = normalize_profile_kind(kind)?;
    let name = normalize_non_empty(Some(name.to_string()))
        .ok_or_else(|| EngineError::InstallFailed("profile name is empty".to_string()))?;
    let item_speaker = speakers::list_item_speaker_settings(paths, item_id)?
        .into_iter()
        .find(|setting| setting.speaker_key == speaker_key)
        .ok_or_else(|| {
            EngineError::InstallFailed(format!(
                "speaker not found for item: {item_id}/{speaker_key}"
            ))
        })?;

    let profile_id = Uuid::new_v4().to_string();
    let refs_dir = paths.voice_library_profile_refs_dir(&profile_id);
    std::fs::create_dir_all(&refs_dir)?;
    let references =
        copy_profile_references(&refs_dir, &item_speaker.tts_voice_profile_paths, None)?;

    let mut conn = db::open(paths)?;
    db::migrate(&conn)?;
    let tx = conn.transaction()?;
    let now = now_ms();
    insert_profile_row(
        &tx,
        &profile_id,
        &kind,
        &name,
        normalize_non_empty(description),
        &item_speaker,
        &references,
        now,
    )?;
    tx.commit()?;

    get_voice_library_profile(paths, &profile_id)
}

pub fn update_voice_library_profile(
    paths: &AppPaths,
    profile_id: &str,
    update: VoiceLibraryProfileUpdate,
) -> Result<VoiceLibraryProfileDetail> {
    let profile_id = profile_id.trim();
    if profile_id.is_empty() {
        return Err(EngineError::InstallFailed(
            "profile_id is empty".to_string(),
        ));
    }
    let mut conn = db::open(paths)?;
    db::migrate(&conn)?;
    let tx = conn.transaction()?;
    let now = now_ms();
    let updated = tx.execute(
        r#"
UPDATE voice_library_profile
SET
  name=COALESCE(?2, name),
  description=?3,
  display_name=?4,
  tts_voice_id=?5,
  style_preset=?6,
  prosody_preset=?7,
  pronunciation_overrides=?8,
  render_mode=?9,
  subtitle_prosody_mode=?10,
  updated_at_ms=?11
WHERE id=?1
"#,
        params![
            profile_id,
            normalize_non_empty(update.name),
            normalize_non_empty(update.description),
            normalize_non_empty(update.display_name),
            normalize_non_empty(update.tts_voice_id),
            normalize_non_empty(update.style_preset),
            normalize_non_empty(update.prosody_preset),
            normalize_non_empty(update.pronunciation_overrides),
            normalize_non_empty(update.render_mode),
            normalize_non_empty(update.subtitle_prosody_mode),
            now,
        ],
    )?;
    if updated == 0 {
        return Err(EngineError::InstallFailed(format!(
            "voice library profile not found: {profile_id}"
        )));
    }
    tx.commit()?;
    get_voice_library_profile(paths, profile_id)
}

pub fn add_voice_library_reference(
    paths: &AppPaths,
    profile_id: &str,
    source_path: &str,
    label: Option<String>,
) -> Result<VoiceLibraryProfileDetail> {
    let profile_id = profile_id.trim();
    let source_path = source_path.trim();
    if profile_id.is_empty() || source_path.is_empty() {
        return Err(EngineError::InstallFailed(
            "profile_id or source_path is empty".to_string(),
        ));
    }
    let refs_dir = paths.voice_library_profile_refs_dir(profile_id);
    std::fs::create_dir_all(&refs_dir)?;
    let mut copied = copy_profile_references(
        &refs_dir,
        &[source_path.to_string()],
        normalize_non_empty(label),
    )?;
    let reference = copied
        .drain(..)
        .next()
        .ok_or_else(|| EngineError::InstallFailed("reference copy failed".to_string()))?;

    let mut conn = db::open(paths)?;
    db::migrate(&conn)?;
    let tx = conn.transaction()?;
    insert_reference_row(&tx, profile_id, &reference)?;
    refresh_profile_reference_cache(&tx, profile_id)?;
    tx.execute(
        "UPDATE voice_library_profile SET updated_at_ms=?2 WHERE id=?1",
        params![profile_id, now_ms()],
    )?;
    tx.commit()?;
    get_voice_library_profile(paths, profile_id)
}

pub fn remove_voice_library_reference(
    paths: &AppPaths,
    profile_id: &str,
    reference_id: &str,
) -> Result<VoiceLibraryProfileDetail> {
    let profile_id = profile_id.trim();
    let reference_id = reference_id.trim();
    if profile_id.is_empty() || reference_id.is_empty() {
        return Err(EngineError::InstallFailed(
            "profile_id or reference_id is empty".to_string(),
        ));
    }
    let mut conn = db::open(paths)?;
    db::migrate(&conn)?;
    let tx = conn.transaction()?;
    let path: String = tx.query_row(
        "SELECT path FROM voice_library_reference WHERE profile_id=?1 AND reference_id=?2",
        params![profile_id, reference_id],
        |row| row.get(0),
    )?;
    tx.execute(
        "DELETE FROM voice_library_reference WHERE profile_id=?1 AND reference_id=?2",
        params![profile_id, reference_id],
    )?;
    refresh_profile_reference_cache(&tx, profile_id)?;
    tx.execute(
        "UPDATE voice_library_profile SET updated_at_ms=?2 WHERE id=?1",
        params![profile_id, now_ms()],
    )?;
    tx.commit()?;

    let path = Path::new(&path);
    if path.exists() {
        let _ = std::fs::remove_file(path);
    }
    get_voice_library_profile(paths, profile_id)
}

pub fn delete_voice_library_profile(paths: &AppPaths, profile_id: &str) -> Result<()> {
    let profile_id = profile_id.trim();
    if profile_id.is_empty() {
        return Err(EngineError::InstallFailed(
            "profile_id is empty".to_string(),
        ));
    }
    let mut conn = db::open(paths)?;
    db::migrate(&conn)?;
    let tx = conn.transaction()?;
    tx.execute(
        "DELETE FROM voice_library_reference WHERE profile_id=?1",
        params![profile_id],
    )?;
    tx.execute(
        "DELETE FROM voice_library_profile WHERE id=?1",
        params![profile_id],
    )?;
    tx.commit()?;

    let profile_dir = paths.voice_library_profile_dir(profile_id);
    if profile_dir.exists() {
        std::fs::remove_dir_all(profile_dir)?;
    }
    Ok(())
}

pub fn apply_voice_library_profile_to_item(
    paths: &AppPaths,
    item_id: &str,
    speaker_key: &str,
    profile_id: &str,
) -> Result<speakers::ItemSpeakerSetting> {
    let item_id = item_id.trim();
    let speaker_key = speaker_key.trim();
    let profile_id = profile_id.trim();
    if item_id.is_empty() || speaker_key.is_empty() || profile_id.is_empty() {
        return Err(EngineError::InstallFailed(
            "item_id, speaker_key, or profile_id is empty".to_string(),
        ));
    }

    let detail = get_voice_library_profile(paths, profile_id)?;
    let existing = speakers::list_item_speaker_settings(paths, item_id)?
        .into_iter()
        .find(|setting| setting.speaker_key == speaker_key);

    speakers::upsert_item_speaker_setting(
        paths,
        item_id,
        speaker_key,
        detail.profile.display_name.clone().or_else(|| {
            existing
                .as_ref()
                .and_then(|value| value.display_name.clone())
        }),
        Some(detail.profile.id.clone()),
        detail.profile.tts_voice_id.clone().or_else(|| {
            existing
                .as_ref()
                .and_then(|value| value.tts_voice_id.clone())
        }),
        detail.profile.tts_voice_profile_path.clone().or_else(|| {
            existing
                .as_ref()
                .and_then(|value| value.tts_voice_profile_path.clone())
        }),
        Some(if detail.profile.tts_voice_profile_paths.is_empty() {
            existing
                .as_ref()
                .map(|value| value.tts_voice_profile_paths.clone())
                .unwrap_or_default()
        } else {
            detail.profile.tts_voice_profile_paths.clone()
        }),
        detail.profile.style_preset.clone().or_else(|| {
            existing
                .as_ref()
                .and_then(|value| value.style_preset.clone())
        }),
        detail.profile.prosody_preset.clone().or_else(|| {
            existing
                .as_ref()
                .and_then(|value| value.prosody_preset.clone())
        }),
        detail.profile.pronunciation_overrides.clone().or_else(|| {
            existing
                .as_ref()
                .and_then(|value| value.pronunciation_overrides.clone())
        }),
        detail.profile.render_mode.clone().or_else(|| {
            existing
                .as_ref()
                .and_then(|value| value.render_mode.clone())
        }),
        detail.profile.subtitle_prosody_mode.clone().or_else(|| {
            existing
                .as_ref()
                .and_then(|value| value.subtitle_prosody_mode.clone())
        }),
    )
}

pub fn fork_voice_library_profile(
    paths: &AppPaths,
    profile_id: &str,
    name: &str,
) -> Result<VoiceLibraryProfileDetail> {
    let detail = get_voice_library_profile(paths, profile_id)?;
    let fork_name = normalize_non_empty(Some(name.to_string()))
        .ok_or_else(|| EngineError::InstallFailed("forked profile name is empty".to_string()))?;
    let fork_id = Uuid::new_v4().to_string();
    let fork_refs_dir = paths.voice_library_profile_refs_dir(&fork_id);
    std::fs::create_dir_all(&fork_refs_dir)?;
    let references = copy_profile_references(
        &fork_refs_dir,
        &detail
            .references
            .iter()
            .map(|reference| reference.path.clone())
            .collect::<Vec<_>>(),
        None,
    )?;
    let mut conn = db::open(paths)?;
    db::migrate(&conn)?;
    let tx = conn.transaction()?;
    let now = now_ms();
    let source_setting = speakers::ItemSpeakerSetting {
        item_id: String::new(),
        speaker_key: detail
            .profile
            .display_name
            .clone()
            .unwrap_or_else(|| detail.profile.name.clone()),
        display_name: detail.profile.display_name.clone(),
        voice_profile_id: None,
        tts_voice_id: detail.profile.tts_voice_id.clone(),
        tts_voice_profile_path: detail.profile.tts_voice_profile_path.clone(),
        tts_voice_profile_paths: detail.profile.tts_voice_profile_paths.clone(),
        style_preset: detail.profile.style_preset.clone(),
        prosody_preset: detail.profile.prosody_preset.clone(),
        pronunciation_overrides: detail.profile.pronunciation_overrides.clone(),
        render_mode: detail.profile.render_mode.clone(),
        subtitle_prosody_mode: detail.profile.subtitle_prosody_mode.clone(),
        created_at_ms: now,
        updated_at_ms: now,
    };
    insert_profile_row(
        &tx,
        &fork_id,
        &detail.profile.kind,
        &fork_name,
        detail.profile.description.clone(),
        &source_setting,
        &references,
        now,
    )?;
    tx.commit()?;
    get_voice_library_profile(paths, &fork_id)
}

pub fn suggest_voice_library_profiles_for_item(
    paths: &AppPaths,
    item_id: &str,
    kind: Option<&str>,
) -> Result<Vec<VoiceLibrarySuggestion>> {
    let item_id = item_id.trim();
    if item_id.is_empty() {
        return Err(EngineError::InstallFailed("item_id is empty".to_string()));
    }
    let speakers = speakers::list_item_speaker_settings(paths, item_id)?;
    let profiles = list_voice_library_profiles(paths, kind)?;
    let mut out: Vec<VoiceLibrarySuggestion> = Vec::new();

    for speaker in speakers {
        let current_display_name = speaker
            .display_name
            .clone()
            .and_then(|value| normalize_non_empty(Some(value)));
        let normalized_name = normalize_token(current_display_name.as_deref().unwrap_or(""));
        let normalized_key = normalize_token(&speaker.speaker_key);
        for profile in &profiles {
            let mut score = 0_i64;
            let mut reason = None;
            if speaker.voice_profile_id.as_deref() == Some(profile.id.as_str()) {
                score = 130;
                reason = Some("profile already applied".to_string());
            } else {
                let profile_display =
                    normalize_token(profile.display_name.as_deref().unwrap_or(""));
                let profile_name = normalize_token(&profile.name);
                if !normalized_name.is_empty()
                    && (!profile_display.is_empty() && normalized_name == profile_display
                        || normalized_name == profile_name)
                {
                    score = 110;
                    reason = Some("exact display-name match".to_string());
                } else if !normalized_key.is_empty()
                    && (normalized_key == profile_name || normalized_key == profile_display)
                {
                    score = 95;
                    reason = Some("exact speaker-key match".to_string());
                } else if !normalized_name.is_empty()
                    && ((!profile_display.is_empty() && profile_display.contains(&normalized_name))
                        || profile_name.contains(&normalized_name))
                {
                    score = 70;
                    reason = Some("partial display-name match".to_string());
                }
            }

            if score <= 0 {
                continue;
            }
            out.push(VoiceLibrarySuggestion {
                item_speaker_key: speaker.speaker_key.clone(),
                current_display_name: current_display_name.clone(),
                profile_id: profile.id.clone(),
                profile_kind: profile.kind.clone(),
                profile_name: profile.name.clone(),
                profile_display_name: profile.display_name.clone(),
                score,
                match_reason: reason.unwrap_or_else(|| "manual review suggested".to_string()),
            });
        }
    }

    out.sort_by(|a, b| {
        a.item_speaker_key
            .cmp(&b.item_speaker_key)
            .then_with(|| b.score.cmp(&a.score))
            .then_with(|| a.profile_name.cmp(&b.profile_name))
    });

    let mut limited: Vec<VoiceLibrarySuggestion> = Vec::new();
    let mut counts: HashMap<String, usize> = HashMap::new();
    for suggestion in out {
        let count = counts
            .entry(suggestion.item_speaker_key.clone())
            .or_insert(0);
        if *count >= 3 {
            continue;
        }
        *count += 1;
        limited.push(suggestion);
    }
    Ok(limited)
}

fn list_references(
    conn: &rusqlite::Connection,
    profile_id: &str,
) -> Result<Vec<VoiceLibraryReference>> {
    let mut stmt = conn.prepare(
        r#"
SELECT
  profile_id,
  reference_id,
  label,
  path,
  sort_order,
  created_at_ms,
  updated_at_ms
FROM voice_library_reference
WHERE profile_id=?1
ORDER BY sort_order ASC, created_at_ms ASC
"#,
    )?;
    let rows = stmt
        .query_map(params![profile_id], |row| {
            Ok(VoiceLibraryReference {
                profile_id: row.get(0)?,
                reference_id: row.get(1)?,
                label: row.get(2)?,
                path: row.get(3)?,
                sort_order: row.get(4)?,
                created_at_ms: row.get(5)?,
                updated_at_ms: row.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn insert_profile_row(
    conn: &rusqlite::Connection,
    profile_id: &str,
    kind: &str,
    name: &str,
    description: Option<String>,
    speaker: &speakers::ItemSpeakerSetting,
    references: &[VoiceLibraryReference],
    now: i64,
) -> Result<()> {
    let reference_paths = references
        .iter()
        .map(|reference| reference.path.clone())
        .collect::<Vec<_>>();
    let primary_reference = reference_paths.first().cloned();
    let stored_paths = if reference_paths.is_empty() {
        speaker.tts_voice_profile_paths.clone()
    } else {
        reference_paths
    };
    conn.execute(
        r#"
INSERT INTO voice_library_profile (
  id,
  kind,
  name,
  description,
  display_name,
  tts_voice_id,
  tts_voice_profile_path,
  tts_voice_profile_paths_json,
  style_preset,
  prosody_preset,
  pronunciation_overrides,
  render_mode,
  subtitle_prosody_mode,
  created_at_ms,
  updated_at_ms
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
"#,
        params![
            profile_id,
            kind,
            name,
            description,
            speaker.display_name,
            speaker.tts_voice_id,
            primary_reference.or_else(|| speaker.tts_voice_profile_path.clone()),
            encode_profile_paths(&stored_paths),
            speaker.style_preset,
            speaker.prosody_preset,
            speaker.pronunciation_overrides,
            speaker.render_mode,
            speaker.subtitle_prosody_mode,
            now,
            now,
        ],
    )?;

    for reference in references {
        insert_reference_row(conn, profile_id, reference)?;
    }
    Ok(())
}

fn insert_reference_row(
    conn: &rusqlite::Connection,
    profile_id: &str,
    reference: &VoiceLibraryReference,
) -> Result<()> {
    conn.execute(
        r#"
INSERT INTO voice_library_reference (
  profile_id,
  reference_id,
  label,
  path,
  sort_order,
  created_at_ms,
  updated_at_ms
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
"#,
        params![
            profile_id,
            reference.reference_id,
            reference.label,
            reference.path,
            reference.sort_order,
            reference.created_at_ms,
            reference.updated_at_ms,
        ],
    )?;
    Ok(())
}

fn refresh_profile_reference_cache(conn: &rusqlite::Connection, profile_id: &str) -> Result<()> {
    let references = list_references(conn, profile_id)?;
    let profile_paths = references
        .iter()
        .map(|reference| reference.path.clone())
        .collect::<Vec<_>>();
    conn.execute(
        "UPDATE voice_library_profile SET tts_voice_profile_path=?2, tts_voice_profile_paths_json=?3 WHERE id=?1",
        params![
            profile_id,
            profile_paths.first().cloned(),
            encode_profile_paths(&profile_paths),
        ],
    )?;
    Ok(())
}

fn copy_profile_references(
    refs_dir: &Path,
    source_paths: &[String],
    override_label: Option<String>,
) -> Result<Vec<VoiceLibraryReference>> {
    let mut out: Vec<VoiceLibraryReference> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for (index, source_path) in source_paths.iter().enumerate() {
        let source_path = source_path.trim();
        if source_path.is_empty() || !seen.insert(source_path.to_string()) {
            continue;
        }
        let src = Path::new(source_path);
        if !src.is_file() {
            continue;
        }
        let reference_id = Uuid::new_v4().to_string();
        let ext = src
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .unwrap_or("wav");
        let dst = refs_dir.join(format!("{reference_id}.{ext}"));
        std::fs::copy(src, &dst)?;
        let now = now_ms();
        out.push(VoiceLibraryReference {
            profile_id: String::new(),
            reference_id,
            label: override_label.clone().or_else(|| {
                src.file_stem()
                    .and_then(|value| value.to_str())
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
            }),
            path: dst.to_string_lossy().to_string(),
            sort_order: index as i64,
            created_at_ms: now,
            updated_at_ms: now,
        });
    }
    Ok(out)
}

fn normalize_profile_kind(value: &str) -> Result<String> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "memory" => Ok("memory".to_string()),
        "character" => Ok("character".to_string()),
        _ => Err(EngineError::InstallFailed(format!(
            "unsupported voice profile kind: {value}"
        ))),
    }
}

fn normalize_profile_kind_opt(value: Option<&str>) -> Result<Option<String>> {
    match value {
        Some(value) => Ok(Some(normalize_profile_kind(value)?)),
        None => Ok(None),
    }
}

fn normalize_non_empty(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

fn normalize_token(value: &str) -> String {
    let mut out = String::new();
    for ch in value.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        }
    }
    out
}

fn encode_profile_paths(profile_paths: &[String]) -> Option<String> {
    if profile_paths.is_empty() {
        None
    } else {
        serde_json::to_string(profile_paths).ok()
    }
}

fn decode_profile_paths(
    profile_paths_json: Option<String>,
    single_profile_path: Option<String>,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(profile_paths_json) = profile_paths_json {
        if let Ok(parsed) = serde_json::from_str::<Vec<Value>>(&profile_paths_json) {
            for value in parsed {
                let Some(path) = value.as_str() else {
                    continue;
                };
                let trimmed = path.trim();
                if trimmed.is_empty() || out.iter().any(|existing| existing == trimmed) {
                    continue;
                }
                out.push(trimmed.to_string());
            }
        }
    }
    if out.is_empty() {
        if let Some(single_profile_path) = single_profile_path {
            let trimmed = single_profile_path.trim();
            if !trimmed.is_empty() {
                out.push(trimmed.to_string());
            }
        }
    }
    out
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
    use tempfile::tempdir;

    fn insert_test_item(paths: &AppPaths, item_id: &str) {
        let conn = db::open(paths).expect("open db");
        db::migrate(&conn).expect("migrate");
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
  thumbnail_path
) VALUES (?1, ?2, 'local_file', ?3, ?4, ?3, NULL, NULL, NULL, NULL, NULL, NULL, NULL)
"#,
            params![
                item_id,
                now_ms(),
                format!("C:/media/{item_id}.mp4"),
                format!("Item {item_id}")
            ],
        )
        .expect("insert item");
    }

    #[test]
    fn create_and_apply_memory_profile_round_trips_references() {
        let dir = tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        db::ensure_schema(&paths).expect("schema");
        insert_test_item(&paths, "item-1");
        let ref_path = dir.path().join("host.wav");
        std::fs::write(&ref_path, b"voice").expect("write ref");
        speakers::upsert_item_speaker_setting(
            &paths,
            "item-1",
            "speaker_a",
            Some("Host".to_string()),
            None,
            Some("af_heart".to_string()),
            Some(ref_path.to_string_lossy().to_string()),
            Some(vec![ref_path.to_string_lossy().to_string()]),
            Some("documentary_narrator".to_string()),
            Some("warmer".to_string()),
            Some("Seoul=>Soul".to_string()),
            Some("clone".to_string()),
            Some("auto".to_string()),
        )
        .expect("upsert speaker");

        let detail = create_voice_library_profile_from_item_speaker(
            &paths,
            "item-1",
            "speaker_a",
            "memory",
            "Series host",
            Some("Recurring host".to_string()),
        )
        .expect("create profile");
        assert_eq!(detail.profile.kind, "memory");
        assert_eq!(detail.references.len(), 1);
        assert!(Path::new(&detail.references[0].path).exists());

        let applied =
            apply_voice_library_profile_to_item(&paths, "item-1", "speaker_b", &detail.profile.id)
                .expect("apply");
        assert_eq!(
            applied.voice_profile_id.as_deref(),
            Some(detail.profile.id.as_str())
        );
        assert_eq!(applied.display_name.as_deref(), Some("Host"));
        assert_eq!(applied.subtitle_prosody_mode.as_deref(), Some("auto"));
    }

    #[test]
    fn suggest_profiles_prefers_exact_matches() {
        let dir = tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        db::ensure_schema(&paths).expect("schema");
        insert_test_item(&paths, "item-1");

        speakers::upsert_item_speaker_setting(
            &paths,
            "item-1",
            "host_main",
            Some("Main Host".to_string()),
            None,
            None,
            None,
            Some(Vec::new()),
            None,
            None,
            None,
            None,
            None,
        )
        .expect("upsert speaker");

        create_voice_library_profile(&paths, "memory", "Main Host", None).expect("create 1");
        create_voice_library_profile(&paths, "character", "Narrator", None).expect("create 2");
        let suggestions = suggest_voice_library_profiles_for_item(&paths, "item-1", Some("memory"))
            .expect("suggest");
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].item_speaker_key, "host_main");
        assert!(suggestions[0].score >= 95);
    }
}
