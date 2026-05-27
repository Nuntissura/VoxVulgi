# Work Packet: WP-0230 - Phase2 install progress bar and status truthfulness

## Status

IN_PROGRESS

## Owner

Claude

## Base Scope

- Replace the single `"updating..." / "interrupted" / "idle"` badge in the Diagnostics "Voice cloning packages" section with a real progress bar that shows:
  - Overall progress as N-of-M packs installed (out of supported, non-skipped).
  - Current step name and its individual status (queued/running/done/failed/interrupted).
  - Time the current step has been running (so an operator can tell the difference between "this is still working" and "this is hung").
- Make the live status text truthful: when a Phase2 install job is actively running, the headline must say "Installing voice packs — step X of Y" and NOT "interrupted".
- Update the per-pack table to show a per-row spinner / hourglass on the `running` row so the operator can see where in the chain the work currently is.
- Out of scope: per-pack sub-progress (no pip parsing). Out of scope: progress notifications outside the Diagnostics page (e.g., header banner — could be a follow-up).

## Scope Decision (2026-05-18 23:50)

The pre-extension WP scope (frontend truthfulness — progress bar + 5 honest headline states + per-pack icons + elapsed time on running row) is implementable in one session against today's `tools_phase2_packs_install_latest_state` data without any backend changes.

The extension below (live bytes/sec/ETA from pip stdout + HF tqdm) requires switching `run_python_checked` from `.output()` (post-completion capture) to `.spawn()` + a reader thread + a new event channel + new state fields on each step. That is its own WP-sized effort and is carved out as **WP-0230b** rather than half-implemented here.

This WP delivers the truthfulness half today; WP-0230b delivers the byte-level streaming half later.

## Scope Extension (2026-05-18)

Operator follow-up 2026-05-18: "we are sure the downloading happens? this is the selling feature. and the app must stay non technical and user freindly". The original WP-0230 scope (top-level progress bar + truthful badge) is necessary but insufficient — for non-technical users to trust the install, the progress must show *real bytes / sec / ETA* during the long Python-wheel-and-model-download steps. Promote the previously-out-of-scope item:

- In scope (added): parse pip's stdout for download lines (`Downloading <name> (<size>): <bytes>/<bytes> [<percent>%]`) and emit progress events to the frontend so the per-pack row shows bytes downloaded + speed + ETA during pip install.
- In scope (added): hook Hugging Face `hf_hub_download` via the `tqdm` callback (kwarg supported as of `huggingface_hub>=0.20`) to surface model-weight download progress with the same bytes/speed/ETA shape.
- In scope (added): the same progress feed is consumed by the WP-0235 first-run setup flow, not only Diagnostics — so the user-friendly entry point benefits from the truthful progress without duplicate UI.
- Still out of scope: header-banner-during-install, notifications-tray progress.

This extension is what makes the install path feel honest to a non-technical user. Without it, even after WP-0237 (bundled wheels) the model-weight download would still look like the old "stuck" state.

## Notes

- 2026-05-18: Frontend truthfulness slice landed (progress bar + 5-state headline + per-row elapsed counter on running step). TypeScript + vite build clean, 13 contract tests pass, 175 engine tests unchanged. Live bytes/sec/ETA streaming carved out as WP-0230b. Proof bundle: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0230/20260518_235218/summary.md`. WP stays IN_PROGRESS pending operator-relayed visual verification on a real install run.

## Operator Request Preserved

- "i want a progress bar for all voicepacks when installing i am just watching the same 'Live progress: interrupted' badge, not really comforting"

The complaint has two halves: (a) no progress bar exists, (b) the badge keeps saying "interrupted" which is misleading when an install actually is running, and ugly when one isn't.

## Research Basis

### Sources checked

- `product/desktop/src/pages/DiagnosticsPage.tsx:3231-3310` is the existing "Voice cloning packages (one-click)" section:
  - Line 3237-3250: button row ("Install Voice cloning packages", "Refresh", "Reveal latest state").
  - Line 3255: state file path display.
  - Line 3260: the existing "Live progress" indicator — currently a `<span>` showing one of `"updating..."`, `"interrupted"`, `"idle"`.
  - Line 3264-3310: per-pack table (Pack, Status, Started, Finished, Δ disk, Error, Actions).
- `product/desktop/src/pages/DiagnosticsPage.tsx:622-639` defines the status helpers used by the table:
  - `phase2StepStatus(step)`, `phase2StepIsActive(step)`, `phase2StepIsComplete(step)`, `phase2StepIsProblem(step)` — they classify a step into running/done/failed/interrupted/skipped/stale.
- The state shape comes from `product/desktop/src-tauri/src/lib.rs:1044` `normalize_phase2_latest_state()` which returns `(state, active_bool, stale_bool, job_status_string)` and reconciles `latest.json` against the job DB. The frontend binds `phase2Latest`, `phase2HasActive`, `phase2HasProblem`, `phase2Steps` from that response.
- The state has a `started_at_ms` per step (engine `Phase2InstallStep` at `jobs.rs:9702-9713`), so a "time in current step" calculation only needs `now - step.started_at_ms` for the step that is currently `running`.
- The poll cadence for `tools_phase2_packs_install_latest_state` is whatever the existing `refresh()` schedule is in `DiagnosticsPage.tsx`. The current "Refresh" is manual; an install in progress is not auto-refreshed. Need an effect that polls every 2 s while a job is active, stops when it finishes.
- The misleading "interrupted" badge is the v0.1.25/v0.1.26 sticky state: `latest.json` records `spleeter` as `running` from a prior killed job; WP-0217 normalization sees the job is failed and the badge code shows "interrupted". Even after WP-0230 lands, the badge will still say "interrupted" between sessions until the operator clicks Install or Force-reinstall — but at least it won't say "interrupted" *during* an active install.
- No existing progress-bar primitive in the codebase. Need to inline a simple one with `<div>`-and-CSS or use a `<progress>` element.

### Selected approach

Replace the existing single-line "Live progress: X" indicator with a structured status block. Show:

1. **Headline** — one of (in priority order):
   - `Installing voice packs — step N of M (<currentStepTitle>)` when `phase2HasActive` is true and a step is `running`.
   - `Voice packs queued — waiting for runner` when `phase2HasActive` is true but no step is yet `running`.
   - `Voice packs installation interrupted — N of M complete` when `phase2HasProblem` is true and no job is active. Replaces the current bare "interrupted".
   - `Voice packs installed — all M of M complete` when every supported step is `done`.
   - `Voice packs not yet installed` when no state file or no progress has ever been made.
2. **Progress bar** — a horizontal bar showing `doneCount / supportedCount` as a percentage. Render as `<progress value={doneCount} max={supportedCount}>` (semantically correct, styleable, no new dependency).
3. **Current step strip** (only when `phase2HasActive` is true and a step is `running`) — small box below the bar:
   - Step title + step id.
   - "Running for: 2 min 14 s" — `Date.now() - step.started_at_ms`, re-rendered every 1 s.
   - Hint when running time exceeds a step-specific soft threshold (e.g., 8 min for Kokoro): "This step downloads model weights from HuggingFace; first-run can take several minutes."
4. **Per-pack table** — keep the existing structure but add a leading icon column with one of: ✓ (done), ⟳ (running, animated CSS spin), ⏸ (queued), ⚠ (failed/interrupted/stale), — (skipped).

Auto-poll cadence:

- `useEffect` keyed on `phase2HasActive`. When `phase2HasActive` becomes `true`, start a `setInterval` that re-fetches `tools_phase2_packs_install_latest_state` every 2 s. When `phase2HasActive` flips to `false`, clear the interval.
- "Running for" updates locally every 1 s using a separate effect (no Tauri call — just re-render from the cached `step.started_at_ms`).

No engine or Rust changes are required for WP-0230 — all the data the UI needs is already in the response from `tools_phase2_packs_install_latest_state`. This WP is pure frontend.

### Rejected options

- **Real per-pack sub-progress (parse pip output)**: pip stdout/stderr varies by package and pip version; fragile to parse; not worth the maintenance. The step-level progress + elapsed time is enough to tell "this is working" from "this is hung".
- **Push updates via a Tauri event from the engine job runner**: cleaner than polling, but requires new event plumbing (engine -> tauri emit -> frontend listener). Polling every 2 s is a 7-pack install's worth of cheap Tauri calls; not worth the architectural complexity for this scope.
- **Move the progress bar out of Diagnostics into a header banner / always-visible widget**: better UX but bigger scope (App shell change, persistence-on-page-switch). File as a follow-up WP if needed; WP-0230 keeps the change inside the existing Diagnostics card.

## High-ROI Additions

- The progress-bar component (`<progress value max>` + CSS) is reusable for any future long-running install (Phase3 packs, model upgrades). Establish the pattern once.
- The "current step elapsed time + soft threshold hint" pattern is also reusable for any long Tauri command (e.g., legacy archive scan, library import). Future operator-facing long tasks can use the same composition.
- Fixing the misleading "interrupted" badge removes a recurring source of operator anxiety — the entire complaint about voice packs being broken is amplified by the UI lying.

## Reused Systems

- Existing `tools_phase2_packs_install_latest_state` Tauri command — unchanged.
- Existing `Phase2InstallState` / `Phase2InstallStep` data shape from the engine — unchanged.
- Existing `phase2StepStatus` / `phase2StepIsActive` / `phase2StepIsComplete` / `phase2StepIsProblem` helpers at `DiagnosticsPage.tsx:622-639` — unchanged.
- Existing `phase2Latest`, `phase2HasActive`, `phase2HasProblem`, `phase2Steps`, `refresh()` state in `DiagnosticsPage` — extended with new derived values.

## Gaps Closed

- Operator has visible progress while voice packs install instead of staring at an unchanging badge.
- The UI stops claiming "interrupted" during an active install.
- Per-pack table communicates *where* in the pipeline the work is (the running pack gets a spinner row).

## Risks And Hardening

- Risk: 2 s polling adds noise to the diagnostics_trace.jsonl trace because each poll invokes a Tauri command that gets logged through `InvokeTimer` if instrumented.
  - Remediation: `tools_phase2_packs_install_latest_state` is not in the eight commands currently instrumented per WP-0226. Polling is invisible to the trace. If we later instrument that command, switch the poll cadence to 5 s and emit only `command_slow` rows above 500 ms.
- Risk: `step.started_at_ms` is a server-side timestamp; clock skew between the operator's PC and the value (always zero on the same machine) could produce a non-monotonic "running for".
  - Remediation: not an issue — both timestamps come from `Date.now()` / `now_ms()` on the same machine. Clamp negative differences to zero for safety.
- Risk: the per-second "running for" re-render wastes CPU.
  - Remediation: only mount the re-render effect when `phase2HasActive` is true and a step is `running`. Otherwise stay static.
- Risk: the soft-threshold hint becomes wrong when offline payload hydration is added later (download skipped, install fast).
  - Remediation: hints are advisory text, not assertions. Worst case they over-promise patience; harmless. Easy to remove later.

## Red-Team

- Failure scenario: install completes between two poll ticks; the UI shows "5 of 7" for 2 s after it's actually 7 of 7.
  - Control: acceptable — at the next tick the bar fills to 100% and the headline flips to "all M of M complete".
- Failure scenario: `phase2HasActive` flickers true/false on every poll because the normalization logic at `lib.rs:1044` sometimes returns `active=true` and sometimes `false` for a job that's between steps.
  - Control: not observed in v0.1.25 traces (`active` stayed consistent across polls during the spleeter step), but if it appears, debounce on the frontend: keep the bar in "active" state for 5 s after the last `active=true` response before flipping to inactive.
- Failure scenario: the operator never opens Diagnostics, so they never see the progress bar.
  - Control: this WP scopes UI to the existing Diagnostics surface per the operator's literal ask ("a progress bar"). A future WP can move the bar to a more discoverable location.

## Acceptance Criteria

- `npm run build` in `product/desktop` succeeds.
- When no Phase2 install has ever been run, the section headline says "Voice packs not yet installed" and the progress bar shows 0 of M.
- When the operator clicks "Install Voice cloning packages", within 2 s the headline flips to "Installing voice packs — step 1 of M (Portable Python …)" (or the first non-skipped step). The progress bar reflects the per-step `done` count and updates as steps complete.
- The "Running for: X" text under the bar updates every second while a step is `running`.
- When the install completes, within 2 s the headline flips to "Voice packs installed — all M of M complete" and the bar shows 100%.
- When the install is interrupted (operator force-quits or app crashes), the next launch of the section shows the headline "Voice packs installation interrupted — N of M complete" with N reflecting actual `done` steps. The badge no longer says only "interrupted" with no context.
- The per-pack table shows a leading status icon column with the icon set listed in Selected Approach.

## Verification

- `npm run build` in `product/desktop` passes.
- Headless verification via the agent bridge (no operator needed):
  1. Reset state by deleting `%APPDATA%\com.voxvulgi.voxvulgi\logs\install\phase2\latest.json`.
  2. Launch app, navigate to Diagnostics via `POST /agent/navigate {"page":"diagnostics"}`.
  3. Capture snapshot via `POST /agent/snapshot {"subfolder":"WP-0230","label":"no_install_yet"}`. Expect: bar at 0%, headline "not yet installed".
  4. Trigger an install via Tauri command `jobs_enqueue_install_phase2_packs_v1`.
  5. Snapshot at `t=5s` (active), `t=30s` (mid-step), and at completion. Compare bar progression and headline transitions.
- Manual smoke by operator: install once, force-quit mid-run, relaunch, verify the headline shows the interrupted state with the actual count.

## Implementation Plan (for a no-context model)

All changes are inside `product/desktop/src/pages/DiagnosticsPage.tsx`. No engine or src-tauri changes.

### Step 1 — Derived values

In the `DiagnosticsPage` component body, near where `phase2Latest`, `phase2HasActive`, `phase2HasProblem`, `phase2Steps` are derived from the latest-state response, add:

```tsx
const phase2Supported = phase2Steps.filter((s) => phase2StepStatus(s) !== "skipped");
const phase2DoneCount = phase2Supported.filter((s) => phase2StepIsComplete(s)).length;
const phase2SupportedCount = phase2Supported.length;
const phase2RunningStep = phase2Steps.find((s) => phase2StepStatus(s) === "running") ?? null;
const phase2QueuedCount = phase2Supported.filter((s) => phase2StepStatus(s) === "queued").length;
const phase2HasAnyState = phase2Steps.length > 0;
```

### Step 2 — Headline text

Add a `phase2Headline` derivation that resolves to one of the five strings in priority order (see Selected Approach). Use a `useMemo` keyed on the inputs above so it doesn't recompute on every render.

```tsx
const phase2Headline = useMemo(() => {
  if (phase2HasActive && phase2RunningStep) {
    const stepIndex = phase2Supported.findIndex((s) => s.id === phase2RunningStep.id) + 1;
    return `Installing voice packs — step ${stepIndex} of ${phase2SupportedCount} (${phase2RunningStep.title})`;
  }
  if (phase2HasActive) {
    return "Voice packs queued — waiting for runner";
  }
  if (phase2HasProblem) {
    return `Voice packs installation interrupted — ${phase2DoneCount} of ${phase2SupportedCount} complete`;
  }
  if (phase2HasAnyState && phase2DoneCount === phase2SupportedCount && phase2SupportedCount > 0) {
    return `Voice packs installed — all ${phase2SupportedCount} of ${phase2SupportedCount} complete`;
  }
  return "Voice packs not yet installed";
}, [phase2HasActive, phase2RunningStep, phase2HasProblem, phase2DoneCount, phase2SupportedCount, phase2HasAnyState, phase2Supported]);
```

### Step 3 — "Running for" timer

Add a state `[runningForMs, setRunningForMs] = useState(0)` and a `useEffect` that, when `phase2RunningStep?.started_at_ms` is present, starts a `setInterval(1000)` that updates `runningForMs = Math.max(0, Date.now() - started_at_ms)`. Clear on dependency change or unmount.

Format helper:

```tsx
function formatRunningFor(ms: number): string {
  const totalSec = Math.floor(ms / 1000);
  const mins = Math.floor(totalSec / 60);
  const secs = totalSec % 60;
  if (mins >= 1) return `${mins} min ${secs.toString().padStart(2, "0")} s`;
  return `${secs} s`;
}
```

### Step 4 — Polling while active

Add a `useEffect` keyed on `phase2HasActive`. When true, start `setInterval(2000)` that calls the existing `refresh()` (or the narrower `refreshPhase2()` if it exists). When false, clear the interval. This auto-refreshes the install state without operator interaction.

```tsx
useEffect(() => {
  if (!phase2HasActive) return;
  const id = window.setInterval(() => { void refresh(); }, 2000);
  return () => window.clearInterval(id);
}, [phase2HasActive, refresh]);
```

If `refresh()` does heavy work beyond just re-fetching the latest state, extract a narrower `refreshPhase2()` that only invokes `tools_phase2_packs_install_latest_state` and updates the relevant state slice.

### Step 5 — Render the new status block

Replace the existing "Live progress" line (around DiagnosticsPage.tsx:3260) with:

```tsx
<div className="phase2-status" style={{ marginTop: 12, marginBottom: 12 }}>
  <div style={{ fontWeight: 600, marginBottom: 6 }}>{phase2Headline}</div>
  <progress
    value={phase2DoneCount}
    max={Math.max(1, phase2SupportedCount)}
    style={{ width: "100%", height: 16 }}
  />
  <div style={{ fontSize: 12, color: "#6b7280", marginTop: 4 }}>
    {phase2DoneCount} of {phase2SupportedCount} packs installed
    {phase2QueuedCount > 0 ? ` · ${phase2QueuedCount} queued` : ""}
  </div>
  {phase2RunningStep ? (
    <div style={{
      marginTop: 8,
      padding: 8,
      border: "1px solid #d1d5db",
      borderRadius: 4,
      background: "#f9fafb",
    }}>
      <div style={{ fontWeight: 600 }}>{phase2RunningStep.title}</div>
      <div style={{ fontSize: 12, color: "#4b5563" }}>
        Running for: {formatRunningFor(runningForMs)}
      </div>
      {runningForMs > 8 * 60 * 1000 && phase2RunningStep.id === "tts_neural_local_v1" ? (
        <div style={{ fontSize: 12, color: "#6b7280", marginTop: 4 }}>
          This step downloads Kokoro voice weights from HuggingFace on first run; allow several more minutes.
        </div>
      ) : null}
      {runningForMs > 8 * 60 * 1000 && phase2RunningStep.id === "tts_voice_preserving_local_v1" ? (
        <div style={{ fontSize: 12, color: "#6b7280", marginTop: 4 }}>
          This step downloads ~1 GB of OpenVoice converter weights from HuggingFace on first run; allow several more minutes.
        </div>
      ) : null}
    </div>
  ) : null}
</div>
```

### Step 6 — Per-pack table status icon column

In the per-pack table at `DiagnosticsPage.tsx:3264-3310`, add a leading `<th>` column "" (icon column, no header text). For each row, render an icon based on `phase2StepStatus(step)`:

```tsx
function phase2StepIcon(step: Phase2InstallStep): string {
  const status = phase2StepStatus(step);
  switch (status) {
    case "done":      return "✓";
    case "running":   return "⟳";
    case "queued":    return "⏸";
    case "skipped":   return "—";
    case "failed":
    case "interrupted":
    case "stale":     return "⚠";
    default:          return "·";
  }
}
```

Add a small CSS animation for the running icon (optional, can also be left static):

```tsx
const runningIconStyle: React.CSSProperties = {
  display: "inline-block",
  animation: "phase2-spin 2s linear infinite",
};
```

…and a `@keyframes phase2-spin { to { transform: rotate(360deg); } }` rule in `App.css`. Or skip animation entirely and leave the static character — the spinner is nice-to-have, not required.

### Step 7 — Build & verify

- `npm run build` in `product/desktop`.
- Visual verification via the agent bridge per Verification section.
- Manual operator smoke per Acceptance Criteria.

## Status Updates

- 2026-05-18: Created from operator complaint about the misleading "interrupted" badge and the absence of any progress visualization. Backlog; ready for pickup by a no-context model using the Implementation Plan above. Pure frontend change — should be a quick build with no Rust recompile.
