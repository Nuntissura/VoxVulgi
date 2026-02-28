# Work Packet: WP-0055 - UI persistence + output/artifact path visibility

## Metadata
- ID: WP-0055
- Owner: Codex
- Status: DONE
- Created: 2026-02-24
- Target milestone: Phase 2 (usability)

## Intent

- What: Make the desktop UI retain key user settings across pane switches, and make it obvious where job outputs and artifacts are written.
- Why: Users expect folder selections and pipeline settings to stick, and need a clear way to find generated files on disk.

## Scope

In scope:

- Persist (local-only) UI preferences across pane switches / app restarts:
  - Library: URL/Instagram/Image batch output folders and related toggles.
  - Global: ASR language preference.
  - Editor: separation backend, diarization backend, mix/mux settings, bilingual toggle.
- Jobs page shows:
  - artifacts folder path (`derived/jobs/<job_id>`) and a button to open it (when created).
  - outputs folder path for item jobs (`derived/items/<item_id>`) and a button to open it.
- Library page shows a per-item button to open that item’s outputs folder.
- Update specs to reflect bundled-dependency stance and to document default output locations.

Out of scope:

- Changing the on-disk layout of derived outputs.
- Per-job custom output folder routing (export is already supported via explicit save dialogs / export packs).

## Acceptance criteria

- Switching between panes does not reset output folder inputs and pipeline setting controls.
- A user can click through from Jobs/Library to the exact folder on disk where outputs/artifacts are written.
- Specs mention that the Windows full installer bundles Phase 1 + Phase 2 dependencies and that derived outputs live under app-data.

## Test / verification plan

- `npm -C product/desktop run build`
- `cargo test` in:
  - `product/engine`
  - `product/desktop/src-tauri`
- Manual smoke:
  - Change Library output folders and Editor mix/mux settings; switch panes; verify values persist.
  - Import a local video; verify Jobs shows output/artifact paths and “Open outputs” opens the derived folder.

## Status updates

- 2026-02-24: Created.
- 2026-02-24: Persisted Library/Editor settings via localStorage; Jobs/Library now show and open artifact/output folders; specs updated for bundled-dependency stance; build + tests verified.
