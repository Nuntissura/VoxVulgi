import { Fragment, useCallback, useEffect, useMemo, useState, type FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { confirm, save } from "@tauri-apps/plugin-dialog";
import { usePageActivity, usePollingLoop } from "../lib/activity";
import { copyPathToClipboard, openPathBestEffort, requireOpenablePath, revealPath } from "../lib/pathOpener";
import {
  buildJobContextSummary,
  safeParseJobParams,
  stringOrNull,
  summarizeJobGroupTargets,
  type JobContextSummary,
} from "../lib/archiverRuntime";

type JobStatus = "queued" | "running" | "succeeded" | "failed" | "canceled";

type JobRow = {
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
  target_title?: string | null;
  retry_of_job_id?: string | null;
  retry_replacement_job_id?: string | null;
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

type JobGroup = {
  key: string;
  batchId: string | null;
  jobs: JobRow[];
};

type JobQueueControlState = {
  paused: boolean;
};

type JobRuntimeSettings = {
  max_concurrency: number;
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

const JOBS_PAGE_REFRESH_LIMIT = 80;
const JOBS_SEARCH_LIMIT = 500;
const JOB_CONTEXT_HYDRATION_LIMIT = 25;
const ACTIVE_JOBS_POLL_INTERVAL_MS = 2_500;
type JobsFilter =
  | "all"
  | "failed"
  | "auth_blocked"
  | "retried"
  | "unretried"
  | "succeeded_retry"
  | "missing_title"
  | "no_output";

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

function isTransientDatabaseLock(error: unknown): boolean {
  return String(error).includes("database is locked");
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, ms));
}

function retrySummaryText(summary: RetryBatchFailedSummary): string {
  const queuedText = summary.queued_jobs
    ? `Queued ${summary.queued_jobs} retry${summary.queued_jobs === 1 ? "" : "ies"}`
    : "No new retries queued";
  const reusedText = summary.reused_active_jobs
    ? `reused ${summary.reused_active_jobs} active target${summary.reused_active_jobs === 1 ? "" : "s"}`
    : "no active duplicate targets";
  const blockedText = summary.blocked_jobs ? `blocked ${summary.blocked_jobs}` : "blocked 0";
  const skippedText = summary.skipped_succeeded_jobs
    ? `skipped ${summary.skipped_succeeded_jobs} succeeded target${summary.skipped_succeeded_jobs === 1 ? "" : "s"}`
    : "skipped 0 succeeded";
  const unresolvedText = `unresolved ${summary.unresolved_jobs}`;
  const failedText = summary.failed_retries ? `failed-to-enqueue ${summary.failed_retries}` : "failed-to-enqueue 0";
  const firstErrorText = summary.first_error ? ` First error: ${summary.first_error}` : "";
  return `${queuedText}; ${reusedText}; ${blockedText}; ${skippedText}; ${failedText}; ${unresolvedText}; canonical retryable ${summary.matched_retryable_jobs}.${firstErrorText}`;
}

function copyText(value: string | null | undefined): Promise<boolean> {
  const text = (value ?? "").trim();
  if (!text) return Promise.resolve(false);
  return navigator.clipboard
    ?.writeText(text)
    .then(() => true)
    .catch(() => false) ?? Promise.resolve(false);
}

function summarizeGroupStatus(jobs: JobRow[]): JobStatus {
  if (jobs.some((job) => job.status === "running")) return "running";
  if (jobs.some((job) => job.status === "queued")) return "queued";
  if (jobs.some((job) => job.status === "failed")) return "failed";
  if (jobs.some((job) => job.status === "canceled")) return "canceled";
  return "succeeded";
}

function summarizeGroupProgress(jobs: JobRow[]): number {
  if (!jobs.length) return 0;
  const total = jobs.reduce((sum, job) => sum + (Number.isFinite(job.progress) ? job.progress : 0), 0);
  return Math.max(0, Math.min(1, total / jobs.length));
}

function summarizeBatchTargetStatus(health: JobBatchHealthSummary): "queued" | "running" | "succeeded" | "failed" {
  if (health.running_jobs > 0) return "running";
  if (health.queued_jobs > 0) return "queued";
  return health.unresolved_targets > 0 ? "failed" : "succeeded";
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
  const pageActive = usePageActivity(visible);
  const [jobs, setJobs] = useState<JobRow[]>([]);
  const [jobItemsById, setJobItemsById] = useState<Record<string, LibraryItem>>({});
  const [itemOutputsById, setItemOutputsById] = useState<Record<string, ItemOutputs>>({});
  const [youtubeSubscriptionsById, setYoutubeSubscriptionsById] = useState<
    Record<string, YoutubeSubscriptionRow>
  >({});
  const [instagramSubscriptionsById, setInstagramSubscriptionsById] = useState<
    Record<string, InstagramSubscriptionRow>
  >({});
  const [expandedGroups, setExpandedGroups] = useState<Record<string, boolean>>({});
  const [appDataDir, setAppDataDir] = useState<string>("");
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [dummySeconds, setDummySeconds] = useState(10);
  const [queuePaused, setQueuePaused] = useState(false);
  const [maxConcurrency, setMaxConcurrency] = useState(4);
  const [jobSearchQuery, setJobSearchQuery] = useState("");
  const [jobsFilter, setJobsFilter] = useState<JobsFilter>("all");
  const [batchDetailsById, setBatchDetailsById] = useState<Record<string, JobBatchDetail>>({});
  const [selectedJobDetail, setSelectedJobDetail] = useState<JobDetail | null>(null);

  async function handlePathOpenFailure(path: string, error: unknown, actionLabel: string) {
    const copied = await copyPathToClipboard(path);
    const suffix = copied ? " Path copied to clipboard." : "";
    setError(`${actionLabel} failed: ${String(error)}.${suffix}`);
  }

  const refreshJobsSnapshot = useCallback(async () => {
    const query = jobSearchQuery.trim();
    const next = query
      ? await invoke<JobRow[]>("jobs_search", { query, limit: JOBS_SEARCH_LIMIT })
      : await invoke<JobRow[]>("jobs_list", { limit: JOBS_PAGE_REFRESH_LIMIT, offset: 0 });
    setJobs(next);
    setError((current) => (current?.includes("database is locked") ? null : current));
  }, [jobSearchQuery]);

  const refreshQueueControls = useCallback(async function refreshQueueControls() {
    const [control, runtime] = await Promise.all([
      invoke<JobQueueControlState>("jobs_queue_control_get").catch((err) => {
        console.warn("jobs_queue_control_get failed", err);
        return null;
      }),
      invoke<JobRuntimeSettings>("jobs_runtime_settings_get").catch((err) => {
        console.warn("jobs_runtime_settings_get failed", err);
        return null;
      }),
    ]);
    if (control) setQueuePaused(control.paused);
    if (runtime) setMaxConcurrency(runtime.max_concurrency);
  }, []);

  const refreshSubscriptionLookups = useCallback(async () => {
    const [youtubeSubscriptions, instagramSubscriptions] = await Promise.all([
      invoke<YoutubeSubscriptionRow[]>("youtube_subscriptions_list").catch(() => []),
      invoke<InstagramSubscriptionRow[]>("instagram_subscriptions_list").catch(() => []),
    ]);
    setYoutubeSubscriptionsById(
      Object.fromEntries(youtubeSubscriptions.map((subscription) => [subscription.id, subscription])),
    );
    setInstagramSubscriptionsById(
      Object.fromEntries(instagramSubscriptions.map((subscription) => [subscription.id, subscription])),
    );
  }, []);

  const refresh = useCallback(async function refresh() {
    try {
      await refreshJobsSnapshot();
    } catch (e) {
      if (!isTransientDatabaseLock(e)) throw e;
      await sleep(1_500);
      await refreshJobsSnapshot();
    }
    await Promise.all([refreshQueueControls(), refreshSubscriptionLookups()]);
  }, [refreshJobsSnapshot, refreshQueueControls, refreshSubscriptionLookups]);

  useEffect(() => {
    if (!pageActive) return;
    refresh().catch((e) => setError(String(e)));
  }, [pageActive, refresh]);

  useEffect(() => {
    invoke<DiagnosticsInfo>("diagnostics_info")
      .then((info) => setAppDataDir(info.app_data_dir ?? ""))
      .catch(() => undefined);
  }, []);

  useEffect(() => {
    if (!pageActive) return;
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
  }, [jobs, pageActive]);

  useEffect(() => {
    if (!pageActive) return;
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
  }, [jobs, pageActive]);

  const hasActive = useMemo(
    () => jobs.some((job) => isActive(job.status)),
    [jobs],
  );
  const terminalShownCount = useMemo(
    () => jobs.filter((job) => isIndividuallyDeletable(job.status)).length,
    [jobs],
  );

  const filteredJobs = useMemo(() => {
    if (jobsFilter === "all") return jobs;
    return jobs.filter((job) => {
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
  }, [jobs, jobsFilter]);

  const groupedJobs = useMemo(() => {
    const byKey = new Map<string, JobGroup>();
    const groups: JobGroup[] = [];

    for (const job of filteredJobs) {
      const key = job.batch_id ? `batch:${job.batch_id}` : `job:${job.id}`;
      let group = byKey.get(key);
      if (!group) {
        group = { key, batchId: job.batch_id ?? null, jobs: [] };
        byKey.set(key, group);
        groups.push(group);
      }
      group.jobs.push(job);
    }

    return groups;
  }, [filteredJobs]);

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

  useEffect(() => {
    setExpandedGroups((prev) => {
      const validKeys = new Set(
        groupedJobs
          .filter((group) => group.jobs.length > 1)
          .map((group) => group.key),
      );
      let changed = false;
      const next: Record<string, boolean> = {};
      for (const [key, value] of Object.entries(prev)) {
        if (validKeys.has(key)) {
          next[key] = value;
        } else {
          changed = true;
        }
      }
      return changed ? next : prev;
    });
  }, [groupedJobs]);

  useEffect(() => {
    if (!pageActive) return;
    let cancelled = false;
    const batchIds = Array.from(
      new Set(
        groupedJobs
          .map((group) => group.batchId?.trim())
          .filter((value): value is string => Boolean(value)),
      ),
    ).slice(0, 12);
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
  }, [groupedJobs, pageActive]);

  usePollingLoop(
    async () => {
      await refreshJobsSnapshot().catch(() => undefined);
    },
    {
      enabled: pageActive && hasActive,
      intervalMs: ACTIVE_JOBS_POLL_INTERVAL_MS,
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
      const next = await invoke<JobRow[]>("jobs_list", { limit: JOBS_PAGE_REFRESH_LIMIT, offset: 0 });
      setJobs(next);
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

  async function cancelGroup(group: JobGroup) {
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
      setNotice(`Canceled ${activeIds.length} active job${activeIds.length === 1 ? "" : "s"} in batch.`);
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

  async function retryGroup(group: JobGroup) {
    if (group.batchId) {
      setBusy(true);
      setError(null);
      setNotice(null);
      try {
        const dryRun = await invoke<RetryBatchFailedSummary>("jobs_retry_batch_failed_dry_run", {
          batchId: group.batchId,
          batch_id: group.batchId,
        });
        if (dryRun.matched_retryable_jobs === 0) {
          setNotice(`No unresolved videos to retry. ${retrySummaryText(dryRun)}`);
          return;
        }
        const ok = await confirm(`Retry failed batch?\n${retrySummaryText(dryRun)}`, {
          title: "Retry failed batch",
          kind: dryRun.blocked_jobs ? "warning" : "info",
        });
        if (!ok) return;
        const summary = await invoke<RetryBatchFailedSummary>("jobs_retry_batch_failed", {
          batchId: group.batchId,
          batch_id: group.batchId,
        });
        setNotice(
          queuePaused
            ? `${retrySummaryText(summary)} Queue is paused; click Resume all to start queued retry work.`
            : retrySummaryText(summary),
        );
        await refresh();
      } catch (e) {
        setError(String(e));
      } finally {
        setBusy(false);
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
    const ok = await confirm("Repair Batch will skip succeeded targets, avoid active duplicates, and retry unresolved failed/canceled targets when auth is not blocked.", {
      title: "Repair Batch",
      kind: "warning",
    });
    if (!ok) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const summary = await invoke<RetryBatchFailedSummary>("jobs_repair_batch", { batchId, batch_id: batchId });
      setNotice(retrySummaryText(summary));
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
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
      const path = outputs.mux_dub_preview_v1_mp4_exists
        ? outputs.mux_dub_preview_v1_mp4_path
        : outputs.mux_dub_preview_v1_mkv_exists
          ? outputs.mux_dub_preview_v1_mkv_path
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
    let preferredExt = "mp4";
    try {
      const outputs = await invoke<ItemOutputs>("item_outputs", { itemId });
      if (outputs.mux_dub_preview_v1_mp4_exists) preferredExt = "mp4";
      else if (outputs.mux_dub_preview_v1_mkv_exists) preferredExt = "mkv";
    } catch {
      // ignore
    }

    const out = await save({
      title: `Export muxed preview (${preferredExt.toUpperCase()})`,
      defaultPath: `${suggestedStem}.${preferredExt}`,
      filters: [
        { name: "MP4", extensions: ["mp4"] },
        { name: "MKV", extensions: ["mkv"] },
      ],
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

  async function applyConcurrency() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const value = Number.isFinite(maxConcurrency)
        ? Math.max(1, Math.min(16, Math.round(maxConcurrency)))
        : 4;
      const runtime = await invoke<JobRuntimeSettings>("jobs_runtime_settings_set", {
        maxConcurrency: value,
      });
      setMaxConcurrency(runtime.max_concurrency);
      setNotice(`Max concurrency set to ${runtime.max_concurrency}.`);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

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

    return (
      <tr key={job.id} className={nested ? "batch-child-row" : undefined}>
        <td>
          {nested ? "\u251C\u2500 " : ""}
          {job.status}
          {job.error ? `: ${job.error}` : ""}
          {waitingForResume ? (
            <div style={{ color: "#7c2d12", fontSize: 12, lineHeight: 1.3 }}>
              Waiting for Resume all
            </div>
          ) : null}
        </td>
        <td title={job.id}>
          <code>{job.item_id ? job.item_id.slice(0, 8) : job.id.slice(0, 8)}</code>
        </td>
        <td style={{ minWidth: 260, maxWidth: 420 }}>
          <div style={{ fontWeight: 600, overflowWrap: "anywhere", lineHeight: 1.3 }}>
            {jobContext?.label ?? "-"}
          </div>
          {jobContext?.detail ? (
            <div style={{ color: "#4b5563", fontSize: 12, lineHeight: 1.3, wordBreak: "break-word" }}>
              {jobContext.detail}
            </div>
          ) : null}
        </td>
        <td>{job.job_type}</td>
        <td>{renderJobProgress(job, itemOutputs)}</td>
        <td>{formatTs(job.created_at_ms)}</td>
        <td>{formatTs(job.started_at_ms)}</td>
        <td>{formatTs(job.finished_at_ms)}</td>
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

      {error ? <div className="error">{error}</div> : null}
      {notice ? <div className="card">{notice}</div> : null}

      <div className="card">
        <h2>Queue controls</h2>
        <div className="row">
          <button type="button" disabled={busy} onClick={() => refresh()}>
            Refresh
          </button>
          <button
            type="button"
            disabled={busy}
            onClick={() => setPauseAll(!queuePaused)}
          >
            {queuePaused ? "Resume all" : "Pause all"}
          </button>
          <button type="button" disabled={busy || !hasActive} onClick={cancelAll}>
            Cancel all active
          </button>
          <button type="button" disabled={busy} onClick={flushCache}>
            Clean up old jobs and logs
          </button>
        </div>
        <div style={{ color: "#4b5563", marginTop: 8 }}>
          {queuePaused
            ? "Queue paused — queued jobs will not start until resumed. Running jobs continue."
            : "Queue running — jobs are being processed normally."}
        </div>
        <details style={{ marginTop: 8 }}>
          <summary style={{ cursor: "pointer", color: "#4b5563", fontSize: 13 }}>Developer tools</summary>
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
        <div style={{ color: "#4b5563", marginTop: 8, fontSize: 12 }}>
          Developer tools only contains the synthetic test job. The main queue below is where real archive and localization work appears.
        </div>
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span>Concurrency</span>
            <input
              type="number"
              min={1}
              max={16}
              value={maxConcurrency}
              disabled={busy}
              onChange={(e) => setMaxConcurrency(Number(e.currentTarget.value))}
              style={{ width: 90 }}
            />
          </label>
          <button type="button" disabled={busy} onClick={applyConcurrency}>
            Apply concurrency
          </button>
        </div>
        <div style={{ color: "#4b5563", marginTop: 8, fontSize: 12 }}>
          Concurrency is the number of jobs that may run in parallel. Retry creates new queued work;
          it does not cancel older running jobs.
        </div>
      </div>

      {selectedJobDetail ? (
        <div className="card">
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
                  <th>Truth</th>
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
                      <div style={{ color: "#4b5563", fontSize: 12 }}>
                        {attempt.is_current_attempt ? "Current truth" : "Historical attempt"} / {attempt.lineage_kind}
                      </div>
                    </td>
                    <td>
                      <div>Job <code>{attempt.job.id.slice(0, 8)}</code></div>
                      {attempt.job.batch_id ? <div>Batch <code>{attempt.job.batch_id.slice(0, 8)}</code></div> : null}
                      {attempt.job.retry_of_job_id ? <div>Retry of <code>{attempt.job.retry_of_job_id.slice(0, 8)}</code></div> : null}
                      {attempt.job.retry_replacement_job_id ? <div>Replaced by <code>{attempt.job.retry_replacement_job_id.slice(0, 8)}</code></div> : null}
                    </td>
                    <td style={{ minWidth: 260, maxWidth: 460, overflowWrap: "anywhere" }}>
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
        </div>
      ) : null}

      <div className="card">
        <h2>Queue</h2>
        <form className="row" style={{ marginTop: 0 }} onSubmit={applyJobSearch}>
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
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span>Filter</span>
            <select value={jobsFilter} onChange={(event) => setJobsFilter(event.currentTarget.value as JobsFilter)}>
              <option value="all">All loaded</option>
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
            Delete failed/canceled shown ({terminalShownCount})
          </button>
        </form>
        <div style={{ color: "#4b5563", marginTop: 6, fontSize: 12 }}>
          {jobSearchQuery.trim()
            ? `Showing up to ${JOBS_SEARCH_LIMIT} matching jobs; filter applies to loaded rows.`
            : `Showing latest ${JOBS_PAGE_REFRESH_LIMIT} jobs; filter applies to loaded rows.`}
        </div>
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Status</th>
                <th>ID</th>
                <th>Target</th>
                <th>Type</th>
                <th>Progress</th>
                <th>Created</th>
                <th>Started</th>
                <th>Finished</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {groupedJobs.length ? (
                groupedJobs.map((group) => {
                  if (group.jobs.length === 1) {
                    return renderJobRow(group.jobs[0], false);
                  }

                  const expanded = expandedGroups[group.key] === true;
                  const status = summarizeGroupStatus(group.jobs);
                  const progress = summarizeGroupProgress(group.jobs);
                  const activeCount = group.jobs.filter((job) => isActive(job.status)).length;
                  const retryableCount = group.jobs.filter(
                    (job) => isRetryable(job.status),
                  ).length;
                  const canonicalDetail = group.batchId ? batchDetailsById[group.batchId] : null;
                  const health = canonicalDetail?.health ?? null;
                  const displayedStatus = health ? summarizeBatchTargetStatus(health) : status;
                  const displayedProgress = health ? summarizeBatchTargetProgress(health) : progress;
                  const batchRetryableCount = health ? health.retryable_targets : retryableCount;
                  const waitingForResume = queuePaused && status === "queued" && activeCount > 0;
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
                          {displayedStatus}{" "}
                          {health
                            ? `(${health.succeeded_targets}/${health.canonical_targets} videos downloaded)`
                            : `(${finishedCount}/${group.jobs.length} done)`}
                          {waitingForResume ? (
                            <div style={{ color: "#7c2d12", fontSize: 12, lineHeight: 1.3 }}>
                              Waiting for Resume all
                            </div>
                          ) : null}
                        </td>
                        <td title={group.batchId ?? group.key}>
                          <code>{(group.batchId ?? group.key).slice(0, 8)}</code>
                        </td>
                        <td style={{ minWidth: 260, maxWidth: 420 }}>
                          <div style={{ fontWeight: 600, overflowWrap: "anywhere", lineHeight: 1.3 }}>
                            {summarizeJobGroupTargets(group.jobs, jobContexts)}
                          </div>
                          <div style={{ color: "#4b5563", fontSize: 12 }}>
                            {health
                              ? batchTargetHealthText(health)
                              : `${group.jobs.length} loaded job${group.jobs.length === 1 ? "" : "s"} in this batch`}
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
                            Canonical batch ID: <code>{group.batchId ?? "-"}</code>
                          </div>
                        </td>
                        <td>{summarizeGroupType(group.jobs)}</td>
                        <td>{Math.round(displayedProgress * 100)}%</td>
                        <td>{formatTs(summarizeCreatedTs(group.jobs))}</td>
                        <td>{formatTs(summarizeStartedTs(group.jobs))}</td>
                        <td>{formatTs(summarizeFinishedTs(group.jobs))}</td>
                        <td>
                          <div className="row" style={{ marginTop: 0 }}>
                            <button
                              type="button"
                              disabled={busy}
                              onClick={() =>
                                setExpandedGroups((prev) => ({
                                  ...prev,
                                  [group.key]: !expanded,
                                }))
                              }
                            >
                              {expanded ? "Collapse" : "Expand"} ({group.jobs.length})
                            </button>
                            <button
                              type="button"
                              disabled={busy || activeCount === 0}
                              onClick={() => cancelGroup(group)}
                            >
                              Cancel active ({activeCount})
                            </button>
                            <button
                              type="button"
                              disabled={busy || (group.batchId ? batchRetryableCount === 0 : retryableCount === 0)}
                              onClick={() => retryGroup(group)}
                            >
                              {group.batchId ? `Retry unresolved (${batchRetryableCount})` : `Retry failed (${retryableCount})`}
                            </button>
                            {group.batchId ? (
                              <>
                                <button type="button" disabled={busy} onClick={() => repairBatch(group.batchId ?? "")}>
                                  Repair batch
                                </button>
                                <button type="button" disabled={busy} onClick={() => backfillBatchTitles(group.batchId ?? "")}>
                                  Backfill titles
                                </button>
                                <button type="button" disabled={busy} onClick={() => exportUnresolvedBatch(group.batchId ?? "", "csv")}>
                                  Export unresolved CSV
                                </button>
                                <button type="button" disabled={busy} onClick={() => exportUnresolvedBatch(group.batchId ?? "", "urls")}>
                                  Copy unresolved URLs
                                </button>
                              </>
                            ) : null}
                            <button
                              type="button"
                              disabled={!groupLogPath}
                              onClick={() => openLogFile(groupLogPath)}
                            >
                              Reveal log
                            </button>
                          </div>
                        </td>
                      </tr>
                      {expanded ? group.jobs.map((job) => renderJobRow(job, true)) : null}
                    </Fragment>
                  );
                })
              ) : (
                <tr>
                  <td colSpan={9}>No jobs yet.</td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </div>
    </section>
  );
}
