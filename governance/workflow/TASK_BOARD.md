# VoxVulgi — Task Board

Last updated: 2026-02-22

This is the single source of truth for work status.

## Status legend

- `BACKLOG` — not started
- `IN_PROGRESS` — actively being worked on
- `BLOCKED` — waiting on external input
- `DONE` — completed + verified (per WP acceptance)

## Work packets

| ID | Title | Status | Owner | Notes |
|---|---|---|---|---|
| WP-0001 | Repo structure: product + governance | DONE | Codex | Repo skeleton + operating docs in place; added reverse-build sanitization script. |
| WP-0002 | Product spec + technical design v0 | DONE | Codex | Drafted specs for cross-platform, local-first vNext. |
| WP-0003 | Downloader design | DONE | Codex | Added provider interface + provenance + UX/safety requirements to `governance/spec/TECHNICAL_DESIGN.md`; follow-up hardening tracked separately. |
| WP-0004 | Voice-preserving dubbing R&D plan | BACKLOG | | Define pipeline, consent gates, and evaluation harness for JA/KO->EN. |
| WP-0005 | Diagnostics + log retention policy | BACKLOG | | Design storage layout, redaction, rotation, export bundles. |
| WP-0006 | Phase 1: Product app skeleton | DONE | Codex | Tauri v2 + React app skeleton created under `product/desktop/`; Rust engine boundary in `product/engine/`; build verified. |
| WP-0007 | Phase 1: Local model runtime + model manager | DONE | Codex | Implemented bundled model manifest + hash verification and Diagnostics inventory (local-first; no default cloud). |
| WP-0008 | Phase 1: Library DB + import pipeline | DONE | Codex | SQLite schema + local import (ffprobe metadata + ffmpeg thumbnail) wired into Library UI. |
| WP-0009 | Phase 1: Jobs engine + queue UI | DONE | Codex | Durable jobs + progress UI; cancel/retry; per-job logs/artifacts; log rotation + retention defaults; build + tests verified. |
| WP-0010 | Phase 1: Local ASR (JA/KO) -> captions | DONE | Codex | `asr_local` job (ffmpeg audio extract -> local whisper.cpp -> `source.json/srt/vtt` per item) + model install via Diagnostics; build verified. |
| WP-0011 | Phase 1: Subtitle editor v1 | DONE | Codex | Subtitle editor UI (preview + edit/split/merge/nudge/normalize) + versioned saves (new `subtitle_track` row + new files) + SRT/VTT export; build verified. |
| WP-0012 | Phase 1: Local translate CC (JA/KO -> EN) | DONE | Codex | `translate_local` job (Whisper.cpp translate + alignment), glossary + QC warnings report, minimal bilingual editor view; build verified. |
| WP-0013 | Phase 1: Diagnostics UI + retention tools | IN_PROGRESS | Codex | Storage breakdown, log rotation, export diagnostics bundle, cleanup actions. |
| WP-0014 | Phase 1: Batch URL ingest | DONE | Codex | Added direct-URL batch ingest (up to 1500 entries), queue jobs + provenance table, Library form, and build/test verification. |
| WP-0015 | Phase 1: In-app image archive batch downloader | DONE | Codex | Added in-app blog/forum image batch crawl job (pagination + content links, profile/avatar skip, full-image preference), new Library form + Tauri command, and build/test verification. |
| WP-0016 | Phase 1: YouTube + Instagram downloader | DONE | Codex | Added `yt-dlp` YouTube ingest + Instagram batch ingest (expand to media targets) on top of URL ingest pipeline; backfilled WP + spec sync. |
| WP-0017 | Phase 1: Downloader privacy hardening | BACKLOG | | Tool bootstrap disclosure + avoid persisting cookies in job params/logs. |
| WP-0018 | Rebrand to VoxVulgi | DONE | Codex | Renamed project branding, Tauri bundle identifiers, and bootstrap tooling names. |
| WP-0019 | Fix mojibake encoding artifacts | DONE | Codex | Replaced mis-encoded punctuation sequences (e.g., `â€”`, `â€œ`) in docs/UI; kept Rust markers ASCII via Unicode escapes. |
