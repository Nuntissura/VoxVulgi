# Work Packet: WP-0278 - Panel switch and watcher contention hardening

## Metadata

- ID: WP-0278
- Owner: Codex
- Status: DONE
- Created: 2026-07-26
- Refinement: `WP-0278_PANEL_SWITCH_AND_WATCHER_CONTENTION_HARDENING_v1_REFINEMENT.md`
- Dependencies: `WP-0275`, `WP-0276`, `WP-0277`

## Intent

Expand low-impact external diagnostics and fix evidence-proven panel/job-start contention while the new identity and cleanup workflows are exercised under realistic heavy host load.

## Scope

- Watcher cadence/probe instrumentation and single-flight budgets.
- Current-artifact version matching.
- Panel transition, command overlap, DB wait, queue/claim, NAS-stage, and host-pressure evidence.
- Evidence-driven panel/job-start responsiveness changes.

## Acceptance criteria

- All refinement acceptance and controls pass.
- Diagnostics do not close, reconfigure, or interrupt unrelated processes.
- Current built artifact passes headless navigation and start-job responsiveness proof.
- Proof bundle satisfies `PROOF_STANDARD.md` before `DONE`.

## Status updates

- 2026-07-26: Contract created; baseline watcher evidence recorded for comparison with the rebuilt artifact.
- 2026-07-26: `vvwatch` now records requested/actual cadence, skipped intervals, per-probe duration, current bridge artifact version, command overlap, database contention, panel-render latency, cleanup-stage timing, and bounded host pressure from a live trace tail.
- 2026-07-26: Historically slow library/subscription/archive/activity/queue-control reads now run through bounded blocking workers instead of synchronous command handlers; panel transitions receive start/render IDs, and superseded library refresh results are discarded after a deferred first paint.
- 2026-07-26: Watcher self-test, frontend production build, and Tauri compile pass. Advanced to REVIEW pending shared packaged current-artifact navigation/watcher proof.
- 2026-07-26: Packaged v0.1.109 proof failed: `subscription_download_activity` occupied a synchronous Tauri handler for 115,366 ms and queued panel events were delivered only afterward. REVIEW was retracted; the command and synchronous track-runtime read were moved to the blocking pool before rebuild.
- 2026-07-26: DONE after corrective proof. The rebuilt packaged artifact delivered all tested panel targets; v0.1.112 measured eight transitions with no slow-panel, freeze/skew, bridge-failure, or incomplete-command event, and the final v0.1.113 CSS-only follow-up built and rendered correctly. The watcher now preserves requested duration while separating lightweight cadence from bounded heavy probes. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0278/summary.md`.
