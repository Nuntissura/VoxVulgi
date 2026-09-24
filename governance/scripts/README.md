# Governance scripts

This folder is for small helper scripts that support the workflow (creating work packets, exporting diagnostics bundles, validating file layout, etc.).

Keep scripts:

- safe by default (no destructive operations without explicit confirmation),
- cross-platform where feasible (or clearly document OS constraints).

## Scripts

- `bootstrap_dev.ps1` - installs dev dependencies (npm/cargo) and bootstraps runtime assets (FFmpeg tools + `whispercpp-tiny` model) into the app data directory.
- `build_desktop_target.ps1` - builds the desktop app with reusable compiler artifacts under `product/desktop/build_target/cargo_cache`, publishes only app deliverables under `product/desktop/build_target/Current`, archives prior published outputs under `old_versions`, and writes logs under `logs`. Actual builds require `-ExpectedVersion <already-assigned-version>`; the script rejects mismatches and proves that product-version files and `governance/release/BUILD_CHANGELOG.md` were not changed. Use `-CoreOnly` for an app build that neither checks nor touches the separately managed offline runtime; payload validation/refresh flags remain available only for legacy payload-coupled flows.
- `cleanup_artifacts.ps1` - dry-run by default; with `-Force` removes generated test/tool artifacts (`tmp_*`, Rust `target*`, and offline tool/model caches). Use `-IncludeBuildTarget` to also clean desktop build outputs.
- `uninstall_voxvulgi.ps1` - deterministic VoxVulgi uninstall cleanup helper; dry-run by default, with `-Force` kills stuck VoxVulgi installer processes, runs uninstall commands when present, removes install leftovers/shortcuts, and verifies no install artifacts remain. Keeps user data by default; use `-PurgeUserData` only for explicit full wipe.
