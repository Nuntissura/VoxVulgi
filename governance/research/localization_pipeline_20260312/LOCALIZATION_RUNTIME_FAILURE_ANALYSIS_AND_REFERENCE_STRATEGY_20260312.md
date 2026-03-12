# Localization Runtime Failure Analysis and Reference Strategy (2026-03-12)

Status: research refresh for post-0.1.6 localization recovery

## 1) Problem statement

VoxVulgi's core product promise is not just "voice cloning exists in the repo". It is:

- import local JA/KO source media,
- produce subtitles and English translation,
- keep background music/noise when possible,
- produce a visible dubbed MP4,
- let an operator find the source, working artifacts, and exported deliverables without guessing.

The current repo proves sub-parts of that chain, but the installed operator path still fails often enough that the feature is perceived as broken.

## 2) Repo-specific failure diagnosis

The main failure is not that the field lacks a better paper or model. It is that the current product path still depends on hidden operator work at the speaker-reference stage.

Current repo facts:

- `WP-0150` proves the staged cascade can reach ASR -> translate -> diarize -> dub -> mix -> mux in a controlled smoke harness.
- The controlled smoke only succeeds after speaker references are explicitly assigned.
- The shipped `jobs_enqueue_localization_run_v1` contract currently pauses at the voice-plan checkpoint when speaker labels exist but reference/routing state is incomplete.
- The frontend now surfaces that checkpoint more clearly, but it still does not help the operator build the missing speaker references quickly enough for a first real dub.

That means the localization workflow can appear to "do nothing" even though the backend is behaving as designed.

## 3) What current primary sources imply

### 3.1 Staged cascade is still the right shipped default

Direct speech-to-speech systems remain strong research references, but they are not yet the best fit for VoxVulgi's local-first Windows desktop operator path.

Primary references:

- Meta Seamless Communication / SeamlessExpressive:
  - `https://github.com/facebookresearch/seamless_communication`
- `Translatotron 2: High-quality direct speech-to-speech translation with voice preservation`
  - `https://arxiv.org/abs/2107.08661`
- `TransVIP: Speech to Speech Translation System with Voice and Isochrony Preservation`
  - `https://arxiv.org/abs/2405.17809`

These systems are useful benchmark and R&D lanes, but the repo should keep the shipped path as a stage-explicit cascade because that is easier to inspect, recover, and package.

### 3.2 Background preservation should stay separation-first with graceful fallback

Primary reference:

- Demucs official repo:
  - `https://github.com/facebookresearch/demucs`

Demucs remains a strong open reference for vocals-vs-accompaniment separation. This supports VoxVulgi's current design choice:

- prefer explicit background stems when available,
- but keep the current source-audio fallback so preview generation does not dead-end when separation fails or is unavailable.

### 3.3 Reference quality is a first-class variable, not an optional detail

Primary references:

- OpenVoice discussion guidance:
  - `https://github.com/myshell-ai/OpenVoice/discussions/313`
- CosyVoice official repo:
  - `https://github.com/FunAudioLLM/CosyVoice`

Relevant practical guidance from those sources:

- short or noisy references reduce output quality,
- single-speaker reference audio matters,
- longer/cleaner references are safer than arbitrary snippets,
- modern multilingual backends are increasingly strong, but reference quality still governs the result.

This matches the product diagnosis exactly: the missing step is not only "show the voice-plan checkpoint", but "help the operator create usable reference clips from the source media".

## 4) Resulting product strategy

The next localization recovery step should be:

1. keep the current staged localization contract,
2. keep separation-first background preservation with current fallback behavior,
3. add assisted speaker-reference extraction from the current source media after diarization,
4. let the operator review and apply those extracted reference bundles quickly,
5. then continue the dub -> mix -> mux run without sending the operator off to manual file hunting.

## 5) Practical recommendation for VoxVulgi

### 5.1 Add assisted speaker-reference extraction

For each diarized speaker:

- gather early subtitle-aligned segments for that speaker,
- exclude very short or silent spans,
- concatenate enough material to reach a practical target duration,
- store the candidate bundle under the current item's managed voice-reference area,
- present it as a reviewable/applicable candidate rather than silently forcing it.

This should produce a fast "first working dub" path without pretending the automatically extracted reference is perfect.

### 5.2 Keep operator control explicit

The operator should be able to:

- see that the localization run is paused on speaker/reference readiness,
- generate candidate references from the current media,
- audition or inspect them,
- apply them to the current voice plan,
- then continue the staged localization run.

### 5.3 Do not silently replace the core backend path

This packet does not recommend swapping VoxVulgi's default backend family yet.

The practical bottleneck is reference acquisition and operator flow, not a lack of candidate backends.

## 6) Follow-on work

- `WP-0152`: assisted speaker-reference extraction and first-dub recovery
- `WP-0143`: current-item handoff, visible outputs, and operator-facing progress recovery
- `WP-0095`: manual smoke only after the above path is implemented and reinstalled
