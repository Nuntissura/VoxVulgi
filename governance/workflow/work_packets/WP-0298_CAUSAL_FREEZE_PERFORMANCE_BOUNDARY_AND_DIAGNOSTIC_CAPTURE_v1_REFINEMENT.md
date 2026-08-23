---
file_id: WP-0298-REFINEMENT-v1
file_kind: work-packet-refinement
updated_at: 2026-08-23
---

<topic id="operator-request-and-verified-state" status="active" version="v1" wp="WP-0298" updated_at="2026-08-09">

# Operator request

- Investigate and remediate frequent freezes when jobs start or panels change.
- Determine whether the large database or NAS connection is responsible.
- Upgrade internal Diagnostics and `vvwatch` until freezes, slowdowns, bottlenecks, and causes can be distinguished without operator guesswork.
- Provide an operator-facing settings/tool surface for bounded performance capture.

# Verified state as of 2026-08-09

- The database size/schema/counts below are a dated incident snapshot, not current topology. The 2026-08-23 inventory in WP-0312/WP-0315 supersedes those mutable fields with a schema-v54 snapshot above 1.1 GB; the dated latency/trace observations remain historical evidence.

- The canonical SQLite database is local at `%APPDATA%\com.voxvulgi.voxvulgi\db\app.sqlite`; it is not stored on the NAS.
- The inspected database is 988,893,184 bytes, schema version 31, WAL mode, with 315,716 jobs and 141,117 library items.
- Job status counts are 256,342 canceled, 31,261 failed, 19,644 succeeded, 8,461 queued, and 8 running at inspection time.
- The configured media root is NAS-backed: `\\?\UNC\MIR\home\Video\4K Video\4K Video 21-08-2025`.
- In the latest inspected 12,000 trace rows, `library_download_preflight` recorded 934 slow calls with p50 1,318 ms, p95 4,867 ms, and max 14,384 ms.
- The same trace window recorded `jobs_overview` up to 22,607 ms, `subscription_download_activity` up to 40,727 ms, `youtube_subscriptions_archive_stats` up to 42,849 ms, and `youtube_subscription_videos` up to 16,830 ms.
- `library_download_preflight` calls `observe_media_path`, which can create one filesystem-probe thread per uncached path and wait up to 1.5 seconds. The short cache is process-local and expires after 30 seconds.
- `youtube_subscriptions_archive_stats` periodically reopens and counts every per-subscription archive file after its 30-second process-local cache expires.
- The inspected trace sample contained one `database_locked` row, regular `worker_alive` rows, and no `freeze_detected`, `freeze_recovered`, or `event_loop_skew` rows. This proves long data commands under a live Worker; it does not prove the reported visual freeze cannot also involve WebView2/DWM composition.
- `diagnostics_trace.jsonl` was 281,132,615 bytes, demonstrating that the current diagnostic stream is not sufficiently bounded.
- The app was not running during final investigation, so the exact operator-observed job-start/panel-switch case has not yet been reproduced under a fresh incident capture.

# Authority and spec anchors

- `AGENTS.md`: Built-in Visual Debugger, Headless Agent Bridge, Freeze Report, and Sibling External Watch.
- `build_rules.md`: quiet app-boundary verification, no focus/keyboard theft, no new cards.
- `governance/workflow/PROOF_STANDARD.md`: UI/operator-heavy packets require app-boundary/manual evidence.
- `governance/spec/PRODUCT_SPEC.md` section 8.2.
- `governance/spec/TECHNICAL_DESIGN.md` section 6.6.
- Existing packets to extend rather than replace: WP-0221, WP-0223, WP-0224, WP-0226, WP-0242, WP-0250, WP-0278, and WP-0280.

# Scope edges

- In scope: causal instrumentation, bounded incident capture, trace rotation, `vvwatch` correlation, panel/job-start performance boundaries, persistent media-observation state, archive/activity rollups, stale-request cancellation, and exact measured remediation.
- In scope: settings under Diagnostics for normal versus incident capture and arming the next job-start/panel-switch capture.
- Safety boundary: every agent-started headless process uses a preflighted owned absolute `VOXVULGI_AGENT_HEADLESS_BASE_DIR`; every agent-started normal-window process runs only in an owned disposable VM/snapshot; and agent-initiated job-start/write scenarios use a verified clone or disposable app-data/output root with independent canonical reconciliation on that isolated state. Exact current-profile panel/job proof attaches only to an already operator-started case; the agent does not launch, navigate, click, start, queue, mutate, or stop it. If agent initiation on the current profile is genuinely indispensable, stop and obtain separate explicit operator authority naming the unchanged case, expected rows/files/outputs, and before/after independent canonical reconciliation.
- Non-goals: moving the SQLite database, moving/deleting NAS media, deleting job history, altering queue semantics, closing operator processes, always-on heavyweight ETW capture, or treating a faster synthetic database as proof for the operator case.

</topic>

<topic id="corrected-evidence-and-successor-boundaries-2026-08-23" status="active" version="v1" wp="WP-0298" updated_at="2026-08-23">

# Corrected evidence and successor boundaries — 2026-08-23

- Governed v0.1.179 packaged trace evidence now distinguishes the download and enumeration protection operations with separate request/span identities. That closes the residual operation-identity build/re-probe item recorded on 2026-08-15.
- WP-0310's database-first startup gate passed before background startup components. It is a completed hard predecessor and must not be reopened or moved later for performance.
- Offline hydration started at 1787456942629 and reached `ready` at 1787457518594, a duration of 575,965 ms. Worker heartbeat payload generation and main-thread heartbeat ticks continued during the apparent trace gap and were persisted after hydration. Any older implication that hydration never terminated or the Worker died is superseded by this exact evidence.
- The governed normal-window watch at `watch_20260823-055128` observed 127 of 127 native-window not-responding samples while all bridge probes succeeded for the same PID. It also observed command overlap, heavy Python capability probes, database-probe timeouts, and host/sample scheduling lag. These are parallel observations. No causal arrow among them is established without the successor packets' receipts.
- The 2026-08-15 v0.1.169 large-database headless run launched an agent-controlled process against canonical operator app data. Its proof summary is now marked `historical-only` and invalidated for current-profile/non-mutation closure. Its timings may inform hypotheses and observer-cost accounting, but it cannot satisfy WP-0298, WP-0314, or WP-0315 proof; fresh current-profile panel evidence must be passive observation of an already operator-started process.
- WP-0311 owns Diagnostics/Options demand orchestration, backend command lifetime, protection projection, and Torch/Demucs semantic single-flight.
- WP-0312 owns the SQLite runtime boundary, bounded reader/writer admission, startup-only migration, operation registry, WAL health, and lock-candidate attribution. It retains SQLite.
- WP-0313 owns offline hydration/provider-verification single-flight and scheduling plus source-emitted/native-received/durably-persisted heartbeat timing.
- WP-0314 owns native-window/WebView2 process liveness and bounded WPR/ETW attribution, followed only by an evidence-selected remediation.
- WP-0315 owns an isolated SurrealDB 3.2.4 + RocksDB shadow benchmark and decision. It is not a freeze fix, does not block WP-0298 remediation, and cannot authorize production migration.
- WP-0298 remains the umbrella integration/closure owner for fresh agent-observation-only panel evidence from an already operator-started process, isolated job-start write proof, agent-observation-only evidence of an operator-initiated exact current database/NAS job, and final incident reproduction. Successor packets must hand their canonical proof back to WP-0298 rather than declaring the overall freeze closed independently.

</topic>

<topic id="research-basis-and-selected-design" status="active" version="v1" wp="WP-0298" updated_at="2026-08-09">

# Sources checked

- Current VoxVulgi engine/frontend source, schema, live read-only database counts, diagnostic trace, freeze reports, WP-0221/WP-0242/WP-0278/WP-0280 artifacts, and historical screenshots.
- Microsoft WebView2 performance guidance and its `WebView2.wprp` ETW/WPR diagnostic path: `https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/performance`.
- W3C Long Tasks API for detecting UI-thread tasks at or above the defined long-task threshold: `https://www.w3.org/TR/longtasks-1/`.
- SQLite query planner guidance: `https://www.sqlite.org/queryplanner.html`.
- SQLite row-value/keyset guidance: `https://www.sqlite.org/rowvalue.html`.

# Relevant patterns

- A freeze report needs cross-layer correlation; separate clocks and uncorrelated event names cannot distinguish queue wait from command work, storage wait, serialization, or render work.
- WebView2 uses multiple processes, so a healthy Rust process or JS Worker does not prove the compositor/renderer is healthy.
- UI navigation should render from bounded cached state and load secondary data independently. A panel should not synchronously await NAS checks or full-history rollups.
- Filesystem reachability is an observation with age and uncertainty, not a property that must be recomputed for every render.
- Persistent/event-updated rollups are cheaper and more stable than recurrent aggregation over 315,000 jobs or recurrent archive-file scans.
- Normal telemetry must be low overhead; detailed capture must be bounded by duration/size and operator/incident trigger.

# Existing systems reused

- `diagnostics_trace.jsonl`, `InvokeTimer`, Worker heartbeat, freeze-event ingress, freeze dump/report, `/agent/state`, `/agent/dump`, `/agent/snapshot`, `/agent/ui_audit`, and `/agent/ui_action`.
- `vvwatch.cmd` and `governance/scripts/vv_watch.ps1`.
- Existing read-only SQLite connection, job/track projections, media-path timeout/cache, archive state, supported headless disposable-base override, and post-database background-work skips. Headless without the override can still initialize/migrate canonical app data and is not a safe proof path.

# Rejected options

- Move SQLite off the NAS: rejected because the canonical database is already local.
- Increase SQLite busy timeout: rejected as a primary fix because recent evidence is dominated by long reads/NAS probes rather than lock errors.
- Hide slow data behind a loading spinner: rejected because it preserves the blocking work and supplies no causal evidence.
- Keep adding unbounded trace rows: rejected because the trace is already 281 MB and will become a product defect.
- Always run WPR/ETW: rejected because it is heavyweight and unnecessary for ordinary operation.
- Treat missing Worker freeze rows as proof there was no freeze: rejected because WebView2 renderer/compositor stalls can occur outside the Worker-observed main thread.

# Selected design

1. Add a common incident/span envelope to relevant frontend, Tauri, engine, storage, downloader-launch, and watcher events.
2. Split command time into bridge/dispatch queue wait, DB open/prepare/step/row-map, storage probe, serialization, frontend receive, and render/commit phases where measurable.
3. Add frontend long-task, interaction-to-render, request-start/stale-cancel, mounted-row count, and viewport/page identity evidence with bounded sampling.
4. Persist media availability observations with state, observed time, source, duration, and next eligible refresh; render from this state.
5. Refresh media observations through a bounded reconciler and force a fresh exact probe only at execution/destructive correctness boundaries already requiring it.
6. Replace per-panel subscription archive recounts and all-history activity aggregation with event-updated/indexed rollups and explicit repair/rebuild commands.
7. Cancel or ignore stale panel requests when navigation/filter/selection changes.
8. Add Diagnostics `Normal` and `Incident` capture modes plus `Arm next job start` and `Arm next panel switch`; show remaining duration, artifact location, size limit, and capture status.
9. Rotate/compress internal traces by size and age; retain bounded incident artifacts and compact aggregates. Expose sampling/drop/rotation state.
10. Extend `vvwatch` with incident IDs, WebView process tree/renderer responsiveness, process I/O, DB probe phase, NAS probe latency, and optional operator-triggered WPR/ETW instructions/receipt when tooling is installed.
11. Reproduce agent-driven database/NAS panel switching and job start on a verified clone/disposable headless root or disposable normal-window VM. Obtain exact current-profile panel/job timing only by observing an already operator-started case without agent launch, navigation, mutation, or stop, or stop for the separate explicit authority/reconciliation gate above. Identify the dominant phase from correlated evidence and apply only the measured query/storage/render remediation.

</topic>

<topic id="roi-red-team-microtasks-and-proof" status="active" version="v1" wp="WP-0298" updated_at="2026-08-09">

# High-ROI additions

- Persist media-observation cache: reuses existing path states, prevents repeat NAS stalls, helps Jobs, Video Archiver, Media Library, repair, and future TikTok/Instagram surfaces. Verify age, invalidation, restart persistence, and exact execution-boundary refresh.
- Add event-updated archive/activity rollups: reuses job transitions and archive writes, closes repeated 40-second queries, and makes later provider modules cheaper. Verify rebuild equals canonical source data.
- Add incident/span IDs to `vvwatch` and internal traces: reuses both diagnostic systems and makes operator/LLM parallel diagnosis attributable. Verify one incident can be reconstructed without timestamp guessing.
- Add trace rotation plus aggregates: prevents disk growth/data loss risk while retaining long-term performance trends. Verify enforced size/age bounds and recovery after interrupted rotation.
- Add a one-action incident arm: reduces operator relay and captures the exact next failure rather than generic background data. Verify it remains quiet and automatically disarms.

# Risks, failure scenarios, controls, and verification

- Risk: instrumentation causes the slowdown it measures.
  - Control: fixed event budgets, sampling in Normal mode, batched writes, bounded strings, and no synchronous trace flush on UI paths.
  - Verify: baseline versus instrumented p50/p95 and trace-write CPU/I/O.
- Risk: persistent availability is stale and allows wrong UI actions.
  - Control: display observation age; invalidate on import/move/delete/restore; retain exact fresh checks at execution and filesystem mutation boundaries.
  - Verify: present→missing, missing→restored, NAS unavailable, and app restart cases.
- Risk: rollup drift hides canonical jobs/media.
  - Control: event transaction updates plus independent rebuild/reconcile command and mismatch diagnostics.
  - Verify: seeded transitions, interrupted update, rebuild parity, and live canonical count comparison.
- Risk: stale panel response overwrites current selection.
  - Control: request generation/token and selected-page/filter check before state commit.
  - Verify: rapid navigation/filter changes under delayed command fixtures.
- Risk: trace rotation deletes the only incident evidence.
  - Control: pin active incident files until finalized, atomic rename, manifest, bounded retained count, explicit export.
  - Verify: forced rotation during incident and interrupted-process recovery.
- Risk: ETW/WPR is unavailable or requires external setup.
  - Control: optional capability check; internal/`vvwatch` capture remains complete enough to state which layer is unresolved.
  - Verify: installed and unavailable paths, with no false `ready` state.
- Risk: headless proof does not reproduce compositor behavior.
  - Control: disposable-root headless proof covers data/navigation contract; agent-driven normal-window proof runs only in an owned disposable VM/snapshot; exact current-profile freeze evidence additionally requires observation of an already operator-started normal-window incident or an explicit unresolved label.
  - Verify: separate resolved-root/VM/operator-initiation proof receipts for headless, agent-driven normal-window, and current-profile observation paths.

# Microtask plan

1. Add focused RED fixtures for slow NAS observations, stale panel response, archive/activity rollup drift, and trace rotation.
2. Implement incident/span schema and frontend/Tauri/engine propagation.
3. Implement bounded trace writer, rotation, incident manifests, and Diagnostics controls.
4. Persist media observations and remove render-time bulk NAS probes.
5. Implement archive/activity rollups plus rebuild/reconciliation.
6. Add stale-request cancellation/guards and phase-level timing.
7. Extend `vvwatch` and optional WebView2 ETW/WPR capability/receipt.
8. Observe the exact current-profile panel-switch and job-start incident only on an already operator-started process, plus run isolated-clone/disposable-root agent-driven panel/job write capture; record initiator/root identity and remediate measured hot paths. Do not launch, navigate, mutate, stop, or initiate a live canonical job without the separate explicit authority/reconciliation gate.
9. Build; headless-audit only with a preflighted owned absolute `VOXVULGI_AGENT_HEADLESS_BASE_DIR`; run agent-driven normal-window reproduction only in an owned disposable VM/snapshot; and write the proof bundle.

# Acceptance and proof gates

- No inspected primary panel waits synchronously for full NAS/path probing, archive-file recount, or all-history job aggregation.
- Correlated evidence distinguishes UI long task/render, bridge wait, SQLite phase, NAS phase, child launch, WebView process state, and host pressure for one incident.
- Current exact panel/job-start cases have named expected/observed conditions, initiator, canonical-versus-disposable root, commands/receipts, and before/after reconciliation where mutation was explicitly authorized; any remaining compositor-only path is explicitly unresolved rather than declared fixed.
- Trace storage obeys tested size/age limits and retains incident manifests plus durable aggregates.
- Persistent observation and rollup rebuilds reconcile to canonical state with no library/subscription/job deletion.
- Focused Rust/frontend tests, TypeScript build, relevant engine suite, governed semantic-version build, changelog, quiet headless audits/snapshots/dumps with the owned disposable base-dir override, disposable-VM normal-window evidence, concurrent `vvwatch`, operator-initiation receipts for current-profile observation, and proof `summary.md` pass.

</topic>
