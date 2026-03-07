# VoxVulgi — Roadmap / Backlog (Draft)

Date: 2026-03-07

## Immediate Track - Stabilization + IA Refresh (2026-03)

- WP-0065: Fix Jobs "Open outputs" ACL failures and standardize open-path behavior.
- WP-0066: Make Localization Studio first and split windows by core workflows.
- WP-0067: Remove window-switch freezes (mount/poll lifecycle cleanup).
- WP-0068: Make Diagnostics non-blocking with per-section readiness states.
- WP-0069: Stage startup initialization and add boot-timing instrumentation.
- WP-0070: Clarify/rename "Items" window to match real user intent.
- WP-0071: Clarify installer maintenance modes (Update/Repair vs Full reinstall vs Uninstall).
- WP-0072: Add pre-maintenance explainer page before NSIS maintenance action selection.
- WP-0073: Add governance-level policy pointers for installer maintenance mode wording.
- WP-0074: Clarify localization output locations, prefer MP4 defaults, repair thumbnail recovery, and remove remaining first-open freezes.
- WP-0075: Fix the voice-preserving dub regression where jobs succeed with silent spoken audio.
- WP-0076: Add reusable voice-template save/apply flow for recurring speaker setups in Localization Studio.

## Voice-Cloning Expansion Program (Post-WP-0076)

- WP-0077: Reusable cast packs for recurring show roles across episodes. DONE.
- WP-0078: Multi-reference cloning with several clean clips per template speaker. DONE.
- WP-0079: Auto-match suggestions between diarized speakers and saved templates/cast packs. DONE.
- WP-0080: Per-speaker style presets (neutral, documentary, game-show, soft, authoritative). DONE.
- WP-0081: Pronunciation locks for names, places, and glossary terms. DONE.
- WP-0082: Emotion/prosody controls and reusable delivery presets. DONE.
- WP-0083: Voice QC for references and dubbed outputs. DONE.
- WP-0084: Batch dubbing across folders, playlists, seasons, or selected item sets. DONE.
- WP-0085: A/B voice previewing before committing to a final voice setup. DONE.
- WP-0086: Export stems and alternate dubbed versions. DONE.
- WP-0087: Cross-episode voice memory for recurring speakers. DONE.
- WP-0088: Reference cleanup before cloning (denoise, de-reverb, isolate, normalize). DONE.
- WP-0089: Hybrid mode combining cloned major speakers with standard TTS for minor speakers. DONE.
- WP-0090: Subtitle-aware prosody driven by punctuation, line breaks, and timing structure. DONE.
- WP-0091: Character libraries for reusable fictional narrator or teaching voices. DONE.

## Voice Workflow Remediation Hardening (2026-03)

- WP-0092: Variant-aware artifact actions, status, and logs for A/B previews, alternates, QC, and export packs. DONE.
- WP-0093: Cleanup integrity for multi-reference voices and collision-safe cleanup storage. DONE.
- WP-0094: Library-scale batch dubbing selection without hidden 500-item caps. DONE.

## Manual App Validation Follow-up (2026-03)

- WP-0095: Run a real Localization Studio app smoke for `WP-0092` to `WP-0094`, capture operator-facing proof, and queue any remaining defects as explicit follow-up packets.

## Workflow and Archive UX Hardening (2026-03)

- WP-0096: Refresh the top-level IA so Localization Studio owns first-step ingest while `Video Archiver` and `Instagram Archiver` are named and scoped clearly. DONE.
- WP-0097: Move the shared download/export root into global Options and make path hydration deterministic across startup, updates, and window switches. DONE.
- WP-0098: Reconcile large existing downloader/NAS archive roots, playlists, and subscriptions without flattening or destructive moves, now including 4KVDP SQLite/app-state correlation and import. DONE.
- WP-0099: Harden Video Archiver and Instagram Archiver workflows with subscription folder reveal, recurring Instagram archive refresh, and uncropped recent thumbnail viewing. DONE.
- WP-0100: Improve Media Library with reliable open-file behavior on secondary drives, tighter card actions, grouped browsing, and media-type filters. DONE.
- WP-0101: Standardize `Open file` and `Open parent folder` actions across all output/artifact surfaces. DONE.
- WP-0102: Add startup progress and deterministic performance/resource tracing, including clearer bundled-versus-loaded tool-state visibility. DONE.
- WP-0103: Retain pane state and reduce content reload/freeze behavior when switching windows. DONE.
- WP-0104: Refine drag and resize ergonomics so the shell does not block text selection, scrolling, or diagonal resize. DONE.
- WP-0105: Make MP4 the default video target and JPEG the default image target wherever the local toolchain can comply sensibly. DONE.
- WP-0106: Add Pinterest board/folder crawling to Image Archive with batch URL intake. DONE.

## Phase 0 — Decisions (1–2 days)

- Pick stack: Tauri/Rust + Python workers (recommended) vs Qt vs Electron.
- Confirm runtime stance: local-first by default; define optional cloud providers + opt-in UX (ASR/translation/dubbing).
- Define policy/UX for consent-gating voice preservation.
- Define downloader provider list (or local import only for MVP).
- Define diagnostics + data retention defaults (logs, cache, derived artifacts).

## Phase 1 — MVP (Library + CC + Translate CC) (2–4 weeks)

- Library DB + import flow (ffprobe metadata, thumbnails).
- Downloader provider interface (local import always supported).
- Library UX: search/filter/collections, item detail view.
- Job system + queue UI (non-blocking).
- ASR pipeline (JA/KO) + subtitle editor v1.
- Translate CC pipeline (JA/KO → EN) with glossary + QC rules.
- Diagnostics page + export bundle.
- Log rotation/retention + cache cleanup tools.

## Phase 2 — Dubbing MVP (Safe; Multi-speaker; Background preservation) (3–6 weeks)

- Diarization + speaker mapping UI.
- Background separation + mixing pipeline (best-effort).
- Multi-speaker English dubbing via selected TTS voices (no voice cloning).
- Alignment/time-fit controls (time-stretch/fit, pacing warnings).
- Export muxed video + separate audio/subtitle artifacts.
- Evaluation harness (sample set + MOS-style rubric) focused on JA/KO → EN educational clarity.

## Phase 3 — Voice-preserving dubbing (R&D; gated) (4–10 weeks)

- Voice conversion / identity-preserving pipeline behind explicit consent.
- Multi-speaker robustness: prevent speaker “bleed” and identity swaps.
- Preserve timbre/tone while matching English prosody as naturally as possible.
- Background preservation benchmarks + regression tests (avoid “underwater” artifacts).
- Provenance + labeling in exports (what was generated, with what settings).

## Phase 4 — Smart tags + Scaling (2–4 weeks)

- Smart tags v1 (language, keywords, topic summary, speaker count estimate).
- Full-text search over transcripts/subtitles (FTS5).
- Batch rules / watch folders (optional).
- Collaboration primitives (shared glossary, review comments, export reports) if needed.

## Quality Gates (Continuous)

- Security: dependency scanning + patch cadence (FFmpeg, webview, TLS).
- Privacy: no network egress by default; all outbound calls visible in diagnostics when enabled.
- Footprint: cap logs and caches; show storage usage.
