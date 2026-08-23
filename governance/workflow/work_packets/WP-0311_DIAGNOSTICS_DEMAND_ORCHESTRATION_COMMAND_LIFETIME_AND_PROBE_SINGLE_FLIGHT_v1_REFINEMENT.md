---
file_id: WP-0311-REFINEMENT-v1
file_kind: refinement
updated_at: 2026-08-23
---

<topic id="operator-request-evidence-and-authority" status="active" version="v1" wp="WP-0311" updated_at="2026-08-23">

# Operator request

- Remediate every confirmed problem from the governed v0.1.179 freeze incident rather than treating a database replacement as the fix.
- Preserve the current diagnostics capabilities while preventing Diagnostics and Options from flooding the backend, database, and Python runtime.
- Make navigation end or safely supersede work owned by the page being left; a frontend stale-state guard alone is not completion.
- Keep WP-0298 as the umbrella integration and exact-current-case closure owner.

# Verified incident state

- Governed v0.1.179, desktop PID 36220, and watcher run `watch_20260823-055128` recorded 127 of 127 Windows `Responding=false` samples while the PID stayed alive and the localhost agent bridge had zero failures.
- The same run recorded a maximum of ten overlapping commands, nine watcher database-probe timeouts, two heavy-child samples, and two frontend long tasks with a maximum of 775 ms.
- `DiagnosticsPage.tsx` currently schedules Build, Core tools, Phase 2, Storage, Trace, and Jobs loads within 0 to 220 ms of visibility.
- Trace auto-load starts two protection-status calls, two history calls, and two complete history-replay calls in one `Promise.all`.
- Runtime trace rows prove Diagnostics work remained active after Options rendered and started its own protection work. One Diagnostics operation completed after 29.464 seconds.
- Existing frontend generation and `canceled` flags suppress some stale state commits but do not cancel work already executing in Tauri, the engine, SQLite, or child processes.
- Diagnostics directly invokes `tools_performance_tier_status`; `voice_backends_catalog` invokes `performance_tier_status`; `voice_backends_recommend` rebuilds the catalogue and therefore invokes it again. Each performance-tier calculation can launch Python and import Torch to call `torch.cuda.is_available()`.
- Demucs status separately launches Python to import `demucs_infer`.
- The watcher captured three logical Torch/CUDA probes and one Demucs import concurrently, plus their portable-Python children. There is no app-wide single-flight result for these semantic probes.
- Current v0.1.179 trace already proves distinct request/span identities for download and enumeration protection operations. This packet must preserve that correction.

# Corrected and forbidden claims

- The incident does not prove that Diagnostics caused the native-window freeze; it proves that Diagnostics produced overlapping backend work during the same incident.
- It does not prove that SQLite, Python, the renderer, the Worker, or the compositor was the sole root cause.
- Offline hydration completed successfully after 575,965 ms. Worker and main-thread heartbeat rows were generated during the apparent gap and persisted later in a burst. This packet must not repeat the invalid claims that hydration never terminated or WebView liveness died.
- Do not preserve the earlier narrative arrow chain as causal fact. Treat it as a list of correlated confirmed pressures that must be removed and re-tested.

# Relevant implementation surfaces

- `product/desktop/src/pages/DiagnosticsPage.tsx`
- `product/desktop/src/pages/OptionsPage.tsx`
- `product/desktop/src/lib/activity.ts`, which owns the current shared `usePollingLoop` helper
- `product/desktop/src-tauri/src/lib.rs`
- `product/engine/src/tools.rs`
- `product/engine/src/voice_backends.rs`
- `product/engine/src/youtube_protection.rs`
- `product/desktop/tests/causalFreezeDiagnosticsContract.test.ts` and adjacent desktop contract tests
- Existing `InvokeTimer`, request/span envelopes, bounded trace writer, diagnostics capture state, and headless agent bridge from WP-0298

# Authority and dependency boundaries

- `build_rules.md`: quiet verification, visual inspection, backend/frontend navigation, and no new cards.
- `governance/workflow/PROOF_STANDARD.md`: this is a manual/UI-heavy and app-boundary packet; build-only proof is insufficient.
- `governance/spec/TECHNICAL_DESIGN.md` sections covering Diagnostics, startup, read projections, and causal freeze evidence.
- WP-0298 remains the umbrella owner of incident/span correlation, bounded captures, exact current-database/NAS panel and job-start proof, and final integrated closure. It is an authority/integration consumer, not a completion predecessor for WP-0311.
- WP-0312 owns the SQLite service boundary and read/write connection discipline. WP-0311 owns demand scheduling, request lifetime, semantic deduplication, and the page-level protection projection contract; the two packets must share one API rather than implement parallel coordinators.
- WP-0313 owns offline hydration scheduling and heartbeat transport/persistence timing.
- WP-0314 owns final native-window/WebView/ETW attribution after this packet removes known demand pressure.
- WP-0309 and WP-0310 are completed dependencies and must not be reopened without new failing evidence.

</topic>

<topic id="research-selected-design-and-scope" status="active" version="v1" wp="WP-0311" updated_at="2026-08-23">

# Research basis

## Sources checked

- Current VoxVulgi Diagnostics, Options, Tauri command, engine probe, protection-history, trace, and focused contract sources.
- Governed v0.1.179 diagnostic trace and `watch_20260823-055128` watcher artifacts.
- React `useEffect` lifecycle and cleanup guidance: `https://react.dev/reference/react/useEffect`.
- React guidance on stale asynchronous response races: `https://react.dev/learn/you-might-not-need-an-effect`.
- Tauri v2 command and channel guidance: `https://v2.tauri.app/develop/calling-rust/`.
- Tauri IPC command/event model: `https://v2.tauri.app/concept/inter-process-communication/`.
- Microsoft WebView2 performance guidance to defer heavy work, reduce redundant communication, batch IPC, and measure real scenarios: `https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/performance`.

## Relevant field patterns

- React cleanup can prevent stale state commits, but it does not terminate an already-running native command. Backend cancellation, bounded paging, or shared single-flight execution is required when resource consumption must stop.
- Tauri commands are asynchronous request/response operations. Cancellation must be an explicit application contract using request identity, a cancellation registry/token, safe checkpoints, or a bounded shared operation; dropping the JavaScript promise is not cancellation.
- Heavy readiness probes should be keyed by semantic input identity and shared across concurrent consumers. A shared result must carry freshness and provenance so a stale cache cannot masquerade as verified readiness.
- Page entry should render a bounded shell and cheap cached truth first. Heavy or historical detail should load only when the existing section is visible/expanded or the operator explicitly refreshes it.
- Replays and full-history calculations are not passive status reads and must not run automatically merely because Diagnostics is visible.

# Selected design

- Introduce one shared diagnostics request coordinator used by Diagnostics and the overlapping Options modules.
- Classify every diagnostics operation into a stable cost class: `cheap`, `db_read`, `filesystem`, `python_heavy`, `history_replay`, or `mutation`. The registry records semantic key, owner page/module, request/span identity, cancellation capability, freshness policy, and maximum concurrency.
- Replace the 0-to-220-ms timer fan-out with bounded priority scheduling. Cheap shell/readiness work runs first. Heavy status work loads on demand for the existing visible section; this changes orchestration, not the card/layout model.
- Enforce one active flight per semantic probe key and at most two Python-heavy operations in the Diagnostics/Options capability-probe admission domain. The exact Torch/performance-tier probe and Demucs import each have a stricter maximum of one active child process. This limit must not throttle localization, downloader, model inference, or other production job-runner Python work; a product-wide Python governor requires a separate explicit contract.
- Add an app-wide current-process single-flight cache for performance-tier/Torch and Demucs/module status. Cache keys include executable/runtime identity and the installed payload/config identity needed to invalidate correctly. Results include `verified_at_ms`, source identity, and error/freshness state.
- Make `voice_backends_catalog` and `voice_backends_recommend` consume the same already-computed performance-tier value rather than recursively calculating it.
- Give every page-owned request a page/module generation plus request/span identity. On navigation, queued operations are removed. Running operations check cancellation at safe engine boundaries. A non-cancellable operation must be shared single-flight, must not launch a replacement, must suppress stale state commit, and must trace `superseded_completion`.
- Replace automatic download/enumeration status/history/full-replay fan-out with one bounded read-only protection snapshot containing both operation identities. Full replay is explicit/incident-only or reads already-maintained rollups; it is not part of ordinary page mount.
- Emit scheduler receipts containing queue wait, cost class, semantic key, owner, dedupe/shared-result state, cancellation requested/observed, child PID where applicable, and terminal outcome.

# Scope edges

## In scope

- Diagnostics and overlapping Options demand orchestration.
- Backend-visible request cancellation/supersession.
- Current-process semantic single-flight and bounded freshness caches.
- Consolidated protection diagnostics projection for ordinary page load.
- Trace receipts and tests that prove work stopped, deduplicated, or completed as a shared non-cancellable operation.
- Exact Diagnostics-to-Options normal-window reproduction under `vvwatch` inside an owned disposable VM/snapshot. A separate current-profile cell may observe an already operator-started process but may not start it, drive navigation, mutate state, or stop it.

## Non-goals

- Removing Diagnostics data or hiding it behind an indefinite spinner.
- Changing YouTube protection policy, history retention, or download/enumeration semantic identity.
- Replacing SQLite or implementing the WP-0312 database service.
- Weakening readiness truth by treating a stale cached result as verified.
- Killing a process not started and owned by the tested request. Global process-stop authorization remains mandatory for unowned processes.
- Increasing timeouts as the primary fix.
- Adding cards or redesigning the page information architecture.

# Rejected options

- Clear only the frontend timers on unmount: already present and does not stop fired commands.
- Keep every current command but delay each by a few more milliseconds: shifts overlap without bounding it.
- Ignore late promises: protects React state but preserves database, child-process, and CPU/I/O pressure.
- Cache forever: can make installed/runtime readiness false after a repair, update, path override, or environment change.
- Automatically replay full history on page entry: converts an operator/debug action into background load.
- Serialize every command globally: prevents duplicates but needlessly blocks independent cheap reads and reduces usability.

</topic>

<topic id="roi-red-team-and-controls" status="active" version="v1" wp="WP-0311" updated_at="2026-08-23">

# High-ROI additions and reuse

- Shared semantic-probe registry.
  - Why high ROI: Diagnostics and Options already call the same probes; one registry removes duplication and makes later modules cheap to integrate.
  - Gap addressed: independent call sites currently cannot know another identical probe is active.
  - Reuse: current request/span tracing, `InvokeTimer`, runtime identity helpers, and existing tool status structs.
  - Validation: concurrent-call tests must return one child PID/one computation and identical provenance to all waiters.
- Cost-class and queue-wait tracing.
  - Why high ROI: it distinguishes time waiting for admission from time spent in SQLite, Python, filesystem, serialization, or rendering.
  - Gap addressed: elapsed command time alone cannot identify pressure source.
  - Reuse: WP-0298 trace envelope and incident capture.
  - Validation: synthetic queue delay and real incident receipts must reconcile start, admission, and completion.
- Explicit refresh/freshness labels for heavy sections.
  - Why high ROI: prevents background churn while keeping operator trust.
  - Gap addressed: current auto-load has no visible age or shared-result provenance.
  - Reuse: existing section loading/error states; do not add cards.
  - Validation: semantic audit and visual snapshot after the timed causal window.
- Reusable cancellation registry.
  - Why high ROI: later pages can adopt the same request lifetime instead of adding more local booleans.
  - Gap addressed: native work outlives frontend ownership.
  - Reuse: page generation/request/span identifiers and command-phase tracing.
  - Validation: delayed-command RED/GREEN fixtures and navigation exercise.

# Red-team risks, scenarios, controls, and verification

- A single-flight entry deadlocks or remains permanently occupied after panic/error.
  - Control: RAII/finally cleanup, terminal outcome for success/error/panic/cancel, bounded waiter timeout, and retry after terminal failure.
  - Verify: injected error, panic boundary, timeout, canceled waiter, and subsequent successful retry.
- Cancellation occurs inside a safety-critical transaction and leaves partial state.
  - Control: check cancellation before opening a transaction and after commit/rollback, never between invariant-dependent statements unless rollback is guaranteed.
  - Verify: injected cancellation at every defined checkpoint plus canonical state reread.
- An old result overwrites current Options or Diagnostics state.
  - Control: page/module generation and semantic query key must be rechecked at commit; trace stale suppression.
  - Verify: deliberately reorder responses across two navigations and two module selections.
- Cache invalidation misses an install, repair, Python override, GPU/runtime change, or payload update.
  - Control: versioned identity key and explicit invalidation calls from every mutation path; fail stale rather than ready when identity cannot be established.
  - Verify: mutate each supported identity input only inside an owned disposable fixture/root and prove a new probe runs exactly once; never alter the operator's installed runtime, payload, or canonical configuration for this test.
- Bounded scheduling starves a low-priority section forever.
  - Control: aging/fairness within cost classes, visible queued state, and a maximum wait receipt.
  - Verify: sustained cheap traffic cannot prevent an admitted heavy request from eventually running.
- Consolidating protection data changes download/enumeration meaning.
  - Control: preserve operation-specific IDs, totals, history ordering, and canonical source queries in one snapshot contract.
  - Verify: parity fixture compares old operation-specific outputs to the new projection before retiring old auto-load calls.
- A non-cancellable child process survives navigation and a replacement starts.
  - Control: owned child handle plus single-flight key; no replacement until terminal/kill-at-safe-boundary; trace PID and owner.
  - Verify: navigate during a deliberately slow owned probe and observe at most one PID.
- UI appears faster only because data silently stopped loading.
  - Control: every section retains explicit `idle/queued/loading/ready/stale/failed` truth, freshness, and manual refresh.
  - Verify: headless semantic audit plus visual inspection at supported viewports.
- A proof launch silently initializes or mutates the canonical app database.
  - Control: every agent-started `--agent-headless` launch sets `VOXVULGI_AGENT_HEADLESS_BASE_DIR` to a preflighted owned disposable absolute root; every agent-started normal-window launch runs only inside an owned disposable VM/snapshot; `--safe-mode` is never described as read-only because visible safe mode writes queue-pause state. Current-profile evidence is observation-only on an already operator-started process.
  - Verify: resolved-path/non-alias receipt, disposable bridge/database/config/trace sidecars, `agent_headless=true`, canonical-target before/after non-access evidence for agent-owned runs, VM/snapshot identity, and explicit process-initiation/ownership receipt. Missing isolation proof keeps the packet not-DONE.

</topic>

<topic id="microtasks-acceptance-and-proof" status="active" version="v1" wp="WP-0311" updated_at="2026-08-23">

# Ordered microtask plan

1. Create RED frontend and Rust fixtures for Diagnostics-to-Options overlap, delayed Tauri completion, duplicate semantic probes, single-flight error cleanup, and protection full-replay auto-load.
2. Inventory every Diagnostics and overlapping Options invoke with semantic key, cost class, current trigger, mutation/read status, cancellation safety, and canonical owner. Store the registry in product code, not governance prose.
3. Implement the shared bounded coordinator and trace schema. Preserve stable request/span identity and add queue/admission/cancel/dedupe receipts.
4. Convert Diagnostics page entry from timer fan-out to cheap-first, section-demand scheduling without adding cards or changing the page's product scope.
5. Implement current-process single-flight for performance tier/Torch and Demucs/module probes; refactor catalogue/recommendation to consume one result and add complete invalidation paths.
6. Implement backend cancellation/supersession checkpoints for bounded DB paging/replay and owned child launches. Mark and handle genuinely non-cancellable operations explicitly.
7. Implement the combined read-only protection snapshot and remove automatic full-history replay from ordinary Diagnostics/Options mount.
8. Add contract/source guards against duplicate semantic probe calls, unbounded page-mount fan-out, and frontend-only cancellation claims.
9. Run focused frontend/Rust tests, TypeScript/build checks, and the full relevant regression suites.
10. Propagate the implemented demand, request-lifetime, capability-probe, protection-projection, and receipt contracts into `governance/spec/PRODUCT_SPEC.md`, `governance/spec/TECHNICAL_DESIGN.md`, `product/desktop/src/pages/DiagnosticsPage.tsx`, and `product/desktop/src/pages/OptionsPage.tsx`. Repo search on 2026-08-23 found no standalone product-code/governance topology or general built-in model-manual artifact; do not invent or claim those updates. Record each missing surface in proof and route a separate operator proposal for its canonical path.
11. Build the next governed semantic version. For packaged headless semantic proof, create and preflight an owned disposable absolute root, set `VOXVULGI_AGENT_HEADLESS_BASE_DIR` before `--agent-headless`, and prove the bridge/database/config/traces resolve there rather than canonical app data. Run the agent-driven normal-window Diagnostics-to-Options incident with concurrent `vvwatch` only inside an owned disposable VM/snapshot. An optional current-profile cell may attach read-only observers to an already operator-started process but may not launch it, navigate it, request a mutation, or stop it. Capture screenshots only after the timed window.
12. Run independent adversarial review, remedy findings, create `summary.md` and structured evidence under the WP-0311 proof path, and hand the result to WP-0298 integration closure.

# Acceptance criteria

- Ordinary Diagnostics entry launches no duplicate semantic probe.
- At most one Torch/performance-tier child and one Demucs-import child exist for concurrent Diagnostics/Options consumers; no more than two Python-heavy tasks run inside the capability-probe admission domain. Production localization/downloader/job-runner Python work is outside this packet's governor and retains its existing authority.
- Page entry no longer starts both full protection-history replays. Download and enumeration retain distinct request/span identity and output semantics.
- Diagnostics-to-Options navigation removes queued Diagnostics work. After cancellation is observed, no new Diagnostics-owned child launch begins and no stale Diagnostics result commits into current UI state.
- A running non-cancellable shared operation produces one PID/computation, one terminal result, and explicit `superseded_completion` for owners that left; it never causes a replacement flight.
- Every section exposes truthful loading/queued/stale/error/freshness state and remains discoverable without new cards.
- Scheduler receipts reconcile queue wait, execution, child ownership, cancellation, deduplication, and completion for the exact incident.
- The shell and Options render remain responsive during the controlled scenario; no acceptance claim uses a post-window screenshot freeze as navigation evidence.
- Existing tool readiness, repair/update invalidation, protection history, and download/enumeration policy tests remain green.

# Proof contract

- Verification class: manual/UI-heavy plus app-boundary.
- Required proof root: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0311/<run-id>/`.
- Required `summary.md` and `evidence.json` name the exact target, expected state, observed state, command/scenario, app version, PID, request/span IDs, semantic probe counts, child PIDs, maximum overlap, cancellation outcomes, and watcher path.
- Required automated proof: RED-to-GREEN focused frontend tests, Rust single-flight/cancellation/invalidation tests, TypeScript, desktop contracts, and affected engine tests.
- Required app-boundary proof: packaged headless semantic audit/dump with `VOXVULGI_AGENT_HEADLESS_BASE_DIR` resolving to an owned disposable absolute root, plus a controlled normal-window Diagnostics-to-Options run with `vvwatch` inside an owned disposable VM/snapshot. Any current-profile evidence is separately labelled operator-initiated/agent-observation-only and cannot replace isolation receipts.
- Independent adversarial review is required before `DONE`.
- WP-0311 remains `BACKLOG` until implementation begins and cannot become `DONE` until its proof passes and its handoff is recorded for WP-0298 integration. WP-0298 itself does not need to be `DONE` first.

</topic>
