# Work Packet: WP-0102 - Startup and performance observability

## Metadata
- ID: WP-0102
- Owner: Codex
- Status: DONE
- Created: 2026-03-07
- Target milestone: Workflow and archive UX hardening

## Intent

- What: Add deterministic startup and runtime performance observability so operators can see what is loading and developers can inspect real resource usage.
- Why: The app still feels resource-intensive, and the current diagnostics do not yet give a detailed enough view of startup phases, tool state, or heavy runtime behavior.

## Scope

In scope:

- Add a startup progress indicator with named phases or loading items.
- Capture deterministic performance traces covering startup, pane activation, resource usage, and major errors.
- Surface an understandable installed-versus-loaded tool-state model so operators can tell whether packs are bundled, hydrated, loaded, or merely optional.
- Make the resulting logs suitable for deep post-hoc diagnosis from Diagnostics.

Out of scope:

- Full remote telemetry.
- Large performance refactors unrelated to observability itself.

## Acceptance criteria

- Startup shows a meaningful progress bar or phase list instead of opaque background loading.
- Diagnostics exposes trace output that can explain what consumed time or resources.
- Tool state is understandable enough that bundled-versus-downloaded-versus-loaded confusion is reduced.
- Observability remains local-first and inspectable.

## Test / verification plan

- Desktop build.
- Manual startup smoke with captured trace artifacts.
- Focused verification of trace export from Diagnostics.

## Status updates

- 2026-03-07: Created from operator feedback on startup opacity, resource intensity, and confusion around bundled versus loaded tools.
- 2026-03-07: Implemented and verified. Startup now reports named phases with progress, Diagnostics shows recent local trace rows with process snapshots, and tool lifecycle state is explained in the UI.
