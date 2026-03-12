# VoxVulgi Localization Pipeline Recommendation (2026-03-12)

Status: recommended implementation strategy

## 1) Recommended shipped pipeline

VoxVulgi should ship Localization Studio as an explicit staged cascade:

1. Select or import source media.
2. Confirm or create source subtitle/ASR track.
3. Confirm or create translated English track.
4. Confirm speaker/reference state.
5. Run target speech generation.
6. Run voice-preserving conversion or experimental backend render where applicable.
7. Run background-aware mix.
8. Run MP4 mux/export.
9. Review outputs from one obvious library surface.

This must be the visible operator contract, not just an internal implementation detail.

## 2) Required operator-facing behaviors

- Localization Studio must let the operator configure settings before the run starts.
- The app must provide an explicit start action for a localization run or an equally explicit pre-start review contract.
- Each active item must show stage progress, not just a generic background queue state.
- Output visibility must be first-class:
  - source video,
  - subtitle files,
  - dubbed speech audio,
  - working artifact folder,
  - exported MP4,
  - export folder.
- If a stage is blocked, the blocking reason must be visible in the workflow itself.

## 3) Required technical behaviors

- The default path remains a cascade, not direct speech-to-speech.
- Direct speech-to-speech systems remain future benchmark/R&D lanes.
- Background preservation should prefer separated background stems where available.
- If separation is unavailable, source-audio fallback must be explicit and operator-visible.
- Backend comparisons must run inside the same stage contract rather than bypassing it.

## 4) Backend policy

- Managed default: keep a single maintained default lane until benchmark evidence supports a change.
- Experimental lanes: run through explicit adapters and standard manifests.
- Promotion: benchmark winners can influence reusable template/cast-pack defaults only after they are visible and testable through the standard Localization Studio flow.

## 5) Work packet implications

This research should drive the next localization-first packets:

- pipeline/stage contract governance,
- installer-state localization runtime repair,
- operator-visible start/progress/output surfaces,
- advanced-surface discoverability only after the baseline path works reliably,
- stronger end-to-end proof that uses the installed app path rather than only sub-step harnesses.
