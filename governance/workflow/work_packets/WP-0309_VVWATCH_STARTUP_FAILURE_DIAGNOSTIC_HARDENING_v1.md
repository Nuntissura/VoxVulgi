---
file_id: WP-0309-v1
file_kind: work_packet
updated_at: 2026-08-23
---

<topic id="contract" status="done" version="v1" wp="WP-0309" ingestable="true" updated_at="2026-08-23">

# Work Packet: WP-0309 — vvwatch startup failure diagnostic hardening

- ID: `WP-0309`
- Status: `DONE`
- Owner: `Codex`
- Refinement: `WP-0309_VVWATCH_STARTUP_FAILURE_DIAGNOSTIC_HARDENING_v1_REFINEMENT.md`
- Board: `../TASK_BOARD.md`

## Deliverable

Extend the existing sibling watcher so a no-context model can distinguish a startup process exit, a stale diagnostic projection, a startup-phase database lock, and a still-running schema migration without perturbing that migration through its own database probe.

## Relevant files

- `governance/scripts/vv_watch.ps1`
- `product/desktop/src-tauri/watcher/vv_watch.ps1`
- `governance/scripts/test_vv_watch.ps1`
- `AGENTS.md`
- `CLAUDE.md`

## Acceptance criteria

```yaml
criteria:
  startup_truth:
    - latest phase states, errors, and incomplete phases are structured
    - startup database-lock errors are explicit
  lifecycle_truth:
    - observed PIDs and exit transitions survive to final summary
    - stale bridge PID evidence is retained
  version_truth:
    - stale reports cannot become live_app_version
    - a live bridge or observed executable remains authoritative
  non_interference:
    - DB probe is skipped while a live schema migration is pending/running
  parity:
    - governance and bundled watcher scripts have identical SHA256
```

</topic>

<topic id="status-updates" status="done" version="v1" wp="WP-0309" updated_at="2026-08-23">

# Status updates

- 2026-08-23: Packet opened from the v0.1.178 launch incident after the governed watcher reproduced an early process exit and a retry exposed an `offline_bundle` database lock concurrent with schema migration.
- 2026-08-23: Completed startup-state, lifecycle, stale-version, and non-interference hardening; synthetic self-test and live v0.1.178 watcher proof passed. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0309/20260823_watcher_hardening/summary.md`.

</topic>
