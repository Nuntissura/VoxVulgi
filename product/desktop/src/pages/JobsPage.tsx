import { Fragment, useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { confirm } from "@tauri-apps/plugin-dialog";

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

type JobFlushSummary = {
  removed_jobs: number;
  removed_log_files: number;
  removed_artifact_dirs: number;
  removed_output_dirs: number;
  removed_cache_entries: number;
};

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

function summarizeGroupType(jobs: JobRow[]): string {
  const unique = Array.from(new Set(jobs.map((job) => job.job_type)));
  if (!unique.length) return "-";
  if (unique.length === 1) return `${unique[0]} batch`;
  return "mixed batch";
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

export function JobsPage() {
  const [jobs, setJobs] = useState<JobRow[]>([]);
  const [expandedGroups, setExpandedGroups] = useState<Record<string, boolean>>({});
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [dummySeconds, setDummySeconds] = useState(10);
  const [queuePaused, setQueuePaused] = useState(false);
  const [maxConcurrency, setMaxConcurrency] = useState(4);

  const refresh = useCallback(async () => {
    const [next, control, runtime] = await Promise.all([
      invoke<JobRow[]>("jobs_list", { limit: 200, offset: 0 }),
      invoke<JobQueueControlState>("jobs_queue_control_get"),
      invoke<JobRuntimeSettings>("jobs_runtime_settings_get"),
    ]);
    setJobs(next);
    setQueuePaused(control.paused);
    setMaxConcurrency(runtime.max_concurrency);
  }, []);

  useEffect(() => {
    refresh().catch((e) => setError(String(e)));
  }, [refresh]);

  const hasActive = useMemo(
    () => jobs.some((job) => isActive(job.status)),
    [jobs],
  );

  const groupedJobs = useMemo(() => {
    const byKey = new Map<string, JobGroup>();
    const groups: JobGroup[] = [];

    for (const job of jobs) {
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
  }, [jobs]);

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
    if (!hasActive) return;
    const id = setInterval(() => {
      refresh().catch(() => undefined);
    }, 1000);
    return () => clearInterval(id);
  }, [hasActive, refresh]);

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
      await invoke("jobs_retry", { jobId: normalized, job_id: normalized });
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function retryGroup(group: JobGroup) {
    const retryableIds = group.jobs
      .filter((job) => job.status === "failed" || job.status === "canceled")
      .map((job) => job.id);
    if (!retryableIds.length) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await Promise.all(
        retryableIds.map((jobId) =>
          invoke("jobs_retry", { jobId, job_id: jobId }),
        ),
      );
      setNotice(`Retried ${retryableIds.length} job${retryableIds.length === 1 ? "" : "s"} in batch.`);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function revealLogs(path: string) {
    setError(null);
    try {
      await revealItemInDir(path);
    } catch (e) {
      setError(String(e));
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
    const ok = await confirm(
      "Flush finished/failed/canceled jobs and clear cache/log artifacts? Active jobs are kept.",
      {
        title: "Flush cache",
        kind: "warning",
      },
    );
    if (!ok) return;

    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const summary = await invoke<JobFlushSummary>("jobs_flush_cache");
      setNotice(
        `Flushed ${summary.removed_jobs} jobs, ${summary.removed_log_files} log files, ${summary.removed_artifact_dirs} artifact folders, ${summary.removed_output_dirs} output folders, ${summary.removed_cache_entries} cache entries.`,
      );
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
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
    return (
      <tr key={job.id} className={nested ? "batch-child-row" : undefined}>
        <td>
          {nested ? "sub " : ""}
          {job.status}
          {job.error ? `: ${job.error}` : ""}
        </td>
        <td title={job.id}>
          <code>{job.id.slice(0, 8)}</code>
        </td>
        <td>{job.job_type}</td>
        <td>{Math.round((job.progress ?? 0) * 100)}%</td>
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
              disabled={busy || job.status !== "failed"}
              onClick={() => retry(job.id)}
            >
              Retry
            </button>
            <button
              type="button"
              disabled={!job.logs_path}
              onClick={() => revealLogs(job.logs_path)}
            >
              Reveal log
            </button>
          </div>
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
        <h2>Enqueue</h2>
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span>Dummy seconds</span>
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
            Enqueue dummy job
          </button>
          <button type="button" disabled={busy} onClick={() => refresh()}>
            Refresh
          </button>
        </div>
        <div className="row">
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
            Flush cache/history
          </button>
        </div>
        <div style={{ color: "#4b5563", marginTop: 8 }}>
          Queue state: {queuePaused ? "paused" : "running"}
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
      </div>

      <div className="card">
        <h2>Queue</h2>
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Status</th>
                <th>ID</th>
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
                    (job) => job.status === "failed" || job.status === "canceled",
                  ).length;
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
                          {status} ({finishedCount}/{group.jobs.length} done)
                        </td>
                        <td title={group.batchId ?? group.key}>
                          <code>{(group.batchId ?? group.key).slice(0, 8)}</code>
                        </td>
                        <td>{summarizeGroupType(group.jobs)}</td>
                        <td>{Math.round(progress * 100)}%</td>
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
                              disabled={busy || retryableCount === 0}
                              onClick={() => retryGroup(group)}
                            >
                              Retry failed ({retryableCount})
                            </button>
                            <button
                              type="button"
                              disabled={!groupLogPath}
                              onClick={() => revealLogs(groupLogPath)}
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
                  <td colSpan={8}>No jobs yet.</td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </div>
    </section>
  );
}
