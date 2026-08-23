---
file_id: WP-0312-REFINEMENT-v1
file_kind: refinement
updated_at: 2026-08-23
---

<topic id="operator-request-current-topology-and-authority" status="active" version="v1" wp="WP-0312" updated_at="2026-08-23">

# Operator request

- Remediate the confirmed database contention and repeated runtime initialization/read-path problems before deciding whether VoxVulgi needs a different database engine.
- Smooth the multiple job, subscription, UI, and diagnostics lanes that access canonical state in parallel.
- Preserve irreplaceable subscriptions, playlists, library metadata, jobs, provenance, and third-party source databases.
- Create an engine-neutral access boundary that makes a later database candidate measurable instead of allowing an incidental production migration.

# Verified current topology

- The canonical database is local at `<app-data>/db/app.sqlite`; it is not on the NAS.
- One 2026-08-23 checkpoint inspection snapshot measured the main file at 1,106,968,576 bytes and showed schema version 54, 67 application tables, 77 named indexes, 24 active triggers, and 47 foreign-key entries. This is a dated baseline, not a timeless live size: a later same-day live observation measured 1,107,058,688 bytes plus an 8,272-byte WAL. Execution must acquire and freeze a new consistent post-predecessor clone/inventory and preserve legitimate later schema growth.
- `product/engine/src/db.rs` declares `CURRENT_SCHEMA_VERSION=54` and 46 explicit migration steps.
- `db::open` creates a new read-write/create/full-mutex connection, applies a ten-second busy timeout, sets WAL and `synchronous=NORMAL`, and enables foreign keys.
- `db::open_readonly` already exists, uses read-only/full-mutex flags and a four-second busy timeout, and must not migrate.
- WP-0310 now performs schema/default-library initialization before the bridge and every runtime background component. That database-first ordering is complete and remains a hard regression gate.
- Runtime engine modules still contain hundreds of direct `db::open`, `db::open_readonly`, and `db::migrate` occurrences. Direct concrete `Connection`/`Transaction` parameters are spread across the engine; there is no swappable app-database service today.
- Read-oriented YouTube protection status, history, and replay paths still use `db::open()+db::migrate()` in production source.
- The governed v0.1.179 incident contains one app-side `instagram_subscription_auto_sync_error: database error: database is locked`, and its watcher recorded nine database-probe timeouts. These observations prove contention/unavailability; they do not identify the lock holder and do not prove SQLite caused native-window unresponsiveness.
- The job runner has nine tracks and up to ten default worker slots. Job claims use immediate transactions to preserve canonical job/identity ownership. Those invariants must survive any connection architecture change.
- The live schema uses triggers for provider-install authority, publication legality, rollup dirtiness, and other invariants. Operation-specific online backups use rusqlite backup plus integrity/hash checks.
- External `vvwatch` copies and several governance/repair tools directly assume SQLite and `app.sqlite`. Third-party 4KVDP SQLite sources are deliberately read-only and remain separate adapters.

# Authority and packet boundaries

- `governance/spec/TECHNICAL_DESIGN.md` currently selects SQLite and numbered transactional migrations.
- WP-0223 established WAL plus `synchronous=NORMAL` and removed one N+1 query; it explicitly retained SQLite for that packet.
- WP-0224 and WP-0226 established read-only panel paths. This packet completes the structural boundary rather than discarding those wins.
- Superseded WP-0258 contains prior lock/long-write/read-projection evidence and must be checked for preserved invariants, but it is not a completion predecessor. Completed WP-0280 owns the carried-forward Jobs/query/retry/archive-stat/render-bound scope and is the hard predecessor.
- WP-0309 owns watcher startup-phase truth and suppresses external probes during schema migration.
- WP-0310 owns database-first startup and is a completed hard predecessor.
- WP-0311 owns Diagnostics/Options demand scheduling and the combined protection snapshot product contract. WP-0312 supplies its bounded database implementation.
- WP-0298 remains the exact current-database/NAS integration closure owner. It is an authority/integration consumer, not a completion predecessor for WP-0312.
- WP-0315 depends on the hardened SQLite boundary and benchmark baseline. WP-0315 does not authorize changing the production engine.

</topic>

<topic id="research-selected-design-and-scope" status="active" version="v1" wp="WP-0312" updated_at="2026-08-23">

# Research basis

## Sources checked

- Current VoxVulgi schema, migrations, production database calls, transactions, triggers, backups, job runner, watcher probes, and database-focused tests.
- SQLite WAL concurrency/checkpoint behavior: `https://www.sqlite.org/wal.html`.
- SQLite isolation and single-writer semantics: `https://www.sqlite.org/isolation.html`.
- SQLite transaction modes and `BEGIN IMMEDIATE`: `https://www.sqlite.org/lang_transaction.html`.
- SQLite busy timeout and busy-handler behavior: `https://sqlite.org/c3ref/busy_timeout.html` and `https://sqlite.org/c3ref/busy_handler.html`.
- SQLite query-planner guidance: `https://www.sqlite.org/queryplanner.html`.
- SQLite backup API: `https://www.sqlite.org/backup.html`.
- rusqlite 0.32 API and backup feature documentation: `https://docs.rs/rusqlite/0.32.1/rusqlite/`.
- SurrealDB/RocksDB research is recorded separately in WP-0315; it is not authority for this packet's implementation.

## Relevant field patterns

- WAL permits readers and a writer concurrently but still has one writer. Adding more read-write connections or a generic pool does not create more SQLite write capacity.
- Long transactions, read-to-write upgrades, repeated schema/pragma work, and unbounded retry can turn the one-writer rule into unpredictable UI/background contention.
- A single bounded writer admission lane makes write ownership observable and prevents connection storms while preserving short immediate transactions.
- Read projections should use read-only connections, bounded indexed queries, and snapshot-consistent grouping where related values must agree.
- SQLite cannot generally name an external lock holder. Internal active-operation receipts can identify internal candidates; evidence must say `external_or_unknown` when ownership is not provable.
- Checkpoints are maintenance with their own latency behavior. Foreground reads must not accidentally perform unbounded checkpoint/rebuild work.

# Selected design

- Introduce one engine-facing `AppDatabase`/`DatabaseRuntime` boundary initialized only after app directories exist and WP-0310's schema/default-library gate passes.
- The boundary owns a bounded serialized writer lane, a small bounded set of read-only connections or equivalent bounded read executors, connection configuration, operation context, and deterministic shutdown.
- Before porting callers, select and freeze writer queue capacity, per-lane admission policy, admission timeout, overload result, cancellation boundary, shutdown drain deadline/failure receipt, priority classes, and fairness/no-starvation rule from observed lane traces plus stress tests. An accepted canonical write must never be silently dropped; overload is explicit before admission, admitted writes drain or produce a reconciled terminal failure, and callers cannot block indefinitely.
- Exact read-pool size, writer batching policy, checkpoint schedule/mode, busy/retry settings, and query/index configuration are selected from a focused symmetric tuning budget, capped in product configuration, and published as the WP-0315 SQLite baseline. Pool/queue capacity may not grow with page invocations or job count.
- Production code obtains `DatabaseReadContext` or `DatabaseWriteContext`/transaction closures from the service. New domain APIs must not accept or return a raw connection outside the database module boundary.
- All migrations remain explicit, numbered, and transactional, but production migration authority exists only at startup or an explicit governed maintenance/CLI path. Ordinary reads/writes cannot call `migrate`.
- The writer lane records lane ID, enqueue/admission/oldest-wait/transaction timing, overload/cancellation/drain outcome, batch identity, and terminal result while preserving immediate-transaction behavior for job/identity claims and safety-critical state changes. Filesystem, NAS, network, hashing, and child-process work occurs before admission or after commit, never while holding the writer lane unless an existing invariant demonstrably requires it.
- Read paths use read-only bounded projections. WP-0311's combined protection snapshot reads both operations under one consistent read boundary without migration or read-write pragmas.
- Add an internal database-operation registry with operation ID, request/span, mode, connection/worker ID, queue/open/busy/prepare/step/map/commit duration, row count, retry count, transaction behavior, and terminal outcome.
- Busy/locked errors attach a bounded snapshot of active internal operations. The receipt labels evidence as internal candidates, external/unknown, or watcher probe; it never claims a holder that SQLite did not identify.
- Bounded retry is permitted only for explicitly idempotent operations, after transaction duration is minimized, using capped backoff/jitter and trace receipts. Raising global busy timeouts is not the fix.
- WAL health exposes WAL bytes, checkpoint progress/outcome, long-reader evidence available from the application, and maintenance time. Checkpoint work stays outside foreground panel paths.
- Retain direct SQLite access only for isolated tests, schema/migration authority, explicit maintenance/backup tools, and read-only third-party SQLite adapters. Every exception is listed in a source guard.

# Scope edges

## In scope

- Production database access inventory and exception registry.
- App-owned database service, bounded reader/writer admission, and operation attribution.
- Removal of runtime `migrate` and read-write connections from read paths.
- Porting all production app-database callers to the boundary, including jobs, subscriptions, protection state/history, library, tools, cleanup, and startup workers.
- Preservation tests for triggers, immediate transactions, backups, migrations, canonical counts, and watcher behavior.
- Exact controlled contention proof on an isolated verified clone or disposable app-data root. Agent-started packaged headless proof must set `VOXVULGI_AGENT_HEADLESS_BASE_DIR` to that owned absolute root; agent-started normal-window proof runs only inside an owned disposable VM/snapshot. The current-profile cell observes only an already operator-started process through independent read-only probes; it never launches or drives `--safe-mode`, which writes queue-pause state.

## Non-goals

- SurrealDB or any other production engine migration.
- Schema/domain redesign for its own sake.
- Deleting or compacting job history, subscriptions, playlists, library metadata, media, or third-party databases.
- Changing product job-queue scheduling, job identity, retry, provider, or backup-recovery semantics. The bounded internal database-admission queue, its capacity/fairness/overload rules, and its proof are explicitly in scope and remain distinct from the product job queue.
- Moving the database to the NAS or moving NAS media.
- Hiding lock errors behind a larger timeout.

# Rejected options

- Replace SQLite before measuring the hardened boundary: confounds engine capability with current misuse and bypasses confirmed non-database remediation.
- Add an unbounded or large read-write connection pool: increases contenders while SQLite still has one writer.
- Convert every engine function to async/Tokio solely for database access: expands blast radius without proving benefit; bounded sync executors can preserve current domain APIs where appropriate.
- Keep `open()+migrate()` but memoize only `migrate`: still leaves unowned connection fan-out and weak attribution.
- One global mutex around all reads and writes: makes attribution easy but needlessly serializes WAL readers and harms UI usability.
- Infer the lock holder from the longest active operation: correlation is not proof.

</topic>

<topic id="roi-red-team-and-controls" status="active" version="v1" wp="WP-0312" updated_at="2026-08-23">

# High-ROI additions and reuse

- Engine-neutral database boundary.
  - Why high ROI: fixes current ownership/connection misuse and makes a future candidate implement the same domain contract.
  - Gap addressed: raw rusqlite calls are woven through production modules.
  - Reuse: existing domain functions, schema migrations, read-only opens, transactions, and tests.
  - Validation: source guard plus complete affected-module regression suite.
- Database operation registry and lock receipt.
  - Why high ROI: future incidents can identify internal pressure without new ad hoc traces.
  - Gap addressed: current lock error proves failure but not the holding/competing operation.
  - Reuse: WP-0298 request/span/phase envelope and WP-0309 watcher summary.
  - Validation: injected lock fixtures and exact watcher/app incident reconciliation.
- Short-transaction enforcement helper.
  - Why high ROI: every write lane benefits and future code gets a safe default.
  - Gap addressed: filesystem/child work can accidentally retain writer ownership.
  - Reuse: current immediate transactions and prefetch-before-claim pattern in `jobs.rs`.
  - Validation: transaction-duration guards and adversarial blocked-I/O fixtures.
- Source-level migration/read-mode contract.
  - Why high ROI: prevents gradual regression back to `open()+migrate()`.
  - Gap addressed: current conventions are distributed prose and easy to bypass.
  - Reuse: desktop contract harness and Rust module tests.
  - Validation: exact allowlist plus failing forbidden fixture.

# Red-team risks, scenarios, controls, and verification

- Writer actor becomes a global bottleneck, starves a lane, drops an accepted write, blocks indefinitely, or deadlocks through nested database calls.
  - Control: pre-frozen queue capacity/admission timeout/overload result, per-lane oldest-wait and fairness receipts, no nested admission, context-aware domain methods, deterministic cancellation boundary, bounded shutdown drain, and fail-fast detection of re-entry.
  - Verify: nested-call adversarial test; saturation/overload/cancel/shutdown fixtures; and nine-track/ten-slot observed-replay, backlog-burst, and starvation stress with canonical reconciliation.
- Port changes the atomicity of job/identity claims.
  - Control: preserve `TransactionBehavior::Immediate`, canonical predicates, trigger behavior, and one-transaction ownership update.
  - Verify: simultaneous claim race tests with exactly one owner and canonical reread.
- Read pool reuses a connection across threads unsafely.
  - Control: one connection per executor/lease according to rusqlite thread guarantees; never share a raw connection concurrently.
  - Verify: thread sanitizer where available plus high-concurrency stress and deterministic lease tests.
- Cancellation or timeout abandons an open transaction.
  - Control: RAII rollback and terminal registry cleanup; no cancellation checkpoint inside an uncommitted invariant block unless rollback is guaranteed.
  - Verify: injected cancel/panic at every transaction phase.
- Retry duplicates a non-idempotent write.
  - Control: explicit idempotency classification/key and retry allowlist; default is no retry.
  - Verify: conflict injection proves one canonical mutation/receipt.
- Operation registry itself becomes high-volume contention or leaks sensitive SQL/data.
  - Control: bounded in-memory entries, low-cardinality operation names, no SQL values/secrets/paths beyond existing redaction policy.
  - Verify: load test, retention bound, and redaction fixtures.
- Checkpoint maintenance stalls readers or grows WAL without bound.
  - Control: measured bounded checkpoint policy, no foreground checkpoint, and health thresholds derived from the current workload.
  - Verify: long-reader fixture, WAL growth/recovery, restart, and checkpoint timing receipt.
- Database-boundary conversion corrupts or weakens trigger/invariant behavior.
  - Control: port access only, not schema semantics; isolated clone; backups; `quick_check`/integrity; trigger and invariant adversarial suite.
  - Verify: acquire and reconcile the fresh implementation-start schema version and complete object inventory, canonical record counts, backup hash, restore, and exact negative-path trigger tests; schema v54 and its recorded object counts are only the dated 2026-08-23 comparison baseline.
- External watcher still times out because host I/O is saturated rather than SQLite locked.
  - Control: report phase-specific timeout/host evidence, not a fabricated lock owner.
  - Verify: separate injected lock and injected disk-delay fixtures.
- A proof labelled read-only mutates canonical queue state during startup.
  - Control: treat visible `--safe-mode` as mutating; require the headless disposable-base override for every agent-started headless process, require an owned disposable VM/snapshot for every agent-started normal-window process, and restrict current-profile work to independent read-only observation of an already operator-started process.
  - Verify: source guard for safe-mode queue-pause mutation; resolved-root/non-alias and disposable sidecar/database receipts; VM identity; process ownership/initiation receipt; and no agent-driven current-profile launch, navigation, mutation, or stop.

</topic>

<topic id="microtasks-acceptance-and-proof" status="active" version="v1" wp="WP-0312" updated_at="2026-08-23">

# Ordered microtask plan

1. Acquire a fresh consistent implementation-start backup/inventory and record schema version/object counts against the dated 2026-08-23 v54 baseline. Inventory every production app-database open, migrate, read, write, transaction, backup, checkpoint, and external-tool access. Classify canonical owner and create an exact allowed-exception registry.
2. Add RED source-contract tests for post-ready migration, read-oriented read-write opens, unbounded connection creation, and raw-connection leakage across the new boundary.
3. Create the `AppDatabase`/`DatabaseRuntime` API and test fixtures without porting production callers yet. Select and freeze writer queue capacity, lane admission/priority/fairness rules, timeout/overload result, cancellation boundary, shutdown drain, read-pool size, writer batching, checkpoint, busy/retry, and query/index settings through a symmetric predeclared tuning budget.
4. Prove connection/thread safety, no-starvation writer fairness, explicit overload, no silent accepted-write loss, bounded waiting, cancellation, shutdown drain/reconciliation, nested-call rejection, RAII rollback, idempotent retry classification, and operation-registry bounds.
5. Port WP-0311's protection snapshot and every other UI/status read to bounded read-only projections; verify indexed query plans and output parity.
6. Port writers by invariant group: jobs/identity claims, subscriptions/auto-sync, provider protection/history, library/provenance, tools/install authority, cleanup/reconciliation, and remaining background workers. Keep external I/O outside transactions.
7. Remove production runtime migration calls and make startup/maintenance migration authority explicit. Keep third-party SQLite readers isolated and read-only.
8. Add busy/lock attribution, WAL/checkpoint health, bounded retry receipts, and watcher projection changes with parity between both watcher copies.
9. On an isolated clone, run schema/object inventory, canonical counts, `quick_check`/integrity, trigger/invariant adversarial tests, backup/restore, interruption, and restart proof.
10. Run focused and full affected Rust/desktop tests, static source guards, and a representative nine-track/ten-slot concurrency benchmark.
11. Build the next governed semantic version. Run all auto-sync/write/contention scenarios against an isolated verified clone or disposable app-data root. For packaged headless proof, set `VOXVULGI_AGENT_HEADLESS_BASE_DIR` to that owned absolute root before launch and prove its database/config/trace/bridge sidecars resolve there. Run any agent-driven normal-window Diagnostics/Options plus five-minute `vvwatch` scenario only inside an owned disposable VM/snapshot. For the exact current profile, observe only an already operator-started process through independent read-only probes; do not start, navigate, safe-mode-toggle, mutate, or stop it. If an exact live auto-sync or other driven current-profile case is genuinely indispensable, stop and obtain explicit operator authority for that unchanged case, record the exact rows/tables that may change, and independently reconcile before/after canonical state.
12. Propagate the implemented database-runtime, migration-authority, operation-receipt, watcher, and recovery contracts into `governance/spec/TECHNICAL_DESIGN.md`, `product/desktop/src/pages/DiagnosticsPage.tsx`, `AGENTS.md`, and `CLAUDE.md` where their watcher/database operating contract changes. Keep `AGENTS.md` and `CLAUDE.md` semantically identical. Repo search on 2026-08-23 found no standalone product-code/governance topology or general built-in model-manual artifact; do not invent or claim those updates. Record each missing surface in proof and route a separate operator proposal for its canonical path before closing the architecture-propagation evidence.
13. Run independent adversarial review, remedy findings, write the WP-0312 proof bundle, publish the hardened SQLite baseline for WP-0315, and hand exact integration evidence to WP-0298.

# Acceptance criteria

- After WP-0310 reports database ready, no ordinary production path calls `db::migrate`; only the documented startup, maintenance/CLI, and test exceptions remain.
- No read-oriented protection, Diagnostics, Options, Jobs, Library, or subscription status projection opens the app database read-write.
- Production app-database callers use the bounded service; raw connection/transaction values do not cross its defined module boundary.
- Writer admission is bounded and observable, preserves every immediate-transaction/trigger invariant, and performs no avoidable filesystem/network/child work while holding a transaction.
- The frozen queue/admission contract names capacity, per-lane priority/fairness, admission timeout, overload result, cancellation boundary, shutdown drain, batching, checkpoint, read-pool, query/index, and retry settings. Saturation proves bounded waiting, no starvation, and no silently dropped accepted canonical write; shutdown independently reconciles every admitted operation to a terminal receipt and canonical state.
- The exact combined protection projection returns download/enumeration parity from one read boundary and maintains distinct operation identity.
- Injected internal contention produces a receipt with active internal candidates and phase timings. External/unknown contention remains explicitly labeled unknown.
- The isolated-clone/disposable-root concurrent Diagnostics/Options/auto-sync fixture has no unexplained `database is locked` failure. Any failure names the exact phase and candidate set.
- A five-minute exact watcher run has successful DB probes or a truthful phase-specific non-lock explanation; DB probe work remains suppressed during schema migration per WP-0309.
- The fresh implementation-start schema and object inventory (dated baseline: v54 with 67 tables, 77 indexes, 24 triggers, and 47 foreign-key entries) plus queue/identity semantics, backups, restore, and canonical records reconcile on the isolated clone. Later schema growth is recorded and preserved rather than forced into the dated counts.
- No user media, subscriptions, playlists, library metadata, job history, or third-party database is deleted or overwritten.

# Proof contract

- Verification class: high-risk code plus isolated-write app-boundary proof, disposable-root headless/VM normal-window proof, and agent-observation-only current-profile evidence.
- Required proof root: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0312/<run-id>/`.
- Required `summary.md` and `evidence.json` include source inventory/exception registry, schema/object counts, query-plan receipts, concurrent-operation traces, injected-lock evidence, backup/restore hashes, canonical reconciliation, exact commands, app version, and watcher path.
- Every agent-started headless process sets `VOXVULGI_AGENT_HEADLESS_BASE_DIR` to a preflighted owned disposable absolute root; every agent-started normal-window process runs only in an owned disposable VM/snapshot. Current-profile work occurs only after isolated-clone proof and is restricted to independent read-only observation of an already operator-started process; visible `--safe-mode` is explicitly mutating and is forbidden as a read-only proof path. Live auto-sync, driven navigation, or any other current-profile mutation requires separate explicit operator authorization naming the exact unchanged case plus before/after independent canonical reconciliation; ordinary "normal app behavior" is not authorization.
- Independent adversarial review is mandatory before `DONE`.
- Build-only, unit-only, a higher timeout, or absence of one lock error cannot close this packet.

</topic>
