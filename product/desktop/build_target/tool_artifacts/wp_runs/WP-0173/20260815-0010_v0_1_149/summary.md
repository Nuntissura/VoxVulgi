---
file_id: wp-0173-proof-v0-1-149
file_kind: proof_summary
updated_at: 2026-08-15
---

<topic id="outcome" status="done" version="0.1.149" wp="WP-0173" updated_at="2026-08-15">

# WP-0173 Localization Studio Keyboard Shortcuts

Status: DONE

Localization Studio exposes keyboard alternatives for start/continue, export, readiness refresh, stage navigation, and subtitle undo/redo. The later WP-0211 master-detail contract intentionally refines the original Ctrl+1…5 section mapping to Ctrl+1…8 for Captions, Translate, Speakers, Voice plan, Dub, Mix, Combine A/V, and Files. The visible reference and product specification now use that current mapping.

</topic>

<topic id="verification" status="passed" version="0.1.149" wp="WP-0173" updated_at="2026-08-15">

## Verification

- Packaged trusted keyboard input changed Ctrl+2 from Captions to Translate and Ctrl+8 to Files.
- With a visible Translate `<select>` focused, Ctrl+3 left the selected stage on Translate, proving the form-field guard.
- Ctrl+Shift+R completed the read-only refresh path and rendered `Localization readiness refreshed.`
- Trusted pointer input opened the persistent keyboard reference. It lists Ctrl+Enter, Ctrl+Shift+E, Ctrl+Shift+R, Ctrl+1…8, and undo/redo.
- Final source inspection confirms the same visible-only keydown listener dispatches Ctrl+Enter to `enqueueLocalizationRun`, Ctrl+Shift+E to `exportSelectedOutputs`, and Ctrl+Shift+R to the proven refresh callback after the shared input/textarea/select guard. The two mutating shortcuts were not fired against operator data during proof.
- `npm run build` passed both before and inside the governed v0.1.149 build.

</topic>

<topic id="evidence" status="verified" version="0.1.149" wp="WP-0173" updated_at="2026-08-15">

## Evidence

- `evidence.json` in this directory.
- `governance/snapshots/WP-0173/keyboard_shortcuts_v149_1786745479602.png`.
- `governance/snapshots/WP-0172_0173_0175_batch/final_v149_1786745652435.dump.json`.

The screenshot was opened and visually inspected. It shows v0.1.149, Files selected by Ctrl+8, every documented shortcut expanded in the left rail, and the sticky quick-action bar without overlap.

</topic>
