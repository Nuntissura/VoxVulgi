# Work Packet: WP-0034 - ROI-07: In-app audio preview player for stems/dub outputs (A/B compare)

## Metadata
- ID: WP-0034
- Owner: Codex
- Status: DONE
- Created: 2026-02-22
- Target milestone: Phase 2 (voice-preserving dubbing)

## Intent

- What: Add an in-app audio preview player so users can listen to derived stems (vocals/background) and dub preview outputs without leaving VoxVulgi.
- Why: Fast A/B iteration is critical for tuning separation, TTS/VC, and mixing.

## Scope

In scope:

- Desktop:
  - Add per-item preview player:
    - original audio,
    - vocals stem,
    - background stem,
    - dub preview mix output,
    - muxed preview output (video).
  - Provide A/B compare workflow (toggle or quick-switch) and basic level controls.
- Engine/Tauri:
  - Provide safe file access to derived outputs for playback (no telemetry).

Out of scope:

- Advanced waveform editor.
- Telemetry.
- Consent gates or anti-abuse controls.

## Acceptance criteria

- Users can play stems and dub outputs for a library item inside the app.
- A/B compare is fast and does not require manual file browsing.

## Test / verification plan

- Generate stems + dub preview for an item, then verify playback for each artifact in the app UI.

## Risks / open questions

- Tauri file access and media playback paths need to remain safe and cross-platform.

## Status updates

- 2026-02-22: Created.
- 2026-02-23: Added in-app artifacts browser with audio preview player for stems + dub outputs and quick video source toggle (original vs muxed preview); verified via build + tests.
