# Work Packet: WP-0104 - Desktop shell drag and resize ergonomics

## Metadata
- ID: WP-0104
- Owner: Codex
- Status: DONE
- Created: 2026-03-07
- Target milestone: Workflow and archive UX hardening

## Intent

- What: Refine the desktop shell so resize and drag behavior supports normal text selection, scrolling, and corner resizing.
- Why: The current drag-anywhere behavior interferes with text selection and scrollbar use, while the corner resize affordance is too small and partly outside the practical work surface.

## Scope

In scope:

- Increase the effective bottom-right resize hitbox and keep it reachable within the app work surface.
- Limit drag-region behavior to the true background or chrome layer instead of all content blocks.
- Preserve text selection, error-code copying, and normal scrollbar interaction inside content modules.

Out of scope:

- Full custom window chrome redesign.
- Non-desktop platforms.

## Acceptance criteria

- Operators can resize diagonally from the corner without hunting for a tiny off-surface hitbox.
- Text and log content can be selected and copied without accidentally dragging the whole app.
- Scrollbars remain usable because content panels are no longer drag targets.

## Test / verification plan

- Desktop build.
- Manual UI smoke for drag, selection, scroll, and corner-resize behavior.

## Status updates

- 2026-03-07: Created from operator feedback on drag-anywhere interference and the undersized corner resize affordance.
- 2026-03-07: Implemented chrome-only dragging plus a larger in-surface bottom-right resize handle so text selection and scrollbars are not treated as drag regions. Verified with `npm run build`, `cargo test -q` in `product/desktop/src-tauri`, and `cargo test -q` in `product/engine`. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0104/20260307_033244/summary.md`.
