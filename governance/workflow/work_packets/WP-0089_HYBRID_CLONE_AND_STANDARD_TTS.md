# Work Packet: WP-0089 - Hybrid clone and standard TTS mode

## Metadata
- ID: WP-0089
- Owner: Codex
- Status: BACKLOG
- Created: 2026-03-06
- Target milestone: Phase 3 efficient dubbing strategies

## Intent

- What: Support hybrid dubbing where major speakers use voice-preserving cloning and minor/background speakers use standard TTS.
- Why: This keeps compute and setup effort focused on the speakers that matter most.

## Scope

In scope:

- Route speakers between clone and non-clone backends inside the same item.
- Persist routing rules in templates/cast packs.
- Keep mixed output compatible with the same mixing/export pipeline.

Out of scope:

- Fully automatic speaker importance ranking without operator review.
- Provider-specific cloud fallback logic.

## Acceptance criteria

- Operators can mix cloned and non-cloned speakers in one dub.
- Routing survives template reuse and batch application.
- Jobs remain debuggable and outputs clearly label which path was used.

## Test / verification plan

- Engine routing tests.
- UI build.
- Manual smoke with one major speaker and several minor speakers.

## Status updates

- 2026-03-06: Created as a pragmatic scale/cost-quality compromise for multi-speaker material.
