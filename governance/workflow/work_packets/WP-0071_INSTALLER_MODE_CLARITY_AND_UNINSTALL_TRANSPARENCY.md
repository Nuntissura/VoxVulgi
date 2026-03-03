# Work Packet: WP-0071 - Installer mode clarity and uninstall transparency

## Metadata
- ID: WP-0071
- Owner: Codex
- Status: DONE
- Created: 2026-03-03
- Target milestone: Stabilization sprint (installer UX + trust)

## Intent

- What: Make installer maintenance flows explicit so users can clearly distinguish Update/Repair, Full reinstall, and Uninstall.
- Why: Current NSIS default wording is ambiguous and contributed to confusion during uninstall/reinstall attempts.

## Scope

In scope:

- Add custom NSIS English language strings that clarify maintenance actions.
- Update installer text for app-data deletion so `%APPDATA%` behavior is explicit.
- Wire custom language file in Tauri NSIS config.
- Update governance/spec docs and task tracking for this change.

Out of scope:

- Replacing the full NSIS template.
- Changing uninstall data-retention policy defaults.
- Building a custom in-app updater flow.

## Acceptance criteria

- Installer maintenance screen explicitly communicates Update/Repair vs Full reinstall vs Uninstall.
- Installer app-data checkbox text clearly indicates data is under `%APPDATA%`.
- Tauri config loads custom NSIS language file without schema/config errors.
- Docs/task board reflect the new WP.

## Test / verification plan

- Build desktop installer via `governance/scripts/build_desktop_target.ps1`.
- Run produced NSIS installer and verify maintenance option wording.
- Verify build changelog includes WP-0071 on successful build.

## Status updates

- 2026-03-03: Created.
- 2026-03-03: Added custom NSIS English language file, wired it through `tauri.conf.json`, and clarified maintenance + app-data wording for install/update/uninstall decisions.
