# Work Packet: WP-0009 — Phase 1: Jobs engine + queue UI

## Metadata
- ID: WP-0009
- Owner: Codex
- Status: DONE
- Created: 2026-02-19
- Target milestone: Phase 1 (MVP)

## Intent

- What: Implement durable background jobs and a queue UI.
- Why: All AI/media workflows must be non-blocking and resumable.

## Scope

In scope:

- Implement:
  - job table schema (if not already created by WP-0008)
  - job runner with concurrency limits
  - structured per-job logs + artifact directory
  - queue UI (running/failed/completed; retry/cancel)

Out of scope:

- ASR/translation job implementations (tracked in WP-0010/WP-0012).

## Acceptance criteria

- Jobs persist across app restart.
- UI shows progress and final status.
- Per-job logs are available and rotated according to policy direction (WP-0005).

## Implementation notes

- Keep job execution in the Rust engine boundary, not the UI thread.

## Test / verification plan

- Enqueue a dummy job that takes ~10 seconds and updates progress; verify persistence and retry.

## Risks / open questions

- Cancellation semantics for FFmpeg/model subprocesses (needs careful cleanup).

## Status updates

- 2026-02-19:
  - Implemented durable job runner (concurrency-limited), queue UI, retry/cancel, and per-job artifacts/logs.
  - Confirmed restart behavior: orphaned `running` jobs are re-queued on startup.
  - Implemented per-job log rotation + retention defaults:
    - rotate at ~50 MB with up to 3 backups per job log
    - prune job logs older than ~30 days
    - cap total job log directory size at ~1 GB (delete oldest first)
