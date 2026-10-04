---
file_id: WP-0323
file_kind: work_packet
updated_at: 2026-10-04
---

# Work Packet: WP-0323 — Readers intermittently fail with "database is locked"

## Metadata

- ID: WP-0323
- Owner: Codex
- Status: IN_PROGRESS
- Created: 2026-09-24
- Board: `../TASK_BOARD.md`
- Related: WP-0312 (SQLite runtime boundary), WP-0322 (actionable errors; the export retry is a symptom workaround)

## Why NEEDS_VALIDATION (2026-09-24 record)

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

### Remediation scope — 2026-10-02

- Installed 0.1.205 still reports read admission timeouts. Live startup receipts show all four read slots occupied; preserve H2-H4 and the original live acceptance requirements.
- Measured canonical read-only activity query: current payload-before-pagination query 6386 ms; ID-first page query 28 ms; all 20 returned rows identical. Query plans show the existing track/status index and a temporary sort; fetch full payloads only after the bounded ID page is selected. Preserve source/status predicates, total, running-first order, ties, offset and limit. No schema or user-data changes.
- The history refresh effect stops permanently on any error, including temporary database contention. Reuse the existing app_busy classifier for four bounded delayed retries, cancel stale requests, clear only the recovered error, and preserve manual retry for exhaustion/permanent failures.
- Proof: focused query regression and retry policy tests; current database row reconciliation; retained-version core build and isolated headless navigation/snapshot. Full WP closure still requires the original 30-minute live check.

- 2026-09-24: Created at operator direction. H1 tested and not reproduced; no code change made. Operator will record findings from daily use for the next session.

## Status reconciliation — 2026-09-30

- Current status: IN_PROGRESS
- Unresolved database-reader investigation, not a completed fix awaiting validation. H1 not reproduced; no remedial code exists for the quoted read_admission_timeout. Next work is exact active-reader/admission reproduction and evidence-led remediation; preserve H2-H4.
- Historical requirements and proof remain preserved. Reconciled by WP-0326; no new runtime proof.

### 2026-10-02 focused proof

- Frontend failure/recovery and projection freshness checks: 41 passed. Engine operator_activity checks: 2 passed, including one-row read budget under 3 MiB with 32 MiB of stored payloads.
- A second canonical read-only probe encountered SQLite database is locked; its additional row reconciliation did not pass. Do not infer that all read locks or admission failures are eliminated.
- Current implementation preserves four-slot bounded admission and all historical requirements. Core rebuild and isolated app-boundary proof follow; installed-app 30-minute acceptance remains outstanding.

### 2026-10-03 quiet export proof boundary

- The existing Options Export now action lacks semantic bridge identity, blocking the original first-try export acceptance without foreground input. Declare that same non-destructive export button as `subscriptions.export-now` with reversible-state effect; use its existing command, receipt and independently reread exported canonical content. No new exporter or test-only path.

### 2026-10-03 event-rollup indexed admission remediation

- Canonical read-only query plan proves each subscription event scans JSON from all queued/running direct children while holding BEGIN IMMEDIATE: 13,535 queued and 8 running at inspection. Existing indexes filter type/status but cannot filter subscription identity without payload reads. Preserve child-owned subscription semantics and the writer-reservation snapshot regression; parent-only narrowing and pre-admission snapshots are rejected.
- Independent canonical read-only reconciliation of all 194 active subscription aggregates equals both the minimal canonical projection and the guarded indexed query. Whole-set canonical aggregate took 3644 ms; a single warm canonical scan took43 ms. On the canonical-field projection the indexed per-subscription probe took0.24 ms, and EXPLAIN searches subscription expression plus status and batch instead of all active payloads. Projection timing is not a full live latency claim.
- Add startup-only schema60 partial expression index for direct jobs, guarding malformed JSON with CASE/json_valid. Event counts/current job use that exact expression/index; writer reservation, status predicates, NULL subscription semantics, batch totals, admission limits and UI behavior remain unchanged. Invalid JSON rows cannot abort index creation. Original lock root-cause and30-minute/export gates remain open; this addresses an observed expensive writer query, not every SQLite lock.
- Root coordinates focused migration/rollup/snapshot/IO-budget tests and packaged database-first startup proof. Evidence: build_target/tool_artifacts/wp_runs/WP-0323/20261003/subscription_index_probe.json. Full1.28 GB backup was unsuitable under load and stopped at its90-second bound; proof uses read-only canonical reconciliation and a minimal disposable canonical-field projection instead.

### 2026-10-03 production migration and failed live acceptance

- Closed canonical schema59 backup passes quick_check; all table counts match. Supported installed background Safe Mode startup migrated to schema60. Independent complete-row hashes, counts and columns for all14 protected library/subscription/playlist/localization tables are identical before/after. Proof: `WP-0323/20261003/protected_comparison.json`.
- First Export now action produced316 subscriptions; independent canonical subscription/group comparison passes. Original single job `c3914f05-29b8-4d2c-a1fc-f47f0da39fb8` independently reads succeeded; the inspected Single videos surface shows classification up to date.
- Normal production background PID238832 observed for1800.001 seconds,59 samples. The acceptance run FAILED: one jobs.overview socket timeout at10 seconds and six actual writer_admission_timeout events in the complete canonical trace. The original monitor searched the incorrect spelling write_admission_timeout, so its empty database_failure_events claim is invalid. No read_admission_timeout/SQLITE_BUSY/database-is-locked was found in the independent complete scan; this does not establish root-cause closure or a clean live gate.
- A29,336ms provider_metadata batch writer overlaps the jobs read timeout, but correlation does not prove causation. Existing batch atomicity remains required. Add request-bound admission/open/query timing and bounded admitted-candidate attribution before selecting remediation. Original H2-H4 and before/after own-runtime reproduction remain open; WP stays IN_PROGRESS.

### 2026-10-03 instrumented live failure and next focused proof

- Installed e318 provenance independently reconciles every executable byte except the documented Tauri bundle-type substitution. Fourteen protected tables are unchanged across its core update. Request-bound bridge/open/query/terminal trace correlation passed.
- Normal background PID237052 reproduced read_admission_timeout and writer_admission_timeout. The observer recorded71 failure events in810.214seconds/27samples; stopped only owned observer PID182976, leaving the app running. This partial run FAILED; no clean30-minute claim. Evidence: `WP-0323/20261003/live_monitor_1790995699526/live_30minute_result.json`.
- The earliest timeout has four admitted read contexts and one writer still active over4seconds after successful opens. Call sites are indexed metadata/status reads and a single-job enqueue INSERT. Successful open timestamps establish occupied admission slots, not whether SQL, caller work or SQLite connection close consumed that time. Provider batch splitting is not justified by this evidence.
- Preserve connection-before-permit release and add post-open use/actual-close phases plus typed read-only `database.runtime_status` observation of existing bounded in-memory receipts and WAL metadata. This opens no SQLite connection or admission slot. Next required proof remains an actual AppDatabase reproduction and evidence-led behavioral remediation; instrumentation alone does not fix the symptom.


### 2026-10-03 own-runtime connection-close reproduction — RED

- Actual non-cfg(test) AppDatabase harness, standalone canonical backup copied from schema59 and migrated only in the disposable C-drive destination to60; three writers/four readers,100ms cadence,30-second workload. Exit1 with two writer_admission_timeout receipts: operation59/request churn_writer_1-6 after5007ms and operation162/request churn_writer_1-16 after5002ms. All43 committed fixture inserts independently match the canonical destination count; no worker panics and shutdown drain succeeds.
- Independent terminal-receipt reconciliation: operation59 waits behind writers57/58, whose close phases are2194/2571ms; approximately4094ms of its5007ms wait overlaps connection close. Operation162 waits behind writers160/161, whose close phases are3130/4128ms; approximately4314ms of its5002ms wait overlaps connection close. Post-open use phases are1/906ms and0/681ms respectively. Source closes SQLite before releasing the admission permit; the reproduced writer queue is therefore dominated by connection close, not simply a large batch SQL loop.
- Proof: `WP-0323/20261003/churn_three_writers_baseline_summary.json` and independent `churn_three_writers_baseline_attribution.json` under desktop build_target/tool_artifacts/wp_runs. Millisecond terminal timestamps approximate overlap; connection use includes SQL and caller work. This reproduces writer admission failure, not read admission timeout, and does not distinguish checkpoint versus WAL cleanup/deletion inside SQLite close.
- Harness-only NO_CKPT_ON_CLOSE counterfactual is pending against the same canonical input, disk, writer/reader counts and cadence. No production connection policy changed; WP remains IN_PROGRESS. Before/after proof, durability-preserving policy validation, and the original clean30-minute live/export acceptance remain required.


### 2026-10-03 checkpoint-on-close counterfactual rejected; VFS attribution pending

- The pending harness-only NO_CKPT_ON_CLOSE counterfactual above finished RED: exit1, five writer_admission_timeout failures, all100 committed inserts independently match the canonical disposable count, and all seven workers prove the requested connection flag plus unchanged1000-page automatic checkpoint threshold. Retained WAL size is5,121,192bytes. Proof: `WP-0323/20261003/churn_no_close_checkpoint_summary.json`.
- Writer close falls to0–1ms, but writer127 post-open use is5675ms and writer152 use is6051ms. Reader132 close remains5393ms (use27ms/open1ms); its interval overlaps writer127, not writer152. Reader201 close is3971ms. Disabling checkpoint-on-close alone does not eliminate admission failures and is not a production fix.
- Pinned SQLite source still calls walIndexClose/xShmUnmap and closes handles when checkpoint-on-close is disabled. Windows winShmUnmap purges mappings/handles at zero reference count under shared-memory mutexes even when deletion is disabled. PERSIST_WAL controls auxiliary-file persistence, not this remaining handle cleanup; a PERSIST_WAL change is not warranted by current evidence. Exact residual callback cause remains unproved.
- Opt-in fixed thread-local timing now measures xClose/xShmUnmap/xShmLock/xSync/xDelete only for the disposable example. Callback delegation and production connection/admission policies remain unchanged; callback durations may overlap and do not partition elapsed time. Exact owning helper test passes1; existing runtime regression batch passes16. An initial wrong filter ran zero tests and supplies no proof. The same-workload VFS-instrumented counterfactual is pending; no production policy change, before/after GREEN, or clean live acceptance is claimed. WP stays IN_PROGRESS.

### 2026-10-03 actual VFS sync attribution — RED; file-kind probe pending

- The pending VFS-instrumented same-workload run finished RED with two writer_admission_timeout failures, all147 committed fixture inserts matching the canonical disposable count, zero worker panics and successful1ms shutdown drain. Independently inspected retained evidence: `WP-0323/20261003/churn_close_vfs_probe_summary.json` and `churn_close_vfs_probe_attribution.json` under desktop build_target/tool_artifacts/wp_runs.
- Actual admitted operation294/request `churn_writer_1-33` lasts5761ms: post-open use5741ms, open19ms and close0ms. Its two delegated xSync callbacks total5739.4522ms, with3331.4679ms maximum single call; xClose totals0.1388ms and xShmUnmap0.1675ms. Writer timeout operations295/300 have their entire waiting intervals inside operation294's admitted lifetime. This attributes this recorded failure to synchronous VFS work during connection use; MAIN_DB versus WAL remains unknown in that retained run. Prior reader132/5393ms close evidence belongs to a different run and is not this failure's cause proof.
- New opt-in TLS64 pointer/open-flag tracking preserves the SQLite file header/layout and classifies the same single xSync measurement as MAIN_DB, WAL or explicit unknown. Unknown/overflow or absent observed xClose prevents the example's file-kind proof from passing. Exact owning helper passes1 in5.03s (`wp0323_vfs_file_kind_exact_test.log`); existing runtime regressions pass16 in46.69s (`wp0323_vfs_file_kind_runtime_regressions.log`). The new actual canonical file-kind probe is pending. No production connection-policy change, root-cause fix, before/after GREEN or clean30-minute live acceptance is claimed; WP remains IN_PROGRESS with original scope and gates preserved.

### 2026-10-03 actual automatic-checkpoint attribution — RED; FULL/auto0 canonical probe pending

- The canonical file-kind run finished RED: four writer_admission_timeout failures, all63 committed inserts matching the canonical disposable count, all seven workers with observed xClose and known file kinds, zero xSyncUnknown/fileKindOverflow and successful1ms drain. Inspected evidence: `WP-0323/20261003/churn_sync_file_kind_summary.json` and `churn_sync_file_kind_independent_attribution.json`. Post-open writer sync includes MAIN_DB4374.1286ms/request `churn_writer_0-22` and WAL2722.7900ms/request `churn_writer_0-16`.
- Pinned SQLite3.46.0 source reconciliation: NORMAL has zero commit-sync bits but checkpoint-sync bits (`sqlite3.c:60724–60732`); committed autocommit runs doWalCallbacks (`91235–91237`, `91098–91110`), whose default1000-page hook performs checkpoint (`182201–182209`), including WAL xSync (`67164`) and completed MAIN_DB xSync (`67218`). The harness autocommit INSERT has no explicit checkpoint, proving automatic-checkpoint sync within the writer's use phase. All four timeout intervals are covered by admitted writer lifetimes; callback start/end timestamps are absent, so whole-request callback totals cannot be clipped to exact timeout overlap. This is disposable-runtime attribution, not live closure or external disk-cause attribution.
- New harness-only `no_close_checkpoint_full_no_auto_checkpoint` verifies NO_CKPT true, synchronous FULL2 and automatic-checkpoint threshold0 on every connection before SQL. Existing policies and production defaults remain unchanged. Fresh-fixture smoke is GREEN: zero failures/panics, all55 committed inserts match, all seven workers prove settings/file-kind observation and drain succeeds. Evidence: `WP-0323/20261003/churn_fresh_no_auto_checkpoint/churn_summary.json`; executable/input identity: `churn_no_auto_checkpoint_identity.json`. Owning classification/regression logs remain `wp0323_vfs_file_kind_exact_test.log` and `wp0323_vfs_file_kind_runtime_regressions.log`.
- The actual large canonical FULL/auto0 counterfactual is pending. Fresh smoke does not prove that recorded canonical failure is fixed. Sustained WAL growth, production checkpoint maintenance, durability-boundary acceptance and original clean30-minute live acceptance remain unproven. WP stays IN_PROGRESS; original scope and historical notes remain preserved.

### 2026-10-03 canonical FULL/auto0 counterfactual — GREEN; production maintenance pending

- The preceding pending canonical run completed: exit0, 31,636ms, zero admission failures/panics, all81 committed inserts independently matching the destination count, verified settings and observed known VFS callbacks on all seven workers, and successful2ms drain. The immutable source/destination hashes and sizes match before destination-only schema59-to60 migration. Evidence: `WP-0323/20261003/churn_full_no_auto_checkpoint_summary.json`, `churn_no_auto_checkpoint_identity.json`, and `wp0323_canonical_full_no_auto_checkpoint.log`.
- Main-database sync is absent; each writer retains FULL WAL sync (maximum801.701ms). Maximum worker operation is2269ms for writers and793ms for readers. The WAL remains7,148,232bytes/1735frames: this proves the disposable counterfactual, not sustained WAL maintenance or a production repair. Production settings remain unchanged.
- Before testing separate concurrent maintenance, require a fixed SQLite implementation. Official SQLite WAL documentation identifies the WAL-reset race through3.51.2, fixed3.51.3 and later; current bundled3.46.0 is within the affected range, without any claim of observed corruption. Verified rusqlite0.40.2 tag bundles SQLite3.53.2 and supports MSRV1.88 (current compiler1.91.1). Research: https://www.sqlite.org/wal.html and https://github.com/rusqlite/rusqlite/releases and the v0.40.2 `libsqlite3-sys/sqlite3/sqlite3.h` version macros. Production maintenance design, durability/recovery proof, original clean30-minute live/download/export acceptance, and final packaged proof remain open; WP stays IN_PROGRESS.

<topic id="database-live-proof-review-20261003" status="IN_PROGRESS" wp="WP-0323" updated_at="2026-10-03">

## Findings and proof limits

- WP-0323-F-20261003-001: Installed13a02c5 completed1800.128seconds/60successful jobs.overview samples, zero probe failures and zero matched database-error events. Evidence: product/desktop/build_target/tool_artifacts/wp_runs/WP-0323/20261003/live_monitor_1791030167663/live_30minute_result.json. This is bounded observation, not full database acceptance.
- WP-0323-F-20261003-002: Latest succeeded count remained432; controlled probe job fadf6174-6aae-4916-8c7d-6ad3d4db11b4 failed with Video unavailable. Evidence: product/desktop/build_target/tool_artifacts/wp_runs/WP-0323/20261003/live_13a02c5_controlled_probe_canonical_failures.json. Provider cooldown/failure cannot establish normal successful-download proof.
- WP-0323-I-20261003-001: Own-runtime writer-timeout RED and passing filename counterpart are not an exact before/after reproduction of the reported read_admission_timeout. Single-video history showing classification up to date is useful workflow evidence but does not prove every database path.

## Still to check

- WP-0323-C-20261003-001: Independently reconcile the complete observation trace window for database_locked/SQLITE_BUSY as well as admission errors; the monitor does not explicitly match those two forms.
- WP-0323-C-20261003-002: Prove normal successful downloads under unchanged provider pacing, and exact reported read-admission/history behavior; do not force cooldown release or treat a removed-video error as database success.
- WP-0323-C-20261003-003: Prove first-attempt nonempty subscription export on final installed code and independently compare canonical subscriptions. Earlier export comparison covered316 subscriptions on an earlier build and does not supply fresh final-build proof.
- WP-0323-C-20261003-004: Reconcile WP-0333 crash/reopen and WP-0332 active shutdown proof. Preserve retained data, original reproduction history, admission bounds and unchanged0.1.205. Status remains IN_PROGRESS; no complete database fix claimed.

</topic>

<topic id="installed-workflow-proof-20261004" status="IN_PROGRESS" wp="WP-0323" updated_at="2026-10-04">

- WP-0323-V-20261004-002: Source `26c1a1ae5c5ca775250aa7168c19f93382815248` managed CoreOnly build and native Update passed. Independent Update review and canonical preservation evidence cover all14 protected tables: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0334/20261003/native_update_26c1a1a/independent_native_update_review.json` and `independent_update_canonical.json`. Installed normal PID137908/native SHA `5d9d36b3f388f849878b4df1441d13210ecf4e69429e4cc982f986dae70e4e02`, version0.1.205; packaging/release gates remain separate.
- WP-0323-V-20261004-003: Exact normal Single videos inspection reports classification pending0,78 canonical single items and1346 preserved unclassified older YouTube items. Evidence: `WP-0323/20261004/fixed_single_snapshot_scrolled.json`, `new_single_audit.json` and `after_download_single_audit.json` beneath `product/desktop/build_target/tool_artifacts/wp_runs`. This proves the narrow phantom-pending workflow correction, not admission-timeout root cause.
- WP-0323-V-20261004-004: One controlled download `604b1623-c1f1-4cfc-bf18-b3595f1607ef` succeeded attempt1/batch `0f5ec289-19bf-437c-ab77-9fb513a0b86d`, item `85ee0517-a2e8-452a-be06-dff5d0a5bcff`; independent canonical reread proves durable exact-job/batch single-download lineage, global pause1, no other running jobs and cleared selected grants. Final MKV contains video/audio/subtitles, full video/audio decode passes and bytes remain unchanged. Evidence: `WP-0323/20261004/controlled_terminal_independent.json`, `controlled_decode.json`, `independent_outcomes_terminal.json`.
- WP-0323-V-20261004-005: First fresh Export now accepted1791080509343/exported1791080509434; independent single-transaction read matches all316 canonical subscriptions and group memberships with zero differences. Evidence: `WP-0323/20261004/export_first_attempt_action.json`, `export_first_attempt_state.json`, `export_independent_comparison.json`.
- WP-0323-F-20261004-002: Clean live acceptance FAILED: `WP-0323/20261003/live_monitor_1791080339335/live_30minute_result.json` retains actual `database_locked` at1791080467031, whose original301515-byte diagnostic exceeded262144 bytes and was truncated. No `read_admission_timeout` is proven by this incident. Writer909 close5638ms overlaps four admitted readers using4355–4400ms; physical cause remains unproven. The observation is still incomplete (inspected958.629s/32samples); finishing it cannot erase its retained failure.
- WP-0323-R-20261004-002: Five owning desktop diagnostic checks GREEN: `product/desktop/build_target/logs/wp0323_lock_evidence_repair_20261004.log` (3), `wp0323_numeric_evidence_repair_exact_20261004.log` (1) and `wp0323_privacy_evidence_regression_20261004.log` (1) in the same log folder. The earlier unqualified numeric log ran0 tests and supplies no proof. Source-only diagnostic preservation patch is not yet built or installed. Operation909 and reader910–913 terminal attribution is retained in `WP-0323/20261003/runtime_receipts_1791080446655/summary.json`; completed-download normal history snapshot is `governance/snapshots/WP-0323/after_completed_download_history_1791081313197.png`, opened by the coordinator. WP-0323 remains IN_PROGRESS. Production checkpoint-policy proposal remains pending the operator decision under WP-0333; clean final live acceptance and exact admission-error attribution remain open.

</topic>

<topic id="history-phantom-pending-20261004" status="IN_PROGRESS" wp="WP-0323" updated_at="2026-10-04">

- WP-0323-F-20261004-001: Fresh native canonical read `.local/wp0323_exact_remaining.json` at1791079124363 proves cursor342295 and exactly one later succeeded direct job: row342370/job`a3bed1a9-dcc0-40c0-a981-3f110f795c60`, attempt1, track`other_video`, item`8d73b7dc-9b01-4298-896c-a55619cc691e`, with both item and durable lineage present. The inspected normal Single videos surface nevertheless reports one proven link left and refreshes every1500ms. This is phantom pending classification, not proof of the reported read-admission timeout cause.
- WP-0323-R-20261004-001: Narrow remediation is to count only post-cursor succeeded direct jobs whose referenced library item exists and lacks durable lineage. Align reachable-item semantics with the existing backfill candidate join; retain cursor advancement, unknown/malformed evidence handling, historical rows, runner/checkpoint policy, admission limits and version. No live data writes are authorized by this remediation.
- WP-0323-V-20261004-001: Add owning natural regressions first; root runs them RED against the original count before the query fix, then GREEN using the same cache. Verify later already-classified completions and missing items do not claim pending work, while malformed existing-item evidence remains inspectable and preserved. Root owns tests/build and exact packaged history reconciliation. Status remains IN_PROGRESS; original timeout and other acceptance gates remain open.

</topic>
