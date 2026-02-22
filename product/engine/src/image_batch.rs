use crate::{EngineError, Result};
use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::cmp::Reverse;
use std::collections::{HashSet, VecDeque};
use std::io::{Read, Write};
use std::path::Path;
use std::thread;
use std::time::Duration;
use url::Url;

const DEFAULT_MAX_PAGES: usize = 1500;
const MAX_MAX_PAGES: usize = 5000;
const DEFAULT_DELAY_MS: u64 = 350;
const MAX_DELAY_MS: u64 = 10_000;
const DEFAULT_OUTPUT_SUBDIR: &str = "image_archive";
const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36";
const MAX_INLINE_RESOLVE_PAGES: usize = 24;
const MAX_INLINE_HTML_BYTES: u64 = 2 * 1024 * 1024;

const PROFILE_MARKERS: &[&str] = &[
    "avatar",
    "profile",
    "userpic",
    "gravatar",
    "author-photo",
    "member-photo",
    "display-picture",
];

const NOISE_IMAGE_MARKERS: &[&str] = &[
    "emoji",
    "emoticon",
    "smiley",
    "icon",
    "sprite",
    "logo",
    "badge",
    "reaction",
    "sticker",
    "rank",
    "spacer",
    "placeholder",
];

const THUMB_HINTS: &[&str] = &[
    "thumb",
    "thumbnail",
    "_tn",
    "-tn",
    "_sm",
    "-sm",
    "_small",
    "-small",
    "small/",
    "/small",
];

const THUMB_ATTR_MARKERS: &[&str] = &["thumb", "thumbnail", "preview", "mini", "small"];

const NEXT_TEXT_MARKERS: &[&str] = &[
    "next", "older", "more", "weiter", "suivant", "volgende", "nast", "\u{203A}", "\u{00BB}", ">>", ">",
];

const IMAGE_ATTRS: &[&str] = &[
    "src",
    "data-src",
    "data-original",
    "data-full",
    "data-lazy-src",
];

const ANCHOR_IMAGE_ATTRS: &[&str] = &[
    "href",
    "data-src",
    "data-original",
    "data-full",
    "data-image",
    "data-url",
    "data-lightbox",
    "data-photo",
];

const URL_QUERY_THUMB_KEYS: &[&str] = &[
    "w",
    "h",
    "width",
    "height",
    "size",
    "thumb",
    "thumbnail",
    "fit",
    "crop",
    "quality",
    "resize",
    "maxwidth",
    "maxheight",
    "sz",
    "s",
];

const IFRAME_CONTENT_HINTS: &[&str] = &[
    "/embed/",
    "/iframe/",
    "/attachment/",
    "/attachments/",
    "/gallery/",
    "/photo/",
    "/photos/",
    "/image/",
    "/images/",
    "/media/",
    "image=",
    "photo=",
    "media=",
    "attachment=",
    "file=",
];

const IMAGE_EXTS: &[&str] = &[
    ".jpg", ".jpeg", ".png", ".gif", ".webp", ".bmp", ".tif", ".tiff", ".svg", ".avif", ".heic",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageBatchRequest {
    pub start_urls: Vec<String>,
    pub max_pages: usize,
    pub delay_ms: u64,
    pub allow_cross_domain: bool,
    pub follow_content_links: bool,
    pub skip_url_keywords: Vec<String>,
    pub output_subdir: String,
    pub auth_cookie: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageBatchSummary {
    pub pages_crawled: usize,
    pub images_downloaded: usize,
    pub skipped_profile_images: usize,
    pub duplicate_images: usize,
    pub failed_images: usize,
    pub manifest_path: String,
    pub output_dir: String,
}

#[derive(Debug, Clone)]
struct ImageCandidate {
    page_url: String,
    urls: Vec<String>,
    skip_profile: bool,
}

#[derive(Debug, Clone, Copy)]
enum CandidateStatus {
    Downloaded,
    Duplicate,
    SkippedProfile,
    SkippedCustomKeyword,
    Failed,
}

pub fn build_image_batch_request(
    start_urls: Vec<String>,
    max_pages: Option<usize>,
    delay_ms: Option<u64>,
    allow_cross_domain: Option<bool>,
    follow_content_links: Option<bool>,
    skip_url_keywords: Vec<String>,
    output_subdir: Option<String>,
    auth_cookie: Option<String>,
) -> Result<ImageBatchRequest> {
    let start_urls = normalize_start_urls(start_urls)?;
    if start_urls.is_empty() {
        return Err(EngineError::InstallFailed(
            "provide at least one valid http(s) page URL".to_string(),
        ));
    }

    let max_pages = max_pages
        .unwrap_or(DEFAULT_MAX_PAGES)
        .clamp(1, MAX_MAX_PAGES);
    let delay_ms = delay_ms.unwrap_or(DEFAULT_DELAY_MS).min(MAX_DELAY_MS);
    let allow_cross_domain = allow_cross_domain.unwrap_or(false);
    let follow_content_links = follow_content_links.unwrap_or(false);
    let output_subdir = sanitize_output_subdir(output_subdir.as_deref().unwrap_or(""));
    let skip_url_keywords = normalize_keywords(skip_url_keywords);
    let auth_cookie = normalize_cookie(auth_cookie.as_deref());

    Ok(ImageBatchRequest {
        start_urls,
        max_pages,
        delay_ms,
        allow_cross_domain,
        follow_content_links,
        skip_url_keywords,
        output_subdir,
        auth_cookie,
    })
}

pub fn run_image_batch_download<FShouldCancel, FSetProgress, FLog>(
    request: &ImageBatchRequest,
    output_root: &Path,
    manifest_path: &Path,
    mut should_cancel: FShouldCancel,
    mut set_progress: FSetProgress,
    mut log_line: FLog,
) -> Result<ImageBatchSummary>
where
    FShouldCancel: FnMut() -> Result<bool>,
    FSetProgress: FnMut(f32) -> Result<()>,
    FLog: FnMut(&str, &str, serde_json::Value) -> Result<()>,
{
    std::fs::create_dir_all(output_root)?;
    if let Some(parent) = manifest_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let manifest_file = std::fs::File::create(manifest_path)?;
    let mut manifest = std::io::BufWriter::new(manifest_file);
    write_manifest_header(&mut manifest)?;

    let mut config = ureq::Agent::config_builder();
    config = config
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(25)))
        .user_agent(DEFAULT_USER_AGENT);
    let agent: ureq::Agent = config.build().into();

    let allowed_hosts: HashSet<String> = request
        .start_urls
        .iter()
        .filter_map(|url| host_of(url))
        .collect();

    let mut queue: VecDeque<String> = request.start_urls.iter().cloned().collect();
    let mut visited_pages: HashSet<String> = HashSet::new();
    let mut seen_image_urls: HashSet<String> = HashSet::new();
    let mut seen_hashes: HashSet<String> = HashSet::new();

    let mut pages_crawled = 0_usize;
    let mut downloaded = 0_usize;
    let mut skipped_profile = 0_usize;
    let mut duplicate_images = 0_usize;
    let mut failed_images = 0_usize;

    while let Some(page_url) = queue.pop_front() {
        if pages_crawled >= request.max_pages {
            break;
        }

        if should_cancel()? {
            break;
        }

        if !visited_pages.insert(page_url.clone()) {
            continue;
        }

        if !request.allow_cross_domain {
            let Some(host) = host_of(&page_url) else {
                continue;
            };
            if !allowed_hosts.contains(&host) {
                continue;
            }
        }

        let mut response =
            match call_get_with_cookie(&agent, &page_url, request.auth_cookie.as_deref()) {
                Ok(resp) => resp,
                Err(err) => {
                    log_line(
                        "warn",
                        "image_batch_page_fetch_failed",
                        serde_json::json!({
                            "url": redact_url_for_log(&page_url),
                            "error": err.to_string()
                        }),
                    )?;
                    continue;
                }
            };

        if response.status().as_u16() >= 400 {
            continue;
        }

        let content_type = header_string(&response, "content-type");
        if !is_html_content_type(&content_type) {
            continue;
        }

        let mut html_buf = Vec::new();
        if response
            .body_mut()
            .as_reader()
            .read_to_end(&mut html_buf)
            .is_err()
        {
            continue;
        }

        pages_crawled += 1;
        let pct = 0.10 + 0.80 * ((pages_crawled as f32) / (request.max_pages as f32)).min(1.0);
        set_progress(pct)?;
        let html = String::from_utf8_lossy(&html_buf).into_owned();
        let document = Html::parse_document(&html);

        log_line(
            "info",
            "image_batch_page_crawled",
            serde_json::json!({
                "index": pages_crawled,
                "url": redact_url_for_log(&page_url),
            }),
        )?;

        let candidates = extract_image_candidates(&document, &page_url);
        for candidate in candidates {
            if should_cancel()? {
                break;
            }
            let Some(first_url) = candidate.urls.first() else {
                continue;
            };
            if !seen_image_urls.insert(first_url.clone()) {
                continue;
            }

            let host_folder = sanitize_name(
                host_of(&candidate.page_url)
                    .as_deref()
                    .unwrap_or("unknown-host"),
            );
            let image_out_dir = output_root.join(host_folder).join("images");
            std::fs::create_dir_all(&image_out_dir)?;

            let (status, saved_path, byte_count, digest) = download_candidate_image(
                &agent,
                &candidate,
                &image_out_dir,
                &mut seen_hashes,
                &request.skip_url_keywords,
                request.auth_cookie.as_deref(),
            );

            match status {
                CandidateStatus::Downloaded => downloaded += 1,
                CandidateStatus::Duplicate => duplicate_images += 1,
                CandidateStatus::SkippedProfile => skipped_profile += 1,
                CandidateStatus::SkippedCustomKeyword => {}
                CandidateStatus::Failed => failed_images += 1,
            }

            write_manifest_row(
                &mut manifest,
                &[
                    &candidate.page_url,
                    first_url,
                    status_as_str(status),
                    saved_path.as_deref().unwrap_or(""),
                    &byte_count.map(|v| v.to_string()).unwrap_or_default(),
                    digest.as_deref().unwrap_or(""),
                    &candidate.urls.len().to_string(),
                ],
            )?;
        }

        let (next_links, content_links) =
            discover_links(&document, &page_url, request.follow_content_links);
        for link in next_links.into_iter().chain(content_links.into_iter()) {
            if visited_pages.contains(&link) {
                continue;
            }
            if !request.allow_cross_domain {
                let Some(host) = host_of(&link) else {
                    continue;
                };
                if !allowed_hosts.contains(&host) {
                    continue;
                }
            }
            queue.push_back(link);
        }

        if request.delay_ms > 0 {
            thread::sleep(Duration::from_millis(request.delay_ms));
        }
    }

    manifest.flush()?;

    Ok(ImageBatchSummary {
        pages_crawled,
        images_downloaded: downloaded,
        skipped_profile_images: skipped_profile,
        duplicate_images,
        failed_images,
        manifest_path: manifest_path.to_string_lossy().to_string(),
        output_dir: output_root.to_string_lossy().to_string(),
    })
}

fn normalize_start_urls(inputs: Vec<String>) -> Result<Vec<String>> {
    let mut output: Vec<String> = Vec::new();
    for input in inputs {
        for part in input.split(|ch| matches!(ch, '\n' | '\r' | '\t' | ',' | ';' | ' ')) {
            let trimmed = part.trim();
            if trimmed.is_empty() {
                continue;
            }
            let normalized = normalize_http_url(trimmed)?;
            if !output.iter().any(|existing| existing == &normalized) {
                output.push(normalized);
            }
        }
    }
    Ok(output)
}

fn normalize_http_url(value: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(EngineError::InstallFailed("empty URL provided".to_string()));
    }
    let parsed = Url::parse(trimmed)
        .map_err(|_| EngineError::InstallFailed("invalid URL format".to_string()))?;
    match parsed.scheme() {
        "http" | "https" => {}
        _ => {
            return Err(EngineError::InstallFailed(format!(
                "unsupported URL scheme for {}; only http/https are allowed",
                redact_url_for_log(trimmed)
            )));
        }
    }
    if parsed.host_str().is_none() {
        return Err(EngineError::InstallFailed(format!(
            "URL is missing host: {}",
            redact_url_for_log(trimmed)
        )));
    }
    Ok(trimmed.to_string())
}

fn sanitize_output_subdir(value: &str) -> String {
    let safe = sanitize_name(value);
    if safe.is_empty() {
        DEFAULT_OUTPUT_SUBDIR.to_string()
    } else {
        safe
    }
}

fn normalize_keywords(values: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in values {
        for part in raw.split(|ch| matches!(ch, '\n' | '\r' | '\t' | ',' | ';' | ' ')) {
            let trimmed = part.trim().to_ascii_lowercase();
            if trimmed.is_empty() {
                continue;
            }
            if !out.iter().any(|existing| existing == &trimmed) {
                out.push(trimmed);
            }
        }
    }
    out
}

fn normalize_cookie(value: Option<&str>) -> Option<String> {
    let raw = value.unwrap_or("").trim();
    if raw.is_empty() {
        return None;
    }

    if let Some(from_json) = cookie_json_to_header(raw) {
        return Some(from_json);
    }

    let path = Path::new(raw);
    if path.exists() && path.is_file() {
        if let Ok(contents) = std::fs::read_to_string(path) {
            if let Some(from_json) = cookie_json_to_header(&contents) {
                return Some(from_json);
            }

            let trimmed = contents.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }

    Some(raw.to_string())
}

fn cookie_json_to_header(raw_json: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(raw_json).ok()?;
    let mut pairs: Vec<(String, String)> = Vec::new();

    fn push_pair(pairs: &mut Vec<(String, String)>, name: &str, value: &str) {
        let name = name.trim();
        if name.is_empty() || name.contains(';') || name.contains('=') {
            return;
        }
        pairs.push((name.to_string(), value.trim().to_string()));
    }

    fn collect_from_value(value: &serde_json::Value, pairs: &mut Vec<(String, String)>) {
        match value {
            serde_json::Value::Array(values) => {
                for item in values {
                    collect_from_value(item, pairs);
                }
            }
            serde_json::Value::Object(map) => {
                if let (Some(name), Some(value)) = (map.get("name"), map.get("value")) {
                    if let (Some(name), Some(value)) = (name.as_str(), value.as_str()) {
                        push_pair(pairs, name, value);
                    }
                    return;
                }

                if let Some(cookies) = map.get("cookies") {
                    collect_from_value(cookies, pairs);
                    return;
                }

                for (key, value) in map {
                    if let Some(value) = value.as_str() {
                        push_pair(pairs, key, value);
                    }
                }
            }
            serde_json::Value::String(cookie) => {
                let trimmed = cookie.trim();
                if let Some((name, value)) = trimmed.split_once('=') {
                    push_pair(pairs, name, value);
                }
            }
            _ => {}
        }
    }

    collect_from_value(&value, &mut pairs);
    if pairs.is_empty() {
        return None;
    }

    // Keep latest value for duplicate keys.
    let mut seen = HashSet::new();
    let mut out: Vec<(String, String)> = Vec::new();
    for (name, value) in pairs.into_iter().rev() {
        if seen.insert(name.clone()) {
            out.push((name, value));
        }
    }
    out.reverse();

    Some(
        out.into_iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; "),
    )
}

fn normalize_url_with_base(raw_url: &str, base_url: &Url) -> Option<String> {
    let raw_url = raw_url.trim();
    if raw_url.is_empty() {
        return None;
    }
    let lower = raw_url.to_ascii_lowercase();
    if lower.starts_with("javascript:")
        || lower.starts_with("mailto:")
        || lower.starts_with("tel:")
        || lower.starts_with('#')
    {
        return None;
    }

    let mut joined = base_url.join(raw_url).ok()?;
    if !matches!(joined.scheme(), "http" | "https") {
        return None;
    }
    joined.set_fragment(None);
    Some(joined.to_string())
}

fn host_of(url: &str) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    parsed.host_str().map(|v| v.to_ascii_lowercase())
}

fn looks_like_image_url(url: &str) -> bool {
    let parsed = match Url::parse(url) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let path = parsed.path().to_ascii_lowercase();
    IMAGE_EXTS.iter().any(|ext| path.ends_with(ext))
}

fn sanitize_name(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.trim().chars() {
        if ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' || ch == '-' {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    out.trim_matches(|ch| ch == '.' || ch == '_').to_string()
}

fn parse_srcset_best(srcset: &str, base_url: &Url) -> Option<String> {
    let mut best_url: Option<String> = None;
    let mut best_score = -1_i64;
    for chunk in srcset.split(',') {
        let part = chunk.trim();
        if part.is_empty() {
            continue;
        }
        let bits: Vec<&str> = part.split_whitespace().collect();
        if bits.is_empty() {
            continue;
        }
        let candidate = normalize_url_with_base(bits[0], base_url)?;
        let mut score = 1_i64;
        if let Some(token) = bits.get(1) {
            let token = token.trim().to_ascii_lowercase();
            if token.ends_with('w') {
                score = token
                    .trim_end_matches('w')
                    .parse::<i64>()
                    .unwrap_or(1)
                    .max(1);
            } else if token.ends_with('x') {
                let parsed = token
                    .trim_end_matches('x')
                    .parse::<f64>()
                    .unwrap_or(1.0)
                    .max(1.0);
                score = (parsed * 1000.0) as i64;
            }
        }

        if score > best_score {
            best_score = score;
            best_url = Some(candidate);
        }
    }
    best_url
}

fn strip_thumbnail_query_params(url: &str) -> String {
    let mut parsed = match Url::parse(url) {
        Ok(v) => v,
        Err(_) => return url.to_string(),
    };
    let pairs: Vec<(String, String)> = parsed.query_pairs().into_owned().collect();
    if pairs.is_empty() {
        return url.to_string();
    }

    let original_len = pairs.len();
    let mut kept: Vec<(String, String)> = Vec::new();
    for (k, v) in pairs {
        if URL_QUERY_THUMB_KEYS
            .iter()
            .any(|key| key.eq_ignore_ascii_case(&k))
        {
            continue;
        }
        kept.push((k, v));
    }
    if kept.len() == original_len {
        return url.to_string();
    }

    parsed.set_query(None);
    if !kept.is_empty() {
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        for (k, v) in kept {
            serializer.append_pair(&k, &v);
        }
        let query = serializer.finish();
        parsed.set_query(Some(&query));
    }
    parsed.to_string()
}

fn guess_fullsize_variants(url: &str) -> Vec<String> {
    let mut variants: Vec<String> = vec![url.to_string()];
    let cleaned_query = strip_thumbnail_query_params(url);
    if cleaned_query != url {
        variants.push(cleaned_query);
    }

    let parsed = match Url::parse(url) {
        Ok(v) => v,
        Err(_) => return dedupe_urls(variants),
    };

    let path = parsed.path().to_string();
    let thumb_re =
        Regex::new(r"(?i)([_-])(thumb|thumbnail|small|sm|tn)\b|\b(thumb|thumbnail|small)[_-]")
            .expect("thumb regex");
    let resized_suffix_re = Regex::new(r"(?i)(.*?)[-_]\d{2,4}x\d{2,4}(\.[a-z0-9]{3,5})$")
        .expect("resized suffix regex");
    let mut path_variants: Vec<String> = vec![
        path.replace("/thumb/", "/"),
        path.replace("/thumbs/", "/"),
        path.replace("/thumbnail/", "/"),
        path.replace("/thumbnails/", "/"),
        path.replace("/cache/", "/"),
        path.replace("/resized/", "/"),
    ];
    path_variants.push(thumb_re.replace_all(&path, "").to_string());
    path_variants.push(resized_suffix_re.replace(&path, "$1$2").to_string());

    for candidate_path in path_variants {
        if candidate_path.is_empty() || candidate_path == path {
            continue;
        }
        let mut updated = parsed.clone();
        updated.set_path(&candidate_path);
        variants.push(updated.to_string());
    }

    dedupe_urls(variants)
}

fn dedupe_urls(values: Vec<String>) -> Vec<String> {
    let mut out = Vec::with_capacity(values.len());
    let mut seen = HashSet::new();
    for value in values {
        if seen.insert(value.clone()) {
            out.push(value);
        }
    }
    out
}

fn keyword_match(value: &str, keywords: &[&str]) -> bool {
    let lowered = value.to_ascii_lowercase();
    keywords.iter().any(|keyword| lowered.contains(keyword))
}

fn keyword_match_dynamic(value: &str, keywords: &[String]) -> bool {
    let lowered = value.to_ascii_lowercase();
    keywords.iter().any(|keyword| lowered.contains(keyword))
}

fn parse_dimension_attr(tag: &ElementRef<'_>, key: &str) -> Option<i64> {
    let raw = tag.value().attr(key)?.trim().trim_end_matches("px");
    raw.parse::<i64>().ok().filter(|v| *v >= 0)
}

fn collect_tag_text(tag: &ElementRef<'_>) -> String {
    [
        tag.value().attr("class").unwrap_or(""),
        tag.value().attr("id").unwrap_or(""),
        tag.value().attr("alt").unwrap_or(""),
        tag.value().attr("title").unwrap_or(""),
        tag.value().attr("aria-label").unwrap_or(""),
        tag.value().attr("data-type").unwrap_or(""),
        tag.value().attr("data-kind").unwrap_or(""),
    ]
    .join(" ")
}

fn looks_like_noise_image_url(url: &str) -> bool {
    keyword_match(url, NOISE_IMAGE_MARKERS)
}

fn is_likely_noise_image(tag: &ElementRef<'_>, url: &str) -> bool {
    if looks_like_noise_image_url(url) {
        return true;
    }

    let attrs = collect_tag_text(tag);
    if keyword_match(&attrs, NOISE_IMAGE_MARKERS) {
        return true;
    }

    let width = parse_dimension_attr(tag, "width");
    let height = parse_dimension_attr(tag, "height");
    match (width, height) {
        (Some(w), Some(h)) if w <= 96 && h <= 96 => true,
        (Some(w), _) if w <= 32 => true,
        (_, Some(h)) if h <= 32 => true,
        _ => false,
    }
}

fn is_likely_thumbnail_image(tag: &ElementRef<'_>, url: &str) -> bool {
    if keyword_match(url, THUMB_HINTS) {
        return true;
    }

    let attrs = collect_tag_text(tag);
    if keyword_match(&attrs, THUMB_ATTR_MARKERS) {
        return true;
    }

    let width = parse_dimension_attr(tag, "width");
    let height = parse_dimension_attr(tag, "height");
    match (width, height) {
        (Some(w), Some(h)) if w <= 360 && h <= 360 => true,
        (Some(w), _) if w <= 240 => true,
        (_, Some(h)) if h <= 180 => true,
        _ => false,
    }
}

fn is_likely_image_asset_link(url: &str) -> bool {
    if looks_like_image_url(url) {
        return true;
    }
    let lower = url.to_ascii_lowercase();
    [
        "/attachment/",
        "/attachments/",
        "/upload/",
        "/uploads/",
        "/media/",
        "/image/",
        "/images/",
        "/photo/",
        "/photos/",
        "/gallery/",
        "image_id=",
        "attachment_id=",
        "photo_id=",
    ]
    .iter()
    .any(|token| lower.contains(token))
}

fn is_probable_iframe_content_link(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    IFRAME_CONTENT_HINTS
        .iter()
        .any(|token| lower.contains(token))
}

fn looks_like_resized_variant(url: &str) -> bool {
    let parsed = match Url::parse(url) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let segment = parsed
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .unwrap_or("");
    let stem = Path::new(segment)
        .file_stem()
        .map(|v| v.to_string_lossy().to_string())
        .unwrap_or_default();
    if stem.is_empty() {
        return false;
    }

    let token = stem
        .rsplit_once('-')
        .map(|(_, rhs)| rhs)
        .or_else(|| stem.rsplit_once('_').map(|(_, rhs)| rhs));
    let Some(token) = token else {
        return false;
    };
    let Some((w, h)) = token.split_once('x') else {
        return false;
    };
    if w.len() < 2 || w.len() > 4 || h.len() < 2 || h.len() > 4 {
        return false;
    }
    w.chars().all(|ch| ch.is_ascii_digit()) && h.chars().all(|ch| ch.is_ascii_digit())
}

fn is_explicit_full_candidate(url: &str) -> bool {
    !keyword_match(url, THUMB_HINTS)
        && !looks_like_noise_image_url(url)
        && !looks_like_resized_variant(url)
}

fn is_strong_full_candidate(url: &str) -> bool {
    !keyword_match(url, THUMB_HINTS)
        && !looks_like_noise_image_url(url)
        && !looks_like_resized_variant(url)
}

fn candidate_url_score(url: &str) -> i32 {
    let mut score = 0_i32;
    if is_strong_full_candidate(url) {
        score += 100;
    }
    if is_likely_image_asset_link(url) {
        score += 35;
    }
    if keyword_match(url, THUMB_HINTS) {
        score -= 60;
    }
    if looks_like_resized_variant(url) {
        score -= 50;
    }
    if looks_like_noise_image_url(url) {
        score -= 120;
    }
    score
}

fn is_likely_profile_image(tag: &ElementRef<'_>, url: &str) -> bool {
    if keyword_match(url, PROFILE_MARKERS) {
        return true;
    }

    let mut attrs_to_scan: Vec<String> = vec![collect_tag_text(tag)];

    if let Some(parent) = tag.parent().and_then(ElementRef::wrap) {
        attrs_to_scan.push(collect_tag_text(&parent));
        for attr in ["class", "id", "aria-label", "title"] {
            if let Some(value) = parent.value().attr(attr) {
                attrs_to_scan.push(value.to_string());
            }
        }
    }

    attrs_to_scan
        .iter()
        .any(|text| keyword_match(text, PROFILE_MARKERS))
}

fn extract_image_candidates(document: &Html, page_url: &str) -> Vec<ImageCandidate> {
    let base_url = match Url::parse(page_url) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let mut out: Vec<ImageCandidate> = Vec::new();
    let selector_img = Selector::parse("img").expect("img selector");
    let selector_anchor = Selector::parse("a[href]").expect("anchor selector");
    let selector_iframe = Selector::parse("iframe[src], frame[src]").expect("iframe selector");

    for img in document.select(&selector_img) {
        let mut urls: Vec<String> = Vec::new();
        let mut has_anchor_image_asset = false;

        if let Some(srcset) = img.value().attr("srcset") {
            if let Some(best) = parse_srcset_best(srcset, &base_url) {
                urls.push(best);
            }
        }

        for attr in IMAGE_ATTRS {
            if let Some(raw) = img.value().attr(attr) {
                if let Some(normalized) = normalize_url_with_base(raw, &base_url) {
                    urls.push(normalized);
                }
            }
        }

        if let Some(parent) = img.parent().and_then(ElementRef::wrap) {
            if parent.value().name() == "a" {
                for attr in ANCHOR_IMAGE_ATTRS {
                    if let Some(raw) = parent.value().attr(attr) {
                        if let Some(anchor_url) = normalize_url_with_base(raw, &base_url) {
                            if is_likely_image_asset_link(&anchor_url) {
                                has_anchor_image_asset = true;
                                urls.insert(0, anchor_url);
                            }
                        }
                    }
                }
            }
        }

        let has_explicit_strong_full = urls.iter().any(|url| is_explicit_full_candidate(url));

        let mut deduped_urls: Vec<String> = Vec::new();
        let mut seen_url = HashSet::new();
        for candidate in urls {
            for variant in guess_fullsize_variants(&candidate) {
                if seen_url.insert(variant.clone()) {
                    deduped_urls.push(variant);
                }
            }
        }
        deduped_urls.sort_by_key(|url| Reverse(candidate_url_score(url)));

        if deduped_urls.is_empty() {
            continue;
        }

        let has_strong_full = deduped_urls.iter().any(|url| is_strong_full_candidate(url));
        let likely_thumb = is_likely_thumbnail_image(&img, &deduped_urls[0]);
        if likely_thumb && !has_anchor_image_asset && !has_explicit_strong_full {
            continue;
        }
        let profile = is_likely_profile_image(&img, &deduped_urls[0]);
        let noise = is_likely_noise_image(&img, &deduped_urls[0]);
        let skip_profile = profile || (noise && !has_strong_full);
        out.push(ImageCandidate {
            page_url: page_url.to_string(),
            urls: deduped_urls,
            skip_profile,
        });
    }

    for a in document.select(&selector_anchor) {
        let mut urls: Vec<String> = Vec::new();
        for attr in ANCHOR_IMAGE_ATTRS {
            let Some(raw) = a.value().attr(attr) else {
                continue;
            };
            let Some(normalized) = normalize_url_with_base(raw, &base_url) else {
                continue;
            };
            if is_likely_image_asset_link(&normalized) {
                urls.push(normalized);
            }
        }
        if urls.is_empty() {
            continue;
        }
        let has_explicit_strong_full = urls.iter().any(|url| is_explicit_full_candidate(url));

        let mut deduped_urls: Vec<String> = Vec::new();
        let mut seen_url = HashSet::new();
        for candidate in urls {
            for variant in guess_fullsize_variants(&candidate) {
                if seen_url.insert(variant.clone()) {
                    deduped_urls.push(variant);
                }
            }
        }
        deduped_urls.sort_by_key(|url| Reverse(candidate_url_score(url)));
        if deduped_urls.is_empty() {
            continue;
        }

        let attrs = collect_tag_text(&a);
        let first = &deduped_urls[0];
        let has_strong_full = deduped_urls.iter().any(|url| is_strong_full_candidate(url));
        if keyword_match(first, THUMB_HINTS)
            && !has_strong_full
            && !has_explicit_strong_full
            && keyword_match(&attrs, THUMB_ATTR_MARKERS)
        {
            continue;
        }
        let skip_profile = keyword_match(first, PROFILE_MARKERS)
            || keyword_match(first, NOISE_IMAGE_MARKERS)
            || keyword_match(&attrs, PROFILE_MARKERS)
            || keyword_match(&attrs, NOISE_IMAGE_MARKERS);

        out.push(ImageCandidate {
            page_url: page_url.to_string(),
            urls: deduped_urls,
            skip_profile,
        });
    }

    for frame in document.select(&selector_iframe) {
        let Some(raw_src) = frame.value().attr("src") else {
            continue;
        };
        let Some(normalized) = normalize_url_with_base(raw_src, &base_url) else {
            continue;
        };
        if looks_like_noise_image_url(&normalized) {
            continue;
        }
        if !looks_like_image_url(&normalized)
            && !is_likely_image_asset_link(&normalized)
            && !is_probable_iframe_content_link(&normalized)
        {
            continue;
        }

        out.push(ImageCandidate {
            page_url: page_url.to_string(),
            urls: {
                let mut variants = guess_fullsize_variants(&normalized);
                variants.sort_by_key(|url| Reverse(candidate_url_score(url)));
                variants
            },
            skip_profile: false,
        });
    }

    let mut deduped = Vec::new();
    let mut seen_first_url = HashSet::new();
    for candidate in out {
        let Some(first) = candidate.urls.first() else {
            continue;
        };
        if !seen_first_url.insert(first.clone()) {
            continue;
        }
        deduped.push(candidate);
    }
    deduped
}

fn is_next_link(tag: &ElementRef<'_>, href: &str) -> bool {
    let rel = tag.value().attr("rel").unwrap_or("").to_ascii_lowercase();
    if rel.contains("next") {
        return true;
    }

    let text = element_text(tag).to_ascii_lowercase();
    let attrs = [
        tag.value().attr("class").unwrap_or(""),
        tag.value().attr("id").unwrap_or(""),
        tag.value().attr("aria-label").unwrap_or(""),
        tag.value().attr("title").unwrap_or(""),
    ]
    .join(" ")
    .to_ascii_lowercase();
    let href_l = href.to_ascii_lowercase();

    if keyword_match(&text, NEXT_TEXT_MARKERS) {
        return true;
    }
    if keyword_match(&attrs, &["next", "pagination", "pager", "older", "newer"]) {
        return true;
    }
    href_l.contains("?page=")
        || href_l.contains("&page=")
        || href_l.contains("?p=")
        || href_l.contains("&p=")
}

fn is_probable_content_link(tag: &ElementRef<'_>, href: &str) -> bool {
    let href_l = href.to_ascii_lowercase();
    let attrs = [
        tag.value().attr("class").unwrap_or(""),
        tag.value().attr("id").unwrap_or(""),
        tag.value().attr("rel").unwrap_or(""),
        tag.value().attr("title").unwrap_or(""),
        tag.value().attr("aria-label").unwrap_or(""),
    ]
    .join(" ")
    .to_ascii_lowercase();
    let text = element_text(tag).to_ascii_lowercase();

    if [
        "/post",
        "/posts/",
        "/blog/",
        "/article",
        "/topic",
        "/thread",
        "/forum/",
        "/photo/",
        "/photos/",
        "/gallery/",
        "/attachment/",
        "/attachments/",
        "/media/",
        "/image/",
        "/images/",
    ]
    .iter()
    .any(|token| href_l.contains(token))
    {
        return true;
    }

    if [
        "post",
        "entry",
        "topic",
        "thread",
        "article",
        "photo",
        "gallery",
        "attachment",
        "media",
    ]
    .iter()
    .any(|token| attrs.contains(token) || text.contains(token))
    {
        return true;
    }

    let selector_img = Selector::parse("img").expect("img selector");
    if tag.select(&selector_img).next().is_some() && !looks_like_image_url(href) {
        return true;
    }

    false
}

fn discover_links(
    document: &Html,
    page_url: &str,
    follow_content_links: bool,
) -> (HashSet<String>, HashSet<String>) {
    let base_url = match Url::parse(page_url) {
        Ok(v) => v,
        Err(_) => return (HashSet::new(), HashSet::new()),
    };
    let mut next_links = HashSet::new();
    let mut content_links = HashSet::new();

    let selector_link = Selector::parse("link[href]").expect("link selector");
    let selector_anchor = Selector::parse("a[href]").expect("a selector");
    let selector_iframe = Selector::parse("iframe[src], frame[src]").expect("iframe selector");

    for link in document.select(&selector_link) {
        let Some(href) = link.value().attr("href") else {
            continue;
        };
        let Some(normalized) = normalize_url_with_base(href, &base_url) else {
            continue;
        };
        let rel = link.value().attr("rel").unwrap_or("").to_ascii_lowercase();
        if rel.contains("next") {
            next_links.insert(normalized);
        }
    }

    for a in document.select(&selector_anchor) {
        let Some(href) = a.value().attr("href") else {
            continue;
        };
        let Some(normalized) = normalize_url_with_base(href, &base_url) else {
            continue;
        };
        if is_next_link(&a, &normalized) {
            next_links.insert(normalized.clone());
        }
        if follow_content_links && is_probable_content_link(&a, &normalized) {
            content_links.insert(normalized);
            continue;
        }
        if follow_content_links
            && is_likely_image_asset_link(&normalized)
            && !looks_like_image_url(&normalized)
        {
            // Follow attachment/gallery pages that are likely to hold full-size images.
            content_links.insert(normalized);
        }
    }

    if follow_content_links {
        for frame in document.select(&selector_iframe) {
            let Some(raw_src) = frame.value().attr("src") else {
                continue;
            };
            let Some(normalized) = normalize_url_with_base(raw_src, &base_url) else {
                continue;
            };
            if looks_like_noise_image_url(&normalized) {
                continue;
            }
            if looks_like_image_url(&normalized)
                || is_likely_image_asset_link(&normalized)
                || is_probable_iframe_content_link(&normalized)
            {
                content_links.insert(normalized);
            }
        }
    }

    (next_links, content_links)
}

fn download_candidate_image(
    agent: &ureq::Agent,
    candidate: &ImageCandidate,
    output_dir: &Path,
    seen_hashes: &mut HashSet<String>,
    skip_url_keywords: &[String],
    auth_cookie: Option<&str>,
) -> (CandidateStatus, Option<String>, Option<u64>, Option<String>) {
    if candidate.skip_profile {
        return (CandidateStatus::SkippedProfile, None, None, None);
    }

    struct DownloadedVariant {
        url: String,
        content_type: String,
        data: Vec<u8>,
        digest: String,
    }

    let mut queue: VecDeque<String> = candidate.urls.iter().cloned().collect();
    let mut visited: HashSet<String> = HashSet::new();
    let mut saw_custom_skip = false;
    let mut saw_duplicate: Option<(u64, String)> = None;
    let mut best: Option<DownloadedVariant> = None;

    while let Some(url) = queue.pop_front() {
        if visited.len() >= MAX_INLINE_RESOLVE_PAGES {
            break;
        }
        if !visited.insert(url.clone()) {
            continue;
        }

        if keyword_match_dynamic(&url, skip_url_keywords) {
            saw_custom_skip = true;
            continue;
        }
        if looks_like_noise_image_url(&url) {
            continue;
        }

        let mut response = match call_get_with_cookie(agent, &url, auth_cookie) {
            Ok(resp) => resp,
            Err(_) => continue,
        };
        if response.status().as_u16() >= 400 {
            continue;
        }

        let content_type = header_string(&response, "content-type");

        if !content_type.contains("image") && !looks_like_image_url(&url) {
            if !is_html_content_type(&content_type) {
                continue;
            }

            let mut html_buf = Vec::new();
            if response
                .body_mut()
                .as_reader()
                .take(MAX_INLINE_HTML_BYTES)
                .read_to_end(&mut html_buf)
                .is_err()
            {
                continue;
            }
            if html_buf.is_empty() {
                continue;
            }

            let html = String::from_utf8_lossy(&html_buf).into_owned();
            let nested = extract_nested_image_urls(&html, &url);
            for nested_url in nested {
                if queue.len() >= MAX_INLINE_RESOLVE_PAGES {
                    break;
                }
                if visited.contains(&nested_url) {
                    continue;
                }
                if keyword_match_dynamic(&nested_url, skip_url_keywords) {
                    saw_custom_skip = true;
                    continue;
                }
                queue.push_back(nested_url);
            }
            continue;
        }

        let mut data = Vec::new();
        if response
            .body_mut()
            .as_reader()
            .read_to_end(&mut data)
            .is_err()
        {
            continue;
        }

        if data.is_empty() {
            continue;
        }
        if data.len() < 512 && keyword_match(&url, THUMB_HINTS) {
            continue;
        }
        if data.len() < 1_500 && looks_like_noise_image_url(&url) {
            continue;
        }
        if data.len() < 4_096 && keyword_match(&url, THUMB_HINTS) {
            continue;
        }

        let digest = {
            let mut hasher = Sha256::new();
            hasher.update(&data);
            let out = hasher.finalize();
            hex::encode(out)
        };
        if seen_hashes.contains(&digest) {
            if saw_duplicate.is_none() {
                saw_duplicate = Some((data.len() as u64, digest));
            }
            continue;
        }

        let replace = match &best {
            None => true,
            Some(current) => {
                prefer_downloaded_variant(&url, data.len(), &current.url, current.data.len())
            }
        };
        if replace {
            best = Some(DownloadedVariant {
                url,
                content_type,
                data,
                digest,
            });
        }
    }

    if let Some(chosen) = best {
        let ext = guess_extension(&chosen.url, &chosen.content_type);
        let stem_raw = Url::parse(&chosen.url)
            .ok()
            .and_then(|parsed| {
                parsed
                    .path_segments()
                    .and_then(|mut segments| segments.next_back().map(str::to_string))
            })
            .and_then(|name| {
                Path::new(&name)
                    .file_stem()
                    .map(|v| v.to_string_lossy().to_string())
            })
            .unwrap_or_else(|| "image".to_string());
        let stem = {
            let value = sanitize_name(&stem_raw);
            if value.is_empty() {
                "image".to_string()
            } else {
                value
            }
        };
        let filename = format!("{stem}_{}{}", &chosen.digest[..12], ext);
        let out_path = output_dir.join(filename);
        if std::fs::write(&out_path, &chosen.data).is_err() {
            return (CandidateStatus::Failed, None, None, None);
        }

        seen_hashes.insert(chosen.digest.clone());
        return (
            CandidateStatus::Downloaded,
            Some(out_path.to_string_lossy().to_string()),
            Some(chosen.data.len() as u64),
            Some(chosen.digest),
        );
    }

    if let Some((bytes, digest)) = saw_duplicate {
        return (CandidateStatus::Duplicate, None, Some(bytes), Some(digest));
    }

    if saw_custom_skip {
        (CandidateStatus::SkippedCustomKeyword, None, None, None)
    } else {
        (CandidateStatus::Failed, None, None, None)
    }
}

fn prefer_downloaded_variant(
    new_url: &str,
    new_bytes: usize,
    current_url: &str,
    current_bytes: usize,
) -> bool {
    let new_thumb_like = is_thumbnail_like_url(new_url);
    let current_thumb_like = is_thumbnail_like_url(current_url);
    if new_thumb_like != current_thumb_like {
        return !new_thumb_like;
    }

    if new_bytes != current_bytes {
        return new_bytes > current_bytes;
    }

    let new_score = candidate_url_score(new_url);
    let current_score = candidate_url_score(current_url);
    if new_score != current_score {
        return new_score > current_score;
    }

    new_url.len() < current_url.len()
}

fn is_thumbnail_like_url(url: &str) -> bool {
    keyword_match(url, THUMB_HINTS) || looks_like_resized_variant(url)
}

fn extract_nested_image_urls(html: &str, page_url: &str) -> Vec<String> {
    let document = Html::parse_document(html);
    let mut urls: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for candidate in extract_image_candidates(&document, page_url) {
        if candidate.skip_profile {
            continue;
        }
        for url in candidate.urls {
            if seen.insert(url.clone()) {
                urls.push(url);
                if urls.len() >= MAX_INLINE_RESOLVE_PAGES {
                    return urls;
                }
            }
        }
    }

    let Ok(base_url) = Url::parse(page_url) else {
        return urls;
    };
    let selector_meta = Selector::parse("meta[content], link[href]").expect("meta/link selector");
    for tag in document.select(&selector_meta) {
        let marker = tag
            .value()
            .attr("property")
            .or_else(|| tag.value().attr("name"))
            .or_else(|| tag.value().attr("rel"))
            .unwrap_or("")
            .to_ascii_lowercase();
        if !marker.contains("image") && !marker.contains("photo") {
            continue;
        }
        let raw = tag
            .value()
            .attr("content")
            .or_else(|| tag.value().attr("href"))
            .unwrap_or("");
        let Some(normalized) = normalize_url_with_base(raw, &base_url) else {
            continue;
        };
        for variant in guess_fullsize_variants(&normalized) {
            if seen.insert(variant.clone()) {
                urls.push(variant);
                if urls.len() >= MAX_INLINE_RESOLVE_PAGES {
                    return urls;
                }
            }
        }
    }

    let unescaped = html.replace("\\/", "/");
    let absolute_image_re = Regex::new(
        r#"(?i)https?://[^"'<>\s]+?\.(?:jpg|jpeg|png|gif|webp|bmp|tif|tiff|svg|avif|heic)(?:\?[^"'<>\s]*)?"#,
    )
    .expect("absolute image regex");
    for m in absolute_image_re.find_iter(&unescaped) {
        let Some(normalized) = normalize_url_with_base(m.as_str(), &base_url) else {
            continue;
        };
        for variant in guess_fullsize_variants(&normalized) {
            if seen.insert(variant.clone()) {
                urls.push(variant);
                if urls.len() >= MAX_INLINE_RESOLVE_PAGES {
                    return urls;
                }
            }
        }
    }

    urls
}

fn is_html_content_type(content_type: &str) -> bool {
    if content_type.is_empty() {
        return true;
    }
    content_type.contains("text/html") || content_type.contains("application/xhtml+xml")
}

fn call_get_with_cookie(
    agent: &ureq::Agent,
    url: &str,
    auth_cookie: Option<&str>,
) -> std::result::Result<ureq::http::Response<ureq::Body>, ureq::Error> {
    let mut request = agent.get(url);
    if let Some(cookie) = auth_cookie {
        let trimmed = cookie.trim();
        if !trimmed.is_empty() {
            request = request.header("Cookie", trimmed);
        }
    }
    request.call()
}

fn header_string(response: &ureq::http::Response<ureq::Body>, key: &str) -> String {
    response
        .headers()
        .get(key)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn guess_extension(url: &str, content_type: &str) -> &'static str {
    if let Ok(parsed) = Url::parse(url) {
        let path = parsed.path().to_ascii_lowercase();
        for ext in IMAGE_EXTS {
            if path.ends_with(ext) {
                return ext;
            }
        }
    }

    if content_type.contains("jpeg") {
        ".jpg"
    } else if content_type.contains("png") {
        ".png"
    } else if content_type.contains("gif") {
        ".gif"
    } else if content_type.contains("webp") {
        ".webp"
    } else if content_type.contains("bmp") {
        ".bmp"
    } else if content_type.contains("tiff") {
        ".tiff"
    } else if content_type.contains("svg") {
        ".svg"
    } else if content_type.contains("avif") {
        ".avif"
    } else if content_type.contains("heic") {
        ".heic"
    } else {
        ".jpg"
    }
}

fn status_as_str(value: CandidateStatus) -> &'static str {
    match value {
        CandidateStatus::Downloaded => "downloaded",
        CandidateStatus::Duplicate => "duplicate",
        CandidateStatus::SkippedProfile => "skipped_profile",
        CandidateStatus::SkippedCustomKeyword => "skipped_custom_keyword",
        CandidateStatus::Failed => "failed_all_variants",
    }
}

fn write_manifest_header(writer: &mut std::io::BufWriter<std::fs::File>) -> std::io::Result<()> {
    writer.write_all(b"page_url,image_url,status,saved_path,bytes,sha256,variant_count\n")
}

fn write_manifest_row(
    writer: &mut std::io::BufWriter<std::fs::File>,
    columns: &[&str],
) -> std::io::Result<()> {
    let line = columns
        .iter()
        .map(|value| csv_escape(value))
        .collect::<Vec<_>>()
        .join(",");
    writer.write_all(line.as_bytes())?;
    writer.write_all(b"\n")
}

fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') || value.contains('\r') {
        let escaped = value.replace('"', "\"\"");
        format!("\"{escaped}\"")
    } else {
        value.to_string()
    }
}

fn element_text(el: &ElementRef<'_>) -> String {
    el.text().collect::<Vec<_>>().join(" ").trim().to_string()
}

fn redact_url_for_log(value: &str) -> String {
    match Url::parse(value) {
        Ok(uri) => {
            let scheme = uri.scheme();
            let authority = uri.host_str().unwrap_or("unknown-host");
            format!("{scheme}://{authority}/...")
        }
        Err(_) => "[invalid-url]".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_http_url_allows_http_https_only() {
        assert!(normalize_http_url("https://example.com").is_ok());
        assert!(normalize_http_url("http://example.com").is_ok());
        assert!(normalize_http_url("ftp://example.com").is_err());
    }

    #[test]
    fn build_request_clamps_limits_and_sanitizes_values() {
        let req = build_image_batch_request(
            vec!["https://example.com/blog".to_string()],
            Some(99_999),
            Some(99_999),
            None,
            None,
            vec![
                "avatar".to_string(),
                "avatar".to_string(),
                " profile ".to_string(),
            ],
            Some("Dad Images/2026".to_string()),
            Some(" session=abc ".to_string()),
        )
        .expect("request");
        assert_eq!(req.max_pages, MAX_MAX_PAGES);
        assert_eq!(req.delay_ms, MAX_DELAY_MS);
        assert_eq!(req.skip_url_keywords.len(), 2);
        assert_eq!(req.output_subdir, "dad_images_2026");
        assert_eq!(req.auth_cookie.as_deref(), Some("session=abc"));
        assert!(!req.follow_content_links);
    }

    #[test]
    fn normalize_cookie_accepts_json_cookie_arrays() {
        let raw = r#"[{"name":"session","value":"abc"},{"name":"pref","value":"on"}]"#;
        let out = normalize_cookie(Some(raw)).expect("cookie");
        assert_eq!(out, "session=abc; pref=on");
    }

    #[test]
    fn normalize_cookie_accepts_json_file_paths() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cookie_file = dir.path().join("cookies.json");
        std::fs::write(
            &cookie_file,
            r#"{"cookies":[{"name":"sid","value":"123"},{"name":"token","value":"xyz"}]}"#,
        )
        .expect("write");

        let out = normalize_cookie(Some(cookie_file.to_string_lossy().as_ref())).expect("cookie");
        assert_eq!(out, "sid=123; token=xyz");
    }

    #[test]
    fn guess_fullsize_variants_removes_thumb_patterns() {
        let variants =
            guess_fullsize_variants("https://example.com/images/thumb/photo_small.jpg?w=320");
        assert!(
            variants.iter().any(|v| {
                v.contains("/images/photo_small.jpg") || v.contains("/images/thumb/photo.jpg")
            }),
            "variants={variants:?}"
        );
        assert!(
            variants
                .iter()
                .any(|v| !v.contains("?w=") && !v.contains("&w=")),
            "variants={variants:?}"
        );
    }

    #[test]
    fn extract_image_candidates_prefers_anchor_image_and_skips_avatar_marker() {
        let html = r#"
        <html><body>
          <a href="/full/image.jpg"><img src="/thumbs/image_tn.jpg" class="content" /></a>
          <img src="/avatars/user.jpg" class="avatar profile" />
        </body></html>
        "#;
        let doc = Html::parse_document(html);
        let out = extract_image_candidates(&doc, "https://example.com/forum");
        assert!(!out.is_empty());
        assert!(out.iter().any(|c| c
            .urls
            .first()
            .map(|v| v.contains("/full/image.jpg"))
            .unwrap_or(false)));
        assert!(out.iter().any(|c| c.skip_profile));
    }

    #[test]
    fn extract_image_candidates_skips_emoji_noise_images() {
        let html = r#"
        <html><body>
          <img src="/emoji/smile.png" class="emoji icon" width="20" height="20" />
        </body></html>
        "#;
        let doc = Html::parse_document(html);
        let out = extract_image_candidates(&doc, "https://example.com/forum");
        assert!(out.is_empty(), "candidates={out:?}");
    }

    #[test]
    fn extract_image_candidates_skips_standalone_thumbnail_without_full_reference() {
        let html = r#"
        <html><body>
          <img src="/uploads/thumbs/trip-150x150.jpg" class="post-thumb thumbnail" width="150" height="150" />
        </body></html>
        "#;
        let doc = Html::parse_document(html);
        let out = extract_image_candidates(&doc, "https://example.com/forum/topic/1");
        assert!(out.is_empty(), "candidates={out:?}");
    }

    #[test]
    fn content_link_detection_covers_attachment_and_gallery_paths() {
        let html = r#"
        <html><body>
          <a class="thumb-link" href="/attachments/12345"><img src="/thumbs/pic.jpg" /></a>
          <a href="/gallery/trips/day-1">More photos</a>
        </body></html>
        "#;
        let doc = Html::parse_document(html);
        let (_next_links, content_links) =
            discover_links(&doc, "https://example.com/forum?page=2", true);
        assert!(
            content_links
                .iter()
                .any(|u| u.contains("/attachments/12345")),
            "content_links={content_links:?}"
        );
        assert!(
            content_links
                .iter()
                .any(|u| u.contains("/gallery/trips/day-1")),
            "content_links={content_links:?}"
        );
    }

    #[test]
    fn discover_links_includes_iframe_embed_sources() {
        let html = r#"
        <html><body>
          <iframe src="/embed/media?image=fullsize"></iframe>
        </body></html>
        "#;
        let doc = Html::parse_document(html);
        let (_next_links, content_links) =
            discover_links(&doc, "https://example.com/topic/123", true);
        assert!(
            content_links
                .iter()
                .any(|u| u.contains("/embed/media?image=fullsize")),
            "content_links={content_links:?}"
        );
    }

    #[test]
    fn extract_image_candidates_includes_iframe_image_assets() {
        let html = r#"
        <html><body>
          <iframe src="/embed/photo?id=77"></iframe>
        </body></html>
        "#;
        let doc = Html::parse_document(html);
        let out = extract_image_candidates(&doc, "https://example.com/topic/123");
        assert!(
            out.iter()
                .any(|c| c.urls.iter().any(|u| u.contains("/embed/photo?id=77"))),
            "candidates={out:?}"
        );
    }
}
