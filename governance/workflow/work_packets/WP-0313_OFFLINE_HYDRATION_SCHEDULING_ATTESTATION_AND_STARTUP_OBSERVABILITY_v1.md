---
file_id: WP-0313-v1
file_kind: work_packet
updated_at: 2026-08-23
---

<topic id="contract" status="backlog" version="v1" wp="WP-0313" updated_at="2026-08-23">

# Work Packet: WP-0313 — Offline hydration scheduling, attestation, and startup observability

## Metadata

- ID: WP-0313
- Owner: —
- Status: BACKLOG
- Created: 2026-08-23
- Refinement: `WP-0313_OFFLINE_HYDRATION_SCHEDULING_ATTESTATION_AND_STARTUP_OBSERVABILITY_v1_REFINEMENT.md`
- Board: `../TASK_BOARD.md`
- Hard predecessor: WP-0310 (already complete)
- Authority/reuse sources: WP-0054, WP-0219, WP-0308; WP-0308 completion is not required unless implementation changes its installer-owned artifacts
- Umbrella/integration owner: WP-0298; it supplies incident authority and receives proof but is not a completion predecessor
- Coordinated packets: WP-0311, WP-0312
- Downstream attribution: WP-0314

## Intent

Preserve full verified-byte offline provider readiness while making hydration one bounded, observable, foreground-friendly flight and making heartbeat/startup generation, receipt, queue, and persistence delay independently measurable.

## Deliverables

- Named hydration/provider-verification phases with progress and terminal truth.
- One current-process provider verification flight and atomic payload-generation attestation.
- Safe bounded Windows background scheduling/chunking selected from exact evidence.
- Research-gated incremental verified-work reuse or an explicit evidence-based rejection with bounded full-scan fallback.
- Event-driven startup progress with canonical snapshot/revision recovery.
- Main/Worker heartbeat emitted/received/persisted/source-acknowledged/queue-dwell contract and watcher parity.
- Exact packaged full-payload headless proof with an owned disposable absolute base-dir override and agent-started normal-window proof only inside an owned disposable VM/snapshot.

## Required implementation order

1. Slow-tree RED fixture and current timing/resource baseline.
2. Phase/progress/terminal schema and single-flight.
3. Heartbeat emitted/received/persisted/source-acknowledged timing, queue accounting, and event-driven startup state.
4. Windows scheduling and incremental-verification research/selection.
5. Foreground-priority and provider-execution gate integration.
6. Negative/security tests and exact full-payload benchmark.
7. Exact product spec/design/offline manual/Diagnostics help propagation, recorded missing-topology/model-manual proposal, governed build, visual proof, and adversarial review.
8. WP-0314/WP-0298 handoff.

## Non-goals

- Skipping authentication, trusting persisted status alone, downloading dependencies, changing payload contents, or moving payload roots.
- Reopening WP-0310 startup ordering.
- Claiming hydration caused the window freeze or that Worker timers died.
- Solving renderer/compositor attribution owned by WP-0314.

## Acceptance and proof

- The refinement is normative.
- The 575,965-ms incident is recorded as a successful but excessively long verification with delayed heartbeat persistence—not a permanent hang.
- Exact full payload, integrity/tamper, responsiveness, heartbeat delivery, and provider execution-gate proof are mandatory.
- Every agent-started headless proof must set and verify `VOXVULGI_AGENT_HEADLESS_BASE_DIR`; every agent-started normal-window proof must remain in an owned disposable VM/snapshot. Optional current-profile evidence observes only an already operator-started process.
- Toy trees, build-only checks, or a persisted receipt cannot close this packet.
- `governance/workflow/PROOF_STANDARD.md`, offline payload policy, WP-0310 ordering, and user-data preservation rules apply.

</topic>

<topic id="status-updates" status="active" version="v1" wp="WP-0313" updated_at="2026-08-23">

# Status updates

- 2026-08-23: Created after reconciling the stale watcher cutoff with the later governed v0.1.179 terminal trace and embedded heartbeat generation times. The refinement explicitly retracts permanent-hang and dead-Worker claims. No product code, payload, app data, or process changed.
- 2026-08-23: Status is BACKLOG pending later implementation.

</topic>
