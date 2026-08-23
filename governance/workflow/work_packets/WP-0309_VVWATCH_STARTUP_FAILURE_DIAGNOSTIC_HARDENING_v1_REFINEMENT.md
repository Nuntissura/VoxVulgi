---
file_id: WP-0309-REFINEMENT-v1
file_kind: refinement
updated_at: 2026-08-23
---

<topic id="operator-request-and-verified-incident" status="done" version="v1" wp="WP-0309" updated_at="2026-08-23">

# Operator request and verified incident

- Operator correction: use the governed `vvwatch.cmd` sibling watcher for the v0.1.178 launch failure.
- Operator extension: if the watcher needs more tools, add them.
- Live proof: normal launch PID `14836` started at 2026-08-23 02:21, briefly appeared, never established a healthy bridge, and exited without a Windows crash or Defender event.
- Successful retry PID `49360` reached app version `0.1.178` and schema version `54`.
- The successful retry logged `offline_bundle = error` with `database error: database is locked` while `db_schema = running`, then completed schema and job-runner startup.
- The first watcher summary incorrectly projected stale freeze-report version `0.1.169` as the live app version while no app process existed.
- The watcher console did not surface startup phase errors, incomplete startup phases, stale report provenance, or the observed process exit.

</topic>

<topic id="research-basis-and-selected-approach" status="done" version="v1" wp="WP-0309" ingestable="true" updated_at="2026-08-23">

# Research basis and selected approach

## Sources checked

- Repo authority: `AGENTS.md` / `CLAUDE.md` Sibling External Watch (WP-0242).
- Existing packet: `WP-0242_SIBLING_EXTERNAL_FREEZE_WATCHDOG.md`.
- Current implementation and tests: `governance/scripts/vv_watch.ps1`, shipped watcher twin, and `governance/scripts/test_vv_watch.ps1`.
- SQLite official documentation: transaction locking, WAL behavior, and `sqlite_schema` locking requirements.

## Selected approach

```yaml
reuse:
  - existing bounded trace reader
  - existing sample JSONL and summary JSON/Markdown
  - existing bridge PID sidecar and process snapshots
  - existing watcher self-test and shipped-script hash parity gate
add:
  - startup phase latest-state/error/incomplete summaries
  - process lifecycle and stale-bridge receipts across the full sample window
  - correct live-version selection from matching live evidence only
  - trace-first sampling and DB-probe suppression while schema migration is running
  - concise console signals for startup errors and process exits
reject:
  - a second watcher implementation
  - unbounded ETW capture by default
  - write probes against the operator database
  - treating historical freeze-report metadata as live process truth
```

</topic>

<topic id="scope-red-team-and-proof" status="done" version="v1" wp="WP-0309" ingestable="true" updated_at="2026-08-23">

# Scope, red team, and proof

## Scope

- Harden `vvwatch` diagnostics only; do not fix the product startup race in this packet.
- Keep all probes bounded, quiet, disk-agnostic, and read-only.
- Preserve existing output paths and fields; additions must be backward-compatible.

## Risks and controls

- Risk: watcher DB reads perturb schema migration.
  - Control: derive current startup phase from trace first and suppress the DB probe while live `db_schema` is `pending` or `running`.
- Risk: stale sidecars or reports create false live-version claims.
  - Control: require matching live PID/bridge evidence for live version; retain stale provenance separately.
- Risk: terminal sample erases evidence of a process that exited mid-watch.
  - Control: aggregate lifecycle evidence across every sample, not only the last sample.
- Risk: governance and bundled scripts drift.
  - Control: exact SHA parity remains a required self-test.

## Acceptance and verification

```yaml
acceptance:
  - synthetic stale-report fixture cannot populate live_app_version
  - startup error and incomplete phase fixtures appear in structured and Markdown summaries
  - schema-running fixture suppresses the DB probe with an explicit reason
  - lifecycle summary retains observed and exited PIDs across terminal no-process samples
  - governance and shipped watcher scripts remain byte-identical
verification:
  - powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File governance/scripts/test_vv_watch.ps1
  - short live vvwatch run against installed v0.1.178
  - inspect summary.json and summary.md
```

</topic>
