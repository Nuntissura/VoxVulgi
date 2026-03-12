# Work Packet: WP-0095 - Localization Studio manual app smoke

## Metadata
- ID: WP-0095
- Owner: Codex
- Status: BLOCKED
- Created: 2026-03-07
- Target milestone: Phase 3 voice-workflow hardening

## Intent

- What: Run a real in-app Localization Studio smoke on the remediated voice-workflow and voice-backend operator paths and capture operator-facing proof.
- Why: `WP-0092` to `WP-0094`, `WP-0109`, and `WP-0112` to `WP-0117` were verified through code review, tests, and builds, but not yet through one fresh GUI click-through against the actual app surface.

## Scope

In scope:

- Manual app smoke for variant-aware artifact reruns, status, and log routing from `WP-0092`.
- Manual app smoke for multi-reference cleanup source selection and non-destructive apply behavior from `WP-0093`.
- Manual app smoke for large-library batch dubbing selection and persistence from `WP-0094`.
- Manual app smoke for benchmark report generation, ranking, and artifact discovery from `WP-0109`.
- Manual app smoke for item voice-plan promotion/apply behavior from `WP-0112`.
- Manual app smoke for experimental backend render runs and manifest/report discovery from `WP-0113`.
- Manual app smoke for bounded batch experimental backend runs from `WP-0114`.
- Manual app smoke for compare-history and leaderboard export surfaces from `WP-0115`.
- Manual app smoke for starter-recipe application in Diagnostics from `WP-0116`.
- Manual app smoke for promoting a benchmark winner into reusable template/cast-pack defaults from `WP-0117`.
- Capture proof artifacts, notes, and any operator-visible defects discovered during the smoke.

Out of scope:

- New product features unrelated to the existing remediation tranche.
- Broad UI redesign work beyond fixes needed to satisfy the smoke acceptance criteria.
- Exhaustive codec/model/provider coverage across every pipeline permutation.

## Acceptance criteria

- A/B or alternate artifact rows in Localization Studio rerun the correct matching jobs and expose the correct status/log links during a manual click-through.
- A speaker with 3 or more references can run cleanup on a selected source clip and apply the cleaned result without collapsing the broader reference set.
- Batch selection remains stable when paging through and selecting more than 500 library items in the app.
- A benchmark run can be generated and inspected in-app, with the visible ranking matching the durable report artifacts.
- Item voice-plan promotion, experimental backend runs, compare-history snapshots, starter-recipe application, and benchmark-winner promotion all have one exercised manual path with operator-facing notes.
- A proof summary is written under `product/desktop/build_target/tool_artifacts/wp_runs/WP-0095/` with the exact scenarios exercised, evidence paths, and any follow-up defects.
- If smoke defects are found, they are tracked as explicit follow-up work packets before fixes are claimed complete.

## Test / verification plan

- Manual Localization Studio smoke on real or representative media/library data and at least one configured experimental backend adapter path.
- Capture proof notes, screenshots or artifact paths, and any job/log references under the WP proof folder.
- If the smoke uncovers code fixes, re-run affected automated checks plus the focused manual scenario before promoting the WP to `DONE`.

## Status updates

- 2026-03-07: Created from the post-remediation next-step recommendation to add real app-level proof for `WP-0092` to `WP-0094`.
- 2026-03-08: Scope expanded by `WP-0124` so the manual smoke also covers the current benchmark, voice-plan, experimental-backend, compare-history, recipe, and benchmark-promotion surfaces added after the original remediation tranche.
- 2026-03-12: Blocked by post-0.1.6 smoke regressions now queued in `WP-0142` to `WP-0150`; manual closeout should resume after those packets land.
- 2026-03-12: The blocked shell-smoke dependency explicitly includes verifying the move affordance and minimize/maximize/close controls are restored to the intended top-right chrome cluster.
