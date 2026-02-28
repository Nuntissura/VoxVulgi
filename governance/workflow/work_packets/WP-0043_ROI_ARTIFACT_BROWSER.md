# Work Packet: WP-0043 - ROI-16: Derived output browser (artifacts timeline)

## Metadata
- ID: WP-0043
- Owner: Codex
- Status: DONE
- Created: 2026-02-22
- Target milestone: Phase 2 (voice-preserving dubbing)

## Intent

- What: Provide a per-item derived output browser showing job artifacts over time with actions like reveal file, open log, and rerun job.
- Why: Users need visibility into what the pipeline produced, and fast iteration without hunting folders.

## Scope

In scope:

- Desktop:
  - Add an "Artifacts" section for an item:
    - list known derived outputs (stems, diarization, TTS manifests, dub preview audio/video),
    - show job timestamps/status,
    - actions: reveal file, open log, rerun job.
- Engine/Tauri:
  - Provide commands to list derived outputs safely and to open/reveal paths.

Out of scope:

- Full DAG visualization.
- Telemetry.
- Consent gates or anti-abuse controls.

## Acceptance criteria

- Derived outputs for an item can be browsed in-app.
- Users can reveal/open artifacts and rerun their generating jobs.

## Test / verification plan

- Run multiple jobs on an item and verify artifact listing and actions.

## Risks / open questions

- Need a stable mapping from job types to expected artifacts.

## Status updates

- 2026-02-22: Created.
- 2026-02-23: Implemented per-item artifacts browser (known outputs list) with play/reveal/open/rerun and job log access in Subtitle Editor; verified via build + tests.
