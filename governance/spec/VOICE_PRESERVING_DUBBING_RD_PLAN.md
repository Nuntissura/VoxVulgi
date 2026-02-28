# Voice-preserving dubbing R&D plan (JA/KO -> EN)

Date: 2026-02-22  
Status: Draft (R&D plan; used to drive Phase 2 implementation work packets).

## 1) Goals and constraints

- Primary goal: generate an English dub that preserves each speaker's identity as much as practical, while keeping background audio.
- Target languages: Korean/Japanese -> English.
- Local-first: default to on-device processing; offline operation after tools/models are installed.
- Inspectable: every run produces artifacts + a machine-readable report so we can debug quality regressions.
- Privacy: no telemetry by default; diagnostics exports must stay redacted and safe-by-default.
- Product constraints (project policy): do not implement explicit consent mechanisms or anti-abuse controls.

## 2) Terminology (for this plan)

- Segment: a time window with one speaker's utterance (diarization label + timestamps).
- Background: non-speech audio that should remain (music, ambience, SFX).
- Separation: splitting source audio into vocals (speech) and background.
- TTS: text-to-speech (generates English speech audio).
- VC (voice conversion): converts a source voice signal into a target speaker's voice characteristics.
- Voice-preserving: either voice-cloning TTS or VC (or a hybrid) that preserves speaker identity better than selecting a generic voice.

## 3) Baseline pipeline (ship-safely-first, non-voice-preserving)

This is the baseline we can ship even if voice-preserving options are not ready.

1. Import/ingest media into library.
2. (If needed) ASR to produce source captions.
3. Translate captions to English (existing translate pipeline).
4. Diarize speech to label speaker segments (Phase 2 prerequisite).
5. Generate English speech per segment using a fixed set of offline TTS voices (one per speaker label).
6. Time-fit each segment to its window (speed control, silence padding, or light time-stretch).
7. Separate background vs vocals, then mix generated speech back over background.
8. Export dubbed audio and optionally mux into the original video container.

Why this baseline exists:

- It gives us end-to-end product value without the added complexity (and compute/licensing risk) of voice cloning/VC.
- It produces the same artifacts we need for voice-preserving R&D (segments, timing-fit, mixing).

## 4) Voice-preserving approaches to evaluate

We should evaluate at least two approaches, because they have different tradeoffs for quality and local feasibility.

### 4.1 Approach A: voice-cloning TTS (direct)

Concept:

- Provide a short reference sample per speaker, then generate English speech directly in that voice.

Pros:

- Potentially the best identity preservation without a separate VC stage.
- Fewer moving parts in the pipeline.

Cons / risks:

- Higher compute requirements.
- Model licensing and redistribution constraints.
- Multilingual robustness (KO/JA source speaker characteristics applied to EN output).

### 4.2 Approach B: generic TTS -> VC (hybrid)

Concept:

1. Generate English speech with a high-quality generic TTS voice (or one per speaker label).
2. Convert that generated speech into the target speaker's voice via VC.

Pros:

- Lets us independently improve TTS intelligibility and VC identity preservation.
- VC stage can be swapped/tuned without changing the translation/TTS stages.

Cons / risks:

- May require per-speaker training or enrollment data beyond a short reference.
- Prosody can degrade (double-model artifacts).

### 4.3 What we explicitly measure

- Identity: does Speaker 1 still sound like Speaker 1?
- Intelligibility: is the English clear and faithful to the translation?
- Timing fit: does speech fit each segment without obvious rushing or long dead air?
- Mix quality: does background remain natural; does speech sit in the mix without clipping or pumping?
- Local feasibility: can this run without a dedicated GPU (or with an acceptable "GPU recommended" tier)?

## 5) Evaluation harness specification

We need a harness that runs a fixed pipeline over a small test set and produces repeatable artifacts and metrics.

### 5.1 Test set guidelines

- A small curated set (10-30 clips) is enough initially if it covers:
  - single speaker, multi speaker (2-4), and overlap/crosstalk cases,
  - clean speech, noisy speech, and strong background music,
  - short segments (1-3s) and longer segments (6-12s),
  - both Japanese and Korean inputs.
- Keep clips short (30-90s) to enable iteration.
- Store test media locally; do not embed media in diagnostics bundles.

### 5.2 Inputs and expected intermediate artifacts

Minimum inputs per clip:

- media file path (local)
- source segments (timings + speaker labels when available)
- English text per segment

Generated artifacts (per run):

- separated audio:
  - `background.wav`
  - `vocals.wav` (or speech stem)
- per speaker:
  - reference samples used (paths only; not copied into reports unless explicitly requested)
- per segment:
  - generated english speech (pre-fit)
  - fitted english speech (post time-fit)
- mixed output:
  - dubbed audio track
  - optional muxed video
- report:
  - a JSON report with metrics + tool versions + config summary

### 5.3 Proposed artifact layout

Use the existing derived/job layout conventions:

- `derived/jobs/<job_id>/dubbing/`
  - `inputs.json` (paths + segment IDs only; avoid raw secrets)
  - `segments.json` (speaker label + timings + EN text)
  - `separation/`
  - `tts/`
  - `vc/` (if applicable)
  - `mix/`
  - `dubbing_report.json`

Per-item outputs (once we ship the product pipeline):

- `derived/items/<item_id>/dub/en_v1/`
  - `dubbed.wav`
  - `mix_settings.json`
  - `report.json`

### 5.4 Metrics (objective + subjective)

Objective (automatable):

- Timing fit:
  - per segment duration ratio (output_duration / window_duration)
  - overflow/underflow counts beyond thresholds (e.g., > 1.10 overflow, < 0.80 underflow)
- Loudness and clipping:
  - integrated loudness (LUFS) targets for speech and final mix
  - peak clipping percentage
- Intelligibility proxy:
  - run local English ASR on generated speech; compute WER/CER versus the segment text
- Identity proxy:
  - speaker embedding cosine similarity between reference audio and generated output (per segment and aggregated)

Subjective (human-in-the-loop):

- Naturalness MOS-style rating (1-5) per clip.
- Speaker similarity rating (1-5) per speaker.
- Mix quality rating (1-5): speech presence, background preservation, artifacts.

### 5.5 Report format (draft schema)

`dubbing_report.json` should include:

- `run_id`, `created_at_ms`
- `tool_versions` (ffmpeg, separation model, diarization model, TTS/VC model)
- `config` (speaker mapping, speed limits, mix levels)
- `segments[]`:
  - `segment_id`, `speaker`, `start_ms`, `end_ms`
  - `text_en`
  - `timing`: `window_ms`, `output_ms`, `ratio`
  - `identity`: `speaker_sim` (optional)
  - `intelligibility`: `wer`/`cer` (optional)
- `summary` aggregates:
  - overflow/underflow counts, mean ratio
  - mean/median identity score (if available)
  - mean WER/CER (if available)
  - loudness/peak stats

## 6) Integration plan into VoxVulgi (Phase 2 work)

### 6.1 Engine jobs (suggested decomposition)

- `diarize_local`: produce speaker-labeled segments.
- `separate_audio`: produce vocals/background stems.
- `dub_generate`: generate EN speech per segment (TTS or voice-cloning TTS).
- `dub_voice_convert` (optional): apply VC per segment or per speaker.
- `dub_mix`: mix speech with background and normalize.
- `dub_mux` (optional): mux audio into the container.

### 6.2 UI surface (Phase 2)

- Speaker mapping UI:
  - map diarized labels to a selected voice option (baseline) or a voice-preserving option (advanced).
- Preview:
  - per-segment audition and per-speaker reference audio preview.
- Export:
  - dubbed audio-only and muxed video.

### 6.3 Storage + cleanup

- Keep all dubbing artifacts local under `derived/`.
- Provide deletion controls for any stored voice representations (reference audio copies, embeddings, caches) and keep them out of diagnostics bundles by default.

## 7) Recommended next steps

1. Implement a minimal evaluation harness as a dev-only pipeline that can run on a single clip and emit `dubbing_report.json` + artifacts.
2. Add separation and diarization jobs (even if quality is MVP-grade) so we can measure timing-fit and mixing end-to-end.
3. Evaluate at least one candidate for Approach A and one for Approach B on the curated test set, using the same harness and reporting.
4. Decide on the initial production baseline (non-voice-preserving) vs voice-preserving default, based on compute and quality results.

