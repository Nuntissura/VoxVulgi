---
file_id: WP-0312-v1
file_kind: work_packet
updated_at: 2026-08-23
---

<topic id="contract" status="backlog" version="v1" wp="WP-0312" updated_at="2026-08-23">

# Work Packet: WP-0312 — SQLite runtime access boundary and lock attribution

## Metadata

- ID: WP-0312
- Owner: —
- Status: BACKLOG
- Created: 2026-08-23
- Refinement: `WP-0312_SQLITE_RUNTIME_ACCESS_BOUNDARY_AND_LOCK_ATTRIBUTION_v1_REFINEMENT.md`
- Board: `../TASK_BOARD.md`
- Hard predecessors: WP-0223, WP-0224, WP-0226, WP-0280, WP-0309, WP-0310
- Historical authority/reuse evidence: superseded WP-0258; it is not a completion predecessor
- Umbrella/integration owner: WP-0298; it supplies incident authority and receives proof but is not a completion predecessor
- Coordinated packet: WP-0311
- Downstream evaluation: WP-0315

## Intent

Retain SQLite while replacing repeated, unowned runtime `open()+migrate()` access with one bounded engine-facing database service that preserves reads, writes, triggers, transactions, backups, and irreplaceable data while making contention attributable.

## Deliverables

- Complete production database-call inventory and explicit exception registry.
- `AppDatabase`/`DatabaseRuntime` boundary with bounded read executors, serialized writer admission, explicit overload/cancellation/shutdown semantics, lane-fair operation context, and per-lane receipts.
- Startup-only migration authority and source guards against runtime migration/read-write reads.
- Port of every production app-database caller to the boundary.
- Busy/lock candidate receipts, WAL/checkpoint health, and bounded idempotent retry contract.
- Isolated-clone integrity/backup/restore proof and exact packaged contention proof on owned disposable app data.
- Hardened SQLite benchmark baseline consumed by WP-0315.

## Required implementation order

1. Inventory and RED architecture/source guards.
2. Database service/executor foundation and concurrency safety tests.
3. Read-only projections, beginning with WP-0311's protection snapshot.
4. Invariant-group writer migration and external-I/O removal from transactions.
5. Startup-only migration authority and documented exceptions.
6. Attribution, WAL/checkpoint health, watcher parity, and retry controls.
7. Isolated-clone write/contention proof; packaged headless proof with an owned disposable base-dir override; agent-started normal-window proof only in an owned disposable VM/snapshot; and an observation-only current-profile cell on an already operator-started process.
8. Exact technical-design/Diagnostics help propagation, recorded missing-topology/model-manual proposal, and governed build.
9. Independent adversarial review and WP-0298/WP-0315 handoff.

## Non-goals

- Production SurrealDB migration, schema redesign, data cleanup, or changes to product job-queue scheduling, job identity, or retry semantics. The bounded internal database-admission queue defined by this packet is explicitly in scope and is not the product job queue.
- Moving the app database or NAS media.
- Deleting history or modifying third-party SQLite sources.
- Raising timeouts or guessing a lock holder as the remediation.

## Acceptance and proof

- The refinement is normative.
- No ordinary post-ready production migration or read-oriented read-write open may remain.
- All production app-database access must use the bounded service while preserving the fresh implementation-start schema and objects (dated 2026-08-23 baseline: v54), triggers, immediate-transaction invariants, backups, canonical records, and watcher behavior.
- Writer queue capacity, admission timeout, overload result, cancellation boundary, shutdown drain, lane priority/fairness, batching, checkpoint, read-pool, query/index, and no-starvation rules are frozen and proven before the baseline is handed to WP-0315; no accepted canonical write may be silently dropped or block indefinitely.
- Exact isolated-clone write/contention proof, disposable-root packaged proof, observation-only current-profile evidence, and independent adversarial proof are required; build-only proof is insufficient. Visible `--safe-mode` is not read-only because startup writes queue-pause state. The agent may not start or drive the current-profile process; any live current-profile mutation requires separate explicit operator authority and before/after reconciliation.
- `governance/workflow/PROOF_STANDARD.md`, user-data preservation rules, and `build_rules.md` apply.

</topic>

<topic id="status-updates" status="active" version="v1" wp="WP-0312" updated_at="2026-08-23">

# Status updates

- 2026-08-23: Created from a dated schema/database topology inspection, governed v0.1.179 incident evidence, direct production call-site audit, current SQLite/rusqlite primary documentation, and SurrealDB research recorded in backlog WP-0315. No SurrealDB benchmark or migration decision is complete, and no product code or database was changed by this packet authoring.
- 2026-08-23: Status is BACKLOG. The production engine remains SQLite unless a later operator-approved packet supersedes the technical design after WP-0315 proof.

</topic>
