# Work Packet: WP-0254 - Lane-based job scheduling + startup subscription auto-sync

## Status

IN_PROGRESS (engine scope complete + tested + built in desktop 0.1.73; pending operator runtime verification)

## Owner

Claude (Opus 4.8)

## Operator Request Preserved

- "i feel vv can not do 2 things at the same time. no parallel jobs, like localization and downloading a video, or updating a playlist and downloading a single video. i think it should be possible but because playlists can have a count over 1000 videos it should be conservative."
- "single videos most of the time should be able to download in parallel with playlists/chanels/subs. single videos can download in parallel but only if the settings allow it (this is already implemented, but i think the setting is a global setting and not the correct behaviour)."
- "[the old 4K downloader] on start up it checked all playlists/subscriptions/chanels to see if there were new videos and started downloading them. this is now a manual action. youtube does have an agressive anti bot policy/prevention so downloading of playlists and subs/chanels should be concervative. and should not block my single download videos."
- Operator decisions (this session): startup behavior = **auto-check + auto-download**; lane defaults = **Single 3 / Recurring 1 / Localization 1**; per-lane limits tunable in Options afterward.
- "there should also be a manual button to update all subscriptions/stop. (when stopped mid download it should not be forgotten on restart)." → engine commands for **update-all** + **stop** the recurring lane, and **resume interrupted downloads on restart** instead of failing-and-forgetting them. (The Update-all/Stop *buttons* are UI → WP-0255; the engine commands + resume live here.)

## Intent

Replace the single global job-concurrency limit + global FIFO queue with **independent per-lane concurrency budgets**, so single one-off downloads run in parallel with conservative playlist/subscription syncing and are never starved by a 1000-video playlist fan-out or by localization. Add 4KVDP-style **startup auto-check + auto-download** of due subscriptions into the conservative Recurring lane. Engine-first, shippable and `cargo test`-verifiable on its own; the Video Archiver UI redesign + per-lane Options UI + "legacy" terminology purge are the follow-on (WP-0255).

## Current State (verified, canonical)

- `jobs.rs`: ONE runner thread (`start_runner`→`runner_loop`, jobs.rs:5302/5950). Each tick reads a single global `get_max_concurrency()` (DB `meta` key `jobs_max_concurrency`, default `DEFAULT_MAX_CONCURRENT_JOBS = 4`; **live value = 2**), computes `available = max - running`, fetches that many **strictly FIFO** queued jobs (`ORDER BY created_at_ms ASC`, jobs.rs:6029) across ALL `JobType`s, and `thread::spawn`s one worker per job.
- A `youtube_subscription_refresh_v1` job **fans out into child `download_direct_url` jobs** (jobs.rs:6504, via `enqueue_download_direct_url_batch_raw_with_subscription`). A 1000-video channel injects ~1000 downloads into the same FIFO pool → starves single videos + localization.
- No lane / job-category concept exists. The only "concurrent" setting in the UI is `yt_dlp_concurrent_fragments` (per-video fragment parallelism), NOT job parallelism — this is the global setting the operator was thinking of.
- Startup subscription sync is **manual** ("Queue due active" button → `queue_all_active_youtube_subscriptions`); to be confirmed there is no existing startup auto-refresh hook in `lib.rs` setup() before adding one.
- **Interrupted jobs are FAILED, not resumed, on restart.** `requeue_orphaned_running_jobs` (jobs.rs:5325, called from `start_runner`:5308) is misnamed: it `UPDATE job SET status='failed', error='interrupted by app shutdown' WHERE status='running'`. So a stop/restart mid-download loses the job → the operator's "forgotten on restart". yt-dlp's download archive makes re-queuing downloads safe (already-downloaded entries are skipped).

## Scope

### 2a - Job lanes + per-lane scheduling (engine: `db.rs`, `jobs.rs`)
- Add an additive `lane TEXT` column to the `job` table via a **v17** migration (explicit `PRAGMA user_version` stepping, per WP-0126). Backfill existing rows' lane from their `type` (and subscription association where derivable). No row deletes.
- Define lanes and the `JobType`→lane map:
  - **single** - user one-off downloads: `download_direct_url` (NOT subscription-associated), `download_image_batch`, `import_local`, `dummy_sleep`.
  - **recurring** - `youtube_subscription_refresh_v1` AND its child `download_direct_url` jobs (subscription-associated). Conservative + anti-bot paced.
  - **localization** - asr / translate / diarize / dub / experimental render / tts (pyttsx3 + neural) / mix / mux / separate (spleeter + demucs) / clean vocals / qc / export pack / `install_phase2_packs_v1` (heavy).
- Stamp `lane` at enqueue time (single vs recurring decided by whether a subscription id is present in the enqueue path).
- Rewrite `runner_loop` scheduling: per tick, compute running-count **per lane** (`SELECT lane, COUNT(*) FROM job WHERE status='running' GROUP BY lane`), and for each lane fetch up to `lane_limit - running_in_lane` queued jobs of that lane (FIFO within lane), claim + spawn. One lane being full must not block another.

### 2b - Per-lane concurrency settings (engine: `jobs.rs` settings + `meta`)
- Replace/extend the single `jobs_max_concurrency` with per-lane limits (`jobs_lane_limit_single|recurring|localization`), defaults **3 / 1 / 1**. Keep reading the legacy global key for back-compat as the single-lane default if per-lane keys are unset (no data reset).
- Extend `JobRuntimeSettings` + `get/set_runtime_*` so the future Options UI (WP-0255) can read/write per-lane limits. Clamp each to `[1, MAX_MAX_CONCURRENT_JOBS]`.

### 2c - Recurring-lane anti-bot conservatism
- Recurring lane limit defaults to 1 (one channel/playlist refresh or child-download at a time). Reuse the existing preset yt-dlp pacing knobs (`sleep_interval`, `sleep_requests`, `throttled_rate`) for recurring downloads. No new throttling subsystem.

### 2d - Startup auto-check + auto-download (engine: `lib.rs` setup() + `subscriptions.rs`/`jobs.rs`)
- On startup (gated by safe-mode OFF), enqueue **due** active subscriptions (respecting each sub's `refresh_interval` + failure backoff) into the recurring lane, **staggered** so it is not a bot-flagging burst. Reuses `queue_all_active_youtube_subscriptions` / `is_subscription_due`. Auto-download follows naturally because refresh fans out into recurring-lane downloads.
- Make it operator-disablable via a config flag (default ON per operator choice) so it can be turned off without a rebuild.

### 2e - Recurring-lane control + resume-on-restart (engine: `jobs.rs`, `subscriptions.rs`)
- **Update-all-subscriptions** engine command: enqueue all active subscriptions (not only "due") into the recurring lane, staggered. (Distinct from the existing due-only `queue_all_active`.)
- **Stop** engine command: pause/cancel in-flight recurring-lane work (refresh + child downloads) without touching the single or localization lanes. Stopped work must be **persisted, not discarded**, so a later "update all" or restart resumes it.
- **Resume-on-restart**: change `requeue_orphaned_running_jobs` so interrupted **download/recurring** jobs are re-queued (status→`queued`) instead of failed-and-forgotten. Localization jobs may remain fail-on-interrupt (heavy Python stages; operator re-runs) — decided by lane. yt-dlp archive makes resumed downloads idempotent. A "stop" must mark recurring jobs in a way that survives restart and is resumable.

Out of scope (→ WP-0255): Video Archiver UI redesign, per-lane Options UI, Update-all/Stop **buttons**, "legacy" terminology purge (UI + code identifiers), Media-Library/Video-Archiver single-history view collapse.
Out of scope (→ WP-0256): Jobs/Queue readable collapsible playlist LIST rows (NO cards — operator hates cards; honor `build_rules.md` no-new-cards) + reliable video-title metadata across all job states.

## Research Basis

- Canonical engine read this session: `jobs.rs` runner_loop/`fetch_queued_jobs`/`claim_job`/`get_max_concurrency`/`set_runtime_max_concurrency`/`JobType`; `enqueue_*` subscription-child fan-out at 6504; live DB probe (`jobs_max_concurrency=2`, schema v16, 255 active subscriptions). Builds on WP-0253 (single-library unification), WP-0220 (multi-library + tab split), WP-0165 (Quick/Advanced gate this supersedes for the archiver), WP-0161 (4KVDP parity), WP-0246 (job-spawn timeout/cancel hardening — lane workers keep using `run_command_output_with_control`).
- yt-dlp anti-bot field practice: low concurrency + sleep/throttle for channel/playlist pulls; one source at a time. Matches recurring-lane=1 + existing preset pacing.

## Acceptance Criteria

- v17 migration runs additively on the real DB (additive `lane` column + backfill; no row deletes; idempotent `ensure_column` + user_version step). `cargo test -p voxvulgi_engine` green.
- A queued single download runs while a recurring playlist refresh + its child downloads are in flight (recurring capped at 1, single lane free) — proven by a runner/scheduling unit test.
- A localization job and a download job run concurrently (separate lanes) and neither blocks the other.
- Per-lane limits persist and are clamped; legacy global `jobs_max_concurrency` still honored as fallback (no reset of the operator's current value).
- Startup auto-check enqueues only **due** subscriptions, staggered, into the recurring lane, and is disablable via config.
- No user library/subscription/playlist data deleted or reset.

## Red-Team

- **Startup auto-download burst trips YouTube anti-bot.** Control: recurring lane = 1, only `is_subscription_due` subs, staggered enqueue, existing failure backoff + preset sleep/throttle.
- **v17 migration on 122k-row DB slow at startup.** Control: ADD COLUMN is metadata-only; one backfill UPDATE in a transaction; idempotent.
- **Per-lane running-count drift if a worker crashes leaving status='running'.** Control: DB-derived per-lane counts each tick (not in-memory atomics) + existing stuck-job watchdog (WP-0246) auto-fails wedged jobs.
- **Localization lane=1 + downloads=3 over-subscribes CPU/GPU/NAS.** Control: conservative defaults, operator-tunable per lane; localization isolated to its own single slot.
- **Lane misclassification routes a subscription child-download into the single lane (re-introduces starvation).** Control: stamp lane from the subscription-aware enqueue path + unit test asserting child downloads are lane=recurring.
- **Auto-sync writes to an unreachable NAS root on startup.** Control: reuse WP-0253 `effective_download_dir_with_fallback` local fallback; due-check is read-only.

## Notes

- 2026-06-15: WP authored. Engine-first slice; UI + legacy purge tracked as WP-0255. Build gated until engine slice + tests are green and operator-confirmed.
- 2026-06-15 (impl): 2a/2b/2c DONE + tested. `db.rs` v17 additive `job.lane` column + backfill + `idx_job_lane_status_created`. `jobs.rs` `JobLane` enum + `for_type` map, lane stamped at enqueue (subscription-child downloads → Recurring via `enqueue_with_type_item_batch_and_lane`), `runner_loop` rewritten to per-lane scheduling with DB-derived running counts (no in-memory atomics; survives restart), `get_lane_limit`/`lane_limit_conn` per-lane settings (defaults Single 3 / Recurring 1 / Localization 1; legacy global `jobs_max_concurrency` retired from scheduling). 3 new unit tests (mapping, stamping incl. recurring override, lane isolation). Then 2e **resume-on-restart DONE**: `requeue_orphaned_running_jobs` now re-queues interrupted single+recurring (download) jobs instead of failing them (yt-dlp archive makes resume idempotent); localization jobs still fail-on-interrupt; old fail-expecting test updated to assert resume + new localization-fails test added. `cargo test -p voxvulgi_engine` = **221 passed, 0 failed**.
- 2026-06-16 (impl cont.): 2e Stop/Update-all + 2d startup auto-sync DONE. `jobs.rs` recurring-lane pause (`set_recurring_paused`/`is_recurring_paused`, meta `jobs_recurring_paused`, runner skips Recurring when paused, cleared at `start_runner` so restarts resume). `subscriptions.rs` `queue_all_active_youtube_subscriptions_now` (force, ignores due gate, keeps backoff). `lib.rs` Tauri commands `youtube_subscriptions_update_all` (clear stop + force-queue), `youtube_subscriptions_stop_recurring` (set stop), `youtube_subscriptions_recurring_paused` (status) — registered. `lib.rs` setup() startup auto-sync thread (deferred 20s, due-only, recurring lane, gated by safe-mode + `config/subscription_auto_sync.txt`, light to avoid WP-0228 regression). New `recurring_pause_round_trips` test. **`cargo test -p voxvulgi_engine` = 222 passed, 0 failed; desktop `cargo check` green.** ENGINE SCOPE COMPLETE. Building 0.1.73 (one build per operator). UI buttons for Stop/Update-all + per-lane Options control = WP-0255; Jobs list + titles = WP-0256. Operator runtime verification of lane parallelism + resume + auto-sync still required before DONE.
