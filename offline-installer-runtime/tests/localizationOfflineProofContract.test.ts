import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import test from "node:test";
import assert from "node:assert/strict";

const repoRoot = fileURLToPath(new URL("../..", import.meta.url));
const desktopRoot = join(repoRoot, "product", "desktop");

test("installed localization proof is an isolated headless-only bridge flight", () => {
  const bridge = readFileSync(join(desktopRoot, "src-tauri", "src", "lib.rs"), "utf8");
  assert.match(bridge, /\("POST", "\/agent\/localization_offline_proof"\)/);
  assert.match(bridge, /\("GET", "\/agent\/localization_offline_proof"\)/);
  assert.match(bridge, /agent_headless_isolated_app_paths/);
  assert.match(bridge, /VOXVULGI_AGENT_HEADLESS_BASE_DIR/);
  assert.match(bridge, /an installed-app localization proof is already running/);
  assert.match(bridge, /std::fs::canonicalize\(request\.media_path\.trim\(\)\)/);
});

test("installed one-shot proof requires explicit headless CLI inputs and exits before bridge startup", () => {
  const bridge = readFileSync(join(desktopRoot, "src-tauri", "src", "lib.rs"), "utf8");
  for (const value of [
    "--offline-localization-proof",
    "--proof-media",
    "--proof-root",
    "--proof-asr-lang",
    "VOXVULGI_AGENT_HEADLESS_BASE_DIR",
  ]) {
    assert.match(bridge, new RegExp(value));
  }
  assert.match(bridge, /OFFLINE_LOCALIZATION_PROOF_FLAG[\s\S]{0,1200}requires --agent-headless/);
  assert.match(bridge, /AGENT_HEADLESS_BASE_DIR_ENV[\s\S]{0,900}OFFLINE_LOCALIZATION_PROOF_ROOT_ARG/);
  const localizationExit = bridge.indexOf("if let Some(cli) = cli_offline_localization_proof");
  const updateSeedExit = bridge.indexOf("if let Some(cli) = cli_offline_update_preservation_seed");
  const bridgeStartup = bridge.indexOf("spawn_agent_bridge", localizationExit);
  assert.ok(localizationExit >= 0, "localization one-shot branch must exist");
  assert.ok(updateSeedExit > localizationExit, "update-preservation seed branch must follow localization proof");
  assert.ok(bridgeStartup > updateSeedExit, "both one-shot branches must exit before agent-bridge startup");
  assert.match(
    bridge.slice(localizationExit, updateSeedExit),
    /std::process::exit\(exit_code\);/,
  );
  assert.match(
    bridge.slice(updateSeedExit, bridgeStartup),
    /std::process::exit\(exit_code\);/,
  );
});

test("one-shot terminal receipt is canonical, atomic and proof-bound", () => {
  const bridge = readFileSync(join(desktopRoot, "src-tauri", "src", "lib.rs"), "utf8");
  assert.match(bridge, /join\("diagnostics"\)/);
  assert.match(bridge, /join\("offline_localization_proof"\)/);
  assert.match(bridge, /join\("proof_summary\.json"\)/);
  assert.match(bridge, /join\("terminal_status\.json"\)/);
  for (const field of [
    "schema_version",
    "kind",
    "outcome",
    "exit_code",
    "proof_root",
    "proof_media_path",
    "proof_media_sha256",
    "proof_summary_path",
    "proof_summary_sha256",
    "app_version",
    "engine_version",
    "started_at_ms",
    "finished_at_ms",
    "error",
    "proof_run_id",
    "owner_pid",
    "proof_root_canonical_path",
    "proof_root_volume_serial",
    "proof_root_file_id",
    "proof_output_dir_canonical_path",
    "proof_output_dir_volume_serial",
    "proof_output_dir_file_id",
  ]) {
    assert.match(bridge, new RegExp(`\\b${field}\\b`));
  }
  assert.match(bridge, /persistence::atomic_write_text/);
  assert.match(bridge, /summary\.media_sha256\.eq_ignore_ascii_case\(&media_sha256\)/);
  assert.match(bridge, /database\.shutdown_and_drain/);
  assert.match(bridge, /OFFLINE_LOCALIZATION_PROOF_EXIT_FAILED: i32 = 30/);
  assert.match(bridge, /OFFLINE_LOCALIZATION_PROOF_EXIT_CONCURRENT: i32 = 32/);
  assert.match(bridge, /Local\\\\VoxVulgi_OfflineProofRoot_/);
  assert.match(bridge, /CreateMutexW/);
  assert.match(bridge, /WAIT_ABANDONED/);
  assert.match(bridge, /FILE_ID_INFO/);
  assert.match(bridge, /FILE_FLAG_OPEN_REPARSE_POINT/);
  assert.match(bridge, /FILE_SHARE_READ \| FILE_SHARE_WRITE/);
  assert.match(bridge, /offline localization proof output must be empty before launch/);
  for (const name of [
    "terminal_status.json",
    "proof_summary.json",
    "localized_dub.wav",
    "localized_dub.mkv",
    "localization_export.zip",
    "voice_report.json",
  ]) {
    assert.match(bridge, new RegExp(name.replaceAll(".", "\\.")));
  }
});

test("installed localization proof uses local orchestrator stages and refuses dependency installation", () => {
  const proof = readFileSync(
    join(repoRoot, "product", "engine", "src", "localization_offline_proof.rs"),
    "utf8",
  );
  for (const stage of [
    "enqueue_import_local",
    "enqueue_localization_run_v1",
    "asr_local",
    "translate_local",
    "diarize_local_v1",
    "separate_audio_demucs_v1",
    "dub_voice_preserving_v1",
    "mix_dub_preview_v1",
    "mux_dub_preview_v1",
    "export_pack_v1",
  ]) {
    assert.match(proof, new RegExp(stage));
  }
  assert.match(proof, /HF_HUB_OFFLINE/);
  assert.match(proof, /PIP_NO_INDEX/);
  assert.match(proof, /cosyvoice_pack_status/);
  assert.match(proof, /verify_model_by_id/);
  assert.doesNotMatch(proof, /install_(ffmpeg|python|demucs|diarization|tts|voice_clone|model)/);
  assert.match(proof, /extension\(\)[\s\S]{0,180}eq_ignore_ascii_case\("mkv"\)/);
  assert.doesNotMatch(proof, /eq_ignore_ascii_case\("mp4"\)/);
});

test("proof receipt atomically publishes semantically validated current-flight artifacts", () => {
  const proof = readFileSync(
    join(repoRoot, "product", "engine", "src", "localization_offline_proof.rs"),
    "utf8",
  );
  assert.match(proof, /proof_summary\.json/);
  assert.match(proof, /sha256_file/);
  assert.match(proof, /publish_proof_artifact_atomic/);
  assert.match(proof, /publish_proof_json_atomic/);
  assert.match(proof, /create_new\(true\)/);
  assert.match(proof, /sync_all\(\)/);
  assert.match(proof, /validate_proof_mkv/);
  assert.match(proof, /validate_proof_export_zip/);
  assert.match(proof, /producer_job_id/);
  assert.match(proof, /proof_batch_ids/);
  assert.match(proof, /proof_started_at_ms/);
  assert.match(proof, /metadata\.len\(\) == 0/);
  assert.match(proof, /"import_local"\.to_string\(\)/);
  assert.match(proof, /mux: mux_artifact/);
  assert.match(proof, /mix: mix_artifact/);
  assert.match(proof, /export_pack: export_artifact/);
  assert.match(proof, /voice_report: voice_artifact/);
});
