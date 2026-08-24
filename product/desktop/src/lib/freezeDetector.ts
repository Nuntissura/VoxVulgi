// WP-0221: Main-thread driver for the freeze detector Worker.
//
// Responsibilities:
//   - Spawn the dedicated freeze-detector Worker at app boot.
//   - Hand the agent-bridge port to the Worker so it can POST freeze events
//     to /agent/freeze_event without using Tauri IPC (which routes through
//     the WebView main thread we are trying to observe).
//   - Respond to the Worker's `ping` messages with a `pong` carrying the
//     last fired window event and the current page so a freeze record has
//     useful context.
//   - Emit a main-thread fallback heartbeat (`main_thread_alive`) every 30
//     seconds so the trace records JS-side liveness even when the Worker
//     fails to install or dies. Long gaps between consecutive heartbeats
//     post-hoc reveal main-thread freezes.
//   - Emit install-step telemetry via diagnostics_trace_write_event so an
//     agent can read the trace and know exactly where Worker construction
//     succeeded or failed (rather than the previous silent try/catch).

import { invoke } from "@tauri-apps/api/core";
// WP-0221 v0.1.22: Vite `?worker` shorthand is the only import form that
// correctly bundles the worker as a `.js` script with module-graph handling.
// v0.1.21 attempted `new Worker(new URL("./freezeDetector.worker.ts", ...))`
// instead, which Vite treated as a raw asset copy (resulting in a `.ts`
// extension and unprocessed TypeScript that the browser refused to execute).
// Trust the shorthand; debug failures via the install telemetry below
// instead of by changing the import form.
// eslint-disable-next-line import/no-unresolved
import FreezeWorker from "./freezeDetector.worker?worker";

let worker: Worker | null = null;
let lastEvent: { name: string; ts_ms: number } | null = null;
let currentPage: string | null = null;
let mainThreadHeartbeatTimer: number | null = null;
let mainThreadHeartbeatTick = 0;
const mainThreadHeartbeatSourceInstance = crypto.randomUUID();

type HeartbeatTraceReceipt = {
  accepted: boolean;
  received_at_ms: number;
  enqueued_at_ms: number;
  persisted_at_ms: number | null;
  acknowledged_at_ms: number;
  acknowledgement_stage: "persisted" | "queued" | "rejected";
  queue_dwell_ms: number | null;
  outcome: string;
  queue_overflow: boolean;
  heartbeat_duplicates_total: number;
  heartbeat_late_total: number;
  duplicate: boolean;
  late: boolean;
  sequence_gap: number;
};

const TRACKED_WINDOW_EVENTS = ["resize", "focus", "blur", "visibilitychange"] as const;
const MAIN_THREAD_HEARTBEAT_INTERVAL_MS = 30_000;

export function setFreezeDetectorPage(page: string | null) {
  currentPage = page;
}

// Fire-and-forget trace write that never raises and never blocks the caller.
function traceWrite(event: string, details: Record<string, unknown>, level = "info") {
  void invoke("diagnostics_trace_write_event", { event, details, level }).catch(
    () => {
      // best-effort
    },
  );
}

async function traceHeartbeat(sequence: number) {
  const emittedAtMs = Date.now();
  const details = {
    source: "main_thread",
    source_instance: mainThreadHeartbeatSourceInstance,
    sequence,
    emitted_at_ms: emittedAtMs,
    uptime_ms: performance.now(),
    worker_installed: worker !== null,
    last_event: lastEvent,
    current_page: currentPage,
  };
  try {
    const receipt = await invoke<HeartbeatTraceReceipt>("diagnostics_trace_write_event", {
      event: "main_thread_alive",
      details,
      level: "info",
    });
    const sourceAcknowledgedAtMs = Date.now();
    traceWrite("heartbeat_source_acknowledged", {
      ...details,
      received_at_ms: receipt.received_at_ms,
      enqueued_at_ms: receipt.enqueued_at_ms,
      persisted_at_ms: receipt.persisted_at_ms,
      source_acknowledged_at_ms: sourceAcknowledgedAtMs,
      acknowledgement_stage: receipt.acknowledgement_stage,
      queue_dwell_ms: receipt.queue_dwell_ms,
      queue_overflow: receipt.queue_overflow,
      duplicate: receipt.duplicate,
      late: receipt.late,
      sequence_gap: receipt.sequence_gap,
      duplicate_overflow_total: receipt.heartbeat_duplicates_total,
      late_total: receipt.heartbeat_late_total,
      outcome: receipt.outcome,
    });
  } catch {
    // A missed acknowledgement is itself visible as a missing paired receipt.
  }
}

function startMainThreadHeartbeat() {
  if (mainThreadHeartbeatTimer !== null) return;
  const tick = () => {
    mainThreadHeartbeatTick += 1;
    void traceHeartbeat(mainThreadHeartbeatTick);
  };
  // Fire immediately so the first heartbeat lands without waiting 30s.
  tick();
  mainThreadHeartbeatTimer = window.setInterval(
    tick,
    MAIN_THREAD_HEARTBEAT_INTERVAL_MS,
  );
}

export async function installFreezeDetector(): Promise<void> {
  if (worker) return;
  traceWrite("freeze_detector_install_attempted", {
    user_agent: navigator.userAgent,
    location_href: window.location.href,
  });
  // Always start the main-thread heartbeat first so we have a liveness
  // signal even if the Worker construction below blows up.
  try {
    startMainThreadHeartbeat();
  } catch (err) {
    traceWrite(
      "freeze_detector_install_failed",
      { stage: "main_thread_heartbeat", error: String(err) },
      "warn",
    );
  }

  try {
    for (const evName of TRACKED_WINDOW_EVENTS) {
      window.addEventListener(
        evName,
        () => {
          lastEvent = { name: evName, ts_ms: Date.now() };
        },
        { capture: true, passive: true },
      );
    }

    let bridgePort: number | null = null;
    try {
      const p = await invoke<number | null>("agent_bridge_port");
      bridgePort = typeof p === "number" ? p : null;
    } catch (err) {
      traceWrite(
        "freeze_detector_install_failed",
        { stage: "agent_bridge_port", error: String(err) },
        "warn",
      );
      bridgePort = null;
    }

    // WP-0221 v0.1.22: the Vite `?worker` shorthand emits a bundled `.js`
    // chunk and a constructor that wraps `new Worker(url, { type: "module" })`
    // with the right options for the Tauri WebView. Earlier silent failures
    // in v0.1.18-v0.1.20 were never proven to be from the import form; the
    // install telemetry rows below will identify the real cause when it
    // reappears.
    let w: Worker;
    try {
      w = new FreezeWorker({ name: "voxvulgi-freeze-detector" });
    } catch (err) {
      traceWrite(
        "freeze_detector_install_failed",
        {
          stage: "worker_construct",
          error: String(err),
        },
        "warn",
      );
      return;
    }

    w.addEventListener("error", (e: ErrorEvent) => {
      traceWrite(
        "freeze_detector_install_failed",
        {
          stage: "worker_runtime_error",
          message: e.message,
          filename: e.filename,
          lineno: e.lineno,
          colno: e.colno,
        },
        "warn",
      );
    });
    w.addEventListener("messageerror", () => {
      traceWrite(
        "freeze_detector_install_failed",
        { stage: "worker_message_error" },
        "warn",
      );
    });
    w.addEventListener("message", (event: MessageEvent) => {
      const msg = event.data as { type?: string; pingId?: number };
      if (msg?.type !== "ping" || typeof msg.pingId !== "number") return;
      try {
        w.postMessage({
          type: "pong",
          pingId: msg.pingId,
          context: { last_event: lastEvent, current_page: currentPage },
        });
      } catch {
        // ignore
      }
    });
    w.postMessage({
      type: "init",
      bridgePort,
      pingIntervalMs: 100,
      freezeThresholdMs: 250,
    });
    worker = w;
    traceWrite("freeze_detector_install_succeeded", {
      bridge_port: bridgePort,
      freeze_threshold_ms: 250,
      ping_interval_ms: 100,
    });
  } catch (err) {
    // eslint-disable-next-line no-console
    console.warn("[freeze-detector] failed to install", err);
    traceWrite(
      "freeze_detector_install_failed",
      { stage: "outer_catch", error: String(err) },
      "warn",
    );
  }
}
