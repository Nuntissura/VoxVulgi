# Work Packet: WP-0206 - Clear failed localization runs per item

## Metadata
- ID: WP-0206
- Owner: Codex
- Status: IN_PROGRESS
- Created: 2026-04-25
- Target milestone: Localization operator usability

## Intent

- What: Give operators an explicit, non-destructive way to clear failed localization run history for a workspace item, so old failed jobs no longer permanently clutter the studio.
- Why: Today the only cleanup affordance is the global `jobs_flush_cache` in Diagnostics, which deletes all terminal job history across the app. There is no way to clear only the failed runs of a specific localization item. Failed runs accumulate, dominate the recent-item status detail, and visually overpower successful deliverables. Operators reported "old failed runs in the localization studio have no way to be cleared".

## Scope

In scope:
- Engine: add `jobs_clear_failed_for_item(item_id, options)` that deletes only `status = 'failed'` rows for the given item in the jobs DB. Successful and running jobs are not touched. Returns a small summary `{ removed_jobs, removed_log_files }`.
- Engine: under an explicit `purge_orphan_artifacts: true` option, also remove working artifacts that are uniquely produced by those failed jobs (i.e., not referenced by any surviving job or deliverable). Default is `false` to honor "non-destructive by default" / user-data preservation.
- Tauri: thin command wrapper exposing the engine call.
- Frontend: add a `Clear failed runs` action on the recent-item card in `LocalizationStudioHome`, gated behind a `confirm()` dialog, with optional checkbox `Also remove orphan working artifacts`. After running, refresh the affected item's status.
- Frontend: surface a per-item failed-run count in the recent-item card (read from existing `jobs_list_for_item`) so the action is only enabled when there is something to clear.

Out of scope:
- Bulk "clear failed runs across all items" (can come later if the per-item action proves valuable).
- Cleanup of Jobs page history beyond the engine call.
- Any change to deliverable retention - deliverables are never touched by this WP.

## Acceptance criteria

- Operators can right-click / button-click on a recent localization item and clear its failed run history without affecting other items, successful runs, or queued/running work.
- Default behavior leaves working artifacts on disk (non-destructive); the orphan-artifact purge is opt-in per click.
- The recent-item card shows a failed-run count, the action is disabled when zero, and the count goes to zero after a successful clear.
- `cargo check` (engine) and desktop `npm run build` pass.

## Test / verification plan

- Engine unit test: seed a small sqlite jobs DB with mixed-status rows for two items, call `jobs_clear_failed_for_item(item_a)`, assert only failed rows for item_a were removed and item_b is untouched.
- Engine unit test: with `purge_orphan_artifacts: true`, assert artifacts referenced by surviving jobs/deliverables are preserved.
- Manual: reproduce a failed run on the Queen sample, confirm the recent-item card shows the failed count, click `Clear failed runs`, confirm the item returns to a clean state without losing successful tracks/deliverables.
- Save a snapshot via the agent bridge before and after clear under `governance/snapshots/WP-0206/`.

## Risks / open questions

- Risk: deleting log files for failed jobs loses post-mortem evidence; mitigated by writing the cleanup summary into the diagnostics trace before deletion.
- Risk: orphan-artifact detection must be conservative (only purge artifacts whose only producing job was deleted). Default-off keeps this risk low for the first pass.

## Status updates

- 2026-04-25: Created. Implementation pass started in parallel with WP-0205.
