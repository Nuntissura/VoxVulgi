import { invoke } from "@tauri-apps/api/core";

export async function diagnosticsTrace(
  event: string,
  details: unknown = null,
  level: "info" | "warn" | "error" = "info",
): Promise<void> {
  try {
    await invoke("diagnostics_trace_write_event", {
      event,
      details,
      level,
    });
  } catch {
    // Never fail UI flows because diagnostics logging is unavailable.
  }
}
