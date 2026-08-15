import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import test from "node:test";
import assert from "node:assert/strict";

const root = fileURLToPath(new URL("..", import.meta.url));

function readRepoFile(...parts: string[]): string {
  return readFileSync(join(root, ...parts), "utf8");
}

test("diagnostics trace storage and writes are bounded", () => {
  const tauri = readRepoFile("src-tauri", "src", "lib.rs");

  assert.match(tauri, /DIAGNOSTICS_TRACE_NORMAL_MAX_BYTES:\s*u64\s*=\s*32\s*\*\s*1024\s*\*\s*1024/);
  assert.match(tauri, /DIAGNOSTICS_TRACE_INCIDENT_MAX_BYTES:\s*u64\s*=\s*64\s*\*\s*1024\s*\*\s*1024/);
  assert.match(tauri, /DIAGNOSTICS_TRACE_QUEUE_CAPACITY:\s*usize\s*=\s*2048/);
  assert.match(
    tauri,
    /sync_channel::<DiagnosticsTraceWriteRequest>\(DIAGNOSTICS_TRACE_QUEUE_CAPACITY\)/,
  );
  assert.match(tauri, /diagnostics_events_dropped/);
  assert.match(tauri, /DIAGNOSTICS_TRACE_ASYNC_WRITE_FAILURES_TOTAL/);
  assert.match(tauri, /async_write_failures_total/);
  const writer = tauri.slice(
    tauri.indexOf("fn diagnostics_trace_queue()"),
    tauri.indexOf("fn append_diagnostics_trace_row_best_effort"),
  );
  assert.match(writer, /record_diagnostics_persistence_failure\(\)/);
  assert.doesNotMatch(writer, /let _ = append_diagnostics_trace_row/);
  assert.match(tauri, /rotate_diagnostics_trace_if_needed/);
  assert.match(tauri, /diagnostics_trace\.1\.jsonl/);
});

test("incident captures correlate panels and expose quiet operator controls", () => {
  const tauri = readRepoFile("src-tauri", "src", "lib.rs");
  const app = readRepoFile("src", "App.tsx");
  const diagnostics = readRepoFile("src", "pages", "DiagnosticsPage.tsx");

  for (const command of [
    "diagnostics_capture_status",
    "diagnostics_capture_panel_transition",
    "diagnostics_capture_panel_transition_cancel",
    "diagnostics_capture_arm",
    "diagnostics_capture_disarm",
  ]) {
    assert.match(tauri, new RegExp(command));
  }
  assert.match(tauri, /incidents["\)]/);
  assert.match(tauri, /manifest\.json/);
  assert.match(app, /const panelSpanId = `panel-\$\{transitionId\}`/);
  assert.match(app, /await invoke\("diagnostics_capture_panel_transition"/);
  assert.match(app, /receipt\.activated_armed_capture[\s\S]*?await invoke\("diagnostics_capture_panel_transition_cancel"/);
  assert.match(app, /if \(panelTransitionSequenceRef\.current !== transitionId\) return;[\s\S]*?beforeCommit\?\.\(\);[\s\S]*?setPage\(next\)/);
  assert.match(tauri, /activate_panel_capture_before_navigation[\s\S]*?diagnostics_capture_envelope\(paths, "panel_switch", &details\)/);
  assert.match(tauri, /root_span_id[\s\S]*?parent_span_id/);
  assert.match(tauri, /panel_activation_precedes_first_destination_command_and_preserves_parent_span/);
  assert.match(app, /mounted_table_rows/);
  assert.match(app, /mounted_controls/);
  assert.match(diagnostics, /Arm next panel switch/);
  assert.match(diagnostics, /Arm next job start/);
  assert.match(diagnostics, /Capture budget/);
});

test("frontend long tasks and watcher WebView correlation remain machine readable", () => {
  const trace = readRepoFile("src", "lib", "diagnosticsTrace.ts");
  const watch = readRepoFile("..", "..", "governance", "scripts", "vv_watch.ps1");

  assert.match(trace, /PerformanceObserver\.supportedEntryTypes/);
  assert.match(trace, /frontend_long_task/);
  assert.match(watch, /webview_descendants/);
  assert.match(watch, /incident_event_count/);
  assert.match(watch, /function Get-WprCapability/);
  assert.match(watch, /capture_started = \$false/);
});

test("request span and invocation identity survive every measured command phase", () => {
  const tauri = readRepoFile("src-tauri", "src", "lib.rs");
  const library = readRepoFile("src", "pages", "LibraryPage.tsx");
  const jobs = readRepoFile("src", "pages", "JobsPage.tsx");

  assert.match(tauri, /"invocation_id": self\.invocation_id,[\s\S]*?"span_id": self\.span_id,[\s\S]*?"request_id": self\.request_id/);
  assert.doesNotMatch(tauri, /"span_id":\s*format!\("invoke-/);
  assert.match(tauri, /struct InvokePhaseRecorder[\s\S]*?invocation_id:\s*u64[\s\S]*?request_id:\s*Option<String>[\s\S]*?span_id:\s*Option<String>/);
  assert.match(tauri, /fn phase_recorder\(&self\)[\s\S]*?invocation_id:\s*self\.invocation_id[\s\S]*?request_id:\s*self\.request_id\.clone\(\)[\s\S]*?span_id:\s*self\.span_id\.clone\(\)/);
  for (const [command, phases] of Object.entries({
    library_query: ["dispatch_queue_wait", "db_open_prepare_step_map"],
    youtube_subscriptions_archive_stats: ["dispatch_queue_wait", "db_storage"],
    subscription_download_activity: ["dispatch_queue_wait", "db_open_prepare_step_map"],
    subscription_projections_rebuild: ["dispatch_queue_wait", "rebuild_reconciliation"],
    jobs_overview: ["dispatch_queue_wait", "db_open_prepare_step_map"],
    library_download_preflight: ["dispatch_queue_wait", "db_storage_observation"],
  })) {
    const start = tauri.indexOf(`async fn ${command}(`);
    assert.ok(start >= 0, `missing ${command}`);
    const next = tauri.indexOf("#[tauri::command]", start + 1);
    const body = tauri.slice(start, next < 0 ? undefined : next);
    assert.match(body, /let phase_recorder = timer\.phase_recorder\(\)/, `${command} must copy the exact timer invocation context`);
    for (const phase of phases) {
      assert.match(body, new RegExp(`(?:phase_recorder|timer)\\.phase\\("${phase}"`), `${command} missing ${phase}`);
    }
  }
  for (const source of [library, jobs]) {
    for (const command of ["library_query", "library_download_preflight", "jobs_overview", "subscription_download_activity"]) {
      const calls = source.match(new RegExp(`invoke<[^>]+>\\("${command}"[\\s\\S]{0,700}?\\}`, "g")) ?? [];
      for (const call of calls) {
        assert.match(call, /requestId/);
        assert.match(call, /spanId/);
      }
    }
  }
});

test("YouTube protection diagnostics keep download and enumeration causally distinct", () => {
  const diagnostics = readRepoFile("src", "pages", "DiagnosticsPage.tsx");
  assert.match(
    diagnostics,
    /const protectionRequestStartedAt = Date\.now\(\);[\s\S]*?const protectionContexts = \{[\s\S]*?requestId: `diagnostics-youtube-protection-download-\$\{protectionGeneration\}-\$\{protectionRequestStartedAt\}`,[\s\S]*?spanId: "diagnostics-youtube-protection-download"[\s\S]*?requestId: `diagnostics-youtube-protection-enumeration-\$\{protectionGeneration\}-\$\{protectionRequestStartedAt\}`,[\s\S]*?spanId: "diagnostics-youtube-protection-enumeration"/,
  );
  assert.equal((diagnostics.match(/protectionContexts\.download/g) ?? []).length, 3);
  assert.equal((diagnostics.match(/protectionContexts\.enumeration/g) ?? []).length, 3);
});
