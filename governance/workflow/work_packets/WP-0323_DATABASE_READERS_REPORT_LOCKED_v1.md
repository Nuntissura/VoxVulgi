---
file_id: WP-0323
file_kind: work_packet
updated_at: 2026-09-24
---

# Work Packet: WP-0323 — Readers intermittently fail with "database is locked"

## Metadata

- ID: WP-0323
- Owner: — (next session)
- Status: NEEDS_VALIDATION
- Created: 2026-09-24
- Board: `../TASK_BOARD.md`
- Related: WP-0312 (SQLite runtime boundary), WP-0322 (actionable errors; the export retry is a symptom workaround)

## Why NEEDS_VALIDATION

Operator direction 2026-09-24: VoxVulgi cargo builds slow down the higher-priority Handshake project, so this session may run its reproduction test but no further builds or cargo tests; the operator will use the app and record findings for the next session. The reproduction below did **not** confirm the leading hypothesis, so no fix was implemented — implementing an unproven cause would be speculation. The next session must validate the remaining hypotheses against live evidence before changing code.

## Symptom (live, 0.1.204 build `20260924-014551`, pid 202596)

- UI reads intermittently fail or stall: `youtube_subscriptions_list` failed 2026-09-23 18:57:31 (rendered as "0 subscriptions" before WP-0321's fix); `subscriptions_export_now` failed "database error: database is locked" 2026-09-24 ~01:3x; bridge `GET /agent/jobs_tracks` failed once after 15 s with `database is locked` at ~02:12.
- Slow reads 2026-09-24 02:11: `youtube_subscriptions_active_refresh_ids` / `subscription_download_activity` / `youtube_subscriptions_activity` 5.8 s, `youtube_subscriptions_archive_stats` 7.9 s, `youtube_subscription_videos` 17.4 s.
- Lock attribution (`database_locked` trace rows, 02:11:01–02:11:12): holder `mode=write op=jobs.rs:12091` (dispatch claim `BEGIN IMMEDIATE` in the job runner, priority background) plus `mode=read op=jobs.rs:21207` (`get_job_tracks_runtime_snapshot`). No `external_or_unknown` holder.
- Database: WAL mode, `app.sqlite` 1.28 GB, `-wal` ~12 KB and constantly rewritten, `-shm` 32 KB.
- Every read/write context opens a fresh connection (`database_runtime.rs` `read_context` → `open_readonly_raw`, `write_context` → `open_write_raw`); read connections are `SQLITE_OPEN_READ_ONLY` + `query_only` + `busy_timeout 4000 ms`; write connections `busy_timeout 10 s`.

## Tested and ruled out (2026-09-24)

- H1 "last connection close checkpoints and deletes the WAL under an exclusive lock, blocking new readers": scratch reproduction `WP-0323_evidence/wp0323_lock_repro.py` ( SQLite 3.50.4; 200k-row table; one writer doing short `BEGIN IMMEDIATE` transactions with per-operation connections; 4 readers opening read-only/query_only/busy_timeout 4000 connections per read; 20 s per scenario). Result: app-like 220 reads, **0 locked**, max read 8.38 s; with a keeper connection 219 reads, 0 locked, max 5.20 s. H1 not reproduced; slow reads here were CPU/IO (unindexed GROUP BY under load), not locking.
- Not the yt-dlp identity probe: fixed separately (WP-0322 follow-up; `jobs_track_runtime_get` 26–78 s → 18 ms–0.9 s after caching the verified engine identity).

## Remaining hypotheses (validate next, in this order)

- H2 Some "read" commands write: projection refreshes (e.g. subscription activity/archive rollups via `derived_projection_state` / `refresh_subscription_activity_rollup_*`) take the writer lane or open a write transaction inside a UI read, so they wait on the runner's claim transaction and surface "database is locked" as a read failure. Check: for each slow command in the trace, whether its code path calls `write_context`/`db::open` or executes INSERT/UPDATE; compare `command_phase` rows.
- H3 Writer-lane starvation: runner claim + subscription fan-out + rollup refreshes keep the single writer busy longer than the callers' timeouts (reads that are really writes fail after ~4–15 s).
- H4 The rusqlite bundled SQLite build or `FULL_MUTEX` open flags behave differently from the Python reproduction (re-run the reproduction as a Rust test against the app's `AppDatabase` runtime when cargo use is allowed again).

## Acceptance (for DONE)

- Root cause proven with a reproduction that fails before and passes after the change (engine test or scratch harness using the app's own runtime).
- Live: 30 min of normal use with downloads running and no `database_locked` rows for read commands; export succeeds first try.
- One bundled build/test run; version retained.

## Status updates

- 2026-09-24: Created at operator direction. H1 tested and not reproduced; no code change made. Operator will record findings from daily use for the next session.
