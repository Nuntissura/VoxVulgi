# Work Packet: WP-0026 - Phase 2: Mux dub preview audio into video

## Metadata
- ID: WP-0026
- Owner: Codex
- Status: DONE
- Created: 2026-02-22
- Target milestone: Phase 2 (voice-preserving dubbing)

## Intent

- What: Add a job that muxes the dub preview audio onto the original video to produce a shareable preview file.
- Why: A playable video output is the simplest "end-to-end" artifact for checking dubbing quality.

## Scope

In scope:

- Engine:
  - Add job `mux_dub_preview_v1`:
    - input: `item_id`,
    - expects `derived/items/<item_id>/dub_preview/mix_dub_preview_v1.wav` to exist,
    - writes `derived/items/<item_id>/dub_preview/mux_dub_preview_v1.mp4`.
- Desktop:
  - Add a Library action to enqueue the mux job.

Out of scope:

- Multiple audio tracks and language metadata.
- Subtitle mux/burn-in.

## Acceptance criteria

- Job produces `mux_dub_preview_v1.mp4` under the item dub preview folder.
- No network access required.

## Status updates

- 2026-02-22: Started.
- 2026-02-22: Implemented `mux_dub_preview_v1` job + Library enqueue action.
- 2026-02-22: Marked done; verification is covered by WP-0027.
