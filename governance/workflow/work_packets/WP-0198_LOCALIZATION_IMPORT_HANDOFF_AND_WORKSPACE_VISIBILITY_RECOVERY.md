# Work Packet: WP-0198 - Localization import handoff and workspace visibility recovery

## Metadata
- ID: WP-0198
- Owner: Codex
- Status: REVIEW
- Created: 2026-04-24
- Target milestone: Localization operator usability

## Intent

- What: Repair the Localization-only local-import path so imported files reliably enter the localization workspace and remain visible to the operator as the current item.
- Why: Operator smoke shows imported files can finish in Jobs while never appearing in `Continue current item` or `Localization Library`, which breaks the core Localization contract.

## Scope

In scope:
- Fix the import command handshake so Localization imports are added to `localization_workspace_item`.
- Ensure import completion updates `Current Item`, recent localization items, and editor handoff consistently.
- Surface import-complete and import-failed state inside Localization Studio rather than only in Jobs.
- Verify that the Localization home `pendingImportPath` logic matches the real item/workspace behavior.

Out of scope:
- A full redesign of Localization Studio visuals.
- Archive/media-library intake flows outside the Localization-owned import path.
- Reworking the wider library schema beyond what is needed for the handoff fix.

## Acceptance criteria
- Importing a file from Localization Studio adds it to the localization workspace every time.
- The imported file appears in `Continue current item` and `Localization Library` without requiring a separate Media Library navigation step.
- Localization Studio shows a visible handoff state while import is pending and a visible completion or failure state after import finishes.
- The kept import is distinguishable from canceled duplicate imports in operator-facing UI.

## Test / verification plan

- Reproduce the current failure using Localization Studio local import.
- Verify the import job log records `added_to_localization_workspace=true`.
- Verify the imported item appears in Localization home and editor recent-item surfaces after completion.
- Re-run targeted desktop verification (`npm run build`) and Rust verification (`cargo check`).

## Risks / open questions

- The current Tauri invoke argument mapping may be silently dropping the workspace flag.
- Duplicate imports of the same source file may still need a clearer dedupe or reuse policy after the handoff bug is fixed.

## Status updates

- 2026-04-24: Created after operator smoke showed a completed Localization import that remained absent from `Continue current item` and `Localization Library`, while the job log recorded `added_to_localization_workspace=false`.
- 2026-04-24: First implementation slice landed: Localization home now uses the Tauri command's camelCase invoke args, imports explicitly request workspace intake, and Localization-owned imports suppress automatic batch-on-import follow-on jobs so the workspace handoff can complete visibly before processing starts. Verification: `npm run build`, `cargo check`.
