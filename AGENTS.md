# Repo Agent Notes

## Desktop Build Target Policy

- For desktop release builds, use `governance/scripts/build_desktop_target.ps1` (or `npm run build:desktop:target` from `product/desktop`).
- Build outputs must go under:
  - `product/desktop/Build Target/Current`
- Previous build outputs must be archived under:
  - `product/desktop/Build Target/Old versions`

## Artifact Cleanup Policy

- Use `governance/scripts/cleanup_artifacts.ps1` to remove generated test/tool artifacts.
- Default mode is dry-run; pass `-Force` to execute deletions.
