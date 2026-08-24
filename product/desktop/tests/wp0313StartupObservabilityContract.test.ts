import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import test from "node:test";
import assert from "node:assert/strict";

const root = fileURLToPath(new URL("..", import.meta.url));

function readRepoFile(...parts: string[]): string {
  return readFileSync(join(root, ...parts), "utf8");
}

test("startup status is revisioned event-first with a bounded adaptive snapshot fallback", () => {
  const app = readRepoFile("src", "App.tsx");
  const eventListen = app.indexOf('listen<StartupStatus>("voxvulgi://startup-status"');
  const snapshotRead = app.indexOf('invoke<StartupStatus>("startup_status")', eventListen);
  assert.ok(eventListen >= 0, "startup status must subscribe to the governed Tauri event");
  assert.ok(snapshotRead > eventListen, "subscribe must precede the canonical snapshot handshake");
  assert.match(app, /status\.revision\s*<\s*startupRevisionRef\.current/);
  assert.match(app, /Math\.min\(fallbackDelayMs \* 2, 30_000\)/);
  assert.doesNotMatch(
    app.slice(eventListen, snapshotRead + 1000),
    /intervalMs:\s*1200/,
    "the fixed 1.2 second startup poll must not return as the primary transport",
  );
});

test("provider verification progress reaches startup revisions during the single flight", () => {
  const tauri = readRepoFile("src-tauri", "src", "lib.rs");
  assert.match(tauri, /const STARTUP_STATUS_EVENT: &str = "voxvulgi:\/\/startup-status"/);
  assert.match(tauri, /tools::youtube_po_provider_verification_progress/);
  assert.match(tauri, /last_provider_revision != Some\(progress\.revision\)/);
  assert.match(tauri, /publish_provider_verification_progress/);
  assert.match(tauri, /files_completed\s*=\s*Some\(progress\.files_completed\)/);
  assert.match(tauri, /bytes_completed\s*=\s*Some\(progress\.bytes_completed\)/);
  assert.match(tauri, /single_flight_32_file_yield_256_file_1ms_checkpoint/);
});

test("the headless provider verification route admits one bounded worker flight", () => {
  const tauri = readRepoFile("src-tauri", "src", "lib.rs");
  assert.match(tauri, /AGENT_PROVIDER_VERIFY_ADMISSION_LOCK/);
  assert.match(tauri, /compare_exchange\(\s*false,\s*true/);
  assert.match(tauri, /AgentProviderVerifyAdmission::Joined/);
  assert.match(tauri, /"status":"joined_flight"/);
  assert.match(tauri, /struct AgentProviderVerifyFlightGuard/);
  assert.match(tauri, /impl Drop for AgentProviderVerifyFlightGuard/);
  assert.match(tauri, /catch_unwind/);
  assert.match(
    tauri,
    /agent_provider_verify_endpoint_admits_one_worker_and_joins_all_other_requests/,
  );
});

test("foreground Diagnostics and Options demand reduces provider verification checkpoints", () => {
  const app = readRepoFile("src", "App.tsx");
  const tauri = readRepoFile("src-tauri", "src", "lib.rs");
  const engine = readRepoFile("..", "engine", "src", "tools.rs");
  assert.match(app, /page !== "diagnostics" && page !== "options"/);
  assert.match(app, /invoke\("provider_verification_foreground_demand"/);
  assert.match(app, /setPressure\(false\)/);
  assert.match(tauri, /fn provider_verification_foreground_demand/);
  assert.match(tauri, /progress\.held_reason\.clone\(\)/);
  assert.match(tauri, /progress\.resource_policy\.clone\(\)/);
  assert.match(engine, /PROVIDER_VERIFICATION_FOREGROUND_LEASE_MS: i64 = 5_000/);
  assert.match(engine, /files_completed % 4 == 0/);
  assert.match(engine, /files_completed % 16 == 0/);
  assert.match(engine, /foreground_navigation_job_or_probe_demand/);
  assert.match(engine, /stale generation cannot clear a newer lease/);
});

test("main and Worker heartbeats preserve emitted receive persist and source-ack boundaries", () => {
  const main = readRepoFile("src", "lib", "freezeDetector.ts");
  const worker = readRepoFile("src", "lib", "freezeDetector.worker.ts");
  const tauri = readRepoFile("src-tauri", "src", "lib.rs");
  for (const source of [main, worker]) {
    assert.match(source, /emitted_at_ms/);
    assert.match(source, /source_acknowledged_at_ms/);
    assert.match(source, /acknowledgement_stage/);
    assert.match(source, /queue_dwell_ms/);
    assert.match(source, /queue_overflow/);
    assert.match(source, /sequence_gap/);
  }
  assert.match(tauri, /"persistence_started_at_ms"\.to_string\(\)/);
  assert.match(tauri, /let persisted_at_ms = now_epoch_ms_i64\(\);/);
  assert.match(tauri, /flush_diagnostics_trace_queue/);
  assert.match(tauri, /source_instance/);
  assert.match(tauri, /HEARTBEAT_DUPLICATES_TOTAL/);
  assert.match(tauri, /HEARTBEAT_LATE_TOTAL/);
  assert.match(tauri, /recv_timeout\(Duration::from_secs\(2\)\)/);
});
