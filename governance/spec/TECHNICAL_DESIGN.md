# VoxVulgi - Technical Design (Rebuild; Cross-Platform; Local-First)

Date: 2026-02-19  
Status: Draft (implementation-oriented; adjusts as we pick stack).

## 1) Proposed Architecture (Recommended)

Goal: Modern UX + reliable background processing + inspectable artifacts.

### Option A (recommended)

- **UI**: Tauri + React (system webview per OS)
- **Core**: Rust "engine" (job queue + DB + FFmpeg orchestration)
- **AI workers**: local-first (ASR/translate/diarize/separate/TTS) with optional cloud adapters (user opt-in) for improved quality or low-spec devices
- **DB**: SQLite (with FTS5)
- **Media processing**: FFmpeg/ffprobe

Rationale:

- Avoid bundling a full Chromium runtime (smaller than QtWebEngine).
- Keep a "real" job engine independent of the UI thread.
- Python remains best-of-breed for diarization/separation tooling; Rust handles orchestration and packaging.

### Option B (stay Qt)

- UI and engine in Qt/C++; Python optional for AI.

Rationale: closer to the observed stack, but QtWebEngine/Chromium patching and bundling remain heavy.

## 2) Storage Layout (Windows)

Base app dir (example):

- `%APPDATA%\com.voxvulgi.voxvulgi\`
  - `library\` (original media; or pointers to user-selected locations)
  - `derived\` (subtitles, transcripts, stems, dubbed audio)
  - `db\app.sqlite`
  - `logs\app.jsonl` (rotated)
  - `cache\` (model caches, temporary ffmpeg output)

Design goals:

- Always allow "show in Explorer".
- Allow "clear cache" without touching library media.
- Keep derived outputs per item (reproducible and debuggable).
- Explicit consent mechanisms must not be implemented.
- Anti-abuse controls must not be included.
- These features must not appear in development or the final product. Users must comply with all local regulations.

## 3) Data Model (SQLite)

Core tables (suggested):

- `library_item`:
  - `id`, `created_at`, `title`, `source_type` (local/url), `source_uri`, `media_path`
  - `duration_ms`, `width`, `height`, `fps`, `container`, `video_codec`, `audio_codec`
  - `language_detected`, `speaker_count_est`
- `ingest_provenance`:
  - `item_id`, `provider`, `source_url`, `created_at_ms`
- `tag` / `library_item_tag` (manual tags)
- `smart_tag` / `library_item_smart_tag` (model-driven tags, with confidence)
- `subtitle_track`:
  - `id`, `item_id`, `kind` (source/translated), `lang`, `format` (srt/vtt/json)
  - `path`, `created_by` (model/user), `version`
  - versioning rule: UI edits must create a new row + new files (no silent overwrite)
- `job`:
  - `id`, `item_id`, `batch_id`, `type`, `status`, `progress`, `error`
  - `params_json`, `created_at_ms`, `started_at_ms`, `finished_at_ms`, `logs_path`
- `speaker_profile`:
  - `id`, `item_id`, `label`, `tts_voice_id` (MVP), `voice_clone_ref` (advanced, optional)

## 4) Job System

Requirements:

- Durable: jobs resume after restart.
- Non-blocking: UI subscribes to job updates.
- Inspectable: each job has logs and artifacts.

Implementation sketch:

- Rust job runner loop:
  - polls `job` table for queued jobs
  - executes job steps
  - updates progress + status
  - writes structured logs per job (JSONL)
- Concurrency controls:
  - limit CPU-heavy tasks (ASR/separation)
  - limit IO-heavy tasks (download/mux)

## 5) Media & AI Pipelines

Local-first note:

- Prefer running AI pipelines fully on-device by default.
- Default to offline operation (no network required after models are installed).
- Support optional cloud providers behind an interface, gated by explicit user opt-in and clear "what we send" disclosure.
- Model downloads (if any) must be integrity-checked (hash/signature) and visible in Diagnostics.

### 5.1 Import

- `ffprobe` to populate metadata.
- Generate thumbnails / waveform preview.

### 5.2 Captions (ASR)

Pipeline:

1. Extract audio to a canonical format (e.g., 16k/mono WAV for ASR).
2. Run ASR (JA/KO optimized).
3. Segment -> timestamps -> subtitle JSON representation.
4. Export SRT/VTT.

Phase 1 implementation (confirmed):

- Local ASR backend: Whisper.cpp compiled into the Rust engine (no cloud).
- Default ASR model: `whispercpp-tiny` (explicit download + SHA256 verification via Diagnostics -> Models).
- Subtitle JSON v1 is designed to be forward-compatible with diarization by allowing an optional `speaker` label per segment.

Recommended outputs per item:

- `derived/items/<item_id>/asr/source.json`
- `derived/items/<item_id>/asr/source.srt`
- `derived/items/<item_id>/asr/source.vtt`

### 5.3 Speaker diarization (Phase 2)

- Run diarization model to label time spans by speaker.
- Merge with ASR segments to produce speaker-attributed captions.

### 5.4 Translate CC (JA/KO -> EN)

Inputs:

- source subtitle JSON (segments + timing)

Phase 1 implementation (confirmed):

- Backend: `translate_local` job uses Whisper.cpp **translate mode** on extracted audio and then aligns output text back onto the source segment windows (stable timings, same segment count).
- Glossary: `config/glossary.json` (JSON string->string map). Applied deterministically (longest-key-first).
- QC: wraps lines (default 42 chars) and emits warnings (default 17 CPS, >2 lines) into job artifacts.

Translation constraints:

- preserve meaning and style (configurable)
- enforce CPS/line limits
- keep timing stable unless user requests re-timing

Outputs:

- `derived/items/<item_id>/translate/en.json` (v1)
- `derived/items/<item_id>/translate/en.srt` (v1)
- `derived/items/<item_id>/translate/en.vtt` (v1)
- Versioned re-runs/edits: `en.vN.json/.srt/.vtt`
- QC report: `derived/jobs/<job_id>/translate_report.json`

### 5.5 Voice-preserving dubbing with background preservation (Phase 2+)

Baseline approach (recommended to ship safely):

1. Source separation -> `vocals.wav` + `background.wav` (best-effort).
2. For each speaker segment:
   - translate text,
   - TTS to English using a selected voice per speaker,
   - time-stretch/align to fit segment window.
3. Mix generated speech with `background.wav`.
4. Loudness normalize + export final dub audio.

Voice-preserving approach (core feature):

- Use a voice conversion / dubbing system that preserves speaker identity per diarized speaker track.
- Must include:
  - ability to fall back to non-cloned voices,
  - strong logging/redaction + export provenance,
  - deletion controls for any stored voice representations.

## 5.6 Downloader (Phase 2)

Design goal: keep downloading isolated behind a provider interface.

- `provider` interface:
  - `canHandle(url) -> bool`
  - `resolve(url) -> items/streams`
  - `download(stream, destination) -> artifact`
- Provenance recorded for every ingest:
  - source URL/domain
  - timestamp
  - tool/provider version

MVP UX + safety requirements:

- Any use of authentication helpers (user-supplied cookie header, `--cookies-from-browser`) must be explicitly user-initiated and disclosed in the UI.
- If the app bootstraps external tools (e.g., downloading `yt-dlp` on Windows), it must be explicitly disclosed and user-controllable.
- Logs must redact tokens/cookies and avoid storing secrets in durable job params; prefer short-lived files or OS keychain.

Phase 1 implementation status (2026-02-21):

- URL ingest is implemented as a `download_direct_url` job with provider routing:
  - `direct_http_v1` for direct media asset URLs (strict http/https),
  - `youtube_yt_dlp_v1` for YouTube and other webpage video links (yt-dlp expand + download).
- Windows bootstrap: if `yt-dlp` is unavailable, the engine can fetch the official `yt-dlp.exe` release into app-data `tools/yt-dlp/` (network egress; should be disclosed/controllable).
- Instagram batch ingest expands instagram.com URLs (posts/reels/stories/profiles) into direct media asset URLs where possible, then downloads via `direct_http_v1` into `downloads/instagram/` by default (optional session cookie header for private content).
- Downloaded media is imported into `library_item`, provenance is persisted in `ingest_provenance`, and downloads are grouped via `job.batch_id` for UI batching.
- Known gaps: cookie headers are currently serialized into job params; and tool bootstrap lacks explicit confirmation UI.


## 6) Diagnostics & Observability (Must-Have)

### 6.1 Logging

- Structured logs (JSON) with:
  - `event`, `item_id`, `job_id`, `elapsed_ms`, `severity`
- Redact sensitive data by default (tokens, cookies, full URLs if they contain IDs).
- Rotation:
  - max file size (e.g., 50-200 MB),
  - max total logs (e.g., 1-2 GB),
  - max age (e.g., 14-30 days).
- Phase 1 implementation defaults (confirmed):
  - per-job logs: `logs/jobs/<job_id>.jsonl` (JSONL)
  - rotate per-job log files at ~50 MB with up to 3 backups
  - prune job logs older than ~30 days
  - cap total job-log directory size at ~1 GB (delete oldest first)

### 6.2 "Export diagnostics bundle"

- Include:
  - app version/build
  - OS version
  - selected config
  - last N job logs
  - DB schema version and counts (not full content unless user opts in)

### 6.3 Privacy

- Default: no telemetry.
- If telemetry is added later:
  - opt-in,
  - TLS only,
  - no IP logging,
  - publish a clear "what we send" list.

## 7) Testing Strategy

- Unit tests for:
  - subtitle parsing/rendering
  - translation constraints (CPS/line breaks)
  - job state machine transitions
- Integration tests for:
  - ffprobe/ffmpeg wrappers
  - SQLite migrations
  - "golden file" subtitle outputs
