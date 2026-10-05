import test from "node:test";
import assert from "node:assert/strict";
import { phase2AdmissionView, PHASE2_JOURNAL_WAIT_MS } from "../src/lib/phase2Admission.ts";
const admitted = { id: "new-original", attempt_no: 1, force: true, held: false, observedAtMs: 1000 };
const row = { id: admitted.id, attempt_no: 1, status: "queued", force: true, held: false };
test("previous completed journal cannot disable newly admitted force polling", () => {
  const view = phase2AdmissionView(admitted, row, "previous-done", 1001);
  assert.equal(view.poll, true); assert.equal(view.previous, true); assert.match(view.label!, /queued/); assert.equal(admitted.force, true);
});
test("delayed journal becomes current without inventing steps or replacing original identity", () => {
  assert.equal(phase2AdmissionView(admitted, { ...row, status: "running" }, "previous-done", 2000).previous, true);
  const view = phase2AdmissionView(admitted, { ...row, status: "running" }, admitted.id, 2001);
  assert.equal(view.previous, false); assert.equal(view.poll, true); assert.equal(view.label, null);
});
test("queued held canonical admission is discoverable after remount and never says installing", () => {
  const remounted = { ...admitted, held: true, observedAtMs: 9000 };
  const view = phase2AdmissionView(remounted, { ...row, held: true }, "old", 9001);
  assert.equal(view.poll, true); assert.equal(view.previous, true); assert.match(view.label!, /queued and held/);
  assert.equal(phase2AdmissionView(remounted, { ...row, status: "running" }, admitted.id, 9002).poll, true);
});
test("matching canonical terminal alone clears polling; competing terminal history cannot", () => {
  assert.equal(phase2AdmissionView(admitted, { ...row, id: "old", status: "succeeded" }, "old", 2000).terminal, false);
  for (const status of ["succeeded", "failed", "canceled"]) {
    const view = phase2AdmissionView(admitted, { ...row, status }, admitted.id, 2000);
    assert.equal(view.terminal, true); assert.equal(view.poll, false);
  }
});
test("missing readback retains exact admission and bounds waiting without claiming failure", () => {
  assert.equal(phase2AdmissionView(admitted, null, "old", 2000).poll, true);
  const view = phase2AdmissionView(admitted, null, "old", 1000 + PHASE2_JOURNAL_WAIT_MS);
  assert.equal(view.poll, false); assert.equal(view.terminal, false); assert.match(view.label!, /No job was stopped/);
});
test("mount cleanup drops local latch; fresh canonical discovery is a separate new observation", () => {
  assert.equal(phase2AdmissionView(null, row, "old", 5000).poll, false);
  const rediscovered = { ...admitted, observedAtMs: 5000 };
  assert.equal(phase2AdmissionView(rediscovered, row, "old", 5001).poll, true);
});

test("durable runner-start refusal remains held until canonical original is actually running", () => {
  const held = { ...admitted, held: true };
  assert.match(phase2AdmissionView(held, { ...row, held: true }, "old", 2000).label!, /queued and held/);
  assert.equal(phase2AdmissionView(held, { ...row, status: "running" }, admitted.id, 2001).label, null);
});

test("a later attempt with the same ID cannot clear the original admission", () => {
  const view = phase2AdmissionView(admitted, { ...row, attempt_no: 2, status: "succeeded" }, admitted.id, 2000);
  assert.equal(view.terminal, false); assert.equal(view.poll, true);
});
test("verified held work remains observable beyond the missing-readback deadline and can resume", () => {
  const held = { ...admitted, held: true };
  assert.equal(phase2AdmissionView(held, { ...row, held: true }, "old", 200000).poll, true);
  assert.equal(phase2AdmissionView(held, { ...row, status: "running" }, admitted.id, 200001).label, null);
  const missing = phase2AdmissionView(held, null, "old", 200000);
  assert.equal(missing.poll, false); assert.match(missing.label!, /observation timed out/);
});
test("Diagnostics loader identity is stable and lifecycle/manual refresh really rediscover canonical work", async () => {
  const { readFile } = await import("node:fs/promises");
  const source = await readFile(new URL("../src/pages/DiagnosticsPage.tsx", import.meta.url), "utf8");
  const loader = source.slice(source.indexOf("  const loadPhase2Section ="), source.indexOf("  const loadStorageSection ="));
  assert.match(loader, /phase2AdmissionRef\.current/);
  assert.doesNotMatch(loader, /phase2Admission\?\.id\]\)/);
  const refresh = source.slice(source.indexOf("  const refresh ="), source.indexOf("  useEffect(() => {", source.indexOf("  const refresh =")));
  assert.match(refresh, /observePhase2Admission\(null\)/);
  const lifecycle = source.slice(source.indexOf("    const generation = createDemandGeneration(\"diagnostics\")"), source.indexOf("  }, [visible, loadBuildSection"));
  assert.match(lifecycle, /observePhase2Admission\(null\);[\s\S]*void loadPhase2Section\(true\)/);
  assert.match(lifecycle, /return \(\) => \{\s*phase2CanonicalGenerationRef\.current = null;\s*observePhase2Admission\(null\)/);
  assert.match(source, /else if \(phase2Admission && phase2AdmissionStatus\.terminal\)/);
  assert.match(source, /attempt_no: admitted\.attempt_no/);
  assert.match(source, /if \(!visible \|\| phase2CanonicalGenerationRef\.current !== demandGenerationRef\.current\) return/);
});
