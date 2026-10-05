/** WP-0230 original state icons; no inferred install progress. */
export function phase2StatusIcon(status: string): string {
  switch (status) {
    case "done": return "✓";
    case "running": return "⟳";
    case "queued": return "⏸";
    case "skipped": return "—";
    case "failed":
    case "interrupted":
    case "stale": return "⚠";
    default: return "·";
  }
}

/** Existing journal totals take precedence; only confirmed absent history uses the supported plan. */
export function phase2ProgressTotal(
  steps: readonly unknown[],
  historyExists: boolean | undefined,
  plan: readonly { supported?: unknown }[] | null,
): number {
  if (steps.length > 0) return steps.length;
  if (historyExists !== false) return 0;
  return plan?.filter((pack) => pack.supported === true).length ?? 0;
}
