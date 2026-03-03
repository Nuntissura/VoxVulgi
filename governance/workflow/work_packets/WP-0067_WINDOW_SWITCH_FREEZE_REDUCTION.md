# Work Packet: WP-0067 - Window switch freeze reduction and mount lifecycle cleanup

## Metadata
- ID: WP-0067
- Owner: Codex
- Status: BACKLOG
- Created: 2026-03-03
- Target milestone: Stabilization sprint (UI responsiveness)

## Intent

- What: Remove UI stalls when switching between windows/panels.
- Why: Momentary freezes during navigation make the app feel unstable and resource-heavy.

## Scope

In scope:

- Replace "mount-everything + hide with CSS" behavior where it causes stalls.
- Add route/window-level lazy loading and controlled prefetching.
- Suspend polling/workers for non-active windows where safe.
- Add lightweight timing instrumentation for window switch latency.

Out of scope:

- Full visual redesign.
- Engine-level pipeline optimization unrelated to navigation.

## Acceptance criteria

- Window switching no longer causes visible multi-second freezes.
- Non-active window background activity is reduced and observable in diagnostics/dev logs.
- Switch latency metrics are captured for regression tracking.

## Test / verification plan

- Manual profiling while switching across all primary windows.
- Verify no regression in state persistence when unmount/remount occurs.
- `npm run build` in `product/desktop`.

## Status updates

- 2026-03-03: Created.
