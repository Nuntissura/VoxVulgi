# Work Packet: WP-0066 - Window architecture refresh with Localization Studio first

## Metadata
- ID: WP-0066
- Owner: Codex
- Status: DONE
- Created: 2026-03-03
- Target milestone: Stabilization sprint (information architecture)

## Intent

- What: Reorder and split top-level windows so Localization Studio (Dub/CC) is the first and primary workspace.
- Why: Current structure hides the core value proposition and makes navigation ambiguous.

## Scope

In scope:

- Define and implement top-level window/workspace order:
  1) Localization Studio (Dub/CC) - default first window
  2) Video Ingest (local ingest + YouTube + playlists/maps/subscriptions)
  3) Instagram Archive
  4) Image Archive
  5) Jobs/Queue
  6) Diagnostics
- Persist last active window while preserving Localization Studio as first-run default.
- Update labels/descriptions so users understand each window purpose.

Out of scope:

- Deep redesign of inner tools in each window.
- Changes to ingestion engine behavior.

## Acceptance criteria

- Fresh app launch lands on Localization Studio.
- Navigation exposes the window list above in stable order.
- Video ingest capabilities are consolidated under a single Video Ingest window.

## Test / verification plan

- Manual navigation smoke on cold start and relaunch.
- Verify persisted active window restore.
- `npm run build` in `product/desktop`.

## Status updates

- 2026-03-03: Created.
- 2026-03-03: Implemented top-level navigation order with `Localization Studio` first, split ingest/archive/media windows, and persisted active window state with first-run default to Localization Studio. Verified with desktop build.
