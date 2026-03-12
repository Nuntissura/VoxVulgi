# Work Packet: WP-0145 - Localization advanced surfaces discoverability

## Metadata
- ID: WP-0145
- Owner: Codex
- Status: BACKLOG
- Created: 2026-03-12
- Target milestone: Post-0.1.6 smoke regression recovery

## Intent

- What: Make the already-built benchmark, backend, QC, rerun, cleanup, and batch dubbing surfaces discoverable enough that operators can actually find and use them.
- Why: The latest smoke shows these features are functionally invisible in normal UI flow even though the repo contains substantial implementation for them.

## Scope

In scope:

- Audit the current discoverability of benchmark, backend, QC, variant rerun, cleanup, and batch dubbing controls in Localization Studio and Diagnostics.
- Add clear entry points, labels, and progression hints so operators can reach the advanced surfaces from the normal localization workflow.
- Ensure benchmark winner promotion into reusable template/cast-pack defaults is visible from the operator path.
- Ensure experimental backend adapter features are visible enough to understand without source-code knowledge.

Out of scope:

- New benchmark/backends functionality that does not improve operator discoverability.

## Acceptance criteria

- Operators can find benchmark/QC/backend/cleanup/batch surfaces through obvious UI entry points.
- Benchmark winner promotion is visible and understandable in the normal localization workflow.
- Advanced surfaces no longer depend on hidden state or guesswork to appear.

## Test / verification plan

- Desktop app-boundary smoke focused on discoverability and path-to-action.
- Desktop build verification.
- Proof bundle with the final operator path for each advanced surface.

## Status updates

- 2026-03-12: Created from smoke findings `ST-035`, `ST-036`, `ST-037`, `ST-038`, `ST-039`, and `ST-040`.
