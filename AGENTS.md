# Repo Agent Notes

## Desktop Build Target Policy

- For desktop release builds, use `governance/scripts/build_desktop_target.ps1` (or `npm run build:desktop:target` from `product/desktop`).
- Desktop installer builds must refresh the bundled offline payload so Phase 1 + Phase 2 dependencies are included in the installer resources.
- Every desktop target build must increment the desktop semantic version.
- Every desktop target build must append an entry to `governance/release/BUILD_CHANGELOG.md` with included Work Packet IDs.
- Build logs for each desktop target build must be written under:
  - `product/desktop/Build Target/logs`
- Build outputs must go under:
  - `product/desktop/Build Target/Current`
- Previous build outputs must be archived under:
  - `product/desktop/Build Target/Old versions`

## Installer Maintenance Mode Policy

- Preserve these exact installer maintenance labels:
  - `Update/Repair`
  - `Full reinstall`
  - `Uninstall`
- Keep existing-install flow clear: show the pre-maintenance explainer before maintenance selection.
- Keep app-data behavior explicit: `%APPDATA%\\com.voxvulgi.voxvulgi` is retained unless delete-app-data is explicitly chosen.
- If wording semantics need to change, update canonical policy docs first:
  - `governance/spec/PRODUCT_SPEC.md`
  - `governance/spec/TECHNICAL_DESIGN.md`

## Artifact Cleanup Policy

- Use `governance/scripts/cleanup_artifacts.ps1` to remove generated test/tool artifacts.
- Default mode is dry-run; pass `-Force` to execute deletions.

## Proof Standard Policy

- A WP is not `DONE` unless it satisfies `governance/workflow/PROOF_STANDARD.md`.
- New proof bundles should include `summary.md` under `product/desktop/Build Target/tool_artifacts/wp_runs/<WP-ID>/...`.
- Build-only verification is not sufficient for UI/operator-heavy packets when the proof standard requires app-boundary or manual evidence.

## Diagnostics Trace Folder Policy

- Default folder: `%APPDATA%\\com.voxvulgi.voxvulgi\\diagnostics\\traces`.
- The user can move this folder in-app (Diagnostics -> Diagnostics trace -> Move folder...).
- The current active folder is read from app config (`config/diagnostics_trace_dir.txt` override when present).
- Legacy compatibility: if `config/codex_diagnostics_dir.txt` exists from older builds, treat it as fallback.

## User Data Preservation Policy (do not delete)

- The user’s **subscription lists**, **playlists**, and **video library metadata** are considered irreplaceable; do not delete or overwrite them.
- Treat third-party app databases/exports (e.g., 4KVDP SQLite + export dirs) as **read-only** unless the user explicitly requests modification.
- Avoid running deletion/cleanup commands against user media/library/export folders; keep cleanup limited to generated artifacts and require explicit confirmation for destructive modes (e.g., `cleanup_artifacts.ps1 -Force`).

