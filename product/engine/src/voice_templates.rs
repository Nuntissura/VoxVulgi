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
pub struct VoiceTemplate {
    pub id: String,
    pub name: String,
    pub speaker_count: usize,
    pub dir_path: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceTemplateReference {
    pub template_id: String,
    pub speaker_key: String,
    pub reference_id: String,
    pub label: Option<String>,
    pub path: String,
    pub sort_order: i64,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceTemplateSpeaker {
    pub template_id: String,
    pub speaker_key: String,
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
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceTemplateDetail {
    pub template: VoiceTemplate,
    pub speakers: Vec<VoiceTemplateSpeaker>,
    pub references: Vec<VoiceTemplateReference>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceTemplateApplyMapping {
    pub item_speaker_key: String,
    pub template_speaker_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceTemplateSpeakerUpdate {
    pub display_name: Option<String>,
    pub tts_voice_id: Option<String>,
    pub style_preset: Option<String>,
    pub prosody_preset: Option<String>,
    pub pronunciation_overrides: Option<String>,
    pub render_mode: Option<String>,
    pub subtitle_prosody_mode: Option<String>,
}

pub fn list_voice_templates(paths: &AppPaths) -> Result<Vec<VoiceTemplate>> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;

    let mut stmt = conn.prepare(
        r#"
SELECT
  vt.id,
  vt.name,
  vt.created_at_ms,
  vt.updated_at_ms,
  COUNT(vts.speaker_key) AS speaker_count
FROM voice_template vt
LEFT JOIN voice_template_speaker vts ON vts.template_id = vt.id
GROUP BY vt.id, vt.name, vt.created_at_ms, vt.updated_at_ms
ORDER BY vt.updated_at_ms DESC, vt.name COLLATE NOCASE ASC
"#,
    )?;

    let rows = stmt
        .query_map([], |row| {
            let id: String = row.get(0)?;
            Ok(VoiceTemplate {
                dir_path: paths.voice_template_dir(&id).to_string_lossy().to_string(),
                id,
                name: row.get(1)?,
                created_at_ms: row.get(2)?,
                updated_at_ms: row.get(3)?,
                speaker_count: row.get::<_, i64>(4)? as usize,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(rows)
}

pub fn get_voice_template(paths: &AppPaths, template_id: &str) -> Result<VoiceTemplateDetail> {
    let template_id = template_id.trim();
    if template_id.is_empty() {
        return Err(EngineError::InstallFailed(
            "template_id is empty".to_string(),
        ));
    }

    let conn = db::open(paths)?;
    db::migrate(&conn)?;

    let template = conn.query_row(
        r#"
SELECT
  vt.id,
  vt.name,
  vt.created_at_ms,
  vt.updated_at_ms,
  COUNT(vts.speaker_key) AS speaker_count
FROM voice_template vt
LEFT JOIN voice_template_speaker vts ON vts.template_id = vt.id
WHERE vt.id=?1
GROUP BY vt.id, vt.name, vt.created_at_ms, vt.updated_at_ms
"#,
        params![template_id],
        |row| {
            let id: String = row.get(0)?;
            Ok(VoiceTemplate {
                dir_path: paths.voice_template_dir(&id).to_string_lossy().to_string(),
                id,
                name: row.get(1)?,
                created_at_ms: row.get(2)?,
                updated_at_ms: row.get(3)?,
                speaker_count: row.get::<_, i64>(4)? as usize,
            })
        },
    )?;

    let references = list_voice_template_references(&conn, template_id)?;
    let mut refs_by_speaker: HashMap<String, Vec<VoiceTemplateReference>> = HashMap::new();
    for reference in &references {
        refs_by_speaker
            .entry(reference.speaker_key.clone())
            .or_default()
            .push(reference.clone());
    }

    let mut stmt = conn.prepare(
        r#"
SELECT
  template_id,
  speaker_key,
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
FROM voice_template_speaker
WHERE template_id=?1
ORDER BY speaker_key ASC
"#,
    )?;

    let speakers = stmt
        .query_map(params![template_id], |row| {
            let template_id: String = row.get(0)?;
            let speaker_key: String = row.get(1)?;
            let single_profile_path: Option<String> = row.get(4)?;
            let mut profile_paths = decode_profile_paths(
                row.get::<_, Option<String>>(5)?,
                single_profile_path.clone(),
            );
            if let Some(reference_paths) = refs_by_speaker.get(&speaker_key).map(|refs| {
                refs.iter()
                    .map(|reference| reference.path.clone())
                    .collect::<Vec<_>>()
            }) {
                if !reference_paths.is_empty() {
                    profile_paths = reference_paths;
                }
            }
            Ok(VoiceTemplateSpeaker {
                template_id,
                speaker_key,
                display_name: row.get(2)?,
                tts_voice_id: row.get(3)?,
                tts_voice_profile_path: profile_paths.first().cloned().or(single_profile_path),
                tts_voice_profile_paths: profile_paths,
                style_preset: row.get(6)?,
                prosody_preset: row.get(7)?,
                pronunciation_overrides: row.get(8)?,
                render_mode: row.get(9)?,
                subtitle_prosody_mode: row.get(10)?,
                created_at_ms: row.get(11)?,
                updated_at_ms: row.get(12)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(VoiceTemplateDetail {
        template,
        speakers,
        references,
    })
}

pub fn create_voice_template_from_item(
    paths: &AppPaths,
    item_id: &str,
    name: &str,
) -> Result<VoiceTemplateDetail> {
    let item_id = item_id.trim();
    if item_id.is_empty() {
        return Err(EngineError::InstallFailed("item_id is empty".to_string()));
    }
    let name = normalize_non_empty(name)
        .ok_or_else(|| EngineError::InstallFailed("template name is empty".to_string()))?;

    let item_speakers = speakers::list_item_speaker_settings(paths, item_id)?;
    if item_speakers.is_empty() {
        return Err(EngineError::InstallFailed(
            "no speaker settings found for item".to_string(),
        ));
    }

    let template_id = Uuid::new_v4().to_string();
    let template_dir = paths.voice_template_dir(&template_id);
    let profiles_dir = paths.voice_template_profiles_dir(&template_id);
    std::fs::create_dir_all(&profiles_dir)?;

    let now = now_ms();
    let result = (|| -> Result<()> {
        let mut conn = db::open(paths)?;
        db::migrate(&conn)?;
        let tx = conn.transaction()?;
        tx.execute(
            r#"
INSERT INTO voice_template (
  id,
  name,
  created_at_ms,
  updated_at_ms
) VALUES (?1, ?2, ?3, ?4)
"#,
            params![template_id, name, now, now],
        )?;

        for speaker in &item_speakers {
            let copied_references = copy_template_references(
                &profiles_dir,
                &speaker.speaker_key,
                &speaker.tts_voice_profile_paths,
            )?;
            insert_template_speaker_row(&tx, &template_id, speaker, &copied_references, now)?;
        }

        tx.commit()?;
        Ok(())
    })();

    if result.is_err() && template_dir.exists() {
        let _ = std::fs::remove_dir_all(&template_dir);
    }
    result?;

    get_voice_template(paths, &template_id)
}

pub fn update_voice_template_speaker(
    paths: &AppPaths,
    template_id: &str,
    speaker_key: &str,
    update: VoiceTemplateSpeakerUpdate,
) -> Result<VoiceTemplateDetail> {
    let template_id = template_id.trim();
    let speaker_key = speaker_key.trim();
    if template_id.is_empty() || speaker_key.is_empty() {
        return Err(EngineError::InstallFailed(
            "template_id or speaker_key is empty".to_string(),
        ));
    }

    let mut conn = db::open(paths)?;
    db::migrate(&conn)?;
    let tx = conn.transaction()?;
    let now = now_ms();
    let updated = tx.execute(
        r#"
UPDATE voice_template_speaker
SET
  display_name=?3,
  tts_voice_id=?4,
  style_preset=?5,
  prosody_preset=?6,
  pronunciation_overrides=?7,
  render_mode=?8,
  subtitle_prosody_mode=?9,
  updated_at_ms=?10
WHERE template_id=?1 AND speaker_key=?2
"#,
        params![
            template_id,
            speaker_key,
            normalize_optional_string(update.display_name),
            normalize_optional_string(update.tts_voice_id),
            normalize_optional_string(update.style_preset),
            normalize_optional_string(update.prosody_preset),
            normalize_optional_string(update.pronunciation_overrides),
            normalize_optional_string(update.render_mode),
            normalize_optional_string(update.subtitle_prosody_mode),
            now,
        ],
    )?;
    if updated == 0 {
        return Err(EngineError::InstallFailed(format!(
            "template speaker not found: {template_id}/{speaker_key}"
        )));
    }
    tx.execute(
        "UPDATE voice_template SET updated_at_ms=?2 WHERE id=?1",
        params![template_id, now],
    )?;
    tx.commit()?;

    get_voice_template(paths, template_id)
}

pub fn add_voice_template_reference(
    paths: &AppPaths,
    template_id: &str,
    speaker_key: &str,
    source_path: &str,
    label: Option<String>,
) -> Result<VoiceTemplateDetail> {
    let template_id = template_id.trim();
    let speaker_key = speaker_key.trim();
    let source_path = source_path.trim();
    if template_id.is_empty() || speaker_key.is_empty() || source_path.is_empty() {
        return Err(EngineError::InstallFailed(
            "template_id, speaker_key, or source_path is empty".to_string(),
        ));
    }

    let template_profiles_dir = paths.voice_template_profiles_dir(template_id);
    std::fs::create_dir_all(&template_profiles_dir)?;

    let mut conn = db::open(paths)?;
    db::migrate(&conn)?;
    let tx = conn.transaction()?;

    let exists = tx.query_row(
        "SELECT COUNT(*) FROM voice_template_speaker WHERE template_id=?1 AND speaker_key=?2",
        params![template_id, speaker_key],
        |row| row.get::<_, i64>(0),
    )?;
    if exists == 0 {
        return Err(EngineError::InstallFailed(format!(
            "template speaker not found: {template_id}/{speaker_key}"
        )));
    }

    let next_sort_order = tx.query_row(
        "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM voice_template_reference WHERE template_id=?1 AND speaker_key=?2",
        params![template_id, speaker_key],
        |row| row.get::<_, i64>(0),
    )?;
    let copied = copy_single_template_reference(
        &template_profiles_dir,
        speaker_key,
        source_path,
        label,
        next_sort_order,
    )?;
    insert_template_reference_row(&tx, template_id, &copied)?;
    refresh_template_speaker_profile_cache(&tx, template_id, speaker_key)?;
    tx.execute(
        "UPDATE voice_template SET updated_at_ms=?2 WHERE id=?1",
        params![template_id, now_ms()],
    )?;
    tx.commit()?;

    get_voice_template(paths, template_id)
}

pub fn remove_voice_template_reference(
    paths: &AppPaths,
    template_id: &str,
    speaker_key: &str,
    reference_id: &str,
) -> Result<VoiceTemplateDetail> {
    let template_id = template_id.trim();
    let speaker_key = speaker_key.trim();
    let reference_id = reference_id.trim();
    if template_id.is_empty() || speaker_key.is_empty() || reference_id.is_empty() {
        return Err(EngineError::InstallFailed(
            "template_id, speaker_key, or reference_id is empty".to_string(),
        ));
    }

    let mut conn = db::open(paths)?;
    db::migrate(&conn)?;
    let tx = conn.transaction()?;
    let reference_path: String = tx.query_row(
        "SELECT path FROM voice_template_reference WHERE template_id=?1 AND speaker_key=?2 AND reference_id=?3",
        params![template_id, speaker_key, reference_id],
        |row| row.get(0),
    )?;
    tx.execute(
        "DELETE FROM voice_template_reference WHERE template_id=?1 AND speaker_key=?2 AND reference_id=?3",
        params![template_id, speaker_key, reference_id],
    )?;
    refresh_template_speaker_profile_cache(&tx, template_id, speaker_key)?;
    tx.execute(
        "UPDATE voice_template SET updated_at_ms=?2 WHERE id=?1",
        params![template_id, now_ms()],
    )?;
    tx.commit()?;

    let reference_path = Path::new(&reference_path);
    if reference_path.exists() {
        let _ = std::fs::remove_file(reference_path);
    }

    get_voice_template(paths, template_id)
}

pub fn delete_voice_template(paths: &AppPaths, template_id: &str) -> Result<()> {
    let template_id = template_id.trim();
    if template_id.is_empty() {
        return Err(EngineError::InstallFailed(
            "template_id is empty".to_string(),
        ));
    }

    let mut conn = db::open(paths)?;
    db::migrate(&conn)?;
    let tx = conn.transaction()?;
    tx.execute(
        "DELETE FROM voice_template_reference WHERE template_id=?1",
        params![template_id],
    )?;
    tx.execute(
        "DELETE FROM voice_template_speaker WHERE template_id=?1",
        params![template_id],
    )?;
    tx.execute(
        "DELETE FROM voice_template WHERE id=?1",
        params![template_id],
    )?;
    tx.commit()?;

    let template_dir = paths.voice_template_dir(template_id);
    if template_dir.exists() {
        std::fs::remove_dir_all(template_dir)?;
    }

    Ok(())
}

pub fn apply_voice_template_to_item(
    paths: &AppPaths,
    item_id: &str,
    template_id: &str,
    mappings: &[VoiceTemplateApplyMapping],
) -> Result<Vec<speakers::ItemSpeakerSetting>> {
    let item_id = item_id.trim();
    if item_id.is_empty() {
        return Err(EngineError::InstallFailed("item_id is empty".to_string()));
    }
    let template_id = template_id.trim();
    if template_id.is_empty() {
        return Err(EngineError::InstallFailed(
            "template_id is empty".to_string(),
        ));
    }
    if mappings.is_empty() {
        return Err(EngineError::InstallFailed(
            "no speaker mappings were provided".to_string(),
        ));
    }

    let detail = get_voice_template(paths, template_id)?;
    let template_by_key: HashMap<String, VoiceTemplateSpeaker> = detail
        .speakers
        .into_iter()
        .map(|speaker| (speaker.speaker_key.clone(), speaker))
        .collect();
    let existing_by_key: HashMap<String, speakers::ItemSpeakerSetting> =
        speakers::list_item_speaker_settings(paths, item_id)?
            .into_iter()
            .map(|setting| (setting.speaker_key.clone(), setting))
            .collect();

    let mut seen_item_keys = HashSet::new();
    for mapping in mappings {
        let item_speaker_key = mapping.item_speaker_key.trim();
        let template_speaker_key = mapping.template_speaker_key.trim();
        if item_speaker_key.is_empty() || template_speaker_key.is_empty() {
            return Err(EngineError::InstallFailed(
                "speaker mapping contains an empty key".to_string(),
            ));
        }
        if !seen_item_keys.insert(item_speaker_key.to_string()) {
            return Err(EngineError::InstallFailed(format!(
                "duplicate mapping for item speaker {item_speaker_key}"
            )));
        }

        let template_speaker = template_by_key.get(template_speaker_key).ok_or_else(|| {
            EngineError::InstallFailed(format!(
                "template speaker not found: {template_speaker_key}"
            ))
        })?;
        let existing = existing_by_key.get(item_speaker_key);
        speakers::upsert_item_speaker_setting(
            paths,
            item_id,
            item_speaker_key,
            template_speaker
                .display_name
                .clone()
                .or_else(|| existing.and_then(|value| value.display_name.clone())),
            existing.and_then(|value| value.voice_profile_id.clone()),
            template_speaker
                .tts_voice_id
                .clone()
                .or_else(|| existing.and_then(|value| value.tts_voice_id.clone())),
            template_speaker
                .tts_voice_profile_path
                .clone()
                .or_else(|| existing.and_then(|value| value.tts_voice_profile_path.clone())),
            Some(if template_speaker.tts_voice_profile_paths.is_empty() {
                existing
                    .map(|value| value.tts_voice_profile_paths.clone())
                    .unwrap_or_default()
            } else {
                template_speaker.tts_voice_profile_paths.clone()
            }),
            template_speaker
                .style_preset
                .clone()
                .or_else(|| existing.and_then(|value| value.style_preset.clone())),
            template_speaker
                .prosody_preset
                .clone()
                .or_else(|| existing.and_then(|value| value.prosody_preset.clone())),
            template_speaker
                .pronunciation_overrides
                .clone()
                .or_else(|| existing.and_then(|value| value.pronunciation_overrides.clone())),
            template_speaker
                .render_mode
                .clone()
                .or_else(|| existing.and_then(|value| value.render_mode.clone())),
            template_speaker
                .subtitle_prosody_mode
                .clone()
                .or_else(|| existing.and_then(|value| value.subtitle_prosody_mode.clone())),
        )?;
    }

    speakers::list_item_speaker_settings(paths, item_id)
}

fn list_voice_template_references(
    conn: &rusqlite::Connection,
    template_id: &str,
) -> Result<Vec<VoiceTemplateReference>> {
    let mut stmt = conn.prepare(
        r#"
SELECT
  template_id,
  speaker_key,
  reference_id,
  label,
  path,
  sort_order,
  created_at_ms,
  updated_at_ms
FROM voice_template_reference
WHERE template_id=?1
ORDER BY speaker_key ASC, sort_order ASC, created_at_ms ASC
"#,
    )?;
    let rows = stmt
        .query_map(params![template_id], |row| {
            Ok(VoiceTemplateReference {
                template_id: row.get(0)?,
                speaker_key: row.get(1)?,
                reference_id: row.get(2)?,
                label: row.get(3)?,
                path: row.get(4)?,
                sort_order: row.get(5)?,
                created_at_ms: row.get(6)?,
                updated_at_ms: row.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn insert_template_speaker_row(
    conn: &rusqlite::Connection,
    template_id: &str,
    speaker: &speakers::ItemSpeakerSetting,
    references: &[VoiceTemplateReference],
    now: i64,
) -> Result<()> {
    let reference_paths = references
        .iter()
        .map(|reference| reference.path.clone())
        .collect::<Vec<_>>();
    let primary_reference = reference_paths.first().cloned();
    conn.execute(
        r#"
INSERT INTO voice_template_speaker (
  template_id,
  speaker_key,
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
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
"#,
        params![
            template_id,
            speaker.speaker_key,
            speaker.display_name,
            speaker.tts_voice_id,
            primary_reference,
            encode_profile_paths(&reference_paths),
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
        insert_template_reference_row(conn, template_id, reference)?;
    }

    Ok(())
}

fn insert_template_reference_row(
    conn: &rusqlite::Connection,
    template_id: &str,
    reference: &VoiceTemplateReference,
) -> Result<()> {
    conn.execute(
        r#"
INSERT INTO voice_template_reference (
  template_id,
  speaker_key,
  reference_id,
  label,
  path,
  sort_order,
  created_at_ms,
  updated_at_ms
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
"#,
        params![
            template_id,
            reference.speaker_key,
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

fn refresh_template_speaker_profile_cache(
    conn: &rusqlite::Connection,
    template_id: &str,
    speaker_key: &str,
) -> Result<()> {
    let references = list_voice_template_references(conn, template_id)?
        .into_iter()
        .filter(|reference| reference.speaker_key == speaker_key)
        .collect::<Vec<_>>();
    let profile_paths = references
        .iter()
        .map(|reference| reference.path.clone())
        .collect::<Vec<_>>();
    let primary_profile_path = profile_paths.first().cloned();
    conn.execute(
        r#"
UPDATE voice_template_speaker
SET
  tts_voice_profile_path=?3,
  tts_voice_profile_paths_json=?4,
  updated_at_ms=?5
WHERE template_id=?1 AND speaker_key=?2
"#,
        params![
            template_id,
            speaker_key,
            primary_profile_path,
            encode_profile_paths(&profile_paths),
            now_ms(),
        ],
    )?;
    Ok(())
}

fn copy_template_references(
    profiles_dir: &Path,
    speaker_key: &str,
    profile_paths: &[String],
) -> Result<Vec<VoiceTemplateReference>> {
    let mut references: Vec<VoiceTemplateReference> = Vec::new();
    for (index, profile_path) in profile_paths.iter().enumerate() {
        references.push(copy_single_template_reference(
            profiles_dir,
            speaker_key,
            profile_path,
            Some(file_name_from_path(profile_path)),
            index as i64,
        )?);
    }
    Ok(references)
}

fn copy_single_template_reference(
    profiles_dir: &Path,
    speaker_key: &str,
    source_path: &str,
    label: Option<String>,
    sort_order: i64,
) -> Result<VoiceTemplateReference> {
    let source_path = normalize_non_empty(source_path).ok_or_else(|| {
        EngineError::InstallFailed("voice profile source path is empty".to_string())
    })?;
    let source = Path::new(&source_path);
    if !source.exists() {
        return Err(EngineError::InstallFailed(format!(
            "voice profile file does not exist for speaker {speaker_key}: {source_path}"
        )));
    }
    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.trim())
        .filter(|value| !value.is_empty());
    let reference_id = Uuid::new_v4().to_string();
    let file_name = if let Some(extension) = extension {
        format!(
            "{}_{:02}_{}.{}",
            sanitize_file_component(speaker_key),
            sort_order.max(0),
            &reference_id[..8],
            sanitize_file_component(extension)
        )
    } else {
        format!(
            "{}_{:02}_{}",
            sanitize_file_component(speaker_key),
            sort_order.max(0),
            &reference_id[..8]
        )
    };
    let destination = profiles_dir.join(file_name);
    std::fs::copy(source, &destination)?;
    let now = now_ms();
    Ok(VoiceTemplateReference {
        template_id: String::new(),
        speaker_key: speaker_key.to_string(),
        reference_id,
        label: normalize_optional_string(label),
        path: destination.to_string_lossy().to_string(),
        sort_order,
        created_at_ms: now,
        updated_at_ms: now,
    })
}

fn file_name_from_path(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .map(|value| value.to_string())
        .unwrap_or_else(|| path.to_string())
}

fn sanitize_file_component(value: &str) -> String {
    let mut out = String::new();
    let mut prev_underscore = false;
    for ch in value.chars() {
        let keep = ch.is_ascii_alphanumeric() || ch == '-' || ch == '_';
        let mapped = if keep { ch } else { '_' };
        if mapped == '_' {
            if prev_underscore {
                continue;
            }
            prev_underscore = true;
        } else {
            prev_underscore = false;
        }
        out.push(mapped);
    }
    let trimmed = out.trim_matches('_').trim();
    if trimmed.is_empty() {
        "speaker".to_string()
    } else {
        trimmed.to_string()
    }
}

fn normalize_non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|value| normalize_non_empty(&value))
}

fn encode_profile_paths(profile_paths: &[String]) -> Option<String> {
    if profile_paths.is_empty() {
        None
    } else {
        Some(serde_json::to_string(profile_paths).unwrap_or_else(|_| "[]".to_string()))
    }
}

fn decode_profile_paths(
    profile_paths_json: Option<String>,
    single_profile_path: Option<String>,
) -> Vec<String> {
    if let Some(profile_paths_json) = profile_paths_json {
        if let Ok(Value::Array(values)) = serde_json::from_str::<Value>(&profile_paths_json) {
            let mut out: Vec<String> = Vec::new();
            for value in values {
                let Some(value) = value.as_str() else {
                    continue;
                };
                let Some(value) = normalize_non_empty(value) else {
                    continue;
                };
                if out.iter().any(|existing| existing == &value) {
                    continue;
                }
                out.push(value);
            }
            if !out.is_empty() {
                return out;
            }
        }
    }

    single_profile_path
        .and_then(|value| normalize_non_empty(&value))
        .map(|value| vec![value])
        .unwrap_or_default()
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
    use rusqlite::params;
    use tempfile::tempdir;

    fn insert_test_item(paths: &AppPaths, item_id: &str, media_path: &Path, title: &str) {
        let conn = db::open(paths).expect("open db");
        db::migrate(&conn).expect("migrate db");
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
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, NULL, NULL, NULL, NULL, NULL, NULL)
"#,
            params![
                item_id,
                now_ms(),
                "local_file",
                media_path.to_string_lossy().to_string(),
                title,
                media_path.to_string_lossy().to_string(),
            ],
        )
        .expect("insert library item");
    }

    #[test]
    fn create_voice_template_copies_reference_clips() {
        let tmp = tempdir().expect("tempdir");
        let paths = AppPaths::new(tmp.path().to_path_buf());
        let media_path = tmp.path().join("item-1.mp4");
        let source_profile = tmp.path().join("source_profiles").join("host.wav");
        std::fs::write(&media_path, b"fake-media").expect("write media");
        std::fs::create_dir_all(source_profile.parent().expect("parent")).expect("mkdirs");
        std::fs::write(&source_profile, b"fake-wav").expect("write source profile");
        insert_test_item(&paths, "item-1", &media_path, "Episode 1");

        speakers::upsert_item_speaker_setting(
            &paths,
            "item-1",
            "S1",
            Some("Host".to_string()),
            None,
            Some("af_heart".to_string()),
            Some(source_profile.to_string_lossy().to_string()),
            Some(vec![source_profile.to_string_lossy().to_string()]),
            Some("documentary".to_string()),
            Some("warm".to_string()),
            Some("Seoul => Soul".to_string()),
            Some("clone".to_string()),
            None,
        )
        .expect("upsert speaker");

        let detail =
            create_voice_template_from_item(&paths, "item-1", "Episode host").expect("template");

        assert_eq!(detail.template.name, "Episode host");
        assert_eq!(detail.template.speaker_count, 1);
        assert_eq!(detail.speakers.len(), 1);
        assert_eq!(detail.references.len(), 1);
        assert_eq!(
            detail.speakers[0].style_preset.as_deref(),
            Some("documentary")
        );
        assert_eq!(detail.speakers[0].prosody_preset.as_deref(), Some("warm"));
        assert_eq!(
            detail.speakers[0].pronunciation_overrides.as_deref(),
            Some("Seoul => Soul")
        );
        assert_eq!(detail.speakers[0].render_mode.as_deref(), Some("clone"));
        let copied = detail.references[0].path.clone();
        assert!(copied.contains("voice_templates"));
        assert!(Path::new(&copied).exists());
        assert_ne!(copied, source_profile.to_string_lossy().to_string());
    }

    #[test]
    fn apply_voice_template_updates_only_selected_speakers() {
        let tmp = tempdir().expect("tempdir");
        let paths = AppPaths::new(tmp.path().to_path_buf());
        let template_media_path = tmp.path().join("template-item.mp4");
        let target_media_path = tmp.path().join("target-item.mp4");
        let source_profile = tmp.path().join("source_profiles").join("panel.wav");
        std::fs::write(&template_media_path, b"fake-media").expect("write template media");
        std::fs::write(&target_media_path, b"fake-media").expect("write target media");
        std::fs::create_dir_all(source_profile.parent().expect("parent")).expect("mkdirs");
        std::fs::write(&source_profile, b"fake-wav").expect("write source profile");
        insert_test_item(
            &paths,
            "template-item",
            &template_media_path,
            "Template source",
        );
        insert_test_item(&paths, "target-item", &target_media_path, "Target source");

        speakers::upsert_item_speaker_setting(
            &paths,
            "template-item",
            "S1",
            Some("Panel Host".to_string()),
            None,
            Some("af_heart".to_string()),
            Some(source_profile.to_string_lossy().to_string()),
            Some(vec![source_profile.to_string_lossy().to_string()]),
            Some("game_show".to_string()),
            Some("excited".to_string()),
            Some("Miyyeon => Miyeon".to_string()),
            Some("standard_tts".to_string()),
            None,
        )
        .expect("template speaker");
        let template =
            create_voice_template_from_item(&paths, "template-item", "Panel").expect("template");

        speakers::upsert_item_speaker_setting(
            &paths,
            "target-item",
            "S9",
            Some("Old Name".to_string()),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .expect("target speaker 1");
        speakers::upsert_item_speaker_setting(
            &paths,
            "target-item",
            "S10",
            Some("Leave Alone".to_string()),
            None,
            Some("bf_alex".to_string()),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .expect("target speaker 2");

        let applied = apply_voice_template_to_item(
            &paths,
            "target-item",
            &template.template.id,
            &[VoiceTemplateApplyMapping {
                item_speaker_key: "S9".to_string(),
                template_speaker_key: "S1".to_string(),
            }],
        )
        .expect("apply template");

        let by_key: HashMap<String, speakers::ItemSpeakerSetting> = applied
            .into_iter()
            .map(|setting| (setting.speaker_key.clone(), setting))
            .collect();
        let mapped = by_key.get("S9").expect("mapped speaker");
        assert_eq!(mapped.display_name.as_deref(), Some("Panel Host"));
        assert_eq!(mapped.tts_voice_id.as_deref(), Some("af_heart"));
        assert_eq!(mapped.style_preset.as_deref(), Some("game_show"));
        assert_eq!(mapped.prosody_preset.as_deref(), Some("excited"));
        assert_eq!(
            mapped.pronunciation_overrides.as_deref(),
            Some("Miyyeon => Miyeon")
        );
        assert_eq!(mapped.render_mode.as_deref(), Some("standard_tts"));
        assert_eq!(mapped.tts_voice_profile_paths.len(), 1);
        assert!(mapped
            .tts_voice_profile_paths
            .first()
            .map(|path| Path::new(path).exists())
            .unwrap_or(false));

        let untouched = by_key.get("S10").expect("untouched speaker");
        assert_eq!(untouched.display_name.as_deref(), Some("Leave Alone"));
        assert_eq!(untouched.tts_voice_id.as_deref(), Some("bf_alex"));
    }

    #[test]
    fn add_and_remove_voice_template_reference_updates_reference_cache() {
        let tmp = tempdir().expect("tempdir");
        let paths = AppPaths::new(tmp.path().to_path_buf());
        let media_path = tmp.path().join("item-2.mp4");
        let source_profile_a = tmp.path().join("source_profiles").join("host_a.wav");
        let source_profile_b = tmp.path().join("source_profiles").join("host_b.wav");
        std::fs::write(&media_path, b"fake-media").expect("write media");
        std::fs::create_dir_all(source_profile_a.parent().expect("parent")).expect("mkdirs");
        std::fs::write(&source_profile_a, b"fake-a").expect("write profile a");
        std::fs::write(&source_profile_b, b"fake-b").expect("write profile b");
        insert_test_item(&paths, "item-2", &media_path, "Episode 2");

        speakers::upsert_item_speaker_setting(
            &paths,
            "item-2",
            "S1",
            Some("Host".to_string()),
            None,
            Some("af_heart".to_string()),
            Some(source_profile_a.to_string_lossy().to_string()),
            Some(vec![source_profile_a.to_string_lossy().to_string()]),
            None,
            None,
            None,
            Some("clone".to_string()),
            None,
        )
        .expect("upsert speaker");

        let template =
            create_voice_template_from_item(&paths, "item-2", "Episode host").expect("template");
        let detail = add_voice_template_reference(
            &paths,
            &template.template.id,
            "S1",
            &source_profile_b.to_string_lossy(),
            Some("backup".to_string()),
        )
        .expect("add reference");
        let speaker = detail.speakers.first().expect("speaker");
        assert_eq!(speaker.tts_voice_profile_paths.len(), 2);
        assert_eq!(detail.references.len(), 2);

        let detail = remove_voice_template_reference(
            &paths,
            &template.template.id,
            "S1",
            &detail.references[0].reference_id,
        )
        .expect("remove reference");
        assert_eq!(detail.references.len(), 1);
        assert_eq!(detail.speakers[0].tts_voice_profile_paths.len(), 1);
    }
}
