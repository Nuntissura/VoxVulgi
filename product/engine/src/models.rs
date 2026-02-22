use crate::paths::AppPaths;
use crate::{EngineError, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::{io::Read, io::Write, path::PathBuf};

const BUNDLED_MANIFEST_JSON: &str = include_str!("../resources/models/manifest.json");
const BUNDLED_DEMO_FILE: &[u8] = include_bytes!("../resources/models/bundled/demo/demo.txt");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelManifest {
    pub schema_version: u32,
    pub models: Vec<ModelSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSpec {
    pub id: String,
    pub name: String,
    pub task: String,
    pub source_lang: Option<String>,
    pub target_lang: Option<String>,
    pub version: String,
    pub license: String,
    pub files: Vec<ModelFileSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelFileSpec {
    pub path: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub source: ModelFileSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ModelFileSource {
    Bundled { resource_id: String },
    Url { url: String },
    LocalPath { path: String },
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelInventory {
    pub models_dir: String,
    pub total_installed_bytes: u64,
    pub models: Vec<ModelInventoryItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelInventoryItem {
    pub id: String,
    pub name: String,
    pub task: String,
    pub source_lang: Option<String>,
    pub target_lang: Option<String>,
    pub version: String,
    pub license: String,
    pub installed: bool,
    pub expected_bytes: u64,
    pub installed_bytes: u64,
    pub install_dir: String,
}

#[derive(Debug, Clone)]
pub struct ModelStore {
    paths: AppPaths,
}

impl ModelStore {
    pub fn new(paths: AppPaths) -> Self {
        Self { paths }
    }

    pub fn bundled_manifest(&self) -> Result<ModelManifest> {
        serde_json::from_str(BUNDLED_MANIFEST_JSON)
            .map_err(|e| EngineError::BundledManifestInvalid(e.to_string()))
    }

    pub fn model_spec_by_id(&self, model_id: &str) -> Result<ModelSpec> {
        let manifest = self.bundled_manifest()?;
        manifest
            .models
            .into_iter()
            .find(|m| m.id == model_id)
            .ok_or_else(|| EngineError::UnknownModel(model_id.to_string()))
    }

    pub fn inventory(&self) -> Result<ModelInventory> {
        self.paths.ensure_dirs()?;

        let manifest = self.bundled_manifest()?;
        let mut total_installed_bytes = 0_u64;
        let mut models = Vec::with_capacity(manifest.models.len());

        for model in &manifest.models {
            let expected_bytes: u64 = model.files.iter().map(|f| f.size_bytes).sum();
            let install_dir = self.paths.model_install_dir(&model.id, &model.version);

            let (installed, installed_bytes) = match self.verify_model(model) {
                Ok(_) => {
                    let bytes = directory_size_bytes(&install_dir).unwrap_or(0);
                    (true, bytes)
                }
                Err(_) => (false, 0),
            };

            total_installed_bytes += installed_bytes;
            models.push(ModelInventoryItem {
                id: model.id.clone(),
                name: model.name.clone(),
                task: model.task.clone(),
                source_lang: model.source_lang.clone(),
                target_lang: model.target_lang.clone(),
                version: model.version.clone(),
                license: model.license.clone(),
                installed,
                expected_bytes,
                installed_bytes,
                install_dir: install_dir.to_string_lossy().to_string(),
            });
        }

        Ok(ModelInventory {
            models_dir: self.paths.models_dir().to_string_lossy().to_string(),
            total_installed_bytes,
            models,
        })
    }

    pub fn install_model(&self, model_id: &str) -> Result<()> {
        self.paths.ensure_dirs()?;

        let model = self.model_spec_by_id(model_id)?;
        let install_root = self.paths.model_install_dir(&model.id, &model.version);
        std::fs::create_dir_all(&install_root)?;

        for file in &model.files {
            let out_path = install_root.join(&file.path);
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            match &file.source {
                ModelFileSource::Bundled { resource_id } => {
                    let bytes = bundled_resource(resource_id)?;
                    write_atomic(&out_path, bytes)?;
                }
                ModelFileSource::Url { url } => {
                    download_atomic(url, &out_path, file.size_bytes, &file.sha256)?;
                }
                ModelFileSource::LocalPath { path } => {
                    copy_atomic(Path::new(path), &out_path)?;
                }
            }

            verify_file(&out_path, file.size_bytes, &file.sha256)?;
        }

        // Full model verification after install.
        self.verify_model(&model)?;
        Ok(())
    }

    pub fn install_bundled_model(&self, model_id: &str) -> Result<()> {
        self.install_model(model_id)
    }

    pub fn verify_model_by_id(&self, model_id: &str) -> Result<()> {
        let model = self.model_spec_by_id(model_id)?;
        self.verify_model(&model)
    }

    fn verify_model(&self, model: &ModelSpec) -> Result<()> {
        let install_root = self.paths.model_install_dir(&model.id, &model.version);
        for file in &model.files {
            let path = install_root.join(&file.path);
            verify_file(&path, file.size_bytes, &file.sha256)?;
        }
        Ok(())
    }

    pub fn installed_model_dir(&self, model_id: &str) -> Result<PathBuf> {
        let model = self.model_spec_by_id(model_id)?;
        Ok(self.paths.model_install_dir(&model.id, &model.version))
    }

    pub fn installed_file_path(&self, model_id: &str, relative_path: &str) -> Result<PathBuf> {
        let root = self.installed_model_dir(model_id)?;
        Ok(root.join(relative_path))
    }
}

fn bundled_resource(resource_id: &str) -> Result<&'static [u8]> {
    match resource_id {
        "demo/demo.txt" => Ok(BUNDLED_DEMO_FILE),
        other => Err(EngineError::UnknownBundledResource(other.to_string())),
    }
}

fn verify_file(path: &Path, expected_size: u64, expected_sha256: &str) -> Result<()> {
    let meta = std::fs::metadata(path)?;
    let actual_size = meta.len();
    if actual_size != expected_size {
        return Err(EngineError::SizeMismatch {
            path: path.to_path_buf(),
            expected: expected_size,
            actual: actual_size,
        });
    }

    let actual_sha256 = sha256_file_hex(path)?;
    if !eq_hex_case_insensitive(&actual_sha256, expected_sha256) {
        return Err(EngineError::HashMismatch {
            path: path.to_path_buf(),
            expected: expected_sha256.to_string(),
            actual: actual_sha256,
        });
    }
    Ok(())
}

fn sha256_file_hex(path: &Path) -> Result<String> {
    let mut hasher = Sha256::new();
    let mut file = std::fs::File::open(path)?;
    std::io::copy(&mut file, &mut hasher)?;
    let digest = hasher.finalize();
    Ok(hex::encode(digest))
}

fn eq_hex_case_insensitive(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp_path = path.with_extension("tmp");
    std::fs::write(&tmp_path, bytes)?;
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    std::fs::rename(&tmp_path, path)?;
    Ok(())
}

fn copy_atomic(src: &Path, dst: &Path) -> Result<()> {
    let tmp_path = dst.with_extension("tmp");
    std::fs::copy(src, &tmp_path)?;
    if dst.exists() {
        std::fs::remove_file(dst)?;
    }
    std::fs::rename(&tmp_path, dst)?;
    Ok(())
}

fn download_atomic(url: &str, dst: &Path, expected_size: u64, expected_sha256: &str) -> Result<()> {
    let tmp_path = dst.with_extension("download");

    let resp = ureq::get(url)
        .call()
        .map_err(|e| EngineError::InstallFailed(format!("model download failed: {url} ({e})")))?;

    let status = resp.status();
    if status.as_u16() >= 400 {
        return Err(EngineError::InstallFailed(format!(
            "model download failed: {url} (status={})",
            status
        )));
    }

    let mut reader = resp.into_body().into_reader();
    let mut file = std::fs::File::create(&tmp_path)?;
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buf = [0u8; 1024 * 64];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        hasher.update(&buf[..n]);
        total += n as u64;
    }
    file.flush()?;

    if total != expected_size {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(EngineError::SizeMismatch {
            path: dst.to_path_buf(),
            expected: expected_size,
            actual: total,
        });
    }

    let actual_sha256 = hex::encode(hasher.finalize());
    if !eq_hex_case_insensitive(&actual_sha256, expected_sha256) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(EngineError::HashMismatch {
            path: dst.to_path_buf(),
            expected: expected_sha256.to_string(),
            actual: actual_sha256,
        });
    }

    if dst.exists() {
        std::fs::remove_file(dst)?;
    }
    std::fs::rename(&tmp_path, dst)?;
    Ok(())
}

fn directory_size_bytes(path: &Path) -> std::io::Result<u64> {
    let mut sum = 0_u64;
    if !path.exists() {
        return Ok(0);
    }
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        if meta.is_file() {
            sum += meta.len();
        } else if meta.is_dir() {
            sum += directory_size_bytes(&entry.path())?;
        }
    }
    Ok(sum)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_demo_model_can_install_and_verify() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        let store = ModelStore::new(paths);

        let manifest = store.bundled_manifest().expect("manifest");
        let demo = manifest
            .models
            .iter()
            .find(|m| m.id == "demo-ja-asr")
            .expect("demo model");

        // Should fail before install.
        assert!(store.verify_model(demo).is_err());

        store.install_bundled_model("demo-ja-asr").expect("install");
        store.verify_model_by_id("demo-ja-asr").expect("verify");
    }
}
