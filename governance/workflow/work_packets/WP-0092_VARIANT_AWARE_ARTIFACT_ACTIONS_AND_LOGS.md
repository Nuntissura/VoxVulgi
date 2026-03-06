# Work Packet: WP-0092 - Variant-aware artifact actions and logs

## Metadata
- ID: WP-0092
- Owner: Codex
- Status: DONE
- Created: 2026-03-06
- Target milestone: Phase 3 voice-workflow hardening

## Intent

- What: Make Localization Studio artifact actions, status, and logs resolve against the concrete artifact variant the operator selected.
- Why: A/B previews, alternate stems, alternate mux outputs, QC variants, and export variants are currently surfaced, but base-only rerun/log routing makes those rows unreliable and undermines the review workflow.

## Scope

In scope:

- Variant-aware rerun routing for voice-preserving manifests, dub mixes, mux outputs, QC reports, and export packs.
- Disable `Rerun` for artifact rows that do not have a real rerun path.
- Match artifact status/log links by artifact identity (including variant label and container), not only by generic job type.
- Keep base artifact behavior unchanged.

Out of scope:

- Redesigning the entire artifacts browser UI.
- New artifact classes unrelated to current dubbing/export outputs.

## Acceptance criteria

- Every listed rerunnable variant/base artifact queues the correct matching job.
- Unsupported artifact rows do not expose a misleading `Rerun` action.
- A/B and alternate artifact rows show status/logs for the matching job run instead of the last generic job of that type.
- QC and export alternates remain variant-specific when rerun from the artifact browser.

## Test / verification plan

- Engine/Tauri tests for variant-aware enqueue helpers where applicable.
- Desktop build.
- Manual artifact-browser smoke notes captured in proof summary.

## Status updates

- 2026-03-06: Created from remediation review after static scan found base-only rerun/log plumbing in the artifact browser.
- 2026-03-06: Completed. Artifact reruns now reuse the matching prior job params via `jobs_retry`, item-scoped job metadata powers variant-aware status/log routing, and unsupported artifact rows no longer expose misleading rerun behavior. Proof: `product/desktop/Build Target/tool_artifacts/wp_runs/WP-0092/20260306_213944/summary.md`.
