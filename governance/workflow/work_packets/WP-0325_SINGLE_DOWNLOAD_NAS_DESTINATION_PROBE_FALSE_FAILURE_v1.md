---
file_id: WP-0325
file_kind: work_packet
updated_at: 2026-09-30
---

# Work Packet: WP-0325 — Single video downloads fail when the NAS destination answers slower than 300 ms

## Metadata

- ID: WP-0325
- Owner: Claude
- Status: DONE
- Created: 2026-09-29
- Board: `../TASK_BOARD.md`
- Related: WP-0322 B1 (app-busy bounded requeue, reused here), WP-0253 item 2d (3 s download-root reachability probe), WP-0320 (`youtube_single` / `youtube_recurring` lanes)
- Proof bundle: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0325/summary.md`

## Operator report (2026-09-29)

Single video downloads do not work. The job appears in the single-video queue, then disappears, and never reaches the downloaded list. The operator also saw `database runtime error: writer_admission_timeout`.

## Evidence (live, installed 0.1.205, pid 227708, normal mode)

Canonical entities, per VV-SOT-002:

- **Job store:** all 17,900 `job` rows were read through `/agent/command jobs.list`, deduplicated by id.
  - Exactly 1 row is on track `youtube_single`: `c3914f05-29b8-4d2c-a1fc-f47f0da39fb8`, URL `https://www.youtube.com/watch?v=afU4QPoSKVk`.
  - Status `failed`, after running 20:29:58 → 20:30:17.
  - Error: `model/tool install failed: verified root alias target is currently unavailable: …\Video\4K Video\4K Video 21-08-2025`.
  - The job did not vanish. It failed, which removed it from the running list. The downloaded list shows successes only.
- **Lanes still separate:** the trace shows `job_track_dispatched` with `track: youtube_single`, independent of `youtube_recurring`. Both lanes share one YouTube pacing gate, by design.
- **`writer_admission_timeout`:** 6 trace rows since startup. All come from subscription refresh or startup reconcile, none from the single download. They are background contention from 17,775 queued subscription jobs and are not the cause of this failure. Out of scope here (WP-0323 / WP-0324).
- **Destination:**
  - Params `output_dir` is `\\MIR\home\Video\4K Video\4K Video 21-08-2025`. It comes from the Video Archiver "Save to" field, persisted in WebView localStorage key `voxvulgi.v1.library.url_batch_output_dir` (confirmed via `/agent/dump`).
  - `config/root_aliases.json` has an active alias `\\?\UNC\MIR\home\Video\4K Video\4K Video 21-08-2025` → `Z:\Video\4K Video\4K Video 21-08-2025`. `Z:` is mapped to `\\MIR\home`.
  - Both paths are directories when probed at 20:5x.
- **Root cause (code):**
  - `jobs.rs::resolve_downloads_dir_with_override` calls `root_rebind::resolve_active_alias_path(.., require_available=true)`.
  - That probes the alias target root and the mapped folder with `ALIAS_TARGET_PROBE_TIMEOUT = 300 ms` (`root_rebind.rs:30`).
  - The same session's trace shows NAS path probes taking 72, 538, 1,624 and 3,465 ms (`media_path_probe_completed`).
  - The bounded probe pool also returns `Unreachable` immediately when its 64-slot queue is full (`paths.rs::probe_path_bounded_internal`).
  - Any of these produces the permanent failure above: no retry, and a misleading `model/tool install failed:` prefix.
  - The frontend classifier has no rule for the text, so the UI shows "Unrecognized error".
- **Why subscriptions still work:** `subscriptions.rs` resolves with `require_available=false`, so no probe runs. The default-destination path already gives the NAS 3 s (`paths.rs::effective_download_dir_with_fallback`) before hitting the same 300 ms alias probe.

## Research basis

- **Sources checked:**
  - Code paths: `root_rebind.rs`, `paths.rs` bounded probe pool, `jobs.rs` destination resolution and WP-0322 app-busy requeue.
  - Live trace probe latencies.
  - `failureStates.ts`.
- **Pattern in repo:** the NAS reachability probe elsewhere uses 3 s (`download_root_reachable`). Transient app-internal failures use the bounded silent requeue from WP-0322 B1 (`requeue_job_for_app_busy_or_exhausted`, 5 attempts × 15 s).
- **Field pattern:** SMB metadata calls on Windows can stall for seconds under load. Bounded probes plus retry-with-backoff is the standard handling; failing a write permanently on one sub-second miss is not.
- **Rejected options:**
  - Removing the write-time probe (it keeps "new writes never fall back to the historical root" fail-closed, per the test in `root_rebind.rs`).
  - Unbounded blocking stat (it can hang a worker on a dead share).
  - Rewriting the operator's saved localStorage path (it is operator data; the alias already maps it correctly).
- **Selected approach:** longer bounded write probe, distinct transient error, reuse of the existing bounded requeue, and a classifier rule.

## Scope

In scope:

1. `root_rebind.rs`: for `require_available=true`, probe the mapped destination once with a 3 s bound (`ALIAS_WRITE_TARGET_PROBE_TIMEOUT`, matching `download_root_reachable`).
   - `Directory` → OK.
   - `Unreachable` → transient error `download folder is not responding: <path> (the NAS or drive did not answer within 3 s)`.
   - `Missing` / `File` → the existing permanent error.
   - The read path (`require_available=false`) is unchanged.
2. `jobs.rs`: `download_direct_url` jobs failing with the transient destination error use the existing bounded requeue (`requeue_job_for_app_busy_or_exhausted`) instead of failing at once. The exhausted final failure keeps the real message, prefixed `download folder unreachable:`, not `app busy:`.
3. `failureStates.ts`: add a rule classifying `download folder is not responding` and `root alias target is currently unavailable` as `storage`, so the operator gets "Could not save the file" with open/change-folder actions. The new message avoids `timeout` / `timed out` so it cannot fall into `youtube_not_responding`.
4. Tests: focused engine tests for the probe outcomes and the requeue classifier; a frontend classifier test.
5. Build 0.1.205 core-only (version retained, changelog untouched), install, and prove at the app boundary with the exact reported case.

Out of scope:

- Subscription-queue database contention (WP-0323 / WP-0324).
- Changing the operator's saved Save-to path.
- The `C:\governance\snapshots` snapshot-root resolution in the installed build (noted as an adjacent finding).
- Renaming `EngineError::InstallFailed` across the engine.

## Acceptance criteria

- A slow NAS response of up to 3 s no longer fails a single download. A longer outage requeues the job (bounded, `job_app_busy_requeued` trace row) instead of failing it immediately.
- A missing destination folder still fails closed and never falls back to the historical root. The existing test `disconnected_alias_target_falls_back_for_reads_but_new_writes_fail_closed` stays green.
- The UI classifies the final failure as `storage` ("Could not save the file"), not "Unrecognized error".
- Exact-case proof: `https://www.youtube.com/watch?v=afU4QPoSKVk` downloaded through the `youtube_single` lane with the operator's `\\MIR\…` Save-to path reaches `succeeded`, with an MKV file in the library (or an exact blocker is stated).
- The version stays 0.1.205. `BUILD_CHANGELOG.md` is unchanged.

## Test / verification plan

- `cargo test -p voxvulgi_engine` focused: `root_rebind` alias tests plus new tests; `jobs` app-busy classifier tests plus new tests.
- Desktop: `npm test` focused on `tests/failureStates.test.ts`; `npm run build` type check.
- Core-only build via `governance/scripts/build_desktop_target.ps1 -CoreOnly -ExpectedVersion 0.1.205`.
- Install over the running app. This needs the operator to close the running app, or to give `PROCESS_STOP_APPROVED`.
- Bridge: `downloads.enqueue` the exact URL, `jobs.inspect` until terminal, and read the canonical job row plus library row.

## Risks / mitigations

- **3 s probes add latency on a dead share:** bounded, and paid once per job attempt only.
- **Requeue loop on a permanently dead NAS:** bounded by `APP_BUSY_MAX_ATTEMPTS` (5), then a real failure with an actionable message.
- **Probe pool saturated by library probes:** reported as `Unreachable` → transient → requeued, not failed.

## Status updates

- 2026-09-29: Opened from live investigation; implementation started.
- 2026-09-29: Implemented in `root_rebind.rs` (`classify_write_target`, 3 s write probe), `jobs.rs` (`is_destination_unreachable_error`, bounded requeue) and `failureStates.ts` (`storage` rule).
  - Frontend `tests/failureStates.test.ts`: 33/33 pass.
  - Engine focused run (`--lib -- --test-threads=1 root_rebind:: app_busy destination_unreachable`): 31/31 pass (log `wp_runs/WP-0325/logs/engine_focused.log`).
  - Core-only build retained 0.1.205, changelog unchanged (`build_target/logs/build_desktop_target_20260929-223737_0_1_205.log`).
  - Isolated headless smoke of the new `desktop.exe`: `agent_headless=true`, `app_version=0.1.205`, Video Archiver renders (`governance/snapshots/WP-0325/headless_video_archiver_1790714992108.png`).
  - Pending: install over the operator's running app (needs operator approval to stop pid 227708), then the exact-case download proof in normal mode (headless skips the job runner).
- 2026-09-30: Install and exact-case retry.
  - Stop: the operator approved `PROCESS_STOP_APPROVED` for pid 227708 and its children.
    - All eight `engine.exe` children had already exited.
    - `CloseMainWindow` closed the WebView and removed the bridge sidecar (graceful shutdown started). `desktop.exe` was still performing library writes after 65 s, so it was force-stopped. The graceful drain was cut short; interrupted jobs are requeued at startup (`requeue_orphaned_running_jobs`).
  - Install: `VoxVulgi_0.1.205_x64-setup.exe /S /VVMAINTENANCE=update` (sha256 `C5A2D44B…BB1FBEC`).
    - Exit 0, `installer_success version=0.1.205 maintenance=update`.
    - The installed exe equals the build except the 3-byte bundle marker, and contains the new message text.
  - Relaunch: `--agent-background` (production state, minimized window), pid 264380. Bridge: `app_version=0.1.205`, `agent_background=true`.
  - Exact case: `jobs.retry` for `c3914f05` (operation `wp0325-exact-case-retry-1`) reopened the same row as attempt 2 with the original `\\MIR\home\…` `output_dir`; receipt saved as `wp_runs/WP-0325/retry_receipt.json`.
  - **Blocker:** the job stays `queued` because the shared YouTube gate is held by `adaptive_youtube_cooldown` until 03:12:58.
    - The cooldown started 23:41:05 on 2026-09-29 in the previous session.
    - It was triggered by subscription downloads failing with `HTTP Error 429: Too Many Requests` and `YouTube exposed only its 360p fallback`.
    - It is independent of this WP. It was not overridden (Return to baseline is operator-owned).
    - Polling continues (`wp_runs/WP-0325/logs/exact_case_poll.log`).
- 2026-09-30: DONE. The cooldown expired at 03:13; the exact job ran on `youtube_single` as the YouTube canary and `succeeded` at 03:15:51 with `app_busy_attempts=0`. The MKV (471,195,009 bytes, valid EBML header) is at `Z:\Video\4K Video\4K Video 21-08-2025\ex_IVE_-_Leeseo_FAP_COMPILATION_2026_Vol.2_afU4QPoSKVk.mkv`; library item `e76648de…` is `available`, with a `work_track=youtube_single` download record. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0325/summary.md`.
