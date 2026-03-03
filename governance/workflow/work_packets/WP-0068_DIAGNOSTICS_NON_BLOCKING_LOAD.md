# Work Packet: WP-0068 - Diagnostics non-blocking load and readiness states

## Metadata
- ID: WP-0068
- Owner: Codex
- Status: BACKLOG
- Created: 2026-03-03
- Target milestone: Stabilization sprint (diagnostics UX)

## Intent

- What: Make Diagnostics open immediately with non-blocking section loading and clear readiness states.
- Why: Diagnostics currently appears to freeze while loading tools/data on demand, which blocks recovery workflows.

## Scope

In scope:

- Render Diagnostics shell instantly with per-card loading skeletons.
- Load diagnostic modules incrementally (versions, storage, models, logs, traces).
- Add explicit "loading / ready / failed" states per diagnostics section.
- Avoid synchronous filesystem scans on the UI thread.

Out of scope:

- New diagnostics features outside existing sections.
- Changes to log retention policy.

## Acceptance criteria

- Entering Diagnostics does not freeze the UI.
- Each diagnostics section reports its own state and can fail independently.
- Users can still navigate away while diagnostics data continues loading.

## Test / verification plan

- Manual smoke with large logs/cache/model folders.
- Verify diagnostics actions still function (export bundle, open folders, clear cache where applicable).
- `npm run build` in `product/desktop`.

## Status updates

- 2026-03-03: Created.
