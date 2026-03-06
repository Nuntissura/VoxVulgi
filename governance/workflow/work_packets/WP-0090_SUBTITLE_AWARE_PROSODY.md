# Work Packet: WP-0090 - Subtitle-aware prosody

## Metadata
- ID: WP-0090
- Owner: Codex
- Status: BACKLOG
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
