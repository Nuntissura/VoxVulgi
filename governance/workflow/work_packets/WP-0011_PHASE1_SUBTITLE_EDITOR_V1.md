# Work Packet: WP-0011 — Phase 1: Subtitle editor v1

## Metadata
- ID: WP-0011
- Owner: Codex
- Status: DONE
- Created: 2026-02-19
- Target milestone: Phase 1 (MVP)

## Intent

- What: Provide a basic subtitle editor for review and correction of AI output.
- Why: The product must be editable and transparent; AI output is never perfect.

## Scope

In scope:

- Implement a subtitle editor UI that supports:
  - segment list + media preview
  - edit text
  - split/merge segments
  - time nudge / reflow
  - export SRT/VTT

Out of scope:

- Advanced timeline editing (keyframes, waveform editing) beyond MVP.

## Acceptance criteria

- User can open the generated subtitle track, edit it, save it, and re-export SRT/VTT.
- Edits are persisted as a new version of the subtitle track (no silent overwrite).

## Implementation notes

- Design the underlying subtitle JSON so it can support speaker labels later (Phase 2).

## Test / verification plan

- Edit a subtitle file and verify:
  - changes persist after restart
  - exported SRT/VTT matches the edits

## Risks / open questions

- Versioning strategy for subtitle edits (simple "v1/v2" vs full history).

## Status updates

- 2026-02-19:
  - Added subtitle editor UI (media preview + segment table) with:
    - text editing, split, merge-next, time nudge, normalize/reflow
    - export current document to SRT/VTT
  - Implemented versioned saves:
    - saving creates a new `subtitle_track` row (incremented `version`)
    - writes new files next to the base track path (e.g. `source.v2.json`, `source.v2.srt`, `source.v2.vtt`)
    - original files are not overwritten
