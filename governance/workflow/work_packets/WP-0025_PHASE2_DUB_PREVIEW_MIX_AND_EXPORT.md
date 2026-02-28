# Work Packet: WP-0025 - Phase 2: Dub preview mix + export

## Metadata
- ID: WP-0025
- Owner: Codex
- Status: DONE
- Created: 2026-02-22
- Target milestone: Phase 2 (voice-preserving dubbing)

## Intent

- What: Add a "dub preview" mixer job that combines background stems with synthesized speech segments into a single preview audio track.
- Why: This produces an end-to-end output (audio dub preview) that can be iterated on before we implement higher-quality TTS/VC backends.

## Scope

In scope:

- Engine:
  - Add job `mix_dub_preview_v1`:
    - reads background stem from the separation output,
    - reads TTS segment manifest from the TTS preview output,
    - mixes speech onto background (simple overlay) using FFmpeg,
    - writes a final preview WAV under the item derived directory.
- Desktop:
  - Minimal UI action to enqueue the mix job (Library items list).

Out of scope:

- Muxing audio back into video containers.
- Loudness normalization and ducking.
- Multi-speaker voice mapping and voice cloning.

## Acceptance criteria

- Job writes:
  - `derived/items/<item_id>/dub_preview/mix_dub_preview_v1.wav`
- No network access required (after prerequisites are installed).

## Risks / open questions

- Mixing speech segments by iterative FFmpeg overlays can be slow for many segments.
- If pyttsx3 outputs non-44100 audio, FFmpeg resampling behavior should remain stable.

## Status updates

- 2026-02-22: Started.
- 2026-02-22: Implemented `mix_dub_preview_v1` job (FFmpeg overlay) + Library enqueue action.
- 2026-02-22: Marked done; verification is covered by WP-0027.
