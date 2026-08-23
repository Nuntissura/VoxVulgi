---
file_id: WP-0313-REFINEMENT-v1
file_kind: refinement
updated_at: 2026-08-23
---

<topic id="operator-request-corrected-evidence-and-authority" status="active" version="v1" wp="WP-0313" updated_at="2026-08-23">

# Operator request

- Remediate the confirmed long-running offline-bundle/provider verification pressure without weakening VoxVulgi's batteries-included, verified-bytes, fully offline readiness contract.
- Keep the interface, bridge, jobs, and diagnostics observable and responsive while verification runs.
- Replace ambiguous delayed heartbeat evidence with generation, receipt, queue, persistence, and source-acknowledgement timing that a no-context model can interpret correctly.

# Corrected governed incident evidence

- WP-0310's database-first startup gate passed in governed v0.1.179: `db_schema` ran from 1787456933794 through 1787456942455 and reached `ready` before offline hydration began.
- Offline hydration started at 1787456942629 and completed `ready` at 1787457518594. Exact duration was 575,965 ms.
- Therefore, the watcher cutoff showing `offline_bundle=pending` did not prove a permanent hang or missing terminal state.
- Worker heartbeat payloads whose generation times span the apparent trace gap and main-thread heartbeat ticks 2 through 20 were persisted in a burst after hydration, around 1787457530.
- Therefore, timers and Worker execution continued. The proven defect is delayed transport, ingestion, queueing, and/or trace persistence observability during the long verification. The exact delay boundary remains unknown.
- Current trace rows primarily timestamp persistence. Without source generation and native receipt timing, file order cannot identify whether delay occurred in Tauri IPC, Worker HTTP fetch, bridge receive, diagnostics queueing, trace locking/rotation, or host/file-I/O scheduling.
- Startup currently spawns a background thread that applies the offline bundle and then calls full `verify_youtube_po_provider_node_modules`. The provider scan is intentionally required before provider readiness.
- The frontend polls `startup_status` every 1.2 seconds while offline-bundle state is pending/running.
- Hot status paths are intended to read current-process attestation rather than start another full tree verification. This invariant must be preserved and proven.

# Authority and packet boundaries

- Installer/offline policy in `AGENTS.md`, `CLAUDE.md`, `governance/spec/PRODUCT_SPEC.md` sections 8.1.8/8.1.9, `governance/spec/TECHNICAL_DESIGN.md` section 2.1, and `governance/release/OFFLINE_INSTALLER_BUILD_MANUAL.md` remains authoritative.
- Readiness must reflect verified bytes on disk, never network reachability; default operation remains fully offline.
- WP-0310's database-first ordering remains a hard predecessor and must not regress.
- WP-0298 owns bounded incident traces/captures and final exact-current-case closure. It is an authority/integration consumer, not a completion predecessor for WP-0313.
- WP-0311 owns page demand scheduling and heavy status single-flight. WP-0313 supplies startup/provider verification progress as one shared flight rather than allowing status consumers to rescan.
- WP-0312 owns database access; provider verification must not retain a database transaction while walking files.
- WP-0314 consumes the corrected heartbeat timestamp contract for final WebView/native-window attribution.
- WP-0308's release payload/manifest/fingerprint systems should be reused where their current artifacts are compatible; this packet does not redesign public installer contents.

# Relevant implementation surfaces

- `product/desktop/src-tauri/src/lib.rs`, especially startup tracking and the offline-bundle thread near the WP-0310 gate.
- `product/engine/src/tools.rs`, especially provider dependency-tree verification, attestation, runtime identity, and invalidation.
- `product/desktop/src/App.tsx` startup-status polling.
- `product/desktop/src/lib/freezeDetector.ts` and `freezeDetector.worker.ts`.
- Diagnostics trace ingress/writer/rotation and `/agent/freeze_event` in `product/desktop/src-tauri/src/lib.rs`.
- Offline-bundle manifests, payload fingerprints, provider install authority, and existing hydration/warmup tests.
- `governance/scripts/vv_watch.ps1` and bundled watcher parity copy/tests.

</topic>

<topic id="research-selected-design-and-scope" status="active" version="v1" wp="WP-0313" updated_at="2026-08-23">

# Research basis

## Sources checked

- Current VoxVulgi startup, offline bundle, provider integrity, attestation, heartbeat, trace writer, polling, installer manifest, and proof sources.
- Governed v0.1.179 trace and watcher artifacts, including payload-generation timestamps inside delayed heartbeat rows.
- Microsoft guidance that CPU priority alone does not prevent background file I/O/memory work from harming foreground responsiveness and that Windows background mode adjusts resource scheduling: `https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-setpriorityclass`.
- Microsoft thread-priority/background-mode API guidance: `https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-setthreadpriority`.
- Microsoft WebView2 performance guidance to defer heavy work, batch communication, minimize redundant work, and test real scenarios: `https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/performance`.
- Tauri v2 events/commands/channels: `https://v2.tauri.app/develop/calling-rust/` and `https://v2.tauri.app/concept/inter-process-communication/`.
- Rust standard file metadata and canonicalization contracts used by the existing provider verifier; any new Windows file-identity/change-journal design requires an additional primary-source research receipt before implementation.

## Relevant field patterns

- A background thread can still saturate disk, memory bandwidth, antivirus scanning, locks, or trace I/O. Moving work off the UI thread is necessary but not sufficient.
- Process-wide background mode is inappropriate for an interactive desktop process because it can lower the UI and bridge too. If Windows background scheduling is selected, it must be thread-scoped or use a separately owned helper process with attributable lifecycle.
- Long integrity work needs one semantic flight, bounded chunks/yields, progress, cancellation only at safe boundaries, and a terminal state.
- A persisted receipt by itself is not executable trust. Current-process readiness must still authenticate the exact current payload according to the provider authority contract.
- Startup state should be pushed on transitions/progress and reread on reconnect. Fixed 1.2-second polling of unchanged state is a fallback, not the primary transport.
- Cross-boundary liveness needs source sequence/generation time plus native receive and durable persist times. Persistence time alone cannot diagnose delivery delay.

# Selected design

- Split offline hydration into stable named phases: `bundle_discovery`, `bundle_application`, `provider_manifest_load`, `provider_tree_verify`, `provider_attestation_publish`, and terminal `ready/error/skipped`. Each phase records files/bytes planned and completed where knowable.
- Run exactly one provider-verification flight per current runtime/payload identity. Every startup/status/catalog consumer observes the same progress/result and cannot start a second tree walk.
- Preserve full current-process content authentication before provider execution. A persisted receipt may seed progress or identify an unchanged candidate but cannot independently establish executable readiness.
- Add a research-gated incremental verification path only if it can prove current-file identity/content against the release manifest and detect tamper, rename, replacement, reparse, interrupted promotion, and payload-version change. If that proof cannot be made, retain the full scan and optimize scheduling rather than weakening trust.
- Schedule tree verification in bounded chunks with explicit yields and foreground-pressure awareness. Use thread-scoped Windows background resource mode only after a focused fixture proves it helps and does not introduce priority inversion; never put the whole VoxVulgi process into background mode.
- Pause or reduce verification admission while foreground navigation/job-start or WP-0311 Python-heavy work needs resources. No database transaction may remain open across file verification.
- Publish startup progress through a Tauri event/channel plus an immediate `startup_status` snapshot for initial/reconnect truth. Replace 1.2-second unchanged polling with event-driven updates and a bounded adaptive fallback.
- Give every main-thread and Worker heartbeat a monotonic sequence and source `emitted_at_ms`. Native ingress adds `received_at_ms`; the trace writer adds `persisted_at_ms`, queue dwell, generation, and outcome; the source records `acknowledged_at_ms` when the native command/HTTP response returns plus an explicit acknowledgement stage (`received`, `queued`, or `persisted`). Preserve payload generation timestamps separately from receipt/persist/ack time. This ingress acknowledgement is not the native-window message-pump acknowledgement owned by WP-0314.
- Add bounded late/dropped/duplicate counters and a terminal flush/barrier so heartbeat/startup events do not silently remain buffered behind long unrelated work.
- Benchmark exact full packaged payload before and after. The selected duration/foreground resource budget must be recorded from the baseline and approved in the packet proof; duration improvement cannot be claimed from a toy tree.

# Scope edges

## In scope

- Offline bundle/provider verification phase model, single-flight, scheduling, progress, terminal truth, and foreground-priority behavior.
- Safe research and implementation of incremental verified work only if security/integrity proof passes.
- Event-driven startup state with bounded fallback polling.
- Corrected heartbeat generation/receive/persist/acknowledgement timestamp and queue-dwell contract across main thread, Worker, bridge, and trace writer.
- Slow-tree, tamper, interruption, and exact-payload proof with `VOXVULGI_AGENT_HEADLESS_BASE_DIR` on every agent-started headless run and an owned disposable VM/snapshot for every agent-started normal-window run.

## Non-goals

- Skipping full content authentication or trusting a persisted success receipt alone.
- Allowing provider execution before current-process readiness.
- Downloading dependencies, changing default models, slimming the public installer, or moving payload roots.
- Changing WP-0310 database-first order.
- Treating the 575,965-ms duration as proven cause of native-window unresponsiveness.
- Solving the final renderer/compositor hypothesis; WP-0314 owns that attribution.

# Rejected options

- Mark hydration ready immediately and verify later without gating provider execution: creates false readiness.
- Cache only path/size/mtime and call it authenticated: can miss replacement/tamper and violates current trust semantics without further proof.
- Increase verification thread CPU priority or parallelize all files: may worsen the exact resource-pressure problem.
- Put the entire process in Windows background mode: Microsoft warns interactive threads in that process also receive lowered resource priority.
- Keep 1.2-second polling as the only progress path: creates redundant IPC and still cannot explain delayed persistence.
- Timestamp only when writing JSONL: repeats the current ambiguity.

</topic>

<topic id="roi-red-team-and-controls" status="active" version="v1" wp="WP-0313" updated_at="2026-08-23">

# High-ROI additions and reuse

- Shared provider verification flight and progress contract.
  - Why high ROI: startup, Options, Diagnostics, and job readiness all need the same truth.
  - Gap addressed: repeated or opaque tree walks and a single pending/running label.
  - Reuse: current in-memory attestation slot, startup tracker, payload identity, and tool status types.
  - Validation: concurrent consumer test proves one scan and identical identity/progress.
- Cross-boundary emitted/received/persisted/source-acknowledged heartbeat receipt.
  - Why high ROI: distinguishes timer death, transport delay, trace queue delay, and durable-write delay in every future incident.
  - Gap addressed: current persistence timestamp hid generation that continued during hydration.
  - Reuse: existing Worker HTTP ingress, main-thread trace command, sequence/tick values, and WP-0298 trace envelope.
  - Validation: injected delay at each boundary and exact latency reconciliation.
- Event-driven startup progress.
  - Why high ROI: better operator truth with less IPC churn.
  - Gap addressed: 1.2-second unchanged polling throughout a 9.6-minute verification.
  - Reuse: Tauri events/channels and current `startup_status` snapshot.
  - Validation: dropped-listener/reconnect test plus bounded fallback count.
- Foreground-pressure-aware verifier.
  - Why high ROI: lets full integrity coexist with an interactive app.
  - Gap addressed: background placement alone does not bound disk/memory pressure.
  - Reuse: WP-0311 scheduler activity and WP-0298 incident state.
  - Validation: slow-tree scan plus active navigation/job-start comparison.

# Red-team risks, scenarios, controls, and verification

- Incremental verification accepts modified executable bytes.
  - Control: no incremental trust until file-identity/content invalidation is proven against every supported mutation on an owned disposable payload clone/fixture; fail closed to full verification. The operator's installed/bundled exact payload is verification-only and must never be the mutation target.
  - Verify: same-size/mtime-restored tamper, replace, rename, hardlink/reparse, interrupted write, manifest mismatch, and payload-version change on the disposable clone, plus before/after identity proof that the installed/bundled payload was only read.
- Background scheduling starves verification forever.
  - Control: bounded priority with progress/fairness and visible held reason; foreground pressure reduces chunks but cannot silently abandon the flight.
  - Verify: sustained foreground fixture still reaches terminal ready within its documented budget.
- Verification starves UI/bridge or trace writer.
  - Control: bounded chunk time/bytes, yields, thread-scoped background mode if proven, separate bounded trace queue, and foreground-priority signal.
  - Verify: bridge latency, navigation, heartbeat receive/persist lag, disk/CPU samples during exact payload scan.
- Startup event listener misses the initial or terminal state.
  - Control: subscribe plus canonical snapshot/revision handshake; events carry monotonic revision and are idempotent.
  - Verify: listener attaches before, during, and after terminal transition; reconnect obtains canonical state.
- Heartbeat clocks are compared incorrectly.
  - Control: use wall-clock only for cross-boundary approximate latency, sequence for ordering, and per-process monotonic clocks only within their origin; record clock source.
  - Verify: clock-skew fixture and sequence-based reconstruction.
- Trace flush barrier blocks the UI or loses rows on shutdown.
  - Control: native bounded queue, nonblocking producer, explicit overflow counter, bounded terminal flush on owned shutdown.
  - Verify: queue saturation, abrupt owned-process termination, restart, and overflow receipt.
- Provider execution races attestation publication.
  - Control: one atomic readiness generation and execution gate; stale generation cannot authorize a newer/different payload.
  - Verify: launch attempt before/during/after verification and payload identity change.
- Faster toy fixture is used as completion proof.
  - Control: exact packaged payload and current machine conditions are mandatory; toy fixture is RED/GREEN only.
  - Verify: proof summary records payload ID, file/byte totals, machine/source/destination, security state, and exact timestamps.
- A full-payload proof launch migrates or mutates canonical app data before background gating.
  - Control: every agent-started headless launch sets the supported base-dir override to a preflighted owned disposable absolute root; every agent-started normal-window launch runs only in an owned disposable VM/snapshot; optional current-profile evidence observes only an already operator-started process without agent navigation, mutation, or stop.
  - Verify: resolved-root/non-alias receipt, disposable database/config/trace/bridge sidecars, `agent_headless=true`, VM/snapshot identity, process-initiation receipt, and host/canonical non-access evidence for agent-owned runs.

</topic>

<topic id="microtasks-acceptance-and-proof" status="active" version="v1" wp="WP-0313" updated_at="2026-08-23">

# Ordered microtask plan

1. Build a slow, large provider-tree RED fixture and instrument current phase, file/byte progress, scan count, foreground command/navigation, bridge latency, status poll count, and heartbeat source/receive/persist/ack timing.
2. Add the named hydration/verification phase and progress schema with monotonic revision, terminal-state enforcement, and source-contract tests.
3. Implement one current-process verification flight keyed by exact runtime/payload identity. Port startup, status, catalogue, and execution gates to observe it without rescanning.
4. Add emitted/received/persisted/acknowledged sequence timing, explicit acknowledgement stage, startup-event ingress, queue-dwell/overflow accounting, bounded flush, and watcher summary parity.
5. Replace fixed primary polling with event/channel delivery plus snapshot/revision handshake and adaptive fallback.
6. Research and prototype thread-scoped background resource scheduling and bounded chunk/yield control on Windows. Record candidate comparison and select only a proven safe approach.
7. Research incremental authenticated verification. Run every tamper/invalidation mutation against an owned disposable payload clone/fixture only. Implement incremental reuse only if all cases pass; otherwise explicitly reject it and retain full current-process verification. The exact installed/bundled payload remains verification-only.
8. Integrate foreground-pressure signals from page/job activity without letting verification disappear or authorize stale bytes.
9. Run slow-tree, tamper, reparse, interrupted-write, restart, single-flight, pre-attestation execution refusal, queue saturation, and event reconnect tests only against owned disposable payload/app-data fixtures; record source/output path identities and prove no operator payload/config/database path aliases a mutation target.
10. Run the exact packaged full payload headlessly only after setting `VOXVULGI_AGENT_HEADLESS_BASE_DIR` to a preflighted owned disposable absolute root, then prove database/config/trace/bridge sidecars resolve there. Run the agent-driven controlled normal-window case with `vvwatch` only inside an owned disposable VM/snapshot. Optional current-profile evidence may observe an already operator-started process but may not launch, navigate, mutate, or stop it. Capture before/after duration, scan count, bridge/navigation, resource, and heartbeat delivery evidence.
11. Propagate the implemented verification phases, current-process attestation, event/revision, heartbeat timing/acknowledgement, scheduling, and recovery contracts into `governance/spec/PRODUCT_SPEC.md`, `governance/spec/TECHNICAL_DESIGN.md`, `product/desktop/src/pages/DiagnosticsPage.tsx`, and `governance/release/OFFLINE_INSTALLER_BUILD_MANUAL.md` only where its canonical release procedure actually changes. Repo search on 2026-08-23 found no standalone product-code/governance topology or general built-in model-manual artifact; do not invent or claim those updates. Record each missing surface in proof and route a separate operator proposal for its canonical path.
12. Build the governed semantic version, inspect startup/Diagnostics surfaces visually after the timed window, run independent adversarial review, remedy findings, and hand the corrected timing contract to WP-0314 and integration evidence to WP-0298.

# Acceptance criteria

- Offline bundle/provider verification has stable named phases, progress, one semantic flight, and exactly one terminal `ready/error/skipped` outcome.
- Full current-process authenticated readiness remains mandatory before provider execution; no persisted receipt alone grants execution.
- Concurrent startup, Diagnostics, Options, catalogue, and job-readiness consumers trigger exactly one tree verification for one payload identity.
- Exact packaged verification records file/byte progress and either safely reuses proven content work or performs one bounded full scan. A toy fixture cannot substitute.
- Main and Worker heartbeat rows carry sequence, emitted, received, persisted, source-acknowledged, acknowledgement-stage, queue-dwell, and outcome fields. Generated-during-scan events do not appear later as an unexplained multi-interval burst, and an acknowledgement never claims durability unless its declared stage is `persisted`.
- During exact payload verification, bridge health remains available, page navigation and job-start controls remain responsive, and foreground activity receives the selected scheduling priority without weakening integrity.
- Startup progress is event-driven with canonical snapshot/revision recovery; adaptive polling is bounded and does not issue unchanged requests every 1.2 seconds for the full scan.
- Tamper, replacement, reparse, interrupted verification, payload change, and stale generation fail closed and require re-verification on owned disposable clones/fixtures. Exact installed/bundled full-payload proof is read/verify-only, with independent before/after identity evidence that no operator payload bytes changed.
- WP-0310 database-first order remains green and no database transaction spans file verification.

# Proof contract

- Verification class: high-risk app-boundary plus full-payload runtime.
- Required proof root: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0313/<run-id>/`.
- Required evidence includes exact payload/build ID, file/byte totals, before/after phase timings, scan count, progress revisions, status poll/event counts, bridge latency, navigation timings, emitted/received/persisted/acknowledged heartbeat boundary latencies and acknowledgement stages, resource samples, tamper/interruption results, and provider pre/post-attestation execution verdicts.
- Required scenarios include packaged headless with `VOXVULGI_AGENT_HEADLESS_BASE_DIR` resolving to an owned disposable absolute root and controlled normal-window runs inside an owned disposable VM/snapshot, with concurrent `vvwatch` and no network access. Any current-profile evidence is separately labelled operator-initiated/agent-observation-only; the packet must not describe that live process as read-only, and agent-owned proof performs no live-user-data mutation.
- Independent adversarial review is mandatory before `DONE`.
- Duration improvement, responsive UI, or integrity cannot be claimed unless its exact canonical proof surface advances.

</topic>
