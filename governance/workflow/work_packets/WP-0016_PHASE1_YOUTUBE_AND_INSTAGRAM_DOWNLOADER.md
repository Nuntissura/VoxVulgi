# Work Packet: WP-0016 - Phase 1: YouTube + Instagram downloader

## Metadata
- ID: WP-0016
- Owner: Codex
- Status: DONE
- Created: 2026-02-21
- Target milestone: Phase 1 (MVP)

## Intent

- What: Support ingesting YouTube and Instagram links into the library.
- Why: Users need URL-based ingestion beyond direct file URLs, especially for educational clip workflows.

## Scope

In scope:

- YouTube download support via `yt-dlp`:
  - accept common YouTube URL forms (video/shorts/live, playlists, channels as applicable),
  - expand URLs into downloadable targets,
  - download to the configured download folder and import into `library_item`,
  - persist provenance (`ingest_provenance`) with provider identity.
- Instagram download support:
  - accept instagram.com post/reel/tv/story/profile URLs,
  - expand URLs into downloadable media targets where possible,
  - download to the configured download folder under an `instagram/` subfolder by default.
- UI + desktop integration:
  - Library UI inputs for YouTube via the existing URL batch ingest, plus a dedicated Instagram batch section.

Out of scope:

- Auth flows beyond an optional user-provided session cookie header.
- Persisting sensitive cookies/tokens long-term (should be avoided; see Risks).
- JavaScript-rendered scraping beyond what static HTML/API endpoints expose.

## Acceptance criteria

- Library UI can enqueue:
  - YouTube URLs through URL batch ingest,
  - Instagram URLs through an Instagram batch ingest form.
- Engine uses a provider selection layer to route:
  - YouTube URLs to a `yt-dlp` provider,
  - direct media asset URLs to direct HTTP.
- Downloaded artifacts are written under the configured download folder, and successful jobs import into the Library.
- Provenance is stored for each ingested item (provider + source URL + timestamp).
- Errors are clear and actionable when `yt-dlp` is unavailable or URLs cannot be expanded.

## Implementation notes

- Engine implementation is primarily in `product/engine/src/jobs.rs` with provider identifiers:
  - `direct_http_v1`
  - `youtube_yt_dlp_v1`
- Desktop wiring:
  - Tauri commands in `product/desktop/src-tauri/src/lib.rs`
  - Library UI in `product/desktop/src/pages/LibraryPage.tsx`

## Test / verification plan

- Enqueue:
  - one public YouTube URL,
  - one small Instagram post/reel URL,
  - one invalid URL scheme.
- Verify:
  - jobs show up as a batch in Jobs,
  - downloaded files are placed under the configured download folder,
  - library items + thumbnails are created on success,
  - failures include actionable messages and redacted logs.

## Risks / open questions

- Provider stability: `yt-dlp` and Instagram behaviors change frequently; treat as best-effort and expect breakage.
- Authentication: avoid persisting cookies/tokens at rest; ensure logs redact cookie values and URL IDs.
- Compliance: clarify "supported providers" policy and required user confirmations in the downloader design doc.

## Status updates

- 2026-02-21:
  - Backfilled Work Packet for already-implemented YouTube and Instagram download support.
  - Governance/spec updates tracked in `governance/spec/` to keep implementation and spec aligned.
