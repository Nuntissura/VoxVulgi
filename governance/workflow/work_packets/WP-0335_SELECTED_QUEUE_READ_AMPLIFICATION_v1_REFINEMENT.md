---
file_id: WP-0335-refinement-v1
file_kind: refinement
updated_at: 2026-10-05
---
<topic id="selected-queue-read-remediation" wp="WP-0335" version="1" status="IN_PROGRESS">

The operator authorized autonomous repairs with subagents and WP status updates before advancing. Preserve before evidence under `product/desktop/build_target/tool_artifacts/wp_runs/WP-0324/20261005/normal_io_prearmed_a0bb600_01/`: `forced_fresh_execution_and_bounded_heavy_reads.json`, `samples.jsonl`, and `hot_query_plans_live_readonly.json`. Installed source a0bb600141fffcfb1f7274189917af11b17e562a produces substantial repeated counted reads in selected-pending, paused fetch and activity. Preliminary metadata-only SQLite3.50.4 EQP shows job-first selected existence/fetch and a non-covering activity sort; bundled proof remains required. No physical disk cause or resolved timeout is established.

Spec anchors: PRODUCT_SPEC.md Selected foreground downloads VV-0334-POLICY-001 through004, Jobs/Queue line464, bounded runtime line241 and section8.2 responsiveness/attribution; TECHNICAL_DESIGN.md Selected foreground admission, startup-only transactional migration/shared runtime lines65-72 and indexed current-work-first Jobs lines623-628. Canonical jobs, exact attempts and grants govern execution; UI pages/counts remain bounded projections.

Research: [optimizer](https://www.sqlite.org/optoverview.html) documents join reordering, CROSS JOIN loop control and bounded selection before expensive work; [query planning](https://www.sqlite.org/queryplanner.html) explains covering indexes; [rowid](https://www.sqlite.org/rowidtable.html) distinguishes TEXT PRIMARY KEY from rowid; [WITH](https://www.sqlite.org/lang_with.html) cautions against unnecessary materialization; [EQP](https://www.sqlite.org/eqp.html) output is unstable; [statement counters](https://www.sqlite.org/c3ref/stmt_status.html) supplement actual VFS bytes.

Selected approach: first prove bundled old/new exact equivalence and counted VFS. Use sparse grant-first selected existence and paused fetch with exact attempt/status and ordering before LIMIT. Preserve ordinary candidates after selected members when unpaused, fairness, recurring cohort order and legacy fallback. Adopt an activity covering index only after bundled equivalence/VFS counterfactual; use additive numbered transactional startup migration and preserve bounded payload retrieval. Reuse WP-0324 VFS, WP-0333 leases/startup gate, WP-0334 grants/claims and quiet bridge/native observer. Reject higher limits/timeouts, live user SQL, rowid ties, dropped attempts, payload indexes, blanket resume and Python-plan-only acceptance. No scheduler policy, version/changelog, cache security, UI redesign, hardware or ISO changes.

Red team/minimum controls: empty/stale/terminal grants require negative admission tests; mixed tracks/types, ordinal/time ties and hidden members require exact ordered IDs/payloads before LIMIT; continue-all retains ordinary candidates and existing Safe/pause/fairness tests; activity covers every source/view, totals, offsets/limits and text-id ties; index migration independently preserves protected rows, attempts, params and timestamps. Keep logical reads distinct from physical disk attribution and mandatory provider integrity scans; never weaken the VFS proof producer.

Root reviews before implementation, coordinates one warm test/build batch, performs retained-version Update and fresh quiet installed normal selected dispatch with unrelated work paused, independently reconciles attempts, affected VFS contexts and actual command p50 over the WP-0324 approximately30minute window, and opens paired Jobs snapshot/dump. WP acceptance and PROOF_STANDARD.md govern DONE; synchronize WP/board before advancing. No separate MT files are needed for this narrow remediation.

</topic>
