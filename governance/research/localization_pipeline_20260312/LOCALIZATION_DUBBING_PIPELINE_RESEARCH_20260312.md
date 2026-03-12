# Localization Dubbing Pipeline Research (2026-03-12)

Status: research refresh for installer-state localization recovery

## 1) Question

What is the best practical pipeline for VoxVulgi's core product goal:

- local-first JA/KO -> EN subtitle + dubbing workflow,
- voice-preserving multi-speaker dubbing,
- background music/noise preservation,
- operator-visible stages and inspectable outputs,
- Windows-packaged desktop delivery.

## 2) Main finding

The strongest current systems fall into two broad families:

1. Direct speech-to-speech translation systems.
2. Staged cascades that separate transcription/translation, target speech generation, optional voice conversion/refinement, and final remix/mux.

For VoxVulgi's shipped default, the research favors a staged cascade, not a direct speech-to-speech default.

Reason:

- staged cascades expose explicit artifacts and failure points,
- staged cascades fit operator review and correction better,
- staged cascades are easier to package and debug on Windows-local desktop,
- direct S2ST systems are strong research references but are still weaker fits for predictable installer-state support and operator-facing troubleshooting.

## 3) Research families

### 3.1 Direct speech-to-speech research systems

Relevant references:

- Meta `SeamlessExpressive` / `seamless_communication`
  - official repo: `https://github.com/facebookresearch/seamless_communication`
- Google `Translatotron 2`
  - official paper title: `Translatotron 2: High-quality direct speech-to-speech translation with voice preservation`
- `TransVIP`
  - official paper title: `TransVIP: Speech-to-Speech Translation With Voice Individuality Preservation`

Why they matter:

- they target direct speech-to-speech transfer,
- they preserve more of speaker/prosody style end to end,
- they reduce the number of explicit intermediate model boundaries.

Why they are not the current VoxVulgi default:

- harder to inspect and recover stage-by-stage when output is wrong,
- weaker operator control over translated text, timing, and speaker mapping,
- more difficult local packaging/runtime assumptions for a Windows desktop product,
- less aligned with the current VoxVulgi artifact model and editable-subtitle workflow.

Conclusion:

- use these systems as R&D references and future benchmark lanes,
- do not make them the default shipped operator path yet.

### 3.2 Staged cascades

Relevant references:

- OpenVoice
  - official repo: `https://github.com/myshell-ai/OpenVoice`
  - pattern: base TTS plus tone-color conversion
- CosyVoice
  - official repo: `https://github.com/FunAudioLLM/CosyVoice`
  - pattern: zero-shot, cross-lingual, controllable multilingual TTS
- Coqui XTTS / TTS
  - official repo: `https://github.com/coqui-ai/TTS`
  - pattern: multilingual cloned/conditioned TTS baseline
- Fish Speech
  - official repo: `https://github.com/fishaudio/fish-speech`
  - pattern: expressive multilingual TTS candidate
- ElevenLabs Dubbing docs
  - official docs root: `https://docs.elevenlabs.io/`
  - useful mainly as product-pattern reference for explicit source -> dub -> output workflow visibility

Why they matter:

- they map well to explicit, inspectable desktop artifacts,
- they support staged operator correction,
- they let VoxVulgi benchmark and compare candidate backends without replacing the full workflow every time,
- they separate identity transfer, intelligibility, timing fit, and remix quality into debuggable seams.

Conclusion:

- this is the practical family for VoxVulgi's shipped path.

## 4) What the research changes in practice

The main lesson is not "pick one better model."

The lesson is:

- define a stricter stage contract,
- make every stage visible in the operator workflow,
- only then upgrade or replace individual backend components.

That means VoxVulgi should treat dubbing as:

1. ingest/source readiness,
2. source subtitle/ASR readiness,
3. translated English track readiness,
4. speaker/reference readiness,
5. target speech generation,
6. optional voice-preserving conversion or backend-specific render,
7. background-aware remix,
8. mux/export,
9. output/artifact review.

If any one stage is weak, the app must show that explicitly instead of appearing to do nothing.

## 5) Research-grounded recommendation for VoxVulgi

### 5.1 Keep the shipped default as a staged cascade

Recommended shipped default contract:

- ASR/subtitle track
- translated English subtitle track
- per-speaker settings and references
- speech generation stage
- optional voice-preserving conversion/refinement stage
- background-aware mix stage
- mux/export stage
- explicit output library

### 5.2 Treat direct S2ST as a future R&D lane

Keep research coverage for:

- SeamlessExpressive-style pipelines
- Translatotron 2 / TransVIP-style pipelines

But do not make them the installer-state default until VoxVulgi can prove:

- stable local packaging,
- operator-reviewable outputs,
- deterministic artifact visibility,
- comparable or better recovery/debug behavior than the cascade.

### 5.3 Backend strategy

The pipeline should support:

- managed default backend lane,
- benchmarked experimental backend lanes,
- explicit BYO adapter lanes.

Candidate families worth evaluating in that framework:

- OpenVoice family for voice conversion/refinement,
- CosyVoice family for controllable multilingual TTS,
- XTTS/Fish Speech/Qwen-class TTS systems as target-speech alternatives,
- future direct S2ST research lanes as benchmark-only candidates until product fit improves.

## 6) Repo-specific diagnosis

The current VoxVulgi failure is primarily productization/integration failure, not pure model weakness.

Observed repo issues:

- important Localization Studio surfaces are gated behind item/track state and long scroll chains,
- import handoff still depends too much on Media Library,
- the operator path is not explicit enough about when work starts and which stage is active,
- output discovery is present in code but not reliable enough in real operator use,
- the app previously proved backend sub-steps more often than the installed end-to-end operator path.

## 7) Resulting implementation direction

Localization recovery should prioritize:

1. explicit staged operator flow,
2. visible start/progress contract,
3. stage-by-stage output discoverability,
4. truthfully surfaced failure reasons,
5. only then backend substitutions or deeper model changes.
