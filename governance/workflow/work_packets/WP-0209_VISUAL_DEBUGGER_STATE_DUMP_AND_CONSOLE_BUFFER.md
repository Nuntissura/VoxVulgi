# Work Packet: WP-0209 - Visual debugger state dump and console buffer

## Metadata
- ID: WP-0209
- Owner: Codex
- Status: IN_PROGRESS
- Created: 2026-04-26
- Target milestone: Localization operator usability / agent debuggability
- Builds on: WP-0163, WP-0171

## Intent

- What: Extend the agent visual debugger so a snapshot can be paired with a structured JSON dump (current page + editor item id + visible section ids + viewport + scroll + filtered localStorage + console buffer) and so an agent can request that dump over the headless agent bridge without restarting the app.
- Why: After the WP-0205-0208 consolidation, verifying UI behavior visually still requires reading PNGs by eye. A small JSON dump lets the agent answer questions like "is loc-run mounted" or "what does Workflow Panel state actually look like" without opening the image, and a console buffer captures the silent React warnings or failed `invoke` calls that don't show up on screen.

## Scope

In scope:
- Frontend: install a small console-buffer at app startup that captures the last 200 entries from `console.log`, `console.warn`, `console.error` with timestamps; expose a getter for the bridge.
- Frontend: add `window.__voxVulgiRequestDump(subfolder?, label?)` that builds a JSON state-dump and writes it via a new `admin_save_dump` Tauri command (sibling of `admin_save_snapshot`). Returns the absolute file path.
- Frontend: listen for `agent-dump-request` Tauri events emitted by the bridge and respond via a new `agent_dump_complete` Tauri command (sibling of `agent_snapshot_complete`).
- Tauri: implement `admin_save_dump`, `agent_dump_complete`, and add a `dump_tx` channel on `AgentBridgeInner`.
- Bridge: add `POST /agent/dump` endpoint following the same channel/event pattern as `/agent/snapshot`. Returns `{"path": "..."}` on success.
- Document the new endpoint and `window.__voxVulgiRequestDump` in `AGENTS.md`.

JSON dump payload:
- `timestamp_ms`, `app_version` (from `appInfo` if known)
- `viewport`: `{ width, height }`
- `content_scroll_top` (from `.content` element if present)
- `current_page`, `editor_item_id`, `safe_mode` (from existing agent bridge state pattern)
- `url`: location.href + hash
- `localstorage_voxvulgi`: only keys starting with `voxvulgi.` (avoid leaking unrelated extension data)
- `mounted_section_ids`: every element id starting with `loc-` that is currently rendered (a quick way for the agent to know which legacy sections are mounted)
- `console_buffer`: last 200 entries, each `{ ts_ms, level, args }` with args JSON-stringified

Out of scope (explicitly deferred to later WPs if needed):
- Performance trace JSON / Chrome trace export.
- Viewport-size selector for snapshot capture.
- React component tree dump.
- DOM full-tree export.

## Acceptance criteria

- `window.__voxVulgiRequestDump(subfolder?, label?)` returns the path to a written JSON file under `governance/snapshots/<subfolder>/<label>_<ts>.dump.json`.
- `Ctrl+Shift+S` continues to capture a PNG snapshot only (unchanged).
- `POST /agent/dump` writes a dump and returns its path; default behavior on error mirrors `/agent/snapshot` (504 on timeout, 500 on failure).
- The dump JSON contains all fields listed above; `console_buffer` is bounded to 200 entries.
- `AGENTS.md` documents the new endpoint and the JS global.
- `cargo check` and desktop `npm run build` pass when run.

## Test / verification plan

- Manual: from the in-app DevTools console, call `window.__voxVulgiRequestDump("WP-0209", "manual")` and confirm the JSON file appears under `governance/snapshots/WP-0209/`.
- Manual: from a terminal, hit `POST http://127.0.0.1:<port>/agent/dump` with `{"subfolder":"WP-0209","label":"bridge"}` and confirm the response includes a path and the file exists.
- Manual: trigger an intentional `console.warn` and confirm it appears in the next dump's `console_buffer`.

## Risks / open questions

- Risk: monkey-patching `console.*` globally could interact with other tooling. Mitigation: keep the patched functions transparent (still call originals), and only retain a bounded ring.
- Risk: `localStorage` may contain large values. Mitigation: filter to `voxvulgi.` keys and cap each value at ~4 KB.
- Open: should the snapshot endpoint also write a dump alongside? For now it is a separate endpoint to keep the two cleanly composable; an agent that wants both calls them in sequence.

## Status updates

- 2026-04-26: Created and implementation started.
