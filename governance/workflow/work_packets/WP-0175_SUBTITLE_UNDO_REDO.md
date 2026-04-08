# Work Packet: WP-0175 - Subtitle Editing Undo/Redo

## Metadata
- ID: WP-0175
- Owner: Codex
- Status: REVIEW
- Created: 2026-04-08
- Target milestone: UX Polish

## Intent

- What: Add undo/redo for subtitle text editing in Localization Studio.
- Why: Operators edit subtitle text directly with no way to reverse mistakes. One wrong edit requires manual re-typing or re-running ASR. Undo/redo is table-stakes for any text editor.

## Scope

In scope:
- Track an undo stack of subtitle segment changes (text, timing, speaker assignment).
- Ctrl+Z to undo, Ctrl+Shift+Z / Ctrl+Y to redo.
- Stack depth limit (e.g. 50 operations) to bound memory.
- Visual indicator showing undo/redo availability (e.g. buttons or status text).
- Stack resets when switching tracks or items.

Out of scope:
- Undo for non-subtitle actions (job queueing, export, voice plan changes).
- Collaborative undo across multiple users.

## Acceptance criteria
- Ctrl+Z reverts the last subtitle text/timing/speaker change.
- Ctrl+Shift+Z or Ctrl+Y re-applies an undone change.
- Stack survives scrolling and section navigation within the same item.
- `npm run build` passes.
