# Work Packet: WP-0106 - Pinterest archive crawler

## Metadata
- ID: WP-0106
- Owner: Codex
- Status: BACKLOG
- Created: 2026-03-07
- Target milestone: Workflow and archive UX hardening

## Intent

- What: Add Pinterest board/folder crawl support to Image Archive.
- Why: Downloading large Pinterest collections one by one is too time-consuming for the operator's archive workflow.

## Scope

In scope:

- Add Pinterest board/folder URL intake to the Image Archive crawler workflow.
- Support batch submission of Pinterest crawl targets.
- Prefer full-size image targets where possible and record crawl/download manifests for auditability.

Out of scope:

- Pinterest account management beyond what is required for local archive crawling.
- Non-image social providers in this WP.

## Acceptance criteria

- Image Archive accepts Pinterest board/folder URLs as crawl targets.
- Batch submission works for multiple Pinterest archive targets.
- Downloaded results are organized and auditable in the same style as existing crawler-based image archive flows.

## Test / verification plan

- Desktop build.
- Manual crawl smoke on representative Pinterest archive targets.

## Status updates

- 2026-03-07: Created from operator feedback requesting Pinterest folder/board crawl support in Image Archive.
