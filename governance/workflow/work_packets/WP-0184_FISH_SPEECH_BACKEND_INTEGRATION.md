# Work Packet: WP-0184 - Fish Speech 1.5 Backend Integration

## Metadata
- ID: WP-0184
- Owner: Codex
- Status: REVIEW
- Created: 2026-04-08
- Target milestone: Voice Cloning Quality

## Intent

- What: Integrate Fish Speech 1.5 as a managed voice cloning backend for highest-quality zero-shot voice cloning with 80+ language support.
- Why: Fish Speech 1.5 has the highest benchmark scores on TTS Arena (ELO 1339) and supports 80+ languages including JA/KO/EN with cross-lingual cloning. It provides a quality ceiling reference for voice clone comparison.

## Scope

In scope:
- Add Fish Speech 1.5 Python inference package as a managed dependency.
- Create a voice backend adapter (text + reference WAV → cloned speech WAV).
- Wire into the voice-preserving pipeline as a selectable backend.
- Add a starter recipe in Diagnostics for Fish Speech.
- Register in the voice backend catalog.
- Benchmark comparison against CosyVoice 2 and OpenVoice V2 + Kokoro.

Out of scope:
- Fish Speech S2 (API-only, not self-hosted).
- Commercial redistribution of weights (CC-BY-NC-SA — weights are for research/evaluation).
- Training custom models.

## Acceptance criteria
- Operator can select Fish Speech 1.5 as the voice backend for an item.
- Voice-preserving dub produces output using Fish Speech zero-shot cloning.
- Benchmark report can compare Fish Speech vs CosyVoice 2 vs OpenVoice V2.
- `cargo check` + `npm run build` pass.

## Research notes
- HuggingFace: fishaudio/fish-speech-1.5
- GitHub: github.com/fishaudio/fish-speech
- License: Code Apache 2.0, Weights CC-BY-NC-SA-4.0
- Languages: 80+ including JA/KO/EN
- Model size: ~4B params (~8 GB weights)
- Note: NC-SA weights mean evaluation/research use only; commercial deployment would require own training or license negotiation.
