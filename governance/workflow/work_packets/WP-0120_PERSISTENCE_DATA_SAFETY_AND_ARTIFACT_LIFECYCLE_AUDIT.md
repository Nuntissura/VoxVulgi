# Work Packet: WP-0120 - Persistence, data safety, and artifact lifecycle audit

## Metadata
- ID: WP-0120
- Owner: Codex
- Status: DONE
- Created: 2026-03-08
- Target milestone: Repo audit tranche

## Intent

- What: Audit DB schema, migrations, app-data layout, backup behavior, resumability, and artifact cleanup/retention.
- Why: VoxVulgi manages large media libraries and derived outputs, so persistence mistakes create real operator risk.

## Scope

In scope:

- SQLite schema and migration safety.
- App-data and build-target artifact layout.
- Backup-first behavior, resumability, idempotency, and cleanup policies.
- Artifact discoverability vs working-file sprawl.

Out of scope:

- Full live migration execution on operator data.
- NAS modification or cleanup.

## Acceptance criteria

- A durable audit report exists under `product/desktop/Build Target/tool_artifacts/wp_runs/WP-0120/`.
- Findings identify any destructive or ambiguous persistence behavior.
- Cleanup/retention gaps and lifecycle ambiguities are documented with remediation proposals.

## Test / verification plan

- Static code and layout inspection.
- Limited local non-destructive verification where necessary.

## Status updates

- 2026-03-08: Created as stage 3 of the multi-stage repo audit tranche.
- 2026-03-08: Completed persistence, data safety, and artifact lifecycle audit. Proof captured under `product/desktop/Build Target/tool_artifacts/wp_runs/WP-0120/20260308_163100/`.
