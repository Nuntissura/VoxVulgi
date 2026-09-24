use crate::models::ModelStore;
use crate::paths::AppPaths;
use crate::{cmd, pinned_dependency_manifest, tools, EngineError, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, OpenOptions};
use std::io::{BufReader, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const RECEIPT_SCHEMA: &str = "voxvulgi.offline_payload_validation.v1";
pub const TREE_IDENTITY_CONTRACT: &str = "sha256_records_v1";
const COSYVOICE_MODEL_MANIFEST: &str =
    include_str!("../resources/tooling/cosyvoice_model_manifest.json");
const COSYVOICE_MODEL_MANIFEST_SHA256: &str =
    "3730f28cf1d7c31e663fb1823b4d1856aba04be7fda68c95f0536d9ea47c5374";
const COSYVOICE_MODEL_REPO: &str = "FunAudioLLM/CosyVoice2-0.5B";
const COSYVOICE_MODEL_REVISION: &str = "eec1ae6c79877dbd9379285cf8789c9e0879293d";
const COSYVOICE_WETEXT_REPO: &str = "pengzhendong/wetext";
const COSYVOICE_WETEXT_REVISION: &str = "b04bc07588601f7619b20efbb01cd1fa7278ccbc";

#[derive(Debug, Clone)]
pub struct ValidationRequest {
    pub stage_base_dir: PathBuf,
    pub export_dir: PathBuf,
    pub receipt_path: PathBuf,
    pub app_version: String,
    pub external_source_lock: Option<ExternalSourceLockRequest>,
}

#[derive(Debug, Clone)]
pub struct ExternalSourceLockRequest {
    pub path: PathBuf,
    pub owner_pid: u32,
    pub transaction_id: String,
    pub token_sha256: String,
    pub record_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfflinePayloadValidationReceipt {
    pub schema: String,
    pub outcome: String,
    pub generated_at_ms: i64,
    pub app_version: String,
    pub inputs: ValidationInputs,
    pub source_bindings: Vec<SourceBinding>,
    pub manifest: PayloadManifestBinding,
    pub trees: BTreeMap<String, TreeIdentity>,
    pub checks: serde_json::Value,
    pub stage_export_equal: StageExportEquality,
    pub source_lock: SourceLockAttestation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reconciliation: Option<ReconciliationAttestation>,
    pub validation_window: ValidationWindow,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationInputs {
    pub stage_base_dir: String,
    pub payload_dir: String,
    pub cosyvoice_venv_dir: String,
    pub voice_backends_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceBinding {
    pub id: String,
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayloadManifestBinding {
    pub path: String,
    pub sha256: String,
    pub schema_version: u64,
    pub bundle_id: String,
    pub created_at_ms: i64,
    pub payload_format: String,
    pub payload_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct TreeIdentity {
    pub path: String,
    pub file_count: u64,
    pub directory_count: u64,
    pub byte_count: u64,
    pub empty_file_count: u64,
    pub tree_sha256: String,
    pub identity_contract: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct CosyvoiceModelManifest {
    schema: String,
    cosyvoice: CosyvoiceModelSource,
    wetext: CosyvoiceModelSource,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct CosyvoiceModelSource {
    provider: String,
    repo: String,
    revision: String,
    #[serde(default)]
    directories: Vec<String>,
    files: Vec<CosyvoiceModelFile>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct CosyvoiceModelFile {
    path: String,
    bytes: u64,
    sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageExportEquality {
    pub tools: bool,
    pub models: bool,
    pub huggingface: bool,
    pub overall: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceLockAttestation {
    pub contract: String,
    pub path: String,
    pub ownership: String,
    pub owner_pid: u32,
    pub validator_pid: u32,
    pub transaction_id: String,
    pub token_sha256: String,
    pub record_sha256: String,
    pub record_bytes_verified: bool,
    pub exclusive_probe_blocked: bool,
    pub owner_alive_at_start: bool,
    pub owner_alive_at_final_rehash: bool,
    pub scope: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationWindow {
    pub contract: String,
    pub initial_tree_sha256: BTreeMap<String, String>,
    pub final_tree_sha256: BTreeMap<String, String>,
    pub initial_lock_set_sha256: String,
    pub final_lock_set_sha256: String,
    pub initial_manifest_sha256: String,
    pub final_manifest_sha256: String,
    pub final_rehash_matched_initial: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconciliationAttestation {
    pub contract: String,
    pub transaction_id: String,
    pub journal_path: String,
    pub journal_sha256: String,
    pub journal_file_id: String,
    pub journal_bytes_verified_at_initial_hash: bool,
    pub journal_bytes_verified_at_final_rehash: bool,
    pub source_lock_identity_matched: bool,
    pub workspace_roots_outside_captured_payload_trees: bool,
    pub legacy_inline_owner_markers_absent: bool,
    pub workspace_roots: BTreeMap<String, ReconciliationWorkspaceRoot>,
    pub unit_workspaces: BTreeMap<String, ReconciliationUnitWorkspace>,
    pub initial_tree_sha256: BTreeMap<String, String>,
    pub final_tree_sha256: BTreeMap<String, String>,
    pub initial_lock_set_sha256: String,
    pub final_lock_set_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct ReconciliationObjectIdentity {
    pub path: String,
    pub volume_serial: String,
    pub file_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconciliationWorkspaceRoot {
    pub collection_root: ReconciliationObjectIdentity,
    pub transaction_root: ReconciliationObjectIdentity,
    pub owner: ReconciliationObjectIdentity,
    pub rename_identity_verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconciliationUnitWorkspace {
    pub target: ReconciliationObjectIdentity,
    pub workspace: ReconciliationObjectIdentity,
    pub owner: ReconciliationObjectIdentity,
    pub original_existed: bool,
    pub original_backup: Option<ReconciliationObjectIdentity>,
}

#[derive(Debug, Clone, Serialize)]
struct RequiredArtifact {
    id: String,
    path: String,
    bytes: u64,
    sha256: String,
}

#[derive(Debug, Clone, Serialize)]
struct LockfileCheck {
    pack: String,
    path: String,
    packages: usize,
    fully_hashed: bool,
    wheel_or_sdist_urls_present: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FinalEnvironmentLock {
    schema: String,
    environment: String,
    python_tag: String,
    indexes: Vec<String>,
    packages: Vec<FinalEnvironmentPackage>,
    requirements_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FinalEnvironmentPackage {
    name: String,
    version: String,
    wheel_filename: String,
    wheel_bytes: u64,
    wheel_sha256: String,
}

#[derive(Debug, Clone, Serialize)]
struct FinalEnvironmentCheck {
    environment: String,
    lock_path: String,
    lock_sha256: String,
    package_count: usize,
    pip_inspect_version: String,
    pip_version: String,
    installed_inventory_sha256: String,
    freeze_sha256: String,
    exact_freeze_only: bool,
    inventory_equal: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct ExportManifest {
    schema_version: u64,
    bundle_id: String,
    created_at_ms: i64,
    payload_format: String,
    payload_bytes: u64,
}

#[derive(Debug, Clone, Copy)]
enum TreePolicy {
    Exact,
    ExportedTools,
}

#[derive(Debug, Clone)]
struct PayloadTreeSnapshot {
    stage_tools: TreeIdentity,
    export_tools: TreeIdentity,
    stage_models: TreeIdentity,
    export_models: TreeIdentity,
    stage_huggingface: TreeIdentity,
    export_huggingface: TreeIdentity,
    cosyvoice_venv: TreeIdentity,
    voice_backends: TreeIdentity,
    lock_set: TreeIdentity,
}

impl PayloadTreeSnapshot {
    fn hashes(&self) -> BTreeMap<String, String> {
        BTreeMap::from([
            (
                "stage_tools".to_string(),
                self.stage_tools.tree_sha256.clone(),
            ),
            (
                "payload_tools".to_string(),
                self.export_tools.tree_sha256.clone(),
            ),
            (
                "stage_models".to_string(),
                self.stage_models.tree_sha256.clone(),
            ),
            (
                "payload_models".to_string(),
                self.export_models.tree_sha256.clone(),
            ),
            (
                "stage_huggingface".to_string(),
                self.stage_huggingface.tree_sha256.clone(),
            ),
            (
                "payload_huggingface".to_string(),
                self.export_huggingface.tree_sha256.clone(),
            ),
            (
                "cosyvoice_venv".to_string(),
                self.cosyvoice_venv.tree_sha256.clone(),
            ),
            (
                "voice_backends".to_string(),
                self.voice_backends.tree_sha256.clone(),
            ),
        ])
    }
}

struct SourceLockGuard {
    _owned_file: Option<File>,
    attestation: SourceLockAttestation,
}

#[derive(Debug, Deserialize)]
struct SourceLockRecord {
    schema: String,
    transaction_id: String,
    owner_pid: u32,
    token_sha256: String,
    scope: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReconciliationJournal {
    schema: String,
    transaction_id: String,
    owner_pid: u32,
    source_lock_path: String,
    source_lock_token_sha256: String,
    source_lock_record_sha256: String,
    app_version: String,
    journal_path: String,
    receipt_path: String,
    validation_receipt_sha256: Option<String>,
    validation_journal_sha256: Option<String>,
    validation_journal_file_id: Option<String>,
    state: String,
    committed: bool,
    cleanup_state: String,
    cleanup_intent: Option<String>,
    created_at_utc: String,
    updated_at_utc: String,
    committed_at_utc: Option<String>,
    rolled_back_at_utc: Option<String>,
    workspace_roots: Vec<ReconciliationJournalRoot>,
    units: Vec<ReconciliationJournalUnit>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReconciliationJournalRoot {
    name: String,
    collection_root: String,
    collection_root_id: String,
    transaction_root: String,
    transaction_root_id: String,
    rename_identity_verified: bool,
    owner_path: String,
    owner_file_id: String,
    trusted_root: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReconciliationJournalUnit {
    name: String,
    target: String,
    collection_root: String,
    transaction_root: String,
    workspace: String,
    owner_path: String,
    owner_file_id: String,
    prepared: String,
    backup: String,
    discard: String,
    trusted_root: String,
    workspace_id: String,
    prepared_directory_id: String,
    original_existed: bool,
    original_directory_id: Option<String>,
    mutation_started: bool,
    state: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReconciliationRootOwner {
    schema: String,
    transaction_id: String,
    name: String,
    collection_root: String,
    collection_root_id: String,
    transaction_root: String,
    transaction_root_id: String,
    rename_identity_verified: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReconciliationUnitOwner {
    schema: String,
    transaction_id: String,
    unit: String,
    workspace: String,
    workspace_id: String,
    target: String,
    prepared: String,
    backup: String,
    discard: String,
    prepared_directory_id: String,
    original_existed: bool,
    original_directory_id: Option<String>,
}

struct VerifiedReconciliationBinding {
    journal_path: PathBuf,
    journal_sha256: String,
    journal_file_id: String,
    attestation_roots: BTreeMap<String, ReconciliationWorkspaceRoot>,
    attestation_units: BTreeMap<String, ReconciliationUnitWorkspace>,
}

pub fn validate_and_write(request: ValidationRequest) -> Result<OfflinePayloadValidationReceipt> {
    if request.app_version.trim().is_empty() {
        return fail("--app-version must not be empty");
    }
    if request.receipt_path.exists() {
        return fail(format!(
            "refusing to replace an existing payload-validation receipt: {}",
            request.receipt_path.display()
        ));
    }

    apply_offline_environment();
    let stage_base = canonical_directory(&request.stage_base_dir, "stage base")?;
    let payload_dir = canonical_directory(&request.export_dir, "exported payload")?;
    let mut source_lock =
        SourceLockGuard::acquire(&stage_base, request.external_source_lock.as_ref())?;
    let cosyvoice_venv = canonical_directory(
        &stage_base
            .join("tools")
            .join("python")
            .join("venv_cosyvoice"),
        "CosyVoice venv",
    )?;
    let voice_backends = canonical_directory(&stage_base.join("voice_backends"), "voice backends")?;
    let paths = AppPaths::new(stage_base.clone());
    let reconciliation_binding = if let Some(external) = request.external_source_lock.as_ref() {
        let captured_roots = [
            canonical_directory(&paths.tools_dir(), "stage tools")?,
            canonical_directory(&payload_dir.join("tools"), "payload tools")?,
            canonical_directory(&paths.models_dir(), "stage models")?,
            canonical_directory(&payload_dir.join("models"), "payload models")?,
            canonical_directory(&paths.huggingface_cache_dir(), "stage Hugging Face cache")?,
            canonical_directory(
                &payload_dir.join("cache").join("huggingface"),
                "payload Hugging Face cache",
            )?,
            cosyvoice_venv.clone(),
            voice_backends.clone(),
            canonical_directory(
                &repo_root().join("product/engine/resources/tooling/final_environment_locks"),
                "final environment lock set",
            )?,
        ];
        Some(verify_external_reconciliation_binding(
            &stage_base,
            &payload_dir,
            &repo_root(),
            &request.receipt_path,
            request.app_version.trim(),
            external,
            &captured_roots
                .iter()
                .map(PathBuf::as_path)
                .collect::<Vec<_>>(),
        )?)
    } else {
        None
    };

    let final_environment_checks = validate_final_environment_locks(&paths)?;

    let manifest_path = payload_dir.join("manifest.json");
    let manifest_artifact = required_file("payload_manifest", &manifest_path)?;
    let export_manifest: ExportManifest = serde_json::from_slice(&std::fs::read(&manifest_path)?)?;
    if export_manifest.schema_version != 1
        || export_manifest.payload_format != "directory"
        || !export_manifest.bundle_id.starts_with("offline_full_win64_")
        || export_manifest.created_at_ms <= 0
        || export_manifest.payload_bytes == 0
    {
        return fail("export manifest identity or payload metadata is invalid");
    }
    for forbidden in ["payload.zip", "payload_inputs.json"] {
        if payload_dir.join(forbidden).exists() {
            return fail(format!(
                "exported payload contains forbidden stale/generated file {forbidden}"
            ));
        }
    }

    let initial_snapshot =
        capture_payload_tree_snapshot(&paths, &payload_dir, &cosyvoice_venv, &voice_backends)?;
    let cosyvoice_model_manifest = parsed_cosyvoice_model_manifest()?;
    let cosy_model = paths.cosyvoice_model_parent_dir().join("CosyVoice2-0.5B");
    let cosy_wetext = paths.cosyvoice_backend_dir().join("wetext");
    let initial_cosyvoice_model = validate_exact_cosyvoice_model_tree(
        &cosy_model,
        &cosyvoice_model_manifest.cosyvoice,
        "CosyVoice2-0.5B model",
    )?;
    let initial_cosyvoice_wetext = validate_exact_cosyvoice_model_tree(
        &cosy_wetext,
        &cosyvoice_model_manifest.wetext,
        "CosyVoice wetext model",
    )?;

    let tools_equal = comparable_tree(&initial_snapshot.stage_tools)
        == comparable_tree(&initial_snapshot.export_tools);
    let models_equal = comparable_tree(&initial_snapshot.stage_models)
        == comparable_tree(&initial_snapshot.export_models);
    let huggingface_equal = comparable_tree(&initial_snapshot.stage_huggingface)
        == comparable_tree(&initial_snapshot.export_huggingface);
    if !tools_equal || !models_equal || !huggingface_equal {
        return fail(format!(
            "fresh stage/export tree mismatch: tools={tools_equal} models={models_equal} huggingface={huggingface_equal}"
        ));
    }
    let observed_payload_bytes = initial_snapshot
        .export_tools
        .byte_count
        .saturating_add(initial_snapshot.export_models.byte_count)
        .saturating_add(initial_snapshot.export_huggingface.byte_count);
    if export_manifest.payload_bytes != observed_payload_bytes {
        return fail(format!(
            "manifest payload_bytes mismatch: expected {} observed {}",
            export_manifest.payload_bytes, observed_payload_bytes
        ));
    }

    let ffmpeg = tools::ffmpeg_tools_status(&paths);
    if !ffmpeg.installed || ffmpeg.ffmpeg_version.is_none() || ffmpeg.ffprobe_version.is_none() {
        return fail("FFmpeg and ffprobe are not both runnable from the prepared stage");
    }
    let portable = tools::portable_python_status(&paths);
    if !portable.installed || portable.python_version.is_none() {
        return fail("portable Python is missing or not runnable");
    }
    let python = tools::python_toolchain_status(&paths);
    if !python.base_available
        || !python.venv_exists
        || python.venv_python_version.is_none()
        || python.venv_pip_version.is_none()
    {
        return fail("main managed Python venv is incomplete or not runnable");
    }

    let spleeter = tools::spleeter_pack_status(&paths);
    if !spleeter.installed || !spleeter.models_installed {
        return fail("Spleeter fallback separation pack or model is incomplete");
    }
    let demucs = tools::demucs_pack_status(&paths);
    let expected_demucs = pinned_dependency_manifest::manifest()
        .demucs
        .pinned_spec
        .split_once("==")
        .map(|(_, version)| version)
        .ok_or_else(|| EngineError::InstallFailed("invalid pinned Demucs spec".to_string()))?;
    if !demucs.installed || demucs.demucs_version.as_deref() != Some(expected_demucs) {
        return fail(format!(
            "Demucs pack does not match the pinned version {expected_demucs}: {:?}",
            demucs.demucs_version
        ));
    }
    let diarization = tools::diarization_pack_status(&paths);
    if !diarization.installed || diarization.repair_required {
        return fail(format!(
            "diarization pack is incomplete or lock-drifted: {}",
            diarization.status_detail
        ));
    }
    let preview = tools::tts_preview_pack_status(&paths);
    if !preview.installed {
        return fail("preview TTS pack is incomplete");
    }
    let neural = tools::tts_neural_local_v1_pack_status(&paths);
    if !neural.installed || neural.repair_required || !neural.version_mismatches.is_empty() {
        return fail(format!(
            "neural TTS pack is incomplete or lock-drifted: {}",
            neural.status_detail
        ));
    }
    let voice = tools::tts_voice_preserving_local_v1_pack_status(&paths);
    if !voice.installed || voice.repair_required || !voice.version_mismatches.is_empty() {
        return fail(format!(
            "voice-preserving pack is incomplete or lock-drifted: {}",
            voice.status_detail
        ));
    }
    let cosyvoice = tools::cosyvoice_pack_status(&paths);
    if !cosyvoice.installed {
        return fail(format!(
            "CosyVoice is incomplete: {}",
            cosyvoice.status_detail
        ));
    }

    let model_store = ModelStore::new(paths.clone());
    let primary_model = AppPaths::DEFAULT_ASR_MODEL_ID;
    model_store.verify_model_by_id(primary_model)?;
    model_store.verify_model_by_id("whispercpp-tiny")?;

    let mut artifacts = vec![
        required_file("ffmpeg", &paths.ffmpeg_bin_path())?,
        required_file("ffprobe", &paths.ffprobe_bin_path())?,
        required_file("portable_python", &paths.python_portable_python_exe())?,
        required_file("main_python", &tools::python_venv_python_path(&paths)?)?,
        required_file(
            "cosyvoice_python",
            &tools::cosyvoice_venv_python_path(&paths)?,
        )?,
        required_file(
            "asr_primary",
            &model_store.installed_file_path(primary_model, "ggml-large-v3-q5_0.bin")?,
        )?,
        required_file(
            "asr_fallback",
            &model_store.installed_file_path("whispercpp-tiny", "ggml-tiny.bin")?,
        )?,
    ];

    let demucs_weights = required_weight_files(
        &paths.python_models_dir().join("demucs"),
        &["th", "pt", "pth", "ckpt", "safetensors"],
        1_000_000,
        "Demucs",
    )?;
    artifacts.extend(demucs_weights.iter().cloned());

    let kokoro_root = paths
        .cache_dir()
        .join("huggingface")
        .join("hub")
        .join("models--hexgrad--Kokoro-82M");
    let kokoro_revision = std::fs::read_to_string(kokoro_root.join("refs").join("main"))?
        .trim()
        .to_string();
    if kokoro_revision.is_empty()
        || !kokoro_revision
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return fail("Kokoro refs/main is empty or unsafe");
    }
    let kokoro_snapshot = kokoro_root.join("snapshots").join(&kokoro_revision);
    artifacts.push(required_file(
        "kokoro_config",
        &kokoro_snapshot.join("config.json"),
    )?);
    artifacts.push(required_file(
        "kokoro_weights",
        &kokoro_snapshot.join("kokoro-v1_0.pth"),
    )?);
    artifacts.push(required_file(
        "kokoro_default_voice",
        &kokoro_snapshot.join("voices").join("af_heart.pt"),
    )?);

    for file in &pinned_dependency_manifest::manifest()
        .tts_voice_preserving_local_v1
        .openvoice_v2
        .files
    {
        let artifact = required_file(
            &format!("openvoice_{}", file.filename.replace(['/', '\\'], "_")),
            &paths
                .python_models_dir()
                .join("openvoice_v2")
                .join(&file.filename),
        )?;
        if !artifact.sha256.eq_ignore_ascii_case(&file.sha256_hex) {
            return fail(format!(
                "OpenVoice pinned hash mismatch for {}: expected {} observed {}",
                file.filename, file.sha256_hex, artifact.sha256
            ));
        }
        artifacts.push(artifact);
    }

    for (id, relative) in [
        ("cosyvoice_render", "voxvulgi_cosyvoice_render.py"),
        ("cosyvoice_api", "cosyvoice/cli/cosyvoice.py"),
        (
            "cosyvoice_matcha",
            "third_party/Matcha-TTS/matcha/models/matcha_tts.py",
        ),
    ] {
        artifacts.push(required_file(
            id,
            &paths.cosyvoice_backend_dir().join(relative),
        )?);
    }

    let lockfile_checks = validate_lockfiles()?;
    let main_smoke = run_python_smoke(&tools::python_venv_python_path(&paths)?, false, &paths)?;
    let cosyvoice_smoke =
        run_python_smoke(&tools::cosyvoice_venv_python_path(&paths)?, true, &paths)?;
    let main_cuda_build = main_smoke
        .get("torch_cuda_build")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty());
    let cosyvoice_cuda_build = cosyvoice_smoke
        .get("torch_cuda_build")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty());
    if main_cuda_build.is_none() {
        return fail(
            "the main prepared Torch runtime is a CPU-only build; the governed full-quality CUDA tier is not bundled",
        );
    }

    let source_bindings = collect_source_bindings()?;
    let cosyvoice_manifest_binding = source_bindings
        .iter()
        .find(|binding| binding.id == "cosyvoice_model_manifest")
        .ok_or_else(|| {
            EngineError::InstallFailed(
                "CosyVoice model manifest source binding is missing".to_string(),
            )
        })?;
    if cosyvoice_manifest_binding.sha256 != COSYVOICE_MODEL_MANIFEST_SHA256 {
        return fail(format!(
            "CosyVoice model manifest source/embedded identity mismatch: expected {} observed {}",
            COSYVOICE_MODEL_MANIFEST_SHA256, cosyvoice_manifest_binding.sha256
        ));
    }

    let checks = serde_json::json!({
        "ffmpeg": ffmpeg,
        "portable_python": portable,
        "python_toolchain": python,
        "packs": {
            "spleeter": spleeter,
            "demucs": demucs,
            "diarization": diarization,
            "tts_preview": preview,
            "tts_neural_local_v1": neural,
            "tts_voice_preserving_local_v1": voice,
            "cosyvoice": cosyvoice,
        },
        "asr_and_translation": {
            "default_model_id": primary_model,
            "fallback_model_id": "whispercpp-tiny",
            "default_and_fallback_bytes_verified_against_model_manifest": true,
            "translation_path": "whisper_cpp_translate_to_english",
        },
        "demucs_weights": demucs_weights,
        "kokoro_revision": kokoro_revision,
        "cosyvoice_model_payload": {
            "manifest_schema": cosyvoice_model_manifest.schema.clone(),
            "manifest_sha256": COSYVOICE_MODEL_MANIFEST_SHA256,
            "cosyvoice": {
                "provider": cosyvoice_model_manifest.cosyvoice.provider.clone(),
                "repo": cosyvoice_model_manifest.cosyvoice.repo.clone(),
                "revision": cosyvoice_model_manifest.cosyvoice.revision.clone(),
                "tree": initial_cosyvoice_model.clone(),
            },
            "wetext": {
                "provider": cosyvoice_model_manifest.wetext.provider.clone(),
                "repo": cosyvoice_model_manifest.wetext.repo.clone(),
                "revision": cosyvoice_model_manifest.wetext.revision.clone(),
                "tree": initial_cosyvoice_wetext.clone(),
            },
            "complete_exact_tree_validation": true,
        },
        "lockfiles": lockfile_checks,
        "final_environment_locks": final_environment_checks,
        "required_artifacts": artifacts,
        "runtime_smoke": {
            "main": main_smoke,
            "cosyvoice": cosyvoice_smoke,
        },
        "tier_coverage": {
            "cpu_fallback": {
                "validated": true,
                "basis": ["whisper.cpp primary and tiny models", "CPU Torch tensor smoke", "Demucs", "diarization", "Kokoro/OpenVoice", "CosyVoice"]
            },
            "full_quality_cuda": {
                "validated": true,
                "main_torch_cuda_build": main_cuda_build,
                "cosyvoice_torch_cuda_build": cosyvoice_cuda_build,
                "gpu_voice_backend": "main-venv OpenVoice",
                "hardware_presence_required_at_build_time": false
            }
        }
    });

    source_lock.assert_held_for_final_rehash()?;
    if let Some(binding) = reconciliation_binding.as_ref() {
        assert_reconciliation_binding_unchanged(binding)?;
    }
    let final_cosyvoice_model = validate_exact_cosyvoice_model_tree(
        &cosy_model,
        &cosyvoice_model_manifest.cosyvoice,
        "CosyVoice2-0.5B model final rehash",
    )?;
    let final_cosyvoice_wetext = validate_exact_cosyvoice_model_tree(
        &cosy_wetext,
        &cosyvoice_model_manifest.wetext,
        "CosyVoice wetext model final rehash",
    )?;
    let final_snapshot =
        capture_payload_tree_snapshot(&paths, &payload_dir, &cosyvoice_venv, &voice_backends)?;
    let final_manifest_artifact = required_file("payload_manifest_final_rehash", &manifest_path)?;
    source_lock.assert_held_for_final_rehash()?;
    if let Some(binding) = reconciliation_binding.as_ref() {
        assert_reconciliation_binding_unchanged(binding)?;
    }
    let mut initial_hashes = initial_snapshot.hashes();
    insert_cosyvoice_tree_hashes(
        &mut initial_hashes,
        &initial_cosyvoice_model,
        &initial_cosyvoice_wetext,
    );
    let mut final_hashes = final_snapshot.hashes();
    insert_cosyvoice_tree_hashes(
        &mut final_hashes,
        &final_cosyvoice_model,
        &final_cosyvoice_wetext,
    );
    ensure_validation_window_unchanged(
        &initial_hashes,
        &final_hashes,
        &initial_snapshot.lock_set.tree_sha256,
        &final_snapshot.lock_set.tree_sha256,
    )?;
    if manifest_artifact.bytes != final_manifest_artifact.bytes
        || manifest_artifact.sha256 != final_manifest_artifact.sha256
    {
        return fail("payload manifest changed inside the exclusive validation window");
    }
    let mut trees = BTreeMap::new();
    trees.insert("stage_tools".to_string(), final_snapshot.stage_tools);
    trees.insert("payload_tools".to_string(), final_snapshot.export_tools);
    trees.insert("stage_models".to_string(), final_snapshot.stage_models);
    trees.insert("payload_models".to_string(), final_snapshot.export_models);
    trees.insert(
        "stage_huggingface".to_string(),
        final_snapshot.stage_huggingface,
    );
    trees.insert(
        "payload_huggingface".to_string(),
        final_snapshot.export_huggingface,
    );
    trees.insert("cosyvoice_venv".to_string(), final_snapshot.cosyvoice_venv);
    trees.insert("voice_backends".to_string(), final_snapshot.voice_backends);
    insert_cosyvoice_tree_identities(&mut trees, final_cosyvoice_model, final_cosyvoice_wetext);
    trees.insert(
        "final_environment_lock_set".to_string(),
        final_snapshot.lock_set.clone(),
    );
    let validation_window = ValidationWindow {
        contract: "exclusive_source_lock_double_sha256_v1".to_string(),
        initial_tree_sha256: initial_hashes,
        final_tree_sha256: final_hashes,
        initial_lock_set_sha256: initial_snapshot.lock_set.tree_sha256,
        final_lock_set_sha256: final_snapshot.lock_set.tree_sha256,
        initial_manifest_sha256: manifest_artifact.sha256.clone(),
        final_manifest_sha256: final_manifest_artifact.sha256,
        final_rehash_matched_initial: true,
    };
    let reconciliation = reconciliation_binding.map(|binding| ReconciliationAttestation {
        contract: "external_transaction_workspace_v1".to_string(),
        transaction_id: request
            .external_source_lock
            .as_ref()
            .expect("external reconciliation binding requires source lock")
            .transaction_id
            .clone(),
        journal_path: display_canonical(&binding.journal_path),
        journal_sha256: binding.journal_sha256,
        journal_file_id: binding.journal_file_id,
        journal_bytes_verified_at_initial_hash: true,
        journal_bytes_verified_at_final_rehash: true,
        source_lock_identity_matched: true,
        workspace_roots_outside_captured_payload_trees: true,
        legacy_inline_owner_markers_absent: true,
        workspace_roots: binding.attestation_roots,
        unit_workspaces: binding.attestation_units,
        initial_tree_sha256: validation_window.initial_tree_sha256.clone(),
        final_tree_sha256: validation_window.final_tree_sha256.clone(),
        initial_lock_set_sha256: validation_window.initial_lock_set_sha256.clone(),
        final_lock_set_sha256: validation_window.final_lock_set_sha256.clone(),
    });

    let receipt = OfflinePayloadValidationReceipt {
        schema: RECEIPT_SCHEMA.to_string(),
        outcome: "validated".to_string(),
        generated_at_ms: now_ms(),
        app_version: request.app_version.trim().to_string(),
        inputs: ValidationInputs {
            stage_base_dir: display_canonical(&stage_base),
            payload_dir: display_canonical(&payload_dir),
            cosyvoice_venv_dir: display_canonical(&cosyvoice_venv),
            voice_backends_dir: display_canonical(&voice_backends),
        },
        source_bindings,
        manifest: PayloadManifestBinding {
            path: manifest_artifact.path,
            sha256: manifest_artifact.sha256,
            schema_version: export_manifest.schema_version,
            bundle_id: export_manifest.bundle_id,
            created_at_ms: export_manifest.created_at_ms,
            payload_format: export_manifest.payload_format,
            payload_bytes: export_manifest.payload_bytes,
        },
        trees,
        checks,
        stage_export_equal: StageExportEquality {
            tools: tools_equal,
            models: models_equal,
            huggingface: huggingface_equal,
            overall: tools_equal && models_equal && huggingface_equal,
        },
        source_lock: source_lock.attestation.clone(),
        reconciliation,
        validation_window,
    };
    write_new_receipt(&request.receipt_path, &receipt)?;
    Ok(receipt)
}

fn apply_offline_environment() {
    for (key, value) in [
        ("HF_HUB_OFFLINE", "1"),
        ("TRANSFORMERS_OFFLINE", "1"),
        ("HF_DATASETS_OFFLINE", "1"),
        ("PIP_NO_INDEX", "1"),
        ("PIP_DISABLE_PIP_VERSION_CHECK", "1"),
        ("PYTHONNOUSERSITE", "1"),
        ("PYTHONDONTWRITEBYTECODE", "1"),
        ("HTTP_PROXY", "http://127.0.0.1:9"),
        ("HTTPS_PROXY", "http://127.0.0.1:9"),
        ("ALL_PROXY", "http://127.0.0.1:9"),
        ("NO_PROXY", "127.0.0.1,localhost"),
    ] {
        std::env::set_var(key, value);
    }
}

impl SourceLockGuard {
    fn acquire(stage_base: &Path, external: Option<&ExternalSourceLockRequest>) -> Result<Self> {
        let expected_path = repo_source_lock_path();
        if let Some(parent) = expected_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if let Some(external) = external {
            validate_external_lock_path(&external.path)?;
            if external.owner_pid == 0
                || external.transaction_id.trim().is_empty()
                || !valid_sha256(&external.token_sha256)
                || !valid_sha256(&external.record_sha256)
            {
                return fail("external payload source-lock identity is incomplete or malformed");
            }
            let token = std::env::var("VOXVULGI_OFFLINE_SOURCE_LOCK_TOKEN").map_err(|_| {
                EngineError::InstallFailed(
                    "external source-lock token was not inherited by the validator".to_string(),
                )
            })?;
            let transaction_id =
                std::env::var("VOXVULGI_OFFLINE_TRANSACTION_ID").map_err(|_| {
                    EngineError::InstallFailed(
                    "external source-lock transaction identity was not inherited by the validator"
                        .to_string(),
                )
                })?;
            if !hex::encode(Sha256::digest(token.as_bytes()))
                .eq_ignore_ascii_case(&external.token_sha256)
                || transaction_id != external.transaction_id
            {
                return fail("external source-lock token or transaction identity mismatch");
            }
            let owner_alive = process_is_alive(external.owner_pid);
            if !owner_alive {
                return fail("external source-lock owner process is not alive");
            }
            verify_external_source_lock_record(&expected_path, external)?;
            if !exclusive_lock_probe_blocked(&expected_path)? {
                return fail(
                    "external source-lock path was not held with exclusive file-share ownership",
                );
            }
            return Ok(Self {
                _owned_file: None,
                attestation: SourceLockAttestation {
                    contract: "voxvulgi.offline_payload_source_lock.v1".to_string(),
                    path: display_canonical(&expected_path),
                    ownership: "external_reconciler".to_string(),
                    owner_pid: external.owner_pid,
                    validator_pid: std::process::id(),
                    transaction_id: external.transaction_id.clone(),
                    token_sha256: external.token_sha256.to_ascii_lowercase(),
                    record_sha256: external.record_sha256.to_ascii_lowercase(),
                    record_bytes_verified: true,
                    exclusive_probe_blocked: true,
                    owner_alive_at_start: true,
                    owner_alive_at_final_rehash: false,
                    scope: vec![
                        "driver".to_string(),
                        "reconciler".to_string(),
                        "validator".to_string(),
                    ],
                },
            });
        }

        reject_existing_lock_reparse(&expected_path)?;
        let mut file = open_owned_source_lock(&expected_path).map_err(|error| {
            EngineError::InstallFailed(format!(
                "offline payload source lock is owned by another driver/reconciler/validator at {}: {error}",
                expected_path.display()
            ))
        })?;
        let transaction_id = format!("validator_{}_{}", std::process::id(), now_ms());
        let token = hex::encode(Sha256::digest(
            format!(
                "{}:{}:{}:{}",
                std::process::id(),
                now_ms(),
                transaction_id,
                display_canonical(stage_base)
            )
            .as_bytes(),
        ));
        let token_sha256 = hex::encode(Sha256::digest(token.as_bytes()));
        let record = serde_json::json!({
            "schema": "voxvulgi.offline_payload_source_lock.v1",
            "transaction_id": transaction_id,
            "owner_pid": std::process::id(),
            "token_sha256": token_sha256.clone(),
            "acquired_at_ms": now_ms(),
            "scope": ["driver", "reconciler", "validator"]
        });
        let body = format!("{}\n", serde_json::to_string(&record)?);
        file.set_len(0)?;
        file.write_all(body.as_bytes())?;
        file.sync_all()?;
        Ok(Self {
            _owned_file: Some(file),
            attestation: SourceLockAttestation {
                contract: "voxvulgi.offline_payload_source_lock.v1".to_string(),
                path: display_canonical(&expected_path),
                ownership: "validator_internal".to_string(),
                owner_pid: std::process::id(),
                validator_pid: std::process::id(),
                transaction_id,
                token_sha256,
                record_sha256: hex::encode(Sha256::digest(body.as_bytes())),
                record_bytes_verified: true,
                exclusive_probe_blocked: true,
                owner_alive_at_start: true,
                owner_alive_at_final_rehash: false,
                scope: vec![
                    "driver".to_string(),
                    "reconciler".to_string(),
                    "validator".to_string(),
                ],
            },
        })
    }

    fn assert_held_for_final_rehash(&mut self) -> Result<()> {
        let path = Path::new(&self.attestation.path);
        if !process_is_alive(self.attestation.owner_pid) {
            return fail("payload source-lock owner exited before final rehash");
        }
        if !exclusive_lock_probe_blocked(path)? {
            return fail("payload source lock was released before final rehash");
        }
        if self._owned_file.is_none() {
            verify_external_source_lock_record(
                path,
                &ExternalSourceLockRequest {
                    path: path.to_path_buf(),
                    owner_pid: self.attestation.owner_pid,
                    transaction_id: self.attestation.transaction_id.clone(),
                    token_sha256: self.attestation.token_sha256.clone(),
                    record_sha256: self.attestation.record_sha256.clone(),
                },
            )?;
        }
        self.attestation.owner_alive_at_final_rehash = true;
        self.attestation.exclusive_probe_blocked = true;
        Ok(())
    }
}

fn repo_source_lock_path() -> PathBuf {
    repo_root().join("product/desktop/build_target/.voxvulgi_offline_payload_source.lock")
}

fn validate_external_lock_path(path: &Path) -> Result<()> {
    let parent = path.parent().ok_or_else(|| {
        EngineError::InstallFailed("external source-lock path has no parent".to_string())
    })?;
    let canonical_parent = canonical_directory(parent, "source-lock parent")?;
    let expected = repo_source_lock_path();
    let expected_parent = canonical_directory(
        expected
            .parent()
            .expect("repo source-lock path must have a parent"),
        "expected source-lock parent",
    )?;
    if canonical_parent != expected_parent
        || path.file_name().and_then(|value| value.to_str())
            != Some(".voxvulgi_offline_payload_source.lock")
    {
        return fail("external source-lock path is not the canonical stage lock path");
    }
    reject_existing_lock_reparse(path)
}

fn verify_external_source_lock_record(
    path: &Path,
    expected: &ExternalSourceLockRequest,
) -> Result<()> {
    let bytes = std::fs::read(path).map_err(|error| {
        EngineError::InstallFailed(format!(
            "external source-lock record is not readable under the owner share contract: {error}"
        ))
    })?;
    if !hex::encode(Sha256::digest(&bytes)).eq_ignore_ascii_case(&expected.record_sha256) {
        return fail("external source-lock record bytes do not match the attested SHA-256");
    }
    let record: SourceLockRecord = serde_json::from_slice(&bytes)?;
    let expected_scope = ["driver", "reconciler", "validator"];
    if record.schema != "voxvulgi.offline_payload_source_lock.v1"
        || record.transaction_id != expected.transaction_id
        || record.owner_pid != expected.owner_pid
        || !record
            .token_sha256
            .eq_ignore_ascii_case(&expected.token_sha256)
        || record.scope.iter().map(String::as_str).collect::<Vec<_>>() != expected_scope
    {
        return fail("external source-lock record identity does not match its owner attestation");
    }
    Ok(())
}

fn reject_existing_lock_reparse(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let metadata = std::fs::symlink_metadata(path)?;
    reject_link_or_reparse(path, &metadata)?;
    if !metadata.is_file() {
        return fail("payload source-lock path is not a regular file");
    }
    Ok(())
}

#[cfg(windows)]
fn open_owned_source_lock(path: &Path) -> std::io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .share_mode(0)
        .open(path)
}

#[cfg(not(windows))]
fn open_owned_source_lock(path: &Path) -> std::io::Result<File> {
    OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .open(path)
}

#[cfg(windows)]
fn exclusive_lock_probe_blocked(path: &Path) -> Result<bool> {
    use std::os::windows::fs::OpenOptionsExt;
    match OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .open(path)
    {
        Ok(_) => Ok(false),
        Err(error)
            if matches!(error.raw_os_error(), Some(5) | Some(32) | Some(33))
                || error.kind() == std::io::ErrorKind::PermissionDenied =>
        {
            Ok(true)
        }
        Err(error) => fail(format!("source-lock exclusivity probe failed: {error}")),
    }
}

#[cfg(not(windows))]
fn exclusive_lock_probe_blocked(path: &Path) -> Result<bool> {
    Ok(path.is_file())
}

#[cfg(windows)]
fn process_is_alive(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return false;
    }
    unsafe { CloseHandle(handle) };
    true
}

#[cfg(not(windows))]
fn process_is_alive(pid: u32) -> bool {
    Path::new("/proc").join(pid.to_string()).is_dir()
}

fn capture_payload_tree_snapshot(
    paths: &AppPaths,
    payload_dir: &Path,
    cosyvoice_venv: &Path,
    voice_backends: &Path,
) -> Result<PayloadTreeSnapshot> {
    let lock_set_path =
        repo_root().join("product/engine/resources/tooling/final_environment_locks");
    Ok(PayloadTreeSnapshot {
        stage_tools: tree_identity(&paths.tools_dir(), TreePolicy::ExportedTools)?,
        export_tools: tree_identity(&payload_dir.join("tools"), TreePolicy::Exact)?,
        stage_models: tree_identity(&paths.models_dir(), TreePolicy::Exact)?,
        export_models: tree_identity(&payload_dir.join("models"), TreePolicy::Exact)?,
        stage_huggingface: tree_identity(&paths.huggingface_cache_dir(), TreePolicy::Exact)?,
        export_huggingface: tree_identity(
            &payload_dir.join("cache").join("huggingface"),
            TreePolicy::Exact,
        )?,
        cosyvoice_venv: tree_identity(cosyvoice_venv, TreePolicy::Exact)?,
        voice_backends: tree_identity(voice_backends, TreePolicy::Exact)?,
        lock_set: tree_identity(&lock_set_path, TreePolicy::Exact)?,
    })
}

fn ensure_validation_window_unchanged(
    initial_trees: &BTreeMap<String, String>,
    final_trees: &BTreeMap<String, String>,
    initial_lock_set: &str,
    final_lock_set: &str,
) -> Result<()> {
    if initial_trees != final_trees || initial_lock_set != final_lock_set {
        return fail(format!(
            "payload or complete lock set changed inside the exclusive validation window: initial_trees={initial_trees:?} final_trees={final_trees:?} initial_lock={initial_lock_set} final_lock={final_lock_set}"
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn reconciliation_object_identity(path: &Path, require_directory: bool) -> Result<String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FileIdInfo, GetFileInformationByHandleEx, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_FLAG_OPEN_REPARSE_POINT, FILE_ID_INFO, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, OPEN_EXISTING,
    };

    let metadata = std::fs::symlink_metadata(path)?;
    reject_link_or_reparse(path, &metadata)?;
    if metadata.is_dir() != require_directory {
        return fail(format!(
            "reconciliation identity path has the wrong object kind: {}",
            path.display()
        ));
    }
    let wide = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return fail(format!(
            "reconciliation directory identity is unavailable for {}: {}",
            path.display(),
            std::io::Error::last_os_error()
        ));
    }
    let mut info = std::mem::MaybeUninit::<FILE_ID_INFO>::zeroed();
    let ok = unsafe {
        GetFileInformationByHandleEx(
            handle,
            FileIdInfo,
            info.as_mut_ptr().cast(),
            std::mem::size_of::<FILE_ID_INFO>() as u32,
        )
    };
    unsafe { CloseHandle(handle) };
    if ok == 0 {
        return fail(format!(
            "reconciliation FILE_ID_INFO is unavailable for {}: {}",
            path.display(),
            std::io::Error::last_os_error()
        ));
    }
    let info = unsafe { info.assume_init() };
    Ok(format!(
        "{:016x}:{}",
        info.VolumeSerialNumber,
        hex::encode(info.FileId.Identifier)
    ))
}

#[cfg(windows)]
fn reconciliation_directory_identity(path: &Path) -> Result<String> {
    reconciliation_object_identity(path, true)
}

#[cfg(windows)]
fn reconciliation_file_identity(path: &Path) -> Result<String> {
    reconciliation_object_identity(path, false)
}

#[cfg(not(windows))]
fn reconciliation_directory_identity(_path: &Path) -> Result<String> {
    fail("external reconciliation FILE_ID_INFO attestation is Windows-only")
}

#[cfg(not(windows))]
fn reconciliation_file_identity(_path: &Path) -> Result<String> {
    fail("external reconciliation FILE_ID_INFO attestation is Windows-only")
}

fn reconciliation_identity_record(
    path: &Path,
    identity: &str,
) -> Result<ReconciliationObjectIdentity> {
    let (volume_serial, file_id) = identity.split_once(':').ok_or_else(|| {
        EngineError::InstallFailed(format!(
            "malformed reconciliation FILE_ID_INFO identity: {identity}"
        ))
    })?;
    if volume_serial.len() != 16
        || file_id.len() != 32
        || !volume_serial.chars().all(|value| value.is_ascii_hexdigit())
        || !file_id.chars().all(|value| value.is_ascii_hexdigit())
    {
        return fail(format!(
            "malformed reconciliation FILE_ID_INFO identity: {identity}"
        ));
    }
    Ok(ReconciliationObjectIdentity {
        path: display_canonical(path),
        volume_serial: volume_serial.to_ascii_lowercase(),
        file_id: file_id.to_ascii_lowercase(),
        sha256: None,
    })
}

fn reconciliation_identity_key(identity: &ReconciliationObjectIdentity) -> String {
    format!("{}:{}", identity.volume_serial, identity.file_id)
}

fn canonical_exact_directory(path: &Path, label: &str) -> Result<PathBuf> {
    let canonical = canonical_directory(path, label)?;
    if !normalize_exact_binding_path(&canonical)?
        .eq_ignore_ascii_case(&normalize_exact_binding_path(path)?)
    {
        return fail(format!(
            "{label} canonical path escaped its exact binding: expected={} observed={}",
            display_canonical(path),
            display_canonical(&canonical)
        ));
    }
    Ok(canonical)
}

#[cfg(windows)]
fn read_reconciliation_bound_file(
    path: &Path,
    label: &str,
    maximum_bytes: u64,
) -> Result<(PathBuf, Vec<u8>, String)> {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        FileIdInfo, GetFileInformationByHandleEx, FILE_FLAG_OPEN_REPARSE_POINT, FILE_ID_INFO,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };

    let canonical = canonical_file(path, label)?;
    let mut options = OpenOptions::new();
    options
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    let mut file = options.open(&canonical).map_err(|error| {
        EngineError::InstallFailed(format!(
            "cannot open {label} reparse-safely at {}: {error}",
            canonical.display()
        ))
    })?;
    let metadata = file.metadata()?;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    if !metadata.is_file() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return fail(format!(
            "{label} handle is not an ordinary file: {}",
            canonical.display()
        ));
    }
    if metadata.len() == 0 || metadata.len() > maximum_bytes {
        return fail(format!(
            "{label} byte length is outside the governed bound: {}",
            metadata.len()
        ));
    }
    let mut info = std::mem::MaybeUninit::<FILE_ID_INFO>::zeroed();
    let ok = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle() as _,
            FileIdInfo,
            info.as_mut_ptr().cast(),
            std::mem::size_of::<FILE_ID_INFO>() as u32,
        )
    };
    if ok == 0 {
        return fail(format!(
            "{label} FILE_ID_INFO is unavailable for {}: {}",
            canonical.display(),
            std::io::Error::last_os_error()
        ));
    }
    let info = unsafe { info.assume_init() };
    let file_id = format!(
        "{:016x}:{}",
        info.VolumeSerialNumber,
        hex::encode(info.FileId.Identifier)
    );
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.read_to_end(&mut bytes)?;
    if bytes.len() as u64 != metadata.len() {
        return fail(format!(
            "{label} size changed while its bound handle was read: {}",
            canonical.display()
        ));
    }
    if reconciliation_file_identity(&canonical)? != file_id {
        return fail(format!(
            "{label} path was replaced while its bound handle was read: {}",
            canonical.display()
        ));
    }
    Ok((canonical, bytes, file_id))
}

#[cfg(not(windows))]
fn read_reconciliation_bound_file(
    _path: &Path,
    _label: &str,
    _maximum_bytes: u64,
) -> Result<(PathBuf, Vec<u8>, String)> {
    fail("external reconciliation bound-file attestation is Windows-only")
}

fn exact_bound_path(observed: &str, expected: &Path, label: &str) -> Result<()> {
    let observed = normalize_exact_binding_path(&PathBuf::from(observed))?;
    let expected = normalize_exact_binding_path(expected)?;
    if !observed.eq_ignore_ascii_case(&expected) {
        return fail(format!(
            "{label} path mismatch: expected={expected} observed={observed}"
        ));
    }
    Ok(())
}

fn normalize_exact_binding_path(path: &Path) -> Result<String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut cursor = PathBuf::new();
    for component in absolute.components() {
        cursor.push(component.as_os_str());
        if !matches!(component, Component::Normal(_)) {
            continue;
        }
        match std::fs::symlink_metadata(&cursor) {
            Ok(metadata) => reject_link_or_reparse(&cursor, &metadata)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(error.into()),
        }
    }

    let mut existing = absolute.as_path();
    let mut suffix = Vec::new();
    loop {
        match std::fs::symlink_metadata(existing) {
            Ok(metadata) => {
                reject_link_or_reparse(existing, &metadata)?;
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let name = existing.file_name().ok_or_else(|| {
                    EngineError::InstallFailed(format!(
                        "exact reconciliation binding has no existing ancestor: {}",
                        absolute.display()
                    ))
                })?;
                suffix.push(name.to_os_string());
                existing = existing.parent().ok_or_else(|| {
                    EngineError::InstallFailed(format!(
                        "exact reconciliation binding escaped its root: {}",
                        absolute.display()
                    ))
                })?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    let mut normalized = std::fs::canonicalize(existing)?;
    for component in suffix.iter().rev() {
        normalized.push(component);
    }
    Ok(display_canonical(&normalized))
}

fn reject_legacy_inline_owner_markers(roots: &[&Path]) -> Result<()> {
    for root in roots {
        let mut pending = vec![root.to_path_buf()];
        while let Some(directory) = pending.pop() {
            for entry in std::fs::read_dir(&directory)? {
                let entry = entry?;
                let path = entry.path();
                let metadata = std::fs::symlink_metadata(&path)?;
                reject_link_or_reparse(&path, &metadata)?;
                if entry.file_name() == ".voxvulgi_reconcile_owner.json" {
                    return fail(format!(
                        "legacy inline reconciliation owner marker is forbidden: {}",
                        path.display()
                    ));
                }
                if metadata.is_dir() {
                    pending.push(path);
                }
            }
        }
    }
    Ok(())
}

fn assert_exact_reconciliation_children(
    directory: &Path,
    expected: &[&str],
    label: &str,
) -> Result<()> {
    let mut observed = BTreeSet::new();
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path)?;
        reject_link_or_reparse(&path, &metadata)?;
        let name = entry.file_name().into_string().map_err(|_| {
            EngineError::InstallFailed(format!(
                "{label} contains a non-Unicode child: {}",
                path.display()
            ))
        })?;
        if !observed.insert(name.clone()) {
            return fail(format!("{label} contains duplicate child name: {name}"));
        }
    }
    let expected = expected
        .iter()
        .map(|value| (*value).to_string())
        .collect::<BTreeSet<_>>();
    if observed != expected {
        return fail(format!(
            "{label} child topology mismatch: expected={expected:?} observed={observed:?}"
        ));
    }
    Ok(())
}

fn verify_external_reconciliation_binding(
    stage: &Path,
    export: &Path,
    repo: &Path,
    receipt: &Path,
    app_version: &str,
    external: &ExternalSourceLockRequest,
    captured_roots: &[&Path],
) -> Result<VerifiedReconciliationBinding> {
    let expected_journal_path = stage.join(".voxvulgi_python_environment_transaction.json");
    let (journal_path, journal_bytes, journal_file_id) = read_reconciliation_bound_file(
        &expected_journal_path,
        "reconciliation journal",
        2 * 1024 * 1024,
    )?;
    let journal_sha256 = hex::encode(Sha256::digest(&journal_bytes));
    let journal: ReconciliationJournal = serde_json::from_slice(&journal_bytes)?;
    if journal.schema != "voxvulgi.python_environment_transaction.v2"
        || journal.transaction_id != external.transaction_id
        || journal.owner_pid != external.owner_pid
        || journal.source_lock_token_sha256 != external.token_sha256
        || journal.source_lock_record_sha256 != external.record_sha256
        || journal.app_version != app_version
        || journal.state != "validator_intent_durable"
        || journal.committed
        || journal.cleanup_state != "pending"
        || journal.cleanup_intent.is_some()
        || journal.validation_receipt_sha256.is_some()
        || journal.validation_journal_sha256.is_some()
        || journal.validation_journal_file_id.is_some()
        || journal.committed_at_utc.is_some()
        || journal.rolled_back_at_utc.is_some()
        || journal.created_at_utc.trim().is_empty()
        || journal.updated_at_utc.trim().is_empty()
    {
        return fail("live reconciliation journal state/source-lock identity mismatch");
    }
    exact_bound_path(
        &journal.journal_path,
        &journal_path,
        "reconciliation journal self binding",
    )?;
    exact_bound_path(
        &journal.receipt_path,
        receipt,
        "reconciliation validation receipt",
    )?;
    exact_bound_path(
        &journal.source_lock_path,
        &external.path,
        "reconciliation journal source lock",
    )?;

    let transaction_id = external.transaction_id.as_str();
    let expected_roots = BTreeMap::from([
        (
            "stage",
            stage
                .join(".voxvulgi_python_environment_transactions")
                .join(transaction_id),
        ),
        (
            "export",
            export
                .join(".voxvulgi_python_environment_transactions")
                .join(transaction_id),
        ),
        (
            "repo",
            repo.join("product/desktop/build_target/.voxvulgi_python_environment_transactions")
                .join(transaction_id),
        ),
    ]);
    let expected_trusted_roots = BTreeMap::from([
        ("stage", stage.to_path_buf()),
        ("export", export.to_path_buf()),
        ("repo", repo.to_path_buf()),
    ]);
    if journal.workspace_roots.len() != expected_roots.len() {
        return fail("reconciliation journal workspace-root cardinality mismatch");
    }
    let mut attestation_roots = BTreeMap::new();
    for root in &journal.workspace_roots {
        let expected = expected_roots.get(root.name.as_str()).ok_or_else(|| {
            EngineError::InstallFailed(format!(
                "unknown reconciliation workspace root: {}",
                root.name
            ))
        })?;
        exact_bound_path(
            &root.transaction_root,
            expected,
            "reconciliation transaction root",
        )?;
        let collection = expected.parent().ok_or_else(|| {
            EngineError::InstallFailed("reconciliation transaction root has no parent".to_string())
        })?;
        exact_bound_path(
            &root.collection_root,
            collection,
            "reconciliation collection root",
        )?;
        let expected_owner = expected.join("owner.json");
        exact_bound_path(
            &root.owner_path,
            &expected_owner,
            "reconciliation transaction-root owner",
        )?;
        let trusted = expected_trusted_roots[root.name.as_str()].as_path();
        exact_bound_path(
            &root.trusted_root,
            trusted,
            "reconciliation workspace trusted root",
        )?;
        let canonical_collection =
            canonical_exact_directory(collection, "reconciliation collection root")?;
        let canonical_transaction =
            canonical_exact_directory(expected, "reconciliation transaction root")?;
        let collection_id = reconciliation_directory_identity(&canonical_collection)?;
        let transaction_root_id = reconciliation_directory_identity(&canonical_transaction)?;
        let (owner_path, owner_bytes, owner_file_id) = read_reconciliation_bound_file(
            &expected_owner,
            "reconciliation transaction-root owner",
            64 * 1024,
        )?;
        if collection_id != root.collection_root_id
            || transaction_root_id != root.transaction_root_id
            || owner_file_id != root.owner_file_id
            || !root.rename_identity_verified
        {
            return fail(format!(
                "reconciliation workspace-root identity mismatch: {}",
                root.name
            ));
        }
        let owner: ReconciliationRootOwner = serde_json::from_slice(&owner_bytes)?;
        if owner.schema != "voxvulgi.python_environment_transaction_root_owner.v1"
            || owner.transaction_id != journal.transaction_id
            || owner.name != root.name
            || owner.collection_root_id != root.collection_root_id
            || owner.transaction_root_id != root.transaction_root_id
            || owner.rename_identity_verified != root.rename_identity_verified
        {
            return fail(format!(
                "reconciliation transaction-root owner content mismatch: {}",
                root.name
            ));
        }
        exact_bound_path(
            &owner.collection_root,
            &canonical_collection,
            "reconciliation owner collection root",
        )?;
        exact_bound_path(
            &owner.transaction_root,
            &canonical_transaction,
            "reconciliation owner transaction root",
        )?;
        let collection_identity =
            reconciliation_identity_record(&canonical_collection, &collection_id)?;
        let transaction_identity =
            reconciliation_identity_record(&canonical_transaction, &transaction_root_id)?;
        let mut owner_identity = reconciliation_identity_record(&owner_path, &owner_file_id)?;
        owner_identity.sha256 = Some(hex::encode(Sha256::digest(&owner_bytes)));
        if collection_identity.volume_serial != transaction_identity.volume_serial
            || transaction_identity.volume_serial != owner_identity.volume_serial
        {
            return fail(format!(
                "reconciliation workspace root crosses volumes: {}",
                root.name
            ));
        }
        for captured in captured_roots {
            if canonical_transaction.starts_with(captured)
                || captured.starts_with(&canonical_transaction)
            {
                return fail(format!(
                    "reconciliation workspace root overlaps captured payload tree: workspace={} captured={}",
                    canonical_transaction.display(),
                    captured.display()
                ));
            }
        }
        if attestation_roots
            .insert(
                root.name.clone(),
                ReconciliationWorkspaceRoot {
                    collection_root: collection_identity,
                    transaction_root: transaction_identity,
                    owner: owner_identity,
                    rename_identity_verified: true,
                },
            )
            .is_some()
        {
            return fail(format!(
                "duplicate reconciliation workspace root: {}",
                root.name
            ));
        }
    }

    let expected_units = BTreeMap::from([
        (
            "main",
            (
                expected_roots["stage"].join("main"),
                stage.join("tools/python/venv"),
            ),
        ),
        (
            "cosyvoice",
            (
                expected_roots["stage"].join("cosyvoice"),
                stage.join("tools/python/venv_cosyvoice"),
            ),
        ),
        (
            "export",
            (
                expected_roots["export"].join("export"),
                export.join("tools/python/venv"),
            ),
        ),
        (
            "lock_set",
            (
                expected_roots["repo"].join("lock_set"),
                repo.join("product/engine/resources/tooling/final_environment_locks"),
            ),
        ),
    ]);
    if journal.units.len() != expected_units.len() {
        return fail("reconciliation journal unit cardinality mismatch");
    }
    let mut attestation_units = BTreeMap::new();
    for unit in &journal.units {
        let (workspace, target) = expected_units.get(unit.name.as_str()).ok_or_else(|| {
            EngineError::InstallFailed(format!("unknown reconciliation unit: {}", unit.name))
        })?;
        let transaction_root = workspace.parent().ok_or_else(|| {
            EngineError::InstallFailed(format!(
                "reconciliation unit workspace has no transaction root: {}",
                unit.name
            ))
        })?;
        let collection_root = transaction_root.parent().ok_or_else(|| {
            EngineError::InstallFailed(format!(
                "reconciliation transaction root has no collection root: {}",
                unit.name
            ))
        })?;
        let trusted_root = if unit.name == "main" || unit.name == "cosyvoice" {
            stage
        } else if unit.name == "export" {
            export
        } else {
            repo
        };
        let owner = workspace.join("owner.json");
        let prepared = workspace.join("prepared");
        let backup = workspace.join("backup");
        let discard = workspace.join("discard");
        exact_bound_path(&unit.workspace, workspace, "reconciliation unit workspace")?;
        exact_bound_path(&unit.target, target, "reconciliation unit target")?;
        exact_bound_path(
            &unit.collection_root,
            collection_root,
            "reconciliation unit collection root",
        )?;
        exact_bound_path(
            &unit.transaction_root,
            transaction_root,
            "reconciliation unit transaction root",
        )?;
        exact_bound_path(&unit.owner_path, &owner, "reconciliation unit owner")?;
        exact_bound_path(
            &unit.prepared,
            &prepared,
            "reconciliation prepared generation",
        )?;
        exact_bound_path(&unit.backup, &backup, "reconciliation backup generation")?;
        exact_bound_path(&unit.discard, &discard, "reconciliation discard generation")?;
        exact_bound_path(
            &unit.trusted_root,
            trusted_root,
            "reconciliation unit trusted root",
        )?;
        let canonical_workspace =
            canonical_exact_directory(workspace, "reconciliation unit workspace")?;
        let canonical_target = canonical_exact_directory(target, "reconciliation unit target")?;
        let workspace_id = reconciliation_directory_identity(&canonical_workspace)?;
        let target_id = reconciliation_directory_identity(&canonical_target)?;
        let (owner_path, owner_bytes, owner_file_id) =
            read_reconciliation_bound_file(&owner, "reconciliation unit owner", 64 * 1024)?;
        if workspace_id != unit.workspace_id
            || target_id != unit.prepared_directory_id
            || owner_file_id != unit.owner_file_id
            || !unit.mutation_started
            || unit.state != "published"
        {
            return fail(format!(
                "reconciliation published unit identity mismatch: {}",
                unit.name
            ));
        }
        if prepared.exists() || discard.exists() {
            return fail(format!(
                "reconciliation published unit retained prepared/discard generation: {}",
                unit.name
            ));
        }
        let owner_document: ReconciliationUnitOwner = serde_json::from_slice(&owner_bytes)?;
        if owner_document.schema != "voxvulgi.python_environment_transaction_unit_owner.v1"
            || owner_document.transaction_id != journal.transaction_id
            || owner_document.unit != unit.name
            || owner_document.workspace_id != unit.workspace_id
            || owner_document.prepared_directory_id != unit.prepared_directory_id
            || owner_document.original_existed != unit.original_existed
            || owner_document.original_directory_id != unit.original_directory_id
        {
            return fail(format!(
                "reconciliation unit owner content mismatch: {}",
                unit.name
            ));
        }
        for (observed, expected, label) in [
            (
                &owner_document.workspace,
                workspace.as_path(),
                "unit-owner workspace",
            ),
            (
                &owner_document.target,
                target.as_path(),
                "unit-owner target",
            ),
            (
                &owner_document.prepared,
                prepared.as_path(),
                "unit-owner prepared",
            ),
            (
                &owner_document.backup,
                backup.as_path(),
                "unit-owner backup",
            ),
            (
                &owner_document.discard,
                discard.as_path(),
                "unit-owner discard",
            ),
        ] {
            exact_bound_path(observed, expected, label)?;
        }
        let workspace_identity =
            reconciliation_identity_record(&canonical_workspace, &workspace_id)?;
        let target_identity = reconciliation_identity_record(&canonical_target, &target_id)?;
        let mut owner_identity = reconciliation_identity_record(&owner_path, &owner_file_id)?;
        owner_identity.sha256 = Some(hex::encode(Sha256::digest(&owner_bytes)));
        if workspace_identity.volume_serial != target_identity.volume_serial
            || target_identity.volume_serial != owner_identity.volume_serial
        {
            return fail(format!(
                "reconciliation unit crosses volumes: {}",
                unit.name
            ));
        }
        let original_backup = if unit.original_existed {
            let original_id = unit.original_directory_id.as_deref().ok_or_else(|| {
                EngineError::InstallFailed(format!(
                    "reconciliation original identity missing: {}",
                    unit.name
                ))
            })?;
            let canonical_backup =
                canonical_exact_directory(&backup, "reconciliation original backup")?;
            if reconciliation_directory_identity(&canonical_backup)? != original_id {
                return fail(format!(
                    "reconciliation original backup identity mismatch: {}",
                    unit.name
                ));
            }
            let identity = reconciliation_identity_record(&canonical_backup, original_id)?;
            if identity.volume_serial != workspace_identity.volume_serial {
                return fail(format!(
                    "reconciliation original backup crosses volumes: {}",
                    unit.name
                ));
            }
            Some(identity)
        } else if unit.original_directory_id.is_some() || backup.exists() {
            return fail(format!(
                "reconciliation originally-absent unit has a backup: {}",
                unit.name
            ));
        } else {
            None
        };
        if attestation_units
            .insert(
                unit.name.clone(),
                ReconciliationUnitWorkspace {
                    target: target_identity,
                    workspace: workspace_identity,
                    owner: owner_identity,
                    original_existed: unit.original_existed,
                    original_backup,
                },
            )
            .is_some()
        {
            return fail(format!("duplicate reconciliation unit: {}", unit.name));
        }
    }
    for (name, transaction_root) in &expected_roots {
        let collection_root = transaction_root.parent().ok_or_else(|| {
            EngineError::InstallFailed(format!(
                "reconciliation collection root is missing for {name}"
            ))
        })?;
        assert_exact_reconciliation_children(
            collection_root,
            &[transaction_id],
            &format!("reconciliation {name} collection root"),
        )?;
        let expected_children = match *name {
            "stage" => vec!["owner.json", "main", "cosyvoice"],
            "export" => vec!["owner.json", "export"],
            "repo" => vec!["owner.json", "lock_set"],
            _ => unreachable!("expected_roots is closed"),
        };
        assert_exact_reconciliation_children(
            transaction_root,
            &expected_children,
            &format!("reconciliation {name} transaction root"),
        )?;
    }
    for (name, (workspace, _)) in &expected_units {
        let unit = journal
            .units
            .iter()
            .find(|unit| unit.name == *name)
            .expect("journal unit cardinality/identity was validated");
        let expected_children = if unit.original_existed {
            vec!["owner.json", "backup"]
        } else {
            vec!["owner.json"]
        };
        assert_exact_reconciliation_children(
            workspace,
            &expected_children,
            &format!("reconciliation {name} unit workspace"),
        )?;
    }
    reject_legacy_inline_owner_markers(captured_roots)?;
    Ok(VerifiedReconciliationBinding {
        journal_path,
        journal_sha256,
        journal_file_id,
        attestation_roots,
        attestation_units,
    })
}

fn assert_reconciliation_binding_unchanged(binding: &VerifiedReconciliationBinding) -> Result<()> {
    let (_, journal_bytes, journal_file_id) = read_reconciliation_bound_file(
        &binding.journal_path,
        "reconciliation journal final rehash",
        2 * 1024 * 1024,
    )?;
    if hex::encode(Sha256::digest(&journal_bytes)) != binding.journal_sha256
        || journal_file_id != binding.journal_file_id
    {
        return fail("reconciliation journal changed inside the validation window");
    }
    for root in binding.attestation_roots.values() {
        let (_, owner_bytes, owner_id) = read_reconciliation_bound_file(
            Path::new(&root.owner.path),
            "reconciliation transaction-root owner final rehash",
            64 * 1024,
        )?;
        if reconciliation_directory_identity(Path::new(&root.collection_root.path))?
            != reconciliation_identity_key(&root.collection_root)
            || reconciliation_directory_identity(Path::new(&root.transaction_root.path))?
                != reconciliation_identity_key(&root.transaction_root)
            || owner_id != reconciliation_identity_key(&root.owner)
            || root.owner.sha256.as_deref()
                != Some(hex::encode(Sha256::digest(&owner_bytes)).as_str())
        {
            return fail(
                "reconciliation workspace-root identity changed inside the validation window",
            );
        }
    }
    for unit in binding.attestation_units.values() {
        let (_, owner_bytes, owner_id) = read_reconciliation_bound_file(
            Path::new(&unit.owner.path),
            "reconciliation unit owner final rehash",
            64 * 1024,
        )?;
        let owner_sha256 = hex::encode(Sha256::digest(&owner_bytes));
        if reconciliation_directory_identity(Path::new(&unit.workspace.path))?
            != reconciliation_identity_key(&unit.workspace)
            || reconciliation_directory_identity(Path::new(&unit.target.path))?
                != reconciliation_identity_key(&unit.target)
            || owner_id != reconciliation_identity_key(&unit.owner)
            || unit.owner.sha256.as_deref() != Some(owner_sha256.as_str())
        {
            return fail("reconciliation unit identity changed inside the validation window");
        }
        if let Some(original) = unit.original_backup.as_ref() {
            if reconciliation_directory_identity(Path::new(&original.path))?
                != reconciliation_identity_key(original)
            {
                return fail(
                    "reconciliation original-backup identity changed inside the validation window",
                );
            }
        }
    }
    Ok(())
}

fn run_python_smoke(python: &Path, cosyvoice: bool, paths: &AppPaths) -> Result<serde_json::Value> {
    let code = if cosyvoice {
        r#"import json, torch
x = torch.tensor([1.0], device='cpu')
assert float(x.sum()) == 1.0
print(json.dumps({'python_runtime':'cosyvoice','torch_version':torch.__version__,'torch_cuda_build':torch.version.cuda or '','cuda_available':bool(torch.cuda.is_available()),'cpu_tensor_smoke':True}))"#
    } else {
        r#"import json, torch, demucs_infer, librosa, sklearn, soundfile, webrtcvad
from resemblyzer import VoiceEncoder
from kokoro import KPipeline
import openvoice.api
x = torch.tensor([1.0], device='cpu')
assert float(x.sum()) == 1.0
VoiceEncoder()
print(json.dumps({'python_runtime':'main','torch_version':torch.__version__,'torch_cuda_build':torch.version.cuda or '','cuda_available':bool(torch.cuda.is_available()),'cpu_tensor_smoke':True,'demucs_import':True,'diarization_encoder_load':True,'kokoro_import':True,'openvoice_import':True}))"#
    };
    let mut command = cmd::command(python);
    command.args(["-I", "-c", code]);
    command.env("HF_HOME", paths.huggingface_cache_dir());
    command.env(
        "HUGGINGFACE_HUB_CACHE",
        paths.huggingface_cache_dir().join("hub"),
    );
    command.env("TORCH_HOME", paths.python_models_dir().join("demucs"));
    let output = cmd::run_owned_output(&mut command, Duration::from_secs(10 * 60), || false)
        .map_err(|error| {
            EngineError::InstallFailed(format!("offline Python smoke failed: {error}"))
        })?;
    if !output.status.success() {
        return fail(format!(
            "offline Python smoke exited {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let line = String::from_utf8_lossy(&output.stdout)
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .ok_or_else(|| EngineError::InstallFailed("Python smoke emitted no JSON".to_string()))?
        .to_string();
    let value: serde_json::Value = serde_json::from_str(&line)?;
    if value
        .get("cpu_tensor_smoke")
        .and_then(|value| value.as_bool())
        != Some(true)
    {
        return fail("Python smoke omitted the CPU fallback proof");
    }
    Ok(value)
}

fn validate_lockfiles() -> Result<Vec<LockfileCheck>> {
    let repo = repo_root();
    let manifest = pinned_dependency_manifest::manifest();
    let mut checks = Vec::new();
    for (pack, relative) in &manifest.lockfiles {
        let path = repo
            .join("product")
            .join("engine")
            .join("resources")
            .join("tooling")
            .join(relative);
        let lock = crate::python_lockfile::PythonLockfile::load(&path)
            .map_err(|error| EngineError::InstallFailed(error.to_string()))?;
        if lock.pack != *pack || lock.packages.is_empty() || lock.schema_version != 1 {
            return fail(format!("invalid canonical lockfile for {pack}"));
        }
        let fully_hashed = lock
            .packages
            .iter()
            .all(|package| package.sha256.as_deref().map(valid_sha256).unwrap_or(false));
        let urls = lock.packages.iter().all(|package| {
            package
                .url
                .as_deref()
                .map(|url| {
                    url.starts_with("https://")
                        && (url.contains(".whl") || url.contains(".tar.gz") || url.contains(".zip"))
                })
                .unwrap_or(false)
        });
        if !fully_hashed || !urls {
            return fail(format!(
                "lockfile {pack} contains an unhashed or non-wheel/sdist package"
            ));
        }
        lock.render_hashed_requirements()
            .map_err(|error| EngineError::InstallFailed(error.to_string()))?;
        checks.push(LockfileCheck {
            pack: pack.clone(),
            path: display_canonical(&canonical_file(&path, "lockfile")?),
            packages: lock.packages.len(),
            fully_hashed,
            wheel_or_sdist_urls_present: urls,
        });
    }
    Ok(checks)
}

fn collect_source_bindings() -> Result<Vec<SourceBinding>> {
    let repo = repo_root();
    let mut sources = vec![
        ("validator_executable", std::env::current_exe()?),
        (
            "validator_source",
            repo.join("product/engine/src/offline_payload_validation.rs"),
        ),
        (
            "validator_entrypoint",
            repo.join("product/engine/src/bin/voxvulgi_offline_payload_validate.rs"),
        ),
        (
            "payload_prep_source",
            repo.join("product/engine/src/bin/voxvulgi_offline_bundle_prep.rs"),
        ),
        (
            "offline_workflow_proof_source",
            repo.join("product/engine/src/localization_offline_proof.rs"),
        ),
        ("tools_source", repo.join("product/engine/src/tools.rs")),
        ("paths_source", repo.join("product/engine/src/paths.rs")),
        ("models_source", repo.join("product/engine/src/models.rs")),
        (
            "python_lockfile_source",
            repo.join("product/engine/src/python_lockfile.rs"),
        ),
        (
            "environment_reconciliation_source",
            repo.join("offline-installer-runtime/scripts/reconcile_offline_python_environments.ps1"),
        ),
        (
            "cosyvoice_requirements_source",
            repo.join("product/engine/resources/tooling/requirements.cosyvoice.txt"),
        ),
        (
            "cosyvoice_constraints_source",
            repo.join("product/engine/resources/tooling/constraints.cosyvoice.txt"),
        ),
        (
            "cosyvoice_model_manifest",
            repo.join("product/engine/resources/tooling/cosyvoice_model_manifest.json"),
        ),
        (
            "pinned_dependency_manifest",
            repo.join("product/engine/resources/tooling/pinned_dependency_manifest.json"),
        ),
        (
            "governed_pure_wheels_manifest",
            repo.join(
                "product/engine/resources/tooling/governed_wheels/governed_pure_wheels.manifest.json",
            ),
        ),
        (
            "governed_pure_wheels_helper",
            repo.join("product/engine/resources/tooling/build_governed_pure_wheels.py"),
        ),
        (
            "governed_pure_wheel_openai_whisper",
            repo.join(
                "product/engine/resources/tooling/governed_wheels/openai_whisper-20231117-py3-none-any.whl",
            ),
        ),
        (
            "governed_pure_wheel_wget",
            repo.join(
                "product/engine/resources/tooling/governed_wheels/wget-3.2-py3-none-any.whl",
            ),
        ),
        (
            "governed_pure_wheel_myshell_openvoice",
            repo.join(
                "product/engine/resources/tooling/governed_wheels/MyShell_OpenVoice-0.0.0-py3-none-any.whl",
            ),
        ),
        (
            "governed_pure_wheel_eng_to_ipa",
            repo.join(
                "product/engine/resources/tooling/governed_wheels/eng_to_ipa-0.0.2-py3-none-any.whl",
            ),
        ),
        (
            "governed_pure_wheel_jieba",
            repo.join(
                "product/engine/resources/tooling/governed_wheels/jieba-0.42.1-py3-none-any.whl",
            ),
        ),
        (
            "model_manifest",
            repo.join("product/engine/resources/models/manifest.json"),
        ),
    ];
    for (pack, relative) in &pinned_dependency_manifest::manifest().lockfiles {
        sources.push((
            Box::leak(format!("lockfile_{pack}").into_boxed_str()),
            repo.join("product/engine/resources/tooling").join(relative),
        ));
    }
    for (environment, relative) in
        &pinned_dependency_manifest::manifest().offline_python_environment_locks
    {
        sources.push((
            Box::leak(format!("final_environment_lock_{environment}").into_boxed_str()),
            repo.join("product/engine/resources/tooling").join(relative),
        ));
    }
    sources
        .into_iter()
        .map(|(id, path)| source_binding(id, &path))
        .collect()
}

fn validate_final_environment_locks(paths: &AppPaths) -> Result<Vec<FinalEnvironmentCheck>> {
    let manifest = pinned_dependency_manifest::manifest();
    let expected = [
        (
            "main_windows_x64_cp311",
            tools::python_venv_python_path(paths)?,
        ),
        (
            "cosyvoice_windows_x64_cp311",
            tools::cosyvoice_venv_python_path(paths)?,
        ),
    ];
    let mut checks = Vec::new();
    for (environment, python) in expected {
        let relative = manifest
            .offline_python_environment_locks
            .get(environment)
            .ok_or_else(|| {
                EngineError::InstallFailed(format!(
                    "pinned manifest is missing complete environment lock {environment}"
                ))
            })?;
        let lock_path = repo_root()
            .join("product/engine/resources/tooling")
            .join(relative);
        let lock_artifact = required_file(environment, &lock_path)?;
        let lock: FinalEnvironmentLock = serde_json::from_slice(&std::fs::read(&lock_path)?)?;
        if lock.schema != "voxvulgi.python_environment_lock.v1"
            || lock.environment != environment
            || lock.python_tag != "cp311-win_amd64"
            || lock.indexes.is_empty()
            || lock.packages.is_empty()
            || lock
                .indexes
                .iter()
                .any(|index| !is_governed_python_index(index))
        {
            return fail(format!("invalid complete environment lock {environment}"));
        }
        let mut locked = BTreeMap::new();
        for package in &lock.packages {
            let name = normalize_package_name(&package.name);
            if name.is_empty()
                || package.version.trim().is_empty()
                || package.version.contains('@')
                || package.wheel_bytes == 0
                || !package
                    .wheel_filename
                    .to_ascii_lowercase()
                    .ends_with(".whl")
                || !valid_sha256(&package.wheel_sha256)
                || locked
                    .insert(name.clone(), package.version.clone())
                    .is_some()
            {
                return fail(format!(
                    "complete environment lock {environment} has an invalid/duplicate package {}",
                    package.name
                ));
            }
        }
        let reconstructed = lock
            .packages
            .iter()
            .map(|package| {
                format!(
                    "{}=={} --hash=sha256:{}\n",
                    package.name, package.version, package.wheel_sha256
                )
            })
            .collect::<String>();
        let reconstructed_sha = hex::encode(Sha256::digest(reconstructed.as_bytes()));
        if !reconstructed_sha.eq_ignore_ascii_case(&lock.requirements_sha256) {
            return fail(format!(
                "complete environment lock {environment} requirements hash mismatch"
            ));
        }

        let inspect = run_python_json(&python, &["-m", "pip", "inspect", "--local"])?;
        let inspect_version = inspect
            .get("version")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string();
        if inspect_version != "1" {
            return fail(format!(
                "pip inspect for {environment} returned unsupported schema {inspect_version:?}"
            ));
        }
        let pip_version = inspect
            .get("pip_version")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string();
        if pip_version.is_empty() {
            return fail(format!("pip inspect for {environment} omitted pip_version"));
        }
        let installed_rows = inspect
            .get("installed")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| {
                EngineError::InstallFailed(format!(
                    "pip inspect for {environment} omitted installed inventory"
                ))
            })?;
        let mut installed = BTreeMap::new();
        for row in installed_rows {
            let metadata = row.get("metadata").ok_or_else(|| {
                EngineError::InstallFailed(format!(
                    "pip inspect for {environment} has a row without metadata"
                ))
            })?;
            let name = metadata
                .get("name")
                .and_then(serde_json::Value::as_str)
                .map(normalize_package_name)
                .unwrap_or_default();
            let version = metadata
                .get("version")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .to_string();
            if name.is_empty()
                || version.is_empty()
                || installed.insert(name.clone(), version).is_some()
            {
                return fail(format!(
                    "pip inspect for {environment} has an invalid/duplicate package {name}"
                ));
            }
        }
        if installed != locked {
            let missing = locked
                .keys()
                .filter(|name| !installed.contains_key(*name))
                .take(5)
                .cloned()
                .collect::<Vec<_>>();
            let unlisted = installed
                .keys()
                .filter(|name| !locked.contains_key(*name))
                .take(5)
                .cloned()
                .collect::<Vec<_>>();
            return fail(format!(
                "complete environment lock {environment} does not equal pip inspect inventory; missing={missing:?} unlisted={unlisted:?}"
            ));
        }
        let inventory = installed
            .iter()
            .map(|(name, version)| format!("{name}=={version}\n"))
            .collect::<String>();
        let freeze = run_python_text(&python, &["-m", "pip", "freeze", "--all"])?;
        let exact_freeze_only = freeze
            .lines()
            .filter(|line| !line.trim().is_empty())
            .all(|line| {
                !line.starts_with("-e ")
                    && !line.contains(" @ ")
                    && line.split_once("==").map(|(name, version)| {
                        !normalize_package_name(name).is_empty() && !version.trim().is_empty()
                    }) == Some(true)
            });
        if !exact_freeze_only {
            return fail(format!(
                "{environment} contains editable, direct-URL, or non-exact installed distributions"
            ));
        }
        checks.push(FinalEnvironmentCheck {
            environment: environment.to_string(),
            lock_path: lock_artifact.path,
            lock_sha256: lock_artifact.sha256,
            package_count: locked.len(),
            pip_inspect_version: inspect_version,
            pip_version,
            installed_inventory_sha256: hex::encode(Sha256::digest(inventory.as_bytes())),
            freeze_sha256: hex::encode(Sha256::digest(freeze.as_bytes())),
            exact_freeze_only,
            inventory_equal: true,
        });
    }
    Ok(checks)
}

fn run_python_json(python: &Path, arguments: &[&str]) -> Result<serde_json::Value> {
    let text = run_python_text(python, arguments)?;
    Ok(serde_json::from_str(&text)?)
}

fn run_python_text(python: &Path, arguments: &[&str]) -> Result<String> {
    let mut command = cmd::command(python);
    command.args(arguments);
    let output = cmd::run_owned_output(&mut command, Duration::from_secs(5 * 60), || false)
        .map_err(|error| EngineError::InstallFailed(format!("Python inventory failed: {error}")))?;
    if !output.status.success() {
        return fail(format!(
            "Python inventory exited {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    String::from_utf8(output.stdout)
        .map_err(|error| EngineError::InstallFailed(format!("Python output is not UTF-8: {error}")))
}

fn normalize_package_name(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace(['_', '.'], "-")
}

fn is_governed_python_index(value: &str) -> bool {
    matches!(
        value.trim_end_matches('/'),
        "https://pypi.org/simple" | "https://download.pytorch.org/whl/cpu"
    )
}

fn source_binding(id: &str, path: &Path) -> Result<SourceBinding> {
    let artifact = required_file(id, path)?;
    Ok(SourceBinding {
        id: artifact.id,
        path: artifact.path,
        bytes: artifact.bytes,
        sha256: artifact.sha256,
    })
}

fn required_weight_files(
    root: &Path,
    extensions: &[&str],
    minimum_bytes: u64,
    label: &str,
) -> Result<Vec<RequiredArtifact>> {
    let root = canonical_directory(root, label)?;
    let mut paths = Vec::new();
    let mut pending = vec![root.clone()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory)? {
            let entry = entry?;
            reject_link_or_reparse(&entry.path(), &entry.metadata()?)?;
            if entry.file_type()?.is_dir() {
                pending.push(entry.path());
            } else if entry.file_type()?.is_file()
                && entry
                    .path()
                    .extension()
                    .and_then(|value| value.to_str())
                    .map(|value| extensions.iter().any(|ext| value.eq_ignore_ascii_case(ext)))
                    .unwrap_or(false)
                && entry.metadata()?.len() >= minimum_bytes
            {
                paths.push(entry.path());
            }
        }
    }
    paths.sort();
    if paths.is_empty() {
        return fail(format!(
            "{label} weights are missing below {}",
            root.display()
        ));
    }
    paths
        .iter()
        .enumerate()
        .map(|(index, path)| {
            required_file(
                &format!("{}_weight_{index}", label.to_ascii_lowercase()),
                path,
            )
        })
        .collect()
}

fn required_file(id: &str, path: &Path) -> Result<RequiredArtifact> {
    let path = canonical_file(path, id)?;
    let metadata = std::fs::symlink_metadata(&path)?;
    reject_link_or_reparse(&path, &metadata)?;
    if metadata.len() == 0 {
        return fail(format!(
            "required artifact {id} is empty: {}",
            path.display()
        ));
    }
    Ok(RequiredArtifact {
        id: id.to_string(),
        path: display_canonical(&path),
        bytes: metadata.len(),
        sha256: sha256_file(&path)?,
    })
}

#[derive(Debug)]
struct CosyvoiceExpectedTree {
    files: BTreeMap<String, (u64, String)>,
    directories: BTreeSet<String>,
    byte_count: u64,
    empty_file_count: u64,
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn cosyvoice_manifest_path_components(relative: &str) -> Result<Vec<&str>> {
    if relative.is_empty()
        || !relative.is_ascii()
        || relative.starts_with('/')
        || relative.ends_with('/')
        || relative.contains('\\')
        || relative.contains(':')
        || relative.bytes().any(|value| value <= 0x1f || value == 0x7f)
    {
        return fail(format!(
            "CosyVoice model manifest contains an unsafe path: {relative:?}"
        ));
    }
    let components = relative.split('/').collect::<Vec<_>>();
    for component in &components {
        if component.is_empty()
            || *component == "."
            || *component == ".."
            || component.ends_with('.')
            || component.ends_with(' ')
        {
            return fail(format!(
                "CosyVoice model manifest contains an unsafe component: {relative:?}"
            ));
        }
        let device_stem = component
            .split_once('.')
            .map(|(stem, _)| stem)
            .unwrap_or(component)
            .to_ascii_uppercase();
        let reserved = matches!(
            device_stem.as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$" | "CLOCK$"
        ) || (device_stem.len() == 4
            && (device_stem.starts_with("COM") || device_stem.starts_with("LPT"))
            && matches!(device_stem.as_bytes()[3], b'1'..=b'9'));
        if reserved {
            return fail(format!(
                "CosyVoice model manifest contains a reserved Windows path: {relative:?}"
            ));
        }
    }
    Ok(components)
}

fn cosyvoice_register_expected_directory(
    relative: &str,
    directories: &mut BTreeSet<String>,
    path_case: &mut BTreeMap<String, String>,
    path_kind: &mut BTreeMap<String, char>,
) -> Result<()> {
    let components = cosyvoice_manifest_path_components(relative)?;
    for index in 1..=components.len() {
        let directory = components[..index].join("/");
        let folded = directory.to_ascii_lowercase();
        if let Some(existing) = path_case.get(&folded) {
            if existing != &directory {
                return fail(format!(
                    "CosyVoice model manifest contains a case-colliding path: {existing} and {directory}"
                ));
            }
        } else {
            path_case.insert(folded.clone(), directory.clone());
        }
        if path_kind.get(&folded) == Some(&'F') {
            return fail(format!(
                "CosyVoice model manifest path is both a file and directory: {directory}"
            ));
        }
        path_kind.insert(folded, 'D');
        directories.insert(directory);
    }
    Ok(())
}

fn cosyvoice_expected_tree(source: &CosyvoiceModelSource) -> Result<CosyvoiceExpectedTree> {
    let mut files = BTreeMap::new();
    let mut directories = BTreeSet::new();
    let mut explicit_directories = BTreeSet::new();
    let mut path_case = BTreeMap::new();
    let mut path_kind = BTreeMap::new();

    for directory in &source.directories {
        let _ = cosyvoice_manifest_path_components(directory)?;
        if !explicit_directories.insert(directory.clone()) {
            return fail(format!(
                "CosyVoice model manifest contains a duplicate directory: {directory}"
            ));
        }
        cosyvoice_register_expected_directory(
            directory,
            &mut directories,
            &mut path_case,
            &mut path_kind,
        )?;
    }

    let mut byte_count = 0_u64;
    let mut empty_file_count = 0_u64;
    for file in &source.files {
        let components = cosyvoice_manifest_path_components(&file.path)?;
        if file.sha256.len() != 64
            || !file
                .sha256
                .bytes()
                .all(|value| value.is_ascii_digit() || matches!(value, b'a'..=b'f'))
        {
            return fail(format!(
                "CosyVoice model manifest SHA256 is invalid for {}",
                file.path
            ));
        }
        for index in 1..components.len() {
            cosyvoice_register_expected_directory(
                &components[..index].join("/"),
                &mut directories,
                &mut path_case,
                &mut path_kind,
            )?;
        }
        let folded = file.path.to_ascii_lowercase();
        if let Some(existing) = path_case.get(&folded) {
            if existing != &file.path {
                return fail(format!(
                    "CosyVoice model manifest contains a case-colliding path: {existing} and {}",
                    file.path
                ));
            }
        } else {
            path_case.insert(folded.clone(), file.path.clone());
        }
        if path_kind.get(&folded) == Some(&'D') {
            return fail(format!(
                "CosyVoice model manifest path is both a file and directory: {}",
                file.path
            ));
        }
        if files
            .insert(file.path.clone(), (file.bytes, file.sha256.clone()))
            .is_some()
        {
            return fail(format!(
                "CosyVoice model manifest contains a duplicate file: {}",
                file.path
            ));
        }
        path_kind.insert(folded, 'F');
        byte_count = byte_count.checked_add(file.bytes).ok_or_else(|| {
            EngineError::InstallFailed("CosyVoice model manifest byte count overflowed".to_string())
        })?;
        if file.bytes == 0 {
            empty_file_count += 1;
        }
    }
    if files.is_empty() {
        return fail("CosyVoice model manifest contains no files");
    }
    Ok(CosyvoiceExpectedTree {
        files,
        directories,
        byte_count,
        empty_file_count,
    })
}

fn parsed_cosyvoice_model_manifest() -> Result<CosyvoiceModelManifest> {
    if sha256_bytes(COSYVOICE_MODEL_MANIFEST.as_bytes()) != COSYVOICE_MODEL_MANIFEST_SHA256 {
        return fail("embedded CosyVoice model manifest identity drifted");
    }
    let manifest: CosyvoiceModelManifest = serde_json::from_str(COSYVOICE_MODEL_MANIFEST)?;
    if manifest.schema != "voxvulgi.cosyvoice_model_manifest.v1"
        || manifest.cosyvoice.provider != "huggingface"
        || manifest.cosyvoice.repo != COSYVOICE_MODEL_REPO
        || manifest.cosyvoice.revision != COSYVOICE_MODEL_REVISION
        || manifest.wetext.provider != "modelscope"
        || manifest.wetext.repo != COSYVOICE_WETEXT_REPO
        || manifest.wetext.revision != COSYVOICE_WETEXT_REVISION
    {
        return fail("embedded CosyVoice model manifest contract drifted");
    }
    let cosyvoice = cosyvoice_expected_tree(&manifest.cosyvoice)?;
    let wetext = cosyvoice_expected_tree(&manifest.wetext)?;
    if cosyvoice.files.len() != 19
        || cosyvoice.directories.len() != 2
        || cosyvoice.byte_count != 4_856_505_002
        || cosyvoice.empty_file_count != 0
        || wetext.files.len() != 26
        || wetext.directories.len() != 9
        || wetext.byte_count != 31_686_632
        || wetext.empty_file_count != 1
    {
        return fail("embedded CosyVoice model manifest topology or byte totals drifted");
    }
    Ok(manifest)
}

#[cfg(windows)]
fn cosyvoice_open_file_identity_and_link_count(file: &File) -> Result<(String, u64)> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
    };
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    let ok = unsafe { GetFileInformationByHandle(file.as_raw_handle() as _, &mut info) };
    if ok == 0 {
        return fail(format!(
            "CosyVoice file identity is unavailable: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok((
        format!(
            "{:08x}:{:08x}{:08x}",
            info.dwVolumeSerialNumber, info.nFileIndexHigh, info.nFileIndexLow
        ),
        info.nNumberOfLinks as u64,
    ))
}

#[cfg(unix)]
fn cosyvoice_open_file_identity_and_link_count(file: &File) -> Result<(String, u64)> {
    use std::os::unix::fs::MetadataExt;
    let metadata = file.metadata()?;
    Ok((
        format!("{:016x}:{:016x}", metadata.dev(), metadata.ino()),
        metadata.nlink(),
    ))
}

#[cfg(not(any(windows, unix)))]
fn cosyvoice_open_file_identity_and_link_count(_file: &File) -> Result<(String, u64)> {
    fail("CosyVoice file identity validation is unavailable on this platform")
}

#[cfg(windows)]
fn cosyvoice_directory_identity(path: &Path) -> Result<String> {
    reconciliation_directory_identity(path)
}

#[cfg(unix)]
fn cosyvoice_directory_identity(path: &Path) -> Result<String> {
    use std::os::unix::fs::MetadataExt;
    let metadata = std::fs::symlink_metadata(path)?;
    reject_link_or_reparse(path, &metadata)?;
    if !metadata.is_dir() {
        return fail(format!(
            "CosyVoice directory identity path is not a directory: {}",
            path.display()
        ));
    }
    Ok(format!("{:016x}:{:016x}", metadata.dev(), metadata.ino()))
}

#[cfg(not(any(windows, unix)))]
fn cosyvoice_directory_identity(_path: &Path) -> Result<String> {
    fail("CosyVoice directory identity validation is unavailable on this platform")
}

fn cosyvoice_hash_exact_regular_file(
    path: &Path,
    expected_bytes: u64,
    expected_sha256: &str,
) -> Result<String> {
    let path_metadata = std::fs::symlink_metadata(path)?;
    reject_link_or_reparse(path, &path_metadata)?;
    if !path_metadata.is_file() || path_metadata.len() != expected_bytes {
        return fail(format!(
            "CosyVoice model file is missing, non-regular, or wrong-sized: {}",
            path.display()
        ));
    }

    let mut file = File::open(path)?;
    let opened_metadata = file.metadata()?;
    let (opened_identity, opened_links) = cosyvoice_open_file_identity_and_link_count(&file)?;
    if !opened_metadata.is_file() || opened_metadata.len() != expected_bytes || opened_links != 1 {
        return fail(format!(
            "CosyVoice model file is not a unique regular file: {}",
            path.display()
        ));
    }
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let after_metadata = file.metadata()?;
    let path_after_metadata = std::fs::symlink_metadata(path)?;
    reject_link_or_reparse(path, &path_after_metadata)?;
    let reopened = File::open(path)?;
    let (reopened_identity, reopened_links) =
        cosyvoice_open_file_identity_and_link_count(&reopened)?;
    let reopened_metadata = reopened.metadata()?;
    let observed_sha256 = hex::encode(hasher.finalize());
    if after_metadata.len() != expected_bytes
        || !path_after_metadata.is_file()
        || path_after_metadata.len() != expected_bytes
        || !reopened_metadata.is_file()
        || reopened_metadata.len() != expected_bytes
        || opened_identity != reopened_identity
        || reopened_links != 1
        || observed_sha256 != expected_sha256
    {
        return fail(format!(
            "CosyVoice model file identity or content mismatch: {}",
            path.display()
        ));
    }
    Ok(observed_sha256)
}

fn reject_cosyvoice_reparse_ancestor_chain(path: &Path, label: &str) -> Result<()> {
    let mut current = Some(path);
    while let Some(candidate) = current {
        let metadata = std::fs::symlink_metadata(candidate).map_err(|error| {
            EngineError::InstallFailed(format!(
                "{label} path chain is unavailable at {}: {error}",
                candidate.display()
            ))
        })?;
        reject_link_or_reparse(candidate, &metadata).map_err(|error| {
            EngineError::InstallFailed(format!(
                "{label} path chain contains a link or reparse point at {}: {error}",
                candidate.display()
            ))
        })?;
        current = candidate.parent();
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn walk_exact_cosyvoice_model_tree(
    root: &Path,
    current: &Path,
    relative_parent: &str,
    expected: &CosyvoiceExpectedTree,
    observed_files: &mut BTreeSet<String>,
    observed_directories: &mut BTreeSet<String>,
    directory_identities: &mut BTreeMap<String, String>,
    records: &mut Vec<String>,
) -> Result<()> {
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(current)? {
        let entry = entry?;
        let name = entry.file_name().into_string().map_err(|_| {
            EngineError::InstallFailed(
                "CosyVoice model tree contains a non-Unicode filename".to_string(),
            )
        })?;
        entries.push((name, entry.path()));
    }
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    for (name, path) in entries {
        let relative = if relative_parent.is_empty() {
            name
        } else {
            format!("{relative_parent}/{name}")
        };
        let _ = cosyvoice_manifest_path_components(&relative)?;
        let metadata = std::fs::symlink_metadata(&path)?;
        reject_link_or_reparse(&path, &metadata)?;
        if metadata.is_dir() {
            if !expected.directories.contains(&relative)
                || !observed_directories.insert(relative.clone())
            {
                return fail(format!(
                    "CosyVoice model tree contains an unexpected or duplicate directory: {relative}"
                ));
            }
            directory_identities.insert(relative.clone(), cosyvoice_directory_identity(&path)?);
            records.push(format!("D\t{relative}\n"));
            walk_exact_cosyvoice_model_tree(
                root,
                &path,
                &relative,
                expected,
                observed_files,
                observed_directories,
                directory_identities,
                records,
            )?;
        } else if metadata.is_file() {
            let (bytes, sha256) = expected.files.get(&relative).ok_or_else(|| {
                EngineError::InstallFailed(format!(
                    "CosyVoice model tree contains an unexpected file: {relative}"
                ))
            })?;
            if !observed_files.insert(relative.clone()) {
                return fail(format!(
                    "CosyVoice model tree contains a duplicate file: {relative}"
                ));
            }
            let observed_sha256 =
                cosyvoice_hash_exact_regular_file(&root.join(&relative), *bytes, sha256)?;
            records.push(format!("F\t{relative}\t{bytes}\t{observed_sha256}\n"));
        } else {
            return fail(format!(
                "CosyVoice model tree contains a non-file/non-directory entry: {relative}"
            ));
        }
    }
    Ok(())
}

fn validate_exact_cosyvoice_model_tree(
    root: &Path,
    source: &CosyvoiceModelSource,
    label: &str,
) -> Result<TreeIdentity> {
    reject_cosyvoice_reparse_ancestor_chain(root, label)?;
    let root = canonical_directory(root, label)?;
    let expected = cosyvoice_expected_tree(source)?;
    let initial_root_identity = cosyvoice_directory_identity(&root)?;
    let mut observed_files = BTreeSet::new();
    let mut observed_directories = BTreeSet::new();
    let mut directory_identities = BTreeMap::new();
    let mut records = Vec::new();
    walk_exact_cosyvoice_model_tree(
        &root,
        &root,
        "",
        &expected,
        &mut observed_files,
        &mut observed_directories,
        &mut directory_identities,
        &mut records,
    )?;
    let expected_files = expected.files.keys().cloned().collect::<BTreeSet<_>>();
    if observed_files != expected_files || observed_directories != expected.directories {
        return fail(format!(
            "{label} exact tree is incomplete: {}",
            root.display()
        ));
    }
    if cosyvoice_directory_identity(&root)? != initial_root_identity {
        return fail(format!(
            "{label} root identity changed during validation: {}",
            root.display()
        ));
    }
    for (relative, identity) in &directory_identities {
        if cosyvoice_directory_identity(&root.join(relative))? != *identity {
            return fail(format!(
                "{label} directory identity changed during validation: {relative}"
            ));
        }
    }
    records.sort();
    let mut hasher = Sha256::new();
    for record in records {
        hasher.update(record.as_bytes());
    }
    Ok(TreeIdentity {
        path: display_canonical(&root),
        file_count: expected.files.len() as u64,
        directory_count: expected.directories.len() as u64,
        byte_count: expected.byte_count,
        empty_file_count: expected.empty_file_count,
        tree_sha256: hex::encode(hasher.finalize()),
        identity_contract: TREE_IDENTITY_CONTRACT.to_string(),
    })
}

fn insert_cosyvoice_tree_hashes(
    hashes: &mut BTreeMap<String, String>,
    cosyvoice_model: &TreeIdentity,
    cosyvoice_wetext: &TreeIdentity,
) {
    hashes.insert(
        "cosyvoice_model".to_string(),
        cosyvoice_model.tree_sha256.clone(),
    );
    hashes.insert(
        "cosyvoice_wetext".to_string(),
        cosyvoice_wetext.tree_sha256.clone(),
    );
}

fn insert_cosyvoice_tree_identities(
    trees: &mut BTreeMap<String, TreeIdentity>,
    cosyvoice_model: TreeIdentity,
    cosyvoice_wetext: TreeIdentity,
) {
    trees.insert("cosyvoice_model".to_string(), cosyvoice_model);
    trees.insert("cosyvoice_wetext".to_string(), cosyvoice_wetext);
}

fn comparable_tree(identity: &TreeIdentity) -> (u64, u64, u64, u64, &str) {
    (
        identity.file_count,
        identity.directory_count,
        identity.byte_count,
        identity.empty_file_count,
        identity.tree_sha256.as_str(),
    )
}

fn tree_identity(root: &Path, policy: TreePolicy) -> Result<TreeIdentity> {
    let root = canonical_directory(root, "tree root")?;
    let mut records = Vec::new();
    let mut pending = vec![root.clone()];
    let mut file_count = 0_u64;
    let mut directory_count = 0_u64;
    let mut byte_count = 0_u64;
    let mut empty_file_count = 0_u64;
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory)? {
            let entry = entry?;
            let path = entry.path();
            let relative = path.strip_prefix(&root).map_err(|_| {
                EngineError::InstallFailed(format!("entry escaped tree root: {}", path.display()))
            })?;
            if should_skip(relative, policy) {
                continue;
            }
            validate_relative_path(relative)?;
            let metadata = std::fs::symlink_metadata(&path)?;
            reject_link_or_reparse(&path, &metadata)?;
            let relative_display = relative.to_string_lossy().replace('\\', "/");
            if metadata.is_dir() {
                directory_count = directory_count.saturating_add(1);
                records.push(format!("D\t{relative_display}\n"));
                pending.push(path);
            } else if metadata.is_file() {
                file_count = file_count.saturating_add(1);
                byte_count = byte_count.saturating_add(metadata.len());
                if metadata.len() == 0 {
                    empty_file_count = empty_file_count.saturating_add(1);
                }
                records.push(format!(
                    "F\t{relative_display}\t{}\t{}\n",
                    metadata.len(),
                    sha256_file(&path)?
                ));
            } else {
                return fail(format!("unsupported payload entry: {}", path.display()));
            }
        }
    }
    records.sort();
    let mut hasher = Sha256::new();
    for record in records {
        hasher.update(record.as_bytes());
    }
    Ok(TreeIdentity {
        path: display_canonical(&root),
        file_count,
        directory_count,
        byte_count,
        empty_file_count,
        tree_sha256: hex::encode(hasher.finalize()),
        identity_contract: TREE_IDENTITY_CONTRACT.to_string(),
    })
}

fn should_skip(relative: &Path, policy: TreePolicy) -> bool {
    matches!(policy, TreePolicy::ExportedTools)
        && relative.components().any(|component| {
            matches!(component, Component::Normal(value) if value == "venv_cosyvoice" || value == "_voxvulgi_stale_python_artifacts")
        })
}

fn validate_relative_path(relative: &Path) -> Result<()> {
    if relative.as_os_str().is_empty() || relative.is_absolute() {
        return fail("payload entry has an empty or absolute relative path");
    }
    for component in relative.components() {
        match component {
            Component::Normal(value) => {
                let value = value.to_string_lossy();
                if value.is_empty() || value.contains(':') {
                    return fail(format!("unsafe payload path component: {value}"));
                }
            }
            _ => {
                return fail(format!(
                    "unsafe payload relative path: {}",
                    relative.display()
                ))
            }
        }
    }
    Ok(())
}

fn reject_link_or_reparse(path: &Path, metadata: &std::fs::Metadata) -> Result<()> {
    if metadata.file_type().is_symlink() {
        return fail(format!(
            "payload contains a symbolic link: {}",
            path.display()
        ));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return fail(format!(
                "payload contains a reparse point: {}",
                path.display()
            ));
        }
    }
    Ok(())
}

fn canonical_directory(path: &Path, label: &str) -> Result<PathBuf> {
    let input_metadata = std::fs::symlink_metadata(path).map_err(|error| {
        EngineError::InstallFailed(format!("{label} is missing at {}: {error}", path.display()))
    })?;
    reject_link_or_reparse(path, &input_metadata)?;
    let canonical = std::fs::canonicalize(path).map_err(|error| {
        EngineError::InstallFailed(format!("{label} is missing at {}: {error}", path.display()))
    })?;
    let metadata = std::fs::symlink_metadata(&canonical)?;
    reject_link_or_reparse(&canonical, &metadata)?;
    if !metadata.is_dir() {
        return fail(format!(
            "{label} is not a directory: {}",
            canonical.display()
        ));
    }
    Ok(canonical)
}

fn canonical_file(path: &Path, label: &str) -> Result<PathBuf> {
    let parent = path.parent().ok_or_else(|| {
        EngineError::InstallFailed(format!("{label} has no parent: {}", path.display()))
    })?;
    let parent = canonical_directory(parent, &format!("{label} parent"))?;
    let name = path.file_name().ok_or_else(|| {
        EngineError::InstallFailed(format!("{label} has no filename: {}", path.display()))
    })?;
    let canonical = parent.join(name);
    let metadata = std::fs::symlink_metadata(&canonical).map_err(|error| {
        EngineError::InstallFailed(format!(
            "{label} is missing at {}: {error}",
            canonical.display()
        ))
    })?;
    reject_link_or_reparse(&canonical, &metadata)?;
    if !metadata.is_file() {
        return fail(format!("{label} is not a file: {}", canonical.display()));
    }
    Ok(canonical)
}

fn sha256_file(path: &Path) -> Result<String> {
    let file = File::open(path)?;
    let mut reader = BufReader::with_capacity(1024 * 1024, file);
    let mut buffer = vec![0_u8; 1024 * 1024];
    let mut hasher = Sha256::new();
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|character| character.is_ascii_hexdigit())
}

fn write_new_receipt(path: &Path, receipt: &OfflinePayloadValidationReceipt) -> Result<()> {
    if path.exists() {
        return fail(format!("receipt already exists: {}", path.display()));
    }
    let parent = path.parent().ok_or_else(|| {
        EngineError::InstallFailed(format!("receipt path has no parent: {}", path.display()))
    })?;
    std::fs::create_dir_all(parent)?;
    let nonce = format!("{}.{}.tmp", std::process::id(), now_ms());
    let temp = parent.join(nonce);
    if temp.exists() {
        return fail(format!(
            "temporary receipt already exists: {}",
            temp.display()
        ));
    }
    let body = format!("{}\n", serde_json::to_string_pretty(receipt)?);
    let write_result = (|| -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)?;
        file.write_all(body.as_bytes())?;
        file.sync_all()?;
        Ok(())
    })();
    if let Err(error) = write_result {
        let _ = std::fs::remove_file(&temp);
        return Err(error.into());
    }
    std::fs::rename(&temp, path).map_err(|error| {
        let _ = std::fs::remove_file(&temp);
        EngineError::InstallFailed(format!(
            "failed to atomically publish validation receipt {}: {error}",
            path.display()
        ))
    })?;
    Ok(())
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("engine crate must live at product/engine")
        .to_path_buf()
}

fn display_canonical(path: &Path) -> String {
    let value = path.to_string_lossy();
    if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    value.strip_prefix(r"\\?\").unwrap_or(&value).to_string()
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn fail<T>(message: impl Into<String>) -> Result<T> {
    Err(EngineError::InstallFailed(message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    struct ReconciliationFixture {
        _root: tempfile::TempDir,
        stage: PathBuf,
        export: PathBuf,
        repo: PathBuf,
        receipt: PathBuf,
        external: ExternalSourceLockRequest,
        captured_roots: Vec<PathBuf>,
        journal_path: PathBuf,
        journal: serde_json::Value,
    }

    #[cfg(windows)]
    impl ReconciliationFixture {
        fn write_journal(&self) {
            let mut bytes = serde_json::to_vec(&self.journal).expect("journal JSON");
            bytes.push(b'\n');
            std::fs::write(&self.journal_path, bytes).expect("write journal");
        }

        fn verify(&self) -> Result<VerifiedReconciliationBinding> {
            verify_external_reconciliation_binding(
                &self.stage,
                &self.export,
                &self.repo,
                &self.receipt,
                "9.9.9-test",
                &self.external,
                &self
                    .captured_roots
                    .iter()
                    .map(PathBuf::as_path)
                    .collect::<Vec<_>>(),
            )
        }
    }

    #[cfg(windows)]
    fn write_json_owner(path: &Path, value: &serde_json::Value) -> String {
        let mut bytes = serde_json::to_vec(value).expect("owner JSON");
        bytes.push(b'\n');
        std::fs::write(path, bytes).expect("write owner");
        reconciliation_file_identity(path).expect("owner FILE_ID_INFO")
    }

    #[cfg(windows)]
    fn new_reconciliation_fixture() -> ReconciliationFixture {
        let root = tempfile::tempdir().expect("fixture root");
        let stage = root.path().join("stage");
        let export = root.path().join("export");
        let repo = root.path().join("repo");
        for directory in [&stage, &export, &repo] {
            std::fs::create_dir_all(directory).expect("trusted root");
        }
        let transaction_id = "0123456789abcdef0123456789abcdef";
        let owner_pid = std::process::id();
        let token_sha256 = "a".repeat(64);
        let record_sha256 = "b".repeat(64);
        let source_lock =
            repo.join("product/desktop/build_target/.voxvulgi_offline_payload_source.lock");
        std::fs::create_dir_all(source_lock.parent().expect("lock parent")).expect("lock parent");
        std::fs::write(&source_lock, b"source-lock-record").expect("source lock");
        let receipt = stage.join("payload_validation/fresh.json");
        std::fs::create_dir_all(receipt.parent().expect("receipt parent")).expect("receipt parent");

        let stage_transaction = stage
            .join(".voxvulgi_python_environment_transactions")
            .join(transaction_id);
        let export_transaction = export
            .join(".voxvulgi_python_environment_transactions")
            .join(transaction_id);
        let repo_transaction = repo
            .join("product/desktop/build_target/.voxvulgi_python_environment_transactions")
            .join(transaction_id);
        for transaction in [&stage_transaction, &export_transaction, &repo_transaction] {
            std::fs::create_dir_all(transaction).expect("transaction root");
        }

        let root_definitions = [
            ("stage", &stage_transaction, &stage),
            ("export", &export_transaction, &export),
            ("repo", &repo_transaction, &repo),
        ];
        let mut journal_roots = Vec::new();
        for (name, transaction, trusted) in root_definitions {
            let collection = transaction.parent().expect("collection");
            let collection_id =
                reconciliation_directory_identity(collection).expect("collection identity");
            let transaction_id_value =
                reconciliation_directory_identity(transaction).expect("transaction identity");
            let owner_path = transaction.join("owner.json");
            let owner = serde_json::json!({
                "schema": "voxvulgi.python_environment_transaction_root_owner.v1",
                "transaction_id": transaction_id,
                "name": name,
                "collection_root": display_canonical(collection),
                "collection_root_id": collection_id,
                "transaction_root": display_canonical(transaction),
                "transaction_root_id": transaction_id_value,
                "rename_identity_verified": true
            });
            let owner_file_id = write_json_owner(&owner_path, &owner);
            journal_roots.push(serde_json::json!({
                "name": name,
                "collection_root": display_canonical(collection),
                "collection_root_id": collection_id,
                "transaction_root": display_canonical(transaction),
                "transaction_root_id": transaction_id_value,
                "rename_identity_verified": true,
                "owner_path": display_canonical(&owner_path),
                "owner_file_id": owner_file_id,
                "trusted_root": display_canonical(trusted)
            }));
        }

        let lock_target = repo.join("product/engine/resources/tooling/final_environment_locks");
        let unit_definitions = [
            (
                "main",
                stage_transaction.join("main"),
                stage.join("tools/python/venv"),
                stage.clone(),
            ),
            (
                "cosyvoice",
                stage_transaction.join("cosyvoice"),
                stage.join("tools/python/venv_cosyvoice"),
                stage.clone(),
            ),
            (
                "export",
                export_transaction.join("export"),
                export.join("tools/python/venv"),
                export.clone(),
            ),
            (
                "lock_set",
                repo_transaction.join("lock_set"),
                lock_target.clone(),
                repo.clone(),
            ),
        ];
        let mut journal_units = Vec::new();
        for (name, workspace, target, trusted) in unit_definitions {
            let backup = workspace.join("backup");
            std::fs::create_dir_all(&backup).expect("unit backup");
            std::fs::write(backup.join("original.bin"), format!("old-{name}"))
                .expect("old target bytes");
            std::fs::create_dir_all(&target).expect("published target");
            std::fs::write(target.join("published.bin"), format!("new-{name}"))
                .expect("new target bytes");
            let workspace_id =
                reconciliation_directory_identity(&workspace).expect("workspace identity");
            let prepared_directory_id =
                reconciliation_directory_identity(&target).expect("published identity");
            let original_directory_id =
                reconciliation_directory_identity(&backup).expect("backup identity");
            let owner_path = workspace.join("owner.json");
            let prepared = workspace.join("prepared");
            let discard = workspace.join("discard");
            let owner = serde_json::json!({
                "schema": "voxvulgi.python_environment_transaction_unit_owner.v1",
                "transaction_id": transaction_id,
                "unit": name,
                "workspace": display_canonical(&workspace),
                "workspace_id": workspace_id,
                "target": display_canonical(&target),
                "prepared": display_canonical(&prepared),
                "backup": display_canonical(&backup),
                "discard": display_canonical(&discard),
                "prepared_directory_id": prepared_directory_id,
                "original_existed": true,
                "original_directory_id": original_directory_id
            });
            let owner_file_id = write_json_owner(&owner_path, &owner);
            journal_units.push(serde_json::json!({
                "name": name,
                "target": display_canonical(&target),
                "collection_root": display_canonical(workspace.parent().and_then(Path::parent).expect("collection")),
                "transaction_root": display_canonical(workspace.parent().expect("transaction")),
                "workspace": display_canonical(&workspace),
                "owner_path": display_canonical(&owner_path),
                "owner_file_id": owner_file_id,
                "prepared": display_canonical(&prepared),
                "backup": display_canonical(&backup),
                "discard": display_canonical(&discard),
                "trusted_root": display_canonical(&trusted),
                "workspace_id": workspace_id,
                "prepared_directory_id": prepared_directory_id,
                "original_existed": true,
                "original_directory_id": original_directory_id,
                "mutation_started": true,
                "state": "published"
            }));
        }

        let journal_path = stage.join(".voxvulgi_python_environment_transaction.json");
        let journal = serde_json::json!({
            "schema": "voxvulgi.python_environment_transaction.v2",
            "transaction_id": transaction_id,
            "owner_pid": owner_pid,
            "app_version": "9.9.9-test",
            "source_lock_path": display_canonical(&source_lock),
            "source_lock_token_sha256": token_sha256,
            "source_lock_record_sha256": record_sha256,
            "journal_path": display_canonical(&journal_path),
            "receipt_path": display_canonical(&receipt),
            "validation_receipt_sha256": null,
            "state": "validator_intent_durable",
            "cleanup_state": "pending",
            "cleanup_intent": null,
            "committed": false,
            "created_at_utc": "2026-08-26T00:00:00.0000000Z",
            "updated_at_utc": "2026-08-26T00:00:01.0000000Z",
            "committed_at_utc": null,
            "rolled_back_at_utc": null,
            "workspace_roots": journal_roots,
            "units": journal_units
        });
        let captured_roots = vec![stage.join("tools"), export.join("tools"), lock_target];
        let fixture = ReconciliationFixture {
            _root: root,
            stage,
            export,
            repo,
            receipt,
            external: ExternalSourceLockRequest {
                path: source_lock,
                owner_pid,
                transaction_id: transaction_id.to_string(),
                token_sha256,
                record_sha256,
            },
            captured_roots,
            journal_path,
            journal,
        };
        fixture.write_journal();
        fixture
    }

    #[test]
    fn tree_identity_includes_empty_directories_and_file_bytes() {
        let root = tempfile::tempdir().expect("root");
        std::fs::create_dir_all(root.path().join("empty")).expect("empty dir");
        std::fs::write(root.path().join("payload.bin"), b"bytes").expect("payload");
        let one = tree_identity(root.path(), TreePolicy::Exact).expect("identity one");
        assert_eq!(one.file_count, 1);
        assert_eq!(one.directory_count, 1);
        assert_eq!(one.byte_count, 5);
        std::fs::remove_dir(root.path().join("empty")).expect("remove empty");
        let two = tree_identity(root.path(), TreePolicy::Exact).expect("identity two");
        assert_ne!(one.tree_sha256, two.tree_sha256);
    }

    #[test]
    fn exported_tools_policy_excludes_only_governed_non_export_roots() {
        let root = tempfile::tempdir().expect("root");
        std::fs::create_dir_all(root.path().join("python/venv_cosyvoice")).expect("cosy");
        std::fs::write(
            root.path().join("python/venv_cosyvoice/python.exe"),
            b"cosy",
        )
        .expect("cosy exe");
        std::fs::create_dir_all(root.path().join("python/venv")).expect("main");
        std::fs::write(root.path().join("python/venv/python.exe"), b"main").expect("main exe");
        let filtered = tree_identity(root.path(), TreePolicy::ExportedTools).expect("filtered");
        assert_eq!(filtered.file_count, 1);
        assert_eq!(filtered.byte_count, 4);
    }

    #[test]
    fn external_reconciliation_workspaces_preserve_real_tools_tree_equality() {
        let root = tempfile::tempdir().expect("root");
        let stage = root.path().join("stage");
        let export = root.path().join("export");
        let stage_tools = stage.join("tools");
        let export_tools = export.join("tools");
        for tools in [&stage_tools, &export_tools] {
            std::fs::create_dir_all(tools.join("python/venv")).expect("main venv");
            std::fs::write(tools.join("python/venv/python.exe"), b"same-main")
                .expect("main python");
        }
        std::fs::create_dir_all(stage_tools.join("python/venv_cosyvoice"))
            .expect("stage-only CosyVoice venv");
        std::fs::write(
            stage_tools.join("python/venv_cosyvoice/python.exe"),
            b"stage-only-cosy",
        )
        .expect("CosyVoice python");

        let stage_initial = tree_identity(&stage_tools, TreePolicy::ExportedTools)
            .expect("initial stage tools identity");
        let export_initial =
            tree_identity(&export_tools, TreePolicy::Exact).expect("initial export tools identity");
        assert_eq!(
            comparable_tree(&stage_initial),
            comparable_tree(&export_initial)
        );

        let legacy_backup = stage_tools.join("python/.venv.reconcile_deadbeef.backup");
        std::fs::create_dir_all(&legacy_backup).expect("legacy backup");
        std::fs::write(legacy_backup.join("python.exe"), b"old-main").expect("legacy bytes");
        let stage_with_legacy_backup = tree_identity(&stage_tools, TreePolicy::ExportedTools)
            .expect("stage identity with legacy backup");
        assert_ne!(
            comparable_tree(&stage_with_legacy_backup),
            comparable_tree(&export_initial),
            "a reconciler generation inside tools must be visible to the real validator snapshot"
        );
        std::fs::remove_dir_all(&legacy_backup).expect("remove legacy backup");

        let transaction_id = "0123456789abcdef0123456789abcdef";
        let stage_workspace = stage
            .join(".voxvulgi_python_environment_transactions")
            .join(transaction_id)
            .join("main");
        let export_workspace = export
            .join(".voxvulgi_python_environment_transactions")
            .join(transaction_id)
            .join("export");
        for workspace in [&stage_workspace, &export_workspace] {
            std::fs::create_dir_all(workspace.join("prepared")).expect("prepared workspace");
            std::fs::create_dir_all(workspace.join("backup")).expect("backup workspace");
            std::fs::create_dir_all(workspace.join("discard")).expect("discard workspace");
            std::fs::write(workspace.join("owner.json"), b"external-owner")
                .expect("external owner");
            std::fs::write(workspace.join("backup/python.exe"), b"different-old-bytes")
                .expect("external backup bytes");
        }

        let stage_final = tree_identity(&stage_tools, TreePolicy::ExportedTools)
            .expect("final stage tools identity");
        let export_final =
            tree_identity(&export_tools, TreePolicy::Exact).expect("final export tools identity");
        assert_eq!(
            comparable_tree(&stage_final),
            comparable_tree(&export_final)
        );
        assert_eq!(stage_initial.tree_sha256, stage_final.tree_sha256);
        assert_eq!(export_initial.tree_sha256, export_final.tree_sha256);
    }

    #[cfg(windows)]
    #[test]
    fn production_reconciliation_attestation_accepts_exact_external_topology() {
        let fixture = new_reconciliation_fixture();
        let binding = fixture.verify().expect("exact production binding");
        assert_eq!(binding.attestation_roots.len(), 3);
        assert_eq!(binding.attestation_units.len(), 4);
        assert_reconciliation_binding_unchanged(&binding).expect("stable final rehash");
    }

    #[cfg(windows)]
    #[test]
    fn production_reconciliation_attestation_rejects_forged_state_paths_ids_and_topology() {
        let mut wrong_state = new_reconciliation_fixture();
        wrong_state.journal["state"] = serde_json::json!("validated");
        wrong_state.write_journal();
        assert!(wrong_state.verify().is_err());

        let mut forged_id = new_reconciliation_fixture();
        forged_id.journal["units"][0]["workspace_id"] =
            serde_json::json!("0000000000000000:00000000000000000000000000000000");
        forged_id.write_journal();
        assert!(forged_id.verify().is_err());

        let mut forged_path = new_reconciliation_fixture();
        forged_path.journal["units"][0]["workspace"] =
            serde_json::json!(display_canonical(&forged_path.stage.join("outside")));
        forged_path.write_journal();
        assert!(forged_path.verify().is_err());

        let mut missing_root = new_reconciliation_fixture();
        missing_root.journal["workspace_roots"]
            .as_array_mut()
            .expect("root array")
            .pop();
        missing_root.write_journal();
        assert!(missing_root.verify().is_err());

        let extra_workspace = new_reconciliation_fixture();
        let stage_transaction = PathBuf::from(
            extra_workspace.journal["workspace_roots"][0]["transaction_root"]
                .as_str()
                .expect("stage transaction"),
        );
        std::fs::create_dir(stage_transaction.join("unexpected")).expect("extra workspace");
        assert!(extra_workspace.verify().is_err());

        let legacy_marker = new_reconciliation_fixture();
        std::fs::write(
            legacy_marker.captured_roots[0].join(".voxvulgi_reconcile_owner.json"),
            b"forbidden",
        )
        .expect("legacy marker");
        assert!(legacy_marker.verify().is_err());

        let cross_transaction_owner = new_reconciliation_fixture();
        let unit_owner_path = PathBuf::from(
            cross_transaction_owner.journal["units"][0]["owner_path"]
                .as_str()
                .expect("unit owner path"),
        );
        let mut owner: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&unit_owner_path).expect("unit owner bytes"))
                .expect("unit owner JSON");
        owner["transaction_id"] = serde_json::json!("ffffffffffffffffffffffffffffffff");
        write_json_owner(&unit_owner_path, &owner);
        assert!(cross_transaction_owner.verify().is_err());
    }

    #[cfg(windows)]
    #[test]
    fn production_reconciliation_attestation_rejects_replaced_journal_and_owner_objects() {
        let journal_replaced = new_reconciliation_fixture();
        let binding = journal_replaced.verify().expect("initial journal binding");
        let journal_bytes = std::fs::read(&journal_replaced.journal_path).expect("journal bytes");
        let old_journal_id = reconciliation_file_identity(&journal_replaced.journal_path)
            .expect("old journal identity");
        std::fs::remove_file(&journal_replaced.journal_path).expect("remove journal");
        std::fs::write(&journal_replaced.journal_path, journal_bytes).expect("replace journal");
        let new_journal_id = reconciliation_file_identity(&journal_replaced.journal_path)
            .expect("new journal identity");
        assert_ne!(old_journal_id, new_journal_id);
        assert!(assert_reconciliation_binding_unchanged(&binding).is_err());

        let owner_replaced = new_reconciliation_fixture();
        let binding = owner_replaced.verify().expect("initial owner binding");
        let owner_path = PathBuf::from(
            owner_replaced.journal["units"][0]["owner_path"]
                .as_str()
                .expect("owner path"),
        );
        let owner_bytes = std::fs::read(&owner_path).expect("owner bytes");
        let old_owner_id = reconciliation_file_identity(&owner_path).expect("old owner identity");
        std::fs::remove_file(&owner_path).expect("remove owner");
        std::fs::write(&owner_path, owner_bytes).expect("replace owner");
        let new_owner_id = reconciliation_file_identity(&owner_path).expect("new owner identity");
        assert_ne!(old_owner_id, new_owner_id);
        assert!(owner_replaced.verify().is_err());
        assert!(assert_reconciliation_binding_unchanged(&binding).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn production_reconciliation_attestation_rejects_owner_reparse_when_supported() {
        use std::os::windows::fs::symlink_file;

        let fixture = new_reconciliation_fixture();
        let owner_path = PathBuf::from(
            fixture.journal["units"][0]["owner_path"]
                .as_str()
                .expect("owner path"),
        );
        let replacement = owner_path.with_extension("replacement.json");
        std::fs::write(
            &replacement,
            std::fs::read(&owner_path).expect("owner bytes"),
        )
        .expect("replacement owner");
        std::fs::remove_file(&owner_path).expect("remove owner");
        if symlink_file(&replacement, &owner_path).is_ok() {
            assert!(fixture.verify().is_err());
        }
    }

    fn test_cosyvoice_source(
        directories: &[&str],
        files: &[(&str, &[u8])],
    ) -> CosyvoiceModelSource {
        CosyvoiceModelSource {
            provider: "fixture".to_string(),
            repo: "fixture/repo".to_string(),
            revision: "0123456789abcdef0123456789abcdef01234567".to_string(),
            directories: directories
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            files: files
                .iter()
                .map(|(path, bytes)| CosyvoiceModelFile {
                    path: (*path).to_string(),
                    bytes: bytes.len() as u64,
                    sha256: sha256_bytes(bytes),
                })
                .collect(),
        }
    }

    fn write_test_cosyvoice_tree(root: &Path, source: &CosyvoiceModelSource) {
        std::fs::create_dir_all(root).expect("model root");
        for directory in &source.directories {
            std::fs::create_dir_all(root.join(directory)).expect("model directory");
        }
        for file in &source.files {
            let path = root.join(&file.path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("model file parent");
            }
            let bytes = match file.path.as_str() {
                "nested/model.bin" => b"model".as_slice(),
                "zero.py" => b"".as_slice(),
                "a.bin" => b"abc".as_slice(),
                _ => panic!("test fixture lacks bytes for {}", file.path),
            };
            assert_eq!(sha256_bytes(bytes), file.sha256);
            std::fs::write(path, bytes).expect("model file");
        }
    }

    #[test]
    fn embedded_cosyvoice_manifest_has_complete_exact_governed_topology() {
        assert_eq!(
            sha256_bytes(COSYVOICE_MODEL_MANIFEST.as_bytes()),
            COSYVOICE_MODEL_MANIFEST_SHA256
        );
        let manifest = parsed_cosyvoice_model_manifest().expect("embedded model manifest");
        let cosyvoice = cosyvoice_expected_tree(&manifest.cosyvoice).expect("CosyVoice topology");
        let wetext = cosyvoice_expected_tree(&manifest.wetext).expect("wetext topology");
        assert_eq!(cosyvoice.files.len(), 19);
        assert_eq!(cosyvoice.directories.len(), 2);
        assert_eq!(cosyvoice.byte_count, 4_856_505_002);
        assert_eq!(cosyvoice.empty_file_count, 0);
        assert_eq!(wetext.files.len(), 26);
        assert_eq!(wetext.directories.len(), 9);
        assert_eq!(wetext.byte_count, 31_686_632);
        assert_eq!(wetext.empty_file_count, 1);
    }

    #[test]
    fn exact_cosyvoice_tree_emits_complete_aggregate_identity() {
        let root = tempfile::tempdir().expect("root");
        let tree = root.path().join("tree");
        let source = test_cosyvoice_source(
            &["nested", "empty"],
            &[("nested/model.bin", b"model"), ("zero.py", b"")],
        );
        write_test_cosyvoice_tree(&tree, &source);
        let identity =
            validate_exact_cosyvoice_model_tree(&tree, &source, "fixture").expect("exact tree");
        assert_eq!(identity.file_count, 2);
        assert_eq!(identity.directory_count, 2);
        assert_eq!(identity.byte_count, 5);
        assert_eq!(identity.empty_file_count, 1);
        assert!(valid_sha256(&identity.tree_sha256));
        assert_eq!(identity.identity_contract, TREE_IDENTITY_CONTRACT);

        let mut hashes = BTreeMap::new();
        insert_cosyvoice_tree_hashes(&mut hashes, &identity, &identity);
        assert_eq!(hashes.get("cosyvoice_model"), Some(&identity.tree_sha256));
        assert_eq!(hashes.get("cosyvoice_wetext"), Some(&identity.tree_sha256));
        let mut trees = BTreeMap::new();
        insert_cosyvoice_tree_identities(&mut trees, identity.clone(), identity.clone());
        assert_eq!(trees.get("cosyvoice_model"), Some(&identity));
        assert_eq!(trees.get("cosyvoice_wetext"), Some(&identity));
    }

    #[test]
    fn exact_cosyvoice_tree_rejects_tamper_missing_and_extra_entries() {
        let source = test_cosyvoice_source(&[], &[("a.bin", b"abc")]);

        let tampered = tempfile::tempdir().expect("tampered");
        write_test_cosyvoice_tree(tampered.path(), &source);
        std::fs::write(tampered.path().join("a.bin"), b"xyz").expect("same-size tamper");
        assert!(validate_exact_cosyvoice_model_tree(tampered.path(), &source, "tampered").is_err());

        let missing = tempfile::tempdir().expect("missing");
        assert!(validate_exact_cosyvoice_model_tree(missing.path(), &source, "missing").is_err());

        let extra = tempfile::tempdir().expect("extra");
        write_test_cosyvoice_tree(extra.path(), &source);
        std::fs::write(extra.path().join("unexpected.bin"), b"extra").expect("extra file");
        assert!(validate_exact_cosyvoice_model_tree(extra.path(), &source, "extra").is_err());
        std::fs::remove_file(extra.path().join("unexpected.bin")).expect("remove extra file");
        std::fs::create_dir(extra.path().join("unexpected_dir")).expect("extra directory");
        assert!(validate_exact_cosyvoice_model_tree(extra.path(), &source, "extra dir").is_err());
    }

    #[test]
    fn exact_cosyvoice_tree_rejects_hardlinked_files() {
        let root = tempfile::tempdir().expect("root");
        let outside = root.path().join("outside.bin");
        let tree = root.path().join("tree");
        std::fs::create_dir(&tree).expect("tree");
        std::fs::write(&outside, b"abc").expect("outside file");
        std::fs::hard_link(&outside, tree.join("a.bin")).expect("hardlink fixture");
        let source = test_cosyvoice_source(&[], &[("a.bin", b"abc")]);
        assert!(validate_exact_cosyvoice_model_tree(&tree, &source, "hardlink").is_err());
    }

    #[test]
    fn cosyvoice_manifest_rejects_unsafe_and_colliding_paths() {
        for unsafe_path in [
            "../escape.bin",
            "CON/file.bin",
            "nested\\file.bin",
            "nested//file.bin",
            "nested./file.bin",
            "nested/file.bin ",
            "nested/file.bin:stream",
        ] {
            let source = test_cosyvoice_source(&[], &[(unsafe_path, b"abc")]);
            assert!(
                cosyvoice_expected_tree(&source).is_err(),
                "unsafe path unexpectedly accepted: {unsafe_path}"
            );
        }

        let case_collision =
            test_cosyvoice_source(&[], &[("Folder/a.bin", b"abc"), ("folder/b.bin", b"abc")]);
        assert!(cosyvoice_expected_tree(&case_collision).is_err());

        let file_directory_collision =
            test_cosyvoice_source(&[], &[("node", b"abc"), ("node/child.bin", b"abc")]);
        assert!(cosyvoice_expected_tree(&file_directory_collision).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn exact_cosyvoice_tree_rejects_symlink_when_supported() {
        use std::os::windows::fs::{symlink_dir, symlink_file};

        let root = tempfile::tempdir().expect("root");
        let outside = root.path().join("outside.bin");
        let tree = root.path().join("tree");
        std::fs::create_dir(&tree).expect("tree");
        std::fs::write(&outside, b"abc").expect("outside file");
        if symlink_file(&outside, tree.join("a.bin")).is_ok() {
            let source = test_cosyvoice_source(&[], &[("a.bin", b"abc")]);
            assert!(validate_exact_cosyvoice_model_tree(&tree, &source, "symlink").is_err());
        }

        let target_parent = root.path().join("target_parent");
        let target_tree = target_parent.join("tree");
        let linked_parent = root.path().join("linked_parent");
        let source = test_cosyvoice_source(&[], &[("a.bin", b"abc")]);
        write_test_cosyvoice_tree(&target_tree, &source);
        if symlink_dir(&target_parent, &linked_parent).is_ok() {
            assert!(validate_exact_cosyvoice_model_tree(
                &linked_parent.join("tree"),
                &source,
                "junctioned ancestor"
            )
            .is_err());
        }
    }

    #[test]
    fn required_file_rejects_zero_bytes() {
        let root = tempfile::tempdir().expect("root");
        let path = root.path().join("empty.bin");
        std::fs::write(&path, []).expect("empty");
        assert!(required_file("empty", &path).is_err());
    }

    #[test]
    fn sha_contract_rejects_malformed_values() {
        assert!(valid_sha256(&"a".repeat(64)));
        assert!(!valid_sha256(&"g".repeat(64)));
        assert!(!valid_sha256(&"a".repeat(63)));
    }

    #[test]
    fn validation_window_rejects_between_hash_mutation() {
        let initial = BTreeMap::from([("stage_tools".to_string(), "a".repeat(64))]);
        let final_trees = BTreeMap::from([("stage_tools".to_string(), "b".repeat(64))]);
        assert!(ensure_validation_window_unchanged(
            &initial,
            &final_trees,
            &"c".repeat(64),
            &"c".repeat(64)
        )
        .is_err());
        assert!(ensure_validation_window_unchanged(
            &initial,
            &initial,
            &"c".repeat(64),
            &"d".repeat(64)
        )
        .is_err());
    }

    #[cfg(windows)]
    #[test]
    fn source_lock_refuses_concurrent_validator_owner() {
        let root = tempfile::tempdir().expect("root");
        let path = root.path().join("source.lock");
        let first = open_owned_source_lock(&path).expect("first lock");
        let contender = open_owned_source_lock(&path);
        assert!(contender.is_err());
        drop(first);
        let replacement = open_owned_source_lock(&path).expect("replacement lock");
        drop(replacement);
    }
}
