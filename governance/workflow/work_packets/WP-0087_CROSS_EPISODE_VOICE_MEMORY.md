# Work Packet: WP-0087 - Cross-episode voice memory

## Metadata
- ID: WP-0087
- Owner: Codex
- Status: BACKLOG
- Created: 2026-03-06
- Target milestone: Phase 3 consistency systems

## Intent

- What: Remember and refine speaker-specific voice settings across episodes so the same recurring speaker stays consistent over time.
- Why: Long-running series need a memory layer stronger than one-off template application.

## Scope

In scope:

- Persist cross-episode speaker memory records.
- Surface suggested carry-forward settings when a recurring speaker returns.
- Keep operator override as the source of truth.

Out of scope:

- Unattended speaker identity enforcement.
- External biometric identity services.

## Acceptance criteria

- Operators can reuse and refine a recurring speaker memory profile.
- The app can suggest prior settings on later items without destructive auto-apply.
- Voice memory can be reset or forked when needed.

## Test / verification plan

- Persistence tests.
- Suggestion regression tests.
- Manual smoke across a multi-episode sample set.

## Status updates

- 2026-03-06: Created as the long-horizon consistency layer above cast packs and templates.
