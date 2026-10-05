import test from "node:test";
import assert from "node:assert/strict";
import { phase2ProgressTotal, phase2StatusIcon } from "../src/lib/phase2Progress";

test("Phase2 original status icons identify running and preserve terminal uncertainty", () => {
  assert.equal(phase2StatusIcon("running"), "⟳");
  assert.equal(phase2StatusIcon("done"), "✓");
  assert.equal(phase2StatusIcon("queued"), "⏸");
  assert.equal(phase2StatusIcon("skipped"), "—");
  for (const status of ["failed", "interrupted", "stale"]) assert.equal(phase2StatusIcon(status), "⚠");
  for (const status of ["", "not started", "unknown"]) assert.equal(phase2StatusIcon(status), "·");
});

test("WP-0230 confirmed never-started history shows the actual supported plan total", () => {
  const plan = [...Array.from({ length: 8 }, () => ({ supported: true })), { supported: false }, {}];
  assert.equal(phase2ProgressTotal([], false, plan), 8);
  assert.equal(phase2ProgressTotal([], false, null), 0);
  assert.equal(phase2ProgressTotal([], false, [{ supported: false }, { supported: "true" }]), 0);
});

test("WP-0230 current or previous journal totals preserve skipped steps and refuse invented history", () => {
  const plan = Array.from({ length: 8 }, () => ({ supported: true }));
  const journal = [{ status: "done" }, { status: "skipped" }, { status: "running" }];
  assert.equal(phase2ProgressTotal(journal, true, plan), 3);
  assert.equal(phase2ProgressTotal(journal, false, plan), 3);
  assert.equal(phase2ProgressTotal([], true, plan), 0);
  assert.equal(phase2ProgressTotal([], undefined, plan), 0);
});
