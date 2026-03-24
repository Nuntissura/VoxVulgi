# Work Packet: WP-0157 - Reusable voice capture and apply first-run flow

## Metadata
- ID: WP-0157
- Owner: Codex
- Status: BACKLOG
- Created: 2026-03-24
- Target milestone: Educational-core voice-clone recovery

## Intent

- What: Compress the reusable-voice operator path into one obvious first-run flow built around capture, save, apply, and dub, while keeping advanced surfaces available but secondary.
- Why: The current product surface is functionally rich but too fragmented across templates, cast packs, memory, character profiles, item plans, and benchmark/backend surfaces before the operator gets the basic reusable-cloning outcome.

## Scope

In scope:

- Design and implement one obvious reusable-voice lane inside Localization Studio:
  - capture reusable voice from current speaker,
  - save reusable voice,
  - apply saved voice to current or later item,
  - continue the translated dub.
- Keep advanced reusable-voice surfaces available, but demote them behind the basic lane.
- Make the chosen reusable voice and its next step obvious from the current-item workflow.

Out of scope:

- Replacing advanced reusable asset classes entirely.
- Deep backend benchmarking changes beyond what is needed for the first-run path.

## Acceptance criteria

- A normal operator can save a reusable voice from one item and apply it to another without needing to understand cast packs, memory, characters, or benchmark promotion first.
- The current item clearly shows which reusable voice asset is active and what to do next.
- Advanced reusable surfaces remain accessible without dominating the first-run cloning path.

## Test / verification plan

- Desktop app-boundary verification of capture -> save -> apply -> dub flow.
- Focused UI verification that the basic lane is discoverable from Localization Studio without external explanation.
- Proof bundle with screenshots/notes of the simplified operator path.

## Risks / open questions

- Over-compressing the path could hide important expert controls if the basic lane does not clearly hand off to advanced surfaces when needed.
- Existing reusable-voice labels may need renaming or grouping to avoid operator confusion.

## Status updates

- 2026-03-24: Created from the finding that reusable-voice power features now outnumber the basic educational-core path they are supposed to support.
