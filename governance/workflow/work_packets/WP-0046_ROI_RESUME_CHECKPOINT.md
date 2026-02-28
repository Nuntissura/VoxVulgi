# Work Packet: WP-0046 - ROI-19: Crash-safe resumable external steps (Python/FFmpeg checkpointing)

## Metadata
- ID: WP-0046
- Owner: Codex
- Status: DONE
- Created: 2026-02-22
- Target milestone: Phase 2 (voice-preserving dubbing)

## Intent

- What: Make long-running external steps (Python/FFmpeg) crash-safe and resumable with checkpointing.
- Why: Large packs and long processing pipelines should not force a full restart after an interruption.

## Scope

In scope:

- Engine:
  - Add job step checkpointing:
    - step-level state persisted to derived outputs,
    - idempotent step behavior where possible (skip if output exists and matches expected shape),
    - clear resume behavior on re-run.
  - Improve external process logs so a resume can be debugged.

Out of scope:

- Distributed processing.
- Telemetry.
- Consent gates or anti-abuse controls.

## Acceptance criteria

- If a job is interrupted mid-way, re-running the job resumes from the last completed step (best-effort).
- Logs clearly indicate resumed steps vs newly executed steps.

## Test / verification plan

- Start a long job, kill it mid-way, then rerun and verify it resumes without repeating completed steps.

## Risks / open questions

- Some steps cannot be safely resumed unless outputs are validated (ties to ROI-15 integrity).

## Status updates

- 2026-02-22: Created.
- 2026-02-23: Added best-effort resumable behavior across long external steps by skipping completed outputs and emitting resume logs; verified via build + tests.
