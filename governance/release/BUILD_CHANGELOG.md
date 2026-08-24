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

## 0.1.52 - 2026-05-28T02:42:01Z
- Work Packets: `WP-9999`
- Commit: `76f243a`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.52_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.52_x64_en-US.msi`
- Notes: Ad-hoc operator request: rebuild installer/exe for youtube downloader troubleshooting (no prior WP requested).

## 0.1.53 - 2026-05-28T23:48:02Z
- Work Packets: `WP-9999`
- Commit: `76f243a`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.53_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.53_x64_en-US.msi`
- Notes: Desktop target build via build_desktop_target.ps1.

## 0.1.54 - 2026-05-29T19:51:53Z
- Work Packets: `WP-0246`
- Commit: `76f243a`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.54_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.54_x64_en-US.msi`
- Notes: Localization Studio hang fix + job-pipeline hardening. Root cause: Spleeter source-separation deadlocked on Windows — the default Separator() builds a multiprocessing Pool() sized to os.cpu_count(); each spawned worker re-imports TensorFlow and the pool then deadlocks (observed: job stuck at progress 0.25 for 5h+, ~33 idle python workers + a blocked ffmpeg decode pipe). Fix: Separator("spleeter:2stems", multiprocess=False). Hardening: spleeter, demucs, pyannote/diarize (x2), pyttsx3, neural TTS, and voice-preserving TTS spawns, plus the vocal-cleanup ffmpeg pass, now run under run_command_output_with_control with timeouts (3600s model jobs, 1800s spleeter/ffmpeg) so they are cancellable and cannot hang forever. New runner-thread stuck-job watchdog: WARNs in the job log after 600s with no progress change and auto-fails as a backstop after 7200s (kept above the longest command timeout so healthy long jobs are never killed).

## 0.1.55 - 2026-06-01T03:39:34Z
- Work Packets: `WP-0247`
- Commit: `76f243a`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.55_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.55_x64_en-US.msi`
- Notes: WP-0247: Options-global YouTube auth is now canonical at job execution time; YouTube archiver panes no longer expose duplicate session inputs; Jobs collapsed/direct rows show source context; YouTube single history lists all single-video candidates with fuzzy search/order; Media Library adds a single-video/legacy filter.

## 0.1.56 - 2026-06-02T03:07:44Z
- Work Packets: `WP-0248`
- Commit: `76f243a`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.56_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.56_x64_en-US.msi`
- Notes: WP-0248: Jobs search can load old batch/job rows by ID/title/URL, failed/canceled rows can be deleted individually, queued retry rows show paused-queue state, and YouTube direct jobs show cached titles.

## 0.1.57 - 2026-06-02T03:33:15Z
- Work Packets: `WP-0248`
- Commit: `76f243a`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.57_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.57_x64_en-US.msi`
- Notes: WP-0248: Jobs search can load old batch/job rows by ID/title/URL, failed/canceled rows can be deleted individually, queued retry rows show paused-queue state, and YouTube direct jobs show cached titles with wrapped target text.

## 0.1.58 - 2026-06-02T08:00:37Z
- Work Packets: `WP-0248`
- Commit: `76f243a`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.58_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.58_x64_en-US.msi`
- Notes: WP-0248 follow-up: prevent duplicate direct-url retries and cap yt-dlp subtitle languages.

## 0.1.59 - 2026-06-02T16:30:51Z
- Work Packets: `WP-0248`
- Commit: `76f243a`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.59_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.59_x64_en-US.msi`
- Notes: WP-0248 follow-up: contain rejected YouTube auth retries and add search cleanup.

## 0.1.60 - 2026-06-02T23:00:02Z
- Work Packets: `WP-0248`
- Commit: `76f243a`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.60_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.60_x64_en-US.msi`
- Notes: WP-0248 follow-up: normalize global YouTube cookie exports to YouTube-only Netscape cookies.

## 0.1.61 - 2026-06-03T03:23:51Z
- Work Packets: `WP-0248`
- Commit: `76f243a`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.61_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.61_x64_en-US.msi`
- Notes: WP-0248 follow-up: batch Retry failed now uses canonical backend batch scope instead of visible Jobs rows; retries every failed/canceled row, reuses active duplicate targets, and reports partial row errors.

## 0.1.62 - 2026-06-03T06:58:56Z
- Work Packets: `WP-0248`
- Commit: `76f243a`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.62_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.62_x64_en-US.msi`
- Notes: WP-0248 follow-up: trustworthy Jobs recovery surface with retry lineage, canonical batch health/detail, attempt history, filters/export/backfill/repair, Cookie Editor cookie.js auth input, and updated YouTube auth preflight URL. Warmup gate skipped on rerun because the immediately preceding WP-0248 attempt passed the gate (20260603_065926) and failed only while archiving a locked running desktop.exe.

## 0.1.63 - 2026-06-03T07:28:18Z
- Work Packets: `WP-0248`
- Commit: `76f243a`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.63_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.63_x64_en-US.msi`
- Notes: WP-0248 follow-up: trustworthy Jobs recovery surface with retry lineage, canonical batch health/detail, attempt history, filters/export/backfill/repair, Cookie Editor cookie.js auth input, updated YouTube auth preflight URL, and stale Jobs DB-lock banner recovery. Warmup gate skipped because the same-run gate passed at 20260603_065926 and this final rerun only changed frontend lock-banner recovery with unchanged pack inputs.

## 0.1.64 - 2026-06-03T07:55:14Z
- Work Packets: `WP-0248`
- Commit: `76f243a`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.64_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.64_x64_en-US.msi`
- Notes: WP-0248 follow-up: trustworthy Jobs recovery surface with retry lineage, canonical batch health/detail, attempt history, filters/export/backfill/repair, Cookie Editor cookie.js auth input, updated YouTube auth preflight URL, stale Jobs DB-lock banner recovery, and one bounded transient-lock refresh retry. Warmup gate skipped because the same-run gate passed at 20260603_065926 and this final rerun only changed frontend lock recovery with unchanged pack inputs.

## 0.1.65 - 2026-06-03T09:21:43Z
- Work Packets: `WP-0248`
- Commit: `76f243a`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.65_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.65_x64_en-US.msi`
- Notes: WP-0248 browser-cookie source alignment: Firefox default with Chrome and Edge supported.

## 0.1.66 - 2026-06-04T00:47:17Z
- Work Packets: `WP-0248`
- Commit: `76f243a`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.66_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.66_x64_en-US.msi`
- Notes: WP-0248 canonical batch target-health display: show videos downloaded/unresolved separately from historical attempts and skip already-downloaded targets on batch retry.

## 0.1.67 - 2026-06-10T02:33:38Z
- Work Packets: `WP-0250`
- Commit: `8d03bf6`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.67_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.67_x64_en-US.msi`
- Notes: WP-0250: disable WebView2 native window occlusion + renderer/background-tab freezing to stop idle-in-background renderer suspension freezes.

## 0.1.68 - 2026-06-15T05:20:45Z
- Work Packets: `WP-0251`, `WP-0252`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.68_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.68_x64_en-US.msi`
- Notes: Localization quality stack: WP-0251 Kokoro offline-cache honest readiness fix; WP-0252 whisper large-v3 default ASR + CosyVoice 2 voice-clone routing (proven cloning the Haerin Korean reference to English)

## 0.1.69 - 2026-06-15T07:26:46Z
- Work Packets: `WP-0251`, `WP-0252`, `WP-0253`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.69_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.69_x64_en-US.msi`
- Notes: Items 1+2: WP-0251 Kokoro offline-cache honest readiness; WP-0252 large-v3 default ASR + CosyVoice 2 voice-clone routing + install fn; WP-0253 library v16 unification migration (origin/library_id + indexes), subscription-list N+1 fix, NAS local-fallback helper, bundled+supervised external watcher

## 0.1.70 - 2026-06-15T10:51:59Z
- Work Packets: `WP-0251`, `WP-0252`, `WP-0253`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.70_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.70_x64_en-US.msi`
- Notes: Items 1+2 cont.: CosyVoice fresh-install seed (bundled 3MB repo code + first-run seed), new-download origin/library_id stamping, live NAS download fallback wiring; on top of 0.1.69 (Kokoro fix, large-v3 ASR, CosyVoice routing+install fn, library v16 unification+indexes, subscription N+1 fix, bundled+supervised watcher)

## 0.1.71 - 2026-06-15T11:18:29Z
- Work Packets: `WP-0251`, `WP-0252`, `WP-0253`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.71_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.71_x64_en-US.msi`
- Notes: WP-0253 Item 2b/2c: paginate the YouTube-history library view (was an unbounded 122k-row scan; now indexed paging like Media Library). On top of 0.1.70 (full items 1+2 stack).

## 0.1.72 - 2026-06-15T15:04:01Z
- Work Packets: `WP-0253`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.72_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.72_x64_en-US.msi`
- Notes: WP-0253 Item 2d auto-resync: when the NAS reconnects, move local-fallback downloads back onto it (copy -> verify size+sha256 -> relink DB -> delete-after-verify; never overwrites; timestamped manifest). Periodic + startup trigger + manual command.

## 0.1.73 - 2026-06-15T22:43:05Z
- Work Packets: `WP-0254`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.73_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.73_x64_en-US.msi`
- Notes: WP-0254: per-lane job scheduling (single 3 / recurring 1 / localization 1) so single one-off downloads run alongside conservative playlist/channel/subscription syncing and are no longer starved by playlist fan-out or localization; subscription-child downloads routed to the recurring lane; resume-on-restart re-queues interrupted downloads (localization fails-on-interrupt); Stop + Update-all recurring commands; 4KVDP-style startup auto-check+auto-download of due subscriptions (recurring lane, config-gated). Engine 222 tests pass. Built off the working tree that also carries in-progress WP-0250/0251/0252/0253.

## 0.1.74 - 2026-06-15T23:24:33Z
- Work Packets: `WP-0255`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.74_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.74_x64_en-US.msi`
- Notes: WP-0255 (slice 1): subscription/playlist tab no longer Advanced-gated (the blank-tab bug) so subscriptions show in any view mode; added 'Update all now' + 'Stop' buttons wired to WP-0254 recurring-lane commands; removed visible 'legacy' wording from subscription target labels/help. Builds on WP-0254 engine lanes/resume/auto-sync.

## 0.1.75 - 2026-06-16T00:05:43Z
- Work Packets: `WP-0255`, `WP-0256`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.75_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.75_x64_en-US.msi`
- Notes: WP-0256: playlist/channel/subscription child download jobs now show the real video title in Jobs (captured free from the yt-dlp --flat-playlist enumeration, stamped onto each child job's target_title; no extra yt-dlp calls). WP-0255 (slice 2): collapsed the create/rename/export/move library controls behind a 'Manage libraries' disclosure so the Video Archiver is less busy (Active-library selector stays visible). Engine 222 tests pass.

## 0.1.76 - 2026-06-16T00:35:56Z
- Work Packets: `WP-0255`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.76_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.76_x64_en-US.msi`
- Notes: WP-0255 (slice 3): grouped the rarely-used subscription import/export/migration buttons (Export/Import JSON, Import 4KVDP, Scan+seed, Import existing) behind an 'Import / export & migration' disclosure so the subscription action bar isn't a wall of buttons. Primary actions (Save, Update all, Stop, Queue due, Refresh) stay visible.

## 0.1.78 - 2026-06-17T04:30:04Z
- Work Packets: `WP-0254`, `WP-0255`, `WP-0257`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.78_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.78_x64_en-US.msi`
- Notes: WP-0257 anti-bot/auth-block cascade hardening: (1) one channel's cookie rejection no longer blocks the whole fleet - a global auth block now requires >=3 DISTINCT subscriptions to reject under the same cookie within 15min (register_youtube_auth_suspicion); (2) the auth block self-heals via a TTL with escalating backoff (5m/15m/1h/6h) and a pre-0.1.78 sticky block auto-clears on first check - so an operator's currently-stuck block resolves on this build; (3) an auth-block refresh failure no longer pins a subscription in per-sub backoff so clearing the block re-enables all subs. 3 new unit tests; engine 225 tests pass. Also carries WP-0254 freeze fix (Update-all/Queue-due enqueue in background; bounded 700ms NAS existence probe) and WP-0255 subscription-groups explainer.

## 0.1.79 - 2026-06-17T12:23:25Z
- Work Packets: `WP-0257`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.79_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.79_x64_en-US.msi`
- Notes: WP-0257 #3/#4 anti-bot burst prevention + tunable Options panel. Engine: recurring-lane inter-dispatch COOLDOWN (default 45s) so a big subscription sweep dispatches one channel at a time instead of bursting (single/localization lanes unaffected); --sleep-requests (default 1) on channel enumeration (the dominant anti-bot surface); 'Update all' now TRICKLES - caps subscriptions enqueued per click (default 250, most-overdue first) while the due/startup path stays uncapped. New Options 'Anti-bot pacing (YouTube subscriptions)' card with 3 tunable controls (refresh spacing / enumeration sleep / update-all batch) backed by antibot_pacing_get/set + clamped meta. Builds on 0.1.78's auth-block cascade containment (#1 corroboration, #2 TTL self-heal, #5 recovery). Engine 226 tests pass.

## 0.1.80 - 2026-06-17T13:24:46Z
- Work Packets: `WP-0257`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.80_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.80_x64_en-US.msi`
- Notes: WP-0257 #6: Jobs list now shows plain-language failure headlines instead of raw 'model/tool install failed: ...' text. classifyJobError maps the error string to a category + tone: 'YouTube blocked - re-authenticate in Options' (amber), 'Members-only content - skipped' / 'Channel/playlist/video no longer exists - skipped' (gray), 'YouTube rate-limited - ease the anti-bot pacing' / 'Temporary network error - retry' / 'Storage/NAS error' (blue), 'FFmpeg error' (red), 'Interrupted by restart'. Raw error preserved on hover (title) + the Copy button. Display-only - the engine still makes the authoritative auth/skip decisions. Completes the WP-0257 tranche (#1/#2/#5 containment in 0.1.78, #3/#4 pacing+Options in 0.1.79, #6 here).

## 0.1.81 - 2026-06-30T23:29:44Z
- Work Packets: `WP-0255`, `WP-0256`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.81_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.81_x64_en-US.msi`
- Notes: WP-0255 + WP-0256: subscription master-detail manager (all-subs status strip, per-sub progress X/Y + new-found, truthful last-checked via additive schema v18, refresh interval edited in hours with 12h default, clarified buttons + group-filter Update-all fix, replaced the wide 15-column horizontally-scrolling table that broke wheel scroll); Jobs/Queue source linkage (each job/batch labeled by owning subscription/playlist/channel vs Single video vs Instagram) + visual progress bars. Engine cargo test green; FE tsc clean + 67/68 contract tests (1 failure is a pre-existing unrelated dependency-install stale test in an untouched path).

## 0.1.82 - 2026-07-01T16:35:32Z
- Work Packets: `WP-0258`, `WP-0259`, `WP-0260`, `WP-0261`, `WP-0262`, `WP-0263`, `WP-0264`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.82_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.82_x64_en-US.msi`
- Notes: 0.1.82: WP-0258 jobs perf + WP-0259 de-legacy + WP-0260 non-technical UX/metallic theme + WP-0261 subscription live progress + WP-0262 localization (multi-speaker visible, subtitles-only, venv pins fixed, kokoro import 12.3s warm) + WP-0263 Instagram parity + WP-0264 failure-state telegraphing (v21 last_error_message, status-code classifier, chips in subscription panel + Jobs) + visual-tool full-page snapshot. Validated: FE tsc + engine 226 tests.

## 0.1.83 - 2026-07-01T22:01:13Z
- Work Packets: `WP-0258`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.83_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.83_x64_en-US.msi`
- Notes: 0.1.83 HOTFIX for the 0.1.82 database write-lock storm that blanked the media library + collapsed the subscription dual-pane + froze the UI. Root cause (WP-0258 2g): the job runner reopened the DB and re-ran the WAL pragma ~4x/100ms while 3 downloads wrote concurrently, starving the UI's library_list/youtube_subscriptions_list reads (4s busy-timeout) -> database is locked -> queries returned empty/errored -> blank library + single-pane. Fix: runner queue-count/fetch reads (running_count_for_lane[_and_type], fetch_queued_jobs_for_lane[_and_type]) now use db::open_readonly (no per-tick WAL-pragma write churn); RECURRING_DOWNLOAD_LIMIT 3->1. Restores library display + dual-pane. Data verified 100% intact (123,766 library_item rows, integrity_check ok, no path regression). Validated: engine 226 tests pass.

## 0.1.84 - 2026-07-01T23:53:31Z
- Work Packets: `WP-0255`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.84_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.84_x64_en-US.msi`
- Notes: 0.1.84 HOTFIX: subscription master-detail was pushed off-screen. Root cause (proven via live bridge snapshot, content_scroll_top=9943): the WP-0261 '.sub-processing' live-activity 'Checking for new videos...' list had no max-height, so with 258 subscriptions all updating it grew ~10000px tall and shoved the .sub-manager master-detail (list + detail pane) entirely off-screen -> operator could not reach/click subscriptions and saw no dual-pane. Fix (App.css .sub-processing): max-height 30vh + overflow-y auto so the activity list scrolls internally and never buries the manager. Selection/click logic confirmed already correct (cross-checked). tsc 0 errors.

## 0.1.85 - 2026-07-02T02:10:20Z
- Work Packets: `WP-0255`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.85_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.85_x64_en-US.msi`
- Notes: 0.1.85: (1) per-subscription video list in the Video Archiver detail pane - clicking a subscription now shows 'Still to download (N)' on top then 'Downloaded (N)'. New READ-ONLY command youtube_subscription_videos (db::open_readonly only, bounded LIMIT, does not touch the job runner - cannot lock the DB); bounded prefix query list_items_under_dir_bounded avoids loading all 123k rows. Additive to the detail pane, nothing removed. (2) readable 'Queue controls' text (lcd-banner high-contrast). Validated: cargo test -p voxvulgi_engine 227 passed/0 failed + FE tsc 0 errors.

## 0.1.86 - 2026-07-02T08:29:56Z
- Work Packets: `WP-0255`, `WP-0264`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260521_181537`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.86_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.86_x64_en-US.msi`
- Notes: 0.1.86: (1) 'Need attention' is now a CLICKABLE filter - click a category to see + fix only those subs; failing subs with no stored error get an actionable 'Unclassified' chip. (2) Green 'Checking' activity list has a Show/Hide toggle, HIDDEN BY DEFAULT so it no longer buries the subscription list. (3) Per-subscription 'Downloaded' videos render as REAL item cards (thumbnail + title + duration + Open/Folder), newest-first, like the Media Library (was text-only). (4) Jobs/Queue poll 6s->3s so it visibly moves (the DB-lock storm that forced 6s was fixed in 0.1.83); per-channel active-first summary surfaces what is downloading now. (5) Queue-controls text readable on the dark panel (lcd-banner: hardcoded dark helper text forced light). (6) '258 updating now' relabeled '258 queued to check - paced' - engine already paces enumeration in chunked batches, not all at once. Validated: cargo check + engine tests green, FE tsc 0.

## 0.1.87 - 2026-07-04T05:29:13Z
- Work Packets: `WP-0251`, `WP-0252`, `WP-0253`, `WP-0254`, `WP-0255`, `WP-0256`, `WP-0257`, `WP-0258`, `WP-0259`, `WP-0260`, `WP-0261`, `WP-0262`, `WP-0263`, `WP-0264`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260704_040344`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.87_x64-setup.exe`
- Notes: 0.1.87 thin NSIS installer build from current working tree. Includes subscription URL/manual-refresh/detail-list/group-unlink work, active WP-0251..WP-0264 tree changes, and dependency-lock alignment so Neural TTS and OpenVoice share huggingface_hub 0.34.4. The full offline payload offline_full_win64_20260704_040344 was validated but intentionally excluded from bundled resources because both WiX/MSI and NSIS hit installer packaging limits above roughly 2GB; packs remain repairable/downloadable after install.

## 0.1.91 - 2026-07-08T01:38:35Z
- Work Packets: `WP-0250`, `WP-0265`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260706_213832`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.91_x64-setup.exe`
- Notes: Thin NSIS installer. Offline payload (offline/**/*) excluded from bundled resources and bundle.targets set to nsis to eliminate the WiX light.exe >2GB packaging hang that stalled the 0.1.89/0.1.90 attempts (~54min then 'failed to run light.exe'). Reuses verified cached payload offline_full_win64_20260706_213832 (no dependency reinstall). Warmup gate skipped: thin installer bundles no packs and the payload was validated 2026-07-06. Contains WP-0250 WebView2 occlusion freeze fix + current working-tree engine/desktop changes.

## 0.1.92 - 2026-07-16T03:38:31Z
- Work Packets: `WP-0256`, `WP-0258`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260706_213832`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.92_x64-setup.exe`
- Notes: Single-video enqueue now persists and returns an immediate job receipt; Jobs uses a bounded canonical overview with Now, Needs attention, and History views plus indexed exact lookup and bounded history search.

## 0.1.94 - 2026-07-16T04:05:10Z
- Work Packets: `WP-0256`, `WP-0258`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260706_213832`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.94_x64-setup.exe`
- Notes: Jobs rebuilt around requested-view projections: Now, Needs attention, and History load independently; canonical totals remain global; immediate single-video receipts and indexed exact URL/video lookup remain included. Previous 0.1.92 Current output was byte-count-verified and preserved under old_versions/20260716-0604_current_0_1_92 because an active yt-dlp child retained the runtime directory as its working directory.

## 0.1.95 - 2026-07-16T04:24:30Z
- Work Packets: `WP-0256`, `WP-0258`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260706_213832`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.95_x64-setup.exe`
- Notes: Final Jobs rebuild: requested-view projections; indexed canonical status counts; collapsed rows no longer fan out batch-detail aggregation; batch detail loads on explicit expansion; immediate single-video receipts and indexed exact URL/video lookup included. Previous 0.1.94 output was byte-count-verified and preserved under old_versions/20260716-0621_current_0_1_94 because active yt-dlp children retained the runtime directory as their working directory.

## 0.1.96 - 2026-07-16T04:31:31Z
- Work Packets: `WP-0256`, `WP-0258`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260706_213832`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.96_x64-setup.exe`
- Notes: Accepted Jobs rebuild candidate: requested-view projections; indexed canonical status counts; no collapsed-row batch-detail fan-out; explicit Loading states replace false zero counts; batch detail loads only on explicit expansion; immediate single-video receipts and indexed exact URL/video lookup included. Previous 0.1.95 output was byte-count-verified and preserved under old_versions/20260716-0629_current_0_1_95 because active yt-dlp children retained the runtime directory as their working directory.

## 0.1.97 - 2026-07-16T17:24:10Z
- Work Packets: `WP-0257`, `WP-0264`, `WP-0266`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260706_213832`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.97_x64-setup.exe`
- Notes: YouTube browser-session Connect and test; explicit failed-status reasons; auth circuit that holds rejected YouTube work without canceling jobs; conservative recurring playlist/subscription pacing and single-fragment recurring downloads.

## 0.1.98 - 2026-07-16T18:01:56Z
- Work Packets: `WP-0257`, `WP-0264`, `WP-0266`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260706_213832`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.98_x64-setup.exe`
- Notes: Corrected YouTube browser-session release candidate: successful global browser connection supersedes stale per-job cookies on old queued work; includes explicit failed-status reasons, auth circuit holding, and conservative recurring pacing.

## 0.1.99 - 2026-07-16T18:23:38Z
- Work Packets: `WP-0257`, `WP-0264`, `WP-0266`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260706_213832`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.99_x64-setup.exe`
- Notes: Final YouTube auth and Jobs telegraphing build: browser session overrides stale queued-job cookies; failed rows classify sign-in, stalled watchdog, missing-output, storage, tool, and network causes; conservative recurring pacing and auth circuit included.

## 0.1.100 - 2026-07-16T20:33:23Z
- Work Packets: `WP-0267`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260706_213832`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.100_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.100_x64_en-US.msi`
- Notes: YouTube sign-in now opens in the selected normal browser, guides new users through sign-in and exact-source verification, persists Ready versus Sign-in-required truth, marks corroborated runtime rejection as reconnect-required, and shows exact recovery steps. Manual YouTube-only cookies remain an advanced save-and-test fallback. Reused the verified offline payload; warmup gate skipped because dependency inputs did not change.

## 0.1.101 - 2026-07-16T20:56:49Z
- Work Packets: `WP-0267`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260706_213832`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.101_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.101_x64_en-US.msi`
- Notes: Final YouTube sign-in/recovery UX. Normal flow opens the selected browser, lets the user sign in and return without mandatory browser closure, then performs exact-source verification. Browser closure, re-login, and manual YouTube-only cookie import are reserved for clear failure recovery. Persists Ready versus Sign-in-required truth and retains auth-circuit queue holding. Reused verified offline payload; warmup skipped because dependency inputs did not change.

## 0.1.103 - 2026-07-22T17:42:02Z
- Work Packets: `WP-0268`, `WP-0269`, `WP-0270`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260706_213832`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.103_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.103_x64_en-US.msi`
- Notes: WP-0268 canonical single-video lineage/history; WP-0269 independent service tracks plus shared safe YouTube start gate; WP-0270 track controls, canonical queue truth, diagnostics, and headless proof surfaces. Warmup gate reused from the immediately preceding all-pack PASS at tool_artifacts/pack_warmup_gate/20260722_171440/report.json. Version 0.1.102 was an aborted pre-build attempt terminated by the shell timeout before artifact or changelog creation.

## 0.1.104 - 2026-07-22T19:17:27Z
- Work Packets: `WP-0268`, `WP-0269`, `WP-0270`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260706_213832`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.104_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.104_x64_en-US.msi`
- Notes: Separates canonical single-video history from subscriptions, preserves independent service tracks with shared YouTube-safe pacing, accelerates bounded autonomous legacy-track and lineage recovery, and adds quiet headless app-boundary proof.

## 0.1.105 - 2026-07-22T19:37:44Z
- Work Packets: `WP-0268`, `WP-0269`, `WP-0270`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260706_213832`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.105_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.105_x64_en-US.msi`
- Notes: Final live-proof correction: canonical single-history polling no longer self-cancels on its busy state, and YouTube background controls distinguish the direct-transfer budget from separately paced subscription enumeration.

## 0.1.106 - 2026-07-22T23:22:45Z
- Work Packets: `WP-0271`, `WP-0272`, `WP-0273`, `WP-0274`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260706_213832`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.106_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.106_x64_en-US.msi`
- Notes: Canonical Jobs track tabs; live single-video batch progress; source dedup and missing-media repair; granular UI refresh and bounded storage observation. Warmup gate skipped on wrapper rerun because the immediately preceding full gate lost its report after command-host timeout; verified offline payload reused.

## 0.1.107 - 2026-07-22T23:36:41Z
- Work Packets: `WP-0271`, `WP-0272`, `WP-0273`, `WP-0274`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260706_213832`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.107_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.107_x64_en-US.msi`
- Notes: Supersedes v0.1.106 with the same WP-0271 through WP-0274 implementation plus explicit closed-disclosure rendering for reliable responsive Jobs visual proof. Verified offline payload reused; product dependency inputs unchanged.

## 0.1.109 - 2026-07-26T17:50:04Z
- Work Packets: `WP-0275`, `WP-0276`, `WP-0277`, `WP-0278`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260706_213832`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.109_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.109_x64_en-US.msi`
- Notes: Unified imported/current YouTube identity and many-to-many source membership; canonical duplicate queue prevention; recoverable exact-hash NAS quarantine and rollback; low-impact watcher and panel/job contention diagnostics. Version 0.1.108 was an aborted pre-build attempt terminated by the command host immediately after version bump, before log or compiler start.

## 0.1.110 - 2026-07-26T17:59:31Z
- Work Packets: `WP-0275`, `WP-0276`, `WP-0277`, `WP-0278`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260706_213832`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.110_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.110_x64_en-US.msi`
- Notes: Supersedes v0.1.109 after packaged proof found a 115,366 ms synchronous subscription activity read delaying panel events. Moves subscription download activity and track-runtime reads to blocking workers while retaining unified YouTube identity, duplicate prevention, recoverable cleanup, and expanded watcher diagnostics.

## 0.1.111 - 2026-07-26T18:07:57Z
- Work Packets: `WP-0275`, `WP-0276`, `WP-0277`, `WP-0278`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260706_213832`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.111_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.111_x64_en-US.msi`
- Notes: Final superseding build: retains v0.1.110 worker-offload freeze fix, makes vvwatch default to lightweight samples with heavy probes every ten seconds and optional tooling inspection, and serializes trace writes without per-command process snapshot overhead.

## 0.1.112 - 2026-07-26T18:13:35Z
- Work Packets: `WP-0275`, `WP-0276`, `WP-0277`, `WP-0278`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260706_213832`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.112_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.112_x64_en-US.msi`
- Notes: Final watcher cadence correction after v0.1.111 proof: honor the full requested monitoring window and limit heavy DB/path/process-tree probes to once per thirty seconds while keeping lightweight bridge, trace, and process responsiveness samples at the requested cadence.

## 0.1.113 - 2026-07-26T18:22:26Z
- Work Packets: `WP-0275`, `WP-0276`, `WP-0277`, `WP-0278`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260706_213832`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.113_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.113_x64_en-US.msi`
- Notes: Final visual-debug fidelity correction: closed details now explicitly hide non-summary descendants so html2canvas snapshots do not overlap hidden import or cleanup controls. Retains v0.1.112 full-window low-impact watcher proof and v0.1.110+ panel freeze fix.

## 0.1.114 - 2026-07-26T21:18:05Z
- Work Packets: `WP-0279`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260706_213832`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.114_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.114_x64_en-US.msi`
- Notes: Headless-only semantic UI inventory and allowlisted structural interaction routes for Video Archiver and Jobs/Queue audit.

## 0.1.115 - 2026-07-26T21:42:56Z
- Work Packets: `WP-0279`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260706_213832`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.115_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.115_x64_en-US.msi`
- Notes: Desktop target build via build_desktop_target.ps1.

## 0.1.116 - 2026-07-26T21:52:13Z
- Work Packets: `WP-0279`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260706_213832`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.116_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.116_x64_en-US.msi`
- Notes: Desktop target build via build_desktop_target.ps1.

## 0.1.117 - 2026-07-26T22:37:03Z
- Work Packets: `WP-0280`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260706_213832`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.117_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.117_x64_en-US.msi`
- Notes: Cohesive Video Archiver and Jobs layouts with bounded render windows; verified offline payload reused.

## 0.1.118 - 2026-07-26T22:43:46Z
- Work Packets: `WP-0280`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260706_213832`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.118_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.118_x64_en-US.msi`
- Notes: Final bounded Jobs preview: 50 groups plus 30 expanded attempts; verified offline payload reused.

## 0.1.119 - 2026-07-26T22:50:19Z
- Work Packets: `WP-0280`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260706_213832`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.119_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.119_x64_en-US.msi`
- Notes: Final cohesive layouts: bounded Jobs preview and disclosed Video Archiver preset editor; verified offline payload reused.

## 0.1.120 - 2026-07-26T23:05:41Z
- Work Packets: `WP-0280`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260706_213832`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.120_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.120_x64_en-US.msi`
- Notes: WP-0280 bounded Video Archiver subscription master list after packaged UI audit; final cohesive workspace verification build.

## 0.1.121 - 2026-07-26T23:13:04Z
- Work Packets: `WP-0280`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260706_213832`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.121_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.121_x64_en-US.msi`
- Notes: WP-0280 explicit safe pagination actions for quiet headless UI navigation; mutating generic controls remain refused.

## 0.1.122 - 2026-07-26T23:24:17Z
- Work Packets: `WP-0280`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260706_213832`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.122_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.122_x64_en-US.msi`
- Notes: WP-0280 compact narrow shell and explicit loaded-row versus canonical queued/archive total wording.

## 0.1.123 - 2026-07-26T23:44:54Z
- Work Packets: `WP-0280`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260706_213832`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.123_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.123_x64_en-US.msi`
- Notes: WP-0280 active-drain subscription activity aggregation; live query benchmark 2063ms to 402ms before build.

## 0.1.124 - 2026-07-27T00:08:13Z
- Work Packets: `WP-0280`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260706_213832`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.124_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.124_x64_en-US.msi`
- Notes: WP-0280: non-blocking attributable retry/repair batch receipts, bounded task registry, and live Jobs operation status.

## 0.1.125 - 2026-07-27T00:15:41Z
- Work Packets: `WP-0280`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260706_213832`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.125_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.125_x64_en-US.msi`
- Notes: WP-0280: remove preset card chrome and replace malformed native disclosure marker with deterministic plus/minus affordance.

## 0.1.126 - 2026-07-27T00:33:37Z
- Work Packets: `WP-0280`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260706_213832`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.126_x64-setup.exe`
- Notes: WP-0280: non-blocking Single videos history, independent unclassified diagnostic count, copy-spacing correction, plus prior cohesive workspace and async batch-operation changes.

## 0.1.127 - 2026-07-27T03:32:31Z
- Work Packets: `WP-0281`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260706_213832`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.127_x64-setup.exe`
- Notes: WP-0281: priority scheduling for overlapping channel/videos/shorts feeds ahead of playlists, plus idempotent historical source-membership backfill; canonical active/present claims remain the final duplicate gate.

## 0.1.128 - 2026-07-27T06:10:41Z
- Work Packets: `WP-0282`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260706_213832`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.128_x64-setup.exe`
- Notes: Manual-only deleted subscription status; exact-404 unavailable status with hosting-channel caveat; preserved metadata and headless assistant control.

## 0.1.129 - 2026-07-27T18:43:20Z
- Work Packets: `WP-0277`, `WP-0283`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260706_213832`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.129_x64-setup.exe`
- Notes: Queue identity execution gate and recoverable NAS cleanup

## 0.1.130 - 2026-07-29T01:48:43Z
- Work Packets: `WP-0284`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260706_213832`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.130_x64-setup.exe`
- Notes: WP-0284: canonical subscription-video selection, recoverable/permanent deletion, durable tombstone suppression, and exact selected-item manual redownload in Video Archiver and Media Library.

## 0.1.131 - 2026-07-29T03:33:10Z
- Work Packets: `WP-0284`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260706_213832`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.131_x64-setup.exe`
- Notes: WP-0284 subscription video lifecycle plus nonblocking hidden agent UI verification startup.

## 0.1.132 - 2026-07-29T03:45:08Z
- Work Packets: `WP-0284`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260706_213832`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.132_x64-setup.exe`
- Notes: WP-0284 subscription video lifecycle, nonblocking headless UI proof, and unambiguous subscription-versus-video deletion labels.

## 0.1.133 - 2026-07-31T06:22:40Z
- Work Packets: `WP-0286`
- Commit: `3fd938c`
- Offline Bundle ID: `offline_full_win64_20260706_213832`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.133_x64-setup.exe`
- Notes: Canonical Media Library backend filtering and live imported identity unification

## 0.1.134 - 2026-08-14T07:56:10Z
- Work Packets: `WP-0298`, `WP-0299`, `WP-0300`, `WP-0301`, `WP-0306`
- Commit: `b5f0b8c`
- Offline Bundle ID: `offline_full_win64_20260814_074826`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.134_x64-setup.exe`
- Notes: Crash-wave diagnostics, secure provider runtime, canonical metadata repair, Options registry, MKV enforcement and root-rebind safety

## 0.1.135 - 2026-08-14T08:44:10Z
- Work Packets: `WP-0265`, `WP-0298`, `WP-0299`, `WP-0300`, `WP-0301`, `WP-0306`
- Commit: `a4fb163`
- Offline Bundle ID: `offline_full_win64_20260814_082842`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.135_x64-setup.exe`
- Notes: v0.1.135 release candidate: crash-wave product changes plus materialized offline payload validation and composable spanned full-installer inputs. Full Inno packaging follows this NSIS build.

## 0.1.136 - 2026-08-14T10:00:34Z
- Work Packets: `WP-0265`, `WP-0298`, `WP-0299`, `WP-0300`, `WP-0301`, `WP-0306`
- Commit: `5476bd2`
- Offline Bundle ID: `offline_full_win64_20260814_082842`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.136_x64-setup.exe`
- Notes: v0.1.136 release candidate: root-rebind backup integrity hardening and serialized offline-installer staging.

## 0.1.137 - 2026-08-14T10:23:42Z
- Work Packets: `WP-0254`, `WP-0265`, `WP-0298`, `WP-0299`, `WP-0300`, `WP-0301`, `WP-0306`
- Commit: `f23531e`
- Offline Bundle ID: `offline_full_win64_20260814_082842`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.137_x64-setup.exe`
- Notes: v0.1.137 release candidate: guarded crash-orphan recovery unblocks exact root-rebind execution without starting background jobs.

## 0.1.138 - 2026-08-14T12:55:59Z
- Work Packets: `WP-0209`, `WP-0210`, `WP-0298`
- Commit: `4d89682`
- Offline Bundle ID: `offline_full_win64_20260814_082842`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.138_x64-setup.exe`
- Notes: Add authoritative runtime fields to visual debugger dumps; re-prove bridge PID sidecar lifecycle and WP-0298 headless diagnostics on the resulting artifact.

## 0.1.139 - 2026-08-14T13:45:26Z
- Work Packets: `WP-0167`
- Commit: `96317e8`
- Offline Bundle ID: `offline_full_win64_20260814_082842`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.139_x64-setup.exe`
- Notes: Make Diagnostics summary tiles semantic, accessible, and safe for headless scroll verification while preserving existing live state.

## 0.1.140 - 2026-08-14T14:23:01Z
- Work Packets: `WP-0166`
- Commit: `ef8cd2c`
- Offline Bundle ID: `offline_full_win64_20260814_082842`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.140_x64-setup.exe`
- Notes: Replace remaining technical jargon with plain-language labels and add visible guidance for presets, crawl scope, sign-in, and inherited storage roots.

## 0.1.141 - 2026-08-14T15:12:29Z
- Work Packets: `WP-0172`
- Commit: `5812aa1`
- Offline Bundle ID: `offline_full_win64_20260814_082842`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.141_x64-setup.exe`
- Notes: Complete all 22 editor help placements, home-screen contextual help, and the persistent Show all help learning-mode toggle.

## 0.1.146 - 2026-08-14T19:32:27Z
- Work Packets: `WP-0183`, `WP-0252`, `WP-0265`
- Commit: `c6b1bc2`
- Offline Bundle ID: `offline_full_win64_20260814_082842`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.146_x64-setup.exe`
- Notes: CosyVoice 2 managed backend selection, offline app-local wetext readiness, and public full-installer input hardening.

## 0.1.147 - 2026-08-14T20:14:45Z
- Work Packets: `WP-0183`
- Commit: `c6b1bc2`
- Offline Bundle ID: `offline_full_win64_20260814_082842`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.147_x64-setup.exe`
- Notes: WP-0183 UI-only semantic stage navigation for safe headless verification. Warmup skip reuses the immediately preceding v0.1.146 full-gate result; no pack, resolver, dependency, payload, Rust, or installer input changed.

## 0.1.148 - 2026-08-14T20:40:09Z
- Work Packets: `WP-0183`
- Commit: `c6b1bc2`
- Offline Bundle ID: `offline_full_win64_20260814_082842`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.148_x64-setup.exe`
- Notes: WP-0183 headless-proof blocker fix: stop Localization item bootstrap from refetch fan-out by calling the latest deferred loader through a stable ref. Reuses the v0.1.146 full pack gate; no pack, resolver, dependency, payload, Rust, or installer input changed.

## 0.1.149 - 2026-08-14T21:56:16Z
- Work Packets: `WP-0182`
- Commit: `b260501`
- Offline Bundle ID: `offline_full_win64_20260814_082842`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.149_x64-setup.exe`
- Notes: WP-0182: enable exact-file dynamic Tauri asset scope for owned source/derived media; harden segment playback cancellation and visible media errors.

## 0.1.150 - 2026-08-14T23:23:36Z
- Work Packets: `WP-0185`, `WP-0186`, `WP-0188`
- Commit: `10944fe`
- Offline Bundle ID: `offline_full_win64_20260814_082842`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.150_x64-setup.exe`
- Notes: Clone outcome diagnostics, canonical per-segment fallback review, and seven-factor reference quality explanation. Built in place because an unidentified directory handle blocked archival while every file passed exclusive-open validation.

## 0.1.151 - 2026-08-14T23:36:53Z
- Work Packets: `WP-0185`, `WP-0186`, `WP-0187`, `WP-0188`
- Commit: `10944fe`
- Offline Bundle ID: `offline_full_win64_20260814_082842`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.151_x64-setup.exe`
- Notes: Final clone UX wave: outcome diagnostics, canonical per-segment fallback review, authoritative sequential clone preflight, visible full voice-plan routing, and seven-factor reference quality explanation. Built in place because an unidentified directory handle blocked archival while every file passed exclusive-open validation.

## 0.1.152 - 2026-08-14T23:47:04Z
- Work Packets: `WP-0185`, `WP-0186`, `WP-0187`, `WP-0188`
- Commit: `10944fe`
- Offline Bundle ID: `offline_full_win64_20260814_082842`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.152_x64-setup.exe`
- Notes: Final clone UX wave with visually reviewed pitch-unavailable guidance: outcome diagnostics, canonical per-segment fallback review, authoritative sequential clone preflight, visible full voice-plan routing, and seven-factor reference quality explanation. Built in place because an unidentified directory handle blocked archival while every file passed exclusive-open validation.

## 0.1.153 - 2026-08-14T23:59:21Z
- Work Packets: `WP-0185`, `WP-0186`, `WP-0187`, `WP-0188`
- Commit: `10944fe`
- Offline Bundle ID: `offline_full_win64_20260814_082842`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.153_x64-setup.exe`
- Notes: Final clone UX wave with dedicated non-queueing clone readiness check, visually reviewed pitch-unavailable guidance, outcome diagnostics, canonical per-segment fallback review, visible full voice-plan routing, and seven-factor reference quality explanation. Built in place because an unidentified directory handle blocked archival while every file passed exclusive-open validation.

## 0.1.154 - 2026-08-15T00:31:51Z
- Work Packets: `WP-0224`
- Commit: `4467f4e`
- Offline Bundle ID: `offline_full_win64_20260814_082842`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.154_x64-setup.exe`
- Notes: Desktop target build via build_desktop_target.ps1.

## 0.1.155 - 2026-08-15T00:49:56Z
- Work Packets: `WP-0224`
- Commit: `4467f4e`
- Offline Bundle ID: `offline_full_win64_20260814_082842`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.155_x64-setup.exe`
- Notes: Desktop target build via build_desktop_target.ps1.

## 0.1.156 - 2026-08-15T01:26:11Z
- Work Packets: `WP-0221`
- Commit: `3c59403`
- Offline Bundle ID: `offline_full_win64_20260814_082842`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.156_x64-setup.exe`
- Notes: Desktop target build via build_desktop_target.ps1.

## 0.1.157 - 2026-08-15T02:16:42Z
- Work Packets: `WP-0226`
- Commit: `0f540a7`
- Offline Bundle ID: `offline_full_win64_20260814_082842`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.157_x64-setup.exe`
- Notes: Desktop target build via build_desktop_target.ps1.

## 0.1.158 - 2026-08-15T02:43:57Z
- Work Packets: `WP-0226`
- Commit: `0f540a7`
- Offline Bundle ID: `offline_full_win64_20260814_082842`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.158_x64-setup.exe`
- Notes: Desktop target build via build_desktop_target.ps1.

## 0.1.159 - 2026-08-15T02:56:13Z
- Work Packets: `WP-0226`
- Commit: `0f540a7`
- Offline Bundle ID: `offline_full_win64_20260814_082842`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.159_x64-setup.exe`
- Notes: Desktop target build via build_desktop_target.ps1.

## 0.1.160 - 2026-08-15T03:06:09Z
- Work Packets: `WP-0226`
- Commit: `0f540a7`
- Offline Bundle ID: `offline_full_win64_20260814_082842`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.160_x64-setup.exe`
- Notes: Desktop target build via build_desktop_target.ps1.

## 0.1.161 - 2026-08-15T04:04:14Z
- Work Packets: `WP-0235`
- Commit: `4a6ee10`
- Offline Bundle ID: `offline_full_win64_20260814_082842`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.161_x64-setup.exe`
- Notes: Explicit first-run voice setup/repair gate with manifest-owned estimate, session-scoped Later/Escape behavior, durable Jobs progress, recovery guidance, and advanced diagnostics route. Public offline-full installs normally bypass the gate because bundled packs are ready.

## 0.1.162 - 2026-08-15T04:18:17Z
- Work Packets: `WP-0235`
- Commit: `4a6ee10`
- Offline Bundle ID: `offline_full_win64_20260814_082842`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.162_x64-setup.exe`
- Notes: Explicit first-run voice setup/repair gate with manifest-owned estimate, session-scoped Later/Escape behavior, durable Jobs progress, recovery guidance, advanced diagnostics route, and headless-only isolated state root for safe proof. Public offline-full installs normally bypass the gate because bundled packs are ready.

## 0.1.163 - 2026-08-15T06:09:06Z
- Work Packets: `WP-0229`
- Commit: `004e44c`
- Offline Bundle ID: `offline_full_win64_20260814_082842`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.163_x64-setup.exe`
- Notes: WP-0229 adds installed-pack short-circuiting for all six Phase2 packs plus explicit force routing and force-safe resume behavior.

## 0.1.164 - 2026-08-15T06:30:00Z
- Work Packets: `WP-0212`
- Commit: `d453405`
- Offline Bundle ID: `offline_full_win64_20260814_082842`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.164_x64-setup.exe`
- Notes: WP-0212 makes topbar chrome discoverable to the existing headless semantic audit while retaining stateful-only click safety.

## 0.1.165 - 2026-08-15T06:43:13Z
- Work Packets: `WP-0212`
- Commit: `d453405`
- Offline Bundle ID: `offline_full_win64_20260814_082842`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.165_x64-setup.exe`
- Notes: WP-0212 completes hidden Safe Mode proof support by auditing app chrome and allowlisting only the session-local exit-notice dismiss control.

## 0.1.166 - 2026-08-15T07:18:22Z
- Work Packets: `WP-0206`
- Commit: `4f3e7e6`
- Offline Bundle ID: `offline_full_win64_20260814_082842`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.166_x64-setup.exe`
- Notes: WP-0206 restores per-item failed-run count and clear actions to the default setup-first Localization home while retaining non-destructive defaults.

## 0.1.167 - 2026-08-15T07:45:26Z
- Work Packets: `WP-0207`, `WP-0208`, `WP-0211`
- Commit: `8f1c42e`
- Offline Bundle ID: `offline_full_win64_20260814_082842`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.167_x64-setup.exe`
- Notes: WP-0211 finishes smart default-stage selection and legacy-anchor routing in the master-detail editor; WP-0207/WP-0208 workflow concepts are retained through the stage rail and selected-stage action strip.

## 0.1.168 - 2026-08-15T07:56:16Z
- Work Packets: `WP-0207`, `WP-0208`, `WP-0211`
- Commit: `8f1c42e`
- Offline Bundle ID: `offline_full_win64_20260814_082842`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.168_x64-setup.exe`
- Notes: WP-0211 master-detail closeout: smart default selection, legacy-anchor stage routing, all eight selected-stage controls, and zero unnamed audited controls across Captions and Files surfaces.

## 0.1.169 - 2026-08-15T15:56:34Z
- Work Packets: `WP-0129`, `WP-0145`, `WP-0196`
- Commit: `a96c105`
- Offline Bundle ID: `offline_full_win64_20260815_151923`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.169_x64-setup.exe`
- Notes: Makes offline payload export interruption-resumable, restores Localization advanced deep-link routing and reveal behavior, and exposes accessible headless-verifiable desktop font-scale controls.

## 0.1.170 - 2026-08-17T16:10:51Z
- Work Packets: `WP-0147`, `WP-0302`, `WP-0303`, `WP-0304`, `WP-0305`
- Commit: `67fdb47`
- Offline Bundle ID: `offline_full_win64_20260815_151923`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.170_x64-setup.exe`
- Notes: Multi-Provider & Archiver: WP-0147, WP-0302, WP-0303, WP-0304, WP-0305

## 0.1.171 - 2026-08-17T19:28:44Z
- Work Packets: `WP-0147`, `WP-0302`, `WP-0303`, `WP-0304`, `WP-0305`
- Commit: `4f1f79c`
- Offline Bundle ID: `offline_full_win64_20260817_192327`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.171_x64-setup.exe`
- Notes: Fix offline bundle hydration readiness gate: require Node and YouTube PO provider binaries before skipping extraction

## 0.1.172 - 2026-08-17T21:10:50Z
- Work Packets: `WP-0147`, `WP-0302`, `WP-0303`, `WP-0304`, `WP-0305`
- Commit: `a0d7d45`
- Offline Bundle ID: `offline_full_win64_20260817_192327`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.172_x64-setup.exe`
- Notes: Enforce Node and YouTube PO Provider offline readiness and verified package assets

## 0.1.173 - 2026-08-18T07:53:53Z
- Work Packets: `WP-0147`, `WP-0302`, `WP-0303`, `WP-0304`, `WP-0305`
- Commit: `c3d1ec5`
- Offline Bundle ID: `offline_full_win64_20260817_192327`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.173_x64-setup.exe`
- Notes: Instagram archiver recovery: dual lanes (single vs subscriptions), browser session cookies, gallery/favorites, and Inno Setup silent execution fix

## 0.1.174 - 2026-08-19T04:01:29Z
- Work Packets: `WP-0265`
- Commit: `c3d1ec5`
- Offline Bundle ID: `offline_full_win64_20260817_192327`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.174_x64-setup.exe`
- Notes: Full-offline installer health fixes: revert SolidCompression regression under DiskSpanning, keep shellexec UAC elevation for the perMachine NSIS core, embed WebView2 offline runtime so no bootstrapper download is required

## 0.1.175 - 2026-08-20T18:39:20Z
- Work Packets: `WP-0265`
- Commit: `c3d1ec5`
- Offline Bundle ID: `offline_full_win64_20260817_192327`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.175_x64-setup.exe`
  - `product/desktop/build_target/Current/offline_full/VoxVulgi_0.1.175_x64_offline_full_setup.exe`
  - `product/desktop/build_target/Current/offline_full/VoxVulgi_0.1.175_x64_offline_full_setup-1.bin` through `-4.bin`
  - `product/desktop/build_target/Current/offline_full/VoxVulgi_0.1.175_x64_offline_full_setup.artifacts.json`
- Notes: Full-offline installer long-path fix: require Inno Setup 7 extended-length path support; preserve disk spanning, non-solid compression, offline WebView2, update semantics, and shellexec NSIS elevation
  - Inno Setup 7.1.0 full compile succeeded; the five-file installer set totals `7,737,985,056` bytes.
  - Isolated runtime probe installed the reported ModelScope filename at a 295-character destination and returned exit code 0 without touching live VoxVulgi app data.

## 0.1.177 - 2026-08-22T20:29:20Z
- Work Packets: `WP-0303`, `WP-0304`
- Commit: `11d61bb`
- Offline Bundle ID: `offline_full_win64_20260822_201543`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.177_x64-setup.exe`
- Notes: Instagram and TikTok provider refactors with canonical metadata, restart-safe dedupe, bounded failure holds, non-destructive subscription archive semantics, and verified Windows provider-install recovery.

## 0.1.178 - 2026-08-22T20:38:19Z
- Work Packets: `WP-0303`, `WP-0304`
- Commit: `11d61bb`
- Offline Bundle ID: `offline_full_win64_20260822_201543`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.178_x64-setup.exe`
- Notes: Final Instagram and TikTok provider refactors; packaged visual audit correction aligns Instagram subscription action label with non-destructive archive semantics.

## 0.1.179 - 2026-08-23T01:19:44Z
- Work Packets: `WP-0309`, `WP-0310`
- Commit: `ecd4437`
- Offline Bundle ID: `offline_full_win64_20260822_201543`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.179_x64-setup.exe`
- Notes: Database-first startup serializes schema/default-library readiness before bridge, offline hydration, watcher supervision, and runtime background work; vvwatch startup diagnostics hardened.

## 0.1.180 - 2026-08-23T17:31:26Z
- Work Packets: `WP-0298`, `WP-0311`, `WP-0312`, `WP-0313`
- Commit: `93fa9ea`
- Offline Bundle ID: `offline_full_win64_20260822_201543`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.180_x64-setup.exe`
- Notes: Bounded database runtime, diagnostics demand coordination, provider verification, and remediation release

## 0.1.181 - 2026-08-24T11:09:31Z
- Work Packets: `WP-0298`, `WP-0308`, `WP-0311`, `WP-0312`, `WP-0313`
- Commit: `93fa9ea`
- Offline Bundle ID: `offline_full_win64_20260822_201543`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.181_x64-setup.exe`
- Notes: Bounded runtime remediation and 64 MiB bounded-solid single-unit offline installer release

## 0.1.182 - 2026-08-24T15:19:13Z
- Work Packets: `WP-0298`, `WP-0308`, `WP-0311`, `WP-0312`, `WP-0313`
- Commit: `93fa9ea`
- Offline Bundle ID: `offline_full_win64_20260822_201543`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.182_x64-setup.exe`
- Notes: Close yt-dlp suspended-launch race and correct bounded-solid manifest metadata
