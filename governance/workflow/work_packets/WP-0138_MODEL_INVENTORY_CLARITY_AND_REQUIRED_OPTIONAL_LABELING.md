# Work Packet: WP-0138 - Model inventory clarity and required/optional labeling

## Metadata
- ID: WP-0138
- Owner: Codex
- Status: DONE
- Created: 2026-03-09
- Target milestone: Installer smoke remediation tranche

## Intent

- What: Clarify the Diagnostics model/tool inventory so required, optional, demo, bundled, hydrated, and manually installable items are obvious.
- Why: Installer smoke shows that the current model inventory is technically correct but operator-confusing, especially around the `demo-ja-asr` placeholder versus the actual bundled `whispercpp-tiny` dependency.

## Scope

In scope:

- Required versus optional labeling for models and packs.
- Demo/test asset labeling or relocation so it cannot be mistaken for a missing real dependency.
- Improved operator copy for bundled/hydrated/manual install state.

Out of scope:

- Replacing the underlying ASR model.

## Acceptance criteria

- Diagnostics makes it obvious which models are required, optional, or demo/test only.
- Bundled and hydrated dependencies are labeled as such.
- Operators do not mistake placeholder/demo assets for failed automatic installation.

## Test / verification plan

- Desktop build and app-boundary diagnostics verification.
- Proof bundle with inventory examples and copy changes.

## Status updates

- 2026-03-09: Created from installer smoke findings around the confusing Models panel.
- 2026-03-09: Implemented explicit role/delivery/expected-install model metadata, separated required runtime models from demo/test assets in Diagnostics, and wrote proof under `product/desktop/Build Target/tool_artifacts/wp_runs/WP-0138/20260309_060022/`.
