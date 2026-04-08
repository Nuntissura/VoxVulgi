# Work Packet: WP-0162 - Global Authentication and Sessions

## Metadata
- ID: WP-0162
- Owner: Codex
- Status: BACKLOG
- Created: 2026-04-08
- Target milestone: Architecture

## Intent

- What: Decouple YouTube authentication from the URL archiver input field, centralizing session cookie management into a global Options/Diagnostics setting that is honored by all downstream jobs.
- Why: The current UX creates ambiguity about whether background subscriptions or retries actually use the pasted cookie. Authentication is a global context, not a per-URL parameter.

## Scope

In scope:
- Create an Authentication & Sessions UI pane inside Options/Diagnostics.
- Store multi-cookie JSON arrays (Netscape format / browser export) in global atomic config.
- Refactor the Rust engine so that all YouTube ingestion/refresh tasks pull session material from global config rather than requiring per-action parameters.

## Acceptance criteria
- Cookie material (like the provided Netscape JSON array) can be saved once.
- Subscriptions, manual URLs, and retry jobs all transparently read the global YouTube session cookie on execution.
- If the cookie expires, the error is elevated to the Diagnostics status level.

## Test / verification plan
- Engine integration test for parsing the JSON array provided by browser extensions.
- Manual smoke of an authenticated download using only global auth.

## Status updates

- 2026-04-08: Created from operator evaluation session. Included tracking for the browser JSON array payload format.
