---
file_id: WP-0315-v1
file_kind: work_packet
updated_at: 2026-08-23
---

<topic id="contract" status="backlog" version="v1" wp="WP-0315" updated_at="2026-08-23">

# Work Packet: WP-0315 — SurrealDB/RocksDB shadow benchmark and migration decision

## Metadata

- ID: WP-0315
- Owner: —
- Status: BACKLOG
- Created: 2026-08-23
- Refinement: `WP-0315_SURREALDB_ROCKSDB_SHADOW_BENCHMARK_AND_MIGRATION_DECISION_v1_REFINEMENT.md`
- Board: `../TASK_BOARD.md`
- Hard predecessor: WP-0312
- Related independent remediation: WP-0298, WP-0311, WP-0313, WP-0314
- Possible successor: separate operator-approved production-migration packet only after a favorable operator decision

## Intent

Answer whether embedded SurrealDB with RocksDB materially improves VoxVulgi's exact multi-lane database workload enough to justify its migration, recovery, Windows-build, resource, installer, and diagnostics costs—without touching the canonical SQLite database or implying it fixes the current freeze.

## Deliverables

- Complete engine-independent database operation, schema, invariant, and query-plan inventory from WP-0312.
- Pinned isolated SurrealDB 3.2.4 + embedded RocksDB shadow importer and benchmark harness; SurrealKV and remote modes excluded.
- Default-off Cargo feature `wp_0315_surrealdb_experiment`, explicitly flagged Tauri experiment mode, disposable manifest/root guard, governed feature-enabled desktop build, and quarantined full-offline installer evidence; ordinary/default builds and launches contain no candidate initialization.
- Independent separate-process exact-clone reconciliation plus import-side-effect rebuild, concurrency, conflict, durability, crash/recovery, backup/export/import, upgrade/rollback, resource, Windows build, and installer evidence.
- Two fair product-shaped comparisons against the hardened SQLite baseline: matched-admission engine isolation and each engine's pre-frozen evidence-safe product policy, using the observed nine-track/up-to-ten-worker arrival trace, backlog burst, and saturation.
- Machine-readable decision matrix and operator-readable verdict: `reject`, `defer`, or `candidate_for_operator_decision`.
- Source guard proving no production migration/default/dual-write/canonical mutation was introduced.

## Relevant files

- `governance/workflow/work_packets/WP-0312_SQLITE_RUNTIME_ACCESS_BOUNDARY_AND_LOCK_ATTRIBUTION_v1.md` and its refinement/proof
- `product/engine/src/db.rs` and database-using engine modules/tests
- `product/desktop/src-tauri/src/lib.rs`, `product/desktop/src/pages/DiagnosticsPage.tsx`, desktop shutdown/build/installer paths, and database/repair tools
- `governance/scripts/vv_watch.ps1` and `product/desktop/src-tauri/watcher/vv_watch.ps1`
- `product/desktop/src-tauri/Cargo.toml` and lockfile
- planned isolated harness root `governance/scripts/experiments/wp_0315_surrealdb/`
- planned default-off integration module `product/desktop/src-tauri/src/wp_0315_surrealdb_experiment.rs` and planned wrapper `governance/scripts/experiments/wp_0315_surrealdb/build_experimental_tauri.ps1`
- `governance/spec/PRODUCT_SPEC.md`, `governance/spec/TECHNICAL_DESIGN.md`, `governance/workflow/PROOF_STANDARD.md`, and `governance/release/OFFLINE_INSTALLER_BUILD_MANUAL.md`
- No standalone product-code/governance topology artifact was found on 2026-08-23; do not invent or claim one in this packet. Record the gap and route a separate topology-foundation proposal to the operator if later adoption needs one.

## Required implementation order

1. Verify WP-0312 hard predecessor, refresh primary-source research, and keep v1 pinned to 3.2.4 unless a v2 refinement explicitly changes the candidate.
2. Complete operation/invariant/schema/query-plan inventory and equivalence map.
3. Build the isolated pinned SurrealDB/RocksDB harness and the exact default-off `wp_0315_surrealdb_experiment` Tauri route with compile-time, explicit-flag, manifest, disposable-root, and production-path guards.
4. Implement fresh-namespace shadow import with explicit skipped-side-effect rebuild, interrupted-prefix replay, full handle drop/lock release, and separate-process reconciliation.
5. Implement exact current-search/query/transaction parity plus separately labelled forward-looking FTS and same-record/write-skew/predicate conflict paths.
6. Pre-register workload, durability, resource, fairness, confidence, and decision budgets; run matched-admission and product-policy cold/warm nine-track/ten-slot matrices.
7. Give both engines the same pre-frozen tuning/trial budget and rerun retained configurations.
8. Prove Windows toolchain, governed feature-enabled Tauri build, quarantined full-offline ISO/install, resources, shutdown, and a final fresh default-off governed build before closure.
9. Prove crash/recovery, live backup/export consistency, disk/RPO/RTO bounds, upgrade/reinstall/rollback, failures, and diagnostics replacement cost.
10. Compute the frozen verdict, record the experimental decision without changing production specs/defaults, complete independent adversarial review, and present any candidate to the operator without migrating.

## Non-goals

- Production SurrealDB adoption, dual-write, live shadowing, canonical conversion, schema/spec supersession, or default changes.
- SurrealKV while beta/Windows thread-safety limitations remain, or remote/cloud/server deployment.
- Comparing only against un-hardened SQLite, using synthetic-only data, or extrapolating generic database benchmarks.
- Treating Rust preference, graph/document features, or theoretical parallelism as correctness/performance proof.
- Replacing any current freeze/startup/diagnostics remediation packet.

## Acceptance and proof

- The refinement is the normative evidence, research, architecture, ROI, red-team, microtask, acceptance, decision, and proof contract.
- Canonical SQLite/user data remain untouched; source is a verified read-only backup/closed clone and candidate output is disposable and path-guarded.
- The optional candidate dependency is absent from default features; even a feature-enabled experimental binary initializes it only with exactly one `--wp-0315-surrealdb-experiment-manifest <absolute-path>` after fail-closed manifest/path validation. No experiment flag follows the ordinary SQLite path; any supplied-but-invalid experiment request, including use with a feature-disabled binary, terminates nonzero before Tauri or database initialization. Every agent-started non-experiment Tauri launch—including feature-enabled/no-flag and final default-off proof—uses `--agent-headless` with a preflighted owned absolute `VOXVULGI_AGENT_HEADLESS_BASE_DIR` or runs only inside an owned disposable VM/snapshot; an ordinary host launch is forbidden. Copies and the ISO stay under the ignored WP proof root; the governed feature build's transient `Current` and mandatory timestamped `old_versions` archive are explicitly quarantined with `NOT_FOR_PUBLICATION` provenance. Publication inputs must mechanically resolve only to the final fresh default-off `build_target/Current`.
- Candidate status requires complete correctness/recovery/build/installer/diagnostic gates plus the frozen material-benefit threshold on the exact product workload.
- When no `reject` condition exists, any unresolved durability alignment, license/distribution acceptance, proof budget, or recovery/rollback requirement produces `defer`; a `defer` condition never overrides an observed `reject` condition, and no qualitative preference can fill missing evidence.
- The only terminal outcomes are `reject`, `defer`, and `candidate_for_operator_decision`; none authorizes migration.
- `governance/workflow/PROOF_STANDARD.md`, user-data preservation, portability, and offline installer policies apply.

</topic>

<topic id="status-updates" status="active" version="v1" wp="WP-0315" updated_at="2026-08-23">

# Status updates

- 2026-08-23: Created from current VoxVulgi database/source inspection and current SurrealDB 3.2.4, SurrealKV 0.21.2, RocksDB, SQLite, Rust-toolchain, licensing, FTS, migration, and embedding primary sources.
- 2026-08-23: Current evidence supports retaining SQLite while WP-0312 hardens its access boundary and authorizes only this isolated SurrealDB/RocksDB decision experiment. Status is BACKLOG; no database migration or user-data mutation occurred.

</topic>
