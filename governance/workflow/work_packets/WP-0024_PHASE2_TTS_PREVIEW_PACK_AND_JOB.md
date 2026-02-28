# Work Packet: WP-0024 - Phase 2: TTS preview (pack + job)

## Metadata
- ID: WP-0024
- Owner: Codex
- Status: DONE
- Created: 2026-02-22
- Target milestone: Phase 2 (voice-preserving dubbing)

## Intent

- What: Add a simple local TTS preview job that can synthesize English speech for subtitle segments and write per-segment audio artifacts + a manifest.
- Why: This enables end-to-end iteration on timing + mixing before we lock in a specific voice cloning backend.

## Scope

In scope:

- Engine:
  - Add a TTS preview pack status + installer (explicit install in Diagnostics).
  - Add job `tts_preview_pyttsx3_v1`:
    - input: `item_id`, `source_track_id` (expected EN track but not required),
    - output: per-segment wav files + `manifest.json`.
- Desktop:
  - Diagnostics shows TTS preview pack status and exposes install.
  - Subtitle editor exposes a "TTS preview" action for the selected track.

Out of scope:

- High quality neural TTS (Kokoro/MeloTTS/etc.) and voice cloning (separate WPs).
- Full mix/mux to background/video (separate WP).

## Acceptance criteria

- Diagnostics shows TTS preview pack installed/not installed.
- Job produces:
  - `derived/items/<item_id>/tts_preview/pyttsx3_v1/manifest.json`
  - `derived/items/<item_id>/tts_preview/pyttsx3_v1/segments/*.wav`
- No silent network egress; installs only run when user clicks install.

## Risks / open questions

- `pyttsx3` output quality/voices depend on OS and installed system voices.
- Some systems may not support `save_to_file` reliably.

## Status updates

- 2026-02-22: Started.
- 2026-02-22: Implemented TTS preview pack status/install (Diagnostics) + `tts_preview_pyttsx3_v1` job + subtitle editor enqueue/polling.
- 2026-02-22: Marked done; verification is covered by WP-0027.
