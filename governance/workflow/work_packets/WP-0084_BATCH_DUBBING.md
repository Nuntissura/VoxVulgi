# Work Packet: WP-0084 - Batch dubbing

## Metadata
- ID: WP-0084
- Owner: Codex
- Status: BACKLOG
- Created: 2026-03-06
- Target milestone: Phase 2 throughput scaling

## Intent

- What: Allow operators to apply a template or cast pack to an entire folder, playlist, season, or selected item set and queue the dubbing pipeline in batch.
- Why: Series localization is too slow if every item must be opened and queued manually.

## Scope

In scope:

- Batch apply templates/cast packs.
- Batch queue translate/dub/separate/mix/mux/export jobs.
- Progress and failure reporting at the batch level.

Out of scope:

- Fully autonomous unattended resolution of ambiguous speakers.
- Cloud batch orchestration.

## Acceptance criteria

- Operators can pick many items and queue a repeatable dubbing batch.
- Batch state is visible in Jobs/Queue and recoverable after restart.
- Per-item failures do not destroy successful outputs for other items.

## Test / verification plan

- Batch job engine tests.
- UI build.
- Manual smoke on a multi-item sample folder.

## Status updates

- 2026-03-06: Created to extend template reuse from single-item workflow to season-scale throughput.
