# Work Packet: WP-0033 - ROI-06: Speaker label management UI

## Metadata
- ID: WP-0033
- Owner: Codex
- Status: DONE
- Created: 2026-02-22
- Target milestone: Phase 2 (voice-preserving dubbing)

## Intent

- What: Provide UI tools to manage speaker labels: rename speakers, merge/split speakers, and propagate labels across tracks.
- Why: Dubbing quality depends on consistent speaker identity and per-speaker voice mapping.

## Scope

In scope:

- Desktop (Subtitle editor):
  - Speaker manager:
    - rename speaker labels,
    - merge speakers (remap segments),
    - split speakers (assign selected segments to a new speaker label),
    - propagate labels across subtitle tracks within the same item (best-effort).
- Engine/data:
  - Define a stable per-item speaker registry (names + IDs) so future VC/TTS mapping can reference it.

Out of scope:

- Automatic speaker identity matching across different library items.
- Telemetry.
- Consent gates or anti-abuse controls.

## Acceptance criteria

- Subtitle editor can rename/merge/split speakers and persists changes.
- Existing segments update speaker labels deterministically (no orphan labels).
- A per-item speaker registry exists and can be referenced by other jobs.

## Test / verification plan

- Run diarization on an item to create speakers, then rename/merge/split and verify persistence.

## Risks / open questions

- Track JSON may need a schema version bump to add stable speaker IDs.

## Status updates

- 2026-02-22: Created.
- 2026-02-23: Added Subtitle Editor speaker tools: per-speaker display names, bulk assign (split), merge, and best-effort propagation to other tracks; speaker registry persists via per-item speaker settings; verified via build + tests.
