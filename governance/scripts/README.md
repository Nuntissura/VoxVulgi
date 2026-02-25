# Governance scripts

This folder is for small helper scripts that support the workflow (creating work packets, exporting diagnostics bundles, validating file layout, etc.).

Keep scripts:

- safe by default (no destructive operations without explicit confirmation),
- cross-platform where feasible (or clearly document OS constraints).

## Scripts

- `bootstrap_dev.ps1` - installs dev dependencies (npm/cargo) and bootstraps runtime assets (FFmpeg tools + `whispercpp-tiny` model) into the app data directory.
- `build_desktop_target.ps1` - builds the desktop app with `CARGO_TARGET_DIR` pinned to `product/desktop/Build Target/Current` and archives prior build outputs to `product/desktop/Build Target/Old versions`.
- `cleanup_artifacts.ps1` - dry-run by default; with `-Force` removes generated test/tool artifacts (`tmp_*`, Rust `target*`, and offline tool/model caches). Use `-IncludeBuildTarget` to also clean desktop build outputs.
