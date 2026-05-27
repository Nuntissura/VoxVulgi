# Work Packet: WP-0237 - Bundle pre-resolved wheels in installer (offline-default)

## Status

BACKLOG

## Owner

-

## Operator Request Preserved

- "how can we harden it ? so we are sure the downloading happens?" (2026-05-18) — the strongest answer to "sure the downloading happens" is "the downloading has already happened, at build time".

## Intent

- What: Ship the entire set of pre-resolved Python wheels for every pack inside the desktop installer. At install time, pip runs with `--no-index --find-links <bundled_wheel_dir> --require-hashes -r <lockfile>`. PyPI is not required.
- Why: Today every fresh install hits PyPI for ~2 GB of wheels. Network drops, PyPI slow paths, mirror unavailability, and corporate-proxy interference all cause partial installs and the WP-0231-class bugs. Bundled wheels remove the entire failure surface; the install becomes "extract a tarball + run pip offline".

## Scope

In scope:
- Extend `governance/scripts/build_desktop_target.ps1` (or the existing offline-bundle prep at `product/engine/src/bin/voxvulgi_offline_bundle_prep.rs`) to:
  - generate the lockfile (WP-0232 prerequisite),
  - download every wheel listed in the lockfile into `product/desktop/build_target/payload/wheels/` at build time,
  - verify sha256s against the lockfile,
  - emit a manifest of bundled wheels.
- Embed `payload/wheels/` into the desktop installer (NSIS / Tauri bundler config).
- At first-run install time, the engine:
  - reads the lockfile,
  - reads the bundled wheel dir from a known APPDATA-relative path,
  - calls `pip install --no-index --find-links <wheels> --require-hashes -r <lockfile>`,
  - falls back to `--find-links + --index-url=https://pypi.org/simple/` only if a wheel is missing (build-error path; shouldn't happen, but defensive).
- Document in `build_rules.md` that the wheel bundle is now part of the release artifact and must be regenerated when the lockfile changes.
- Reuse the existing offline-payload-policy from `build_rules.md` (routine builds reuse verified payload; full-refresh builds explicitly regenerate).

Out of scope:
- Bundling model weights (separate WP-0238 — different cost/benefit tradeoff).
- Switching to uv (Tier 4, deferred).
- A "download wheels later" online-only installer variant — the offline-default is the only target here.

## Acceptance Criteria

- A built installer contains all wheels needed for every pack the app ships.
- A fresh install on a machine with NO internet access completes the Python pack install successfully.
- Total installer size delta is < 2 GB (Kokoro+OpenVoice deps dominate; torch wheel alone is ~800 MB).
- `pip install` at first-run completes in < 60 seconds (vs current 5–15 min) — record actual time in the WP Notes after first run.
- Wheel bundle is regenerated when `pinned_dependency_manifest.json` or any pack lockfile changes; build refuses to ship a stale bundle.
- Proof bundle under `product/desktop/build_target/tool_artifacts/wp_runs/WP-0237/...`.

## Research Basis

### Sources checked
- `product/engine/src/bin/voxvulgi_offline_bundle_prep.rs` — already exists to pre-build the offline payload at build time. The wheel bundle is a natural extension.
- `build_rules.md` (referenced from CLAUDE.md) — codifies the existing offline-payload policy: routine builds reuse a verified payload, full-refresh builds regenerate. WP-0237 just makes the offline payload the *default* install path, not just the build path.
- `governance/workflow/work_packets/WP-0189_OFFLINE_BUNDLE_VOICE_PACK_COMPATIBILITY_REPAIR.md` (DONE) — proves the offline payload prep path already exists and works; WP-0237 productionizes it for end-user installs.
- pip docs: `pip install --no-index --find-links <dir> -r <file>` is the canonical offline-install pattern, supported since pip 1.x.

### Selected approach
- Bundled wheels live under the installed app's APPDATA root: `%APPDATA%\com.voxvulgi.voxvulgi\install_payload\wheels\`.
- After successful first-run install, the bundle can be optionally deleted (toggleable in Options: "Free up disk space — remove install payload after first run"); not deleted by default so Repair (WP-0236) stays offline-capable.
- Installer-side: NSIS / Tauri bundler embeds the wheels as a subdirectory. Installer size doubles, but install speed is 10x faster and the failure surface shrinks dramatically.

### Rejected options
- CDN-hosted wheel mirror under operator control. Rejected: still has network dependency; just moves the failure surface from PyPI to operator infrastructure.
- Bundle in a separate "voice pack installer" download. Rejected: defeats the user-friendly goal; one installer that just works is better than two.
- Use `pip download` to a system-wide cache and rely on it. Rejected: pip's cache isn't bundled, and a hashed `--find-links` is the deterministic option.

### Risks and mitigations
- Risk: installer size doubles, hurting download/uptake. Mitigation: surface the tradeoff at the download page ("Includes everything you need — no extra downloads"); offer an "online installer" variant later if data shows users care about the smaller initial download.
- Risk: bundled wheels become stale relative to security patches. Mitigation: each release regenerates the lockfile + bundle; release cadence already exists.
- Risk: torch wheel size + bundling triggers Windows installer (NSIS) limits. Mitigation: NSIS handles multi-GB installers fine; only edge cases are 4 GB cabinets (NSIS uses zlib chunked). Test during implementation.
- Risk: corporate AV / Defender scans every wheel on extract, adds 5+ minutes. Mitigation: document the AV impact in the installer release notes; consider a single `wheels.tar` blob extracted at first run only when install is invoked.

### Validation plan
- Build an installer with the bundle. Take a Windows VM with no internet. Install and run Localization Studio setup. Confirm pack install completes.
- Build an installer without the bundle (regression). Confirm install still works with internet (proves the fallback path).
- Measure installer size and first-run install time before/after; record in the WP Notes.

## Red-Team

- Failure: a transitive dep is missing from the bundle because lockfile generation skipped it. Control: install path uses `--no-index`; missing dep is a hard error, not a silent PyPI fetch. CI gate (WP-0233) catches this before release.
- Failure: bundled wheel architecture doesn't match the user's Python (e.g., wrong cp311 vs cp312). Control: portable Python is pinned; lockfile is generated against that exact Python; CI gate verifies.
- Failure: installer signing breaks because of size. Control: existing signing pipeline handles ~1 GB installers (already true for prior offline-bundled builds); confirm at first build of this WP.

## Notes

- 2026-05-18: WP created as Tier-3 hardening. The big-bet WP: removes the entire network-during-install failure surface. Depends on WP-0232 (lockfile) being in place.
