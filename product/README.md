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

## Runtime deps (FFmpeg + models)

The app installs runtime tools/models into its app-data folder (local-first; explicit action required).

- One-shot bootstrap (Windows dev): run `governance/scripts/bootstrap_dev.ps1`
- Or in-app: open **Diagnostics** and click **Install FFmpeg tools** and **Install** for `whispercpp-tiny`
- For YouTube links/channels/playlists, install `yt-dlp` (`winget install yt-dlp.yt-dlp` or `pip install -U yt-dlp`)

## Download folder behavior

- Default download folder: `downloads/` next to the running app executable.
- You can switch to any folder from **Library -> Download folder -> Choose folder**.
- If the configured folder is missing, the app prompts you to choose an existing folder or create the default one.

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
  `python scripts/image_batch_downloader.py "https://example.com/blog" "https://example.com/forum" --output "P:\YT fetch\downloads\dad-images" --max-pages 1500`
- It crawls pagination and post/thread links, skips likely profile/avatar photos, and prefers full-size images over thumbnail variants.
- Use `--dry-run` first to validate what it will collect without writing files.
- Use `--skip-url-keyword` (repeatable) for site-specific exclusions, for example:
  `--skip-url-keyword avatar --skip-url-keyword profile --skip-url-keyword logo`
