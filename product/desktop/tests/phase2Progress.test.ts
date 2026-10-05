import test from "node:test";
import assert from "node:assert/strict";
import { phase2StatusIcon } from "../src/lib/phase2Progress";

test("Phase2 original status icons identify running and preserve terminal uncertainty", () => {
  assert.equal(phase2StatusIcon("running"), "⟳");
  assert.equal(phase2StatusIcon("done"), "✓");
  assert.equal(phase2StatusIcon("queued"), "⏸");
  assert.equal(phase2StatusIcon("skipped"), "—");
  for (const status of ["failed", "interrupted", "stale"]) assert.equal(phase2StatusIcon(status), "⚠");
  for (const status of ["", "not started", "unknown"]) assert.equal(phase2StatusIcon(status), "·");
});
