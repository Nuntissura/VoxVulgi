//! WP-0233: Pack warmup gate binary.
//!
//! Runs every Python pack install (or a subset) against a throwaway APPDATA root, then
//! reports per-pack pass/fail + elapsed time. Intended to be invoked from
//! `governance/scripts/pack_warmup_gate.ps1` as a pre-build step in
//! `build_desktop_target.ps1`. Catches resolver drift / lockfile breakage on the
//! developer side instead of letting it ship to users.
//!
//! Usage:
//!
//!   voxvulgi_pack_warmup_gate --stage-base-dir <path> [--pack <name> ...] [--out <dir>]
//!
//! `--stage-base-dir` is the throwaway APPDATA-equivalent root (the wrapper script
//! creates this under `$TEMP\voxvulgi_warmup_gate_<ts>` per WP-0233 scope).
//! `--pack` may be repeated; if omitted, every supported pack is installed.
//! `--out` directs the report files; defaults to `<stage-base-dir>/_gate_report/`.
//!
//! Exits 0 if every requested pack passed. Exits 1 if any pack failed.
//! Always writes `report.json` and `report.md`; never deletes the stage dir (the
//! wrapper script owns cleanup so the operator can inspect failures).

use std::path::{Path, PathBuf};
use std::time::Instant;

use voxvulgi_engine::paths::AppPaths;
use voxvulgi_engine::tools;

const ALL_PACKS: &[&str] = &[
    "spleeter",
    "demucs",
    "diarization",
    "tts_preview",
    "tts_neural_local_v1",
    "tts_voice_preserving_local_v1",
];

#[derive(Debug, serde::Serialize)]
struct PackResult {
    pack: &'static str,
    status: &'static str, // "ok" | "failed" | "skipped"
    elapsed_seconds: f64,
    error: Option<String>,
}

#[derive(Debug, serde::Serialize)]
struct GateReport {
    generated_at_utc: String,
    stage_base_dir: String,
    packs: Vec<PackResult>,
    overall_status: &'static str, // "ok" | "failed"
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(exit) => exit,
        Err(err) => {
            eprintln!("gate error: {err}");
            std::process::ExitCode::from(2)
        }
    }
}

fn run() -> Result<std::process::ExitCode, String> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_help();
        return Ok(std::process::ExitCode::SUCCESS);
    }

    let mut stage_base_dir: Option<PathBuf> = None;
    let mut out_dir: Option<PathBuf> = None;
    let mut packs: Vec<&'static str> = Vec::new();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--stage-base-dir" | "--base-dir" => {
                i += 1;
                let v = args
                    .get(i)
                    .ok_or_else(|| "--stage-base-dir requires a value".to_string())?;
                stage_base_dir = Some(PathBuf::from(v));
            }
            "--out" | "--out-dir" => {
                i += 1;
                let v = args
                    .get(i)
                    .ok_or_else(|| "--out requires a value".to_string())?;
                out_dir = Some(PathBuf::from(v));
            }
            "--pack" => {
                i += 1;
                let v = args
                    .get(i)
                    .ok_or_else(|| "--pack requires a value".to_string())?;
                let matched = ALL_PACKS
                    .iter()
                    .copied()
                    .find(|name| *name == v.as_str())
                    .ok_or_else(|| {
                        format!("unknown pack name: {v}. Valid: {}", ALL_PACKS.join(", "))
                    })?;
                packs.push(matched);
            }
            other => return Err(format!("unknown arg: {other} (try --help)")),
        }
        i += 1;
    }

    let stage = stage_base_dir.ok_or_else(|| "missing required --stage-base-dir".to_string())?;
    if packs.is_empty() {
        packs = ALL_PACKS.to_vec();
    }
    let out = out_dir.unwrap_or_else(|| stage.join("_gate_report"));
    std::fs::create_dir_all(&out).map_err(|e| format!("create out dir: {e}"))?;

    let paths = AppPaths::new(stage.clone());
    paths
        .ensure_dirs()
        .map_err(|e| format!("ensure_dirs: {e}"))?;

    println!("==> pack_warmup_gate");
    println!("    stage base dir: {}", paths.base_dir.display());
    println!("    out dir:        {}", out.display());
    println!("    packs:          {}", packs.join(", "));

    // Python toolchain must be present before any pack runs.
    println!("==> install_python_toolchain");
    let toolchain_started = Instant::now();
    if let Err(e) = tools::install_python_toolchain(&paths) {
        let report = build_report(
            &paths,
            vec![PackResult {
                pack: "python_toolchain",
                status: "failed",
                elapsed_seconds: toolchain_started.elapsed().as_secs_f64(),
                error: Some(e.to_string()),
            }],
        );
        write_reports(&out, &report).map_err(|e| format!("write_reports: {e}"))?;
        return Ok(std::process::ExitCode::from(1));
    }
    println!(
        "    toolchain ready in {:.1}s",
        toolchain_started.elapsed().as_secs_f64()
    );

    // Run each pack install + warmup (warmup is embedded inside each install_*_pack).
    let mut results: Vec<PackResult> = Vec::new();
    for pack in &packs {
        println!("==> {pack}");
        let started = Instant::now();
        let pack_result = run_one_pack(&paths, pack);
        let elapsed = started.elapsed().as_secs_f64();
        match pack_result {
            Ok(()) => {
                println!("    {pack}: ok ({elapsed:.1}s)");
                results.push(PackResult {
                    pack,
                    status: "ok",
                    elapsed_seconds: elapsed,
                    error: None,
                });
            }
            Err(e) => {
                eprintln!("    {pack}: FAILED ({elapsed:.1}s) — {e}");
                results.push(PackResult {
                    pack,
                    status: "failed",
                    elapsed_seconds: elapsed,
                    error: Some(e),
                });
            }
        }
    }

    let report = build_report(&paths, results);
    write_reports(&out, &report).map_err(|e| format!("write_reports: {e}"))?;

    println!();
    println!("==> Summary");
    for pr in &report.packs {
        println!(
            "    {:<32} {:<7} {:>6.1}s",
            pr.pack, pr.status, pr.elapsed_seconds
        );
    }
    println!("    overall: {}", report.overall_status);

    if report.overall_status == "failed" {
        Ok(std::process::ExitCode::from(1))
    } else {
        Ok(std::process::ExitCode::SUCCESS)
    }
}

fn run_one_pack(paths: &AppPaths, pack: &str) -> Result<(), String> {
    match pack {
        "spleeter" => tools::install_spleeter_pack(paths).map(|_| ()),
        "demucs" => tools::install_demucs_pack(paths).map(|_| ()),
        "diarization" => tools::install_diarization_pack(paths).map(|_| ()),
        "tts_preview" => tools::install_tts_preview_pack(paths).map(|_| ()),
        "tts_neural_local_v1" => tools::install_tts_neural_local_v1_pack(paths).map(|_| ()),
        "tts_voice_preserving_local_v1" => {
            tools::install_tts_voice_preserving_local_v1_pack(paths).map(|_| ())
        }
        other => return Err(format!("unknown pack in dispatch: {other}")),
    }
    .map_err(|e| e.to_string())
}

fn build_report(paths: &AppPaths, packs: Vec<PackResult>) -> GateReport {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    // Simple ISO-ish formatting; the wrapper script can reformat for humans.
    let generated = format!("{ms}ms-unix");
    let overall = if packs.iter().any(|p| p.status == "failed") {
        "failed"
    } else {
        "ok"
    };
    GateReport {
        generated_at_utc: generated,
        stage_base_dir: paths.base_dir.to_string_lossy().into_owned(),
        packs,
        overall_status: overall,
    }
}

fn write_reports(out_dir: &Path, report: &GateReport) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(report).unwrap_or_else(|_| "{}".to_string());
    std::fs::write(out_dir.join("report.json"), json.as_bytes())?;

    let mut md = String::new();
    md.push_str("# Pack Warmup Gate Report\n\n");
    md.push_str(&format!("Generated: {}\n\n", report.generated_at_utc));
    md.push_str(&format!("Stage base dir: `{}`\n\n", report.stage_base_dir));
    md.push_str(&format!(
        "Overall status: **{}**\n\n",
        report.overall_status
    ));
    md.push_str("| Pack | Status | Elapsed (s) | Error |\n");
    md.push_str("|---|---|---:|---|\n");
    for p in &report.packs {
        let err = match &p.error {
            Some(e) => {
                let truncated = if e.len() > 240 {
                    format!("{}…", &e[..240])
                } else {
                    e.clone()
                };
                truncated.replace('|', "\\|").replace('\n', " ")
            }
            None => String::from(""),
        };
        md.push_str(&format!(
            "| {} | {} | {:.1} | {} |\n",
            p.pack, p.status, p.elapsed_seconds, err
        ));
    }
    std::fs::write(out_dir.join("report.md"), md.as_bytes())?;
    Ok(())
}

fn print_help() {
    println!(
        "voxvulgi_pack_warmup_gate (WP-0233)\n\
\n\
Usage:\n\
  voxvulgi_pack_warmup_gate --stage-base-dir <path> [--pack <name> ...] [--out <dir>]\n\
\n\
Required:\n\
  --stage-base-dir <path>   Throwaway APPDATA-equivalent root. The wrapper script\n\
                            creates this under TEMP\\voxvulgi_warmup_gate_<ts>.\n\
\n\
Optional:\n\
  --pack <name>             Repeatable. Subset of packs to install. If omitted, all\n\
                            packs run: {}\n\
  --out <dir>               Report output dir. Default: <stage-base-dir>/_gate_report\n\
\n\
Exit codes:\n\
  0  every requested pack passed install + warmup\n\
  1  at least one pack failed (details in report.json / report.md)\n\
  2  bad arguments or setup error\n",
        ALL_PACKS.join(", ")
    );
}
