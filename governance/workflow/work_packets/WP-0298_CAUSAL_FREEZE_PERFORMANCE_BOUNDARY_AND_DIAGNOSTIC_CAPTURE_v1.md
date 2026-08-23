---
file_id: WP-0298-v1
file_kind: work-packet
updated_at: 2026-08-23
---

<topic id="contract" status="in-progress" version="v1" wp="WP-0298" updated_at="2026-08-23">

# Work Packet: WP-0298 — Causal freeze performance boundary and diagnostic capture

## Metadata

- ID: WP-0298
- Owner: agent-wp0298
- Status: IN_PROGRESS
- Created: 2026-08-09
- Refinement: `WP-0298_CAUSAL_FREEZE_PERFORMANCE_BOUNDARY_AND_DIAGNOSTIC_CAPTURE_v1_REFINEMENT.md`
- Board: `../TASK_BOARD.md`
- Dependencies: WP-0221, WP-0242, WP-0250, WP-0278, WP-0280
- Coordinated remediation packets: WP-0311, WP-0312, WP-0313, WP-0314
- Independent database decision packet: WP-0315

## Intent

Make job startup and panel navigation responsive on the exact large local database/NAS environment, and make every remaining freeze attributable to a measured UI, bridge, database, storage, child-process, WebView2, or host-pressure phase.

## Base scope

- Implement the refinement's incident/span contract, bounded diagnostic storage, persistent media observations, activity/archive rollups, stale-request guards, Diagnostics capture controls, and `vvwatch` correlation.
- Reproduce and remediate agent-driven panel switching and job-start/write scenarios only against a verified clone, an owned `VOXVULGI_AGENT_HEADLESS_BASE_DIR`, or an owned disposable normal-window VM/snapshot. Exact current-profile proof observes panel switching and job start only on an already operator-started process, without the agent launching, navigating, initiating, altering, or stopping it, unless the operator separately authorizes one unchanged live case with named mutations/outputs and before/after canonical reconciliation.
- Preserve all library metadata, subscriptions, playlists, jobs, media, and third-party databases.

## Required implementation order

1. RED fixtures and incident/span schema.
2. Bounded trace writer and operator capture surface.
3. Persistent media observations and derived rollups.
4. Panel request cancellation/guards and phase timing.
5. `vvwatch`/WebView2 correlation.
6. Exact current-profile operator-initiated/agent-observation-only evidence plus isolated/disposable write reproduction, measured remediation, governed build, and proof.

## Acceptance and proof

- Every acceptance criterion, red-team control, and verification requirement in the refinement is normative.
- The packet cannot be `DONE` from synthetic tests, build-only proof, or missing Worker events.
- Exact current-case proof must state target, expected condition, observed condition, capture/query/validator, who initiated any job, and whether the data root was canonical or disposable. Current-profile mutation is not authorized by this packet alone.
- `governance/workflow/PROOF_STANDARD.md` and `build_rules.md` apply.

</topic>

<topic id="status-updates" status="active" version="v1" wp="WP-0298" updated_at="2026-08-23">

# Status updates

- 2026-08-09: Created from direct source, live read-only database, trace, existing proof, and current external diagnostic research. No product code, user data, queue state, NAS media, or running process changed.
- 2026-08-09: Implementation started in the first dependency-safe parallel wave; independent adversarial review is required before any completion claim.
- 2026-08-15: Independent adversarial review completed at HIGH risk. The initial packaged v0.1.169 incident probe failed causal operation identity because Diagnostics issued download and enumeration protection commands with the same request/span pair. Source was remediated to create exactly two stable contexts, preserving correlation within each operation while separating the operations; focused contracts pass 5/5. Overall WP remains IN_PROGRESS pending a governed build, operator-initiated/agent-observation-only current-profile panel proof, isolated job-start write proof, and observation of an operator-initiated exact current job.
- 2026-08-15: A packaged v0.1.169 run against the then-1,066,110,976-byte canonical database recorded an 85-ms Options -> Media Library commit, 964 controls/162 rows, one 68-ms long task, a 3,105-ms asynchronous query, and html2canvas-associated 1,091-ms/339-ms Worker stalls. On 2026-08-23 this run was durably invalidated for current-profile/non-mutation closure because an agent-controlled headless process was launched against canonical operator app data. Preserve its measurements only as historical timing and observer-interference evidence; it cannot satisfy WP-0298, WP-0314, or WP-0315 proof. Supersession receipt: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0298/20260815_large_live_db_v169/summary.md`.
- 2026-08-23: The governed v0.1.179 runtime closed the earlier operation-identity residual: download and enumeration protection work now has distinct request/span identities in packaged trace evidence. Do not reopen that completed causal-identity fix.
- 2026-08-23: Corrected the startup interpretation from exact timestamps. Database-first startup passed. Offline hydration then ran for 575,965 ms and reached `ready`; Worker and main-thread heartbeat payload generation continued and rows were persisted later in a burst. This proves delayed transport/ingress/persistence observability, not dead Worker execution or permanently hung hydration. WP-0313 owns verification scheduling and emitted/received/persisted timing.
- 2026-08-23: The exact v0.1.179 normal-window watch observed 127/127 native-window `Responding=false` samples and 127/127 successful bridge probes for the same app PID, plus ten overlapping commands, three Torch/CUDA capability probes, duplicate Demucs/module discovery pressure, nine watcher database-probe timeouts, and one app-side SQLite lock error. These are correlated findings, not one proven causal chain. WP-0311 owns demand/lifetime/single-flight, WP-0312 owns SQLite access/lock attribution, and WP-0314 owns native-window/WebView2/ETW attribution.
- 2026-08-23: WP-0298 remains the integration owner. Its remaining closure is fresh agent-observation-only panel evidence from an already operator-started process, isolated job-start write proof plus agent-observation-only evidence of an operator-initiated exact current job, query-latency budget reconciliation, and final current-build incident proof after the coordinated remediation packets advance. WP-0315 is an independent SurrealDB/RocksDB decision experiment and cannot substitute for freeze closure.

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
- Synthetic/isolated proof alone does not satisfy the current-profile panel/operator-initiated-job observation or compositor-only gates; conversely, exact current-profile observation does not authorize agent-initiated navigation, clicks, writes, or process control. The 2026-08-15 agent-launched canonical-data run is invalid for closure and historical-only. The dated 988 MB snapshot has also been superseded by the 2026-08-23 schema-v54 inventory above 1.1 GB, so proof records a fresh immutable database identity rather than reusing either old byte count.
- Rust tests were not rerun during this review because foreign Cargo/rustc processes were already saturating the host; the current governed executable and prior checkpoint compile establish buildability only for the pre-remediation source.

### Verdict

Code-level adversarial verdict: PASS after the operation-identity remediation. Overall WP verdict: NOT DONE until the residual runtime/build gates pass.

</topic>
