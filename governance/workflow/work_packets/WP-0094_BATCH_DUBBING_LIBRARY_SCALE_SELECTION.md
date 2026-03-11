# Work Packet: WP-0094 - Batch dubbing library-scale selection

## Metadata
- ID: WP-0094
- Owner: Codex
- Status: DONE
- Created: 2026-03-06
- Target milestone: Phase 3 voice-workflow hardening

## Intent

- What: Remove hidden item-count caps from the batch dubbing picker and keep selection stable across large libraries.
- Why: Batch dubbing is specified for item sets and seasons, but the current picker only loads the first 500 items and silently trims selections on navigation.

## Scope

In scope:

- Load the full library into the batch item picker through paging or repeated page fetches.
- Remove silent batch-selection truncation tied to current-item navigation.
- Keep current batch queue semantics, template/cast-pack application, and follow-on QC/export behavior.

Out of scope:

- A separate dedicated batch-management window.
- New batch orchestration semantics beyond item selection/remediation.

## Acceptance criteria

- Operators can select batch items beyond the first 500 library entries.
- Navigating between items does not silently drop already-selected batch items.
- The current item can still be added to the batch automatically without truncating the selection.

## Test / verification plan

- Desktop build.
- Manual batch-picker smoke on synthetic large selection notes captured in proof summary.

## Status updates

- 2026-03-06: Created from remediation review after confirming a hard 500-item picker cap and selection truncation.
- 2026-03-06: Completed. Batch item loading now pages through the full library, and current-item auto-selection no longer truncates the batch selection state. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0094/20260306_213944/summary.md`.
