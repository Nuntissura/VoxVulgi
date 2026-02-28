# Work Packet: WP-0039 - ROI-12: Batch processing rules on import (local-only)

## Metadata
- ID: WP-0039
- Owner: Codex
- Status: DONE
- Created: 2026-02-22
- Target milestone: Phase 2 (voice-preserving dubbing)

## Intent

- What: Allow users to define local-only batch processing rules that run automatically on import (e.g., ASR, translate, dub preview).
- Why: Reduce repetitive manual steps for multi-item workflows.

## Scope

In scope:

- Desktop:
  - Settings UI for batch rules (off by default):
    - toggles for auto ASR / auto translate / auto separate / auto diarize / auto dub preview.
  - Import UI indicates what will run after import (transparent).
- Engine:
  - Job orchestration for per-item pipelines based on enabled rules.
  - Ensure rules do not run unless explicitly enabled by the user.

Out of scope:

- Cloud provider automation (local-only rules only).
- Telemetry.
- Consent gates or anti-abuse controls.

## Acceptance criteria

- With rules enabled, importing media automatically queues the configured jobs for each new item.
- With rules disabled, import does not queue any background processing.

## Test / verification plan

- Enable rules, import multiple items, verify job queues are created and execute in order.
- Disable rules, import an item, verify no jobs are queued.

## Risks / open questions

- Orchestration needs clear error handling (one failure should not silently block the rest).

## Status updates

- 2026-02-22: Created.
- 2026-02-23: Implemented batch-on-import rules (local-only; off by default) with Diagnostics settings UI and transparent Library import summary; engine queues ASR/separation/translate/diarize/dub preview based on rules; verified via build + tests.
