# Work Packet: WP-0265 - Installer packaging fix (thin NSIS) + full-offline packaging plan

## Status

IN_PROGRESS (operator-requested 2026-07-08; thin build 0.1.91 shipped; full-quality single-exe OFFLINE installer built + component-verified offline this session, pending final clean-machine install proof)

## Owner

Claude (Opus 4.8)

## Operator Request Preserved

- 2026-07-08: "i tried to create a new installer because we made some changes but i was working with a lightweight model and it could not do this and got stuck for some unknown reason" ... "i think it tried to reinstall all dependencies perhaps? not sure"
- 2026-07-08 decision (AskUserQuestion): "Thin now + fix root cause" — build the thin NSIS installer now, set the config so the >2GB MSI hang can't recur, and open this WP to solve full-offline packaging later.

## Intent

Produce a working desktop installer for the current working-tree changes, and remove the root cause that made the previous attempts hang for ~54 minutes and fail. Preserve the goal of an eventual fully-offline installer as an explicit follow-on rather than dropping it.

## Evidence / root cause (verified 2026-07-08, this machine)

- The Rust release compile and the frontend build SUCCEEDED in every recent attempt (`Finished release profile ... in 4m 25s` -> `desktop.exe` built; `npm run build` passed). The code was never the problem.
- The hang/failure was the installer PACKAGING step, specifically WiX `light.exe` (the MSI linker):
  - `build_desktop_target_20260706-225209_0_1_89.log`: ran 54m35s, then `Error failed to bundle project 'failed to run C:\Users\Ilja Smets\AppData\Local\tauri\WixTools314\light.exe'`.
  - `tauri_msi_verbose2.log.err`: reached the `light.exe` link step and ends in `^C` (a manual interrupt on a run that looked hung).
- Cause: `product/desktop/src-tauri/tauri.conf.json` bundled `"offline/**/*"` — the ~5.08 GB offline payload (`offline_full_win64_20260706_213832`, 5,450,290,488 bytes) — into the installer, with `bundle.targets: "all"` (MSI + NSIS). This is above the packaging ceiling the project already documented.
- The project already knew this: BUILD_CHANGELOG **0.1.87** (2026-07-04) says the full payload was "intentionally excluded from bundled resources because both WiX/MSI and NSIS hit installer packaging limits above roughly 2GB" and shipped as a **thin NSIS installer**; packs remain repairable/downloadable after install.
- Disk space is NOT the cause (D: ~1012 GB free, C:/TEMP ~721 GB free at build time).
- The operator's "reinstall all dependencies" memory maps to a REAL but EARLIER step: a payload/dependency refresh did run 2026-07-06 ~21:38 (it produced bundle `offline_full_win64_20260706_213832` and modified `pinned_dependency_manifest.json` + the tts lockfiles). The runs that then got STUCK reused that cached payload (fingerprint matched — no reinstall) and hung at MSI packaging.

## Research basis

- Sources checked: repo BUILD_CHANGELOG (0.1.87 thin-NSIS precedent), the failed build logs on this machine, `build_desktop_target.ps1`, `tauri.conf.json`, WiX Toolset 3.14 behavior (single-threaded `light.exe` cabinet compression, practical multi-GB failure/OOM), Tauri bundler resource model.
- Selected approach: thin installer (exclude the multi-GB payload from bundled resources) + NSIS-only target — matches the last known-good build (0.1.87) and the documented ~2GB ceiling.
- Rejected (for now): bundling the full 5GB payload into MSI/NSIS — proven to hang/fail; not viable with the current single-archive bundler.

## What was done (thin fix, this WP)

`product/desktop/src-tauri/tauri.conf.json`:
- `bundle.resources`: removed `"offline/**/*"`; kept `"watcher/**/*"` (WP-0253 bundled watcher) and `"voice_backends_seed/**/*"` (both small).
- `bundle.targets`: `"all"` -> `"nsis"` so the default build no longer attempts the MSI path that hangs on large payloads.
- Build: `build_desktop_target.ps1 -WorkPackets WP-0265,WP-0250 -SkipWarmupGate ... -NoArchiveCurrent -- --bundles nsis` (reuses the verified cached payload; no dependency reinstall). Version 0.1.90 -> 0.1.91.

Runtime behavior after a thin install: the app provisions/repairs the offline packs on demand (the same install/warmup path the pack warmup gate exercises). This is the established 0.1.87 tradeoff — a thin install needs a first-run pack download for localization to work fully.

## Relevant files

- `product/desktop/src-tauri/tauri.conf.json` (resources + targets) — CHANGED.
- `governance/scripts/build_desktop_target.ps1` (build entrypoint) — unchanged.
- `product/desktop/src-tauri/offline/manifest.json` (payload id/bytes) — unchanged.
- `product/engine/src/tools.rs` (pack install/warmup/repair) — the post-install download path a thin installer relies on.

## Acceptance Criteria (thin build)

- A `VoxVulgi_0.1.91_x64-setup.exe` NSIS installer is produced under `product/desktop/build_target/Current/release/bundle/nsis/`.
- The build completes without a WiX/MSI `light.exe` stall (no MSI target attempted).
- BUILD_CHANGELOG has a 0.1.91 entry naming WP-0265 + WP-0250 and noting the warmup-gate skip.
- No user data touched; the cached offline payload was reused, not rebuilt.

## Full-quality single-exe OFFLINE installer (built this session)

Operator 2026-07-08 chose (AskUserQuestion): single self-installing ~big .exe, "Full quality set incl CosyVoice" — every feature works with zero internet, out of the box, for non-technical users.

### Approach (Inno Setup, no app rebuild needed)
- Tooling: **Inno Setup 6.7.3** (`ISCC.exe`). NSIS/WiX cannot make a single archive >~2 GB; Inno natively handles the multi-GB size, per-user file placement, and a real progress bar.
- The single `.exe` = the existing 8.6 MB NSIS `setup.exe` (run silently `/S`, keeps the app install + Update/Reinstall/Full-reinstall/Uninstall maintenance flow) + the ~12.83 GB relocatable pack payload laid into per-user `%APPDATA%\com.voxvulgi.voxvulgi`.
- Script: `product/desktop/src-tauri/installer/VoxVulgi_offline_full.iss`; driver: `governance/scripts/build_offline_full_installer.ps1` (both parameterized/portable via `/D` defines; the driver guards the payload before compiling).
- `PrivilegesRequired=lowest` so `{userappdata}`/`{userprofile}` resolve to the real user; the NSIS `setup.exe` self-elevates for its per-machine app install.

### Payload build (12.83 GB, from the LIVE working install)
- Source = the operator's live, proven-working `%APPDATA%\com.voxvulgi.voxvulgi` (NOT the stale `src-tauri/offline/` export, which was broken). Copied with **robocopy** (which dereferences symlinks) — pack roots only: `tools/`, `models/`, `cache/huggingface/`, `voice_backends/`. **No DB / subscriptions / config / personal data.**
- Root cause of the old broken payload FOUND: the Rust `copy_tree`/`export_offline_payload` dropped the HF-cache symlinks, so the Kokoro `snapshots/<sha>/{config.json,kokoro-v1_0.pth,voices/af_heart.pt}` triplet went missing → `kokoro_app_cache_ready` false → offline dub dead. robocopy materializes them as real files (verified in the payload).

### The two transformations the installer performs on the target
1. Rewrite BOTH `tools/python/venv/pyvenv.cfg` and `tools/python/venv_cosyvoice/pyvenv.cfg` to the target user's AppData portable-python path (the `[Code]` `RewritePyvenv` proc). Venvs proven relocatable with only this rewrite.
2. Place the 20.86 MB **wetext ModelScope cache** (with its `.msc` index) at `%USERPROFILE%\.cache\modelscope\hub\pengzhendong\wetext`.

### CosyVoice offline gap FOUND + FIXED (was WP-0252 "wetext pre-cache" remaining)
- CosyVoice's text-normalizer (`wetext` 0.0.4) calls `snapshot_download("pengzhendong/wetext")` which makes a MANDATORY modelscope.cn revision-check at load time (no cache fallback). modelscope 1.20.0 has NO offline env var (`MODELSCOPE_OFFLINE`/`HUB_OFFLINE` do not exist — verified in-package and against master); offline is param-only (`local_files_only=True`).
- Fix (in the bundled Python venv, so NO app/Rust rebuild): patch `venv_cosyvoice/.../wetext/wetext.py` to `snapshot_download(..., local_files_only=True)` with an online fallback, + bundle the wetext cache. Proven under a dead-proxy offline sim: `COSYVOICE2_OFFLINE_LOAD_OK` with zero successful network calls.

### Offline audit (relocated venvs + bundled cache, `HTTP(S)_PROXY=dead` to force offline)
- Kokoro TTS: KPipeline built + synth OK (94800 samples), no network.
- OpenVoice: `ToneColorConverter(enable_watermark=False)` loads checkpoint offline (app always passes `enable_watermark=False`, so wavmark is never needed).
- spleeter (DEFAULT separation): loads 2stems + separates `['accompaniment','vocals']` offline.
- CosyVoice (premium): loads offline after the wetext fix.
- Whisper ASR: C++/FFI (`asr.rs`) on a local `.bin` — no network by design.
- demucs (optional, non-default separation): pre-broken on the LIVE install too (`torchaudio` `PyInit__torchaudio`, torch 2.10) — NOT introduced by relocation, NOT the default; out of scope (separate dependency task if wanted).

### Artifacts
- `product/desktop/src-tauri/installer/VoxVulgi_offline_full.iss`
- `governance/scripts/build_offline_full_installer.ps1`
- Output: `VoxVulgi_0.1.91_x64_offline_full_setup.exe` (built under the operator's chosen output dir; ~10-12 GB).

### Acceptance / remaining proof
- Component-level offline proof is DONE (every feature loads/synths offline from the relocated payload — see audit above).
- FINAL end-to-end proof (run the single .exe on a CLEAN machine/VM with no internet -> app installs + every feature works) is NOT run here: the installer writes to the SAME `%APPDATA%`/Program Files as the operator's live install, so running it on this machine would overwrite the working setup and fight the running app for locked files. Run it on a clean PC/VM (or here only with explicit operator consent to overwrite the live setup).

## Verification steps

- Confirm the produced `...setup.exe` exists and its size is sane (tens–hundreds of MB, NOT ~5GB).
- Confirm the 0.1.91 BUILD_CHANGELOG entry and the build log under `product/desktop/build_target/logs/`.
- Per `build_rules.md`, app-boundary verification of the built app (bridge health/state + a snapshot) once installed, without stealing focus.

## Red-Team

- Thin installer on an offline machine can't complete localization (no packs). Control: this is a known, operator-accepted tradeoff (0.1.87); the full-offline follow-on above is tracked, not dropped.
- Dropping MSI could break any workflow that expected an `.msi` artifact. Control: `targets: "nsis"` matches the last-good 0.1.87 build; MSI is now trivially re-enable-able because a THIN installer is small enough for WiX to package — re-enable and validate under the follow-on if MSI is still required.
- Config drift: someone re-adds `offline/**/*` to `resources` and the hang returns. Control: this WP + the 0.1.91 changelog note document why it must stay out until the follow-on packaging exists.
- Version/build drift (changelog versions have historically run ahead of committed build messages, per WP-0262). Control: out of scope here; flagged for a separate reconciliation.

## Notes

- Depends on / relates to: WP-0250 (the WebView2 occlusion fix carried in this build), WP-0253 (bundled watcher resource), WP-0251/WP-0252 (offline localization packs that a full-offline installer would need to carry).
