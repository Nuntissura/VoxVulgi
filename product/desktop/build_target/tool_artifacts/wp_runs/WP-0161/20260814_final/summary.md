# WP-0161 Summary

Status: DONE
Date: 2026-08-14

## Outcome
- The Video Archiver subscription manager exposes fractional downloaded/upstream totals when the last refresh has a canonical upstream count.
- YouTube Shorts, channel Videos, and Playlists are distinct source types in both canonical membership routing and operator-facing rows.
- The phase-1 deferred total was delivered by the preserved WP-0255 implementation and is now proven on the current governed artifact.

## Verification
- Governed desktop v0.1.139 passed its frontend and release build gates.
- Hidden app state reported `agent_headless=true`, `app_version=0.1.139`, and `current_page=video_ingest`.
- Full semantic audit returned 102/102 candidates, `truncated=false`, and zero missing accessible names.
- The live canonical subscription list reported 303 subscriptions and exposed separate rows such as `hearts2hearts.official - Shorts` (`Shorts`) and `hearts2hearts.official - Videos` (`Channel`).
- Live fractional rows included `FancamBot - Shorts 0 / 829`, `@CLUCK_CLUCK_KPOP Videos 0 / 139`, `@CLUCK_CLUCK_KPOP Shorts 0 / 123`, and Playlist rows with their own totals.
- Directly inspected the list screenshot and fractional Shorts screenshot. Labels, state pills, progress values, and progress bars are readable without overlap.
- Inspected engine routing: `/shorts` -> `shorts_page`, `/videos` -> `videos_page`, playlists -> `playlist`, and other channel sources -> `channel_page`. The refresh records `entries.len()` as `upstream_total` and persists it with new/queued counts.
- Focused structured yt-dlp enumeration regression passed: 1 passed, 0 failed in 2.36 seconds.
- Closed the current-session headless app through Tauri; PID 49576 exited and both bridge sidecars were absent.

## Evidence
- `evidence.json`
- `governance/snapshots/WP-0161_build_0_1_139/subscription_progress_rows_1786715535634.png`
- `governance/snapshots/WP-0161_build_0_1_139/fractional_shorts_progress_1786715557551.png`
- `product/desktop/build_target/logs/wp_0161_flat_provider_test.stdout.log`
- `product/desktop/build_target/logs/build_desktop_target_20260814-152500_0_1_139.log`
- `product/desktop/src/pages/LibraryPage.tsx`
- `product/engine/src/jobs.rs`
- `product/engine/src/subscriptions.rs`
