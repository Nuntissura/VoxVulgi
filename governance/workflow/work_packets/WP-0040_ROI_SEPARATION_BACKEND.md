# Work Packet: WP-0040 - ROI-13: Better separation backend option (explicit install)

## Metadata
- ID: WP-0040
- Owner: Codex
- Status: DONE
- Created: 2026-02-22
- Target milestone: Phase 2 (voice-preserving dubbing)

## Intent

- What: Provide an optional higher-quality separation backend (explicit install) in addition to Spleeter.
- Why: Separation quality strongly impacts dub quality; Spleeter is a baseline and may be insufficient for some content.

## Scope

In scope:

- Governance:
  - Select a candidate separation backend with acceptable licensing/weights for personal use and potential future freemium distribution.
- Engine/Desktop:
  - Add a new explicit-install pack + status in Diagnostics.
  - Add a separation job using the new backend.
  - Allow users to select separation backend per item/job.

Out of scope:

- Automatically downloading models without user action.
- Telemetry.
- Consent gates or anti-abuse controls.

## Acceptance criteria

- A second separation backend can be installed explicitly and executed to produce vocals/background stems.
- The user can choose between Spleeter and the optional backend.

## Test / verification plan

- Run both separation backends on the same clip and compare outputs subjectively.

## Risks / open questions

- Some top-quality separation tools rely on gated weights or uncertain licensing; selection must be deliberate.

## Status updates

- 2026-02-22: Created.
- 2026-02-23: Added Demucs optional separation backend (explicit install) alongside Spleeter, with per-job backend selection in UI and artifact listing; verified via build + tests.
