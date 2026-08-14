import { invoke } from "@tauri-apps/api/core";

let performanceDiagnosticsInstalled = false;
let currentPage: string | null = null;
const TRACE_QUEUE_CAPACITY = 256;
const TRACE_PAYLOAD_LIMIT_BYTES = 32 * 1024;
type TraceItem = { event: string; details: unknown; level: "info" | "warn" | "error"; repeats: number; semanticKey: string };
const traceQueue: TraceItem[] = [];
let traceDrainActive = false;
let frontendDroppedTotal = 0;
let backendDroppedTotal = 0;
let backendAsyncWriteFailuresTotal = 0;
let backendPendingLossEvents = 0;

function payloadBytes(value: unknown): number {
  try { return new TextEncoder().encode(JSON.stringify(value)).byteLength; } catch { return TRACE_PAYLOAD_LIMIT_BYTES + 1; }
}

function tokenizeQuotedText(input: string): string[] {
  const tokens: string[] = [];
  let current = "";
  let quote: "'" | '"' | null = null;
  for (const character of input) {
    if (quote) {
      current += character;
      if (character === quote) quote = null;
    } else if (character === "'" || character === '"') {
      quote = character;
      current += character;
    } else if (/\s/.test(character)) {
      if (current) {
        tokens.push(current);
        current = "";
      }
    } else current += character;
  }
  if (current) tokens.push(current);
  return tokens;
}

function sensitiveFreeTextName(value: string): boolean {
  const normalized = value.trim().replace(/^-+/, "").replace(/:$/, "").replace(/^(['"])(.*)\1$/, "$2").toLowerCase();
  return ["password", "passwd", "token", "secret", "key", "api-key", "api_key", "apikey", "cookie", "cookies", "proxy", "authorization"].includes(normalized);
}

function authorizationFreeTextName(value: string): boolean {
  const normalized = value.trim().replace(/^-+/, "").replace(/:+$/, "").replace(/^(['"])(.*)\1$/, "$2").toLowerCase();
  return normalized === "authorization" || normalized === "proxy-authorization";
}

function authorizationScheme(value: string): boolean {
  const normalized = value.trim().replace(/^(['"])(.*)\1$/, "$2").replace(/[:=]+$/, "").toLowerCase();
  return normalized === "bearer" || normalized === "basic";
}

function redactQueueText(input: string): string {
  // A quoted --header value is one shell token containing spaces, so consume the complete
  // Authorization tuple before the generic token pass.
  input = input
    .replace(/(--header(?:=|\s+))(["'])\s*((?:proxy-)?authorization)\s*:\s*[^"']*\2/gi, "$1$2$3: <redacted>$2")
    .replace(/\b((?:proxy-)?authorization)\b\s*[:=]\s*(?:bearer|basic)\s+(?:"[^"]*"|'[^']*'|[^\s,;:=]+)/gi, "$1: <redacted>");
  const output: string[] = [];
  let redactNext = false;
  let authorizationState: "none" | "value_or_scheme" | "credential" = "none";
  for (const token of tokenizeQuotedText(input)) {
    if (authorizationState !== "none") {
      if (token === "=" || token === ":") {
        output.push(token);
        continue;
      }
      const value = token.replace(/^[:=]+/, "");
      const prefix = token.slice(0, token.length - value.length);
      if (!value) {
        output.push(token);
        continue;
      }
      const wasScheme: boolean = authorizationState === "value_or_scheme" && authorizationScheme(value);
      output.push(`${prefix}<redacted>`);
      authorizationState = wasScheme ? "credential" : "none";
      continue;
    }
    if (redactNext) {
      if (token === "=" || token === ":") {
        output.push(token);
        continue;
      }
      output.push("<redacted>");
      redactNext = false;
      continue;
    }
    if (token.toLowerCase() === "bearer") {
      output.push(token);
      redactNext = true;
      continue;
    }
    if (authorizationFreeTextName(token)) {
      output.push(token);
      authorizationState = "value_or_scheme";
      continue;
    }
    if (sensitiveFreeTextName(token)) {
      output.push(token);
      redactNext = true;
      continue;
    }
    const equalsAt = token.indexOf("=");
    if (equalsAt >= 0 && sensitiveFreeTextName(token.slice(0, equalsAt))) {
      output.push(`${token.slice(0, equalsAt)}=<redacted>`);
      const value = token.slice(equalsAt + 1);
      if (authorizationFreeTextName(token.slice(0, equalsAt))) {
        authorizationState = value.length === 0 ? "value_or_scheme" : authorizationScheme(value) ? "credential" : "none";
      } else {
        redactNext = value.length === 0;
      }
      continue;
    }
    const colonAt = token.indexOf(":");
    if (colonAt >= 0 && sensitiveFreeTextName(token.slice(0, colonAt))) {
      output.push(`${token.slice(0, colonAt)}:<redacted>`);
      const value = token.slice(colonAt + 1);
      if (authorizationFreeTextName(token.slice(0, colonAt))) {
        authorizationState = value.length === 0 ? "value_or_scheme" : authorizationScheme(value) ? "credential" : "none";
      } else {
        redactNext = value.length === 0;
      }
      continue;
    }
    output.push(token.replace(/([a-z][a-z0-9+.-]*:\/\/)[^/@\s]+@/i, "$1<redacted>@"));
  }
  return output.join(" ");
}

function redactQueueValue(value: unknown, key = ""): unknown {
  const sensitive = /password|passwd|token|secret|cookie|authorization|api[_-]?key|proxy|command_line/i.test(key);
  if (sensitive) return "<redacted>";
  if (Array.isArray(value)) return value.map((entry) => redactQueueValue(entry));
  if (value && typeof value === "object") return Object.fromEntries(Object.entries(value as Record<string, unknown>).sort(([a], [b]) => a.localeCompare(b)).map(([entryKey, entry]) => [entryKey, redactQueueValue(entry, entryKey)]));
  if (typeof value === "string") return redactQueueText(value);
  return value;
}

function traceSemanticKey(event: string, level: string, details: unknown): string {
  return JSON.stringify({ event, level, details: redactQueueValue(details) });
}
export const diagnosticsTraceSemanticKeyForTest = traceSemanticKey;

async function drainTraceQueue(): Promise<void> {
  if (traceDrainActive) return;
  traceDrainActive = true;
  try {
    while (traceQueue.length) {
      const item = traceQueue.shift()!;
      try {
        const receipt = await invoke<{ accepted: boolean; dropped_events_total: number; async_write_failures_total?: number; pending_loss_events?: number }>("diagnostics_trace_write_event", {
          event: item.event,
          details: { ...(typeof item.details === "object" && item.details ? item.details : { value: item.details }), repeats: item.repeats, frontend_dropped_total: frontendDroppedTotal },
          level: item.level,
        });
        backendDroppedTotal = Math.max(backendDroppedTotal, receipt.dropped_events_total ?? 0);
        backendAsyncWriteFailuresTotal = Math.max(backendAsyncWriteFailuresTotal, receipt.async_write_failures_total ?? 0);
        backendPendingLossEvents = Math.max(0, receipt.pending_loss_events ?? 0);
        if (!receipt.accepted) frontendDroppedTotal += item.repeats;
      } catch { frontendDroppedTotal += item.repeats; }
    }
  } finally { traceDrainActive = false; }
}

export function setDiagnosticsTracePage(page: string | null) {
  currentPage = page;
}

export function installPerformanceDiagnostics(): void {
  if (performanceDiagnosticsInstalled) return;
  performanceDiagnosticsInstalled = true;
  if (typeof PerformanceObserver === "undefined") return;
  try {
    const supported = PerformanceObserver.supportedEntryTypes ?? [];
    if (!supported.includes("longtask")) {
      void diagnosticsTrace("frontend_long_task_unavailable", {
        page: currentPage,
        supported_entry_types: supported,
      });
      return;
    }
    const observer = new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) {
        void diagnosticsTrace(
          "frontend_long_task",
          {
            page: currentPage,
            start_time_ms: Math.round(entry.startTime),
            duration_ms: Math.round(entry.duration),
            entry_name: entry.name,
          },
          entry.duration >= 250 ? "warn" : "info",
        );
      }
    });
    observer.observe({ type: "longtask", buffered: true });
  } catch (error) {
    void diagnosticsTrace(
      "frontend_long_task_install_failed",
      { page: currentPage, error: String(error) },
      "warn",
    );
  }
}

export function diagnosticsTrace(
  event: string,
  details: unknown = null,
  level: "info" | "warn" | "error" = "info",
): Promise<void> {
  if (payloadBytes({ event, details, level }) > TRACE_PAYLOAD_LIMIT_BYTES) {
    frontendDroppedTotal += 1;
    details = { reason: "payload_too_large", frontend_dropped_total: frontendDroppedTotal };
    event = "frontend_diagnostics_payload_dropped";
    level = "warn";
  }
  const previous = traceQueue[traceQueue.length - 1];
  const semanticKey = traceSemanticKey(event, level, details);
  if (previous?.semanticKey === semanticKey) previous.repeats += 1;
  else if (traceQueue.length < TRACE_QUEUE_CAPACITY) traceQueue.push({ event, details, level, repeats: 1, semanticKey });
  else frontendDroppedTotal += 1;
  void drainTraceQueue();
  return Promise.resolve();
}

export function diagnosticsTraceQueueStatus() {
  return { queued: traceQueue.length, frontendDroppedTotal, backendDroppedTotal, backendAsyncWriteFailuresTotal, backendPendingLossEvents, active: traceDrainActive };
}
