# Work Packet: WP-0148 - Frameless window bounds and fullscreen correctness

## Metadata
- ID: WP-0148
- Owner: Codex
- Status: BACKLOG
- Created: 2026-03-12
- Target milestone: Post-0.1.6 smoke regression recovery

## Intent

- What: Repair the frameless desktop shell so maximize/fullscreen behavior and the actual clickable window surface match the visible app surface.
- Why: The current smoke shows that the undecorated floating-window shell still uses an oversized or mismatched native window bounds box, so maximizing does not truly fill the display width and invisible blocked surface can sit on top of neighboring apps.

## Scope

In scope:

- Diagnose the current mismatch between the visible frameless app surface and the underlying native window bounds.
- Repair maximize/fullscreen sizing so the visible app truly matches the usable window surface.
- Ensure no invisible native window area continues blocking clicks on adjacent visible applications in side-by-side layouts.
- Keep the frameless top-right chrome cluster anchored correctly when restored/maximized so native bounds changes do not displace shell controls.
- Preserve the existing frameless/no-OS-border design direction while correcting interaction bounds.

Out of scope:

- Returning to the default Windows bordered window model.
- Broad shell redesign unrelated to the bounds/fullscreen regression itself.

## Acceptance criteria

- Maximizing the app produces a true edge-aligned usable surface instead of only stretching in one dimension.
- No invisible portion of the native window blocks clicks on other visible applications beside or behind VoxVulgi.
- Frameless presentation remains intact after the bounds fix.
- The move affordance and window control cluster remain in the intended top-right location in restored and maximized states.
- The fix works together with move, resize, text selection, and scroll interactions.

## Test / verification plan

- Desktop app-boundary smoke in restored/maximized side-by-side layouts.
- Desktop build verification.
- Proof bundle with final shell-behavior notes and exact verification scenarios.

## Status updates

- 2026-03-12: Created from operator feedback that the frameless shell still occupies an invisible bounding box, breaking maximize/fullscreen correctness and blocking interaction with adjacent apps.
- 2026-03-12: Expanded from operator screenshot feedback that the shell-control cluster also drifts away from the intended top-right position.
