---
file_id: WP-0316-refinement-v1
file_kind: refinement
updated_at: 2026-08-30
---

<topic id="operator-request-and-evidence" status="active" version="v1" wp="WP-0316" updated_at="2026-08-30">

# Operator request and verified evidence

- Deliver one offline ISO for non-technical users containing the already-working VoxVulgi app, all required tools, models, caches, and installers. Installation and the default workflow must require no downloads, terminals, pip, or manual model placement.
- Treat the repo as the build site and the installed product as an independent application. User data remains under `%APPDATA%\\com.voxvulgi.voxvulgi`; installer-managed runtime bytes must not depend on the repo, current directory, system `PATH`, or test fixtures.
- Retire `offline-installer-runtime/scripts/build_offline_release_fast.ps1`. Inspection on 2026-08-30 proved that it rebuilds the desktop, refreshes/exports/reconciles payloads, builds a validator, runs performance proof, and cleans attempt trees. Those actions violate the package-only authority in `offline-installer-runtime/GUIDE.md`.
- The 2026-08-28 and 2026-08-30 postmortems prove that release attempts repeatedly replaced packaging with product rebuild, dependency recertification, and ten warmup gates totaling about 8.23 hours. No passing public installer resulted.
- The installed app currently resolves dependency roots from the same roaming app-data root as durable user state. Existing tests also left dependency fixtures in the operator profile. The runtime marker records only a bundle ID and does not prove the selected generation or component hashes.
- The existing archive cache contains reusable component candidates, but its bounded-solid metadata conflicts with the current guide's non-solid requirement and therefore is not automatically qualified for publication.

</topic>

<topic id="research-basis" status="active" version="v1" wp="WP-0316" updated_at="2026-08-30">

# Research basis and selected design

- Python's official `venv` documentation states that environments are inherently non-portable because installed scripts contain absolute interpreter paths; moved environments should be recreated. The Windows embeddable distribution is designed to be part of another application and is isolated from the system. Therefore the public runtime must use prequalified self-contained Python layouts or prove relocation at qualification time; the installer must never create or repair a venv.
- Microsoft Windows application-data guidance and Tauri's `app_local_data_dir` contract place large machine-local data in Local AppData. Therefore immutable runtime generations live under `%LOCALAPPDATA%\\com.voxvulgi.voxvulgi\\runtime`, while databases, preferences, library metadata, secrets, and operator configuration remain under `%APPDATA%\\com.voxvulgi.voxvulgi`.
- Inno Setup's official archive documentation supports external archive extraction with hashes and warns that solid archives impair selective extraction. Therefore qualified components are reusable non-solid archives, independently hashed and consumed directly from the ISO.
- Squirrel.Windows' source documentation installs new versions into versioned Local AppData directories and switches the selected version only after verification. Electron updater source rejects artifacts without checksums. Therefore VoxVulgi uses immutable generation directories plus a hash-bound `current.json` activation pointer; incomplete or corrupt selected generations fail closed instead of falling back.
- The package-only release entrypoint consumes an explicit qualified-runtime manifest and an explicit already-built core installer, verifies their hashes, builds only the wrapper/manifest/ISO, tests that exact ISO offline, and publishes only the passing ISO hash. It does not invoke Cargo, npm, Tauri, pip, model downloads, runtime warmups, payload repair, or product version mutation.
- Tauri's official NSIS source/config supports `zlib` as the quick compression mode and its standard maintenance flow keeps same-version reinstall inside the installer process. NSIS's official `FileOpen` reference states that append mode preserves contents but positions at byte zero, requiring `FileSeek ... END` before each append. Therefore VoxVulgi reinstall actions use one CRC-checked installer process, zlib packaging, deterministic passive selection, and an explicitly seeked durable phase log.

# Rejected approaches

- Rebuilding or recertifying the app/runtime during installer creation: it changes the input set, invalidates reuse, and caused the observed multi-hour loops.
- Treating an ordinary venv copied from the repo or another installation as a portable product runtime: Python explicitly does not guarantee this.
- Extracting dependencies into roaming user data: it couples replaceable runtime state to irreplaceable preferences/library state and allows tests to pollute production roots.
- Falling back to the repo, current directory, system `PATH`, or downloads when a managed runtime is selected: it can make an incomplete installer appear healthy on a developer machine.
- Mutating a selected runtime generation in place: interruption can leave the only runtime half-updated and makes rollback ambiguous.

</topic>

<topic id="risks-and-controls" status="active" version="v1" wp="WP-0316" updated_at="2026-08-30">

# Red team, risks, and controls

- A corrupt or partially extracted generation is selected. Control: extract to a new generation, verify manifest/component hashes and required executables, then atomically replace `current.json`; retain the previous pointer for rollback. Verify interruption before extraction, before activation, and after activation.
- A developer machine masks missing payload bytes. Control: managed mode forbids repo/current-directory/PATH/download fallback. Verify with poisoned PATH, unavailable repo, and blocked network.
- Tests touch the operator profile. Control: every installer, qualification, performance, and app-boundary test requires an explicit disposable root; guard against canonical APPDATA/LocalAppData roots and aliases. Verify sentinel preservation.
- Python starts but later native imports fail after relocation. Control: qualification must run representative imports and the default offline workflow from a relocated root under blocked network; packaging may reuse only its passing manifest/hash.
- App and runtime generations are incompatible. Control: runtime manifest records compatible app-version bounds and component identities; startup rejects an incompatible selected generation with actionable diagnostics.
- Update damages user data. Control: installer writes only the new LocalAppData runtime generation, activation pointer, program files, and installer logs during keep-actions. Verify database/preferences/library metadata hashes before and after `Update` and `Reinstall (keep preferences and options)`.
- Packaging again becomes a build pipeline. Control: source and process-contract tests forbid product build tools and network/runtime-repair commands in the package entrypoint. Verify transcript contains only input verification, wrapper/ISO construction, offline acceptance, and publication.
- Large archives are rewritten unnecessarily. Control: content-address qualified component archives separately; a package run reuses an archive when its source identity and qualification receipt match. Verify a second identical package run leaves archive hashes and timestamps unchanged.
- Reinstall removes the old app and then fails or appears stalled before the parent installer resumes. Control: `Reinstall (keep preferences and options)` and `Full reinstall` replace managed program files inside the already-running CRC-checked installer; only uninstall-only actions invoke the prior uninstaller. Use zlib and durable phase logging, then verify the exact setup with `/P /VVMAINTENANCE=reinstall_keep` while hashing preserved data.

</topic>

<topic id="acceptance" status="active" version="v1" wp="WP-0316" updated_at="2026-08-30">

# Acceptance surface

- The app exposes separate roaming user-data and local managed-runtime roots, immutable selected generation identity, manifest hash/status, compatibility, and fallback policy in Diagnostics and `GET /agent/state` without adding a card.
- Managed mode resolves tools, models, Hugging Face cache, voice backends, and both Python runtimes only from the selected generation and never silently falls back or downloads.
- Qualification produces reusable non-solid component archives plus a machine-readable runtime manifest and independent offline relocation/default-workflow proof.
- Packaging produces exactly one UDF ISO with root `Install_VoxVulgi.exe`, README, release manifest, qualified-runtime manifest, and component archives. It does not rebuild or modify product/runtime inputs or bump the app version.
- The exact ISO hash passes clean-profile blocked-network install, first-run default localization, update, reinstall-keep, rollback/interruption, long-path, nontechnical UX, and user-data-preservation checks before publication.
- WP state records every attempt, input identity, failure phase, diagnosis, and next action. WP-0265 and WP-0308 remain historical and are superseded, not reused as active procedure.

</topic>
