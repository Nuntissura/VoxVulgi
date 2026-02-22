use crate::asr;
use crate::paths::AppPaths;
use crate::subtitles::{SubtitleDocument, SubtitleSegment, SUBTITLE_JSON_SCHEMA_VERSION};
use crate::{EngineError, Result};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct TranslateOptions {
    pub max_line_chars: usize,
    pub max_lines: usize,
    pub max_cps: f64,
}

impl Default for TranslateOptions {
    fn default() -> Self {
        Self {
            max_line_chars: 42,
            max_lines: 2,
            max_cps: 17.0,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TranslateQcWarning {
    pub segment_index: u32,
    pub code: String,
    pub message: String,
    pub actual: f64,
    pub limit: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TranslateReport {
    pub engine: String,
    pub model_id: String,
    pub source_lang: Option<String>,
    pub glossary_path: String,
    pub glossary_entries: usize,
    pub warnings: Vec<TranslateQcWarning>,
}

#[derive(Debug, Clone)]
pub struct TranslateResult {
    pub doc: SubtitleDocument,
    pub report: TranslateReport,
}

pub fn translate_doc_whisper_to_en(
    paths: &AppPaths,
    source_doc: &SubtitleDocument,
    wav_path: &Path,
    model_id: &str,
    options: TranslateOptions,
) -> Result<TranslateResult> {
    if source_doc.schema_version != SUBTITLE_JSON_SCHEMA_VERSION {
        return Err(EngineError::InstallFailed(format!(
            "unsupported subtitle schema_version: {}",
            source_doc.schema_version
        )));
    }

    let glossary_path = paths.glossary_path();
    ensure_default_glossary(&glossary_path)?;
    let glossary_map = load_glossary(&glossary_path)?;
    let glossary_entries_sorted = glossary_entries_sorted(&glossary_map);

    let source_lang = match source_doc.lang.as_str() {
        "ja" | "ko" => Some(source_doc.lang.clone()),
        _ => None,
    };

    // Run Whisper.cpp in translate mode (speech -> English).
    let translated_raw = asr::translate_whisper_wav_16k_mono_to_en(
        paths,
        model_id,
        wav_path,
        source_lang.as_deref(),
    )?;

    // Align Whisper segments onto the source segment windows to keep timing stable.
    let aligned_texts = align_translated_to_source(source_doc, &translated_raw);

    let mut out_segments: Vec<SubtitleSegment> = Vec::with_capacity(source_doc.segments.len());
    let mut warnings: Vec<TranslateQcWarning> = Vec::new();

    for (i, src) in source_doc.segments.iter().enumerate() {
        let mut text = aligned_texts.get(i).cloned().unwrap_or_default();
        text = apply_glossary(&text, &glossary_entries_sorted);
        let qc = qc_format_and_warn(i as u32, src.start_ms, src.end_ms, &text, &options);
        text = qc.text;
        warnings.extend(qc.warnings);

        out_segments.push(SubtitleSegment {
            index: i as u32,
            start_ms: src.start_ms,
            end_ms: src.end_ms,
            text,
            speaker: src.speaker.clone(),
        });
    }

    let doc = SubtitleDocument {
        schema_version: SUBTITLE_JSON_SCHEMA_VERSION,
        kind: "translated".to_string(),
        lang: "en".to_string(),
        segments: out_segments,
    };

    let report = TranslateReport {
        engine: "whispercpp_translate".to_string(),
        model_id: model_id.to_string(),
        source_lang,
        glossary_path: glossary_path.to_string_lossy().to_string(),
        glossary_entries: glossary_map.len(),
        warnings,
    };

    Ok(TranslateResult { doc, report })
}

fn ensure_default_glossary(path: &Path) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, "{\n}\n")?;
    Ok(())
}

fn load_glossary(path: &Path) -> Result<BTreeMap<String, String>> {
    let bytes = std::fs::read(path)?;
    let map: BTreeMap<String, String> = serde_json::from_slice(&bytes).map_err(|e| {
        EngineError::InstallFailed(format!(
            "failed to parse glossary json at {}: {e}",
            path.to_string_lossy()
        ))
    })?;
    Ok(map)
}

fn glossary_entries_sorted(map: &BTreeMap<String, String>) -> Vec<(String, String)> {
    let mut entries: Vec<(String, String)> =
        map.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    entries.sort_by(|(a, _), (b, _)| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    entries
}

fn apply_glossary(text: &str, entries: &[(String, String)]) -> String {
    let mut out = text.to_string();
    for (from, to) in entries {
        if from.is_empty() {
            continue;
        }
        out = out.replace(from, to);
    }
    out
}

fn align_translated_to_source(
    source: &SubtitleDocument,
    translated: &SubtitleDocument,
) -> Vec<String> {
    let n = source.segments.len();
    if n == 0 {
        return Vec::new();
    }

    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| {
        let sa = &source.segments[a];
        let sb = &source.segments[b];
        sa.start_ms
            .cmp(&sb.start_ms)
            .then_with(|| sa.end_ms.cmp(&sb.end_ms))
            .then_with(|| sa.index.cmp(&sb.index))
    });

    let mut buckets_sorted: Vec<Vec<String>> = vec![Vec::new(); n];
    let mut j = 0_usize;
    for seg in &translated.segments {
        let mid = (seg.start_ms + seg.end_ms) / 2;
        while j < n {
            let src = &source.segments[order[j]];
            if src.end_ms > mid {
                break;
            }
            j += 1;
        }
        if j >= n {
            break;
        }
        let src = &source.segments[order[j]];
        if mid >= src.start_ms && mid < src.end_ms {
            let t = seg.text.trim();
            if !t.is_empty() {
                buckets_sorted[j].push(t.to_string());
            }
        }
    }

    let mut out: Vec<String> = vec![String::new(); n];
    for sorted_idx in 0..n {
        let orig_idx = order[sorted_idx];
        let joined = buckets_sorted[sorted_idx].join(" ").trim().to_string();
        out[orig_idx] = joined;
    }
    out
}

struct QcResult {
    text: String,
    warnings: Vec<TranslateQcWarning>,
}

fn qc_format_and_warn(
    segment_index: u32,
    start_ms: i64,
    end_ms: i64,
    text: &str,
    options: &TranslateOptions,
) -> QcResult {
    let mut warnings = Vec::new();
    let cleaned = text.replace('\r', "").trim().to_string();
    if cleaned.is_empty() {
        warnings.push(TranslateQcWarning {
            segment_index,
            code: "missing_translation".to_string(),
            message: "No translated text produced for this segment.".to_string(),
            actual: 0.0,
            limit: 1.0,
        });
        return QcResult {
            text: String::new(),
            warnings,
        };
    }

    let wrapped = wrap_text_lines(&cleaned, options.max_line_chars);
    let line_lens: Vec<usize> = wrapped.split('\n').map(visible_len_chars).collect();
    if let Some(max_len) = line_lens.iter().copied().max() {
        if max_len > options.max_line_chars {
            warnings.push(TranslateQcWarning {
                segment_index,
                code: "line_length".to_string(),
                message: "Line exceeds max length after wrapping.".to_string(),
                actual: max_len as f64,
                limit: options.max_line_chars as f64,
            });
        }
    }
    let line_count = wrapped.split('\n').count();
    if line_count > options.max_lines {
        warnings.push(TranslateQcWarning {
            segment_index,
            code: "line_count".to_string(),
            message: "Subtitle uses more than the recommended number of lines.".to_string(),
            actual: line_count as f64,
            limit: options.max_lines as f64,
        });
    }

    let duration_ms = (end_ms - start_ms).max(0) as f64;
    let duration_s = duration_ms / 1000.0;
    if duration_s <= 0.0 {
        warnings.push(TranslateQcWarning {
            segment_index,
            code: "duration".to_string(),
            message: "Segment has non-positive duration.".to_string(),
            actual: duration_s,
            limit: 0.001,
        });
    } else {
        let cps = visible_len_chars(&wrapped.replace('\n', " ")) as f64 / duration_s;
        if cps > options.max_cps {
            warnings.push(TranslateQcWarning {
                segment_index,
                code: "cps".to_string(),
                message: "Chars-per-second exceeds target (may be hard to read).".to_string(),
                actual: cps,
                limit: options.max_cps,
            });
        }
    }

    QcResult {
        text: wrapped,
        warnings,
    }
}

fn wrap_text_lines(text: &str, max_line_chars: usize) -> String {
    let max_line_chars = max_line_chars.max(1);
    let cleaned = text
        .replace('\n', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if visible_len_chars(&cleaned) <= max_line_chars {
        return cleaned;
    }

    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();

    for word in cleaned.split(' ') {
        if word.is_empty() {
            continue;
        }

        if visible_len_chars(word) > max_line_chars {
            if !current.is_empty() {
                lines.push(current);
                current = String::new();
            }
            let mut chunk = String::new();
            for ch in word.chars() {
                chunk.push(ch);
                if visible_len_chars(&chunk) >= max_line_chars {
                    lines.push(chunk);
                    chunk = String::new();
                }
            }
            if !chunk.is_empty() {
                lines.push(chunk);
            }
            continue;
        }

        if current.is_empty() {
            current.push_str(word);
            continue;
        }

        let proposed_len = visible_len_chars(&current) + 1 + visible_len_chars(word);
        if proposed_len <= max_line_chars {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(current);
            current = word.to_string();
        }
    }

    if !current.is_empty() {
        lines.push(current);
    }

    lines.join("\n")
}

fn visible_len_chars(s: &str) -> usize {
    s.chars().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glossary_replacements_are_deterministic_longest_first() {
        let mut map = BTreeMap::new();
        map.insert("foo".to_string(), "X".to_string());
        map.insert("foobar".to_string(), "Y".to_string());
        let entries = glossary_entries_sorted(&map);
        assert_eq!(apply_glossary("foobar foo", &entries), "Y X");
    }

    #[test]
    fn wrap_text_lines_wraps_at_max_chars() {
        let text = "This is a somewhat long subtitle line that should wrap nicely.";
        let wrapped = wrap_text_lines(text, 20);
        for line in wrapped.split('\n') {
            assert!(visible_len_chars(line) <= 20);
        }
    }

    #[test]
    fn align_translated_to_source_assigns_midpoints() {
        let source = SubtitleDocument {
            schema_version: 1,
            kind: "source".to_string(),
            lang: "ja".to_string(),
            segments: vec![
                SubtitleSegment {
                    index: 0,
                    start_ms: 0,
                    end_ms: 1000,
                    text: "a".to_string(),
                    speaker: None,
                },
                SubtitleSegment {
                    index: 1,
                    start_ms: 1000,
                    end_ms: 2000,
                    text: "b".to_string(),
                    speaker: None,
                },
            ],
        };

        let translated = SubtitleDocument {
            schema_version: 1,
            kind: "translated".to_string(),
            lang: "en".to_string(),
            segments: vec![
                SubtitleSegment {
                    index: 0,
                    start_ms: 100,
                    end_ms: 900,
                    text: "A".to_string(),
                    speaker: None,
                },
                SubtitleSegment {
                    index: 1,
                    start_ms: 1100,
                    end_ms: 1900,
                    text: "B".to_string(),
                    speaker: None,
                },
            ],
        };

        let aligned = align_translated_to_source(&source, &translated);
        assert_eq!(aligned.len(), 2);
        assert_eq!(aligned[0], "A");
        assert_eq!(aligned[1], "B");
    }
}
