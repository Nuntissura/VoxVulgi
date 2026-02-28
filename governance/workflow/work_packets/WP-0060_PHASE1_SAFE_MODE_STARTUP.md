# Work Packet: WP-0060 - Phase 1: Safe Mode startup (non-destructive recovery mode)

## Metadata
- ID: WP-0060
- Owner: Codex
- Status: BACKLOG
- Created: 2026-02-28
- Target milestone: Phase 1 (downloader UX hardening)

## Intent

- What: Add a **Safe Mode** boot path so VoxVulgi can always start reliably even with very large libraries/subscription sets or provider regressions.
- Why: Third-party downloader apps can freeze/crash under large datasets; Safe Mode ensures the user can still access/export/manage their subscription lists and library metadata without risk.

## Scope

In scope:

- Add a persistent `safe_mode` flag in app config (default: off).
- Add a launch entrypoint:
  - CLI arg `--safe-mode` (or equivalent) and/or a UI action “Restart in Safe Mode”.
- Safe Mode behavior (minimum viable):
  - do **not** auto-queue subscription refresh on startup,
  - avoid running expensive background scans at boot (best-effort defer),
  - show a clear banner “Safe Mode is ON” with an explanation and a way to exit.
- Ensure **non-destructive** operations by default (no list/subscription deletion; no DB resets).

Out of scope:

- General performance optimizations unrelated to startup safety (separate WP).
- New downloader providers.

## Acceptance criteria

- App can start in Safe Mode and render the Library + Subscriptions UI without triggering auto-refresh.
- User can export/import subscriptions and view existing library items in Safe Mode.
- No destructive actions are performed automatically in Safe Mode.

## Test / verification plan

- `cargo test` in `product/engine`
- `cargo test` in `product/desktop/src-tauri`
- `npm -C product/desktop run build`
- Manual smoke:
  - toggle Safe Mode on,
  - restart,
  - confirm subscriptions are not auto-queued.

## Status updates

- 2026-02-28: Created.

