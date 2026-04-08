# Repo Agent Notes

## Desktop Build Output Policy

- For desktop release builds, use `governance/scripts/build_desktop_target.ps1` (or `npm run build:desktop:target` from `product/desktop`).
- Desktop installer builds must refresh the bundled offline payload so Phase 1 + Phase 2 dependencies are included in the installer resources.
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

On startup the app writes the bridge port to:
```
%APPDATA%\com.voxvulgi.voxvulgi\agent_bridge_port.txt
```
Read that file to get the port number, then call `http://127.0.0.1:<port>/agent/health` to verify.

### Endpoints

| Method | Path | Body | Description |
|--------|------|------|-------------|
| `GET` | `/agent/health` | — | Liveness check. Returns `{"status":"ok"}`. |
| `GET` | `/agent/state` | — | Returns `{"current_page","editor_item_id","safe_mode"}`. |
| `POST` | `/agent/navigate` | `{"page":"video_ingest"}` | Switches the active page. Valid pages: `localization`, `video_ingest`, `instagram_archive`, `image_archive`, `media_library`, `jobs`, `diagnostics`, `options`. |
| `POST` | `/agent/snapshot` | `{"subfolder":"WP-0171","label":"jobs_page"}` | Captures a snapshot via html2canvas and returns `{"path":"..."}`. Blocks up to 15 seconds. |

### Example (from a terminal or agent script)

```bash
PORT=$(cat "$APPDATA/com.voxvulgi.voxvulgi/agent_bridge_port.txt")
# Navigate to Video Archiver
curl -s -X POST http://127.0.0.1:$PORT/agent/navigate -d '{"page":"video_ingest"}'
sleep 2
# Capture snapshot
curl -s -X POST http://127.0.0.1:$PORT/agent/snapshot -d '{"subfolder":"audit","label":"video_archiver"}'
```

### JS globals (in-WebView use)

- `window.__voxVulgiNavigate(page)` — switch page programmatically.
- `window.__voxVulgiRequestSnapshot(subfolder?, label?)` — capture snapshot (returns path).
