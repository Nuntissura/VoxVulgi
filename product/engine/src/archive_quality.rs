//! Archive policy receipts contain only media facts, never provider URLs or credentials.
use crate::{paths::AppPaths, persistence, EngineError, Result};
use serde_json::{json, Value};
use std::path::Path;

pub fn install_plugin(attempt: &Path) -> Result<std::path::PathBuf> {
    let root = attempt.join("policy_plugin");
    let module = root.join("voxvulgi/yt_dlp_plugins/postprocessor");
    std::fs::create_dir_all(&module)?;
    persistence::atomic_write_text(&module.join("voxvulgi_archive_policy.py"),
        include_str!("../resources/tooling/voxvulgi_archive_policy.py"))?;
    Ok(root)
}

pub fn selection(info: &Value) -> Result<Value> {
    let policy = &info["voxvulgi_archive_policy"];
    if policy["schema_version"] != 1 {
        return Err(EngineError::InstallFailed("Archive quality policy did not run; refusing an unverified download".into()));
    }
    let width = info["width"].as_i64().unwrap_or(0);
    let height = info["height"].as_i64().unwrap_or(0);
    let maximum = policy["best_available_short_side"].as_i64().unwrap_or(0);
    if maximum > 0 && width.min(height) < maximum {
        return Err(EngineError::InstallFailed(format!("Quality check: selected {width}x{height}, but {maximum}p is available; refusing a degraded result")));
    }
    Ok(json!({"schema_version":1,"state":"selected","width":width,"height":height,
        "fps":info["fps"],"format_id":info["format_id"],"video_codec":info["vcodec"],
        "audio_codec":info["acodec"],"best_available_short_side":maximum,
        "original_languages":policy["original_languages"],"subtitles":policy["subtitles"],
        "english_available":policy["english_available"],"original_available":policy["original_available"]}))
}

pub fn save(paths: &AppPaths, id: &str, receipt: &Value) -> Result<()> {
    let folder = paths.job_artifacts_dir(id);
    std::fs::create_dir_all(&folder)?;
    persistence::atomic_write_text(&folder.join("archive_quality.json"), &serde_json::to_string(receipt)?)?;
    Ok(())
}

pub fn read(paths: &AppPaths, id: &str) -> Option<Value> {
    let path = paths.job_artifacts_dir(id).join("archive_quality.json");
    if std::fs::metadata(&path).ok()?.len() > 32768 { return None; }
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

pub fn validate_output(stdout: &[u8], probe: &crate::ffmpeg::MediaProbe) -> Result<Value> {
    let text = String::from_utf8_lossy(stdout);
    let info: Value = text.lines().filter_map(|s|s.strip_prefix("VV_MEDIA_PRE:")).last()
        .ok_or_else(||EngineError::InstallFailed("Missing archive selection receipt".into()))
        .and_then(|s|Ok(serde_json::from_str(s)?))?;
    let mut receipt = selection(&info)?;
    if probe.width.unwrap_or(0) != receipt["width"].as_i64().unwrap_or(0)
        || probe.height.unwrap_or(0) != receipt["height"].as_i64().unwrap_or(0) {
        return Err(EngineError::InstallFailed("Final MKV resolution does not match selected source video".into()));
    }
    receipt["state"] = json!("verified");
    receipt["embedded_subtitle_count"] = json!(probe.subtitle_streams.len());
    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn archive_quality_rejects_missing_policy_and_degraded_selection() {
        assert!(selection(&json!({"width":1080,"height":1920})).is_err());
        let mut info = json!({"width":360,"height":640,"voxvulgi_archive_policy":{"schema_version":1,"best_available_short_side":1080}});
        assert!(selection(&info).is_err());
        info["width"]=json!(1080); info["height"]=json!(1920);
        assert_eq!(selection(&info).unwrap()["width"],1080);
        // A genuinely low-resolution source is valid when that is its best rendition.
        info["width"]=json!(240); info["height"]=json!(426);
        info["voxvulgi_archive_policy"]["best_available_short_side"]=json!(240);
        assert!(selection(&info).is_ok());
    }
}
