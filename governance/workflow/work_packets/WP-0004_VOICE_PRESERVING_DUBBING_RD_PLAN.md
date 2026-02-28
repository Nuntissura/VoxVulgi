# Work Packet: WP-0004 - Voice-preserving dubbing R&D plan

## Metadata
- ID: WP-0004
- Owner: Codex
- Status: DONE
- Created: 2026-02-22
- Target milestone: Phase 2 (voice-preserving dubbing MVP)

## Intent

- What: Produce a concrete R&D plan for voice-preserving dubbing (JA/KO -> EN), including an evaluation harness spec and an integration path into the existing job/artifacts system.
- Why: Voice-preserving dubbing is the core differentiator and highest-risk pipeline (quality + compute + licensing). We need a measurable plan before building the production pipeline.

## Scope

In scope:

- Define the baseline and voice-preserving dubbing approaches to evaluate.
- Define a repeatable evaluation harness (inputs, artifacts, metrics, reporting).
- Define recommended next implementation work packets (jobs, storage layout, UI surface).

Out of scope:

- Implementing diarization / separation / dubbing in code (follow-up work packets).
- Training custom models.

## Acceptance criteria

- `governance/spec/VOICE_PRESERVING_DUBBING_RD_PLAN.md` exists with:
  - baseline pipeline + voice-preserving approach options,
  - evaluation harness spec (artifact layout + metrics + report format),
  - selection criteria for candidate tooling (quality, perf, licensing, offline/local-first),
  - a recommended next-step implementation sequence.
- `governance/spec/TECHNICAL_DESIGN.md` links to the R&D plan from the dubbing section.

## Test / verification plan

- Desk review the plan against local-first and privacy requirements in `MODEL_BEHAVIOR.md`.

## Risks / open questions

- Compute requirements (GPU vs CPU viability) for local voice cloning/VC.
- Licensing constraints for candidate models and speaker-encoder tooling.
- Timing-fit quality for long segments (prosody and speed control).
- Multi-speaker overlap handling (crosstalk) and diarization errors.

## Status updates

- 2026-02-22: Completed and linked from `governance/spec/TECHNICAL_DESIGN.md`.

