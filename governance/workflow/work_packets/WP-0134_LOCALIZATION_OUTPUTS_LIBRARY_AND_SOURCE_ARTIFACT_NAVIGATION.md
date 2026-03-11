# Work Packet: WP-0134 - Localization outputs library and source/artifact navigation

## Metadata
- ID: WP-0134
- Owner: Codex
- Status: DONE
- Created: 2026-03-09
- Target milestone: Installer smoke remediation tranche

## Intent

- What: Add a dedicated Localization outputs browser/library and make source media, working artifacts, deliverables, and export folders easy to open from one place.
- Why: Operators still cannot reliably find localization outputs even though paths/buttons exist in the page, and the current experience does not feel like a real outputs library.

## Scope

In scope:

- Localization output library or dedicated browser view grouped by source item.
- Direct open/reveal actions for source video, working artifact folder, export folder, final dubbed video, subtitle files, and dub audio.
- Clear separation between working files and deliverables.
- Visibility for exported localization items inside Media Library or a dedicated localization-outputs surface.

Out of scope:

- Rewriting unrelated Media Library archive semantics.

## Acceptance criteria

- Operators can find the source item and all key localization outputs from one obvious surface.
- Final MP4, subtitle exports, dub audio, and source video are each reachable with direct open/reveal actions.
- Localization outputs no longer feel hidden in app-data internals.

## Test / verification plan

- App-boundary verification on a real item with generated localization artifacts.
- Desktop build and focused path-action checks.
- Proof bundle with artifact paths and operator flow.

## Status updates

- 2026-03-09: Created from installer smoke findings `ST-007` and `ST-016`.
- 2026-03-09: Implemented a dedicated `Localization Library` browser with grouped source/working/deliverable entries, explicit open/reveal/copy-path actions, and proof under `product/desktop/build_target/tool_artifacts/wp_runs/WP-0134/20260309_060801/`.
