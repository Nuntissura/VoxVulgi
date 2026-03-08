# Work Packet: WP-0129 - Reproducible offline bundle and vendor patch hardening

## Metadata
- ID: WP-0129
- Owner: Codex
- Status: BACKLOG
- Created: 2026-03-08
- Target milestone: Audit remediation tranche

## Intent

- What: Make the offline bundle reproducible from pinned inputs and replace fragile live vendor-source patching with maintainable, testable mechanisms.
- Why: `WP-0122` found mutable bundle inputs, unpinned fallback installs, and brittle third-party patch flows.

## Scope

In scope:

- Pinned binary/wheel/source manifest for bundled dependencies.
- Removal or isolation of unpinned fallback installs in release preparation.
- Hardening/replacement of live third-party source patching.
- Integrity/provenance improvements for shipped bundle contents.

Out of scope:

- Full cloud/vendor service integrations.

## Acceptance criteria

- Offline bundle inputs are reproducibly pinned.
- Release prep no longer depends on mutable unpinned fallback installs.
- Third-party patch flows are maintainable and regression-tested.

## Test / verification plan

- Installer/bundle preparation verification with durable manifests and proof outputs.
- Focused tests for patched dependency handling.

## Status updates

- 2026-03-08: Created from `WP-0122` dependency and supply-chain findings.
