---
file_id: WP-0310-v1
file_kind: work_packet
updated_at: 2026-08-23
---

<topic id="contract" status="done" version="v1" wp="WP-0310" ingestable="true" updated_at="2026-08-23">

# Work Packet: WP-0310 — database-first desktop startup

- ID: `WP-0310`
- Status: `DONE`
- Owner: `Codex`
- Refinement: `WP-0310_DATABASE_FIRST_DESKTOP_STARTUP_v1_REFINEMENT.md`
- Board: `../TASK_BOARD.md`

## Deliverable

Make schema migration and default-library initialization a hard predecessor to the agent bridge, offline-bundle hydration, watcher supervisor, and other runtime background work so first launch after an update cannot exit because startup components contend for SQLite.

## Acceptance criteria

```yaml
criteria:
  ordering:
    - database schema/default-library initialization completes before background startup
    - offline bundle work cannot begin before the database-ready boundary
  failure_truth:
    - schema/default-library failure is recorded as a db_schema startup error before setup exits
  regression:
    - focused Rust tests prove successful database readiness and guarded source ordering
  build:
    - governed desktop target build increments semantic version and records WP-0310
  app_boundary:
    - packaged headless launch reaches a healthy bridge with the built version
```

</topic>

<topic id="status-updates" status="done" version="v1" wp="WP-0310" updated_at="2026-08-23">

# Status updates

- 2026-08-23: Opened from the v0.1.178 first-launch incident after `vvwatch` proved concurrent `offline_bundle` and `db_schema` SQLite access.
- 2026-08-23: Database-first gate implemented; two focused Rust regressions passed; all six pack warmups passed; governed v0.1.179 executable/installer built; packaged headless trace, bridge, watcher, and visual proof passed. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0310/20260823_database_first_startup/summary.md`.

</topic>
