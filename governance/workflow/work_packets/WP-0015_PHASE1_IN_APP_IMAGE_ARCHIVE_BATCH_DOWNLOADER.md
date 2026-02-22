# Work Packet: WP-0015 - Phase 1: In-app image archive batch downloader

## Metadata
- ID: WP-0015
- Owner: Codex
- Status: DONE
- Created: 2026-02-20
- Target milestone: Phase 1 (MVP)

## Intent

- What: Add an in-app batch image downloader for blog/forum archives.
- Why: User needs to preserve a large personal media archive by downloading full images across many pages in one queued job.

## Scope

In scope:

- Add a new queue job type for image-archive crawling in the Rust engine.
- Add a Tauri command and Library UI controls to queue the image batch job.
- Crawl behavior:
  - multiple start URLs,
  - pagination and post/thread link traversal,
  - skip likely profile/avatar images,
  - prefer full-size image candidates over thumbnails.
- Persist outputs:
  - downloaded files under selected download folder (default `image_archive/<host>/images`),
  - manifest CSV and summary artifacts for auditability.

Out of scope:

- Authenticated crawling with account login/cookies.
- JavaScript-rendered infinite-scroll scraping beyond static HTML discovery.

## Acceptance criteria

- Library has an "Image archive (batch)" section that queues the job.
- New job type appears in Jobs and can be canceled/retried like other jobs.
- Crawler follows pagination/content links and downloads images to configured download folder.
- Profile/avatar images are filtered by heuristics.
- Thumbnail variants are de-prioritized in favor of full-size candidates.
- Manifest and summary artifacts are written per run.

## Implementation notes

- Engine module added: `product/engine/src/image_batch.rs`.
- Job integration in runner: `product/engine/src/jobs.rs` (`download_image_batch`).
- Tauri command: `jobs_enqueue_image_batch` in `product/desktop/src-tauri/src/lib.rs`.
- UI section added to Library page: `product/desktop/src/pages/LibraryPage.tsx`.
- Product docs updated: `product/README.md` and governance task/spec docs.

## Test / verification plan

- Engine tests:
  - `cargo test --manifest-path product/engine/Cargo.toml`
- Desktop backend compile check:
  - `cargo check --manifest-path product/desktop/src-tauri/Cargo.toml`
- Desktop frontend production build:
  - `npm -C product/desktop run build`
- Packaging:
  - `npm -C product/desktop run tauri build`
  - verify the packaged executable refreshed.

## Risks / open questions

- Some sites block non-browser crawlers or require session auth; these should fail clearly.
- Highly dynamic sites may hide links/images from static HTML crawl.
- Heuristic filtering can miss edge cases; keyword tuning may be needed per site.

## Status updates

- 2026-02-20:
  - Implemented new image batch crawl engine and queue job integration.
  - Added in-app Library form and Tauri command for image batch enqueue.
  - Added manifest/summary outputs and job log events.
  - Verified with engine tests, desktop checks/build, and packaged fresh `.exe`.
