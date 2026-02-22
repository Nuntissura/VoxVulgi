# Work Packet: WP-0018 - Rebrand to VoxVulgi

## Metadata
- ID: WP-0018
- Owner: Codex
- Status: DONE
- Created: 2026-02-22
- Target milestone: Phase 1 (repo hygiene)

## Intent

- What: Rename project branding to VoxVulgi.
- Why: Prepare for a new public GitHub repository with a non-platform-branded identity.

## Scope

In scope:

- Update governance docs (`MODEL_BEHAVIOR.md`, specs, roadmap/task board) to use VoxVulgi.
- Update desktop app branding (window title + in-app brand text).
- Update packaging identifiers (Tauri `productName` + `identifier`) to VoxVulgi.
- Update developer bootstrap tooling names accordingly.

Out of scope:

- Renaming or regenerating prebuilt binaries committed to the repo.
- Changing internal file-format identifiers used for subtitle artifacts.

## Acceptance criteria

- No user-facing UI or governance docs refer to the previous project name.
- Tauri bundle metadata uses VoxVulgi.
- Dev bootstrap script still points at the correct setup binary.

## Status updates

- 2026-02-22:
  - Applied repo-wide rebrand to VoxVulgi (docs + Tauri config + UI brand).
