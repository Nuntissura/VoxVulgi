# Work Packet: WP-0169 - Options Page Consolidation

## Metadata
- ID: WP-0169
- Owner: Codex
- Status: BACKLOG
- Created: 2026-04-08
- Target milestone: UX Polish

## Intent

- What: Consolidate the Options page from 5 separate cards (base root + 4 feature roots) into a compact table view, and improve the YouTube auth UX.
- Why: Each feature root card shows Status/Effective/Default/Override with identical layout, creating visual repetition. The spec says "feature panes should show the resolved effective path but should not own or duplicate the root-configuration card" (Section 4.1). The YouTube cookie textarea has no guidance for non-technical users.

## Scope

In scope:
- Replace the 4 feature-root cards with a single table:
  | Feature | Effective path | Status | Override |
  - Each row has a "Change" button for the override.
- Keep the base storage root card as the primary configuration point.
- Improve YouTube auth UI:
  - Add radio buttons: "No authentication" / "Use browser profile cookies" / "Paste exported cookies"
  - Add placeholder text explaining cookie format
  - Add brief help text or link explaining how to export cookies from a browser
- Show free disk space next to the effective storage path.

Out of scope:
- Adding new configuration options.
- Changing storage root backend logic.

## Acceptance criteria
- Feature roots are shown in a single table instead of 4 separate cards.
- YouTube auth has radio-button selection with help text.
- Free disk space is visible next to storage paths.
- `npm run build` passes.

## Test / verification plan
- Visual snapshot of Options page showing consolidated layout.
- Verify override changes persist after page switch.
