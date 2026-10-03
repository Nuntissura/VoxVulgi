import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { shouldOfferSelectedDownloadStart, selectedDownloadStartMessage } from "../src/lib/selectedDownloadStart.ts";
import type { YoutubeGateSnapshot } from "../src/lib/youtubeGateText.ts";

const normal: YoutubeGateSnapshot = { state: "ready", mode: "normal", hold_reason: null, next_eligible_at_ms: null, cooldown_attempt: 0, entered_at_ms: null };

test("ordinary pacing does not ask to change pause scope, while explicit pause and protection do", () => {
  assert.equal(shouldOfferSelectedDownloadStart(false, normal), false);
  assert.equal(shouldOfferSelectedDownloadStart(false, { ...normal, state: "waiting", hold_reason: "paced_after_youtube_start" }), false);
  assert.equal(shouldOfferSelectedDownloadStart(true, normal), true);
  assert.equal(shouldOfferSelectedDownloadStart(false, { ...normal, mode: "cooldown", state: "waiting" }), true);
  assert.equal(shouldOfferSelectedDownloadStart(false, { ...normal, state: "held", hold_reason: "youtube_auth_circuit_open" }), true);
  assert.equal(shouldOfferSelectedDownloadStart(false, { ...normal, mode: "cooldown", state: "waiting" }, false), false);
  assert.equal(shouldOfferSelectedDownloadStart(true, normal, false), true);
});

test("selection receipts preserve pause scope and never turn held into a download success", () => {
  assert.match(selectedDownloadStartMessage({ rest_paused: true, held: true, hold_reason: "adaptive_youtube_cooldown" }), /other queued work stays paused.*still waiting: adaptive_youtube_cooldown/);
  assert.match(selectedDownloadStartMessage({ rest_paused: false, held: false, hold_reason: null }), /queue resumes with this selection first.*pacing still applies/);
  assert.match(selectedDownloadStartMessage({ rest_paused: true, held: true, hold_reason: null, next_eligible_at_ms: 1791030621743 }), /still waiting: provider gate.*Next eligible check:/);
});

test("semantic UI and backend explicit selected decisions share hidden runner activation", () => {
  const desktop = readFileSync(new URL("../src-tauri/src/lib.rs", import.meta.url), "utf8");
  const bridge = readFileSync(new URL("../src-tauri/src/agent_control.rs", import.meta.url), "utf8");
  const body = (name: string) => desktop.slice(desktop.indexOf(`fn ${name}(`)).split("#[tauri::command]")[0];
  assert.match(body("jobs_start_selected_downloads"), /agent_control::reconcile_selected_runner_start\(&state.paths, &mut receipt\)/);
  assert.match(body("jobs_enqueue_selected_download_batch"), /if let Some\(start\) = receipt.start.as_mut\(\).*reconcile_selected_runner_start/);
  assert.doesNotMatch(body("jobs_enqueue_download_batch"), /ensure_explicit_headless_runner/);
  assert.match(bridge, /pub\(super\) fn ensure_explicit_headless_runner/);
  assert.match(bridge, /state.agent_headless && !safe_mode/);
  assert.match(bridge, /app.try_state::<AppState>\(\).is_none_or\(\|state\| state.safe_mode_enabled.load\(Ordering::SeqCst\)\)/);
});
