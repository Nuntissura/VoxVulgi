use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::sync::OnceLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinnedDependencyManifest {
    pub schema_version: u32,
    pub allow_unpinned_fallback_env: String,
    /// WP-0232: per-pack relative path to a hashed lockfile under
    /// `product/engine/resources/tooling/`. Packs without a lockfile entry continue to
    /// use the legacy `pip install <pinned list>` path at install time.
    #[serde(default)]
    pub lockfiles: BTreeMap<String, String>,
    /// Complete, mutually coherent installed-environment locks. Unlike the per-pack
    /// resolver locks above, these cover every distribution in each shipped venv after
    /// all packs have been composed. The full-offline payload validator fails closed
    /// when either referenced lock is missing or the installed inventory drifts.
    #[serde(default)]
    pub offline_python_environment_locks: BTreeMap<String, String>,
    pub yt_dlp_windows: YtDlpWindowsPin,
    pub instagram_profile_provider: StandaloneZipToolPin,
    pub instagram_profile_enumerator: PythonWheelPin,
    pub portable_python_windows: PortablePythonWindowsPin,
    pub deno_windows: DenoWindowsPin,
    pub node_windows: NodeWindowsPin,
    pub youtube_po_provider: YoutubePoProviderPin,
    pub spleeter: SpleeterPins,
    pub demucs: DemucsPin,
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
pub struct StandaloneZipToolPin {
    pub version: String,
    pub url: String,
    pub sha256_hex: String,
    pub file_bytes: u64,
    pub executable_sha256_hex: String,
    pub executable_bytes: u64,
    pub source_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PythonWheelPin {
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
pub struct NodeWindowsPin {
    pub version: String,
    pub npm_version: String,
    pub node_exe_sha256_hex: String,
    pub npm_cmd_sha256_hex: String,
    pub complete_tree_sha256_hex: String,
    pub url: String,
    pub sha256_hex: String,
    pub file_bytes: u64,
    pub source_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoutubePoProviderPin {
    pub version: String,
    pub plugin_url: String,
    pub plugin_sha256_hex: String,
    pub plugin_file_bytes: u64,
    pub plugin_tree_sha256_hex: String,
    pub plugin_files_sha256: BTreeMap<String, String>,
    pub source_commit: String,
    pub source_url: String,
    pub source_sha256_hex: String,
    pub source_file_bytes: u64,
    pub derived_lock_resource: String,
    pub derived_lock_sha256_hex: String,
    pub node_modules_tree_sha256_hex: String,
    pub server_entrypoint_sha256_hex: String,
    pub application_complete_tree_sha256_hex: String,
    pub canvas_prebuilt_url: String,
    pub canvas_prebuilt_sha256_hex: String,
    pub canvas_prebuilt_file_bytes: u64,
    pub source_label: String,
}

pub const YOUTUBE_PO_PROVIDER_DERIVED_LOCK: &str =
    include_str!("../resources/tooling/youtube_po_provider_1_3_1.package-lock.json");

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
pub struct DemucsPin {
    pub pinned_spec: String,
    pub unpinned_fallback_spec: String,
    pub model_url: String,
    pub model_relative_path: String,
    pub model_file_bytes: u64,
    pub model_sha256_hex: String,
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
    pub spacy_model: PythonWheelPin,
    pub kokoro_model: HuggingFaceModelPin,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HuggingFaceModelPin {
    pub repo_id: String,
    pub revision: String,
    pub files: Vec<SizedPinnedFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SizedPinnedFile {
    pub filename: String,
    pub file_bytes: u64,
    pub sha256_hex: String,
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
        assert_eq!(manifest.schema_version, 3);
        assert_eq!(
            manifest.allow_unpinned_fallback_env,
            "VOXVULGI_ALLOW_UNPINNED_FALLBACK"
        );
        assert_eq!(manifest.yt_dlp_windows.version, "2026.07.04");
        assert_eq!(manifest.instagram_profile_provider.version, "4.15.3");
        assert_eq!(
            manifest.yt_dlp_windows.sha256_hex,
            "52FE3C26DCF71FBDC85B528589020BB0B8E383155CFA81B64DD447BBE35E24B8"
        );
        assert_eq!(manifest.portable_python_windows.version, "3.11.9");
        assert_eq!(
            manifest
                .offline_python_environment_locks
                .get("main_windows_x64_cp311")
                .map(String::as_str),
            Some("final_environment_locks/main_windows_x64_cp311.lock.json")
        );
        assert_eq!(
            manifest
                .offline_python_environment_locks
                .get("cosyvoice_windows_x64_cp311")
                .map(String::as_str),
            Some("final_environment_locks/cosyvoice_windows_x64_cp311.lock.json")
        );
        assert_eq!(manifest.deno_windows.version, "2.7.5");
        assert_eq!(manifest.node_windows.version, "24.19.0");
        assert_eq!(manifest.node_windows.npm_version, "11.17.0");
        assert_eq!(
            manifest.node_windows.complete_tree_sha256_hex,
            "2D0906A746B7AB1280DDCF6B3C884068FDE9C00FB647BC31DDD1EB82BBC50FBB"
        );
        assert_eq!(manifest.youtube_po_provider.version, "1.3.1");
        assert_eq!(
            manifest.youtube_po_provider.canvas_prebuilt_file_bytes,
            15_235_616
        );
        assert_eq!(
            manifest.youtube_po_provider.canvas_prebuilt_sha256_hex,
            "BA953CC8C38303AB94CC83461C7561506A5A3AF37D6C678DE4A47914D2D0BB48"
        );
        assert_eq!(manifest.demucs.model_file_bytes, 84_141_911);
        assert_eq!(
            manifest.demucs.model_sha256_hex,
            "8726E21A993978C7BA086D3872E7608D7D5BFCA646CA4ACA459FFDA844FAA8B4"
        );
        assert_eq!(
            manifest.youtube_po_provider.derived_lock_sha256_hex,
            "1716EE78267544F03D64AC1CCBB365C718B9CC667BD965406311076AE849F8B4"
        );
        assert_eq!(
            manifest.youtube_po_provider.node_modules_tree_sha256_hex,
            "28ED8B72BE8F8AFF827B94ABDF3722F0A1DEE1B3CEAA575ADB561A9AFD22A486"
        );
        assert_eq!(
            manifest
                .youtube_po_provider
                .application_complete_tree_sha256_hex,
            "28A699B7B70B8D5B5F8B1ADB2848BBFBF60F90319D4F9CE0643EE3303957D684"
        );
        assert!(manifest
            .diarization
            .pinned
            .iter()
            .any(|pin| pin == "numba==0.66.0"));
        assert!(manifest
            .diarization
            .pinned
            .iter()
            .any(|pin| pin == "llvmlite==0.48.0"));
        assert_eq!(
            manifest
                .tts_voice_preserving_local_v1
                .openvoice_v2
                .files
                .len(),
            3
        );
        // WP-0231: Kokoro group must pin transformers + huggingface_hub so the venv state
        // stays coherent at every install stage, not only after OpenVoice's pin runs.
        assert!(manifest
            .tts_neural_local_v1
            .pinned
            .iter()
            .any(|pin| pin == "huggingface_hub==0.34.4"));
        assert!(manifest
            .tts_neural_local_v1
            .pinned
            .iter()
            .any(|pin| pin == "transformers==4.49.0"));
        assert_eq!(manifest.tts_neural_local_v1.spacy_model.version, "3.8.0");
        assert_eq!(
            manifest.tts_neural_local_v1.kokoro_model.revision,
            "f3ff3571791e39611d31c381e3a41a3af07b4987"
        );
        assert_eq!(manifest.tts_neural_local_v1.kokoro_model.files.len(), 3);
        // hf_hub pin must match between Kokoro and OpenVoice groups so neither one
        // downgrades the venv on top of the other.
        let openvoice_hf = manifest
            .tts_voice_preserving_local_v1
            .pinned_dependencies
            .iter()
            .find(|pin| pin.starts_with("huggingface_hub=="))
            .expect("OpenVoice must pin huggingface_hub");
        assert_eq!(openvoice_hf, "huggingface_hub==0.34.4");
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
