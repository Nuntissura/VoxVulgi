# VoxVulgi: Top 20 ROI features (next additions)

Date: 2026-02-22  
Scope: High-return additions that improve user value quickly while preserving local-first + no telemetry by default.

1. One-click "Phase 2 Packs" installer with progress and disk impact estimates.
2. Portable Python distribution option (no system Python required).
3. Neural TTS baseline (commercial-friendly default) to replace system TTS previews.
4. Voice-preserving dubbing backend (OpenVoice/CosyVoice) with per-speaker mapping UI.
5. Single-pass audio mixer (avoid iterative FFmpeg overlays) with ducking + loudness normalization.
6. Speaker label UI: rename speakers, merge/split speakers, and propagate labels across tracks.
7. Audio preview player for stems/dub outputs inside the item view (A/B compare).
8. Automatic timing-fit tools for dub speech (time-stretch/phoneme-aware alignment to segment windows).
9. Subtitle-to-dub QC report: CPS/line-length + "timing mismatch" + "overlaps" + "untranslated segments".
10. Optional background noise reduction and de-reverb for vocals before TTS/VC (explicit install).
11. Mux options: keep original audio as an additional track, choose output container, and tag language metadata.
12. Batch processing rules (local-only): "auto ASR", "auto translate", "auto dub preview" on import.
13. Better separation backend option (explicit install) when licenses/weights are acceptable.
14. Better diarization backend option (BYO gated models) for power users, kept off by default.
15. Model/packs integrity: hash-verified downloads and pinned versions for reproducible installs.
16. Derived output browser: per-item artifacts timeline with "reveal file", "open log", "rerun job".
17. Export packs: one zip with dubbed audio, muxed video, subtitles, and a provenance manifest.
18. Performance tiering: CPU baseline vs GPU acceleration detection and recommended settings.
19. Crash-safe resumable external steps (Python/FFmpeg) with checkpointing and clear resume behavior.
20. Licensing/attribution report generator for any installed packs and models (for future commercial distribution).

