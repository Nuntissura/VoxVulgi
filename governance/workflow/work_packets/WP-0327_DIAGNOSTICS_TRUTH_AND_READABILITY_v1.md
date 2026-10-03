---
file_id: WP-0327
file_kind: work_packet
updated_at: 2026-10-03
---

# Work Packet: WP-0327 — Diagnostics truth and readability

## Metadata
- ID: WP-0327
- Owner: Codex diagnostics_repair
- Status: DONE
- Created: 2026-10-03
- Refinement: WP-0327_DIAGNOSTICS_TRUTH_AND_READABILITY_v1_REFINEMENT.md
- Board: ../TASK_BOARD.md
- Related: WP-0311, WP-0320; historical DONE record retained, diagnosed Diagnostics defects remediated here.

## Intent and scope
Implement operator-approved work-list items 2 and 5: truthful independent check outcomes, terminal failures instead of permanent checking, installation history separated from current readiness, compact readable Diagnostics with no new cards. No runtime repair, process disruption, user-data mutation, version/changelog changes or new probe scheduler.

## Files and constraints
DiagnosticsPage.tsx; diagnosticsResults.ts; DiagnosticsPage.css; focused helper tests. Preserve demand-generation guards, native probe bounds, two-heavy-probe coordinator limit, stable navigation anchors and existing product actions.

## Acceptance criteria
- Each successful command commits independently even when a sibling fails; all failed commands have named terminal errors.
- Failed/timeout/superseded capability probes cannot report missing, CPU, CUDA absence or device recommendations as verified facts.
- Unrequested state is not labelled checking; active queued/loading states are; failure is unknown/failed.
- Installation attempt history is explicitly distinguished from current package readiness.
- Native expandable sections reduce scrolling and card count; paths wrap, ordinary words do not break mid-word; existing actions remain reachable.
- Focused tests, desktop type/build gate and exact packaged headless visual plus semantic interaction proof pass before DONE.

## Verification
Root bundles node --import tsx --test tests/diagnosticsResults.test.ts with existing diagnostics demand/coordinator tests and desktop build. Root performs hidden packaged-app snapshots and semantic disclosure/retry actions. Keep IN_PROGRESS until proof-standard summary exists with all acceptance evidence.

## Status updates
- 2026-10-03: Created from inspected live v0.1.205 defects and source evidence; implementation in parallel with database, runtime and quiet-launch remediation. No app processes stopped.

## Final proof reconciliation (2026-10-03)

- DONE: Six acceptance criteria mapped: independent partial/shared-flight and unknown-state tests, truthful terminal/history/readability boundary, final corrected headless reason, final type/build checks. Prior unchanged Diagnostics inputs reused explicitly; no claim of all sections/failure injections.
- Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0327/20261003_final_9f8a137/summary.md`; final automated/build observation: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0331/20261003/summary.md`.
