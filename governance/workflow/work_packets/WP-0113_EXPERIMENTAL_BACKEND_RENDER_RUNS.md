# Work Packet: WP-0113 - Experimental backend render runs

## Metadata
- ID: WP-0113
- Owner: Codex
- Status: DONE
- Created: 2026-03-08
- Target milestone: Voice backend modernization

## Intent

- What: Extend BYO backend adapters from probe-only readiness into explicit experimental render runs that emit standard VoxVulgi manifests and reports.
- Why: The research is only operationally useful if operators can run candidate backends against real subtitle tracks and benchmark the resulting artifacts inside the app.

## Scope

In scope:

- Extend adapter configs with an explicit render-command contract.
- Add a queued experimental render run that writes a standard manifest/report into item artifact folders.
- Reuse existing artifact browser, mix/mux pipeline, QC, and benchmark flows on those outputs.
- Keep the contract explicit, local, and operator-supplied.

Out of scope:

- Silent backend installs or runtime downloads.
- Shipping heavyweight experimental backends inside the default installer.

## Acceptance criteria

- Operators can queue an experimental backend render run for an item/track when a BYO adapter is configured.
- The run writes a standard manifest that benchmark/mix/mux flows can read.
- The app captures render request/report artifacts and exposes them through existing artifact/output surfaces.
- Failures are explicit and do not corrupt the managed default path.

## Test / verification plan

- Rust tests for adapter command resolution/render contract validation.
- Desktop build.
- Mock-adapter engine/Tauri smoke that writes a manifest and proves artifact discovery.

## Status updates

- 2026-03-08: Created from the voice-cloning research modernization tranche.
- 2026-03-08: Added explicit adapter render commands, queued experimental backend render jobs, backend-aware manifest selection in mix/QC, Localization Studio run controls, Diagnostics render-command editing, and proof under `product/desktop/Build Target/tool_artifacts/wp_runs/WP-0113/20260308_144524/`.
