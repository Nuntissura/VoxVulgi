# Work Packet: WP-0072 - NSIS pre-maintenance explainer page

## Metadata
- ID: WP-0072
- Owner: Codex
- Status: DONE
- Created: 2026-03-03
- Target milestone: Stabilization sprint (installer UX + trust)

## Intent

- What: Add a short explainer page before the maintenance-option screen in NSIS installers.
- Why: Operators need clear up-front guidance on Update/Repair vs Full reinstall vs Uninstall before selecting an option.

## Scope

In scope:

- Add a custom NSIS template that inserts a maintenance explainer page before `PageReinstall`.
- Show three one-line mode explanations (Update/Repair, Full reinstall, Uninstall) with explicit app-data behavior.
- Keep the page conditional so first-time installs skip it.
- Wire the custom template through Tauri NSIS config.
- Update governance/spec docs and task tracking.

Out of scope:

- Replacing uninstall behavior.
- Changing data retention defaults.
- Implementing a separate updater binary/flow.

## Acceptance criteria

- On reinstall/update flows, installer shows explainer page before maintenance choice page.
- Explainer page is skipped when no previous installation is detected.
- `tauri.conf.json` points to the custom NSIS template and build succeeds.
- Task board + roadmap + specs reference the change.

## Test / verification plan

- Build desktop installer via `governance/scripts/build_desktop_target.ps1`.
- Launch NSIS installer and verify explainer page appears before maintenance choice.
- Confirm build changelog includes WP-0072.

## Status updates

- 2026-03-03: Created.
- 2026-03-03: Added custom NSIS template with conditional pre-maintenance explainer page and wired it in Tauri config.
