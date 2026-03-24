# Work Packet: WP-0154 - Localization first-screen hierarchy and shell-status demotion

## Metadata
- ID: WP-0154
- Owner: Codex
- Status: IN_PROGRESS
- Created: 2026-03-22
- Target milestone: Post-0.1.6 localization-first recovery

## Intent

- What: Finish the remaining first-screen Localization UX work by giving the home surface a stronger operator-orientation layer and by demoting generic startup/recovery chrome so Localization Studio visually owns the default workspace.
- Why: The recent dashboard refactor improved the home surface, but the shell still spends too much first-screen vertical space on generic status cards and still lacks one very explicit `Now / Next / Last Output` orientation band.

## Scope

In scope:

- Add a first-screen orientation layer that makes the current item, recommended next action, and latest preview/deliverable obvious at a glance.
- Demote non-failure startup/recovery messaging from large content cards into more compact shell-level status affordances.
- Keep detailed startup and recovery controls accessible without letting them dominate the main Localization page.
- Preserve direct access to run controls, outputs, advanced tools, source media, and preview outputs from the first screen.

Out of scope:

- New localization backend/runtime features.
- Manual proof closeout for operator-heavy validation packets.

## Acceptance criteria

- Localization Studio is visually the dominant first-screen surface when the app is otherwise usable.
- The first screen exposes a clear `Now / Next / Last Output` orientation path for the current localization workflow.
- Startup/recovery state remains accessible, but no longer crowds the main Localization home surface during normal use.
- Desktop build verification passes after the UI changes.

## Test / verification plan

- Desktop build verification.
- Focused visual/operator review of the Localization first screen in the Tauri app.
- Follow-on manual proof remains tracked under the broader operator-smoke packets rather than closed here by build alone.

## Risks / open questions

- Compacting shell status too aggressively could make recovery controls harder to find if the hierarchy is over-corrected.
- The final closeout still depends on operator validation in the live frameless desktop shell.

## Status updates

- 2026-03-22: Created as the implementation follow-on to `WP-0153` after the first dashboard refactor still left startup/recovery chrome too dominant on the Localization first screen.
- 2026-03-22: Implemented the follow-on hierarchy tranche in the desktop app: Localization home now shows an explicit `Now / Next / Last Output` orientation layer, while Safe Mode and startup hydration states render as compact shell-status cards/pills instead of large generic content cards. Verified with `npm run build`; live operator validation is still required.
