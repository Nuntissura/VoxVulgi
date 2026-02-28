# Voice dubbing tooling landscape (2026): local-first, open-source, license-aware

Date: 2026-02-22  
Status: Final (WP-0020; informs Phase 2 implementation choices).

## 1) Goal

Build a practical, local-first dubbing stack for JA/KO -> EN that can be packaged for desktop and remains viable if we later distribute commercially.

This document focuses on:

- separating local OSS from service-backed offerings,
- distinguishing code licenses from model/weight licenses,
- defining what we can ship by default vs what must be explicit-install or BYO.

## 2) Repo constraints (non-negotiables)

- Local-first default.
- No silent network egress.
- No telemetry by default.
- License-aware packaging (code and weights must be checked separately).
- Project guardrails:
  - Explicit consent mechanisms must not be implemented.
  - Anti-abuse controls must not be included.

## 3) Packaging categories (terms we use)

- `Bundle`: can ship inside the app installer (code + weights are redistributable).
- `Explicit install`: app can download/install on user action; still must be redistributable and integrity-checked.
- `BYO`: user supplies weights/assets locally; we do not download or redistribute them.
- `Avoid`: incompatible with our project constraints or licensing goals.
- `Service`: hosted API/service; avoid for default pipeline.

## 4) Tooling matrix (primary sources)

Notes:

- Many "best" demos are service-backed; we list them for context but do not depend on them.
- If a tool requires a Hugging Face token, gated model terms, or "share contact info" click-through, treat it as `BYO`.
- If weights are non-commercial (NC) or unclear, treat as `BYO` (or `Avoid`) for a commercial-friendly default.

### 4.1 Subtitles (ASR)

| Candidate | Local/Service | Code license | Weights license | Packaging | Notes / primary sources |
|---|---|---|---|---|---|
| Whisper reference (openai/whisper) | Local | MIT | Weights distributed by OpenAI; treat as not-bundled unless terms are explicit | BYO / explicit install | https://github.com/openai/whisper |
| whisper.cpp (ggml-org/whisper.cpp) | Local | MIT | Whisper-family weights | Explicit install (recommended) | CPU-friendly; already integrated in VoxVulgi Phase 1. https://github.com/ggml-org/whisper.cpp |
| WhisperX (m-bain/whisperX) | Local | BSD-2-Clause | Uses Whisper + separate alignment models | Explicit install | Word timestamps + optional diarization; see diarization notes about gated models. https://github.com/m-bain/whisperX |

### 4.2 Alignment (forced alignment / word timestamps)

| Candidate | Local/Service | License | Packaging | Notes / primary sources |
|---|---|---|---|---|
| WhisperX alignment | Local | BSD-2-Clause | Explicit install | Uses external alignment models; check each model's license at download source. https://github.com/m-bain/whisperX |
| Montreal Forced Aligner (MFA) | Local | MIT | Explicit install | Good for "have transcript, align to audio" workflows. Code license: https://raw.githubusercontent.com/MontrealCorpusTools/Montreal-Forced-Aligner/master/LICENSE . Model distribution docs: https://montreal-forced-aligner.readthedocs.io/en/stable/user_guide/models/index.html |
| whisper-timestamped | Local | AGPL-3.0 | Avoid | Not commercial-friendly. https://github.com/linto-ai/whisper-timestamped |

### 4.3 Diarization ("who spoke when?")

We split diarization into:

- ready-made diarization pipelines (often gated),
- building blocks (VAD + speaker embeddings) for a local, non-gated pipeline.

#### A) Ready-made diarization pipelines

| Candidate | Local/Service | License | Packaging | Notes / primary sources |
|---|---|---|---|---|
| pyannote.audio pipeline + Community-1 weights | Local | Code: MIT; model: varies; commonly CC-BY | BYO (gated access) | HF token + gated terms for pipelines. Example: https://huggingface.co/pyannote/speaker-diarization-community-1 and code: https://github.com/pyannote/pyannote-audio |
| WhisperX diarization path | Local | BSD-2-Clause (code) | BYO (gated) | WhisperX diarization errors often stem from gated pyannote models requiring HF token + accepted terms. Example issue: https://github.com/m-bain/whisperX/issues/705 |
| NVIDIA NeMo Sortformer diarization | Local | CC-BY-NC-4.0 (weights) | BYO (non-commercial) | Non-commercial weights: https://huggingface.co/nvidia/diar_sortformer_4spk-v1 |

#### B) Building blocks (recommended for a commercial-friendly default)

| Candidate | Local/Service | Code/weights license | Packaging | Notes / primary sources |
|---|---|---|---|---|
| Silero VAD (snakers4/silero-vad) | Local | MIT | Explicit install (CPU) | VAD building block; can be used to segment speech without gated weights. License: https://raw.githubusercontent.com/snakers4/silero-vad/master/LICENSE . Models list/wiki: https://github.com/snakers4/silero-vad/wiki/Version-history-and-Available-Models |
| SpeechBrain ECAPA speaker embeddings | Local | Apache-2.0 (HF license) | Explicit install | Embeddings for clustering. https://huggingface.co/speechbrain/spkrec-ecapa-voxceleb |
| NVIDIA Titanet speaker embeddings | Local | CC-BY-4.0 (HF license) | Explicit install | Alternative embedding model; supports commercial use with attribution. https://huggingface.co/nvidia/speakerverification_en_titanet_large |

Implementation implication:

- We can build a local diarization baseline as: VAD -> chunk -> speaker embedding -> clustering.
- This avoids gated pyannote pipelines for the default stack, while still allowing a BYO pyannote path for power users.

### 4.4 Separation (vocals/background)

| Candidate | Local/Service | Code license | Weights/model license | Packaging | Notes / primary sources |
|---|---|---|---|---|---|
| Spleeter (deezer/spleeter) | Local | MIT | Repo distributes pretrained models via releases | Explicit install | "including pretrained models": https://github.com/deezer/spleeter . Model distribution doc: https://github.com/deezer/spleeter/wiki/3.-Models |
| Demucs (facebookresearch/demucs) | Local | MIT | Pretrained weights license unclear | BYO (until clarified) | Code MIT: https://raw.githubusercontent.com/facebookresearch/demucs/main/LICENSE . Weight-license uncertainty: https://github.com/facebookresearch/demucs/issues/327 |
| Open-Unmix (sigsep/open-unmix-pytorch) | Local | MIT (code) | Some provided weights are non-commercial | BYO (weights vary) | README notes `umxl` weights are CC BY-NC-SA 4.0: https://raw.githubusercontent.com/sigsep/open-unmix-pytorch/master/README.md |

### 4.5 TTS (baseline speech generation)

| Candidate | Local/Service | Code license | Weights license | Packaging | Notes / primary sources |
|---|---|---|---|---|---|
| Kokoro-82M | Local | Apache-2.0 | Apache-2.0 | Explicit install | HF model card license: https://huggingface.co/hexgrad/Kokoro-82M . GitHub repo notes espeak-ng for some languages: https://github.com/hexgrad/kokoro |
| MeloTTS | Local | MIT | MIT | Explicit install | HF model card license: https://huggingface.co/myshell-ai/MeloTTS-English . Code repo: https://github.com/myshell-ai/MeloTTS |
| Bark (suno-ai/bark) | Local | MIT | MIT | Explicit install | Repo states MIT + commercial use: https://github.com/suno-ai/bark |
| Piper (rhasspy/piper) | Local | MIT (archived) | Voices: check per-voice model cards | BYO (runtime moved to GPL) | Repo is archived and says "Development has moved: ... piper1-gpl": https://raw.githubusercontent.com/rhasspy/piper/master/README.md . Voice set has per-voice MODEL_CARD with dataset license links: https://huggingface.co/rhasspy/piper-voices/blob/main/en/en_US/lessac/medium/MODEL_CARD |

### 4.6 Voice-preserving (voice cloning / VC)

| Candidate | Local/Service | Code license | Weights license | Packaging | Notes / primary sources |
|---|---|---|---|---|---|
| OpenVoice V2 | Local | MIT | MIT | Explicit install | HF model card states MIT + free commercial use: https://huggingface.co/myshell-ai/OpenVoiceV2 . Uses MeloTTS: https://github.com/myshell-ai/MeloTTS |
| CosyVoice2 / CosyVoice3 family | Local | Apache-2.0 (HF license) | Apache-2.0 | Explicit install | HF model card license: https://huggingface.co/FunAudioLLM/CosyVoice2-0.5B |
| RVC WebUI | Local | MIT | BYO | BYO | Typically user-trained models. https://github.com/RVC-Project/Retrieval-based-Voice-Conversion-WebUI |
| GPT-SoVITS | Local | MIT | Mixed / third-party | BYO | Model packs often bundle third-party weights; license hygiene required. https://github.com/RVC-Boss/GPT-SoVITS |
| F5-TTS | Local | MIT (code) | CC-BY-NC-4.0 (weights) | BYO (non-commercial) | HF license: https://huggingface.co/SWivid/F5-TTS |
| Chatterbox | Local | MIT | (verify per release) | Avoid (project constraint) | Built-in watermarking is an anti-abuse control and violates our repo guardrail. README: https://github.com/resemble-ai/chatterbox |

### 4.7 End-to-end speech-to-speech translation (optional)

| Candidate | Local/Service | Weights license | Packaging | Notes / primary sources |
|---|---|---|---|---|
| SeamlessM4T | Local | CC-BY-NC 4.0 | BYO (non-commercial) | License statement: https://huggingface.co/facebook/seamless-m4t-medium/blob/main/README.md |

### 4.8 Service-backed solutions (context only; avoid for default)

Examples (not exhaustive):

- ElevenLabs (voice cloning + TTS) - Service.
- HeyGen / Rask / Dubverse style products - Service.
- Cloud STT/MT APIs - Service.

We do not pick these for the default pipeline due to local-first + no silent egress. If we ever add them, it must be explicit user action with clear disclosure.

## 5) Recommended stack for VoxVulgi Phase 2

### Default stack (commercial-friendly, local-first)

- ASR: keep current `whisper.cpp` (explicit model installs).
- Alignment: MFA (explicit install) for "fit TTS to exact timings" workflows.
- Diarization baseline: VAD + embeddings + clustering (Silero VAD + SpeechBrain ECAPA or Titanet).
- Separation baseline: Spleeter (explicit install).
- Baseline TTS (non voice-preserving): Kokoro or MeloTTS (explicit install). Useful for preview dubs and timing.

### Voice-preserving (explicit-install GPU tier)

- OpenVoice V2 (MIT) as first voice-cloning candidate.
- CosyVoice2/3 as alternative open-weight candidate (Apache-2.0) if packaging/runtime fits.

### BYO-only add-ons (not safe to bundle by default)

- pyannote diarization pipelines (gated HF).
- F5-TTS weights (non-commercial).
- Demucs weights (unclear) and Open-Unmix weights that are non-commercial.
- Piper runtime moved to GPL (license implications for distribution).

## 6) Open questions (track for Phase 2 WPs)

- Can we get acceptable diarization quality from a non-gated VAD+embedding pipeline for JA/KO content, or do we need BYO pyannote as an opt-in power-user path?
- What is the best "voice-preserving default" that is both high quality and realistically packageable on Windows/macOS without silent downloads?
- How do we score identity similarity locally (speaker-embedding cosine similarity) without exporting raw user audio in diagnostics bundles?

