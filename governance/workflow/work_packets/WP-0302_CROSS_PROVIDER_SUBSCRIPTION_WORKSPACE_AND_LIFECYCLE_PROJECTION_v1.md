---
file_id: WP-0302-v1
file_kind: work-packet
updated_at: 2026-08-09
---

<topic id="contract" status="backlog" version="v1" wp="WP-0302" updated_at="2026-08-09">

# Work Packet: WP-0302 — Cross-provider subscription workspace and lifecycle projection

## Metadata

- ID: WP-0302
- Owner: Codex
- Status: DONE
- Created: 2026-08-09
- Refinement: `WP-0302_CROSS_PROVIDER_SUBSCRIPTION_WORKSPACE_AND_LIFECYCLE_PROJECTION_v1_REFINEMENT.md`
- Board: `../TASK_BOARD.md#wp-0302`
- Dependencies: WP-0280, WP-0281, WP-0282, WP-0284, WP-0300, WP-0301

## Intent

Replace the chaotic provider-specific subscription document with one bounded, accessible, provider-neutral master-detail workspace while preserving canonical provider-specific behavior and data.

## Base scope

- Implement the projection, capability contract, toolbar/master/detail hierarchy, backend filtering and canonical action receipts, YouTube migration, adapter slots, and all proof requirements in the refinement.
- Preserve current subscription rows, groups, media, memberships, lifecycle, job history, destinations, and recurring behavior.

## Required implementation order

1. Existing-control map and provider contract.
2. Backend projection/filter/action receipts.
3. Shared UI with YouTube parity.
4. Provider adapter slots/manual/a11y.
5. Live 262-row and packaged proof.

## Acceptance and proof

- The refinement is normative.
- No provider implementation may redefine canonical totals or selection from rendered rows.
- No old control is removed until its replacement path and semantic audit are proven.

</topic>

<topic id="status-updates" status="active" version="v1" wp="WP-0302" updated_at="2026-08-09">

# Status updates

- 2026-08-09: Created from exact current source/schema/count inspection and historical packaged UI evidence. No subscription or product code changed.

</topic>
