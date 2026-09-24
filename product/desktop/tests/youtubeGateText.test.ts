import test from "node:test";
import assert from "node:assert/strict";
import { youtubeGateText, type YoutubeGateSnapshot } from "../src/lib/youtubeGateText";

function gate(overrides: Partial<YoutubeGateSnapshot>): YoutubeGateSnapshot {
  return {
    state: "ready",
    next_eligible_at_ms: null,
    hold_reason: null,
    mode: null,
    cooldown_attempt: 0,
    entered_at_ms: null,
    ...overrides,
  };
}

test("ready gate with no policy pause returns null", () => {
  assert.equal(youtubeGateText(null), null);
  assert.equal(youtubeGateText(undefined), null);
  assert.equal(youtubeGateText(gate({ state: "ready", mode: "normal" })), null);
});

test("cooldown mode reports attempt count and next test download time", () => {
  const next = Date.UTC(2026, 8, 23, 14, 30);
  const entered = Date.UTC(2026, 8, 23, 13, 0);
  const text = youtubeGateText(gate({
    state: "held",
    mode: "cooldown",
    cooldown_attempt: 2,
    entered_at_ms: entered,
    next_eligible_at_ms: next,
  })) ?? "";
  assert.match(text, /^YouTube blocked downloads \(cooldown, attempt 2\) since /);
  assert.match(text, / — one test download at .+; the wait doubles after each failed test, up to the longest wait\.$/);
});

test("cooldown hold reasons (adaptive_youtube_cooldown, adaptive_youtube_canary_pending) read as cooldown text", () => {
  for (const holdReason of ["adaptive_youtube_cooldown", "adaptive_youtube_canary_pending"]) {
    const text = youtubeGateText(gate({ state: "held", mode: null, hold_reason: holdReason, cooldown_attempt: 0 }));
    assert.match(text ?? "", /^YouTube blocked downloads \(cooldown\)\.$/);
  }
});

test("youtube_auth_circuit_open reports the sign-in breaker text", () => {
  const next = Date.UTC(2026, 8, 23, 15, 0);
  const text = youtubeGateText(gate({
    state: "held",
    mode: null,
    hold_reason: "youtube_auth_circuit_open",
    next_eligible_at_ms: next,
  }));
  assert.match(text ?? "", /^YouTube rejected the sign-in; downloads paused until .+ or until you reconnect \(Options → YouTube sign-in\)\.$/);
});

test("waiting state without a policy pause reports the safe-start window", () => {
  const next = Date.UTC(2026, 8, 23, 10, 0, 5);
  const text = youtubeGateText(gate({ state: "waiting", mode: null, next_eligible_at_ms: next }));
  assert.match(text ?? "", /^Waiting for the safe-start window \(next start .+\)\.$/);

  const textWithoutTime = youtubeGateText(gate({ state: "waiting", mode: null }));
  assert.equal(textWithoutTime, "Waiting for the safe-start window.");
});

test("other hold reasons (paced_after_youtube_start, youtube_po_provider_unavailable, queue_paused) fall back to a plain paused message", () => {
  for (const holdReason of ["paced_after_youtube_start", "youtube_po_provider_unavailable", "queue_paused"]) {
    const text = youtubeGateText(gate({ state: "held", mode: null, hold_reason: holdReason }));
    assert.equal(text, `Paused: ${holdReason}`);
  }
});

test("ready state hides a stale hold reason once the policy pause clears", () => {
  assert.equal(youtubeGateText(gate({ state: "ready", mode: "normal", hold_reason: "adaptive_youtube_cooldown" })), null);
});
