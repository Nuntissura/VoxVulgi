# Work Packet: WP-0031 - ROI-02: Portable Python distribution option (no system Python required)

## Metadata
- ID: WP-0031
- Owner: Codex
- Status: DONE
- Created: 2026-02-22
- Target milestone: Phase 2 (voice-preserving dubbing)

## Intent

- What: Add an optional portable Python distribution path so VoxVulgi can run Python-side packs without relying on a system Python installation.
- Why: Reduce setup friction and support environments where Python is unavailable or locked down.

## Scope

In scope:

- Engine:
  - Extend the Python toolchain manager to support a "portable python" install root.
  - Prefer the portable python when installed, otherwise fall back to current behavior.
  - Keep the existing user override (`config/python_exe.txt`) as the highest priority.
- Desktop (Diagnostics):
  - Add "Install portable Python" (explicit user action).
  - Show installed portable Python version and location.

Out of scope:

- Bundling Python inside the default app installer (this is optional and may be platform-specific).
- Telemetry.
- Consent gates or anti-abuse controls.

## Acceptance criteria

- A user can install a portable Python distribution via Diagnostics (explicit click).
- After portable Python is installed, Python pack installs and Python jobs work without system Python.
- Portable Python install does not occur without explicit user action.

## Test / verification plan

- On a machine without Python in PATH: install portable Python, install one Python pack, run one Python job.

## Risks / open questions

- Cross-platform portable Python distribution choices vary; keep this optional and explicit.
- Prefer hash-verified downloads (ties to ROI-15).

## Status updates

- 2026-02-22: Created.
- 2026-02-23: Added portable Python status + explicit install action in Diagnostics and ensured portable Python is preferred when installed; verified via build + tests.
