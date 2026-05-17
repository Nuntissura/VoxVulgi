# Work Packet: WP-0227 - Voice pack auto-install and resume

## Status

IN_PROGRESS

## Base Scope

- Voice cloning packages must install themselves on app launch when not yet fully installed, so the operator does not have to discover and click a button in Diagnostics.
- Installs that get interrupted (app shutdown mid-run) must resume on the next launch instead of restarting from scratch.

## Operator Request Preserved

- "this should not be a technical app, voice packs should be installed on opening."
- "this should not change with updates unless voicepacks gets changed or removed."
- "we already made them hydrate to make start up easier but now its just broken."
- "voice cloning / dubbing… has never worked."

## Research Basis

- Phase2 install flow audited (this session, agent report):
  - Manual trigger only: `tools::phase2_packs_install_plan` (7 steps) is enqueued by `jobs::enqueue_install_phase2_packs_v1` exclusively from a Tauri command bound to a Diagnostics page button. No auto-start at app launch.
  - Steps span Python venv creation, Spleeter, diarization, system TTS, Kokoro neural TTS, OpenVoice voice-preserving dub. Several steps download model weights (Kokoro voice files, OpenVoice converter ~1 GB) from HuggingFace — total install time 6-15 minutes on a healthy connection.
  - On shutdown, `requeue_orphaned_running_jobs()` at `product/engine/src/jobs.rs:3688-3701` marks any in-flight job as failed with `"interrupted by app shutdown"`. **No auto-resume logic — the next launch leaves the job failed and the operator must click Install again to start over from step 1.**
  - Operator's `latest.json` (from April 2026) showed five of seven steps marked `interrupted by app shutdown`, sitting unchanged for weeks. Voice cloning has never worked because every install attempt starts from scratch, takes 6-15 minutes, and gets killed before completion.
- Offline payload (WP-0054) does extract Phase1 tools + Python wheels into app data on first startup. Whether it pre-populates the Kokoro/OpenVoice model weights is unverified; even if it does, the Phase2 install job itself is still needed to wire pip-installed packages into the venv and trigger the warmup loads.

### Selected approach

Two structural changes — both engine-side, no UI churn beyond a single trace row:

1. **Resume on retry** (in the install job handler, `product/engine/src/jobs.rs` around line 9716): before constructing fresh per-step state, parse the prior `latest.json` and carry forward any step whose previous status was `"done"`. New state marks those steps `"done"` with their old `delta_bytes`/`finished_at_ms`. Loop body adds a `if status == "done" { continue; }` guard so they are not re-run. Seeds the `completed_steps` progress counter to the resumed count.
2. **Auto-enqueue on startup** (`product/desktop/src-tauri/src/lib.rs` startup phase): spawn a 5-second-delayed background thread that calls a new `jobs::should_auto_install_phase2(paths)` predicate. The predicate returns `true` if (a) no Phase2 install is currently queued/running and (b) `latest.json` either doesn't exist or has any non-`done`/`skipped` step. If true, the thread calls `jobs::enqueue_install_phase2_packs_v1`. Suppressed when Safe Mode is active. Emits `phase2_auto_install_enqueue` (or `_failed`) trace rows so an agent can verify behavior from the freeze report.

### Rejected options

- Hiding the Diagnostics "Install voice cloning packages" button behind an Advanced disclosure: cosmetic improvement, deferred — the auto-install fix removes the user need to click it at all. Button can stay visible as a manual repair path.
- Pre-extracting all Phase2 model weights into the offline payload: verifies whether the payload contains Kokoro/OpenVoice weights. Out of scope for v0.1.25; if v0.1.25 trace shows the auto-install completes successfully, no further payload work is needed; if it shows model-download steps take 5+ minutes every launch, we revisit the payload contents in a follow-up.
- Foreground install with blocking UI: would block app usage during a 6-15-minute install. Rejected. Install runs in background like every other job; the in-progress indicator (when added in a follow-up WP) tells the operator the install is happening.

## High-ROI Additions

- Resume logic is also valuable for Jobs queue cancellation: cancelling Phase2 install mid-run no longer means redoing everything if the operator changes their mind and starts again.
- The `should_auto_install_phase2` predicate is a reusable shape for any future "should X be installed at startup" check (e.g., reasoning over the offline payload state).
- Trace rows (`phase2_auto_install_enqueue`, `phase2_auto_install_enqueue_failed`, `phase2_auto_install_check_failed`) give an agent reading the next freeze report ground truth on whether auto-install was attempted and whether enqueue succeeded — no operator relay needed.

## Reused Systems

- Existing `Phase2InstallState` / `Phase2InstallStep` structs (nested in the install job handler at `jobs.rs:9702-9722`) — only added `serde::Deserialize` derive.
- Existing `write_state` helper, the per-step loop, the progress accounting at `jobs.rs:9892`.
- Existing `jobs::enqueue_install_phase2_packs_v1` entrypoint at `jobs.rs:1247`.
- Existing safe-mode flag in startup state (`safe_mode_enabled`).
- Existing `append_diagnostics_trace_row_best_effort` for the auto-install trace rows.

## Gaps Closed

- Voice packs install themselves at app launch when not already done — no operator discovery required.
- Interrupted installs no longer restart from scratch — previously-done steps are preserved across launches.
- The freeze trace now records auto-install activity so an agent can verify v0.1.25 behavior remotely.

## Risks And Hardening

- Risk: auto-install loops indefinitely if every attempt fails on the same step (e.g., persistent network failure on Kokoro download).
  - Remediation: each step that fails marks the state as `failed` and stops the job. The next auto-install attempt resumes with the failed step still pending — same outcome. Operator can cancel the install from the Jobs page to break the loop. Long-term: add per-step retry/backoff in a follow-up WP.
- Risk: auto-install fires while the operator is mid-task and the install eats CPU/network/disk.
  - Remediation: install runs as a regular job behind the existing queue, capped by `max_concurrency`. Operator can pause the queue from Jobs page. Background nature means no UI is hijacked.
- Risk: `latest.json` schema drift between versions causes deserialization to fail silently.
  - Remediation: the resume reader is defensive — any deserialize error falls back to "no prior state", so the install runs from scratch. Same behavior as before this WP. No worse than baseline.
- Risk: a step's install function is not actually idempotent and re-running it (in a non-resumed install) corrupts state.
  - Remediation: each install function is supposed to be idempotent per the existing design. WP-0227 only changes the resume path — it does NOT change install function behavior. If an install function has a hidden non-idempotency, that's a pre-existing bug surfaced (not introduced) by this WP.

## Red-Team

- Failure scenario: a future schema change to `Phase2InstallStep` breaks `Deserialize` on the field rename.
  - Control: derive uses default error handling; failure falls back to "no prior state". An operator-visible regression is "install starts from scratch again" — annoying but not corrupting. Add a `#[serde(default)]` to new fields in future schema changes to keep deserialize tolerant.
- Failure scenario: Safe Mode is enabled at startup, so auto-install is skipped, and the operator exits Safe Mode mid-session expecting voice packs to start installing.
  - Control: the auto-install check runs once at app boot. Operator exits Safe Mode via Options/Safe Mode toggle but Phase2 install won't auto-fire. This is acceptable for v0.1.25 — exiting Safe Mode already requires a restart per the existing "exit-rehydrate notice" (WP-0212), and on restart the auto-install fires. Document this in a follow-up if it becomes a real complaint.
- Failure scenario: app launches before the offline payload extraction completes; auto-install fires against an incomplete environment.
  - Control: the 5-second start delay covers most setups; offline bundle extraction at `lib.rs:7385-7415` runs in a separate startup phase that finishes well before the install would start. If it doesn't, the install will fail on the first step that needs an extracted tool, and the resume logic will pick it up on next launch.

## Acceptance Criteria

- `cargo build --release` succeeds in `product/engine` and `product/desktop/src-tauri`.
- A fresh install of v0.1.25 on a workstation with **no prior `latest.json`** auto-enqueues a Phase2 install within 10 seconds of app launch (verifiable in `diagnostics_trace.jsonl` by a `phase2_auto_install_enqueue` row).
- A v0.1.25 launch on a workstation with `latest.json` showing one or more `done` steps and one or more pending steps auto-enqueues a Phase2 install that **does not re-run the `done` steps** (verifiable by inspecting the new `latest.json` after install completes, or by the per-step `.log` files showing no `begin` entry for resumed steps).
- A v0.1.25 launch in Safe Mode does **not** auto-enqueue (operator must opt in via the existing Diagnostics button).

## Verification

- `cargo test --manifest-path product/engine/Cargo.toml`.
- Desktop build via `governance/scripts/build_desktop_target.ps1`.
- Post-install verification by the operator:
  - On first launch of v0.1.25, leave the app open and watch the Diagnostics page Phase2 table — install should progress through steps without operator action.
  - After install completes, the table should show all supported steps `done` (the system pyttsx3 step may still show `skipped` per the existing plan; Spleeter and Kokoro and OpenVoice should be `done`).
  - Open a Localization run and confirm voice cloning is actually selectable / runnable. If it works end-to-end, WP-0227 is closed. If not, the failure log (per-step `.log` file under `%APPDATA%\com.voxvulgi.voxvulgi\logs\install\phase2\<job_id>\`) is the next investigation source.

## Status Updates

- 2026-05-17: Created from operator request "voice packs should be installed on opening." Phase2 install audit confirmed the chain — no auto-enqueue + no resume after interruption + 6-15-minute install time. WP-0227 adds both auto-enqueue and resume in the engine layer. Hidden-Advanced-disclosure UX and offline-payload model-weights audit are out of scope for v0.1.25; will be revisited only if v0.1.25 trace shows the auto-install is not landing as intended. Ships in v0.1.25 alongside WP-0226's read-only UI sweep.
