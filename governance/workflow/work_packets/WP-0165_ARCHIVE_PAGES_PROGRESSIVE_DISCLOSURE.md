# Work Packet: WP-0165 - Archive Pages Progressive Disclosure

## Metadata
- ID: WP-0165
- Owner: Codex
- Status: REVIEW
- Created: 2026-04-08
- Target milestone: UX Polish

## Intent

- What: Add Quick/Advanced toggle to Video Archiver, Instagram Archiver, and Image Archive pages so the default view is simple.
- Why: Video Archiver shows 6+ card sections (legacy reconciliation, batch URLs, presets, groups, subscriptions, folder mapping) all at once. 80% of users just want to paste URLs. The spec says archive windows "should not duplicate Localization Studio ingest controls" and should focus on their core purpose.

## Scope

In scope:
- Add a Quick/Advanced toggle at the top of each archive page.
- **Video Archiver Quick mode**: Show only the batch URL input, output folder, and preset selector. Hide subscriptions, groups, legacy reconciliation, and advanced preset management.
- **Instagram Archiver Quick mode**: Show only the batch URL input with auth field. Hide recurring subscriptions.
- **Image Archive Quick mode**: Show only the URL input and output folder. Hide crawl depth, delay, cross-domain, skip keywords behind an "Advanced options" collapsible.
- Move legacy archive reconciliation below subscriptions or into a collapsible section (it's niche, not a daily-use feature).
- Persist the Quick/Advanced choice in localStorage.

Out of scope:
- Changing subscription or preset functionality.
- Removing any existing controls (they stay in Advanced mode).

## Acceptance criteria
- Each archive page defaults to Quick mode with minimal controls.
- Advanced mode shows all existing controls unchanged.
- Toggle persists across page switches.
- `npm run build` passes.

## Test / verification plan
- Visual snapshot of each archive page in Quick and Advanced mode.
- Verify localStorage persistence after page switch.
