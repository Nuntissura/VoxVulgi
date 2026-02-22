# Work Packet: WP-0014 - Phase 1: Batch URL ingest

## Metadata
- ID: WP-0014
- Owner: Codex
- Status: DONE
- Created: 2026-02-19
- Target milestone: Phase 1 (MVP)

## Intent

- What: Add a URL ingest path with batch enqueue support.
- Why: Language learners need faster ingestion of media beyond local file picking.

## Scope

In scope:

- Implement a new ingest mode for direct media URLs where:
  - one or more URLs can be submitted in one action (batch)
- Queue-backed execution:
  - one job per URL
  - download artifact to app library storage
  - import into `library_item` with source provenance fields
- Guardrails in UI/engine:
  - reject unsupported URL schemes

Out of scope:

- Site-specific extractors/adapters.
- Authentication/cookie-based fetching.

## Acceptance criteria

- Library UI can submit a batch of URLs.
- Engine enqueues one URL-download job per URL.
- Successful jobs create `library_item` rows and thumbnails like local import.
- Failed jobs surface clear errors and leave logs/artifacts in normal job locations.

## Implementation notes

- Start with a strict direct-file fetcher (HTTP/HTTPS only) and treat broader provider adapters as future work.
- Store original URL as provenance in existing source fields.
- Keep logs redacted-by-default where possible.

## Test / verification plan

- Enqueue two valid direct media URLs and one invalid URL, verify mixed success/failure behavior.
- Verify Task Queue and Library update behavior after completion.

## Risks / open questions

- Some servers block non-browser clients or require tokens/cookies; this should fail clearly with an explanatory error.
- Storage impact for large batch jobs.

## Status updates

- 2026-02-19:
  - Activated work packet.
  - Implemented `download_direct_url` job type and batch enqueue API with strict guardrails:
    - HTTP/HTTPS URLs only
    - batch size cap + URL dedupe
  - Added downloader import path into library + provenance persistence (`ingest_provenance` table).
  - Added Library UI batch form and Tauri command `jobs_enqueue_download_batch`.
  - Verification completed:
    - `cargo test --manifest-path product/engine/Cargo.toml`
    - `cargo check --manifest-path product/desktop/src-tauri/Cargo.toml`
    - `npm -C product/desktop run build`
