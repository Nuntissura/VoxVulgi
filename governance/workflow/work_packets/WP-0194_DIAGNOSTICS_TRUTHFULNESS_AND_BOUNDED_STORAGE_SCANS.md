# Work Packet: WP-0194 - Diagnostics truthfulness and bounded storage scans

## Metadata
- ID: WP-0194
- Owner: Codex
- Status: BACKLOG
- Created: 2026-04-23
- Target milestone: Desktop diagnostics reliability

## Intent

- What: Make Diagnostics sections settle truthfully after startup and avoid effectively unbounded storage scans on large or NAS-backed roots.
- Why: Operator smoke showed Diagnostics still reporting Tools/Storage as loading long after startup completed, while startup traces indicate hydration already reached `ready` and tool binaries such as FFmpeg exist on disk.

## Scope

In scope:
- Audit why Diagnostics sections can remain stuck in `loading` after successful startup.
- Ensure tool-state sections reflect current installed binaries after offline hydration.
- Bound or stage storage scans so large archive roots do not leave the dashboard effectively unfinished.
- Keep summary tiles and section states consistent with the underlying backend truth.

Out of scope:
- New diagnostics feature areas unrelated to truthfulness or bounded completion.
- Deep storage analytics beyond what is needed for truthful section completion.

## Acceptance criteria
- Diagnostics no longer reports FFmpeg missing when the bundled binaries are already present.
- Tools and Storage sections eventually settle to `ready` or `failed` instead of effectively loading forever.
- Storage accounting uses a bounded strategy suitable for large/NAS-backed roots.
- Focused verification covers startup traces plus in-app Diagnostics behavior.

## Test / verification plan

- Compare startup traces, on-disk tool presence, and Diagnostics UI states.
- Add focused verification for tool-state and storage-section completion paths.
- Re-run desktop build verification after changes land.

## Status updates

- 2026-04-23: Created after smoke reported Tools/Storage still loading long after startup, while trace evidence showed offline bundle hydration already completed successfully.
