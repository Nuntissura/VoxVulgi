# VoxVulgi — Roadmap / Backlog (Draft)

Date: 2026-02-19

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
