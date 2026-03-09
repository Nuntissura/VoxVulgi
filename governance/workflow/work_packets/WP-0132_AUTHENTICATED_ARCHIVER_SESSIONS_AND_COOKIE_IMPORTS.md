# Work Packet: WP-0132 - Authenticated archiver sessions and cookie imports

## Metadata
- ID: WP-0132
- Owner: Codex
- Status: BACKLOG
- Created: 2026-03-09
- Target milestone: Installer smoke remediation tranche

## Intent

- What: Harden authenticated YouTube and Instagram archiver flows with explicit session inputs that match what the UI promises.
- Why: Current smoke results show that login-required downloads are broken, the UI advertises JSON cookie support that the engine does not implement, and browser-cookie fallback can fail noisily on locked Chrome profiles.

## Scope

In scope:

- Accept cookie header strings, browser-export JSON cookie blobs, Netscape cookie files, and explicit cookie-file paths.
- Reuse session inputs across one-shot archiver batches and saved Instagram subscriptions where configured.
- Improve browser-cookie fallback handling and operator messaging.
- Surface authenticated-session inputs near the top of archiver panels instead of burying them.

Out of scope:

- Silent background browser scraping or auto-capture of credentials.

## Acceptance criteria

- Pasted browser-export JSON cookies are parsed correctly into yt-dlp-compatible cookie files.
- Invalid or unavailable browser-cookie sources fail clearly without masking explicit cookie inputs.
- Instagram and YouTube login-required flows can be configured through explicit operator session inputs.

## Test / verification plan

- Engine tests for cookie parsing and cookie-file generation.
- Desktop build plus focused app-boundary smoke on session-required archiver paths.
- Proof bundle with successful authenticated extraction evidence and failure-path coverage.

## Status updates

- 2026-03-09: Created from installer smoke findings `ST-010` and `ST-011`.
