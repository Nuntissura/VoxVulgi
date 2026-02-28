use crate::cmd;
use crate::paths::AppPaths;
use crate::{EngineError, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaProbe {
    pub duration_ms: Option<i64>,
    pub container: Option<String>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
}

pub fn probe(paths: &AppPaths, input: &Path) -> Result<MediaProbe> {
    let output = cmd::command(paths.ffprobe_cmd())
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(input)
        .output()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => EngineError::ExternalToolMissing {
                tool: "ffprobe".to_string(),
            },
            _ => EngineError::Io(e),
        })?;

    if !output.status.success() {
        return Err(EngineError::ExternalToolFailed {
            tool: "ffprobe".to_string(),
            code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }

    let parsed: FfprobeOutput = serde_json::from_slice(&output.stdout)?;

    let container = parsed
        .format
        .as_ref()
        .and_then(|f| f.format_name.as_deref())
        .map(first_format_name);
    let duration_ms = parsed
        .format
        .as_ref()
        .and_then(|f| f.duration.as_deref())
        .and_then(parse_seconds_to_ms);

    let (video_codec, width, height) = parsed
        .streams
        .as_ref()
        .and_then(|s| {
            s.iter()
                .find(|st| st.codec_type.as_deref() == Some("video"))
        })
        .map(|st| (st.codec_name.clone(), st.width, st.height))
        .unwrap_or((None, None, None));

    let audio_codec = parsed
        .streams
        .as_ref()
        .and_then(|s| {
            s.iter()
                .find(|st| st.codec_type.as_deref() == Some("audio"))
        })
        .and_then(|st| st.codec_name.clone());

    Ok(MediaProbe {
        duration_ms,
        container,
        video_codec,
        audio_codec,
        width,
        height,
    })
}

pub fn generate_thumbnail(
    paths: &AppPaths,
    input: &Path,
    output_image: &Path,
    timestamp_seconds: f64,
) -> Result<()> {
    if let Some(parent) = output_image.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let ts = if timestamp_seconds.is_finite() && timestamp_seconds >= 0.0 {
        timestamp_seconds
    } else {
        0.0
    };

    let output = cmd::command(paths.ffmpeg_cmd())
        .args(["-nostdin", "-y"])
        .args(["-ss", &format!("{ts:.3}")])
        .arg("-i")
        .arg(input)
        .args(["-frames:v", "1"])
        .args(["-vf", "scale='min(480,iw)':-2"])
        .args(["-q:v", "3"])
        .arg(output_image)
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

    Ok(())
}

pub fn extract_audio_wav_16k_mono(paths: &AppPaths, input: &Path, output_wav: &Path) -> Result<()> {
    if let Some(parent) = output_wav.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let output = cmd::command(paths.ffmpeg_cmd())
        .args(["-nostdin", "-y"])
        .arg("-i")
        .arg(input)
        .args(["-vn", "-ac", "1", "-ar", "16000"])
        .args(["-c:a", "pcm_s16le"])
        .arg(output_wav)
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

    Ok(())
}

pub fn extract_audio_wav_44k_stereo(paths: &AppPaths, input: &Path, output_wav: &Path) -> Result<()> {
    if let Some(parent) = output_wav.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let output = cmd::command(paths.ffmpeg_cmd())
        .args(["-nostdin", "-y"])
        .arg("-i")
        .arg(input)
        .args(["-vn", "-ac", "2", "-ar", "44100"])
        .args(["-c:a", "pcm_s16le"])
        .arg(output_wav)
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

    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
struct FfprobeOutput {
    streams: Option<Vec<FfprobeStream>>,
    format: Option<FfprobeFormat>,
}

#[derive(Debug, Clone, Deserialize)]
struct FfprobeStream {
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<i64>,
    height: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
struct FfprobeFormat {
    format_name: Option<String>,
    duration: Option<String>,
}

fn first_format_name(value: &str) -> String {
    value.split(',').next().unwrap_or(value).trim().to_string()
}

fn parse_seconds_to_ms(value: &str) -> Option<i64> {
    let seconds: f64 = value.parse().ok()?;
    if !seconds.is_finite() || seconds < 0.0 {
        return None;
    }
    Some((seconds * 1000.0).round() as i64)
}
