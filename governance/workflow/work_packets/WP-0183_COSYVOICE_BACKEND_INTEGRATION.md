# Work Packet: WP-0183 - CosyVoice 2 Backend Integration

## Metadata
- ID: WP-0183
- Owner: Codex
- Status: REVIEW
- Created: 2026-04-08
- Target milestone: Voice Cloning Quality

## Intent

- What: Integrate CosyVoice 2 as a managed voice cloning backend that replaces the two-stage Kokoro+OpenVoice pipeline with single-pass zero-shot cloned TTS.
- Why: The current two-stage pipeline (Kokoro TTS → OpenVoice V2 tone color swap) loses prosody, emotion, and accent. CosyVoice 2 does TTS + voice cloning in one pass with native JA/KO/EN cross-lingual support. Apache 2.0 license.

## Scope

In scope:
- Add CosyVoice 2 Python package as a managed dependency (pip install).
- Create a voice backend adapter that accepts text + reference WAV and returns cloned speech WAV.
- Wire into the existing voice-preserving pipeline as an alternative to OpenVoice V2 + Kokoro.
- Add a starter recipe in Diagnostics for CosyVoice 2.
- Register as a selectable backend in the voice backend catalog.
- Benchmark comparison against the existing Kokoro + OpenVoice V2 pipeline.

Out of scope:
- Replacing the default managed backend (requires benchmark evidence first).
- CosyVoice 3 (evaluate v2 first, upgrade later).
- Training custom models.

## Acceptance criteria
- Operator can select CosyVoice 2 as the voice backend for an item.
- Voice-preserving dub produces output using CosyVoice 2 zero-shot cloning.
- Benchmark report can compare CosyVoice 2 vs OpenVoice V2 + Kokoro.
- `cargo check` + `npm run build` pass.

## Research notes
- HuggingFace: FunAudioLLM/CosyVoice2-0.5B
- GitHub: github.com/FunAudioLLM/CosyVoice
- License: Apache 2.0
- Languages: JA/KO/EN/ZH + 6 more
- Model size: 0.5B params (~1-2 GB weights)
