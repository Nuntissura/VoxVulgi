# Work Packet: WP-0151 - Localization runtime failure analysis and reference strategy

## Metadata
- ID: WP-0151
- Owner: Codex
- Status: DONE
- Created: 2026-03-12
- Target milestone: Post-0.1.6 localization-first recovery

## Intent

- What: Convert the current localization failures into a research-grounded runtime diagnosis and define the next practical recovery step for first-dub success.
- Why: The repo already contains a staged localization pipeline, but the installed operator path still fails because speaker-reference acquisition is too manual and too hidden.

## Scope

In scope:

- Reconcile current repo behavior, recent installer smoke findings, and current primary-source guidance on staged dubbing pipelines, source separation, and reference quality.
- Document why the next recovery step is assisted speaker-reference acquisition rather than another blind backend swap.
- Update spec/design and queue the follow-on implementation packet.

Out of scope:

- Shipping the follow-on implementation itself.
- Changing the managed default backend family in this packet.

## Acceptance criteria

- Research findings are written under `governance/research/localization_pipeline_20260312/`.
- Spec/design explicitly mention assisted speaker-reference extraction as the next practical bridge to a first working dub.
- A concrete follow-on implementation packet is queued from those findings.

## Test / verification plan

- Governance review of the resulting research note and linked spec updates.
- Proof bundle summarizing the diagnosis, sources, and resulting implementation direction.

## Status updates

- 2026-03-12: Created to enforce another research-backed checkpoint before deeper localization implementation.
- 2026-03-12: Completed. Added runtime failure analysis and reference-acquisition strategy research, updated spec/design, and queued `WP-0152`.
