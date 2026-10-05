import test from "node:test";
import assert from "node:assert/strict";
import { createDiagnosticsInstallConfirmation } from "../src/lib/diagnosticsInstallConfirmation.ts";

const prompt = { title: "Force reinstall all packs", message: "Explicit confirmation" };
function fixture() {
  let available = true;
  const shown: unknown[] = [];
  const gate = createDiagnosticsInstallConfirmation(() => available, (value) => shown.push(value));
  return { gate, shown, block: () => { available = false; } };
}
async function originalHandler(gate: ReturnType<typeof fixture>["gate"], force: boolean, calls: boolean[]) {
  if (!await gate.request(prompt) || !gate.canStart()) return;
  calls.push(force);
}
test("cancel and route departure preserve zero installer calls", async () => {
  for (const hidden of [false, true]) {
    const f = fixture(); const calls: boolean[] = [];
    const job = originalHandler(f.gate, false, calls);
    if (hidden) f.block();
    f.gate.cancel(); await job;
    assert.deepEqual(calls, []); assert.equal(f.shown.at(-1), null);
  }
});
test("original normal and force selection is dispatched once after confirmation", async () => {
  for (const force of [false, true]) {
    const f = fixture(); const calls: boolean[] = [];
    const job = originalHandler(f.gate, force, calls);
    f.gate.resolve(true); f.gate.resolve(true); await job;
    assert.deepEqual(calls, [force]);
  }
});
test("other mutation becoming busy denies confirmation", async () => {
  const f = fixture(); const calls: boolean[] = [];
  const job = originalHandler(f.gate, true, calls);
  f.block(); f.gate.resolve(true); await job;
  assert.deepEqual(calls, []);
});
test("busy change after approval but before continuation denies installer dispatch", async () => {
  const f = fixture(); const calls: boolean[] = [];
  const job = originalHandler(f.gate, true, calls);
  f.gate.resolve(true); f.block(); await job;
  assert.deepEqual(calls, []);
});
test("duplicate pending requests cannot overwrite original selection", async () => {
  const f = fixture(); const calls: boolean[] = [];
  const first = originalHandler(f.gate, false, calls);
  await originalHandler(f.gate, true, calls);
  assert.equal(f.shown.length, 1);
  f.gate.resolve(true); await first; assert.deepEqual(calls, [false]);
});
