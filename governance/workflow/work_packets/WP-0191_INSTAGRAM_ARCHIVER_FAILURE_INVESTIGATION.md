# Work Packet: WP-0191 - Instagram Archiver Failure Investigation

## Metadata
- ID: WP-0191
- Owner: Codex
- Status: IN_PROGRESS
- Created: 2026-04-23
- Target milestone: Archive reliability

## Intent

- What: Inspect recent Instagram Archiver failures and identify whether failures are caused by yt-dlp extractor drift, browser-cookie/session handling, database locking, queue behavior, or unsupported Instagram profile/post inputs.
- Why: Recent app logs show repeated Instagram Archiver failures, including profile fetch errors and subscription heartbeat warnings. The failure modes need to be separated before making changes.

## Scope

In scope:
- Read recent app diagnostics traces and job logs for Instagram-related failures.
- Inspect Instagram subscription rows and queue state read-only.
- Classify failures by type, including:
  - `got more than 100 headers`
  - `Unable to extract data`
  - browser-cookie/session issues
  - `database is locked` heartbeat warnings
- Identify the exact code paths involved in Instagram direct downloads and subscription refresh.
- Recommend narrow follow-up implementation packets if fixes are needed.

Out of scope:
- Deleting or modifying Instagram subscription lists.
- Modifying third-party exports, browser cookie stores, or downloaded media.
- Shipping a code fix before the failure analysis is complete.
- Broad archive UI redesign.

## Acceptance criteria
- A short failure-analysis report exists under the WP proof/artifact path.
- The report includes recent failure examples with sensitive URLs/session material redacted.
- The report distinguishes app/database/queue issues from upstream yt-dlp/Instagram extractor issues.
- Follow-up fix WPs are proposed if implementation changes are needed.

## Status updates

- 2026-04-23: Created from recent Instagram Archiver failures and diagnostic-trace review.
- 2026-04-23: Operator smoke reproduced the one-shot profile failure path with `ERROR: [instagram:user] ... Unable to extract data`, while older failures in the same app data also show repeated `got more than 100 headers` extraction failures. The current failed one-shot jobs never create a `library_item`, which leaves Jobs without item/output context and makes root-path verification harder for operators.
