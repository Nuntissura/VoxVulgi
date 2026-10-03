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

<topic id="production-filename-policy" status="active" wp="WP-0333" updated_at="2026-10-03">

Independent canonical counterpart review permits selecting the conservative filename adapter separately from checkpoint policy. Exact scope, primary research, rejected options, hazards and owning/boundary validation are recorded in the JSON phase_authority.production_filename_policy and research_basis.production_filename_policy_2026_10_03. The common factory preserves canonical registry identity and uses I/O-free guarded dunce1.0.5 only for safe short local verbatim-drive paths; every uncertain or unsupported spelling is retained. No new raw-open exception, fixture TLS promotion, maintenance/durability change or live closure is selected. Latest101-write GREEN and earlier48-write/two-timeout RED both remain evidence; their variance is unexplained. Independent amendment review precedes implementation.

</topic>
