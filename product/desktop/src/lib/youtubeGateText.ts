// WP-0320/WP-0321 S4: plain-language text for the shared YouTube start gate, shared by the
// Video Archiver activity strip and the Jobs page so both surfaces say the same
// thing about a held/waiting gate instead of the raw engine `hold_reason`.
//
// Modes are "normal" | "cooldown" only. Hold reasons: adaptive_youtube_cooldown,
// adaptive_youtube_canary_pending, youtube_auth_circuit_open (sign-in breaker;
// next_eligible_at_ms = expiry), paced_after_youtube_start, youtube_po_provider_unavailable,
// queue_paused.

export type YoutubeGateSnapshot = {
  state: "ready" | "waiting" | "held" | string;
  next_eligible_at_ms: number | null;
  hold_reason: string | null;
  mode: string | null;
  cooldown_attempt: number;
  entered_at_ms: number | null;
};

function formatClock(ms: number, withSeconds: boolean): string {
  try {
    return new Date(ms).toLocaleTimeString([], withSeconds
      ? { hour: "2-digit", minute: "2-digit", second: "2-digit" }
      : { hour: "2-digit", minute: "2-digit" });
  } catch {
    return new Date(ms).toISOString();
  }
}

// Returns null when the gate is ready (nothing to tell the operator).
export function youtubeGateText(gate: YoutubeGateSnapshot | null | undefined): string | null {
  if (!gate) return null;
  // A cooldown in the durable policy state is a pause even if the runner has not observed the
  // gate yet (nothing queued): never hide it behind a "ready" runtime state.
  const pausedByPolicy = gate.mode === "cooldown";
  if (gate.state === "ready" && !pausedByPolicy) return null;

  if (gate.hold_reason === "youtube_auth_circuit_open") {
    const until = gate.next_eligible_at_ms ? formatClock(gate.next_eligible_at_ms, false) : "the wait expires";
    return `YouTube rejected the sign-in; downloads paused until ${until} or until you reconnect (Options → YouTube sign-in).`;
  }

  const isCooldown = pausedByPolicy || gate.hold_reason === "adaptive_youtube_cooldown" || gate.hold_reason === "adaptive_youtube_canary_pending";
  if (isCooldown) {
    const attempt = gate.cooldown_attempt > 0 ? `, attempt ${gate.cooldown_attempt}` : "";
    const since = gate.entered_at_ms ? ` since ${formatClock(gate.entered_at_ms, false)}` : "";
    const next = gate.next_eligible_at_ms
      ? ` — one test download at ${formatClock(gate.next_eligible_at_ms, false)}; the wait doubles after each failed test, up to the longest wait.`
      : ".";
    return `YouTube blocked downloads (cooldown${attempt})${since}${next}`;
  }

  if (gate.state === "waiting") {
    return gate.next_eligible_at_ms
      ? `Waiting for the safe-start window (next start ${formatClock(gate.next_eligible_at_ms, true)}).`
      : "Waiting for the safe-start window.";
  }

  if (gate.state === "held") {
    return gate.hold_reason ? `Paused: ${gate.hold_reason}` : "Paused: new starts temporarily held.";
  }

  return gate.hold_reason ? `Paused: ${gate.hold_reason}` : null;
}
