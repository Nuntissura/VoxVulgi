# Work Packet: WP-0036 - ROI-09: Subtitle-to-dub QC report

## Metadata
- ID: WP-0036
- Owner: Codex
- Status: DONE
- Created: 2026-02-22
- Target milestone: Phase 2 (voice-preserving dubbing)

## Intent

- What: Generate a QC report that flags subtitle and dub issues (CPS, line length, overlaps, untranslated segments, timing mismatches).
- Why: Make problems visible before exporting a dub, and reduce manual review time.

## Scope

In scope:

- Engine:
  - Compute QC metrics for a selected subtitle track:
    - CPS and line length thresholds,
    - overlaps/gaps,
    - untranslated segments (empty/placeholder),
    - timing mismatches vs dub audio duration (when available).
  - Output a machine-readable report (JSON) into derived outputs.
- Desktop:
  - Add "Generate QC report" action and UI to view results.
  - Provide navigation from a QC issue to the segment in the subtitle editor.

Out of scope:

- Automated rewriting of translations.
- Telemetry.
- Consent gates or anti-abuse controls.

## Acceptance criteria

- A QC report can be generated for a track and is viewable in the UI.
- Issues are grouped and actionable (jump-to-segment).

## Test / verification plan

- Create a track with known issues and verify the report detects them and the UI navigation works.

## Risks / open questions

- Decide thresholds (CPS, line length) and make them configurable with sane defaults.

## Status updates

- 2026-02-22: Created.
- 2026-02-23: Implemented QC report generation (JSON) and Subtitle Editor UI to view issues and jump-to-segment; verified via build + tests.
