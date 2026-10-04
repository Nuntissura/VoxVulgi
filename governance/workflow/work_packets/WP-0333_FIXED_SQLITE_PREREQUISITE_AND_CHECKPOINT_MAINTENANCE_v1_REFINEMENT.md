---
file_id: WP-0333-REFINEMENT-v1
file_kind: refinement
updated_at: 2026-10-03
---

<topic id="fixed-sqlite-and-maintenance" status="active" version="v1" wp="WP-0333" updated_at="2026-10-03">

# Fixed SQLite prerequisite and checkpoint maintenance

Operator scope: complete database repairs, protect subscriptions/playlists/library metadata, retain forgotten work in packets and build the updated installer without changing0.1.205. This refinement supports WP-0323; its original own-runtime RED/GREEN and normal30-minute/export acceptance remains intact. Canonical contract: `WP-0333_FIXED_SQLITE_PREREQUISITE_AND_CHECKPOINT_MAINTENANCE_v1.json`; board owner is root.

Spec anchors: TECHNICAL_DESIGN59–61 requires shared AppDatabase, four readers,64-entry queues,4/5-second read/FIFO-write admission and10-second drain; maintenance is explicit PASSIVE. AGENTS VV-DBRUNTIME001–005 requires readiness, reviewed exceptions and reconciled shutdown. WP-0312 freezes checkpoint/read-pool/fairness behavior. PRODUCT_SPEC preserves canonical media identity and source metadata.

Measured boundary: `WP-0323/20261003/churn_sync_file_kind_summary.json` has4 writer-admission failures. Independent attribution pairs MAIN_DB/WAL syncs during post-open writer use under NORMAL/no-close-checkpoint/auto1000; close0–1ms. Three waits span multiple admitted writers. Callback aggregates lack event timestamps and cannot prove exact per-sync overlap. `churn_full_no_auto_checkpoint_summary.json` has81canonical writes/0failures in31636ms under FULL/no-close-checkpoint/auto0, but WAL7148232bytes/1735frames. That is a disposable counterfactual, not a production maintenance solution or live closure.

Research checked2026-10-03: SQLite WAL https://www.sqlite.org/wal.html, checkpoint https://www.sqlite.org/c3ref/wal_checkpoint_v2.html and synchronous https://www.sqlite.org/pragma.html#pragma_synchronous; tagged rusqlite release https://github.com/rusqlite/rusqlite/releases/tag/v0.40.2 and native manifest https://raw.githubusercontent.com/rusqlite/rusqlite/v0.40.2/libsqlite3-sys/Cargo.toml. WAL-reset race documented for3.7.0–3.51.2 includes current bundled3.46.0; concurrent writer/checkpointer ownership needs a fixed implementation first. Tagged rusqlite0.40.2 uses native sys0.38.2. Select exact `=0.40.2` in both engine and desktop manifests, preserve bundled/backup features, verify actual linked SQLite at least3.51.3 and source identity. No dependency label alone proves the fix.

Exact tag header https://raw.githubusercontent.com/rusqlite/rusqlite/v0.40.2/libsqlite3-sys/sqlite3/sqlite3.h143–145 identifies bundled SQLite3.53.2/3053002 and sourceID dated2026-06-03. Root verified release MSRV1.88 and currentrustc1.91.1. This replaces earlier3.53.1 shorthand; actual linked runtime identity remains a required gate.

Reuse AppDatabase canonical-path ownership, operation IDs, bounded receipts, counted VFS, protected disposable backup provenance, existing maintenance receipt and runner/database shutdown. Reject keeper alone (automatic checkpoint remains), PERSIST_WAL (checkpoint precedes persistence), higher admission timeouts, atomic-batch chunking, and permanent FULL/auto0 without maintenance. PASSIVE skips busy-handler waiting but sync I/O can still stall. FULL supplies commit durability while concurrent maintenance ownership remains unselected.

Red team: old-pin concurrent checkpoint can expose corruption; fixed library is a hard predecessor. FFI/API/default-feature upgrades can break VFS/backup/schema paths; run owning compatibility tests and runtime identity. Long readers can starve checkpoints; measure partial progress/WAL growth and recovery. Separate maintenance can escape startup/drain or still cause disk contention; require one attributable owner, stop/join and real admission/callback proof. Close suppression can widen loss windows; independently prove acknowledged commits after crash/reopen under verified FULL. No canonical writes, foreign process actions or manually repaired records are authorized by the experiment.

Execution: phase1 upgrades fixed SQLite and makes minimum compatibility repairs, then exercises only a guarded disposable maintenance counterfactual. Root centrally batches checks/builds. Phase2 production ownership/policy requires independent passing counterpart and separate reviewed authority/spec/exception amendment. It is not authorized by this contract alone. Preserve4/5-second bounds, FIFO, atomicity and protected data. No broad pool redesign or production solution claim before evidence.

Acceptance: actual fixed linked version; owning regressions; canonical-density maintenance progress/growth/starvation/admission/shutdown proof; durable crash/reopen reconciliation; final installed normal30-minute downloads/reads/export and active-runner close. Required proof summary/evidence and independent review precede DONE. Update root-owned board after contract review; retain IN_PROGRESS and all WP-0323 remaining gates.

</topic>

<topic id="production-checkpoint-policy" status="active" wp="WP-0333" updated_at="2026-10-04">

Operator approved implementation/testing on2026-10-04: "ok do that" and "you have my approval". Canonical normative policy is JSON phase_authority.production_checkpoint_policy, decision WP-0333-DEC-20261004-001. This supersedes earlier unselected-policy statements for this checkpoint scope only; original acceptance, earlier RED variance and unresolved live lock remain intact. A current-schema independent canonical counterpart and independent amendment review remain predecessors to product edits.

Select one counted persistent PASSIVE owner outside application FIFO, after schema/default-library readiness and before bridge/runners. Successful fixed-SQLite/WAL/FULL/no-close/auto0 handshake precedes runtime-writer policy; startup factory remains separate. A bounded manual channel shares scheduled500ms maintenance. Expose cycle/partial/busy/error/backlog/physical-WAL state and terminal receipts. Selected safeguards warn at64MiB uncheckpointed backlog, reject new writers at256MiB or10second stale owner/3 consecutive errors, check both enqueue and actual permit acquisition, preserve admitted atomic writes and permit read/owner recovery. Thresholds are policy selections, not measured hard storage/sync ceilings. Never silently fall back to stronger checkpoint modes or weaker durability.

Red team: queued writers must not bypass a newly failed owner; partial/busy recovery cannot clear a still-high backlog gate; negative/no-WAL frame results require truthful handling and saturating arithmetic. Owner panic must fail closed, and snapshots/control mutexes cannot span SQLite I/O. Shutdown reconciles admitted operations after runner join, then final PASSIVE/owner close and join before claiming drain; nonpreemptible I/O may exceed the shared budget and must report failure. Independent owning/canonical/crash and exact packaged live proof is required; no timeout-resolution claim follows from policy selection.

</topic>

<topic id="production-filename-policy" status="active" wp="WP-0333" updated_at="2026-10-03">

Independent canonical counterpart review permits selecting the conservative filename adapter separately from checkpoint policy. Exact scope, primary research, rejected options, hazards and owning/boundary validation are recorded in the JSON phase_authority.production_filename_policy and research_basis.production_filename_policy_2026_10_03. The common factory preserves canonical registry identity and uses I/O-free guarded dunce1.0.5 only for safe short local verbatim-drive paths; every uncertain or unsupported spelling is retained. No new raw-open exception, fixture TLS promotion, maintenance/durability change or live closure is selected. Latest101-write GREEN and earlier48-write/two-timeout RED both remain evidence; their variance is unexplained. Independent amendment review precedes implementation.

</topic>


<topic id="diagnostic-bounded-connection-reuse" status="proposed" wp="WP-0333" updated_at="2026-10-04">

Diagnostic counterpart only: canonical amendment `phase_authority.diagnostic_connection_reuse_counterpart_20261004`, stable ID WP-0333-AMD-REUSE-PROOF-20261004-001. Independent review precedes code. Off-by-default `wp0333_connection_reuse_proof` is limited to guarded disposable example/tests; desktop never enables it. Reuse exactly one counted writer/up to four counted read-only owners with unchanged FIFO/admission/durability/checkpoint safeguards. Restore manual-transaction/autocommit/query_only/FK/busy policy and quarantine panic/reset failures without losing admitted outcomes. Healthy lease return differs from physical close; owned checked close remains inside shared10second shutdown.

Exact PID150452 captured stacks resolve NTFS paging-resource exclusive acquisition and NtfsCommonCleanup/NtfsFsdCleanup for TIDs261404/227028/236224, plus shared paging acquisition/NtfsCopyReadA for TID18228. Exact FileIO/Flush TID240008 begins1791111802719 and matched IRP OperationEnd1791111804792 succeeds NtStatus0 after2072.679ms; exact local executable/PDB resolves winSync, walCheckpoint, sqlite3WalCheckpoint and the AppDatabase maintenance closure on that flush stack. Positive callee evidence supports a bounded connection-lifetime counterfactual; it does not identify the resource holder or prove a kernel/filter/storage root cause.245893 lost events retained; both actual admission failures precede recording. Raw CSwitch waitmode96 is outside the documented0/1 domain and TraceEvent3.2.8/main do not mask it; no packed-bit interpretation accepted.

Research: SQLite close/get_autocommit/query_only/WAL sources are recorded in the canonical amendment. Reuse existing rusqlite/common factory; reject wider bounds, weaker sync, cadence changes and unbounded caches. Red team: retained transaction/session state, leaked statement owners, panic/reset failure and delayed shutdown close require readback/quarantine/counting/ACK and no-orphan tests.

Acceptance: actual component reuse/policy reset/quarantine/shutdown plus matched PK180s3writer/3reader workload, independently reconciled exact stdout ACKs and69 protected tables/fixture trigger/sourceSHA. Production reuse remains unselected until independent current-input GREEN, separate reviewed production authority and operator approval. All original30minute/download/export/active-shutdown criteria and historic REDs remain; no closure claim.

Evidence: `.local/proofWP333/wpr_capture_01/decoded_stream_03/symbol_resolution_02/receipt.json`, `.local/proofWP333/wpr_capture_01/decoded_stream_03/flush_waits_raw_review.json`, `.local/proofWP333/wpr_capture_01/decoded_stream_03/flush_symbols_02/receipt.json`, `.local/proofWP333/wpr_capture_01/collector_receipt.json`.

</topic>


<topic id="diagnostic-reuse-pk180-result" status="IN_PROGRESS" wp="WP-0333" updated_at="2026-10-04">

Diagnostic-only feature counterpart PK180 actual GREEN:1392 external stdout ACKs independently persisted1392,5291 reads,0 errors; schema61/all69 protected tables and exact fixture INSERT-trigger delta PASS/sourceSHA unchanged. Matched prior nonreuse PK180 remains RED2 writer-admission failures/720ACK/2468reads, same backup/SQL/3writers/3readers/100ms/5s pin; binary/runtime differ by reviewed diagnostic feature implementation, not an OS rootcause proof. Actual reuse1writer/4readers,max1/4,6684 logical returns,5 checked physical closes,0quarantines/closeerrors/remainingowners,joined; maintenance final PASSIVE1309/1309,busy0,joined391ms,drainerrornull,pinned partial/recovery observed. Nine current feature tests GREEN and feature example build passed. Default baseline retained33 passing tests with1 original feature-off test RED; exact repaired feature-off test separately GREEN1 (no false full-suite rerun claim). Prior feature-round REDs retained; production connection lifetime remains unselected/unauthorized and live30minute running-download gate remains open.

Candidate EXE SHA `118088bbd37ed4bf3467c1a195d9a5546af9a0e0d9399f5fd87d29f07f7c3aa0`; runtime SHA `f7f0b7a785093bb44f22a90cbdc40690f7427a63e7bbb065980baf50cd08c4d7`; source backup SHA `a07dc9b6ded8cbf3b2730bc0abb0ada3e1802ef83208780603017c07cf847088`.

Evidence: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0333/20261004/production_owner_native_reuse_pk_driver.json`, `product/desktop/build_target/tool_artifacts/wp_runs/WP-0333/20261004/production_owner_native_reuse_pk_summary.json`, `product/desktop/build_target/tool_artifacts/wp_runs/WP-0333/20261004/production_owner_native_reuse_pk_independent.json`, `product/desktop/build_target/tool_artifacts/wp_runs/WP-0333/20261004/production_owner_native_reuse_pk_stdout.log`, `.local/proofWP333/reuse_validation_03/00_feature_reuse.log`, `.local/proofWP333/reuse_validation_03/01_feature_example.log`, `.local/proofWP333/reuse_validation_01/00_default_runtime.log`, `.local/proofWP333/reuse_validation_02/00_default_feature_off_exact.log`, `product/desktop/build_target/tool_artifacts/wp_runs/WP-0333/20261004/production_owner_native_pk_summary.json`.

Remaining: Independent current-input diagnostic counterpart acceptance review; Separate reviewed production lifetime amendment and explicit operator production approval; Exact installed30minute downloads/read/export/recovery acceptance; all original criteria preserved.

Independent /root/open_wps acceptance PASS:1392 unique external ACKs equal canonical,69-table preservation/exact dirty-trigger+1392, partial222/0 -> recovery915/915 -> final1309/1309 and5 distinct owned close receipts. Cross-thread native close kind is `xCloseUnknown`; no MainDB close-kind claim. Production authorization remains absent.

</topic>


<topic id="production-connection-lifetime" status="pending-review" wp="WP-0333" updated_at="2026-10-04">

Operator2026-10-04 explicitly approved production integration and verification. Canonical amendment `phase_authority.production_connection_lifetime_policy`, WP-0333-AMD-PRODUCTION-LIFETIME-20261004-001, requires independent review PASS before code. This changes the diagnostic-only production restriction for the newly approved production scope; historical diagnostic authority and prior results remain unchanged.

Select one counted reusable writer/up to four counted read-only owners within existing canonical AppDatabase, after database readiness before bridge/runners, with unchanged admission/execution/durability/maintenance safeguards. Reuse existing rusqlite/common factory; no pool dependency/raw-open exception and no desktop diagnostic feature. Restore transaction/session policies at lease return; manual rollback cannot claim durable ACK, panic/reset failure quarantines exact tracked owners, and committed outcomes survive later cleanup errors. Distinguish logical returns from physical close. Runner -> maintenance -> reusable checked close -> database drain retains one shared10second post-runner budget and truthful late/unjoined ownership.

Research/risks reuse the existing diagnostic refinement and primary SQLite sources; proven PK1801392exactACK/5291reads/0errors/all69tables is a scoped counterpart, not full live acceptance. Minimum controls remain owning reset/quarantine/shutdown tests, independent canonical preservation/crash proof and exact packaged30minute running downloads/history/first export/recovery. Preserve all original criteria, priorREDs, fixedSQLite/FULL/no-close/auto0/500ms/bounds and unchanged0.1.205. WP remains IN_PROGRESS.

</topic>


<topic id="production-reuse-pk180-scoped-result" status="IN_PROGRESS" wp="WP-0333" updated_at="2026-10-04">

Operator-approved/reviewed production ordinaryAPI connection lifetime implemented; current owning validation actualGREEN39 default runtime+9 feature reuse+3 desktop ordering tests and nonfeature production example build. Fresh ordinaryAPI PK180 counterpart child0/verifier0/driverPASS:1432 exact external stdout ACK independently retained1432,5272 reads,0errors; schema61/all69protected tables+only exact fixture dirty-trigger1432/sourcebackupSHA unchanged. Actual1writer4readers/max1/4,6705lease returns,5checked physical closes,0quarantine/closeerrors/remainingowners,joined. Maintenance finalPASSIVE1846/1846 busy0/ownerjoined756ms/drainerrornull/pinnedpartial+recovery observed. Final independent counterpart acceptance review currently running; verifier results are actual inspected scoped proof. All prior REDs/diagnosticPK1392 preserved; no OS-rootcause or installed/live30minute timeout-resolution claim. WP remainsIN_PROGRESS; production authority REVIEWED_APPROVED/operatorwaiver preserved exactly; next managedCoreOnly/nativeUpdate/preservation/originalrecovery/final30minrunning/visual/export/activeclose required.

Exact counterpart EXE SHA `d2c1cb891cd9cdd13003445baa315632536b633e66f1d6ba36f3e96e9a93252f`; runtime SHA `12676258f3af991617892d63e8ef0cc19c077b885c07322b6bb3590fc109724f`; factory SHA `bab5963e446f46b97c3fc40dd82d301b7e5792c2dbec311eabf5cc9bb40a381b`.

Evidence: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0333/20261004/production_owner_native_production_reuse_pk_driver.json`, `product/desktop/build_target/tool_artifacts/wp_runs/WP-0333/20261004/production_owner_native_production_reuse_pk_summary.json`, `product/desktop/build_target/tool_artifacts/wp_runs/WP-0333/20261004/production_owner_native_production_reuse_pk_independent.json`, `product/desktop/build_target/tool_artifacts/wp_runs/WP-0333/20261004/production_owner_native_production_reuse_pk_stdout.log`, `.local/proofWP333/production_validation_01/receipt.json`, `.local/proofWP333/production_validation_01/00_default_runtime.log`, `.local/proofWP333/production_validation_01/01_feature_reuse.log`, `.local/proofWP333/production_validation_01/02_desktop_ordering.log`, `.local/proofWP333/production_validation_01/03_production_example.log`.

</topic>
