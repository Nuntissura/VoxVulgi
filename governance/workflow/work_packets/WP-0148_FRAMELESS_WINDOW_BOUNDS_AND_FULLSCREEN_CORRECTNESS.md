# Work Packet: WP-0148 - Frameless window bounds and fullscreen correctness

## Metadata
- ID: WP-0148
- Owner: Codex
- Status: IN_PROGRESS
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
- 2026-03-12: Added frontend shell-window mode tracking so maximized/fullscreen states switch the visible shell from centered floating chassis mode to edge-aligned full-surface mode; awaiting operator side-by-side validation against the invisible-click-block regression.
- 2026-03-12: Removed inherited maximized/fullscreen shell padding and border/shadow chrome that left a transparent blocked rim inside the native window, and kept the top-right chrome cluster anchored through a two-row responsive shell layout. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0148/20260312_044237/`.
- 2026-03-13: Removed the floating-shell width cap and center alignment so the visible chassis now fills the native window surface in restored/maximized/fullscreen states instead of leaving transparent side margins inside the native bounds. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0148/20260313_000157/`.
- 2026-03-13: Added explicit full-surface sizing (`100vw`/`100vh` shell host in maximized/fullscreen mode plus non-shrinking top-right chrome) so the visible shell and chrome cluster stay locked to the native window surface during maximize/restore transitions. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0148/20260313_020200/`.
- 2026-03-22: Added viewport-based full-window inference so the shell still switches into the edge-aligned full-surface layout when the native maximize/fullscreen state arrives late or reports inconsistently, and hardened the shell host to occupy the entire webview surface (`100vw`/`100vh` with overflow clipped). This should eliminate the case where a larger transparent native window remains interactive outside the visible chassis while also keeping the chrome cluster compact in the top-right corner. Verification: `cargo check --offline --manifest-path product/desktop/src-tauri/Cargo.toml` and `npm run build`; side-by-side operator validation is still required.
