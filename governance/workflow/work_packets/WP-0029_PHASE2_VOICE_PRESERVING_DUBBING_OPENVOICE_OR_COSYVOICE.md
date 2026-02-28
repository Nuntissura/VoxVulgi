# Work Packet: WP-0029 - Phase 2: Voice-preserving dubbing (OpenVoice/CosyVoice)

## Metadata
- ID: WP-0029
- Owner: Codex
- Status: DONE
- Created: 2026-02-22
- Target milestone: Phase 2 (core differentiator)

## Intent

- What: Implement a voice-preserving dubbing path (per-speaker identity) using an explicit-install local model stack with commercial-friendly defaults where possible.
- Why: Voice-preserving dubbing is the core feature; it must work locally-first and remain compatible with possible commercial distribution.

## Scope

In scope:

- Pick a first voice-preserving backend candidate (see tooling landscape doc):
  - OpenVoice V2 (MIT), or
  - CosyVoice2/3 (Apache-2.0).
- Implement:
  - explicit install + model download (no silent egress),
  - per-speaker voice profile mapping,
  - `dub_voice_preserving_v1` job producing a synthesized speech track compatible with the mixer.

Out of scope:

- Training new models.
- Any anti-abuse controls.

## Acceptance criteria

- A multi-speaker item can be dubbed with voice preservation (best-effort) and exported via the mixer/mux pipeline.

## Implementation notes

- Status moved to IN_PROGRESS on 2026-02-22 to begin immediately after WP-0028.
- Date: 2026-02-22 to 2026-02-23
- Completed items (scaffolding + UI wiring; not yet full acceptance):
  - Added explicit-install + status commands for the voice-preserving Python pack (OpenVoice/CosyVoice) and exposed it in Diagnostics.
  - Persisted per-speaker voice profile paths (reference audio) and added Subtitle Editor UI to set/clear them per diarized speaker.
  - Added `dub_voice_preserving_v1` job enqueue + status polling in the Subtitle Editor.
  - Updated mixer to prefer `dub_voice_preserving_v1` manifest when present (fallbacks to neural/pyttsx3 previews).
  - Verified: `cargo test` (engine + tauri) and `npm -C product/desktop run build`.

## Remaining work (to meet acceptance)

- Replace the placeholder voice-preserving implementation with a real local voice-cloning backend:
  - pick one initial backend (OpenVoice V2 or CosyVoice2/3),
  - add explicit model download into `tools/python/models` (no silent downloads during jobs),
  - ensure the runtime job is offline-safe (fail if models missing; do not egress),
  - ensure anti-abuse/watermarking features (if any) are disabled.
- Validate end-to-end on a multi-speaker KO/JA clip:
  - ASR -> translate -> diarize -> voice-preserving dub -> separate -> mix -> mux.

## Completion notes (2026-02-23)

- Implemented `dub_voice_preserving_v1` as a real local pipeline:
  - Kokoro neural TTS as the base speech stage
  - OpenVoice V2 ToneColorConverter as the voice-conversion stage (watermark disabled; no `wavmark` install)
  - Outputs a mixer-compatible manifest + per-segment WAVs
- Explicit installs + model downloads:
  - OpenVoice installed into the app Python venv (no implicit downloads during jobs)
  - OpenVoiceV2 model files downloaded into `tools/python/models/openvoice_v2`
- Runtime offline safety:
  - Voice-preserving job sets `HF_HUB_OFFLINE=1` and `TRANSFORMERS_OFFLINE=1`
  - Job fails fast when prerequisites are missing (FFmpeg tools, neural TTS pack, OpenVoice models)
- Verified end-to-end smoke chain on a KO multi-speaker clip:
  - ASR (ko) -> translate -> diarize -> voice-preserving dub -> separate -> mix -> mux
  - Artifacts produced: `manifest.json`, `mix_dub_preview_v1.wav`, `mux_dub_preview_v1.mp4`
