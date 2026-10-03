import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { classifySafeAgentActions } from "../src/lib/agentUiAudit.ts";
const manual = JSON.parse(readFileSync(new URL("../src/lib/agentManual.json", import.meta.url), "utf8"));
test("manual command schemas cover authenticated requests and mutation receipts", () => {
  assert.equal(new Set(manual.commands.map((c: any) => c.name)).size, manual.commands.length);
  for (const command of manual.commands) {
    assert.equal(command.input_schema.properties.command.const, command.name);
    assert.ok(command.input_schema.required.includes("bridge_token"));
    if (!command.read_only) assert.ok(command.input_schema.required.includes("operation_id"));
  }
  assert.ok(manual.endpoints.some((e: any) => e.path === "/agent/command"));
});
test("download URL inputs and manual search use bounded declared semantic actions", () => {
  assert.deepEqual(classifySafeAgentActions("textarea", "textbox", false, false, "downloads.urlbatchtext", "reversible_state_change", "text"), ["scroll_into_view", "set_value"]);
  assert.deepEqual(classifySafeAgentActions("input", "searchbox", false, false, "manual.search", "read_only", "text"), ["scroll_into_view", "set_value"]);
});

test("selected download choices and pause expose the exact shared product decisions", () => {
  const start = manual.commands.find((command: any) => command.name === "downloads.start_selected");
  assert.deepEqual(start.input_schema.properties.mode.enum, ["only", "continue_all"]);
  assert.equal(start.input_schema.properties.job_ids.maxItems, 1500);
  assert.deepEqual(manual.commands.find((command: any) => command.name === "downloads.enqueue").input_schema.properties.mode.enum, ["only", "continue_all"]);
  assert.ok(start.input_schema.required.includes("operation_id"));
  const members = manual.commands.find((command: any) => command.name === "downloads.batch_members");
  assert.equal(members.read_only, true);
  assert.ok(members.input_schema.required.includes("batch_id"));
  assert.equal(manual.commands.find((command: any) => command.name === "queue.pause").read_only, false);
  for (const id of ["downloads.start-only", "downloads.continue-all", "downloads.start-cancel", "downloads.start-existing.job-1", "downloads.start-batch.batch-1"]) {
    assert.deepEqual(classifySafeAgentActions("button", "button", false, false, id, "reversible_state_change", ""), ["scroll_into_view", "activate_product_action"]);
  }
});
