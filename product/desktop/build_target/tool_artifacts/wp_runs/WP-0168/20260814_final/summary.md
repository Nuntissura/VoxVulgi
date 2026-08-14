# WP-0168 Summary

Status: DONE
Date: 2026-08-14

## Outcome
- Developer-only test-job controls and cleanup are hidden in the `Queue status, cleanup, and developer tools` disclosure rather than the default Jobs view.
- Current rows expose primary Retry/Cancel actions while secondary actions are grouped under `More…`; source titles and readable source group labels replace an ID-only wall.
- Cleanup performs a read-only preview and explicit staged confirmations before deleting terminal history, logs, artifacts, or output folders.

## Verification
- Governed v0.1.138 build completed successfully.
- Navigated to Jobs through the hidden bridge, waited for the settled view, captured and directly inspected `jobs_confirmed_1786712911289.png`.
- Headless semantic audit returned 222/222 candidates, `truncated=false`, and zero missing accessible names.
- The default audit contained no `Run test job` control; it exposed the collapsed developer-tools disclosure instead.
- Current job/source groups exposed `Expand`, `Retry unfinished`, and `More…`; individual attempt rows exposed primary `Cancel`/`Retry` plus `More…`.
- Inspected `flushCache()` and confirmed it calls `jobs_cleanup_preview`, then asks separate explicit confirmations for terminal/log/artifact/cache cleanup, managed output folders, and external output folders before invoking deletion.

## Evidence
- `evidence.json`
- `governance/snapshots/WP-0171_build_0_1_138/jobs_confirmed_1786712911289.png`
- `product/desktop/src/pages/JobsPage.tsx`
- `product/desktop/build_target/logs/build_desktop_target_20260814-143555_0_1_138.log`

## Notes
- No cleanup or other mutating Jobs action was executed against the operator's live database.
