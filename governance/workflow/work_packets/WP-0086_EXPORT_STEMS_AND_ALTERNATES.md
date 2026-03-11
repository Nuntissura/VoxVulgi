# Work Packet: WP-0086 - Export stems and alternate versions

## Metadata
- ID: WP-0086
- Owner: Codex
- Status: DONE
- Created: 2026-03-06
- Target milestone: Phase 2 export maturity

## Intent

- What: Export dubbed speech only, background only, final mix, and alternate voice versions as first-class outputs.
- Why: Editors and teachers often need separate deliverables for post-processing, review, or alternate classroom versions.

## Scope

In scope:

- Stem export controls in Localization Studio.
- Alternate version naming and reveal/open support.
- Export-pack inclusion rules for stems and variants.

Out of scope:

- DAW-style multitrack editing inside VoxVulgi.
- External NLE integration plugins.

## Acceptance criteria

- Operators can export speech stem, background stem, final mix, and chosen alternates.
- Output locations are predictable and discoverable.
- Exported metadata makes each version clear.

## Test / verification plan

- Export-path tests.
- Desktop build.
- Manual smoke on stem and alternate export layout.

## Status updates

- 2026-03-06: Created to expand deliverables beyond one final preview mix.
- 2026-03-06: Completed. Speech stems and alternate variant outputs now appear as first-class item artifacts and are included in export-pack generation when present. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0086/20260306_204301/summary.md`.
