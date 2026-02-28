# Work Packet: WP-0061 - Thumbnail disk cache + Library virtualization (large-library performance)

## Metadata
- ID: WP-0061
- Owner: Codex
- Status: BACKLOG
- Created: 2026-02-28
- Target milestone: Phase 1 (downloader UX hardening)

## Intent

- What: Make the Library UI remain responsive with **tens of thousands** of items by using a disk-based thumbnail cache and list virtualization.
- Why: Large libraries can cause startup freezes and UI stalls if thumbnails are stored/decoded eagerly or stored as giant DB blobs.

## Scope

In scope:

- Store thumbnails on disk (e.g. `cache/thumbs/`) and reference them from SQLite (no BLOB thumbnail storage).
- Implement an LRU eviction policy for the thumbnail cache (bounded by size and/or age).
- UI: virtualize the Library list/grid and lazy-load thumbnails.
- Diagnostics: show approximate cache size and provide a “Clear thumbnail cache” action.

Out of scope:

- Changing media import pipeline semantics (beyond thumbnail handling).
- Provider-specific image extraction features (separate WP).

## Acceptance criteria

- Library remains usable (scroll/search/filter) with very large libraries without freezing.
- Thumbnail cache is bounded and can be cleared without deleting user media/library metadata.

## Test / verification plan

- Unit tests for cache keying + eviction selection.
- `cargo test` in `product/engine`
- `npm -C product/desktop run build`
- Manual smoke:
  - import many items,
  - confirm UI stays responsive and thumbnail cache grows within bounds.

## Status updates

- 2026-02-28: Created.

