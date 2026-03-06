# Work Packet: WP-0074 - Localization output clarity, MP4 defaults, thumbnails, and responsiveness

## Metadata
- ID: WP-0074
- Owner: Codex
- Status: DONE
- Created: 2026-03-06
- Target milestone: Stabilization sprint (localization UX hardening)

## Intent

- What: Make localized outputs easy to find, make downloaded/muxed video default to MP4, repair missing thumbnails, and remove remaining first-open freezes.
- Why: Core educational workflows are unclear or unreliable today: users cannot consistently find SRT/dub/video outputs, downloader defaults are not MP4-first, stale thumbnails silently fail, and Diagnostics can still stall on first entry.

## Scope

In scope:

- Change default user-facing localization exports from "next to source" to a predictable app-managed export map under the configured download root.
- Clarify where working artifacts live versus where exported deliverables live.
- Prefer MP4 for downloaded video outputs and keep mux-preview exports MP4-first by default.
- Add direct open/reveal actions for source files and exported outputs where applicable.
- Repair thumbnail failures caused by stale/missing cached files with best-effort regeneration and safer UI fallback.
- Remove repeated missing-download-folder prompts caused by pane switches/remounts.
- Reduce remaining Diagnostics entry stalls by moving blocking status work off the UI-critical path.
- Add a concise in-app step-by-step guide for producing a first dubbed video.

Out of scope:

- New downloader providers or crawler features.
- Replacing the current voice-preserving dubbing stack in this WP.
- Cloud dubbing providers or any workflow that weakens local-first/privacy defaults.

## Acceptance criteria

- Localization Studio makes it obvious where SRT, dubbed audio, preview MP4, and export-pack outputs go.
- Default exported localization outputs land under a predictable app-managed folder tree rooted at the configured download directory.
- Default downloaded video outputs are MP4 whenever the source/toolchain allows normal yt-dlp remux/merge behavior.
- Users can reveal/open downloaded media and exported outputs directly from the app.
- Broken/stale thumbnail references recover automatically or fall back cleanly without persistent broken-image tiles.
- Switching between windows no longer re-triggers missing-download-folder error prompts.
- Opening Diagnostics remains interactive while heavy checks continue loading.

## Test / verification plan

- Manual smoke with sample Korean/Japanese clips and the existing dubbing pipeline:
  - confirm source subtitle outputs,
  - confirm dubbed-audio and mux-preview locations,
  - confirm export folder layout,
  - confirm open/reveal actions.
- Manual smoke for thumbnail recovery with missing cached thumbnail files.
- Manual smoke for Diagnostics first-open responsiveness.
- `cargo test` in `product/engine`
- `cargo test` in `product/desktop/src-tauri`
- `npm -C product/desktop run build`

## Status updates

- 2026-03-06: Created to address output discoverability, MP4 defaults, thumbnail regressions, repeated folder prompts, and remaining Diagnostics hitching.
- 2026-03-06: Implemented predictable Localization Studio exports under the download-root `localization/en/<media-stem>/` map, added source/open actions and first-dub guidance, switched yt-dlp defaults to prefer MP4 merge/remux, replaced fragile thumbnail file URLs with self-healing thumbnail data loading, removed repeated missing-download-folder prompts, and moved remaining Diagnostics status/trace work onto non-blocking async paths. Verified with `npm -C product/desktop run build`, `cargo test` in `product/engine`, and `cargo test` in `product/desktop/src-tauri`.
- 2026-03-06: Manual end-to-end smoke succeeded on `Test material\[4K] Queen is here 😍 Miyeon so cute 💕 (ENG SUB).mp4` via `cargo run --example wp0029_smoke`; proof artifacts written under `product/desktop/Build Target/tool_artifacts/wp_runs/WP-0074/manual_smoke_queen/` including `smoke_run.log`, `smoke_summary.md`, `ffprobe_mux_preview.json`, and deliverables (`queen_dub_preview.mp4`, `queen_dub_preview.wav`, `queen_en.srt`, `queen_en.vtt`).
