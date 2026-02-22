use crate::models::ModelStore;
use crate::paths::AppPaths;
use crate::subtitles::{SubtitleDocument, SubtitleSegment, SUBTITLE_JSON_SCHEMA_VERSION};
use crate::{EngineError, Result};
use hound::SampleFormat;
use serde::Deserialize;
use std::path::Path;
use std::{ffi::CStr, ffi::CString, os::raw::c_char};

pub fn transcribe_whisper_wav_16k_mono(
    paths: &AppPaths,
    model_id: &str,
    wav_path: &Path,
    lang: Option<&str>,
) -> Result<SubtitleDocument> {
    let model_path = resolve_whisper_model_path(paths, model_id)?;
    let audio = load_wav_16k_mono_f32(wav_path)?;

    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2)
        .clamp(1, 8) as i32;

    let model_path_c = CString::new(model_path.to_string_lossy().as_bytes())
        .map_err(|_| EngineError::InstallFailed("model path contains NUL byte".to_string()))?;
    let language_c = lang
        .map(|v| CString::new(v.as_bytes()))
        .transpose()
        .map_err(|_| EngineError::InstallFailed("language contains NUL byte".to_string()))?;

    let out_ptr = unsafe {
        ytf_whisper_transcribe_json(
            model_path_c.as_ptr(),
            audio.as_ptr(),
            audio.len() as i32,
            language_c
                .as_ref()
                .map(|s| s.as_ptr())
                .unwrap_or(std::ptr::null()),
            threads,
            false,
        )
    };

    if out_ptr.is_null() {
        let msg = unsafe {
            let p = ytf_whisper_last_error();
            if p.is_null() {
                "whisper failed".to_string()
            } else {
                CStr::from_ptr(p).to_string_lossy().to_string()
            }
        };
        return Err(EngineError::InstallFailed(msg));
    }

    let json = unsafe { CStr::from_ptr(out_ptr) }
        .to_string_lossy()
        .to_string();
    unsafe { ytf_whisper_free_string(out_ptr) };

    let parsed: WhisperJson = serde_json::from_str(&json)?;

    let mut segments = Vec::new();
    for seg in parsed.segments {
        let text = seg.text.trim().to_string();
        if text.is_empty() {
            continue;
        }
        let mut start_ms = seg.start_ms;
        let mut end_ms = seg.end_ms;
        if start_ms < 0 {
            start_ms = 0;
        }
        if end_ms < start_ms {
            end_ms = start_ms;
        }
        segments.push(SubtitleSegment {
            index: segments.len() as u32,
            start_ms,
            end_ms,
            text,
            speaker: None,
        });
    }

    let lang_out = if let Some(v) = parsed.lang {
        let v = v.trim().to_string();
        if v.is_empty() {
            None
        } else {
            Some(v)
        }
    } else {
        None
    };

    Ok(SubtitleDocument {
        schema_version: SUBTITLE_JSON_SCHEMA_VERSION,
        kind: "source".to_string(),
        lang: lang_out
            .or_else(|| lang.map(|s| s.to_string()))
            .unwrap_or_else(|| "und".to_string()),
        segments,
    })
}

pub fn translate_whisper_wav_16k_mono_to_en(
    paths: &AppPaths,
    model_id: &str,
    wav_path: &Path,
    lang: Option<&str>,
) -> Result<SubtitleDocument> {
    let model_path = resolve_whisper_model_path(paths, model_id)?;
    let audio = load_wav_16k_mono_f32(wav_path)?;

    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2)
        .clamp(1, 8) as i32;

    let model_path_c = CString::new(model_path.to_string_lossy().as_bytes())
        .map_err(|_| EngineError::InstallFailed("model path contains NUL byte".to_string()))?;
    let language_c = lang
        .map(|v| CString::new(v.as_bytes()))
        .transpose()
        .map_err(|_| EngineError::InstallFailed("language contains NUL byte".to_string()))?;

    let out_ptr = unsafe {
        ytf_whisper_transcribe_json(
            model_path_c.as_ptr(),
            audio.as_ptr(),
            audio.len() as i32,
            language_c
                .as_ref()
                .map(|s| s.as_ptr())
                .unwrap_or(std::ptr::null()),
            threads,
            true,
        )
    };

    if out_ptr.is_null() {
        let msg = unsafe {
            let p = ytf_whisper_last_error();
            if p.is_null() {
                "whisper failed".to_string()
            } else {
                CStr::from_ptr(p).to_string_lossy().to_string()
            }
        };
        return Err(EngineError::InstallFailed(msg));
    }

    let json = unsafe { CStr::from_ptr(out_ptr) }
        .to_string_lossy()
        .to_string();
    unsafe { ytf_whisper_free_string(out_ptr) };

    let parsed: WhisperJson = serde_json::from_str(&json)?;

    let mut segments = Vec::new();
    for seg in parsed.segments {
        let text = seg.text.trim().to_string();
        if text.is_empty() {
            continue;
        }
        let mut start_ms = seg.start_ms;
        let mut end_ms = seg.end_ms;
        if start_ms < 0 {
            start_ms = 0;
        }
        if end_ms < start_ms {
            end_ms = start_ms;
        }
        segments.push(SubtitleSegment {
            index: segments.len() as u32,
            start_ms,
            end_ms,
            text,
            speaker: None,
        });
    }

    Ok(SubtitleDocument {
        schema_version: SUBTITLE_JSON_SCHEMA_VERSION,
        kind: "translated".to_string(),
        lang: "en".to_string(),
        segments,
    })
}

#[derive(Debug, Deserialize)]
struct WhisperJson {
    lang: Option<String>,
    segments: Vec<WhisperJsonSegment>,
}

#[derive(Debug, Deserialize)]
struct WhisperJsonSegment {
    start_ms: i64,
    end_ms: i64,
    text: String,
}

fn resolve_whisper_model_path(paths: &AppPaths, model_id: &str) -> Result<std::path::PathBuf> {
    let store = ModelStore::new(paths.clone());
    store.verify_model_by_id(model_id)?;
    let spec = store.model_spec_by_id(model_id)?;
    let file = spec
        .files
        .first()
        .ok_or_else(|| EngineError::InstallFailed(format!("model has no files: {model_id}")))?;
    Ok(paths
        .model_install_dir(model_id, &spec.version)
        .join(&file.path))
}

fn load_wav_16k_mono_f32(path: &Path) -> Result<Vec<f32>> {
    let mut reader =
        hound::WavReader::open(path).map_err(|e| EngineError::InstallFailed(e.to_string()))?;
    let spec = reader.spec();
    if spec.channels != 1 || spec.sample_rate != 16_000 {
        return Err(EngineError::InstallFailed(format!(
            "unexpected wav format (need 16kHz mono): channels={}, sample_rate={}",
            spec.channels, spec.sample_rate
        )));
    }

    let mut samples = Vec::new();
    match spec.sample_format {
        SampleFormat::Int => {
            if spec.bits_per_sample == 16 {
                for s in reader.samples::<i16>() {
                    let v = s.map_err(|e| EngineError::InstallFailed(e.to_string()))?;
                    samples.push((v as f32) / (i16::MAX as f32));
                }
            } else if spec.bits_per_sample == 32 {
                for s in reader.samples::<i32>() {
                    let v = s.map_err(|e| EngineError::InstallFailed(e.to_string()))?;
                    samples.push((v as f32) / (i32::MAX as f32));
                }
            } else {
                return Err(EngineError::InstallFailed(format!(
                    "unsupported wav int bits_per_sample: {}",
                    spec.bits_per_sample
                )));
            }
        }
        SampleFormat::Float => {
            for s in reader.samples::<f32>() {
                let v = s.map_err(|e| EngineError::InstallFailed(e.to_string()))?;
                samples.push(v);
            }
        }
    }

    Ok(samples)
}

extern "C" {
    fn ytf_whisper_transcribe_json(
        model_path: *const c_char,
        samples: *const f32,
        n_samples: i32,
        language: *const c_char,
        n_threads: i32,
        translate: bool,
    ) -> *mut c_char;

    fn ytf_whisper_free_string(ptr: *mut c_char);

    fn ytf_whisper_last_error() -> *const c_char;
}
