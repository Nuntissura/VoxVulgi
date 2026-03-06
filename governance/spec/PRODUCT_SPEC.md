# VoxVulgi — Product Spec (Rebuild; Cross-Platform; Local-First)

Date: 2026-02-20  
Status: Draft (assumptions noted; intended as the starting point for implementation planning).

## 1) Framing / Constraints

- This is a **rebuild** (new codebase), inspired by the observed behavior of 4K Video Downloader+ on this machine. We do **not** reuse proprietary code or decompile binaries.
- Target platforms: cross-platform desktop (Windows + macOS first; Linux later if needed).
- Local-first: default to on-device ASR/translation/dubbing for privacy and offline use, with optional cloud providers only when the user opts in (and with clear disclosure of what data is sent).
- Offline-friendly: after models are installed, core workflows (import, captions, translate, export) should work without network access.
- Voice-preserving dubbing is **voice cloning/voice conversion**. This is a core feature.
- Explicit consent mechanisms must not be implemented.
- Anti-abuse controls must not be included.
- These features must not appear in development or the final product. Users must comply with all local regulations.


## 2) Product Goal

Build a modern desktop app that:

1) Downloads or imports videos into a **library** (collections, search, smart tags).  
2) Generates and edits **captions/subtitles** (JA/KO first).  
3) Produces **English translations** of captions with high readability and correct timing.  
4) Produces **voice-preserving dubbed audio** (multi-speaker), keeping background sound as much as possible and sounding natural for students.

Initial language focus: **Korean + Japanese → English**.

## 3) Target Users

- Creators/editors who localize content (YouTube/TikTok clips, interviews, podcasts).
- Language learners who want accurate captions + translations.
- Archivists who want a searchable library with tags/metadata.

## 4) MVP Scope (Phase 1)

### 4.1 Library + Ingestion (core UX)

- Import local video/audio files.
- Downloading: provider layer + batch URL ingest:
  - direct HTTP/HTTPS media URLs (strict schemes; best-effort),
  - YouTube (and many webpage video links) via `yt-dlp` (local tool),
  - Instagram batch ingest (posts/reels/stories/profiles) that expands into media targets (optional session cookie header for private content),
  - provenance captured per ingest (provider + source URL).
- Add YouTube subscription management:
  - save persistent subscriptions (channel/playlist/video feed URLs),
  - define a folder map per subscription so each subscription writes into its own mapped folder,
  - set a per-subscription refresh interval (minutes) that can be edited in the Library UI,
  - queue refresh for one subscription or all active subscriptions,
  - keep loaded subscriptions stable across pane switches and window focus changes.
- Add subscription export/import:
  - export all subscriptions to JSON (portable backup/migration file),
  - import from JSON with merge-by-URL behavior (upsert existing, add missing),
  - no subscription deletion on import unless explicitly requested by the user.
- Add an in-app **image archive batch** mode for blogs/forums:
  - accepts multiple start URLs in one submission,
  - crawls pagination + post/thread links,
  - skips likely profile/avatar images,
  - prefers full-size image URLs over thumbnail variants,
  - writes a manifest for audit/review.
- Auto-extract metadata (duration, codecs, resolution) + generate thumbnails.
- Performance stance (large libraries):
  - thumbnails should be stored on disk (cache) and lazy-loaded (no giant DB BLOB storage),
  - Library list/grid should be virtualized to stay responsive with very large libraries.
- Library list with:
  - search (title/tags/text),
  - filters (language, status, date, source),
  - collections/playlists.
- “Smart tags” v1:
  - language detected,
  - speaker count (rough),
  - topics/keywords summary.

### 4.2 Captions (CC) v1

- Generate captions with timestamps (SRT + VTT export).
- Basic subtitle editor:
  - segment list + timeline,
  - text edit,
  - split/merge,
  - time nudge and reflow.

### 4.3 Translate CC (JA/KO → EN) v1

- One-click translation pass producing:
  - translated subtitles (EN),
  - optional bilingual view (source + EN).
- Quality features:
  - glossary (custom term mappings),
  - style settings (formal/informal, honorific handling, punctuation rules),
  - line-length and CPS (characters-per-second) constraints.

### 4.4 Diagnostics (must-have)

- “Diagnostics” page:
  - versions of major components (app, ffmpeg, models),
  - model inventory (what’s installed, where it’s stored, and how much space it uses),
  - storage usage breakdown (library, cache, logs),
  - last job errors with copy/export.
- Log rotation and retention (cap by size + age).
- “Export diagnostics bundle” (logs + job metadata + redacted config).
- Recovery UX:
  - a **Safe Mode** startup path to open the app without auto-refresh or heavy background work (so users can export/manage data even when providers regress).

## 5) Phase 2 (Voice-preserving dubbing MVP)

### 5.1 Multi-speaker segmentation

- Speaker diarization (label Speaker 1/2/3…).
- UI to map speaker labels to:
  - a TTS voice (MVP-safe approach), or
  - a voice-preserved model (advanced).
- Operators must be able to save reusable voice templates for recurring speakers/series and re-apply them to later items through explicit speaker-slot mapping.
- Current reusable-voice support includes:
  - reusable cast packs for recurring show roles (`host`, `narrator`, `contestant`, `guest`, and custom roles via template labels),
  - multi-reference speaker cloning with backward-compatible single-reference fallback,
  - operator-reviewed auto-match suggestions from diarized speakers to saved template speakers or cast-pack roles,
  - per-speaker render-mode routing so clone and standard-TTS speakers can coexist in one item,
  - cross-episode voice memory profiles for recurring real speakers,
  - separate character libraries for reusable narrator/teaching voices.

### 5.2 Background preservation

- Separate vocals vs background (best-effort source separation).
- Generate English speech per segment and mix back with background.
- Provide mix controls:
  - ducking, loudness normalization, fade, noise reduction (optional).
- Current dubbing-quality controls:
  - per-speaker style presets,
  - pronunciation locks for names/places/glossary terms,
  - emotion/prosody controls with reusable presets,
  - hybrid mode where major speakers use cloning and minor/background speakers use standard TTS,
  - explicit subtitle-aware prosody toggles on speaker/template/profile data,
  - optional reference cleanup before cloning,
  - voice QC for reference and output quality.

### 5.3 Export

- Export:
  - dubbed audio track (WAV/AAC),
  - muxed video with new audio track (MP4/MKV),
  - subtitles as sidecar or burned-in.
- Planned export/review additions:
  - A/B preview variants before committing to a final voice choice,
  - batch dubbing across item sets or seasons,
  - export stems (speech only, background only, final mix) and alternate dubbed versions.

Current implementation status:

- Localization Studio surfaces batch dubbing, A/B speaker previews, export stems/alternates visibility, voice memory, character libraries, and reference cleanup controls.
- Export packs include speech stems and alternate dubbed variants when available.

## 6) Phase 3 (Power Features)

- “Smart tags” v2:
  - named entities (people/places/orgs),
  - topic clustering,
  - “find similar clips” via embeddings.
- Content-aware workflows:
  - batch processing rules (“auto-translate all new JA videos”),
  - watch folders,
  - scheduled tasks.
- Collaboration:
  - shared glossary,
  - subtitle review comments,
  - export reports.
- Advanced dubbing library features:
  - richer evaluation/QC heuristics,
  - stronger subtitle-aware prosody controls and future expressive hinting,
  - deeper reuse/reporting workflows on top of the now-implemented memory/character libraries.

## 7) UX Principles

- Fast: UI never blocks on AI jobs (always queued with progress).
- Fast: queueing URL/subscription downloads must return quickly; heavy URL expansion/extraction runs in worker jobs, not on the UI thread.
- Transparent: show what data is stored and where; easy cleanup.
- Editable: every AI output is reviewable and editable.
- Offline by default: no background network egress. Windows "full" installers bundle required local tools/models for Phase 1+2 and bootstrap them into app-data on first launch, so the core pipeline can run fully offline without manual pack installs.
- Safe defaults: no voice cloning by default.
- Voice and dubbing controls remain operator-directed; VoxVulgi should not add content-judgment or censorship workflows as part of these features.

## 8) Key UX Screens

- **Library**: grid/list, filters, collections, “import” CTA.
- **Item detail**:
  - player preview,
  - job history,
  - subtitles tabs (original, translated),
  - tags + notes.
- **Subtitle editor**:
  - timeline + segment table,
  - speaker labels,
  - reusable voice-template save/apply for recurring speaker setups,
  - translation side-by-side,
  - QC warnings (too fast, too long).
- **Jobs/Queue**: running/failed/completed, retry, logs link.
- **Diagnostics**: storage usage, logs export, version info, privacy settings.

### 8.0.1 Current top-level windows (implemented 2026-03-03)

- **Localization Studio**: first/default window, focused on subtitles + dubbing workflow.
- **Video Ingest**: local import + URL batch ingest + presets/templates + YouTube subscriptions/groups.
- **Instagram Archive**: dedicated Instagram batch ingest workflow.
- **Image Archive**: dedicated crawler-based image archive ingest workflow.
- **Media Library**: renamed from ambiguous â€œItemsâ€; browse imported media and hand off to Localization Studio.
- **Jobs/Queue**: execution state + retry/cancel + logs and output reveal.
- **Diagnostics**: non-blocking, section-by-section loading with explicit readiness states.

## 8.1) Stabilization priorities for commercial readiness (2026-03-03)

### 8.1.1 Operator goals and needs

- Localization Studio (Dub/CC) must be the primary, first-visible workspace.
- Navigation must be split into clear top-level windows for ingest/archive/localization work.
- Window switching, Diagnostics entry, and startup must stay responsive (no visible freezing).
- Jobs "Open outputs" actions must reliably open paths without ACL errors.
- Localization exports must be easy to find, with a predictable default folder map and direct open/reveal actions for both source files and generated deliverables.
- Default preview/download video outputs should be MP4 wherever the local toolchain can merge/remux cleanly.
- UX must be fast enough for daily production use before any commercial release.

### 8.1.2 Required top-level window model

- Localization Studio (Dub/CC) - default first-run window and main feature surface.
- Video Ingest - local ingest + YouTube ingest + playlist/subscription/folder-map flows.
- Instagram Archive - dedicated archive workflow.
- Image Archive - dedicated archive workflow.
- Jobs/Queue - execution visibility + controls.
- Diagnostics - health/recovery tooling with non-blocking load.
- "Items" must be either clearly defined and renamed or merged into a clearer workspace label.

### 8.1.3 Performance and responsiveness budgets (target)

- Startup: app shell becomes interactable before heavyweight background initialization completes.
- Startup instrumentation: boot timeline markers must identify slow phases in logs/diagnostics.
- Window switching: no multi-second freezes during normal navigation.
- Diagnostics entry: render shell immediately and load sections incrementally with explicit readiness states.

### 8.1.4 Reliability requirement: output path opening

- Queue/Library/Diagnostics open-path actions must work for valid output/artifact paths.
- Blocked/invalid paths must return actionable errors with copy-path fallback.

### 8.1.5 Installer and uninstall clarity requirement

- Setup/maintenance UI must clearly present:
  - Update/Repair in place,
  - Full reinstall (uninstall then install),
  - Uninstall.
- For existing installs, show a short explainer page before maintenance choice so operators see one-line outcomes for each mode.
- Uninstall flow must explicitly indicate that app-data lives under `%APPDATA%\com.voxvulgi.voxvulgi` and is only removed when the operator chooses delete-app-data.

## 9) Top 20 ROI backlog (next additions)

Current direction keeps baseline values intact; these are explicitly deferred/planned features.

ROI-01. One-click Phase 2 Packs installer UI (no consent gate), progress, and disk impact estimates.  
ROI-02. Portable Python distribution option so system Python is not required.  
ROI-03. Neural TTS baseline (commercial-friendly default) to replace system TTS preview.  
ROI-04. Voice-preserving dubbing backend (OpenVoice/CosyVoice) with per-speaker mapping UI.  
ROI-05. Single-pass audio mixer (replace iterative overlay) with ducking + loudness normalization.  
ROI-06. Speaker label UI for rename/merge/split and propagation across tracks.  
ROI-07. In-app audio preview player for stems/dub outputs with A/B comparison.  
ROI-08. Timing-fit tools for dub outputs (time-stretch alignment to segment windows).  
ROI-09. Subtitle-to-dub QC report (CPS/line length, timing mismatch, overlaps, untranslated coverage).  
ROI-10. Optional vocal cleanup (noise reduction and de-reverb) as an explicit-install pipeline option.  
ROI-11. Mux options: keep original audio as extra track, container choice, language metadata tags.  
ROI-12. Batch processing rules on import (auto ASR/auto translate/auto dub preview).  
ROI-13. Better separation backend option when license/model fit is favorable.  
ROI-14. Better diarization backend option (BYO gated models) for power users, off by default.  
ROI-15. Pack/model integrity with pinned versions and hash verification for reproducible installs.  
ROI-16. Derived output browser showing per-item artifacts timeline, reveal/open log, rerun.  
ROI-17. Export pack (audio + subtitles + muxed video + provenance manifest) as a single zip.  
ROI-18. Performance tiering (CPU baseline vs GPU) with recommended settings and runtime checks.  
ROI-19. Crash-safe resumable external steps with checkpoint and clear resume behavior.  
ROI-20. Licensing/attribution report for all installed packs and models.
ROI-21. Safe Mode startup (no auto-refresh; minimal background work; export-first recovery).  
ROI-22. Thumbnail disk cache + Library virtualization + bounded cache eviction (LRU).  
ROI-23. Subscription groups/tags + failure backoff so large subscription sets remain manageable.  
ROI-24. Output folder/file templates + reusable downloader presets (“Smart Mode”-like).  
ROI-25. Migration hardening: scan existing download folders to seed dedupe archives + optional index-only library import.

## 10) Locked-in answers (from operator)

1) Platform: cross-platform.
2) AI runtime: local-first by default; optional cloud providers only with explicit user opt-in and clear disclosure.
3) Primary differentiator: voice-preserving dubbing (multi-speaker) + background preservation, focused on Korean/Japanese -> English.
4) Include downloading for educational use.
