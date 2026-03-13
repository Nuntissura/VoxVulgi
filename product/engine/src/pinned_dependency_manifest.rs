use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::sync::OnceLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinnedDependencyManifest {
    pub schema_version: u32,
    pub allow_unpinned_fallback_env: String,
    pub yt_dlp_windows: YtDlpWindowsPin,
    pub portable_python_windows: PortablePythonWindowsPin,
    pub deno_windows: DenoWindowsPin,
    pub spleeter: SpleeterPins,
    pub demucs: SingleSpecPin,
    pub diarization: PythonPackageSet,
    pub tts_preview: PythonPackageSet,
    pub tts_neural_local_v1: NeuralTtsPins,
    pub tts_voice_preserving_local_v1: VoicePreservingPins,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YtDlpWindowsPin {
    pub version: String,
    pub url: String,
    pub sha256_hex: String,
    pub file_bytes: u64,
    pub source_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortablePythonWindowsPin {
    pub version: String,
    pub url: String,
    pub sha256_hex: String,
    pub source_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DenoWindowsPin {
    pub version: String,
    pub url: String,
    pub sha256_hex: String,
    pub file_bytes: u64,
    pub source_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpleeterPins {
    pub bootstrap_packages: Vec<String>,
    pub candidate_pins: SpleeterCandidatePins,
    pub unpinned_fallback_spec: String,
    pub model: SpleeterModelPin,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpleeterCandidatePins {
    pub py38_to_py311: String,
    pub py_lt_38: String,
    pub default_pinned: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpleeterModelPin {
    pub repo: String,
    pub release: String,
    pub model_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingleSpecPin {
    pub pinned_spec: String,
    pub unpinned_fallback_spec: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PythonPackageSet {
    pub pinned: Vec<String>,
    pub unpinned_fallback: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeuralTtsPins {
    pub compatibility_upgrades: Vec<String>,
    pub pinned: Vec<String>,
    pub unpinned_fallback: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoicePreservingPins {
    pub openvoice_git_spec: String,
    pub pinned_dependencies: Vec<String>,
    pub unpinned_fallback_dependencies: Vec<String>,
    pub openvoice_v2: OpenVoiceModelPin,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenVoiceModelPin {
    pub repo_id: String,
    pub revision: String,
    pub files: Vec<PinnedFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinnedFile {
    pub filename: String,
    pub sha256_hex: String,
}

pub fn manifest() -> &'static PinnedDependencyManifest {
    static MANIFEST: OnceLock<PinnedDependencyManifest> = OnceLock::new();
    MANIFEST.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../resources/tooling/pinned_dependency_manifest.json"
        ))
        .expect("pinned dependency manifest must parse")
    })
}

pub fn manifest_json_value() -> serde_json::Value {
    serde_json::to_value(manifest()).expect("pinned dependency manifest must serialize")
}

pub fn allow_unpinned_fallback_env_name() -> &'static str {
    manifest().allow_unpinned_fallback_env.as_str()
}

pub fn allow_unpinned_fallback() -> bool {
    std::env::var_os(allow_unpinned_fallback_env_name())
        .as_ref()
        .map(|value| parse_truthy_env(value))
        .unwrap_or(false)
}

fn parse_truthy_env(value: &OsStr) -> bool {
    let normalized = value.to_string_lossy().trim().to_ascii_lowercase();
    matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_parses_and_contains_expected_sections() {
        let manifest = manifest();
        assert_eq!(manifest.schema_version, 1);
        assert_eq!(
            manifest.allow_unpinned_fallback_env,
            "VOXVULGI_ALLOW_UNPINNED_FALLBACK"
        );
        assert_eq!(manifest.yt_dlp_windows.version, "2026.03.03");
        assert_eq!(manifest.portable_python_windows.version, "3.11.9");
        assert_eq!(manifest.deno_windows.version, "2.7.5");
        assert_eq!(
            manifest
                .tts_voice_preserving_local_v1
                .openvoice_v2
                .files
                .len(),
            3
        );
    }

    #[test]
    fn allow_unpinned_fallback_is_opt_in_only() {
        let env_name = allow_unpinned_fallback_env_name().to_string();
        std::env::remove_var(&env_name);
        assert!(!allow_unpinned_fallback());

        std::env::set_var(&env_name, "true");
        assert!(allow_unpinned_fallback());

        std::env::set_var(&env_name, "0");
        assert!(!allow_unpinned_fallback());

        std::env::remove_var(&env_name);
    }
}
