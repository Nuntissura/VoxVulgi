# Work Packet: WP-0184 - Fish Speech 1.5 Backend Integration

## Metadata
- ID: WP-0184
- Owner: Codex
- Status: REVIEW
- Created: 2026-04-08
- Target milestone: Voice Cloning Quality

## Intent

- What: Integrate the pinned Fish Speech 1.5.1 legacy release as a managed voice cloning backend for zero-shot multilingual voice cloning.
- Why: Fish Speech 1.5.1 provides cross-lingual JA/KO/EN cloning and a useful quality comparison against the shipped voice backends. The earlier `80+ languages` and live TTS Arena rank claims are retired because the official 1.5 model card lists 13 languages and the current rank was not preserved as a stable acceptance fact.

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
- HuggingFace: https://huggingface.co/fishaudio/fish-speech-1.5
- GitHub release line: https://github.com/fishaudio/fish-speech/tree/v1.5.1
- License: Code Apache 2.0, Weights CC-BY-NC-SA-4.0
- Languages: the official 1.5 model card lists 13; the pinned v1.5.1 README explicitly names English, Japanese, Korean, Chinese, French, German, Arabic, and Spanish.
- Model payload: approximately 1.47 GB in the official `fishaudio/fish-speech-1.5` repository.
- Note: NC-SA weights mean evaluation/research use only; commercial deployment would require own training or license negotiation.

## Current research and implementation basis (2026-08-15)

- Official `v1.5.1` is the last stable 1.x release. Current Fish-Speech `main` is Fish Audio S2, which replaced the 1.5 model architecture, requires a different checkpoint/runtime, and is not a drop-in implementation of this packet.
- The official 1.5.1 server exposes `GET/POST /v1/health` and `POST /v1/tts`. TTS requests use MessagePack, carry text plus explicit reference-audio bytes and reference text, and return WAV/MP3 audio. Source: https://github.com/fishaudio/fish-speech/blob/v1.5.1/tools/server/views.py and https://github.com/fishaudio/fish-speech/blob/v1.5.1/tools/api_client.py.
- The official 1.5.1 server defaults to `checkpoints/fish-speech-1.5`, binds localhost by default, supports one worker, and accepts explicit device/half/checkpoint arguments. Source: https://github.com/fishaudio/fish-speech/blob/v1.5.1/tools/server/api_utils.py.
- The 1.5.1 Python contract pins `numpy<=1.26.4`, `vector_quantize_pytorch==1.14.24`, and `torch<=2.4.1`; it therefore needs a separate managed environment rather than reuse of VoxVulgi's main Python venv or the CosyVoice environment. Source: https://github.com/fishaudio/fish-speech/blob/v1.5.1/pyproject.toml.
- The official 1.5 checkpoint repository is approximately 1.47 GB and contains the semantic model plus codec generator. The managed installer must verify the exact required files and bytes before reporting ready.
- Current VoxVulgi implementation is incomplete: it has a catalog row and generic starter-recipe metadata only. No Fish-Speech checkout, managed venv, verified weights, server lifecycle, render wrapper, or successful dub proof exists; the advertised `python -m fish_speech` entry command is not the official v1.5.1 server command.
- Selected approach: pin the upstream `v1.5.1` source contract; create an isolated managed environment and verified checkpoint manifest; launch the official localhost-only API server quietly with a bounded health/readiness probe; render through a VoxVulgi wrapper that sends the official MessagePack schema and atomically publishes WAV output; preserve speaker-to-reference text/audio mapping in the existing voice-render request and manifest.
- Rejected: tracking current `main`/S2 under this packet (different architecture and hardware/runtime contract); treating a user-started server as a managed dependency; marking readiness from a reachable port without verified source/runtime/checkpoint identity; sharing the main or CosyVoice venv; and accepting the current catalog scaffold as integration proof.
- Required validation: exact source/runtime/checkpoint identity; offline reinstall from the packaged payload; stale/wrong server rejection; missing/corrupt weight rejection; bounded startup and shutdown; reference-audio/text propagation; successful item-level zero-shot render; a voice-preserving dub; comparison against CosyVoice 2 and OpenVoice V2; governed desktop build and headless packaged UI proof.

## Status updates

- 2026-08-15: Adversarial inspection corrected stale upstream facts and proved the repository implementation is still a scaffold. Status remains `REVIEW`; no completion claim is valid until the managed runtime, weights, render, dub, comparison, and packaged proof gates above pass.
