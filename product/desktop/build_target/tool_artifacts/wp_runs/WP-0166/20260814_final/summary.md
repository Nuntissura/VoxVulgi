# WP-0166 Summary

Status: DONE
Date: 2026-08-14

## Outcome
- Replaced the packet's remaining operator-facing technical labels with plain language across Localization Studio, the subtitle editor, Diagnostics, Jobs, and Options.
- Added visible explanations for YouTube cookie import, download preset source/quality/subtitle/path fields, Image Archive linked-post and cross-site crawl settings, and custom versus inherited storage roots.
- Preserved backend behavior and data models; this packet changed labels and guidance only.

## Verification
- `npm run build` passed from `product/desktop`; TypeScript and Vite completed successfully in 2.79 seconds.
- The governed v0.1.140 desktop build passed in 22m13s using one Cargo job at BelowNormal priority. It reused verified offline bundle `offline_full_win64_20260814_082842` because no payload input changed.
- Exact fixed-string checks across `product/desktop/src` and the compiled `product/desktop/dist` returned zero occurrences for all twelve retired phrases: `ASR lang`, `reference candidates`, `voice memory profile`, `Flush cache`, `Enqueue dummy job`, `Tool lifecycle model`, `Startup hydration`, `Phase 2 packs`, `dub truth`, `Reconciliation`, `Bundled resource`, and `Bundled Deno`.
- No localization or i18n key files exist in the desktop source (`LOCALE_FILES=0`), so the localization-key acceptance item is not applicable.
- Hidden app state reported `agent_headless=true` and `app_version=0.1.140`. Semantic audits returned every candidate without truncation and zero missing accessible names on each exercised surface:
  - Options: 61/61 after opening the cookie fallback.
  - Video Archiver: 117/117 after opening Download presets.
  - Image Archive: 19/19 after opening Advanced options.
  - Localization Studio: 25/25.
  - Diagnostics: 119/119 after readiness settled.
  - Jobs: 222/222.
- Direct visual inspection confirmed readable, non-overlapping UI for the required Options, Jobs, and Diagnostics pages, plus the changed Localization Studio, Video Archiver preset, and Image Archive crawl surfaces.
- Options visibly explains the manual YouTube-cookie fallback and accepted Netscape/cookies.txt or Cookie Editor JSON formats.
- Video Archiver visibly explains source format, quality, subtitle embedding, and path/filename templates.
- Image Archive visibly explains that linked-post crawling visits individual posts and that cross-site crawling leaves the original site.
- Diagnostics visibly reports `included and ready now` for yt-dlp and its JS runtime.
- Closed the current-session hidden app through `window.__TAURI_INTERNALS__.invoke('window_close')`; PID 65716 exited and both bridge sidecars were removed.

## Evidence
- `evidence.json`
- `product/desktop/build_target/logs/build_desktop_target_20260814-160011_0_1_140.log`
- `governance/snapshots/WP-0166_build_0_1_140/options_youtube_cookie_help_1786717503291.png`
- `governance/snapshots/WP-0166_build_0_1_140/video_archiver_preset_plain_language_1786717578045.png`
- `governance/snapshots/WP-0166_build_0_1_140/image_archive_crawl_help_1786717611723.png`
- `governance/snapshots/WP-0166_build_0_1_140/localization_plain_language_controls_1786717652864.png`
- `governance/snapshots/WP-0166_build_0_1_140/diagnostics_included_ready_labels_1786717716084.png`
- `governance/snapshots/WP-0166_build_0_1_140/jobs_plain_language_1786717739334.png`

## Notes
- The generic source-level words `available` and `bundled` remain valid in internal identifiers and non-operator implementation logic; the proof checks the retired rendered phrases and exercises the affected live UI states.
