---
file_id: WP-0314-REFINEMENT-v1
file_kind: refinement
updated_at: 2026-08-23
---

<topic id="operator-request-evidence-and-authority" status="active" version="v1" wp="WP-0314" updated_at="2026-08-23">

# Operator request

- Remediate every verified freeze finding, including the case where the Windows shell reports VoxVulgi as not responding while its localhost agent bridge remains healthy.
- Establish the causal boundary before selecting a renderer, compositor, native-window, WebView2, database, storage, Python, or host-scheduling fix.
- Leave a no-context model a reproducible capture and decision workflow that can distinguish those failure classes without operator mouse or keyboard takeover.

# Corrected governed incident evidence

- The governed v0.1.179 normal-window watch at `%APPDATA%\com.voxvulgi.voxvulgi\diagnostics\external_watch\watch_20260823-055128` observed PID 36220 for the complete 300,362-ms sample window.
- All 127 samples reported the app window as not responding, while all 127 bridge probes succeeded and the bridge PID remained the app PID. This proves a split between native-window responsiveness and the bridge thread during that incident. It does not identify why the window was unresponsive.
- The same run observed 27 command starts and 27 completions, zero incomplete commands, maximum command overlap 10, two frontend long tasks with maximum 775 ms, two samples with heavy child processes, and nine read-only database probe timeouts.
- Those co-occurring observations do not prove a causal chain. In particular, bridge liveness does not prove WebView renderer/compositor health, a database timeout does not identify a lock holder, and a long Tauri command does not prove that command blocked the native message pump.
- The watch reported `WPR/WebView2 capability ready: False` because no Microsoft WebView2 WPR profile path was supplied. No ETW trace exists for the exact incident, so renderer, GPU, compositor, DWM, native message-pump, storage, and host-scheduling attribution remains unresolved.
- Offline hydration later completed successfully after 575,965 ms. Worker and main-thread heartbeat payload generation continued during the apparent trace gap and was persisted later in a burst. Therefore, that incident cannot be described as a dead Worker or permanently hung hydration. WP-0313 owns the corrected generation/receipt/persistence timing contract.
- A historical v0.1.169 run against a then-1,066,110,976-byte canonical database recorded an 85-ms Options-to-Media-Library render, a 3,105-ms asynchronous query, and html2canvas-associated 1,091-ms/339-ms Worker freeze detections. Because an agent-controlled headless process was launched against canonical operator app data, its proof summary is invalidated for current-profile/non-mutation closure. The timings may inform observer-interference hypotheses only; they cannot prove ordinary navigation behavior or satisfy WP-0314. Fresh exact panel evidence must be agent-observation-only on an already operator-started process.

# Authority and packet boundaries

- `AGENTS.md`, `CLAUDE.md`, `build_rules.md`, `governance/workflow/PROOF_STANDARD.md`, and the existing headless bridge contract remain authoritative.
- WP-0298 owns the overall freeze investigation, bounded incident capture, exact current-case job-start proof, and final integration closure. It is an authority/integration consumer, not a completion predecessor for WP-0314.
- WP-0309 owns the existing external-watch startup/lifecycle hardening and is complete. Extend its machine-readable schema and maintain both watcher copies plus synthetic tests; do not replace it with an unrelated monitor.
- WP-0310's database-first startup order remains fixed and complete.
- WP-0311 must first bound Diagnostics demand, command lifetime, and duplicate heavy probes so attribution is not polluted by known avoidable fan-out.
- WP-0312 supplies database-operation and lock-candidate attribution; WP-0314 must consume its receipts rather than infer database causality from watcher timeout counts.
- WP-0313 supplies source-emitted, native-received, and durably-persisted heartbeat timing. WP-0314 consumes that contract to separate Worker/main-thread/transport/trace-writer delay.
- This packet may instrument and diagnose the native-window/WebView2 boundary. It may implement a product remediation only after an exact captured incident selects the smallest causal class. It must not preselect transparent-window, frameless-window, GPU, browser-argument, or DWM changes from hypothesis.

# Relevant implementation and evidence surfaces

- `product/desktop/src/lib/freezeDetector.ts` and `product/desktop/src/lib/freezeDetector.worker.ts`.
- `product/desktop/src-tauri/src/lib.rs`, especially bridge startup/health/state, freeze ingress/dump, diagnostics trace, and desktop setup.
- `product/desktop/src-tauri/tauri.conf.json` and Windows/WebView2 runtime construction.
- `governance/scripts/vv_watch.ps1`, `product/desktop/src-tauri/watcher/vv_watch.ps1`, and `governance/scripts/test_vv_watch.ps1`.
- Diagnostics incident/freeze-report UI and the headless `/agent/state`, `/agent/dump`, `/agent/ui_audit`, and `/agent/ui_action` routes.
- `%APPDATA%\com.voxvulgi.voxvulgi\diagnostics\external_watch\watch_20260823-055128\summary.md`, `summary.json`, `samples.jsonl`, and `wpr_capability.json` as the governed baseline.
- WP-0298's invalidated historical large-database timing bundle, usable only for hypothesis/observer-cost context and never as closure evidence, plus the governed v0.1.179 trace generations.

</topic>

<topic id="research-selected-design-and-scope" status="active" version="v1" wp="WP-0314" updated_at="2026-08-23">

# Research basis

## Sources checked

- Current VoxVulgi freeze detector, Worker, trace writer, bridge, watcher, WebView configuration, proof tooling, and governed incident artifacts.
- Microsoft WebView2 process model, including distinct browser, renderer, GPU, utility, and audio processes and the `ProcessFailed` signal: `https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/process-model`.
- Microsoft WebView2 performance guidance, including reducing host/web IPC, splitting long JavaScript, careful native workload priority, real-scenario testing, and using the Microsoft `WebView2.wprp` profile with WPR/ETW: `https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/performance`.
- Microsoft WPR, custom recording-profile, and WPA Exporter documentation: `https://learn.microsoft.com/en-us/windows-hardware/test/wpt/introduction-to-wpr`, `https://learn.microsoft.com/en-us/windows-hardware/test/wpt/wpr-command-line-options`, `https://learn.microsoft.com/en-us/windows-hardware/test/wpt/authoring-recording-profiles`, and `https://learn.microsoft.com/en-us/windows-hardware/test/wpt/exporter`.
- Windows process/window responsiveness and process-tree data already collected by `vvwatch`; exact API semantics used by any new probe must be recorded in its implementation research receipt.

## Relevant field patterns

- A WebView2 desktop app spans at least the host/native window and browser, renderer, GPU, and utility processes. One healthy process or thread cannot establish that the others are responsive.
- Windows `Responding=false` is a native-window/message-response observation. It is not equivalent to JavaScript event-loop death, renderer crash, database lock, or bridge failure.
- Cross-boundary events need a shared incident ID, sequence, source timestamp, receive timestamp, durable timestamp, and process/thread identity. Without those fields, clocks and file ordering can create false causal arrows.
- ETW/WPR is the appropriate bounded system-level evidence source for CPU scheduling, disk I/O, process/thread activity, and WebView2 providers. A capture must be explicitly started/stopped around an operator-recognized incident and must report dropped events and profile availability.
- Observer tools can create the stall they claim to measure. Screenshot/html2canvas, UI audit, database probes, path probes, ETW, and high-frequency sampling each need cost and interference receipts.

# Selected design

- Define one machine-readable liveness matrix per sample/incident with independent states for: native window/message response, WebView2 browser process, renderer process, GPU process, JavaScript main heartbeat, Worker heartbeat, Worker-to-bridge request, agent bridge, Tauri command executor, diagnostics ingress queue, diagnostics durable writer, database runtime, child-process lane, filesystem/NAS probe, and host scheduling.
- Correlate observations only through a shared incident ID, app/build/PID, process creation identity, monotonic sequence, and explicit time fields. Preserve each observer's clock source and uncertainty; do not treat wall-clock order alone as causality.
- Extend `vvwatch` to inventory the VoxVulgi-owned WebView2 process group and record per-process kind where the current runtime exposes it, otherwise record executable/PID/parent/creation-time evidence and mark kind `unresolved`. Track lifecycle, CPU, working set/private bytes, I/O deltas, responsiveness/crash signals where supported, and sample cost.
- Add a low-cost native-window heartbeat/ack path independent from the JavaScript Worker and bridge. It must expose message-pump acknowledgement latency without stealing focus or simulating keyboard/mouse input. If the platform/API cannot provide this truth from the current stack, record the limitation rather than synthesize a proxy.
- Consume WP-0313's emitted/received/persisted/source-acknowledged heartbeat schema to classify `not_generated`, `generated_not_received`, `received_not_acknowledged`, `received_not_persisted`, `persisted_late`, `acknowledged_stage_mismatch`, `duplicate`, and `healthy`. WP-0314 adds only its separate native-window acknowledgement path; it must not reopen or modify WP-0313's completed heartbeat contract. Consume WP-0312's database operation/lock receipts and WP-0311's semantic-operation registry.
- Make Microsoft WebView2 WPR capture a capability of `vvwatch`: detect `wpr.exe`, validate an operator-supplied `WebView2.wprp`, state the exact unavailable reason, and expose explicit bounded start/mark/stop/cancel commands. Never start ETW on ordinary app startup or watcher invocation.
- Bound capture by duration and output bytes, use a unique WPR instance name, always finalize or cancel an owned session, record dropped events/status, and write the ETL plus a machine-readable manifest beside the watch. The watcher may stop only the exact WPR session it started.
- Detect the Windows Performance Toolkit's `wpaexporter.exe` separately from `wpr.exe`. Add a versioned repo-owned WPA view profile and quiet export/parser workflow under `governance/scripts/etw/` that exports the incident-marker time range to CSV and canonical JSON. The profile/manifest maps required CPU scheduling, process/thread, disk/file I/O, WebView2 generic/provider, and available GPU/compositor tables to exact classifier fields; it records WPR/WPAExporter/profile hashes and versions, table/filter names, row counts, symbol state, missing tables, parse errors, and dropped events. The classifier consumes this reproducible export, not an opaque ETL or unsupported analyst recollection.
- Add incident markers for page/command/startup phase, snapshot/audit activity, heavy child processes, database operations, and operator freeze recognition. Mark proof-tool interference and exclude it from ordinary-product latency verdicts.
- Run an exact baseline on the governed normal desktop build with WP-0311 through WP-0313 changes present. Use the liveness matrix plus ETW to classify the incident as one or more evidenced boundaries: native message pump, JavaScript main task, Worker, host/IPC transport, diagnostics writer, WebView2 browser, renderer, GPU/compositor, database/storage/child workload, host scheduling, or `unresolved`.
- Only after classification, implement the smallest remediation supported by the capture. Examples are deliberately non-normative: splitting a proven main-thread task, fixing a native message-pump stall, changing a proven harmful window/composition flag, bounding a proven GPU path, or rescheduling an attributed native workload. Every change requires an identical before/after incident recipe and rollback switch where practical.
- `unresolved_with_complete_capture` is allowed only after a valid synchronized WPR/ETW capture, successful versioned WPA export, required-table/marker coverage, and acceptable dropped-event receipt still fail to select a causal boundary. Missing `WebView2.wprp`, WPR/WPAExporter, required tables, permissions, or valid export keeps WP-0314 blocked/not-DONE unless the operator explicitly changes the proof surface. This packet is not allowed to turn missing proof into a completed diagnosis or speculative fix.

# Scope edges

## In scope

- Cross-process liveness schema, process-group lifecycle/resource observation, native-window acknowledgement, clock/sequence correlation, observer-cost reporting, and incident classification.
- Operator-triggered, bounded WebView2 WPR/ETW capture and its availability/failure contract.
- Exact current-profile incident observation after upstream load/DB/hydration hardening by attaching only to an already operator-started normal-window process; no agent launch, navigation, mutation, or stop is allowed in that cell.
- One evidence-selected remediation plus identical before/after proof, or `unresolved_with_complete_capture` only after the full valid WPR/WPA export contract selects no remediation.
- Quiet headless regression proof with `VOXVULGI_AGENT_HEADLESS_BASE_DIR` set to an owned disposable absolute root, plus agent-driven controlled normal-window proof only inside an owned disposable VM/snapshot.

## Non-goals

- Assuming `transparent: true`, `decorations: false`, a DWM issue, GPU acceleration, renderer starvation, SQLite, hydration, or Python is the root cause before capture.
- Replacing the database, changing installer payloads, or reopening completed WP-0309/WP-0310 scope.
- Starting ETW continuously, shipping large traces by default, recording unrelated processes without a bounded incident need, or stopping an unowned WPR session/process.
- Using html2canvas or a UI audit inside the causal timing window without marking and separately measuring its interference.
- Declaring the complete freeze problem fixed from a headless-only run, a bridge-only health check, or disappearance of a single trace event.

# Rejected options

- Disable transparency/decorations or GPU immediately: the exact incident has no ETW evidence selecting any of those hypotheses.
- Treat 127 healthy bridge probes as proof the UI was healthy: contradicts the 127 native-window not-responding samples.
- Treat 127 native-window failures as proof JavaScript stopped: heartbeat payload generation continued in the corrected hydration evidence.
- Infer a SQLite cause from nine watcher timeouts: the lock holder and relationship to native response are unproven.
- Increase watcher sample rate until the issue is visible: raises observer cost and still lacks cross-boundary identity.
- Make automatic screenshots the freeze detector: current exact proof shows html2canvas itself can cause Worker stalls.

</topic>

<topic id="roi-red-team-and-controls" status="active" version="v1" wp="WP-0314" updated_at="2026-08-23">

# High-ROI additions and reuse

- Unified liveness matrix.
  - Why high ROI: turns future freeze reports into boundary evidence instead of a new round of hypotheses.
  - Gap addressed: current bridge, Worker, native window, command, database, and watcher signals are not reconciled in one receipt.
  - Reuse: WP-0298 incident IDs, WP-0309 watcher schema, WP-0311 operation IDs, WP-0312 DB receipts, and WP-0313 heartbeat times.
  - Validation: deterministic injected failures at each observable boundary produce different classifications.
- Owned bounded WPR workflow.
  - Why high ROI: captures renderer/GPU/host scheduling evidence that in-app traces cannot see.
  - Gap addressed: exact v0.1.179 incident lacked a supplied WebView2 WPR profile.
  - Reuse: current watcher capability file and output directory lifecycle.
  - Validation: unavailable, invalid-profile, start, mark, stop, timeout, cancellation, and dropped-event cases.
- Observer-interference ledger.
  - Why high ROI: prevents another screenshot-induced stall from being reported as the product defect.
  - Gap addressed: current proof already contains html2canvas-induced freezes.
  - Reuse: trace operation IDs and watcher sample elapsed/schedule-lag fields.
  - Validation: capture with/without each observer and explicit inclusion/exclusion reason.
- Evidence-gated A/B runtime switches.
  - Why high ROI: lets a no-context model test one selected renderer/window hypothesis without permanently altering production defaults.
  - Gap addressed: rollback would otherwise require a second speculative code change.
  - Reuse: current Tauri config/build pipeline and packaged proof process.
  - Validation: same incident recipe, one variable, versioned artifacts, independently reviewed verdict.

# Red-team risks, scenarios, controls, and verification

- WPR captures unrelated sensitive activity or grows without bound.
  - Control: operator-triggered only, explicit profile/provider manifest, bounded duration/size, output under governed diagnostics, unique owned instance, no automatic upload.
  - Verify: duration/size cutoff, stop/cancel, unexpected-exit cleanup, manifest, and no background capture after exit.
- The watcher disrupts the process it observes.
  - Control: bounded low-cost samples, per-sample elapsed/schedule lag, no direct live-database probe during DB-sensitive phases, no focus/input actions, and interference comparison.
  - Verify: baseline with watcher disabled/enabled plus skipped intervals and observer CPU/I/O.
- PID reuse attaches evidence to the wrong process.
  - Control: record PID plus creation time, executable identity, product version, parent relation, and bridge startup identity.
  - Verify: synthetic app restart/PID change and stale sidecar tests.
- WebView2 helper processes are misclassified.
  - Control: prefer runtime process-kind APIs/events; otherwise label heuristics as unresolved and preserve raw parent/path/creation evidence.
  - Verify: process creation/exit fixture and comparison with WebView2 process information where accessible.
- Wall-clock skew creates false ordering.
  - Control: sequences and per-origin monotonic deltas; wall time only for approximate cross-process correlation with uncertainty recorded.
  - Verify: injected wall-clock skew and reordered persistence.
- A crash is mistaken for a hang or a hang for a crash.
  - Control: separate process-exit/`ProcessFailed`/bridge loss/native-response/heartbeat states and terminal reason.
  - Verify: owned renderer test crash, main-task block, Worker stop, bridge stop, and normal recovery.
- ETW is unavailable on the operator machine.
  - Control: capability receipt states missing WPR, WPAExporter, capture profile, export profile, permission, provider, or required table separately; WP-0314 remains `BLOCKED`/not-DONE rather than substituting conjecture. Only the operator may redefine the proof surface.
  - Verify: each unavailable path, a known-valid capture/export path, and the DONE evaluator rejecting unavailable evidence.
- ETL exists but analysis cannot be reproduced.
  - Control: versioned `.wpaProfile`, marker-bounded WPAExporter command, canonical CSV-to-JSON parser, tool/profile hashes, required-table coverage, and classifier input manifest.
  - Verify: independent re-export of the same ETL produces equivalent canonical classifier input; missing table/parse/dropped-event cases fail closed.
- A renderer/window flag workaround masks but does not fix the cause.
  - Control: only one evidence-selected variable, same before/after workload, rollback switch, regression matrix for visual/composition/accessibility behavior.
  - Verify: repeated incidents, ETW delta, screenshots outside timing window, keyboard navigation, multi-monitor/DPI/resume where relevant.
- The packet claims success because the bridge stayed alive.
  - Control: success requires native-window response plus all selected boundary budgets; bridge is one cell only.
  - Verify: acceptance evaluator rejects bridge-only proof.
- No incident recurs during a short validation run.
  - Control: record exposure duration/workload count and confidence; require repeated baseline reproduction or a deterministic injected boundary before claiming a remediation effect.
  - Verify: identical bounded repeated trials and truthful inconclusive outcome.
- A causal proof launch changes the operator's canonical database or queues background work before the incident window.
  - Control: the exact current-profile cell attaches observers only to an already operator-started process and records that the live process may perform ordinary operator-owned background mutations; it is never labelled read-only. Every agent-started headless regression sets the supported disposable base-dir override, and every agent-driven normal-window, injected, visual, input, or A/B run stays in an owned disposable VM/snapshot.
  - Verify: process-initiation/ownership receipt; no agent-driven current-profile launch/navigation/mutation/stop; resolved disposable-root and sidecar/database evidence for headless; VM/snapshot identity for normal-window work; and independent read-only observer commands. Missing operator exposure or isolation proof keeps WP-0314 blocked/not-DONE.

</topic>

<topic id="microtasks-acceptance-and-proof" status="active" version="v1" wp="WP-0314" updated_at="2026-08-23">

# Ordered microtask plan

1. Freeze the corrected v0.1.179 baseline into RED fixtures and a machine-readable expected-liveness matrix; assert that native-window failure plus healthy bridge is `split_liveness_unattributed`, not a root cause.
2. Define versioned liveness-sample, process-identity, observer-cost, incident-marker, classification, and WPR-manifest schemas with stable IDs and source/clock semantics.
3. Consume WP-0313's completed generated/received/persisted/source-acknowledged heartbeat receipts, WP-0311 semantic operations, and WP-0312 database receipts without modifying their owned contracts.
4. Add a quiet native-window acknowledgement probe and deterministic main-task, Worker, bridge, trace-writer, and native-message delay fixtures. Record unsupported platform/API states truthfully.
5. Extend `governance/scripts/vv_watch.ps1` and `product/desktop/src-tauri/watcher/vv_watch.ps1` with WebView2 process-group lifecycle/resource inventory, PID-creation identity, observer-cost fields, and exact parity/self-tests.
6. Implement operator-triggered owned WPR capability/start/mark/stop/cancel with `WebView2.wprp`, unique instance identity, bounded duration/size, dropped-event/status capture, and cleanup. Do not autostart it.
7. Add the versioned repo-owned WPA view profile, WPAExporter detection/command, incident-marker time-range export, CSV-to-canonical-JSON parser, required-table/provider mapping, tool/profile hashes, symbol state, and independent re-export parity tests under `governance/scripts/etw/`.
8. Build the cross-source reconciler/classifier over canonical watcher/trace/WPA-export input and prove each deterministic injected boundary maps to a distinct result or explicitly `unresolved`.
9. Run upstream WP-0311 through WP-0313 regressions using owned disposable roots. For the exact current-profile causal cell, attach watcher/WPR/ETW observers only after the operator has independently started the unchanged normal-window process and triggers the exposure; the agent may not launch, navigate, mutate, or stop it. Capture watcher, WPR/ETL, marker-bounded WPA exports, trace, and operator freeze marker without screenshots/audits inside the timing window. If that operator-initiated exposure is unavailable, keep WP-0314 blocked/not-DONE rather than substituting an agent-started host run.
10. Verify WPR/WPA tool/profile/table/marker/dropped-event completeness, then compare ETW/process/liveness evidence and select one causal class. If the valid complete capture selects none, document `unresolved_with_complete_capture` and stop product mutation. If required ETW evidence is unavailable/invalid, mark WP-0314 `BLOCKED` and do not classify it complete.
11. If evidence selects a class, implement the smallest remediation behind a reversible configuration/build switch where practical. Run the identical workload and exposure window before/after.
12. Run headless bridge/UI regressions with `VOXVULGI_AGENT_HEADLESS_BASE_DIR` set to a preflighted owned disposable absolute root and prove its database/config/trace/bridge sidecars resolve there. Run controlled agent-driven normal-window visual, input, DPI/multi-monitor/resume, injected-boundary, and before/after tests only inside an owned disposable VM/snapshot. Capture screenshots only after the timing window.
13. Propagate the implemented capture/export/classifier/recovery workflow and any selected product remediation into the Sibling External Watch sections of `AGENTS.md` and `CLAUDE.md`, `governance/spec/TECHNICAL_DESIGN.md`, and `product/desktop/src/pages/DiagnosticsPage.tsx`; keep the twin authority files semantically identical. Repo search on 2026-08-23 found no standalone product-code/governance topology or general built-in model-manual artifact; do not invent or claim those updates. Record each missing surface in proof and route a separate operator proposal for its canonical path.
14. Build the governed semantic version, perform independent adversarial review, remediate every finding, and hand the exact classification/remediation receipt to WP-0298 final integration closure.

# Acceptance criteria

- A no-context model can reconstruct each incident across native window, WebView2 process group, JavaScript main thread, Worker, bridge, command executor, trace writer, database, child workload, and host-scheduling observations using stable IDs and stated clock semantics.
- `Responding=false` plus healthy bridge is represented as split liveness and never converted automatically into renderer, compositor, database, or JavaScript causality.
- The watcher identifies process instances without PID-reuse ambiguity, records its own cost, preserves both copies byte-for-byte, and passes synthetic/live parity tests.
- WPR/WPA capability reports exact availability. Capture starts only on explicit operator action, is duration/size bounded, uses an owned unique instance, finalizes/cancels on every owned path, and records capture/export profiles, tool versions/hashes, status, dropped events, markers, required tables, parse outcome, and output identities.
- The exact current-profile workload is operator-initiated and has a valid synchronized read-only watcher + trace + WPR/ETL + marker-bounded WPA export. The agent did not launch, navigate, mutate, or stop that process. Missing operator exposure or invalid/missing WPR/WPA evidence keeps the packet `BLOCKED`/not-DONE unless the operator explicitly changes the proof surface; disposable-VM, headless-only, and bridge-only evidence cannot replace this causal cell.
- Proof tools used inside the timing window are itemized with measured cost; screenshot/html2canvas interference is excluded from ordinary navigation verdicts.
- The incident receives one evidence-supported classification or `unresolved_with_complete_capture` only after valid required ETW/export coverage still selects no class. No speculative product fix is made for an unresolved or blocked capture.
- If a remediation is made, identical before/after repeated trials show native-window responsiveness and the selected boundary advancing without regression in bridge, UI, jobs, database-first startup, or visual behavior.
- No operator keyboard/mouse is hijacked and no unowned process or WPR session is stopped. Agent-owned proof never targets user databases/media. The current-profile cell is explicitly agent-observation-only through independent read-only probes; ordinary mutations by the already operator-started app are attributed to that process and are never misreported as a globally read-only session.

# Proof contract

- Verification class: high-risk operator-initiated current-profile causal attribution plus disposable-root headless and disposable-VM normal-window regression.
- Required proof root: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0314/<run-id>/`.
- Required artifacts: baseline and final liveness matrices, watcher outputs, exact process identities, observer-cost ledger, WPR/WPA capability receipt, ETL, capture manifest, versioned WPA profile, marker-bounded CSV exports, canonical JSON classifier input, export/parity receipt, trace/freeze report, workload recipe, classification decision, and before/after result if remediated.
- Required RED/GREEN cases: native-message delay, JavaScript main-task delay, Worker failure, bridge failure, trace-writer delay, PID restart/reuse, WPR/WPA unavailable/invalid/start/stop/cancel/export, required-table loss, parse failure, dropped events, observer interference, disposable-root/VM isolation, and the exact operator-initiated current-profile normal-window workload. Unavailable operator exposure or invalid ETW cases must prove the packet cannot become `DONE`.
- Required regression cases: WP-0311 demand bounds, WP-0312 database receipts, WP-0313 timestamp reconciliation, WP-0310 startup order, headless bridge/API with the owned base-dir override, disposable-VM visual readability/navigation/input, and no-focus/no-input operation on the operator host.
- Independent adversarial review must verify that every causal claim is supported by the exact capture and that no co-occurrence was promoted to cause.

</topic>
