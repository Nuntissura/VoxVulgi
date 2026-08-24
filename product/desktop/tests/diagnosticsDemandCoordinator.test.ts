import assert from "node:assert/strict";
import test from "node:test";

import {
  DIAGNOSTICS_OPERATION_REGISTRY,
  DemandSupersededError,
  DiagnosticsDemandCoordinator,
  aggregateDiagnosticsSectionSnapshots,
  createDemandGeneration,
  demandGenerationOwnsCommit,
  type DiagnosticsDemandSnapshot,
} from "../src/lib/diagnosticsDemandCoordinator.ts";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((nextResolve, nextReject) => {
    resolve = nextResolve;
    reject = nextReject;
  });
  return { promise, resolve, reject };
}

function demandSnapshot(
  operation_id: DiagnosticsDemandSnapshot["operation_id"],
  state: DiagnosticsDemandSnapshot["state"],
  overrides: Partial<DiagnosticsDemandSnapshot> = {},
): DiagnosticsDemandSnapshot {
  return {
    operation_id,
    semantic_key: operation_id,
    owner: "diagnostics",
    generation: 1,
    state,
    queued_at_ms: null,
    admitted_at_ms: null,
    verified_at_ms: null,
    freshness_ms: 0,
    shared: false,
    error: null,
    ...overrides,
  };
}

test("section aggregation keeps queued distinct, retains errors, and ignores unverified freshness", () => {
  const queuedWithPriorFailure = aggregateDiagnosticsSectionSnapshots([
    demandSnapshot("capability.voice-backends", "failed", { error: "adapter probe failed" }),
    demandSnapshot("diagnostics.tools-core", "queued"),
  ]);
  assert.equal(queuedWithPriorFailure.state, "queued");
  assert.equal(queuedWithPriorFailure.error, "adapter probe failed");

  const loadingWithPriorFailure = aggregateDiagnosticsSectionSnapshots([
    demandSnapshot("capability.voice-backends", "failed", { error: "adapter probe failed" }),
    demandSnapshot("diagnostics.tools-core", "loading"),
  ]);
  assert.equal(loadingWithPriorFailure.state, "loading");
  assert.equal(loadingWithPriorFailure.error, "adapter probe failed");

  const stale = aggregateDiagnosticsSectionSnapshots([
    demandSnapshot("capability.performance-tier", "stale", {
      verified_at_ms: 123,
      freshness_ms: 30_000,
      error: "fresh verification failed",
    }),
    demandSnapshot("capability.demucs", "ready", {
      verified_at_ms: 456,
      freshness_ms: 10_000,
    }),
  ]);
  assert.equal(stale.state, "stale");
  assert.equal(stale.verified_at_ms, 123);
  assert.equal(stale.freshness_ms, 10_000);
  assert.equal(stale.error, "fresh verification failed");
});

test("generation ownership is rechecked at the commit boundary", () => {
  const first = createDemandGeneration("diagnostics");
  assert.equal(demandGenerationOwnsCommit(first, first), true);
  first.canceled = true;
  assert.equal(demandGenerationOwnsCommit(first, first), false);
  const successor = createDemandGeneration("diagnostics");
  assert.equal(demandGenerationOwnsCommit(successor, first), false);
  assert.equal(demandGenerationOwnsCommit(successor, successor), true);
});

test("operation registry is machine-readable and classifies the shared heavy probes", () => {
  const byId = new Map(DIAGNOSTICS_OPERATION_REGISTRY.map((entry) => [entry.id, entry]));
  assert.equal(byId.get("capability.performance-tier")?.costClass, "python_heavy");
  assert.equal(byId.get("capability.performance-tier")?.semanticKey, "capability.performance-tier");
  assert.equal(byId.get("capability.demucs")?.costClass, "python_heavy");
  assert.equal(byId.get("protection.snapshot")?.costClass, "db_read");
  assert.equal(byId.get("protection.history-replay")?.costClass, "history_replay");
  assert.equal(byId.get("protection.history-replay")?.automatic, false);
  for (const entry of DIAGNOSTICS_OPERATION_REGISTRY) {
    assert.ok(entry.id && entry.semanticKey && entry.ownerModules.length > 0);
    assert.ok(entry.trigger);
    assert.ok(entry.freshnessMs >= 0);
    assert.ok(entry.maxConcurrency >= 1);
  }
});

test("native probe truth can resolve data without fabricating a ready verification", async () => {
  const snapshots: DiagnosticsDemandSnapshot[] = [];
  const receipts: Array<Record<string, unknown>> = [];
  const coordinator = new DiagnosticsDemandCoordinator({
    trace: async (_event, details) => receipts.push(details as Record<string, unknown>),
  });
  const owner = createDemandGeneration("diagnostics");
  const result = await coordinator.request(
    "capability.performance-tier",
    owner,
    async () => ({
      tier: "cpu",
      child_pid: 4242,
      verified_at_ms: 0,
      probe_state: "failed",
      probe_error: "Torch import failed",
    }),
    {
      onState: (snapshot) => snapshots.push(snapshot),
      resultTruth: (value) => {
        const status = value as { verified_at_ms: number; probe_error: string };
        return { state: "failed", verifiedAtMs: status.verified_at_ms || null, error: status.probe_error };
      },
    },
  );
  assert.equal(result.value.tier, "cpu", "degraded detail remains available to the UI");
  assert.equal(result.verifiedAtMs, 0);
  assert.equal(snapshots.at(-1)?.state, "failed");
  assert.equal(snapshots.at(-1)?.verified_at_ms, null);
  assert.equal(snapshots.at(-1)?.error, "Torch import failed");
  const terminal = receipts.find((receipt) => receipt.phase === "terminal");
  assert.equal(terminal?.outcome, "probe_failed");
  assert.deepEqual(terminal?.child_pids, [4242]);
});

test("one semantic flight is shared and python-heavy admission never exceeds two", async () => {
  const coordinator = new DiagnosticsDemandCoordinator({ maxConcurrent: 4, maxPythonHeavy: 2 });
  const first = deferred<number>();
  const second = deferred<number>();
  const third = deferred<number>();
  let active = 0;
  let maximumActive = 0;
  let firstRuns = 0;
  const run = (gate: ReturnType<typeof deferred<number>>, countFirst = false) => async () => {
    if (countFirst) firstRuns += 1;
    active += 1;
    maximumActive = Math.max(maximumActive, active);
    try {
      return await gate.promise;
    } finally {
      active -= 1;
    }
  };

  const ownerA = createDemandGeneration("diagnostics");
  const ownerB = createDemandGeneration("options.video_archiver");
  const sharedA = coordinator.request("capability.performance-tier", ownerA, run(first, true), { identity: "runtime-a" });
  const sharedB = coordinator.request("capability.performance-tier", ownerB, run(first, true), { identity: "runtime-a" });
  const demucs = coordinator.request("capability.demucs", ownerA, run(second), { identity: "runtime-a" });
  const queued = coordinator.request("capability.voice-backends", ownerA, run(third), { identity: "runtime-a" });

  await Promise.resolve();
  assert.equal(firstRuns, 1);
  assert.equal(maximumActive, 2);
  first.resolve(1);
  second.resolve(2);
  await Promise.all([sharedA, sharedB, demucs]);
  third.resolve(3);
  await queued;
  assert.equal(maximumActive, 2);
});

test("canceling a generation removes queued work and suppresses a late running result", async () => {
  const coordinator = new DiagnosticsDemandCoordinator({ maxConcurrent: 1, maxPythonHeavy: 1 });
  const blocker = deferred<number>();
  const runningOwner = createDemandGeneration("diagnostics");
  const queuedOwner = createDemandGeneration("diagnostics");
  let queuedRuns = 0;
  const running = coordinator.request("capability.performance-tier", runningOwner, () => blocker.promise, { identity: "runtime-b" });
  const queued = coordinator.request("capability.demucs", queuedOwner, async () => {
    queuedRuns += 1;
    return 2;
  }, { identity: "runtime-b" });

  coordinator.cancelGeneration(queuedOwner);
  await assert.rejects(queued, DemandSupersededError);
  assert.equal(queuedRuns, 0);
  coordinator.cancelGeneration(runningOwner);
  await assert.rejects(running, DemandSupersededError);
  blocker.resolve(1);
  await coordinator.whenIdle();
});

test("freshness cache is explicit, invalidation reruns once, and failures release single-flight", async () => {
  let now = 1_000;
  const coordinator = new DiagnosticsDemandCoordinator({ now: () => now });
  const owner = createDemandGeneration("diagnostics");
  let runs = 0;
  const run = async () => {
    runs += 1;
    return runs;
  };

  const first = await coordinator.request("capability.performance-tier", owner, run, { identity: "runtime-c" });
  const reverified = await coordinator.request("capability.performance-tier", owner, run, { identity: "runtime-c" });
  assert.equal(first.source, "executed");
  assert.equal(reverified.source, "executed");
  assert.equal(reverified.value, 2);
  assert.equal(runs, 2, "zero-freshness probes must not be cached by the frontend coordinator");

  coordinator.invalidate("capability.performance-tier", "runtime-c");
  const refreshed = await coordinator.request("capability.performance-tier", owner, run, { identity: "runtime-c" });
  assert.equal(refreshed.value, 3);
  assert.equal(runs, 3);

  let attempts = 0;
  const failThenPass = async () => {
    attempts += 1;
    if (attempts === 1) throw new Error("fixture failure");
    return 9;
  };
  await assert.rejects(
    coordinator.request("capability.demucs", owner, failThenPass, { identity: "runtime-c" }),
    /fixture failure/,
  );
  const retry = await coordinator.request("capability.demucs", owner, failThenPass, { identity: "runtime-c" });
  assert.equal(retry.value, 9);
  now += 1;
});

test("a synchronous runner throw releases admission and permits the queued successor", async () => {
  const receipts: Array<Record<string, unknown>> = [];
  const coordinator = new DiagnosticsDemandCoordinator({
    maxConcurrent: 1,
    trace: async (_event, details) => {
      receipts.push(details as Record<string, unknown>);
    },
  });
  const owner = createDemandGeneration("diagnostics");
  const syncThrow = (() => {
    throw new Error("synchronous fixture failure");
  }) as unknown as (signal: AbortSignal) => Promise<number>;

  const failed = coordinator.request("diagnostics.build", owner, syncThrow, {
    requestIdentity: "sync-throw",
  });
  const successor = coordinator.request("diagnostics.jobs", owner, async () => 42, {
    requestIdentity: "queued-successor",
  });

  await assert.rejects(failed, /synchronous fixture failure/);
  assert.equal((await successor).value, 42);
  await coordinator.whenIdle();
  assert.equal(
    receipts.filter((receipt) => receipt.request_identity === "sync-throw" && receipt.phase === "terminal" && receipt.outcome === "failed").length,
    1,
  );
  assert.equal(
    receipts.filter((receipt) => receipt.request_identity === "queued-successor" && receipt.phase === "terminal" && receipt.outcome === "succeeded").length,
    1,
  );
});

test("forced refresh waits for an older semantic flight and then shares one fresh successor", async () => {
  const coordinator = new DiagnosticsDemandCoordinator();
  const owner = createDemandGeneration("options.video_archiver");
  const oldGate = deferred<number>();
  let runs = 0;
  const initial = coordinator.request("protection.snapshot", owner, async () => {
    runs += 1;
    return oldGate.promise;
  });
  const forceRun = async () => {
    runs += 1;
    return 2;
  };
  const forcedA = coordinator.request("protection.snapshot", owner, forceRun, { force: true });
  const forcedB = coordinator.request("protection.snapshot", owner, forceRun, { force: true });
  await Promise.resolve();
  assert.equal(runs, 1);
  oldGate.resolve(1);
  assert.equal((await initial).value, 1);
  const [freshA, freshB] = await Promise.all([forcedA, forcedB]);
  assert.equal(freshA.value, 2);
  assert.equal(freshB.value, 2);
  assert.equal(runs, 2);
});

test("forced refresh still shares one fresh successor after the older flight fails", async () => {
  const coordinator = new DiagnosticsDemandCoordinator();
  const owner = createDemandGeneration("options.video_archiver");
  const oldGate = deferred<number>();
  let runs = 0;
  const initial = coordinator.request("protection.snapshot", owner, async () => {
    runs += 1;
    return oldGate.promise;
  });
  const forceRun = async () => {
    runs += 1;
    return 22;
  };
  const forcedA = coordinator.request("protection.snapshot", owner, forceRun, { force: true });
  const forcedB = coordinator.request("protection.snapshot", owner, forceRun, { force: true });
  await Promise.resolve();
  assert.equal(runs, 1);
  oldGate.reject(new Error("old semantic flight failed"));
  await assert.rejects(initial, /old semantic flight failed/);
  const [freshA, freshB] = await Promise.all([forcedA, forcedB]);
  assert.equal(freshA.value, 22);
  assert.equal(freshB.value, 22);
  assert.equal(runs, 2, "both force waiters must join one successor after old failure");
});

test("scheduler receipts distinguish waiter detachment from backend cancellation", async () => {
  const receipts: Array<Record<string, unknown>> = [];
  const coordinator = new DiagnosticsDemandCoordinator({
    trace: async (event, details) => {
      assert.equal(event, "diagnostics_demand_scheduler");
      receipts.push(details as Record<string, unknown>);
    },
  });
  const gate = deferred<{ child_pid: number; ready: boolean }>();
  const diagnosticsOwner = createDemandGeneration("diagnostics");
  const optionsOwner = createDemandGeneration("options.video_archiver");
  const first = coordinator.request("capability.performance-tier", diagnosticsOwner, () => gate.promise);
  const shared = coordinator.request("capability.performance-tier", optionsOwner, () => gate.promise);
  coordinator.cancelGeneration(diagnosticsOwner);
  await assert.rejects(first, DemandSupersededError);
  gate.resolve({ child_pid: 4242, ready: true });
  assert.equal((await shared).value.child_pid, 4242);
  await coordinator.whenIdle();

  const phases = new Set(receipts.map((receipt) => receipt.phase));
  for (const phase of ["queued", "admitted", "shared", "cancel_requested", "waiter_detached", "terminal", "superseded_completion"]) {
    assert.ok(phases.has(phase), `missing ${phase} receipt`);
  }
  assert.equal(phases.has("cancel_observed"), false, "a detached frontend waiter is not an observed cancellation");
  assert.equal(phases.has("backend_cancel_observed"), false, "the backend did not acknowledge cancellation");
  assert.equal(phases.has("frontend_abort_signaled"), false, "shared non-cancellable work must remain alive for its owner");
  const terminal = receipts.find((receipt) => receipt.phase === "terminal" && receipt.outcome === "succeeded");
  assert.deepEqual(terminal?.child_pids, [4242]);
  assert.equal(terminal?.semantic_key, "capability.performance-tier");
  assert.equal(terminal?.cost_class, "python_heavy");
  assert.equal(typeof terminal?.request_identity, "string");
  assert.equal(typeof terminal?.queue_wait_ms, "number");
  assert.equal(typeof terminal?.execution_ms, "number");

  const detachedSpans = new Set(
    receipts
      .filter((receipt) => receipt.request_identity === receipts.find((entry) => entry.phase === "queued")?.request_identity)
      .map((receipt) => receipt.span_id),
  );
  assert.equal(
    detachedSpans.size,
    1,
    "a superseded waiter must retain the same flight span through its late completion receipt",
  );
});

test("shared requests use one flight span while successor flights receive unique spans", async () => {
  const receipts: Array<Record<string, unknown>> = [];
  const coordinator = new DiagnosticsDemandCoordinator({
    trace: async (_event, details) => {
      receipts.push(details as Record<string, unknown>);
    },
  });
  const gate = deferred<number>();
  const diagnosticsOwner = createDemandGeneration("diagnostics");
  const optionsOwner = createDemandGeneration("options.video_archiver");
  const first = coordinator.request("capability.performance-tier", diagnosticsOwner, () => gate.promise, {
    identity: "span-runtime",
    requestIdentity: "span-first",
  });
  const shared = coordinator.request("capability.performance-tier", optionsOwner, () => gate.promise, {
    identity: "span-runtime",
    requestIdentity: "span-shared",
  });
  gate.resolve(7);
  await Promise.all([first, shared]);

  await coordinator.request("capability.performance-tier", diagnosticsOwner, async () => 8, {
    identity: "span-runtime",
    requestIdentity: "span-successor",
  });
  await coordinator.whenIdle();

  const spansFor = (requestIdentity: string) => new Set(
    receipts
      .filter((receipt) => receipt.request_identity === requestIdentity)
      .map((receipt) => receipt.span_id),
  );
  const firstSpans = spansFor("span-first");
  const sharedSpans = spansFor("span-shared");
  const successorSpans = spansFor("span-successor");
  assert.equal(firstSpans.size, 1, "one request must not drift across span IDs during its flight");
  assert.deepEqual(sharedSpans, firstSpans, "waiters sharing semantic work must cite the same flight span");
  assert.equal(successorSpans.size, 1);
  assert.notDeepEqual(successorSpans, firstSpans, "a later semantic flight must receive a unique span ID");
});
