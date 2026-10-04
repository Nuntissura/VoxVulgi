# VoxVulgi full-offline installer

## Repository placement

`offline-installer-runtime/` is the offline-installer worktree. It is a top-level sibling of `product/` and `governance/`.

Installer scripts, minimal recovery state, temporary packaging files, payload archives, `Install_VoxVulgi.exe`, and `simple-offline-installer.iso` stay under `offline-installer-runtime/`. Files under `product/` are read-only packaging inputs.

## Definition

A VoxVulgi full-offline installer is one ISO named `simple-offline-installer.iso` containing:

```text
Install_VoxVulgi.exe
README.txt
release_manifest.json
payload/
  runtime_manifest.json
  payload_tools.7z
  payload_models.7z
  payload_huggingface.7z
  payload_voice_backends.7z
```

The user launches only `Install_VoxVulgi.exe`. The payload archives are internal ISO files; the user does not extract, configure, or manage them.

The installer packages the existing working VoxVulgi app/core setup and the existing working dependency directories. Packaging does not download, rebuild, reinstall, regenerate, or reconstruct the app or dependencies.

The ISO includes the app, models, Hugging Face cache, Python runtimes and environments, FFmpeg, providers, voice backends, and every dependency used from a default path.

Installation and first launch require no network access, downloads, terminal, PowerShell, pip, Python setup, package manager, developer tool, or manual path configuration.

## Authoritative contract

- [VV-INSTALL-001] The offline-installer task packages the existing selected VoxVulgi app/core, one already-qualified runtime artifact, installer logic, and required offline tooling into one full-offline ISO; packaging does not create, repair, upgrade, or requalify those inputs.
- [VV-INSTALL-002] Every app/core and dependency input is read-only during packaging. Packaging must not modify files under `product/`, an installed VoxVulgi runtime, a dependency environment, a model/cache root, or retained user data.
- [VV-INSTALL-003] Packaging must not launch VoxVulgi, launch the core installer, import a model package, execute a dependency, run a model warmup, start a provider, create an environment, or probe runtime health. Runtime behavior is tested only after a complete candidate ISO exists.
- [VV-INSTALL-004] Packaging must not run Cargo, Tauri, npm, pip, Python, Git, Git LFS, Hugging Face, ModelScope, a downloader, a dependency installer, archive creation, runtime qualification, or a product build. The only executable tooling allowed during packaging is bounded file hashing/copying/hard-linking, 7-Zip archive testing/listing, Inno Setup compilation, ISO creation/listing, and minimum read-only inspection.
- [VV-INSTALL-005] Packaging consumes explicit existing input paths and their recorded identities. If an input is absent, unusable, internally inconsistent, or not the intended working input, packaging stops with the exact input blocker; it must not download, rebuild, reinstall, reconcile, or substitute that input.
- [VV-INSTALL-006] A packaging retry does not increment the VoxVulgi product version, installer version, or changelog. Failed packaging, failed acceptance, and failed publication are not releases and consume no version. Any intentional product-version change must be completed separately before packaging begins.
- [VV-INSTALL-007] A packaging retry always reuses the qualified component archives. It may reuse a wrapper or candidate ISO whose complete recorded input identity and output hash still match; freshness alone is not a reason to rebuild a valid output.
- [VV-INSTALL-008] Candidate construction precedes runtime acceptance. Clean-profile installation, offline launch, dependency-health checks, workflow proof, update proof, and retained-data proof may begin only after the exact candidate wrapper and ISO exist and their hashes have been recorded.
- [VV-INSTALL-009] Candidate failure and release failure are distinct. A complete candidate that has not passed offline acceptance is retained and labeled `CANDIDATE_NOT_RELEASED`; it must not be published or described as working, release-proven, or delivered.
- [VV-INSTALL-010] Publication is permitted only for the exact candidate hash that passed clean-profile install, offline required-workflow proof, existing-install update, retained-data preservation, zero-download verification, and independent artifact-identity verification.
- [VV-INSTALL-011] Packaging and acceptance must remain quiet and background-safe until a documented acceptance step requires foreground installer or app interaction. Foreground interaction must be announced before it begins and must not occur while the operator is using the computer unless the operator explicitly authorizes it.
- [VV-INSTALL-012] The working ISO is the primary deliverable. Logs, receipts, state, hashes, tests, archives, and the wrapper are supporting artifacts and do not count as delivery without the accepted ISO.
- [VV-INSTALL-022] Before qualifying changed installer inputs, check the latest published yt-dlp release through the existing VoxVulgi download-engine update/selection workflow and select that release; reuse the current selected binary only when its version and SHA-256 already match that release.
- [VV-INSTALL-023] Before qualifying changed installer inputs, run one bounded compatibility probe through VoxVulgi's real selected-engine resolver and adapter using the exact selected yt-dlp binary; the probe must bind the exact version and SHA-256, run `--version`, and complete one no-network local-fixture parse invocation. A direct `yt-dlp --version` call, fixture executable, mock, or raw executable probe is not sufficient.
- [VV-INSTALL-024] Only the exact yt-dlp binary identity that passed [VV-INSTALL-023] may enter qualification. Phase Q packages it with the rest of the existing tool tree in `payload_tools.7z`; do not create a separate archive, installer phase, or packaging architecture for yt-dlp.
- [VV-INSTALL-025] Phase A remains package-only and offline-input-only: it must neither check for nor download an yt-dlp update. A packaging retry reuses the qualified yt-dlp bytes when their recorded identity is unchanged.

## Installer behavior

- For a new installation, install the app and extract the bundled dependencies into an immutable runtime generation under `%LOCALAPPDATA%\com.voxvulgi.voxvulgi\runtime\generations\<runtime_id>`.
- For an existing installation, show the pre-maintenance explainer and these exact actions:
  - `Update`
  - `Reinstall (keep preferences and options)`
  - `Full reinstall`
  - `Uninstall (keep preferences and options)`
  - `Full uninstall`
- `Update` installs a new immutable runtime generation and atomically changes the hash-bound `current.json` pointer only after the generation and core app pass installer checks.
- `Update`, `Reinstall (keep preferences and options)`, and `Uninstall (keep preferences and options)` preserve `%APPDATA%\com.voxvulgi.voxvulgi`, including settings, options, database, subscriptions, playlists, library metadata, and other retained state.
- Only `Full reinstall` and `Full uninstall` may remove retained app data.
- The installed app loads dependencies only from the selected managed generation, not from the repo, current directory, system `PATH`, roaming user-data tree, or downloads.
- Detect and close a running VoxVulgi instance before replacing managed files.
- If installation or update fails, restore the previous managed files and leave retained user data untouched.
- Never modify, rename, overwrite, delete, or repurpose another installer.

## What this work is not

- It is not a clean-room app or dependency build.
- It is not a downloader or dependency-acquisition pipeline.
- It does not run Cargo, Tauri, npm, pip, Git, Git LFS, Hugging Face, ModelScope, model warmups, or environment creation.
- It is not a dependency upgrade, reproducibility project, governance project, reporting project, or proof-bundle project.
- Caches, archives, logs, state files, and partial outputs are not the deliverable. The working ISO is the deliverable.

## Input boundary

Runtime qualification and ISO packaging are separate commands. Qualification may execute only its explicit source runtimes to prove relocation and imports; packaging never executes them.

### yt-dlp input selection gate

Run this gate once while preparing changed inputs for Phase Q, not during Phase A and not for a packaging retry whose qualified input identity remains unchanged:

1. Use VoxVulgi's existing download-engine update/selection workflow to check the latest published yt-dlp release.
2. Select the latest release. Reuse an already-selected yt-dlp only when its reported version, byte size, and SHA-256 match the release identity; otherwise let the normal VoxVulgi engine workflow stage and verify it outside the qualified-runtime and package-candidate directories.
3. Record the exact selected executable path, version, byte size, and SHA-256.
4. Through VoxVulgi, select that exact executable and run one bounded compatibility probe through the same selected-engine resolver and yt-dlp adapter used by ordinary jobs. The probe must run the engine's `--version` operation and one no-network local-fixture parse invocation; it must not download media or contact a provider.
5. Pass only when the probe receipt identifies the exact version and SHA-256 from step 3, both operations complete successfully, and the adapter returns the expected structured local-fixture result. Do not accept `--version` alone, direct executable invocation outside VoxVulgi, a fixture executable, a mock, or a different binary as proof.
6. If the bounded probe fails, stop and keep the prior qualified runtime. Do not package the failed yt-dlp candidate.
7. After the test passes, run Phase Q normally. The selected yt-dlp stays in the existing tool tree and is included in `payload_tools.7z` with the rest; there is no separate yt-dlp archive or installer workflow.

Before qualification, resolve and record:

1. The existing tool/runtime input tree, including portable Python and the working primary environment.
2. The existing model tree.
3. The existing Hugging Face cache tree.
4. The existing working CosyVoice environment.
5. The existing voice-backend tree.

Qualification converts the two existing Python environments into isolated self-contained `runtime_main` and `runtime_cosyvoice` layouts, runs the named import probes with offline environment flags, produces four reusable non-solid archives, and writes `runtime_manifest.json` plus `qualification_receipt.json`. It must stop on any failed relocation/import check; it must not download, install, repair, or silently substitute a dependency.

Before packaging, resolve and record only:

1. The selected already-built VoxVulgi app/core setup.
2. The selected passing qualified-runtime directory and receipt.
3. The installer source and explicit Inno Setup, 7-Zip, and Oscdimg executables.

For each input, record its resolved path, input-tree identity, size, and file count. The input-tree identity must be deterministic over relative path, file size, and file SHA-256. Machine-local source paths belong only in local recovery state and receipts; they must not be hardcoded into product code or portable installer logic.

Existing qualification evidence may be referenced when its bound input identity still matches, but packaging does not require fresh qualification. A changed input invalidates only that input's prior identity and outputs derived from it.

## Packaging method

- Use the current 64-bit Inno Setup 7 release.
- Embed the existing working app/core setup in `Install_VoxVulgi.exe`.
- Keep the four qualified dependency archives and `runtime_manifest.json` under the ISO's `payload/` folder.
- Verify every external archive and the runtime manifest by exact byte count and SHA-256 before bulk extraction.
- Set `ArchiveExtraction=enhanced/nopassword` for normal memory use with large `.7z` contents.
- Use non-solid `.7z` archives. The official Inno documentation advises against solid archives for `extractarchive` because they can reduce extraction performance.
- Keep a stable Inno `AppId`; close only exact-path VoxVulgi-owned runtimes before transactional promotion.
- Create the ISO with `oscdimg -u2 -udfver102 -m`.
- Archive creation belongs only to qualification. Packaging tests and lists the existing archives but never recreates them.
- Derive wrapper and ISO rebuild decisions from recorded input identities, not from timestamps, attempt nonces, or a requirement that every attempt be fresh.
- Keep the app/core version already embedded in the selected app/core input. Packaging must not edit version-bearing source files.

Official implementation references:

- [Inno Setup downloads](https://jrsoftware.org/isdl.php)
- [Inno Setup external archive extraction and SHA-256 Hash](https://jrsoftware.org/ishelp/topic_filessection.htm)
- [Inno Setup archive extraction modes](https://jrsoftware.org/ishelp/topic_setup_archiveextraction.htm)
- [Inno Setup AppId](https://jrsoftware.org/ishelp/topic_setup_appid.htm)
- [Inno Setup CloseApplications](https://jrsoftware.org/ishelp/topic_setup_closeapplications.htm)
- [Microsoft Oscdimg options](https://learn.microsoft.com/en-us/windows-hardware/manufacture/desktop/oscdimg-command-line-options?view=windows-11)

## Minimal recovery state

Use only this packaging workspace:

```text
offline-installer-runtime/
  qualified_runtime/
    runtime_<content-id>/
      qualification_receipt.json
      runtime_manifest.json
      payload/
        runtime_manifest.json
        payload_tools.7z
        payload_models.7z
        payload_huggingface.7z
        payload_voice_backends.7z
  package_candidates/
    candidate_<app-version>_<runtime-id>/
      simple-offline-installer.iso
      candidate_receipt.json
```

The qualification and candidate receipts contain only:

- Input-tree identity and output hash for each qualified archive.
- Input hash and output hash for `Install_VoxVulgi.exe`.
- Input hash and output hash for `simple-offline-installer.iso`.
- The most recent terminal error, if any.
- Candidate state: `PACKAGING`, `CANDIDATE_NOT_RELEASED`, `ACCEPTANCE_PASSED`, or `PUBLISHED`.
- The exact candidate ISO hash bound to acceptance, when acceptance has begun.

Recovery rules:

1. Qualification writes to a bounded staging directory and renames it to its content-addressed final directory only after its tests and receipt pass.
2. Packaging hard-links the four immutable archives into a bounded ISO stage, builds the wrapper and ISO, and records the candidate hash.
3. On retry, reuse a qualified runtime when its deterministic input identity and receipt still match.
4. Requalify only after a source input changes or qualification proof fails; packaging never requalifies.
5. On failure, record the terminal error and remove only the current partial output.
6. Never delete a valid completed archive, wrapper, or ISO merely because a later step failed.
7. Never delete prior receipts, status, or logs required to determine whether a completed output can be reused.
8. Never clean another attempt's completed outputs at the start of a new attempt.
9. Keep partial-output cleanup limited to the current attempt's exact `partial/` path.
10. A later acceptance failure retains the exact candidate and records `CANDIDATE_NOT_RELEASED` plus the terminal blocker.

## Package and release workflow

The only authorized qualification and packaging commands are the two explicit entrypoints below. The separate input-preparation freeze command only prepares immutable inputs; it does not create an installer. A filename never overrides this contract.

### Current implementation conformity gate

`build_offline_release_fast.ps1` was deleted on 2026-08-30 as an **invalid, unwanted stale artifact** because it invoked the desktop build with offline-payload refresh, triggered version increment, ran dependency warmups, downloaded/installed dependencies, created fresh environments, required fresh payload generation, and performed destructive prior-attempt cleanup.

`build_offline_full_installer.ps1`, `prep_offline_bundle.ps1`, and `reconcile_offline_python_environments.ps1` are retained only as **legacy nonconforming recovery/history artifacts**. They are not authorized installer-creation commands because they combine qualification, repair, archive construction, acceptance, or publication with packaging.

### Input preparation: freeze existing validated payload inputs

[VV-OFFLINE-PREP-001] Input acquisition is separate from Phase Q and packaging. For a targeted CosyVoice repair, use the product `voxvulgi_setup --base-dir <explicit-absolute-mutable-base> --install-cosyvoice` entrypoint after its owning validation passes; preserve prepared caches, qualified generations, installed runtimes and user data. The actual product model graph/readiness and dependency identity must pass before calling that input working.

[VV-OFFLINE-PREP-002] `governance/scripts/freeze_prepared_payload.ps1` is the authorized narrow immutable-input producer. It only copies and hashes five explicit existing source roots; it never downloads, installs, executes Python/models/apps, builds, repairs, archives or publishes a release. Freezing alone makes no working-runtime claim.

```powershell
pwsh -NoProfile -File governance/scripts/freeze_prepared_payload.ps1 `
  -ToolsDir <existing-tools-dir> -ModelsDir <existing-models-dir> `
  -HuggingFaceDir <existing-huggingface-dir> `
  -CosyVoiceVenvDir <existing-validated-cosyvoice-venv> `
  -VoiceBackendsDir <existing-voice-backends-dir> `
  -PreparedParent product/desktop/build_target/offline_payload_cache/prepared
```

[VV-OFFLINE-PREP-003] The producer refuses linked inputs/output ancestors, source/output overlap and overwriting any deterministic `prepared_<contract-hash>` output. Tools exclude exactly `python/venv_cosyvoice`; the independent CosyVoice source populates that destination. Source-before/source-after and copied-destination identities must agree before atomic same-parent publication of `immutable_cache.json`, with uppercase SHA-256 identities and read-only copied files. Failed staging is retained, never represented as published input.

[VV-OFFLINE-PREP-004] Reuse unchanged main Python/tools, models, Hugging Face and voice-backend inputs through their explicit source roots; freezing does not modify those sources or prior prepared outputs. Phase Q consumes the resulting canonical prepared receipt and retains its actual import/model-graph qualification gates. Final exact-ISO offline acceptance and publication gates remain unchanged.

### Phase Q — qualify an existing working runtime when its inputs changed

```powershell
pwsh -NoProfile -File offline-installer-runtime/scripts/qualify_offline_runtime.ps1 `
  -PayloadDir <existing_payload_dir> `
  -CosyVoiceVenvDir <existing_cosyvoice_venv_dir> `
  -VoiceBackendsDir <existing_voice_backends_dir> `
  -SelectedYtDlpPath <exact_voxvulgi_probe_passing_engine.exe> `
  -SelectedYtDlpVersion <probe_receipt_version> `
  -SelectedYtDlpSha256 <probe_receipt_sha256> `
  -PreparedPayloadReceipt <prepared_payload_immutable_cache.json> `
  -AppVersion <already_built_app_version> `
  -OutputRoot <qualified_runtime_output_root> `
  -SevenZipPath <7z.exe>
```

Do not run Phase Q for an unchanged passing input identity. This phase is deliberately separate because hashing, copying, self-contained-Python assembly, import proof, and compression are the expensive work.

### Phase A — package existing inputs

1. Acquire one packaging lock without stopping or disturbing unrelated processes.
2. Load `state.json` and inspect existing final outputs before creating or deleting anything.
3. Resolve the explicit existing app/core and selected qualified-runtime directory.
4. Verify the qualification receipt, runtime manifest, four archives, exact byte sizes, hashes, integrity, and non-solid property using read-only inspection.
5. Stop with the exact blocker if an input is missing or cannot be read. Do not repair or replace it.
6. Reuse all four qualified archives without changing their timestamps or hashes.
7. Compile `Install_VoxVulgi.exe` from the selected existing app/core setup and qualified manifest/archive contracts without launching the embedded setup.
8. Assemble the ISO tree with archive hard links, create the ISO, and record its exact hash in the candidate receipt.
9. Independently list the candidate ISO and verify exact member names and hashes before acceptance.
10. Record the exact candidate ISO hash and set state to `CANDIDATE_NOT_RELEASED`.

Canonical Phase A command:

```powershell
pwsh -NoProfile -File offline-installer-runtime/scripts/package_offline_release.ps1 `
  -SetupExe <already_built_core_setup.exe> `
  -QualifiedRuntimeDir <passing_runtime_content_directory> `
  -OutputDir <package_candidate_output_root> `
  -AppVersion <already_built_app_version> `
  -IsccPath <ISCC.exe> `
  -SevenZipPath <7z.exe> `
  -OscdimgPath <oscdimg.exe>
```

Phase A success means a complete, integrity-checked candidate exists. It does not mean the installer works or may be published. Test exact ISO hash in Phase B; bind receipts/source hashes to that same entity.

### Phase B — accept the exact candidate offline

1. Use a controlled clean standard-user profile and isolate network access before installation begins.
2. Mount the exact candidate ISO and independently verify its hash matches the Phase A candidate hash.
3. Run `Install_VoxVulgi.exe` from the mounted ISO and prove a clean install completes without downloads.
4. Launch the installed app and prove required bundled dependency availability without acquiring or repairing dependencies.
5. Prove the required offline workflow: `import -> captions -> translate -> dub -> export`.
6. Create or use retained preferences, subscriptions, playlists, database content, and library metadata appropriate to the acceptance fixture.
7. Run the exact candidate's existing-install `Update` path and prove retained data remains present and usable.
8. Record zero-download/network-isolation evidence and the installed app/core identity.
9. Independently re-read the installed marker, candidate ISO hash, wrapper hash, and archive hashes.
10. Set state to `ACCEPTANCE_PASSED` only when every required proof is true for the same candidate hash.

If Phase B fails, retain the candidate as `CANDIDATE_NOT_RELEASED`, record the exact blocker, and return to Phase A only when the blocker requires a packaging-input or packaging-logic change. Do not rebuild unchanged archives or bump a version merely to retry acceptance.

### Phase C — publish

1. Reconfirm the candidate hash still matches the `ACCEPTANCE_PASSED` receipt.
2. Atomically publish ISO+receipt by copying the exact accepted pair to `product/desktop/build_target/Current/offline_full`.
3. Independently hash the published copies and compare them to the accepted identities.
4. Set state to `PUBLISHED` only after the independent publication read succeeds.
5. Deliver one UDF ISO. The ISO is the sole public artifact; the receipt remains internal release evidence.

Sole public artifact is the one ISO; qualification directories, archives, manifests, and receipts remain internal build/release evidence.

No candidate may be copied, renamed, or described as released before the exact mounted-ISO runtime proof and atomic publication complete.

## Validation ordering

- [VV-INSTALL-013] During Phase A, use the narrowest file-level check that can expose the current packaging failure. Do not run runtime acceptance while packaging is failing.
- [VV-INSTALL-014] Do not rerun a green archive, wrapper, or ISO integrity check after an unrelated input remains unchanged; reuse its recorded proof.
- [VV-INSTALL-015] Performance benchmarking is not a predecessor to candidate construction. Run it separately only when packaging performance changed or an explicit acceptance criterion requires fresh performance proof.
- [VV-INSTALL-016] Dependency qualification and warmups are not installer-packaging validation. Run them in their owning product/dependency workflow only when those inputs changed.
- [VV-INSTALL-017] Phase B must test the exact candidate entity reported by Phase A, not a copied, rebuilt, renamed, or newly generated substitute.
- [VV-INSTALL-018] A passing mock, fixture-only test, source inspection, or self-authored receipt cannot replace mounted-ISO clean-profile proof.

## Version and changelog rules

- [VV-INSTALL-019] Phase A reads the selected app/core version and embeds or references it; Phase A does not write `package.json`, `tauri.conf.json`, `Cargo.toml`, lockfiles, product source, or `BUILD_CHANGELOG.md`.
- [VV-INSTALL-020] Phase B and Phase C do not change the product version. A packaging or acceptance fix that does not change the selected app/core binary keeps the same version and invalidates only the outputs affected by that fix.
- [VV-INSTALL-021] Record publication in the changelog only after Phase C succeeds, and do not represent failed attempts as released versions.

## Checklist

- [ ] Existing working app/core setup identified.
- [ ] Five existing working source input sets identified for qualification.
- [ ] Latest published yt-dlp release identity checked through VoxVulgi before changed-input qualification.
- [ ] One bounded VoxVulgi resolver/adapter probe bound the exact selected yt-dlp version and SHA-256, passed `--version` plus the no-network local-fixture parse, and that exact binary is present in the source tool tree.
- [ ] yt-dlp remains inside the existing `payload_tools.7z`; no separate archive, packaging phase, or installer architecture was introduced.
- [ ] Deterministic identities recorded for every existing input without executing it.
- [ ] Packaging invoked no app/core build, dependency build, downloader, package manager, environment creator, runtime health probe, provider, or model tool.
- [ ] Packaging launched neither VoxVulgi nor the embedded core installer.
- [ ] Product version files and `BUILD_CHANGELOG.md` were not changed by packaging or a failed attempt.
- [ ] Four matching qualified archives and one hash-bound runtime manifest exist; unchanged valid archives were reused.
- [ ] `Install_VoxVulgi.exe` exists at the ISO root.
- [ ] `simple-offline-installer.iso` exists.
- [ ] Candidate ISO identity and internal member hashes passed read-only integrity checks before runtime acceptance.
- [ ] Candidate remained labeled `CANDIDATE_NOT_RELEASED` until mounted-ISO acceptance passed.
- [ ] Offline existing-install `Update` succeeds.
- [ ] VoxVulgi launches with all bundled dependencies healthy and zero downloads.
- [ ] Settings, options, database, subscriptions, playlists, and library metadata remain present and usable.
- [ ] Exact ISO hash passed clean install, update, offline workflow, and zero-download proof.
- [ ] Published ISO+receipt is under `Current/offline_full`; only the ISO is handed to users.
- [ ] Valid completed outputs and recovery evidence survived any later-step failure.
- [ ] No other installer was modified, renamed, overwritten, deleted, or repurposed.
