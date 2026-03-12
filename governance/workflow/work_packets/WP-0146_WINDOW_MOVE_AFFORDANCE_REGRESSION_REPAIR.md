# Work Packet: WP-0146 - Window move affordance regression repair

## Metadata
- ID: WP-0146
- Owner: Codex
- Status: BACKLOG
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
