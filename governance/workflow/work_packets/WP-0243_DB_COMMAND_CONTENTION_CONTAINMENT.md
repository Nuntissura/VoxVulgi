# Work Packet: WP-0243 - DB command contention containment

## Status

DONE

## Owner

Codex

## Operator Request Preserved

- "so what is next?"
- "ok proceed"
- The selected next action was to stop the strongest freeze signal: background command and DB contention from hidden or overly broad polling.

## Problem Statement

Freeze reports after v0.1.28 still show multi-second UI command stalls. The latest report on the Video Archiver page is dominated by `instagram_subscriptions_queue_all_active`, `library_get`, and `jobs_queue_control_get`. An older Jobs-page report is dominated by `jobs_list`. The sibling external monitor also observed an intermittent SQLite `database is locked` error.

## Research Basis

- Repo evidence:
  - `App.tsx` runs a global Instagram subscription heartbeat whenever the desktop window is active, even while the active page is not Instagram Archive.
  - `JobsPage.tsx` polls the full `refresh()` while active jobs exist, and `refresh()` fans out to `jobs_list`, queue control, runtime settings, and subscription lists.
  - `JobsPage.tsx` hydrates `library_get` and `item_outputs` for all unique item IDs in the loaded jobs list.
  - `db::open_readonly()` waits up to 10 seconds on SQLite busy handling, which turns DB contention into multi-second UI command stalls.
- Runtime evidence:
  - v0.1.28 latest freeze report: `instagram_subscriptions_queue_all_active` reached 15,429 ms; `library_get` reached 3,718 ms; `jobs_queue_control_get` reached 2,409 ms.
  - v0.1.27 freeze report: `jobs_list` reached 16,873 ms and 15,058 ms.
  - WP-0242 external monitor found no heavy installer child process during its sample window, but did find large job/library tables and an intermittent DB lock outside the app.
- Primary references checked:
  - SQLite PRAGMA `busy_timeout`: busy timeouts are per connection and measured in milliseconds.
  - SQLite WAL: readers and writers can usually run concurrently, but there is only one writer and long read/checkpoint interactions can still block progress.
  - React `useEffect`: effects re-run based on reactive dependencies and cleanup is the mechanism for stopping stale work.

## Scope

Base scope:

- Gate the global Instagram heartbeat so it only runs while Instagram Archive is the active page.
- Reduce the heartbeat cadence so it cannot repeatedly contend with visible workflows.
- Split Jobs-page active polling so active-job polling refreshes only the job snapshot, not queue controls, runtime settings, and subscription lookup tables.
- Bound Jobs-page context hydration so it does not launch `library_get` and `item_outputs` for up to 200 rows at once.
- Stop Jobs-page context hydration while the page is hidden.
- Shorten read-only SQLite busy waits so contention fails fast instead of freezing the UI for seconds.
- Trace explicit `database_locked` / `database_busy` events for the commands identified by the freeze reports.

Out of scope:

- Full SQL query/index redesign.
- YouTube cookie rejection and duplicate-link UX.
- Python dependency repair.
- A compiled sibling diagnostic GUI.

## Implementation Plan

1. Add red source-contract tests for the polling and DB-busy containment requirements.
2. Update `App.tsx` Instagram heartbeat wiring.
3. Update `JobsPage.tsx` polling and context hydration.
4. Update `db.rs` read-only busy timeout.
5. Add Tauri command error tracing for DB lock/busy errors on affected commands.
6. Run contract tests, TypeScript build, Rust check/tests where practical, and `vvwatch.cmd` live smoke.

## Verification Plan

- `npm run test:contracts` from `product/desktop`.
- `npm run build` from `product/desktop`.
- `cargo check` from `product/desktop/src-tauri`.
- `.\vvwatch.cmd -DurationSeconds 20 -IntervalSeconds 2 -NoPathProbe`.
- Fresh freeze report comparison after installing the build, if a desktop target build is produced.

## Proof Bundle

Written under:

`product/desktop/build_target/tool_artifacts/wp_runs/WP-0243/2026-05-20_db_command_contention_containment/summary.md`

## Completion Evidence

- Contract tests: `npm run test:contracts` passed with 20 passed, 0 failed.
- Engine regression: `cargo test queue_all_active_instagram_subscriptions_respects_paused_queue` passed with 1 passed, 0 failed.
- Desktop target build: v0.1.29 build log `product/desktop/build_target/logs/build_desktop_target_20260520-224252_0_1_29.log` ends with `Build completed`.
- Install check: `C:\Program Files\VoxVulgi\desktop.exe` reports file/product version `0.1.29`.
- Bridge check: v0.1.29 app process `64516` responded on port `56181`, current page `video_ingest`, safe mode `false`.
- Fresh freeze report: current-pid rows had 0 `instagram_subscriptions_queue_all_active` rows and 0 `freeze_detected` / `freeze_recovered` rows.
- Residual issue carried forward: fresh v0.1.29 trace emitted `database_locked` for `youtube_subscriptions_list`, and startup reads still showed slow `video_libraries_list` / `youtube_subscriptions_list` commands.
