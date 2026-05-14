# Work Packet: WP-0219 - Build script offline payload reuse

## Status

REVIEW

## Base Scope

- Change the governed desktop build script so routine builds validate and reuse an existing offline payload when bundled dependency inputs did not change.
- Keep explicit payload refresh paths for full release/dependency-refresh work.
- Keep build verification headless and do not launch or focus the app window.

## High-ROI Additions

- Add a validation-only mode so agents can check payload readiness without bumping versions or building installers.
- Add a local payload-input fingerprint tied to the pinned dependency manifest so future builds can detect stale payloads cheaply.
- Keep the legacy no-prep alias but make it fail on stale or missing payloads instead of silently building an installer with an invalid dependency bundle.
- Add npm aliases for normal, refresh, force-refresh, and payload-validate workflows.

## Reused Systems

- Existing `governance/scripts/build_desktop_target.ps1` target build flow.
- Existing `governance/scripts/prep_offline_bundle.ps1` payload prep script.
- Existing pinned dependency manifest at `product/engine/resources/tooling/pinned_dependency_manifest.json`.
- Existing generated offline payload files under `product/desktop/src-tauri/offline/`.

## Gaps Closed

- Routine builds no longer treat every app/UI change as a full dependency-payload rebuild.
- Build logs now state whether the payload was reused, refreshed, or rejected as stale.
- Future dependency changes can force a payload refresh without relying on operator memory.

## Risks And Hardening

- Risk: an old payload without a fingerprint could be reused incorrectly.
  - Remediation: only auto-adopt an unfingerprinted payload when both payload and manifest are newer than the pinned dependency manifest.
- Risk: a missing or corrupt payload could produce a broken installer.
  - Remediation: default and no-prep paths validate manifest schema, payload presence, and payload byte count before packaging.
- Risk: dependency input changes might not fully clean an existing stage cache.
  - Remediation: keep `-ForceRefreshOfflinePayload` for clean refreshes; default stale refresh still reuses cache for speed.

## Verification

- 2026-05-14: PowerShell parser check passed for `governance/scripts/build_desktop_target.ps1`.
- 2026-05-14: `powershell -ExecutionPolicy Bypass -File governance/scripts/build_desktop_target.ps1 -ValidateOfflinePayloadOnly` passed and adopted the current local payload fingerprint.
- 2026-05-14: `npm run build:desktop:payload:validate` passed from `product/desktop`.
- 2026-05-14: `git diff --check` passed.
