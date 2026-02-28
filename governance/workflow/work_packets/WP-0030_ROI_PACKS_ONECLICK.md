# Work Packet: WP-0030 - ROI-01: One-click "Phase 2 Packs" installer

## Metadata
- ID: WP-0030
- Owner: Codex
- Status: DONE
- Created: 2026-02-22
- Target milestone: Phase 2 (voice-preserving dubbing)

## Intent

- What: Provide a single "Install Phase 2 packs" action that installs all optional Phase 2 Python packs with clear progress, expected disk impact, and clear failure reporting.
- Why: Reduce friction. Users should not have to click-install multiple packs individually or guess whether the machine has enough disk.

## Scope

In scope:

- Desktop:
  - Add a one-click installer entry point in Diagnostics.
  - Show progress per pack (queued, downloading, installing, done, failed).
  - Show disk impact:
    - best-effort estimate before install (may be "unknown"),
    - actual disk usage after install (measured locally).
- Engine:
  - Add a "Phase 2 packs install plan" list in one place so UI can drive it deterministically.
  - Ensure installs remain explicit user actions (no background auto-install).
  - Persist per-pack install logs in app data for troubleshooting.

Out of scope:

- Any silent network egress or background downloads.
- Telemetry.
- Consent gates or anti-abuse controls.

## Acceptance criteria

- Diagnostics shows an "Install Phase 2 packs" button that installs:
  - Python toolchain (if needed),
  - Spleeter pack,
  - Diarization pack,
  - TTS preview pack,
  - any other Phase 2 packs listed in the install plan.
- Installer progress is visible and does not appear "stuck" (step-level state changes).
- Disk usage for installed packs is shown after installation (best-effort).
- Failures surface a readable error and a link/action to open the install log.

## Test / verification plan

- Run the one-click installer on a clean machine profile.
- Simulate a failure (no network / insufficient disk) and verify error reporting and logs.

## Risks / open questions

- Reliable pre-install disk estimates are hard with pip; we may only be able to provide post-install measurements.
- Some packs may do lazy model downloads; we should ensure those are triggered during explicit install where possible.

## Status updates

- 2026-02-22: Created.
- 2026-02-23: Implemented one-click Phase 2 packs installer job + persisted per-step logs/state + Diagnostics UI progress table; verified via `cargo test` + Windows bundle build.
