import { Fragment, useCallback, useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { confirm, save } from "@tauri-apps/plugin-dialog";
import { usePageActivity, usePollingLoop } from "../lib/activity";
import { copyPathToClipboard, openPathBestEffort, requireOpenablePath, revealPath } from "../lib/pathOpener";
import {
  buildJobContextSummary,
  jobTrackLabel,
  safeParseJobParams,
  stringOrNull,
  summarizeJobGroupTargets,
  type CanonicalJobTrack,
  type DisplayJobTrack,
  type JobContextSummary,
} from "../lib/archiverRuntime";
// WP-0264: shared failure-state classifier (same rules the subscription panel uses).
import { classifyFailure, toneStyle } from "../lib/failureStates";
import { diagnosticsTrace } from "../lib/diagnosticsTrace";
import {
  titleProvenanceLabel,
  type CanonicalTitleProjection,
} from "../lib/providerMetadata";

type JobStatus = "queued" | "running" | "succeeded" | "failed" | "canceled";

type JobRow = CanonicalTitleProjection & {
  id: string;
  item_id: string | null;
  batch_id: string | null;
  job_type: string;
  status: JobStatus;
  progress: number;
  error: string | null;
  created_at_ms: number;
  started_at_ms: number | null;
  finished_at_ms: number | null;
  logs_path: string;
  params_json?: string;
  retry_of_job_id?: string | null;
  retry_replacement_job_id?: string | null;
  // WP-0270: durable scheduler classification. Null/unknown means the engine
  // has not backfilled a legacy row; the UI must render it as Unclassified.
  track?: string | null;
};

type JobsOverviewCounts = {
  queued: number;
  running: number;
  succeeded: number;
  failed: number;
  canceled: number;
  total: number;
};

type JobsOverviewSnapshot = {
  jobs: JobRow[];
  counts: JobsOverviewCounts;
  selected_counts: JobsOverviewCounts;
  generated_at_ms: number;
  preview_view: JobsPrimaryView;
  selected_track: string | null;
  running_preview_limit: number;
  queued_preview_limit: number;
  attention_preview_limit: number;
  history_preview_limit: number;
};

type JobBatchHealthSummary = {
  batch_id: string;
  total_jobs: number;
  canonical_targets: number;
  queued_jobs: number;
  running_jobs: number;
  succeeded_jobs: number;
  failed_jobs: number;
  canceled_jobs: number;
  blocked_jobs: number;
  unknown_jobs: number;
  retryable_targets: number;
  active_targets: number;
  succeeded_targets: number;
  unresolved_targets: number;
  missing_title_jobs: number;
  no_output_jobs: number;
  retried_jobs: number;
  unretried_failed_jobs: number;
};

type JobAttemptInspectionRow = {
  job: JobRow;
  canonical_key: string;
  source_title: string | null;
  source_url: string | null;
  video_id: string | null;
  source_path: string | null;
  filename: string | null;
  output_path: string | null;
  output_dir: string | null;
  bundle_membership: string | null;
  is_current_attempt: boolean;
  current_attempt_job_id: string;
  lineage_kind: string;
  status_label: string;
  can_delete: boolean;
  can_retry: boolean;
  blocked_by_youtube_auth: boolean;
  has_output: boolean;
};

type JobBatchDetail = {
  health: JobBatchHealthSummary;
  attempts: JobAttemptInspectionRow[];
};

type JobDetail = {
  selected_job_id: string;
  batch_id: string | null;
  current_attempt_job_id: string;
  attempts: JobAttemptInspectionRow[];
};

type JobTitleBackfillSummary = {
  scanned_jobs: number;
  updated_jobs: number;
  missing_titles: number;
};

type JobExportPayload = {
  format: string;
  item_count: number;
  content: string;
};

type LibraryItem = {
  id: string;
  title: string;
  source_uri: string;
  media_path: string;
};

const SINGLE_VIDEO_SUMMARY_ID = "__single_videos_no_subscription__";
const SINGLE_VIDEO_SUMMARY_LABEL = "YouTube singles";
const JOB_GROUP_RENDER_STEP = 30;
const JOB_GROUP_PREVIEW_RENDER_STEP = 50;

type YoutubeSubscriptionRow = {
  id: string;
  title: string;
  source_url: string;
  folder_map: string;
  output_dir_override: string | null;
};

type InstagramSubscriptionRow = {
  id: string;
  title: string;
  source_url: string;
  folder_map: string;
  output_dir_override: string | null;
};

// WP-0261 / WP-0256: a live per-channel/subscription download roll-up. Operator: "if something
// is downloading in Video Archiver / Instagram / Localization, i expect to see a job for that
// channel/subscription in Jobs, per subscription/channel, in real time." We reuse the origin
// label archiverRuntime already derives ("Channel · aespa", "Playlist · aespa", "Instagram · …",
// "Direct download", "Image batch") as the grouping key, and count each download job's live state so
// the queue answers, per channel, what is checking / downloading now / queued / done — off the
// same existing poll (no new timers). Counts are over the loaded queue (same rows as the table
// below), per [VV-SOT-003]; the label says so.
type ChannelDownloadSummary = {
  summaryId: string;
  label: string;
  sourceUrl: string | null;
  checking: number;
  running: number;
  queued: number;
  succeeded: number;
  failed: number;
  total: number;
  jobCount: number;
  runningProgressSum: number;
  runningProgressCount: number;
  isSingle: boolean;
  currentTitle: string | null;
  currentProgress: number | null;
};

type SubscriptionDownloadActivityRow = {
  subscription_id: string;
  running: number;
  queued: number;
  succeeded: number;
  failed: number;
  current_title?: string | null;
  current_progress?: number | null;
};

type JobQueueControlState = {
  paused: boolean;
};

type JobRuntimeSettings = {
  youtube_single: number;
  youtube_recurring: number;
  instagram_single: number;
  instagram_recurring: number;
  tiktok_single: number;
  tiktok_recurring: number;
  other_video: number;
  image_archive: number;
  localization: number;
};

type JobTrackStatusTotals = {
  queued: number;
  running: number;
  succeeded: number;
  failed: number;
  canceled: number;
  total: number;
};

type JobTrackRuntimeRow = JobTrackStatusTotals & {
  track: CanonicalJobTrack;
  configured_budget: number;
  effective_budget: number;
  paused: boolean;
  hold_reason: string | null;
};

type YoutubeGateState = {
  state: "ready" | "waiting" | "held" | string;
  next_eligible_at_ms: number | null;
  hold_reason: string | null;
};

// Provisional WP-0270 desktop contract shared with the Tauri layer:
// jobs_track_runtime_get -> this read-only snapshot. Persistent budget writes are owned by Options.
// `unclassified` is a canonical legacy count, never a budgeted runnable track.
type JobsTrackRuntimeSnapshot = {
  tracks: JobTrackRuntimeRow[];
  unclassified: JobTrackStatusTotals;
  youtube_gate: YoutubeGateState;
};

type JobCleanupOutputTarget = {
  path: string;
  source_job_ids: string[];
};

type JobCleanupPreview = {
  terminal_job_count: number;
  log_file_count: number;
  artifact_dir_count: number;
  cache_entry_count: number;
  managed_output_dirs: JobCleanupOutputTarget[];
  external_output_dirs: JobCleanupOutputTarget[];
};

type JobCleanupOptions = {
  remove_managed_output_dirs: boolean;
  remove_external_output_dirs: boolean;
};

type JobCleanupFailure = {
  scope: string;
  path: string;
  message: string;
};

type JobCleanupSummary = {
  removed_jobs: number;
  kept_jobs_due_to_failures: number;
  removed_log_files: number;
  removed_artifact_dirs: number;
  removed_managed_output_dirs: number;
  removed_external_output_dirs: number;
  skipped_managed_output_dirs: number;
  skipped_external_output_dirs: number;
  removed_cache_entries: number;
  failed_paths: JobCleanupFailure[];
};

type ClearTerminalJobsSearchSummary = {
  query: string;
  matched_terminal_jobs: number;
  removed_jobs: number;
};

type RetryBatchFailedSummary = {
  batch_id: string;
  matched_retryable_jobs: number;
  queued_jobs: number;
  reused_active_jobs: number;
  failed_retries: number;
  blocked_jobs: number;
  skipped_succeeded_jobs: number;
  skipped_active_jobs: number;
  unresolved_jobs: number;
  dry_run: boolean;
  first_error: string | null;
};

type BatchOperationMode = "dry_run" | "retry" | "repair";

type BatchOperationSnapshot = {
  request_id: string;
  mode: BatchOperationMode;
  batch_query: string;
  state: "running" | "succeeded" | "failed";
  started_at_ms: number;
  finished_at_ms: number | null;
  summary: RetryBatchFailedSummary | null;
  error: string | null;
};

type FfmpegToolsStatus = {
  installed: boolean;
  ffmpeg_path: string;
  ffprobe_path: string;
  ffmpeg_version: string | null;
  ffprobe_version: string | null;
};

type DiagnosticsInfo = {
  app_data_dir: string;
};

type ItemOutputs = {
  item_id: string;
  source_media_path: string;
  source_media_exists: boolean;
  derived_item_dir: string;
  dub_preview_dir: string;
  source_track_count: number;
  source_usable_segment_count: number;
  latest_source_track_path: string | null;
  translated_en_track_count: number;
  translated_en_usable_segment_count: number;
  translated_en_speaker_count: number;
  latest_translated_en_track_path: string | null;
  mix_dub_preview_v1_wav_path: string;
  mix_dub_preview_v1_wav_exists: boolean;
  mux_dub_preview_v1_mp4_path: string;
  mux_dub_preview_v1_mp4_exists: boolean;
  mux_dub_preview_v1_mkv_path: string;
  mux_dub_preview_v1_mkv_exists: boolean;
  export_pack_v1_zip_path: string;
  export_pack_v1_zip_exists: boolean;
  terminal_state: string;
  terminal_summary: string;
  terminal_detail: string;
  terminal_stage_label: string | null;
  terminal_progress: number | null;
  terminal_error: string | null;
  deliverable_path: string | null;
  deliverable_exists: boolean;
};

type ExportedFile = {
  out_path: string;
  file_bytes: number;
};

const JOBS_SEARCH_LIMIT = 500;
const JOB_CONTEXT_HYDRATION_LIMIT = 25;
const ACTIVE_JOBS_OVERVIEW_POLL_INTERVAL_MS = 5_000;
const IDLE_JOBS_OVERVIEW_POLL_INTERVAL_MS = 10_000;
const ACTIVE_JOB_PROGRESS_POLL_INTERVAL_MS = 750;
const IDLE_JOB_PROGRESS_POLL_INTERVAL_MS = 2_500;
const TRACK_RUNTIME_POLL_INTERVAL_MS = 12_000;
  // WP-0258 (2b): cap how many distinct batches we fetch `jobs_batch_detail` for per render. The
// aggregation is the heaviest read command in the trace; keeping the ceiling small bounds the
// per-render read cost even on a page full of large subscription batches.
const JOBS_BATCH_DETAIL_LIMIT = 12;
// WP-0258 (2b): delimiter used to fold the visible batch-id set into a single stable string key
// for the batch-detail effect dependency. A newline can never appear inside a batch id (UUID /
// prefix), so join/split round-trips cleanly and the effect re-runs only when the set changes.
const BATCH_ID_KEY_SEP = "\n";
type JobsFilter =
  | "all"
  | "failed"
  | "auth_blocked"
  | "retried"
  | "unretried"
  | "succeeded_retry"
  | "missing_title"
  | "no_output";
type JobsPrimaryView = "now" | "attention" | "history";

const NO_SUMMARY_ID = "__unclassified_jobs__";

const CANONICAL_JOB_TRACKS: CanonicalJobTrack[] = [
  "youtube_single",
  "youtube_recurring",
  "instagram_single",
  "instagram_recurring",
  "tiktok_single",
  "tiktok_recurring",
  "other_video",
  "image_archive",
  "localization",
];

function displayJobTrack(track: string | null | undefined): DisplayJobTrack {
  return CANONICAL_JOB_TRACKS.includes(track as CanonicalJobTrack)
    ? (track as CanonicalJobTrack)
    : "unclassified";
}

function jobTrackTabLabel(track: CanonicalJobTrack): string {
  if (track === "youtube_single") return "YouTube singles";
  if (track === "youtube_recurring") return "Subscriptions";
  if (track === "other_video") return "Other videos";
  return jobTrackLabel(track);
}

function countForJobsView(counts: JobsOverviewCounts, view: JobsPrimaryView): number {
  if (view === "now") return counts.running + counts.queued;
  if (view === "attention") return counts.failed;
  return counts.succeeded + counts.failed + counts.canceled;
}

function trackSettingsFromRuntime(snapshot: JobsTrackRuntimeSnapshot): JobRuntimeSettings | null {
  const byTrack = new Map(snapshot.tracks.map((row) => [row.track, row]));
  const values = CANONICAL_JOB_TRACKS.map((track) => byTrack.get(track)?.configured_budget);
  if (values.some((value) => !Number.isInteger(value) || (value ?? 0) < 1)) return null;
  return {
    youtube_single: byTrack.get("youtube_single")!.configured_budget,
    youtube_recurring: byTrack.get("youtube_recurring")!.configured_budget,
    instagram_single: byTrack.get("instagram_single")!.configured_budget,
    instagram_recurring: byTrack.get("instagram_recurring")!.configured_budget,
    tiktok_single: byTrack.get("tiktok_single")!.configured_budget,
    tiktok_recurring: byTrack.get("tiktok_recurring")!.configured_budget,
    other_video: byTrack.get("other_video")!.configured_budget,
    image_archive: byTrack.get("image_archive")!.configured_budget,
    localization: byTrack.get("localization")!.configured_budget,
  };
}

function canonicalTrackRows(snapshot: JobsTrackRuntimeSnapshot | null): JobTrackRuntimeRow[] {
  if (!snapshot) return [];
  const byTrack = new Map(snapshot.tracks.map((row) => [row.track, row]));
  const rows = CANONICAL_JOB_TRACKS.map((track) => byTrack.get(track));
  return rows.every((row): row is JobTrackRuntimeRow => Boolean(row)) ? rows : [];
}

function plainTrackHoldReason(reason: string | null | undefined): string | null {
  if (!reason) return null;
  const normalized = reason.trim().toLowerCase();
  if (normalized.includes("queue") && normalized.includes("pause")) {
    return "The overall queue is paused, so this track cannot start new work.";
  }
  if (normalized.includes("track") && normalized.includes("pause")) {
    return "This track is paused, so it cannot start new work.";
  }
  if (normalized.includes("budget") || normalized.includes("limit") || normalized.includes("capacity")) {
    return "The configured track budget is currently in use.";
  }
  if (normalized.includes("youtube") && (normalized.includes("wait") || normalized.includes("pace"))) {
    return "Waiting for the shared YouTube safe-start window.";
  }
  if (normalized.includes("youtube") || normalized.includes("auth")) {
    return "The shared YouTube gate is temporarily holding new starts.";
  }
  return "The scheduler is temporarily holding new starts for this track.";
}

function plainYoutubeGateState(state: string | null | undefined): string {
  switch ((state ?? "").trim().toLowerCase()) {
    case "ready":
      return "Ready to start";
    case "waiting":
      return "Waiting for safe start window";
    case "held":
      return "New starts temporarily held";
    default:
      return "State unavailable";
  }
}

function groupTrackLabel(jobs: JobRow[]): string {
  const labels = Array.from(new Set(jobs.map((job) => jobTrackLabel(job.track))));
  if (!labels.length) return "Unclassified";
  return labels.length <= 2 ? labels.join(" · ") : `${labels.slice(0, 2).join(" · ")} +${labels.length - 2}`;
}

type JobSummaryGroup = {
  key: string;
  summaryId: string;
  label: string;
  sourceUrl: string | null;
  isSingle: boolean;
  jobs: JobRow[];
  batchIds: string[];
  checking: number;
  running: number;
  queued: number;
  succeeded: number;
  failed: number;
  total: number;
  active: number;
  currentTitle: string | null;
};

function joinPath(dir: string, file: string): string {
  const d = dir.trim().replace(/[\\/]+$/, "");
  const f = file.trim().replace(/^[\\/]+/, "");
  const sep = d.includes("\\") ? "\\" : "/";
  return d && f ? `${d}${sep}${f}` : d || f;
}

function formatTs(ms: number | null): string {
  if (!ms) return "-";
  try {
    return new Date(ms).toLocaleString();
  } catch {
    return String(ms);
  }
}

function isActive(status: JobStatus): boolean {
  return status === "queued" || status === "running";
}

function subscriptionKindFromUrl(sourceUrl: string | null): "Playlist" | "Channel" | "Shorts" | "Subscription" {
  const value = (sourceUrl ?? "").toLowerCase();
  if (!value) return "Subscription";
  if (/[?&]list=/.test(value) || /\/playlist\b/.test(value)) return "Playlist";
  if (/\/shorts\b/.test(value) || /\/@[^/]+\/shorts/.test(value)) return "Shorts";
  if (/@/.test(value) || /\/(channel|c|user)\//.test(value)) return "Channel";
  return "Subscription";
}

function resolveJobSubscriptionId(job: JobRow): string | null {
  if (job.job_type !== "download_direct_url" && job.job_type !== "youtube_subscription_refresh_v1") {
    return null;
  }
  const params = safeParseJobParams(job);
  return stringOrNull(params?.subscription_id);
}

function resolveJobSourceSnapshot(job: JobRow): { name: string | null; url: string | null } {
  const params = safeParseJobParams(job);
  return {
    name: stringOrNull(params?.source_display_name),
    url: stringOrNull(params?.source_page_url),
  };
}

function resolveJobSummaryId(job: JobRow): string | null {
  const subscriptionId = resolveJobSubscriptionId(job);
  if (subscriptionId) return subscriptionId;
  // A pasted URL is not automatically a YouTube single. Instagram and other
  // direct providers remain visible through their persisted product tracks.
  if (job.job_type === "download_direct_url" && displayJobTrack(job.track) === "youtube_single") {
    return SINGLE_VIDEO_SUMMARY_ID;
  }
  return null;
}

function resolveSubscriptionLabel(
  subscriptionId: string,
  youtubeSubscriptionsById: Record<string, YoutubeSubscriptionRow>,
): string {
  const sub = youtubeSubscriptionsById[subscriptionId];
  if (!sub) return `Subscription ${subscriptionId.slice(0, 8)}`;
  const kind = subscriptionKindFromUrl(sub.source_url);
  return `${kind} · ${sub.title || "Untitled"}`;
}

function resolveSummaryLabel(
  summaryId: string,
  youtubeSubscriptionsById: Record<string, YoutubeSubscriptionRow>,
): string {
  if (summaryId === NO_SUMMARY_ID) return "Other jobs";
  if (summaryId === SINGLE_VIDEO_SUMMARY_ID) return SINGLE_VIDEO_SUMMARY_LABEL;
  return resolveSubscriptionLabel(summaryId, youtubeSubscriptionsById);
}

function isRetryable(status: JobStatus): boolean {
  return status === "failed" || status === "canceled";
}

function isIndividuallyDeletable(status: JobStatus): boolean {
  return status === "failed" || status === "canceled";
}

function isAuthBlockedJob(job: JobRow): boolean {
  const error = (job.error ?? "").toLowerCase();
  return (
    error.includes("youtube auth is blocked") ||
    error.includes("sign in to confirm") ||
    error.includes("not a bot") ||
    error.includes("youtube rejected") ||
    error.includes("saved youtube cookies")
  );
}

// WP-0264: the WP-0257 (#6) job-error headline classifier is superseded by the shared
// `classifyFailure` in ../lib/failureStates, so a failed Jobs row and a failed subscription
// telegraph the same plain STATE + required action from one rule set (order matters; HTTP
// status code is decisive). The raw error stays one "Show technical details" expander away.

function isTransientDatabaseLock(error: unknown): boolean {
  return String(error).includes("database is locked");
}

// WP-0258 (2b): treat two job snapshots as equal when every row's mutable fields match, so a poll
// that returns an unchanged list keeps the previous React state identity. That stops the whole
// downstream cascade (grouping, batch-detail fetch, context hydration) from re-running on every
// poll when nothing actually changed — the common case for a large, mostly-idle queue. We compare
// only the fields the UI renders/keys on (status/progress/timestamps/error/lineage/title), which
// are exactly the fields that change as a job advances.
function jobRowsEqual(a: JobRow[], b: JobRow[]): boolean {
  if (a === b) return true;
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i += 1) {
    const x = a[i];
    const y = b[i];
    if (
      x.id !== y.id ||
      x.status !== y.status ||
      x.progress !== y.progress ||
      x.error !== y.error ||
      x.started_at_ms !== y.started_at_ms ||
      x.finished_at_ms !== y.finished_at_ms ||
      x.target_title !== y.target_title ||
      x.target_title_provenance !== y.target_title_provenance ||
      x.target_title_problem !== y.target_title_problem ||
      x.retry_of_job_id !== y.retry_of_job_id ||
      x.retry_replacement_job_id !== y.retry_replacement_job_id ||
      x.batch_id !== y.batch_id ||
      x.item_id !== y.item_id ||
      x.track !== y.track
    ) {
      return false;
    }
  }
  return true;
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, ms));
}

// Sentinel that separates the plain-language notice from the optional raw
// technical detail. The notice renderer (see NOTICE_DETAIL_SEP usage) splits on
// this and puts the raw detail behind a "Show technical details" expander.
const NOTICE_DETAIL_SEP = "␞";

// Plain, friendly one/two-sentence summary of a retry/repair result. No jargon.
function retrySummaryText(summary: RetryBatchFailedSummary): string {
  const queued = summary.queued_jobs;
  const stillNeedAttention = summary.unresolved_jobs;
  const parts: string[] = [];
  if (queued > 0) {
    parts.push(`Queued ${queued} ${queued === 1 ? "retry" : "retries"}.`);
  } else {
    parts.push("No new retries were queued.");
  }
  if (stillNeedAttention > 0) {
    parts.push(`${stillNeedAttention} ${stillNeedAttention === 1 ? "video" : "videos"} still need attention.`);
  } else if (queued > 0) {
    parts.push("Nothing else needs attention.");
  }
  if (summary.blocked_jobs > 0) {
    parts.push(
      `${summary.blocked_jobs} ${summary.blocked_jobs === 1 ? "video is" : "videos are"} waiting on YouTube sign-in.`,
    );
  }
  return parts.join(" ");
}

// Raw, exact breakdown for operators who want the numbers. Kept behind an
// expander in the notice UI; not shown by default.
function retrySummaryDetail(summary: RetryBatchFailedSummary): string {
  const queuedText = summary.queued_jobs
    ? `queued ${summary.queued_jobs}`
    : "queued 0";
  const reusedText = summary.reused_active_jobs
    ? `reused ${summary.reused_active_jobs} active target${summary.reused_active_jobs === 1 ? "" : "s"}`
    : "reused 0 active targets";
  const blockedText = summary.blocked_jobs ? `blocked ${summary.blocked_jobs}` : "blocked 0";
  const skippedText = summary.skipped_succeeded_jobs
    ? `skipped ${summary.skipped_succeeded_jobs} succeeded target${summary.skipped_succeeded_jobs === 1 ? "" : "s"}`
    : "skipped 0 succeeded";
  const unresolvedText = `unresolved ${summary.unresolved_jobs}`;
  const failedText = summary.failed_retries ? `failed-to-enqueue ${summary.failed_retries}` : "failed-to-enqueue 0";
  const firstErrorText = summary.first_error ? ` First error: ${summary.first_error}` : "";
  return `${queuedText}; ${reusedText}; ${blockedText}; ${skippedText}; ${failedText}; ${unresolvedText}; canonical retryable ${summary.matched_retryable_jobs}.${firstErrorText}`;
}

// Builds a notice string that carries the plain summary plus the raw detail
// separated by NOTICE_DETAIL_SEP so the notice card can offer an expander.
function retrySummaryNotice(summary: RetryBatchFailedSummary, prefix = "", suffix = ""): string {
  const plain = `${prefix}${retrySummaryText(summary)}${suffix}`.trim();
  return `${plain}${NOTICE_DETAIL_SEP}${retrySummaryDetail(summary)}`;
}

function copyText(value: string | null | undefined): Promise<boolean> {
  const text = (value ?? "").trim();
  if (!text) return Promise.resolve(false);
  return navigator.clipboard
    ?.writeText(text)
    .then(() => true)
    .catch(() => false) ?? Promise.resolve(false);
}

function summarizeGroupStatusFromCounts(
  group: Pick<JobSummaryGroup, "running" | "queued" | "failed" | "succeeded" | "total">,
): "running" | "queued" | "failed" | "succeeded" {
  if (group.running > 0) return "running";
  if (group.queued > 0) return "queued";
  if (group.failed > 0) return "failed";
  if (group.succeeded > 0 || group.total > 0) return "succeeded";
  return "queued";
}

function summarizeGroupFailure(jobs: JobRow[]) {
  const states = jobs
    .filter((job) => job.status === "failed" && Boolean(job.error?.trim()))
    .map((job) => classifyFailure(job.error));
  if (!states.length) return null;
  const counts = new Map<string, { state: (typeof states)[number]; count: number }>();
  for (const state of states) {
    const current = counts.get(state.kind);
    counts.set(state.kind, { state, count: (current?.count ?? 0) + 1 });
  }
  const ranked = Array.from(counts.values()).sort((a, b) => b.count - a.count);
  return {
    ...ranked[0].state,
    label:
      ranked.length === 1
        ? ranked[0].state.label
        : `${ranked[0].state.label} + ${ranked.length - 1} other reason${ranked.length === 2 ? "" : "s"}`,
  };
}

function jobDisplayLabel(job: JobRow, contextLabel: string | null | undefined): string {
  const title = (job.target_title ?? "").trim();
  const context = (contextLabel ?? "").trim();
  if (title) return title;
  return context || jobTrackLabel(job.track);
}

function summarizeGroupProgress(jobs: JobRow[]): number {
  if (!jobs.length) return 0;
  const total = jobs.reduce((sum, job) => sum + (Number.isFinite(job.progress) ? job.progress : 0), 0);
  return Math.max(0, Math.min(1, total / jobs.length));
}

function summarizeGroupActivity(
  group: Pick<JobSummaryGroup, "checking" | "running" | "queued" | "succeeded" | "failed" | "total">,
): string {
  const parts: string[] = [];
  if (group.checking > 0) parts.push(`${group.checking} checking`);
  if (group.running > 0) parts.push(`${group.running} downloading`);
  if (group.queued > 0) parts.push(`${group.queued} queued`);
  if (group.succeeded > 0) parts.push(`${group.succeeded} done`);
  if (group.failed > 0) parts.push(`${group.failed} failed`);
  if (!parts.length) {
    if (group.total > 0) return `${group.total} video${group.total === 1 ? "" : "s"} total`;
    return "No videos loaded yet";
  }
  if (group.total > 0 && !parts.some((part) => part.includes("total"))) {
    parts.push(`${group.total} total`);
  }
  return parts.join(" • ");
}

function summarizeBatchTargetProgress(health: JobBatchHealthSummary): number {
  if (health.canonical_targets <= 0) return 0;
  return Math.max(0, Math.min(1, health.succeeded_targets / health.canonical_targets));
}

function batchTargetHealthText(health: JobBatchHealthSummary): string {
  return `${health.canonical_targets} videos: ${health.succeeded_targets} downloaded / ${health.active_targets} queued or running / ${health.unresolved_targets} unresolved`;
}

function batchAttemptHealthText(health: JobBatchHealthSummary): string {
  return `${health.total_jobs} attempts: ${health.succeeded_jobs} succeeded / ${health.failed_jobs} failed / ${health.canceled_jobs} canceled / ${health.blocked_jobs} auth-blocked`;
}

function summarizeGroupType(jobs: JobRow[]): string {
  const unique = Array.from(new Set(jobs.map((job) => job.job_type)));
  if (!unique.length) return "-";
  if (unique.length === 1) return `${unique[0]} batch`;
  return "mixed batch";
}

function renderJobProgress(job: JobRow, outputs: ItemOutputs | null) {
  const pct = Math.round((job.progress ?? 0) * 100);
  const stage = outputs?.terminal_stage_label?.trim() || "";
  const summary = outputs?.terminal_summary?.trim() || "";
  const detail = outputs?.terminal_detail?.trim() || "";
  const lines = [stage, summary]
    .filter(Boolean)
    .filter((line, index, all) => all.indexOf(line) === index);
  return (
    <div style={{ minWidth: 150 }}>
      {/* WP-0256: visual progress bar instead of bare "0%". */}
      <div className="job-bar">
        <div className={`job-bar-fill job-bar-${job.status}`} style={{ width: `${pct}%` }} />
      </div>
      <div style={{ fontWeight: 600 }}>{pct}%</div>
      {lines.length ? (
        <div style={{ color: "#4b5563", fontSize: 12, lineHeight: 1.3 }}>
          {lines.join(" | ")}
        </div>
      ) : null}
      {detail && job.status !== "succeeded" ? (
        <div style={{ color: "#4b5563", fontSize: 12, lineHeight: 1.3, wordBreak: "break-word" }}>
          {detail}
        </div>
      ) : null}
    </div>
  );
}

function summarizeCreatedTs(jobs: JobRow[]): number | null {
  if (!jobs.length) return null;
  return jobs.reduce((min, job) => Math.min(min, job.created_at_ms), jobs[0].created_at_ms);
}

function summarizeStartedTs(jobs: JobRow[]): number | null {
  const values = jobs
    .map((job) => job.started_at_ms)
    .filter((value): value is number => value !== null);
  if (!values.length) return null;
  return Math.min(...values);
}

function summarizeFinishedTs(jobs: JobRow[]): number | null {
  if (!jobs.length) return null;
  if (jobs.some((job) => !job.finished_at_ms)) return null;
  return jobs.reduce((max, job) => Math.max(max, job.finished_at_ms ?? 0), jobs[0].finished_at_ms ?? 0);
}

function parseExternalToolMissing(error: string | null): string | null {
  if (!error) return null;
  const prefix = "external tool missing:";
  const idx = error.toLowerCase().indexOf(prefix);
  if (idx < 0) return null;
  const tool = error.slice(idx + prefix.length).trim();
  return tool ? tool.split(/\s+/)[0] : null;
}

export function JobsPage({ visible = true }: { visible?: boolean }) {
  // WP-0256: keep polling and rendering tied to page visibility, but do not pause jobs view
  // while the browser window is only blurred. Subscriptions/jobs keep arriving; a visible Jobs page
  // should reflect that without forcing users to click through focus.
  const pageActive = usePageActivity(visible);
  const shouldPoll = pageActive || visible;
  const [jobs, setJobs] = useState<JobRow[]>([]);
  const [overviewCounts, setOverviewCounts] = useState<JobsOverviewCounts>({
    queued: 0,
    running: 0,
    succeeded: 0,
    failed: 0,
    canceled: 0,
    total: 0,
  });
  const [selectedOverviewCounts, setSelectedOverviewCounts] = useState<JobsOverviewCounts>({
    queued: 0,
    running: 0,
    succeeded: 0,
    failed: 0,
    canceled: 0,
    total: 0,
  });
  const [jobsLoaded, setJobsLoaded] = useState(false);
  const [primaryView, setPrimaryView] = useState<JobsPrimaryView>("now");
  const [jobItemsById, setJobItemsById] = useState<Record<string, LibraryItem>>({});
  const [itemOutputsById, setItemOutputsById] = useState<Record<string, ItemOutputs>>({});
  const [youtubeSubscriptionsById, setYoutubeSubscriptionsById] = useState<
    Record<string, YoutubeSubscriptionRow>
  >({});
  const [instagramSubscriptionsById, setInstagramSubscriptionsById] = useState<
    Record<string, InstagramSubscriptionRow>
  >({});
  const [expandedGroups, setExpandedGroups] = useState<Record<string, boolean>>({});
  const [groupRenderLimits, setGroupRenderLimits] = useState<Record<string, number>>({});
  const [groupPreviewRenderLimit, setGroupPreviewRenderLimit] = useState(
    JOB_GROUP_PREVIEW_RENDER_STEP,
  );
  const [appDataDir, setAppDataDir] = useState<string>("");
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [batchOperationsById, setBatchOperationsById] = useState<
    Record<string, BatchOperationSnapshot>
  >({});
  const [busy, setBusy] = useState(false);
  const [dummySeconds, setDummySeconds] = useState(10);
  const [queuePaused, setQueuePaused] = useState(false);
  const [trackRuntime, setTrackRuntime] = useState<JobsTrackRuntimeSnapshot | null>(null);
  const [trackRuntimeState, setTrackRuntimeState] = useState<"loading" | "ready" | "stale" | "error">("loading");
  const [jobSearchQuery, setJobSearchQuery] = useState("");
  const [jobsFilter, setJobsFilter] = useState<JobsFilter>("all");
  const [selectedTrack, setSelectedTrack] = useState<DisplayJobTrack | "all">("all");
  const [batchDetailsById, setBatchDetailsById] = useState<Record<string, JobBatchDetail>>({});
  const [selectedJobDetail, setSelectedJobDetail] = useState<JobDetail | null>(null);
  // WP-0261 / WP-0256: clicking a subscription summary chip filters the jobs list to that
  // subscription so the operator sees all in-flight and recent entries for one line at a time.
  const [selectedSubscriptionId, setSelectedSubscriptionId] = useState<string | null>(null);
  const [subscriptionDownloadActivityById, setSubscriptionDownloadActivityById] = useState<
    Record<string, SubscriptionDownloadActivityRow>
  >({});
  const [activeRefreshSubscriptionIds, setActiveRefreshSubscriptionIds] = useState<Set<string>>(new Set());
  const jobsSnapshotGenerationRef = useRef(0);
  const jobsProjectionGenerationRef = useRef({ lookups: 0, download: 0, active: 0 });
  const [jobsProjectionState, setJobsProjectionState] = useState<
    Record<"lookups" | "download" | "active", "loading" | "ready" | "stale" | "error">
  >({ lookups: "loading", download: "loading", active: "loading" });
  const markJobsProjectionFailure = useCallback((key: "lookups" | "download" | "active") => {
    setJobsProjectionState((current) => ({
      ...current,
      [key]: current[key] === "ready" || current[key] === "stale" ? "stale" : "error",
    }));
  }, []);

  async function handlePathOpenFailure(path: string, error: unknown, actionLabel: string) {
    const copied = await copyPathToClipboard(path);
    const suffix = copied ? " Path copied to clipboard." : "";
    setError(`${actionLabel} failed: ${String(error)}.${suffix}`);
  }

  const refreshJobsSnapshot = useCallback(async () => {
    const generation = ++jobsSnapshotGenerationRef.current;
    const requestId = `jobs-${generation}-${Date.now()}`;
    const started = performance.now();
    void diagnosticsTrace("frontend_request_started", { request_id: requestId, span_id: requestId, pane: "jobs" });
    const query = jobSearchQuery.trim();
    const overviewRequest = invoke<JobsOverviewSnapshot>("jobs_overview", {
      view: primaryView,
      track: selectedTrack,
      requestId,
      spanId: requestId,
    });
    if (!query) {
      const overview = await overviewRequest;
      void diagnosticsTrace("frontend_receive", { request_id: requestId, span_id: requestId, pane: "jobs", elapsed_ms: Math.round(performance.now() - started) });
      if (generation !== jobsSnapshotGenerationRef.current) {
        void diagnosticsTrace("frontend_request_stale", { request_id: requestId, span_id: requestId, pane: "jobs" }, "warn");
        return;
      }
      setOverviewCounts(overview.counts);
      setSelectedOverviewCounts(overview.selected_counts);
      setJobs((current) => (jobRowsEqual(current, overview.jobs) ? current : overview.jobs));
      setJobsLoaded(true);
      setError((current) => (current?.includes("database is locked") ? null : current));
      requestAnimationFrame(() => void diagnosticsTrace("frontend_render_commit", { request_id: requestId, span_id: requestId, pane: "jobs", elapsed_ms: Math.round(performance.now() - started) }));
      return;
    }
    const [overview, next] = await Promise.all([
      overviewRequest,
      invoke<JobRow[]>("jobs_search", { query, limit: JOBS_SEARCH_LIMIT, track: selectedTrack }),
    ]);
    void diagnosticsTrace("frontend_receive", { request_id: requestId, span_id: requestId, pane: "jobs", elapsed_ms: Math.round(performance.now() - started) });
    if (generation !== jobsSnapshotGenerationRef.current) {
      void diagnosticsTrace("frontend_request_stale", { request_id: requestId, span_id: requestId, pane: "jobs" }, "warn");
      return;
    }
    // Search is only a bounded preview. Continue loading canonical overview
    // counts in parallel so the status strip never becomes a rendered-subset count.
    setOverviewCounts(overview.counts);
    setSelectedOverviewCounts(overview.selected_counts);
    // WP-0258 (2b): keep the previous array identity when the poll returned no material change so
    // downstream effects (grouping, batch-detail fetch, context hydration) don't re-run needlessly.
    setJobs((current) => (jobRowsEqual(current, next) ? current : next));
    setJobsLoaded(true);
    setError((current) => (current?.includes("database is locked") ? null : current));
    requestAnimationFrame(() => void diagnosticsTrace("frontend_render_commit", { request_id: requestId, span_id: requestId, pane: "jobs", elapsed_ms: Math.round(performance.now() - started) }));
  }, [jobSearchQuery, primaryView, selectedTrack]);

  const refreshTrackRuntime = useCallback(async function refreshTrackRuntime() {
    try {
      const runtime = await invoke<JobsTrackRuntimeSnapshot>("jobs_track_runtime_get");
      if (!trackSettingsFromRuntime(runtime) || canonicalTrackRows(runtime).length !== CANONICAL_JOB_TRACKS.length) {
        throw new Error("The canonical track runtime response was incomplete.");
      }
      setTrackRuntime(runtime);
      setTrackRuntimeState("ready");
    } catch (err) {
      console.warn("jobs_track_runtime_get failed", err);
      // Preserve the last verified snapshot but make its age/truth boundary visible.
      setTrackRuntimeState((current) => (current === "ready" || current === "stale" ? "stale" : "error"));
    }
  }, []);

  const refreshQueueControls = useCallback(async function refreshQueueControls() {
    const [control] = await Promise.all([
      invoke<JobQueueControlState>("jobs_queue_control_get").catch((err) => {
        console.warn("jobs_queue_control_get failed", err);
        return null;
      }),
      refreshTrackRuntime(),
    ]);
    if (control) setQueuePaused(control.paused);
  }, [refreshTrackRuntime]);

  const refreshSubscriptionLookups = useCallback(async () => {
    const generation = ++jobsProjectionGenerationRef.current.lookups;
    const [youtubeSubscriptions, instagramSubscriptions] = await Promise.all([
      invoke<YoutubeSubscriptionRow[]>("youtube_subscriptions_list").catch(() => null),
      invoke<InstagramSubscriptionRow[]>("instagram_subscriptions_list").catch(() => null),
    ]);
    if (generation !== jobsProjectionGenerationRef.current.lookups) return;
    if (youtubeSubscriptions === null || instagramSubscriptions === null) {
      markJobsProjectionFailure("lookups");
      return;
    }
    setYoutubeSubscriptionsById(
      Object.fromEntries(youtubeSubscriptions.map((subscription) => [subscription.id, subscription])),
    );
    setInstagramSubscriptionsById(
      Object.fromEntries(instagramSubscriptions.map((subscription) => [subscription.id, subscription])),
    );
    setJobsProjectionState((current) => ({ ...current, lookups: "ready" }));
  }, [markJobsProjectionFailure]);

  const refreshSubscriptionDownloadActivity = useCallback(async () => {
    const generation = ++jobsProjectionGenerationRef.current.download;
    const requestId = `jobs-download-activity-${Date.now()}`;
    const started = performance.now();
    void diagnosticsTrace("frontend_request_started", { request_id: requestId, span_id: requestId, pane: "jobs_subscription_download_activity" });
    const rows = await invoke<SubscriptionDownloadActivityRow[]>(
      "subscription_download_activity",
      { requestId, spanId: requestId },
    ).catch(() => null);
    void diagnosticsTrace("frontend_receive", { request_id: requestId, span_id: requestId, pane: "jobs_subscription_download_activity", elapsed_ms: Math.round(performance.now() - started) });
    if (generation !== jobsProjectionGenerationRef.current.download) return;
    if (rows === null) {
      markJobsProjectionFailure("download");
      return;
    }
    const next: Record<string, SubscriptionDownloadActivityRow> = {};
    for (const row of rows) {
      if (!row || typeof row.subscription_id !== "string") continue;
      next[row.subscription_id] = {
        subscription_id: row.subscription_id,
        running: Number(row.running) || 0,
        queued: Number(row.queued) || 0,
        succeeded: Number(row.succeeded) || 0,
        failed: Number(row.failed) || 0,
        current_title: row.current_title ? String(row.current_title) : null,
        current_progress: Number.isFinite(row.current_progress) ? Number(row.current_progress) : null,
      };
    }
    setSubscriptionDownloadActivityById(next);
    setJobsProjectionState((current) => ({ ...current, download: "ready" }));
    requestAnimationFrame(() => void diagnosticsTrace("frontend_render_commit", { request_id: requestId, span_id: requestId, pane: "jobs_subscription_download_activity" }));
  }, [markJobsProjectionFailure]);

  const refreshActiveSubscriptionRefreshes = useCallback(async () => {
    const generation = ++jobsProjectionGenerationRef.current.active;
    const ids = await invoke<string[]>("youtube_subscriptions_active_refresh_ids").catch(() => null);
    if (generation !== jobsProjectionGenerationRef.current.active) return;
    if (ids === null) {
      markJobsProjectionFailure("active");
      return;
    }
    const normalized = ids
      .map((id) => stringOrNull(id))
      .filter((id): id is string => Boolean(id))
      .map((id) => id);
    setActiveRefreshSubscriptionIds(new Set(normalized));
    setJobsProjectionState((current) => ({ ...current, active: "ready" }));
  }, [markJobsProjectionFailure]);

  const refresh = useCallback(async function refresh() {
    try {
      await refreshJobsSnapshot();
    } catch (e) {
      if (!isTransientDatabaseLock(e)) throw e;
      await sleep(1_500);
      await refreshJobsSnapshot();
    }
    await Promise.all([
      refreshQueueControls(),
      refreshSubscriptionLookups(),
      refreshSubscriptionDownloadActivity(),
      refreshActiveSubscriptionRefreshes(),
    ]);
  }, [
    refreshJobsSnapshot,
    refreshActiveSubscriptionRefreshes,
    refreshQueueControls,
    refreshSubscriptionDownloadActivity,
    refreshSubscriptionLookups,
  ]);

  useEffect(() => {
    if (!shouldPoll) {
      jobsSnapshotGenerationRef.current += 1;
      jobsProjectionGenerationRef.current.lookups += 1;
      jobsProjectionGenerationRef.current.download += 1;
      jobsProjectionGenerationRef.current.active += 1;
      return;
    }
    refresh().catch((e) => setError(String(e)));
  }, [shouldPoll, refresh]);

  useEffect(() => {
    invoke<DiagnosticsInfo>("diagnostics_info")
      .then((info) => setAppDataDir(info.app_data_dir ?? ""))
      .catch(() => undefined);
  }, []);

  useEffect(() => {
    if (!shouldPoll) return;
    let cancelled = false;
    const itemIds = Array.from(
      new Set(
        jobs
          .map((job) => job.item_id?.trim())
          .filter((value): value is string => Boolean(value)),
      ),
    ).slice(0, JOB_CONTEXT_HYDRATION_LIMIT);
    if (!itemIds.length) {
      setJobItemsById({});
      return () => {
        cancelled = true;
      };
    }

    // WP-0245: single batched invoke replaces the prior `Promise.all(library_get
    // per item)` fan-out. Previously a 50-job page fired 50 IPC dispatches per
    // poll; the v0.1.50 freeze trace logged 672 slow `library_get` rows.
    invoke<LibraryItem[]>("library_get_many", { itemIds })
      .then((items) => {
        if (cancelled) return;
        const next: Record<string, LibraryItem> = {};
        for (const item of items) {
          next[item.id] = item;
        }
        setJobItemsById(next);
      })
      .catch(() => {
        if (cancelled) return;
        setJobItemsById({});
      });

    return () => {
      cancelled = true;
    };
  }, [jobs, shouldPoll]);

  useEffect(() => {
    if (!shouldPoll) return;
    let cancelled = false;
    const itemIds = Array.from(
      new Set(
        jobs
          .map((job) => job.item_id?.trim())
          .filter((value): value is string => Boolean(value)),
      ),
    ).slice(0, JOB_CONTEXT_HYDRATION_LIMIT);
    if (!itemIds.length) {
      setItemOutputsById({});
      return () => {
        cancelled = true;
      };
    }

    // WP-0245: single batched invoke replaces the prior `Promise.all(item_outputs
    // per item)` fan-out. The v0.1.50 trace logged 227 slow `item_outputs` rows.
    invoke<ItemOutputs[]>("item_outputs_many", { itemIds })
      .then((outputs) => {
        if (cancelled) return;
        const next: Record<string, ItemOutputs> = {};
        for (const output of outputs) {
          next[output.item_id] = output;
        }
        setItemOutputsById(next);
      })
      .catch(() => {
        if (cancelled) return;
        setItemOutputsById({});
      });

    return () => {
      cancelled = true;
    };
  }, [jobs, shouldPoll]);

  const hasActive = overviewCounts.queued + overviewCounts.running > 0 || jobs.some((job) => isActive(job.status));
  const terminalShownCount = useMemo(
    () => jobs.filter((job) => isIndividuallyDeletable(job.status)).length,
    [jobs],
  );

  // WP-0256: derive each job's source context (label + origin channel/subscription). Moved above
  // `filteredJobs` so the origin can also drive the live per-channel summary and the channel filter.
  const jobContexts = useMemo(() => {
    const next: Record<string, JobContextSummary> = {};
    for (const job of jobs) {
      const item = job.item_id ? jobItemsById[job.item_id] : undefined;
      next[job.id] = buildJobContextSummary(job, {
        item,
        itemOutputs: item ? itemOutputsById[item.id] : null,
        youtubeSubscriptionsById,
        instagramSubscriptionsById,
      });
    }
    return next;
  }, [instagramSubscriptionsById, itemOutputsById, jobItemsById, jobs, youtubeSubscriptionsById]);

  const jobSummaryIdByJobId = useMemo(() => {
    const next: Record<string, string | null> = {};
    for (const job of jobs) {
      next[job.id] = resolveJobSummaryId(job);
    }
    return next;
  }, [jobs]);

  // WP-0261 / WP-0256: live per-channel/subscription download roll-up over the loaded queue.
  // Downloads fanned out by a subscription refresh carry `subscription_id` in params, which
  // archiverRuntime turns into an origin like "Channel · aespa" / "Instagram · user" — the same
  // label shared by that subscription's own refresh job. Grouping by origin therefore ties every
  // download back to its channel and lets us count what is checking / downloading now / queued /
  // done, live, without any new command or timer.
  const downloadChannelSummaries = useMemo<ChannelDownloadSummary[]>(() => {
    const bySummaryId = new Map<string, ChannelDownloadSummary>();
    const ensureSummary = (summaryId: string, isSingle: boolean): ChannelDownloadSummary => {
      const existing = bySummaryId.get(summaryId);
      if (existing) return existing;
      const sourceUrl = isSingle ? null : youtubeSubscriptionsById[summaryId]?.source_url ?? null;
      const summary = {
        summaryId,
        label: resolveSummaryLabel(summaryId, youtubeSubscriptionsById),
        sourceUrl,
        checking: 0,
        running: 0,
        queued: 0,
        succeeded: 0,
        failed: 0,
        total: 0,
        jobCount: 0,
        runningProgressSum: 0,
        runningProgressCount: 0,
        isSingle,
        currentTitle: null,
        currentProgress: null,
      };
      bySummaryId.set(summaryId, summary);
      return summary;
    };
    const summariesWithJobs = new Set<string>();

    for (const job of jobs) {
      const summaryId = jobSummaryIdByJobId[job.id];
      if (!summaryId) continue;
      const isDownload = job.job_type === "download_direct_url";
      const isActiveRefresh =
        job.job_type === "youtube_subscription_refresh_v1" &&
        (job.status === "running" || job.status === "queued");
      if (!isDownload && !isActiveRefresh) continue;
      const entry = ensureSummary(summaryId, summaryId === SINGLE_VIDEO_SUMMARY_ID);
      const sourceSnapshot = resolveJobSourceSnapshot(job);
      if (sourceSnapshot.url && !entry.sourceUrl) entry.sourceUrl = sourceSnapshot.url;
      if (jobContexts[job.id]?.origin && sourceSnapshot.name) {
        entry.label = jobContexts[job.id].origin!;
      }
      summariesWithJobs.add(summaryId);
      entry.jobCount += 1;

      // An active subscription refresh means "this channel is being checked right now" — surface it
      // as a checking marker but keep it out of the download totals (it produces no file itself).
      if (isActiveRefresh) {
        entry.checking += 1;
        continue;
      }

      entry.total += 1;
      if (job.status === "running") entry.running += 1;
      else if (job.status === "queued") entry.queued += 1;
      else if (job.status === "succeeded") entry.succeeded += 1;
      else if (job.status === "failed" || job.status === "canceled") entry.failed += 1;
      if (!entry.currentTitle && job.target_title) {
        entry.currentTitle = job.target_title;
      }
      if (job.status === "running" && Number.isFinite(job.progress)) {
        entry.runningProgressSum += job.progress;
        entry.runningProgressCount += 1;
      }
    }

    for (const activity of Object.values(subscriptionDownloadActivityById)) {
      if (!activity || !activity.subscription_id) continue;
      if (summariesWithJobs.has(activity.subscription_id)) continue;
      const entry = ensureSummary(activity.subscription_id, false);
      entry.running += Number(activity.running) || 0;
      entry.queued += Number(activity.queued) || 0;
      entry.succeeded += Number(activity.succeeded) || 0;
      entry.failed += Number(activity.failed) || 0;
      if (!entry.currentTitle && activity.current_title) {
        entry.currentTitle = activity.current_title;
      }
      if (entry.currentProgress == null && Number.isFinite(activity.current_progress)) {
        entry.currentProgress = Number(activity.current_progress);
      }
    }

    for (const subscriptionId of activeRefreshSubscriptionIds) {
      const entry = ensureSummary(subscriptionId, false);
      entry.checking = Math.max(entry.checking, 1);
    }

    for (const summary of bySummaryId.values()) {
      if (summary.currentProgress === null && summary.runningProgressCount > 0) {
        summary.currentProgress = summary.runningProgressSum / summary.runningProgressCount;
      }
    }

    return Array.from(bySummaryId.values()).sort((a, b) => {
      // Channels with live work (checking / downloading / queued) float to the top so the operator
      // sees what is moving right now first; then most-active, then largest, then stable by name.
      const aActive = a.checking + a.running + a.queued;
      const bActive = b.checking + b.running + b.queued;
      if (aActive !== bActive) return bActive - aActive;
      if (a.running !== b.running) return b.running - a.running;
      if (a.queued !== b.queued) return b.queued - a.queued;
      if (a.total !== b.total) return b.total - a.total;
      if (a.isSingle !== b.isSingle) return a.isSingle ? 1 : -1;
      return a.label.localeCompare(b.label);
    });
  }, [
    jobs,
    jobContexts,
    jobSummaryIdByJobId,
    youtubeSubscriptionsById,
    subscriptionDownloadActivityById,
    activeRefreshSubscriptionIds,
  ]);

  useEffect(() => {
    if (!selectedSubscriptionId) return;
    const exists = downloadChannelSummaries.some((summary) => summary.summaryId === selectedSubscriptionId);
    if (!exists) {
      setSelectedSubscriptionId(null);
    }
  }, [selectedSubscriptionId, downloadChannelSummaries]);

  const filteredJobs = useMemo(() => {
    const viewJobs = jobSearchQuery.trim()
      ? jobs
      : primaryView === "now"
        ? jobs.filter((job) => job.status === "queued" || job.status === "running")
        : primaryView === "attention"
          ? jobs.filter((job) => job.status === "failed")
          : jobs.filter(
              (job) =>
                job.status === "succeeded" || job.status === "failed" || job.status === "canceled",
            );
    const base =
      jobsFilter === "all"
        ? viewJobs
        : viewJobs.filter((job) => {
            if (jobsFilter === "failed") return job.status === "failed" || job.status === "canceled";
            if (jobsFilter === "auth_blocked") return isAuthBlockedJob(job);
            if (jobsFilter === "retried") return Boolean(job.retry_of_job_id || job.retry_replacement_job_id);
            if (jobsFilter === "unretried") {
              return isRetryable(job.status) && !job.retry_replacement_job_id;
            }
            if (jobsFilter === "succeeded_retry") return job.status === "succeeded" && Boolean(job.retry_of_job_id);
            if (jobsFilter === "missing_title") {
              return job.job_type === "download_direct_url" && !job.target_title;
            }
            if (jobsFilter === "no_output") return job.status === "succeeded" && !job.item_id;
            return true;
          });
    // WP-0261 / WP-0256: when a subscription chip is selected, show only its jobs so the
    // live summary and the queue list stay coherent.
    if (!selectedSubscriptionId) return base;
    return base.filter((job) => (jobSummaryIdByJobId[job.id] ?? null) === selectedSubscriptionId);
  }, [
    jobSearchQuery,
    jobs,
    jobsFilter,
    jobSummaryIdByJobId,
    primaryView,
    selectedSubscriptionId,
  ]);

  const groupedJobs = useMemo(() => {
    const byKey = new Map<string, JobSummaryGroup>();
    const groups: JobSummaryGroup[] = [];

    for (const job of filteredJobs) {
      const summaryId = jobSummaryIdByJobId[job.id] ?? NO_SUMMARY_ID;
      const isSingleVideo = summaryId === SINGLE_VIDEO_SUMMARY_ID;
      const groupKey = isSingleVideo ? `${summaryId}:${job.item_id ?? job.id}` : summaryId;
      let group = byKey.get(groupKey);
      if (!group) {
        const sourceSnapshot = resolveJobSourceSnapshot(job);
        const sourceUrl = summaryId === SINGLE_VIDEO_SUMMARY_ID
          ? null
          : youtubeSubscriptionsById[summaryId]?.source_url ?? sourceSnapshot.url;
        const newGroup: JobSummaryGroup = {
          key: groupKey,
          summaryId,
          label: sourceSnapshot.name && jobContexts[job.id]?.origin
            ? jobContexts[job.id].origin!
            : resolveSummaryLabel(summaryId, youtubeSubscriptionsById),
          sourceUrl,
          isSingle: summaryId === SINGLE_VIDEO_SUMMARY_ID,
          jobs: [],
          batchIds: [],
          checking: 0,
          running: 0,
          queued: 0,
          succeeded: 0,
          failed: 0,
          total: 0,
          active: 0,
          currentTitle: null,
        };
        group = newGroup;
        byKey.set(groupKey, group);
        groups.push(group);
      }

      const isActiveRefresh = job.job_type === "youtube_subscription_refresh_v1";
      if (isActiveRefresh) {
        if (job.status === "running" || job.status === "queued") {
          group.checking += 1;
        }
      } else {
        group.total += 1;
      }

      group.jobs.push(job);
      if (job.batch_id) {
        if (!group.batchIds.includes(job.batch_id)) {
          group.batchIds.push(job.batch_id);
        }
      }

      if (job.status === "running") {
        group.running += 1;
      } else if (job.status === "queued") {
        group.queued += 1;
      } else if (job.status === "failed" || job.status === "canceled") {
        group.failed += 1;
      } else if (job.status === "succeeded") {
        group.succeeded += 1;
      }

      if (isActive(job.status)) {
        group.active += 1;
      }

      if (!group.currentTitle && (job.target_title || jobContextLabelForJob(job))) {
        group.currentTitle = job.target_title || jobContextLabelForJob(job);
      }
    }

    groups.sort((a, b) => {
      const aWork = a.checking + a.running + a.queued;
      const bWork = b.checking + b.running + b.queued;
      if (aWork !== bWork) return bWork - aWork;
      if (a.running !== b.running) return b.running - a.running;
      if (a.queued !== b.queued) return b.queued - a.queued;
      if (a.succeeded !== b.succeeded) return b.succeeded - a.succeeded;
      if (a.failed !== b.failed) return b.failed - a.failed;
      return a.label.localeCompare(b.label);
    });

    return groups;
  }, [filteredJobs, jobSummaryIdByJobId, youtubeSubscriptionsById, jobContexts]);
  const visibleGroupedJobs = useMemo(
    () => groupedJobs.slice(0, groupPreviewRenderLimit),
    [groupPreviewRenderLimit, groupedJobs],
  );

  function jobContextLabelForJob(job: JobRow): string | null {
    const context = jobContexts[job.id];
    return context?.label ? context.label : null;
  }

  useEffect(() => {
    setExpandedGroups((prev) => {
      const next: Record<string, boolean> = {};
      for (const group of groupedJobs) {
        next[group.key] = Object.prototype.hasOwnProperty.call(prev, group.key)
          ? prev[group.key]
          : group.isSingle;
      }
      return next;
    });
  }, [groupedJobs]);

  useEffect(() => {
    setGroupRenderLimits({});
    setGroupPreviewRenderLimit(JOB_GROUP_PREVIEW_RENDER_STEP);
  }, [primaryView, selectedTrack, jobsFilter, jobSearchQuery, selectedSubscriptionId]);

  // WP-0258 v2: canonical batch detail is secondary information. Fetch it only after the operator
  // expands a multi-job group; the current-work overview must not fan out up to twelve heavyweight
  // aggregation commands merely because collapsed rows are visible.
  const visibleBatchIdsKey = useMemo(() => {
    const ids = Array.from(
      new Set(
        visibleGroupedJobs
          .filter(
            (group) =>
              !group.isSingle &&
              expandedGroups[group.key] === true,
          )
          .flatMap((group) => group.batchIds.map((id) => id.trim()))
          .filter((value): value is string => Boolean(value)),
      ),
    )
      .slice(0, JOBS_BATCH_DETAIL_LIMIT)
      .sort();
    return ids.join(BATCH_ID_KEY_SEP);
  }, [expandedGroups, visibleGroupedJobs]);

  useEffect(() => {
    if (!shouldPoll) return;
    let cancelled = false;
    const batchIds = visibleBatchIdsKey ? visibleBatchIdsKey.split(BATCH_ID_KEY_SEP) : [];
    if (!batchIds.length) {
      setBatchDetailsById({});
      return () => {
        cancelled = true;
      };
    }

    Promise.all(
      batchIds.map((batchId) =>
        invoke<JobBatchDetail>("jobs_batch_detail", { batchId, batch_id: batchId })
          .then((detail) => [detail.health.batch_id, detail] as const)
          .catch(() => null),
      ),
    ).then((entries) => {
      if (cancelled) return;
      const next: Record<string, JobBatchDetail> = {};
      for (const entry of entries) {
        if (entry) next[entry[0]] = entry[1];
      }
      setBatchDetailsById(next);
    });

    return () => {
      cancelled = true;
    };
  }, [visibleBatchIdsKey, shouldPoll]);

  const visibleJobIdsKey = useMemo(
    () => jobs.map((job) => job.id).slice(0, 500).sort().join(BATCH_ID_KEY_SEP),
    [jobs],
  );

  const refreshVisibleJobProgress = useCallback(async () => {
    if (!visibleJobIdsKey) return;
    const ids = visibleJobIdsKey.split(BATCH_ID_KEY_SEP);
    const updates = await invoke<JobRow[]>("jobs_progress_many", { jobIds: ids });
    const byId = new Map(updates.map((job) => [job.id, job]));
    setJobs((current) => {
      let changed = false;
      const next = current.map((job) => {
        const update = byId.get(job.id);
        if (!update || jobRowsEqual([job], [update])) return job;
        changed = true;
        return update;
      });
      return changed ? next : current;
    });
  }, [visibleJobIdsKey]);

  usePollingLoop(
    async () => {
      await refreshVisibleJobProgress().catch(() => undefined);
    },
    {
      enabled: shouldPoll && Boolean(visibleJobIdsKey),
      intervalMs: hasActive
        ? ACTIVE_JOB_PROGRESS_POLL_INTERVAL_MS
        : IDLE_JOB_PROGRESS_POLL_INTERVAL_MS,
    },
  );

  usePollingLoop(
    async () => {
      await refreshJobsSnapshot().catch(() => undefined);
    },
    {
      enabled: shouldPoll,
      intervalMs: hasActive
        ? ACTIVE_JOBS_OVERVIEW_POLL_INTERVAL_MS
        : IDLE_JOBS_OVERVIEW_POLL_INTERVAL_MS,
    },
  );

  usePollingLoop(
    async () => {
      await refreshTrackRuntime();
    },
    {
      enabled: shouldPoll,
      intervalMs: TRACK_RUNTIME_POLL_INTERVAL_MS,
    },
  );

  async function enqueueDummy() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await invoke("jobs_enqueue_dummy", { seconds: dummySeconds });
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function applyJobSearch(event?: FormEvent<HTMLFormElement>) {
    event?.preventDefault();
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await refreshJobsSnapshot();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function clearJobSearch() {
    setJobSearchQuery("");
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const requestId = `jobs-clear-search-${Date.now()}`;
      const started = performance.now();
      void diagnosticsTrace("frontend_request_started", { request_id: requestId, span_id: requestId, pane: "jobs_clear_search" });
      const overview = await invoke<JobsOverviewSnapshot>("jobs_overview", {
        view: primaryView,
        track: selectedTrack,
        requestId,
        spanId: requestId,
      });
      void diagnosticsTrace("frontend_receive", { request_id: requestId, span_id: requestId, pane: "jobs_clear_search", elapsed_ms: Math.round(performance.now() - started) });
      setOverviewCounts(overview.counts);
      setSelectedOverviewCounts(overview.selected_counts);
      setJobs((current) => (jobRowsEqual(current, overview.jobs) ? current : overview.jobs));
      setJobsLoaded(true);
      requestAnimationFrame(() => void diagnosticsTrace("frontend_render_commit", { request_id: requestId, span_id: requestId, pane: "jobs_clear_search" }));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function deleteTerminalJobsMatchingSearch() {
    const query = jobSearchQuery.trim();
    if (!query) {
      setError("Enter a Jobs search before deleting matching failed/canceled rows.");
      return;
    }
    const ok = await confirm(
      `Delete failed/canceled job-history rows matching "${query}"? The backend re-runs this search up to ${JOBS_SEARCH_LIMIT} rows. Media files, library items, subscriptions, playlists, and running/queued/succeeded jobs are not touched.`,
      {
        title: "Delete matching failed/canceled jobs",
        kind: "warning",
      },
    );
    if (!ok) return;

    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const summary = await invoke<ClearTerminalJobsSearchSummary>("jobs_delete_terminal_matching_search", {
        query,
        limit: JOBS_SEARCH_LIMIT,
      });
      setNotice(
        `Deleted ${summary.removed_jobs} failed/canceled job${summary.removed_jobs === 1 ? "" : "s"} matching "${summary.query}".`,
      );
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function cancel(jobId: string) {
    const normalized = (jobId ?? "").trim();
    if (!normalized) {
      setError("Cannot cancel job: missing job id.");
      return;
    }
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await invoke("jobs_cancel", { jobId: normalized, job_id: normalized });
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function cancelGroup(group: JobSummaryGroup) {
    const activeIds = group.jobs.filter((job) => isActive(job.status)).map((job) => job.id);
    if (!activeIds.length) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await Promise.all(
        activeIds.map((jobId) =>
          invoke("jobs_cancel", { jobId, job_id: jobId }),
        ),
      );
    setNotice(`Canceled ${activeIds.length} active job${activeIds.length === 1 ? "" : "s"} in ${group.label}.`);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function retry(jobId: string) {
    const normalized = (jobId ?? "").trim();
    if (!normalized) {
      setError("Cannot retry job: missing job id.");
      return;
    }
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const retried = await invoke<JobRow>("jobs_retry", { jobId: normalized, job_id: normalized });
      const reusedActive = jobs.some((job) => job.id === retried.id && isActive(job.status));
      setNotice(
        reusedActive
          ? "Retry already has an active queued/running row for this target; using that row instead of adding a duplicate."
          : queuePaused
          ? "Retried job. Queue is paused; click Resume all to start queued retry work."
          : "Retried job.",
      );
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function runBatchOperation(
    mode: BatchOperationMode,
    batchId: string,
  ): Promise<BatchOperationSnapshot> {
    let operation = await invoke<BatchOperationSnapshot>("jobs_batch_operation_start", {
      mode,
      batchId,
      batch_id: batchId,
    });
    setBatchOperationsById((current) => ({
      ...current,
      [operation.request_id]: operation,
    }));
    while (operation.state === "running") {
      await sleep(750);
      operation = await invoke<BatchOperationSnapshot>("jobs_batch_operation_get", {
        requestId: operation.request_id,
        request_id: operation.request_id,
      });
      setBatchOperationsById((current) => ({
        ...current,
        [operation.request_id]: operation,
      }));
    }
    if (operation.state === "failed") {
      throw new Error(operation.error || `Batch ${mode} operation failed.`);
    }
    if (!operation.summary) {
      throw new Error(`Batch ${mode} operation completed without a result summary.`);
    }
    return operation;
  }

  function batchOperationRunning(batchId: string | null): boolean {
    if (!batchId) return false;
    return Object.values(batchOperationsById).some(
      (operation) => operation.batch_query === batchId && operation.state === "running",
    );
  }

  async function retryGroup(group: JobSummaryGroup) {
    const groupBatchId = group.batchIds.length === 1 ? group.batchIds[0] : null;
    if (groupBatchId && !group.isSingle) {
      setError(null);
      setNotice(null);
      try {
        const dryRunOperation = await runBatchOperation("dry_run", groupBatchId);
        const dryRun = dryRunOperation.summary!;
        if (dryRun.matched_retryable_jobs === 0) {
          setNotice(
            retrySummaryNotice(
              dryRun,
              "Nothing to retry — no videos are waiting. ",
              ` Receipt ${dryRunOperation.request_id}.`,
            ),
          );
          return;
        }
        const ok = await confirm(`Retry the videos that did not finish?\n${retrySummaryText(dryRun)}`, {
          title: "Retry failed batch",
          kind: dryRun.blocked_jobs ? "warning" : "info",
        });
        if (!ok) return;
        const retryOperation = await runBatchOperation("retry", groupBatchId);
        const summary = retryOperation.summary!;
        setNotice(
          retrySummaryNotice(
            summary,
            "",
            `${queuePaused ? " The queue is paused — click Resume all to start the retries." : ""} Receipt ${retryOperation.request_id}.`,
          ),
        );
        await refresh();
      } catch (e) {
        setError(String(e));
      }
      return;
    }

    const retryableIds = group.jobs
      .filter((job) => isRetryable(job.status))
      .map((job) => job.id);
    if (!retryableIds.length) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const activeBefore = new Set(jobs.filter((job) => isActive(job.status)).map((job) => job.id));
      const returnedIds = new Set<string>();
      let queuedCount = 0;
      let reusedCount = 0;
      for (const jobId of retryableIds) {
        const retried = await invoke<JobRow>("jobs_retry", { jobId, job_id: jobId });
        if (activeBefore.has(retried.id) || returnedIds.has(retried.id)) {
          reusedCount += 1;
        } else {
          queuedCount += 1;
        }
        returnedIds.add(retried.id);
      }
      const queuedText = queuedCount
        ? `Queued ${queuedCount} retry${queuedCount === 1 ? "" : "ies"}`
        : "No new retries queued";
      const reusedText = reusedCount
        ? `reused ${reusedCount} already-active target${reusedCount === 1 ? "" : "s"}`
        : "no duplicate active targets found";
      setNotice(
        queuePaused
          ? `${queuedText}; ${reusedText}. Queue is paused; click Resume all to start queued retry work.`
          : `${queuedText}; ${reusedText}. Retries were processed sequentially so active work keeps its queue slot.`,
      );
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function inspectJob(jobId: string) {
    setBusy(true);
    setError(null);
    try {
      const detail = await invoke<JobDetail>("jobs_detail", { jobId, job_id: jobId });
      setSelectedJobDetail(detail);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function repairBatch(batchId: string) {
    const ok = await confirm("Fix this batch? Finished videos are left alone, videos already running are not started twice, and any that failed or were canceled are retried (except ones waiting on YouTube sign-in).", {
      title: "Fix batch",
      kind: "warning",
    });
    if (!ok) return;
    setError(null);
    setNotice(null);
    try {
      const repairOperation = await runBatchOperation("repair", batchId);
      setNotice(
        retrySummaryNotice(
          repairOperation.summary!,
          "",
          ` Receipt ${repairOperation.request_id}.`,
        ),
      );
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  async function backfillBatchTitles(batchId: string) {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const summary = await invoke<JobTitleBackfillSummary>("jobs_backfill_titles_for_batch", {
        batchId,
        batch_id: batchId,
        limit: 500,
      });
      setNotice(
        `Title backfill scanned ${summary.scanned_jobs}, updated ${summary.updated_jobs}, still missing ${summary.missing_titles}.`,
      );
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function exportUnresolvedBatch(batchId: string, format: "csv" | "json" | "urls") {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const payload = await invoke<JobExportPayload>("jobs_export_unresolved_batch", {
        batchId,
        batch_id: batchId,
        format,
      });
      const copied = await copyText(payload.content);
      setNotice(
        copied
          ? `Copied ${payload.item_count} unresolved ${payload.format} item${payload.item_count === 1 ? "" : "s"} to clipboard.`
          : `Prepared ${payload.item_count} unresolved ${payload.format} item${payload.item_count === 1 ? "" : "s"}, but clipboard copy failed.`,
      );
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function deleteTerminalJob(job: JobRow) {
    if (!isIndividuallyDeletable(job.status)) return;
    const ok = await confirm(
      `Delete failed/canceled job ${job.id.slice(0, 8)} from the queue history? Media files, library items, subscriptions, and batch siblings are not touched.`,
      {
        title: "Delete job",
        kind: "warning",
      },
    );
    if (!ok) return;

    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const removed = await invoke<boolean>("jobs_delete_terminal", { jobId: job.id, job_id: job.id });
      setNotice(
        removed
          ? `Deleted job ${job.id.slice(0, 8)} from queue history.`
          : `Job ${job.id.slice(0, 8)} was already gone.`,
      );
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function openLogFile(path: string) {
    setError(null);
    try {
      const target = requireOpenablePath(path, "Log path");
      const opened = await openPathBestEffort(target);
      setNotice(
        opened.method === "shell_open_path"
          ? `Opened log file: ${opened.path}`
          : `Opened log parent folder: ${opened.path}`,
      );
    } catch (e) {
      setError(String(e));
    }
  }

  async function openJobArtifactsDir(jobId: string) {
    if (!appDataDir) return;

    const derivedDir = joinPath(appDataDir, "derived");
    const artifactsDir = joinPath(joinPath(derivedDir, "jobs"), jobId);

    setError(null);
    try {
      const opened = await openPathBestEffort(artifactsDir);
      setNotice(
        opened.method === "shell_open_path"
          ? `Artifacts folder: ${opened.path}`
          : `Artifacts folder revealed in file explorer: ${opened.path}`,
      );
    } catch (e) {
      await handlePathOpenFailure(artifactsDir, e, "Open artifacts");
    }
  }

  async function openItemOutputsDir(itemId: string) {
    if (!appDataDir) return;

    const derivedDir = joinPath(appDataDir, "derived");
    const outputsDir = joinPath(joinPath(derivedDir, "items"), itemId);

    setError(null);
    try {
      const opened = await openPathBestEffort(outputsDir);
      setNotice(
        opened.method === "shell_open_path"
          ? `Outputs folder: ${opened.path}`
          : `Outputs folder revealed in file explorer: ${opened.path}`,
      );
    } catch (e) {
      await handlePathOpenFailure(outputsDir, e, "Open outputs");
    }
  }

  async function openJobContextTarget(job: JobRow) {
    const params = safeParseJobParams(job);
    try {
      if (job.job_type === "download_direct_url" || job.job_type === "download_image_batch") {
        const outputDir = stringOrNull(params?.output_dir);
        if (!outputDir) {
          throw new Error("No explicit target root was recorded for this job.");
        }
        const opened = await openPathBestEffort(outputDir);
        setNotice(
          opened.method === "shell_open_path"
            ? `Opened target root: ${opened.path}`
            : `Opened target root in file explorer: ${opened.path}`,
        );
        return;
      }

      if (job.job_type === "youtube_subscription_refresh_v1") {
        const subscriptionId = stringOrNull(params?.subscription_id);
        if (!subscriptionId) {
          throw new Error("No subscription id was recorded for this job.");
        }
        const path = await invoke<string>("youtube_subscriptions_output_dir", { id: subscriptionId });
        const opened = await openPathBestEffort(path);
        setNotice(
          opened.method === "shell_open_path"
            ? `Opened subscription target: ${opened.path}`
            : `Opened subscription target in file explorer: ${opened.path}`,
        );
        return;
      }

      if (job.job_type === "import_local") {
        const path = stringOrNull(params?.path);
        if (!path) {
          throw new Error("No source path was recorded for this job.");
        }
        const opened = await openPathBestEffort(path);
        setNotice(
          opened.method === "shell_open_path"
            ? `Opened source path: ${opened.path}`
            : `Opened source path in file explorer: ${opened.path}`,
        );
        return;
      }

      throw new Error("This job does not expose a direct target folder.");
    } catch (e) {
      setError(String(e));
    }
  }

  async function openMuxedPreview(itemId: string) {
    setError(null);
    try {
      const outputs = await invoke<ItemOutputs>("item_outputs", { itemId });
      const path = outputs.mux_dub_preview_v1_mkv_exists
        ? outputs.mux_dub_preview_v1_mkv_path
        : outputs.mux_dub_preview_v1_mp4_exists
          ? outputs.mux_dub_preview_v1_mp4_path
          : "";
      if (!path) {
        throw new Error(
          "Muxed preview not found yet. Run the 'mux_dub_preview_v1' job first.",
        );
      }
      const opened = await openPathBestEffort(path);
      setNotice(
        opened.method === "shell_open_path"
          ? `Opened preview: ${opened.path}`
          : `Opened preview folder: ${opened.path}`,
      );
    } catch (e) {
      setError(String(e));
    }
  }

  async function exportMuxedPreview(itemId: string, suggestedStem: string) {
    setError(null);
    try {
      const outputs = await invoke<ItemOutputs>("item_outputs", { itemId });
      if (!outputs.mux_dub_preview_v1_mkv_exists) {
        throw new Error("MKV mux preview not found. Legacy MP4 previews remain playable but are not copied as new exports.");
      }
    } catch (e) {
      setError(String(e));
      return;
    }

    const out = await save({
      title: "Export muxed preview (MKV)",
      defaultPath: `${suggestedStem}.mkv`,
      filters: [{ name: "MKV", extensions: ["mkv"] }],
    });
    if (!out || typeof out !== "string") return;

    setBusy(true);
    setNotice(null);
    try {
      const result = await invoke<ExportedFile>("item_export_mux_preview_mp4", {
        itemId,
        outPath: out,
      });
      setNotice(`Exported preview: ${result.out_path}`);
      try {
        await revealPath(result.out_path);
      } catch {
        // ignore
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function installFfmpegTools() {
    setBusy(true);
    setError(null);
    setNotice("Installing FFmpeg tools. This may take a minute.");
    try {
      await invoke<FfmpegToolsStatus>("tools_ffmpeg_install");
      setNotice("FFmpeg tools installed. Retry the failed job.");
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function setPauseAll(paused: boolean) {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const state = await invoke<JobQueueControlState>("jobs_queue_control_set", { paused });
      setQueuePaused(state.paused);
      setNotice(state.paused ? "Queue paused. Running jobs continue." : "Queue resumed.");
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function cancelAll() {
    const ok = await confirm(
      "Cancel all queued/running jobs? Running tasks may take a short moment to stop.",
      {
        title: "Cancel all jobs",
        kind: "warning",
      },
    );
    if (!ok) return;

    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const updated = await invoke<number>("jobs_cancel_all");
      setNotice(`Canceled ${updated} active job${updated === 1 ? "" : "s"}.`);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function flushCache() {
    try {
      const preview = await invoke<JobCleanupPreview>("jobs_cleanup_preview");
      if (
        preview.terminal_job_count === 0 &&
        preview.log_file_count === 0 &&
        preview.artifact_dir_count === 0 &&
        preview.cache_entry_count === 0 &&
        preview.managed_output_dirs.length === 0 &&
        preview.external_output_dirs.length === 0
      ) {
        setNotice("No terminal jobs, logs, artifacts, cache entries, or output folders need cleanup.");
        return;
      }

      const ok = await confirm(
        `Forget ${preview.terminal_job_count} terminal job${preview.terminal_job_count === 1 ? "" : "s"}, remove ${preview.log_file_count} log file${preview.log_file_count === 1 ? "" : "s"}, ${preview.artifact_dir_count} job artifact folder${preview.artifact_dir_count === 1 ? "" : "s"}, and ${preview.cache_entry_count} cache entr${preview.cache_entry_count === 1 ? "y" : "ies"}? Output folders are handled by separate prompts.`,
        {
          title: "Clean up old jobs and logs",
          kind: "warning",
        },
      );
      if (!ok) return;

      let removeManagedOutputDirs = false;
      if (preview.managed_output_dirs.length > 0) {
        removeManagedOutputDirs = await confirm(
          `Also delete ${preview.managed_output_dirs.length} app-managed output folder${preview.managed_output_dirs.length === 1 ? "" : "s"} created by terminal jobs? Deliverables outside those folders are not touched.`,
          {
            title: "Delete managed output folders",
            kind: "warning",
          },
        );
      }

      let removeExternalOutputDirs = false;
      if (preview.external_output_dirs.length > 0) {
        removeExternalOutputDirs = await confirm(
          `Also delete ${preview.external_output_dirs.length} external/custom output folder${preview.external_output_dirs.length === 1 ? "" : "s"}? These may be outside VoxVulgi-managed paths.`,
          {
            title: "Delete external output folders",
            kind: "warning",
          },
        );
      }

      setBusy(true);
      setError(null);
      setNotice(null);
      try {
        const summary = await invoke<JobCleanupSummary>("jobs_flush_cache", {
          options: {
            remove_managed_output_dirs: removeManagedOutputDirs,
            remove_external_output_dirs: removeExternalOutputDirs,
          } satisfies JobCleanupOptions,
        });
        setNotice(
          `Flushed ${summary.removed_jobs} jobs, kept ${summary.kept_jobs_due_to_failures} job${summary.kept_jobs_due_to_failures === 1 ? "" : "s"} due to cleanup failures, removed ${summary.removed_log_files} log files, ${summary.removed_artifact_dirs} artifact folders, ${summary.removed_managed_output_dirs} managed output folders, ${summary.removed_external_output_dirs} external output folders, and ${summary.removed_cache_entries} cache entries.`,
        );
        if (summary.failed_paths.length > 0) {
          const detail = summary.failed_paths
            .slice(0, 5)
            .map((failure) => `${failure.scope}: ${failure.path} (${failure.message})`)
            .join("\n");
          setError(
            `Cleanup left ${summary.failed_paths.length} path failure${summary.failed_paths.length === 1 ? "" : "s"}.\n${detail}`,
          );
        }
        await refresh();
      } catch (e) {
        setError(String(e));
      } finally {
        setBusy(false);
      }
    } catch (e) {
      setError(String(e));
    }
  }

  const trackRows = canonicalTrackRows(trackRuntime);
  const unclassifiedTrackTotals = trackRuntime?.unclassified ?? null;
  const youtubeGate = trackRuntime?.youtube_gate ?? null;

  function renderJobRow(job: JobRow, nested: boolean) {
    const missingTool = parseExternalToolMissing(job.error);
    const canInstallFfmpegTools =
      missingTool === "ffprobe" || missingTool === "ffmpeg";
    const canRevealMuxedPreview =
      job.status === "succeeded" &&
      job.job_type === "mux_dub_preview_v1" &&
      Boolean(job.item_id);
    const derivedDir = appDataDir ? joinPath(appDataDir, "derived") : "";
    const artifactsDir = derivedDir ? joinPath(joinPath(derivedDir, "jobs"), job.id) : "";
    const outputsDir =
      derivedDir && job.item_id
        ? joinPath(joinPath(derivedDir, "items"), job.item_id)
        : "";
    const canOpenArtifacts = Boolean(artifactsDir) && job.status !== "queued";
    const canOpenOutputs = Boolean(outputsDir);
  const jobContext = jobContexts[job.id];
  const itemOutputs = job.item_id ? itemOutputsById[job.item_id] : null;
  const canOpenContextTarget = !job.item_id && Boolean(jobContext?.target_action_label);
  const waitingForResume = queuePaused && job.status === "queued";
  const displayLabel = jobDisplayLabel(job, jobContext?.label);

    return (
      <tr key={job.id} className={nested ? "batch-child-row" : undefined}>
        <td>
          {nested ? "\u251C\u2500 " : ""}
          {(() => {
            // WP-0264: for a failed/canceled row, telegraph a plain STATE chip + required
            // action (shared classifier) instead of raw jargon, and keep the raw error one
            // "Show technical details" expander away. Non-terminal rows / empty errors: no chip.
            const isTerminalFailure = job.status === "failed" || job.status === "canceled";
            const rawError = (job.error ?? "").trim();
            if (!isTerminalFailure || !rawError) {
              return rawError ? `${job.status}: ${rawError}` : job.status;
            }
            const state = classifyFailure(rawError);
            return (
              <div>
                <div style={{ fontWeight: 700, color: toneStyle(state.tone).color }}>
                  {job.status === "failed" ? "Failed" : "Canceled"} &mdash; {state.label}
                </div>
                {state.requirement ? (
                  <div style={{ color: "#4b5563", fontSize: 12, lineHeight: 1.3, marginTop: 2 }}>
                    {state.requirement}
                  </div>
                ) : null}
                <details style={{ marginTop: 4 }}>
                  <summary style={{ cursor: "pointer", color: "#4b5563", fontSize: 12 }}>
                    Show technical details
                  </summary>
                  <div
                    style={{
                      color: "#4b5563",
                      fontSize: 12,
                      lineHeight: 1.3,
                      marginTop: 4,
                      overflowWrap: "anywhere",
                      whiteSpace: "pre-wrap",
                    }}
                  >
                    {rawError}
                  </div>
                </details>
              </div>
            );
          })()}
          {waitingForResume ? (
            <div style={{ color: "#7c2d12", fontSize: 12, lineHeight: 1.3 }}>
              Waiting for Resume all
            </div>
          ) : null}
        </td>
        <td style={{ minWidth: 260, maxWidth: 420 }}>
          <div style={{ fontWeight: 600, overflowWrap: "anywhere", lineHeight: 1.3 }}>
            {displayLabel}
          </div>
          <div className="jobs-row-id" title={job.id}>
            Job <code>{job.id.slice(0, 8)}</code>
            {job.item_id ? <> · Item <code>{job.item_id.slice(0, 8)}</code></> : null}
          </div>
          {titleProvenanceLabel(job.target_title_provenance) ? (
            <div style={{ color: "#6b7a8a", fontSize: 11 }}>
              {titleProvenanceLabel(job.target_title_provenance)}
              {job.target_title_problem ? ` · ${job.target_title_problem.replace(/_/g, " ")}` : ""}
            </div>
          ) : null}
          {jobContext?.detail ? (
            <div style={{ color: "#4b5563", fontSize: 12, lineHeight: 1.3, wordBreak: "break-word" }}>
              {jobContext.detail}
            </div>
          ) : null}
        </td>
        <td>
          {/* Track is scheduler truth; origin remains secondary lineage context. */}
          <div style={{ fontWeight: 600 }}>{jobTrackLabel(job.track)}</div>
          {jobContext?.origin ? (
            <>
              <div style={{ color: "#6b7a8a", fontSize: 11 }}>{jobContext.origin}</div>
              <div style={{ color: "#6b7a8a", fontSize: 11 }}>{job.job_type}</div>
            </>
          ) : (
            <div style={{ color: "#6b7a8a", fontSize: 11 }}>{job.job_type}</div>
          )}
        </td>
        <td>{renderJobProgress(job, itemOutputs)}</td>
        <td className="jobs-timing">
          <div>Created {formatTs(job.created_at_ms)}</div>
          {job.started_at_ms ? <div>Started {formatTs(job.started_at_ms)}</div> : null}
          {job.finished_at_ms ? <div>Finished {formatTs(job.finished_at_ms)}</div> : null}
        </td>
        <td>
          <div className="row" style={{ marginTop: 0 }}>
            <button
              type="button"
              disabled={busy || !isActive(job.status)}
              onClick={() => cancel(job.id)}
            >
              Cancel
            </button>
            <button
              type="button"
              disabled={busy || !isRetryable(job.status)}
              onClick={() => retry(job.id)}
            >
              Retry
            </button>
            <button
              type="button"
              disabled={busy || !isIndividuallyDeletable(job.status)}
              onClick={() => deleteTerminalJob(job)}
            >
              Delete
            </button>
            <details style={{ display: "inline" }}>
              <summary style={{ cursor: "pointer", fontSize: 13 }}>More…</summary>
              <div className="row" style={{ marginTop: 4, flexWrap: "wrap" }}>
                {canInstallFfmpegTools ? (
                  <button type="button" disabled={busy} onClick={installFfmpegTools}>
                    Install FFmpeg
                  </button>
                ) : null}
                {canRevealMuxedPreview ? (
                  <button
                    type="button"
                    disabled={busy}
                    onClick={() => openMuxedPreview(job.item_id ?? "")}
                  >
                    Open preview
                  </button>
                ) : null}
                {canRevealMuxedPreview ? (
                  <button
                    type="button"
                    disabled={busy}
                    onClick={() =>
                      exportMuxedPreview(
                        job.item_id ?? "",
                        `voxvulgi-dub-preview-${(job.item_id ?? job.id).slice(0, 8)}`,
                      )
                    }
                  >
                    Export preview…
                  </button>
                ) : null}
                <button
                  type="button"
                  disabled={!job.logs_path}
                  onClick={() => openLogFile(job.logs_path)}
                >
                  Open log
                </button>
                <button type="button" disabled={busy} onClick={() => inspectJob(job.id)}>
                  Details
                </button>
                <button type="button" onClick={() => copyText(job.id).then((ok) => setNotice(ok ? "Copied job ID." : "Copy failed."))}>
                  Copy job ID
                </button>
                {job.batch_id ? (
                  <button type="button" onClick={() => copyText(job.batch_id).then((ok) => setNotice(ok ? "Copied batch ID." : "Copy failed."))}>
                    Copy batch ID
                  </button>
                ) : null}
                {jobContext?.detail ? (
                  <button type="button" onClick={() => copyText(jobContext.detail).then((ok) => setNotice(ok ? "Copied source context." : "Copy failed."))}>
                    Copy source
                  </button>
                ) : null}
                {job.error ? (
                  <button type="button" onClick={() => copyText(job.error).then((ok) => setNotice(ok ? "Copied error." : "Copy failed."))}>
                    Copy error
                  </button>
                ) : null}
                {canOpenContextTarget ? (
                  <button type="button" disabled={busy} onClick={() => openJobContextTarget(job)}>
                    {jobContext?.target_action_label}
                  </button>
                ) : null}
                {canOpenOutputs ? (
                  <button
                    type="button"
                    disabled={busy}
                    onClick={() => openItemOutputsDir(job.item_id ?? "")}
                  >
                    Open outputs
                  </button>
                ) : null}
                {canOpenArtifacts ? (
                  <button type="button" disabled={busy} onClick={() => openJobArtifactsDir(job.id)}>
                    Open artifacts
                  </button>
                ) : null}
              </div>
            </details>
          </div>
          {artifactsDir ? (
            <div style={{ marginTop: 6, color: "#4b5563", fontSize: 12, lineHeight: 1.3 }}>
              {itemOutputs?.terminal_summary ? (
                <div>
                  Outcome: {itemOutputs.terminal_summary}
                  {itemOutputs.deliverable_path ? (
                    <>
                      {" "}
                      Deliverable: <code>{itemOutputs.deliverable_path}</code>
                    </>
                  ) : null}
                </div>
              ) : null}
              <div>
                Artifacts: <code>{artifactsDir}</code>
              </div>
              {outputsDir ? (
                <div>
                  Outputs: <code>{outputsDir}</code>
                </div>
              ) : null}
            </div>
          ) : null}
        </td>
      </tr>
    );
  }

  return (
    <section>
      <h1>Jobs</h1>
      <p className="jobs-page-intro">See what is happening now, what needs action, and what finished.</p>

      {error ? <div className="error">{error}</div> : null}
      {notice ? (
        (() => {
          const sepIndex = notice.indexOf(NOTICE_DETAIL_SEP);
          const plain = sepIndex === -1 ? notice : notice.slice(0, sepIndex);
          const detail = sepIndex === -1 ? "" : notice.slice(sepIndex + NOTICE_DETAIL_SEP.length);
          return (
            <div className="jobs-notice" role="status">
              <div>{plain}</div>
              {detail ? (
                <details style={{ marginTop: 6 }}>
                  <summary style={{ cursor: "pointer", color: "#4b5563", fontSize: 13 }}>
                    Show technical details
                  </summary>
                  <div style={{ color: "#4b5563", fontSize: 12, marginTop: 4, overflowWrap: "anywhere" }}>
                    {detail}
                  </div>
                </details>
              ) : null}
            </div>
          );
        })()
      ) : null}
      {Object.keys(batchOperationsById).length ? (
        <div className="jobs-batch-operations" role="status" aria-label="Batch operation status">
          {Object.values(batchOperationsById)
            .sort((a, b) => b.started_at_ms - a.started_at_ms)
            .slice(0, 3)
            .map((operation) => (
              <div key={operation.request_id} className={`is-${operation.state}`}>
                <strong>
                  {operation.mode === "dry_run"
                    ? "Checking batch"
                    : operation.mode === "retry"
                      ? "Retrying batch"
                      : "Fixing batch"}
                </strong>
                <span>
                  {operation.state === "running"
                    ? "Running in the background — this page remains usable."
                    : operation.state === "succeeded"
                      ? "Completed."
                      : "Failed."}
                </span>
                <code>{operation.request_id}</code>
              </div>
            ))}
        </div>
      ) : null}

      <section className="jobs-toolbar" aria-label="Queue controls">
        <div className="jobs-toolbar-main">
          <div className={`jobs-queue-state ${queuePaused ? "is-paused" : "is-running"}`}>
            <strong>{queuePaused ? "Queue paused" : "Queue running"}</strong>
            <span>
              {queuePaused
                ? "Queued work waits until you resume. Running work continues."
                : jobsLoaded
                  ? `${overviewCounts.running} running · ${overviewCounts.queued} queued`
                  : "Loading current work…"}
            </span>
          </div>
          <div className="row" style={{ marginTop: 0 }}>
          <button
            type="button"
            disabled={busy}
            onClick={() => refresh()}
            title="Reload the list of jobs to show the latest progress."
          >
            Refresh
          </button>
          <button
            type="button"
            disabled={busy}
            onClick={() => setPauseAll(!queuePaused)}
            title={
              queuePaused
                ? "Start processing queued jobs again. Jobs already running are unaffected."
                : "Stop new jobs from starting. Jobs already running keep going."
            }
          >
            {queuePaused ? "Resume all" : "Pause all"}
          </button>
          <button
            type="button"
            disabled={busy || !hasActive}
            onClick={cancelAll}
            title="Stop every job that is currently running or waiting to run."
          >
            Cancel all active
          </button>
          </div>
        </div>
        <details className="jobs-scheduler-health">
          <summary>
            Scheduler health
            <span>
              {trackRuntimeState === "loading"
                ? "Loading…"
                : trackRuntimeState === "error"
                  ? "Unavailable"
                  : `${overviewCounts.running} running · ${overviewCounts.queued} queued · YouTube ${plainYoutubeGateState(youtubeGate?.state)}`}
            </span>
          </summary>
        <div className="jobs-track-strip" aria-label="Canonical track status">
          {Object.values(jobsProjectionState).some((state) => state === "stale" || state === "error") ? (
            <div data-testid="jobs-subscription-projection-state" className="jobs-track-runtime-state is-stale" role="status">
              {Object.values(jobsProjectionState).some((state) => state === "stale")
                ? "Subscription activity could not refresh; showing the last confirmed state."
                : "Subscription activity is unavailable; no failed poll was projected as an empty result."}
            </div>
          ) : null}
          {trackRuntimeState === "loading" ? (
            <div id="jobs-track-runtime-state" data-testid="jobs-track-runtime-state" className="jobs-track-runtime-state" role="status">
              Loading canonical track status and scheduler budgets…
            </div>
          ) : trackRuntimeState === "error" ? (
            <div id="jobs-track-runtime-state" data-testid="jobs-track-runtime-state" className="jobs-track-runtime-state is-error" role="status">
              Canonical track status is unavailable. No track totals or budgets are shown until it loads.
            </div>
          ) : (
            <>
              {trackRuntimeState === "stale" ? (
                <div id="jobs-track-runtime-state" data-testid="jobs-track-runtime-state" className="jobs-track-runtime-state is-stale" role="status">
                  Canonical track status could not refresh; showing the last confirmed state.
                </div>
              ) : null}
              {trackRows.map((track) => (
                <div
                  key={track.track}
                  id={`jobs-track-summary-${track.track}`}
                  data-testid={`jobs-track-summary-${track.track}`}
                  className="jobs-track-summary"
                >
                  <strong>{jobTrackLabel(track.track)}</strong>
                  <span>{track.running} running · {track.queued} queued</span>
                  <span>
                    {track.paused
                      ? "Paused"
                      : track.track === "youtube_recurring"
                        ? `Direct transfers ${track.effective_budget}/${track.configured_budget}`
                        : `Budget ${track.effective_budget}/${track.configured_budget}`}
                  </span>
                  {track.hold_reason ? <small>{plainTrackHoldReason(track.hold_reason)}</small> : null}
                </div>
              ))}
              {unclassifiedTrackTotals && unclassifiedTrackTotals.total > 0 ? (
                <div
                  id="jobs-track-summary-unclassified"
                  data-testid="jobs-track-summary-unclassified"
                  className="jobs-track-summary is-unclassified"
                >
                  <strong>Unclassified</strong>
                  <span>{unclassifiedTrackTotals.running} running · {unclassifiedTrackTotals.queued} queued</span>
                  <small>Unclassified jobs awaiting track repair; their route is not guessed.</small>
                </div>
              ) : null}
            </>
          )}
        </div>
        <div id="jobs-youtube-gate" data-testid="jobs-youtube-gate" className="jobs-youtube-gate">
          <strong>
            Shared YouTube start gate: {trackRuntimeState === "loading"
              ? "Loading canonical runtime…"
              : trackRuntimeState === "error"
                ? "Unavailable"
                : plainYoutubeGateState(youtubeGate?.state)}
          </strong>
          <span>
            YouTube single and YouTube background keep independent track slots and may overlap. Their
            process starts are staggered through one shared safe 5–10 second pacing/auth gate.
          </span>
          {trackRuntimeState === "stale" ? <small>Last confirmed gate state; refresh is currently unavailable.</small> : null}
          {youtubeGate?.hold_reason ? <small>{plainTrackHoldReason(youtubeGate.hold_reason)}</small> : null}
          {trackRuntimeState !== "error" && youtubeGate?.next_eligible_at_ms ? (
            <small>Next eligible start: {formatTs(youtubeGate.next_eligible_at_ms)}</small>
          ) : null}
        </div>
        </details>
        <details className="jobs-toolbar-more">
          <summary>Queue status, cleanup, and developer tools</summary>
          <div className="jobs-track-controls" aria-label="Scheduler track budgets">
            {trackRuntimeState === "loading" || trackRuntimeState === "error" ? (
              <div className="jobs-track-runtime-state">
                {trackRuntimeState === "loading"
                  ? "Loading canonical scheduler budgets…"
                  : "Scheduler budgets are unavailable until canonical track status loads."}
              </div>
            ) : trackRows.map((track) => (
              <div
                key={track.track}
                id={`jobs-track-control-${track.track}`}
                data-testid={`jobs-track-control-${track.track}`}
                className="jobs-track-control"
              >
                <span>
                  {jobTrackLabel(track.track)}{track.track === "youtube_recurring" ? " direct transfer" : ""} budget
                </span>
                <output>{track.configured_budget} configured / {track.effective_budget} effective</output>
                <small>
                  {track.paused
                    ? "Held by the scheduler."
                    : track.track === "youtube_recurring"
                      ? `Running ${track.running} total jobs (subscription checks and transfers); queued ${track.queued}.`
                      : `Running ${track.running}; queued ${track.queued}.`}
                </small>
              </div>
            ))}
          </div>
          <div className="row" style={{ marginTop: 8 }}>
            <button
              type="button"
              disabled={busy}
              onClick={flushCache}
              title="Remove finished jobs, old logs, and leftover files to free up space. Your videos are not touched."
            >
              Clean up old jobs and logs
            </button>
          </div>
          <div className="jobs-help-text">
            Queue budgets are edited in Options → Jobs / Queue. A reported hold stops new starts on that
            track while running work continues. Retry creates new queued work; it does not cancel older running jobs.
          </div>
          <div className="row" style={{ marginTop: 8 }}>
            <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span>Test duration (seconds)</span>
              <input
                type="number"
                min={1}
                max={600}
                value={dummySeconds}
                onChange={(e) => setDummySeconds(Number(e.currentTarget.value))}
                style={{ width: 110 }}
              />
            </label>
            <button type="button" disabled={busy} onClick={enqueueDummy}>
              Run test job
            </button>
          </div>
        </details>
      </section>

      {selectedJobDetail ? (
        <section className="jobs-detail-panel">
          <div className="row" style={{ justifyContent: "space-between", marginTop: 0 }}>
            <h2 style={{ margin: 0 }}>Job detail</h2>
            <button type="button" onClick={() => setSelectedJobDetail(null)}>
              Close
            </button>
          </div>
          <div style={{ color: "#4b5563", fontSize: 12, marginTop: 8 }}>
            Selected <code>{selectedJobDetail.selected_job_id}</code>. Current attempt{" "}
            <code>{selectedJobDetail.current_attempt_job_id}</code>
            {selectedJobDetail.batch_id ? (
              <>
                . Batch <code>{selectedJobDetail.batch_id}</code>
              </>
            ) : null}
          </div>
          <div className="table-wrap" style={{ maxHeight: 360, overflow: "auto", marginTop: 12 }}>
            <table>
              <thead>
                <tr>
                  <th>Status</th>
                  <th>IDs</th>
                  <th>Source</th>
                  <th>Output</th>
                  <th>Error</th>
                  <th>Actions</th>
                </tr>
              </thead>
              <tbody>
                {selectedJobDetail.attempts.map((attempt) => (
                  <tr key={attempt.job.id}>
                    <td>
                      <strong>{attempt.status_label}</strong>
                      <div
                        style={{ color: "#4b5563", fontSize: 12 }}
                        title={`Internal detail: ${attempt.lineage_kind}`}
                      >
                        {attempt.is_current_attempt ? "Latest try" : "Earlier try"}
                      </div>
                    </td>
                    <td>
                      <div>Job <code>{attempt.job.id.slice(0, 8)}</code></div>
                      {attempt.job.batch_id ? <div>Batch <code>{attempt.job.batch_id.slice(0, 8)}</code></div> : null}
                      {attempt.job.retry_of_job_id ? <div>Retry of <code>{attempt.job.retry_of_job_id.slice(0, 8)}</code></div> : null}
                      {attempt.job.retry_replacement_job_id ? <div>Replaced by <code>{attempt.job.retry_replacement_job_id.slice(0, 8)}</code></div> : null}
                    </td>
                    <td style={{ minWidth: 260, maxWidth: 460, overflowWrap: "anywhere" }}>
                      <div style={{ fontWeight: 600 }}>{jobTrackLabel(attempt.job.track)}</div>
                      <div style={{ fontWeight: 600 }}>{attempt.source_title || "(missing title)"}</div>
                      {attempt.source_url ? <div>{attempt.source_url}</div> : null}
                      {attempt.video_id ? <div>Video ID: <code>{attempt.video_id}</code></div> : null}
                      {attempt.source_path ? <div>Source path: <code>{attempt.source_path}</code></div> : null}
                      {attempt.filename ? <div>Filename: <code>{attempt.filename}</code></div> : null}
                    </td>
                    <td style={{ minWidth: 220, maxWidth: 360, overflowWrap: "anywhere" }}>
                      {attempt.output_path ? <div><code>{attempt.output_path}</code></div> : "(no output)"}
                      {attempt.output_dir ? <div>Target root: <code>{attempt.output_dir}</code></div> : null}
                    </td>
                    <td style={{ minWidth: 240, maxWidth: 420, overflowWrap: "anywhere" }}>
                      {attempt.job.error || "-"}
                    </td>
                    <td>
                      <div className="row" style={{ marginTop: 0 }}>
                        <button type="button" onClick={() => copyText(attempt.source_url).then((ok) => setNotice(ok ? "Copied URL." : "Copy failed."))}>
                          Copy URL
                        </button>
                        <button type="button" onClick={() => copyText(attempt.video_id).then((ok) => setNotice(ok ? "Copied video ID." : "Copy failed."))}>
                          Copy video ID
                        </button>
                        <button type="button" onClick={() => copyText(attempt.job.id).then((ok) => setNotice(ok ? "Copied job ID." : "Copy failed."))}>
                          Copy job ID
                        </button>
                        <button type="button" onClick={() => copyText(attempt.output_path).then((ok) => setNotice(ok ? "Copied output path." : "Copy failed."))}>
                          Copy output
                        </button>
                        <button type="button" onClick={() => copyText(attempt.job.error).then((ok) => setNotice(ok ? "Copied error." : "Copy failed."))}>
                          Copy error
                        </button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </section>
      ) : null}

      <section className="jobs-workspace">
        <h2 className="jobs-workspace-title">Work</h2>
        <nav className="jobs-view-tabs" aria-label="Jobs view">
          <button
            type="button"
            className={primaryView === "now" ? "is-active" : undefined}
            aria-pressed={primaryView === "now"}
            onClick={() => {
              setJobsLoaded(false);
              setPrimaryView("now");
            }}
          >
            Now <span>{jobsLoaded ? `${overviewCounts.running} running · ${overviewCounts.queued} queued` : "Loading…"}</span>
          </button>
          <button
            type="button"
            className={primaryView === "attention" ? "is-active" : undefined}
            aria-pressed={primaryView === "attention"}
            onClick={() => {
              setJobsLoaded(false);
              setPrimaryView("attention");
            }}
          >
            Needs attention <span>{jobsLoaded ? overviewCounts.failed : "…"}</span>
          </button>
          <button
            type="button"
            className={primaryView === "history" ? "is-active" : undefined}
            aria-pressed={primaryView === "history"}
            onClick={() => {
              setJobsLoaded(false);
              setPrimaryView("history");
            }}
          >
            History <span>{jobsLoaded ? overviewCounts.succeeded + overviewCounts.failed + overviewCounts.canceled : "…"}</span>
          </button>
        </nav>

        <div className="jobs-track-filter">
          <label htmlFor="jobs-track-filter">
            <span>Work source</span>
            <select
              id="jobs-track-filter"
              data-testid="jobs-track-filter"
              value={selectedTrack}
              onChange={(event) => {
                setJobsLoaded(false);
                setSelectedSubscriptionId(null);
                setSelectedTrack(event.currentTarget.value as DisplayJobTrack | "all");
              }}
            >
              <option value="all">
                All sources · {jobsLoaded ? countForJobsView(overviewCounts, primaryView) : "…"}
              </option>
              {CANONICAL_JOB_TRACKS.map((track) => {
                const runtime = trackRows.find((row) => row.track === track);
                const count = runtime
                  ? primaryView === "now"
                    ? runtime.running + runtime.queued
                    : primaryView === "attention"
                      ? runtime.failed
                      : runtime.succeeded + runtime.failed + runtime.canceled
                  : selectedTrack === track
                    ? countForJobsView(selectedOverviewCounts, primaryView)
                    : null;
                return (
                  <option key={track} value={track}>
                    {jobTrackTabLabel(track)} · {count ?? "…"}
                  </option>
                );
              })}
              {unclassifiedTrackTotals && unclassifiedTrackTotals.total > 0 ? (
                <option value="unclassified">
                  Unclassified · {countForJobsView(unclassifiedTrackTotals, primaryView)}
                </option>
              ) : null}
            </select>
          </label>
          <span>The backend applies this source before building the bounded preview.</span>
        </div>

        <form className="jobs-findbar" onSubmit={applyJobSearch}>
          <label style={{ display: "flex", alignItems: "center", gap: 8, minWidth: 360 }}>
            <span>Find</span>
            <input
              type="search"
              value={jobSearchQuery}
              onChange={(event) => setJobSearchQuery(event.currentTarget.value)}
              placeholder="Batch ID, job ID, URL, title, error"
              style={{ minWidth: 300 }}
            />
          </label>
          <button type="submit" disabled={busy}>
            Search
          </button>
          <button type="button" disabled={busy || !jobSearchQuery.trim()} onClick={clearJobSearch}>
            Clear
          </button>
          {downloadChannelSummaries.length ? (
            <label className="jobs-source-filter">
              <span>Source</span>
              <select
                value={selectedSubscriptionId ?? ""}
                onChange={(event) => setSelectedSubscriptionId(event.currentTarget.value || null)}
              >
                <option value="">All sources in this preview</option>
                {downloadChannelSummaries.map((summary) => (
                  <option key={summary.summaryId} value={summary.summaryId}>
                    {summary.label} · {summary.running} running · {summary.queued} queued · {summary.failed} failed
                  </option>
                ))}
              </select>
            </label>
          ) : null}
        </form>

        <details className="jobs-advanced-filters">
          <summary>Advanced filters and history cleanup</summary>
          <div className="row" style={{ marginTop: 8 }}>
            <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span>Filter loaded preview</span>
              <select value={jobsFilter} onChange={(event) => setJobsFilter(event.currentTarget.value as JobsFilter)}>
                <option value="all">No extra filter</option>
                <option value="failed">Failed/canceled</option>
                <option value="auth_blocked">Blocked by YouTube auth</option>
                <option value="retried">Retried</option>
                <option value="unretried">Unretried failed</option>
                <option value="succeeded_retry">Succeeded on retry</option>
                <option value="missing_title">Missing title</option>
                <option value="no_output">No output</option>
              </select>
            </label>
            <button
              type="button"
              disabled={busy || !jobSearchQuery.trim() || terminalShownCount === 0}
              onClick={deleteTerminalJobsMatchingSearch}
            >
              Delete failed/canceled search results ({terminalShownCount})
            </button>
          </div>
        </details>

        <div className="jobs-preview-note">
          {jobSearchQuery.trim()
            ? `Search shows up to ${JOBS_SEARCH_LIMIT} matches. Exact job IDs, batch IDs, queued YouTube video IDs, and exact source URLs search the full store; other text searches the 10,000 newest attempts.`
            : primaryView === "now"
              ? `Canonical current work: ${overviewCounts.running} running and ${overviewCounts.queued} queued. The list shows the newest bounded preview and always includes running jobs.`
              : primaryView === "attention"
                ? `Canonical failed attempts: ${overviewCounts.failed}. The list shows the newest bounded preview; batch health distinguishes unresolved videos from historical failed attempts.`
                : `Canonical terminal history: ${overviewCounts.succeeded} succeeded, ${overviewCounts.failed} failed, ${overviewCounts.canceled} canceled. The list is a bounded recent preview.`}
          {selectedSubscriptionId
            ? ` Source filter: "${resolveSummaryLabel(selectedSubscriptionId, youtubeSubscriptionsById)}".`
            : ""}
          {selectedTrack !== "all"
            ? ` Track: "${selectedTrack === "unclassified" ? "Unclassified" : jobTrackTabLabel(selectedTrack)}". The backend selected this track before applying the bounded preview limit; canonical all-track totals above are unchanged.`
            : ""}
        </div>
        <div className="table-wrap jobs-table-wrap">
          <table>
            <thead>
              <tr>
                <th>Status</th>
                <th>Work</th>
                <th>Source</th>
                <th>Progress</th>
                <th>Timing</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {groupedJobs.length ? (
                visibleGroupedJobs.map((group) => {
                  const isSingleGroup = group.jobs.length === 1;
                  const expanded = expandedGroups[group.key] ?? group.isSingle;
                  const groupRenderLimit = groupRenderLimits[group.key] ?? JOB_GROUP_RENDER_STEP;
                  const status = summarizeGroupStatusFromCounts(group);
                  const progress = summarizeGroupProgress(group.jobs);
                  const activeCount = group.jobs.filter((job) => isActive(job.status)).length;
                  const retryableCount = group.jobs.filter(
                    (job) => isRetryable(job.status),
                  ).length;
                  const canonicalBatchId = group.batchIds.length === 1 ? group.batchIds[0] : null;
                  const canonicalDetail = canonicalBatchId ? batchDetailsById[canonicalBatchId] : null;
                  const health = canonicalDetail?.health ?? null;
                  const displayedStatus = status;
                  const groupFailure = status === "failed" ? summarizeGroupFailure(group.jobs) : null;
                  const displayedProgress = isSingleGroup
                    ? progress
                    : group.running > 0 || !health
                      ? progress
                      : summarizeBatchTargetProgress(health);
                  const batchRetryableCount = health ? health.retryable_targets : retryableCount;
                  const canOperateOnBatch = Boolean(canonicalBatchId) && !group.isSingle;
                  const waitingForResume = queuePaused && status === "queued" && activeCount > 0;
                  const groupSummaryText = summarizeGroupActivity(group);
                  const finishedCount = group.jobs.filter(
                    (job) =>
                      job.status === "succeeded" ||
                      job.status === "failed" ||
                      job.status === "canceled",
                  ).length;
                  const groupLogPath = group.jobs.find((job) => Boolean(job.logs_path))?.logs_path ?? "";

                  return (
                    <Fragment key={group.key}>
                      <tr className="batch-row">
                        <td>
                          <div
                            style={{
                              fontWeight: 700,
                              color: groupFailure ? toneStyle(groupFailure.tone).color : undefined,
                            }}
                          >
                            {groupFailure ? `Failed — ${groupFailure.label}` : displayedStatus}
                          </div>
                          {groupFailure?.requirement ? (
                            <div style={{ color: "#4b5563", fontSize: 12, lineHeight: 1.3 }}>
                              {groupFailure.requirement}
                            </div>
                          ) : null}
                          <div style={{ color: "#4b5563", fontSize: 12, lineHeight: 1.3 }}>
                            {health && group.batchIds.length === 1
                              ? `(${health.succeeded_targets}/${health.canonical_targets} videos downloaded)`
                              : groupSummaryText}
                          </div>
                          {finishedCount > 0 ? (
                            <div style={{ color: "#4b5563", fontSize: 11, lineHeight: 1.3 }}>
                              {`(${finishedCount}/${group.jobs.length} done)`}
                            </div>
                          ) : null}
                          {waitingForResume ? (
                            <div style={{ color: "#7c2d12", fontSize: 12, lineHeight: 1.3 }}>
                              Waiting for Resume all
                            </div>
                          ) : null}
                        </td>
                        <td style={{ minWidth: 260, maxWidth: 420 }}>
                          <div style={{ fontWeight: 600, overflowWrap: "anywhere", lineHeight: 1.3 }}>
                            {group.label}
                          </div>
                          <div className="jobs-row-id" title={canonicalBatchId ?? group.key}>
                            {canonicalBatchId ? "Batch" : "Group"}{" "}
                            <code>{(canonicalBatchId ?? group.key).slice(0, 8)}</code>
                          </div>
                          <div style={{ color: "#0f766e", fontSize: 12, lineHeight: 1.3 }}>
                            {summarizeJobGroupTargets(group.jobs, jobContexts)}
                          </div>
                          <div style={{ color: "#4b5563", fontSize: 12 }}>
                            {health
                              ? batchTargetHealthText(health)
                              : `${group.jobs.length} loaded job${group.jobs.length === 1 ? "" : "s"} in this group`}
                          </div>
                          {health ? (
                            <div style={{ color: "#4b5563", fontSize: 12 }}>
                              {batchAttemptHealthText(health)}. Loaded preview: {group.jobs.length}. Retryable unresolved videos: {health.retryable_targets}.
                            </div>
                          ) : null}
                          {health ? (
                            <div style={{ color: "#4b5563", fontSize: 12 }}>
                              Attempt metadata: missing titles {health.missing_title_jobs}; no-output attempts {health.no_output_jobs}. Failed attempts can be historical after a video downloaded.
                            </div>
                          ) : null}
                          <div style={{ color: "#4b5563", fontSize: 12 }}>
                            {canonicalBatchId ? (
                              <span>
                                Batch ID: <code>{canonicalBatchId}</code>
                              </span>
                            ) : group.batchIds.length > 1 ? (
                              <span>Batches: {group.batchIds.slice(0, 3).join(", ")}</span>
                            ) : (
                              "Batch ID: -"
                            )}
                          </div>
                        </td>
                        <td>
                          {/* Track is durable scheduler truth. Origin remains a secondary
                              explanation because a batch can contain more than one track. */}
                          {(() => {
                            const groupOrigin = jobContexts[group.jobs[0]?.id]?.origin;
                            return (
                              <>
                                <div style={{ fontWeight: 600 }}>{groupTrackLabel(group.jobs)}</div>
                                {groupOrigin ? <div style={{ color: "#6b7a8a", fontSize: 11 }}>{groupOrigin}</div> : null}
                                <div style={{ color: "#6b7a8a", fontSize: 11 }}>
                                  {summarizeGroupType(group.jobs)}
                                </div>
                              </>
                            );
                          })()}
                        </td>
                        <td>
                          {/* WP-0256: visual batch progress bar. */}
                          <div className="job-bar">
                            <div
                              className={`job-bar-fill job-bar-${displayedStatus}`}
                              style={{ width: `${Math.round(displayedProgress * 100)}%` }}
                            />
                          </div>
                          <div style={{ fontWeight: 600 }}>{Math.round(displayedProgress * 100)}%</div>
                        </td>
                        <td className="jobs-timing">
                          <div>Created {formatTs(summarizeCreatedTs(group.jobs))}</div>
                          {summarizeStartedTs(group.jobs) ? (
                            <div>Started {formatTs(summarizeStartedTs(group.jobs))}</div>
                          ) : null}
                          {summarizeFinishedTs(group.jobs) ? (
                            <div>Finished {formatTs(summarizeFinishedTs(group.jobs))}</div>
                          ) : null}
                        </td>
                        <td>
                          <div className="row" style={{ marginTop: 0 }}>
                            <button
                              type="button"
                              disabled={busy}
                              aria-expanded={expanded}
                              onClick={() =>
                                setExpandedGroups((prev) => ({
                                  ...prev,
                                  [group.key]: !expanded,
                                }))
                              }
                              title="Show or hide the individual videos in this group."
                            >
                              {expanded ? "Collapse" : "Expand"} ({group.jobs.length})
                            </button>
                            <button
                              type="button"
                              disabled={
                                busy ||
                                batchOperationRunning(canonicalBatchId) ||
                                (canOperateOnBatch
                                  ? Boolean(health) && batchRetryableCount === 0
                                  : retryableCount === 0)
                              }
                              onClick={() => retryGroup(group)}
                              title="Try again on the videos in this group that failed or did not finish."
                            >
                              {canOperateOnBatch
                                ? health
                                  ? `Retry unfinished (${batchRetryableCount})`
                                  : "Retry unfinished"
                                : `Retry failed (${retryableCount})`}
                            </button>
                            <details style={{ display: "inline" }}>
                              <summary style={{ cursor: "pointer", fontSize: 13 }}>More…</summary>
                              <div className="row" style={{ marginTop: 4, flexWrap: "wrap" }}>
                                <button
                                  type="button"
                                  disabled={busy || activeCount === 0}
                                  onClick={() => cancelGroup(group)}
                                  title="Stop the videos in this group that are running or waiting to run."
                                >
                                  Cancel active ({activeCount})
                                </button>
                                {canOperateOnBatch ? (
                                  <>
                                    <button
                                      type="button"
                                      disabled={busy || batchOperationRunning(canonicalBatchId)}
                                      onClick={() => repairBatch(canonicalBatchId ?? "")}
                                      title="Leave finished videos alone and retry the ones that failed or were canceled."
                                    >
                                      Fix batch
                                    </button>
                                    <button
                                      type="button"
                                      disabled={busy}
                                      onClick={() => backfillBatchTitles(canonicalBatchId ?? "")}
                                      title="Look up and fill in any missing video titles in this group."
                                    >
                                      Fix missing titles
                                    </button>
                                    <button
                                      type="button"
                                      disabled={busy}
                                      onClick={() => exportUnresolvedBatch(canonicalBatchId ?? "", "csv")}
                                      title="Save a spreadsheet listing the videos in this group that have not finished."
                                    >
                                      Save list of unfinished videos
                                    </button>
                                    <button
                                      type="button"
                                      disabled={busy}
                                      onClick={() => exportUnresolvedBatch(canonicalBatchId ?? "", "urls")}
                                      title="Copy the links of the videos in this group that have not finished."
                                    >
                                      Copy links of unfinished videos
                                    </button>
                                  </>
                                ) : null}
                                <button
                                  type="button"
                                  disabled={!groupLogPath}
                                  onClick={() => openLogFile(groupLogPath)}
                                  title="Open the detailed activity log for this group."
                                >
                                  Open log file
                                </button>
                              </div>
                            </details>
                          </div>
                        </td>
                      </tr>
                      {expanded
                        ? group.jobs
                            .slice(0, groupRenderLimit)
                            .map((job) => renderJobRow(job, isSingleGroup ? false : true))
                        : null}
                      {expanded && groupRenderLimit < group.jobs.length ? (
                        <tr className="jobs-group-load-more">
                          <td colSpan={6}>
                            <span>
                              Showing {groupRenderLimit} of {group.jobs.length} loaded attempts.
                            </span>
                            <button
                              type="button"
                              data-agent-safe-action="true"
                              onClick={() =>
                                setGroupRenderLimits((current) => ({
                                  ...current,
                                  [group.key]: Math.min(
                                    groupRenderLimit + JOB_GROUP_RENDER_STEP,
                                    group.jobs.length,
                                  ),
                                }))
                              }
                            >
                              Load {Math.min(
                                JOB_GROUP_RENDER_STEP,
                                group.jobs.length - groupRenderLimit,
                              )} more
                            </button>
                          </td>
                        </tr>
                      ) : null}
                    </Fragment>
                  );
                })
              ) : (
                <tr className="jobs-empty-row">
                  <td colSpan={6}>
                    {!jobsLoaded
                      ? "Loading current work…"
                      : error && jobs.length === 0
                        ? "Jobs could not be loaded. Use Refresh after the error above is resolved."
                        : jobSearchQuery.trim()
                          ? "No jobs match this search."
                          : primaryView === "now"
                            ? overviewCounts.running + overviewCounts.queued > 0
                              ? "Current work exists in the canonical queue but is outside this preview. Search by job ID, video ID, or URL."
                              : "Nothing is running or queued."
                            : primaryView === "attention"
                              ? overviewCounts.failed > 0
                                ? "Failed attempts exist outside this recent preview. Search for the source, video ID, or batch."
                                : "Nothing needs attention."
                              : overviewCounts.total === 0
                                ? "No job history exists yet."
                                : "No terminal jobs match the current filters."}
                  </td>
                </tr>
              )}
            </tbody>
          </table>
          {groupPreviewRenderLimit < groupedJobs.length ? (
            <div className="jobs-preview-load-more">
              <span>
                Showing {groupPreviewRenderLimit} of {groupedJobs.length} groups in this canonical preview.
              </span>
              <button
                type="button"
                data-agent-safe-action="true"
                onClick={() =>
                  setGroupPreviewRenderLimit((current) =>
                    Math.min(current + JOB_GROUP_PREVIEW_RENDER_STEP, groupedJobs.length),
                  )
                }
              >
                Load {Math.min(
                  JOB_GROUP_PREVIEW_RENDER_STEP,
                  groupedJobs.length - groupPreviewRenderLimit,
                )} more groups
              </button>
            </div>
          ) : null}
        </div>
      </section>
    </section>
  );
}
