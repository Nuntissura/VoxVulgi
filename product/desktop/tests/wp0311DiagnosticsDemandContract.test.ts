import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import { DIAGNOSTICS_OPERATION_REGISTRY } from "../src/lib/diagnosticsDemandCoordinator.ts";

const root = fileURLToPath(new URL("..", import.meta.url));
const repoRoot = join(root, "..", "..");
const read = (...parts: string[]) => readFileSync(join(root, ...parts), "utf8");
const readRepo = (...parts: string[]) => readFileSync(join(repoRoot, ...parts), "utf8");

function sliceBetween(source: string, startMarker: string, endMarker: string): string {
  const start = source.indexOf(startMarker);
  assert.notEqual(start, -1, `missing start marker: ${startMarker}`);
  const end = source.indexOf(endMarker, start + startMarker.length);
  assert.notEqual(end, -1, `missing end marker: ${endMarker}`);
  return source.slice(start, end);
}

test("Diagnostics mounts cheap-first and demands remaining sections without timer fan-out", () => {
  const source = read("src", "pages", "DiagnosticsPage.tsx");
  const mountStart = source.indexOf("const sectionDemandLoaders");
  const mountEnd = source.indexOf("const modelGroups", mountStart);
  const mount = source.slice(mountStart, mountEnd);
  assert.match(mount, /loadBuildSection/);
  assert.match(mount, /IntersectionObserver/);
  assert.doesNotMatch(mount, /setTimeout/);
  assert.doesNotMatch(mount, /loadToolsSupplementalData\(\).*220/);
  assert.match(source, /"idle" \| "queued" \| "loading" \| "ready" \| "stale" \| "failed"/);
  assert.match(source, /verified_at_ms/);
  assert.match(source, /freshness_ms/);
});

test("ordinary Diagnostics protection load never replays full history", () => {
  const source = read("src", "pages", "DiagnosticsPage.tsx");
  const start = source.indexOf("const loadTraceSection");
  const end = source.indexOf("const replayYoutubeProtectionHistory", start);
  const body = source.slice(start, end);
  assert.match(body, /loadYoutubeProtectionSnapshot/);
  assert.doesNotMatch(body, /youtube_protection_history_replay/);
  assert.match(source, /Replay bounded history/);
});

test("Diagnostics and Options share the same protection snapshot and owner generations", () => {
  const diagnostics = read("src", "pages", "DiagnosticsPage.tsx");
  const options = read("src", "pages", "OptionsPage.tsx");
  for (const source of [diagnostics, options]) {
    assert.match(source, /diagnosticsDemandCoordinator\.request/);
    assert.match(source, /createDemandGeneration/);
    assert.match(source, /cancelGeneration/);
    assert.match(source, /loadYoutubeProtectionSnapshot/);
  }
});

test("coordinator emits complete bounded scheduler receipts with child provenance", () => {
  const source = read("src", "lib", "diagnosticsDemandCoordinator.ts");
  assert.match(source, /diagnostics_demand_scheduler/);
  for (const phase of ["queued", "admitted", "shared", "cancel_requested", "waiter_detached", "frontend_abort_signaled", "backend_cancel_observed", "terminal", "superseded_completion"]) {
    assert.match(source, new RegExp(`"${phase}"`));
  }
  for (const field of ["semantic_key", "request_identity", "cost_class", "queue_wait_ms", "execution_ms", "child_pids"]) {
    assert.match(source, new RegExp(field));
  }
});

test("every literal Diagnostics invoke is classified in its operation registry", () => {
  const source = read("src", "pages", "DiagnosticsPage.tsx");
  const invoked = new Set<string>();
  const invokePattern = /\binvoke(?:<[^()]*>)?\s*\(\s*["']([A-Za-z0-9_]+)["']/g;
  for (const match of source.matchAll(invokePattern)) invoked.add(match[1]);
  assert.ok(invoked.size > 50, "invoke extraction unexpectedly missed most Diagnostics commands");

  const classified = new Set(
    DIAGNOSTICS_OPERATION_REGISTRY
      .filter((entry) => entry.ownerModules.includes("diagnostics"))
      .flatMap((entry) => [...entry.commands]),
  );
  assert.deepEqual(
    [...invoked].filter((command) => !classified.has(command)).sort(),
    [],
    "Diagnostics contains unclassified native work that can bypass cost and trigger policy",
  );
});

test("Diagnostics derives section state from every operation snapshot in that section", () => {
  const source = read("src", "pages", "DiagnosticsPage.tsx");
  const aggregation = sliceBetween(
    source,
    "const sectionOperationStatusRef",
    "const updateSectionStatus",
  );
  assert.match(aggregation, /Record<DiagnosticsSectionKey, Record<string, DiagnosticsDemandSnapshot>>/);
  assert.match(aggregation, /emptySectionOperationStatuses\(\)/);
  assert.match(aggregation, /demandGenerationRef\.current\?\.id !== snapshot\.generation/);
  assert.match(aggregation, /operations\[operationId\] = snapshot/);
  assert.match(aggregation, /aggregateDiagnosticsSectionSnapshots\(Object\.values\(operations\)\)/);

  const requestPath = sliceBetween(source, "const requestDemand", "const loadBuildSection");
  assert.match(requestPath, /updateDemandSectionStatus\(section, operationId, snapshot\)/);
  assert.match(requestPath, /demandGenerationOwnsCommit\(demandGenerationRef\.current, generation\)/);
  assert.match(requestPath, /const commitDemandResult/);
  assert.match(source, /sectionOperationStatusRef\.current = emptySectionOperationStatuses\(\)/);
  assert.match(source, /capabilityProbeResultTruth/);
  assert.match(source, /Performance probe: \{capabilityProbeProvenance\(perfTier\)\}/);
  assert.match(source, /Demucs probe: \{capabilityProbeProvenance\(demucs\)\}/);
});

test("ordinary protection projection crosses the Tauri boundary through one snapshot command", () => {
  const helper = read("src", "lib", "youtubeProtectionSnapshot.ts");
  const diagnostics = read("src", "pages", "DiagnosticsPage.tsx");
  const options = read("src", "pages", "OptionsPage.tsx");
  const tauri = read("src-tauri", "src", "lib.rs");
  const jobs = readRepo("product", "engine", "src", "jobs.rs");
  const command = sliceBetween(
    tauri,
    "async fn youtube_protection_snapshot_get(",
    "async fn youtube_protection_history_replay(",
  );
  const projection = sliceBetween(
    jobs,
    "pub fn get_youtube_protection_snapshot(",
    "pub struct YoutubeProtectionHistoryExportReceipt",
  );

  assert.equal((helper.match(/return invoke</g) ?? []).length, 1);
  assert.equal((helper.match(/"youtube_protection_snapshot_get"/g) ?? []).length, 1);
  for (const legacyCommand of [
    "youtube_protection_status_get",
    "youtube_protection_history_get",
    "youtube_protection_history_replay",
  ]) {
    assert.doesNotMatch(helper, new RegExp(`"${legacyCommand}"`));
  }
  for (const page of [diagnostics, options]) {
    assert.match(page, /loadYoutubeProtectionSnapshot/);
    assert.doesNotMatch(page, /invoke<[^\n]+>\("youtube_protection_(?:status|history)_get"/);
  }

  assert.match(command, /history_limit\.unwrap_or\(100\)\.clamp\(1, 500\)/);
  assert.equal((command.match(/jobs::get_youtube_protection_snapshot\(/g) ?? []).length, 1);
  assert.doesNotMatch(command, /replay_youtube_protection_history/);
  assert.equal((projection.match(/db::open_readonly\(paths\)/g) ?? []).length, 1);
  assert.equal((projection.match(/transaction_with_behavior\(TransactionBehavior::Deferred\)/g) ?? []).length, 1);
  assert.match(projection, /OPERATION_DOWNLOAD/);
  assert.match(projection, /OPERATION_ENUMERATION/);
  assert.equal((projection.match(/policy_history_conn\(/g) ?? []).length, 2);
  assert.match(tauri, /generate_handler!\[[\s\S]*youtube_protection_snapshot_get/);
});

test("catalog and recommendation project one shared performance-tier result", () => {
  const tools = readRepo("product", "engine", "src", "tools.rs");
  const voiceBackends = readRepo("product", "engine", "src", "voice_backends.rs");
  const tauri = read("src-tauri", "src", "lib.rs");
  const catalogWithPerformance = sliceBetween(
    voiceBackends,
    "pub fn backend_catalog_with_performance(",
    "pub fn recommend_backend(",
  );
  const recommendationWithCatalog = sliceBetween(
    voiceBackends,
    "pub fn recommend_backend_with_catalog(",
    "fn normalize_goal(",
  );

  assert.match(
    voiceBackends,
    /pub fn backend_catalog\([\s\S]*?let performance = tools::performance_tier_status\(paths\);[\s\S]*?backend_catalog_with_performance\(paths, &performance\)/,
  );
  assert.doesNotMatch(catalogWithPerformance, /tools::performance_tier_status\(/);
  assert.match(catalogWithPerformance, /let tier = performance\.tier\.clone\(\)/);
  assert.match(
    voiceBackends,
    /pub fn recommend_backend\([\s\S]*?let catalog = backend_catalog\(paths\);[\s\S]*?recommend_backend_with_catalog\(&catalog, request\)/,
  );
  assert.doesNotMatch(recommendationWithCatalog, /performance_tier_status\(|backend_catalog\(/);
  assert.match(recommendationWithCatalog, /let tier = catalog\.performance_tier\.clone\(\)/);
  assert.match(
    tauri,
    /struct VoiceBackendsSnapshot[\s\S]*?performance_tier: tools::PerformanceTierStatus[\s\S]*?performance_tier: performance/,
  );

  assert.match(tools, /struct SemanticProbeSlot<T>/);
  assert.match(tools, /performance_tier_probe_slot\(\)[\s\S]*?\.run\(/);
  assert.match(tools, /freshness: if waited_for_shared_flight[\s\S]*?"shared_flight"[\s\S]*?"cached"/);
  assert.match(tools, /pub child_pid: Option<u32>/);
  assert.match(tools, /terminal_by_flight/);
  assert.match(tools, /wait_for_owned_capability_probe/);
  assert.match(tools, /CAPABILITY_PROBE_CHILD_TIMEOUT/);

  const gpuNames = sliceBetween(
    tools,
    "fn detect_gpu_names_best_effort()",
    "fn capability_probe_pid_from_error(",
  );
  assert.match(gpuNames, /crate::cmd::command\("nvidia-smi"\)/);
  assert.match(gpuNames, /wait_for_owned_capability_probe\(/);
  assert.match(gpuNames, /Duration::from_secs\(10\)/);
  assert.doesNotMatch(gpuNames, /\.spawn\(\)/);
  assert.doesNotMatch(gpuNames, /\.output\(\)/);
  const ownedProbe = sliceBetween(
    tools,
    "fn wait_for_owned_capability_probe(",
    "fn detect_torch_cuda(",
  );
  assert.match(ownedProbe, /run_owned_output_with_pid\(/);
  assert.match(ownedProbe, /external_command_cancel_requested/);
});
