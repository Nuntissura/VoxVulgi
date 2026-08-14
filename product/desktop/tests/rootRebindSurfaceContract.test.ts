import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import test from "node:test";
import assert from "node:assert/strict";

const root = fileURLToPath(new URL("..", import.meta.url));

function readRepoFile(...parts: string[]): string {
  return readFileSync(join(root, ...parts), "utf8");
}

test("Diagnostics exposes the complete guarded root-rebind workflow", () => {
  const diagnostics = readRepoFile("src", "pages", "DiagnosticsPage.tsx");
  const surface = readRepoFile("src", "components", "RootRebindControl.tsx");
  assert.match(diagnostics, /<RootRebindControl\s*\/>/);
  for (const command of [
    "root_rebind_dry_run",
    "root_rebind_prepare",
    "root_rebind_apply",
    "root_rebind_status",
    "root_rebind_rollback",
    "root_rebind_recover",
    "root_rebind_task_status",
    "root_rebind_task_cancel",
  ]) {
    assert.match(surface, new RegExp(`"${command}"`), `${command} must be reachable from Diagnostics`);
  }
  assert.match(surface, /APPLY:\$\{receiptId/);
  assert.match(surface, /ROLLBACK:\$\{receiptId/);
  assert.match(surface, /evidence: \[\]/, "engine must select trusted canonical evidence");
  assert.match(surface, /data-testid="root-rebind-task-status"/);
  assert.match(surface, /data-testid="root-rebind-receipts"/);
});

test("root-rebind task polling is a nonblocking snapshot and heavy work uses the fixed queue", () => {
  const engine = readRepoFile("..", "engine", "src", "root_rebind.rs");
  const tauri = readRepoFile("src-tauri", "src", "lib.rs");
  const statusStart = engine.indexOf("pub fn root_rebind_task_status");
  const statusEnd = engine.indexOf("pub enum RootRebindStopAfter", statusStart);
  const statusBlock = engine.slice(statusStart, statusEnd);
  assert.match(engine, /const ROOT_REBIND_WORKER_COUNT: usize = 2/);
  assert.match(engine, /const ROOT_REBIND_RECOVERY_WORKER_COUNT: usize = 1/);
  assert.match(engine, /submit_root_rebind_task_cancellable/);
  assert.match(engine, /sync_channel::<RootRebindWorkRequest>\(ROOT_REBIND_QUEUE_CAPACITY\)/);
  assert.doesNotMatch(statusBlock, /sleep|recv|canonicalize|metadata|Sha256/);
  assert.match(tauri, /root_rebind::submit_root_rebind_task\("startup_recover"/);
  assert.match(tauri, /root_rebind_task_status,/);
});
