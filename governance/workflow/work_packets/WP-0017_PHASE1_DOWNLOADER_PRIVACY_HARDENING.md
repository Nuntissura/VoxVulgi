# Work Packet: WP-0017 - Phase 1: Downloader privacy hardening

## Metadata
- ID: WP-0017
- Owner: Codex
- Status: DONE
- Created: 2026-02-21
- Target milestone: Phase 1 (MVP hardening)

## Intent

- What: Harden downloader UX and secrets handling (cookies/tokens/tool bootstrap disclosure).
- Why: Downloading is a high-risk surface; we must keep network egress explicit and avoid persisting sensitive auth material.

## Scope

In scope:

- Secrets handling:
  - do not persist raw cookie headers/tokens in durable job params or logs,
  - prefer short-lived temp files or OS credential storage for any needed auth.
- Disclosure + control:
  - make any use of `--cookies-from-browser` explicit in the UI,
  - make `yt-dlp` bootstrap/download explicit and user-controllable.
- Diagnostics visibility:
  - surface downloader tool availability/state (yt-dlp, cookie mode) and where data is stored.

Out of scope:

- Adding new provider integrations.
- Cloud download proxying.

## Acceptance criteria

- Cookie headers are never written to logs and are not persisted in `job.params_json`.
- Users can opt in/out of browser-cookie usage and tool bootstrapping.
- Diagnostics page documents any network egress paths for download features.

## Test / verification plan

- Enqueue a download job that requires auth; verify cookies are not present in DB and are redacted from logs.
- Verify toggles affect behavior deterministically (no silent fallback).

## Risks / open questions

- Cross-platform secure storage: decide on a keychain strategy for macOS/Windows/Linux.
- UX: balance friction vs safety; keep defaults conservative.

## Status updates

- 2026-02-22: Started implementation (remove cookie persistence from job params/logs; explicit yt-dlp install + browser-cookie toggle UX).
- 2026-02-22: Completed.
  - Cookies are not persisted in `job.params_json`; per-job cookie secrets are stored on disk outside the DB and removed at job start and during cancel/flush cleanup.
  - `yt-dlp` no longer auto-downloads during job execution; explicit install is exposed in Diagnostics and tool availability is visible.
  - Browser-cookie usage for `yt-dlp` is opt-in via explicit Library checkboxes.
  - Verified:
    - `cargo test --manifest-path product/engine/Cargo.toml --locked`
    - `cargo test --manifest-path product/desktop/src-tauri/Cargo.toml --locked`
    - `npm run build` (from `product/desktop/`)
