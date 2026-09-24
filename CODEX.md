# VoxVulgi Codex Reminder

## Offline Installer Reminder

- [VV-CODEX-INSTALL-001] The offline installer is one ISO containing the existing working VoxVulgi app/core, all existing dependencies, installer, and offline tooling; packaging only bundles them and never builds, downloads, repairs, starts, warms up, or upgrades them.
- [VV-CODEX-INSTALL-002] Create it package-first: hash and reuse existing inputs, build the wrapper and ISO, then test that exact ISO offline and publish only the passing hash.
- [VV-CODEX-INSTALL-003] Creating or retrying the installer is not a desktop build and never bumps the version or changelog; product builds and version changes happen separately.
- [VV-CODEX-INSTALL-004] `offline-installer-runtime/GUIDE.md` is the sole detailed authority; use only an entrypoint it marks conforming and do not duplicate its procedure here.
