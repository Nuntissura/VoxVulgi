use crate::cmd;
use crate::paths::AppPaths;
use crate::{EngineError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

const FFMPEG_COMMAND_TIMEOUT_SECS: u64 = 1800;

pub(crate) fn run_output(
    command: &mut std::process::Command,
    tool: &str,
) -> Result<std::process::Output> {
    cmd::run_owned_output(
        command,
        Duration::from_secs(FFMPEG_COMMAND_TIMEOUT_SECS),
        crate::jobs::external_command_cancel_requested,
    )
    .map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => EngineError::ExternalToolMissing {
            tool: tool.to_string(),
        },
        std::io::ErrorKind::Interrupted => EngineError::ExternalToolFailed {
            tool: tool.to_string(),
            code: None,
            stderr: format!("{tool} canceled"),
        },
        std::io::ErrorKind::TimedOut => EngineError::ExternalToolFailed {
            tool: tool.to_string(),
            code: None,
            stderr: format!("{tool} timed out after {FFMPEG_COMMAND_TIMEOUT_SECS}s"),
        },
        _ => EngineError::Io(error),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubtitleStreamProbe {
    pub index: Option<usize>,
    pub codec_name: Option<String>,
    pub language: Option<String>,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AudioStreamProbe {
    pub language: Option<String>,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaProbe {
    pub duration_ms: Option<i64>,
    pub container: Option<String>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub video_stream_count: usize,
    pub audio_stream_count: usize,
    pub audio_streams: Vec<AudioStreamProbe>,
    pub subtitle_streams: Vec<SubtitleStreamProbe>,
}

pub fn probe(paths: &AppPaths, input: &Path) -> Result<MediaProbe> {
    let mut command = cmd::command(paths.ffprobe_cmd());
    command
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(input);
    let output = run_output(&mut command, "ffprobe")?;

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

    let streams = parsed.streams.as_deref().unwrap_or_default();
    let (video_codec, width, height) = streams
        .iter()
        .find(|st| st.codec_type.as_deref() == Some("video"))
        .map(|st| (st.codec_name.clone(), st.width, st.height))
        .unwrap_or((None, None, None));

    let audio_codec = streams
        .iter()
        .find(|st| st.codec_type.as_deref() == Some("audio"))
        .and_then(|st| st.codec_name.clone());
    let video_stream_count = streams
        .iter()
        .filter(|st| st.codec_type.as_deref() == Some("video"))
        .count();
    let audio_stream_count = streams
        .iter()
        .filter(|st| st.codec_type.as_deref() == Some("audio"))
        .count();
    let audio_streams = streams
        .iter()
        .filter(|st| st.codec_type.as_deref() == Some("audio"))
        .map(|st| AudioStreamProbe {
            language: st
                .tags
                .as_ref()
                .and_then(|tags| case_insensitive_tag(tags, "language")),
            title: st
                .tags
                .as_ref()
                .and_then(|tags| case_insensitive_tag(tags, "title")),
        })
        .collect();
    let subtitle_streams = streams
        .iter()
        .filter(|st| st.codec_type.as_deref() == Some("subtitle"))
        .map(|st| SubtitleStreamProbe {
            index: st.index,
            codec_name: st.codec_name.clone(),
            language: st
                .tags
                .as_ref()
                .and_then(|tags| case_insensitive_tag(tags, "language")),
            title: st
                .tags
                .as_ref()
                .and_then(|tags| case_insensitive_tag(tags, "title")),
        })
        .collect();

    Ok(MediaProbe {
        duration_ms,
        container,
        video_codec,
        audio_codec,
        width,
        height,
        video_stream_count,
        audio_stream_count,
        audio_streams,
        subtitle_streams,
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

    let mut command = cmd::command(paths.ffmpeg_cmd());
    command
        .args(["-nostdin", "-y"])
        .args(["-ss", &format!("{ts:.3}")])
        .arg("-i")
        .arg(input)
        .args(["-frames:v", "1"])
        .args(["-vf", "scale='min(480,iw)':-2"])
        .args(["-q:v", "3"])
        .arg(output_image);
    let output = run_output(&mut command, "ffmpeg")?;

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

    let mut command = cmd::command(paths.ffmpeg_cmd());
    command
        .args(["-nostdin", "-y"])
        .arg("-i")
        .arg(input)
        .args(["-vn", "-ac", "1", "-ar", "16000"])
        .args(["-c:a", "pcm_s16le"])
        .arg(output_wav);
    let output = run_output(&mut command, "ffmpeg")?;

    if !output.status.success() {
        return Err(EngineError::ExternalToolFailed {
            tool: "ffmpeg".to_string(),
            code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }

    Ok(())
}

pub fn extract_audio_clip_wav_16k_mono(
    paths: &AppPaths,
    input: &Path,
    output_wav: &Path,
    start_ms: i64,
    end_ms: i64,
) -> Result<()> {
    if let Some(parent) = output_wav.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let start_seconds = (start_ms.max(0) as f64) / 1000.0;
    let duration_ms = (end_ms - start_ms).max(1);
    let duration_seconds = (duration_ms as f64) / 1000.0;

    let mut command = cmd::command(paths.ffmpeg_cmd());
    command
        .args(["-nostdin", "-y"])
        .args(["-ss", &format!("{start_seconds:.3}")])
        .arg("-i")
        .arg(input)
        .args(["-t", &format!("{duration_seconds:.3}")])
        .args(["-vn", "-ac", "1", "-ar", "16000"])
        .args(["-c:a", "pcm_s16le"])
        .arg(output_wav);
    let output = run_output(&mut command, "ffmpeg")?;

    if !output.status.success() {
        return Err(EngineError::ExternalToolFailed {
            tool: "ffmpeg".to_string(),
            code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }

    Ok(())
}

pub fn concat_wav_files_16k_mono(
    paths: &AppPaths,
    inputs: &[PathBuf],
    output_wav: &Path,
) -> Result<()> {
    if inputs.is_empty() {
        return Err(EngineError::InstallFailed(
            "concat requested with no input wav files".to_string(),
        ));
    }
    if let Some(parent) = output_wav.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if inputs.len() == 1 {
        std::fs::copy(&inputs[0], output_wav)?;
        return Ok(());
    }

    let mut command = cmd::command(paths.ffmpeg_cmd());
    command.args(["-nostdin", "-y"]);
    for input in inputs {
        command.arg("-i").arg(input);
    }
    let concat_inputs = (0..inputs.len())
        .map(|index| format!("[{index}:a]"))
        .collect::<String>();
    let filter = format!("{concat_inputs}concat=n={}:v=0:a=1[out]", inputs.len());
    command
        .args(["-filter_complex", &filter])
        .args(["-map", "[out]"])
        .args(["-ac", "1", "-ar", "16000"])
        .args(["-c:a", "pcm_s16le"])
        .arg(output_wav);
    let output = run_output(&mut command, "ffmpeg")?;

    if !output.status.success() {
        return Err(EngineError::ExternalToolFailed {
            tool: "ffmpeg".to_string(),
            code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }

    Ok(())
}

pub fn extract_audio_wav_44k_stereo(
    paths: &AppPaths,
    input: &Path,
    output_wav: &Path,
) -> Result<()> {
    if let Some(parent) = output_wav.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut command = cmd::command(paths.ffmpeg_cmd());
    command
        .args(["-nostdin", "-y"])
        .arg("-i")
        .arg(input)
        .args(["-vn", "-ac", "2", "-ar", "44100"])
        .args(["-c:a", "pcm_s16le"])
        .arg(output_wav);
    let output = run_output(&mut command, "ffmpeg")?;

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
    index: Option<usize>,
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<i64>,
    height: Option<i64>,
    tags: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Deserialize)]
struct FfprobeFormat {
    format_name: Option<String>,
    duration: Option<String>,
}

fn first_format_name(value: &str) -> String {
    value.split(',').next().unwrap_or(value).trim().to_string()
}

fn case_insensitive_tag(tags: &HashMap<String, String>, name: &str) -> Option<String> {
    tags.iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn parse_seconds_to_ms(value: &str) -> Option<i64> {
    let seconds: f64 = value.parse().ok()?;
    if !seconds.is_finite() || seconds < 0.0 {
        return None;
    }
    Some((seconds * 1000.0).round() as i64)
}
