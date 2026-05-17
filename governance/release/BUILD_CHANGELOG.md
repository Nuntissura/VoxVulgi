# VoxVulgi Build Changelog

This changelog tracks desktop installer builds produced by `governance/scripts/build_desktop_target.ps1`.

## Policy

- Every desktop target build must increment the desktop app semantic version.
- Every desktop target build must append a build entry in this file.
- Every build entry must include the Work Packet IDs included in that build.
- Build entries are append-only and listed newest last.

## Entry Template

## <version> - <UTC timestamp>
- Work Packets: `<WP-ID>`, `<WP-ID>`
- Commit: `<short-sha>`
- Offline Bundle ID: `<bundle-id>`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_<version>_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_<version>_x64_en-US.msi`
- Notes: `<freeform summary>`

## Historical Baseline

## 0.1.0 - 2026-03-02T00:00:00Z
- Work Packets: `WP-0001` .. `WP-0064`
- Commit: `a289631`
- Offline Bundle ID: `offline_full_win64_20260301_232842`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.0_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.0_x64_en-US.msi`
- Notes: Baseline build before automated build changelog/version policy enforcement.

## 0.1.2 - 2026-03-03T06:41:59Z
- Work Packets: `WP-0071`
- Commit: `47fd7a6`
- Offline Bundle ID: `offline_full_win64_20260303_061326`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.2_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.2_x64_en-US.msi`
- Notes: Installer UX clarity: explicit Update/Repair vs Full reinstall vs Uninstall wording; app-data deletion text clarified.

## 0.1.3 - 2026-03-03T19:39:46Z
- Work Packets: `WP-0072`
- Commit: `74904a5`
- Offline Bundle ID: `offline_full_win64_20260303_191450`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.3_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.3_x64_en-US.msi`
- Notes: Installer pre-maintenance explainer page before maintenance action selection (Update/Repair, Full reinstall, Uninstall).

## 0.1.4 - 2026-03-07T00:27:49Z
- Work Packets: `WP-0092`, `WP-0093`, `WP-0094`
- Commit: `06db8ea`
- Offline Bundle ID: `offline_full_win64_20260306_235943`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.4_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.4_x64_en-US.msi`
- Notes: Installer build after voice workflow remediation hardening on 2026-03-07.

## 0.1.5 - 2026-03-08T19:48:51Z
- Work Packets: `WP-0129`, `WP-0130`
- Commit: `eb54fd6`
- Offline Bundle ID: `offline_full_win64_20260308_191916`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.5_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.5_x64_en-US.msi`
- Notes: Desktop target build via build_desktop_target.ps1.

## 0.1.6 - 2026-03-11T18:02:38Z
- Work Packets: `WP-0141`
- Commit: `40e0e3c`
- Offline Bundle ID: `offline_full_win64_20260311_173920`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.6_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.6_x64_en-US.msi`
- Notes: Installer maintenance standard refresh.

## 0.1.7 - 2026-03-23T02:46:26Z
- Work Packets: `WP-0143`, `WP-0145`, `WP-0146`, `WP-0148`, `WP-0153`, `WP-0154`
- Commit: `6e9dede`
- Offline Bundle ID: `offline_full_win64_20260323_021717`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.7_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.7_x64_en-US.msi`
- Notes: Installer build for Localization first-screen recovery, shell drag/resize repair, and compact startup/recovery chrome.

## 0.1.8 - 2026-04-19T17:35:10Z
- Work Packets: `WP-0161`, `WP-0162`, `WP-0163`, `WP-0164`, `WP-0165`, `WP-0166`, `WP-0167`, `WP-0168`, `WP-0169`, `WP-0170`, `WP-0171`, `WP-0172`, `WP-0173`, `WP-0174`, `WP-0175`, `WP-0176`, `WP-0177`, `WP-0178`, `WP-0179`, `WP-0180`, `WP-0181`, `WP-0182`, `WP-0183`, `WP-0184`, `WP-0185`, `WP-0186`, `WP-0187`, `WP-0188`, `WP-0189`
- Commit: `14ee0ce`
- Offline Bundle ID: `offline_full_win64_20260419_171202`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.8_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.8_x64_en-US.msi`
- Notes: Desktop target build from current main after post-0.1.7 UX and voice-cloning tranche, plus WP-0189 offline-bundle compatibility repair.

## 0.1.9 - 2026-04-23T12:45:37Z
- Work Packets: `WP-0142`, `WP-0190`
- Commit: `14ee0ce`
- Offline Bundle ID: `offline_full_win64_20260423_122359`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.9_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.9_x64_en-US.msi`
- Notes: Desktop test build for YouTube downloader yt-dlp refresh and app readability/version badge.

## 0.1.10 - 2026-04-24T00:55:28Z
- Work Packets: `WP-0197`
- Commit: `7b8d2bc`
- Offline Bundle ID: `offline_full_win64_20260424_003520`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.10_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.10_x64_en-US.msi`
- Notes: Desktop test build for Localization workspace decoupling.

## 0.1.11 - 2026-04-25T03:38:34Z
- Work Packets: `WP-0200`, `WP-0201`, `WP-0202`, `WP-0203`, `WP-0204`
- Commit: `40566e1`
- Offline Bundle ID: `offline_full_win64_20260425_030631`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.11_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.11_x64_en-US.msi`
- Notes: Post-localization-smoke desktop target build with localization reliability and voice/backend updates.

## 0.1.12 - 2026-04-26T01:14:35Z
- Work Packets: `WP-0205`, `WP-0206`, `WP-0207`, `WP-0208`, `WP-0209`, `WP-0210`
- Commit: `f77dcd0`
- Offline Bundle ID: `offline_full_win64_20260426_003826`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.12_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.12_x64_en-US.msi`
- Notes: Localization Studio consolidation: home dashboard simplification (WP-0205), per-item clear failed runs engine+UI (WP-0206), unified Workflow Panel merging loc-workflow + loc-run (WP-0207), inline stage controls per row (WP-0208), agent /agent/dump endpoint + console buffer (WP-0209), bridge port-file PID sidecar + exit cleanup + StrictMode listener race fix (WP-0210).

## 0.1.13 - 2026-04-26T08:01:47Z
- Work Packets: `WP-0211`
- Commit: `359ce67`
- Offline Bundle ID: `offline_full_win64_20260426_003826`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.13_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.13_x64_en-US.msi`
- Notes: Localization editor master-detail layout: one panel with item-header strip + 8-stage left rail + right pane that renders only the selected stage. Per-stage actions strip at top of right pane. Card chrome stripped inside content. Deleted Workflow / First Dub Guide / Advanced Tools cards (redundant). Per operator: 'single panel, not a fan of the card system.'

## 0.1.14 - 2026-04-27T02:18:35Z
- Work Packets: `WP-0212`
- Commit: `4bcf8dd`
- Offline Bundle ID: `offline_full_win64_20260427_011703`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.14_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.14_x64_en-US.msi`
- Notes: WP-0212 Safe Mode pill placement and exit-rehydrate notice

## 0.1.15 - 2026-05-13T23:56:13Z
- Work Packets: `WP-0213`, `WP-0214`, `WP-0215`, `WP-0216`, `WP-0217`, `WP-0218`
- Commit: `07700da`
- Offline Bundle ID: `offline_full_win64_20260513_232818`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.15_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.15_x64_en-US.msi`
- Notes: Localization Studio setup-first workbench, multi-speaker controls, automatic voice reference continuation, voice setup Start flow, and headless build rules.

## 0.1.16 - 2026-05-15T02:33:46Z
- Work Packets: `WP-0195`
- Commit: `5b267de`
- Offline Bundle ID: `offline_full_win64_20260513_232818`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.16_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.16_x64_en-US.msi`
- Notes: Desktop target build via build_desktop_target.ps1.

## 0.1.17 - 2026-05-15T18:52:42Z
- Work Packets: `WP-0220`
- Commit: `fafbe71`
- Offline Bundle ID: `offline_full_win64_20260513_232818`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.17_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.17_x64_en-US.msi`
- Notes: WP-0220 multi-library video archive, Options advanced recovery move, Video Archiver tab split, and explicit browser-cookie source selection.

## 0.1.18 - 2026-05-17T11:50:51Z
- Work Packets: `WP-0221`
- Commit: `fafbe71`
- Offline Bundle ID: `offline_full_win64_20260513_232818`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.18_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.18_x64_en-US.msi`
- Notes: WP-0221 freeze diagnostic instrumentation: Worker heartbeat, process-scheduling skew detector, per-command timing on 8 suspect commands, /agent/freeze_event endpoint, and Diagnostics 'Freeze events' subsection.

## 0.1.19 - 2026-05-17T12:32:36Z
- Work Packets: `WP-0220`, `WP-0221`, `WP-0222`
- Commit: `fafbe71`
- Offline Bundle ID: `offline_full_win64_20260513_232818`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.19_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.19_x64_en-US.msi`
- Notes: WP-0221 freeze-report bundling (POST /agent/freeze_dump, vvfreeze.cmd, agent_freeze_dump_now, CLAUDE.md/AGENTS.md docs), WP-0220 single-video subfolder default fix (jobs.rs:12589 -> %(channel,uploader|misc)s), WP-0222 reveal exit-code-1 truthfulness.

## 0.1.20 - 2026-05-17T15:19:13Z
- Work Packets: `WP-0221`
- Commit: `fafbe71`
- Offline Bundle ID: `offline_full_win64_20260513_232818`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.20_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.20_x64_en-US.msi`
- Notes: WP-0221 v0.1.20 diagnostic upgrade: Worker liveness heartbeat (worker_alive every 30s), freeze threshold lowered 500ms -> 250ms, agent_handle_freeze_event whitelist updated. Use to determine whether prior absence of freeze_detected rows was due to a dead Worker or to UI-thread freezes the JS Worker cannot see.

## 0.1.21 - 2026-05-17T16:14:19Z
- Work Packets: `WP-0221`, `WP-0223`
- Commit: `fafbe71`
- Offline Bundle ID: `offline_full_win64_20260513_232818`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.21_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.21_x64_en-US.msi`
- Notes: WP-0223 SQLite contention fix (PRAGMA synchronous=NORMAL + list_youtube_subscriptions N+1 -> single GROUP_CONCAT query). WP-0221 Worker install fix (explicit new URL syntax, install-step telemetry, main-thread fallback heartbeat main_thread_alive every 30s).

## 0.1.22 - 2026-05-17T16:38:57Z
- Work Packets: `WP-0221`, `WP-0223`
- Commit: `fafbe71`
- Offline Bundle ID: `offline_full_win64_20260513_232818`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.22_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.22_x64_en-US.msi`
- Notes: v0.1.22: fix Worker bundling (revert to Vite ?worker shorthand which emits proper .js chunk; v0.1.21 used new URL(...ts) which Vite treated as asset copy and shipped unprocessed TypeScript). All other v0.1.21 content preserved: PRAGMA synchronous=NORMAL, list_youtube_subscriptions GROUP_CONCAT JOIN, install-step telemetry, main_thread_alive heartbeat.

## 0.1.23 - 2026-05-17T17:51:52Z
- Work Packets: `WP-0221`, `WP-0223`, `WP-0224`
- Commit: `fafbe71`
- Offline Bundle ID: `offline_full_win64_20260513_232818`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.23_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.23_x64_en-US.msi`
- Notes: WP-0224 v0.1.23 freeze fix: agent bridge CORS unblock (Access-Control-Allow-Origin + OPTIONS preflight) so freeze-detector Worker can actually post; db::open_readonly bypasses job-runner write queue for the 5 UI list functions (library_list, list_localization_workspace_items, instagram_subscriptions_list, youtube_subscriptions_list, youtube_subscription_groups_list).

## 0.1.24 - 2026-05-17T20:05:38Z
- Work Packets: `WP-0221`, `WP-0223`, `WP-0224`, `WP-0226`
- Commit: `fafbe71`
- Offline Bundle ID: `offline_full_win64_20260513_232818`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.24_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.24_x64_en-US.msi`
- Notes: WP-0226 v0.1.24 comprehensive read-only UI sweep: 10 engine pure-read functions now use db::open_readonly (jobs::list_jobs, jobs::get_job, jobs::list_jobs_for_item, jobs::get_queue_control, jobs::get_runtime_settings, library::get_item_by_id, subtitle_tracks::list_tracks, subtitle_tracks::get_track, video_libraries::get_video_library_by_id) bypassing the job-runner write queue. InvokeTimer added to library_get, jobs_list, jobs_list_for_item, jobs_queue_control_get, jobs_runtime_settings_get so any residual slow Jobs path is visible in the next freeze trace.

## 0.1.25 - 2026-05-17T20:49:11Z
- Work Packets: `WP-0221`, `WP-0223`, `WP-0224`, `WP-0226`, `WP-0227`
- Commit: `fafbe71`
- Offline Bundle ID: `offline_full_win64_20260513_232818`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.25_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.25_x64_en-US.msi`
- Notes: WP-0227 v0.1.25 voice pack auto-install + resume: Phase2 install job handler now reads prior latest.json and carries forward done steps so resumed installs don't redo 5-15 minutes of work; startup-phase background thread auto-enqueues install when packs aren't fully done (Safe Mode suppresses). Trace rows phase2_auto_install_enqueue / _failed. Bundles WP-0226 read-only sweep already in source.

## 0.1.26 - 2026-05-17T21:55:54Z
- Work Packets: `WP-0221`, `WP-0223`, `WP-0224`, `WP-0226`, `WP-0227`, `WP-0228`
- Commit: `fafbe71`
- Offline Bundle ID: `offline_full_win64_20260513_232818`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.26_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.26_x64_en-US.msi`
- Notes: WP-0228 EMERGENCY ROLLBACK of WP-0227 auto-install: operator reported v0.1.25 became unusable due to background install contention. Auto-enqueue thread removed from lib.rs. Resume logic in jobs.rs kept (safe; helps manual installs). v0.1.26 = v0.1.24 + Phase2 install resume logic + freeze diagnostic infrastructure.
