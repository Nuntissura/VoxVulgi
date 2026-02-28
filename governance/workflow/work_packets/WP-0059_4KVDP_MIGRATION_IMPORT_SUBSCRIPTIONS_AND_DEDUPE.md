# Work Packet: WP-0059 - 4KVDP migration: import subscriptions + per-subscription dedupe

## Metadata
- ID: WP-0059
- Owner: Codex
- Status: DONE
- Created: 2026-02-28
- Target milestone: Phase 1 (downloader UX)

## Intent

- What: Import an existing 4K Video Downloader+ (4KVDP) subscription library into VoxVulgi so the user can stop relying on the crashing app without losing their curated lists/foldering.
- Why: 4KVDP currently crashes on this machine; the user’s subscriptions and “already downloaded” history must be preserved to avoid re-downloading everything.

## Scope

In scope:

- Engine: import from a 4KVDP export directory containing:
  - `subscriptions.json` (from our audit export)
  - optional `subscription_entries.csv` (to seed “already downloaded” history)
- Map each 4KVDP subscription to a VoxVulgi `youtube_subscription` row:
  - `source_url` from 4KVDP
  - `title` from 4KVDP metadata when available (fallback to folder name)
  - per-subscription output folder preservation:
    - default: set `output_dir_override` to 4KVDP `dirname` (so new downloads land in the existing folder structure)
    - derive `folder_map` from the final path segment (for display / fallback mapping)
- Add per-subscription dedupe/skip behavior by seeding a **yt-dlp download archive** from `subscription_entries.csv`.
- Add a subscription refresh flow that:
  - expands a channel/playlist URL to recent video URLs,
  - skips already-archived entries,
  - enqueues per-video download jobs (so each job imports exactly one file),
  - updates the archive on successful downloads.
- Desktop UI: add an “Import 4KVDP exports…” action (directory picker) that calls the new engine import.

Out of scope:

- Importing 4KVDP’s existing downloaded *files* into VoxVulgi’s Library DB (separate WP; VoxVulgi already supports local file import).
- Copying 4KVDP thumbnails into the VoxVulgi DB (we intentionally avoid storing giant BLOB thumbnails).
- Instagram subscription migration (4KVDP export format + semantics differ; treat separately).

## Acceptance criteria

- Importing a directory with `subscriptions.json` adds subscriptions into VoxVulgi and preserves output paths/folder intent.
- If `subscription_entries.csv` is present, future subscription refresh runs do **not** re-download previously downloaded videos.
- Queueing a subscription results in per-video jobs placed in the correct folder(s), without blocking the UI thread.
- Engine tests cover:
  - 4KVDP import mapping,
  - archive seeding behavior.

## Test / verification plan

- `cargo test` in `product/engine`
- `cargo test` in `product/desktop/src-tauri`
- `npm -C product/desktop run build`
- Manual smoke:
  - import 4KVDP exports directory,
  - verify subscriptions appear,
  - queue one subscription and verify only new items are enqueued (based on seeded history),
  - verify output path matches the subscription override directory.

## Status updates

- 2026-02-28: Created; implementation in progress.
- 2026-02-28: Implemented import + archive seeding + refresh job flow; verified via `cargo test` (engine), `cargo test` (src-tauri), and `npm run build` (desktop).
