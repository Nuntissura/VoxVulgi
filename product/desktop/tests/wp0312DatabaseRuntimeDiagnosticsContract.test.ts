import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import { DIAGNOSTICS_OPERATION_REGISTRY } from "../src/lib/diagnosticsDemandCoordinator.ts";

const desktopRoot = fileURLToPath(new URL("..", import.meta.url));
const repoRoot = join(desktopRoot, "..", "..");
const readDesktop = (...parts: string[]) => readFileSync(join(desktopRoot, ...parts), "utf8");
const readRepo = (...parts: string[]) => readFileSync(join(repoRoot, ...parts), "utf8");

function sliceBetween(source: string, startMarker: string, endMarker: string): string {
  const start = source.indexOf(startMarker);
  assert.notEqual(start, -1, `missing start marker: ${startMarker}`);
  const end = source.indexOf(endMarker, start + startMarker.length);
  assert.notEqual(end, -1, `missing end marker: ${endMarker}`);
  return source.slice(start, end);
}

test("Tauri exposes bounded runtime/WAL status and explicit passive checkpoint commands", () => {
  const tauri = readDesktop("src-tauri", "src", "lib.rs");
  const status = sliceBetween(
    tauri,
    "async fn database_runtime_status(",
    "async fn database_checkpoint_passive(",
  );
  const checkpoint = sliceBetween(
    tauri,
    "async fn database_checkpoint_passive(",
    "async fn diagnostics_clear_cache(",
  );

  assert.match(status, /database\.snapshot\(\)/);
  assert.match(status, /database\.wal_health\(\)/);
  assert.match(status, /WRITER_QUEUE_CAPACITY/);
  assert.match(status, /READ_ADMISSION_CAPACITY/);
  assert.doesNotMatch(status, /checkpoint_passive|wal_checkpoint|db::migrate/);
  assert.match(checkpoint, /database\.checkpoint_passive\(\)/);
  assert.match(tauri, /generate_handler!\[[\s\S]*database_runtime_status[\s\S]*database_checkpoint_passive/);
});

test("Database runtime helpers keep foreground reads separate from explicit maintenance", () => {
  const runtime = readRepo("product", "engine", "src", "database_runtime.rs");
  const status = sliceBetween(runtime, "pub fn wal_health(&self)", "pub fn checkpoint_passive(&self)");
  const checkpoint = sliceBetween(runtime, "pub fn checkpoint_passive(&self)", "pub fn contention_receipt(&self");

  assert.match(status, /std::fs::metadata/);
  assert.match(status, /oldest_reader_age_ms/);
  assert.match(status, /long_reader_candidates/);
  assert.match(status, /last_checkpoint/);
  assert.doesNotMatch(status, /wal_checkpoint|write_context/);
  assert.match(checkpoint, /DatabaseOperationContext::new\("database_maintenance", "wal_checkpoint_passive"\)/);
  assert.match(checkpoint, /PRAGMA wal_checkpoint\(PASSIVE\)/);
  assert.match(checkpoint, /checkpoint_completed/);
  assert.match(checkpoint, /checkpoint_busy/);
});

test("Database runtime receipts, FIFO readers, retries, and contention attribution are fail-closed", () => {
  const runtime = readRepo("product", "engine", "src", "database_runtime.rs");
  assert.match(runtime, /waiting_readers: VecDeque<WaitingReader>/);
  assert.match(runtime, /waiting_readers\s*\.\s*front\(\)/);
  assert.match(runtime, /read_admission_overloaded/);
  assert.match(runtime, /completed_write_context/);
  assert.match(runtime, /completed_read_context/);
  assert.match(runtime, /total_changes\(\)/);
  assert.match(runtime, /candidate\.admitted_at_ms\.is_some\(\)/);
  assert.match(runtime, /idempotent_retry_cancelled/);
  assert.match(runtime, /seed % 11/);

  const jobs = readRepo("product", "engine", "src", "jobs.rs");
  const productionRetry = sliceBetween(
    jobs,
    "pub fn record_instagram_enumeration_dispatch(",
    "fn is_queue_paused_conn(",
  );
  assert.match(productionRetry, /write_idempotent/);
  assert.match(productionRetry, /TransactionBehavior::Immediate/);

  const tauri = readDesktop("src-tauri", "src", "lib.rs");
  const trace = sliceBetween(tauri, "fn trace_database_command_error(", "struct InvokeTimer");
  assert.match(trace, /contention_snapshot/);
  assert.match(trace, /"contention": contention/);
});

test("Diagnostics loads runtime status automatically but checkpoints only on operator action", () => {
  const diagnostics = readDesktop("src", "pages", "DiagnosticsPage.tsx");
  const traceLoad = sliceBetween(diagnostics, "const loadTraceSection", "const replayYoutubeProtectionHistory");
  const checkpointAction = sliceBetween(diagnostics, "const runPassiveDatabaseCheckpoint", "const loadJobsSection");

  assert.match(traceLoad, /invoke<DatabaseRuntimeStatus>\("database_runtime_status"\)/);
  assert.doesNotMatch(traceLoad, /database_checkpoint_passive/);
  assert.match(checkpointAction, /invoke<WalCheckpointReceipt>\("database_checkpoint_passive"\)/);
  assert.match(checkpointAction, /invoke<DatabaseRuntimeStatus>\("database_runtime_status"\)/);
  assert.match(diagnostics, /data-testid="database-passive-checkpoint"/);
  assert.doesNotMatch(diagnostics, /data-testid="database-passive-checkpoint"[^>]*data-agent-safe-action/);
});

test("Database runtime help stays in the existing trace card and states evidence limits", () => {
  const diagnostics = readDesktop("src", "pages", "DiagnosticsPage.tsx");
  const traceCard = sliceBetween(diagnostics, '<div className="card" id="diag-trace">', '<div className="card" id="diag-failures">');
  const databaseSection = sliceBetween(traceCard, "<h2>Database runtime</h2>", "<h2>Diagnostics trace</h2>");

  assert.doesNotMatch(databaseSection, /className="card/);
  assert.match(databaseSection, /This is the live bounded SQLite access service, not a database scan/);
  assert.match(databaseSection, /it never runs a checkpoint/);
  assert.match(databaseSection, /without identifying a lock holder/);
  assert.match(databaseSection, /does not delete canonical records or infer which process holds a lock/);
  assert.match(databaseSection, /database-active-operations/);
  assert.match(databaseSection, /database-recent-operation-receipts/);
});

test("Demand registry classifies status as visibility read and checkpoint as mutation", () => {
  const trace = DIAGNOSTICS_OPERATION_REGISTRY.find((entry) => entry.id === "diagnostics.trace");
  const mutations = DIAGNOSTICS_OPERATION_REGISTRY.find((entry) => entry.id === "diagnostics.operator-mutation");

  assert.ok(trace);
  assert.ok(mutations);
  assert.ok(trace.commands.includes("database_runtime_status"));
  assert.ok(mutations.commands.includes("database_checkpoint_passive"));
  assert.equal(trace.trigger, "section_visibility");
  assert.equal(mutations.trigger, "operator_action");
  assert.equal(mutations.costClass, "mutation");
});

test("database shutdown never rejects work from a runner that failed its bounded join", () => {
  const tauri = readDesktop("src-tauri", "src", "lib.rs");
  const exitHandler = sliceBetween(
    tauri,
    "if let tauri::RunEvent::Exit = event",
    "        });",
  );

  assert.match(exitHandler, /runner_join_succeeded = runner_join\.is_ok\(\)/);
  assert.match(exitHandler, /if runner_join_succeeded \{[\s\S]*?shutdown_and_drain/);
  assert.match(exitHandler, /skipped_runner_not_joined/);
  const skipped = exitHandler.slice(exitHandler.indexOf("skipped_runner_not_joined"));
  assert.doesNotMatch(skipped, /shutdown_and_drain/);
});
