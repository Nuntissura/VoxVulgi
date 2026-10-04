// WP-0322: fixture list — every normalized message family from the WP-0322
// evidence section must map to its intended Scope C class. Order-sensitivity
// (app_busy before any timeout/network rule) is asserted explicitly.
import test from "node:test";
import assert from "node:assert/strict";
import { classifyFailure, historyReadRetryDelay, type FailureKind } from "../src/lib/failureStates.ts";

test("history reads recover from transient admission failures with a finite retry budget", () => {
  const error = "Single-video history classification paused: database runtime error: read_admission_timeout";
  assert.deepEqual([1, 2, 3, 4, 5].map((failures) => historyReadRetryDelay(error, failures)),
    [2000, 5000, 10000, 20000, null]);
  assert.equal(historyReadRetryDelay("database is locked", 1), 2000);
  assert.equal(historyReadRetryDelay("no such table: library_item", 1), null);
  assert.equal(historyReadRetryDelay(null, 1), null);
  assert.equal(historyReadRetryDelay(error, 0), null);
});

test("maintenance backpressure stays recoverable and describes the database safeguard", () => {
  for (const reason of ["maintenance_unavailable", "maintenance_backlog_limit"]) {
    const message = `database runtime error: ${reason}`;
    const failure = classifyFailure(message);
    assert.equal(failure.kind, "app_busy");
    assert.equal(failure.label, "Database maintenance blocked new writes");
    assert.match(failure.appWillDo, /after recovery/);
    assert.equal(historyReadRetryDelay(message, 1), 2000);
    assert.equal(historyReadRetryDelay(message, 5), null);
  }
});

const FIXTURES: Array<{ message: string; kind: FailureKind }> = [
  // 58 subs mislabeled "Network problem" before WP-0322 — must be app_busy.
  { message: "database runtime error: writer_admission_timeout", kind: "app_busy" },
  { message: "writer_admission_timeout", kind: "app_busy" },
  { message: "read_admission_timeout", kind: "app_busy" },
  { message: "database is locked", kind: "app_busy" },
  // 81 yt-dlp timeouts.
  { message: "yt-dlp process timed out after 900s", kind: "youtube_not_responding" },
  { message: "managed_yt_dlp timed out after 60s", kind: "youtube_not_responding" },
  // 33 PO provider / engine unavailable.
  { message: "pinned localhost PO provider payload failed integrity validation", kind: "youtube_helper" },
  { message: "selected download engine is unavailable", kind: "youtube_helper" },
  // ~12 wrong tab / needs sign-in.
  { message: "This channel does not have a videos tab", kind: "wrong_link" },
  { message: "Playlists that require authentication to view are not supported", kind: "wrong_link" },
  // ~6 source gone.
  { message: "playlist does not exist", kind: "source_gone" },
  { message: "channel was removed", kind: "source_gone" },
  { message: "channel does not exist", kind: "source_gone" },
  { message: "account has been terminated", kind: "source_gone" },
  { message: "ERROR: Unable to download API page: HTTP Error 404: Not Found", kind: "source_gone" },
  // 2 internal merge-intent invariants.
  { message: "youtube archive merge intent target or members are invalid", kind: "internal" },
  { message: "managed output is already being finalized", kind: "internal" },
  // Instagram checkpoint.
  { message: "feedback_required", kind: "instagram_checkpoint" },
  // Additional Scope C classes not in the raw evidence dump but required by the catalogue.
  { message: "HTTP Error 429: Too Many Requests", kind: "youtube_blocked" },
  { message: "Sign in to confirm you're not a bot", kind: "youtube_blocked" },
  { message: "cookies were rejected", kind: "sign_in_rejected" },
  { message: "This video is members-only", kind: "members_only" },
  { message: "job stalled: no progress for 600s, watchdog backstop firing", kind: "stalled" },
  { message: "no space left on device", kind: "storage" },
  // WP-0325: slow/offline NAS destination and missing alias destination (live 0.1.205 text).
  {
    message:
      "download folder unreachable: model/tool install failed: download folder is not responding: Z:\\Video\\4K Video\\4K Video 21-08-2025 (the NAS or drive did not answer within 3 s)",
    kind: "storage",
  },
  {
    message:
      "model/tool install failed: verified root alias target is currently unavailable: Z:\\Video\\4K Video\\4K Video 21-08-2025",
    kind: "storage",
  },
  { message: "some completely novel wording never seen before", kind: "unknown" },
];

for (const { message, kind } of FIXTURES) {
  test(`classifies "${message}" as ${kind}`, () => {
    assert.equal(classifyFailure(message).kind, kind);
  });
}

test("app_busy is matched before youtube_not_responding even when both patterns could apply", () => {
  const state = classifyFailure("database runtime error: writer_admission_timeout after 30s");
  assert.equal(state.kind, "app_busy");
});

test("app_busy carries no operator action", () => {
  const state = classifyFailure("writer_admission_timeout");
  assert.deepEqual(state.actions, []);
  assert.equal(state.whoActs, "app");
});

test("source_gone offers keep-as-archive and mark-deleted actions", () => {
  const state = classifyFailure("channel was removed");
  assert.deepEqual(state.actions, ["keep_as_archive", "mark_deleted"]);
  assert.equal(state.whoActs, "you");
});

test("youtube_not_responding adds slower_pacing after 3+ consecutive failures", () => {
  const withoutStreak = classifyFailure("yt-dlp timed out after 900s");
  assert.deepEqual(withoutStreak.actions, ["retry_now"]);
  const withStreak = classifyFailure("yt-dlp timed out after 900s", { consecutiveFailures: 3 });
  assert.deepEqual(withStreak.actions, ["retry_now", "slower_pacing"]);
});

test("empty/null input classifies as ok with no rendering", () => {
  assert.equal(classifyFailure(null).kind, "ok");
  assert.equal(classifyFailure("").kind, "ok");
  assert.equal(classifyFailure("   ").kind, "ok");
});

test("unknown failures still expose a Technical details affordance via yourFix", () => {
  const state = classifyFailure("some completely novel wording never seen before");
  assert.equal(state.kind, "unknown");
  assert.match(state.yourFix ?? "", /technical details/i);
});
