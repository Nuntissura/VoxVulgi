import { classifyFailure } from "./failureStates";

export type ActivityJob = {
  id: string; target_title: string | null; job_type: string; status: string;
  progress: number; error: string | null; params_json: string; track: string;
  // WP-0321 S6: downloads reopen the same row on retry, so this stays null there; use
  // attempt_no > 1 to detect a retried download. Other job types still populate it.
  retry_replacement_job_id?: string | null;
  attempt_no?: number;
};
export type LiveActivity = {
  updated_at_ms: number; phase: string; title: string | null;
  transfer_updated_at_ms?: number;
  downloaded_bytes: number | null; total_bytes: number | null;
  speed: number | null; eta: number | null; lines: string[];
};
export type ArchiveQuality = { state: string; width: number; height: number; fps: number | null;
  subtitles: { language: string; kind: string }[]; english_available: boolean; original_available: boolean | null };
export type ActivityRow = { job: ActivityJob; live: LiveActivity | null; quality?: ArchiveQuality | null };
export function activityTitle({ job, live }: ActivityRow): string {
  if (live?.title) return live.title;
  if (job.target_title) return job.target_title;
  try {
    const p = JSON.parse(job.params_json);
    if (typeof p.title === "string" && p.title.trim()) return p.title;
    if (typeof p.url === "string") {
      const url = new URL(p.url);
      // Never display embedded authentication or signed query parameters as the title fallback.
      const videoId = /(^|\.)youtube\.com$/i.test(url.hostname) ? url.searchParams.get("v") : null;
      return `${url.hostname}${url.pathname}${videoId ? ` · ${videoId}` : ""}`;
    }
  } catch { /* Missing metadata remains explicit. */ }
  return job.job_type.includes("subscription") ? "Checking a subscription" : "Title not available yet";
}
export function transferPercent(live: LiveActivity | null): number | null {
  if (live?.phase !== "downloading" || live.total_bytes == null || live.downloaded_bytes == null || live.total_bytes <= 0) return null;
  return Math.min(100, Math.max(0, 100 * live.downloaded_bytes / live.total_bytes));
}
export function activityStage({ job, live }: ActivityRow): string {
  if (job.status === "queued") return "Waiting to start";
  if (job.status === "succeeded") return "Finished";
  if (job.status === "failed") return "Needs attention";
  if (job.status === "canceled") return "Stopped";
  if (live?.phase === "downloading") return "Downloading";
  if (live?.phase === "processing") return "Preparing the saved media";
  if (live?.phase === "waiting") return "Waiting before the next request";
  if (job.job_type.includes("subscription")) return "Checking for new media";
  if (job.job_type === "download_image_batch") return "Collecting images";
  return live ? "Preparing download" : "Running · live detail unavailable";
}
export function formatBytes(value: number): string {
  const units = ["B", "KB", "MB", "GB"];
  let index = 0;
  while (value >= 1024 && index < units.length - 1) { value /= 1024; index++; }
  return `${value.toFixed(index > 0 ? 1 : 0)} ${units[index]}`;
}
export function activityFailure(error: string): string {
  const failure = classifyFailure(error);
  // A recorded failed attempt does not prove another attempt is scheduled.
  switch (failure.kind) {
    case "app_busy": return "The app's own database was briefly busy. Retries automatically.";
    case "youtube_not_responding": return "yt-dlp stopped responding — usually YouTube throttling. Retries automatically.";
    case "youtube_blocked": return "YouTube is limiting requests. Retries automatically after a cooldown.";
    case "unknown": return "Open Details to inspect the recorded error.";
    default: return `${failure.label}. ${failure.requirement}`;
  }
}
