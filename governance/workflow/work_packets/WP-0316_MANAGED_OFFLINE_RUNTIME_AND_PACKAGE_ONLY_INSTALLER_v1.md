---
file_id: WP-0316-v1
file_kind: work-packet
updated_at: 2026-09-30
---

<topic id="contract" status="needs-validation" version="v1" wp="WP-0316" updated_at="2026-08-30">

# Work Packet: WP-0316 — Managed offline runtime and package-only installer

## Metadata

- ID: WP-0316
- Owner: Codex
- Status: NEEDS_VALIDATION
- Created: 2026-08-30
- Refinement: `WP-0316_MANAGED_OFFLINE_RUNTIME_AND_PACKAGE_ONLY_INSTALLER_v1_REFINEMENT.md`
- Board: `../TASK_BOARD.md#wp-0316`
- Supersedes as active delivery authority: WP-0265, WP-0308
- Detailed installer authority: `../../../offline-installer-runtime/GUIDE.md`
- Failure evidence: `../../release/OFFLINE_INSTALLER_BUILD_POSTMORTEM_2026-08-28.md`, `../../release/OFFLINE_INSTALLER_RELEASE_FAILURE_POSTMORTEM_2026-08-30.md`

## Intent

Ship one offline ISO for non-technical users by packaging an explicit already-built app and already-qualified reusable runtime. Decouple replaceable runtime generations from durable user data and remove developer-machine fallbacks that can conceal an incomplete release.

## Required order

1. Retire the stale fast-release script and conflicting active guidance; preserve historical postmortem references only when labelled historical/nonconforming.
2. Split roaming user data from LocalAppData managed runtime generations; add manifest validation, atomic activation/rollback, strict managed resolution, test isolation, and existing-surface diagnostics provenance.
3. Qualify the existing working dependency/model trees into reusable non-solid component archives from explicit inputs. Qualification may build/repair only when separately authorized; it is not installer packaging.
4. Implement one conforming package-only entrypoint from `offline-installer-runtime/GUIDE.md`. It verifies explicit input hashes, builds the wrapper and ISO, and never invokes product/runtime creation.
5. Test the exact ISO offline on disposable roots/VM, publish only its passing hash, and record failure state here when any gate fails.

## Ordered microtasks

1. Update PRODUCT_SPEC, TECHNICAL_DESIGN, build rules, guide references, taskboard, and conflicting packet statuses.
2. Add `AppPaths` user-data/runtime separation with compatibility-preserving isolated constructors.
3. Add runtime manifest/current-pointer schema, path confinement, hash binding, compatibility checks, immutable generation selection, and atomic pointer helpers.
4. Route tools/models/Hugging Face/voice backends/Python through the selected runtime generation.
5. Disable PATH, current-directory, repo, automatic install/download, and runtime repair fallbacks in managed mode; preserve explicit developer/legacy behavior outside managed mode.
6. Expose runtime provenance and health through existing Diagnostics rows/export and `GET /agent/state`; add no card.
7. Guard installer/qualification/performance/proof paths against production APPDATA/LocalAppData and require disposable headless roots.
8. Define and produce separately qualified, content-addressed, non-solid archives plus runtime manifest/receipts.
9. Replace the retired entrypoint with one package-only script; add source/process contract tests that reject build/download/warmup behavior.
10. Build wrapper and ISO from frozen inputs; verify topology, hashes, exact input lineage, and unchanged product version/changelog.
11. Run exact-ISO clean/offline install, default workflow, update/reinstall-keep, interruption/rollback, long-path, and data-preservation proof.
12. Run independent adversarial review; repair findings and rerun affected gates.
13. Remove automatic semantic-version and release-changelog mutation from desktop/installer builds; require callers to name the already-assigned product version and prove those release-identity files remain unchanged.
14. Cover migration from legacy machine-wide installs: a current-user installer must detect and log the HKLM installation, remain confined to its LocalAppData target even when stale state or `/D` names Program Files, preserve user data, and prove the exact mixed-scope machine case.

## Proof contract

- Proof root: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0316/<run-id>/`.
- Required evidence: spec/authority diff; runtime manifest and pointer schemas; path-confinement/hash/compatibility tests; strict fallback negative tests; production-root sentinel proof; Diagnostics/agent-state receipts; qualified archive identities and reuse proof; desktop-build and package transcripts; product-version/changelog non-mutation receipts for desktop/installer builds and packaging; independent ISO listing/hash; exact-ISO blocked-network clean install/default workflow; keep-data hashes; rollback/interruption matrix; and adversarial review.
- Component tests cannot replace exact packaged app or exact ISO proof. A package build is not a desktop build and must not change the semantic version or build changelog.
- Status remains `IN_PROGRESS` until the exact public ISO hash passes every required gate; a failed attempt must append its phase, input IDs, evidence path, root cause, and next action below.

</topic>

<topic id="attempt-ledger" status="active" version="v1" wp="WP-0316" updated_at="2026-08-30">

# Attempt and state ledger

- 2026-08-30 / `WP-0316-A001` / `IN_PROGRESS`: retired the stale fast entrypoint; updated the sole guide, specs, build rules, taskboard, and postmortem warnings; implemented split roaming user data and LocalAppData immutable runtime generations, hash-bound manifest activation/rollback, strict managed resolution, mandatory disposable headless user-data roots, and runtime provenance in existing Diagnostics/agent-state surfaces. Added separate qualification and package-only entrypoints plus a four-archive Inno wrapper. Focused proof passed: six managed-runtime unit tests, strict no-fallback test, TypeScript compilation, 37 installer/runtime contracts, PowerShell parsing, desktop/engine Cargo checks, real Inno 7.1 compilation, reparse/overlap negative probes, and a final tiny frozen-input package-only run that produced and independently opened `simple-offline-installer.iso` SHA-256 `69ec839d167d8e5bc03e0c8f94c26c3365e9d09b91acf77efded2cd01223f42d` without changing the app version or changelog and with zero partial package stages left behind. This tiny ISO proves the package boundary only and is not a release candidate.
- 2026-08-30 / `WP-0316-A001` / `BLOCKED_GATE`: canonical desktop payload validation fails because `product/desktop/src-tauri/offline/manifest.json` is absent. No desktop build, version bump, real runtime qualification, or exact-ISO acceptance was attempted. Reconstructing dependencies during packaging is forbidden. Next action: separately identify or qualify the explicit existing working payload and core setup, then run the package-only command and the exact resulting ISO through microtasks 10–12.
- 2026-08-30 / `WP-0316-A001` / `APP_SLICE_PASS`: superseded the app-build portion of the prior blocked gate by adding `-CoreOnly`. The governed desktop build now skips payload validation/refresh and pack warmups, keeps compiler artifacts in stable `build_target/cargo_cache`, and publishes only app deliverables to `Current`. Version `0.1.201` built successfully while `offline/manifest.json` remained absent. Exact outputs: `desktop.exe` SHA-256 `78970a543bf2f159ce96b22527278dde1e9e726820566fa2701bbff4960e8309`; normal NSIS setup SHA-256 `d27d4ef87466953f4426fc9aabda6b41e4aa9e2346226947e8a8fa3e429518f6`. Hidden headless proof reported `app_version=0.1.201`, isolated user data, and LocalAppData runtime root; Diagnostics screenshots show the runtime provenance without adding a card. Real runtime qualification and exact-ISO acceptance remain outstanding.
- 2026-08-30 / `WP-0316-A002` / `INSTALLER_MAINTENANCE_PASS`: diagnosed the reported keep-data reinstall failure from the live machine: the v0.1.188 uninstaller had removed program files and registry state while preserving the 1.11 GB database/settings, but the parent installer had no durable phase log. Reworked both reinstall actions into one CRC-checked NSIS process, changed compression from LZMA to zlib, added `/VVMAINTENANCE=<action>`, and added an error-state-preserving phase log. Exact v0.1.204 setup SHA-256 `a7d29e9a51153c69e4cc5d7498030a79f08e66292eba682aad7d2eca19b77736` completed `reinstall_keep` in 47 seconds with exit 0; installed app/registry report `0.1.204`; database SHA-256 `2a7f87e0beb807b0a955863319bc39b6e5baeb6687ac8afcaa5c4f194968ee03` was unchanged; 10/10 configuration files were unchanged; 7/7 required log phases were present; the packaged headless app returned health `ok`; and all 328 contracts passed. Evidence: `../../../product/desktop/build_target/tool_artifacts/wp_runs/WP-0316/20260830_installer_maintenance_v0_1_204/summary.md`. This closes the normal core-installer maintenance defect only; real runtime qualification and exact-ISO acceptance remain outstanding.
- 2026-08-30 / `WP-0316-A003` / `VERSION_GUARD_PASS`: removed automatic patch-version calculation/writes and automatic release-changelog publication from `build_desktop_target.ps1`. Actual builds now require an exact `-ExpectedVersion`, retain that already-assigned version, and hash-check all three product-version files plus `BUILD_CHANGELOG.md` after packaging. Proof: PowerShell parse passed; six focused managed-runtime/build contracts and all 329 repository contracts passed; a real invocation without `-ExpectedVersion` failed before build/archive/publish work; all four protected hashes remained unchanged throughout. No desktop build or version change was performed.
- 2026-09-23 / `WP-0316-A005` / `OPERATOR_DECISION`: the operator reviewed the no-bump policy this packet authored (`AGENTS.md`/`CLAUDE.md` VV-CODEX-VERSION-001…003, `build_rules.md` VV-BUILD-VERSION-001…004, `-ExpectedVersion` guard in `build_desktop_target.ps1`) after noticing builds had stopped incrementing, and confirmed it as intended authority: versions change only by an explicit operator release action. These authority edits are still uncommitted as of this note.
- 2026-08-30 / `WP-0316-A004` / `LEGACY_MACHINE_MIGRATION_PASS`: fixed the exact mixed-scope failure shown by the operator. Live state contained current-user v0.1.204 under LocalAppData plus stale machine-wide v0.1.179 under Program Files; the old Program Files executable was administrator-owned and not writable by the current-user installer. The current-user NSIS initializer now treats HKLM as separate migration evidence, logs it, and unconditionally confines `$INSTDIR` to `%LOCALAPPDATA%\\VoxVulgi`, including against stale saved state and `/D`. Package-only recompilation reused the existing v0.1.204 desktop binary and produced setup SHA-256 `0dda2e70bf1e5db092ed28f296c3808b8c8507fe36d43938309a8a258e3b8ad8` (279,373,871 bytes) without a desktop build, version change, or changelog entry. The exact mixed-state passive reinstall exited 0 and logged the detected v0.1.179 path, enforced LocalAppData target, in-process cleanup, binary write, metadata write, and success; 14/14 protected database/config files (1,111,426,220 bytes) remained byte-identical. The legacy HKLM registration, public shortcuts, Program Files binary, and abandoned 1.83 GB offline payload were then removed while HKCU v0.1.204 remained. Installed-app proof on an isolated C: root returned health `ok`, `agent_headless=true`, and `app_version=0.1.204`; all 330 contracts passed. Evidence: `../../../product/desktop/build_target/tool_artifacts/wp_runs/WP-0316/20260830_legacy_machine_migration_v0_1_204/summary.md`. This closes the core-installer mixed-scope migration defect only; real runtime qualification and exact-ISO acceptance remain outstanding.

</topic>

<topic id="status-reconciliation-2026-09-30" status="needs-validation" wp="WP-0316" updated_at="2026-09-30">

## Status reconciliation — 2026-09-30

Package-only app/runtime split and maintenance/migration implementation recorded. Remaining real runtime qualification and exact full ISO clean/offline/update/rollback acceptance; sole current installer authority retained, not declared production-qualified. Historical requirements and proof remain preserved. Reconciled by WP-0326; no new runtime proof.

</topic>

<topic id="runtime-input-remediation-20261003" status="IN_PROGRESS" wp="WP-0329" updated_at="2026-10-03">

Verified defect: old qualified runtime 0.1.204 copied flat Whisper files, while ModelStore/asr resolves models/<id>/<version>/<file>. Its required_files covered executables but omitted required ASR/Kokoro assets. Revised Phase Q uses exact existing model-manifest-bound bytes, creates canonical model paths, checks required Kokoro revision and three asset hashes, and includes them in required_files. Qualification identity now binds qualification recipe plus product model/dependency manifest hashes. Package-only script/old receipt remain unchanged.

Implementation: tools.rs returns unknown after missing/failed capability probe, with regression test; generated ModelScope Git subprocess uses CREATE_NO_WINDOW only on Windows. runtimeAssetLayout.test.ts executes the actual normalization block with owned tiny fixture inputs and corrupt-file refusal. Root owns all test execution/Cargo cache.

Exact live diagnostic: owned hidden Python PID216332 observed Torch import blocked at torch/__init__.py256 kernel32.LoadLibraryExW before CUDA query. Faulthandler logs under %TEMP%/vv_runtime_probe_20261003. Only this session-owned PID was stopped after bounded observation. Live Python base_prefix/sys.path points to an old repo release payload. This is not proof the stale path causes the DLL stall. Metadata torch2.10.0/torchaudio2.11.0 mismatch is recorded, not blindly repaired. Production app/process/runtime files remain untouched.

Remaining: root focused bundled checks; real new qualification after latest yt-dlp selected-engine compatibility gate; active model/cache restoration at controlled installed-generation boundary; ModelStore/ASR resolution and actual ASR/translation/dub outputs; exact ISO acceptance. Old archives cannot be promoted by rewriting compatibility. Full-stack import proof alone is insufficient.

</topic>

<topic id="offline-acceptance-research-deliberation-20261003" status="IN_PROGRESS" wp="WP-0316" updated_at="2026-10-03">

## Inspected findings and pending decisions

GUIDE Phase B requires a controlled clean standard-user profile and isolated networking; it does not require CodexSandboxOffline. The current verifier, offline-installer-runtime/scripts/test_offline_full_installer_runtime.ps1, unconditionally calls Get-CanonicalFirewallAttestation at lines1059/1152. That function at lines177–206 hardcodes this host's SID ending1005, account label and three firewall-rule GUIDs. The general Get-NetworkIsolationSample function at lines166–175 is unused. Thus another genuinely isolated Windows VM is rejected by machine-specific verification inputs, contrary to the environment-neutral GUIDE boundary. Proposed correction, NOT_IMPLEMENTED and NOT_APPROVED: explicit expected profile SID plus independently attested host/guest isolation mode, retaining nonadministrator/native-profile, exact-ISO, complete workflow, retained-data and zero-download gates. This finding does not authorize a validator bypass or acceptance change.

The verifier defaults InstallTimeoutSeconds to 1800. Actual normal_update_13a02c5_terminal.json timestamps 1791027649698→1791029494323 show 1844.625 seconds before failed terminal/complete rollback. Proposed explicit argument `-InstallTimeoutSeconds 3600` avoids the already-observed inadequate deadline for unchanged extraction inputs; no packaging or deadline change was executed by this research pass.

Official Inno documentation supports the existing enhanced/nopassword extraction choice for normal memory use with large files and advises against solid archives for extraction performance. Exec with SW_HIDE and ewWaitUntilTerminated returns the embedded installer's exit code using Setup's credentials. Inno's installation-order documentation explicitly says errors after uninstall-log finalization do not automatically roll back preceding installation work; VoxVulgi's custom transaction journal, postcondition checks and rollback remain necessary. Sources inspected2026-10-03: [ArchiveExtraction](https://jrsoftware.org/ishelp/topic_setup_archiveextraction.htm), [external archive files](https://jrsoftware.org/ishelp/topic_filessection.htm), [Exec](https://jrsoftware.org/ishelp/topic_isxfunc_exec.htm), [installation order](https://jrsoftware.org/ishelp/topic_installorder.htm), [NSIS user execution level](https://nsis.sourceforge.io/Reference/RequestExecutionLevel). No evidence justified replacing the existing archive/embedded-core architecture.

The previous wrapper's omitted production MAIN_BINARY_NAME binding was an execution/input-binding defect, distinct from those sound patterns. Current package_offline_release.ps1 explicitly binds desktop.exe; VerifyCorePostcondition still checks registry version, runtime generation, binary identity/name and file version. Seven focused package contracts passed in wp0331_real_binary_package_contract.log. The prior exact13a ISO exited3 after core exit0 and its actual Inno log records core/runtime/generation rollback completion. A separate core-only Update exited0 and protected before/after snapshots retain all14 table counts/hashes; it does not prove full-ISO update success.

Current e180 candidate receipt/state identify ISO ca6d97c36e5aa60809cbe2658c44bc4b2743e8ed0fbef9370b2a71ea942b4950,7377000448bytes, runtime_b9bd5d61388584cb50b2dadf and retained0.1.205. State remains CANDIDATE_NOT_RELEASED. The operator's single final acceptance attempt remains unused. Reuse unchanged qualified archives and passing package proof; complete product/runtime-changing repairs before final acceptance. Proposed deliberation must distinguish intermediate controlled runtime/workflow verification from the final release attempt because WP-0329 requires actual activated-runtime localization proof while delivery depends on that repair; no intermediate execution or altered attempt accounting is approved here.

Remaining exact-candidate checks: clean standard-user native AppData and network isolation; mounted ISO/hash and wrapper identity; offline clean install; required dependency availability without acquisition/repair; import→captions→translate→dub→export with actual outputs; exact-candidate Update and retained preferences/subscriptions/playlists/database/library preservation; applicable recovery/rollback proof; independent marker/archive/core reread; zero-download evidence; ACCEPTANCE_PASSED only after all required gates, followed by atomic publication and independent published-hash reconciliation. Actual installed ASR/translation/dub remains unproven by component imports alone.

Existing account preflight still reports native AppData under the operator profile and unloaded Offline profile; no environment-variable override proves native profile readiness. Microsoft documents Windows Sandbox networking disable/read-only mappings but its default logon account is administrator, so it is not a drop-in standard-user proof environment. A dedicated Windows VM with a real standard-user profile and independently verified disconnected networking is a researched alternative, pending verified platform availability and operator-approved verifier correction. Sources: [Windows Sandbox configuration](https://learn.microsoft.com/en-us/windows/security/threat-protection/windows-sandbox/windows-sandbox-configure-using-wsb-file), [Hyper-V VM prerequisites and optional networking](https://learn.microsoft.com/en-us/windows-server/virtualization/hyper-v/get-started/create-a-virtual-machine-in-hyper-v). No VM, account, service, installer, build, test or process action occurred in this recording pass. Original contract, GUIDE and open status remain unchanged.

</topic>
