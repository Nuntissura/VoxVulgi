use crate::paths::AppPaths;
use crate::{db, EngineError, Result};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemSpeakerSetting {
    pub item_id: String,
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
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

pub fn list_item_speaker_settings(
    paths: &AppPaths,
    item_id: &str,
) -> Result<Vec<ItemSpeakerSetting>> {
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
  tts_voice_profile_paths_json,
  style_preset,
  prosody_preset,
  pronunciation_overrides,
  render_mode,
  created_at_ms,
  updated_at_ms
FROM item_speaker
WHERE item_id=?1
ORDER BY speaker_key ASC
"#,
    )?;

    let rows = stmt
        .query_map(params![item_id], |row| {
            let single_profile_path: Option<String> = row.get(4)?;
            Ok(ItemSpeakerSetting {
                item_id: row.get(0)?,
                speaker_key: row.get(1)?,
                display_name: row.get(2)?,
                tts_voice_id: row.get(3)?,
                tts_voice_profile_path: single_profile_path.clone(),
                tts_voice_profile_paths: decode_profile_paths(row.get::<_, Option<String>>(5)?, single_profile_path),
                style_preset: row.get(6)?,
                prosody_preset: row.get(7)?,
                pronunciation_overrides: row.get(8)?,
                render_mode: row.get(9)?,
                created_at_ms: row.get(10)?,
                updated_at_ms: row.get(11)?,
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
    tts_voice_profile_paths: Option<Vec<String>>,
    style_preset: Option<String>,
    prosody_preset: Option<String>,
    pronunciation_overrides: Option<String>,
    render_mode: Option<String>,
) -> Result<ItemSpeakerSetting> {
    let item_id = item_id.trim();
    if item_id.is_empty() {
        return Err(EngineError::InstallFailed("item_id is empty".to_string()));
    }
    let speaker_key = speaker_key.trim();
    if speaker_key.is_empty() {
        return Err(EngineError::InstallFailed(
            "speaker_key is empty".to_string(),
        ));
    }

    let display_name = display_name.and_then(|v| {
        let t = v.trim().to_string();
        if t.is_empty() {
            None
        } else {
            Some(t)
        }
    });
    let tts_voice_id = tts_voice_id.and_then(|v| {
        let t = v.trim().to_string();
        if t.is_empty() {
            None
        } else {
            Some(t)
        }
    });
    let tts_voice_profile_path = tts_voice_profile_path.and_then(|v| {
        let t = v.trim().to_string();
        if t.is_empty() {
            None
        } else {
            Some(t)
        }
    });
    let tts_voice_profile_paths = normalize_profile_paths(tts_voice_profile_path.clone(), tts_voice_profile_paths);
    let style_preset = normalize_optional_string(style_preset);
    let prosody_preset = normalize_optional_string(prosody_preset);
    let pronunciation_overrides = normalize_optional_string(pronunciation_overrides);
    let render_mode = normalize_optional_string(render_mode);
    let primary_profile_path = tts_voice_profile_paths
        .first()
        .cloned()
        .or(tts_voice_profile_path);
    let tts_voice_profile_paths_json = encode_profile_paths(&tts_voice_profile_paths);

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
  tts_voice_profile_paths_json,
  style_preset,
  prosody_preset,
  pronunciation_overrides,
  render_mode,
  created_at_ms,
  updated_at_ms
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
ON CONFLICT(item_id, speaker_key) DO UPDATE SET
  display_name=excluded.display_name,
  tts_voice_id=excluded.tts_voice_id,
  tts_voice_profile_path=excluded.tts_voice_profile_path,
  tts_voice_profile_paths_json=excluded.tts_voice_profile_paths_json,
  style_preset=excluded.style_preset,
  prosody_preset=excluded.prosody_preset,
  pronunciation_overrides=excluded.pronunciation_overrides,
  render_mode=excluded.render_mode,
  updated_at_ms=excluded.updated_at_ms
"#,
        params![
            item_id,
            speaker_key,
            display_name,
            tts_voice_id,
            primary_profile_path,
            tts_voice_profile_paths_json,
            style_preset,
            prosody_preset,
            pronunciation_overrides,
            render_mode,
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
  tts_voice_profile_paths_json,
  style_preset,
  prosody_preset,
  pronunciation_overrides,
  render_mode,
  created_at_ms,
  updated_at_ms
FROM item_speaker
WHERE item_id=?1 AND speaker_key=?2
"#,
        params![item_id, speaker_key],
        |row| {
            let single_profile_path: Option<String> = row.get(4)?;
            Ok(ItemSpeakerSetting {
                item_id: row.get(0)?,
                speaker_key: row.get(1)?,
                display_name: row.get(2)?,
                tts_voice_id: row.get(3)?,
                tts_voice_profile_path: single_profile_path.clone(),
                tts_voice_profile_paths: decode_profile_paths(row.get::<_, Option<String>>(5)?, single_profile_path),
                style_preset: row.get(6)?,
                prosody_preset: row.get(7)?,
                pronunciation_overrides: row.get(8)?,
                render_mode: row.get(9)?,
                created_at_ms: row.get(10)?,
                updated_at_ms: row.get(11)?,
            })
        },
    )
    .map_err(|e| EngineError::Database(e))
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|v| {
        let t = v.trim().to_string();
        if t.is_empty() {
            None
        } else {
            Some(t)
        }
    })
}

fn normalize_profile_paths(
    single_profile_path: Option<String>,
    profile_paths: Option<Vec<String>>,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(profile_paths) = profile_paths {
        for value in profile_paths {
            let trimmed = value.trim();
            if trimmed.is_empty() || out.iter().any(|existing| existing == trimmed) {
                continue;
            }
            out.push(trimmed.to_string());
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
                let trimmed = value.trim();
                if trimmed.is_empty() || out.iter().any(|existing| existing == trimmed) {
                    continue;
                }
                out.push(trimmed.to_string());
            }
            if !out.is_empty() {
                return out;
            }
        }
    }

    single_profile_path
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|value| vec![value])
        .unwrap_or_default()
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
