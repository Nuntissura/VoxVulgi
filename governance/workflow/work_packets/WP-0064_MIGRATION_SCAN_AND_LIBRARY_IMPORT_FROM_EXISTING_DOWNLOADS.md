# Work Packet: WP-0064 - Migration hardening: scan existing downloads to seed dedupe + optional library import

## Metadata
- ID: WP-0064
- Owner: Codex
- Status: BACKLOG
- Created: 2026-02-28
- Target milestone: Phase 1 (migration polish)

## Intent

- What: Improve migration away from third-party downloaders by scanning existing folders to prevent re-downloads and optionally importing existing files into VoxVulgi’s Library DB.
- Why: Exported “already downloaded” histories may be incomplete; filesystem truth is the most reliable source.

## Scope

In scope:

- Add an action to scan a user-selected folder (or a subscription output folder) and:
  - infer provider IDs when possible (e.g., YouTube IDs from filenames/metadata),
  - append IDs to a yt-dlp-compatible archive file for dedupe.
- Optional (if feasible without risk):
  - import discovered media files into the Library DB without moving/deleting them (index-only import).
- UI entrypoints:
  - “Scan folder and seed archive…”
  - “Import existing downloads…”

Out of scope:

- Automated mass renaming/re-foldering of user libraries.
- Any deletion of existing user files or subscription lists.

## Acceptance criteria

- After seeding an archive from an existing folder, subscription refresh does not re-download already present items.
- Index-only import does not move/delete user files; it only adds Library entries.

## Test / verification plan

- Unit tests for ID inference heuristics (safe, conservative).
- `cargo test` in `product/engine`
- Manual smoke on a small folder sample first.

## Status updates

- 2026-02-28: Created.

