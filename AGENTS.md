# Repo Agent Notes

## Desktop Build Output Policy

- Follow `build_rules.md` for build verification and UI construction rules: built surfaces must be inspected visually and through backend or frontend navigation/interaction without popping up the app window or hijacking the operator keyboard/mouse, and new UI must not introduce more cards.
- For desktop release builds, use `governance/scripts/build_desktop_target.ps1` (or `npm run build:desktop:target` from `product/desktop`).
- Follow the offline payload policy in `build_rules.md`: routine builds must reuse a verified payload when inputs did not change, while explicit release/full-refresh builds must state that payload refresh can be slow and show useful progress.
- Every desktop target build must increment the desktop semantic version.
- Every desktop target build must append an entry to `governance/release/BUILD_CHANGELOG.md` with included Work Packet IDs.
- Managed desktop build-output folders and filenames we control must not use spaces; prefer `snake_case`.
- Build logs for each desktop target build must be written under:
  - `product/desktop/build_target/logs`
- Build outputs must go under:
  - `product/desktop/build_target/Current`
- Previous build outputs must be archived under:
  - `product/desktop/build_target/old_versions`

## Installer Maintenance Mode Policy

- Preserve these exact installer maintenance labels:
  - `Update`
  - `Reinstall (keep preferences and options)`
  - `Full reinstall`
  - `Uninstall (keep preferences and options)`
  - `Full uninstall`
- Keep existing-install flow clear: show the pre-maintenance explainer before maintenance selection.
- Keep app-data behavior explicit: `%APPDATA%\\com.voxvulgi.voxvulgi` is retained by the keep-actions and only removed by the full actions.
- Every managed desktop installer build must increment semantic version.
- If wording semantics need to change, update canonical policy docs first:
  - `governance/spec/PRODUCT_SPEC.md`
  - `governance/spec/TECHNICAL_DESIGN.md`

## Artifact Cleanup Policy

- Use `governance/scripts/cleanup_artifacts.ps1` to remove generated test/tool artifacts.
- Default mode is dry-run; pass `-Force` to execute deletions.

## Proof Standard Policy

- A WP is not `DONE` unless it satisfies `governance/workflow/PROOF_STANDARD.md`.
- New proof bundles should include `summary.md` under `product/desktop/build_target/tool_artifacts/wp_runs/<WP-ID>/...`.
- Build-only verification is not sufficient for UI/operator-heavy packets when the proof standard requires app-boundary or manual evidence.

## Research-First Implementation Policy

- Do not vibecode medium- or high-difficulty technical implementations.
- For medium- or high-difficulty technical work, research first:
  - inspect the current repo/code path,
  - inspect the current spec/design intent,
  - consult primary-source documentation, papers, or official vendor/project references when the solution space is uncertain or fast-moving.
- Convert that research into explicit governance before implementation when scope, architecture, or runtime behavior changes.
- Do not ship speculative integrations or architecture changes based only on plausible-sounding patterns; implementation must be grounded in repo evidence plus researched technical constraints.

## Diagnostics Trace Folder Policy

- Default folder: `%APPDATA%\\com.voxvulgi.voxvulgi\\diagnostics\\traces`.
- The user can move this folder in-app (Diagnostics -> Diagnostics trace -> Move folder...).
- The current active folder is read from app config (`config/diagnostics_trace_dir.txt` override when present).
- Legacy compatibility: if `config/codex_diagnostics_dir.txt` exists from older builds, treat it as fallback.

## User Data Preservation Policy (do not delete)

- The user’s **subscription lists**, **playlists**, and **video library metadata** are considered irreplaceable; do not delete or overwrite them.
- Treat third-party app databases/exports (e.g., 4KVDP SQLite + export dirs) as **read-only** unless the user explicitly requests modification.
- Avoid running deletion/cleanup commands against user media/library/export folders; keep cleanup limited to generated artifacts and require explicit confirmation for destructive modes (e.g., `cleanup_artifacts.ps1 -Force`).

## Built-in Visual Debugger (Agent Usage)

- Agents can capture a snapshot of the current application surface to visually debug the frontend state.
- **Trigger via JS**: Evaluate `window.__voxVulgiRequestSnapshot(subfolder?, label?)` in the active WebView. This returns the absolute file path to the saved PNG snapshot.
  - `subfolder` (optional): organizes snapshots into `governance/snapshots/<subfolder>/`. Use a WP ID (e.g. `"WP-0161"`), audit name (e.g. `"audit_2026-04-08"`), or test label.
  - `label` (optional): prefixes the filename instead of the default `snapshot` (e.g. `label: "library_page"` → `library_page_<timestamp>.png`).
- **Trigger via hotkey**: While the app window is focused, press `Ctrl + Shift + S`. Hotkey snapshots go to `governance/snapshots/manual/`.
- **Folder structure**:
  ```
  governance/snapshots/
    manual/              ← hotkey captures
    WP-0161/             ← per-work-packet test captures
    audit_2026-04-08/    ← agent audit runs
  ```
- Agents can then use their `view_file` tool to inspect the captured PNG file to visually evaluate layout, UI state, or evaluate QA conditions.

## Headless Agent Bridge (WP-0171)

The app exposes a localhost-only HTTP API so agents can navigate pages, trigger snapshots, and read state **without stealing window focus or using keyboard/mouse simulation**.

### Discovery

On startup the app writes two files:
```
%APPDATA%\com.voxvulgi.voxvulgi\agent_bridge_port.txt   (port number, plain text)
%APPDATA%\com.voxvulgi.voxvulgi\agent_bridge.json       ({"port", "pid", "started_at_ms"})
```
Both are removed on graceful shutdown. After a hard kill the JSON file is **stale** — verify the `pid` is still alive before trusting the port (avoids hanging on a network probe to a dead listener).

Recommended discovery flow for an agent:
1. Read `agent_bridge.json`. If missing, fall back to `agent_bridge_port.txt`.
2. If JSON present: confirm the PID is alive (`Get-Process -Id $pid` on Windows). If the PID is dead, treat as stale and stop here.
3. Probe `http://127.0.0.1:<port>/agent/health` with a **short timeout (≤ 3 seconds)** to distinguish stale-port (timeout) from busy-app (slow but eventually responding).

A timed-out health check on a stale port file is the most common false-negative — always pair the file read with a PID check or a short timeout. (WP-0210)

### Endpoints

| Method | Path | Body | Description |
|--------|------|------|-------------|
| `GET` | `/agent/health` | — | Liveness check. Returns `{"status":"ok"}`. |
| `GET` | `/agent/state` | — | Returns `{"current_page","editor_item_id","safe_mode"}`. |
| `POST` | `/agent/navigate` | `{"page":"video_ingest"}` | Switches the active page. Valid pages: `localization`, `video_ingest`, `instagram_archive`, `image_archive`, `media_library`, `jobs`, `diagnostics`, `options`. |
| `POST` | `/agent/snapshot` | `{"subfolder":"WP-0171","label":"jobs_page"}` | Captures a snapshot via html2canvas and returns `{"path":"..."}`. Blocks up to 30 seconds. |
| `POST` | `/agent/dump` | `{"subfolder":"WP-0209","label":"after_run"}` | Writes a JSON state dump (URL, viewport, `.content` scroll, filtered `voxvulgi.*` localStorage, mounted `loc-*` element ids, last 200 console entries) and returns `{"path":"..."}`. Blocks up to 10 seconds. (WP-0209) |
| `POST` | `/agent/freeze_event` | `{"event":"freeze_detected","details":{...},"level":"warn"}` | Worker-only ingress used by the freeze detector. Accepted `event` values: `freeze_detected`, `freeze_recovered`, `worker_alive` (the v0.1.20 liveness heartbeat, fires every 30 s). Appends a row to `diagnostics_trace.jsonl`. Returns `{"status":"ok"}`. (WP-0221) |
| `POST` | `/agent/freeze_dump` | `{"limit":1000,"note":"..."}` | Bundles app version, pid, bridge port, agent state, and the recent trace tail into a single JSON report. Writes a timestamped file plus `freeze_report_latest.json` under the trace dir's `freeze_reports/` subfolder. Returns `{"path","latest_path","trace_rows_included"}`. Runs on the bridge thread, so it works even when the WebView is frozen. (WP-0221) |

### Example (from a terminal or agent script)

```bash
PORT=$(cat "$APPDATA/com.voxvulgi.voxvulgi/agent_bridge_port.txt")
# Navigate to Video Archiver
curl -s -X POST http://127.0.0.1:$PORT/agent/navigate -d '{"page":"video_ingest"}'
sleep 2
# Capture snapshot + paired state dump
curl -s -X POST http://127.0.0.1:$PORT/agent/snapshot -d '{"subfolder":"audit","label":"video_archiver"}'
curl -s -X POST http://127.0.0.1:$PORT/agent/dump     -d '{"subfolder":"audit","label":"video_archiver"}'
```

### JS globals (in-WebView use)

- `window.__voxVulgiNavigate(page)` — switch page programmatically.
- `window.__voxVulgiRequestSnapshot(subfolder?, label?)` — capture snapshot (returns path).
- `window.__voxVulgiRequestDump(subfolder?, label?)` — write paired JSON state dump (returns path). The dump file is `<label>_<ts>.dump.json` next to the snapshot under the same subfolder.

## Freeze Report (WP-0221)

The app continuously records UI freeze evidence (Worker-driven main-thread heartbeat + OS-thread scheduling skew + per-command timing for the most-suspect Tauri commands). Any agent investigating a freeze should read the freeze report directly — no operator relay required.

### Where the data lives (works on v0.1.18+)

- **Self-contained report**: `%APPDATA%\com.voxvulgi.voxvulgi\diagnostics\traces\freeze_reports\freeze_report_latest.json`
  - Overwritten each time the dump is triggered. **This is the canonical path agents should `Read` first.**
  - A timestamped sibling `freeze_report_<ts>.json` is kept alongside for history.
- **Raw continuous trace**: `%APPDATA%\com.voxvulgi.voxvulgi\diagnostics\traces\diagnostics_trace.jsonl`
  - JSON Lines, one row per event. Freeze-related event names: `freeze_detected`, `freeze_recovered`, `worker_alive` (v0.1.20+), `event_loop_skew`, `command_slow`, `command_completed`, `freeze_report_written`.
- Both paths follow the diagnostics trace dir override (`config/diagnostics_trace_dir.txt`); if the operator moved the trace folder, the freeze reports moved with it.

### How the operator (or an agent) triggers a fresh dump

- **From a terminal, while the app is frozen or responsive**: run `vvfreeze.cmd` at the repo root. It reads `agent_bridge_port.txt`, verifies the pid (per WP-0210), POSTs `/agent/freeze_dump`, and prints the resulting paths. Works while the WebView main thread is hung because the bridge runs on its own thread.
- **From inside the app**: Diagnostics → "Diagnostics trace" → "Freeze events" → "Capture freeze report now". Equivalent Tauri command: `invoke("agent_freeze_dump_now", { note })`.
- **Direct HTTP**: `POST http://127.0.0.1:<bridge_port>/agent/freeze_dump` with body `{"limit": 1000, "note": "..."}` (both fields optional).

### Report payload schema

```json
{
  "wp": "WP-0221",
  "generated_at_ms": 1715900000000,
  "app_version": "0.1.18",
  "pid": 12345,
  "bridge_port": 51234,
  "agent_state": { "current_page": "diagnostics", "editor_item_id": null, "safe_mode": false },
  "note": "operator note or null",
  "trace_limit_requested": 1000,
  "recent_trace_count": 137,
  "recent_trace": [ /* DiagnosticsTraceEntry rows, oldest first */ ]
}
```

### Recommended agent flow

1. Operator says "I just ran vvfreeze" (or clicked the button). Read `freeze_report_latest.json` with the absolute APPDATA path above.
2. **Sanity check the Worker first (v0.1.20+)**: grep for `worker_alive` rows in `recent_trace`. They should appear every ~30 seconds. If they are entirely missing while `runtime_sample` rows are still ticking, the freeze-detector Worker silently failed to install — the absence of `freeze_detected` rows in older traces does not mean "no freeze happened", it means "the Worker never ran". Surface that as a separate bug rather than diagnosing the freeze itself.
3. Scan `recent_trace` for `freeze_detected` / `freeze_recovered` pairs to get freeze duration and surrounding context (last window event, current page, in-flight ping id). The threshold is **250 ms** as of v0.1.20 (was 500 ms in v0.1.18 / v0.1.19), so short stalls are now visible.
4. Cross-reference timestamps with `command_completed` / `command_slow` rows from the same trace window to identify which Tauri command was in flight when the freeze started.
5. `event_loop_skew` rows indicate process-level scheduling starvation (SMB stall on UNC paths, AV scan, DLL loader lock) — these are often the upstream cause of a freeze that masquerades as a Tauri command stall.
6. If `worker_alive` rows are present **but** there are no `freeze_detected` rows during a freeze the operator observed, the JS event loop is not the blocked thread. Suspect the WebView UI / GPU compositor layer (the app uses `transparent: true, decorations: false` in `tauri.conf.json`, a frameless+transparent config that has known DWM-compositor stalls on Windows under load). That class of freeze needs a different probe than the JS Worker — log it for the next diagnostic build.
