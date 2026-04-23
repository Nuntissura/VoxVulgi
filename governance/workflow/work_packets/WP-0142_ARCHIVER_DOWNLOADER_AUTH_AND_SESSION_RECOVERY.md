# Work Packet: WP-0142 - Archiver downloader auth and session recovery

## Metadata
- ID: WP-0142
- Owner: Codex
- Status: IN_PROGRESS
- Created: 2026-03-12
- Target milestone: Post-0.1.6 smoke regression recovery

## Intent

- What: Repair the real-world YouTube and Instagram downloader path on installer state, including public downloads, explicit session inputs, and clearer tool/runtime failures.
- Why: The current smoke shows that public YouTube downloads are failing with `HTTP Error 403`, protected downloads remain broken even with explicit session material, and Instagram extraction is still failing in operator use.

## Scope

In scope:

- Diagnose and repair current bundled yt-dlp execution for public YouTube downloads.
- Ensure explicit operator session inputs win over browser-cookie fallback.
- Re-verify browser-export JSON, header-style cookies, Netscape cookie input, and cookie-file paths for both one-shot and saved-session flows.
- Repair Instagram one-shot and recurring subscription paths that currently fail even with explicit session input.
- Improve operator-facing failure messages so tool/runtime/session problems are distinguishable.

Out of scope:

- New provider integrations unrelated to YouTube or Instagram.
- Silent background browser-cookie scraping or hidden credential capture.

## Acceptance criteria

- Public YouTube downloads succeed on installer state and produce the expected MP4-compatible output path.
- Explicit session input is honored without falling back to locked browser-cookie sources unless the operator chose that path.
- Instagram one-shot and saved-session flows succeed for a currently supported account/profile scenario or fail with an actionable, truthful operator message.
- Job/Queue error text clearly distinguishes extractor failure, session failure, and missing-runtime failure.

## Test / verification plan

- Focused engine/runtime tests for cookie normalization and yt-dlp invocation seams where practical.
- Installer-state app-boundary verification for public YouTube, authenticated YouTube, Instagram one-shot, and Instagram subscription refresh.
- Proof bundle with logs, successful artifact paths where available, and documented failure-path behavior where upstream extractor limits still apply.

## Status updates

- 2026-03-12: Created from smoke findings `ST-015`, `ST-017`, `ST-018`, `ST-024`, and `ST-025`.
- 2026-03-12: First runtime-hardening slice implemented: YouTube yt-dlp runs now prefer conservative documented player clients, queue failures now append clearer locked-cookie / YouTube 403 / Instagram extractor hints, and the multi-executable failure summary now suppresses irrelevant `python3` Microsoft Store alias noise and duplicate fallback errors. Full installer-state auth and Instagram recovery remain open.
- 2026-03-13: Research-backed runtime recovery slice implemented: VoxVulgi now treats a supported JavaScript runtime as part of the yt-dlp toolchain, adds pinned bundled Deno install/status plumbing plus Diagnostics visibility, includes Deno in offline bundle prep for installer-state runs, and only forces fallback YouTube `player_client` overrides when no JS runtime is available. Direct probe evidence showed the previous forced-client strategy still failed even with Deno, while `yt-dlp + Deno + documented defaults` succeeded on a public YouTube simulate run. Instagram/session closeout remains open.
- 2026-04-23: YouTube downloader refresh slice started from recent failed-job evidence. Bundled yt-dlp pin moved from `2026.03.03` to `2026.03.17`, and offline-bundle prep now refreshes yt-dlp when the bundled copy is present but below the pinned version.
