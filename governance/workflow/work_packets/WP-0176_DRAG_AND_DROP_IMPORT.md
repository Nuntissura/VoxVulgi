# Work Packet: WP-0176 - Drag-and-Drop Import

## Metadata
- ID: WP-0176
- Owner: Codex
- Status: DONE
- Created: 2026-04-08
- Target milestone: UX Polish

## Intent

- What: Allow operators to drag media files onto the Localization Studio home screen to import them.
- Why: Currently import requires clicking Import → file picker → navigate → select. Drag-and-drop is the expected interaction for media apps and removes friction from the most common first action.

## Scope

In scope:
- Add a drop zone on the Localization Studio home screen that accepts video/audio files.
- Show a visual drop indicator (border highlight, overlay text) when dragging over the window.
- On drop, trigger the same import flow as the Import button (with current ASR lang and batch-on-import rules).
- Support multiple files in one drop (batch import).
- Accept common media formats: mp4, mkv, avi, mov, mp3, wav, flac, ogg, webm.

Out of scope:
- Drag-and-drop onto other pages (Video Archiver, etc.).
- Drag-and-drop of URLs (only local files).
- Drag-and-drop reordering of subtitle segments.

## Acceptance criteria
- Dragging a video file onto the Localization Studio home imports it.
- Visual feedback shown during drag-over.
- Multiple files can be dropped at once.
- `npm run build` passes.

## Status updates

- 2026-08-15: Closed DONE against packaged v0.1.169. The clean isolated Localization surface visibly advertises `Select or drop a video/audio file` and remains import-only; semantic audit returned 51 elements with zero truncation/missing names. Two focused contracts prove the native window drag/drop listener, overlay state, full governed media-format filter, and multi-file import fan-out. Closure proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0176/20260815-193000-v0_1_169/summary.md`.
