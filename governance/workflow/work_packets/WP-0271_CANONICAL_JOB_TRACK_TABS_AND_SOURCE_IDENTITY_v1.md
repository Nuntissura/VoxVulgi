# Work Packet: WP-0271 - Canonical job track tabs and source identity

## Metadata

- ID: WP-0271
- Owner: Codex
- Status: DONE
- Created: 2026-07-22
- Refinement: `WP-0271_CANONICAL_JOB_TRACK_TABS_AND_SOURCE_IDENTITY_v1_REFINEMENT.md`
- Dependencies: `WP-0268`, `WP-0269`, `WP-0270`

## Intent

Give each canonical work track a truthful Jobs/Queue subtab and make subscription source identity durable on every job state.

## Scope

- Backend-selected bounded track views and search.
- Six no-card Jobs subtabs plus All.
- Enqueue-time subscription channel/playlist/page snapshots with retry inheritance.
- Stable element IDs, bridge/dump observability, migrations/backfill, and tests.

## Acceptance criteria

- All acceptance and red-team controls in the refinement pass.
- The exact selected track is proven against canonical jobs outside the initial unfiltered slice.
- Source names survive never-started, failure, retry, success, and source-edit/delete paths.
- Proof bundle satisfies `PROOF_STANDARD.md` before `DONE`.

## Verification

- Engine migration/query/backfill/retry tests and representative query plans.
- Frontend contract tests for every tab and source fallback.
- Installed-app headless snapshots/dumps at normal and narrow widths.

## Status updates

- 2026-07-22: Contract activated before product-code edits, per operator sequencing.
- 2026-07-23: DONE in desktop v0.1.107. Backend-selected canonical tabs, durable source snapshots, focused tests, and hidden app-boundary Jobs snapshot/dump passed. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0271/summary.md`.
