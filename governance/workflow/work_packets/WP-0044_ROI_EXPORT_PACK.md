# Work Packet: WP-0044 - ROI-17: Export packs (zip) + provenance manifest

## Metadata
- ID: WP-0044
- Owner: Codex
- Status: DONE
- Created: 2026-02-22
- Target milestone: Phase 2 (voice-preserving dubbing)

## Intent

- What: Add a single export operation that produces a zip containing muxed video, dubbed audio, subtitles, and a provenance manifest.
- Why: Make outputs portable and auditable, and reduce "where are my files?" friction.

## Scope

In scope:

- Engine:
  - Implement an export job that collects:
    - muxed preview output (or final output when available),
    - dub audio mix,
    - subtitle tracks (source + translated),
    - a provenance manifest (JSON) describing:
      - job versions,
      - pack versions,
      - configuration parameters.
- Desktop:
  - "Export pack" action per item.

Out of scope:

- Cloud storage integrations.
- Telemetry.
- Consent gates or anti-abuse controls.

## Acceptance criteria

- Export job produces a single zip file per item containing the expected artifacts.
- Provenance manifest exists in the zip and is readable.

## Test / verification plan

- Export an item after running Phase 2 jobs and verify zip contents and manifest correctness.

## Risks / open questions

- Decide which artifacts are "required" vs "optional" based on what jobs have been run.

## Status updates

- 2026-02-22: Created.
- 2026-02-23: Added export pack job producing a single zip with previews/subtitles/stems + provenance manifest; exposed in UI with reveal action; verified via build + tests.
