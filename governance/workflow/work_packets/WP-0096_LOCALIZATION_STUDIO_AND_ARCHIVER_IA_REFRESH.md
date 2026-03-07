# Work Packet: WP-0096 - Localization Studio and archiver IA refresh

## Metadata
- ID: WP-0096
- Owner: Codex
- Status: BACKLOG
- Created: 2026-03-07
- Target milestone: Workflow and archive UX hardening

## Intent

- What: Refine the top-level workspace model so Localization Studio and the archive windows match the operator's actual workflow.
- Why: Localization Studio is the product's standout feature, but the current window naming and ingest split do not yet reflect how operators move from ingest to ASR and dubbing.

## Scope

In scope:

- Add a lightweight video-ingest block inside Localization Studio for local import/refresh and ASR language selection (`auto` plus explicit language choices).
- Rename `Video Ingest` to `Video Archiver` and make it the home for URL ingest, download presets/templates, subscription groups, and YouTube subscriptions.
- Rename `Instagram Archive` to `Instagram Archiver`.
- Update navigation labels, window descriptions, and related operator guidance to reflect the refined workspace model.

Out of scope:

- New downloader providers.
- Subscription engine changes beyond what is needed for the IA refresh.

## Acceptance criteria

- Localization Studio exposes an ingest/import block appropriate for the subtitle and dubbing workflow.
- `Video Archiver` replaces `Video Ingest` consistently across navigation and operator-facing copy.
- `Instagram Archiver` replaces `Instagram Archive` consistently across navigation and operator-facing copy.
- The archive/localization split remains clear and no existing ingest capability becomes harder to discover.

## Test / verification plan

- Desktop build.
- Manual UI smoke covering navigation labels and the new Localization Studio ingest block.

## Status updates

- 2026-03-07: Created from operator feedback on workspace naming, window responsibilities, and the need for Localization Studio to own the first ingest step for dubbing workflows.
