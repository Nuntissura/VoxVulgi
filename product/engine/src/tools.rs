use crate::paths::AppPaths;
use crate::{EngineError, Result};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct FfmpegToolsStatus {
    pub installed: bool,
    pub ffmpeg_path: String,
    pub ffprobe_path: String,
}

pub fn ffmpeg_tools_status(paths: &AppPaths) -> FfmpegToolsStatus {
    let ffmpeg_path = paths.ffmpeg_bin_path();
    let ffprobe_path = paths.ffprobe_bin_path();
    let installed = ffmpeg_path.exists() && ffprobe_path.exists();

    FfmpegToolsStatus {
        installed,
        ffmpeg_path: ffmpeg_path.to_string_lossy().to_string(),
        ffprobe_path: ffprobe_path.to_string_lossy().to_string(),
    }
}

pub fn install_ffmpeg_tools(paths: &AppPaths) -> Result<FfmpegToolsStatus> {
    paths.ensure_dirs()?;

    let destination = paths.ffmpeg_dir();
    std::fs::create_dir_all(&destination)?;

    let download_url = ffmpeg_sidecar::download::ffmpeg_download_url()
        .map_err(|e| EngineError::InstallFailed(e.to_string()))?;
    let archive_path =
        ffmpeg_sidecar::download::download_ffmpeg_package(download_url, &destination)
            .map_err(|e| EngineError::InstallFailed(e.to_string()))?;
    ffmpeg_sidecar::download::unpack_ffmpeg(&archive_path, &destination)
        .map_err(|e| EngineError::InstallFailed(e.to_string()))?;

    Ok(ffmpeg_tools_status(paths))
}
