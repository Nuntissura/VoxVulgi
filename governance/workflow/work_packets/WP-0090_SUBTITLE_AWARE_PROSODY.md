# Work Packet: WP-0090 - Subtitle-aware prosody

## Metadata
- ID: WP-0090
- Owner: Codex
- Status: DONE
- Created: 2026-03-06
- Target milestone: Phase 3 naturalness improvements

## Intent

- What: Use subtitle punctuation, line breaks, emphasis markers, and timing structure to shape spoken pauses and emphasis.
- Why: Subtitle text already contains strong cues about delivery; exploiting them can make English dubbing sound less flat.

## Scope

In scope:

- Derive prosody hints from subtitle structure.
- Let operators review or disable subtitle-aware shaping.
- Feed the same hint layer into preview and final dubbing jobs.

Out of scope:

- Full expressive script markup language.
- End-to-end neural reenactment outside the subtitle workflow.

## Acceptance criteria

- Subtitle-aware prosody can be enabled per item/template.
- Generated speech reflects stronger pauses/emphasis when cues exist.
- Operators can revert to plain delivery when subtitle cues are noisy.

## Test / verification plan

- Hint extraction tests.
- Timing-fit regression tests.
- Manual preview smoke on punctuated dialogue.

## Status updates

- 2026-03-06: Created to improve naturalness using data already present in subtitle tracks.
- 2026-03-06: Initial groundwork landed through line-break and punctuation shaping inside TTS text preparation, but explicit operator toggles/review remain open.
- 2026-03-06: Completed. Subtitle-aware pacing is now operator-controlled through per-speaker, template, and profile settings and flows through preview plus voice-preserving jobs. Proof: `product/desktop/Build Target/tool_artifacts/wp_runs/WP-0090/20260306_204301/summary.md`.
