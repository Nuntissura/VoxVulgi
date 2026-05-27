# Work Packet: WP-0244 - DB startup read/write contention

## Status

DONE

## Owner

Codex

## Operator Request Preserved

- "proceed"
- Continue from WP-0243's residual finding: fresh v0.1.29 traces no longer show the hidden Instagram heartbeat on Video Archiver, but still show `database_locked` for `youtube_subscriptions_list` and slow startup reads for `video_libraries_list` / `youtube_subscriptions_list`.

## Problem Statement

Video Archiver startup still performs several database and filesystem-heavy reads at once. Some commands that are semantically UI reads still open write-capable SQLite connections and call `db::migrate()`. Archive stats also call the full subscription list and may create or merge archive state by checking legacy output paths, which can touch NAS roots during a visible refresh.

## Research Basis

- Repo evidence:
  - `LibraryPage.refresh()` runs Video Archiver startup work in one `Promise.all`, including `youtube_subscriptions_list`, `youtube_subscription_groups_list`, `video_libraries_list`, `youtube_subscriptions_archive_stats`, and `youtube_subscriptions_active_refresh_ids`.
  - `video_libraries::list_video_libraries()` uses `db::open()` and `db::migrate()` even though it is a list command.
  - `jobs::active_youtube_subscription_refresh_ids()` uses `db::open()` and `db::migrate()` even though it is a status read.
  - `subscriptions::youtube_subscriptions_archive_stats()` calls `load_youtube_subscription_archive_ids()`, which calls `ensure_youtube_subscription_archive_state()` and can create app-managed archive files or check legacy output archive paths.
- Runtime evidence:
  - Fresh v0.1.29 freeze report on Video Archiver: `youtube_subscriptions_list` emitted `database_locked`, `video_libraries_list` took 1400 ms, and `youtube_subscriptions_list` took 1261 ms.
  - Direct read-only probe against the current app DB showed `youtube_subscription` has 255 rows, `youtube_subscription_group_member` has 500 rows, `job` has 6171 rows, and `library_item` has 122325 rows.
  - A cold read-only probe of the full subscription list took 4158 ms once, then repeated warm reads were ~12-16 ms, showing cold cache / contention sensitivity.
- Primary references checked:
  - SQLite WAL docs: readers and writers can usually proceed concurrently, but WAL still has one writer at a time and long readers/checkpoints can still affect progress.
  - SQLite checkpoint docs: checkpoint modes can block or return busy depending on readers/writers.
  - SQLite `wal_checkpoint` pragma docs: checkpoint status exposes busy/incomplete checkpoint state.

## Scope

Base scope:

- Make Video Archiver UI status/list commands read-only where they do not need to write.
- Move default video library bootstrap out of the list command and into explicit startup / mutation paths.
- Make archive stats non-invasive so routine refresh does not create archive files or check legacy NAS archive paths.
- Add command timers and DB error tracing around the remaining Video Archiver startup commands that were previously untraced.
- Move visible Video Archiver list reads off the synchronous Tauri command lane so a slow read does not serialize or freeze page entry.
- Keep actual subscription queue/download flows responsible for archive-state migration because those are explicit operator workflows.

Out of scope:

- Full SQLite architecture rewrite.
- Background DB actor / serialized command queue.
- YouTube cookie rejection UX.
- Duplicate-link UX.
- Python dependency repair.

## Implementation Plan

1. Add red contract tests for read-only UI commands and traced startup commands.
2. Add a Rust unit test proving archive stats are non-invasive and do not merge legacy output archives during refresh.
3. Export a default video library bootstrap function and call it during startup after `db::ensure_schema()`.
4. Change `video_libraries::list_video_libraries()` to use `db::open_readonly()` and skip `db::migrate()`.
5. Change `jobs::active_youtube_subscription_refresh_ids()` to use `db::open_readonly()` and skip `db::migrate()`.
6. Change archive stats to count only existing app-managed archive state files.
7. Add timers / DB error tracing for `video_libraries_list`, `youtube_subscriptions_archive_stats`, and `youtube_subscriptions_active_refresh_ids`.
8. Move visible Video Archiver list commands to async Tauri handlers using `spawn_blocking`.
9. Verify with contract tests, focused Rust tests, build checks, desktop target build, install, and a fresh freeze report.

## Verification Plan

- `npm run test:contracts` from `product/desktop`.
- Focused Rust tests from `product/engine`.
- `npm run build` from `product/desktop`.
- `cargo check` from `product/desktop/src-tauri`.
- Desktop target build with `WP-0244`.
- Silent install and bridge/freeze report smoke.

## Proof Bundle

To be written under:

`product/desktop/build_target/tool_artifacts/wp_runs/WP-0244/2026-05-21_db_startup_read_write_contention/summary.md`

## Closure Evidence

- Installed version: `0.1.34`.
- Build log: `product/desktop/build_target/logs/build_desktop_target_20260521-034004_0_1_34.log`.
- Runtime freeze report: `C:\Users\Ilja Smets\AppData\Roaming\com.voxvulgi.voxvulgi\diagnostics\traces\freeze_reports\freeze_report_1779330240634.json`.
- Video Archiver command timings in the installed v0.1.34 report: `youtube_subscriptions_list` 144 ms, `youtube_subscription_groups_list` 49 ms, `video_libraries_list` 66 ms, `youtube_subscriptions_active_refresh_ids` 40 ms.
- The v0.1.34 Video Archiver capture showed no `database_locked`, `database_busy`, `command_slow`, `freeze_detected`, or `freeze_recovered` rows.
- Visual evidence: `governance/snapshots/WP-0244/video_archiver_v0_1_34_1779330288396.png`.
- State dump evidence: `governance/snapshots/WP-0244/video_archiver_v0_1_34_1779330288497.dump.json`.
- Verification passed: `npm run test:contracts -- tests/dbStartupContentionContract.test.ts`, `cargo test archive_stats_ -- --nocapture`, `npm run build`, `cargo check`, desktop target build, silent install.

## Follow-Up Risks

- Python dependency repair remains out of scope for this packet.
- Jobs page query cost remains a separate performance issue; v0.1.33 captured one `jobs_list` call at 9909 ms.
