# Governance scripts

This folder is for small helper scripts that support the workflow (creating work packets, exporting diagnostics bundles, validating file layout, etc.).

Keep scripts:

- safe by default (no destructive operations without explicit confirmation),
- cross-platform where feasible (or clearly document OS constraints).

## Scripts

- `bootstrap_dev.ps1` - installs dev dependencies (npm/cargo) and bootstraps runtime assets (FFmpeg tools + `whispercpp-tiny` model) into the app data directory.
