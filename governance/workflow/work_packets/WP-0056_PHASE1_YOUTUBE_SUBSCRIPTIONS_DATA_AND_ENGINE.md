# Work Packet: WP-0056 - Phase 1: YouTube subscriptions data model + engine flow

## Metadata
- ID: WP-0056
- Owner: Codex
- Status: DONE
- Created: 2026-02-25
- Target milestone: Phase 1 (downloader UX)

## Intent

- What: Add persistent YouTube subscription entities with per-subscription folder mapping and queueing hooks in the engine.
- Why: URL batches are one-shot; users need durable subscriptions that can be re-queued safely without rebuilding inputs every session.

## Scope

In scope:

- Extend SQLite schema with `youtube_subscription`.
- Add engine APIs to:
  - create/update/delete/list subscriptions,
  - queue one subscription or all active subscriptions,
  - apply mapped output location per subscription.
- Keep output mapping deterministic:
  - default mapped path under downloads/video/subscriptions/<folder_map>,
  - optional absolute output override.
- Add engine JSON export/import helpers for subscriptions (URL-keyed upsert).

Out of scope:

- Background scheduler/cron.
- Subscription-level dedupe history and skip logic.
- Automatic deletion of existing subscriptions during import.

## Acceptance criteria

- Subscriptions persist across app restarts and page remounts.
- Queueing all active subscriptions produces grouped jobs via `batch_id`.
- Folder mapping is applied per subscription.
- Export file is valid JSON and import merges by `source_url`.

## Test / verification plan

- Engine unit test coverage for schema migration and import upsert behavior.
- `cargo test` in `product/engine`.

## Status updates

- 2026-02-25: Created.
- 2026-02-25: Implemented SQLite `youtube_subscription` schema + engine CRUD/queue/export/import APIs, including per-subscription folder mapping and URL-keyed import upsert behavior.
- 2026-02-25: Verified with `cargo test` in `product/engine` (50 passed).
