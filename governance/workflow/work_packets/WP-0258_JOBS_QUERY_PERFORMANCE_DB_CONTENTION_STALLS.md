# Work Packet: WP-0258 - Jobs/Queue query performance + DB-contention stall elimination

## Status

SUPERSEDED by WP-0280 (0.1.96 shipped the v2 bounded read-path slice; unresolved p95, retry/repair, render-bound, and contention acceptance moved intact)

## Owner

Claude (Opus 4.8)

## Operator Request Preserved

- 2026-07-01: "the app responsiveness is much better as in the past. it still freezes from time to time ... my pc is under constant strain because i am building apps 24/7. but does the connections to my nas and the massive library cause this?"
- 2026-07-01 (follow-up): "yes prepare a follow up wp, let the build finish, i will do a visual inspection and tell you if changes are needed. then we can proceed with the new wp and the possible new proposed changes."

## Evidence / Root Cause (from THIS machine's live trace)

Read-only analysis of `%APPDATA%\com.voxvulgi.voxvulgi\diagnostics\traces\diagnostics_trace.jsonl` (93 MB, written 2026-07-01) + the latest `external_watch` run. The remaining "freezes" are NOT JS-event-loop hangs and NOT NAS SMB stalls — they are the UI waiting on slow database commands:

- Last 20,000 trace rows: `freeze_detected` = **0**, `event_loop_skew` = **0**, `worker_alive` ticking normally → the JS UI thread is alive; the stalls are command waits, not a hard hang.
- `command_slow` = **1,332**; `database_locked` = **109**. Dominated by the Jobs subsystem:

  | command | count | max ms | avg ms | conn |
  |---|---|---|---|---|
  | `jobs_retry_batch_failed` | 1 | 197,408 | 197,408 | `db::open` (WRITE), jobs.rs:4564 |
  | `jobs_list` | 253 | 121,169 | 12,829 | `db::open_readonly`, jobs.rs:3622 |
  | `jobs_batch_detail` (`get_batch_detail`) | 972 | 112,114 | 7,099 | `db::open_readonly`, jobs.rs:5229 |
  | `jobs_retry_batch_failed_dry_run` | 1 | 71,454 | 71,454 | read-only |
  | `youtube_subscriptions_archive_stats` | 4 | 42,925 | 23,414 | filesystem reads |
  | `youtube_subscriptions_list` | 5 | 9,262 | 2,768 | read-only |

- `database_locked` by command: **`jobs_batch_detail` = 92 / 109**, `jobs_list` = 4, others 1 each.
- The sampled 26.7 s `jobs_batch_detail` stall had `cpu_percent = 0.0` → the process was WAITING on a lock/IO, not computing.
- External watch (v0.1.67 run): 0 not-responding, 0 bridge failures, **33/45 heavy-child-process** samples, **14/45 DB-timeout** samples, **2/45 NAS path-timeout** samples.

### Conclusion (answers the operator's question)
- **Massive library / job history = primary cause**, specifically the **Jobs/Queue queries** contending on the local SQLite DB — not the library row count (WP-0253 already indexed `library_item`).
- **NAS = minor/occasional** — the DB is on local C:, there are 0 event_loop_skews and only 2/45 NAS path timeouts. NAS contributes only when a command touches a UNC path (e.g. `archive_stats`).
- The idle/background **WebView2 occlusion** freeze was a separate, already-fixed class (WP-0250, in 0.1.80) — why responsiveness is "much better than past."
- The 24/7 build load (33/45 heavy child processes) amplifies every DB lock wait.

### Why read-only conns are not enough (the real mechanisms)
1. **Call volume**: `get_batch_detail` aggregates over every job in a batch and is called per-visible-batch on every Jobs poll (972 calls). Even read-only, hundreds of repeated aggregations over a large `job` table contend.
2. **Short read-only busy timeout**: `READ_ONLY_BUSY_TIMEOUT_MS = 750` (db.rs:7) — a WAL checkpoint or a long writer transaction holding the lock >750 ms makes a read-only command fail with `database is locked` (92× on batch detail).
3. **Write-connection blocking ops on the UI path**: `retry_failed_jobs_for_batch` / `repair_batch` use `db::open` (write) and ran 197 s, contending with the job runner's writes.
4. **spawn_blocking saturation**: under 33/45 heavy-child-process load, Tauri `spawn_blocking` command threads + the runner's per-job worker threads oversubscribe CPU, so even cheap queries wait in the pool for seconds (wall-clock slow with cpu 0%).
5. **Filesystem hot path**: `youtube_subscriptions_archive_stats` reads per-subscription yt-dlp archive files (up to 43 s) and is polled by the subscription list (now also feeding the WP-0255 manager's progress) — should be cached off the hot path.

## Intent

Eliminate the multi-second-to-multi-minute UI stalls under load by cutting Jobs query volume + cost and removing the lock-contention/queue-saturation mechanisms above — without changing job correctness, retry lineage, or batch health truth (VV-SOT). Local, additive, measurable against the trace.

## Scope (candidate fixes — confirm each against the code before implementing)

### 2a - Cut `jobs_batch_detail` volume + cache health
- Only call `get_batch_detail` for **expanded** batches (and on demand), not for every visible batch on every poll. Memoize `JobBatchHealthSummary` per `batch_id` with a short TTL / invalidate-on-change so a poll reuses the last computed health instead of recomputing (972→ a handful). Frontend (`JobsPage.tsx`) + possibly a lighter engine "batch health since version" path.
- Confirm `idx` coverage for the batch-health aggregation query (`WHERE batch_id=? GROUP BY ...`); add an index if the plan is a scan.

### 2b - `jobs_list` polling + cost
- Verify the Jobs page poll cadence (WP-0127 visibility-aware) and lengthen / event-drive it; ensure `list_jobs(limit, offset)` is tightly bounded and its query plan is indexed (no full `job` scan). Avoid re-fetching the full list when only a few rows changed.

### 2c - DB busy-timeout + checkpoint tuning
- Raise `READ_ONLY_BUSY_TIMEOUT_MS` (750 ms) to a value that rides out a checkpoint (e.g. 3–5 s) so reads wait instead of erroring with `database is locked`, and/or tune `wal_autocheckpoint` + run `wal_checkpoint(PASSIVE)` off the hot path. Confirm WAL settings (db.rs already sets WAL + synchronous=NORMAL, WP-0223).

### 2d - Make write-path batch ops non-blocking
- `retry_failed_jobs_for_batch` / `repair_batch` (write conn) must not block the UI for minutes: enqueue the retry work and return fast, or bound + chunk the write transaction so it doesn't hold the lock (and doesn't starve reads). Keep retry lineage correctness (WP-0248).

### 2e - `archive_stats` off the hot path
- Cache per-subscription downloaded counts (filesystem archive id count) and refresh on a slow timer / on refresh-completion, instead of a synchronous read on every subscription-list poll. Relevant now that WP-0255's manager shows the count.

### 2f - spawn_blocking / runner thread budget
- Investigate whether the runner's worker threads + command `spawn_blocking` pool oversubscribe under heavy external load; consider a bounded DB-command executor or a dedicated read pool so a burst of slow commands can't queue the cheap ones behind them.

### 2g - Subscription download throughput: decouple download dispatch from enumeration pacing (LIVE BUG, 2026-07-01)
- Evidence (live DB, read-only): **12,658 `download_direct_url` jobs queued, ALL lane=recurring**, plus 256 queued refreshes, with the recurring lane throttled to ~1 dispatch / 45s by the WP-0257 anti-bot cooldown (`antibot recurring_min_interval_secs=45`). Downloads made **zero** progress in an 8-minute monitor window while the queue grew; at 1/45s a 12,658-item backlog drains in ~6.6 DAYS. The queue is NOT paused (`jobs_queue_paused=0`, `jobs_recurring_paused=0`) — it is over-throttled.
- Root cause: the 45s recurring-lane cooldown is correct for **channel enumeration** (the anti-bot-sensitive step that caused the WP-0257 cookie cascade) but is ALSO gating the **actual video downloads**, which are normal traffic and already paced by the per-video yt-dlp sleep knobs.
- Fix: gate the recurring inter-dispatch cooldown on **enumeration/refresh** jobs only (`youtube_subscription_refresh_v1`), and let subscription-child `download_direct_url` jobs dispatch without the 45s gate (e.g. a separate faster sub-lane for subscription downloads, or exclude download_direct_url from the cooldown while keeping recurring concurrency conservative). Preserve anti-bot safety on enumeration; do NOT advise lowering the global anti-bot interval (that re-risks the auth-block cascade). Runner logic in `jobs.rs` (runner_loop + the WP-0257 cooldown). Implement AFTER the in-flight 0.1.82 workflow (jobs.rs is being edited).

Out of scope: NAS resync/fallback (WP-0253, done), localization perf, schema redesign of the job/batch model, anything that alters retry/batch-health truth.

## Research Basis

- Live freeze evidence above (read-only trace + watch analysis, 2026-07-01). Strong, machine-specific.
- Code grounding: `list_jobs` jobs.rs:3620-3645 (read-only, paginated); `get_batch_detail` jobs.rs:5228 (read-only); `retry_failed_jobs_for_batch` jobs.rs:4553-4564 (write); `READ_ONLY_BUSY_TIMEOUT_MS=750` db.rs:7; WAL+synchronous=NORMAL db.rs:65-71.
- Lineage to build on (do not regress): WP-0223 (WAL/synchronous), WP-0224 (read-only UI conns), WP-0243/0244/0245 (DB-contention containment, batched library/item commands), WP-0127 (visibility-aware polling), WP-0248 (retry lineage + batch health truth), WP-0253 (library indexes + NAS fallback).
- Before implementing each item, inspect the exact command implementation + run `EXPLAIN QUERY PLAN` on the hot queries against a copy of the real DB (per the project's research-first rule).

## Acceptance Criteria

- On the Jobs page under normal use: `jobs_list` and `jobs_batch_detail` p95 wall-clock drop below a target (e.g. < 1 s) and `database_locked` events on Jobs commands go to ~0 over a representative session — proven by a fresh `diagnostics_trace.jsonl` window (same method as the evidence above), not by claim.
- `get_batch_detail` call count per Jobs poll is bounded to expanded batches (verified in trace).
- A batch retry no longer blocks the UI for minutes (returns fast; work proceeds in the runner).
- No regression to retry lineage, canonical batch health counts, or video-title truth (`cargo test` green; the WP-0248 batch-health tests still pass).
- No user job/library/subscription data deleted or reset.

## Red-Team

- Caching batch health risks showing stale counts: use change/version invalidation (e.g. max(updated/finished) per batch) so the cache refreshes when the batch actually changes; never cache across a retry/repair.
- Raising the read-only busy timeout could make a genuinely wedged DB hang a read longer: keep it bounded (single-digit seconds) + keep the existing stale-lock banner/recovery (WP-0248).
- Lengthening poll cadence could make Jobs feel stale: pair with an explicit Refresh + event-driven invalidation on job state changes.
- Async retry could double-enqueue if the operator clicks twice: idempotency guard on the batch retry (dedupe by batch_id + in-flight marker).
- Measuring "faster" without proof is a VV-SOT violation: every acceptance claim must cite a fresh trace window, not a single hand-timed run.

## Notes

- 2026-07-01: WP authored from live freeze evidence as the operator-requested follow-up to the WP-0255/WP-0256 build (0.1.81). Implementation gated on operator go-ahead after the 0.1.81 visual inspection. This WP is the performance/stall lineage; WP-0255/0256 were readability/visibility and do not themselves fix these stalls.
- 2026-07-16: Operator explicitly requested a Jobs rebuild after the exact YouTube single attempt reported queued but Jobs rendered empty. Reactivated for the v2 bounded-overview/read-path work in `WP-0258_JOBS_QUERY_PERFORMANCE_DB_CONTENTION_STALLS_v2_REFINEMENT.md`.
- 2026-07-16: Desktop 0.1.96 shipped requested-view projections, indexed canonical totals and exact URL lookup, explicit loading state, and zero collapsed-row `jobs_batch_detail` calls in the final trace. WP remains IN_PROGRESS because the representative 20-call `jobs_overview` p95 was 2,520 ms and retry/repair write paths were not re-proven. Evidence: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0258/20260716_jobs_rebuild_v2/summary.md`.
- 2026-07-27: Superseded by `WP-0280_VIDEO_ARCHIVER_JOBS_COHESIVE_WORKSPACES_v1.md`. The shipped bounded read path and its proof remain preserved; all unresolved performance, retry/repair, archive-stat, contention, and verification scope is carried forward into the consolidated packet.
