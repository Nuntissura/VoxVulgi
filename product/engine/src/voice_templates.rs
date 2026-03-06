use crate::paths::AppPaths;
use crate::{db, speakers, EngineError, Result};
use rusqlite::params;
use serde::{Deserialize, Serialize};
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
pub struct VoiceTemplateSpeaker {
    pub template_id: String,
    pub speaker_key: String,
    pub display_name: Option<String>,
    pub tts_voice_id: Option<String>,
    pub tts_voice_profile_path: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceTemplateDetail {
    pub template: VoiceTemplate,
    pub speakers: Vec<VoiceTemplateSpeaker>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceTemplateApplyMapping {
    pub item_speaker_key: String,
    pub template_speaker_key: String,
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

    let mut stmt = conn.prepare(
        r#"
SELECT
  template_id,
  speaker_key,
  display_name,
  tts_voice_id,
  tts_voice_profile_path,
  created_at_ms,
  updated_at_ms
FROM voice_template_speaker
WHERE template_id=?1
ORDER BY speaker_key ASC
"#,
    )?;

    let speakers = stmt
        .query_map(params![template_id], |row| {
            Ok(VoiceTemplateSpeaker {
                template_id: row.get(0)?,
                speaker_key: row.get(1)?,
                display_name: row.get(2)?,
                tts_voice_id: row.get(3)?,
                tts_voice_profile_path: row.get(4)?,
                created_at_ms: row.get(5)?,
                updated_at_ms: row.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(VoiceTemplateDetail { template, speakers })
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
            let copied_profile = copy_template_profile_if_present(
                &profiles_dir,
                &speaker.speaker_key,
                speaker.tts_voice_profile_path.as_deref(),
            )?;
            tx.execute(
                r#"
INSERT INTO voice_template_speaker (
  template_id,
  speaker_key,
  display_name,
  tts_voice_id,
  tts_voice_profile_path,
  created_at_ms,
  updated_at_ms
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
"#,
                params![
                    template_id,
                    speaker.speaker_key,
                    speaker.display_name,
                    speaker.tts_voice_id,
                    copied_profile,
                    now,
                    now
                ],
            )?;
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
        "DELETE FROM voice_template_speaker WHERE template_id=?1",
        params![template_id],
    )?;
    tx.execute("DELETE FROM voice_template WHERE id=?1", params![template_id])?;
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
        let display_name = template_speaker
            .display_name
            .clone()
            .or_else(|| existing.and_then(|value| value.display_name.clone()));
        let tts_voice_id = template_speaker
            .tts_voice_id
            .clone()
            .or_else(|| existing.and_then(|value| value.tts_voice_id.clone()));
        let tts_voice_profile_path = template_speaker
            .tts_voice_profile_path
            .clone()
            .or_else(|| existing.and_then(|value| value.tts_voice_profile_path.clone()));

        speakers::upsert_item_speaker_setting(
            paths,
            item_id,
            item_speaker_key,
            display_name,
            tts_voice_id,
            tts_voice_profile_path,
        )?;
    }

    speakers::list_item_speaker_settings(paths, item_id)
}

fn copy_template_profile_if_present(
    profiles_dir: &Path,
    speaker_key: &str,
    source_path: Option<&str>,
) -> Result<Option<String>> {
    let Some(source_path) = source_path.and_then(normalize_non_empty) else {
        return Ok(None);
    };
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
    let file_name = if let Some(extension) = extension {
        format!(
            "{}.{}",
            sanitize_file_component(speaker_key),
            sanitize_file_component(extension)
        )
    } else {
        sanitize_file_component(speaker_key)
    };
    let destination = profiles_dir.join(file_name);
    std::fs::copy(source, &destination)?;
    Ok(Some(destination.to_string_lossy().to_string()))
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
            Some("af_heart".to_string()),
            Some(source_profile.to_string_lossy().to_string()),
        )
        .expect("upsert speaker");

        let detail =
            create_voice_template_from_item(&paths, "item-1", "Episode host").expect("template");

        assert_eq!(detail.template.name, "Episode host");
        assert_eq!(detail.template.speaker_count, 1);
        assert_eq!(detail.speakers.len(), 1);
        let copied = detail.speakers[0]
            .tts_voice_profile_path
            .clone()
            .expect("copied profile path");
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
        insert_test_item(&paths, "template-item", &template_media_path, "Template source");
        insert_test_item(&paths, "target-item", &target_media_path, "Target source");

        speakers::upsert_item_speaker_setting(
            &paths,
            "template-item",
            "S1",
            Some("Panel Host".to_string()),
            Some("af_heart".to_string()),
            Some(source_profile.to_string_lossy().to_string()),
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
        )
        .expect("target speaker 1");
        speakers::upsert_item_speaker_setting(
            &paths,
            "target-item",
            "S10",
            Some("Leave Alone".to_string()),
            Some("bf_alex".to_string()),
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
        assert!(
            mapped
                .tts_voice_profile_path
                .as_deref()
                .map(Path::new)
                .is_some_and(|path| path.exists())
        );

        let untouched = by_key.get("S10").expect("untouched speaker");
        assert_eq!(untouched.display_name.as_deref(), Some("Leave Alone"));
        assert_eq!(untouched.tts_voice_id.as_deref(), Some("bf_alex"));
    }
}
