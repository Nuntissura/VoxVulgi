# Work Packet: WP-0170 - Media Library Search and Filters

## Metadata
- ID: WP-0170
- Owner: Codex
- Status: REVIEW
- Created: 2026-04-08
- Target milestone: UX Parity

## Intent

- What: Add search and filter controls to the Media Library page to meet the spec requirements for library browsing.
- Why: The spec (Section 4.1) requires "search (title/tags/text), filters (language, status, date, source), collections/playlists, grouped browsing by source container". None of these are currently implemented in the Media Library UI. Users with large libraries have no way to find items except scrolling.

## Scope

In scope:
- Add a search input at the top of the Media Library that filters by title and file path (client-side for MVP, engine-side query for scale).
- Add filter dropdowns:
  - **Source**: YouTube / Instagram / Local import / All
  - **Status**: Has subtitles / Has translation / Has dub / Any
  - **Date range**: Last 7 days / Last 30 days / All time
- Add a "Sort by" selector: Date added / Title / Source
- Show active filter count as a badge.
- Wire search/filter state to the existing `library_list` engine query where possible, or filter client-side for MVP.

Out of scope:
- Collections/playlists (separate WP).
- Smart tags (separate WP).
- Grouped browsing by container (separate WP).
- Engine-side full-text search index.

## Acceptance criteria
- Search input filters library items by title in real-time.
- At least Source and Sort filters work.
- Filters persist across page switches via localStorage.
- `cargo check` + `npm run build` pass.

## Test / verification plan
- Import 5+ items from different sources, verify filters isolate correctly.
- Visual snapshot showing search bar and filter controls.
