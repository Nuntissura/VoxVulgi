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
