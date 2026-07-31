# Work Packet: WP-0261 - Consumer process signaling + subscription progress observability

## Status

IN_PROGRESS (design complete + grounded; implementation this session, NO build)

## Owner

Claude (Opus 4.8)

## Operator Request Preserved

- 2026-07-01: "i still have no clue what is being processed after i pressed update all earlier. this should also need better signaling in both the subscription panel as the jobs/queue ... the subscription panel is supposed to be the front end so state/process with moving progressbar/numeration should be here too for the none technical consumer."
- 2026-07-01 (re the monitoring blind spot): "find a way to fix this and also to visually display it at the correct panel." (My trace monitoring could not see whether subscriptions were actually progressing — that data wasn't observable.)

## Intent

Make the subscription panel (the consumer front-end) show, live, WHAT is being processed after "Update all": which subscription is checking/downloading, and "X of Y this run" with a moving bar + the current video title — reusing the app's existing slow-poll cadence (no new fast timers; the app is intentionally conservative). Also make that progress OBSERVABLE to an external agent (trace events + a bridge endpoint) so headless monitoring can confirm progress, not just health.

## Research Basis

Read-only design investigation (subagent, 49 tool-uses). Verified: all needed data already exists in the DB; only a read/aggregate command + trace emission are missing.
- Active refresh subs: `jobs::active_youtube_subscription_refresh_ids` (jobs.rs:3852) — tells WHICH sub is enumerating, nothing about its download fan-out.
- Child downloads carry BOTH `params_json.subscription_id` AND `job.batch_id = refresh job_id` (jobs.rs:6842) — `batch_id` is the clean top-level join key. Per-job `status`/`progress`/`target_title` on `JobRow` (462-478).
- Downloaded=archive count (`youtube_subscriptions_archive_stats`, subscriptions.rs:2441); total/new/last_checked = v18 columns (WP-0255).
- THE GAP: no command aggregates per-subscription child-download state (queued/running/succeeded/failed + current title/progress); the engine writes NOTHING to `diagnostics_trace.jsonl` (only per-job `log_line`), so external agents can't see refresh progress.

## Scope

### 2a - Engine: `youtube_subscriptions_activity()` (subscriptions.rs + lib.rs command)
- Add `SubscriptionActivityRow { subscription_id, title, phase: "checking"|"downloading"|"idle", downloaded, total, queued, running, succeeded, failed, current_title, current_progress }` and `pub fn youtube_subscriptions_activity(paths) -> Result<Vec<SubscriptionActivityRow>>`.
- Read-only conn (`db::open_readonly`, matching jobs.rs:3855). Queries:
  - Step A: `SELECT id, params_json FROM job WHERE type='youtube_subscription_refresh_v1' AND status IN ('queued','running')` -> map subscription_id -> refresh job id (=batch_id). Sub with refresh active + zero children = phase "checking".
  - Step B: `SELECT batch_id, status, COUNT(*) FROM job WHERE type='download_direct_url' AND batch_id IN (...) GROUP BY batch_id, status` -> queued/running/succeeded/failed(+canceled). Any running/queued child => phase "downloading".
  - Step C: in-flight child per batch: `SELECT target_title, progress, params_json FROM job WHERE type='download_direct_url' AND batch_id=?1 AND status='running' ORDER BY started_at_ms DESC LIMIT 1`.
  - Step D: enrich with `archive_stats` (downloaded), `list_youtube_subscriptions` (title, upstream_total). Only emit rows for active subs (payload ~1 row given the limit-1 recurring lane).
- Register the Tauri command next to `youtube_subscriptions_active_refresh_ids` (lib.rs:7577-7594, list at 8989). Keep it read-only (do NOT `db::open`+migrate on this poll path).

### 2b - Frontend: live "Processing now" signaling (LibraryPage.tsx, no new timers)
- Add a deferred loader `refreshSubscriptionActivity` into the EXISTING visible+showVideoIngest deferred effect (1635-1647, same shape as `refreshArchiveStats`), into new state `subActivity: Record<string, SubscriptionActivityRow>`. Inherits the intentional slow cadence.
- "Processing now" line above the status strip (4294): `Checking aespa …` / `aespa — downloading 12/40 · <current_title>` with a moving `.sub-bar` fill. Numeration = (succeeded+running)/childTotal; the in-flight sub-bar uses current_progress. NO new card.
- Per-sub list row (4308-4348): when downloading, append live run state `… · ⏳ 12/40 this run` and make the bar reflect the RUN ratio (so it visibly moves). Distinguish phase checking vs downloading in `subscriptionRunState` (224-236).
- Detail pane (4355-4395): phase label + `Queued N · Running M · Done K · Failed F` + current item + mini progress. Extend `subscriptionOverview` (994-1005) "N updating now" to key off activity phase.

### 2c - Agent/trace observability (engine trace events + bridge endpoint)
- Engine-side trace writer resolving `effective_diagnostics_trace_dir` (paths.rs:192; honor the override), appending `DiagnosticsTraceEntry`-shaped rows. Emit best-effort from the refresh arm (jobs.rs:6693) at the points that already `log_line`: `subscription_refresh_begin` (6735), `subscription_refresh_enumerated` (6849, w/ upstream_total/new_found/queued/batch_id), `subscription_refresh_done` (6862/6818), `subscription_refresh_failed` (6884, level warn). Names slot next to existing `subscription_auto_sync`. Per-child progress stays OUT of the trace (would flood at pacing×N).
- Bridge endpoint `GET /agent/subscriptions_activity` (lib.rs:261-269 dispatch; AGENT_APP_HANDLE access like agent_handle_freeze_event 731-734) returning the activity JSON — works even if the WebView is frozen. Document in CLAUDE.md/AGENTS.md endpoint table.

### 2d - Jobs/Queue signaling (keep technical)
- Jobs stays the technical/diagnostic view; the WP-0256 origin labels + progress bars already improved it. Minor: ensure the batch progress reflects canonical health (already done). No consumer-facing rework here (that lives in the subscription panel per operator).

## Acceptance Criteria

- After "Update all", the subscription panel shows a live "Processing now" line with a moving bar + "X/Y this run" numeration + current video title, updating on the existing poll cadence.
- `youtube_subscriptions_activity` returns correct per-sub queued/running/succeeded/failed + current title/progress (unit-test the aggregation where practical).
- `diagnostics_trace.jsonl` gains `subscription_refresh_begin/enumerated/done/failed` rows; `GET /agent/subscriptions_activity` returns live activity.
- Read-only, off the writer path; no new fast timers; `cargo test` green; `tsc` clean. NOT built.

## Red-Team

- Batch-id linkage load-bearing: retried children must keep the batch (else undercount). Keep `subscription_id`-in-params fallback. ([VV-SOT-004] whole-set vs visible-subset.)
- `succeeded` is per-run, not lifetime: label "X/Y **this run**"; lifetime stays archive `downloaded` (avoids the very confusion WP-0255 targets).
- `job.progress` for downloads is coarse (few checkpoints): lead the banner with numeration (always advances), not the fractional bar.
- Read contention on the hot poll path: read-only conn only; keep payload ~1 row.
- Trace-dir override: engine writer must honor `config/diagnostics_trace_dir.txt`, not hardcode APPDATA, so it lands where vvwatch/freeze tooling reads.

## Notes

- 2026-07-01: authored from the signaling design investigation during the overnight autonomous overhaul. Fixes both the operator's "no clue what's processing" and the agent monitoring blind spot. Engine changes batched with WP-0259/0262; FE batched with WP-0259/0260. Validated with tsc + cargo, NOT built.
