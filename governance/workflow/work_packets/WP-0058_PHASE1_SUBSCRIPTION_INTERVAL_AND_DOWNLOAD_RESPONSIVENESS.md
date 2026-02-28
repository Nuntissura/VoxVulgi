# Work Packet: WP-0058 - Phase 1: Subscription interval + download responsiveness hardening

## Metadata
- ID: WP-0058
- Owner: Codex
- Status: DONE
- Created: 2026-02-25
- Target milestone: Phase 1 (downloader UX)

## Intent

- What: Add a configurable subscription refresh interval input and remove known UI-thread blocking paths that cause long launch/queue freezes.
- Why: Current behavior can feel stalled at startup and when queueing downloads; users need controllable subscription cadence and fast queue actions.

## Scope

In scope:

- Add `refresh_interval_minutes` to YouTube subscriptions:
  - schema + migration,
  - engine model/validation + import/export behavior,
  - Library UI input/edit/display,
  - Tauri command payload compatibility.
- Responsiveness hardening for download flows:
  - make URL and Instagram batch enqueue non-blocking by avoiding pre-expansion in the enqueue command path.
- Startup responsiveness hardening:
  - move runner log-pruning off the startup critical path (best-effort background prune).

Out of scope:

- New automatic scheduler daemon for subscription polling.
- Full profiling suite or benchmark framework.
- Deep rewrite of downloader worker internals.

## Acceptance criteria

- Subscription editor has an interval input (minutes), persists per row, and survives pane switches/app restart.
- Export/import roundtrip preserves interval values.
- Queueing URL/Instagram batches returns quickly without minute-long UI blocking before jobs appear.
- App startup no longer waits for full log-prune completion before runner start.

## Test / verification plan

- `cargo test` in `product/engine`
- `cargo test` in `product/desktop/src-tauri`
- `npm -C product/desktop run build`
- Manual smoke:
  - save/edit subscription interval, switch panes, verify value persists,
  - queue a mixed URL batch and verify jobs appear immediately.

## Status updates

- 2026-02-25: Created and started.
- 2026-02-25: Added per-subscription `refresh_interval_minutes` across schema, engine upsert/import/export, and Library UI input/table.
- 2026-02-25: Queue-all-active now respects per-subscription interval using `last_queued_at_ms`, with direct queue-one still available.
- 2026-02-25: Hardened responsiveness by moving runner log prune off startup critical path and switching URL/Instagram enqueue to raw non-blocking enqueue path.
- 2026-02-25: Verified with `cargo test` in `product/engine`, `cargo test` in `product/desktop/src-tauri`, and `npm -C product/desktop run build`.
