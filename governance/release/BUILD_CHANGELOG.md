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

> **Retrospective commit attribution (v0.1.27..v0.1.51):** these 23 builds were produced from the working tree between commits `521447b` (prior ship) and `fc5027e` (next ship). No per-build commit was taken at the time. All v0.1.27..v0.1.51 rows below cite `fc5027e` because that is the captured tree state; earlier builds in the series correspond to subsets of that tree.

## 0.1.27 - 2026-05-19T00:52:19Z
- Work Packets: `WP-0230`, `WP-0231`, `WP-0232`, `WP-0233`, `WP-0234`, `WP-0239`
- Commit: `fc5027e`
- Offline Bundle ID: `offline_full_win64_20260519_001043`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.27_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.27_x64_en-US.msi`
- Notes: v0.1.27: voice-pack reliability sweep (WP-0231 Kokoro hf_hub pin + recovery; WP-0232 hashed lockfiles + --require-hashes install path with include_str bundling fix; WP-0233 pack warmup gate binary + wrapper + build hook; WP-0234 install-state journal + auto force-reinstall; WP-0230 progress UI truthfulness). Offline payload adopted from a successful out-of-band direct prep run (the in-script cargo-run path was being killed early; payload itself built from current code at 2026-05-19 02:10).

## 0.1.28 - 2026-05-20T16:17:36Z
- Work Packets: `WP-0241`
- Commit: `fc5027e`
- Offline Bundle ID: `offline_full_win64_20260519_001043`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.28_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.28_x64_en-US.msi`
- Notes: v0.1.28 emergency freeze containment for WP-0241: stop App from pre-mounting hidden Diagnostics after startup and gate Diagnostics initial probe fan-out behind visible=true. Reuses the verified v0.1.27 offline payload; no payload refresh. Pack warmup gate intentionally skipped for this UI-only emergency build because the gate invokes the Python pack probes implicated in the operator freeze report.

## 0.1.29 - 2026-05-20T21:46:02Z
- Work Packets: `WP-0243`
- Commit: `fc5027e`
- Offline Bundle ID: `offline_full_win64_20260519_001043`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.29_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.29_x64_en-US.msi`
- Notes: v0.1.29 emergency freeze containment for WP-0243: gate Instagram subscription heartbeat to the Instagram page, split Jobs active polling into a lightweight job snapshot, bound Jobs context hydration, shorten read-only SQLite busy waits, and emit database_locked/database_busy trace rows.

## 0.1.30 - 2026-05-20T23:13:59Z
- Work Packets: `WP-0244`
- Commit: `fc5027e`
- Offline Bundle ID: `offline_full_win64_20260519_001043`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.30_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.30_x64_en-US.msi`
- Notes: WP-0244 moves Video Archiver startup list/status commands to read-only DB paths, makes archive stats non-invasive, bootstraps default video library during startup, and adds missing command timing/DB lock traces.

## 0.1.31 - 2026-05-20T23:57:44Z
- Work Packets: `WP-0244`
- Commit: `fc5027e`
- Offline Bundle ID: `offline_full_win64_20260519_001043`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.31_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.31_x64_en-US.msi`
- Notes: WP-0244 v0.1.31 adds Video Archiver base-refresh deferral so archive stats and active refresh id probes no longer block first paint; keeps read-only DB/startup command containment from v0.1.30.

## 0.1.33 - 2026-05-21T01:22:55Z
- Work Packets: `WP-0244`
- Commit: `fc5027e`
- Offline Bundle ID: `offline_full_win64_20260519_001043`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.33_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.33_x64_en-US.msi`
- Notes: WP-0244 final build removes archive stats from SQLite entirely, keeps Video Archiver deferred badge/status probes, and preserves read-only startup list commands to reduce DB lock/freezing during app startup.

## 0.1.34 - 2026-05-21T02:18:36Z
- Work Packets: `WP-0244`
- Commit: `fc5027e`
- Offline Bundle ID: `offline_full_win64_20260519_001043`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.34_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.34_x64_en-US.msi`
- Notes: WP-0244 final build moves Video Archiver visible SQLite list commands off the synchronous Tauri command lane, removes archive stats from SQLite entirely, and keeps deferred badge/status probes.

## 0.1.35 - 2026-05-21T18:53:54Z
- Work Packets: `WP-0235`
- Commit: `fc5027e`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.35_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.35_x64_en-US.msi`
- Notes: WP-0235 Localization Studio setup/repair entry now queues the durable Phase 2 voice-pack install job from the page, avoids foreground setup confirmation dialogs, shows checking/queued states, and keeps Jobs/Queue as the progress and recovery surface. Runtime voice-cloning proof for Haerin single-speaker and Queen multi-speaker samples is captured in WP-0235 proof folders; this build reuses the newly validated offline payload offline_full_win64_20260521_181537.

## 0.1.36 - 2026-05-21T22:43:07Z
- Work Packets: `WP-0235`
- Commit: `fc5027e`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.36_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.36_x64_en-US.msi`
- Notes: WP-0235 runtime hardening: stale voice-pack install receipts no longer block runnable local voice cloning, the agent bridge handles visual-debug waits without starving health/state probes, and vvwatch flags stale freeze reports.

## 0.1.37 - 2026-05-21T23:31:50Z
- Work Packets: `WP-0235`
- Commit: `fc5027e`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.37_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.37_x64_en-US.msi`
- Notes: WP-0235 runtime hardening v0.1.37: Localization readiness loads for visible headless/background pages, transient DB startup locks retry without a permanent red banner, stale voice-pack install receipts no longer block runnable local voice cloning, the agent bridge handles visual-debug waits without starving health/state probes, and vvwatch flags stale freeze reports.

## 0.1.38 - 2026-05-22T00:21:01Z
- Work Packets: `WP-0235`
- Commit: `fc5027e`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.38_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.38_x64_en-US.msi`
- Notes: WP-0235 runtime hardening v0.1.38: startup skips the expensive offline payload walk when the localization and voice-preserving runtime is already ready, preventing the shell from staying at 75% loading on a stale bundle marker while preserving the runtime-ready gate.

## 0.1.40 - 2026-05-22T02:15:02Z
- Work Packets: `WP-0235`, `WP-0242`
- Commit: `fc5027e`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.40_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.40_x64_en-US.msi`
- Notes: WP-0235/WP-0242 current Localization setup and watcher evidence build: vvwatch flags stale installed builds and separates stale app-data requirements from current bundled lockfile failures; current build is needed for installed-app bridge proof.

## 0.1.41 - 2026-05-22T03:20:09Z
- Work Packets: `WP-0235`, `WP-0242`
- Commit: `fc5027e`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.41_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.41_x64_en-US.msi`
- Notes: WP-0235/WP-0242 startup and watcher hardening build: diarization status no longer instantiates Resemblyzer VoiceEncoder during startup/status checks, and vvwatch no longer treats stale freeze-report app_version as the live app version.

## 0.1.42 - 2026-05-22T04:09:01Z
- Work Packets: `WP-0235`, `WP-0242`
- Commit: `fc5027e`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.42_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.42_x64_en-US.msi`
- Notes: WP-0235/WP-0242 final startup/status hardening build: diarization status is now metadata/lockfile-only during startup and no longer runs VoiceEncoder or importlib find_spec probes; full runtime validation remains in install/repair and actual diarization execution.

## 0.1.43 - 2026-05-22T04:58:48Z
- Work Packets: `WP-0235`, `WP-0242`
- Commit: `fc5027e`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.43_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.43_x64_en-US.msi`
- Notes: WP-0235/WP-0242 final startup-gate hardening build: diarization installed status now reflects metadata presence so startup does not force offline hydration for lockfile receipt drift; lockfile drift remains visible as Diagnostics repair guidance.

## 0.1.44 - 2026-05-22T06:44:47Z
- Work Packets: `WP-0235`, `WP-0242`
- Commit: `fc5027e`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.44_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.44_x64_en-US.msi`
- Notes: WP-0235/WP-0242 follow-up: Localization home no longer renders stale failed dub progress as active work and labels historic failures as retry-needed; vvwatch now handles SQLite WAL/SHM files disappearing during probes.

## 0.1.45 - 2026-05-22T07:27:19Z
- Work Packets: `WP-0235`, `WP-0242`
- Commit: `fc5027e`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.45_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.45_x64_en-US.msi`
- Notes: WP-0235/WP-0242 follow-up: Localization home now hydrates recent item status through a batched output/status command and reuses already-read recent job rows instead of launching a second jobs_list_for_item query per item.

## 0.1.46 - 2026-05-22T08:06:48Z
- Work Packets: `WP-0235`, `WP-0242`
- Commit: `fc5027e`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.46_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.46_x64_en-US.msi`
- Notes: WP-0235/WP-0242 follow-up: Localization home status hydration now uses one bounded read-only DB pass for recent jobs/tracks and builds lightweight home outputs without looping through the full item_outputs path per recent item.

## 0.1.47 - 2026-05-22T09:56:45Z
- Work Packets: `WP-0171`, `WP-0209`, `WP-0235`, `WP-0242`
- Commit: `fc5027e`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.47_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.47_x64_en-US.msi`
- Notes: WP-0171/WP-0209/WP-0235/WP-0242 follow-up: Visual debugger snapshots now capture the bounded app shell viewport with html2canvas time and image-load limits instead of directly rendering document.body, improving headless visual proof reliability under load.

## 0.1.48 - 2026-05-22T11:49:23Z
- Work Packets: `WP-0171`, `WP-0209`, `WP-0235`, `WP-0242`
- Commit: `fc5027e`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.48_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.48_x64_en-US.msi`
- Notes: WP-0171/WP-0209/WP-0235/WP-0242 follow-up: Agent bridge snapshots try a Windows native PrintWindow capture first and keep the bounded html2canvas path as fallback, so visual proof can work even when the WebView main thread is slow or blocked.

## 0.1.49 - 2026-05-22T12:42:47Z
- Work Packets: `WP-0171`, `WP-0209`, `WP-0235`, `WP-0242`
- Commit: `fc5027e`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.49_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.49_x64_en-US.msi`
- Notes: WP-0171/WP-0209/WP-0235/WP-0242 follow-up: Agent bridge native screenshots now reject blank minimized-window captures and fall back to the bounded frontend renderer, preserving readable visual proof without keyboard or mouse automation.

## 0.1.50 - 2026-05-22T13:33:48Z
- Work Packets: `WP-0171`, `WP-0209`, `WP-0235`, `WP-0242`
- Commit: `fc5027e`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.50_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.50_x64_en-US.msi`
- Notes: WP-0171/WP-0209/WP-0235/WP-0242 follow-up: Agent bridge snapshots now reject blank native captures and retry frontend fallback emits every five seconds during the 30 second window so startup listener races do not lose the request.

## 0.1.51 - 2026-05-22T23:38:50Z
- Work Packets: `WP-0245`
- Commit: `fc5027e`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.51_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.51_x64_en-US.msi`
- Notes: WP-0245: Jobs page no longer fans out per-item `library_get`/`item_outputs` (replaced by batched `library_get_many` + `item_outputs_many` Tauri commands) — v0.1.50 freeze trace had 672+227 slow rows from this pattern. All six pack-install Tauri commands now async + `InvokeTimer`-traced so reinstalls leave forensic evidence and don't block the IPC dispatcher. Localization Studio surfaces a "Job queue is paused" banner with one-click Resume so a silently-paused queue can never again hide running work. Build script's `Get-FileSha256Hex` switched to .NET SHA256 because the 60+ min WP-0233 warmup gate subprocess corrupts `$env:PSModulePath`. Warmup gate was skipped on this build (auditable reason recorded in log); the same v0.1.51 manifest passed the gate at 2026-05-22 23:59 (all 6 packs OK).
