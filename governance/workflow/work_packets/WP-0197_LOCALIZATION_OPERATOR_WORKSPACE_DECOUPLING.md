# Work Packet: WP-0197 - Localization operator workspace decoupling

## Metadata
- ID: WP-0197
- Owner: Codex
- Status: REVIEW
- Created: 2026-04-23
- Target milestone: Localization operator usability

## Intent

- What: Make Localization Studio operate from an explicit operator-selected workspace instead of reading from the shared archive/media library.
- Why: Operator direction is to keep Localization Studio disconnected from YouTube downloader/archive flows and old 4KVDP inventory. Only files deliberately selected by the operator should appear as localization work.

## Scope

In scope:
- Add a separate operator-scoped intake/workspace list for localization items.
- Ensure Localization home and editor-facing recent-item flows use that workspace instead of the global `library_item` recency list.
- Keep operator import/selection as the only way an item enters Localization Studio.
- Remove passive archive/media-library entry points that make Localization feel coupled to downloader/archive state.

Out of scope:
- Deleting or migrating shared archive library data.
- Removing archive metadata from the broader app.
- A full redesign of Media Library or Localization Studio visuals.

## Acceptance criteria
- Localization Studio home only shows items explicitly selected/imported by the operator for localization.
- Archive/download activity no longer injects itself into Localization recent items or default editor flows.
- Legacy 4KVDP-imported inventory no longer appears in Localization unless the operator intentionally selects that file for localization intake.
- Desktop build verification passes after the separation.

## Test / verification plan

- Inspect current Localization home/editor sources and replace shared-library recency with workspace recency.
- Verify operator import still opens the new item into Localization.
- Verify archive/download activity does not alter Localization recent items unless the operator explicitly imports/selects a file.
- Re-run `npm run build` and `cargo check`.

## Risks / open questions

- Some existing convenience flows currently assume the shared library and localization workspace are the same thing.
- Reusing an already-indexed media path may need explicit duplicate-handling policy so operator intent stays clear.

## Status updates

- 2026-04-23: Created after operator direction clarified that Localization Studio must only work on files explicitly selected for localization and must not remain coupled to YouTube/archive or legacy 4KVDP inventory.
- 2026-04-23: Landed the first implementation slice: added an explicit `localization_workspace_item` store, routed Localization home/editor recent-item flows through that workspace, required Localization imports to opt into workspace intake, and removed archive/media-library item actions that opened shared items straight into Localization. Verification: `cargo check`, `npm run build`.
