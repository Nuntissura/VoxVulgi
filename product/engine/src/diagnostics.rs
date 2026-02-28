use crate::models::{ModelInventory, ModelStore};
use crate::paths::AppPaths;
use crate::{db, jobs, tools, Result};
use regex::Regex;
use serde::Serialize;
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[derive(Debug, Clone, Serialize)]
pub struct StorageBreakdown {
    pub library_bytes: u64,
    pub derived_bytes: u64,
    pub cache_bytes: u64,
    pub logs_bytes: u64,
    pub db_bytes: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CacheClearSummary {
    pub removed_entries: usize,
    pub removed_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticsBundleResult {
    pub out_path: String,
    pub file_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct LicensingReportResult {
    pub out_path: String,
    pub file_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
struct DiagnosticsBundleManifest {
    schema_version: u32,
    created_at_ms: i64,
    app: BundleAppInfo,
    engine: BundleEngineInfo,
    os: BundleOsInfo,
    storage: StorageBreakdown,
    models: BundleModelsInfo,
    db: BundleDbInfo,
    jobs: BundleJobsInfo,
    retention: jobs::JobLogRetentionPolicy,
    config: BundleConfigInfo,
}

#[derive(Debug, Clone, Serialize)]
struct BundleAppInfo {
    name: String,
    version: String,
}

#[derive(Debug, Clone, Serialize)]
struct BundleEngineInfo {
    version: String,
}

#[derive(Debug, Clone, Serialize)]
struct BundleOsInfo {
    os: String,
    arch: String,
}

#[derive(Debug, Clone, Serialize)]
struct BundleModelsInfo {
    total_installed_bytes: u64,
    models: Vec<BundleModelRow>,
}

#[derive(Debug, Clone, Serialize)]
struct BundleModelRow {
    id: String,
    task: String,
    source_lang: Option<String>,
    target_lang: Option<String>,
    version: String,
    license: String,
    installed: bool,
    expected_bytes: u64,
    installed_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
struct BundleDbInfo {
    schema_version: Option<u32>,
    counts: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Serialize)]
struct BundleJobsInfo {
    recent_jobs: Vec<BundleJobRow>,
    recent_failed_jobs: Vec<BundleJobRow>,
}

#[derive(Debug, Clone, Serialize)]
struct BundleJobRow {
    id: String,
    item_id: Option<String>,
    batch_id: Option<String>,
    job_type: String,
    status: String,
    progress: f32,
    error: Option<String>,
    created_at_ms: i64,
    started_at_ms: Option<i64>,
    finished_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
struct BundleConfigInfo {
    glossary_present: bool,
    glossary_bytes: u64,
    glossary_entries: Option<u64>,
    download_dir_override_present: bool,
}

pub fn engine_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn storage_breakdown(paths: &AppPaths) -> Result<StorageBreakdown> {
    paths.ensure_dirs()?;

    let library_bytes = directory_size_bytes_best_effort(&paths.library_dir());
    let derived_bytes = directory_size_bytes_best_effort(&paths.derived_dir());
    let cache_bytes = directory_size_bytes_best_effort(&paths.cache_dir());
    let logs_bytes = directory_size_bytes_best_effort(&paths.logs_dir());
    let db_bytes = file_size_bytes_best_effort(&paths.db_dir().join("app.sqlite"));
    let total_bytes = library_bytes
        .saturating_add(derived_bytes)
        .saturating_add(cache_bytes)
        .saturating_add(logs_bytes)
        .saturating_add(db_bytes);

    Ok(StorageBreakdown {
        library_bytes,
        derived_bytes,
        cache_bytes,
        logs_bytes,
        db_bytes,
        total_bytes,
    })
}

pub fn clear_cache(paths: &AppPaths) -> Result<CacheClearSummary> {
    paths.ensure_dirs()?;
    clear_dir_entries_with_bytes(&paths.cache_dir())
}

pub fn export_diagnostics_bundle(
    paths: &AppPaths,
    out_path: impl AsRef<Path>,
    app_name: &str,
    app_version: &str,
) -> Result<DiagnosticsBundleResult> {
    paths.ensure_dirs()?;

    let out_path = out_path.as_ref().to_path_buf();
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let retention = jobs::job_log_retention_policy();
    let storage = storage_breakdown(paths)?;
    let models = export_models_inventory(ModelStore::new(paths.clone()).inventory().unwrap_or(
        ModelInventory {
            models_dir: String::new(),
            total_installed_bytes: 0,
            models: Vec::new(),
        },
    ));
    let (db_info, recent_jobs) = export_db_and_jobs(paths, 200)?;
    let recent_failed_jobs: Vec<BundleJobRow> = recent_jobs
        .iter()
        .filter(|row| row.status == "failed")
        .cloned()
        .take(20)
        .collect();

    let config = export_config_summary(paths);

    let manifest = DiagnosticsBundleManifest {
        schema_version: 1,
        created_at_ms: now_ms(),
        app: BundleAppInfo {
            name: app_name.to_string(),
            version: app_version.to_string(),
        },
        engine: BundleEngineInfo {
            version: engine_version().to_string(),
        },
        os: BundleOsInfo {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
        },
        storage: storage.clone(),
        models,
        db: db_info,
        jobs: BundleJobsInfo {
            recent_jobs: recent_jobs.clone(),
            recent_failed_jobs: recent_failed_jobs.clone(),
        },
        retention: retention.clone(),
        config,
    };

    let file = std::fs::File::create(&out_path)?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);

    write_pretty_json_to_zip(&mut zip, "manifest.json", &manifest, options)?;
    write_pretty_json_to_zip(&mut zip, "storage.json", &storage, options)?;

    write_pretty_json_to_zip(
        &mut zip,
        "jobs_failed.json",
        &recent_failed_jobs,
        options,
    )?;

    add_redacted_failed_job_logs(
        &mut zip,
        paths,
        &retention,
        &recent_failed_jobs,
        options,
    )?;

    zip.finish().map_err(zip_err_to_io)?;

    let file_bytes = std::fs::metadata(&out_path).map(|m| m.len()).unwrap_or(0);
    Ok(DiagnosticsBundleResult {
        out_path: out_path.to_string_lossy().to_string(),
        file_bytes,
    })
}

pub fn generate_licensing_report(paths: &AppPaths) -> Result<LicensingReportResult> {
    paths.ensure_dirs()?;

    let out_dir = paths.derived_dir().join("reports");
    std::fs::create_dir_all(&out_dir)?;
    let out_path = out_dir.join("licensing_report.md");

    let models_inventory = ModelStore::new(paths.clone()).inventory().unwrap_or(ModelInventory {
        models_dir: String::new(),
        total_installed_bytes: 0,
        models: Vec::new(),
    });

    let python_packages = list_python_packages_best_effort(paths);
    let openvoice_manifest = read_json_best_effort(
        &paths
            .python_models_dir()
            .join("openvoice_v2")
            .join("voxvulgi_openvoicev2_manifest.json"),
    );
    let spleeter_manifest = read_json_best_effort(
        &paths
            .python_models_dir()
            .join("spleeter")
            .join("2stems")
            .join("voxvulgi_spleeter_manifest.json"),
    );

    let mut md = String::new();
    md.push_str("# VoxVulgi licensing / attribution report (best-effort)\n\n");
    md.push_str("> Not legal advice. This is an automated best-effort inventory of locally installed packs/models.\n\n");
    md.push_str(&format!(
        "- Generated at (ms since epoch): {}\n",
        now_ms()
    ));
    md.push_str(&format!("- App data dir: `{}`\n", paths.base_dir.to_string_lossy()));
    md.push_str("\n");

    md.push_str("## Engine models (managed)\n\n");
    if models_inventory.models.is_empty() {
        md.push_str("- No managed models found.\n\n");
    } else {
        md.push_str("| ID | Task | Version | License |\n|---|---|---|---|\n");
        for m in &models_inventory.models {
            md.push_str(&format!(
                "| `{}` | {} | {} | {} |\n",
                m.id, m.task, m.version, m.license
            ));
        }
        md.push_str("\n");
    }

    md.push_str("## Python packages (venv)\n\n");
    match python_packages {
        Some(pkgs) if !pkgs.is_empty() => {
            md.push_str("| Name | Version | License |\n|---|---|---|\n");
            for p in pkgs {
                let license = p.license.unwrap_or_else(|| "unknown".to_string());
                md.push_str(&format!("| `{}` | {} | {} |\n", p.name, p.version, license));
            }
            md.push_str("\n");
        }
        Some(_) => {
            md.push_str("- No packages detected in the VoxVulgi Python venv.\n\n");
        }
        None => {
            md.push_str("- Python venv not available; no Python package inventory.\n\n");
        }
    }

    md.push_str("## Model / weights manifests (explicit installs)\n\n");
    md.push_str("### OpenVoiceV2\n\n");
    if let Some(v) = openvoice_manifest {
        md.push_str("```json\n");
        md.push_str(&serde_json::to_string_pretty(&v).unwrap_or_else(|_| "{}".to_string()));
        md.push_str("\n```\n\n");
    } else {
        md.push_str("- Not installed / manifest not found.\n\n");
    }

    md.push_str("### Spleeter 2stems\n\n");
    if let Some(v) = spleeter_manifest {
        md.push_str("```json\n");
        md.push_str(&serde_json::to_string_pretty(&v).unwrap_or_else(|_| "{}".to_string()));
        md.push_str("\n```\n\n");
    } else {
        md.push_str("- Not installed / manifest not found.\n\n");
    }

    std::fs::write(&out_path, md)?;
    let file_bytes = std::fs::metadata(&out_path).map(|m| m.len()).unwrap_or(0);
    Ok(LicensingReportResult {
        out_path: out_path.to_string_lossy().to_string(),
        file_bytes,
    })
}

#[derive(Debug, Clone, serde::Deserialize)]
struct PythonPackageRow {
    name: String,
    version: String,
    license: Option<String>,
}

fn list_python_packages_best_effort(paths: &AppPaths) -> Option<Vec<PythonPackageRow>> {
    let venv_python = tools::python_venv_python_path(paths).ok()?;
    let code = r#"
import json
try:
    import importlib.metadata as md
except Exception:  # pragma: no cover
    import importlib_metadata as md

rows = []
for dist in md.distributions():
    meta = dist.metadata
    name = meta.get("Name") or meta.get("Summary") or dist.metadata.get("Name") or "unknown"
    version = getattr(dist, "version", None) or meta.get("Version") or "unknown"
    lic = meta.get("License")
    if lic:
        lic = lic.strip()
    rows.append({"name": str(name), "version": str(version), "license": (lic if lic else None)})

rows.sort(key=lambda r: r["name"].lower())
print(json.dumps(rows, ensure_ascii=False))
"#;

    let output = crate::cmd::command(venv_python)
        .args(["-c", code])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let last = text.lines().rev().find(|l| !l.trim().is_empty())?.trim();
    let parsed: Vec<PythonPackageRow> = serde_json::from_str(last).ok()?;
    Some(parsed)
}

fn read_json_best_effort(path: &Path) -> Option<serde_json::Value> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn export_models_inventory(inventory: ModelInventory) -> BundleModelsInfo {
    BundleModelsInfo {
        total_installed_bytes: inventory.total_installed_bytes,
        models: inventory
            .models
            .into_iter()
            .map(|m| BundleModelRow {
                id: m.id,
                task: m.task,
                source_lang: m.source_lang,
                target_lang: m.target_lang,
                version: m.version,
                license: m.license,
                installed: m.installed,
                expected_bytes: m.expected_bytes,
                installed_bytes: m.installed_bytes,
            })
            .collect(),
    }
}

fn export_db_and_jobs(paths: &AppPaths, jobs_limit: usize) -> Result<(BundleDbInfo, Vec<BundleJobRow>)> {
    let conn = db::open(paths)?;
    db::migrate(&conn)?;

    let schema_version_raw: Option<String> = match conn.query_row(
        "SELECT value FROM meta WHERE key='schema_version'",
        [],
        |row| row.get(0),
    ) {
        Ok(v) => Some(v),
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(e) => return Err(e.into()),
    };
    let schema_version = schema_version_raw.and_then(|v| v.parse::<u32>().ok());

    let mut counts = BTreeMap::new();
    for (name, sql) in [
        ("library_item", "SELECT COUNT(*) FROM library_item"),
        ("subtitle_track", "SELECT COUNT(*) FROM subtitle_track"),
        ("job", "SELECT COUNT(*) FROM job"),
        ("ingest_provenance", "SELECT COUNT(*) FROM ingest_provenance"),
    ] {
        let count: u64 = conn.query_row(sql, [], |row| row.get::<_, i64>(0))? as u64;
        counts.insert(name.to_string(), count);
    }

    let jobs = jobs::list_jobs(paths, jobs_limit, 0).unwrap_or_default();
    let rows = jobs
        .into_iter()
        .map(|j| BundleJobRow {
            id: j.id,
            item_id: j.item_id,
            batch_id: j.batch_id,
            job_type: j.job_type,
            status: job_status_as_str(&j.status).to_string(),
            progress: j.progress,
            error: j.error.map(|value| redact_free_text(&value)),
            created_at_ms: j.created_at_ms,
            started_at_ms: j.started_at_ms,
            finished_at_ms: j.finished_at_ms,
        })
        .collect();

    Ok((
        BundleDbInfo {
            schema_version,
            counts,
        },
        rows,
    ))
}

fn job_status_as_str(status: &jobs::JobStatus) -> &'static str {
    match status {
        jobs::JobStatus::Queued => "queued",
        jobs::JobStatus::Running => "running",
        jobs::JobStatus::Succeeded => "succeeded",
        jobs::JobStatus::Failed => "failed",
        jobs::JobStatus::Canceled => "canceled",
    }
}

fn export_config_summary(paths: &AppPaths) -> BundleConfigInfo {
    let glossary_path = paths.glossary_path();
    let (glossary_present, glossary_bytes, glossary_entries) = match std::fs::metadata(&glossary_path)
    {
        Ok(m) if m.is_file() => {
            let bytes = m.len();
            let entries = std::fs::read_to_string(&glossary_path)
                .ok()
                .and_then(|raw| serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&raw).ok())
                .map(|map| map.len() as u64);
            (true, bytes, entries)
        }
        _ => (false, 0, None),
    };

    let download_dir_override_present = paths
        .download_dir_override()
        .map(|v| v.is_some())
        .unwrap_or(false);

    BundleConfigInfo {
        glossary_present,
        glossary_bytes,
        glossary_entries,
        download_dir_override_present,
    }
}

fn add_redacted_failed_job_logs<W: Write + std::io::Seek>(
    zip: &mut zip::ZipWriter<W>,
    paths: &AppPaths,
    retention: &jobs::JobLogRetentionPolicy,
    recent_failed_jobs: &[BundleJobRow],
    options: zip::write::FileOptions,
) -> Result<()> {
    let job_rows = jobs::list_jobs(paths, 500, 0).unwrap_or_default();
    let mut failed_by_id: BTreeMap<String, jobs::JobRow> = BTreeMap::new();
    for job in job_rows {
        if !matches!(job.status, jobs::JobStatus::Failed) {
            continue;
        }
        failed_by_id.insert(job.id.clone(), job);
    }

    const MAX_LOG_BYTES_PER_FILE: u64 = 2 * 1024 * 1024;

    for row in recent_failed_jobs.iter().take(10) {
        let Some(job) = failed_by_id.get(&row.id) else {
            continue;
        };
        let base_path = PathBuf::from(&job.logs_path);
        let log_paths = log_path_candidates(&base_path, retention.max_backups);
        for path in log_paths {
            if !path.exists() || !path.is_file() {
                continue;
            }

            let file_name = match path.file_name().and_then(|v| v.to_str()) {
                Some(v) if !v.trim().is_empty() => v,
                _ => continue,
            };
            let zip_path = format!("logs/jobs/{file_name}");
            write_redacted_jsonl_file_to_zip(zip, &zip_path, &path, MAX_LOG_BYTES_PER_FILE, options)?;
        }
    }

    Ok(())
}

fn log_path_candidates(base: &Path, max_backups: usize) -> Vec<PathBuf> {
    let mut paths = Vec::with_capacity(1 + max_backups);
    paths.push(base.to_path_buf());
    for i in 1..=max_backups {
        paths.push(path_with_suffix(base, &format!(".{i}")));
    }
    paths
}

fn path_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let file_name = match path.file_name() {
        Some(n) => n.to_string_lossy().to_string(),
        None => suffix.to_string(),
    };
    path.with_file_name(format!("{file_name}{suffix}"))
}

fn write_pretty_json_to_zip<W: Write + std::io::Seek, T: Serialize>(
    zip: &mut zip::ZipWriter<W>,
    zip_path: &str,
    value: &T,
    options: zip::write::FileOptions,
) -> Result<()> {
    zip.start_file(zip_path, options).map_err(zip_err_to_io)?;
    let json = serde_json::to_string_pretty(value)?;
    zip.write_all(json.as_bytes())?;
    zip.write_all(b"\n")?;
    Ok(())
}

fn write_redacted_jsonl_file_to_zip<W: Write + std::io::Seek>(
    zip: &mut zip::ZipWriter<W>,
    zip_path: &str,
    src_path: &Path,
    max_bytes: u64,
    options: zip::write::FileOptions,
) -> Result<()> {
    let file = std::fs::File::open(src_path)?;
    let mut reader = BufReader::new(file);
    zip.start_file(zip_path, options).map_err(zip_err_to_io)?;

    let mut written: u64 = 0;
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            break;
        }

        let raw = line.trim_end_matches(&['\r', '\n'][..]);
        let redacted = redact_jsonl_line(raw);
        let out = format!("{redacted}\n");
        if written.saturating_add(out.len() as u64) > max_bytes {
            let truncated = serde_json::json!({
                "event": "truncated",
                "message": "log truncated in diagnostics bundle",
                "original_path_present": true
            });
            let out = format!("{}\n", truncated.to_string());
            if written.saturating_add(out.len() as u64) <= max_bytes.saturating_add(512) {
                let _ = zip.write_all(out.as_bytes());
            }
            break;
        }
        zip.write_all(out.as_bytes())?;
        written = written.saturating_add(out.len() as u64);
    }

    Ok(())
}

fn redact_jsonl_line(line: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(line) {
        Ok(mut value) => {
            redact_value_in_place(&mut value);
            serde_json::to_string(&value).unwrap_or_else(|_| {
                serde_json::json!({"event": "redaction_failed"}).to_string()
            })
        }
        Err(_) => serde_json::json!({
            "event": "raw_line",
            "line": redact_free_text(line),
        })
        .to_string(),
    }
}

fn redact_value_in_place(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, val) in map.iter_mut() {
                if should_redact_key(key) {
                    *val = serde_json::Value::String("<redacted>".to_string());
                    continue;
                }
                redact_value_in_place(val);
            }
        }
        serde_json::Value::Array(values) => {
            for v in values.iter_mut() {
                redact_value_in_place(v);
            }
        }
        serde_json::Value::String(s) => {
            *s = redact_free_text(s);
        }
        _ => {}
    }
}

fn should_redact_key(key: &str) -> bool {
    let key = key.trim().to_ascii_lowercase();
    if key.is_empty() {
        return false;
    }
    key.contains("cookie")
        || key.contains("authorization")
        || key.contains("auth_header")
        || key.contains("token")
        || key.contains("secret")
        || key.contains("password")
        || key.contains("api_key")
}

fn redact_string(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    // Redact obvious absolute file paths (PII risk).
    if looks_like_absolute_path(trimmed) {
        return "<redacted_path>".to_string();
    }

    // Redact URLs down to origin only (PII/ID risk in paths and query strings).
    if let Ok(url) = url::Url::parse(trimmed) {
        if let Some(host) = url.host_str() {
            return format!("{}://{}/<redacted>", url.scheme(), host);
        }
        return format!("{}://<redacted>", url.scheme());
    }

    trimmed.to_string()
}

fn redact_free_text(value: &str) -> String {
    static URL_RE: OnceLock<Regex> = OnceLock::new();
    static WIN_ABS_PATH_RE: OnceLock<Regex> = OnceLock::new();
    static UNC_PATH_RE: OnceLock<Regex> = OnceLock::new();
    static UNIX_HOME_PATH_RE: OnceLock<Regex> = OnceLock::new();
    static SENSITIVE_KV_RE: OnceLock<Regex> = OnceLock::new();
    static BEARER_RE: OnceLock<Regex> = OnceLock::new();

    let mut out = value.to_string();

    let url_re = URL_RE.get_or_init(|| Regex::new(r"https?://\S+").unwrap());
    out = url_re
        .replace_all(&out, |caps: &regex::Captures<'_>| {
            redact_string(caps.get(0).map(|m| m.as_str()).unwrap_or(""))
        })
        .to_string();

    let win_re = WIN_ABS_PATH_RE
        .get_or_init(|| Regex::new(r#"(?i)[a-z]:\\[^\s"']+"#).unwrap());
    out = win_re.replace_all(&out, "<redacted_path>").to_string();

    let unc_re = UNC_PATH_RE
        .get_or_init(|| Regex::new(r#"\\\\[^\s"']+"#).unwrap());
    out = unc_re.replace_all(&out, "<redacted_path>").to_string();

    let unix_re =
        UNIX_HOME_PATH_RE.get_or_init(|| Regex::new(r#"/(Users|home)/[^\s"']+"#).unwrap());
    out = unix_re.replace_all(&out, "<redacted_path>").to_string();

    let bearer_re = BEARER_RE
        .get_or_init(|| Regex::new(r"(?i)\bbearer\s+[a-z0-9._-]+").unwrap());
    out = bearer_re.replace_all(&out, "Bearer <redacted>").to_string();

    let kv_re = SENSITIVE_KV_RE.get_or_init(|| {
        Regex::new(r#"(?i)\b(authorization|cookie|token|api[_-]?key|secret|password)\b\s*[:=]\s*([^\s"']+)"#)
        .unwrap()
    });
    out = kv_re.replace_all(&out, "$1=<redacted>").to_string();

    out
}

fn looks_like_absolute_path(value: &str) -> bool {
    if value.starts_with('/') || value.starts_with("\\\\") {
        return true;
    }
    // Windows drive path: C:\...
    if value.len() >= 3 {
        let bytes = value.as_bytes();
        if bytes[1] == b':' && (bytes[2] == b'\\' || bytes[2] == b'/') {
            let c = bytes[0];
            if (b'a'..=b'z').contains(&c) || (b'A'..=b'Z').contains(&c) {
                return true;
            }
        }
    }
    value.contains("\\Users\\") || value.contains("/Users/") || value.contains("/home/")
}

pub(crate) fn directory_size_bytes_best_effort(path: &Path) -> u64 {
    let mut sum = 0_u64;
    if !path.exists() {
        return 0;
    }

    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(v) => v,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let file_type = match entry.file_type() {
                Ok(v) => v,
                Err(_) => continue,
            };
            if file_type.is_symlink() {
                continue;
            }

            if file_type.is_file() {
                if let Ok(meta) = entry.metadata() {
                    sum = sum.saturating_add(meta.len());
                }
            } else if file_type.is_dir() {
                stack.push(entry.path());
            }
        }
    }

    sum
}

fn zip_err_to_io(err: zip::result::ZipError) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, err)
}

fn file_size_bytes_best_effort(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

fn clear_dir_entries_with_bytes(dir: &Path) -> Result<CacheClearSummary> {
    if !dir.exists() {
        return Ok(CacheClearSummary {
            removed_entries: 0,
            removed_bytes: 0,
        });
    }

    let mut removed_entries = 0_usize;
    let mut removed_bytes = 0_u64;

    for entry in std::fs::read_dir(dir)? {
        let entry = match entry {
            Ok(v) => v,
            Err(_) => continue,
        };
        let path = entry.path();
        let bytes = if path.is_dir() {
            directory_size_bytes_best_effort(&path)
        } else {
            file_size_bytes_best_effort(&path)
        };

        let removed = if path.is_dir() {
            std::fs::remove_dir_all(&path).is_ok()
        } else {
            std::fs::remove_file(&path).is_ok()
        };
        if removed {
            removed_entries += 1;
            removed_bytes = removed_bytes.saturating_add(bytes);
        }
    }

    Ok(CacheClearSummary {
        removed_entries,
        removed_bytes,
    })
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use filetime::{set_file_mtime, FileTime};
    use rusqlite::params;
    use std::io::Read;

    #[test]
    fn prune_job_logs_removes_old_files_by_age() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        paths.ensure_dirs().expect("ensure dirs");

        let old_path = paths.job_logs_dir().join("old.jsonl");
        let recent_path = paths.job_logs_dir().join("recent.jsonl");
        std::fs::write(&old_path, "{}\n").expect("write old");
        std::fs::write(&recent_path, "{}\n").expect("write recent");

        let now = std::time::SystemTime::now();
        let forty_days = std::time::Duration::from_secs(40 * 24 * 60 * 60);
        let old_time = now.checked_sub(forty_days).expect("checked_sub");
        set_file_mtime(&old_path, FileTime::from_system_time(old_time)).expect("set mtime");

        jobs::prune_job_logs_now(&paths).expect("prune");

        assert!(!old_path.exists(), "old log should be removed");
        assert!(recent_path.exists(), "recent log should be kept");
    }

    #[test]
    fn export_bundle_redacts_secrets_in_logs_and_job_errors() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        paths.ensure_dirs().expect("ensure dirs");
        db::ensure_schema(&paths).expect("schema");

        let job_id = "job_diagnostics_redaction_test";
        let log_path = paths.job_logs_dir().join(format!("{job_id}.jsonl"));
        std::fs::write(
            &log_path,
            concat!(
                "{\"event\":\"download\",\"auth_cookie\":\"verysecret\",\"authorization\":\"Bearer abcdef\",",
                "\"url\":\"https://example.com/private?id=123\",\"path\":\"C:\\\\Users\\\\Alice\\\\video.mp4\"}\n"
            ),
        )
        .expect("write log");

        let logs_path_str = log_path.to_string_lossy().to_string();
        let conn = db::open(&paths).expect("open db");
        db::migrate(&conn).expect("migrate");
        conn.execute(
            r#"
INSERT INTO job(
  id, item_id, batch_id, type, status, progress, error, params_json,
  created_at_ms, started_at_ms, finished_at_ms, logs_path
) VALUES (?1, NULL, NULL, ?2, ?3, 0.0, ?4, ?5, ?6, NULL, ?7, ?8)
"#,
            params![
                job_id,
                "download_direct_url",
                "failed",
                "authorization: Bearer abcdef cookie=verysecret url=https://example.com/private?id=123 path=C:\\Users\\Alice\\video.mp4",
                "{}",
                now_ms(),
                now_ms(),
                logs_path_str
            ],
        )
        .expect("insert job");

        let out_path = dir.path().join("diagnostics.zip");
        export_diagnostics_bundle(&paths, &out_path, "VoxVulgi", "0.0.0").expect("export");

        let file = std::fs::File::open(&out_path).expect("open zip");
        let mut archive = zip::ZipArchive::new(file).expect("zip archive");

        let mut manifest = String::new();
        archive
            .by_name("manifest.json")
            .expect("manifest.json")
            .read_to_string(&mut manifest)
            .expect("read manifest");

        let mut jobs_failed = String::new();
        archive
            .by_name("jobs_failed.json")
            .expect("jobs_failed.json")
            .read_to_string(&mut jobs_failed)
            .expect("read jobs_failed");

        let mut redacted_log = String::new();
        archive
            .by_name(&format!("logs/jobs/{job_id}.jsonl"))
            .expect("job log")
            .read_to_string(&mut redacted_log)
            .expect("read log");

        for content in [&manifest, &jobs_failed, &redacted_log] {
            assert!(!content.contains("verysecret"), "cookie should be redacted");
            assert!(!content.contains("abcdef"), "bearer token should be redacted");
            assert!(
                !content.contains("private?id=123"),
                "URL path/query should be redacted"
            );
            assert!(
                !content.contains("C:\\\\Users\\\\Alice"),
                "absolute path should be redacted"
            );
        }

        assert!(
            redacted_log.contains("<redacted>") || redacted_log.contains("<redacted_path>"),
            "redacted log should include redaction markers"
        );
    }
}
