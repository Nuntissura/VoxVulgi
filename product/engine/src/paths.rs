use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub base_dir: PathBuf,
}

impl AppPaths {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    pub fn config_dir(&self) -> PathBuf {
        self.base_dir.join("config")
    }

    pub fn glossary_path(&self) -> PathBuf {
        self.config_dir().join("glossary.json")
    }

    pub fn library_dir(&self) -> PathBuf {
        self.base_dir.join("library")
    }

    pub fn derived_dir(&self) -> PathBuf {
        self.base_dir.join("derived")
    }

    pub fn derived_items_dir(&self) -> PathBuf {
        self.derived_dir().join("items")
    }

    pub fn derived_jobs_dir(&self) -> PathBuf {
        self.derived_dir().join("jobs")
    }

    pub fn derived_item_dir(&self, item_id: &str) -> PathBuf {
        self.derived_items_dir().join(item_id)
    }

    pub fn job_artifacts_dir(&self, job_id: &str) -> PathBuf {
        self.derived_jobs_dir().join(job_id)
    }

    pub fn db_dir(&self) -> PathBuf {
        self.base_dir.join("db")
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.base_dir.join("logs")
    }

    pub fn job_logs_dir(&self) -> PathBuf {
        self.logs_dir().join("jobs")
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.base_dir.join("cache")
    }

    pub fn download_dir_override_path(&self) -> PathBuf {
        self.config_dir().join("download_dir.txt")
    }

    pub fn default_download_dir(&self) -> PathBuf {
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(parent) = exe_path.parent() {
                return parent.join("downloads");
            }
        }
        self.library_dir().join("downloads")
    }

    pub fn download_dir_override(&self) -> std::io::Result<Option<PathBuf>> {
        let path = self.download_dir_override_path();
        if !path.exists() {
            return Ok(None);
        }

        let raw = std::fs::read_to_string(path)?;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }

        Ok(Some(PathBuf::from(trimmed)))
    }

    pub fn effective_download_dir(&self) -> std::io::Result<PathBuf> {
        if let Some(override_dir) = self.download_dir_override()? {
            return Ok(override_dir);
        }
        Ok(self.default_download_dir())
    }

    pub fn set_download_dir_override(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(self.config_dir())?;
        std::fs::write(
            self.download_dir_override_path(),
            format!("{}\n", dir.to_string_lossy()),
        )?;
        Ok(())
    }

    pub fn clear_download_dir_override(&self) -> std::io::Result<()> {
        let path = self.download_dir_override_path();
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    pub fn models_dir(&self) -> PathBuf {
        self.base_dir.join("models")
    }

    pub fn tools_dir(&self) -> PathBuf {
        self.base_dir.join("tools")
    }

    pub fn ffmpeg_dir(&self) -> PathBuf {
        self.tools_dir().join("ffmpeg")
    }

    pub fn ffmpeg_bin_path(&self) -> PathBuf {
        let mut path = self.ffmpeg_dir().join("ffmpeg");
        if cfg!(windows) {
            path.set_extension("exe");
        }
        path
    }

    pub fn ffprobe_bin_path(&self) -> PathBuf {
        let mut path = self.ffmpeg_dir().join("ffprobe");
        if cfg!(windows) {
            path.set_extension("exe");
        }
        path
    }

    pub fn ffmpeg_cmd(&self) -> PathBuf {
        let path = self.ffmpeg_bin_path();
        if path.exists() {
            path
        } else {
            PathBuf::from("ffmpeg")
        }
    }

    pub fn ffprobe_cmd(&self) -> PathBuf {
        let path = self.ffprobe_bin_path();
        if path.exists() {
            path
        } else {
            PathBuf::from("ffprobe")
        }
    }

    pub fn model_install_dir(&self, model_id: &str, version: &str) -> PathBuf {
        self.models_dir().join(model_id).join(version)
    }

    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(self.config_dir())?;
        std::fs::create_dir_all(self.library_dir())?;
        std::fs::create_dir_all(self.derived_items_dir())?;
        std::fs::create_dir_all(self.derived_jobs_dir())?;
        std::fs::create_dir_all(self.db_dir())?;
        std::fs::create_dir_all(self.logs_dir())?;
        std::fs::create_dir_all(self.job_logs_dir())?;
        std::fs::create_dir_all(self.cache_dir())?;
        std::fs::create_dir_all(self.models_dir())?;
        std::fs::create_dir_all(self.ffmpeg_dir())?;
        Ok(())
    }

    pub fn normalize_base_dir(base_dir: &Path) -> PathBuf {
        // Keep it simple for now; callers should provide an app-specific directory.
        base_dir.to_path_buf()
    }
}
