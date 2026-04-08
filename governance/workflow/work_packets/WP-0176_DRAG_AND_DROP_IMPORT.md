# Work Packet: WP-0176 - Drag-and-Drop Import

## Metadata
- ID: WP-0176
- Owner: Codex
- Status: REVIEW
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
