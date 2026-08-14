---
file_id: wp-0175-proof-v0-1-149
file_kind: proof_summary
updated_at: 2026-08-15
---

<topic id="outcome" status="done" version="0.1.149" wp="WP-0175" updated_at="2026-08-15">

# WP-0175 Subtitle Editing Undo/Redo

Status: DONE

Subtitle text, timing, and speaker edits share a bounded 50-document history. Ctrl+Z and the Undo button revert; Ctrl+Shift+Z, Ctrl+Y, and Redo reapply. Availability is visible through live Undo/Redo counts, history survives scrolling and stage navigation within the same item, and the stack resets when the track or item changes.

</topic>

<topic id="verification" status="passed" version="0.1.149" wp="WP-0175" updated_at="2026-08-15">

## Packaged no-save scenario

1. Trusted input replaced the first visible translated segment with the original text plus `[WP0175_UNDO_PROBE]`; Undo changed from 0 to 1 and the UI reported unsaved changes.
2. Ctrl+Z restored the exact original text and produced Undo 0 / Redo 1.
3. Ctrl+Shift+Z restored the marker and produced Undo 1 / Redo 0.
4. Ctrl+2 navigated to Translate and Ctrl+1 returned to Captions; the edited text and Undo 1 state survived the stage round trip.
5. Ctrl+Z again restored the original after navigation. Ctrl+Y reapplied the marker, proving the second redo binding.
6. A final Ctrl+Z restored the original. No Save action was invoked.
7. An independent native `subtitles_load_track` reread of track `a975cfce-6d05-4c0a-a4e0-d9bf988d4b40` returned the original first-segment text and confirmed the probe marker was absent from every persisted segment.
8. Source inspection confirms the history push slices to the last 49 documents before adding the current one, bounding the stack at 50, and resets both stacks on track/item change.
- `npm run build` passed both before and inside the governed v0.1.149 build.

</topic>

<topic id="evidence" status="verified" version="0.1.149" wp="WP-0175" updated_at="2026-08-15">

## Evidence

- `evidence.json` in this directory.
- `governance/snapshots/WP-0175/redo_after_navigation_v149_1786745605525.png`.
- `governance/snapshots/WP-0172_0173_0175_batch/final_v149_1786745652435.dump.json`.

The screenshot was opened and visually inspected. It shows the real v0.1.149 segment editor after redo, the unsaved-state indicator, editable timing/text fields, and the sticky quick-action bar without overlap.

</topic>
