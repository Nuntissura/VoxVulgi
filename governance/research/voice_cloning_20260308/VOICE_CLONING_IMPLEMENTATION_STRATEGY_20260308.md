# Voice Cloning Implementation Strategy (2026-03-08)

Date: 2026-03-08  
Status: Final strategy note for `WP-0107`

## 1) Strategy summary

Do not replace VoxVulgi's shipped OpenVoice path blindly.

Instead:

1. keep the current managed backend as the default
2. add a first-class backend catalog and recommendation surface
3. add a benchmark lab that measures the outputs VoxVulgi actually produced
4. add a BYO experimental adapter layer for faster-moving OSS backends

This preserves the stable path while opening a serious research lane.

## 2) Why this is the right sequence

### 2.1 OpenVoice is narrow but operational

The current VoxVulgi stack already has:

- packaging
- install/status plumbing
- voice templates and cast packs
- multi-reference storage
- QC
- A/B previewing
- artifact browser integration

Replacing it immediately would throw away real product value before we have a benchmark harness.

### 2.2 The field is moving too fast for one hard-coded backend

CosyVoice, Seed-VC, IndexTTS2, XTTS v2, and Fish-Speech move quickly. A product that hard-codes only one backend will age badly.

### 2.3 Product users need backend strategy, not only backend names

Users do not just need "install model X". They need to know:

- when to choose a two-stage TTS + VC backend
- when to choose direct zero-shot TTS
- what reference quality and quantity are needed
- what tradeoffs exist between timing, identity, speed, and naturalness

## 3) Proposed work packets

### 3.1 WP-0107

Research transfer and spec sync:

- new research corpus
- machine-readable candidate matrix
- spec and roadmap updates

### 3.2 WP-0108

Voice backend catalog and recommendation surface:

- built-in descriptor list for shipped and experimental backends
- capability/risk/license/runtime notes
- recommendation logic based on item language, available references, performance tier, and operator priorities

### 3.3 WP-0109

Voice benchmark lab:

- discover current and variant output artifacts
- compute comparable metrics
- rank outputs and emit JSON/Markdown reports
- make backend or variant changes evidence-driven

### 3.4 WP-0110

Experimental BYO backend adapters:

- explicit local adapter registration
- no silent installs or downloads
- probe/test support
- readiness to benchmark stronger OSS backends without replacing the default path

## 4) Data and architecture additions

### 4.1 Backend catalog

Needed because:

- Diagnostics currently exposes only installed package versions for the shipped path
- Localization Studio has no explicit concept of backend family or recommendation

Suggested descriptor fields:

- `id`
- `display_name`
- `family`
- `mode` (`tts_vc`, `direct_zero_shot_tts`, `voice_conversion_only`)
- `install_mode` (`managed`, `explicit_install`, `byo`)
- `code_license`
- `weights_license`
- `source_languages`
- `target_languages`
- `gpu_recommended`
- `reference_style`
- `strengths`
- `risks`
- `status`

### 4.2 Recommendation engine

Recommendation inputs:

- source language
- target language
- performance tier
- reference count and estimated quality
- operator goal:
  - best identity
  - best timing fit
  - best expressivity
  - fastest local turnaround

Recommendation outputs:

- preferred backend
- fallback backend
- warnings
- reasoning text

### 4.3 Benchmark report

Benchmark scope should stay local and artifact-driven.

Candidate metrics:

- converted segment ratio
- output segment coverage
- total rendered duration vs target subtitle duration
- silence/clipping/noise warnings
- reference coverage and reference duration
- speaker-similarity proxy from local embeddings
- average loudness and peak headroom
- variant ranking score with explicit weights

Outputs:

- item-scoped JSON report
- operator-facing Markdown summary

### 4.4 BYO adapter contract

The adapter layer should remain explicit and local.

Suggested contract:

- operator supplies local executable/script path
- operator supplies working directory and probe arguments
- adapter declares capability metadata
- app can:
  - validate config
  - run a non-destructive probe
  - expose readiness in Diagnostics
  - surface the adapter in the catalog and benchmark UI

Do not require the app to auto-install or auto-update experimental backends.

## 5) Strong OSS candidate mapping for VoxVulgi

### 5.1 Seed-VC

Use as:

- experimental alternative voice-conversion stage

Why:

- aligns with the current TTS + VC design
- especially attractive for timbre preservation

### 5.2 CosyVoice

Use as:

- experimental direct zero-shot TTS backend

Why:

- multilingual
- promptable
- better match for vendor-like modern voice controls

### 5.3 IndexTTS2

Use as:

- benchmark candidate and possible future advanced dubbing backend

Why:

- duration and emotion control map unusually well to dubbing needs

### 5.4 XTTS v2

Use as:

- practical OSS baseline comparator for direct zero-shot TTS

### 5.5 Fish-Speech

Use as:

- long-form expressive benchmark candidate and feature reference

## 6) What not to do yet

- do not swap the shipped default backend before a benchmark harness exists
- do not auto-download experimental backends
- do not promise commercial-safe packaging for every promising OSS repo without confirming code and weight licenses
