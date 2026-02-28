# Work Packet: WP-0035 - ROI-08: Dub timing-fit tools (time-stretch to segment windows)

## Metadata
- ID: WP-0035
- Owner: Codex
- Status: DONE
- Created: 2026-02-22
- Target milestone: Phase 2 (voice-preserving dubbing)

## Intent

- What: Add automatic timing-fit tools that adjust dub audio (TTS/VC) to fit within subtitle segment windows.
- Why: Prevent overlapping speech and keep dubs aligned with on-screen captions.

## Scope

In scope:

- Engine:
  - Add a timing-fit step for rendered dub segments:
    - measure segment audio duration,
    - time-stretch (best-effort) to fit within available window,
    - optionally pad with silence when shorter.
  - Provide a conservative default (quality first).
- Desktop:
  - Expose a per-job toggle and basic bounds (min/max time-stretch).

Out of scope:

- Full phoneme-aware alignment.
- Telemetry.
- Consent gates or anti-abuse controls.

## Acceptance criteria

- When enabled, dub segment audio respects the segment time window (best-effort).
- Job outputs indicate which segments were stretched and by what factor.

## Test / verification plan

- Use a track with short segment windows and confirm long segments are fitted and output remains intelligible.

## Risks / open questions

- FFmpeg `atempo` has limited range; may require chained filters or an alternate time-stretch backend.

## Status updates

- 2026-02-22: Created.
- 2026-02-23: Added timing-fit controls (enable + min/max stretch factors) to dub mix job and UI; timing-fit report saved in job artifacts and included in export pack; verified via build + tests.
