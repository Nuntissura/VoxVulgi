# Work Packet: WP-0212 - Safe Mode pill placement and exit-rehydrate notice

## Metadata
- ID: WP-0212
- Owner: Codex
- Status: IN_PROGRESS
- Created: 2026-04-27
- Target milestone: Operator UX clarity
- Builds on: WP-0060

## Intent

- What: Replace the always-visible top-bar "Recovery" pill with a smaller end-of-topbar Safe Mode toggle that always reflects current state ("Safe Mode ON" / "Safe Mode OFF"), and show an in-app dismissable notice on exit that warns the operator a restart is required to rehydrate bundled assets.
- Why: Operator feedback is direct: the current "Recovery" label is unclear (recovery of what?), the pill is sized like a primary feature panel even though Safe Mode is a recovery affordance, and `safe_mode_set` flips the in-process flag and un-pauses the queue but does **not** kick the `offline_bundle` startup phase, so a user who exits Safe Mode is silently left in a half-state until they restart. The new pill makes state legible without dominating the chrome, and the in-app notice (not a native dialog, with an X to dismiss) keeps the operator informed without blocking interaction.

## Scope

In scope:
- Replace the `startup-pill-recovery` / `startup-pill-safe` button in `topbar-center` with a smaller pill anchored at the right end of the topbar (after the page nav), visually de-emphasized vs the page-nav buttons. Label always reflects current state: `"Safe Mode OFF"` when off, `"Safe Mode ON"` when on. Tooltip clarifies the action ("Enter Safe Mode" / "Exit Safe Mode").
- Toggle behavior:
  - **OFF -> ON**: instant. Calls `safe_mode_set(true)`, which already persists the flag and pauses the queue. The existing `shell-status-strip` "Safe Mode is ON" card continues to surface as today.
  - **ON -> OFF**: instant flag flip via `safe_mode_set(false)` (persist, un-pause queue). Then surface a new in-app dismissable notice in the existing `shell-status-strip` zone: title "Safe Mode disabled", body "Restart the app to rehydrate bundled assets that were skipped during Safe Mode startup." The notice has an `x` close button. Dismiss state is per-session only (clears on restart).
- Update the existing "Safe Mode is ON" banner copy to mention exit requires a restart to rehydrate bundled assets, so the operator is not surprised by the notice when they later toggle off.
- No native OS dialog and no modal blocking. The notice lives inline in the shell status strip alongside any startup status card.

Out of scope (deferred):
- Mid-session re-trigger of the `offline_bundle` phase from `safe_mode_set`. The phase has its own running/ready/error state machine and the rest of the startup tracker (db_schema, job_runner) already ran assuming the bundle was skipped, so a fresh boot is the safe path. Tracked as a follow-up if operator demand emerges.
- Moving the entry point into Options or Diagnostics. The pill itself remains the toggle; Options gains nothing this WP.
- Any change to `--safe-mode` CLI semantics or `cli_enabled` reporting in `SafeModeStatus`.

## Acceptance criteria

- The topbar no longer renders the `Recovery` label. A smaller pill sits at the right end of the topbar (after the page nav) and shows `"Safe Mode OFF"` or `"Safe Mode ON"` reflecting `safeMode.enabled`.
- Clicking the pill while OFF enters Safe Mode immediately (no notice appears). Clicking while ON exits Safe Mode immediately and surfaces the in-app dismiss-with-X notice.
- The notice is rendered in-app (no native window/modal), is non-blocking, and persists until the user clicks the X or restarts the app.
- The "Safe Mode is ON" banner copy mentions restart-on-exit.
- `cargo check` (engine + tauri) and desktop `npm run build` pass.
- Diagnostics startup card still reports `skipped_safe_mode` when the offline bundle was skipped during a Safe Mode boot (no regression).

## Test / verification plan

- Build: `cargo check` in `product/engine` and `product/desktop/src-tauri`; `npm -C product/desktop run build`.
- Manual smoke via Agent Bridge under `governance/snapshots/WP-0212/`:
  1. Fresh boot, Safe Mode OFF -> snapshot `topbar_off`. Verify pill at right end of topbar reads `"Safe Mode OFF"`.
  2. Click pill -> snapshot `topbar_on`. Verify pill reads `"Safe Mode ON"`, "Safe Mode is ON" status card visible with restart-on-exit copy, queue paused (visible in Jobs).
  3. Restart app (still ON) -> snapshot `restart_safemode`. Verify Diagnostics startup shows `skipped_safe_mode` for `offline_bundle`.
  4. Click pill -> snapshot `exit_notice`. Verify in-app notice appears with X close, pill reads `"Safe Mode OFF"`, queue un-paused.
  5. Click X on notice -> snapshot `exit_notice_dismissed`. Verify notice gone, no other UI change.
  6. Restart app -> snapshot `after_restart`. Verify `offline_bundle` reaches `ready`, no notice present, pill reads `"Safe Mode OFF"`.
- Pair each snapshot with `/agent/dump` so the proof bundle records mounted ids and console state.

## Risks / open questions

- Risk: the notice persisting only for the current session could surprise an operator who closes it then forgets to restart. Mitigation: the existing Diagnostics startup card already shows `skipped_safe_mode` so the truth is still visible even after dismiss.
- Risk: end-of-topbar placement could collide with the existing `topbar-chrome` move handle / window controls cluster. Mitigation: keep the pill inside `topbar-center` at its trailing edge rather than `topbar-chrome`, and verify across narrow window widths.
- Open: should an OFF -> ON toggle also require a restart to fully take effect (since `offline_bundle` already ran on this boot, mid-session enable only pauses the queue + persists the flag for next boot)? Defer: today's behavior is the correct minimum; flag this only if operator confusion shows up.

## Status updates

- 2026-04-27: Created.
