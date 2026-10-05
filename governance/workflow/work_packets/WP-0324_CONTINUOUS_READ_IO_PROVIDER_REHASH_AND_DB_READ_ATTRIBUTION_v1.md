---
file_id: WP-0324
file_kind: work_packet
updated_at: 2026-10-05
---

# Work Packet: WP-0324 — Continuous read I/O: provider re-hash on every poll, unattributed database page reads

## Metadata

- ID: WP-0324
- Owner: Claude
- Status: DONE
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

<topic id="root-process-read-io-measurement-20261004" status="NEEDS_VALIDATION" wp="WP-0324" updated_at="2026-10-04">

The existing watcher lacked root-process read-transfer bytes; prior database-only30minute proof cannot satisfy this packet's provider-runner I/O gate. The operator-authorized remediation adds native `GetProcessIoCounters` accounting to both byte-identical watcher twins, preserving all existing probes and startup SQLite suppression.

Research: [Microsoft GetProcessIoCounters](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getprocessiocounters) requires query or limited-query rights and exposes failure through GetLastError; [Microsoft IO_COUNTERS](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-io_counters) defines six unsigned64-bit counters, including ReadTransferCount bytes and ReadOperationCount operations. Selected implementation uses one limited-query handle per sample, reads creation time and counters on that handle, and closes it. No ETW, elevation, filesystem scan, hashing or SQLite is added by the counter.

- WP-0324-IO-001: `samples.jsonl` root `io` records cumulative `read_transfer_bytes`, `read_operation_count`, attributable PID/creation FILETIME, monotonic interval delta/rate and sample UTC timestamp; accounting represents root logical I/O, not physical disk traffic, child I/O or provider/file attribution.
- WP-0324-IO-002: Native failure is `ok=false`, explicit error and unknown/null metrics; first sample, identity change, counter decrease or prior failed sample cannot create a fabricated zero delta/rate.
- WP-0324-IO-003: Optional `TargetProcessId`, `TargetStartedAtMs` and `TargetExecutablePath` must be supplied together; exact bound identity and bridge disagreement refuse process-name fallback. Default discovery behavior remains unchanged when all are omitted. `metadata.json` preserves the selected binding.
- WP-0324-IO-004: Required component proof covers native API success/failure, unsigned counter arithmetic, identity reset and exact-target refusal, watcher syntax and existing owning watcher tests. Synchronize governance/shipped twins before their byte-equality test; root owns all execution.
- WP-0324-IO-005: Final proof remains actual queued YouTube normal-runner30minutes, read-byte deltas correlated with candidate/gate polls, no per-tick/poll/candidate90–110MB provider rehash bursts, `jobs_track_runtime_get` p50<1second, exact VFS heavy-read callsite attribution and existing follow-up remediation. Preserve permitted execution full-byte verification and10minute reverification; bytes alone cannot identify a provider hash. Existing contract/status/version/changelog obligations remain unchanged.

Run the watcher unpackaged through the verified quiet native launcher: MSIX tool processes can resolve operator paths to a virtualized old artifact. Independently bind installed physical hash/creation before the observer; the watcher does not claim a physical SHA from its process pathname. Root-selected output directory remains configurable; no30minute run or runtime acceptance is recorded by this amendment.

</topic>

<topic id="provider-cache-recompute-observation-20261004" status="NEEDS_VALIDATION" wp="WP-0324" updated_at="2026-10-04">

Source inspection: `tools.rs::provider_file_identity` has two cache-hit exits around the single-flight lock; only `compute_provider_file_identity` performs full-byte identity recomputation. Polling uses `fresh=false`, while the execution gate intentionally uses `fresh=true`. Root I/O bytes alone cannot distinguish these computations from database/media reads. The selected additive diagnostic reuses the existing in-memory provider-status/verification pattern, existing stamp/TTL cache and existing serialized install status; no new endpoint or readiness/trust predicate is introduced.

- WP-0324-CACHE-001: Count one polling or forced-fresh request per actual identity call, one cache hit per returned cached identity (including the second lookup after another flight), and one recomputation start/completion around the actual compute; classify misses under the single-flight lock as cold/input_changed/ttl_expired or forced_fresh.
- WP-0324-CACHE-002: Keep per-root counters in at most16 in-memory entries with generation identity on eviction/recreation; keep at most16 recent completed recomputation records per entry. Serialize no root/path, secrets, stamps, file hashes or provider inputs. Report counter generations so eviction cannot fabricate continuity.
- WP-0324-CACHE-003: Diagnostics acquire no filesystem/network/database work and do not change cache keys, stamping, TTL, fresh verification, single-flight policy, live flags or readiness. Extend the existing cache reuse/change/fresh test with exact counter/reason checks; independently test bounded state retention and serialize safety.
- WP-0324-CACHE-004: Final native30minute observer must bind exact current installed PID/creation/hash and canonical YouTube jobs, read-byte accounting and actual gate-command latency, and recorded cache generations/recompute reasons. Allowed cold/10minute/fresh-execution computations are not per-poll regressions; no measured runtime PASS is recorded here.

Research basis remains the packet's existing cache/headless A/B investigation and inspected current cache/attestation implementation, plus the primary Microsoft process-I/O API research above. Root owns tests/builds/launches; original acceptance and NEEDS_VALIDATION status remain unchanged.

Counters are scoped to the current root generation, not process-lifetime totals. A completed recomputation retains `started_generation`; if more than 16 roots evict its diagnostic record during computation, the differing generation explicitly prevents treating the new counters as a continuous interval. `elapsed_ms` covers computation plus the existing post-compute input-stamp validation, not physical storage I/O alone.

- WP-0324-CACHE-005: Expose the memory-only counters as additive optional `provider_file_identity_diagnostics` on the existing canonical `JobTracksRuntimeSnapshot` / `/agent/jobs_tracks` surface, populated after the existing YouTube gate projection. Do not add an identity/provider recomputation, database query, endpoint or readiness predicate. Existing frontend consumers may ignore the field; counter types retain snapshot equality compatibility.

</topic>

<topic id="normal-selected-runner-read-amplification-20261005" status="NEEDS_VALIDATION" wp="WP-0324" updated_at="2026-10-05">

Actual installed sourcea0bb600/nativeSHAa197ae258a379124b8797f00d0559ec5496c8c6fcbe1b840379de0e036faad11/version0.1.205 normal background PID211564 creation1791162730491; canonical global pause retained. Five exact prior failed originals were reopened through downloads.enqueue without mode, same IDs/attempt2/new foreground batch0631f980-304e-48b6-875b-d7c3a1f0dc2e, archived failed attempt1 preserved. Native observer187224 armed before start_selected only request1791163101894; receipt heldfalse/pausedtrue/rest_pausedtrue, exact5 grants. Independent canonical read/snapshot confirmed four running/fifth queued, no foreign running; later native plan snapshot shows first4 succeeded/fifth running. Full1800-second observation remains in progress; no30minute PASS yet.

Provider forced-fresh23 then stable while polling/cache hits continue. Four actual yt-dlp launch receipts follow four groups of5 forced execution scans, as source ensure/append_runtime_args/run_yt_dlp requires. This observed segment does not show ongoing per-tick rehashing. Full source/receipt reconciliation: product/desktop/build_target/tool_artifacts/wp_runs/WP-0324/20261005/normal_io_prearmed_a0bb600_01/forced_fresh_execution_and_bounded_heavy_reads.json.

Separate VFS attribution in bounded current trace segment: selected_pending18,500,505,600bytes/808 receipts; fetch_queued_jobs_for_track_inner34,688,180,224/1490; operator_activity_page3,006,590,976/128; four claim contexts3,177,234,432bytes. Logical context reads, not physical disk IO, exact statement or timeout root-cause proof. Native read-only metadata plans at hot_query_plans_live_readonly.json (Python SQLite3.50.4, not bundled3.53.2) show selected-existence optimizer drives job status index; paused fetch drives queued job index then LEFT JOIN/sort; activity inner page is non-covering because text id is absent from existing sort indexes. Metadata-only plans executed under RO/query_only/BEGIN/WAL, no SQL mutation or full query benchmark.

Follow-up WP0335 owns minimal measured selected-query/activity read amplification remediation and its actual bundled-VFS/order-equivalence/installed proof; WP0323 retains existing older activity payload-before-pagination remediation. Original provider30minute/candidate/Tauri p50/VFS acceptance remains unchanged. Status remains NEEDS_VALIDATION before parallel follow-up implementation; do not infer completion from plans or launch-only evidence.

</topic>

<topic id="normal-observer-failure-20261005" wp="WP-0324" status="NEEDS_VALIDATION" updated_at="2026-10-05">

Observer187224 terminal FAIL: Windows WinError5 replacing progress.json at1791164885193;1782samples/1781580ms. Original failed verdict retained. Bounded sharing-conflict retry added only to ignored harness; no product gate weakened. Native watcher completed1950580ms,1682samples,zero unresponsive/bridge-failure samples. Its three external RO database probes timed out; these are not AppDatabase read_admission_timeout proof.

Independent canonical BEGIN at1791164966631: all5 originals SUCCEEDED attempt2/errornull, globalpause1, selectedgrants0/foreignrunning0. Five distinct NAS MKVs independently probed with AV1/VP9 video, Korean Opus and embedded English/Korean subtitle tracks; output_metadata.json under .local/proofVVRemaining/wp0324_output_metadata_review_01. Root opened paired Jobs screenshot normal_selected_5_terminal_independent_1791164967239.png: paused queue and waiting controls readable.

Selected work first exhausted1791164514065 (~23m32), so the partial observer does not establish original continuous30min acceptance. Full-window actual Tauri p50 is unproven; bounded current tail median3.5ms is partial evidence only. Generation1 provider records reconcile28forcedfresh+2TTL; no cold/input_changed in observed records. Preserve original acceptance and repeat on rebuilt WP0335 candidate with adequate canonical selected workload.

</topic>

<topic id="verified-normal-runner-closure-20261005" wp="WP-0324" status="DONE" updated_at="2026-10-05">

Original provider-cache/candidate/30minute/Tauri-latency/VFS-attribution acceptance independently passed on source d066e3c / installed6cf6592e. Root inspected actual independent artifacts and source:1800051ms pending selected work,60 Tauri samples/p50=3ms,3402 polls=3402 cache hits,18 execution windows/90 unique forced-fresh flights with initial-boundary counters reconciled,0 database failures in2246 retained trace rows. Follow-upWP0335 exists. Proof: product/desktop/build_target/tool_artifacts/wp_runs/WP-0324/20261005/normal_io_prearmed_d066e3c_01/summary.md. Preserve79 logical large-read intervals/20 without provider-flight overlap as unassigned; no physical-I/O ownership, zero-all-bursts or universal timeout-resolution claim. WP0335 readable Jobs UI and remaining selected completion are separate. Historical incomplete/failing observations remain retained; version/changelog unchanged under current authority.

</topic>
