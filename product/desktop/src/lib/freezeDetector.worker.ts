// WP-0221: Web Worker freeze detector.
//
// Runs on a dedicated browser thread so it survives WebView main-thread
// hangs. Posts a `ping` to the main thread every `pingIntervalMs`. If no
// `pong` arrives within `freezeThresholdMs`, declares a freeze and POSTs a
// `freeze_detected` event to the agent-bridge HTTP server (which runs on a
// Tokio task and is independent of the WebView main thread). When pong
// eventually arrives, POSTs a `freeze_recovered` event with the measured
// gap.

type InitMessage = {
  type: "init";
  bridgePort: number | null;
  pingIntervalMs: number;
  freezeThresholdMs: number;
};

type PongMessage = {
  type: "pong";
  pingId: number;
  context: {
    last_event: { name: string; ts_ms: number } | null;
    current_page: string | null;
  };
};

type WorkerInput = InitMessage | PongMessage;

let bridgePort: number | null = null;
let pingIntervalMs = 100;
let freezeThresholdMs = 250;
let started = false;

// WP-0221 (v0.1.20): liveness heartbeat. Sent every 30s so an agent can tell
// from the trace alone whether the Worker is actually running. If the
// heartbeat rows stop appearing while the Rust-side `runtime_sample` rows
// continue, the Worker died or never installed.
const WORKER_ALIVE_INTERVAL_MS = 30_000;
let lastAliveAt = 0;

let nextPingId = 1;
let lastSentPingId = 0;
let lastSentAt = 0;
let lastReceivedPongId = 0;
let inFreeze = false;
let freezeStartedAt = 0;
let lastContext: PongMessage["context"] | null = null;

self.addEventListener("message", (event: MessageEvent<WorkerInput>) => {
  const msg = event.data;
  if (!msg) return;
  if (msg.type === "init") {
    bridgePort = msg.bridgePort ?? null;
    if (typeof msg.pingIntervalMs === "number" && msg.pingIntervalMs > 0) {
      pingIntervalMs = msg.pingIntervalMs;
    }
    if (typeof msg.freezeThresholdMs === "number" && msg.freezeThresholdMs > 0) {
      freezeThresholdMs = msg.freezeThresholdMs;
    }
    startLoop();
    return;
  }
  if (msg.type === "pong") {
    if (msg.pingId > lastReceivedPongId) {
      lastReceivedPongId = msg.pingId;
    }
    lastContext = msg.context;
    if (inFreeze && msg.pingId >= lastSentPingId) {
      const totalMs = Date.now() - freezeStartedAt;
      postFreezeEvent("freeze_recovered", {
        total_freeze_ms: totalMs,
        last_event: msg.context?.last_event ?? null,
        current_page: msg.context?.current_page ?? null,
      });
      inFreeze = false;
      freezeStartedAt = 0;
    }
  }
});

function startLoop() {
  if (started) return;
  started = true;
  const tick = () => {
    try {
      const now = Date.now();
      const pingOutstanding = lastSentPingId > lastReceivedPongId;
      if (!inFreeze && pingOutstanding) {
        const sincePing = now - lastSentAt;
        if (sincePing >= freezeThresholdMs) {
          inFreeze = true;
          freezeStartedAt = lastSentAt;
          postFreezeEvent("freeze_detected", {
            gap_ms: sincePing,
            last_event: lastContext?.last_event ?? null,
            current_page: lastContext?.current_page ?? null,
            unanswered_ping_id: lastSentPingId,
          });
        }
      }
      // Keep one unanswered ping stable so its age can cross the freeze
      // threshold. Replacing it every 100 ms makes a 250 ms freeze
      // mathematically undetectable because `sincePing` never grows.
      if (!pingOutstanding) {
        lastSentPingId = nextPingId++;
        lastSentAt = now;
        (self as unknown as Worker).postMessage({ type: "ping", pingId: lastSentPingId });
      }
      if (now - lastAliveAt >= WORKER_ALIVE_INTERVAL_MS) {
        lastAliveAt = now;
        postFreezeEvent("worker_alive" as never, {
          uptime_ms: now,
          ping_interval_ms: pingIntervalMs,
          freeze_threshold_ms: freezeThresholdMs,
          in_freeze: inFreeze,
        });
      }
    } catch {
      // never let the loop crash silently — but never propagate either
    } finally {
      setTimeout(tick, pingIntervalMs);
    }
  };
  setTimeout(tick, pingIntervalMs);
}

function postFreezeEvent(
  event: "freeze_detected" | "freeze_recovered" | "worker_alive",
  details: Record<string, unknown>,
) {
  if (bridgePort == null) return;
  const url = `http://127.0.0.1:${bridgePort}/agent/freeze_event`;
  const level = event === "worker_alive" ? "info" : "warn";
  const body = JSON.stringify({ event, details, level });
  const ctrl = new AbortController();
  const timeoutId = setTimeout(() => ctrl.abort(), 3000);
  fetch(url, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body,
    signal: ctrl.signal,
  })
    .catch(() => {
      // best-effort: the detector must never raise
    })
    .finally(() => clearTimeout(timeoutId));
}
