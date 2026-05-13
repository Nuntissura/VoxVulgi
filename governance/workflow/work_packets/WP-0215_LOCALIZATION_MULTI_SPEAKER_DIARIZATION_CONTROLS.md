# Work Packet: WP-0215 - Localization multi-speaker diarization controls

## Status

REVIEW

## Owner

Codex

## Scope

- Add explicit speaker-count controls for diarization in Localization Studio:
  - automatic speaker detection,
  - exact speaker count,
  - minimum/maximum speaker range.
- Flow the controls through direct diarization jobs and full localization-run jobs.
- Apply the requested speaker-count intent to the baseline Resemblyzer clustering path and the optional pyannote BYO path.
- Persist diarization run metadata so speaker-count intent, backend, assignment source, and observed speakers are auditable.
- Keep current one-speaker-label-per-subtitle behavior while documenting overlap/exclusive-diarization limits as follow-up work.

## Out of Scope

- Replacing the diarization backend.
- Full subtitle schema migration for speaker confidence, overlap ratios, or multi-label segment ownership.
- Multi-language subtitle/dub target generation.
- Automatic voice identity resolution beyond the existing speaker settings and reference flows.

## Acceptance

- Localization Studio exposes speaker-count mode controls near the localization setup and diarization controls.
- Full localization runs and direct diarization runs pass the selected speaker-count intent to the engine.
- Baseline diarization respects exact and ranged speaker-count requests.
- Pyannote BYO diarization receives `num_speakers`, `min_speakers`, or `max_speakers` when configured.
- Diarization emits a report with requested count mode, observed speaker count, backend, and assignment-source metadata.
- Automated checks cover request serialization or queued continuation behavior.
- UI-impacting verification follows `build_rules.md` without stealing focus or using operator keyboard/mouse input.

## Notes

- 2026-05-13: Created after multi-speaker readiness review found existing per-speaker dubbing support but implicit diarization speaker-count behavior.
- 2026-05-13: Implemented speaker-count request plumbing across setup-first runs, editor runs, direct diarization, baseline clustering, pyannote BYO kwargs, and diarization reports. Verification summary: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0215/20260513_031159/summary.md`.
