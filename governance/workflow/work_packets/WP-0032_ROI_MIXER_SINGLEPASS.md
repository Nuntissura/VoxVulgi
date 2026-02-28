# Work Packet: WP-0032 - ROI-05: Single-pass audio mixer (ducking + loudness normalization)

## Metadata
- ID: WP-0032
- Owner: Codex
- Status: DONE
- Created: 2026-02-22
- Target milestone: Phase 2 (voice-preserving dubbing)

## Intent

- What: Replace the iterative FFmpeg overlay approach for dub mixing with a single-pass (or near single-pass) mix that supports ducking and loudness normalization.
- Why: Improve performance and consistency, reduce job time, and produce a more natural dub preview.

## Scope

In scope:

- Engine:
  - Add a new mixer implementation for dub preview mixing:
    - single-pass `ffmpeg -filter_complex` where feasible,
    - optional ducking (reduce background under speech),
    - loudness normalization (e.g., EBU R128).
  - Keep the existing mixer as a fallback (until verified stable).
- Desktop:
  - Add basic settings (defaults): ducking strength and loudness target.

Out of scope:

- Real-time mixing / streaming playback.
- Telemetry.
- Consent gates or anti-abuse controls.

## Acceptance criteria

- A dub preview mix job can run without iterative per-segment overlay loops.
- Output audio is within a reasonable loudness target (best-effort).
- Background audio is audibly reduced under dub speech when ducking is enabled.

## Test / verification plan

- Compare old vs new mixer on a known item and confirm runtime improvements and non-clipped audio.

## Risks / open questions

- Large numbers of segments may create long FFmpeg filter graphs; may require batching.

## Status updates

- 2026-02-22: Created.
- 2026-02-23: Implemented single-pass FFmpeg mixer (ducking + loudnorm) with UI settings and timing-fit support; verified via build + tests.
