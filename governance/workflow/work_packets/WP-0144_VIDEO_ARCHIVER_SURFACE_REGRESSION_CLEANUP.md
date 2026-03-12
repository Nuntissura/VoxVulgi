# Work Packet: WP-0144 - Video Archiver surface regression cleanup

## Metadata
- ID: WP-0144
- Owner: Codex
- Status: IN_PROGRESS
- Created: 2026-03-12
- Target milestone: Post-0.1.6 smoke regression recovery

## Intent

- What: Remove leftover Localization Studio controls from Video Archiver and make the archiver surface match its intended scope again.
- Why: The current smoke still shows Localization ingest controls leaking into Video Archiver, which breaks the IA split and confuses operators about where to start each workflow.

## Scope

In scope:

- Remove leftover localization ingest/ASR controls from Video Archiver.
- Reconfirm path/panel ownership inside Video Archiver after the split from Localization Studio.
- Tighten the Video Archiver panel contract so archive-only flows remain visible without cross-workflow clutter.

Out of scope:

- Downloader/runtime repairs themselves.
- Broad redesign of the full application navigation.

## Acceptance criteria

- Video Archiver no longer shows Localization Studio ingest controls.
- Video Archiver surfaces only archive-relevant controls and path information.
- The split between Localization Studio and Video Archiver is obvious in normal operator use.

## Test / verification plan

- Focused desktop smoke on Video Archiver after the UI cleanup.
- Desktop build verification.
- Proof bundle with before/after operator-surface summary.

## Status updates

- 2026-03-12: Created from smoke finding `ST-013`.
- 2026-03-12: Cut the shared import-control gate so Media Library keeps the import/ASR block but Video Archiver no longer renders the Localization Studio ingest controls; awaiting operator smoke on the cleaned archive surface.
