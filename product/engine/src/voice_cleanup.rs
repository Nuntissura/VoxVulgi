use crate::cmd;
use crate::paths::AppPaths;
use crate::{EngineError, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceReferenceCleanupOptions {
    #[serde(default)]
    pub denoise: bool,
    #[serde(default)]
    pub de_reverb: bool,
    #[serde(default)]
    pub speech_focus: bool,
    #[serde(default)]
    pub loudness_normalize: bool,
}

impl Default for VoiceReferenceCleanupOptions {
    fn default() -> Self {
        Self {
            denoise: true,
            de_reverb: true,
            speech_focus: true,
            loudness_normalize: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceReferenceCleanupRecord {
    pub cleanup_id: String,
    pub item_id: String,
    pub speaker_key: String,
    pub source_path: String,
    pub cleaned_path: String,
    pub manifest_path: String,
    pub filter_chain: String,
    pub options: VoiceReferenceCleanupOptions,
    pub created_at_ms: i64,
}

pub fn list_item_speaker_cleanups(
    paths: &AppPaths,
    item_id: &str,
    speaker_key: &str,
) -> Result<Vec<VoiceReferenceCleanupRecord>> {
    let item_id = item_id.trim();
    let speaker_key = speaker_key.trim();
    if item_id.is_empty() || speaker_key.is_empty() {
        return Err(EngineError::InstallFailed(
            "item_id or speaker_key is empty".to_string(),
        ));
    }
    let dir = cleanup_root(paths, item_id, speaker_key);
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut out: Vec<VoiceReferenceCleanupRecord> = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let manifest_path = path.join("manifest.json");
        if !manifest_path.exists() {
            continue;
        }
        let bytes = std::fs::read(&manifest_path)?;
        let record: VoiceReferenceCleanupRecord = serde_json::from_slice(&bytes)?;
        out.push(record);
    }
    out.sort_by(|a, b| b.created_at_ms.cmp(&a.created_at_ms));
    Ok(out)
}

pub fn run_item_speaker_reference_cleanup(
    paths: &AppPaths,
    item_id: &str,
    speaker_key: &str,
    source_path: &str,
    options: VoiceReferenceCleanupOptions,
) -> Result<VoiceReferenceCleanupRecord> {
    let item_id = item_id.trim();
    let speaker_key = speaker_key.trim();
    let source_path = source_path.trim();
    if item_id.is_empty() || speaker_key.is_empty() || source_path.is_empty() {
        return Err(EngineError::InstallFailed(
            "item_id, speaker_key, or source_path is empty".to_string(),
        ));
    }
    let source_path = PathBuf::from(source_path);
    if !source_path.is_file() {
        return Err(EngineError::InstallFailed(format!(
            "reference file not found: {}",
            source_path.to_string_lossy()
        )));
    }

    let cleanup_id = Uuid::new_v4().to_string();
    let out_dir = cleanup_root(paths, item_id, speaker_key).join(&cleanup_id);
    std::fs::create_dir_all(&out_dir)?;
    let cleaned_path = out_dir.join("cleaned_ref.wav");
    let manifest_path = out_dir.join("manifest.json");
    let filter_chain = build_filter_chain(&options);

    let output = cmd::command(paths.ffmpeg_cmd())
        .args(["-nostdin", "-y"])
        .arg("-i")
        .arg(&source_path)
        .args(["-vn", "-ac", "1", "-ar", "16000"])
        .args(["-af", &filter_chain])
        .args(["-c:a", "pcm_s16le"])
        .arg(&cleaned_path)
        .output()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => EngineError::ExternalToolMissing {
                tool: "ffmpeg".to_string(),
            },
            _ => EngineError::Io(e),
        })?;
    if !output.status.success() {
        return Err(EngineError::ExternalToolFailed {
            tool: "ffmpeg".to_string(),
            code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }

    let record = VoiceReferenceCleanupRecord {
        cleanup_id,
        item_id: item_id.to_string(),
        speaker_key: speaker_key.to_string(),
        source_path: source_path.to_string_lossy().to_string(),
        cleaned_path: cleaned_path.to_string_lossy().to_string(),
        manifest_path: manifest_path.to_string_lossy().to_string(),
        filter_chain,
        options,
        created_at_ms: now_ms(),
    };
    std::fs::write(
        &manifest_path,
        format!("{}\n", serde_json::to_string_pretty(&record)?),
    )?;
    Ok(record)
}

fn cleanup_root(paths: &AppPaths, item_id: &str, speaker_key: &str) -> PathBuf {
    paths
        .derived_item_voice_dir(item_id)
        .join("cleanup")
        .join(sanitize_segment(speaker_key))
}

fn sanitize_segment(raw: &str) -> String {
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
        "speaker".to_string()
    } else {
        out.to_string()
    }
}

fn build_filter_chain(options: &VoiceReferenceCleanupOptions) -> String {
    let mut filters: Vec<String> = vec!["highpass=f=70".to_string(), "lowpass=f=12000".to_string()];
    if options.denoise {
        filters.push("afftdn=nf=-22".to_string());
    }
    if options.de_reverb {
        filters.push("agate=threshold=0.02:ratio=1.8:attack=20:release=250".to_string());
    }
    if options.speech_focus {
        filters.push("speechnorm=e=12:r=0.0001:l=1".to_string());
    }
    if options.loudness_normalize {
        filters.push("dynaudnorm=f=150:g=15".to_string());
    }
    filters.join(",")
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
