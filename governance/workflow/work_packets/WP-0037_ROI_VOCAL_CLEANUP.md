# Work Packet: WP-0037 - ROI-10: Optional vocals cleanup (noise reduction + de-reverb; explicit install)

## Metadata
- ID: WP-0037
- Owner: Codex
- Status: DONE
- Created: 2026-02-22
- Target milestone: Phase 2 (voice-preserving dubbing)

## Intent

- What: Add an optional vocals cleanup step (noise reduction + de-reverb) to improve downstream TTS/VC quality.
- Why: Voice conversion and TTS mixing work better with clean speech.

## Scope

In scope:

- Engine:
  - Add an optional "vocals cleanup" job that reads the vocals stem and outputs a cleaned vocals WAV.
  - Implement a baseline cleanup pipeline (CPU) using FFmpeg filters and/or an explicit-install Python pack.
- Desktop:
  - Diagnostics:
    - show install status for the cleanup backend if Python-based.
  - Item view:
    - expose "Clean vocals" action (explicit user action).

Out of scope:

- Running cleanup automatically without user action.
- Telemetry.
- Consent gates or anti-abuse controls.

## Acceptance criteria

- When enabled/installed, a user can run vocals cleanup and get a cleaned vocals WAV output (path defined in implementation).
- Cleanup is optional and clearly labeled as such.

## Test / verification plan

- Run cleanup on a noisy clip and confirm the output is produced and has audible noise reduction (best-effort).

## Risks / open questions

- "De-reverb" quality may require heavier models; keep baseline simple and explicit-install for advanced options.

## Status updates

- 2026-02-22: Created.
- 2026-02-23: Added optional vocals cleanup job (FFmpeg filter pipeline) + Subtitle Editor action and artifact listing; verified via build + tests.
