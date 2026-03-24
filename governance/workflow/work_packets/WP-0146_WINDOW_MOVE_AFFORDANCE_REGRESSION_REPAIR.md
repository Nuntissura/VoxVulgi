# Work Packet: WP-0146 - Window move affordance regression repair

## Metadata
- ID: WP-0146
- Owner: Codex
- Status: IN_PROGRESS
- Created: 2026-03-12
- Target milestone: Post-0.1.6 smoke regression recovery

## Intent

- What: Repair the current window-move affordance so the app can actually be dragged reliably again without re-breaking text selection, scrollbars, or other shell interactions.
- Why: The latest smoke reports that the explicit move affordance is present but does not actually move the app, leaving no reliable way to drag the window.

## Scope

In scope:

- Diagnose the current shell move wiring in the desktop window chrome.
- Repair move behavior for normal operator use.
- Restore the intended chrome layout so the move affordance and window controls live together in the top-right shell cluster.
- Preserve the previous fixes that kept text selection, scrollbars, and resize interactions working.

Out of scope:

- Large shell redesign unrelated to the move regression itself.

## Acceptance criteria

- The app can be moved reliably with the intended affordance.
- The move affordance is grouped with the minimize/maximize/close controls in the intended top-right chrome cluster rather than drifting to another area of the shell.
- Text selection, panel scrolling, and corner resize behavior remain intact after the repair.
- No hidden background drag-region regression reappears.

## Test / verification plan

- Desktop app-boundary smoke of move/select/scroll/resize interactions.
- Desktop build verification.
- Proof bundle with final interaction contract notes.

## Status updates

- 2026-03-12: Created from smoke finding `ST-051`.
- 2026-03-12: Expanded from operator screenshot feedback that the move affordance and window controls drifted away from the intended top-right chrome cluster.
- 2026-03-12: Restored the move affordance to the top-right chrome cluster, added explicit shell-window mode tracking, and switched the handle to a Tauri drag-region plus direct drag-call hybrid pending operator smoke confirmation.
- 2026-03-12: Hardened the move handle hit path by keeping drag-region markers on the handle children, using direct pointer-down drag start on the explicit handle, and pinning the chrome cluster into the top-right grid area even on narrower layouts. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0146/20260312_044237/`.
- 2026-03-12: Replaced the earlier button/drag-region hybrid with a dedicated chrome handle that starts window drag through the Tauri window API on `mousedown`, keeps maximize-on-double-click, and marks the control cluster as non-drag chrome. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0146/20260312_053054/`.
- 2026-03-22: Tightened the top-right chrome contract again after fresh operator regression feedback: the move handle is now a compact grip anchored beside the window controls, uses pointer-down drag start, keeps maximize-on-double-click, and no longer consumes enough width to drift the control cluster across narrow restored windows. Added explicit edge/corner resize hit zones and a Rust-side resize-drag fallback so move/resize no longer depend on a single frontend window API path. Verification: `cargo check --offline --manifest-path product/desktop/src-tauri/Cargo.toml` and `npm run build`; live desktop smoke still pending.
