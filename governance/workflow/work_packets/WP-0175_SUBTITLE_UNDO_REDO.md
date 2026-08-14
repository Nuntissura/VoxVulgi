# Work Packet: WP-0175 - Subtitle Editing Undo/Redo

## Metadata
- ID: WP-0175
- Owner: Codex
- Status: DONE
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

## Implementation status (2026-08-15)

- The shared subtitle document updater maintains a bounded 50-document undo stack, clears redo on new edits, exposes live Undo/Redo counts, and resets both stacks when the item or track changes.
- Governed packaged v0.1.149 proof used trusted text and keyboard input to exercise Ctrl+Z, Ctrl+Shift+Z, Ctrl+Y, and a Captions→Translate→Captions stage round trip. The original subtitle text was restored at the end and Save was never invoked.
- An independent native reread of the canonical track confirmed the probe marker never persisted. The real segment UI screenshot was visually inspected. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0175/20260815-0010_v0_1_149/summary.md`.
