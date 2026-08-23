---
file_id: WP-0314-v1
file_kind: work_packet
updated_at: 2026-08-23
---

<topic id="contract" status="backlog" version="v1" wp="WP-0314" updated_at="2026-08-23">

# Work Packet: WP-0314 — WebView/native-window liveness and ETW attribution

## Metadata

- ID: WP-0314
- Owner: —
- Status: BACKLOG
- Created: 2026-08-23
- Refinement: `WP-0314_WEBVIEW_NATIVE_WINDOW_LIVENESS_AND_ETW_ATTRIBUTION_v1_REFINEMENT.md`
- Board: `../TASK_BOARD.md`
- Hard predecessors: WP-0221, WP-0309, WP-0310, WP-0311, WP-0312, WP-0313
- Umbrella/integration owner: WP-0298; it supplies incident authority and receives proof but is not a completion predecessor
- Integration successor: WP-0298 final closure

## Intent

Explain and remediate the proven split in which the native VoxVulgi window is continuously not responding while its agent bridge remains healthy, using cross-process liveness and bounded WebView2 WPR/ETW evidence before changing product behavior.

## Deliverables

- Versioned cross-boundary liveness matrix and incident classifier.
- WebView2 process-group and native-window observation in both `vvwatch` copies with measured observer cost.
- Operator-triggered, bounded, owned `WebView2.wprp` WPR capture plus versioned WPAExporter extraction/classifier input; unavailable required evidence is a blocker, not a completed diagnosis.
- Exact governed current-profile capture by attaching observers only to an already operator-started normal-window process, plus agent-driven normal-window/A-B work only inside an owned disposable VM/snapshot.
- One evidence-selected reversible remediation with identical before/after proof, or an explicit `unresolved_with_complete_capture` verdict without speculative mutation.
- WP-0298 integration handoff.

## Relevant files

- `product/desktop/src/lib/freezeDetector.ts`
- `product/desktop/src/lib/freezeDetector.worker.ts`
- `product/desktop/src-tauri/src/lib.rs`
- `product/desktop/src-tauri/tauri.conf.json`
- `governance/scripts/vv_watch.ps1`
- `product/desktop/src-tauri/watcher/vv_watch.ps1`
- `governance/scripts/test_vv_watch.ps1`
- Diagnostics/freeze-report UI and focused frontend/Rust tests

## Required implementation order

1. RED corrected-baseline and injected-boundary fixtures.
2. Liveness/process/clock/observer/WPR schemas.
3. WP-0311/WP-0312/WP-0313 receipt reconciliation.
4. Native-window acknowledgement and WebView2 process-group watcher extension.
5. Owned bounded WPR capability and capture workflow.
6. Versioned WPAExporter profile/export/parser and reproducible classifier input.
7. Cross-source classifier validation.
8. Exact current-profile watcher + ETW observation on an already operator-started process; block if the operator exposure or required ETW/export evidence is unavailable. Run agent-driven reproduction only in an owned disposable VM/snapshot.
9. Evidence-selected remediation or `unresolved_with_complete_capture` only after a valid complete capture.
10. Identical before/after, disposable-root headless, disposable-VM normal-window visual/input, and regression proof.
11. Exact authority/spec/Diagnostics help propagation, recorded missing-topology/model-manual proposal, governed build, independent adversarial review, and WP-0298 handoff.

## Non-goals

- Preselecting DWM, transparency, frameless window, GPU, renderer, SQLite, hydration, or Python as root cause.
- Treating bridge health, Worker heartbeats, native `Responding`, database timeouts, or command duration alone as full-app proof.
- Continuous ETW, unbounded traces, automatic screenshots in the timing window, input takeover, or stopping unowned processes/sessions.
- Closing a normal-window defect with headless-only evidence.

## Acceptance and proof

- The refinement is the normative evidence, research, architecture, ROI, red-team, microtask, acceptance, and proof contract.
- The exact incident must be causally classified from synchronized native-window, WebView2 process, heartbeat, bridge, trace, database, workload, host, and valid marker-bounded ETW/WPA-export evidence, or receive `unresolved_with_complete_capture` only after that complete capture. Missing required ETW/export capability leaves the packet blocked/not-DONE.
- Product behavior changes are forbidden until capture selects their causal class.
- The agent may not launch, navigate, mutate, or stop the current-profile process used for causal evidence. Every agent-started headless process uses an owned disposable `VOXVULGI_AGENT_HEADLESS_BASE_DIR`; every agent-started normal-window process runs only inside an owned disposable VM/snapshot.
- `governance/workflow/PROOF_STANDARD.md` and `build_rules.md` apply; build-only, bridge-only, screenshot-only, and synthetic-only proof cannot close this packet.

</topic>

<topic id="status-updates" status="active" version="v1" wp="WP-0314" updated_at="2026-08-23">

# Status updates

- 2026-08-23: Created from the governed v0.1.179 127/127 native-window not-responding versus 127/127 bridge-healthy split, corrected hydration/heartbeat evidence, WP-0298 proof-tool interference, current source inspection, and Microsoft WebView2/WPR primary documentation.
- 2026-08-23: Status is BACKLOG because the operator requested implementation-ready packets before later remediation. No renderer/window remediation is selected by this planning artifact.

</topic>
