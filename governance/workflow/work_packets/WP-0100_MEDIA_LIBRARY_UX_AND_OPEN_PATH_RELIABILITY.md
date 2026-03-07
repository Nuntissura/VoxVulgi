# Work Packet: WP-0100 - Media Library UX and open-path reliability

## Metadata
- ID: WP-0100
- Owner: Codex
- Status: BACKLOG
- Created: 2026-03-07
- Target milestone: Workflow and archive UX hardening

## Intent

- What: Improve Media Library layout and make file-opening behavior reliable across storage locations.
- Why: Operators need direct, dependable access to archived media, and the current card layout wastes space while important open actions fail on some non-system drives.

## Scope

In scope:

- Fix `Open file` behavior for valid library media paths on internal non-system drives such as `D:`.
- Move key per-item actions underneath the item card so grid layouts remain compact and aligned.
- Add grouped browsing by subscription, playlist, or other source container so large feeds do not collapse into one undifferentiated stream.
- Add explicit media-type filters such as video and image.

Out of scope:

- Artifact browser changes outside Media Library.
- Major visual redesign unrelated to the stated layout and grouping problems.

## Acceptance criteria

- `Open file` works for valid local paths on secondary/internal drives.
- Media Library item cards use an under-card action layout that supports tighter side-by-side grids.
- Operators can browse media grouped by playlist/subscription or comparable source container.
- Media-type filtering is available and useful on large mixed libraries.

## Test / verification plan

- Desktop build.
- Manual UI smoke with media stored on multiple local drives.

## Status updates

- 2026-03-07: Created from operator feedback on D-drive file opening failures, oversized staggered cards, and the need for grouped browsing in large libraries.
