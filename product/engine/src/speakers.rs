use crate::paths::AppPaths;
use crate::{db, EngineError, Result};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemSpeakerSetting {
    pub item_id: String,
    pub speaker_key: String,
    pub display_name: Option<String>,
    pub tts_voice_id: Option<String>,
    pub tts_voice_profile_path: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

pub fn list_item_speaker_settings(paths: &AppPaths, item_id: &str) -> Result<Vec<ItemSpeakerSetting>> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;

    let mut stmt = conn.prepare(
        r#"
SELECT
  item_id,
  speaker_key,
  display_name,
  tts_voice_id,
  tts_voice_profile_path,
  created_at_ms,
  updated_at_ms
FROM item_speaker
WHERE item_id=?1
ORDER BY speaker_key ASC
"#,
    )?;

    let rows = stmt
        .query_map(params![item_id], |row| {
            Ok(ItemSpeakerSetting {
                item_id: row.get(0)?,
                speaker_key: row.get(1)?,
                display_name: row.get(2)?,
                tts_voice_id: row.get(3)?,
                tts_voice_profile_path: row.get(4)?,
                created_at_ms: row.get(5)?,
                updated_at_ms: row.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(rows)
}

pub fn upsert_item_speaker_setting(
    paths: &AppPaths,
    item_id: &str,
    speaker_key: &str,
    display_name: Option<String>,
    tts_voice_id: Option<String>,
    tts_voice_profile_path: Option<String>,
) -> Result<ItemSpeakerSetting> {
    let item_id = item_id.trim();
    if item_id.is_empty() {
        return Err(EngineError::InstallFailed("item_id is empty".to_string()));
    }
    let speaker_key = speaker_key.trim();
    if speaker_key.is_empty() {
        return Err(EngineError::InstallFailed("speaker_key is empty".to_string()));
    }

    let display_name = display_name.and_then(|v| {
        let t = v.trim().to_string();
        if t.is_empty() { None } else { Some(t) }
    });
    let tts_voice_id = tts_voice_id.and_then(|v| {
        let t = v.trim().to_string();
        if t.is_empty() { None } else { Some(t) }
    });
    let tts_voice_profile_path = tts_voice_profile_path.and_then(|v| {
        let t = v.trim().to_string();
        if t.is_empty() { None } else { Some(t) }
    });

    let conn = db::open(paths)?;
    db::migrate(&conn)?;

    let now = now_ms();
    conn.execute(
        r#"
INSERT INTO item_speaker (
  item_id,
  speaker_key,
  display_name,
  tts_voice_id,
  tts_voice_profile_path,
  created_at_ms,
  updated_at_ms
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
ON CONFLICT(item_id, speaker_key) DO UPDATE SET
  display_name=excluded.display_name,
  tts_voice_id=excluded.tts_voice_id,
  tts_voice_profile_path=excluded.tts_voice_profile_path,
  updated_at_ms=excluded.updated_at_ms
"#,
        params![
            item_id,
            speaker_key,
            display_name,
            tts_voice_id,
            tts_voice_profile_path,
            now,
            now
        ],
    )?;

    conn.query_row(
        r#"
SELECT
  item_id,
  speaker_key,
  display_name,
  tts_voice_id,
  tts_voice_profile_path,
  created_at_ms,
  updated_at_ms
FROM item_speaker
WHERE item_id=?1 AND speaker_key=?2
"#,
        params![item_id, speaker_key],
        |row| {
            Ok(ItemSpeakerSetting {
                item_id: row.get(0)?,
                speaker_key: row.get(1)?,
                display_name: row.get(2)?,
                tts_voice_id: row.get(3)?,
                tts_voice_profile_path: row.get(4)?,
                created_at_ms: row.get(5)?,
                updated_at_ms: row.get(6)?,
            })
        },
    )
    .map_err(|e| EngineError::Database(e))
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
