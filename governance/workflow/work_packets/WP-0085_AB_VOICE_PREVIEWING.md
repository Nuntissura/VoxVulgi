# Work Packet: WP-0085 - A/B voice previewing

## Metadata
- ID: WP-0085
- Owner: Codex
- Status: DONE
- Created: 2026-03-06
- Target milestone: Phase 2 operator review tooling

## Intent

- What: Allow operators to generate and compare two or more clone/style variants for the same speaker before committing to a full dub.
- Why: Voice setup choices are easier to judge from quick side-by-side previews than from one irreversible run.

## Scope

In scope:

- Variant preview generation for the same segment set.
- Side-by-side compare controls.
- Persist preferred variant back into the active template or item settings.

Out of scope:

- Full automatic ranking with no operator review.
- Cloud-based review services.

## Acceptance criteria

- Operators can generate at least two preview variants for a speaker.
- The app keeps variant outputs separate and clearly labeled.
- One variant can be promoted into the active template/item settings.

## Test / verification plan

- Artifact naming tests.
- UI build.
- Manual preview compare smoke.

## Status updates

- 2026-03-06: Created to make voice-choice review faster and more repeatable.
- 2026-03-06: Completed. Operators can queue two labeled voice-preserving variants for a chosen speaker, inspect alternate artifacts, and promote either variant back into live speaker settings. Proof: `product/desktop/Build Target/tool_artifacts/wp_runs/WP-0085/20260306_204301/summary.md`.
