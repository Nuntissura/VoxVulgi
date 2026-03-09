# Work Packet: WP-0137 - Shell move affordance and panel-scroll polish

## Metadata
- ID: WP-0137
- Owner: Codex
- Status: DONE
- Created: 2026-03-09
- Target milestone: Installer smoke remediation tranche

## Intent

- What: Clarify how the desktop shell can be moved and ensure scrolling behavior is aligned with panel-level interaction expectations.
- Why: Installer smoke shows that the current drag model still does not match the operator's mental model of the background layer, and panel scrolling remains too implicit.

## Scope

In scope:

- Explicit move affordance or draggable shell handle.
- Clear drag-region semantics that do not interfere with text selection or scrollbars.
- Panel/block scrollbar polish where dense content needs local scrolling.

Out of scope:

- Reverting back to drag-anywhere behavior that breaks selection and scrolling.

## Acceptance criteria

- Operators can reliably move the app using an explicit affordance.
- Scrollbars and text-selection behavior remain intact.
- Panel-level scrolling is clearer where horizontal or dense content would otherwise be obscured.

## Test / verification plan

- Desktop build and manual interaction verification.
- Proof bundle with app-boundary interaction notes.

## Status updates

- 2026-03-09: Created from installer smoke findings around move/drag/background semantics and per-panel scrolling.
- 2026-03-09: Implemented an explicit `Move window` shell handle, removed the ambiguous whole-topbar drag behavior, and upgraded dense subscription tables to keep scrolling local to the panel with pinned action columns. Proof: `product/desktop/Build Target/tool_artifacts/wp_runs/WP-0137/20260309_164659/`.
