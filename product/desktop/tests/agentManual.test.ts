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
