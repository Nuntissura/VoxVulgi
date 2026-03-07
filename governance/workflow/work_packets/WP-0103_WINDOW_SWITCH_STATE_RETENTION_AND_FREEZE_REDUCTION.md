# Work Packet: WP-0103 - Window-switch state retention and freeze reduction

## Metadata
- ID: WP-0103
- Owner: Codex
- Status: BACKLOG
- Created: 2026-03-07
- Target milestone: Workflow and archive UX hardening

## Intent

- What: Stop reloading unchanged pane content on every window switch and further reduce UI freezes caused by repeated hydration work.
- Why: Operators still report that switching windows reloads content and freezes the app, even after earlier lazy-load and Diagnostics hardening work.

## Scope

In scope:

- Cache and retain pane state where appropriate instead of refetching or recomputing on every switch.
- Remove repeated path, subscription, or artifact hydration work that can be reused safely.
- Reduce freeze risk during normal navigation across the main windows.

Out of scope:

- Broad startup observability changes beyond what is covered by `WP-0102`.
- Large architectural rewrites unrelated to the observed pane-switch behavior.

## Acceptance criteria

- Window switching no longer causes avoidable full-content reloads for unchanged panes.
- The app remains responsive when moving between the core workspaces under normal data volumes.
- Shared state such as folder selections and archive data remains stable across switches.

## Test / verification plan

- Desktop build.
- Manual pane-switch smoke across the main windows with representative library/archive state.

## Status updates

- 2026-03-07: Created from operator feedback that window switching still reloads content and freezes the app in normal use.
