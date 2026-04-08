# Work Packet: WP-0187 - Clone Pre-Flight Check

## Metadata
- ID: WP-0187
- Owner: Codex
- Status: REVIEW
- Created: 2026-04-08
- Target milestone: Voice Cloning UX

## Intent

- What: Validate clone readiness before queuing an expensive voice-preserving job, and show clear warnings about missing or weak references.
- Why: Voice-preserving jobs take 10-30 minutes. If a speaker's reference profile is missing or too short, the job will silently fall back to standard TTS — wasting compute and operator time. A pre-flight check catches these issues upfront.

## Scope

In scope:
- Before enqueuing voice-preserving job, check for each clone-intent speaker:
  - Reference profile exists and file is accessible
  - Reference duration is within quality range (3-12s)
  - Reference quality score is above minimum threshold
- Show a pre-flight summary card in the Localization Run section:
  - Green: "All speakers ready for cloning"
  - Yellow: "2 speakers have weak references (short duration)" with details
  - Red: "1 speaker missing reference profile — will fall back to standard TTS"
- Allow operator to proceed anyway (with warning) or fix issues first.
- Add reference quality tips: "For best cloning, use 3-12 seconds of clear speech, no background music, natural pace."

Out of scope:
- Automatic reference acquisition (already exists via Generate source ref).
- Blocking the job entirely (always allow proceed with warning).

## Acceptance criteria
- Pre-flight check runs before voice-preserving job is queued.
- Missing/weak references are clearly flagged with actionable guidance.
- Operator can proceed or fix before continuing.
- `cargo check` + `npm run build` pass.
