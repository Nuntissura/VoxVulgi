# Work Packet: WP-0088 - Reference cleanup before cloning

## Metadata
- ID: WP-0088
- Owner: Codex
- Status: BACKLOG
- Created: 2026-03-06
- Target milestone: Phase 3 preprocessing improvements

## Intent

- What: Add cleanup passes for reference clips such as denoise, de-reverb, speaker isolation, and loudness normalization before cloning.
- Why: Operators often start from noisy broadcast samples; cleanup can improve clone stability without manual external editing.

## Scope

In scope:

- Optional preprocessing pipeline for reference clips.
- Before/after preview and reveal of cleaned references.
- Store provenance for cleaned derivatives.

Out of scope:

- Automatic destructive replacement of original references.
- General-purpose audio restoration suite.

## Acceptance criteria

- Operators can run cleanup on a reference clip and compare the result.
- Original references remain preserved.
- Cleaned references can be selected for template use.

## Test / verification plan

- Audio preprocessing tests.
- Artifact path tests.
- Manual smoke with noisy reference clips.

## Status updates

- 2026-03-06: Created to reduce external prep work for real-world source material.
