# Work Packet: WP-0161 - Video Archiver progress UX vs 4K Downloader

## Metadata
- ID: WP-0161
- Owner: Codex
- Status: REVIEW
- Created: 2026-04-08
- Target milestone: UX Parity

## Intent

- What: Expand the `youtube_subscriptions` UI and schema to track and surface granular progress (total videos detected vs. items downloaded) and cleanly separate channels vs. shorts, matching the legacy 4KVDP UX.
- Why: Operators need transparency into what the job engine is actively doing, how large a playlist is, and whether the downloaded subset actually matches the channel's live size.

## Scope

In scope:
- Schema expansion to include `total_items_detected`, `items_downloaded`, and `last_sync_status`.
- Update the Rust engine's `refresh_youtube_subscription` logic to parse total items from yt-dlp.
- Video Archiver UI work to show fractional progress and current download state.
- Separation of routing for Channels vs. Shorts.

## Acceptance criteria
- Operators can see fractional downloaded progress on subscriptions (e.g., 50/100).
- Shorts and Videos are represented naturally in the UI with distinct parsing logic.

## Test / verification plan
- Engine tests validating yt-dlp parsing for total detected item counts.
- UI snapshot/manual validation of the subscription cards demonstrating the new progress states.

## Status updates

- 2026-04-08: Created from operator evaluation session.
- 2026-04-08: Implemented phase 1 — archive stats, type labels, and active job indicators.
  - Engine: `youtube_subscriptions_archive_stats()` in `subscriptions.rs` returns downloaded count per subscription.
  - Engine: `active_youtube_subscription_refresh_ids()` in `jobs.rs` returns subscription IDs with running/queued refresh jobs.
  - Tauri: `youtube_subscriptions_archive_stats` and `youtube_subscriptions_active_refresh_ids` commands registered.
  - Frontend: Three new columns in subscription table — **Type** (Channel/Shorts/Playlist/URL), **Downloaded** (archive count), **Status** (Downloading…/Idle).
  - Note: "detected" count (total items on remote) deferred — requires yt-dlp expand call which is not cheap. Only downloaded count shown for now.
  - Verified: `cargo check` (0 errors, pre-existing warnings only) + `npm run build` (exit 0).
