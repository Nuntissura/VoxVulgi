# Work Packet: WP-0028 - Phase 2: Neural TTS baseline (Kokoro or MeloTTS)

## Metadata
- ID: WP-0028
- Owner: Codex
- Status: DONE
- Created: 2026-02-22
- Target milestone: Phase 2

## Intent

- What: Replace/augment the `pyttsx3` TTS preview with an explicit-install neural TTS baseline (commercial-friendly weights preferred).
- Why: System TTS is not representative of final dubbing quality; we need a local neural TTS baseline that can be packaged safely.

## Scope

In scope:

- Pick one default neural TTS candidate with clear redistribution terms (see tooling landscape doc).
- Implement:
  - pack install (Python deps + model download, explicit user action),
  - `tts_neural_local_v1` job that renders per-segment WAV outputs and a manifest compatible with the mixer.

Out of scope:

- Voice cloning / identity preservation (WP-0029).

## Acceptance criteria

- A neural TTS job runs locally and produces consistent segment audio artifacts suitable for mixing.

## Implementation notes

- Date: 2026-02-22
- Completed items:
  - Added explicit-install neural pack commandset (status + install).
  - Added `tts_neural_local_v1` job producing segment WAVs and `tts_neural_local_v1` manifest under derived `tts_preview`.
  - Exposed new diagnostics diagnostics row + install button for the pack.
  - Updated mix job to prefer neural preview manifest when present (fallbacks to `pyttsx3_v1`).
  - Added editor action/button and job status polling for the neural preview path.
