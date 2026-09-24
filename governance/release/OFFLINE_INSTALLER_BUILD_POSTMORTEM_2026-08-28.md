---
file_id: PM-OFFLINE-INSTALLER-2026-08-28
file_kind: postmortem
updated_at: 2026-08-28
status: corrected
scope: offline-installer-release
---

<topic id="historical-entrypoint-warning" status="active" updated_at="2026-08-30">

> Every reference below to `build_offline_release_fast.ps1` describes the failed/nonconforming lineage. The script was deleted on 2026-08-30 as an invalid, unwanted stale artifact and must not be recreated or run; WP-0316 and `offline-installer-runtime/GUIDE.md` own the replacement.

</topic>

<topic id="incident-summary" status="corrected" summary="Why delivery took more than one working day and the actual outcome" updated_at="2026-08-28">

## Impact

The requested batteries-included offline installer was not available during the operator-reported 12+ hour period. Repeated canonical release attempts stopped during payload preparation, before archive and ISO packaging. Packaging eventually completed, but the first mounted `0.1.183` installer failed its functional postcondition and was correctly rolled back. No corrected installer is currently built.

## Artifact status

- Staged artifact: [simple-offline-installer.iso](../../product/desktop/build_target/offline_installer_staging/candidate_0.1.183_41856_9d6464d47f7f4cd0b350991bc2ce461c/delivery/simple-offline-installer.iso)
- Size: `7,541,039,104` bytes
- SHA-256: `670B51961BC3F74093B14E1A279DE625738650256635EDFF5A96A03CF5CECFD9`
- Release version: `0.1.183`
- Functional status: **invalid / not releasable**. The mounted `N:\Install_VoxVulgi.exe` is the broken `0.1.183` root installer; its SHA-256 is `7CFB22DA40777746F233E3751F0738D6FC636A2F2F5D0659A2EA6427596623A2`, matching the staged ISO root installer.
- The source-side marker support exists only as uncommitted working-tree changes. There is no corrected installer build to distribute.

</topic>

<topic id="timeline" status="corrected" summary="Observed sequence of failures, recovery, and invalid artifact discovery" updated_at="2026-08-28">

1. The canonical command from `offline-installer-runtime/GUIDE.md` was used for the release attempts.
2. Payload preparation failed on CosyVoice/Wetext readiness and download checks, so packaging could not start.
3. A failed attempt left an owned wrapper process/lock active; the next retry was correctly refused until that exact task-owned process was stopped.
4. After the runtime checks were corrected, the wrapper failed while reading the preparation result because progress text and the structured result were mixed on stdout.
5. The wrapper was changed to select the structured dictionary result explicitly.
6. The canonical builder completed payload archive creation, Inno Setup compilation, and UDF ISO assembly.
7. The mounted `N:\Install_VoxVulgi.exe` ran its embedded core installer. All five payload archives verified and extracted; the core returned exit code `0`.
8. The wrapper observed an empty `OfflineInstallGeneration` marker, treated the success state as invalid, quarantined/rolled back the core and all promoted payload roots, cleared the transaction journal, and ended with `outcome=failure`.
9. The source marker support was added afterward, but no rebuilt corrected installer exists.

</topic>

<topic id="root-causes" status="complete" summary="Technical causes established from source and run evidence" updated_at="2026-08-28">

### 1. ModelScope metadata/download contract was wrong

The implementation required every downloaded file's ModelScope `Revision` metadata to equal the requested snapshot revision. ModelScope exposes a file's last-touch commit in that field, so this strict equality rejected valid files. The legacy metadata path also returned intermittent `403`/rate-limit/endpoint failures.

**Correction:** use the official pinned ModelScope resolve URL for snapshot `b04bc07588601f7619b20efbb01cd1fa7278ccbc`, browser-like headers, and bounded retries across the documented alternate hosts/statuses.

### 2. CosyVoice CPU readiness probe matched the wrong Python syntax

The probe searched for `cuda = None`, while the actual PyTorch CPU `torch/version.py` used the annotated form `cuda: Optional[str] = None`.

**Correction:** accept both the unannotated and annotated assignment forms.

### 3. The readiness probe contradicted its own locked environment

The probe rejected any standalone `wheel` distribution, while the generated lock and prepared environment intentionally contained `wheel==0.48.0`.

**Correction:** require the locked `wheel` version and the expected local-venv placement instead of rejecting the package.

### 4. The PowerShell wrapper had an untyped return boundary

`Get-OrCreatePreparedCache` wrote human-readable progress strings and then returned an ordered dictionary. The caller assumed the captured pipeline value was always the dictionary; under strict mode, `$prepared.stage` failed when the first captured object was a string.

**Correction:** filter the captured output to `Collections.IDictionary` and fail explicitly if no structured cache record is returned.

### 5. The packaged core installer did not satisfy the marker postcondition

The background installer log records `core_installer_return exit_code=0`, followed immediately by `core_rollback_start reason="The core installer did not record the fresh offline generation."` The embedded `0.1.183` artifact therefore reported process success without producing the registry state required by the wrapper. Marker support was present only in uncommitted source after this artifact had been built; source state was not independently tied to the mounted executable before distribution.

**Required correction:** rebuild the entire governed installer after marker support is included, then run the clean-profile installed-app acceptance test against that exact rebuilt artifact.

</topic>

<topic id="contributing-factors" status="complete" summary="Process and observability conditions that multiplied the impact" updated_at="2026-08-28">

- The full offline payload is multi-gigabyte and contains tens of thousands of files. Each failed isolated stage repeated expensive download, environment, hashing, and archive work.
- Progress output from large 7z operations was excessively verbose, which made the actual failing phase harder to see.
- The generic readiness failure did not initially expose enough subprocess stdout/stderr to identify which assertion failed.
- There were no focused regression tests for annotated `torch/version.py` syntax, the wheel-lock/probe contract, or the PowerShell structured-return boundary.
- A failed wrapper process left a retry lock. The lock protected concurrent releases, but the recovery path was manual and the failure looked like another build problem.
- The successful artifact was only produced after several independent defects were corrected; the work was not one single slow build.

</topic>

<topic id="what-worked" status="partial" summary="Controls that caught the bad installer and protected rollback" updated_at="2026-08-28">

- The canonical offline-installer guide and its prescribed fast builder remained the workflow source of truth.
- Isolated staging and atomic promotion prevented incomplete payloads from being mistaken for a valid prepared cache.
- Pin and integrity checks remained enabled for the prepared payload.
- Online research was used when current ModelScope and Setuptools behavior differed from the assumptions in the code. Sources included the [Wetext ModelScope page](https://modelscope.cn/models/pengzhendong/wetext), [ModelScope downloader source](https://github.com/modelscope/modelscope/blob/master/modelscope/hub/file_download.py), [ModelScope API source](https://github.com/modelscope/modelscope/blob/master/modelscope/hub/api.py), and official [Setuptools `pkg_resources` deprecation guidance](https://setuptools.pypa.io/en/stable/deprecated/pkg_resources.html).
- The mounted-installer log independently recorded successful verification and extraction of all five payload archives.
- The wrapper correctly rejected the false success state, rolled back the app and payload roots, and cleared the transaction journal. The log shows an existing install snapshot and no full-uninstall path.
- Structural ISO checks and the focused Rust regression test passed, but those checks did not prove a successful installed-app transaction. The focused test passed: `tools::tests::cosyvoice_wetext_download_pins_snapshot_not_file_last_touch_commit` (`1 passed`, `0 failed`).

</topic>

<topic id="corrective-actions" status="complete" summary="Changes made during this task and their state" updated_at="2026-08-28">

| Action | State | Evidence |
| --- | --- | --- |
| Replace invalid ModelScope revision equality with pinned resolve downloads and bounded retries | Done | `product/engine/src/tools.rs`; final payload preparation completed |
| Accept annotated CPU CUDA assignment syntax | Done | `product/engine/src/tools.rs`; final payload preparation completed |
| Align wheel readiness check with `wheel==0.48.0` lock | Done | `product/engine/src/tools.rs`; final payload preparation completed |
| Make the PowerShell preparation-result boundary type-safe | Done | `offline-installer-runtime/scripts/build_offline_release_fast.ps1`; canonical run completed |
| Run the focused CosyVoice/Wetext regression test | Done | `1 passed`, `0 failed` |
| Build and structurally inspect the `0.1.183` offline ISO | Done, but insufficient | ISO test/listing and SHA-256 evidence recorded above |
| Prove a successful install/update from the exact mounted ISO | **Not done** | `installer_0.1.183_latest.log`: exit `0` but missing `OfflineInstallGeneration`; rollback followed |
| Rebuild after marker support and re-run installed-app acceptance | **Not done** | No corrected installer currently exists |

</topic>

<topic id="prevention-guards" status="follow-up" summary="Concrete changes that would prevent recurrence" updated_at="2026-08-28">

1. Add a unit test for both annotated and unannotated `cuda = None` forms.
2. Add a readiness-contract test that derives the allowed wheel version from the generated lock and rejects drift in either direction.
3. Add a PowerShell test that invokes the preparation function with noisy progress output and asserts that the caller receives exactly one structured result.
4. Add a release preflight that validates the existing prepared cache, lock, and venv before starting a new multi-gigabyte stage.
5. Persist a small phase receipt with phase name, start/end time, last successful phase, and failure diagnostics; expose it in the release log.
6. Make large archive progress bounded and phase-oriented so errors remain visible (while preserving the guide's required progress behavior).
7. Ensure a failed wrapper releases its own lock/process in a `finally` path, and make retry diagnostics identify the owning PID and lock age.
8. Add a regression check for ModelScope resolve status handling (`403`, `429`, transient `5xx`) and pinned-file hash validation.
9. Make clean-profile install/update from the exact release ISO a hard release gate: assert core exit code, fresh `OfflineInstallGeneration`, version, install path, main binary, and journal cleanup before publishing.
10. Record the wrapper-source hash, embedded core hash, and marker-support source revision in one release manifest, and compare those identities before mounting or distributing an artifact.

These are follow-up hardening items; they are not claimed as completed by the final ISO proof.

</topic>

<topic id="verification" status="corrected" summary="Structural evidence and the failed functional acceptance gate" updated_at="2026-08-28">

The final canonical run used:

```powershell
pwsh -NoProfile -File .\offline-installer-runtime\scripts\build_offline_release_fast.ps1
```

Recorded checks:

- ISO integrity test: `7z t -tiso` returned `Everything is Ok`.
- ISO listing contained root `Install_VoxVulgi.exe`.
- ISO listing contained exactly the required payload archives: `payload_cosyvoice_venv.7z`, `payload_huggingface.7z`, `payload_models.7z`, `payload_tools.7z`, and `payload_voice_backends.7z`.
- Focused Rust regression: `1 passed`, `0 failed`.
- Final SHA-256 was independently read from the delivered ISO path.
- Mounted installer log: `%APPDATA%\\com.voxvulgi.voxvulgi\\diagnostics\\installer\\installer_0.1.183_latest.log`.
- Payload verification/extraction: all five phases completed.
- Core process result: `exit_code=0`.
- Required postcondition: failed because `OfflineInstallGeneration` was empty.
- Recovery: core rollback complete, all four payload roots rolled back, `journal_cleared`, terminal `outcome=failure`.
- Therefore the ISO passed structural checks but failed the production acceptance boundary and must not be called a releasable offline installer.

</topic>

<topic id="evidence-and-attribution" status="corrected" summary="What this postmortem attributes and what remains unproven" updated_at="2026-08-28">

Operator correction: all work described in this postmortem was performed by this assistant; no other assistant contributed. The intentional source edits and release-work changes are therefore attributable to this task's assistant execution, even though the worktree contains changes spanning multiple project areas.

The directly relevant source files include:

- `product/engine/src/tools.rs`
- `offline-installer-runtime/scripts/build_offline_release_fast.ps1`
- `product/desktop/src-tauri/installer/templates/installer.nsi` (uncommitted marker support)

The worktree contains many modified, deleted, and untracked files from the same assistant's broader work. This postmortem does not claim that every such change is required for this installer incident; it records the files and runtime evidence directly tied to the release command and the marker failure.

The corrected main lesson is stronger: structural payload and ISO checks are not installer proof. A release is invalid until the exact mounted artifact completes an install/update and independently proves the fresh registry marker, binary identity, retained-data behavior, and cleared journal. The wrapper's rollback protection worked; the release acceptance gate did not pass.

</topic>
