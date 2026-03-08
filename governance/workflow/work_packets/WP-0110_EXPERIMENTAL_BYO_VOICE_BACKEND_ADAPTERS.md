# Work Packet: WP-0110 - Experimental BYO voice backend adapters

## Metadata
- ID: WP-0110
- Owner: Codex
- Status: DONE
- Created: 2026-03-08
- Target milestone: Voice backend modernization

## Intent

- What: Add an explicit local adapter registry for experimental voice backends that VoxVulgi does not ship or auto-install.
- Why: The best OSS voice backends move quickly and are often too heavyweight or license-sensitive to manage as part of the default product image.

## Scope

In scope:

- Store operator-supplied adapter configs for local executables/scripts.
- Validate adapters with explicit probe/test commands.
- Surface adapter readiness and metadata in Diagnostics and the backend catalog.
- Keep the contract local-first and fully operator-directed.

Out of scope:

- Silent downloads or auto-installs.
- Shipping these experimental backends inside the installer by default.

## Acceptance criteria

- Operators can register, inspect, update, and remove a BYO backend adapter.
- The app can run a non-destructive probe and report readiness/errors.
- The catalog/recommendation surface can distinguish managed vs BYO experimental backends.

## Test / verification plan

- Desktop build.
- Rust tests for adapter config validation.
- Probe-path smoke with a local mock adapter.

## Status updates

- 2026-03-08: Created from the research transfer packet.
- 2026-03-08: Implemented local BYO adapter config/probe storage, catalog status overlays, and Diagnostics management UI; proof in `product/desktop/Build Target/tool_artifacts/wp_runs/WP-0110/20260308_033400/`.
