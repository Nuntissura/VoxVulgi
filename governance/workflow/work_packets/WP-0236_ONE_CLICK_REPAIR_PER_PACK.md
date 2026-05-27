# Work Packet: WP-0236 - One-click "Repair this pack" using lockfile

## Status

BACKLOG

## Owner

-

## Operator Request Preserved

- "how can we harden it ? so we are sure the downloading happens?" (2026-05-18) — user-friendly recovery is the other half of "sure the downloading happens".

## Intent

- What: A single button per pack ("Repair") that wipes the pack's installed state and reinstalls cleanly from the lockfile + bundled wheels. The user does not need to know what pip, transformers, or hf_hub are.
- Why: When something goes wrong (and with multi-GB downloads + Python venvs, it eventually will), the current recovery path is "delete the APPDATA folder and reinstall the whole app". Repair turns recovery into a 1-click, 1-pack operation.

## Scope

In scope:
- Frontend: Add a "Repair" button per pack row in the Localization Studio setup flow (WP-0235) and in the Diagnostics voice-pack section. Confirmation dialog: "This will reinstall <pack name> (about X MB). Other packs are not affected. Continue?"
- Backend: A `repair_pack` Tauri command that:
  - removes the pack's staged venv / site-packages (depends on WP-0234 atomic install primitive),
  - clears `kokoro/.warmup_ok` and any equivalent probe files,
  - re-runs the pack install with the new state.
- Repair is per-pack, never global ("Repair all" is intentionally not added — too easy a foot-gun, and Diagnostics already has the broader "Reinstall all" path).
- Repair returns a structured result to the UI: `{status: ok|failed, what_was_replaced: [...], next_step_for_user: string}`.
- Repair logs to the diagnostics trace with a `pack_repair_*` event family so freeze reports / WP-0221 tooling pick it up.

Out of scope:
- The lockfile itself (WP-0232 prerequisite).
- The atomic install (WP-0234 prerequisite).
- The progress UI (WP-0230 extended scope).
- "Repair all": deliberately not added.

## Acceptance Criteria

- "Repair" button visible on both the Diagnostics voice-pack section and the first-run flow (WP-0235), per-pack.
- Clicking Repair on a working pack still works (re-runs install from lockfile, no observable change at the end).
- Clicking Repair on a broken pack (simulated: corrupt one file in the venv) brings it back to a working state.
- Repair does NOT touch other installed packs — verified by running Repair on the Kokoro pack and confirming Spleeter still imports.
- Repair surfaces a clear final message to the user ("Voice cloning is repaired and ready" / "Repair failed: <plain language reason>, try <next step>").
- Snapshot + dump captured under `governance/snapshots/WP-0236/`.
- Proof bundle under `product/desktop/build_target/tool_artifacts/wp_runs/WP-0236/...`.

## Research Basis

### Sources checked
- `product/engine/src/tools.rs:2247-2324` and similar `install_*_pack` functions — the existing entrypoints. Repair is "remove + reinstall", so the primitive is mostly composition.
- WP-0217 (DONE) — confirms the install job state can disagree with the on-disk pack status. Repair needs to reset both.
- WP-0229 (BACKLOG) — adds a short-circuit so install doesn't redo work. Repair must NOT short-circuit; it must reinstall even if status reports "installed".

### Selected approach
- Repair is just `force_reinstall_pack(pack)` followed by `install_pack(pack)`. With WP-0234 in place, `force_reinstall_pack` is "delete the staging area + delete the pack's site-packages subset"; with WP-0232 in place, `install_pack` is "consume the lockfile".

### Rejected options
- "Repair = restart the install job currently running". Rejected: Repair is for the "not currently installing" state. The active-job restart is a separate operation handled by Diagnostics cancel-and-retry.
- "Repair = delete the entire venv". Rejected: punishes the user for one bad pack; nukes packs that are fine.

### Risks and mitigations
- Risk: user mashes Repair while an install is already in progress. Mitigation: button disabled while any install job is `running`/`queued` for the pack; tooltip explains why.
- Risk: Repair leaves orphaned files if the cleanup step fails halfway. Mitigation: cleanup uses the same staging-rename pattern from WP-0234, so partial failures are recoverable on next launch.

### Validation plan
- Manual: corrupt a Kokoro file, click Repair, confirm working.
- Manual: click Repair on a healthy pack, confirm idempotent and no side-effects on neighbors.
- Headless: `agent_state` + `agent/dump` before and after to confirm UI state transitions are visible to the bridge.

## Red-Team

- Failure: Repair on Kokoro inadvertently breaks OpenVoice (shared deps in the shared venv path). Control: WP-0234's per-pack venv split for the heavy packs eliminates this; the WP depends on WP-0234.
- Failure: user clicks Repair instead of Install on a fresh state; Repair tries to "fix" nothing and fails confusingly. Control: Repair is only shown for packs whose status is `installed` or `failed`; the fresh-state row shows "Install" only.

## Notes

- 2026-05-18: WP created as Tier-2 user-friendliness hardening. Depends on WP-0232 + WP-0234 for the underlying primitives.
