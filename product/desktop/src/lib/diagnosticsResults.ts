import type { DiagnosticsDemandState } from "./diagnosticsDemandCoordinator";

export class DiagnosticReadErrors extends Error {
  constructor(readonly errors: readonly unknown[], message: string) {
    super(message);
    this.name = "DiagnosticReadErrors";
  }
}

// Reverse-mapped result fields infer each command's own return type even when
// the promise is passed through the generic demand coordinator.
export async function collectDiagnosticsFields<T extends Record<string, unknown>>(
  reads: { [K in keyof T]: () => Promise<T[K]> },
): Promise<T> {
  const values: Partial<T> = {};
  const errors: Error[] = [];
  for (const name of Object.keys(reads) as Array<keyof T>) {
    try { values[name] = await reads[name](); }
    catch (error) { errors.push(new Error(`${String(name)}: ${String(error)}`)); }
  }
  if (errors.length) throw new DiagnosticReadErrors(errors, errors.map((error) => error.message).join("; "));
  // Every key was visited and no read failed; the partial accumulator is complete.
  return values as T;
}

export type DiagnosticFieldResults<T> = { values: Partial<T>; errors: string[] };

// A shared flight transports partial successes to every current waiter rather
// than relying solely on the original caller's generation-bound callbacks.
export async function collectDiagnosticsFieldResults<T extends Record<string, unknown>>(
  reads: { [K in keyof T]: () => Promise<T[K]> },
): Promise<DiagnosticFieldResults<T>> {
  const values: Partial<T> = {};
  const errors: string[] = [];
  for (const name of Object.keys(reads) as Array<keyof T>) {
    try { values[name] = await reads[name](); }
    catch (error) { errors.push(`${String(name)}: ${String(error)}`); }
  }
  return { values, errors };
}

// Wait for all independently committing demands before reporting terminal failure.
export async function settleDiagnosticDemands<const T extends readonly unknown[]>(demands: { [K in keyof T]: Promise<T[K]> }): Promise<T> {
  const results = await Promise.allSettled(demands);
  const failures = results.filter((result): result is PromiseRejectedResult => result.status === "rejected");
  if (failures.length) throw new DiagnosticReadErrors(failures.map((result) => result.reason), failures.map((result) => String(result.reason)).join("; "));
  return results.map((result) => (result as PromiseFulfilledResult<unknown>).value) as unknown as T;
}

export function diagnosticPendingText(state: DiagnosticsDemandState): string {
  if (state === "queued" || state === "loading") return "Checking…";
  if (state === "failed") return "Unknown — check failed";
  if (state === "stale") return "Unknown — refresh needed";
  if (state === "ready") return "Unknown — no result";
  return "Not checked";
}

export function capabilityIsVerified(status: { probe_state: string; probe_error: string | null; freshness: string } | null): boolean {
  return !!status && status.probe_state === "verified"
    && !status.probe_error && !status.freshness.startsWith("stale");
}
