# Work Packet: WP-0152 - Assisted speaker-reference extraction and first-dub recovery

## Metadata
- ID: WP-0152
- Owner: Codex
- Status: DONE
- Created: 2026-03-12
- Target milestone: Post-0.1.6 localization-first recovery

## Intent

- What: Add an assisted reference-extraction path inside Localization Studio so diarized speakers can quickly receive usable reference bundles from the current source media and reach a first real dubbed preview.
- Why: The staged localization contract is now explicit, but the current run still pauses at the speaker/reference checkpoint without helping the operator bridge that gap.

## Scope

In scope:

- Extract candidate speaker-reference audio from the current source media after speaker labels exist.
- Store those candidates under item-managed voice-reference storage.
- Surface them in Localization Studio as reviewable/applicable candidates for the current voice plan.
- Let the operator continue the staged localization run after applying candidate references.
- Keep the feature additive and reviewable rather than silently overriding manual references.

Out of scope:

- Replacing the default voice backend family.
- Fully automatic final-quality cast curation without operator review.

## Acceptance criteria

- After diarization, Localization Studio can generate candidate reference bundles for detected speakers from the current item.
- Operators can apply those candidate bundles into the current speaker/voice plan without manual filesystem hunting.
- The first-dub path becomes practical enough that a normal operator can reach dub -> mix -> mux from the current item workflow.
- Existing manual multi-reference workflows remain intact.

## Test / verification plan

- Engine/Tauri tests for reference extraction, storage, and apply behavior.
- Installer-state-equivalent localization smoke updated to use the new assisted path.
- Proof bundle with generated candidate references, applied voice plan state, and resulting dubbed preview outputs.

## Status updates

- 2026-03-12: Created from `WP-0151` research findings as the next practical localization recovery step.
- 2026-03-12: Implemented item-scoped generated speaker-reference bundles in the engine/Tauri/frontend path, plus apply-as-append/replace flows inside Localization Studio.
- 2026-03-12: Verified with the staged `wp0150_localization_run_smoke` on the Queen sample after switching the smoke harness from manual reference injection to the new assisted-reference path.
- 2026-03-24: Follow-on reusable-voice basics remediation is now split into `WP-0155` to `WP-0159` so first-dub recovery, reusable-voice reuse, and clone-vs-fallback truth do not drift together under one broad packet.
