# Work Packet: WP-0229 - Phase2 install short-circuit when packs already present

## Status

IN_PROGRESS

## Base Scope

- Make each `install_*_pack` function in `product/engine/src/tools.rs` short-circuit when its corresponding `*_pack_status(paths).installed` check already reports `true`, so re-clicking "Install Voice cloning packages" after a successful prior install does not re-run pip's resolver / re-load Kokoro warmup / re-validate diarization runtime when no work is actually needed.
- Keep an explicit force-reinstall path for operator-triggered repair when a pack is broken or a pinned dependency version changes.
- Out of scope: the install function's *upgrade-when-pin-changes* behavior is not added here. Today the install functions are idempotent only at the "package presence" level, not the "version matches pin" level. That follow-up belongs in a separate WP.

## Operator Request Preserved

- Operator asked whether re-running install after success would be a no-op. Investigation showed it is **not** a no-op — every pack's install function unconditionally runs all pip commands, even when the packages are already installed. pip's "Requirement already satisfied" path still walks the dependency resolver, costing tens of seconds to minutes per pack, plus Kokoro warmup re-loads voice models from disk every time.

## Research Basis

### Sources checked

- `product/engine/src/tools.rs` `install_portable_python` (line 845): already idempotent via `.probe` marker check (line 862-868) — confirms the pattern.
- `product/engine/src/tools.rs` `install_python_toolchain` (line 937): only short-circuits the `python -m venv` call when `venv_dir.exists()`; pip work still continues unconditionally.
- `product/engine/src/tools.rs` `install_spleeter_pack` (line 1232): runs `pip install --upgrade pip setuptools wheel`, then bootstrap packages, then multi-strategy install candidates for spleeter, every time. No early return.
- `product/engine/src/tools.rs` `install_diarization_pack` (line 2100): always runs binary-repair pip install for numba/llvmlite pair, then pinned-args install, then `validate_diarization_runtime`. No early return.
- `product/engine/src/tools.rs` `install_tts_preview_pack` (line 2184): always runs pip bootstrap and install.
- `product/engine/src/tools.rs` `install_tts_neural_local_v1_pack` (line 2247): always runs pip bootstrap + compatibility upgrades + pinned install + **Kokoro warmup load** (multi-attempt, time-expensive). The warmup writes a probe marker but the install function itself does not check the probe before running.
- `product/engine/src/tools.rs` `install_tts_voice_preserving_local_v1_pack` (line 2385): always runs pip bootstrap + install + OpenVoice patch + OpenVoice model download.
- Existing per-pack status functions that already correctly report installed-vs-not (these are reused without modification):
  - `python_toolchain_status` at `tools.rs:553`
  - `portable_python_status` at `tools.rs:834`
  - `spleeter_pack_status` at `tools.rs:1203`
  - `diarization_pack_status` at `tools.rs:2005`
  - `tts_preview_pack_status` at `tools.rs:2167`
  - `tts_neural_local_v1_pack_status` at `tools.rs:2229`
  - `tts_voice_preserving_local_v1_pack_status` at `tools.rs:2337` — already does a sophisticated check (Kokoro version present + warmup probe exists + OpenVoice version present + OpenVoice models on disk + OpenVoice patch applied).
- Existing callers of `install_*_pack` that this WP must not break:
  - The Phase2 install job handler at `product/engine/src/jobs.rs:9810-9854` (the `match step_id.as_str()` block calls each `install_*_pack(paths)?`).
  - Test/example callers (must keep working unchanged):
    - `product/engine/tests/wp0027_smoke.rs` lines 142, 146, 153
    - `product/engine/examples/wp0029_smoke.rs` lines 159, 165, 179
    - `product/engine/examples/wp0131_localization_smoke.rs` lines 157, 163, 169
    - `product/engine/examples/wp0150_localization_run_smoke.rs` lines 225, 229, 233
    - `product/engine/src/bin/voxvulgi_offline_bundle_prep.rs` lines 153, 169, 177, 185, 193
- `product/engine/src/jobs.rs:323-326` defines `InstallPhase2PacksV1Params`. Currently has only `resume_localization_run: Option<LocalizationRunRequest>`. Needs an optional `force: bool` field for the repair-by-force path.
- `product/desktop/src-tauri/src/lib.rs:6708-6713` defines the Tauri command `jobs_enqueue_install_phase2_packs_v1`. Takes no parameters today — needs to accept an optional `force: bool` request body field.
- `product/desktop/src/pages/DiagnosticsPage.tsx:3237-3250` is the existing "Install Voice cloning packages" button row. Needs a sibling "Force reinstall all packs" button for the repair path.

### Selected approach

**Wrapper-function pattern**, not signature change. Rationale: avoids breaking the 15+ existing callers in tests, examples, and the offline bundle prep tool. The wrapper is the new public API for the production install path; the bare function is preserved as the "force install" path for both tests and the operator-facing repair button.

For each of the five Phase2 pack install functions, add a sibling `install_*_pack_if_needed(paths)` that:

1. Calls the corresponding `*_pack_status(paths)`.
2. If `status.installed` is `true`, returns `Ok(status)` immediately (no pip work, no validation, no warmup).
3. Otherwise calls the existing `install_*_pack(paths)` and returns its result.

Update the Phase2 install job handler to:

- When the new `params.force` flag is `false` (default), call `install_*_pack_if_needed(paths)` for every step.
- When `params.force` is `true`, call the bare `install_*_pack(paths)` for every step (current behavior — always re-run everything).

Update the Tauri command to accept an optional `force: bool` (defaulting to `false`) and propagate it into `InstallPhase2PacksV1Params`.

Add a "Force reinstall all packs" button in the Diagnostics page next to the existing "Install Voice cloning packages" button. The new button calls the existing Tauri command with `{ force: true }`.

### Rejected options

- **Signature change** (add `force: bool` to every `install_*_pack`): forces updates to all 15+ callers in tests / examples / offline_bundle_prep. Higher blast radius, longer review, more chances for a missed call site. The wrapper pattern delivers the same operator-facing behavior with one new caller-relevant function per pack.
- **Environment-variable force toggle**: hidden behavior, hard to discover, leaks state across processes. Rejected.
- **Per-pack auto-version-check (install only when pin changed)**: more correct long-term but requires comparing installed version against pin manifest version for each pack. Different logic per pack (some packs have multiple pip packages, some have model files in addition). Out of scope for this WP — file a follow-up if needed after WP-0229 lands.
- **Make `install_portable_python` and `install_python_toolchain` also wrapped**: `install_portable_python` already has the `.probe` early-return at line 862-868. `install_python_toolchain` is cheap when the venv exists (its pip work is mostly setuptools+wheel upgrades). Leaving both as-is keeps the change scope tight; both can be wrapped later if their re-run cost becomes a complaint.

## High-ROI Additions

- Re-using the existing `*_pack_status` functions means zero new validation logic. The status functions already encode "what does installed actually mean" for each pack (e.g., `tts_voice_preserving_local_v1_pack_status` checks Kokoro version + warmup probe + OpenVoice version + OpenVoice converter files + OpenVoice patch — all five conditions). The wrapper inherits whatever the status function reports.
- The new force-reinstall button in Diagnostics gives operators a deliberate repair path that did not previously exist — today the only way to force a redo is to delete `latest.json` and the venv contents by hand.
- The WP-0227 resume logic in the Phase2 install job handler at `jobs.rs:9744-9803` is unaffected: it still preserves `done` steps from prior `latest.json` and skips them in the loop. The short-circuit added here is a second-layer safety net that catches the case where a step was attempted but the status check shows the pack is actually fully installed (e.g., from an offline-payload prehydration the install job didn't track in `latest.json`).

## Reused Systems

- Five existing `*_pack_status` functions in `tools.rs` — unmodified.
- `Phase2InstallState` / `Phase2InstallStep` structs in `jobs.rs` around line 9702 — unmodified.
- WP-0227 resume logic at `jobs.rs:9744-9803` — unmodified; complementary.
- Existing Tauri command registration in `lib.rs:7457` `tauri::generate_handler![...]` — unmodified entry needed (the command name `jobs_enqueue_install_phase2_packs_v1` stays the same).

## Gaps Closed

- Operator can click "Install Voice cloning packages" after a successful install without paying the multi-minute pip-resolver-walk + Kokoro-warmup-reload cost.
- Operator has a deliberate "Force reinstall" button when something is actually broken and they need a clean redo.

## Risks And Hardening

- Risk: a `*_pack_status` function returns `installed: true` when the pack is actually partially-broken (e.g., import works but the Kokoro warmup probe is stale), and the short-circuit returns early without repairing.
  - Remediation: each status function is the canonical "is this pack working" check used by other parts of the engine; if it's lying, that's a pre-existing bug surfaced (not introduced) by this WP. The force button gives the operator an escape hatch. Long-term: tighten the status functions if false-positive cases appear.
- Risk: short-circuit hides a real install-failure regression — operator stops noticing that their install is broken because the button says "done" instantly.
  - Remediation: the WP only short-circuits when `status.installed` is `true`. If it's `false`, the install runs as before and any failure surfaces normally. The button label stays "Install Voice cloning packages" (not "Voice cloning packages are installed"), so the operator's mental model is unchanged.
- Risk: future code adds a new install function and forgets to add the `_if_needed` wrapper.
  - Remediation: an inline comment on each wrapper points back to this WP. A grep for `install_*_pack` shows the pattern.
- Risk: the `force` parameter is wired to the Tauri command but not propagated to the job handler.
  - Remediation: the acceptance criteria below explicitly verify the propagation by inspecting both the Tauri command and the `match step_id.as_str()` block in `jobs.rs`.

## Red-Team

- Failure scenario: operator clicks "Force reinstall" while a normal install is mid-run.
  - Control: the existing `jobs::enqueue_install_phase2_packs_v1` enqueues a new job into the job table. The job runner picks up jobs in queue order; the force-job runs after the in-flight job finishes. Acceptable for now. A future WP can disable the force button while a Phase2 install is in flight if this becomes an operator-visible problem.
- Failure scenario: `InstallPhase2PacksV1Params` schema change breaks deserialization of old in-flight jobs after upgrade.
  - Control: the new `force` field uses `#[serde(default)]` so it parses as `false` for any job row written by an older app version. No migration needed.
- Failure scenario: a test that calls `install_*_pack` directly expects the side effect of "all pip commands ran" (e.g., to verify pip-install does not fail in CI).
  - Control: existing test/example callers are unchanged because the bare function still exists with current behavior. Only the Phase2 install job handler is rewired.

## Acceptance Criteria

- `cargo build --release` succeeds in `product/engine` and `product/desktop/src-tauri`.
- Existing tests pass without modification: `cargo test --manifest-path product/engine/Cargo.toml`.
- Five new wrapper functions exist in `product/engine/src/tools.rs`:
  - `install_spleeter_pack_if_needed`
  - `install_diarization_pack_if_needed`
  - `install_tts_preview_pack_if_needed`
  - `install_tts_neural_local_v1_pack_if_needed`
  - `install_tts_voice_preserving_local_v1_pack_if_needed`
  Each follows the exact pattern in the Implementation Plan below.
- `InstallPhase2PacksV1Params` in `product/engine/src/jobs.rs:323` gains a `#[serde(default)] force: bool` field.
- The Phase2 install job handler's `match step_id.as_str()` block at `jobs.rs:9810-9854` branches on `p.force`: calls the `_if_needed` wrapper when `!p.force`, calls the bare function when `p.force`.
- The Tauri command `jobs_enqueue_install_phase2_packs_v1` in `product/desktop/src-tauri/src/lib.rs:6708-6713` accepts an optional `force: bool` request body field (defaulting to `false`) and threads it into `InstallPhase2PacksV1Params`.
- The Diagnostics page (`product/desktop/src/pages/DiagnosticsPage.tsx:3237-3250`) gains a "Force reinstall all packs" button next to the existing "Install Voice cloning packages" button. The new button invokes `jobs_enqueue_install_phase2_packs_v1` with `{ force: true }`.
- After the build is installed, a single operator workflow proves the short-circuit:
  1. Get one Phase2 install to complete successfully (all steps `done` in `latest.json`).
  2. Click "Install Voice cloning packages" again.
  3. Watch the per-step `.log` files. They should remain empty for the newly-clicked job, OR contain only the `begin step=…` lines without the pip output. Total job runtime should be sub-30-seconds total instead of multi-minute.
  4. Click "Force reinstall all packs".
  5. Watch the per-step `.log` files. They should now contain the full pip install output as before (forced execution).

## Verification

- `cargo build --release` succeeds.
- `cargo test --manifest-path product/engine/Cargo.toml` passes.
- Desktop build via `governance/scripts/build_desktop_target.ps1 -WorkPackets WP-0229`.
- Post-install: operator-side acceptance test from the criteria above. Total time budget for a click-after-success short-circuit job should be measurable in seconds, not minutes.

## Implementation Plan (for a no-context model)

### Step 1 — `product/engine/src/tools.rs`

Add five wrapper functions, one for each pack. Insert each wrapper immediately AFTER its corresponding `install_*_pack` function in the file (so a reader sees them as a pair).

Pattern (substitute `xxx` for each pack name):

```rust
/// WP-0229: short-circuit when the pack is already installed. Avoids the
/// multi-minute pip resolver walk + warmup load when the operator (or a
/// resume-after-interrupt) triggers an install on a pack that is already
/// fully present. Use this from the Phase2 install job handler unless the
/// operator explicitly requested a force-reinstall.
pub fn install_xxx_pack_if_needed(paths: &AppPaths) -> Result<XxxPackStatus> {
    let status = xxx_pack_status(paths);
    if status.installed {
        return Ok(status);
    }
    install_xxx_pack(paths)
}
```

Five concrete substitutions to apply (verify each `*_pack_status` function name and `*PackStatus` return type before editing — they are listed in the Research Basis):

1. `install_spleeter_pack_if_needed` → calls `spleeter_pack_status` → returns `SpleeterPackStatus` → falls through to `install_spleeter_pack(paths)`.
2. `install_diarization_pack_if_needed` → `diarization_pack_status` → `DiarizationPackStatus` → `install_diarization_pack`.
3. `install_tts_preview_pack_if_needed` → `tts_preview_pack_status` → `TtsPreviewPackStatus` → `install_tts_preview_pack`.
4. `install_tts_neural_local_v1_pack_if_needed` → `tts_neural_local_v1_pack_status` → `TtsNeuralLocalV1PackStatus` → `install_tts_neural_local_v1_pack`.
5. `install_tts_voice_preserving_local_v1_pack_if_needed` → `tts_voice_preserving_local_v1_pack_status` → `TtsVoicePreservingLocalV1PackStatus` → `install_tts_voice_preserving_local_v1_pack`.

Do not modify the existing `install_*_pack` functions. Do not modify the `*_pack_status` functions. Do not modify `install_portable_python` (already idempotent via `.probe` marker) or `install_python_toolchain` (cheap when venv exists).

### Step 2 — `product/engine/src/jobs.rs` (`InstallPhase2PacksV1Params`)

Find `struct InstallPhase2PacksV1Params` at approximately line 323. Add a `force` field:

```rust
struct InstallPhase2PacksV1Params {
    #[serde(default)]
    resume_localization_run: Option<LocalizationRunRequest>,
    /// WP-0229: when true, every step calls the bare `install_*_pack`
    /// function (always re-runs pip). When false (default), every step
    /// calls `install_*_pack_if_needed` and short-circuits if already
    /// installed.
    #[serde(default)]
    force: bool,
}
```

The `#[serde(default)]` annotation makes the field tolerant of old job rows written before this WP — they deserialize as `force: false`.

### Step 3 — `product/engine/src/jobs.rs` (Phase2 install loop)

Find the `match step_id.as_str()` block at approximately line 9810. Each arm currently looks like:

```rust
"spleeter" => {
    append_log_line(&log_path, "install: spleeter pack");
    let _ = tools::install_spleeter_pack(paths)?;
    Ok(())
}
```

Change each pack arm to branch on `p.force` (variable `p` is the deserialized `InstallPhase2PacksV1Params` in scope):

```rust
"spleeter" => {
    append_log_line(&log_path, "install: spleeter pack");
    if p.force {
        let _ = tools::install_spleeter_pack(paths)?;
    } else {
        let _ = tools::install_spleeter_pack_if_needed(paths)?;
    }
    Ok(())
}
```

Apply the same change to the five pack arms: `spleeter`, `diarization`, `tts_preview`, `tts_neural_local_v1`, `tts_voice_preserving_local_v1`.

Do **not** change the `portable_python_win64` or `python_toolchain` arms — they continue to call the existing functions.

### Step 4 — `product/desktop/src-tauri/src/lib.rs` (Tauri command + engine API)

The Tauri command at approximately line 6708 currently is:

```rust
#[tauri::command]
fn jobs_enqueue_install_phase2_packs_v1(
    state: State<'_, AppState>,
) -> Result<jobs::JobRow, String> {
    jobs::enqueue_install_phase2_packs_v1(&state.paths).map_err(|e| e.to_string())
}
```

Change to accept an optional `force` field:

```rust
#[tauri::command]
fn jobs_enqueue_install_phase2_packs_v1(
    state: State<'_, AppState>,
    force: Option<bool>,
) -> Result<jobs::JobRow, String> {
    jobs::enqueue_install_phase2_packs_v1_with_options(&state.paths, force.unwrap_or(false))
        .map_err(|e| e.to_string())
}
```

In `product/engine/src/jobs.rs`, find `pub fn enqueue_install_phase2_packs_v1(paths: &AppPaths) -> Result<JobRow>` at approximately line 1247. Add a new sibling that accepts options:

```rust
/// WP-0229: preserved for callers that do not need the `force` knob; calls
/// the options variant with `force=false`.
pub fn enqueue_install_phase2_packs_v1(paths: &AppPaths) -> Result<JobRow> {
    enqueue_install_phase2_packs_v1_with_options(paths, false)
}

pub fn enqueue_install_phase2_packs_v1_with_options(
    paths: &AppPaths,
    force: bool,
) -> Result<JobRow> {
    let params_json = serde_json::to_string(&InstallPhase2PacksV1Params {
        force,
        ..Default::default()
    })?;
    enqueue(paths, JobType::InstallPhase2PacksV1, params_json)
}
```

`InstallPhase2PacksV1Params` needs `#[derive(Default)]` for `..Default::default()` to work. Check its derive list (around line 323) and add `Default` if missing.

### Step 5 — `product/desktop/src/pages/DiagnosticsPage.tsx`

Find the Voice-cloning-packages button row at approximately line 3237-3250. Add a "Force reinstall all packs" button next to the existing "Install Voice cloning packages" button. The new button calls the existing Tauri command with `{ force: true }`:

```tsx
<button type="button" disabled={busy} onClick={() => enqueueInstallPhase2Packs(true)}>
  Force reinstall all packs
</button>
```

Find the `enqueueInstallPhase2Packs` function in the same file. It currently invokes `jobs_enqueue_install_phase2_packs_v1` with no arguments. Change the signature to accept an optional `force` flag:

```tsx
async function enqueueInstallPhase2Packs(force = false) {
  // ...existing setup...
  await invoke("jobs_enqueue_install_phase2_packs_v1", { force });
  // ...existing follow-up...
}
```

The existing "Install Voice cloning packages" button continues to call `enqueueInstallPhase2Packs()` (no argument, defaults to `force=false`).

### Step 6 — Build & verify

- `cargo build --release` for both crates.
- `cargo test --manifest-path product/engine/Cargo.toml`.
- Desktop build via `governance/scripts/build_desktop_target.ps1 -WorkPackets WP-0229`.
- Operator-side smoke per Acceptance Criteria step 5.

## Status Updates

- 2026-05-18: Created from operator request after discovering that `install_*_pack` functions are not short-circuit-idempotent. Backlog status; ready for implementation by a no-context model using the Implementation Plan above. Should ship in a future version (likely v0.1.27 or later) alongside the operator's voice-pack work, not as an emergency.
- 2026-08-15: Revalidated against current source plus official pip 26.x install semantics and Tauri v2 command-argument documentation. Current Phase2 now includes CosyVoice 2, so the selected wrapper pattern must cover six packs. The newer WP-0227/WP-0245 prior-done gate must apply only when `force == false`; otherwise a force job would never reach the bare installers. Frontend handlers must pass explicit booleans through closures so React click events cannot become a truthy force value. Validation plan: focused Rust parameter/queue tests, frontend contract tests, serialized governed build, then the packet's installed-app short-circuit/force smoke.
- 2026-08-15: Six installed-status wrappers, force-aware queue params, force-safe resume logic, Tauri propagation, and explicit normal/force Diagnostics actions implemented. Focused frontend contracts passed 21/21, TypeScript passed, focused Rust passed 1/1, desktop `cargo check --lib` passed, and full engine suite passed 542 with 4 explicit ignores. Status remains `IN_PROGRESS` until the governed build and installed-app normal-vs-force runtime/log smoke complete.
- 2026-08-15: Governed desktop build `0.1.163` completed with the verified offline payload reused. The exact packaged executable passed hidden bridge identity/state checks and a Diagnostics semantic audit (115 candidates, 0 missing accessible names); visual inspection confirmed both distinct install controls are readable and non-overlapping. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0229/20260815-0811_v0_1_163/summary.md`. Status remains `IN_PROGRESS` only because Acceptance Criterion 5 requires installed-app normal-versus-force execution/log proof, and neither heavy mutating action was run on the loaded host.
