# Work Packet: WP-0153 - Localization home first-screen contract refresh

## Metadata
- ID: WP-0153
- Owner: Codex
- Status: DONE
- Created: 2026-03-22
- Target milestone: Post-0.1.6 localization-first recovery

## Intent

- What: Tighten the canonical Localization Studio first-screen contract so the home surface is explicitly the main operator dashboard rather than a sparse ingest page with generic shell chrome above it.
- Why: Fresh operator feedback and the recent screenshot confirm that the first screen still felt underrepresented as the product's main feature, even after earlier recovery work restored more item/output actions in code.

## Scope

In scope:

- Reconcile current operator feedback with the existing Localization-first product intent.
- Update spec/design language so the home surface clearly prioritizes current item, next action, latest output, workflow/readiness, recent work, and advanced entrypoints.
- Define the desired hierarchy between Localization home content and generic startup/recovery messaging.
- Queue the concrete follow-on implementation packet for the remaining UX remediation.

Out of scope:

- Shipping the follow-on frontend implementation itself.
- New backend/runtime research unrelated to the first-screen operator contract.

## Acceptance criteria

- `PRODUCT_SPEC.md` explicitly describes the Localization home surface as the main first-screen dashboard rather than only a lightweight ingest block.
- `TECHNICAL_DESIGN.md` defines how shell-level startup/recovery state should stay visible without visually displacing the main Localization workspace.
- `ROADMAP.md`, `TASK_BOARD.md`, and active localization recovery packets point to the follow-on implementation packet.

## Test / verification plan

- Governance review of the updated spec/design/task-board wording.
- Traceability check that the new implementation packet is linked from the active localization recovery tranche.

## Status updates

- 2026-03-22: Created from fresh operator screenshot feedback that the Localization home surface still read as sparse and secondary instead of as the application's main feature.
- 2026-03-22: Completed. Updated spec/design language to define the first-screen Localization dashboard and queued `WP-0154` for the remaining shell-status and orientation-strip implementation work.
