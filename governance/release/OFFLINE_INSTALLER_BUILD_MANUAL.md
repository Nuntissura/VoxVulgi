---
file_id: VV-INSTALLER-BUILD-MANUAL
file_kind: build-manual
updated_at: 2026-08-24
---

<topic id="purpose-and-authority" status="active" version="v1" wp="WP-0308" updated_at="2026-08-24">

# VoxVulgi Full-Offline Installer Build Manual

This is the single canonical, no-context procedure for creating every public VoxVulgi full-offline installer or updater. `AGENTS.md`, `CLAUDE.md`, `PROJECT_CODEX.md`, and `build_rules.md` must link here instead of duplicating the procedure.

The public deliverable is exactly one UDF ISO named `VoxVulgi_<version>_x64_offline_full.iso`. Its root contains `Install_VoxVulgi.exe`; users do not download companion `.bin` files, extract archives, install Python, run pip, use a terminal, or download models for the default workflow.

The governed payload layout is five external bounded-solid `.7z` archives using a 64 MiB solid block. `ArchiveExtraction=enhanced/nopassword`, `DiskSpanning=no`, and `SolidCompression=no` remain mandatory in the Inno wrapper; `SolidCompression=no` applies to Inno's compiled stream and does not disable solid blocks in the external payload archives.

Normative product behavior remains in:

- `AGENTS.md` / `CLAUDE.md`: `[VV-INSTALL-001]` through `[VV-INSTALL-008]`.
- `governance/spec/PRODUCT_SPEC.md`: section 8.1.8.
- `governance/spec/TECHNICAL_DESIGN.md`: section 2.1.
- `build_rules.md`: single-unit offline installer build gate.

Implementation and proof surfaces:

- Core desktop build: `governance/scripts/build_desktop_target.ps1`.
- ISO build driver: `governance/scripts/build_offline_full_installer.ps1`.
- Inno wrapper: `product/desktop/src-tauri/installer/VoxVulgi_offline_full.iss`.
- Performance gate: `governance/scripts/test_offline_installer_performance.ps1`.
- Contract test: `product/desktop/tests/offlineFullInstallerContract.test.ts`.
- Active delivery contract: `governance/workflow/work_packets/WP-0308_SINGLE_UNIT_FAST_OFFLINE_INSTALLER_v1.md`.

</topic>

<topic id="prerequisites" status="active" version="v1" wp="WP-0308" updated_at="2026-08-23">

## Build-machine prerequisites

- Windows x64 with sufficient local free space for the source payload, reusable archive cache, ISO staging tree, and final ISO.
- Inno Setup 7 or newer. Inno 6 is rejected because the Python payload exceeds classic Windows path limits.
- Full x64 7-Zip CLI version 26.02 or newer at `7z.exe`. The reduced `7zr.exe` is not accepted because final ISO listing is required.
- Microsoft Windows ADK Deployment Tools providing `oscdimg.exe`.
- The repository's normal Node, Rust, Tauri, and desktop-build dependencies.
- No concurrent VoxVulgi installer build. Do not run the full build while another installer is saturating the source or target disks.
- No concurrent full-offline wrapper run in the same Windows session. The wrapper owns a setup mutex and refuses a second writer before payload extraction or promotion begins; do not bypass that guard.

The build scripts auto-discover standard tool locations. Use `-IsccPath`, `-SevenZipPath`, or `-OscdimgPath` only for a verified non-standard installation. Missing or wrong-version tools are hard failures; do not bypass them.

</topic>

<topic id="required-inputs" status="active" version="v1" wp="WP-0308" updated_at="2026-08-21">

## Required inputs

The ISO driver requires five explicit, independently validated inputs:

1. `PayloadDir`: the complete default relocatable payload containing `tools/`, `models/`, and `cache/huggingface/`. The governed repo-local candidate is `product/desktop/src-tauri/offline` after the desktop payload validator passes.
2. `CosyVoiceVenvDir`: the complete relocatable CosyVoice virtual environment containing `Scripts/python.exe`.
3. `VoiceBackendsDir`: the complete voice-backend tree containing the governed CosyVoice wrapper, code, pretrained model weights, Matcha-TTS, and wetext graphs.
4. `SetupExe`: the just-built core NSIS installer named exactly `VoxVulgi_<version>_x64-setup.exe`.
5. `OutputDir`: the managed release folder, normally `product/desktop/build_target/Current/offline_full`.

`CosyVoiceVenvDir` and `VoiceBackendsDir` have no assumed machine-independent repo default. Resolve the configured release-payload roots on the current build machine and let `-ValidateInputsOnly` prove their contents. Never substitute a partial seed, live guess, symlinked Hugging Face cache, or user database/media folder.

The driver verifies the Kokoro readiness triplet, CosyVoice interpreter/model/code/wetext files, real-file materialization, and the staged wrapper hash before building.

</topic>

<topic id="build-procedure" status="active" version="v1" wp="WP-0308" ingestable="true" updated_at="2026-08-24">

## Canonical build procedure

Run from the repository root. Replace the two explicitly configured CosyVoice roots, the three explicitly configured build-tool executable paths, and the work-packet list. The desktop build increments the semantic version once; the ISO build reuses that same version and must not bump it again.

### 1. Validate contracts and the existing offline payload

```powershell
npm --prefix product/desktop run test:contracts
.\governance\scripts\build_desktop_target.ps1 -ValidateOfflinePayloadOnly
```

If the payload validator reports stale or missing inputs, use the governed refresh path in the desktop build. Do not replace validation with a manual file-count check.

For WP-0313 release proof, stage the complete installed provider payload under an owned disposable absolute app-data root, launch the packaged executable with `--agent-headless` and `VOXVULGI_AGENT_HEADLESS_BASE_DIR` set to that root, then start `POST /agent/provider_verify` and poll `GET /agent/provider_verify` alongside `/agent/health`. Record the resolved root, source identity, scan count, file/byte totals, terminal attestation, bridge latency, and before/after payload identity. This proof route never enables ordinary headless startup hydration and never targets the operator profile.

### 2. Build the versioned core NSIS installer

Routine release build reuses a verified unchanged payload. Add every included work-packet ID.

```powershell
.\governance\scripts\build_desktop_target.ps1 `
  -WorkPackets WP-0308 `
  -BuildNotes "Single-unit full-offline ISO release" `
  -TauriArgs '--bundles=nsis'
```

Use `-RefreshOfflinePayload` only when dependency/model inputs changed. Use `-ForceRefreshOfflinePayload` only when the normal fingerprint path cannot safely reuse the payload. Both can take a long time and must show useful progress.

### 3. Resolve and validate the ISO inputs

```powershell
$vv_repo_root = (Resolve-Path -LiteralPath '.').Path
$vv_version = (Get-Content -LiteralPath 'product/desktop/package.json' -Raw | ConvertFrom-Json).version
$vv_payload = (Resolve-Path -LiteralPath 'product/desktop/src-tauri/offline').Path
$vv_cosyvoice_venv = (Resolve-Path -LiteralPath '<configured-cosyvoice-venv-root>').Path
$vv_voice_backends = (Resolve-Path -LiteralPath '<configured-complete-voice-backends-root>').Path
$vv_setup = (Resolve-Path -LiteralPath "product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_${vv_version}_x64-setup.exe").Path
$vv_output = Join-Path $vv_repo_root 'product/desktop/build_target/Current/offline_full'
$vv_iscc = (Resolve-Path -LiteralPath '<configured-Inno-Setup-7-ISCC.exe>').Path
$vv_7zip = (Resolve-Path -LiteralPath '<configured-7-Zip-7z.exe>').Path
$vv_oscdimg = (Resolve-Path -LiteralPath '<configured-Windows-ADK-oscdimg.exe>').Path

$vv_installer_args = @{
  PayloadDir = $vv_payload
  CosyVoiceVenvDir = $vv_cosyvoice_venv
  VoiceBackendsDir = $vv_voice_backends
  SetupExe = $vv_setup
  OutputDir = $vv_output
  AppVersion = $vv_version
  IsccPath = $vv_iscc
  SevenZipPath = $vv_7zip
  OscdimgPath = $vv_oscdimg
}

.\governance\scripts\build_offline_full_installer.ps1 @vv_installer_args -ValidateInputsOnly
```

Stop on any validation error and repair the exact input. Do not remove a required file check or point the build at user databases, subscriptions, playlists, library metadata, downloads, or media.

### 4. Run the representative installation-speed gate

```powershell
$vv_perf_evidence = Join-Path $vv_repo_root 'product/desktop/build_target/tool_artifacts/wp_runs/WP-0308/performance_fixture'
.\governance\scripts\test_offline_installer_performance.ps1 `
  -EvidenceDir $vv_perf_evidence `
  -IsccPath $vv_iscc `
  -SevenZipPath $vv_7zip
```

The external-archive fixture must use the governed 64 MiB bounded-solid policy, be at least 2x faster than the legacy raw-file fixture, and produce an identical output tree. The canonical 2026-08-24 result passed at 2.128x: 222.717 seconds raw median versus 104.677 seconds archive median, with identical output-tree SHA-256. Preserve the generated receipt and logs; a passing representative fixture does not replace the full-payload timing or clean-profile 30-minute proof.

### 5. Build the single ISO

```powershell
.\governance\scripts\build_offline_full_installer.ps1 @vv_installer_args
```

Use `-RefreshPayloadArchives` only when forcing regeneration of content-matched archives. Routine builds hash inputs, test cached archives, and reuse matching 64 MiB bounded-solid archives from `product/desktop/build_target/offline_archive_cache`. A cache entry created under the retired non-solid policy is not content-equivalent and must not be reused.

</topic>

<topic id="outputs-and-proof" status="active" version="v1" wp="WP-0308" updated_at="2026-08-24">

## Expected outputs

The managed output folder contains:

- `VoxVulgi_<version>_x64_offline_full.iso`: the sole public user-required download.
- `VoxVulgi_<version>_x64_offline_full.artifacts.json`: internal build receipt; not a second user download.
- `offline_full_build_<timestamp>_<version>.log`: internal durable build transcript with resolved tool provenance and phase output; not a public user download.

The ISO root must contain:

```text
Install_VoxVulgi.exe
README.txt
payload_manifest.json
payload/payload_tools.7z
payload/payload_models.7z
payload/payload_huggingface.7z
payload/payload_cosyvoice_venv.7z
payload/payload_voice_backends.7z
```

The build driver must test every archive, reject links and unsafe paths, compile the wrapper, create a UDF 1.02 ISO, independently list the ISO with full 7-Zip, verify every required path, reject `.bin` slices inside the ISO, and write a receipt with `user_required_download_count: 1`.

## Release proof gates

Do not publish until all are true:

- Desktop contract suite passes.
- PowerShell build scripts parse and execute without bypassed checks.
- Inno wrapper compiles with Inno Setup 7+.
- Representative speed fixture uses the governed 64 MiB bounded-solid archives and passes at >=2x with identical output-tree SHA-256. The canonical result is 2.128x (222.717 s raw median; 104.677 s archive median).
- Final ISO and archive SHA-256 values are recorded by the governed build.
- Independent ISO listing contains every expected root/archive entry and no legacy `.bin` slice.
- Clean-profile installation on the documented local-SSD reference machine completes in <=30 minutes with default security settings.
- Existing-install update preserves preferences, options, database, subscriptions, playlists, and library metadata.
- Fully offline first run completes import -> captions -> translate -> dub -> export with zero downloads.
- Installer failure/cancel paths leave a durable log under `%APPDATA%\com.voxvulgi.voxvulgi\diagnostics\installer`.
- A second wrapper run is rejected by the setup mutex before it can mutate a managed payload root.
- Setup registers the shipped VoxVulgi executable for Inno close-application handling before managed-root promotion; a locked runtime is a visible, logged failure and never licenses a partial replacement.
- Disk preflight logs the destination volume, available bytes, governed required bytes, and every archive-byte contribution before extraction starts. Insufficient capacity fails before the installed payload is changed.
- Each run uses generation-specific staging and backup paths. A persistent promotion journal records the generation and every managed-root transition; fixed shared `stage_current`/`backup_current` paths are forbidden.
- Promotion success means all four managed roots (`tools`, `models`, `cache/huggingface`, and `voice_backends`) committed. Any promotion failure rolls the whole generation back, including roots already promoted in that run.
- Successful, failed, and cancelled runs continuously update `installer_<version>_latest.log` and retain a timestamped final log in that directory.
- The final log records wrapper source/expected version, every named payload phase start and completion, core-installer launch and return, before/after installed state, observed registry version, install path, main-binary file version, verification result or failure reason, and terminal outcome.
- The wrapper refuses success unless the uninstall registry reports the expected version and the installed main binary exists with a matching file version.

The build is not complete merely because the wrapper compiled or the ISO file exists.

</topic>

<topic id="failure-recovery" status="active" version="v1" wp="WP-0308" updated_at="2026-08-24">

## Failure and recovery

- Missing Inno, 7-Zip, or Oscdimg: install/correct the named build prerequisite or pass its verified explicit path; rerun the same command.
- Payload validation failure: repair or refresh the canonical payload input; never skip required model, cache, wrapper, or real-file checks.
- Interrupted archive/ISO build: rerun the same build. Content-matched verified archives are reused; the PID-scoped ISO staging tree is bounded and cleaned by the driver.
- Cached archive corruption: the archive integrity test fails. Rerun with `-RefreshPayloadArchives` after verifying the source root.
- Retired archive policy in cache: a cache receipt that does not record the governed 64 MiB solid block is stale even when source content matches. Regenerate it with `-RefreshPayloadArchives`; do not relabel a non-solid archive as compliant.
- ISO content verification failure: do not publish. Preserve the log, correct staging/build inputs, and rebuild.
- Concurrent wrapper: the later run must stop at the setup mutex without extracting, promoting, deleting, or recovering payload files. Finish or cancel the owning run, inspect its terminal log, then start a new run. Never delete the journal, staging generation, or backup generation merely to bypass the mutex.
- Insufficient disk: preflight must stop before extraction and before any installed managed root is renamed. Free space on the reported destination volume or choose a supported location, then start a new run. Do not remove archive-byte contributions, lower the governed reserve, or rely on compression ratio as installed-size proof.
- Locked VoxVulgi runtime: allow Setup's registered close-application flow to close the shipped executable, then retry. If the managed roots remain locked, stop the run and inspect the logged runtime/promotion failure. Do not kill an unowned process, copy over a live runtime, or accept a partially refreshed payload.
- Interrupted extraction: the current generation-specific staging tree is disposable because no installed managed root has been promoted. On the next mutex-owning run, recovery may remove only the abandoned generation identified by the journal; it must preserve the installed roots.
- Interrupted or failed promotion: the persistent journal is canonical. Recovery must restore all four managed roots from the same generation's backups, including roots already promoted before the failure, before deleting that generation's staging/backup trees or beginning a new extraction. Missing backup/target combinations are a hard failure requiring log inspection; never guess which tree is newer.
- Installation error: collect `installer_<version>_latest.log` and the timestamped final log from `%APPDATA%\com.voxvulgi.voxvulgi\diagnostics\installer`; inspect the last `VV_INSTALLER_EVENT` rows for disk preflight, generation/journal identity, the active payload phase, promotion/rollback state, core handoff, observed versions/paths, verification result, failure reason, and terminal outcome.
- Core-installer failure after payload promotion: the core verification is the transaction postcondition. Restore all four payload roots from that same generation's backups, retain the core-failure and rollback terminal records, and leave the journal only when rollback itself cannot complete. Never restore from an unrelated generation. Repair the exact NSIS/version/lock cause and rerun the same full-offline ISO; the wrapper may report success only after the independent uninstall-registry and main-binary version checks pass and the payload transaction commits.
- Missing or mismatched post-install version: do not publish or retry blindly. Compare `expected_version`, `observed_registry_version`, `observed_binary_version`, `install_location`, and `main_binary` in `core_install_verification`; repair the core installer/version inputs and rebuild.
- Missing terminal verification evidence: treat the installer artifact as invalid even if the UI appeared to complete; rebuild from the governed script and repeat clean-profile and update-path proof.
- Abnormally slow installation: capture source/target disk type, active disk contention, current payload phase, elapsed time, and the durable installer log. Do not label multi-hour local-SSD installation normal.
- Failed durable-log copy: treat the run as unproven. Every checkpoint and final-copy call must check its result and emit a terminal persistence failure when possible; a silent `CopyFile` failure is not acceptable proof.
- Active installer or disk saturation: wait for the existing installer to finish or for the operator to stop it. Never terminate an operator-owned process without the required exact process-stop authorization.

</topic>

<topic id="maintenance" status="active" version="v1" wp="WP-0308" updated_at="2026-08-21">

## Manual maintenance rule

When installer architecture, inputs, commands, outputs, tools, proof gates, or recovery behavior changes, update this file in the same change. Keep startup files as short links to this manual and keep normative product requirements in their existing authority/spec sections. Do not create a second installer build guide.

</topic>
