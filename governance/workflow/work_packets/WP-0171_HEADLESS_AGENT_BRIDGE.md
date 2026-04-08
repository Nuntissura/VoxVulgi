# Work Packet: WP-0171 - Headless Agent Bridge

## Metadata
- ID: WP-0171
- Owner: Codex
- Status: REVIEW
- Created: 2026-04-08
- Target milestone: Tooling

## Intent

- What: Add a programmatic agent control API so LLM agents can navigate pages, trigger snapshots, and read UI state without stealing window focus, moving the mouse, or sending keystrokes.
- Why: The current visual debugger (WP-0163) requires keyboard focus to trigger snapshots via Ctrl+Shift+S or JS evaluation. When agents automate the app via PowerShell/keybd_event, they steal focus, hijack the mouse, and block the operator from doing any work. A headless bridge lets agents test and debug without disrupting the operator.

## Scope

In scope:
- Add a lightweight local HTTP server (localhost-only, random high port) inside the Tauri app that accepts JSON commands:
  - `POST /agent/navigate` — `{"page": "video_ingest"}` — switches the active page without focus change
  - `POST /agent/snapshot` — `{"subfolder": "WP-0171", "label": "video_archiver"}` — captures a snapshot via html2canvas and returns the file path
  - `GET /agent/state` — returns current page, window size, active item ID, safe mode status
  - `GET /agent/health` — simple liveness check
- Write the agent bridge port to a well-known file (`%APPDATA%/com.voxvulgi.voxvulgi/agent_bridge_port.txt`) so agents can discover it.
- Add a `window.__voxVulgiNavigate(page)` JS global for internal use.
- Update `AGENTS.md` with bridge usage documentation.
- The bridge must only bind to `127.0.0.1` (no remote access).

Out of scope:
- Authentication/token system for the bridge (localhost-only is sufficient for single-user desktop app).
- Full UI automation (clicking buttons, filling forms) — that is a future extension.
- WebSocket/streaming support.

## Acceptance criteria
- Agent can navigate to any page and capture a snapshot via HTTP without the app window gaining focus.
- Port file is written on startup and cleaned up on exit.
- `AGENTS.md` documents the bridge endpoints.
- `cargo check` + `npm run build` pass.

## Test / verification plan
- From a terminal, use `curl` to navigate to each page and capture snapshots while the operator uses another application in the foreground.
- Verify no focus stealing occurs during the entire sequence.
- Verify port file is created on startup and removed on exit.
