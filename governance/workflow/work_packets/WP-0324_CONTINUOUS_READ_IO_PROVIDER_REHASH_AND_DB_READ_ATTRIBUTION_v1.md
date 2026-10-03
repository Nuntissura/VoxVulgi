---
file_id: WP-0324
file_kind: work_packet
updated_at: 2026-09-30
---

# Work Packet: WP-0324 — Continuous read I/O: provider re-hash on every poll, unattributed database page reads

## Metadata

- ID: WP-0324
- Owner: Claude
- Status: NEEDS_VALIDATION
- Created: 2026-09-24
- Board: `../TASK_BOARD.md`
- Related: WP-0322 (download-engine identity cache by size+mtime), WP-0321 S3 (provider tree receipt fast path), WP-0312 (SQLite runtime boundary), WP-0323 (readers report locked)
- Proof bundle: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0324/summary.md`

## Why still NEEDS_VALIDATION

Two acceptance items need the operator's normal (non-headless) app. Headless mode skips the job runner, so the per-candidate dispatch path can't be exercised there. The `database_heavy_read` attribution also needs the real 1.2 GB database under real use.

## Operator report and evidence (live, 0.1.204, pid 202596, started 02:07:35)

- Another agent reported `desktop.exe` had read 141.7 GB since start and was reading ~40 MB/s, blaming it for stalling the disk. At 03:22: `ReadTransferCount` was 148.9 GB, 10,421,528 read ops, `WriteTransferCount` was 0.
- Per-second counters (03:28:43–03:29:13): `desktop.exe` "IO Read Bytes" peaked at 115.7 MB/s while PhysicalDisk C: read 2.9 MB/s, so most of these reads are served from the file cache. C: stayed at 0% idle even when `desktop.exe` read 0 B/s, and was writing 20–90 MB/s from other sources (a `cargo test` of another project, among others). **VoxVulgi is not the main cause of the disk stall.** Its read volume is still wasteful: CPU, cache churn, and command latency. C: is a Samsung 870 QVO 8 TB (QLC) with ~205 GB free; disk-space cleanup is operator-owned and out of scope.
- Burst shape (03:20:07–03:20:37): alternating 25–100 MB/s bursts. Most average exactly ~4 KB/op (25,605,920 B / 6,272 ops). One burst was 98.8 MB at 45 KB/op at 03:20:18, matching `jobs_track_runtime_get` (invocation 871) to the second.
- Diagnostics trace, last 71 min: `jobs_track_runtime_get` ran 230 times, averaging 6,449 ms, max 58,391 ms; 5 `database_locked` rows.
- `db/app.sqlite` is 1,222.8 MB, page_size 4096. The `jobs_track_runtime_get` `GROUP BY track, status` uses the covering index `idx_job_track_status_created`, so that query is not the bulk reader.

## Item 1 — provider files re-hashed on every poll

Verified code path (pre-change):

- `tools::youtube_po_provider_install_status` computed full SHA-256 of `node.exe` (88.5 MB), `npm.cmd`, the plugin tree, the server entrypoint and `package-lock.json`, and launched `node --version` and `npm --version` on **every** call. Nothing cached it.
- `tools::youtube_po_provider_runtime_status` calls it, and the job runner calls that **once per queued YouTube candidate** in the dispatch loop (`jobs.rs` ~11358 single downloads, ~11472 subscription refresh). The gate snapshot (`/agent/jobs_tracks`, `jobs_track_runtime_get`) reaches it through `youtube_policy_gate_fields` (15 s TTL) and `load_runtime_identity` (5 s TTL).
- Correction of the first diagnosis: `engine.exe` (yt-dlp) was **not** re-hashed per call. WP-0322 already caches its verified identity by size+mtime.

Change (`product/engine/src/tools.rs`):

- `ProviderFileIdentity` holds exactly the previously recomputed values; `compute_provider_file_identity` produces it.
- Polling (`youtube_po_provider_install_status`) reuses the identity while every hashed input keeps its size and mtime: node.exe, npm.cmd, server entrypoint, lock file, and every file under the plugin dir, as sorted `symlink_metadata` stamps. It re-hashes full bytes at least every `PROVIDER_FILE_IDENTITY_REVERIFY_INTERVAL` (10 min). A single-flight mutex stops parallel hashing. A result is cached only if the stamps did not change during the hash.
- Status flags that change at runtime (plugin archive marker, audit marker, `package.json` allowlist, node_modules in-memory attestation, verifying/invalid flags) are still read live on every call.
- `youtube_po_provider_install_status_fresh` always re-hashes. It is used by the execution gate and verification paths (`ensure_youtube_po_provider`, `youtube_po_provider_execution_status`, `verify_youtube_po_provider_node_modules*`, install/repair completions).
- Security trade-off (same accepted pattern as WP-0322 and WP-0321 S3): on the polling path, a same-size byte replacement with a restored mtime goes undetected for at most 10 min. Every provider launch still re-hashes full bytes.

## Item 2 — unattributed 4 KB database page-read bursts

It is not verified which code runs the repeated 25–100 MB bursts of 4 KB reads, so this item adds exact attribution rather than guessing a query to change.

First attempt, rejected by its own test: SQLite's `SQLITE_DBSTATUS_CACHE_MISS` × page size. The new focused test read a 24 MiB blob table and recorded only 45,056 bytes. Cause, verified in the bundled amalgamation (`libsqlite3-sys-0.30.1/sqlite3/sqlite3.c:14096`, `# define SQLITE_DIRECT_OVERFLOW_READ 1`): overflow pages of large TEXT/BLOB values are read straight from the file, one page per read, bypassing the page cache. That read pattern matches the 4 KB bursts, so the page-cache counter would have missed exactly what item 2 must find.

Change (`product/engine/src/database_runtime.rs`, `product/engine/src/db.rs`):

- A read-counting SQLite VFS `voxvulgi_read_counting`, registered once and not as the process default. It copies the default VFS, overrides only `xOpen`, and wraps each opened file so every I/O method forwards to the real file (pattern of SQLite's `ext/misc/appendvfs.c` and `vfsstat.c`). `xRead` adds the bytes read to a per-thread counter. If registration fails, connections open with the default VFS, uncounted.
- `open_write_raw` / `open_readonly_raw` open through `open_counted_connection`.
- Each runtime read or write context records `file_bytes_read`: the counter delta between context open and close, on the opening thread. It is `None` if the context closes on another thread, and it includes nested operations on the same thread.
- `file_bytes_read` is added to `ActiveDatabaseOperation` and `DatabaseOperationReceipt` (additive).
- An operation reading at least `HEAVY_READ_TRACE_BYTES` (16 MiB) emits a `database_heavy_read` (warn) row into the existing diagnostics trace. The row carries `operation` (call site file:line), `lane`, `mode`, `priority`, `file_bytes_read`, `execution_ms`, `row_count` and `outcome`. It is emitted after the registry lock is released.

## Release identity

- Operator 2026-09-24 ("do a new build ... bump version"): desktop version assigned 0.1.204 → 0.1.205 in `tauri.conf.json`, `package.json`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`. `package-lock.json` (0.1.7) is unmanaged and was left alone, as in earlier bumps.
- `BUILD_CHANGELOG.md` unchanged; per VV-CODEX-VERSION-003 the entry is added only after this release passes its proof gates.

## Files touched

- `product/engine/src/tools.rs`
- `product/engine/src/database_runtime.rs`
- `product/engine/src/db.rs`
- `product/desktop/src-tauri/tauri.conf.json`, `product/desktop/package.json`, `product/desktop/src-tauri/Cargo.toml`, `product/desktop/src-tauri/Cargo.lock` (version)
- `governance/workflow/work_packets/WP-0324_CONTINUOUS_READ_IO_PROVIDER_REHASH_AND_DB_READ_ATTRIBUTION_v1.md`
- `governance/workflow/TASK_BOARD.md`

## Verification done (2026-09-24)

See `summary.md` in the proof bundle for commands and logs.

- Build: core-only 0.1.205, `build_desktop_target_20260924-051544_0_1_205.log`, 0 warnings, version files and changelog verified unchanged.
- Focused engine tests: 19/19 pass (`wp0324_engine_focused_vfs.log`), including both new tests.
- Full engine lib suite: 714 pass / 22 fail in parallel. 19 of the 22 pass single-threaded (parallel timing/`ffmpeg canceled`, as in WP-0320). The last 3 fail identically at HEAD with the WP-0324 changes stashed, so they are pre-existing: `named_status_and_list_projections_use_bounded_readonly_admission` (missing `youtube_protection.rs::get_tuning`), `kokoro_app_cache_ready_requires_snapshot_files_not_just_marker`, `v47_unbound_identity_binds_only_to_exact_authenticated_destination` (`no such table: library_item`).
- Install: silent `Update` (`/S /VVMAINTENANCE=update`), exit 0, `installer_success version=0.1.205`; the installed exe equals the build exe except Tauri's 3-byte bundle-type marker (`UNK`→`NSS`); app data retained.
- Headless A/B, same disposable root with copies of the real Node runtime and provider tree, 6 polls of `/agent/jobs_tracks` at 16 s after 20 s warm-up:
  - 0.1.204: read 662.7 MB, latencies 42,606 / 14,074 / 866 / 3,801 / 858 / 1,157 ms.
  - 0.1.205: read 131.4 MB (one full hash on the first poll), latencies 14,577 / 26 / 30 / 31 / 24 / 329 ms.
- Visual: `governance/snapshots/WP-0324/jobs_v0_1_205_*.png` renders v0.1.205, readable, navigation intact.

## Remaining (operator runs the installed 0.1.205 normally)

1. With YouTube jobs queued for ~30 min, `desktop.exe` IO Read Bytes/sec shows no ~90–110 MB large-block bursts per runner tick or gate poll, and `jobs_track_runtime_get` p50 is under 1 s.
2. `database_heavy_read` rows in `diagnostics_trace.jsonl` name the call site(s) behind the 4 KB bursts; open a follow-up WP to fix them.
3. Then mark DONE and add the 0.1.205 changelog entry.

## Acceptance (for DONE)

- Compiles; focused tests pass. (met)
- Live: provider hashing no longer runs per poll or per candidate. (headless polling path met; runner path pending)
- Live: `database_heavy_read` rows identify the source of the 4 KB page-read bursts, and a follow-up WP exists for it. (pending)

## Status updates

- 2026-09-24: Created and implemented at operator direction (items 1 and 2 only). Not compiled or tested; operator paused cargo work because it slows other projects.
- 2026-09-24: Operator directed a build with version bump. Built 0.1.205; the attribution test exposed the direct-overflow-read gap, and item 2 was reimplemented with the read-counting VFS; rebuilt, tested, installed via silent Update after the operator closed the app; headless A/B proves item 1 on the polling path. Live runner and attribution proof pending.

## Status reconciliation — 2026-09-30

- Current status: NEEDS_VALIDATION
- Provider cache and read-counting VFS built/installed; original uncompiled/page-cache statements are historical. Current session observed repeated database_heavy_read at jobs.rs:5344 (operator_activity_page); this proves attribution is emitted, not the timeout cause or complete live runner acceptance. Remaining runner I/O/cache proof and follow-up remediation of measured reads.
- Historical requirements and proof remain preserved. Reconciled by WP-0326; no new runtime proof.

<topic id="normal-runner-io-proof-review-20261003" status="NEEDS_VALIDATION" wp="WP-0324" updated_at="2026-10-03">

## Findings and insights

- WP-0324-F-20261003-001: Provider cache/read-counting VFS are implemented and have focused/headless A/B evidence. The initially uncompiled record is historical. Existing live attribution identifies operator_activity_page; it does not establish the read timeout cause.
- WP-0324-I-20261003-001: WP-0323's successful30-minute jobs.overview monitor measures database requests/errors, not process read-I/O, provider rehash frequency or per-candidate gate timing. It cannot close this packet. Evidence: product/desktop/build_target/tool_artifacts/wp_runs/WP-0324/summary.md and WP-0323/20261003/live_monitor_1791030167663/live_30minute_result.json.

## Still to check

- WP-0324-C-20261003-001: Actual normal-runner candidate path reuses provider identity cache; measure absence of repeated90–110MB provider rehash bursts over the required observation interval and jobs_track_runtime_get p50<1second against the original contract.
- WP-0324-C-20261003-002: Correlate database_heavy_read VFS receipts with exact normal-runner/query call sites and changed read inputs, preserving attribution versus root-cause distinction.
- WP-0324-C-20261003-003: Reuse unchanged focused proof; record final candidate/runtime identities and independent live result before DONE. Status remains NEEDS_VALIDATION.

</topic>
