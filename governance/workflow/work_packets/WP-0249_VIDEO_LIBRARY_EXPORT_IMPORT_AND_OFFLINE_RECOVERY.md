# Work Packet: WP-0249 - Video library export/import and offline recovery

## Status

DONE

## Base Scope

- Add a video-library bundle export/import that captures library registry rows, selected library state, YouTube subscriptions/playlists bound to those libraries, and indexed video-library item metadata.
- Keep the export/import metadata-only; never copy, move, delete, or overwrite media files as part of export/import.
- When the active NAS-backed YouTube library is missing and the operator queues new YouTube singles, prompt to create/select a replacement library instead of only failing.
- Let the operator reconnect the old NAS library by making it active again when the path is available.
- Add a first move/copy workflow for library-contained metadata:
  - move saved YouTube subscriptions/playlists from one library to another by rebinding `library_id`,
  - copy/move indexed library item metadata by replacing the old library root prefix with the destination library root,
  - defer subscription copy because `youtube_subscription.source_url` is unique and true duplicated subscriptions need a schema/identity decision,
  - do not move physical media files in this slice.

## Operator Request Preserved

- "i want all new videos and playlists/subscriptions together with the legacy videos and playlists as a single library, that we can export and import."
- "if the nas is not available this should not crash the app and a new labrary should get asked to be created."
- "when the nas is back available we can reconnect to the old library again."
- "in the future we should als ebe able to move items, videos and subscriptons from one library to another, or copy."

## Research Basis

- Existing VoxVulgi evidence: `video_library` registry already stores roots and availability, `youtube_subscription.library_id` binds recurring targets, `library_item.media_path` stores indexed media paths, and existing subscription JSON import/export already provides non-destructive metadata transfer patterns.
- Plex library docs model libraries as named media sets with source folders added by the operator; this supports keeping paths explicit rather than hiding NAS roots behind app-managed relocation.
- Jellyfin library docs allow multiple paths in one library and manual path entry when a picker cannot find the exact location; this supports preserving manual UNC/NAS paths and not requiring a live visual folder picker for reconnect.
- Tauri v2 dialog docs support the existing `open` and `save` plugin pattern already used in `LibraryPage.tsx`.

## High-ROI Additions

- Export/import subscriptions and indexed items with libraries in the same bundle.
  - Why high ROI: the existing subscription export/import and registry already exist, so this prevents future split-brain recovery exports with limited new machinery.
  - Gap closed: a "library" is no longer just a UI root selector; it becomes a portable metadata unit.
  - Reuses: `video_libraries`, `subscriptions`, `library_item`, existing dialog file pickers.
  - Validation: backend roundtrip tests plus UI contract tests.
- Offline NAS prompt before queue.
  - Why high ROI: the current missing-root error blocks operator flow after input; a prompt can reuse the existing library picker/upsert code.
  - Gap closed: NAS outage becomes recoverable instead of feeling like an app crash.
  - Reuses: active library status, `video_libraries_upsert`, existing Tauri `open` dialog.
  - Validation: frontend contract test and app-boundary snapshot.
- Metadata copy/move now, physical file moves later.
  - Why high ROI: rebinding subscriptions and indexed paths supports planning and library reorganization without data-loss risk.
  - Gap closed: operators can start moving ownership between libraries while media-file relocation remains explicitly out of scope.
  - Reuses: library root prefix replacement and subscription `library_id`.
  - Validation: backend tests assert no media file deletion/move.

## Risks And Hardening

- Risk: export/import accidentally becomes a media migration.
  - Scenario: operator imports a bundle and expects files to be created or overwritten.
  - Remediation: command names, UI copy, and notices say metadata-only; tests assert media paths are stored but files are untouched.
  - Verification: backend import/export tests use nonexistent paths and pass without filesystem writes.
- Risk: moving library item metadata can point to files that do not exist at the destination.
  - Scenario: operator moves metadata to a local library without copying NAS files.
  - Remediation: label this action metadata-only and keep copy/move counts visible; no media delete/copy is attempted.
  - Verification: tests assert source media still exists and destination path is just metadata.
- Risk: NAS outage prompt could override the old NAS library permanently.
  - Scenario: operator creates a local temporary library, then loses the old selected NAS row.
  - Remediation: create/select a separate active library; old library row stays registered and can be reselected when available.
  - Verification: backend library registry tests and UI contract for prompt wording.
- Risk: bundle import overwrites newer subscription metadata.
  - Scenario: importing an older bundle changes titles/folder maps.
  - Remediation: reuse upsert-by-source-url semantics and report inserted/updated counts; no media rows are deleted.
  - Verification: roundtrip/import tests check additive updates only.

## Acceptance Criteria

- Video Archiver exposes export/import controls for a video-library bundle.
- Exported bundle includes `video_library` rows, active library ID, library-bound YouTube subscription rows, and video `library_item` metadata under registered library roots.
- Importing a bundle restores library rows, active selection when possible, subscriptions, and indexed item metadata without touching media files.
- If the active video library is missing during YouTube single queueing and no per-batch override is set, the UI asks the operator to select/create another library and then queues into it.
- The old NAS library remains in the registry after creating a temporary library and can be reselected later.
- The UI exposes copy/move controls for item metadata between libraries and a move control for subscriptions.
- Copy/move item metadata can transfer library item rows to another library without deleting, moving, or overwriting media files.
- Subscription metadata can move between libraries by rebinding `library_id`; subscription copy remains deferred until duplicated subscription identity is designed.
- The GUI is visually checked through the headless agent bridge or an explicitly documented fallback if the bridge cannot run.

## Verification

- Add Rust tests for bundle export/import and metadata copy/move.
- Add desktop contract tests for export/import controls, offline prompt, and copy/move UI command wiring.
- Run `cargo test --manifest-path product/engine/Cargo.toml --lib`.
- Run `npm run test:contracts` in `product/desktop`.
- Run `npm run build` in `product/desktop`.
- Run `cargo check --manifest-path product/desktop/src-tauri/Cargo.toml`.
- Start or attach to the app bridge, navigate to Video Archiver, capture snapshot/dump, and inspect that the library bundle controls and copy/move controls are visible and non-overlapping.

## Status Updates

- 2026-06-04: Created from operator request to complete remaining library gaps after WP-0220 fixed NAS-shaped YouTube output folders.
- 2026-06-04: Implemented metadata-only video-library bundle export/import, NAS-offline create/select-library prompt before YouTube batch queueing, metadata-only item copy/move, and subscription move/rebind. Verification passed: `cargo test --manifest-path product/engine/Cargo.toml --lib` (216/216), `npm run test:contracts` (67/67), `npm run build`, and `cargo check --manifest-path product/desktop/src-tauri/Cargo.toml`. Visual proof captured via bridge at `governance/snapshots/WP-0249/video_library_controls_live3_1780538584350.png`.
- 2026-08-15: Promoted to DONE after current reconciliation. Four final-state frontend contracts passed, the disposable metadata-only bundle round-trip passed, the packaged v0.1.66 UI artifact was visually re-inspected, and hidden packaged v0.1.153 semantic inspection retained the complete library-management group. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0249/20260815_board_reconciliation_v0_1_153/summary.md`.
