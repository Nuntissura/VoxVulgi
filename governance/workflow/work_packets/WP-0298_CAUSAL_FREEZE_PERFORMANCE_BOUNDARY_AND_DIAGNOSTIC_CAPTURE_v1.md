---
file_id: WP-0298-v1
file_kind: work-packet
updated_at: 2026-08-15
---

<topic id="contract" status="in-progress" version="v1" wp="WP-0298" updated_at="2026-08-15">

# Work Packet: WP-0298 — Causal freeze performance boundary and diagnostic capture

## Metadata

- ID: WP-0298
- Owner: agent-wp0298
- Status: IN_PROGRESS
- Created: 2026-08-09
- Refinement: `WP-0298_CAUSAL_FREEZE_PERFORMANCE_BOUNDARY_AND_DIAGNOSTIC_CAPTURE_v1_REFINEMENT.md`
- Board: `../TASK_BOARD.md#wp-0298`
- Dependencies: WP-0221, WP-0242, WP-0250, WP-0278, WP-0280

## Intent

Make job startup and panel navigation responsive on the exact large local database/NAS environment, and make every remaining freeze attributable to a measured UI, bridge, database, storage, child-process, WebView2, or host-pressure phase.

## Base scope

- Implement the refinement's incident/span contract, bounded diagnostic storage, persistent media observations, activity/archive rollups, stale-request guards, Diagnostics capture controls, and `vvwatch` correlation.
- Reproduce and remediate the exact operator job-start and panel-switch cases using the current canonical database and configured NAS root.
- Preserve all library metadata, subscriptions, playlists, jobs, media, and third-party databases.

## Required implementation order

1. RED fixtures and incident/span schema.
2. Bounded trace writer and operator capture surface.
3. Persistent media observations and derived rollups.
4. Panel request cancellation/guards and phase timing.
5. `vvwatch`/WebView2 correlation.
6. Exact live reproduction, measured remediation, governed build, and proof.

## Acceptance and proof

- Every acceptance criterion, red-team control, and verification requirement in the refinement is normative.
- The packet cannot be `DONE` from synthetic tests, build-only proof, or missing Worker events.
- Exact current-case proof must state target, expected condition, observed condition, and capture/query/validator.
- `governance/workflow/PROOF_STANDARD.md` and `build_rules.md` apply.

</topic>

<topic id="status-updates" status="active" version="v1" wp="WP-0298" updated_at="2026-08-09">

# Status updates

- 2026-08-09: Created from direct source, live read-only database, trace, existing proof, and current external diagnostic research. No product code, user data, queue state, NAS media, or running process changed.
- 2026-08-09: Implementation started in the first dependency-safe parallel wave; independent adversarial review is required before any completion claim.
- 2026-08-15: Independent adversarial review completed at HIGH risk. The initial packaged v0.1.169 incident probe failed causal operation identity because Diagnostics issued download and enumeration protection commands with the same request/span pair. Source was remediated to create exactly two stable contexts, preserving correlation within each operation while separating the operations; focused contracts pass 5/5. Overall WP remains IN_PROGRESS pending a governed build of the remediation and the refinement's exact current-database/NAS panel/job-start reproduction.
- 2026-08-15: Exact canonical-database panel proof advanced on packaged v0.1.169 with the 1,066,110,976-byte live database. A clean Options -> Media Library switch committed in 85 ms, mounted 964 controls/162 rows, recorded one 68 ms long task and zero Worker freezes while the asynchronous query completed in 3,105 ms. Separate html2canvas capture produced 1,091 ms and 339 ms freezes, proving capture self-interference must not be attributed to plain navigation. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0298/20260815_large_live_db_v169/summary.md`. Remaining exact case: job start; remaining performance decision: reconcile/remediate the 3.1-7.7 second query cost against the refinement threshold.

## Independent adversarial review — 2026-08-15

### DIFF_ATTACK_SURFACES

- Incident state/manifest writer versus restart loader/finalizer.
- Frontend panel transition producer versus Tauri trace envelope and command-phase consumers.
- Download versus enumeration protection operations sharing command names and formerly sharing causal identity.
- Persistent media-observation writer/read cache versus path-rewrite invalidation and exact execution-boundary refresh.
- Archive/activity event writers versus committed rollup readers and rebuild reconciliation.
- Async panel requests versus page/query-generation supersession.
- Bounded trace redaction/coalescing versus retained diagnostic meaning.

### INDEPENDENT_CHECKS_RUN

- Seeded an armed capture in an isolated v0.1.169 app root, launched the exact governed executable headlessly at BelowNormal priority, navigated through `/agent/navigate`, and directly inspected the canonical capture state, incident manifest, and JSONL trace.
- Verified a crash/restart-style expired active capture normalizes to `mode: normal` and finalizes its existing manifest as `completed_expired` with 18,342 incident bytes.
- Direct trace grouping exposed the download/enumeration identity collision; this check was not derived from the candidate tests.
- Ran `governance/scripts/test_vv_watch.ps1`; its isolated watcher fixture passed and preserved the exact armed incident ID.
- Ran the focused frontend suite; 19/19 passed after remediation.

### COUNTERFACTUAL_CHECKS

- If `activate_panel_capture_before_navigation` stopped activating before `setPage`, the destination's first Tauri commands would lose the incident parent span and exact panel attribution.
- If `projectionGenerationRef`/query-key guards in `JobsPage.tsx` or `LibraryPage.tsx` were removed, an older delayed response could overwrite the current page/filter projection.
- If `invalidate_media_path_observation_rewrite` stopped invalidating both old and replacement paths, relocation/rebind could retain a false availability observation until TTL/reconciliation.
- If the two `protectionContexts` in `DiagnosticsPage.tsx` collapsed to one context, the incident trace could not attribute protection cost to download versus enumeration.

### BOUNDARY_PROBES

- Armed-state JSON -> startup loader -> panel transition -> incident state -> manifest/trace artifact passed on the packaged executable.
- Active incident -> abrupt owned-process stop -> expired persisted state -> new packaged process -> `completed_expired` manifest passed.
- Frontend request/span fields -> Tauri command started/phase/completed rows -> incident parent span passed; operation identity failed before remediation and is now source-contract protected.

### NEGATIVE_PATH_CHECKS

- Expired persisted incident state recovered without leaving capture mode pinned or losing the prior artifact.
- Stale projection/error contracts passed: failed polls retain the last verified state rather than projecting false emptiness.
- Shared diagnostics redaction vectors passed for proxy authorization, quoted headers, spaced secrets, and malformed authorization tuples.

### INDEPENDENT_FINDINGS

- FINDING-1 (medium, remediated in source): download and enumeration protection status/history/replay requests used one request/span context, so the trace could not distinguish the operation responsible for latency. `DiagnosticsPage.tsx` now creates one stable operation-specific context for each operation; `causalFreezeDiagnosticsContract.test.ts` prevents collapse or per-call request drift.

### RESIDUAL_UNCERTAINTY

- The remediation is not present in packaged v0.1.169; a subsequent governed build and packaged incident re-probe must show distinct operation request/span IDs.
- Synthetic/isolated proof does not satisfy the packet's exact current 988 MB database, configured NAS, job-start, panel-switch, and any compositor-only reproduction gates.
- Rust tests were not rerun during this review because foreign Cargo/rustc processes were already saturating the host; the current governed executable and prior checkpoint compile establish buildability only for the pre-remediation source.

### Verdict

Code-level adversarial verdict: PASS after the operation-identity remediation. Overall WP verdict: NOT DONE until the residual runtime/build gates pass.

</topic>
