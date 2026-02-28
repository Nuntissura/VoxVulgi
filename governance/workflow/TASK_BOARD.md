# VoxVulgi — Task Board

Last updated: 2026-02-28

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
| WP-0004 | Voice-preserving dubbing R&D plan | DONE | Codex | R&D plan documented in `governance/spec/VOICE_PRESERVING_DUBBING_RD_PLAN.md`. |
| WP-0005 | Diagnostics + log retention policy | DONE | Codex | Reconciled: policy documented in `governance/spec/TECHNICAL_DESIGN.md` section 6; safe-by-default diagnostics export with redaction. |
| WP-0006 | Phase 1: Product app skeleton | DONE | Codex | Tauri v2 + React app skeleton created under `product/desktop/`; Rust engine boundary in `product/engine/`; build verified. |
| WP-0007 | Phase 1: Local model runtime + model manager | DONE | Codex | Implemented bundled model manifest + hash verification and Diagnostics inventory (local-first; no default cloud). |
| WP-0008 | Phase 1: Library DB + import pipeline | DONE | Codex | SQLite schema + local import (ffprobe metadata + ffmpeg thumbnail) wired into Library UI. |
| WP-0009 | Phase 1: Jobs engine + queue UI | DONE | Codex | Durable jobs + progress UI; cancel/retry; per-job logs/artifacts; log rotation + retention defaults; build + tests verified. |
| WP-0010 | Phase 1: Local ASR (JA/KO) -> captions | DONE | Codex | `asr_local` job (ffmpeg audio extract -> local whisper.cpp -> `source.json/srt/vtt` per item) + model install via Diagnostics; build verified. |
| WP-0011 | Phase 1: Subtitle editor v1 | DONE | Codex | Subtitle editor UI (preview + edit/split/merge/nudge/normalize) + versioned saves (new `subtitle_track` row + new files) + SRT/VTT export; build verified. |
| WP-0012 | Phase 1: Local translate CC (JA/KO -> EN) | DONE | Codex | `translate_local` job (Whisper.cpp translate + alignment), glossary + QC warnings report, minimal bilingual editor view; build verified. |
| WP-0013 | Phase 1: Diagnostics UI + retention tools | DONE | Codex | Diagnostics UI + retention/export tools implemented; verified via `cargo test` + `npm run build`. |
| WP-0014 | Phase 1: Batch URL ingest | DONE | Codex | Added direct-URL batch ingest (up to 1500 entries), queue jobs + provenance table, Library form, and build/test verification. |
| WP-0015 | Phase 1: In-app image archive batch downloader | DONE | Codex | Added in-app blog/forum image batch crawl job (pagination + content links, profile/avatar skip, full-image preference), new Library form + Tauri command, and build/test verification. |
| WP-0016 | Phase 1: YouTube + Instagram downloader | DONE | Codex | Added `yt-dlp` YouTube ingest + Instagram batch ingest (expand to media targets) on top of URL ingest pipeline; backfilled WP + spec sync. |
| WP-0017 | Phase 1: Downloader privacy hardening | DONE | Codex | Verified: engine+tauri `cargo test` + desktop `npm run build`; explicit yt-dlp install + cookies not persisted in job params. |
| WP-0018 | Rebrand to VoxVulgi | DONE | Codex | Renamed project branding, Tauri bundle identifiers, and bootstrap tooling names. |
| WP-0020 | Voice dubbing tooling landscape research (2026) | DONE | Codex | Research local OSS vs service-backed tools; license/weights matrix; recommend default + BYO stacks for Phase 2. |
| WP-0021 | Phase 2: Python toolchain runner + explicit installers | DONE | Codex | Verified: engine+tauri `cargo test` + desktop `npm run build`; Python venv setup/status surfaced in Diagnostics. |
| WP-0022 | Phase 2: Separation (Spleeter pack + job) | DONE | Codex | Explicit-install Spleeter pack + `separate_audio_spleeter` job to produce vocals/background stems. |
| WP-0023 | Phase 2: Diarization baseline (pack + job) | DONE | Codex | Explicit-install diarization pack + `diarize_local_v1` job to add speaker labels to subtitle tracks. |
| WP-0024 | Phase 2: TTS preview (pack + job) | DONE | Codex | Explicit-install TTS preview pack + `tts_preview_pyttsx3_v1` job to synthesize per-segment audio + manifest. |
| WP-0025 | Phase 2: Dub preview mix + export | DONE | Codex | `mix_dub_preview_v1` job overlays TTS segments on separation background stem to produce a preview dub WAV. |
| WP-0026 | Phase 2: Mux dub preview into video | DONE | Codex | `mux_dub_preview_v1` job muxes dub preview audio onto original media to produce a preview MP4. |
| WP-0027 | Phase 2: Verification + packaging hardening (Python packs) | DONE | Codex | Manual smoke chain now executes end-to-end on sample, including Spleeter separation/runtime compatibility (HTTP/2 dependency fix for `httpx`). |
| WP-0028 | Phase 2: Neural TTS baseline (Kokoro or MeloTTS) | DONE | Codex | Added explicit-install neural TTS (Kokoro) pack and `tts_neural_local_v1` preview job producing per-segment WAVs + mixer-compatible manifest. |
| WP-0029 | Phase 2: Voice-preserving dubbing (OpenVoice/CosyVoice) | DONE | Codex | Implemented Kokoro+OpenVoice V2 voice-preserving dub with explicit model download; verified end-to-end through mix+mux on KO multi-speaker sample. |
| WP-0052 | FFmpeg dependency UX (ffprobe missing) | DONE | Codex | Library import preflight + Jobs remediation button for FFmpeg/ffprobe missing; explicit install only (no silent downloads). |
| WP-0053 | UI pipeline coverage (feature discoverability) | DONE | Codex | Subtitle Editor exposes ASR->translate->diarize->TTS/dub->separate->mix->mux plus Outputs (reveal/export MP4); Library keeps quick item actions + Editor entrypoint. |
| WP-0050 | Phase 2: Spleeter install resilience on Windows | DONE | Codex | Make Spleeter installer robust for CPython 3.8+ without requiring incompatible wheels; add explicit remediation guidance. |
| WP-0051 | Phase 2: Spleeter runtime compatibility fallback path | DONE | Codex | Deterministic Spleeter fallback path plus runtime dependency bootstrap added; WP-0027 smoke confirmed passing (including Spleeter runtime compatibility). |
| WP-0030 | ROI-01: One-click "Phase 2 Packs" installer | DONE | Codex | One-click installer job + per-step logs/state + Diagnostics progress UI; verified via `cargo test` + Windows bundle build. |
| WP-0031 | ROI-02: Portable Python distribution option | DONE | Codex | Portable Python status + explicit install + preference order; verified via build + tests. |
| WP-0032 | ROI-05: Single-pass audio mixer | DONE | Codex | Single-pass FFmpeg mixer (ducking + loudnorm) + UI settings + timing-fit; verified via build + tests. |
| WP-0033 | ROI-06: Speaker label management UI | DONE | Codex | Speaker display names + bulk assign/merge + best-effort propagation across tracks; verified via build + tests. |
| WP-0034 | ROI-07: In-app audio preview player (A/B) | DONE | Codex | Artifacts browser + in-app audio preview + quick video source toggle; verified via build + tests. |
| WP-0035 | ROI-08: Dub timing-fit tools | DONE | Codex | Timing-fit controls (enable + min/max) surfaced in UI; report emitted + export pack includes it; verified via build + tests. |
| WP-0036 | ROI-09: Subtitle-to-dub QC report | DONE | Codex | QC report job + Subtitle Editor viewer with jump-to-segment; verified via build + tests. |
| WP-0037 | ROI-10: Optional vocals cleanup | DONE | Codex | Optional vocals cleanup job + UI action + artifact listing; verified via build + tests. |
| WP-0038 | ROI-11: Mux options + metadata | DONE | Codex | Mux container (mp4/mkv) + keep-original + lang tags + UI controls; verified via build + tests. |
| WP-0039 | ROI-12: Batch processing rules on import | DONE | Codex | Batch-on-import rules (local-only; off by default) + Diagnostics UI + Library transparency; verified via build + tests. |
| WP-0040 | ROI-13: Better separation backend option | DONE | Codex | Demucs optional pack + job + UI selection; verified via build + tests. |
| WP-0041 | ROI-14: Better diarization backend option (BYO) | DONE | Codex | Optional BYO diarization backend (pyannote) config + per-job selection; verified via build + tests. |
| WP-0042 | ROI-15: Pack/model integrity | DONE | Codex | Integrity manifest generation + Diagnostics UI; pinned/hash-verified downloads where applicable; verified via build + tests. |
| WP-0043 | ROI-16: Derived output browser | DONE | Codex | Per-item artifacts list with reveal/open/rerun + job log access; verified via build + tests. |
| WP-0044 | ROI-17: Export packs | DONE | Codex | Export pack job (zip) + UI entrypoints; verified via build + tests. |
| WP-0045 | ROI-18: Performance tiering | DONE | Codex | Diagnostics shows tier + recommended settings (best-effort); verified via build + tests. |
| WP-0046 | ROI-19: Crash-safe resumable external steps | DONE | Codex | Best-effort resume/skip behavior across external steps; verified via build + tests. |
| WP-0047 | ROI-20: Licensing/attribution report generator | DONE | Codex | Licensing report generator + Diagnostics UI reveal; verified via build + tests. |
| WP-0048 | Phase 2: Speaker -> TTS voice mapping UI (pyttsx3) | DONE | Codex | Persist per-item speaker settings and select per-speaker system TTS voice; feed mapping into `tts_preview_pyttsx3_v1`. |
| WP-0019 | Fix mojibake encoding artifacts | DONE | Codex | Replaced mis-encoded punctuation sequences (e.g., `â€”`, `â€œ`) in docs/UI; kept Rust markers ASCII via Unicode escapes. |
| WP-0054 | Offline full bundle (Phase 1 + Phase 2 dependencies) | DONE | Codex | Bundled Phase 1+2 deps into installer as offline `payload.zip` + `manifest.json`; bootstrap extracts into app-data on first run (no downloads). |
| WP-0055 | UI persistence + output/artifact path visibility | DONE | Codex | Persist key UI settings across panes and make output/artifact folders easy to open from Jobs/Library; update specs for bundled-deps stance. |
| WP-0056 | Phase 1: YouTube subscriptions data model + engine flow | DONE | Codex | Added `youtube_subscription` schema + engine CRUD/queue/export/import; queueing applies per-subscription folder maps under downloads/video/subscriptions (or output override). |
| WP-0057 | Phase 1: YouTube subscriptions UI + portability | DONE | Codex | Added Library subscription UI (save/edit/delete, queue one/all, JSON export/import) with Tauri command wiring and pane-switch persistence via DB reload. |
| WP-0058 | Phase 1: Subscription interval + download responsiveness hardening | DONE | Codex | Added per-subscription refresh interval input/persistence (including import/export) and removed enqueue/startup blocking paths; verified via engine/tauri tests + desktop build. |
| WP-0059 | 4KVDP migration: import subscriptions + per-subscription dedupe | DONE | Codex | Import 4KVDP exports (`subscriptions.json` + optional `subscription_entries.csv`), preserve folder intent/output overrides, seed yt-dlp archive to avoid re-downloads, and queue per-video refresh jobs (verified via engine+tauri tests + desktop build). |
| WP-0060 | Phase 1: Safe Mode startup (recovery mode) | BACKLOG | Codex | Boot path to open app without auto-refresh/heavy background work so users can export/manage data safely. |
| WP-0061 | Thumbnail disk cache + Library virtualization | BACKLOG | Codex | Disk-based thumbnail cache with bounded eviction + virtualized Library UI for large libraries. |
| WP-0062 | Subscription groups/tags + failure backoff | BACKLOG | Codex | Organize subscriptions into groups and add robust backoff/pause controls for repeated failures. |
| WP-0063 | Output templates + “Smart presets” | BACKLOG | Codex | Output folder/file naming templates and reusable downloader presets (Smart Mode-like). |
| WP-0064 | Migration scan + optional index-only library import | BACKLOG | Codex | Scan existing download folders to seed dedupe archives and optionally index existing files into the Library DB (non-destructive). |
