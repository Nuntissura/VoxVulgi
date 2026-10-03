import test from "node:test";
import assert from "node:assert/strict";
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
