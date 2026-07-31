# Work Packet: WP-0256 - Jobs/Queue readable rows + source linkage + real progress

## Status

DONE

## Owner

Claude (Opus 4.8)

## Note on this file

The WP-0256 row has existed in `TASK_BOARD.md` (IN_PROGRESS, partially shipped in desktop 0.1.75+) but the WP **file** was never created. This file is authored now to resolve that drift and capture the 2026-06-30/07-01 operator scope. Prior shipped work is preserved below.

## Operator Request Preserved

- 2026-06-15 (original, repeated): "jobs should show the video title ... same for playlists/chanels subscriptions ... same when i collapse a playlist/chanel subscription job to inspect the downloaded videos." Clarification: "i do not mean cards, the current list is messy. i hate cards so do not introduce it."
- 2026-06-30/07-01 (this session): "the jobs/queue panel is even worse. this is totally unclear what is happening. there is zero logic to me. i want to know what job is getting done, sure i see video names. but does it belong to a playlist/subscription, to a single video, something else. i never ever have a single idea what is happening in the app. what the status of something is, i want also a better visual for progression of items."

## Prior Shipped State (preserve - do not regress)

Shipped in desktop 0.1.75+ under this WP id:
- Playlist/channel/subscription **child download jobs show the real video title** in Jobs across all states — captured free from the yt-dlp `--flat-playlist` enumeration (`expand_yt_dlp_entries` returns (url,title); `stamp_job_target_titles_by_url` fills each child job's `target_title`; no extra yt-dlp calls). Engine 222 tests pass.
- WP-0257 #6 also added `classifyJobError` (JobsPage.tsx, display-only) → plain-language failure headlines (auth / content / network / tool / interrupted) with raw error on hover + Copy.

## Intent

Make the Jobs/Queue page answer, for every row, "what is this, where did it come from, and how far along is it" — by (1) labeling each job/batch with its **source** (this subscription/playlist/channel vs a one-off single vs Instagram/image), (2) showing **real progress** (bars, downloaded/total), and (3) making the dense rows + banner readable as a clean list (NO cards, per `build_rules.md` and the operator's explicit hatred of cards). Build on the existing data — linkage already exists in job params, progress already exists per job + per batch.

## Scope (this session)

### 2a - Source linkage "belongs to subscription X" (frontend: `archiverRuntime.ts`, `JobsPage.tsx`)
- Root cause (confirmed): child `download_direct_url` jobs already carry `params_json.subscription_id`, and JobsPage already loads `youtubeSubscriptionsById` (JobsPage.tsx:569-573), but `buildJobContextSummary`'s `download_direct_url` branch (`archiverRuntime.ts` ~294-311) reads only target_title/url/output_dir and never reads `subscription_id` — only the refresh-parent branch surfaces the subscription. So download batches show as a generic "download_direct_url batch" with no owner.
- Fix (no schema change): in the `download_direct_url` branch, read `params.subscription_id`, look up the subscription title, and label the job/batch as belonging to that subscription/playlist/channel. Distinguish origin in the UI: **Subscription/Playlist/Channel** (has subscription_id) vs **Single video** (one-off, no subscription_id) vs **Instagram/Image** (by job_type). Replace the cosmetic "<type> batch" Type cell with a human origin label + the source title.
- Optional engine follow-up (deferred unless cheap): expose `subscription_id`/origin on the `JobRow` projection so the UI need not re-parse params. Not required for this session if the params parse is sufficient.

### 2b - Real progress visuals (frontend: `JobsPage.tsx`)
- Per-job: render `job.progress` (0..1, already a column) as a progress bar, not bare "0%".
- Per-batch: use the existing `JobBatchHealthSummary` (canonical_targets / succeeded_targets / unresolved_targets / active_targets) to show a "downloaded X / Y" bar for the batch + counts (queued/running/failed) — data already computed by `jobs_batch_detail`/health, currently shown only as terse text.
- Status pills with consistent tone (queued / running / done / failed / canceled) instead of the dense "failed (76/76 done)" text.

### 2c - Readable rows + banner (frontend: `JobsPage.tsx`, no new CSS cards)
- The top summary banner (assembled by `retrySummaryText` from `RetryBatchFailedSummary`) is a wall of jargon ("failed-to-enqueue 124; unresolved 124; canonical retryable 200; First error: model/tool install failed:"). Replace with a compact status strip: plain counts + the WP-0257 `classifyJobError` plain-language headline for the dominant failure, raw kept behind an expander.
- Collapse the per-row button wall (Expand / Cancel active / Retry unresolved / Repair batch / Backfill titles / Export unresolved CSV / Copy unresolved URLs / Reveal log) into a primary action + a "More…" menu (WP-0168 pattern already exists in this repo) so the row reads clean.
- Keep it a LIST (header strip + list rows + accordion expand for batch children), not cards.

### 2d - DB-lock console warnings (note, light fix only if cheap)
- The state dump showed `jobs_queue_control_get` / `jobs_runtime_settings_get` "database is locked" warnings. WP-0243/0244/0245 already hardened DB contention. If these are still firing on Jobs, log/confirm; a fix beyond a bounded retry is out of this WP's scope (defer to the contention WPs) unless trivially the same pattern.

## Research Basis

- Live inspection of the Jobs page on 0.1.80 (`governance/snapshots/audit_ux_2026-06-30/02_jobs_queue_*`): dense banner, generic "download_direct_url batch" Type with no owner, flat "0%" progress, button wall, `database is locked` console warnings.
- Understand workflow `wf_30f244e6-69e` (eng-jobs-linkage agent): job/batch schema has NO origin column and NO batch table — `batch_id` is a bare UUID; origin lives only in `params_json.subscription_id` (child + parent). `job.progress` REAL column exists; `JobBatchHealthSummary` computes per-batch counts at query time. 20 `JobType` kinds; "batch" suffix is cosmetic (`summarizeGroupType`). The cheap linkage fix point is `archiverRuntime.ts:294-311`.
- Builds on WP-0248/WP-0193 (Jobs recovery/context), WP-0254 (lane/job model), WP-0257 (`classifyJobError`).

## Acceptance Criteria

- Every download job/batch in Jobs shows its origin: the owning subscription/playlist/channel title, or "Single video", or Instagram/Image — verified visually on the new build against real batches.
- Per-job and per-batch progress render as bars with downloaded/total, not bare "0%".
- The summary banner reads in plain language; the per-row action wall is reduced to a primary action + More.
- No cards introduced; the page reads as a clean list (visual verification via bridge snapshot).
- No regression to the shipped video-title display or retry/recovery behavior; `cargo test` (if engine touched) green.

## Red-Team

- Parsing `params_json` client-side per row could be slow on large queues: parse once per loaded row (already loaded), memoize subscription lookup by id (subscriptions already in a map).
- A `download_direct_url` job with no `subscription_id` is a genuine single — must not be mislabeled as a subscription; default to "Single video" only when subscription_id is absent AND job_type is download_direct_url.
- Instagram downloads carry no subscription linkage (engine never writes instagram_subscription_id) — label by job_type, do not fabricate an owner.
- Progress bar must reflect truth: use canonical batch health (succeeded/total), not loaded-row counts, per VV-SOT-002/003 (don't treat the visible subset as the whole batch).
- Banner simplification must not hide actionable failure info: keep raw error + counts behind an expander.

## Notes

- 2026-07-01: WP file authored (resolving missing-file drift) + scope extended to source linkage, real progress visuals, and readable no-card rows per the operator's 2026-06-30 request. Implemented + built + visually verified in this session, paired with WP-0255.
- 2026-07-16: Reactivated after operator reported that Jobs remains bloated and cannot surface the current single-video attempt. The v2 current-work-first/no-card restructuring contract is in `WP-0256_JOBS_QUEUE_READABLE_SOURCE_LINKED_PROGRESS_v2_REFINEMENT.md` and is paired with WP-0258's bounded backend overview.
- 2026-07-16: V2 completed in desktop 0.1.96. Exact operator URL, durable successful job/library/media linkage, no-card `Now`/`Needs attention`/`History` UI, loading/error/empty states, receipt-linked enqueue, release build, and bridge visual proof are recorded in `product/desktop/build_target/tool_artifacts/wp_runs/WP-0256/20260716_jobs_rebuild_v2/summary.md`.
