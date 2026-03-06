# Work Packet: WP-0082 - Emotion and prosody controls

## Metadata
- ID: WP-0082
- Owner: Codex
- Status: BACKLOG
- Created: 2026-03-06
- Target milestone: Phase 2 dubbing naturalness controls

## Intent

- What: Add per-speaker emotion/prosody controls such as slower, warmer, more excited, less robotic, and tighter timing fit.
- Why: Operators need a predictable way to shape English delivery for teaching use without breaking timing or voice identity.

## Scope

In scope:

- Add operator-facing prosody controls at speaker/template/item level.
- Persist reusable defaults.
- Feed timing-fit and backend-supported expressive controls from one unified settings surface.

Out of scope:

- Free-form prompt chaining for each backend.
- Emotion inference from copyrighted original performances.

## Acceptance criteria

- Operators can adjust prosody without editing each line manually.
- Controls integrate with template reuse and batch dubbing.
- Unsupported backends fail soft instead of crashing jobs.

## Test / verification plan

- Settings serialization tests.
- Job parameter regression tests.
- Manual smoke on timing-sensitive segments.

## Status updates

- 2026-03-06: Created as the structured control layer above raw timing-fit and voice identity.
