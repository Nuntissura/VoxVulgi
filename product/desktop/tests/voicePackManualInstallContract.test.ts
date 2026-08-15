import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const desktopRoot = fileURLToPath(new URL("..", import.meta.url));
const repoRoot = join(desktopRoot, "..", "..");
const bridge = readFileSync(join(desktopRoot, "src-tauri", "src", "lib.rs"), "utf8");
const diagnostics = readFileSync(join(desktopRoot, "src", "pages", "DiagnosticsPage.tsx"), "utf8");
const jobs = readFileSync(join(repoRoot, "product", "engine", "src", "jobs.rs"), "utf8");
const tools = readFileSync(join(repoRoot, "product", "engine", "src", "tools.rs"), "utf8");

test("WP-0228 keeps Phase 2 installation operator-triggered", () => {
  assert.doesNotMatch(bridge, /phase2_auto_install_enqueue/);
  assert.doesNotMatch(bridge, /should_auto_install_phase2\s*\(/);
  assert.match(bridge, /fn jobs_enqueue_install_phase2_packs_v1/);
  assert.match(diagnostics, /Installs only after this explicit click/);
  assert.match(diagnostics, /if \(!ok\) return/);
  assert.match(diagnostics, /jobs_enqueue_install_phase2_packs_v1/);
});

test("WP-0229 skips satisfied packs and preserves an explicit all-pack force path", () => {
  for (const wrapper of [
    "install_spleeter_pack_if_needed",
    "install_diarization_pack_if_needed",
    "install_tts_preview_pack_if_needed",
    "install_tts_neural_local_v1_pack_if_needed",
    "install_tts_voice_preserving_local_v1_pack_if_needed",
    "install_voice_clone_cosyvoice_v1_pack_if_needed",
  ]) {
    assert.match(tools, new RegExp(`pub fn ${wrapper}\\b`), `${wrapper} must exist`);
  }
  assert.match(jobs, /#\[serde\(default\)\]\s*force: bool/);
  assert.match(jobs, /filter\(\|_\| !p\.force && tools::phase2_pack_step_satisfied/);
  assert.match(jobs, /if p\.force[\s\S]{0,180}install_spleeter_pack\(paths\)[\s\S]{0,180}install_spleeter_pack_if_needed\(paths\)/);
  assert.match(bridge, /force: Option<bool>[\s\S]{0,180}force\.unwrap_or\(false\)/);
  assert.match(diagnostics, /onClick=\{\(\) => enqueueInstallPhase2Packs\(false\)\}/);
  assert.match(diagnostics, /onClick=\{\(\) => enqueueInstallPhase2Packs\(true\)\}[\s\S]{0,100}Force reinstall all packs/);
  assert.match(diagnostics, /invoke\("jobs_enqueue_install_phase2_packs_v1", \{ force \}\)/);
});

test("WP-0228 preserves only still-valid completed steps when resuming", () => {
  assert.match(jobs, /read_to_string\(&latest_path\)[\s\S]{0,500}filter\(\|s\| s\.status == "done"\)/);
  assert.match(
    jobs,
    /prior_done_steps[\s\S]{0,800}phase2_pack_step_satisfied\(paths, &item\.id\)[\s\S]{0,500}status: "done"\.to_string\(\)/,
  );
  assert.match(jobs, /if state\.steps\[step_index\]\.status == "done"[\s\S]{0,80}continue/);
});
