---
file_id: WP-0320
file_kind: work_packet
updated_at: 2026-09-23
---

# Work Packet: WP-0320 — Lane pacing, cooldown visibility, truthful diagnostics, and job-history retention

## Metadata

- ID: WP-0320
- Owner: Claude
- Status: DONE (2026-09-23; proof bundle `product/desktop/build_target/tool_artifacts/wp_runs/WP-0320/summary.md`)
- Created: 2026-09-23
- Refinement: `WP-0320_LANE_PACING_COOLDOWN_VISIBILITY_AND_JOB_HISTORY_RETENTION_v1_REFINEMENT.md`
- Board: `../TASK_BOARD.md`
- Closes: the database-engine question (WP-0315 outcome `reject`, SQLite retained)
- Related: WP-0312 (SQLite runtime boundary), WP-0317 (agent bridge), WP-0318 (jobs/archiver workflow)

## Intent

- What: fix every finding of the 2026-09-23 live investigation on v0.1.204, split YouTube pacing into `youtube_single` / `youtube_recurring` lanes with separate Options values, make the anti-bot cooldown retry visible and field-aligned, and give the operator a backup-first way to purge historical job rows.
- Why: Diagnostics showed "not installed" for unloaded state and took 56–84 s per probe; downloads sat behind an invisible 6 h cooldown labelled "Waiting for the scheduler"; a yt-dlp timeout was labelled "Network problem"; `database_locked` attribution rows were truncated away; 985 NAS probes ran in 70 min; the `job` table holds 339,092 rows of which 87 % are terminal.

## Scope

In scope (all mandatory, operator-listed):

1. **Trace truncation** — `database_locked` / `database_busy` rows must survive the 256 KB row cap (cap the process snapshot; never drop the lock details).
2. **Tool probes** — version probes time-bounded (15 s), results cached in-process (TTL 10 min, keyed by executable path + mtime), `force` bypass for the Diagnostics Refresh button.
3. **Diagnostics rendering** — unloaded state renders "checking…", never "not installed" / "Not ready".
4. **YouTube lane pacing** — `ProviderTransferSettings` gains `youtube_single` and `youtube_recurring` policies with Options fields; lane policy is a floor over the default preset (sleep = max, fragments = min). Image archive and Localization Studio stay single-lane.
5. **Cooldown retry** — escalating dwell: `cooldown_base_dwell_secs` (default 3600) × 2^failed_canaries, capped by `cooldown_dwell_secs` (default 21600); tuning exposed in Options; state remains per operation (download / enumeration) because the block is on the YouTube identity, not the lane.
6. **Gate visibility** — Video Archiver shows the gate state, plain reason, "next attempt at HH:MM", cooldown attempt number, and a `Return to baseline` action; Jobs page shows the same time.
7. **Failure classification** — yt-dlp "timed out after Ns" is `download_stalled`, not `network`; engine classifier gains rules for the most frequent `unknown` messages; the real `last_error_message` is visible on the subscription chip.
8. **NAS probe load** — the persisted `present` observation refresh interval rises from 10 min to 60 min; `observe_media_path_fresh` stays uncached (an in-probe cache was tried and rejected: it made the queue-identity pre-apply safety check miss a deletion — `queue_identity_apply_rejects_storage_state_that_changed_after_backup`).
9. **Job-history retention** — `jobs_purge_terminal_history` command (dry-run + execute, backup-first via `VACUUM INTO`, excludes localization job types, deletes rows before files) and a `terminal_job_retention_days` setting (default 0 = off, opt-in; when enabled the runner tick applies it no earlier than 24 h after start, never at startup) ; Jobs page control with preview.

Out of scope:

- Any database-engine change or benchmark (closed by WP-0315).
- Deleting queued rows, subscriptions, library rows, or the existing `db/backups` copies — operator decision only.
- Per-lane cooldown state (rejected: a rate limit applies to the identity, splitting state would extend the block).

## Contracts (backend ↔ frontend)

- `provider_transfer_settings_get/set`: `ProviderTransferSettings { …existing, youtube_single: ProviderTransferPolicy, youtube_recurring: ProviderTransferPolicy }`. Defaults: single `{concurrent_fragments:1, limit_rate:null, sleep_interval_secs:5, sleep_requests_secs:2}`, recurring `{1, null, 10, 3}`.
- `youtube_protection_tuning_get/set/reset`: `YoutubeProtectionTuning` gains `cooldown_base_dwell_secs` (default 3600, clamp 600..cooldown_dwell_secs).
- `jobs_track_runtime_get`: `youtube_gate` gains `mode: "normal"|"cautious"|"conservative"|"cooldown"|"hold"`, `cooldown_attempt: u32`, `entered_at_ms: Option<i64>`.
- `tools_*_status` commands accept `force: Option<bool>`.
- `jobs_purge_terminal_history { older_than_days: u32, include_succeeded: bool, dry_run: bool }` → `{ dry_run, counts_by_type_status: [{type,status,count}], total, backup_path: Option<String>, deleted: u64 }`.
- `jobs_terminal_retention_get/set { days: u32 }`.

## Acceptance criteria

- Diagnostics never shows "not installed" before a probe result exists; a warm second visit renders tool state within 1 s (cached); Refresh re-probes.
- A synthetic `database_locked` trace row with an oversized snapshot is written under 256 KB with `cmd`, `message`, and `busy_attribution` intact (engine/Tauri test).
- With protection state `cooldown`, the Video Archiver activity strip shows the reason and the next-attempt time; `Return to baseline` clears it and the strip updates on the next poll.
- Lane policies persist and are applied: a `youtube_recurring` child job's effective sleep ≥ the recurring lane floor; a `youtube_single` job's effective sleep ≥ the single lane floor (engine test).
- Cooldown dwell escalates 1 h → 2 h → 4 h → 6 h (cap) on consecutive failed canaries and resets on `controlled_canary_success` (engine test).
- `jobs_purge_terminal_history` dry-run on the live database reports counts without writing; execute creates the backup before the first delete and never touches queued/running or localization rows (engine test + live dry-run receipt).
- `failureStates` classifies "managed_yt_dlp timed out after 900s" as `download_stalled` (frontend test).
- One bundled verification run: `cargo test` (engine + desktop, single invocation each), `npm run build`, headless app-boundary check through the bridge. No per-check cargo runs.

## Test / verification plan

- Engine: focused tests added next to changed code; executed once in the bundled run.
- Desktop: `npm run build` + existing vitest suite once.
- App boundary: packaged headless launch, `GET /agent/jobs_tracks` shows the new gate fields, `POST /agent/command jobs.overview` after dry-run purge shows unchanged counts.
- Proof bundle: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0320/summary.md` + `evidence.json`.

## Risks / open questions

- Purge execution on the live 1.2 GB database is an operator decision; this packet ships the tool and a dry-run receipt only.
- The Torch capability probe (90 s child) remains a separate slow path; it is already bounded and superseded correctly — not changed here.

## Status updates

- 2026-09-23: Created from the live investigation (trace 11:55–12:15, read-only DB queries, code inspection) and two Explore reports (pacing lanes; job-row dependencies). Research basis in the refinement. Implementation dispatched to three parallel agents (engine A: lanes/cooldown/retention; engine B: probes/trace/NAS; frontend).
- 2026-09-23: DONE. Verification during review changed two designs: the in-probe NAS `present` cache was reverted (it made `queue_identity_apply_rejects_storage_state_that_changed_after_backup` miss a deletion) in favour of a 60 min persisted refresh for `present`; automatic retention became opt-in (default 0, first tick ≥24 h after start, off-thread) after the initial version would have purged at first launch. Added: gate snapshot projects durable cooldown/hold as `held` with the next probe time when the runner has not dispatched (empty queue), Jobs landing shows the gate line, Diagnostics summary tiles show "Checking...". Final build `build_desktop_target_20260923-145715_0_1_204` (core-only, version retained). Pre-existing failures outside this packet are itemised in the proof summary (engine cross-test cancel flag, kokoro/phase2 stale fixtures, three WP-0317/0318 contract strings). Purge execution on the live database remains an operator decision.
