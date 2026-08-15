# Work Packet: WP-0250 - WebView2 occlusion/background renderer-freeze fix

## Status

DONE

## Owner

Claude

## Operator Request Preserved

- "currently vv is stuck and frozen, this happens a lot lately when the app is idle in the background. i did launch vvwatch.cmd but only after i realised the app was frozen"
- "commit and push current dirt in repo then start working"

## Problem Statement

VoxVulgi (v0.1.66, Tauri v2 + WebView2) freezes while idle in the background. The window
goes Not Responding at the OS level but the process does not crash. Existing freeze
tooling (WP-0221 in-app detector, WP-0242 sibling watch) records the *symptom* but never
caught a *cause*, because this freeze class suspends the very Worker that would report it.

## Diagnosis (evidence captured 2026-06-10)

Fresh bridge-thread freeze dump while the app was hung
(`freeze_report_latest.json`, app_version 0.1.66, pid 135748, 1500 trace rows):

- OS: `Get-Process -Id 135748` -> `Responding = False`, CPU ~403 s over ~32 h uptime, ~46 MB working set.
- Agent bridge (native Rust thread): `/agent/health` = ok, `/agent/state` = `{current_page: jobs}` — backend fully alive.
- Trace event inventory over a ~5.75 h window: only `runtime_sample` (690), `main_thread_alive` (405), `worker_alive` (405). **Zero** `freeze_detected`, `event_loop_skew`, `command_slow`, `command_completed`.
- `runtime_sample` (native background sampler) ticked continuously, **no gap > 35 s**, right up to 4.6 s before the dump. `cpu_percent` = 0.0 across all 690 samples. RSS flat ~46 MB (the 221 MB blip is the dump itself) — **no leak, no busy-loop**.
- Both JS heartbeats — `main_thread_alive` (main thread) **and** `worker_alive` (a separate Worker thread) — **stopped at the same instant, ~8,555 s (~2 h 22 m) before the dump**, and never resumed.
- Last UI event recorded before the JS stop: `blur` (window backgrounded), early in the session.

Interpretation: a JS busy-loop would block only the main thread and leave the Worker
ticking; a Tauri-command deadlock would emit `command_slow`. Neither occurred. The main
thread and the Worker — which live on **separate threads inside the WebView2/Chromium
renderer process** — died together while the native process kept running at 0% CPU. That
is the signature of the **entire renderer process being frozen from the outside** after the
window was backgrounded/occluded, not a code-level hang.

This is a **distinct freeze class** from the WP-0242 reports (which showed slow `jobs_list` /
`library_get` / `instagram_subscriptions_queue_all_active` commands = DB contention). That
is why DB/command hardening never stopped the idle-in-background freezes.

## Research Basis

- Repo evidence:
  - `product/desktop/src-tauri/tauri.conf.json` window config: `transparent: true`, `decorations: false`, `shadow: false` — a frameless+transparent window, fully occludable. AGENTS.md "Freeze Report" point 6 already flagged this config as the suspect for "Worker present but no `freeze_detected`" freezes.
  - Grep of `product/desktop/src-tauri/` for `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`, `additional_browser_args`, `--disable-backgrounding-occluded-windows`, `CalculateNativeWinOcclusion`: **none present** — no mitigation is set today.
  - Stack: Tauri `2`, `tauri-runtime 2.10.0`. Single main window declared in `tauri.conf.json` (label `main`); no Rust-side `WebviewWindowBuilder` for the main window.
- Primary sources checked (2026-06-10):
  - Chromium docs, "Windows Native Window Occlusion Tracking" (`chromium/docs/windows_native_window_occlusion_tracking.md`): when a window is occluded/minimized, Chromium treats foreground tabs as backgrounded — "rendering stops, and JavaScript is throttled". Doc explicitly notes the occlusion cost "probably outweighs the benefits for other Chromium-based applications" (i.e., embedders should consider disabling it). Flags: `--disable-features=CalculateNativeWinOcclusion`, `--disable-backgrounding-occluded-windows`.
  - Tauri v2 config reference + issue tauri-apps/tauri#7692: `additionalBrowserArgs` **replaces** wry's default `--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection`; if set, those defaults must be re-included by hand.
  - Tauri issue tauri-apps/tauri#13092: setting `additionalBrowserArgs` in `tauri.conf.json` has caused blank/white-screen on *additional* webview windows; risk noted, mitigated here because VoxVulgi uses a single main window and we verify load post-build.
- Primary-source and packaged-runtime recheck (2026-08-14):
  - Chromium's `windows_native_window_occlusion_tracking.md` locates native occlusion calculation in the browser-side Aura/Windows root-HWND tracker; `content/public/common/content_switches.cc` defines `disable-backgrounding-occluded-windows` as the switch that disables backgrounding renders for occluded windows.
  - The packaged v0.1.160 WebView2 root browser process carried the complete four-switch override. Its renderer child carried Chromium's propagated `CalculateNativeWinOcclusion` feature disable and `--disable-background-timer-throttling`, but not the browser-side `--disable-backgrounding-occluded-windows` or `--disable-renderer-backgrounding` switches. Acceptance criterion 4 was corrected to match Chromium's process boundary rather than requiring switches on a child process to which WebView2 does not propagate them.
- Rejected options:
  - Disable hardware acceleration / `transparent:false` / add decorations — larger UX regression, not the root cause.
  - Native-side periodic `window.set_focus()` keep-alive — fights the OS, racy, and steals focus (violates the no-focus-steal build rule).
  - Only adding better detection (heartbeat-gap alarm) without fixing the cause — does not stop the freeze; tracked as a follow-up, not this WP.
- Selected approach: set WebView2 additional browser args to disable native window occlusion calculation and renderer/background-tab freezing, re-including wry's defaults.

## Scope

In scope:

- Add `additionalBrowserArgs` to the main window in `product/desktop/src-tauri/tauri.conf.json` with value:
  `--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,CalculateNativeWinOcclusion --disable-backgrounding-occluded-windows --disable-renderer-backgrounding --disable-background-timer-throttling`
  (wry defaults preserved + occlusion/background-freeze flags appended).
- Add a config-contract test asserting the args string contains both the preserved wry defaults and each occlusion/background flag, so the override-caveat can never regress silently.
- Increment desktop semantic version and append a BUILD_CHANGELOG entry per build rules.
- Update `governance/spec/TECHNICAL_DESIGN.md` with the WebView2 occlusion policy.

Out of scope:

- An external/native heartbeat-gap detector that auto-flags renderer suspension (follow-up WP).
- The WP-0242 `vvwatch` "metadata.json only, no samples.jsonl" run failure (separate tooling bug; logged for a follow-up).
- DB-contention / slow-command freezes (already tracked elsewhere).

## Risks and Mitigations

- R1: `additionalBrowserArgs` blank-screen bug (#13092). Mitigation: single main window only; verify the app loads after build (snapshot + bridge `/agent/state`).
- R2: Dropping wry defaults by overriding. Mitigation: defaults re-included verbatim and asserted by the contract test.
- R3: Slightly higher idle power/CPU from disabled occlusion savings. Mitigation: acceptable for a local-first production tool; documented in TECHNICAL_DESIGN.
- R4: The flag set is necessary but the soak proof is long. Mitigation: interim proof inspects the live WebView2 browser process for the complete override and the renderer child for the arguments Chromium propagates; full proof is an idle-background soak watching `main_thread_alive`/`worker_alive` survive occlusion.

## Acceptance Criteria

1. `tauri.conf.json` main window carries the exact `additionalBrowserArgs` string above.
2. Config-contract test (RED before, GREEN after) asserts preserved defaults + every occlusion/background flag.
3. Desktop build succeeds; app loads (no blank screen) — bridge `/agent/state` responds and a snapshot renders UI.
4. The running root `msedgewebview2.exe` browser process command line includes the complete exact override, and its renderer child includes Chromium's propagated `CalculateNativeWinOcclusion` feature disable and `--disable-background-timer-throttling`. Browser-side switches are not required on the renderer child.
5. Desktop semantic version incremented; BUILD_CHANGELOG entry added with WP-0250.

## Verification

- RED: contract test fails before `additionalBrowserArgs` is added.
- GREEN: contract test passes after.
- Build: `npm run build` (desktop) + `cargo test` (engine/tauri).
- Runtime: launch, capture bridge `/agent/state` + a snapshot (load proof), and dump the root browser plus renderer-child `msedgewebview2.exe` command lines (complete-override and propagation proof).
- Soak (interim/operator-assisted, long-horizon): leave the app idle and backgrounded; confirm `main_thread_alive` + `worker_alive` keep ticking through occlusion in `diagnostics_trace.jsonl`. Documented as the canonical long-horizon proof since the freeze took ~5 h to manifest.

## Completion Evidence (2026-08-15)

- Governed desktop v0.1.160 and its NSIS installer already contain the exact WP-0250 configuration; version and changelog gates are present.
- `node --import tsx --test tests/freezeContainmentContract.test.ts`: 20 passed, 0 failed.
- Exact managed executable launched with `--agent-headless`; `/agent/state` returned v0.1.160 and a visually inspected Diagnostics snapshot rendered without the blank-screen regression.
- Root WebView2 browser PID 108404 contained the complete override; renderer PID 94248 contained Chromium's propagated feature/timer subset.
- Fresh `vvfreeze.cmd` report contained current main-thread and Worker heartbeat rows. The multi-hour operator-assisted soak remains explicitly unclaimed.
- Proof bundle: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0250/20260815_0517_v0_1_160/summary.md`.
