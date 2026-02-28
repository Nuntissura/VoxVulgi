# Work Packet: WP-0013 — Phase 1: Diagnostics UI + retention tools

## Metadata
- ID: WP-0013
- Owner: Codex
- Status: DONE
- Created: 2026-02-19
- Target milestone: Phase 1 (MVP)

## Intent

- What: Implement the Diagnostics page and safe cleanup/export tools.
- Why: Local-first apps still need transparency (storage, models, logs) and safe support bundles.

## Scope

In scope:

- Implement a Diagnostics UI that shows:
  - app version/build
  - ffmpeg version
  - installed model inventory + sizes (from WP-0007)
  - storage breakdown (library/derived/cache/logs/db)
  - recent job failures with copy/export
- Implement retention tools:
  - log rotation + caps (per WP-0005)
  - cache cleanup (without deleting library media)
  - "export diagnostics bundle" (safe-by-default redactions)

Out of scope:

- Any always-on telemetry (must remain opt-in).

## Acceptance criteria

- Logs do not grow unbounded (caps are enforced).
- User can export a diagnostics bundle that is safe to share by default.
- User can clear cache and see reclaimed space without losing library items.

## Implementation notes

- Ensure any outbound network calls are visible and controllable (even if the default is "none").

## Test / verification plan

- Generate enough logs to trigger rotation and verify old logs are removed/compressed per policy.
- Export a diagnostics bundle and confirm it excludes sensitive tokens/cookies/PII by default.

## Risks / open questions

- Avoid accidentally including media content in diagnostics bundles.

## Status updates

- 2026-02-22: Started implementation (Diagnostics UI: storage breakdown, failures view, export bundle, cleanup actions).
- 2026-02-22: Implemented Diagnostics UI (build info, FFmpeg versions, model inventory, storage breakdown, recent failures) + retention/export tools (clear cache, prune job logs, export redacted diagnostics bundle).
- 2026-02-22: Verified via `cargo test` (engine + desktop) and `npm run build`; added unit tests for log pruning + bundle redaction.
