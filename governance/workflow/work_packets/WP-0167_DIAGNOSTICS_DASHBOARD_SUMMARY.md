# Work Packet: WP-0167 - Diagnostics Dashboard Summary

## Metadata
- ID: WP-0167
- Owner: Codex
- Status: REVIEW
- Created: 2026-04-08
- Target milestone: UX Polish

## Intent

- What: Add a summary dashboard grid at the top of the Diagnostics page so operators can see critical status at a glance without scrolling through 15+ sections.
- Why: The Diagnostics page has 15+ card sections spanning 3800+ lines of code. Users looking for one piece of info (e.g., "is FFmpeg installed?") must scroll past technical details they don't need. The spec requires "render shell immediately and load sections incrementally" (Section 4.4).

## Scope

In scope:
- Add a top-of-page summary grid with 5-6 status tiles:
  1. **App version** — current version, up-to-date indicator
  2. **Voice packages** — Not installed / Installed / Install button
  3. **FFmpeg** — Ready / Missing with install action
  4. **Storage** — total app data usage with disk free context
  5. **Recent failures** — count with "View" link to failures section
  6. **Queue health** — running/queued/paused status
- Each tile links/scrolls to its detail section below.
- Keep all existing detail sections below the summary, unchanged.
- Use color coding: green (ready), yellow (action needed), red (error/missing).

Out of scope:
- Removing any existing Diagnostics sections.
- Changing section load behavior (already lazy).

## Acceptance criteria
- Top of Diagnostics shows a compact summary grid visible without scrolling.
- Each tile reflects live state from the existing data load.
- Clicking a tile scrolls to the corresponding detail section.
- `cargo check` + `npm run build` pass.

## Test / verification plan
- Visual snapshot showing summary grid at top.
- Verify tiles update after state changes (e.g., install FFmpeg, then tile updates).
