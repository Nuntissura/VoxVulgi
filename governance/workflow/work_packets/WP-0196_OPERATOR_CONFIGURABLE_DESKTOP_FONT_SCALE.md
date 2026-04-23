# Work Packet: WP-0196 - Operator-configurable desktop font scale

## Metadata
- ID: WP-0196
- Owner: Codex
- Status: IN_PROGRESS
- Created: 2026-04-23
- Target milestone: Desktop accessibility and operator comfort

## Intent

- What: Add a real in-app desktop font-size preference instead of relying only on a single hard-coded global scale.
- Why: The baseline readability fix helped, but operator feedback still wants a tunable font size that can be adjusted for different displays and viewing distances.

## Scope

In scope:
- Add a desktop font-scale preference exposed in-app.
- Persist the preference and apply it app-wide.
- Keep the visible app-version change from `WP-0190` intact.
- Verify that the most common shell, form, table, and page layouts still behave correctly at supported sizes.

Out of scope:
- A full accessibility settings suite beyond font scale.
- Per-page typography themes or a complete design-system rewrite.

## Acceptance criteria
- Operators can change the desktop font scale from within the app.
- The preference persists across restarts.
- The shell and major pages remain usable at the supported font-scale options.
- Desktop build verification passes and focused manual smoke covers the setting.

## Test / verification plan

- Inspect the current global font implementation from `WP-0190`.
- Add the font-scale preference and persistence wiring.
- Re-run desktop build verification plus focused desktop smoke on the main pages.

## Status updates

- 2026-04-23: Created after post-`WP-0190` operator feedback asked for a real adjustable font size instead of only the new default scale.
- 2026-04-23: Implemented a first desktop font-scale preference in `OptionsPage` plus app bootstrap wiring. The chosen percentage now persists locally and applies app-wide through the root CSS scale variable. `npm run build` passed.
