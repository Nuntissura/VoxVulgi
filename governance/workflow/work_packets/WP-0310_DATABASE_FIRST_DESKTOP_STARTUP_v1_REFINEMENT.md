---
file_id: WP-0310-REFINEMENT-v1
file_kind: refinement
updated_at: 2026-08-23
---

<topic id="request-and-evidence" status="done" version="v1" wp="WP-0310" updated_at="2026-08-23">

# Request and evidence

- Operator request: make the v0.1.178 launch remediation durable.
- Reproduced failure: normal launch PID `14836` exited during `db_schema` without establishing a bridge.
- Successful retry: PID `49360` logged `offline_bundle` database lock while `db_schema` ran, then completed schema migration.
- Source inspection: `run()` starts the agent bridge and offline hydration thread before `db::ensure_schema`; watcher supervision also starts before schema.

</topic>

<topic id="research-selected-approach" status="done" version="v1" wp="WP-0310" ingestable="true" updated_at="2026-08-23">

# Research and selected approach

- Repo sources: desktop startup implementation, engine schema/default-library functions, build rules, proof standard, and WP-0309 live evidence.
- External sources: official SQLite transaction, WAL, and schema-lock documentation confirm that concurrent readers/writers can return busy/locked rather than wait indefinitely.
- Selected approach: complete schema/default-library initialization synchronously, record success/error, then expose the bridge and start all runtime background work.
- Rejected: retry loops and longer busy timeouts, because they retain nondeterminism and mask incorrect startup ownership.

</topic>

<topic id="red-team-and-proof" status="done" version="v1" wp="WP-0310" ingestable="true" updated_at="2026-08-23">

# Red team and proof

```yaml
risks:
  - risk: startup appears slower because bridge/background hydration begins later
    control: schema was already synchronous; only competing work is deferred
  - risk: future refactor moves a DB consumer ahead of schema again
    control: source-order regression test and database-ready helper boundary
  - risk: schema failure remains silent
    control: emit db_schema error state before returning the setup error
proof:
  - focused desktop Rust tests
  - governed desktop target build with WP-0310
  - packaged headless health/state verification on an isolated root
```

</topic>
