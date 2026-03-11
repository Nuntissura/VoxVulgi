# Work Packet: WP-0140 - Build output path normalization (no spaces)

## Metadata
- ID: WP-0140
- Owner: Codex
- Status: DONE
- Created: 2026-03-11
- Target milestone: Repo tooling path normalization

## Intent

- What: Rename the managed desktop build-output contract to no-space paths and make that a repo rule.
- Why: The current `Build Target` and `Old versions` paths are awkward to copy/paste and create avoidable friction in scripts, proof references, and operator workflows.

## Scope

In scope:

- Canonical path migration from `product/desktop/Build Target` to `product/desktop/build_target`.
- Canonical archive path migration from legacy `product/desktop/Build Target/Old versions` to `product/desktop/build_target/old_versions`.
- Governance/rules updates so the no-space contract is documented as the repo standard for managed build-output paths.
- Script and ignore-rule updates so build logs, archived releases, cleanup flows, and proof bundles continue to work.
- Safe on-disk migration of the existing ignored build-output folder.

Out of scope:

- Renaming unrelated runtime/app-data paths.
- Rewriting arbitrary user-generated filenames outside the managed build-output contract.

## Acceptance criteria

- Canonical repo docs point to `product/desktop/build_target/...` rather than `product/desktop/Build Target/...`.
- Managed build-output paths controlled by the repo no longer contain spaces.
- Desktop build, cleanup, proof, and changelog/update flows resolve to the new contract without breaking.
- Existing ignored desktop build outputs are migrated on disk without destructive loss.

## Test / verification plan

- Desktop/frontend build verification.
- Engine and Tauri verification to confirm repo changes did not break normal checks.
- Dry-run cleanup verification against the new path contract.
- Focused path-migration evidence showing the on-disk folder moved to the new canonical location.

## Status updates

- 2026-03-11: Created from operator feedback requesting a repo-wide no-spaces build-output policy and explicit protection against breaking tests/scripts/checks/artifact creation.
- 2026-03-11: Completed. Canonical desktop build-output paths now use `product/desktop/build_target` and `old_versions`, the ignored on-disk folder was migrated safely, and build/cleanup/proof docs plus scripts were updated without breaking verification flows. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0140/20260311_135422/`.
