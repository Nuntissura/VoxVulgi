import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import test from "node:test";
import assert from "node:assert/strict";

const root = fileURLToPath(new URL("..", import.meta.url));

function readRepoFile(...parts: string[]): string {
  return readFileSync(join(root, ...parts), "utf8");
}

function readRepoJson<T>(...parts: string[]): T {
  return JSON.parse(readRepoFile(...parts)) as T;
}

function functionBlock(source: string, name: string): string {
  const marker = `fn ${name}`;
  const publicMarker = `pub fn ${name}`;
  let start = source.indexOf(publicMarker);
  if (start === -1) start = source.indexOf(marker);
  assert.notEqual(start, -1, `${name} must exist`);
  const openBrace = source.indexOf("{", start);
  assert.notEqual(openBrace, -1, `${name} must have a body`);
  let depth = 0;
  for (let index = openBrace; index < source.length; index += 1) {
    const char = source[index];
    if (char === "{") depth += 1;
    if (char === "}") {
      depth -= 1;
      if (depth === 0) return source.slice(start, index + 1);
    }
  }
  assert.fail(`${name} body was not closed`);
}

test("voice pack status is tied to the current bundled lockfile", () => {
  const toolsSource = readRepoFile("..", "engine", "src", "tools.rs");

  for (const statusFn of [
    "tts_neural_local_v1_pack_status",
    "tts_voice_preserving_local_v1_pack_status",
  ]) {
    const block = functionBlock(toolsSource, statusFn);
    assert.match(
      block,
      /pack_install_satisfied\(paths,\s*"tts_/,
      `${statusFn} must require the current lockfile install state, not just imports or warmup markers`,
    );
    assert.match(
      block,
      /repair_required/,
      `${statusFn} must tell the UI when an installed-looking pack needs repair`,
    );
    assert.match(
      block,
      /status_detail/,
      `${statusFn} must expose a human-readable reason for repair`,
    );
    assert.match(
      block,
      /version_mismatches/,
      `${statusFn} must expose actual package version mismatches so stale completed journals do not look healthy`,
    );
    assert.match(
      block,
      /version_mismatches\.is_empty\(\)/,
      `${statusFn} must include actual installed package version checks in its installed decision`,
    );
  }
});

test("voice pack status stays runnable when only the install receipt is stale", () => {
  const toolsSource = readRepoFile("..", "engine", "src", "tools.rs");

  assert.match(
    toolsSource,
    /fn pack_lockfile_runtime_ready/,
    "voice-pack status needs a helper that separates runnable dependency state from stale install receipts",
  );

  for (const statusFn of [
    "tts_neural_local_v1_pack_status",
    "tts_voice_preserving_local_v1_pack_status",
  ]) {
    const block = functionBlock(toolsSource, statusFn);
    assert.match(
      block,
      /pack_lockfile_runtime_ready\(lockfile_ready,\s*versions_ready\)/,
      `${statusFn} must allow a runnable pack when installed package versions match the bundled lockfile but the old receipt SHA is stale`,
    );
    assert.match(
      block,
      /receipt|journal/i,
      `${statusFn} should surface stale install receipt detail without blocking voice runtime`,
    );
  }
});

test("voice-preserving lockfile keeps NumPy aligned with the Kokoro base pack", () => {
  const manifest = readRepoJson<{
    tts_voice_preserving_local_v1: { pinned_dependencies: string[] };
  }>("..", "engine", "resources", "tooling", "pinned_dependency_manifest.json");
  const lockfile = readRepoJson<{
    source_pins: string[];
    packages: Array<{ name: string; version: string }>;
  }>("..", "engine", "resources", "tooling", "lockfiles", "tts_voice_preserving_local_v1.lock.json");

  assert.ok(
    manifest.tts_voice_preserving_local_v1.pinned_dependencies.includes("numpy==1.26.4"),
    "OpenVoice dependency pins must keep NumPy at the Kokoro-compatible 1.26.4 version",
  );
  assert.ok(
    lockfile.source_pins.includes("numpy==1.26.4"),
    "OpenVoice lockfile must be generated with the same NumPy pin",
  );
  const numpyEntry = lockfile.packages.find((pkg) => pkg.name.toLowerCase() === "numpy");
  assert.equal(
    numpyEntry?.version,
    "1.26.4",
    "OpenVoice lockfile must not upgrade NumPy and break the Kokoro base pack",
  );
});

test("phase2 resume skips prior done steps only when current pack state is still satisfied", () => {
  const jobsSource = readRepoFile("..", "engine", "src", "jobs.rs");
  const block = functionBlock(jobsSource, "execute_job");
  assert.match(
    block,
    /phase2_pack_step_satisfied\(paths,\s*&item\.id\)/,
    "resumed voice-pack installs must re-run stale prior-done steps when the current lockfile/status no longer matches",
  );
});

test("lockfile installs clean stale Python metadata before force repair", () => {
  const toolsSource = readRepoFile("..", "engine", "src", "tools.rs");
  const block = functionBlock(toolsSource, "install_pack_from_lockfile");

  assert.match(
    block,
    /lockfile_source_pin_mismatches\(python,\s*pack_name\)/,
    "install mode must notice actual installed version drift, not only failed journals",
  );
  assert.match(
    block,
    /cleanup_stale_distribution_metadata\(python,\s*pack_name\)/,
    "force repairs must remove stale *.dist-info folders before pip reinstall",
  );
  assert.match(
    toolsSource,
    /fn cleanup_stale_distribution_metadata/,
    "engine must have a targeted stale dist-info cleanup helper",
  );
});

test("lockfile installs retry post-install metadata checks under filesystem lag", () => {
  const toolsSource = readRepoFile("..", "engine", "src", "tools.rs");
  const block = functionBlock(toolsSource, "install_pack_from_lockfile");

  assert.match(
    toolsSource,
    /PYTHON_POST_INSTALL_VERSION_CHECK_RETRIES/,
    "post-pip package metadata checks need a bounded retry window for Windows/AV lag under load",
  );
  assert.match(
    toolsSource,
    /fn lockfile_source_pin_mismatches_with_retries/,
    "engine must have a retrying lockfile version check helper",
  );
  assert.match(
    toolsSource,
    /fn python_distribution_version_from_site_packages/,
    "metadata checks need a filesystem dist-info fallback when importlib metadata lags under load",
  );
  assert.match(
    block,
    /lockfile_source_pin_mismatches_with_retries\(python,\s*pack_name\)/,
    "install_pack_from_lockfile must not fail on the first transient metadata miss after pip exits",
  );
});

test("python installer commands have a hard timeout and kill hung child processes", () => {
  const toolsSource = readRepoFile("..", "engine", "src", "tools.rs");
  const block = functionBlock(toolsSource, "run_python_checked");

  assert.match(
    toolsSource,
    /PYTHON_COMMAND_TIMEOUT_SECS/,
    "Python install and warmup commands need a bounded timeout under heavy load",
  );
  assert.match(
    block,
    /spawn\(\)/,
    "run_python_checked must spawn the process so it can enforce a timeout",
  );
  assert.match(
    block,
    /kill\(\)/,
    "run_python_checked must kill hung child processes after timeout",
  );
});

test("direct voice pack install commands run off the Tauri command lane", () => {
  const tauriSource = readRepoFile("src-tauri", "src", "lib.rs");

  for (const command of [
    "tools_tts_neural_local_v1_install",
    "tools_tts_voice_preserving_local_v1_install",
  ]) {
    assert.match(
      tauriSource,
      new RegExp(`async fn ${command}\\(`),
      `${command} must be async because package install/warmup can run for minutes under load`,
    );
    assert.match(
      tauriSource,
      new RegExp(`async fn ${command}\\([\\s\\S]{0,900}spawn_blocking`),
      `${command} must run install work in spawn_blocking`,
    );
  }
});

test("Diagnostics voice package headline prefers current runtime readiness over stale install journal", () => {
  const diagnosticsSource = readRepoFile("src", "pages", "DiagnosticsPage.tsx");

  assert.match(
    diagnosticsSource,
    /const\s+voicePackagesRuntimeReady\s*=/,
    "Diagnostics must compute current package readiness independently from the latest one-click install journal",
  );
  assert.match(
    diagnosticsSource,
    /voicePackagesRuntimeReady[\s\S]{0,220}\?\s*"Installed"[\s\S]{0,220}: phase2HasProblem/,
    "a stale failed phase2 journal must not make the headline say Interrupted when current voice runtime probes are ready",
  );
});

test("agent bridge handles long visual-debug requests without starving health and state", () => {
  const tauriSource = readRepoFile("src-tauri", "src", "lib.rs");
  const bridgeBlock = functionBlock(tauriSource, "spawn_agent_bridge");

  assert.match(
    bridgeBlock,
    /std::thread::spawn\(\s*move\s*\|\|\s*\{\s*handle_agent_request/,
    "the localhost agent bridge must handle each accepted connection on its own thread so dump/snapshot waits do not block health/state probes",
  );
});

test("startup offline bundle skips payload walk when localization runtime is already ready", () => {
  const tauriSource = readRepoFile("src-tauri", "src", "lib.rs");
  const applyBlock = functionBlock(tauriSource, "apply_offline_bundle_if_present");

  assert.match(
    tauriSource,
    /fn offline_bundle_runtime_already_ready/,
    "startup needs a fast runtime-readiness gate before walking a large offline payload zip",
  );
  assert.match(
    tauriSource,
    /fn offline_bundle_runtime_ready_from_flags/,
    "the readiness gate must be testable without spawning local tools",
  );
  assert.match(
    applyBlock,
    /patch_venv_pyvenv_cfg_best_effort\(paths\)[\s\S]{0,260}offline_bundle_runtime_already_ready\(paths\)/,
    "startup must repair pyvenv.cfg before runtime-readiness checks so moved builds do not call a stale staging Python",
  );
  assert.match(
    applyBlock,
    /offline_bundle_runtime_already_ready\(paths\)[\s\S]{0,220}write_offline_bundle_marker/,
    "when the runtime is already ready, startup must record the bundle marker and skip the expensive payload walk",
  );
  assert.match(
    tauriSource,
    /diarization_installed[\s\S]*neural_tts_installed[\s\S]*voice_preserving_installed/,
    "the fast skip must still require the localization diarization and voice-cloning runtime, not only generic tools",
  );
});

test("voice-preserving status checks OpenVoice runtime availability without importing heavy modules", () => {
  const toolsSource = readRepoFile("..", "engine", "src", "tools.rs");
  const block = functionBlock(toolsSource, "tts_voice_preserving_local_v1_pack_status");
  const statusPinBlock = functionBlock(toolsSource, "status_pin_names");

  assert.match(
    toolsSource,
    /fn python_module_available/,
    "status checks need a fast importlib.util.find_spec helper instead of importing runtime modules",
  );
  assert.match(
    block,
    /python_distribution_version\(&venv_python,\s*"MyShell-OpenVoice"\)/,
    "OpenVoice installed from git reports distribution metadata as MyShell-OpenVoice, not openvoice",
  );
  assert.match(
    block,
    /python_module_available\(&venv_python,\s*"openvoice\.api"\)/,
    "OpenVoice status must verify the runnable openvoice.api module exists",
  );
  assert.match(
    statusPinBlock,
    /"tts_voice_preserving_local_v1"\s*=>\s*Some\(&\[[\s\S]*"numpy"/,
    "OpenVoice status must treat the shared NumPy pin as critical because a NumPy drift breaks Kokoro",
  );
  assert.doesNotMatch(
    block,
    /python_module_version\(&venv_python,\s*"openvoice"\)/,
    "status must not import openvoice just to decide whether Diagnostics is healthy",
  );
});

test("diarization status is metadata-only and avoids startup import probes", () => {
  const toolsSource = readRepoFile("..", "engine", "src", "tools.rs");
  const block = functionBlock(toolsSource, "diarization_pack_status");
  const statusPinBlock = functionBlock(toolsSource, "status_pin_names");

  assert.match(
    block,
    /python_distribution_versions/,
    "diarization status must use package metadata rather than importing runtime modules during startup",
  );
  assert.doesNotMatch(
    block,
    /validate_diarization_runtime/,
    "startup/status checks must not run the full diarization runtime validation path",
  );
  assert.doesNotMatch(
    block,
    /VoiceEncoder/,
    "startup/status checks must not instantiate Resemblyzer VoiceEncoder",
  );
  assert.match(
    statusPinBlock,
    /"diarization"\s*=>\s*Some\(&\[[\s\S]*"scikit-learn"/,
    "diarization status must still compare the installed package metadata against the bundled lockfile pins",
  );
  assert.match(
    block,
    /let installed = all_required_present;/,
    "diarization installed state must represent startup runtime presence, not force offline hydration for lockfile receipt drift",
  );
  assert.match(
    block,
    /repair_required[\s\S]{0,260}versions_ready/,
    "diarization lockfile drift should surface as repair guidance without blocking startup readiness",
  );
  assert.doesNotMatch(
    block,
    /python_modules_available|python_module_available|find_spec/,
    "startup/status checks must not use importlib find_spec because dotted module probes can still execute heavy parent imports",
  );
});

test("vvwatch marks voice-preserving pack unsatisfied when runtime modules are missing", () => {
  const watchSource = readRepoFile("..", "..", "governance", "scripts", "vv_watch.ps1");
  const blockStart = watchSource.indexOf("function Get-VoicePackInstallProbe");
  assert.notEqual(blockStart, -1, "vvwatch must expose Get-VoicePackInstallProbe");
  const blockEnd = watchSource.indexOf("\nfunction ", blockStart + 1);
  const block = watchSource.slice(blockStart, blockEnd === -1 ? undefined : blockEnd);

  assert.match(
    watchSource,
    /module_specs/,
    "python environment probe must include importlib.util.find_spec module availability",
  );
  assert.match(
    watchSource,
    /MyShell-OpenVoice/,
    "vvwatch must recognize OpenVoice git installs by their real distribution name",
  );
  assert.match(
    block,
    /runtime_modules_satisfied/,
    "voice pack satisfaction must include runtime module availability, not just lockfile deps",
  );
  assert.match(
    block,
    /bundled_lockfile_satisfied/,
    "vvwatch must distinguish stale app-data rendered requirements from the current bundled repo lockfile",
  );
  assert.match(
    block,
    /openvoice\.api/,
    "voice-preserving pack must require the openvoice.api runtime module",
  );
});

test("vvwatch marks stale freeze reports when report pid differs from the live app pid", () => {
  const watchSource = readRepoFile("..", "..", "governance", "scripts", "vv_watch.ps1");
  const traceBlockStart = watchSource.indexOf("function Get-TraceSummary");
  assert.notEqual(traceBlockStart, -1, "vvwatch must expose Get-TraceSummary");
  const traceBlockEnd = watchSource.indexOf("\nfunction ", traceBlockStart + 1);
  const traceBlock = watchSource.slice(
    traceBlockStart,
    traceBlockEnd === -1 ? undefined : traceBlockEnd,
  );

  assert.match(
    traceBlock,
    /current_process_pid/,
    "trace summary must include the live app pid used for stale-report comparison",
  );
  assert.match(
    traceBlock,
    /report_stale/,
    "trace summary must flag freeze_report_latest.json as stale when it belongs to an older process",
  );
  assert.match(
    watchSource,
    /Stale freeze report/i,
    "human vvwatch summaries must call out stale freeze reports explicitly",
  );
});

test("vvwatch separates stale rendered-requirement mismatches from current bundled lockfile failures", () => {
  const watchSource = readRepoFile("..", "..", "governance", "scripts", "vv_watch.ps1");

  assert.match(
    watchSource,
    /stale rendered requirement mismatch/i,
    "human vvwatch summaries must label mismatches from old app-data requirements as stale rendered-requirement evidence",
  );
  assert.match(
    watchSource,
    /current bundled lockfile mismatch/i,
    "human vvwatch summaries must label current bundled lockfile mismatches separately from stale app-data noise",
  );
  assert.doesNotMatch(
    watchSource,
    /\s-\s+mismatch \$\(\$mismatch\.package\): required/,
    "human vvwatch summaries must not print stale app-data mismatches as generic current failures",
  );
});

test("vvwatch flags live app version drift from the repo under test", () => {
  const watchSource = readRepoFile("..", "..", "governance", "scripts", "vv_watch.ps1");

  assert.match(
    watchSource,
    /repo_desktop_version/,
    "vvwatch summary JSON must include the repo desktop version so stale installed builds are obvious",
  );
  assert.match(
    watchSource,
    /app_version_mismatch/,
    "vvwatch summary JSON must flag when the live app version differs from the repo under test",
  );
  assert.match(
    watchSource,
    /App version mismatch/i,
    "human vvwatch summaries must call out live-app versus repo version drift",
  );
});

test("vvwatch does not use stale freeze report app_version as live app version", () => {
  const watchSource = readRepoFile("..", "..", "governance", "scripts", "vv_watch.ps1");
  const summaryStart = watchSource.indexOf("function Write-Summary");
  assert.notEqual(summaryStart, -1, "vvwatch must expose Write-Summary");
  const summaryEnd = watchSource.indexOf("\nfunction ", summaryStart + 1);
  const summaryBlock = watchSource.slice(summaryStart, summaryEnd === -1 ? undefined : summaryEnd);

  assert.match(
    summaryBlock,
    /\$latestTrace\.report_stale/,
    "summary generation must inspect whether freeze_report_latest.json belongs to an older process",
  );
  assert.match(
    summaryBlock,
    /liveAppVersion[\s\S]{0,160}-not\s+\$latestTrace\.report_stale/,
    "summary generation must only use trace app_version as live app version when the trace pid matches the current app pid",
  );
});
