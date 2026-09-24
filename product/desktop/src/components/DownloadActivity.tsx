import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { activityStage, activityTitle, formatBytes, transferPercent, type ActivityRow } from "../lib/downloadActivity";
import { classifyFailure } from "../lib/failureStates";
import { FailureExplainer } from "./FailureExplainer";
import "./DownloadActivity.css";

type Page = { jobs: ActivityRow[]; total: number; offset: number; limit: number; has_more: boolean };
export function DownloadActivity({ source = "all", visible = true, compact = false, gateText = null }: { source?: string; visible?: boolean; compact?: boolean; gateText?: string | null }) {
  const [selectedSource, setSource] = useState(source);
  const [view, setView] = useState("now");
  const [offset, setOffset] = useState(0);
  const [page, setPage] = useState<Page | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [paused, setPaused] = useState(false);
  const [busy, setBusy] = useState(false);
  const [revision, setRevision] = useState(0);
  const [showOptions, setShowOptions] = useState(false);
  const generation = useRef(0);
  useEffect(() => { setSource(source); setOffset(0); }, [source]);
  useEffect(() => {
    const current = ++generation.current;
    setPage(null);
    if (!visible) return;
    let stopped = false;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      try {
        const next = await invoke<Page>("jobs_activity_page", { source: selectedSource, view, offset, limit: compact ? 2 : 20 });
        const control = await invoke<{ paused: boolean }>("jobs_queue_control_get");
        if (!stopped && generation.current === current && offset > 0 && next.jobs.length === 0) { setOffset(0); return; }
        if (!stopped && generation.current === current) { setPage(next); setPaused(control.paused); setError(null); }
      } catch (e) { if (!stopped && generation.current === current) setError(String(e)); }
      if (!stopped) timer = setTimeout(poll, view === "now" ? 2500 : 15000);
    };
    void poll();
    return () => { stopped = true; clearTimeout(timer); };
  }, [visible, selectedSource, view, offset, revision, compact]);
  async function action(command: string, args: Record<string, unknown>) {
    setBusy(true);
    try { await invoke(command, args); setRevision(v => v + 1); }
    catch (e) { setError(String(e)); }
    finally { setBusy(false); }
  }
  return <section className={`download-activity${compact ? " activity-compact" : ""}`} aria-label="Download activity" data-testid={`download-activity-${source}`}>
    <div className="activity-toolbar">
      <h2>{compact ? "Activity" : "Jobs / Queue"}</h2>
      {compact && <><span>{page ? `${page.total} ${view === "now" ? "running or waiting" : "attempts"}` : "Reading activity…"}</span><button type="button" aria-expanded={showOptions} data-agent-safe-action="true" onClick={() => setShowOptions(v=>!v)}>Views</button></>}
      {(!compact || showOptions) && <nav aria-label="Activity views">{[["now", "Downloading & waiting"], ["attention", "Needs attention"], ["history", "History"]].map(([id, label]) =>
        <button key={id} type="button" aria-pressed={view === id} data-agent-action-id={`activity.view.${id}`} data-agent-effect-class="read_only" onClick={() => { setView(id); setOffset(0); }}>{label}</button>)}</nav>}
      {!compact && <label>Source <select aria-label="Activity source" data-agent-action-id="activity.source" data-agent-effect-class="read_only" data-agent-input-kind="select" value={selectedSource} onChange={e => { setSource(e.target.value); setOffset(0); }}>
        {[["all","All"],["youtube","YouTube"],["instagram","Instagram"],["tiktok","TikTok"],["images","Images"],["other","Other websites"],["localization","Localization"]].map(([id,label])=><option value={id} key={id}>{label}</option>)}
      </select></label>}
      {!compact && <button type="button" disabled={busy} onClick={() => void action("jobs_queue_control_set", { paused: !paused })}>{paused ? "Resume queue" : "Pause new starts"}</button>}
    </div>
    {paused && <p role="status">Queue paused. Downloads already running can finish; waiting work starts after Resume queue.</p>}
    {!compact && view === "now" && gateText && <p role="status" className="activity-gate">{gateText}</p>}
    {error && <div role="alert">Activity could not refresh. <button type="button" onClick={() => setRevision(v=>v+1)}>Try again</button><details><summary>Technical details</summary>{error}</details></div>}
    {!compact && !page && !error && <p role="status">Reading activity…</p>}
    {!compact && page?.jobs.length === 0 && <p className="activity-empty">{view === "now" ? "No downloads running or waiting." : view === "attention" ? "No failed attempts awaiting a retry." : "No history for this source."}</p>}
    <div className="activity-list">{page?.jobs.map(row => {
      const { job, live, quality } = row;
      const percent = transferPercent(live);
      const transferQuietSeconds = live ? Math.max(0, Math.floor((Date.now() - (live.transfer_updated_at_ms || live.updated_at_ms)) / 1000)) : 0;
      return <article className="activity-row" key={job.id} data-testid={`activity-job-${job.id}`}>
        <div className="activity-description"><strong>{activityTitle(row)}</strong><span>{activityStage(row)}</span>
          {quality && <small>{quality.width} × {quality.height}{quality.fps ? ` · ${quality.fps} fps` : ""} · {quality.state === "verified" ? "MKV quality verified" : "Selected source"}
            {quality.subtitles.length ? ` · Captions: ${quality.subtitles.map(s => `${s.language} (${s.kind})`).join(", ")}` : " · No captions supplied by source"}
            {!quality.english_available && quality.subtitles.length > 0 ? " · English unavailable" : ""}
            {quality.original_available === false && quality.subtitles.length > 0 ? " · Original-language captions unavailable" : ""}
          </small>}
          {job.error && (job.status === "failed" || job.status === "canceled") && (
            <FailureExplainer
              failure={classifyFailure(job.error)}
              rawMessage={job.error}
              actionHandlers={{ retry_now: () => void action("jobs_retry", { jobId: job.id }) }}
              disabled={busy}
            />
          )}
          {job.status === "queued" && <small>{paused ? "Queue paused" : gateText ? `Queued — ${gateText}` : "Waiting for the scheduler; no transfer has started."}</small>}
        </div>
        <div className="activity-progress">
          {job.status === "running" && <><div className={`activity-bar ${percent == null ? "is-indeterminate" : ""}`} role="progressbar" aria-label={`${activityTitle(row)} transfer`} aria-valuemin={0} aria-valuemax={100} aria-valuenow={percent ?? undefined}><span style={{width:percent == null ? "35%" : `${percent}%`}}/></div>{percent != null && <span>{percent.toFixed(1)}% of this transfer</span>}</>}
          {live?.phase === "downloading" && <small>{live.downloaded_bytes != null ? formatBytes(live.downloaded_bytes) : ""}{live.total_bytes != null ? ` / ${formatBytes(live.total_bytes)}` : ""}{transferQuietSeconds >= 15 ? ` · No transfer update for ${transferQuietSeconds}s` : <>{live.speed != null ? ` · ${formatBytes(live.speed)}/s` : ""}{live.eta != null ? ` · about ${Math.ceil(live.eta)}s left` : ""}</>}</small>}
        </div>
        <div className="activity-actions"><button type="button" aria-expanded={expanded === job.id} data-agent-action-id={`activity.details.${job.id}`} data-agent-effect-class="read_only" onClick={() => setExpanded(expanded === job.id ? null : job.id)}>Details</button>
          {(job.status === "running" || job.status === "queued") && <button type="button" disabled={busy} onClick={() => void action("jobs_cancel", { jobId: job.id })}>Stop</button>}
          {/* WP-0321 S6: downloads reopen the same row on retry, so retry_replacement_job_id
              stays null and no longer gates this button; other job types were the only ones
              that ever populated it, and cannot reach this download-focused panel.
              WP-0322: the Retry action now renders inline via FailureExplainer below (avoids a
              duplicate button) except when there is no stored error to classify. */}
          {(job.status === "failed" || job.status === "canceled") && !job.error && (
            <button type="button" disabled={busy} onClick={() => void action("jobs_retry", { jobId: job.id })}>Retry</button>
          )}
        </div>
        {expanded === job.id && <div className="activity-detail">
          <h3>Live activity</h3>
          {live ? <><p>Last update {new Date(live.updated_at_ms).toLocaleTimeString()}. Transfer completion is followed by saving and verification.</p><pre>{live.lines.map(line => line.replace(/^(\d+)  /, (_, ts) => `${new Date(Number(ts)).toLocaleTimeString()}  `)).join("\n")}</pre></> : <p>{job.status === "running" ? "This attempt has not supplied live downloader detail in this app session." : "Live output is available while this attempt is running."}</p>}
          <details><summary>Technical details</summary><p>Job {job.id} · {job.job_type}</p>{job.error && <pre>{job.error}</pre>}</details>
        </div>}
      </article>;
    })}</div>
    {page && page.total > 0 && (!compact || showOptions) && <div className="activity-pagination"><span>Showing {page.offset + 1}–{page.offset + page.jobs.length} of {page.total} attempts</span><button type="button" disabled={!offset} data-agent-safe-action="true" onClick={() => setOffset(v => Math.max(0, v - page.limit))}>Previous</button><button type="button" disabled={!page.has_more} data-agent-safe-action="true" onClick={() => setOffset(v=>v+page.limit)}>Next</button></div>}
  </section>;
}
