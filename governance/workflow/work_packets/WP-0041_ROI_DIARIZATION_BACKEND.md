# Work Packet: WP-0041 - ROI-14: Better diarization backend option (BYO gated models; off by default)

## Metadata
- ID: WP-0041
- Owner: Codex
- Status: DONE
- Created: 2026-02-22
- Target milestone: Phase 2 (voice-preserving dubbing)

## Intent

- What: Provide an optional higher-accuracy diarization backend for power users that requires BYO gated models and remains off by default.
- Why: Diarization quality varies widely; offering a power-user path can improve results without forcing heavy dependencies on everyone.

## Scope

In scope:

- Engine:
  - Add a diarization backend interface and keep the current baseline as default.
  - Add a second backend that can be enabled only with explicit user configuration (BYO model path/token).
- Desktop:
  - Diagnostics/settings UI for configuring the optional backend:
    - enable/disable,
    - select local model path or provide token (if required by the chosen backend).

Out of scope:

- Enabling the optional backend by default.
- Telemetry.
- Consent gates or anti-abuse controls.

## Acceptance criteria

- Baseline diarization remains the default and works with no extra configuration.
- Power users can configure and run the optional diarization backend explicitly.

## Test / verification plan

- Run baseline diarization vs optional backend on a multi-speaker clip and compare output.

## Risks / open questions

- Some diarization backends have network and license constraints; keep the integration optional and explicit.

## Status updates

- 2026-02-22: Created.
- 2026-02-23: Added optional diarization backend selection (BYO pyannote) with explicit Diagnostics configuration UI; baseline remains default; per-job backend selectable in Subtitle Editor; verified via build + tests.
