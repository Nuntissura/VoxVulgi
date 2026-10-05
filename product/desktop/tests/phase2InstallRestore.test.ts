import test from "node:test";
import assert from "node:assert/strict";
import { restorePhase2Install } from "../src/lib/phase2InstallRestore.ts";
import type { Phase2TransferStatus } from "../src/lib/usePhase2Transfer.ts";

const original = {
  id: "original", job_type: "install_phase2_packs_v1", attempt_no: 1,
  status: "failed" as const, progress: 0.875, error: "Missing shipped backend",
};
const receipt = (job: Phase2TransferStatus["canonical_install"]): Phase2TransferStatus =>
  ({ canonical_install: job, owner: null, transfer: null });

test("restore uses journal identity only to read exact canonical failure and preserves its error", async () => {
  const selected: string[] = []; const committed: unknown[] = [];
  await restorePhase2Install(async () => ({ state: { job_id: original.id } }),
    async (id) => { selected.push(id); return receipt(original); },
    (job) => committed.push(job), () => true);
  assert.deepEqual(selected, [original.id]); assert.deepEqual(committed, [original]);
});

test("current canonical active installer takes precedence over an older failed journal", async () => {
  const running = { ...original, id: "new-original", status: "running" as const, error: null };
  const selected: string[] = []; const committed: unknown[] = [];
  await restorePhase2Install(async () => ({ canonical_install: { id: running.id }, state: { job_id: original.id } }),
    async (id) => { selected.push(id); return receipt(running); },
    (job) => committed.push(job), () => true);
  assert.deepEqual(selected, [running.id]); assert.deepEqual(committed, [running]);
});

test("unbound journals, absent jobs, wrong types, and terminal success/cancel cannot manufacture recovery", async () => {
  for (const job of [null, { ...original, id: "other" }, { ...original, job_type: "download_video" },
    { ...original, status: "succeeded" as const }, { ...original, status: "canceled" as const }]) {
    let committed = false;
    await restorePhase2Install(async () => ({ state: { job_id: original.id } }),
      async () => receipt(job), () => { committed = true; }, () => true);
    assert.equal(committed, false);
  }
  for (const job_id of [null, 123, "", "  "]) {
    let read = false;
    await restorePhase2Install(async () => ({ state: { job_id } }),
      async () => { read = true; return receipt(original); }, () => assert.fail("Unexpected restore"), () => true);
    assert.equal(read, false);
  }
});

test("new admission invalidates pending latest read before any stale exact lookup", async () => {
  let current = true; let resolve!: (value: { state: { job_id: string } }) => void;
  const latest = new Promise<{ state: { job_id: string } }>((done) => { resolve = done; });
  let read = false;
  const pending = restorePhase2Install(() => latest, async () => { read = true; return receipt(original); }, () => assert.fail("Unexpected stale restore"), () => current);
  current = false; resolve({ state: { job_id: original.id } }); await pending;
  assert.equal(read, false);
});

test("late exact read cannot restore after navigation, unmount, or a newer tracked job", async () => {
  let current = true; let resolve!: (value: Phase2TransferStatus) => void;
  const exact = new Promise<Phase2TransferStatus>((done) => { resolve = done; });
  const pending = restorePhase2Install(async () => ({ state: { job_id: original.id } }), () => exact, () => assert.fail("Unexpected late restore"), () => current);
  await Promise.resolve(); current = false; resolve(receipt(original)); await pending;
});
