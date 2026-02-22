# Work Packet: WP-0017 - Phase 1: Downloader privacy hardening

## Metadata
- ID: WP-0017
- Owner:
- Status: BACKLOG
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
