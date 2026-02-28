use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("failed to parse bundled model manifest: {0}")]
    BundledManifestInvalid(String),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("unknown bundled resource id: {0}")]
    UnknownBundledResource(String),

    #[error("unknown model id: {0}")]
    UnknownModel(String),

    #[error("model contains unsupported source types; only bundled sources are supported for now")]
    UnsupportedModelSource,

    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("csv error: {0}")]
    Csv(#[from] csv::Error),

    #[error("external tool is missing: {tool}")]
    ExternalToolMissing { tool: String },

    #[error("external tool failed: {tool} (code={code:?}) {stderr}")]
    ExternalToolFailed {
        tool: String,
        code: Option<i32>,
        stderr: String,
    },

    #[error("hash mismatch for {path}: expected {expected}, got {actual}")]
    HashMismatch {
        path: PathBuf,
        expected: String,
        actual: String,
    },

    #[error("size mismatch for {path}: expected {expected} bytes, got {actual} bytes")]
    SizeMismatch {
        path: PathBuf,
        expected: u64,
        actual: u64,
    },

    #[error("model/tool install failed: {0}")]
    InstallFailed(String),
}

pub type Result<T> = std::result::Result<T, EngineError>;
