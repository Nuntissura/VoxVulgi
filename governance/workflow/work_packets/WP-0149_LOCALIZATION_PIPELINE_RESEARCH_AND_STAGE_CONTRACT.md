# Work Packet: WP-0149 - Localization pipeline research and stage contract

## Metadata
- ID: WP-0149
- Owner: Codex
- Status: DONE
- Created: 2026-03-12
- Target milestone: Post-0.1.6 localization-first recovery

## Intent

- What: Convert research on modern dubbing pipelines into a concrete VoxVulgi localization stage contract so the shipped path is explicit, inspectable, and not based on ad hoc backend assumptions.
- Why: The core product is still failing in operator use. The repo already contains many backend and artifact capabilities, but the installed localization workflow still behaves like a hidden multi-stage pipeline instead of a clear source-to-output product flow.

## Scope

In scope:

- Compare direct speech-to-speech research families against staged dubbing cascades for VoxVulgi's local-first Windows desktop constraints.
- Define the canonical shipped localization stage contract for VoxVulgi.
- Reconcile spec/design wording so localization recovery work is grounded in that stage contract.
- Queue follow-on implementation work against the research-backed contract rather than informal assumptions.

Out of scope:

- Shipping a new managed dubbing backend by itself.
- Replacing the current default with a direct speech-to-speech system in this packet.

## Acceptance criteria

- Research artifacts are written under `governance/research/localization_pipeline_20260312/`.
- `PRODUCT_SPEC.md` and `TECHNICAL_DESIGN.md` explicitly describe the shipped localization path as a stage-explicit cascade and explain the role of direct speech-to-speech systems as R&D/benchmark references rather than the immediate default.
- Follow-on localization work packets reference the stage contract explicitly.

## Test / verification plan

- Governance review of the resulting research docs and spec updates.
- Proof bundle summarizing research sources, resulting pipeline choice, and linked follow-on packets.

## Status updates

- 2026-03-12: Created to enforce research-first localization recovery instead of another speculative implementation pass.
- 2026-03-12: Completed. Research artifacts were added under `governance/research/localization_pipeline_20260312/`, spec/design were updated to the staged-cascade contract, and follow-on packets `WP-0143` and `WP-0150` were grounded on that contract.
