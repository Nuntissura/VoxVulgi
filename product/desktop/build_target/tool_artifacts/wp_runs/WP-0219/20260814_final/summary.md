# WP-0219 Summary

Status: DONE
Date: 2026-08-14

## Outcome
- Routine governed builds validate and reuse the existing offline payload when its fingerprint matches the pinned dependency manifest.
- Explicit refresh, force-refresh, validation-only, and stale-rejecting skip paths are present, with npm aliases for operator/agent use.

## Verification
- Inspected `governance/scripts/build_desktop_target.ps1` for `ValidateOfflinePayloadOnly`, `RefreshOfflinePayload`, `ForceRefreshOfflinePayload`, fingerprint validation/adoption, stale rejection, refresh, and reuse branches.
- Inspected `product/desktop/package.json` and confirmed the normal, refresh, force-refresh, and payload-validation aliases.
- Parsed `product/desktop/src-tauri/offline/payload_inputs.json`; it binds offline bundle `offline_full_win64_20260814_082842` and 6,161,329,153 payload bytes to pinned-manifest SHA-256 `5286FA2F...310773A`.
- The governed v0.1.138 build log records `Reusing verified offline bundle payload`, `offline payload fingerprint matches pinned dependency manifest`, the exact payload size, and `Build completed`.

## Evidence
- `evidence.json`
- `product/desktop/build_target/logs/build_desktop_target_20260814-143555_0_1_138.log`
- `product/desktop/src-tauri/offline/payload_inputs.json`
- `governance/scripts/build_desktop_target.ps1`
- `product/desktop/package.json`

## Notes
- No additional build was needed: the just-completed v0.1.138 governed build exercised the routine reuse path on the current code and payload.
