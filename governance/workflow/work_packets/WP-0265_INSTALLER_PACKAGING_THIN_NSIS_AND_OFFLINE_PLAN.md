# Work Packet: WP-0265 - Installer packaging fix (thin NSIS) + full-offline packaging plan

> Historical record only. Its installer build commands, Inno 6/spanned-output architecture, and artifact expectations are superseded. The only current installer creation procedure is `governance/release/OFFLINE_INSTALLER_BUILD_MANUAL.md`; current delivery is governed by WP-0308.

## Status

BLOCKED (thin build 0.1.91 shipped; full-quality single-exe OFFLINE installer built + component-verified; pending final clean-machine install proof)

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

## 2026-08-19 installer health check + fixes (v0.1.174)

Operator report: a prior lightweight-model session attempted the Inno transfer; the earlier installer "ran for hours and gave an error", and the follow-up changes were untested.

### Root cause of the hours-then-error failure (config chain verified, error dialog not observed)

- The Inno wrapper runs `PrivilegesRequired=lowest` by design so `{userappdata}` resolves to the invoking user.
- The core NSIS setup is `installMode: perMachine`, and `installer.nsi:104` compiles `RequestExecutionLevel admin` for that mode.
- The committed `[Run]` entry launched it WITHOUT `shellexec`, i.e. via CreateProcess, which cannot trigger UAC and fails with Windows error 740 (`ERROR_ELEVATION_REQUIRED`) — and only AFTER the multi-GB extraction completes, which matches the reported symptom.
- `UNVERIFIED`: the operator's exact on-screen error text was not captured, so the 740 attribution is from the configuration chain, not from an observed dialog.

### Changes made

1. `VoxVulgi_offline_full.iss` — KEPT the prior session's `shellexec` addition (correct fix for the above).
2. `VoxVulgi_offline_full.iss` — REVERTED `Compression=lzma2/fast` -> `lzma2/normal` and `SolidCompression=yes` -> `no`. Inno documents that under solid compression a failed extraction cannot seek back to the failed file and must re-decompress from the first slice; on a 4-slice 7 GB spanned set that makes any single-file Retry restart from slice 1. Rationale is now recorded inline in the `.iss` and in TECHNICAL_DESIGN 2.1 as a packaging invariant.
3. `tauri.conf.json` — added `bundle.windows.webviewInstallMode = { "type": "offlineInstaller" }`. Previously unset, so Tauri's default `downloadBootstrapper` applied and a machine without WebView2 would have needed internet, violating the zero-download contract. First attempt placed the key under `bundle.windows.nsis` and the build failed schema validation in seconds; corrected against the Tauri v2 config schema, where `webviewInstallMode` is a sibling of `nsis`.

### Evidence

- `tsc --noEmit` clean; contract tests 244/244 pass.
- Core NSIS setup grew 10.4 MB -> **215.7 MB**, confirming the WebView2 Evergreen runtime is embedded rather than downloaded.
- Full-offline set compiled successfully in 3413 s: `VoxVulgi_0.1.174_x64_offline_full_setup.exe` + 4 `.bin` slices, 7,737,939,347 bytes total, manifest written.
- Delivery folder: `product/desktop/build_target/Current/offline_full/`.
- BUILD_CHANGELOG entry `0.1.174` records WP-0265 and the offline bundle id `offline_full_win64_20260817_192327`.

### User-data boundary (verified against the `[Files]` section)

The installer writes ONLY to `tools/`, `models/`, `cache/huggingface/`, `tools/python/venv_cosyvoice`, and `voice_backends/` under `%APPDATA%\com.voxvulgi.voxvulgi`, all with `ignoreversion uninsneveruninstall`. It does not write `app.sqlite`, `db/`, `config/`, `secrets/`, `library/`, `derived/`, `voice_library/`, `voice_templates/`, `logs/`, or `diagnostics/`.

### Still NOT proven (unchanged blocker)

End-to-end install on a clean machine/VM with no internet is still not run. Running it on the build machine overwrites the operator's live install and cannot demonstrate the no-WebView2 path, because this machine already has WebView2. The UAC prompt now appears at the END of the extraction (`shellexec`), so an unattended run can leave the payload installed with the app itself not installed.

## 2026-08-20 Inno 6 long-path extraction failure and Inno 7 migration

### Exact observed failure and canonical path

- Operator screenshot: `MoveFile failed; code 3. The system cannot find the path specified` while extracting `image_quality_assessment_degradation_dataset.cpython-311.pyc`.
- Build-log reconciliation identifies the source as `venv_cosyvoice\Lib\site-packages\modelscope\msdatasets\dataset_cls\custom_datasets\image_quality_assessment_degradation\__pycache__\image_quality_assessment_degradation_dataset.cpython-311.pyc`.
- Under the actual target root `C:\Users\Ilja Smets\AppData\Roaming\com.voxvulgi.voxvulgi`, the installed path is 263 characters. A second bundled CosyVoice `.pyc` destination is 261 characters. The failing public artifact was compiled with Inno Setup 6.7.3.

### Current research basis

- Inno Setup 7 revision history: Setup and Uninstall gained extended-length-path support throughout, removing `MAX_PATH` limitations; the change applies to both 32-bit and 64-bit installers: https://jrsoftware.org/files/is7-whatsnew.htm
- Inno Setup 7 64-bit documentation: both compiler editions can build either installer architecture, coexist with Inno 6, and the 64-bit compiler is recommended: https://jrsoftware.org/ishelp/topic_64bit.htm
- Inno Setup `[Files]` documentation confirms destination directories are created automatically and files are initially written under temporary names before their final rename, matching the observed extraction-then-`MoveFile` failure boundary: https://jrsoftware.org/ishelp/topic_filessection.htm
- Local payload scan: only the two CosyVoice `.pyc` paths cross 259 characters for the current profile. Three default-payload paths also cross it, but all are inside the already-excluded `youtube_po_provider_previous_*` rollback tree; the voice-backend input has no over-limit paths for this profile.

### Options and selected approach

- Rejected: exclude only the two `.pyc` files. This is profile-depth-specific, can regress for longer usernames or redirected AppData roots, and discards warm bytecode from a payload whose first-import latency was previously a production failure.
- Rejected: shorten or relocate the product AppData root. This breaks the canonical runtime path and update compatibility.
- Selected: require Inno Setup 7 or newer in both the `.iss` preprocessor and governed PowerShell driver. Preserve payload contents, target paths, update semantics, disk spanning, non-solid compression, and the `shellexec` NSIS elevation handoff.

### Risks, controls, and verification

- Risk: a developer directly invokes Inno 6 and recreates the broken artifact. Control: `.iss` compile-time `Ver >= 7.0.0` guard plus driver version discovery/rejection; contract test covers both surfaces.
- Risk: the toolchain migration changes the existing update/elevation behavior. Control: no changes to `[Files]`, `PrivilegesRequired`, `[Run]`, `DiskSpanning`, or `SolidCompression`; the regression contract pins those invariants.
- Risk: compile success alone does not prove installation. Control: compile the new versioned full artifact, then perform an isolated install-boundary probe that reaches and verifies both formerly over-limit files without touching the operator's live AppData, followed by clean-machine/offline end-to-end proof when that environment is available.
- Verification completed: Inno 6.7.3 aborts on the new guard; Inno 7.1.0 compiled the complete `0.1.175` spanned installer; `offlineFullInstallerContract.test.ts` passes 3/3; the complete desktop contract suite passes 247/247; TypeScript passes with `--noEmit`.
- Artifact receipt: setup executable plus four payload slices total `7,737,985,056` bytes and match `VoxVulgi_0.1.175_x64_offline_full_setup.artifacts.json`.
- Runtime boundary receipt: an isolated Inno 7.1.0 installer installed the reported `image_quality_assessment_degradation_dataset.cpython-311.pyc` filename at a 295-character destination, logged `Installation process succeeded.`, returned exit code 0, and cleaned its isolated temporary tree. The operator's live VoxVulgi AppData was not touched.
- Remaining external proof edge: the exact 7.7 GB production artifact has not been run end-to-end on a clean offline Windows profile because its canonical dependency destination is the live user AppData root. That clean-profile proof remains separate from the demonstrated long-path fix.

## 2026-08-21 orphaned yt-dlp lifecycle hardening

### Observed failure and source-of-truth boundary

- The installer reported that `yt-dlp` was still running and could not close it. Live process inspection found VoxVulgi-owned `yt-dlp.exe` PID `265420` still executing subscription enumeration with `--sleep-requests 600` after its recorded VoxVulgi parent PID `331620` had exited.
- The canonical launch path was `jobs::run_yt_dlp -> run_command_output_with_control -> Command::spawn`; cancellation and timeouts killed the tree, but app exit/crash had no ownership primitive for the spawned process.

### Research basis and selected approach

- Microsoft Job Objects documentation states that associated child processes inherit the job by default and that closing the last handle with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` terminates all associated processes: https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects
- Microsoft `AssignProcessToJobObject` documents the process association and nested-job behavior used by current Windows versions: https://learn.microsoft.com/en-us/windows/win32/api/jobapi2/nf-jobapi2-assignprocesstojobobject
- Microsoft `TerminateJobObject` provides the explicit graceful-shutdown path and cannot be postponed or handled by the child: https://learn.microsoft.com/en-us/windows/win32/api/jobapi2/nf-jobapi2-terminatejobobject
- Selected: reuse the repo's existing provider Job Object pattern as one shared, process-local yt-dlp lifecycle job. Bind each spawned yt-dlp immediately, explicitly terminate the job from Tauri `RunEvent::Exit`, and retain kill-on-last-handle for crash cleanup.
- Rejected: installer-only name-based process killing. It cannot establish ownership and risks terminating unrelated user-managed yt-dlp work.
- Rejected: apply the Job Object to every external command. The reported defect is downloader-specific, and broader process semantics could interfere with Python/model backends that manage their own children.

### Risks, controls, and verification

- Risk: Job Object creation or assignment fails. Control: fail closed by killing the just-spawned process tree and returning a launch error; never leave an untracked downloader running.
- Risk: a new downloader races graceful shutdown. Control: serialize bind/shutdown, mark shutdown before termination, and reject every later bind.
- Risk: a downloader-created child survives. Control: do not enable breakaway; Windows default job inheritance covers descendants.
- Verification: focused Windows tests must prove both explicit `TerminateJobObject` and last-handle closure terminate real sleeping child processes; engine and desktop regression checks must pass; the governed build must increment the desktop semantic version.
- Verification completed: Windows lifecycle tests pass 4/4, including a separate owner process terminated abruptly followed by an independent wait on its assigned child PID; focused yt-dlp tests pass 29/29; `cargo check` for the desktop manifest passes; `git diff --check` passes; live process reread finds no `yt-dlp.exe` or lifecycle-probe `ping.exe` remaining.
- Build state: no v0.1.176 artifact was produced. The governed PowerShell build runner remained inert before its first step, and even fresh `powershell.exe -NoProfile -NonInteractive -Command exit` probes hung. The attempted build was terminated without reaching the version bump; the three canonical desktop version files remain `0.1.175`.

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
