# Work Packet: WP-0095 - Localization Studio manual app smoke

## Metadata
- ID: WP-0095
- Owner: Codex
- Status: BACKLOG
- Created: 2026-03-07
- Target milestone: Phase 3 voice-workflow hardening

## Intent

- What: Run a real in-app Localization Studio smoke on the remediated voice-workflow paths and capture operator-facing proof.
- Why: `WP-0092` to `WP-0094` were verified through code review, tests, and builds, but not yet through a fresh GUI click-through against the actual app surface.

## Scope

In scope:

- Manual app smoke for variant-aware artifact reruns, status, and log routing from `WP-0092`.
- Manual app smoke for multi-reference cleanup source selection and non-destructive apply behavior from `WP-0093`.
- Manual app smoke for large-library batch dubbing selection and persistence from `WP-0094`.
- Capture proof artifacts, notes, and any operator-visible defects discovered during the smoke.

Out of scope:

- New product features unrelated to the existing remediation tranche.
- Broad UI redesign work beyond fixes needed to satisfy the smoke acceptance criteria.
- Exhaustive codec/model/provider coverage across every pipeline permutation.

## Acceptance criteria

- A/B or alternate artifact rows in Localization Studio rerun the correct matching jobs and expose the correct status/log links during a manual click-through.
- A speaker with 3 or more references can run cleanup on a selected source clip and apply the cleaned result without collapsing the broader reference set.
- Batch selection remains stable when paging through and selecting more than 500 library items in the app.
- A proof summary is written under `product/desktop/Build Target/tool_artifacts/wp_runs/WP-0095/` with the exact scenarios exercised, evidence paths, and any follow-up defects.
- If smoke defects are found, they are tracked as explicit follow-up work packets before fixes are claimed complete.

## Test / verification plan

- Manual Localization Studio smoke on real or representative media/library data.
- Capture proof notes, screenshots or artifact paths, and any job/log references under the WP proof folder.
- If the smoke uncovers code fixes, re-run affected automated checks plus the focused manual scenario before promoting the WP to `DONE`.

## Status updates

- 2026-03-07: Created from the post-remediation next-step recommendation to add real app-level proof for `WP-0092` to `WP-0094`.
