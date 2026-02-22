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
- Add an in-app **image archive batch** mode for blogs/forums:
  - accepts multiple start URLs in one submission,
  - crawls pagination + post/thread links,
  - skips likely profile/avatar images,
  - prefers full-size image URLs over thumbnail variants,
  - writes a manifest for audit/review.
- Auto-extract metadata (duration, codecs, resolution) + generate thumbnails.
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

## 5) Phase 2 (Voice-preserving dubbing MVP)

### 5.1 Multi-speaker segmentation

- Speaker diarization (label Speaker 1/2/3…).
- UI to map speaker labels to:
  - a TTS voice (MVP-safe approach), or
  - a voice-preserved model (advanced).

### 5.2 Background preservation

- Separate vocals vs background (best-effort source separation).
- Generate English speech per segment and mix back with background.
- Provide mix controls:
  - ducking, loudness normalization, fade, noise reduction (optional).

### 5.3 Export

- Export:
  - dubbed audio track (WAV/AAC),
  - muxed video with new audio track (MP4/MKV),
  - subtitles as sidecar or burned-in.

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

## 7) UX Principles

- Fast: UI never blocks on AI jobs (always queued with progress).
- Transparent: show what data is stored and where; easy cleanup.
- Editable: every AI output is reviewable and editable.
- Offline by default: no background network egress unless explicitly enabled (optional cloud providers, model downloads, update checks).
- Safe defaults: no voice cloning by default.

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
  - translation side-by-side,
  - QC warnings (too fast, too long).
- **Jobs/Queue**: running/failed/completed, retry, logs link.
- **Diagnostics**: storage usage, logs export, version info, privacy settings.

## 9) Locked-in answers (from operator)

1) Platform: cross-platform.
2) AI runtime: local-first by default; optional cloud providers only with explicit user opt-in and clear disclosure.
3) Primary differentiator: voice-preserving dubbing (multi-speaker) + background preservation, focused on Korean/Japanese -> English.
4) Include downloading for educational use.
