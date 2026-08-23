---
file_id: WP-0311-v1
file_kind: work_packet
updated_at: 2026-08-23
---

<topic id="contract" status="backlog" version="v1" wp="WP-0311" updated_at="2026-08-23">

# Work Packet: WP-0311 — Diagnostics demand orchestration, command lifetime, and probe single-flight

## Metadata

- ID: WP-0311
- Owner: —
- Status: BACKLOG
- Created: 2026-08-23
- Refinement: `WP-0311_DIAGNOSTICS_DEMAND_ORCHESTRATION_COMMAND_LIFETIME_AND_PROBE_SINGLE_FLIGHT_v1_REFINEMENT.md`
- Board: `../TASK_BOARD.md`
- Hard predecessors: WP-0221, WP-0309, WP-0310 (already complete)
- Umbrella/integration owner: WP-0298; it supplies incident authority and receives proof but is not a completion predecessor
- Coordinated packet: WP-0312
- Integration successor: WP-0298 final closure

## Intent

Stop Diagnostics and overlapping Options modules from automatically flooding Tauri, SQLite, the filesystem, and Python; make page-owned native work cancel, safely supersede, or share one bounded semantic flight when navigation changes ownership.

## Deliverables

- Shared cost-aware diagnostics request coordinator used by Diagnostics and overlapping Options modules.
- Cheap-first and section-demand loading without adding cards or removing data.
- App-wide single-flight/freshness contract for Torch/performance-tier and Demucs/module probes.
- Backend cancellation/supersession contract with safe checkpoints and trace receipts.
- One bounded read-only protection snapshot for download and enumeration; no automatic full-history replay on page mount.
- Exact packaged Diagnostics-to-Options runtime proof under `vvwatch`, using an owned disposable absolute headless root and an owned disposable VM/snapshot for every agent-started normal-window run.

## Relevant files

- `product/desktop/src/pages/DiagnosticsPage.tsx`
- `product/desktop/src/pages/OptionsPage.tsx`
- shared polling/request helpers under `product/desktop/src/lib/`
- `product/desktop/src-tauri/src/lib.rs`
- `product/engine/src/tools.rs`
- `product/engine/src/voice_backends.rs`
- `product/engine/src/youtube_protection.rs`
- focused desktop and engine contract tests

## Required implementation order

1. RED overlap, duplicate-probe, cancellation, cache-invalidation, and protection-replay fixtures.
2. Machine-readable semantic operation registry and bounded coordinator.
3. Diagnostics/Options demand orchestration and truthful section state.
4. Heavy-probe single-flight and complete invalidation.
5. Backend cancellation/supersession and owned-child lifetime.
6. Combined protection projection and removal of automatic replay.
7. Trace/source guards, focused and regression tests.
8. Exact product spec/design/Diagnostics/Options help propagation plus a recorded proposal for the currently missing standalone topology/model-manual surfaces.
9. Governed packaged headless proof with `VOXVULGI_AGENT_HEADLESS_BASE_DIR` set to an owned disposable absolute root, plus normal-window `vvwatch` proof only in an owned disposable VM/snapshot; optional current-profile evidence is observation-only on an already operator-started process.
10. Independent adversarial review and WP-0298 handoff.

## Non-goals

- Database-engine replacement or WP-0312 implementation.
- Removing diagnostics, changing protection policy, adding cards, or hiding missing data.
- Killing unowned processes or claiming frontend stale-result suppression is backend cancellation.
- Treating the correlated v0.1.179 pressures as a proven single causal chain.
- Throttling production localization, downloader, inference, or job-runner Python work; this packet's two-task bound is only for Diagnostics/Options capability probes.

## Acceptance and proof

- The refinement is the normative evidence, research, architecture, ROI, red-team, microtask, acceptance, and proof contract.
- Ordinary page entry must have zero duplicate semantic probes, at most one Torch child, at most one Demucs child, no automatic full-history replay, and at most two Python-heavy tasks inside the capability-probe admission domain without changing production-job concurrency.
- Diagnostics-to-Options proof must show queued work removed, safe cancellation or one shared terminal flight, no stale commit, and exact trace/child-PID receipts.
- No agent-started proof launch may resolve to canonical app data: headless runs must prove the disposable override is effective, normal-window runs must stay inside the disposable VM, and any current-profile cell observes only an already operator-started process without agent launch, navigation, mutation, or stop.
- Build-only or synthetic-only proof cannot close this packet.
- `governance/workflow/PROOF_STANDARD.md` and `build_rules.md` apply.

</topic>

<topic id="status-updates" status="active" version="v1" wp="WP-0311" updated_at="2026-08-23">

# Status updates

- 2026-08-23: Created from corrected governed v0.1.179 trace/watcher evidence, direct source inspection, WP-0298 ownership reconciliation, and current React/Tauri/WebView2 primary documentation. No product code, user data, task runtime, or process state changed.
- 2026-08-23: Status is BACKLOG because the operator requested implementation-ready work packets before later remediation. Packet creation is not implementation progress.

</topic>
