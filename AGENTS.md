# Repo Agent Notes

## Desktop Build Target Policy

- For desktop release builds, use `governance/scripts/build_desktop_target.ps1` (or `npm run build:desktop:target` from `product/desktop`).
- Desktop installer builds must refresh the bundled offline payload so Phase 1 + Phase 2 dependencies are included in the installer resources.
- Build outputs must go under:
  - `product/desktop/Build Target/Current`
- Previous build outputs must be archived under:
  - `product/desktop/Build Target/Old versions`

## Artifact Cleanup Policy

- Use `governance/scripts/cleanup_artifacts.ps1` to remove generated test/tool artifacts.
- Default mode is dry-run; pass `-Force` to execute deletions.

## Diagnostics Trace Folder Policy

- Default folder: `%APPDATA%\\com.voxvulgi.voxvulgi\\diagnostics\\traces`.
- The user can move this folder in-app (Diagnostics -> Diagnostics trace -> Move folder...).
- The current active folder is read from app config (`config/diagnostics_trace_dir.txt` override when present).
- Legacy compatibility: if `config/codex_diagnostics_dir.txt` exists from older builds, treat it as fallback.

## User Data Preservation Policy (do not delete)

- The user’s **subscription lists**, **playlists**, and **video library metadata** are considered irreplaceable; do not delete or overwrite them.
- Treat third-party app databases/exports (e.g., 4KVDP SQLite + export dirs) as **read-only** unless the user explicitly requests modification.
- Avoid running deletion/cleanup commands against user media/library/export folders; keep cleanup limited to generated artifacts and require explicit confirmation for destructive modes (e.g., `cleanup_artifacts.ps1 -Force`).

