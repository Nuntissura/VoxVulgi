# Work Packet: WP-0124 - Governance hygiene and manual smoke scope reconciliation

## Metadata
- ID: WP-0124
- Owner: Codex
- Status: BACKLOG
- Created: 2026-03-08
- Target milestone: Audit remediation tranche

## Intent

- What: Reconcile governance drift uncovered in `WP-0118`, including contradictory roadmap language, stale manual-smoke scope, stale proof references, and residual mojibake.
- Why: Governance has to be internally consistent before follow-on implementation tranches can be trusted.

## Scope

In scope:

- Canonical governance docs and roadmap/taskboard wording.
- `WP-0095` smoke scope expansion to cover newer voice-backend and benchmark surfaces.
- Refreshing stale `WP-0098` proof references.
- Removing remaining mojibake in canonical governance docs.

Out of scope:

- Product-code implementation changes.

## Acceptance criteria

- Canonical governance files no longer contradict the current spec stance.
- `WP-0095` reflects the real current operator surfaces that need manual smoke coverage.
- Stale proof references and mojibake are removed from canonical governance docs.

## Test / verification plan

- Static governance diff review.
- Proof bundle with before/after references and changed files.

## Status updates

- 2026-03-08: Created from `WP-0118` governance drift findings.
