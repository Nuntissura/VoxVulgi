# Work Packet: WP-0021 - Phase 2: Python toolchain runner + explicit installers (sidecar)

## Metadata
- ID: WP-0021
- Owner: Codex
- Status: DONE
- Created: 2026-02-22
- Target milestone: Phase 2 (voice-preserving dubbing)

## Intent

- What: Add a managed Python sidecar toolchain (venv under app-data) plus engine + UI plumbing to check status and set it up via explicit user action.
- Why: Most diarization/separation/TTS/VC candidates for Phase 2 are Python-first. We need a local-first, explicit-install path that does not introduce silent network egress.

## Scope

In scope:

- Engine:
  - Define Python toolchain paths under `tools/python/` (venv location).
  - Implement `tools_python_status` and `tools_python_install`:
    - status reports base Python availability/version + venv availability/version,
    - install creates the venv (and verifies pip availability).
  - Provide a helper to get the venv Python executable path for future job runners.
- Desktop:
  - Diagnostics UI displays Python toolchain status and exposes a "Setup Python toolchain" button (explicit user action).
- Governance:
  - Add WP row to `governance/workflow/TASK_BOARD.md`.

Out of scope:

- Implementing Phase 2 jobs (separation/diarization/TTS/VC).
- Bundling GPU frameworks (PyTorch/TensorFlow) inside the main installer.
- Any telemetry.

## Acceptance criteria

- `governance/workflow/work_packets/WP-0021_PHASE2_PYTHON_TOOLCHAIN_RUNNER_AND_INSTALLERS.md` exists and is tracked on the Task Board.
- Diagnostics page shows:
  - whether base Python is available and its version,
  - venv dir location and whether it exists,
  - venv Python version (if created).
- Clicking "Setup Python toolchain" creates a venv under app-data and refreshes status.
- No silent network egress is introduced (only user-initiated setup actions run external tooling).

## Test / verification plan

- Open Diagnostics:
  - confirm toolchain status renders (before setup).
- Click "Setup Python toolchain":
  - verify venv is created and status updates.
- If Python is not available on the system:
  - setup fails with a clear error (no partial state that breaks other features).

## Risks / open questions

- Packaging strategy: BYO Python vs bundling a portable Python distribution.
- Dependency footprint: separation/diarization/TTS stacks can be very large; we may need per-capability "packs" rather than one huge environment.

## Status updates

- 2026-02-22: Started.
- 2026-02-22: Implemented Python toolchain status + setup (venv) + Diagnostics UI wiring.
- 2026-02-22: Verified:
  - `cargo test --manifest-path product/engine/Cargo.toml --locked`
  - `cargo test --manifest-path product/desktop/src-tauri/Cargo.toml --locked`
  - `npm run build` (from `product/desktop/`)
