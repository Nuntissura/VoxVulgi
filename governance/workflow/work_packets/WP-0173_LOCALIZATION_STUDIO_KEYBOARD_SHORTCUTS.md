# Work Packet: WP-0173 - Localization Studio Keyboard Shortcuts

## Metadata
- ID: WP-0173
- Owner: Codex
- Status: REVIEW
- Created: 2026-04-08
- Target milestone: UX Polish

## Intent

- What: Add keyboard shortcuts for the most common Localization Studio actions.
- Why: Operators perform dozens of clicks per item with no keyboard alternatives. Shortcuts reduce friction for daily production use and make the workflow feel professional.

## Scope

In scope:
- Ctrl+Enter — Start / continue localization run
- Ctrl+Shift+E — Export selected outputs
- Ctrl+Shift+R — Refresh readiness
- Ctrl+1 through Ctrl+5 — Jump to Track, Voice Basics, Run, Outputs, Artifacts
- Shortcuts only active when editor is visible and focus is not in an input/textarea/select
- Visible shortcut reference in the Workflow Map card (collapsible)

## Acceptance criteria
- All listed shortcuts work when the editor is active.
- Shortcuts do not fire when typing in text fields.
- Shortcut reference is visible in the UI.
- `npm run build` passes.
