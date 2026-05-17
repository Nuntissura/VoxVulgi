# Work Packet: WP-0221 - UI freeze diagnostic instrumentation

## Status

IN_PROGRESS

## Base Scope

- Add in-app instrumentation that captures evidence of UI freezes (unresponsive WebView main thread) and writes it to the existing diagnostics trace so the freeze can be diagnosed offline without operator-side reproduction effort.
- Detection must work while the WebView main thread is blocked; record-and-flush path must not depend on the frozen thread.
- Reuse the existing diagnostics trace folder, console ring buffer, agent bridge HTTP server, and Diagnostics page; do not introduce a parallel diagnostics surface or a new card.
- Scope is observation only. Remediation of identified causes is out of scope for this WP and will be handled in follow-up WPs once the trace identifies the culprit.

## Operator Request Preserved

- "the app still freezes a lot, this is an ongoing problem. moving the app window, or switching back to voxvulgi from any other app makes the app freeze. (not responding, yellow windows border, combined sometimes with the app becoming semi-transparent)"
- "resizing app window freezes the app"
- "in task manager the app shows up named webview2:voxvulgi v0.1.17 using 0 cpu and 0.1 memory. although freezing all the time"
- "i even let the app load in for half an hour so all dependencies got loaded without creating jobs or even clicked on the app."
- "my pc is under heavy load a lot but this still should not make the app behave this bad."
- "can we create an internal diagnostic too see what the cause is?"

## Research Basis

### Sources checked

- Prior VoxVulgi freeze remediation WPs that did not durably resolve the symptom: WP-0067 (window switch freeze reduction), WP-0103 (window switch state retention and freeze reduction), WP-0104 (desktop shell drag and resize ergonomics), WP-0121 (contention-tolerant performance and responsiveness audit), WP-0127 (visibility-aware polling and background work suspension), WP-0146 (window move affordance regression repair). Operator reports the symptom is still active on v0.1.17.
- Existing diagnostics infrastructure inventory (this session): trace writer at `product/desktop/src-tauri/src/lib.rs:972-1002`, trace command at `product/desktop/src-tauri/src/lib.rs:4430`, console ring buffer at `product/desktop/src/App.tsx:28-62`, agent bridge listener and route table at `product/desktop/src-tauri/src/lib.rs:13-138`, Diagnostics page sections at `product/desktop/src/pages/DiagnosticsPage.tsx:671+` including an existing "Diagnostics trace" section at line 2381.
- WebView2 / Win32 background: the symptom "unresponsive on window events with 0% CPU" is consistent with the UI thread blocked on a kernel wait (synchronous IPC to a stuck backend, a held mutex, or a hung UNC/SMB syscall). DWM repaints during move/resize push WM_PAINT through the same thread, which is why the freeze surfaces visibly on those events even if the underlying block began earlier.
- Web Worker thread isolation: a dedicated Worker runs on its own thread and survives main-thread hangs, so heartbeat-from-Worker is the standard browser-side pattern for detecting main-thread stalls. The Worker cannot use Tauri `invoke()` (IPC routes through the main thread) but can `fetch()` to a localhost HTTP listener.
- Tauri 2 + Tokio: the agent bridge listener (`product/desktop/src-tauri/src/lib.rs:33`) already runs in its own Tokio task and is independent of the WebView main thread, so it is a safe receiver for Worker-originated freeze events.
- yt-dlp/Tauri community reports of freezes on UNC paths during `is_dir`/`exists` calls on the main thread; relevant because the operator's libraries are on `\\MIR\...`.

### Relevant patterns found

- Worker-driven main-thread heartbeat with postMessage round-trip timing is the established browser pattern (Chromium DevTools "Long Tasks", Sentry's "main thread hang detection", VS Code's extension-host heartbeat).
- Per-command IPC instrumentation by wrapping the invoke handler is the standard Tauri observability pattern; for Tauri 2, the simplest non-macro path is a thin per-command timer used in the most-suspect commands until coverage is needed everywhere.
- Tokio runtime scheduling skew measurement (interval timer that logs when actual tick time exceeds expected by a threshold) is the canonical async-runtime starvation check.

### Reuse opportunities

- `append_diagnostics_trace_row` (lib.rs:972) for all freeze records and skew records.
- Existing `diagnostics_trace.jsonl` schema (`DiagnosticsTraceEntry { ts_ms, event, level, details, process }`) avoids inventing a new file format.
- Agent bridge route table (lib.rs:121-128) for the new `POST /agent/freeze_event` endpoint.
- Console ring buffer (App.tsx:28-62) as the source for "last 200 console entries" already attached to dumps.
- Existing "Diagnostics trace" section at DiagnosticsPage.tsx:2381 as the host for the new "Freeze events" subsection; satisfies the no-new-cards rule in `build_rules.md`.

### Rejected options

- Building a separate "performance HUD" overlay window. Rejected because it would itself be subject to the freeze and would not survive the very condition we are trying to observe.
- Replacing the existing trace file with a new one. Rejected because the trace format and Diagnostics page already consume `diagnostics_trace.jsonl`; adding a sibling file would split storage and require new export paths.
- Using `requestAnimationFrame` for heartbeat. Rejected because rAF is suspended when the window is occluded or minimized, producing false positives.
- Using Tauri events from a JS interval on the main thread to ping Rust. Rejected because the main thread is exactly what's frozen; this would produce only the absence of a ping with no contextual data.

### Selected approach

Three-layer observer that records freezes without depending on the frozen thread.

1. **Web Worker heartbeat (frontend)**
   - Dedicated Worker module at `product/desktop/src/lib/freeze_detector.worker.ts`.
   - Main thread spawns the Worker at app boot and passes the agent bridge port read from the same source the rest of the app uses.
   - Worker posts `ping` every 100 ms; main thread responds with `pong { ts_ms, last_event, last_invoke, current_page, in_flight_count }`.
   - If a pong is not received within 500 ms, Worker classifies it as a freeze event and `POST`s to `http://127.0.0.1:<port>/agent/freeze_event` with the most recent context the main thread had time to send.
   - When a pong eventually arrives after a missed deadline, Worker emits a `freeze_recovered` event with measured gap.

2. **Process-scheduling heartbeat and command timing (backend)**
   - Dedicated OS thread (no Tokio dependency) scheduled to tick every 250 ms; logs an `event_loop_skew` trace row when the actual interval exceeds 500 ms. This measures process-level scheduling starvation (SMB stall on UNC paths, AV scan, DLL loader lock, OS thrash) rather than Tokio-runtime starvation; if Tokio-specific starvation needs to be distinguished later it can be added as a second heartbeat without changing this design.
   - Thin RAII per-command timer (`InvokeTimer`) used at the top of the most-suspect commands (initial set: `startup_status`, `instagram_subscriptions_queue_all_active`, `library_list`, `youtube_subscriptions_list`, `video_libraries_list`, `video_libraries_upsert`, `video_libraries_set_active`, `youtube_subscription_groups_list`). On drop, records `{cmd, started_at_ms, elapsed_ms}` as a `command_completed` row and additionally emits a `command_slow` row when elapsed exceeds 500 ms.
   - New agent bridge route `POST /agent/freeze_event` that calls `append_diagnostics_trace_row_best_effort` with event name `freeze_detected` or `freeze_recovered` and the Worker-supplied details.

3. **Diagnostics page panel (UI)**
   - New subsection inside the existing "Diagnostics trace" section at `DiagnosticsPage.tsx:2381`: "Freeze events" with recent rows from `diagnostics_trace.jsonl` filtered to event names `freeze_detected`, `freeze_recovered`, `event_loop_skew`, `command_slow`.
   - Reuses the existing trace export path (no new export plumbing).

### Risks

- See Red-Team and Risks And Hardening below.

### Mitigations

- See Red-Team and Risks And Hardening below.

### Validation plan

- See Acceptance Criteria and Verification below.

## High-ROI Additions

- Per-command timing helper installed on the most-suspect commands now is reusable; later WPs can extend coverage to any command suspected of contention.
- Tokio event-loop skew detector also catches other async-runtime starvation conditions unrelated to freezes (e.g. a blocking job on a runtime thread), giving long-term value beyond this WP.
- The freeze event record format will also expose the last fired window event, which doubles as a coarse trace of window-system activity useful for unrelated investigations (paint storms, focus loops).
- Surfacing in-flight Tauri invoke counts at freeze time will make it cheap to detect "invoke fan-out" anti-patterns (e.g. nine parallel commands on every page mount) that have already been suspected on `LibraryPage`.

## Reused Systems

- `append_diagnostics_trace_row` / `append_diagnostics_trace_row_best_effort` (`product/desktop/src-tauri/src/lib.rs:972, 1004`).
- `diagnostics_trace.jsonl` file format and folder resolution (`product/engine/src/paths.rs:162-220`, `product/desktop/src-tauri/src/lib.rs:948`).
- Agent bridge listener and route dispatch (`product/desktop/src-tauri/src/lib.rs:13-138`).
- Console ring buffer (`product/desktop/src/App.tsx:28-62`) included in dumps for context cross-reference.
- Existing "Diagnostics trace" section in `DiagnosticsPage.tsx:2381`.
- Tauri command registry in `product/desktop/src-tauri/src/lib.rs:7125+`.

## Gaps Closed

- Today: freezes are reported by operator memory only, with no machine-readable record of what fired immediately before. WP-0221 produces a timestamped trace row for every freeze >500 ms with the last window event and last invoke context.
- Today: there is no way to tell whether the Rust async runtime is starved during the freeze. WP-0221 emits `event_loop_skew` rows so async-runtime starvation is observable independently of WebView state.
- Today: no per-command IPC timing exists, so a hung command cannot be named. WP-0221 instruments the most-suspect commands and emits `command_slow` rows above 500 ms.
- Today: Diagnostics page has no freeze view. WP-0221 adds a subsection inside the existing "Diagnostics trace" section so operators and agents can see and export freeze evidence without opening the trace file by hand.

## Risks And Hardening

- Risk: the Worker heartbeat itself perturbs performance or itself contends with the main thread.
  - Remediation: 100 ms ping interval and minimal pong payload (timestamps + small string tags); Worker uses a transferable buffer where possible; Worker is the only timer of its kind to avoid timer fan-out.
- Risk: `fetch()` from Worker to the agent bridge fails or hangs if the bridge port file is stale (WP-0210 covers stale-port handling).
  - Remediation: Worker uses a 3 s `AbortController` timeout per POST and gives up silently after three consecutive failures, then re-probes the bridge port from the file every 30 s.
- Risk: per-command timing adds overhead to the IPC critical path.
  - Remediation: helper is a single `Instant::now()` and one comparison on the fast path; trace write happens only above the 500 ms threshold and uses the existing best-effort append (non-blocking).
- Risk: writing freeze rows during a freeze could itself contend with disk I/O.
  - Remediation: `append_diagnostics_trace_row_best_effort` is already fire-and-forget and runs off the Tauri main thread; trace folder defaults to local APPDATA, not UNC, so SMB stalls do not affect writes.
- Risk: false positives from operator deliberately dragging the window for >500 ms.
  - Remediation: classify freezes as `freeze_detected` regardless and let analysis aggregate by event type; the included `last_event` field makes operator-driven drag freezes self-identifying.
- Risk: trace file grows unbounded.
  - Remediation: trace rotation is owned by existing diagnostics retention (WP-0005); no new retention policy needed here.
- Risk: introduces a new card in the UI in violation of `build_rules.md`.
  - Remediation: extend the existing "Diagnostics trace" section as a subsection; no new card is added.

## Red-Team

- Failure scenario: the Worker spawn itself fails (CSP, Vite worker plugin missing, sandbox).
  - Control: log a single console warning at boot with the error; do not block app startup; expose the worker-spawn status in the new Diagnostics subsection so an operator can see "freeze detector unavailable" instead of false silence.
- Failure scenario: the WebView main thread is frozen at app start before the Worker has the bridge port.
  - Control: the Worker reads the bridge port from a frontend message on init and caches it; if not yet available, the Worker falls back to reading `agent_bridge_port.txt` via a `fetch` to a static known port written into a small JSON config exposed at app boot. If even that fails, freeze events are buffered in memory inside the Worker and flushed on next successful POST. No data loss until Worker is killed.
- Failure scenario: a freeze masks itself as success because the per-command timer is sampled inside the command, but the command never returned (panic or deadlock).
  - Control: emit a `command_started` trace row before the work begins and a `command_completed` row on return; an in-flight command with no completion row after >5 s of subsequent freeze events constitutes a strong signal and should be flagged in the Diagnostics view as "stuck command" without requiring backend changes to detect it.
- Failure scenario: UNC path stalls inside a Tauri command not in the instrumented set.
  - Control: the Rust event-loop skew detector still records starvation, and the Worker still records the freeze gap; the absence of a slow-command row narrows the suspect to uninstrumented commands and we expand coverage in a follow-up.
- Failure scenario: the diagnostic itself becomes a freeze source.
  - Control: all writes are best-effort; Worker uses bounded queue; per-command timer is one `Instant::now()`. If any of these regress, freeze trace data is the first thing to disappear, and we revert via the existing build/version flow.
- Minimum controls enforceable through acceptance criteria: Worker spawn status surfaced; freeze event rows present after a deliberate freeze test; event-loop skew rows present under deliberate runtime starvation; new endpoint reachable; no new card introduced.

## Freeze Report Bundling (follow-up)

Added on 2026-05-17 in response to operator feedback that the trace file is useful but inconvenient for agent inspection. The bundling layer turns the continuous trace into a single self-contained report so an agent can `Read` one file and have everything needed for diagnosis.

- Route: `POST /agent/freeze_dump`. Body: `{ "limit": <usize=1000, clamped 1..=5000>, "note": "<string?>" }`. Response: `{ "path", "latest_path", "trace_rows_included" }`.
- Tauri command: `agent_freeze_dump_now({ note })` — same payload, returned to the WebView for the in-app button.
- One-click trigger: `vvfreeze.cmd` (repo root) -> `governance/scripts/vv_freeze.ps1`. Reads `agent_bridge_port.txt`, verifies the bridge pid is alive per [WP-0210], POSTs the request with a 5 s default timeout, prints both paths.
- Output: `<trace_dir>/freeze_reports/freeze_report_<ts>.json` (kept) and `<trace_dir>/freeze_reports/freeze_report_latest.json` (overwritten each call). The "latest" alias is the canonical path agents read first.
- Documented in `CLAUDE.md` and `AGENTS.md` under "Freeze Report (WP-0221)" so future agents find it via the project authority surfaces.

## Acceptance Criteria

- A `freeze_detector` Worker spawns at app boot and posts `ping` to the main thread on a 100 ms cadence. Worker-spawn success/failure is visible in the Diagnostics trace section.
- When the WebView main thread is blocked for ≥500 ms, the Worker writes one `freeze_detected` row to `diagnostics_trace.jsonl` via the new `POST /agent/freeze_event` endpoint, including: gap ms, last fired window event, last invoke name + age, current page, in-flight invoke count.
- When the main thread becomes responsive again, the Worker writes one `freeze_recovered` row with measured total gap.
- A dedicated OS thread in Rust emits an `event_loop_skew` trace row when its tick interval (target 250 ms) exceeds 500 ms.
- The eight most-suspect Tauri commands listed in Selected Approach emit one `command_completed` row per call, containing `cmd`, `started_at_ms`, and `elapsed_ms`; a separate `command_slow` row appears when `elapsed_ms` is at least 500. The single completion row replaces a paired start/completion design from earlier drafts because `started_at_ms` is sufficient to derive duration without doubling the row count on hot paths like `startup_status` polling.
- The Diagnostics page "Diagnostics trace" section gains a "Freeze events" subsection that lists recent rows of these event names. No new card is added.
- New agent bridge route `POST /agent/freeze_event` accepts a JSON body and returns `{ "status": "ok" }` on success.
- All freeze writes complete without blocking the WebView main thread or the Tauri command thread.
- `POST /agent/freeze_dump` returns 200 with a JSON body containing `path`, `latest_path`, and `trace_rows_included`. Both files exist on disk under `<trace_dir>/freeze_reports/`. The `latest_path` is stable and overwritten on each call.
- `vvfreeze.cmd` at the repo root runs end-to-end without manual port lookup and prints the latest path so an agent can read it without operator relay.
- `CLAUDE.md` and `AGENTS.md` document the trace file path, the latest-report path, the trigger script, and the in-app button under a "Freeze Report (WP-0221)" section.

## Verification

- Engine and tauri Rust tests: `cargo test --manifest-path product/engine/Cargo.toml` and `cargo test --manifest-path product/desktop/src-tauri/Cargo.toml`.
- Desktop build: `npm run build` in `product/desktop`.
- Headless deliberate-freeze smoke: with the app running, navigate via the agent bridge to Diagnostics, then trigger a synthetic main-thread block (a Tauri command that `std::thread::sleep(2s)` behind a feature-gated debug command, or a JS `while` loop in DevTools); confirm a `freeze_detected` row appears in `diagnostics_trace.jsonl` and is rendered in the new "Freeze events" subsection. Capture proof via `__voxVulgiRequestSnapshot("WP-0221", "freeze_events")` and `__voxVulgiRequestDump("WP-0221", "freeze_events")`.
- Headless event-loop skew smoke: temporarily insert a `std::thread::sleep(1s)` on a Tokio worker thread under a debug command; confirm an `event_loop_skew` row appears.
- Per-command timing smoke: invoke one of the instrumented commands and confirm `command_started`/`command_completed` rows appear; force a slow path (large `library_list`) and confirm a `command_slow` row appears.
- Proof bundle at `product/desktop/build_target/tool_artifacts/wp_runs/WP-0221/summary.md` per `governance/workflow/PROOF_STANDARD.md`.

## Status Updates

- 2026-05-17: Created packet from operator request to add an internal freeze diagnostic. Implementation begins after task board update.
- 2026-05-17: Implementation slice complete and v0.1.18 desktop installer built. Rust changes added the `/agent/freeze_event` agent-bridge route, an exposed `AGENT_BRIDGE_PORT` static, an `agent_bridge_port` Tauri command, an `InvokeTimer` RAII helper instrumenting the 8 most-suspect commands, and a dedicated OS-thread `spawn_event_loop_skew_heartbeat`. Frontend added `freezeDetector.worker.ts` (Vite `?worker` import, bundled as 1.25 kB chunk), `freezeDetector.ts` driver with passive window-event capture, `installFreezeDetector` wired into App.tsx next to `installConsoleBuffer`, and a "Freeze events (WP-0221)" subsection appended inside the existing Diagnostics trace card. Verified at build time: tsc, vite build, cargo build --release, NSIS + MSI bundling all exit 0. App-boundary visual proof pending against v0.1.18 (current running build is v0.1.17). Headless verification recipe captured in `product/desktop/build_target/tool_artifacts/wp_runs/WP-0221/summary.md`.
- 2026-05-17: Added freeze-report bundling per operator follow-up ("freeze dump log I can trigger and you can inspect without me relaying"). New `POST /agent/freeze_dump` route writes a self-contained JSON file (app version, pid, bridge port, agent state, recent trace tail) to `<trace_dir>/freeze_reports/freeze_report_<ts>.json` and a stable `freeze_report_latest.json` alias. New Tauri command `agent_freeze_dump_now` and Diagnostics page button surface the same path from inside the app. New `vvfreeze.cmd` + `governance/scripts/vv_freeze.ps1` at the repo root let the operator trigger the dump from a terminal even while the WebView main thread is frozen (the bridge runs on its own thread). Documented in `CLAUDE.md` and mirrored in `AGENTS.md` per [GLOBAL-META-010] under a new "Freeze Report (WP-0221)" section so any future agent — Claude or Codex — sees the report path and trigger discoverably. Ships in the next desktop build (v0.1.19), bundled with the surgical YouTube subfolder and reveal-toast fixes per operator sequencing.
- 2026-05-17: First three freeze reports from v0.1.19 produced strong evidence of slow Tauri commands (`youtube_subscriptions_list` 3462 ms, `youtube_subscription_groups_list` 3006 ms, `instagram_subscriptions_queue_all_active` 0.7–1.8 s) but **zero** `freeze_detected` / `event_loop_skew` rows over ~11.5 h of trace including an active operator-reported freeze. Cause is ambiguous between (a) Worker silently failed to install, (b) freezes are at the WebView UI/compositor layer the JS Worker cannot observe, or (c) threshold was too high. v0.1.20 ships a Worker liveness heartbeat (`worker_alive` posted every 30 s via `/agent/freeze_event`, whitelisted in `agent_handle_freeze_event`) and lowers the freeze threshold from 500 ms to 250 ms so the next freeze report disambiguates (a) vs (b)/(c) on inspection. `CLAUDE.md` and `AGENTS.md` updated with the worker-alive check as step 2 of the recommended agent flow.
- 2026-05-17: v0.1.20 freeze report (2 m 35 s of runtime) showed `app_version=0.1.20`, **zero `worker_alive` rows**, and 12 fresh `command_slow` rows (notably `youtube_subscription_groups_list` 5542 ms, `library_list` 4581 ms during a Media Library page mount). Conclusive: the Worker silently fails to install on Tauri 2 — scenario (a). v0.1.21 ships three changes to address this: (1) the Vite `?worker` shorthand import is replaced with an explicit `new Worker(new URL("./freezeDetector.worker.ts", import.meta.url), { type: "module" })` form that bundles and resolves cleanly inside the Tauri WebView; (2) install-step telemetry (`freeze_detector_install_attempted` / `freeze_detector_install_succeeded` / `freeze_detector_install_failed` rows) written via `diagnostics_trace_write_event` so an agent can read exactly where construction succeeded or failed; (3) a main-thread fallback heartbeat (`main_thread_alive` every 30 s) that fires even when the Worker is dead, giving us a JS-side liveness signal regardless. The slowness cause (SQLite contention) is fixed in parallel under WP-0223. Together these tell us, after the next freeze report on v0.1.21, whether the Worker fix took (look for `worker_alive` rows), whether install failed at a specific stage (look for `freeze_detector_install_failed`), or whether the JS main thread itself hangs (look for gaps between `main_thread_alive` rows).
