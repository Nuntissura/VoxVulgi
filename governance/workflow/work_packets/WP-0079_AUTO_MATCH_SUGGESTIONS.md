# Work Packet: WP-0079 - Auto-match speaker suggestions

## Metadata
- ID: WP-0079
- Owner: Codex
- Status: BACKLOG
- Created: 2026-03-06
- Target milestone: Phase 2 operator-speed improvements

## Intent

- What: Suggest likely mappings between diarized speakers and saved template speakers or cast-pack roles.
- Why: Explicit mapping should remain operator-controlled, but the app should remove repetitive manual matching work when good heuristics are available.

## Scope

In scope:

- Generate non-destructive speaker-match suggestions with confidence.
- Use local heuristics and stored voice metadata/reference summaries.
- Surface suggestions in Localization Studio for accept/reject/edit.

Out of scope:

- Unreviewed auto-apply to production jobs.
- Cloud identity matching services.

## Acceptance criteria

- Suggestions appear after diarization when saved templates/cast packs exist.
- Operators can accept, reject, or override any suggested mapping.
- The app records accepted mappings without silently applying low-confidence guesses.

## Test / verification plan

- Heuristic unit tests.
- UI build verification.
- Manual smoke on recurring-show samples with known speakers.

## Status updates

- 2026-03-06: Created as a follow-up to explicit template mapping introduced in WP-0076.
