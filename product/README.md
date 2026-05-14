# Product

This folder contains the actual product code (UI + job engine + workers).

Specs and technical design docs live in `governance/spec/`.

## Layout

- `product/desktop/` - Tauri v2 + React desktop app (Windows/macOS; Linux later).
- `product/engine/` - Rust engine crate (models/jobs/db boundaries; called from the desktop shell).

## Dev (desktop)

From `product/desktop/`:

- `npm install`
- `npm run tauri dev`

## Build output policy (desktop)

From `product/desktop/`:

- Build with managed target folder: `npm run build:desktop:target`
- Default build validates and reuses the existing bundled offline payload when it matches the pinned dependency inputs.
- Refresh the bundled offline payload explicitly with `npm run build:desktop:target:refresh`.
- Force a clean dependency refresh with `npm run build:desktop:target:force-refresh`.
- Validate the current payload without bumping/building with `npm run build:desktop:payload:validate`.
- Legacy fast rebuild alias: `npm run build:desktop:target:no-prep` validates/reuses the existing payload and refuses stale or missing payloads.
- Build artifacts are written to `build_target/Current`
- Previous builds are archived to `build_target/old_versions`

## Runtime deps (FFmpeg + models)

The app installs runtime tools/models into its app-data folder (local-first; explicit action required).

- One-shot bootstrap (Windows dev): run `governance/scripts/bootstrap_dev.ps1`
- Or in-app: open **Diagnostics** and click **Install FFmpeg tools** and **Install** for `whispercpp-tiny`
- For YouTube links/channels/playlists, install `yt-dlp` (`winget install yt-dlp.yt-dlp` or `pip install -U yt-dlp`)

## Download folder behavior

- Default download folder: `downloads/` next to the running app executable.
- You can switch to any folder from **Library -> Download folder -> Choose folder**.
- If the configured folder is missing, the app shows the status inline so you can choose an existing folder or recreate the default folder without a modal interrupt.
- Default app-managed export roots under that folder are:
  - `video/` for general video downloads and playlist/channel output trees
  - `video/subscriptions/` for YouTube subscription folder maps
  - `instagram/` for Instagram archive downloads
  - `images/` for forum/blog image archive downloads
  - `localization/en/<media-stem>/` for exported SRT/VTT and dubbed preview MP4 files from Localization Studio

## Localization Studio output behavior

- Working artifacts stay in app-data under `derived/items/<item_id>/...` so jobs stay reproducible and easy to inspect.
- The mixed dubbed audio track is `derived/items/<item_id>/dub_preview/mix_dub_preview_v1.wav`.
- The muxed preview video defaults to `derived/items/<item_id>/dub_preview/mux_dub_preview_v1.mp4`.
- Exporting from Localization Studio copies user-facing deliverables into the app-managed localization export folder by default instead of writing next to the source media file.
- The export set can include an operator-selected source-media copy, and language-marked names use the source stem, for example `<source>.source.<ext>`, `<source>.sub-en.srt`, `<source>.sub-en.vtt`, and `<source>.dub-en.mp4`.
- The default yt-dlp preset now prefers MP4-compatible formats and requests MP4 merge/remux when the toolchain can do that without re-encoding.

## In-app image archive batch

- Open **Library -> Image archive (batch)**.
- Paste one or more blog/forum start URLs.
- Queue one job that crawls next pages and post/thread links, skips likely profile/avatar photos, and prefers full-size image URLs over thumbnail variants.
- Output goes under your selected download folder in `image_archive/<host>/images` by default (customizable subfolder).
- Job manifest and summary are written to job artifacts/logs and visible from **Jobs**.

## Image batch downloader (blog/forum)

- Script: `scripts/image_batch_downloader.py`
- Install dependencies: `python -m pip install requests beautifulsoup4`
- Example run:
  `python scripts/image_batch_downloader.py "https://example.com/blog" "https://example.com/forum" --output "./downloads/dad-images" --max-pages 1500`
- It crawls pagination and post/thread links, skips likely profile/avatar photos, and prefers full-size images over thumbnail variants.
- Use `--dry-run` first to validate what it will collect without writing files.
- Use `--skip-url-keyword` (repeatable) for site-specific exclusions, for example:
  `--skip-url-keyword avatar --skip-url-keyword profile --skip-url-keyword logo`
