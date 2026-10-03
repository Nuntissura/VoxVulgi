import test from "node:test";
import assert from "node:assert/strict";
import { collectDiagnosticsFields, collectDiagnosticsFieldResults, settleDiagnosticDemands, diagnosticPendingText, capabilityIsVerified } from "../src/lib/diagnosticsResults.ts";

test("diagnostic reads retain successful peers and continue after a failed command", async () => {
  const committed: string[] = [];
  await assert.rejects(collectDiagnosticsFields({
    first: async () => { committed.push("ffmpeg ready"); return true; },
    failed: async () => { throw new Error("database is locked"); },
    last: async () => { committed.push("python ready"); return true; },
  }), /failed: Error: database is locked/);
  assert.deepEqual(committed, ["ffmpeg ready", "python ready"]);
});

test("all successful diagnostic fields retain typed values", async () => {
  assert.deepEqual(await collectDiagnosticsFields({ tool: async () => "ready", count: async () => 4 }), { tool: "ready", count: 4 });
});

test("terminal failure waits for independently committing sibling demand", async () => {
  let release!: () => void;
  let committed = false;
  const sibling = new Promise<void>((resolve) => { release = resolve; }).then(() => { committed = true; return "ready"; });
  const combined = settleDiagnosticDemands([Promise.reject(new Error("probe failed")), sibling]);
  let settled = false;
  const checked = assert.rejects(combined, /probe failed/).then(() => { settled = true; });
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.equal(settled, false);
  release(); await checked;
  assert.equal(committed, true);
});

test("pending projections distinguish unrequested, active and terminal states", () => {
  assert.equal(diagnosticPendingText("idle"), "Not checked");
  assert.equal(diagnosticPendingText("queued"), "Checking…");
  assert.equal(diagnosticPendingText("loading"), "Checking…");
  assert.equal(diagnosticPendingText("failed"), "Unknown — check failed");
  assert.equal(diagnosticPendingText("ready"), "Unknown — no result");
  assert.equal(diagnosticPendingText("stale"), "Unknown — refresh needed");
});

test("failed probes cannot establish CPU, missing package or CUDA absence", () => {
  for (const state of ["failed", "timeout", "superseded"]) {
    assert.equal(capabilityIsVerified({ probe_state: state, probe_error: null, freshness: "fresh" }), false);
  }
  assert.equal(capabilityIsVerified({ probe_state: "ready", probe_error: "import failed", freshness: "fresh" }), false);
  assert.equal(capabilityIsVerified({ probe_state: "ready", probe_error: null, freshness: "stale_cached" }), false);
  assert.equal(capabilityIsVerified({ probe_state: "verified", probe_error: null, freshness: "fresh" }), true);
  assert.equal(capabilityIsVerified(null), false);
});


test("missing runtime cannot establish verified hardware or CPU recommendation", () => {
  assert.equal(capabilityIsVerified({ probe_state: "missing_runtime", probe_error: null, freshness: "verified_missing_runtime" }), false);
});

test("current shared-flight waiter receives successful fields after old generation detaches and sibling fails", async () => {
  const { DiagnosticsDemandCoordinator, createDemandGeneration, demandGenerationOwnsCommit } = await import("../src/lib/diagnosticsDemandCoordinator.ts");
  const coordinator = new DiagnosticsDemandCoordinator({ trace: async () => {} });
  const oldGeneration = createDemandGeneration("diagnostics");
  let currentGeneration = oldGeneration;
  let release!: () => void;
  const barrier = new Promise<void>((resolve) => { release = resolve; });
  let nativeRuns = 0;
  const run = async () => {
    nativeRuns += 1;
    return collectDiagnosticsFieldResults({
      info: async () => { await barrier; return { app_version: "0.1.205" }; },
      inventory: async () => { throw new Error("sibling failed"); },
    });
  };
  const truth = (result: Awaited<ReturnType<typeof run>>) => ({ state: result.errors.length ? "failed" as const : "ready" as const, verifiedAtMs: null, error: result.errors.join("; ") || null });
  const oldOutcome = coordinator.request("diagnostics.build", oldGeneration, run, { resultTruth: truth }).catch(() => null);
  await new Promise((resolve) => setTimeout(resolve, 0));
  coordinator.cancelGeneration(oldGeneration);
  currentGeneration = createDemandGeneration("diagnostics");
  const liveGeneration = currentGeneration;
  const live = coordinator.request("diagnostics.build", liveGeneration, run, { resultTruth: truth });
  release();
  const result = await live;
  assert.equal(demandGenerationOwnsCommit(currentGeneration, liveGeneration), true);
  assert.deepEqual(result.value.values.info, { app_version: "0.1.205" });
  assert.match(result.value.errors.join("; "), /inventory: Error: sibling failed/);
  assert.equal(result.source, "shared");
  assert.equal(nativeRuns, 1, "reattachment must not launch overlapping native work");
  assert.equal(await oldOutcome, null);
});
