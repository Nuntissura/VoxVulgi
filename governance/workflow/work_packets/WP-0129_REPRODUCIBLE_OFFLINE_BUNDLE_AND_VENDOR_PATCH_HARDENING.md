# Work Packet: WP-0129 - Reproducible offline bundle and vendor patch hardening

## Metadata
- ID: WP-0129
- Owner: Codex
- Status: DONE
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
- 2026-03-08: Added a tracked pinned dependency manifest for bundled tools and Python packs, made mutable fallback installs opt-in via `VOXVULGI_ALLOW_UNPINNED_FALLBACK`, replaced inline third-party patch scripts with tested Rust patch helpers, and added offline payload byte/hash verification before bundle hydration.
- 2026-08-15: Reopened after two governed-build attempts proved the 7.70 GB / 152,376-file directory exporter was not interruption-safe: it deleted the prior payload before every refresh, and a native `-1` exit during `std::fs::copy` discarded all completed work. Official Rust `std::fs::copy` and `std::fs::rename` documentation confirmed the selected recovery design: retain completed destination files, cryptographically reuse only byte-identical files, copy replacements to a sibling temporary file, and promote only after the copy returns successfully. Added bounded progress output and stale-entry reconciliation. Focused offline-prep binary tests passed 7/7; status remains `IN_PROGRESS` pending a successful resumed payload refresh and governed desktop build.
- 2026-08-15: DONE on packaged v0.1.169. The resumed exporter completed 80,946 tool files plus models/cache without discarding verified work; the independent repository validator adopted and re-read the 6,161,358,602-byte payload against the pinned manifest; the governed build exited 0 and produced the v0.1.169 NSIS installer. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0129/20260815-180640-v0_1_169/summary.md`.
