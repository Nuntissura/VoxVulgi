# VoxVulgi Build Rules

Date: 2026-05-12

These rules apply to frontend builds, backend builds, desktop builds, installer builds, UI-impacting changes, and any claim that a built surface is ready for operator use.

## Headless Build Verification

- Every build or UI-impacting change must be tested through the real app boundary, not only compiled.
- Verification must include visual inspection of the affected surface and backend or frontend navigation/interaction evidence for the affected behavior.
- Routine verification must not pop up the app window, steal focus, or hijack the operator keyboard or mouse.
- [VV-BUILD-AGENT-001] Computer-use, desktop mouse/keyboard simulation, and foreground-window automation are not acceptable routine VoxVulgi verification paths; needing one means the product's agent surface is incomplete.
- [VV-BUILD-AGENT-002] Every operator workflow required for development proof or supported agent operation must expose the same product behavior through the semantic agent surface, not through a bespoke test-only implementation.
- [VV-BUILD-AGENT-003] Agent-operable controls must be discoverable through stable semantic identity and declare their permitted action, effect class, and current availability; generic selectors, arbitrary script execution, and implicit permission from button shape remain forbidden.
- [VV-BUILD-AGENT-004] Agent actions must be bounded, attributable, and return a structured receipt; long-running actions must remain independently observable while health, state, audit, navigation, and visual capture stay responsive.
- [VV-BUILD-AGENT-005] Destructive or credential-revealing behavior is never made agent-operable merely to satisfy test coverage; it requires an explicit product authorization contract appropriate to the same real-world action.
- [VV-BUILD-AGENT-006] Semantic UI inspection and action routes used against a running installed app must require a per-process unguessable bridge token read from the PID-bound bridge sidecar; never expose that token through health, state, audit, action receipts, WebView payloads, or diagnostics traces.
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
- Build the app/core against the separately installed managed runtime with `governance/scripts/build_desktop_target.ps1 -CoreOnly` (or `npm run build:desktop:target:core-only` from `product/desktop`).
- A core-only build must not require, validate, refresh, build, bundle, install, warm up, or certify an offline payload; it retains the already-assigned product version, performs the desktop build and output placement, and writes a build log without changing the release changelog.
- The pack warmup gate is not applicable to a core-only build because that build neither changes nor ships the dependency pack.
- Desktop compilation must reuse `product/desktop/build_target/cargo_cache`; `Current` contains only the published app deliverables and must never be used as the compiler cache.
- Routine app builds, UI checks, and developer verification must reuse an existing verified offline payload when the payload inputs did not change.
- Do not refresh or rebuild the offline payload merely to prove unrelated UI/backend code changes.
- Refresh the offline payload only when building a release that explicitly requires a fresh payload, when bundled dependency inputs changed, when the payload is missing/stale, or when the operator asks for a full dependency refresh.
- Before starting a payload-refreshing build, state that it can be slow because it downloads, installs, verifies, and packages the local toolchain and models.
- Payload refresh logs must show the active dependency/package/model stage clearly enough that a long run can be distinguished from a hang.

## Package-Only Offline Installer Gate (WP-0316)

- `[VV-BUILD-INSTALL-000]` through `[VV-BUILD-INSTALL-010]` live only in `offline-installer-runtime/GUIDE.md`.
- Use only an entrypoint that `offline-installer-runtime/GUIDE.md` explicitly marks conforming.
- `offline-installer-runtime/scripts/build_offline_release_fast.ps1` was deleted on 2026-08-30 because it was a stale, invalid, unwanted artifact that rebuilt and recertified product/runtime inputs during packaging.
- Offline-installer packaging consumes explicit already-built and already-qualified inputs; it must not build, download, install, repair, warm up, upgrade, or mutate them.
- Do not duplicate or replace that procedure here.

## Release Identity Gate (WP-0316)

- [VV-BUILD-VERSION-001] Desktop/installer build, repair, retry, test, and offline-packaging commands must not calculate, increment, or write a semantic version.
- [VV-BUILD-VERSION-002] `build_desktop_target.ps1` requires `-ExpectedVersion <already-assigned-version>` for an actual desktop build and fails before building when it does not exactly match all product-version files.
- [VV-BUILD-VERSION-003] Desktop and offline packaging commands must verify that the product-version files and `governance/release/BUILD_CHANGELOG.md` remain byte-for-byte unchanged.
- [VV-BUILD-VERSION-004] Only an explicit operator-directed product-release action may assign a new version; only an exact proven, operator-designated release may be appended to the release changelog.

## No More Cards

- Do not introduce new card-based UI.
- Do not use generic bordered boxes as the default way to separate page sections.
- New and touched UI should favor clear workflow structures: header strips, stepper rows, master-detail panes, tables, lists, toolbars, drawers, accordions, status strips, and focused modals.
- When touching an existing card-heavy surface, reduce the card count and remove competing start points, repeated actions, and unclear end points.
- A workflow screen must make the current item, active step, next action, and terminal output state obvious without requiring a separate explanatory card.
