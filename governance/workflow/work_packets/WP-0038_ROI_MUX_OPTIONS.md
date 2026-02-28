# Work Packet: WP-0038 - ROI-11: Mux options (multi-track audio) + language metadata

## Metadata
- ID: WP-0038
- Owner: Codex
- Status: DONE
- Created: 2026-02-22
- Target milestone: Phase 2 (voice-preserving dubbing)

## Intent

- What: Improve muxing controls: keep original audio as an additional track, choose output container, and tag language metadata.
- Why: Users want flexible exports and better compatibility with media players.

## Scope

In scope:

- Engine:
  - Extend mux job parameters:
    - output container (mp4/mkv baseline),
    - keep original audio as a second track,
    - set language metadata tags for audio/subtitle tracks (best-effort).
- Desktop:
  - Add advanced mux options UI with safe defaults.

Out of scope:

- Full encoding presets UI (bitrate, codecs) beyond a small set of sane defaults.
- Telemetry.
- Consent gates or anti-abuse controls.

## Acceptance criteria

- User can mux with dubbed audio track and optionally preserve the original audio track.
- Output container selection works for at least mp4 and mkv.
- Language metadata tags are applied when requested (best-effort).

## Test / verification plan

- Mux an item with and without original audio preservation and verify multiple audio tracks exist (via ffprobe).

## Risks / open questions

- Container/codec compatibility constraints across platforms.

## Status updates

- 2026-02-22: Created.
- 2026-02-23: Extended mux job with container selection (mp4/mkv), keep-original-audio option, and language tags; exposed safe defaults + controls in UI; verified via build + tests.
