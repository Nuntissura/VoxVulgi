# Work Packet: WP-0274 - Granular UI refresh and storage-contention hardening

## Metadata

- ID: WP-0274
- Owner: Codex
- Status: DONE
- Created: 2026-07-22
- Refinement: `WP-0274_GRANULAR_UI_REFRESH_AND_STORAGE_CONTENTION_HARDENING_v1_REFINEMENT.md`
- Dependencies: `WP-0272`, `WP-0273`

## Intent

Keep progress fluid and the shell responsive by separating small live projections from expensive library, archive, and storage work, then prove the actual contention path.

## Scope

- Poll inventory and visible-surface gating.
- Lightweight active projections and stable keyed reconciliation.
- Cached/incremental heavy stats and bounded storage health checks.
- Freeze/command/query diagnostics, installed-app external-watch and headless proof.

## Acceptance criteria

- All refinement criteria and red-team controls pass.
- No NAS-only root-cause claim is made without exact evidence.
- Subscriptions remain independently runnable during foreground work.
- Proof bundle satisfies `PROOF_STANDARD.md` before `DONE`.

## Verification

- Frontend poll lifecycle/identity tests; engine query-plan/cache/storage-timeout tests.
- Representative backlog trace comparison.
- Installed-app bridge snapshots/dumps, `vvfreeze.cmd` where relevant, and sibling `vvwatch.cmd` without foreground focus or process interference.

## Status updates

- 2026-07-22: Evidence-grounded contract created before product-code edits; queued after canonical repair work.
- 2026-07-23: DONE in desktop v0.1.107. Lightweight progress projections, split polling cadences, visibility gating, bounded cached storage observation, managed build, and hidden app-boundary proof passed. No NAS-only root-cause claim is made. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0274/summary.md`.
