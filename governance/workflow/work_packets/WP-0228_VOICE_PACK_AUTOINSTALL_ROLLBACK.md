# Work Packet: WP-0228 - Voice pack auto-install rollback (v0.1.26)

## Status

IN_PROGRESS

## Base Scope

- Remove the WP-0227 startup auto-enqueue of the Phase2 voice-pack install. Returns voice-pack install to being explicitly operator-triggered from the Diagnostics page.
- Keep the WP-0227 resume logic (carry forward already-`done` steps when the install runs) — safe addition, helps any manual install.

## Operator Request Preserved

- "app is unusable, freezes all the time have not touched it once because nothing happens because of freeze"
- "try to resize but froze, left it doing its thing but few minutes later still frozen, or frozen again"

WP-0227's auto-enqueue made the app unusable. The install's combined disk I/O + Python subprocess work + WAL writes during HuggingFace downloads saturated the operator's machine. Net regression vs v0.1.24.

## Research Basis

- v0.1.25 freeze trace (`freeze_report_1779053415656.json`) showed the auto-enqueue did fire as designed (`phase2_auto_install_enqueue` row 2 s after startup, reason "voice packs not fully installed at startup"). 27 `command_slow` rows followed, with the "first call slow, rest fast" pattern characteristic of cold SQLite / disk cache contending with heavy concurrent I/O.
- Two `main_thread_alive` gaps (61 s and 51 s) consistent with real main-thread blocks long enough that the operator could not interact with the app.
- Operator reported the running install left the app permanently unresponsive — could not even reach Diagnostics to cancel or check progress.
- Auto-enqueue was the dominant new behavior in v0.1.25 vs v0.1.24. Removing it returns the app to v0.1.24's responsive baseline.

### Selected approach

Remove the auto-enqueue spawn-thread block in `product/desktop/src-tauri/src/lib.rs` (the WP-0227 addition). Leave:

- The WP-0227 resume logic inside the install job handler at `product/engine/src/jobs.rs:9716+` — when the operator does click Install, it correctly carries forward steps marked `done` in the prior `latest.json` instead of redoing them. This is pure UX improvement to the manual install path.
- The `jobs::should_auto_install_phase2` predicate at `product/engine/src/jobs.rs:1252+` — unused after this rollback but harmless. May be reused later if auto-install returns as opt-in.

### Rejected options

- Throttling the auto-install: would require queue-priority work and per-job CPU/disk throttling, neither of which the engine currently supports. Out of scope for an emergency rollback.
- Detecting "operator active" and pausing the install: complex behavior, risks getting stuck never installing. Defer to a future Settings toggle if voice packs are ever brought back to auto.
- Making auto-install opt-in via a Settings toggle: correct longer-term direction but requires schema + UI work. v0.1.26 takes the minimum safe action.

## High-ROI Additions

- The decision to make voice packs manual restores the operator's control over when to spend the disk/network on the install.
- The kept resume logic ensures that the next time the operator does click Install (and lets it run), it picks up from where any prior interrupted attempt left off — the v0.1.25 lesson learned without the v0.1.25 cost.

## Reused Systems

- Existing manual install Tauri command `jobs_enqueue_install_phase2_packs_v1` (lib.rs:6709) still works exactly as before.
- Existing WP-0217 stale-state normalization still surfaces accurate per-pack status in Diagnostics.

## Gaps Closed

- App is usable again on operator's machine.
- v0.1.26 == v0.1.24 + read-only sweep + Phase2 install resume logic + freeze diagnostic infrastructure. No new behavior that competes for the operator's resources.

## Risks And Hardening

- Risk: operator forgets that voice packs need manual install.
  - Remediation: the Diagnostics → Voice cloning packages section is still present with the explicit "Install Voice cloning packages" button. Future WP could promote this to a more discoverable location (Localization page prerequisite check, first-run hint).
- Risk: a future agent re-introduces auto-enqueue without re-validating performance impact.
  - Remediation: WP-0228 status updates explicitly document the regression. Any future auto-install proposal must include a plan to throttle / yield to UI activity.

## Red-Team

- Failure scenario: rollback leaves the WP-0227 resume logic in a broken state if the manual install can no longer be triggered.
  - Control: the Tauri command path is unchanged. Manual install still calls `jobs::enqueue_install_phase2_packs_v1` which runs the install job handler. Resume logic is in that handler, exercised by every install attempt. Verified by reading the unchanged Tauri command at lib.rs:6709-6713.

## Acceptance Criteria

- `cargo build --release` succeeds.
- v0.1.26 startup trace shows **no** `phase2_auto_install_enqueue` row in the first minute.
- Operator confirms the app is usable on launch (no freeze cascade from background install activity).
- Manual click on "Install Voice cloning packages" in Diagnostics still works and respects the WP-0227 resume logic (skips previously-`done` steps).

## Verification

- `cargo build --release` clean.
- Desktop build via `governance/scripts/build_desktop_target.ps1`.
- Post-install: operator reports the app is usable on launch. No `vvfreeze.cmd` should be needed.

## Status Updates

- 2026-05-17: Emergency rollback created in direct response to operator report that v0.1.25 made the app unusable. Auto-enqueue removed; resume logic kept. Ships in v0.1.26.
