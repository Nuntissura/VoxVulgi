import type { YoutubeGateSnapshot } from "./youtubeGateText";

export function shouldOfferSelectedDownloadStart(paused: boolean, gate: YoutubeGateSnapshot, hasYoutube = true): boolean {
  return paused || (hasYoutube && (gate.mode === "cooldown" || gate.state === "held"));
}

export function selectedDownloadStartMessage(receipt: { rest_paused: boolean; held: boolean; hold_reason: string | null; next_eligible_at_ms?: number | null }): string {
  const scope = receipt.rest_paused ? "All other queued work stays paused." : "The queue resumes with this selection first.";
  const next = receipt.held && receipt.next_eligible_at_ms ? ` Next eligible check: ${new Date(receipt.next_eligible_at_ms).toLocaleString()}.` : "";
  return `${scope}${receipt.held ? ` This selection is still waiting: ${receipt.hold_reason || "provider gate"}.` : " Normal download pacing still applies."}${next}`;
}
