use crate::{EngineError, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const SUBTITLE_JSON_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtitleDocument {
    pub schema_version: u32,
    pub kind: String,
    pub lang: String,
    pub segments: Vec<SubtitleSegment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtitleSegment {
    pub index: u32,
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
    #[serde(default)]
    pub speaker: Option<String>,
}

pub fn write_artifacts(
    doc: &SubtitleDocument,
    json_path: &Path,
    srt_path: &Path,
    vtt_path: &Path,
) -> Result<()> {
    if let Some(parent) = json_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let json = serde_json::to_string_pretty(doc)?;
    std::fs::write(json_path, format!("{json}\n"))?;

    let srt = render_srt(doc)?;
    std::fs::write(srt_path, srt)?;

    let vtt = render_vtt(doc)?;
    std::fs::write(vtt_path, vtt)?;

    Ok(())
}

pub fn render_srt(doc: &SubtitleDocument) -> Result<String> {
    let mut out = String::new();
    for (idx, seg) in doc.segments.iter().enumerate() {
        let n = idx + 1;
        out.push_str(&format!("{n}\n"));
        out.push_str(&format!(
            "{} --> {}\n",
            format_srt_ts(seg.start_ms),
            format_srt_ts(seg.end_ms)
        ));
        out.push_str(&sanitize_text(&seg.text));
        out.push_str("\n\n");
    }
    Ok(out)
}

pub fn render_vtt(doc: &SubtitleDocument) -> Result<String> {
    let mut out = String::new();
    out.push_str("WEBVTT\n\n");
    for seg in &doc.segments {
        out.push_str(&format!(
            "{} --> {}\n",
            format_vtt_ts(seg.start_ms),
            format_vtt_ts(seg.end_ms)
        ));
        out.push_str(&sanitize_text(&seg.text));
        out.push_str("\n\n");
    }
    Ok(out)
}

fn sanitize_text(text: &str) -> String {
    text.replace('\r', "").trim().to_string()
}

fn format_srt_ts(ms: i64) -> String {
    let ms = ms.clamp(0, i64::MAX);
    let total_ms = ms as u64;
    let hours = total_ms / 3_600_000;
    let minutes = (total_ms / 60_000) % 60;
    let seconds = (total_ms / 1_000) % 60;
    let millis = total_ms % 1_000;
    format!("{hours:02}:{minutes:02}:{seconds:02},{millis:03}")
}

fn format_vtt_ts(ms: i64) -> String {
    let ms = ms.clamp(0, i64::MAX);
    let total_ms = ms as u64;
    let hours = total_ms / 3_600_000;
    let minutes = (total_ms / 60_000) % 60;
    let seconds = (total_ms / 1_000) % 60;
    let millis = total_ms % 1_000;
    format!("{hours:02}:{minutes:02}:{seconds:02}.{millis:03}")
}

pub fn validate_document(doc: &SubtitleDocument) -> Result<()> {
    if doc.schema_version != SUBTITLE_JSON_SCHEMA_VERSION {
        return Err(EngineError::InstallFailed(format!(
            "unsupported subtitle schema_version: {}",
            doc.schema_version
        )));
    }
    Ok(())
}
