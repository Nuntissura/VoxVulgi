# Work Packet: WP-0008 — Phase 1: Library DB + import pipeline

## Metadata
- ID: WP-0008
- Owner: Codex
- Status: DONE
- Created: 2026-02-19
- Target milestone: Phase 1 (MVP)

## Intent

- What: Implement the library database, storage layout, and local import flow.
- Why: Everything else (jobs, captions, translate, dubbing later) needs a stable media + metadata foundation.

## Scope

In scope:

- Create SQLite schema for:
  - library items
  - tags and smart tags (v1 placeholders)
  - subtitle track references
  - jobs (minimal; expanded in WP-0009)
- Implement local import:
  - select files
  - run ffprobe to extract metadata
  - generate thumbnails
  - persist to DB and derived folder structure

Out of scope:

- Downloader providers (interface design in WP-0003; implementation later).

## Acceptance criteria

- A user can import a local media file and see it in the library list.
- Metadata (duration/resolution/codecs) is populated via ffprobe.
- Thumbnail generation produces a stable derived artifact per item.

## Implementation notes

- Follow the storage layout described in `governance/spec/TECHNICAL_DESIGN.md`.

## Test / verification plan

- Import a known media file and confirm:
  - DB entry exists
  - derived artifacts created
  - re-opening the app shows the item

## Risks / open questions

- Handling moved/deleted source files (broken pointers) gracefully.

## Status updates

- 2026-02-19: Added SQLite schema + library import/list commands and Library UI. Import uses ffprobe/ffmpeg (installable via Diagnostics -> Tools).
