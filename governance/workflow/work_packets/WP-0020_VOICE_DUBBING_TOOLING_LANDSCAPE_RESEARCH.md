# Work Packet: WP-0020 - Voice dubbing tooling landscape research (2026)

## Metadata
- ID: WP-0020
- Owner: Codex
- Status: DONE
- Created: 2026-02-22
- Target milestone: Phase 2 (voice-preserving dubbing)

## Intent
- What: Research the current (2026) landscape of local-first, open-source tooling for subtitles, alignment, diarization, separation, TTS, and voice conversion/voice cloning for JA/KO -> EN dubbing.
- Why: The quality and licensing landscape changes quickly. We want a commercially-safe default stack, and a clear BYO (bring-your-own-weights) path where licenses restrict redistribution.

## Scope

In scope:
- Subtitle + alignment tooling (local): Whisper/whisper.cpp ecosystem, forced alignment approaches, diarization options.
- Audio separation tooling (local): vocals/background.
- Speech generation tooling (local): TTS candidates with licensing and packaging notes.
- Voice-preserving candidates (local): voice conversion / voice cloning (or hybrid pipelines).
- For each candidate:
  - code license and weight/model license,
  - whether it is a service or local tool,
  - OS support (Windows/macOS/Linux) and runtime dependencies,
  - CPU/GPU requirements and expected performance tier,
  - packaging strategy (bundle vs explicit install vs BYO).

Out of scope:
- Implementing the dubbing pipeline (tracked separately).
- Training new models.
- Any telemetry or background network egress.

## Acceptance criteria
- `governance/spec/VOICE_DUBBING_TOOLING_LANDSCAPE_2026.md` exists and includes:
  - a shortlist of candidates by category (ASR, alignment, diarization, separation, TTS, VC),
  - a license/redistribution matrix (code + weights),
  - a recommended default stack that is safe for possible commercial distribution,
  - a BYO path for non-commercial or unclear-weight-license options.
- `governance/spec/TECHNICAL_DESIGN.md` links to the landscape doc from the dubbing section.

## Implementation notes
- Assume possible future commercial distribution; do not depend on non-commercial weights for default/bundled features.
- Preserve project constraints: no silent network egress; any downloads must be explicit and user-controlled.

## Test / verification plan
- Desk review: cross-check licenses against primary sources (upstream repos, model cards/licenses) and record citations.

## Risks / open questions
- Licenses for model weights often differ from code licenses; redistribution may be restricted even when code is permissive.
- Many "amazing" demos are service-backed; we must separate local OSS from hosted offerings.
- GPU requirements may make some approaches impractical for the default experience.

## Status updates
- 2026-02-22: Started (create landscape doc + begin research and matrix).
- 2026-02-22: Completed landscape doc with local OSS vs service separation, license/weights matrix, and default vs BYO recommendations.
