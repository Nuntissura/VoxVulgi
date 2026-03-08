# Work Packet: WP-0126 - Schema versioning and atomic config persistence

## Metadata
- ID: WP-0126
- Owner: Codex
- Status: BACKLOG
- Created: 2026-03-08
- Target milestone: Audit remediation tranche

## Intent

- What: Introduce explicit schema versioning and transactional migration structure, and replace in-place config/secret writes with atomic local persistence helpers.
- Why: `WP-0119` and `WP-0120` found additive migration drift and crash-sensitive config persistence.

## Scope

In scope:

- SQLite schema versioning strategy.
- Numbered/named migration steps with safer execution structure.
- Atomic write helpers for config, overrides, and secret files.
- Targeted migration/config tests.

Out of scope:

- Large product-surface redesign unrelated to persistence.

## Acceptance criteria

- DB schema evolution uses explicit version tracking.
- Config/override/secret persistence no longer relies on raw in-place writes.
- Migration and persistence helpers have focused regression coverage.

## Test / verification plan

- Engine tests for migration stepping and atomic-write helpers.
- Proof bundle with migration plan and verified code paths.

## Status updates

- 2026-03-08: Created from `WP-0119` and `WP-0120` persistence findings.
