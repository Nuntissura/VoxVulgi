# Work Packet: WP-0045 - ROI-18: Performance tiering (CPU baseline vs GPU acceleration)

## Metadata
- ID: WP-0045
- Owner: Codex
- Status: DONE
- Created: 2026-02-22
- Target milestone: Phase 2 (voice-preserving dubbing)

## Intent

- What: Detect CPU-only vs GPU-capable environments and recommend settings accordingly.
- Why: Many users will have a GPU available, but the default must remain CPU-safe and predictable.

## Scope

In scope:

- Engine:
  - Add environment detection (best-effort):
    - GPU presence,
    - relevant runtime availability for selected packs (e.g., CUDA-enabled dependencies).
  - Provide recommended settings for:
    - separation,
    - diarization,
    - TTS/VC (when added).
- Desktop:
  - Diagnostics shows detected tier and recommended settings.

Out of scope:

- Auto-changing user settings without explicit confirmation.
- Telemetry.
- Consent gates or anti-abuse controls.

## Acceptance criteria

- Diagnostics reports a performance tier and recommended settings.
- Defaults remain CPU-safe.

## Test / verification plan

- Verify behavior on CPU-only and GPU-equipped machines (best-effort).

## Risks / open questions

- GPU detection varies by platform; keep the implementation best-effort and transparent.

## Status updates

- 2026-02-22: Created.
- 2026-02-23: Implemented best-effort performance tier detection + recommended settings and surfaced in Diagnostics; verified via build + tests.
