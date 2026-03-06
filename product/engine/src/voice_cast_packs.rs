use crate::paths::AppPaths;
use crate::{db, speakers, voice_templates, EngineError, Result};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceCastPack {
    pub id: String,
    pub name: String,
    pub role_count: usize,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceCastPackRole {
    pub pack_id: String,
    pub role_key: String,
    pub display_name: Option<String>,
    pub template_id: String,
    pub template_speaker_key: String,
    pub style_preset: Option<String>,
    pub prosody_preset: Option<String>,
    pub pronunciation_overrides: Option<String>,
    pub render_mode: Option<String>,
    pub subtitle_prosody_mode: Option<String>,
    pub sort_order: i64,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceCastPackDetail {
    pub pack: VoiceCastPack,
    pub roles: Vec<VoiceCastPackRole>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceCastPackApplyMapping {
    pub item_speaker_key: String,
    pub pack_role_key: String,
}

pub fn list_voice_cast_packs(paths: &AppPaths) -> Result<Vec<VoiceCastPack>> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;

    let mut stmt = conn.prepare(
        r#"
SELECT
  p.id,
  p.name,
  p.created_at_ms,
  p.updated_at_ms,
  COUNT(r.role_key) AS role_count
FROM voice_cast_pack p
LEFT JOIN voice_cast_pack_role r ON r.pack_id = p.id
GROUP BY p.id, p.name, p.created_at_ms, p.updated_at_ms
ORDER BY p.updated_at_ms DESC, p.name COLLATE NOCASE ASC
"#,
    )?;

    let rows = stmt
        .query_map([], |row| {
            Ok(VoiceCastPack {
                id: row.get(0)?,
                name: row.get(1)?,
                created_at_ms: row.get(2)?,
                updated_at_ms: row.get(3)?,
                role_count: row.get::<_, i64>(4)? as usize,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn get_voice_cast_pack(paths: &AppPaths, pack_id: &str) -> Result<VoiceCastPackDetail> {
    let pack_id = pack_id.trim();
    if pack_id.is_empty() {
        return Err(EngineError::InstallFailed("pack_id is empty".to_string()));
    }

    let conn = db::open(paths)?;
    db::migrate(&conn)?;

    let pack = conn.query_row(
        r#"
SELECT
  p.id,
  p.name,
  p.created_at_ms,
  p.updated_at_ms,
  COUNT(r.role_key) AS role_count
FROM voice_cast_pack p
LEFT JOIN voice_cast_pack_role r ON r.pack_id = p.id
WHERE p.id=?1
GROUP BY p.id, p.name, p.created_at_ms, p.updated_at_ms
"#,
        params![pack_id],
        |row| {
            Ok(VoiceCastPack {
                id: row.get(0)?,
                name: row.get(1)?,
                created_at_ms: row.get(2)?,
                updated_at_ms: row.get(3)?,
                role_count: row.get::<_, i64>(4)? as usize,
            })
        },
    )?;

    let mut stmt = conn.prepare(
        r#"
SELECT
  pack_id,
  role_key,
  display_name,
  template_id,
  template_speaker_key,
  style_preset,
  prosody_preset,
  pronunciation_overrides,
  render_mode,
  subtitle_prosody_mode,
  sort_order,
  created_at_ms,
  updated_at_ms
FROM voice_cast_pack_role
WHERE pack_id=?1
ORDER BY sort_order ASC, role_key ASC
"#,
    )?;
    let roles = stmt
        .query_map(params![pack_id], |row| {
            Ok(VoiceCastPackRole {
                pack_id: row.get(0)?,
                role_key: row.get(1)?,
                display_name: row.get(2)?,
                template_id: row.get(3)?,
                template_speaker_key: row.get(4)?,
                style_preset: row.get(5)?,
                prosody_preset: row.get(6)?,
                pronunciation_overrides: row.get(7)?,
                render_mode: row.get(8)?,
                subtitle_prosody_mode: row.get(9)?,
                sort_order: row.get(10)?,
                created_at_ms: row.get(11)?,
                updated_at_ms: row.get(12)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(VoiceCastPackDetail { pack, roles })
}

pub fn create_voice_cast_pack_from_template(
    paths: &AppPaths,
    template_id: &str,
    name: &str,
) -> Result<VoiceCastPackDetail> {
    let template_id = template_id.trim();
    let name = normalize_non_empty(name)
        .ok_or_else(|| EngineError::InstallFailed("cast pack name is empty".to_string()))?;
    if template_id.is_empty() {
        return Err(EngineError::InstallFailed(
            "template_id is empty".to_string(),
        ));
    }

    let template = voice_templates::get_voice_template(paths, template_id)?;
    if template.speakers.is_empty() {
        return Err(EngineError::InstallFailed(
            "template has no speakers".to_string(),
        ));
    }

    let pack_id = Uuid::new_v4().to_string();
    let now = now_ms();
    let mut used_role_keys = HashSet::new();

    let mut conn = db::open(paths)?;
    db::migrate(&conn)?;
    let tx = conn.transaction()?;
    tx.execute(
        r#"
INSERT INTO voice_cast_pack (
  id,
  name,
  created_at_ms,
  updated_at_ms
) VALUES (?1, ?2, ?3, ?4)
"#,
        params![pack_id, name, now, now],
    )?;

    for (index, speaker) in template.speakers.iter().enumerate() {
        let role_key = unique_role_key(
            &mut used_role_keys,
            &speaker
                .display_name
                .clone()
                .unwrap_or_else(|| speaker.speaker_key.clone()),
        );
        tx.execute(
            r#"
INSERT INTO voice_cast_pack_role (
  pack_id,
  role_key,
  display_name,
  template_id,
  template_speaker_key,
  style_preset,
  prosody_preset,
  pronunciation_overrides,
  render_mode,
  subtitle_prosody_mode,
  sort_order,
  created_at_ms,
  updated_at_ms
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
"#,
            params![
                pack_id,
                role_key,
                speaker.display_name,
                template_id,
                speaker.speaker_key,
                speaker.style_preset,
                speaker.prosody_preset,
                speaker.pronunciation_overrides,
                speaker.render_mode,
                speaker.subtitle_prosody_mode,
                index as i64,
                now,
                now,
            ],
        )?;
    }
    tx.commit()?;

    get_voice_cast_pack(paths, &pack_id)
}

pub fn update_voice_cast_pack(
    paths: &AppPaths,
    pack_id: &str,
    name: &str,
) -> Result<VoiceCastPackDetail> {
    let pack_id = pack_id.trim();
    let name = normalize_non_empty(name)
        .ok_or_else(|| EngineError::InstallFailed("cast pack name is empty".to_string()))?;
    if pack_id.is_empty() {
        return Err(EngineError::InstallFailed("pack_id is empty".to_string()));
    }

    let mut conn = db::open(paths)?;
    db::migrate(&conn)?;
    let tx = conn.transaction()?;
    let updated = tx.execute(
        "UPDATE voice_cast_pack SET name=?2, updated_at_ms=?3 WHERE id=?1",
        params![pack_id, name, now_ms()],
    )?;
    if updated == 0 {
        return Err(EngineError::InstallFailed(format!(
            "cast pack not found: {pack_id}"
        )));
    }
    tx.commit()?;
    get_voice_cast_pack(paths, pack_id)
}

pub fn delete_voice_cast_pack(paths: &AppPaths, pack_id: &str) -> Result<()> {
    let pack_id = pack_id.trim();
    if pack_id.is_empty() {
        return Err(EngineError::InstallFailed("pack_id is empty".to_string()));
    }

    let mut conn = db::open(paths)?;
    db::migrate(&conn)?;
    let tx = conn.transaction()?;
    tx.execute(
        "DELETE FROM voice_cast_pack_role WHERE pack_id=?1",
        params![pack_id],
    )?;
    tx.execute("DELETE FROM voice_cast_pack WHERE id=?1", params![pack_id])?;
    tx.commit()?;
    Ok(())
}

pub fn apply_voice_cast_pack_to_item(
    paths: &AppPaths,
    item_id: &str,
    pack_id: &str,
    mappings: &[VoiceCastPackApplyMapping],
) -> Result<Vec<speakers::ItemSpeakerSetting>> {
    let item_id = item_id.trim();
    let pack_id = pack_id.trim();
    if item_id.is_empty() || pack_id.is_empty() {
        return Err(EngineError::InstallFailed(
            "item_id or pack_id is empty".to_string(),
        ));
    }
    if mappings.is_empty() {
        return Err(EngineError::InstallFailed(
            "no pack mappings were provided".to_string(),
        ));
    }

    let detail = get_voice_cast_pack(paths, pack_id)?;
    let roles_by_key: HashMap<String, VoiceCastPackRole> = detail
        .roles
        .iter()
        .cloned()
        .map(|role| (role.role_key.clone(), role))
        .collect();

    let mut template_mapping: Vec<voice_templates::VoiceTemplateApplyMapping> = Vec::new();
    let mut seen_item_keys = HashSet::new();
    for mapping in mappings {
        let item_speaker_key = mapping.item_speaker_key.trim();
        let pack_role_key = mapping.pack_role_key.trim();
        if item_speaker_key.is_empty() || pack_role_key.is_empty() {
            return Err(EngineError::InstallFailed(
                "pack mapping contains an empty key".to_string(),
            ));
        }
        if !seen_item_keys.insert(item_speaker_key.to_string()) {
            return Err(EngineError::InstallFailed(format!(
                "duplicate mapping for item speaker {item_speaker_key}"
            )));
        }
        let role = roles_by_key.get(pack_role_key).ok_or_else(|| {
            EngineError::InstallFailed(format!("pack role not found: {pack_role_key}"))
        })?;
        template_mapping.push(voice_templates::VoiceTemplateApplyMapping {
            item_speaker_key: item_speaker_key.to_string(),
            template_speaker_key: role.template_speaker_key.clone(),
        });
    }

    let template_id = detail
        .roles
        .first()
        .map(|role| role.template_id.clone())
        .ok_or_else(|| EngineError::InstallFailed("cast pack has no roles".to_string()))?;
    let _ = voice_templates::apply_voice_template_to_item(
        paths,
        item_id,
        &template_id,
        &template_mapping,
    )?;
    let existing_by_key: HashMap<String, speakers::ItemSpeakerSetting> =
        speakers::list_item_speaker_settings(paths, item_id)?
            .into_iter()
            .map(|setting| (setting.speaker_key.clone(), setting))
            .collect();

    for mapping in mappings {
        let item_speaker_key = mapping.item_speaker_key.trim();
        let pack_role_key = mapping.pack_role_key.trim();
        let role = roles_by_key.get(pack_role_key).ok_or_else(|| {
            EngineError::InstallFailed(format!("pack role not found: {pack_role_key}"))
        })?;
        let existing = existing_by_key.get(item_speaker_key);
        speakers::upsert_item_speaker_setting(
            paths,
            item_id,
            item_speaker_key,
            role.display_name
                .clone()
                .or_else(|| existing.and_then(|value| value.display_name.clone())),
            existing.and_then(|value| value.voice_profile_id.clone()),
            existing.and_then(|value| value.tts_voice_id.clone()),
            existing.and_then(|value| value.tts_voice_profile_path.clone()),
            Some(
                existing
                    .map(|value| value.tts_voice_profile_paths.clone())
                    .unwrap_or_default(),
            ),
            role.style_preset
                .clone()
                .or_else(|| existing.and_then(|value| value.style_preset.clone())),
            role.prosody_preset
                .clone()
                .or_else(|| existing.and_then(|value| value.prosody_preset.clone())),
            role.pronunciation_overrides
                .clone()
                .or_else(|| existing.and_then(|value| value.pronunciation_overrides.clone())),
            role.render_mode
                .clone()
                .or_else(|| existing.and_then(|value| value.render_mode.clone())),
            role.subtitle_prosody_mode
                .clone()
                .or_else(|| existing.and_then(|value| value.subtitle_prosody_mode.clone())),
        )?;
    }

    speakers::list_item_speaker_settings(paths, item_id)
}

fn unique_role_key(used: &mut HashSet<String>, raw: &str) -> String {
    let base = sanitize_role_key(raw);
    if used.insert(base.clone()) {
        return base;
    }
    let mut suffix = 2_u32;
    loop {
        let candidate = format!("{base}_{suffix}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
        suffix += 1;
    }
}

fn sanitize_role_key(raw: &str) -> String {
    let mut out = String::new();
    let mut prev_underscore = false;
    for ch in raw.trim().chars() {
        let mapped = if ch.is_ascii_alphanumeric() {
            ch.to_ascii_lowercase()
        } else {
            '_'
        };
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
    let out = out.trim_matches('_');
    if out.is_empty() {
        "role".to_string()
    } else {
        out.to_string()
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
    use crate::db;
    use rusqlite::params;
    use std::path::Path;
    use tempfile::tempdir;

    fn insert_test_item(paths: &AppPaths, item_id: &str, media_path: &Path, title: &str) {
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
    fn create_voice_cast_pack_uses_template_speakers_as_roles() {
        let tmp = tempdir().expect("tempdir");
        let paths = AppPaths::new(tmp.path().to_path_buf());
        let media_path = tmp.path().join("episode.mp4");
        let reference_path = tmp.path().join("refs").join("host.wav");
        std::fs::write(&media_path, b"fake-media").expect("write media");
        std::fs::create_dir_all(reference_path.parent().expect("parent")).expect("mkdir refs");
        std::fs::write(&reference_path, b"fake-wav").expect("write ref");
        insert_test_item(&paths, "item-1", &media_path, "Episode 1");

        speakers::upsert_item_speaker_setting(
            &paths,
            "item-1",
            "S1",
            Some("Host".to_string()),
            None,
            Some("af_heart".to_string()),
            Some(reference_path.to_string_lossy().to_string()),
            Some(vec![reference_path.to_string_lossy().to_string()]),
            Some("documentary_narrator".to_string()),
            Some("natural".to_string()),
            Some("Seoul=>Soul".to_string()),
            Some("clone".to_string()),
            None,
        )
        .expect("upsert speaker");

        let template =
            voice_templates::create_voice_template_from_item(&paths, "item-1", "Episode host")
                .expect("template");
        let pack =
            create_voice_cast_pack_from_template(&paths, &template.template.id, "Game show pack")
                .expect("cast pack");

        assert_eq!(pack.pack.name, "Game show pack");
        assert_eq!(pack.roles.len(), 1);
        assert_eq!(pack.roles[0].display_name.as_deref(), Some("Host"));
        assert_eq!(
            pack.roles[0].style_preset.as_deref(),
            Some("documentary_narrator")
        );
        assert_eq!(pack.roles[0].render_mode.as_deref(), Some("clone"));
    }

    #[test]
    fn apply_voice_cast_pack_maps_roles_to_target_item() {
        let tmp = tempdir().expect("tempdir");
        let paths = AppPaths::new(tmp.path().to_path_buf());
        let template_media = tmp.path().join("template.mp4");
        let target_media = tmp.path().join("target.mp4");
        let reference_path = tmp.path().join("refs").join("panel.wav");
        std::fs::write(&template_media, b"fake-media").expect("write template media");
        std::fs::write(&target_media, b"fake-media").expect("write target media");
        std::fs::create_dir_all(reference_path.parent().expect("parent")).expect("mkdir refs");
        std::fs::write(&reference_path, b"fake-wav").expect("write ref");
        insert_test_item(&paths, "template-item", &template_media, "Template");
        insert_test_item(&paths, "target-item", &target_media, "Target");

        speakers::upsert_item_speaker_setting(
            &paths,
            "template-item",
            "S1",
            Some("Panel Host".to_string()),
            None,
            Some("af_heart".to_string()),
            Some(reference_path.to_string_lossy().to_string()),
            Some(vec![reference_path.to_string_lossy().to_string()]),
            Some("game_show_energy".to_string()),
            Some("more_excited".to_string()),
            Some("Miyeon=>Mee-yeon".to_string()),
            Some("standard_tts".to_string()),
            None,
        )
        .expect("template speaker");
        let template =
            voice_templates::create_voice_template_from_item(&paths, "template-item", "Panel")
                .expect("template");
        let pack =
            create_voice_cast_pack_from_template(&paths, &template.template.id, "Panel pack")
                .expect("cast pack");

        speakers::upsert_item_speaker_setting(
            &paths,
            "target-item",
            "S9",
            Some("Old name".to_string()),
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
        .expect("target speaker");

        let applied = apply_voice_cast_pack_to_item(
            &paths,
            "target-item",
            &pack.pack.id,
            &[VoiceCastPackApplyMapping {
                item_speaker_key: "S9".to_string(),
                pack_role_key: pack.roles[0].role_key.clone(),
            }],
        )
        .expect("apply cast pack");

        let mapped = applied
            .iter()
            .find(|setting| setting.speaker_key == "S9")
            .expect("mapped speaker");
        assert_eq!(mapped.display_name.as_deref(), Some("Panel Host"));
        assert_eq!(mapped.style_preset.as_deref(), Some("game_show_energy"));
        assert_eq!(mapped.prosody_preset.as_deref(), Some("more_excited"));
        assert_eq!(
            mapped.pronunciation_overrides.as_deref(),
            Some("Miyeon=>Mee-yeon")
        );
        assert_eq!(mapped.render_mode.as_deref(), Some("standard_tts"));
        assert_eq!(mapped.tts_voice_profile_paths.len(), 1);
        assert!(mapped
            .tts_voice_profile_paths
            .first()
            .map(|path| Path::new(path).exists())
            .unwrap_or(false));
    }

    #[test]
    fn update_voice_cast_pack_renames_existing_pack() {
        let tmp = tempdir().expect("tempdir");
        let paths = AppPaths::new(tmp.path().to_path_buf());
        let media_path = tmp.path().join("episode.mp4");
        let reference_path = tmp.path().join("refs").join("host.wav");
        std::fs::write(&media_path, b"fake-media").expect("write media");
        std::fs::create_dir_all(reference_path.parent().expect("parent")).expect("mkdir refs");
        std::fs::write(&reference_path, b"fake-wav").expect("write ref");
        insert_test_item(&paths, "item-1", &media_path, "Episode 1");

        speakers::upsert_item_speaker_setting(
            &paths,
            "item-1",
            "S1",
            Some("Host".to_string()),
            None,
            Some("af_heart".to_string()),
            Some(reference_path.to_string_lossy().to_string()),
            Some(vec![reference_path.to_string_lossy().to_string()]),
            None,
            None,
            None,
            Some("clone".to_string()),
            None,
        )
        .expect("upsert speaker");
        let template =
            voice_templates::create_voice_template_from_item(&paths, "item-1", "Episode host")
                .expect("template");
        let pack =
            create_voice_cast_pack_from_template(&paths, &template.template.id, "Original pack")
                .expect("cast pack");

        let renamed =
            update_voice_cast_pack(&paths, &pack.pack.id, "Renamed pack").expect("rename pack");
        assert_eq!(renamed.pack.name, "Renamed pack");
    }
}
