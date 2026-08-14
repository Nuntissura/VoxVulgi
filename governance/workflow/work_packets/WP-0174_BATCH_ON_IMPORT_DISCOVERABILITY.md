# Work Packet: WP-0174 - Batch-on-Import Discoverability

## Metadata
- ID: WP-0174
- Owner: Codex
- Status: SUPERSEDED
- Superseded by: WP-0199
- Created: 2026-04-08
- Target milestone: UX Polish

## Intent

- What: Surface the batch-on-import auto-processing toggles in the Localization Studio home screen instead of only in the Diagnostics page.
- Why: The feature exists (auto ASR, auto translate, auto dub preview on import) but is buried in Diagnostics where operators don't know it exists. Making it visible on the Import and Setup card saves significant time for batch workflows.

## Scope

In scope:
- Add collapsible "Auto-processing on import" section to the Import and Setup card on the Localization Studio home screen.
- Show 5 checkboxes matching the Diagnostics toggles: Speech recognition, Translate to English, Separate audio stems, Label speakers, Dub preview.
- Changes save immediately (no separate Save button needed).
- Show "(active)" or "(off)" in the summary line.
- Also renamed "ASR language" to "Source language" with expanded option labels.

Out of scope:
- Removing the Diagnostics batch-on-import card (it stays for power users).
- Changing batch-on-import backend logic.

## Acceptance criteria
- Batch-on-import toggles visible on Localization Studio home.
- Toggling a checkbox persists immediately via the existing config command.
- `npm run build` passes.

## Status updates

- 2026-08-14: Superseded by WP-0199's later explicit-start contract. Localization-owned intake now passes `applyBatchOnImport: false`, remains idle after import, and requires the operator to review settings before starting a run. Batch-on-import defaults remain available outside that intake path through Options/Diagnostics and compatible import flows.
