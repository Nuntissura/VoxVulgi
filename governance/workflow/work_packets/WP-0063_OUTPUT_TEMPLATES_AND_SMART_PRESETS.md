# Work Packet: WP-0063 - Output folder/file templates + “Smart presets” (feature parity)

## Metadata
- ID: WP-0063
- Owner: Codex
- Status: BACKLOG
- Created: 2026-02-28
- Target milestone: Phase 1 (downloader UX parity)

## Intent

- What: Add output path templates and reusable download presets (similar to “Smart Mode” behavior) for consistent foldering and naming.
- Why: Users with large libraries depend on predictable, stable folder structures (per channel/playlist) and consistent download settings.

## Scope

In scope:

- Define a template system for output paths and filenames (sanitized for filesystem safety), supporting variables like:
  - `{provider}`, `{channel}`, `{playlist}`, `{upload_date}`, `{title}`, `{id}`.
- Add a “preset” model:
  - default preset for ad-hoc URL ingest,
  - optional per-subscription override preset,
  - settings include format/quality preferences and subtitle options (as supported by the provider).
- UI:
  - create/edit/select presets,
  - set a default preset,
  - override preset per subscription.

Out of scope:

- Full yt-dlp option surface replication.
- Complex per-site rules (separate WP if needed).

## Acceptance criteria

- Downloads land in the expected folder structure without manual re-sorting.
- Naming is stable and includes stable identifiers to avoid duplicates on title changes.
- Presets can be exported/imported (portable) without deleting existing subscriptions.

## Test / verification plan

- Unit tests for template rendering + sanitization.
- `cargo test` in `product/engine`
- `npm -C product/desktop run build`
- Manual smoke:
  - define preset + template,
  - download sample URLs,
  - verify outputs match template.

## Status updates

- 2026-02-28: Created.

