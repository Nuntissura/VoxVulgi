# VoxVulgi Build Rules

Date: 2026-05-12

These rules apply to frontend builds, backend builds, desktop builds, installer builds, UI-impacting changes, and any claim that a built surface is ready for operator use.

## Headless Build Verification

- Every build or UI-impacting change must be tested through the real app boundary, not only compiled.
- Verification must include visual inspection of the affected surface and backend or frontend navigation/interaction evidence for the affected behavior.
- Routine verification must not pop up the app window, steal focus, or hijack the operator keyboard or mouse.
- Prefer the Headless Agent Bridge and built-in visual debugger for app-boundary checks:
  - `GET /agent/health`
  - `GET /agent/state`
  - `POST /agent/navigate`
  - `POST /agent/snapshot`
  - `POST /agent/dump`
- In-WebView globals such as `window.__voxVulgiNavigate`, `window.__voxVulgiRequestSnapshot`, and `window.__voxVulgiRequestDump` are acceptable when already available without focus stealing.
- If a headless route is missing or broken, the build is not fully verified until the route is repaired or the verification gap is recorded as a blocker.

## Pack Warmup Gate (WP-0233)

- Every desktop target build must pass the pack warmup gate before producing an installer.
- The gate is `governance/scripts/pack_warmup_gate.ps1`, invoked automatically by `governance/scripts/build_desktop_target.ps1` as a pre-build step.
- The gate runs every Python `install_*_pack` + warmup probe against a throwaway APPDATA-equivalent root and refuses to continue the build on any pack failure.
- The build may skip the gate only with both `-SkipWarmupGate` AND `-SkipWarmupGateReason '<reason>'`; the reason is logged into the build transcript so the skip is auditable.
- A skipped-gate release must also note the skip in `governance/release/BUILD_CHANGELOG.md`.
- The gate is meant to catch resolver drift, lockfile breakage, or transient PyPI failures on the developer's machine instead of letting the regression ship to users.
- A full gate run installs the entire pack stack (~2 GB pip downloads on a clean cache), expect 10-20 min wall time on first run; subsequent runs reuse the pip wheel cache.

## Offline Payload Build Policy

- Treat the offline payload as the large bundled runtime dependency pack, not as normal app source.
- Routine app builds, UI checks, and developer verification must reuse an existing verified offline payload when the payload inputs did not change.
- Do not refresh or rebuild the offline payload merely to prove unrelated UI/backend code changes.
- Refresh the offline payload only when building a release that explicitly requires a fresh payload, when bundled dependency inputs changed, when the payload is missing/stale, or when the operator asks for a full dependency refresh.
- Before starting a payload-refreshing build, state that it can be slow because it downloads, installs, verifies, and packages the local toolchain and models.
- Payload refresh logs must show the active dependency/package/model stage clearly enough that a long run can be distinguished from a hang.

## Single-Unit Offline Installer Build Gate (WP-0308)

- [VV-BUILD-INSTALL-001] Every future public full-offline install/update build must publish one UDF ISO as the sole user-required artifact.
- [VV-BUILD-INSTALL-002] The ISO must contain root `Install_VoxVulgi.exe`, `README.txt`, the payload manifest, and every governed 64 MiB bounded-solid payload archive; list and verify the completed ISO before publication.
- [VV-BUILD-INSTALL-003] The installer definition must use Inno 7 `TExtractionWizardPage` to bulk-extract the externally shipped 64 MiB bounded-solid archives into an owned generation-specific staging tree, then transactionally promote the complete managed roots. `SolidCompression=no` remains mandatory for Inno's own compiled wrapper stream and does not disable solid blocks inside the external `.7z` payload archives. Inno `[Files] external extractarchive` entries, raw recursive dependency trees, and public `setup-*.bin` slices are forbidden.
- [VV-BUILD-INSTALL-004] Archive builds must be content-fingerprinted and reused when inputs are unchanged. Routine builds must use atomic durable metadata receipts plus the prior full source-audit digest and must not recursively re-read source file bytes on a valid receipt hit. `-AuditPayloadSources` forces a fresh source-byte audit; `-RefreshPayloadArchives` forces that audit plus archive recreation. Every reuse still verifies the cached archive SHA-256, `7z t`, path safety, metadata policy, bounded-solid layout, and a second source-metadata snapshot before ISO creation.
- [VV-BUILD-INSTALL-005] Every archive must pass `7z t`, path-safety inventory, and destination-boundary checks before ISO creation.
- [VV-BUILD-INSTALL-006] Every installer run must enable logging, continuously checkpoint `installer_<version>_latest.log`, and retain a timestamped final log under `%APPDATA%\com.voxvulgi.voxvulgi\diagnostics\installer` on success, failure, or cancellation.
- [VV-BUILD-INSTALL-007] Release verification must preserve the canonical representative Python-tree result of at least 2x versus the legacy raw-file fixture. The selected 64 MiB bounded-solid layout passed at 2.128x (222.717 s raw median versus 104.677 s archive median, identical output-tree SHA-256). A full clean-profile offline install on the documented reference local-SSD machine with default security settings remains a separate required target of at most 30 minutes.
- [VV-BUILD-INSTALL-008] The single canonical build procedure is `governance/release/OFFLINE_INSTALLER_BUILD_MANUAL.md`; read it before installer work and update it whenever installer inputs, commands, outputs, tools, proof gates, or recovery behavior changes.
- [VV-BUILD-INSTALL-009] The durable log must cover wrapper identity, named payload phase boundaries, core-installer handoff, before/after installed state, observed registry and binary versions, verification result, failure reason, and terminal outcome.
- [VV-BUILD-INSTALL-010] Refuse publication when the wrapper can report success without proving the expected core uninstall-registry version and installed main-binary file version.

## No More Cards

- Do not introduce new card-based UI.
- Do not use generic bordered boxes as the default way to separate page sections.
- New and touched UI should favor clear workflow structures: header strips, stepper rows, master-detail panes, tables, lists, toolbars, drawers, accordions, status strips, and focused modals.
- When touching an existing card-heavy surface, reduce the card count and remove competing start points, repeated actions, and unclear end points.
- A workflow screen must make the current item, active step, next action, and terminal output state obvious without requiring a separate explanatory card.
