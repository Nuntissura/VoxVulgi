# WP-0210 Summary

Status: DONE
Date: 2026-08-14

## Outcome
- The bridge writes a live JSON PID/port/start-time sidecar, agents can identity-check it before probing, one bridge request creates one capture, and graceful Tauri window closure removes both bridge sidecars.
- The documented hard-kill behavior was also observed: a session-owned forced termination left both files stale until the next app start/explicit test cleanup.

## Verification
- Started the governed v0.1.138 desktop artifact in hidden `--agent-headless` mode at BelowNormal priority with no pre-existing bridge sidecars.
- Parsed `agent_bridge.json` as `{pid:55532,port:65335,started_at_ms:...}` and independently confirmed PID 55532 was alive before probing `/agent/health`.
- A single snapshot request and a single dump request created exactly one PNG and one JSON file in an initially empty folder.
- Invoked the frontend's actual `window.__TAURI_INTERNALS__.invoke('window_close')` command through the hidden WebView debugging channel. The command resolved successfully; PID 55532 exited and both `agent_bridge.json` and `agent_bridge_port.txt` were absent three seconds later.

## Evidence
- `evidence.json`
- `product/desktop/build_target/logs/build_desktop_target_20260814-143555_0_1_138.log`
- `governance/snapshots/WP-0209_build_0_1_138/runtime_state_1786712263595.png`
- `governance/snapshots/WP-0209_build_0_1_138/runtime_state_1786712263626.dump.json`

## Notes
- An OS `CloseMainWindow` message closed the hidden WebView surface but did not end the process, so it was not accepted as proof. The final proof used the exact Tauri `window_close` command invoked by the app's X control.
