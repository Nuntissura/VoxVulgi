use crate::asr;
use crate::paths::AppPaths;
use crate::subtitles::{SubtitleDocument, SubtitleSegment, SUBTITLE_JSON_SCHEMA_VERSION};
use crate::{EngineError, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::OnceLock;

const GLOSSARY_SCHEMA_VERSION: u32 = 1;
const MAX_GLOSSARY_ENTRIES: usize = 5_000;
const MAX_GLOSSARY_TERM_BYTES: usize = 1_024;
const MAX_GLOSSARY_NOTE_BYTES: usize = 4_096;
const MAX_WHISPER_GLOSSARY_PROMPT_CHARS: usize = 480;
const TRANSLATION_STYLE_SCHEMA_VERSION: u32 = 1;
const MAX_CUSTOM_STYLE_INSTRUCTION_BYTES: usize = 1_024;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TranslationStyle {
    #[default]
    Neutral,
    Formal,
    Informal,
    Custom,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HonorificMode {
    #[default]
    Preserve,
    Translate,
    Drop,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TranslationStyleSettings {
    pub schema_version: u32,
    pub style: TranslationStyle,
    pub honorific_mode: HonorificMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_instruction: Option<String>,
}

impl Default for TranslationStyleSettings {
    fn default() -> Self {
        Self {
            schema_version: TRANSLATION_STYLE_SCHEMA_VERSION,
            style: TranslationStyle::Neutral,
            honorific_mode: HonorificMode::Preserve,
            custom_instruction: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GlossaryEntry {
    pub source: String,
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GlossaryDocument {
    pub schema_version: u32,
    pub entries: Vec<GlossaryEntry>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GlossaryBundle {
    pub global_entries: Vec<GlossaryEntry>,
    pub item_entries: Vec<GlossaryEntry>,
    pub effective_entries: Vec<GlossaryEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum GlossaryFile {
    Document(GlossaryDocument),
    Legacy(BTreeMap<String, String>),
}

#[derive(Debug, Clone)]
pub struct TranslateOptions {
    pub max_line_chars: usize,
    pub max_lines: usize,
    pub max_cps: f64,
    pub glossary_entries: Option<Vec<GlossaryEntry>>,
    pub translation_style: Option<TranslationStyleSettings>,
}

impl Default for TranslateOptions {
    fn default() -> Self {
        Self {
            max_line_chars: 42,
            max_lines: 2,
            max_cps: 17.0,
            glossary_entries: None,
            translation_style: None,
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
    pub glossary_prompt_entries: usize,
    pub translation_style: TranslationStyleSettings,
    pub source_segment_count: usize,
    pub translated_raw_segment_count: usize,
    pub translated_usable_segment_count: usize,
    pub aligned_usable_segment_count: usize,
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
    let glossary_entries = match options.glossary_entries.as_ref() {
        Some(entries) => normalize_entries(entries.clone())?,
        None => load_glossary_entries(&glossary_path)?,
    };
    let glossary_entries_sorted = glossary_entries_sorted(&glossary_entries);
    let glossary_prompt_entries = glossary_entries_for_source(&glossary_entries, source_doc);
    let translation_style =
        normalize_translation_style(options.translation_style.clone().unwrap_or_default())?;
    let (translation_prompt, glossary_prompt_entry_count) =
        build_translation_prompt(&translation_style, &glossary_prompt_entries);

    let source_lang = match source_doc.lang.as_str() {
        "ja" | "ko" => Some(source_doc.lang.clone()),
        _ => None,
    };

    // Run Whisper.cpp in translate mode (speech -> English).
    let translated_raw = asr::translate_whisper_wav_16k_mono_to_en_with_stats(
        paths,
        model_id,
        wav_path,
        source_lang.as_deref(),
        translation_prompt.as_deref(),
    )?;

    // Align Whisper segments onto the source segment windows to keep timing stable.
    let aligned_texts = align_translated_to_source(source_doc, &translated_raw.doc);

    let mut out_segments: Vec<SubtitleSegment> = Vec::with_capacity(source_doc.segments.len());
    let mut warnings: Vec<TranslateQcWarning> = Vec::new();

    for (i, src) in source_doc.segments.iter().enumerate() {
        let mut text = aligned_texts.get(i).cloned().unwrap_or_default();
        text = apply_glossary(&text, &glossary_entries_sorted);
        text = apply_translation_style(&text, &translation_style);
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
    let aligned_usable_segment_count = crate::subtitles::usable_segment_count(&doc);

    let report = TranslateReport {
        engine: "whispercpp_translate".to_string(),
        model_id: model_id.to_string(),
        source_lang,
        glossary_path: glossary_path.to_string_lossy().to_string(),
        glossary_entries: glossary_entries.len(),
        glossary_prompt_entries: glossary_prompt_entry_count,
        translation_style,
        source_segment_count: source_doc.segments.len(),
        translated_raw_segment_count: translated_raw.stats.raw_segment_count,
        translated_usable_segment_count: translated_raw.stats.usable_segment_count,
        aligned_usable_segment_count,
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
    let document = GlossaryDocument {
        schema_version: GLOSSARY_SCHEMA_VERSION,
        entries: Vec::new(),
    };
    crate::persistence::atomic_write_text(
        path,
        &format!("{}\n", serde_json::to_string_pretty(&document)?),
    )?;
    Ok(())
}

fn load_glossary_entries(path: &Path) -> Result<Vec<GlossaryEntry>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let bytes = std::fs::read(path)?;
    let file: GlossaryFile = serde_json::from_slice(&bytes).map_err(|e| {
        EngineError::InstallFailed(format!(
            "failed to parse glossary json at {}: {e}",
            path.to_string_lossy()
        ))
    })?;
    let entries = match file {
        GlossaryFile::Document(document) => {
            if document.schema_version != GLOSSARY_SCHEMA_VERSION {
                return Err(EngineError::InstallFailed(format!(
                    "unsupported glossary schema_version {} at {}",
                    document.schema_version,
                    path.to_string_lossy()
                )));
            }
            document.entries
        }
        GlossaryFile::Legacy(map) => map
            .into_iter()
            .map(|(source, target)| GlossaryEntry {
                source,
                target,
                context: None,
                notes: None,
            })
            .collect(),
    };
    normalize_entries(entries)
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn normalize_entries(entries: Vec<GlossaryEntry>) -> Result<Vec<GlossaryEntry>> {
    if entries.len() > MAX_GLOSSARY_ENTRIES {
        return Err(EngineError::InstallFailed(format!(
            "glossary contains {} entries; maximum is {MAX_GLOSSARY_ENTRIES}",
            entries.len()
        )));
    }
    let mut by_source = BTreeMap::new();
    for entry in entries {
        let source = entry.source.trim().to_string();
        let target = entry.target.trim().to_string();
        if source.is_empty() || target.is_empty() {
            return Err(EngineError::InstallFailed(
                "glossary source and target must not be empty".to_string(),
            ));
        }
        let context = normalize_optional(entry.context);
        let notes = normalize_optional(entry.notes);
        if source.chars().any(char::is_control)
            || target.chars().any(char::is_control)
            || context
                .as_deref()
                .is_some_and(|value| value.chars().any(char::is_control))
        {
            return Err(EngineError::InstallFailed(
                "glossary source, target, and context must not contain control characters"
                    .to_string(),
            ));
        }
        if source.len() > MAX_GLOSSARY_TERM_BYTES || target.len() > MAX_GLOSSARY_TERM_BYTES {
            return Err(EngineError::InstallFailed(format!(
                "glossary source and target must each be at most {MAX_GLOSSARY_TERM_BYTES} UTF-8 bytes"
            )));
        }
        if context.as_ref().map(String::len).unwrap_or(0) > MAX_GLOSSARY_NOTE_BYTES
            || notes.as_ref().map(String::len).unwrap_or(0) > MAX_GLOSSARY_NOTE_BYTES
        {
            return Err(EngineError::InstallFailed(format!(
                "glossary context and notes must each be at most {MAX_GLOSSARY_NOTE_BYTES} UTF-8 bytes"
            )));
        }
        by_source.insert(
            source.clone(),
            GlossaryEntry {
                source,
                target,
                context,
                notes,
            },
        );
    }
    Ok(by_source.into_values().collect())
}

fn glossary_entries_sorted(entries: &[GlossaryEntry]) -> Vec<(String, String)> {
    let mut entries: Vec<(String, String)> = entries
        .iter()
        .map(|entry| (entry.source.clone(), entry.target.clone()))
        .collect();
    entries.sort_by(|(a, _), (b, _)| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    entries
}

fn glossary_entries_for_source(
    entries: &[GlossaryEntry],
    source_doc: &SubtitleDocument,
) -> Vec<GlossaryEntry> {
    let source_text = source_doc
        .segments
        .iter()
        .map(|segment| segment.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let mut matched: Vec<(usize, usize, GlossaryEntry)> = entries
        .iter()
        .filter_map(|entry| {
            source_text
                .find(&entry.source)
                .map(|position| (position, usize::MAX - entry.source.len(), entry.clone()))
        })
        .collect();
    matched.sort_by_key(|(position, inverse_length, _)| (*position, *inverse_length));
    matched.into_iter().map(|(_, _, entry)| entry).collect()
}

fn translation_style_prompt(settings: &TranslationStyleSettings) -> String {
    let style = match settings.style {
        TranslationStyle::Neutral => {
            "Use clear, natural English subtitles with standard English punctuation.".to_string()
        }
        TranslationStyle::Formal => {
            "Use formal, professional English, complete sentences, standard punctuation, and avoid slang and contractions."
                .to_string()
        }
        TranslationStyle::Informal => {
            "Use casual conversational English, natural contractions, and light subtitle punctuation.".to_string()
        }
        TranslationStyle::Custom => settings
            .custom_instruction
            .as_deref()
            .filter(|value| !value.is_empty())
            .map(|value| format!("Follow this English translation style: {value}"))
            .unwrap_or_else(|| "Use clear, natural English subtitles.".to_string()),
    };
    let honorifics = match settings.honorific_mode {
        HonorificMode::Preserve => {
            "Preserve romanized Japanese and Korean honorific suffixes such as -san, -sama, -sensei, -senpai, -kun, -chan, and -nim."
        }
        HonorificMode::Translate => {
            "Translate Japanese and Korean honorific meaning into natural English titles when context supports it."
        }
        HonorificMode::Drop => {
            "Omit Japanese and Korean honorific suffixes such as -san, -sama, -sensei, -senpai, -kun, -chan, and -nim."
        }
    };
    // Put the honorific rule first so a long custom instruction cannot displace it when the
    // bounded Whisper prompt is truncated.
    format!("{honorifics} {style}")
}

fn build_translation_prompt(
    settings: &TranslationStyleSettings,
    glossary_entries: &[GlossaryEntry],
) -> (Option<String>, usize) {
    let mut prompt = translation_style_prompt(settings);
    if prompt.chars().count() > MAX_WHISPER_GLOSSARY_PROMPT_CHARS {
        prompt = prompt
            .chars()
            .take(MAX_WHISPER_GLOSSARY_PROMPT_CHARS)
            .collect();
        return (Some(prompt), 0);
    }

    let mut included = 0;
    if !glossary_entries.is_empty() {
        let heading = " Preferred exact terminology: ";
        if prompt.chars().count() + heading.chars().count() <= MAX_WHISPER_GLOSSARY_PROMPT_CHARS {
            prompt.push_str(heading);
            for entry in glossary_entries {
                let context = entry
                    .context
                    .as_deref()
                    .map(|value| format!(" ({value})"))
                    .unwrap_or_default();
                let candidate = format!("{} = {}{}; ", entry.source, entry.target, context);
                if prompt.chars().count() + candidate.chars().count()
                    > MAX_WHISPER_GLOSSARY_PROMPT_CHARS
                {
                    break;
                }
                prompt.push_str(&candidate);
                included += 1;
            }
        }
    }
    (Some(prompt.trim().to_string()), included)
}

fn honorific_suffix_regex() -> &'static Regex {
    static HONORIFIC_SUFFIX_RE: OnceLock<Regex> = OnceLock::new();
    HONORIFIC_SUFFIX_RE.get_or_init(|| {
        Regex::new(
            r"(?i)[\-\u{2010}\u{2011}\u{2012}\u{2013}\u{2014}](?:san|sama|sensei|senpai|kun|chan|nim)\b",
        )
        .expect("valid honorific suffix regex")
    })
}

fn apply_translation_style(text: &str, settings: &TranslationStyleSettings) -> String {
    let mut styled = text.trim().to_string();
    if settings.honorific_mode == HonorificMode::Drop {
        styled = honorific_suffix_regex()
            .replace_all(&styled, "")
            .to_string();
    }
    match settings.style {
        TranslationStyle::Formal => {
            if let Some(first) = styled.chars().next() {
                if first.is_ascii_lowercase() {
                    styled.replace_range(
                        0..first.len_utf8(),
                        &first.to_ascii_uppercase().to_string(),
                    );
                }
            }
            if !styled.is_empty() && !styled.ends_with(['.', '?', '!', '…']) {
                styled.push('.');
            }
        }
        TranslationStyle::Informal => {
            if styled.ends_with('.') && !styled.ends_with("...") {
                styled.pop();
            }
        }
        TranslationStyle::Neutral | TranslationStyle::Custom => {}
    }
    styled
}

fn normalize_translation_style(
    mut settings: TranslationStyleSettings,
) -> Result<TranslationStyleSettings> {
    if settings.schema_version != TRANSLATION_STYLE_SCHEMA_VERSION {
        return Err(EngineError::InstallFailed(format!(
            "unsupported translation style schema_version: {}",
            settings.schema_version
        )));
    }
    settings.custom_instruction = normalize_optional(settings.custom_instruction);
    if let Some(instruction) = settings.custom_instruction.as_deref() {
        if instruction.len() > MAX_CUSTOM_STYLE_INSTRUCTION_BYTES {
            return Err(EngineError::InstallFailed(format!(
                "custom translation style instruction must be at most {MAX_CUSTOM_STYLE_INSTRUCTION_BYTES} UTF-8 bytes"
            )));
        }
        if instruction.chars().any(char::is_control) {
            return Err(EngineError::InstallFailed(
                "custom translation style instruction must not contain control characters"
                    .to_string(),
            ));
        }
    }
    Ok(settings)
}

pub fn translation_style_load(
    paths: &AppPaths,
    item_id: &str,
) -> Result<Option<TranslationStyleSettings>> {
    let path = paths.item_translation_style_path(validate_item_id(item_id)?);
    if !path.exists() {
        return Ok(None);
    }
    let settings: TranslationStyleSettings = serde_json::from_slice(&std::fs::read(&path)?)
        .map_err(|error| {
            EngineError::InstallFailed(format!(
                "failed to parse translation style json at {}: {error}",
                path.to_string_lossy()
            ))
        })?;
    Ok(Some(normalize_translation_style(settings)?))
}

pub fn translation_style_save(
    paths: &AppPaths,
    item_id: &str,
    settings: TranslationStyleSettings,
) -> Result<TranslationStyleSettings> {
    let item_id = validate_item_id(item_id)?;
    let settings = normalize_translation_style(settings)?;
    let path = paths.item_translation_style_path(item_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    crate::persistence::atomic_write_text(
        &path,
        &format!("{}\n", serde_json::to_string_pretty(&settings)?),
    )?;
    Ok(settings)
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

// ---------------------------------------------------------------------------
// Public glossary API (WP-0177)
// ---------------------------------------------------------------------------

fn validate_item_id(item_id: &str) -> Result<&str> {
    let item_id = item_id.trim();
    if item_id.is_empty()
        || item_id == "."
        || item_id == ".."
        || item_id.contains('/')
        || item_id.contains('\\')
    {
        return Err(EngineError::InstallFailed(
            "invalid glossary item id".to_string(),
        ));
    }
    Ok(item_id)
}

fn glossary_path_for_scope(
    paths: &AppPaths,
    scope: &str,
    item_id: Option<&str>,
) -> Result<std::path::PathBuf> {
    match scope {
        "global" => Ok(paths.glossary_path()),
        "item" => Ok(
            paths.item_glossary_path(validate_item_id(item_id.ok_or_else(|| {
                EngineError::InstallFailed("item glossary requires item_id".to_string())
            })?)?),
        ),
        _ => Err(EngineError::InstallFailed(format!(
            "unsupported glossary scope: {scope}"
        ))),
    }
}

fn save_glossary_document(path: &Path, entries: Vec<GlossaryEntry>) -> Result<Vec<GlossaryEntry>> {
    let entries = normalize_entries(entries)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let document = GlossaryDocument {
        schema_version: GLOSSARY_SCHEMA_VERSION,
        entries: entries.clone(),
    };
    crate::persistence::atomic_write_text(
        path,
        &format!("{}\n", serde_json::to_string_pretty(&document)?),
    )?;
    Ok(entries)
}

pub fn glossary_bundle(paths: &AppPaths, item_id: Option<&str>) -> Result<GlossaryBundle> {
    let path = paths.glossary_path();
    ensure_default_glossary(&path)?;
    let global_entries = load_glossary_entries(&path)?;
    let item_entries = match item_id {
        Some(item_id) => {
            load_glossary_entries(&paths.item_glossary_path(validate_item_id(item_id)?))?
        }
        None => Vec::new(),
    };
    let mut effective = BTreeMap::new();
    for entry in global_entries.iter().chain(item_entries.iter()) {
        effective.insert(entry.source.clone(), entry.clone());
    }
    Ok(GlossaryBundle {
        global_entries,
        item_entries,
        effective_entries: effective.into_values().collect(),
    })
}

pub fn glossary_save_scoped(
    paths: &AppPaths,
    scope: &str,
    item_id: Option<&str>,
    entries: Vec<GlossaryEntry>,
) -> Result<GlossaryBundle> {
    let path = glossary_path_for_scope(paths, scope, item_id)?;
    save_glossary_document(&path, entries)?;
    glossary_bundle(paths, item_id)
}

pub fn glossary_load(paths: &AppPaths) -> Result<BTreeMap<String, String>> {
    Ok(glossary_bundle(paths, None)?
        .global_entries
        .into_iter()
        .map(|entry| (entry.source, entry.target))
        .collect())
}

pub fn glossary_save(paths: &AppPaths, entries: &BTreeMap<String, String>) -> Result<()> {
    let entries = entries
        .iter()
        .map(|(source, target)| GlossaryEntry {
            source: source.clone(),
            target: target.clone(),
            context: None,
            notes: None,
        })
        .collect();
    save_glossary_document(&paths.glossary_path(), entries)?;
    Ok(())
}

pub fn glossary_export_csv(paths: &AppPaths, out_path: &Path) -> Result<usize> {
    glossary_export_scoped(paths, "global", None, out_path)
}

pub fn glossary_export_scoped(
    paths: &AppPaths,
    scope: &str,
    item_id: Option<&str>,
    out_path: &Path,
) -> Result<usize> {
    let path = glossary_path_for_scope(paths, scope, item_id)?;
    let entries = load_glossary_entries(&path)?;
    if out_path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("json"))
    {
        let document = GlossaryDocument {
            schema_version: GLOSSARY_SCHEMA_VERSION,
            entries,
        };
        crate::persistence::atomic_write_text(
            out_path,
            &format!("{}\n", serde_json::to_string_pretty(&document)?),
        )?;
        return Ok(document.entries.len());
    }
    let mut wtr = csv::Writer::from_path(out_path)?;
    wtr.write_record(["source", "target", "context", "notes"])?;
    for entry in &entries {
        wtr.write_record([
            entry.source.as_str(),
            entry.target.as_str(),
            entry.context.as_deref().unwrap_or(""),
            entry.notes.as_deref().unwrap_or(""),
        ])?;
    }
    wtr.flush()?;
    Ok(entries.len())
}

pub fn glossary_import_csv(paths: &AppPaths, csv_path: &Path) -> Result<usize> {
    glossary_import_scoped(paths, "global", None, csv_path)
}

pub fn glossary_import_scoped(
    paths: &AppPaths,
    scope: &str,
    item_id: Option<&str>,
    import_path: &Path,
) -> Result<usize> {
    let scope_path = glossary_path_for_scope(paths, scope, item_id)?;
    let mut by_source: BTreeMap<String, GlossaryEntry> = load_glossary_entries(&scope_path)?
        .into_iter()
        .map(|entry| (entry.source.clone(), entry))
        .collect();
    let imported = if import_path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("json"))
    {
        load_glossary_entries(import_path)?
    } else {
        let mut rdr = csv::ReaderBuilder::new()
            .has_headers(true)
            .from_path(import_path)?;
        let mut entries = Vec::new();
        for result in rdr.records() {
            let record = result?;
            if record.len() >= 2 {
                entries.push(GlossaryEntry {
                    source: record[0].to_string(),
                    target: record[1].to_string(),
                    context: record.get(2).map(str::to_string),
                    notes: record.get(3).map(str::to_string),
                });
            }
        }
        normalize_entries(entries)?
    };
    let count = imported.len();
    for entry in imported {
        by_source.insert(entry.source.clone(), entry);
    }
    save_glossary_document(&scope_path, by_source.into_values().collect())?;
    Ok(count)
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
        let entries = glossary_entries_sorted(&[
            GlossaryEntry {
                source: "foo".to_string(),
                target: "X".to_string(),
                context: None,
                notes: None,
            },
            GlossaryEntry {
                source: "foobar".to_string(),
                target: "Y".to_string(),
                context: None,
                notes: None,
            },
        ]);
        assert_eq!(apply_glossary("foobar foo", &entries), "Y X");
    }

    #[test]
    fn glossary_prompt_only_contains_terms_present_in_source() {
        let source = SubtitleDocument {
            schema_version: SUBTITLE_JSON_SCHEMA_VERSION,
            kind: "source".to_string(),
            lang: "ja".to_string(),
            segments: vec![SubtitleSegment {
                index: 0,
                start_ms: 0,
                end_ms: 1_000,
                text: "東京の先生".to_string(),
                speaker: None,
            }],
        };
        let entries = vec![
            GlossaryEntry {
                source: "東京".to_string(),
                target: "Tokyo".to_string(),
                context: Some("place name".to_string()),
                notes: None,
            },
            GlossaryEntry {
                source: "大阪".to_string(),
                target: "Osaka".to_string(),
                context: None,
                notes: None,
            },
        ];
        let relevant = glossary_entries_for_source(&entries, &source);
        let (prompt, prompt_entry_count) =
            build_translation_prompt(&TranslationStyleSettings::default(), &relevant);
        let prompt = prompt.expect("prompt");
        assert_eq!(relevant.len(), 1);
        assert_eq!(prompt_entry_count, 1);
        assert!(prompt.contains("東京 = Tokyo (place name)"));
        assert!(!prompt.contains("Osaka"));
    }

    #[test]
    fn translation_prompt_combines_style_honorifics_and_relevant_glossary() {
        let settings = TranslationStyleSettings {
            schema_version: 1,
            style: TranslationStyle::Formal,
            honorific_mode: HonorificMode::Drop,
            custom_instruction: None,
        };
        let entries = vec![GlossaryEntry {
            source: "先生".to_string(),
            target: "teacher".to_string(),
            context: Some("school title".to_string()),
            notes: None,
        }];
        let (prompt, count) = build_translation_prompt(&settings, &entries);
        let prompt = prompt.expect("translation prompt");
        assert!(prompt.contains("formal, professional English"));
        assert!(prompt.contains("Omit Japanese and Korean honorific suffixes"));
        assert!(prompt.contains("先生 = teacher (school title)"));
        assert_eq!(count, 1);
        assert!(prompt.chars().count() <= MAX_WHISPER_GLOSSARY_PROMPT_CHARS);

        let long_custom = TranslationStyleSettings {
            style: TranslationStyle::Custom,
            honorific_mode: HonorificMode::Drop,
            custom_instruction: Some("x".repeat(MAX_CUSTOM_STYLE_INSTRUCTION_BYTES)),
            ..TranslationStyleSettings::default()
        };
        let (bounded_prompt, count) = build_translation_prompt(&long_custom, &entries);
        let bounded_prompt = bounded_prompt.expect("bounded custom prompt");
        assert!(bounded_prompt.starts_with("Omit Japanese and Korean honorific suffixes"));
        assert_eq!(
            bounded_prompt.chars().count(),
            MAX_WHISPER_GLOSSARY_PROMPT_CHARS
        );
        assert_eq!(count, 0);
    }

    #[test]
    fn translation_style_changes_punctuation_and_safely_drops_suffixes() {
        let formal = TranslationStyleSettings {
            style: TranslationStyle::Formal,
            ..TranslationStyleSettings::default()
        };
        let informal = TranslationStyleSettings {
            style: TranslationStyle::Informal,
            ..TranslationStyleSettings::default()
        };
        let drop = TranslationStyleSettings {
            honorific_mode: HonorificMode::Drop,
            ..TranslationStyleSettings::default()
        };
        assert_eq!(
            apply_translation_style("hello there", &formal),
            "Hello there."
        );
        assert_eq!(
            apply_translation_style("Hello there.", &informal),
            "Hello there"
        );
        assert_eq!(
            apply_translation_style("Aiko-san and Sun", &drop),
            "Aiko and Sun"
        );
    }

    #[test]
    fn translation_style_persists_per_item_and_rejects_unsafe_input() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app"));
        paths.ensure_dirs().expect("dirs");
        assert_eq!(
            translation_style_load(&paths, "item-1").expect("load"),
            None
        );

        let saved = translation_style_save(
            &paths,
            "item-1",
            TranslationStyleSettings {
                schema_version: 1,
                style: TranslationStyle::Custom,
                honorific_mode: HonorificMode::Translate,
                custom_instruction: Some(" terse broadcast English ".to_string()),
            },
        )
        .expect("save");
        assert_eq!(
            saved.custom_instruction.as_deref(),
            Some("terse broadcast English")
        );
        assert_eq!(
            translation_style_load(&paths, "item-1").expect("reload"),
            Some(saved)
        );
        assert_eq!(
            translation_style_load(&paths, "item-2").expect("other item"),
            None
        );

        let traversal =
            translation_style_save(&paths, "../outside", TranslationStyleSettings::default())
                .expect_err("path traversal must fail");
        assert!(traversal.to_string().contains("invalid glossary item id"));

        let control = translation_style_save(
            &paths,
            "item-1",
            TranslationStyleSettings {
                style: TranslationStyle::Custom,
                custom_instruction: Some("broadcast\0style".to_string()),
                ..TranslationStyleSettings::default()
            },
        )
        .expect_err("control characters must fail");
        assert!(control
            .to_string()
            .contains("must not contain control characters"));

        let too_long = translation_style_save(
            &paths,
            "item-1",
            TranslationStyleSettings {
                style: TranslationStyle::Custom,
                custom_instruction: Some("x".repeat(MAX_CUSTOM_STYLE_INSTRUCTION_BYTES + 1)),
                ..TranslationStyleSettings::default()
            },
        )
        .expect_err("oversized custom instruction must fail");
        assert!(too_long.to_string().contains("must be at most"));
    }

    #[test]
    fn item_glossary_overrides_legacy_global_entry() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app"));
        paths.ensure_dirs().expect("dirs");
        crate::persistence::atomic_write_text(
            &paths.glossary_path(),
            "{\"先生\":\"Teacher\",\"東京\":\"Tokyo\"}\n",
        )
        .expect("legacy global glossary");

        glossary_save_scoped(
            &paths,
            "item",
            Some("item-1"),
            vec![GlossaryEntry {
                source: "先生".to_string(),
                target: "Sensei".to_string(),
                context: Some("honorific".to_string()),
                notes: None,
            }],
        )
        .expect("item glossary");

        let bundle = glossary_bundle(&paths, Some("item-1")).expect("bundle");
        assert_eq!(bundle.global_entries.len(), 2);
        assert_eq!(bundle.item_entries.len(), 1);
        assert_eq!(bundle.effective_entries.len(), 2);
        assert_eq!(
            bundle
                .effective_entries
                .iter()
                .find(|entry| entry.source == "先生")
                .map(|entry| entry.target.as_str()),
            Some("Sensei")
        );
    }

    #[test]
    fn item_glossary_rejects_path_traversal() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app"));
        paths.ensure_dirs().expect("dirs");
        let error = glossary_save_scoped(&paths, "item", Some("../outside"), Vec::new())
            .expect_err("path traversal must fail");
        assert!(error.to_string().contains("invalid glossary item id"));
    }

    #[test]
    fn glossary_rejects_empty_target_and_control_context() {
        let empty_target = normalize_entries(vec![GlossaryEntry {
            source: "東京".to_string(),
            target: " ".to_string(),
            context: None,
            notes: None,
        }])
        .expect_err("empty target must fail");
        assert!(empty_target.to_string().contains("must not be empty"));

        let control_context = normalize_entries(vec![GlossaryEntry {
            source: "東京".to_string(),
            target: "Tokyo".to_string(),
            context: Some("place\0name".to_string()),
            notes: None,
        }])
        .expect_err("control context must fail");
        assert!(control_context
            .to_string()
            .contains("must not contain control characters"));
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
