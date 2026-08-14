# Work Packet: WP-0210 - Bridge and visual debugger reliability

## Metadata
- ID: WP-0210
- Owner: Codex
- Status: DONE
- Created: 2026-04-26
- Target milestone: Localization operator usability / agent debuggability
- Builds on: WP-0163, WP-0171, WP-0209

## Intent

- What: Fix the two highest-impact reliability gaps in the agent bridge and visual debugger that surfaced during the WP-0205-0209 session: stale port-file misdirection, and the React StrictMode async-listener race that doubles every snapshot/dump capture.
- Why: During this session I read a stale `agent_bridge_port.txt` (last written the previous day), probed `/agent/health`, got a timeout, and assumed "app frozen" — when in reality the app simply was not running. The current bridge contract is "trust the port file; verify with /health" but a dead port produces a *timeout*, not a refused connection, which is indistinguishable from a frozen process. Separately, every snapshot under `governance/snapshots/WP-0202` and `WP-0203_0204` was saved twice within 10-30 ms (same byte size); cause traced to React.StrictMode mounting the bridge useEffect twice while async `await listen(...)` registrations leak past the cleanup function.

## Scope

In scope:
- Bridge: write a JSON sidecar (`agent_bridge.json`) at the same time as the existing `agent_bridge_port.txt`, containing `{port, pid, started_at_ms}`. Agents can confirm the PID is alive before trusting the port (skips a network probe).
- Bridge: register a Tauri `RunEvent::Exit` handler that best-effort deletes both `agent_bridge_port.txt` and `agent_bridge.json` on graceful shutdown.
- Frontend: fix the StrictMode async-listener race in the bridge `useEffect`. Use a `disposed` flag captured by the closure; if `disposed` is true after the `await listen(...)` resolves, immediately call the returned unlisten and skip pushing it. Same fix for `agent-navigate`, `agent-snapshot-request`, and `agent-dump-request`.
- AGENTS.md: document the new sidecar and recommend `Invoke-RestMethod -TimeoutSec 3` (or equivalent short timeout) for `/agent/health`.

Out of scope:
- B3 (panic recovery for the bridge thread) - lower-priority, deferred.
- V2 (walk-up `current_dir` fragility in installed builds) - latent only; deferred.
- V3 (empty-canvas detection) - deferred until we observe a real empty capture.

## Acceptance criteria

- After a graceful app shutdown, both `agent_bridge_port.txt` and `agent_bridge.json` are removed from `%APPDATA%\com.voxvulgi.voxvulgi\`.
- `agent_bridge.json` contains valid JSON with port, pid, started_at_ms.
- A snapshot/dump request through the bridge produces exactly one PNG / one JSON file (no near-twin), confirmed by listing `governance/snapshots/WP-0210/` after a single capture.
- AGENTS.md describes the sidecar lifecycle and the short-timeout health probe.
- `cargo check` and desktop `npm run build` pass when run.

## Test / verification plan

- Start the app, confirm both files exist and JSON parses, PID matches the running process.
- Quit the app via the X button (graceful), confirm both files are removed.
- Force-kill the app (simulate crash) - JSON file remains; agent treats it as stale by detecting that PID is no longer running. Capture this case in AGENTS.md.
- Trigger a snapshot via `/agent/snapshot` and a dump via `/agent/dump`, confirm exactly one file each ends up under the target subfolder.

## Risks / open questions

- Risk: the bridge already runs in a daemon thread; on hard kill, the JSON sidecar is stale and can only be detected by PID lookup. We accept this and document it.
- Open: should agents be told to delete a stale sidecar themselves on detection? For now no - the next app start overwrites it.

## Status updates

- 2026-04-26: Created during the bridge/visual debugger investigation. Implementation immediately follows.
- 2026-08-14: Current packaged-app bridge verification on v0.1.133 confirmed a valid live PID/port sidecar and exactly one dump plus one snapshot for one request each. A close-window message to the agent-headless process did not terminate it within 10 seconds, so graceful-exit sidecar removal remains unproven; the packet stays `IN_PROGRESS`. Evidence is recorded under WP-0307.
- 2026-08-14: DONE on governed v0.1.138. The live JSON sidecar matched the running PID before the health probe; one request created one PNG and one dump; the documented stale-sidecar hard-kill behavior was observed; and invoking the app's exact Tauri `window_close` command terminated the hidden process and removed both sidecars. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0210/20260814_final/`.
