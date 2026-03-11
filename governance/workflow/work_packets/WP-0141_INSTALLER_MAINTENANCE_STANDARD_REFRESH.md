# Work Packet: WP-0141 - Installer maintenance standard refresh

## Metadata
- ID: WP-0141
- Owner: Codex
- Status: DONE
- Created: 2026-03-11
- Target milestone: Installer standards refresh

## Intent

- What: Replace the current installer maintenance wording and overloaded two-choice logic with an explicit five-action maintenance standard.
- Why: The current installer still uses `Update/Repair` / `Full reinstall` / `Uninstall` wording and version-dependent option meanings. The operator wants clearer names, a stronger explainer page, and a durable governance standard for future installer builds.

## Scope

In scope:

- Define canonical installer maintenance actions and labels in governance.
- Update NSIS wizard copy and the pre-maintenance explainer page.
- Replace the version-dependent two-choice maintenance selection with explicit actions:
  - `Update`
  - `Reinstall (keep preferences and options)`
  - `Full reinstall`
  - `Uninstall (keep preferences and options)`
  - `Full uninstall`
- Wire full uninstall/full reinstall to delete app-data/preferences during uninstall.
- Preserve the rule that every desktop installer build increments version.

Out of scope:

- Non-Windows installer flows.
- Arbitrary installer redesign outside the maintenance/upgrade/uninstall experience.

## Acceptance criteria

- Governance documents the new installer maintenance standard as canonical.
- The installer wizard shows the new action names and a clear explainer.
- Full actions remove preferences/options; keep-actions preserve them.
- Uninstall-only actions exit after uninstall instead of continuing into install.
- A fresh desktop target build packages the updated installer and increments version.

## Test / verification plan

- Focused Tauri verification plus desktop build verification.
- Real desktop target installer build using the managed build script.
- Proof bundle documenting the final labels, action mapping, built installer path, and verification commands.

## Status updates

- 2026-03-11: Created from operator feedback requesting a clearer installer maintenance standard with explicit keep-vs-full uninstall/reinstall actions and stronger governance backing.
- 2026-03-11: Replaced the version-dependent two-choice NSIS maintenance mapping with five explicit actions (`Update`, `Reinstall (keep preferences and options)`, `Full reinstall`, `Uninstall (keep preferences and options)`, `Full uninstall`) and wired full actions to pass `/DELETEAPPDATA`.
- 2026-03-11: Built desktop target `0.1.6` with the managed build script. Installer outputs:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.6_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.6_x64_en-US.msi`
- 2026-03-11: Verification passed with `cargo test -q --manifest-path product/desktop/src-tauri/Cargo.toml`, `git diff --check`, and `powershell -ExecutionPolicy Bypass -File governance/scripts/build_desktop_target.ps1 -WorkPackets WP-0141 -BuildNotes "Installer maintenance standard refresh."`
