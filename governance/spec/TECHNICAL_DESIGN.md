# VoxVulgi - Technical Design (Rebuild; Cross-Platform; Local-First)

Date: 2026-03-09  
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
- Python toolchain is managed as an app-data sidecar (`tools/python/venv`) surfaced in Diagnostics. Windows "full" installers bundle and bootstrap this sidecar by default; explicit install actions remain for slim/dev builds.

### Option B (stay Qt)

- UI and engine in Qt/C++; Python optional for AI.

Rationale: closer to the observed stack, but QtWebEngine/Chromium patching and bundling remain heavy.

## 2) Storage Layout (Windows)

Base app dir (example):

- `%APPDATA%\com.voxvulgi.voxvulgi\`
  - `library\` (original media; or pointers to user-selected locations)
  - `derived\` (subtitles, transcripts, stems, dubbed audio; per-job artifacts + outputs)
  - `voice_templates\` (reusable speaker-template manifests + copied reference clips)
  - `db\app.sqlite`
  - `logs\app.jsonl` (rotated)
  - `cache\` (model caches, temporary ffmpeg output)
    - `cache\thumbs\` (thumbnail disk cache; bounded and safe to clear)

Design goals:

- Always allow "show in Explorer".
- Do not write derived outputs next to the source media path by default; exporting is explicit and user-directed.
- Default user-facing exports should land under the configured download root in predictable app-managed folders (for example `localization/en/<media-stem>/` for Localization Studio deliverables).
- The shared download/export root should be configured once from a global Options surface, not duplicated as pane-local state.
- The single-root model should evolve into durable per-feature roots managed from Options, with feature panes showing resolved effective paths only.
- Selecting an existing valid root should hydrate expected app-managed folders and index known existing archive folders instead of briefly treating the root as missing.
- Allow "clear cache" without touching library media.
- Generic job-history cleanup must never remove item-scoped deliverables or user-selected output folders unless the operator explicitly opts into those output-folder classes.
- Avoid storing large thumbnail blobs inside SQLite; store thumbnails on disk and keep the DB as metadata-only.
- Keep derived outputs per item (reproducible and debuggable).
- Item-scoped derived outputs should be classified as working files, durable reports, and durable deliverables, and Diagnostics should surface that contract.
- Cleanup summaries should surface failed path deletions and keep job provenance for job-linked cleanup failures.
- SQLite schema evolution should be driven by explicit numbered migrations via `PRAGMA user_version`, with each migration step applied transactionally.
- Config, override, adapter-store, and secret writes should use atomic temp-file-plus-replace helpers instead of direct in-place writes.
- Recurring UI polling should use shared non-overlapping visibility-aware loops, and mounted-but-hidden pages must suspend background refresh until active again.
- Localization Studio artifact/runtime contracts should be serialized from Rust as typed metadata (`kind`, job type, variant, backend, container, rerun class) and consumed from shared frontend modules rather than reconstructed ad hoc from artifact IDs and filenames.
- Localization Studio should provide a dedicated outputs browser/library layer that groups source media, working artifacts, and exported deliverables for the active item instead of relying only on scattered path fields/buttons.
- Explicit consent mechanisms must not be implemented.
- Anti-abuse controls must not be included.
- These features must not appear in development or the final product. Users must comply with all local regulations.

### 2.1 Windows "full" installer (bundled dependencies)

To minimize setup friction and support offline use, the Windows "full" installer bundles the core local toolchain for Phase 1 + Phase 2 (FFmpeg/ffprobe, yt-dlp, whisper model(s), portable Python + venv, and Phase 2 packs/models).

Implementation notes (desktop):

- The installer includes an `offline/` resource payload:
  - `offline/manifest.json`
  - `offline/payload.zip` (contains `tools/`, `models/`, and `cache/huggingface/`)
- On first run, the app extracts the payload into the user app-data dir and writes a marker (`config/offline_bundle_applied_v1.json`) so it only applies once per bundle id.
- Build policy: desktop installer packaging must refresh `src-tauri/offline/payload.zip` before each release build so bundled dependencies match the current engine/toolchain state.
- `offline/manifest.json` should carry payload byte size and SHA-256 when available, and startup hydration must verify those before extraction.
- Bundled toolchain inputs should be tracked in a single pinned dependency manifest (`product/engine/resources/tooling/pinned_dependency_manifest.json`) so release provenance is reproducible and inspectable.
- Mutable unpinned recovery installs remain available only behind an explicit local opt-in environment variable; release preparation must succeed without depending on them.
- Third-party package patching should live in small tested Rust helper modules instead of large inline runtime patch scripts embedded in installer code paths.
- Diagnostics should distinguish required, optional, demo/test, bundled, hydrated, and manually installable dependencies so the operator inventory matches the real runtime contract.

### 2.1.1 Installer maintenance mode clarity

Windows NSIS installer UX must explicitly communicate maintenance outcomes:

- **Update**: installs this version over the current install and preserves preferences/options.
- **Reinstall (keep preferences and options)**: uninstalls installed program files, then installs again while preserving preferences/options.
- **Full reinstall**: uninstalls installed program files and removes preferences/options before installing again.
- **Uninstall (keep preferences and options)**: removes installed program files only.
- **Full uninstall**: removes installed program files plus preferences/options.

Preferences/options under `%APPDATA%\com.voxvulgi.voxvulgi` are retained by default unless the operator explicitly chooses one of the full actions.

Implementation note:

- Custom NSIS language strings are defined in `product/desktop/src-tauri/installer/languages/English.nsh` and wired in `tauri.conf.json`.
- Custom NSIS template is defined in `product/desktop/src-tauri/installer/templates/installer.nsi` and inserts a short explainer page before maintenance option selection when an existing installation is detected.
- The maintenance selector should use explicit action modes rather than version-dependent reinterpretation of two radio buttons.
- Uninstall-only actions should exit after uninstall completes instead of flowing forward into installation pages.
- Desktop installer packaging remains versioned monotonically: each managed desktop target build increments semantic version.

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
- `voice_template`:
  - `id`, `name`, `created_at_ms`, `updated_at_ms`
- `voice_template_speaker`:
  - `template_id`, `speaker_key`, `display_name`, `tts_voice_id`, `tts_voice_profile_path`, `created_at_ms`, `updated_at_ms`
- Planned voice-dubbing expansion tables:
  - `voice_template_reference`:
    - `template_id`, `speaker_key`, `reference_id`, `path`, `label`, `sort_order`, `cleaned_from_path`, `created_at_ms`, `updated_at_ms`
  - `voice_cast_pack`:
    - `id`, `name`, `series_key`, `created_at_ms`, `updated_at_ms`
  - `voice_cast_pack_role`:
    - `pack_id`, `role_key`, `display_name`, `template_id`, `template_speaker_key`, `style_preset`, `prosody_preset`, `created_at_ms`, `updated_at_ms`
  - `voice_pronunciation_lock`:
    - `id`, `scope_kind`, `scope_id`, `term`, `spoken_override`, `notes`, `created_at_ms`, `updated_at_ms`
  - `voice_preview_variant`:
    - `id`, `item_id`, `speaker_key`, `label`, `settings_json`, `artifact_path`, `created_at_ms`
  - `voice_library_profile`:
    - `id`, `kind` (`memory` or `character`), `name`, `description`, `display_name`, `tts_voice_id`,
      `tts_voice_profile_path`, `tts_voice_profile_paths_json`, `style_preset`, `prosody_preset`,
      `pronunciation_overrides`, `render_mode`, `subtitle_prosody_mode`, `created_at_ms`, `updated_at_ms`
  - `voice_library_reference`:
    - `profile_id`, `reference_id`, `label`, `path`, `sort_order`, `created_at_ms`, `updated_at_ms`
- `youtube_subscription`:
  - `id`, `title`, `source_url`, `folder_map`, `output_dir_override`, `active`
  - `refresh_interval_minutes` (integer, clamped range; user-editable in Library UI)
  - `use_browser_cookies`, `last_queued_at_ms`, `created_at_ms`, `updated_at_ms`
  - `source_url` is unique (merge key for import/upsert)
- `instagram_subscription`:
  - `id`, `title`, `source_url`, `folder_map`, `output_dir_override`, `active`,
  `refresh_interval_minutes`, `last_queued_at_ms`, `created_at_ms`, `updated_at_ms`
- Authenticated session material:
  - one-shot jobs and saved subscription rows should be able to reference explicit operator-managed session inputs,
  - accepted import forms should include raw cookie headers, Netscape cookie files, browser-export JSON cookie blobs, and explicit cookie-file paths,
  - secrets must remain redacted in logs and excluded from durable `job.params_json`.
- Planned archive-expansion tables:
  - `library_container`:
    - `id`, `kind` (`playlist`, `subscription`, `folder`, `channel`, or similar),
      `source_key`, `display_name`, `created_at_ms`, `updated_at_ms`
  - `library_item_container`:
    - `item_id`, `container_id`

- Current Media Library UX should remain list-first for large archives even before the normalized
  `library_container` tables exist. Until those tables are introduced, the frontend may infer
  provider and container semantics from source URI plus storage-relative path segments so operators
  can still distinguish playlist/subscription/folder/single-file rows.

- Legacy reconciliation reports are written as local JSON artifacts under the app-managed derived tree so large/NAS-backed archive analysis remains read-only and inspectable.

Additional tables (planned; large-subscription UX hardening):

- `youtube_subscription_group`:
  - `id`, `name`, `created_at_ms`, `updated_at_ms`
- `youtube_subscription_group_member`:
  - `group_id`, `subscription_id`
- (optional) backoff fields (either in `youtube_subscription` or a separate state table):
  - `consecutive_failures`, `last_error_at_ms`, `next_allowed_refresh_at_ms`

## 4) Job System

Requirements:

- Durable: jobs resume after restart.
- Non-blocking: UI subscribes to job updates.
- Inspectable: each job has logs and artifacts.
- Recovery: support a **Safe Mode** startup path that disables auto-refresh and heavy background work so users can always export/manage their data.
- Shared window data should be retained and reused where safe so pane switches do not refetch or recompute unchanged state.
- Contention-tolerant runtime behavior is a design requirement:
  - heavy external CPU load from other apps/models is expected on operator machines,
  - UI-thread work must stay minimal even when local workers or third-party tools are saturated,
  - long scans, indexing, diagnostics reads, and archive operations should prefer bounded, resumable, and observable execution over large eager passes.

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
- Diagnostics and startup surfaces should expose numeric progress where practical and should provide richer state snapshots for support and LLM-assisted analysis.

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
- Phase 2 baseline: `diarize_local_v1` (resemblyzer partial embeddings + clustering) writes `speaker` labels into subtitle JSON. In Windows "full" installers this pack is bundled; explicit install remains for slim/dev paths.

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
   - Phase 2 baseline: Spleeter 2-stem separation via bundled Python pack in Windows "full" installers (explicit install remains for slim/dev paths; no silent background downloads).
2. For each speaker segment:
   - translate text,
   - TTS to English using a selected voice per speaker,
   - time-stretch/align to fit segment window.
3. Mix generated speech with `background.wav`.
4. Loudness normalize + export final dub audio.

Phase 2 preview implementation notes (current):

- TTS preview: `tts_preview_pyttsx3_v1` renders per-segment wavs + a manifest (system TTS; quality varies by OS).
- Mix preview: `mix_dub_preview_v1` overlays TTS segments onto the separation background stem into a single wav, but falls back to the source-media audio when no background stem is available so preview generation does not hard-fail under separation/runtime contention.
- Mux preview: `mux_dub_preview_v1` muxes the preview dub audio onto the original media into an MP4.
- User-facing exports are separated from working artifacts:
  - working artifacts remain under `derived/items/<item_id>/...`
  - exported deliverables default to `<download_root>/localization/en/<media-stem>/`
  - the separate dubbed audio track remains the working `mix_dub_preview_v1.wav`; the exported/muxed MP4 embeds that dubbed audio into video
- Localization Studio should auto-prefer the latest translated English track for dubbing, benchmarking, experimental backend runs, and A/B preview actions, and should surface a compact workflow/readiness map so operators can see track/runtime state before queueing jobs.
- The shipped localization path should stay stage-explicit and inspectable:
  - source import/select,
  - subtitle/ASR readiness,
  - translated-track readiness,
  - speaker/reference readiness,
  - generated speech artifacts,
  - voice-preserved or experimental-backend artifacts,
  - mix artifact,
  - muxed MP4 artifact,
  - deliverable/export surface.
- Direct speech-to-speech systems (for example SeamlessExpressive-, Translatotron-, or TransVIP-style families) are useful research references, but they should remain future R&D or benchmark lanes rather than the default shipped path until they satisfy local packaging, operator-control, and artifact-inspection requirements at the same level as the staged cascade.

Voice-preserving approach (core feature):

- Use a voice conversion / dubbing system that preserves speaker identity per diarized speaker track.
- Must include:
  - ability to fall back to non-cloned voices,
  - strong logging/redaction + export provenance,
  - deletion controls for any stored voice representations.
- Reusable voice templates should be stored in app data, copy their reference clips into app-managed storage, and apply back onto per-item speaker settings so existing jobs do not need a separate template-aware request format.
- Current reusable-voice layers on top of reusable templates:
  - reusable cast packs that group template speakers into recurring series roles,
  - multi-reference speaker profiles with 1..N reference clips and backward-compatible single-reference fallback,
  - advisory auto-match suggestions for diarized speakers (non-destructive, operator-reviewed),
  - style/prosody presets, pronunciation locks, hybrid clone-vs-standard-TTS routing, and subtitle-aware prosody toggles passed through one unified speaker settings layer,
  - voice QC reports for both reference quality and output quality,
  - batch dubbing orchestration that applies cast/template settings to many items,
  - A/B preview variants stored as separate artifacts before final selection,
  - export modes for speech stem, background stem, final mix, and alternate versions,
  - cross-episode voice memory plus character libraries as separate reusable asset classes,
  - reference cleanup manifests and cleaned-reference reuse under per-item voice artifact folders.
- Hardening requirements for the current voice stack:
  - artifact-browser job/status/log resolution must be keyed by artifact identity, including variant label, track id, and mux container where applicable,
  - artifact rerun helpers must accept and preserve variant/base context instead of assuming the base artifact path,
  - reference cleanup storage must use collision-safe speaker keys and stay backward-compatible with previously written cleanup manifests,
  - applying cleaned references must support non-destructive multi-reference reuse,
  - batch dubbing item selection must page through the full library and keep selections stable without hidden caps.
- Voice-backend modernization strategy:
  - keep the current OpenVoice V2 + Kokoro path as the managed default until benchmark evidence supports a change,
  - add a built-in backend catalog with descriptors for managed and experimental candidates,
  - add recommendation logic keyed by source language, target language, performance tier, reference availability, and operator goal,
  - add explicit BYO adapter configs for experimental backends that the app should not auto-install,
  - add a benchmark lab that evaluates existing voice artifacts and variants before backend promotion decisions,
  - add item-scoped voice plans so recommendation and benchmark outcomes become durable operator choices,
  - add ranked reference-bundle curation so multi-reference profiles are evidence-driven rather than ad hoc,
  - add explicit experimental render runs so configured BYO backends can produce standard manifests for downstream VoxVulgi workflows,
  - add bounded batch experimental runs so backend experiments can be repeated over one operator-selected item set,
  - add durable benchmark compare history plus leaderboard export artifacts,
  - add backend-specific starter recipes so known OSS adapter families are bootstrapable without hand-authoring every command,
  - add reusable template/cast-pack backend defaults so benchmark winners can be carried forward beyond one item.
- Dubbing-control expansion remains operator-directed; the app should not add content-judgment or censorship workflows as part of these features.

Operator-flow implementation requirements:

- Localization Studio should expose one explicit localization-run contract instead of relying on implicit background starts after import alone.
- If the UI supports auto-queueing from import, it must still show:
  - what will run,
  - which stage is active,
  - what prerequisites are still missing,
  - where the resulting outputs will appear.
- Item handoff from import -> current localization item should be visible inside Localization Studio rather than hidden behind a separate Media Library navigation step.

R&D plan: see `governance/spec/VOICE_PRESERVING_DUBBING_RD_PLAN.md`.
Tooling landscape research: see `governance/spec/VOICE_DUBBING_TOOLING_LANDSCAPE_2026.md`.
Research refresh corpus: see `governance/research/voice_cloning_20260308/`.
Localization pipeline refresh corpus: see `governance/research/localization_pipeline_20260312/`.

Voice-backend catalog design:

- Add a new engine module to expose a typed catalog of backends, including:
  - `id`, `display_name`, `family`, `mode`, `install_mode`
  - code-license and weights-license posture
  - supported language directions
  - GPU recommendation and reference expectations
  - strengths, risks, and recommendation notes
- The catalog should include:
  - managed backends already shipped by VoxVulgi,
  - experimental built-in research candidates,
  - operator-registered BYO adapters.
- Diagnostics should render this catalog together with current readiness state.
- Localization Studio should render a recommendation summary and make the currently preferred backend family explicit.

Voice benchmark lab design:

- Add a new engine module that can:
  - discover current-item voice output artifacts and variants,
  - compute a stable benchmark report with local metrics,
  - emit both JSON and Markdown reports under item artifact directories.
- Candidate metrics:
  - rendered segment coverage,
  - converted segment ratio where available,
  - duration fit against subtitle timing windows,
  - silence/clipping/noise warnings,
  - reference coverage and reference duration,
  - similarity proxies derived from local embeddings or existing QC metrics,
  - a transparent weighted ranking score.
- The benchmark lab should work on already-rendered artifacts first; it should not require a second backend to be installed in order to be useful.
- Current implementation shape:
  - engine module `voice_benchmarks` discovers manifest-backed candidates per item/track,
  - it reuses existing local voice QC analysis for reference/output health and combines that with subtitle timing-fit metrics,
  - it writes durable `voice_benchmark_v1_<track>_<goal>.json` and `.md` artifacts under `derived/items/<item>/voice_benchmark/`,
  - Localization Studio loads and displays the top benchmark candidates for the currently selected goal,
  - immutable snapshot copies are archived under a history folder for the same item/track/goal,
  - leaderboard exports are written as durable JSON/Markdown/CSV artifacts built from the saved snapshot set.
- Next operational tranche:
  - allow batch render flows to optionally emit or refresh benchmark artifacts over a bounded item set.

Reference-curation design:

- Add a new engine module that can:
  - inspect current reference paths for an item speaker,
  - compute a ranked per-reference quality score using existing QC/audio-stat signals,
  - recommend a primary clip and a compact multi-reference bundle,
  - emit JSON/Markdown curation artifacts under item-scoped voice folders.
- Default application behavior should be non-destructive:
  - the operator may promote ranked order while preserving all references,
  - the operator may explicitly promote the compact recommended bundle when they want a tighter set.

Item voice-plan design:

- Add a durable per-item voice-plan record that stores:
  - goal,
  - preferred backend,
  - fallback backend,
  - selected candidate id and/or variant label,
  - optional operator notes.
- Localization Studio should:
  - show the active item plan,
  - allow promoting recommendation and benchmark outcomes into it,
  - use that plan as the default for subsequent experimental runs.

Experimental BYO adapter design:

- Store adapter configs in app-managed local config/state, not governance folders.
- Each adapter config should be explicit and operator-supplied:
  - executable or interpreter path,
  - working directory,
  - probe arguments,
  - environment overrides if needed,
  - capability metadata and operator notes.
- The app may probe adapters and surface readiness/errors, but must not silently install or update them.
- Current implementation shape:
  - adapter configs are stored in app config as local JSON, plus a cached probe-results file,
  - Diagnostics provides explicit save/probe/remove controls for known BYO backend templates,
  - the backend catalog reads cached probe state so Diagnostics and Localization Studio can distinguish `available_via_byo`, `byo_configured_unprobed`, `byo_ready`, and `byo_probe_failed`.
- Current execution shape:
  - adapter configs support an explicit render-command template with placeholder expansion for request/manifest/report/output paths,
  - experimental runs execute as queued jobs, not as inline UI invocations,
  - the run emits a standard manifest under `derived/items/<item>/tts_preview/<backend>/variants/<label>/manifest.json`,
  - existing artifact discovery, benchmark, mix, mux, QC, and export flows treat these runs like first-class candidates instead of a separate side channel,
  - bounded batch experimental queueing reuses the existing item-set picker pattern so multiple items can be evaluated against one or more ready BYO adapters under one batch ID,
  - starter-recipe definitions now provide backend-specific default probe/render command templates and bootstrap notes for known OSS adapter families,
  - reusable voice templates and cast packs now also store backend-default metadata so benchmark-winner promotions can seed later item plans during apply.

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
- Browser-export JSON cookie blobs and Netscape cookie files should be normalized into yt-dlp-compatible cookie files inside app-managed short-lived paths rather than assumed to already be in the correct format.
- Full installers may ship with bundled external tools. If the app bootstraps or downloads tools at runtime (e.g., slim installers), it must be explicitly user-initiated and disclosed.
- Logs must redact tokens/cookies and avoid storing secrets in durable job params; prefer short-lived files or OS keychain.

Phase 1 implementation status (2026-02-22):

- URL ingest is implemented as a `download_direct_url` job with provider routing:
  - `direct_http_v1` for direct media asset URLs (strict http/https),
  - `youtube_yt_dlp_v1` for YouTube and other webpage video links (yt-dlp expand + download).
- yt-dlp is bundled in Windows full installers; Diagnostics can install it if missing (network egress is user-initiated; jobs do not auto-download tools during execution).
- Default download presets should prefer MP4-compatible format selection, and yt-dlp execution should request MP4 merge/remux where supported so final containers are predictable by default.
- Image/archive providers should prefer JPEG defaults when multiple equivalent encodings are available and JPEG is the practical archive target; avoid surprising WebM-first or similarly unsuitable defaults.
- Instagram batch ingest expands instagram.com URLs (posts/reels/stories/profiles) into direct media asset URLs where possible, then downloads via `direct_http_v1` into `downloads/instagram/` by default (optional session cookie header for private content).
- Planned archive additions:
  - Pinterest board/folder crawl support should plug into the existing crawler-style image archive flow.
  - Instagram recurring archive targets should reuse the subscription/interval model already established for YouTube where practical.
- Downloaded media is imported into `library_item`, provenance is persisted in `ingest_provenance`, and downloads are grouped via `job.batch_id` for UI batching.
  - Privacy hardening: cookie headers are not persisted in `job.params_json` and browser-cookie usage is opt-in via explicit Library toggles.

Phase 1 extension status (2026-02-25):

- Added persistent YouTube subscriptions in SQLite (`youtube_subscription`) with a per-subscription folder map.
- Added per-subscription refresh interval (`refresh_interval_minutes`) so users can control how often each subscription should be refreshed.
- Queue-all-active honors interval gating by comparing `last_queued_at_ms` against each subscription's `refresh_interval_minutes`; users can still queue a specific subscription directly.
- Queueing a subscription expands its URL(s) through the existing provider pipeline and applies subscription-specific output mapping:
  - default mapped path: `downloads/video/subscriptions/<folder_map>/`
  - optional absolute output override per subscription (`output_dir_override`)
- For subscriptions that already point at an existing archive folder, refresh logic should reconcile already-downloaded items against that folder and seed/refresh dedupe state where practical before queueing new media.
- Added JSON export/import for subscription portability:
  - export path is user selected in desktop UI,
  - import uses URL-keyed upsert (`source_url`) and keeps existing rows not present in the import file.
- Subscriptions are loaded from DB whenever the Library page mounts, so pane/window switches do not clear loaded subscription state.
- Legacy reconciliation now also supports the old 4KVDP app-state SQLite:
  - auto-detect the largest Local AppData 4KVDP SQLite store when available,
  - correlate stored `dirname` basenames against the selected legacy root,
  - classify managed subscription/channel rows vs playlist rows, then separate those from unmatched manual folders and loose root files,
  - import managed rows directly into `youtube_subscription` plus `youtube_subscription_group` memberships,
  - seed VoxVulgi archive files from legacy `subscription_entries` so refresh jobs inherit dedupe state without touching the NAS.

Responsiveness hardening:

- Startup log-pruning is best-effort background work (runner boot does not block app launch on log scan/delete).
- URL/Instagram batch enqueue path avoids blocking pre-expansion on the invoke/UI thread; expensive extraction work is deferred to job execution.

Subscription export JSON shape (v1):

```json
{
  "schema_version": 1,
  "exported_at_ms": 0,
  "app": "VoxVulgi",
  "subscriptions": [
    {
      "title": "My Channel",
      "source_url": "https://www.youtube.com/@example/videos",
      "folder_map": "my_channel",
      "output_dir_override": null,
      "use_browser_cookies": false,
      "active": true,
      "refresh_interval_minutes": 60
    }
  ]
}
```


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

- Bundle is a zip that is safe-by-default and redacts secrets/PII:
  - `manifest.json`: app/engine/os, storage summary, models inventory summary, DB schema version + table counts, recent jobs (<= 200) + recent failed jobs (<= 20), retention policy, and a minimal config summary.
  - `storage.json`: byte breakdown for library/derived/cache/logs/DB.
  - `jobs_failed.json`: recent failed jobs (<= 20) with redacted errors.
  - `logs/jobs/*`: redacted per-job JSONL logs for up to 10 failed jobs (including rotated backups); each log file is truncated to 2 MiB.
- Redaction rules:
  - redact values for JSON keys containing `cookie`, `authorization`, `token`, `secret`, `password`, `api_key` (replace with `<redacted>`),
  - in free text, redact bearer tokens, reduce URLs to origin only, and redact absolute paths (replace with `<redacted_path>`).

### 6.3 Privacy

- Default: no telemetry.
- If telemetry is added later:
  - opt-in,
  - TLS only,
  - no IP logging,
  - publish a clear "what we send" list.

### 6.4 Startup and performance traces

- Capture local-first trace sessions for:
  - startup phase timings,
  - pane activation latency,
  - heavyweight background tasks,
  - major resource snapshots and failures.
- Traces should be readable from Diagnostics and exportable in a deterministic form for support/debug use.
- Tool state should be represented explicitly in operator-facing UI and traces, distinguishing:
  - bundled,
  - hydrated into app data,
  - installed,
  - loaded,
  - ready.
- Diagnostics should also be able to assemble one coherent app-state snapshot spanning startup state, storage roots, tool/model readiness, queue/library counts, recent trace rows, and feature-health summaries.
- Snapshot exports should emit both JSON and Markdown from the same captured state so support handoff and LLM analysis use the same underlying point-in-time record.

### 6.5 Desktop shell interaction rules

- Drag-region behavior should be restricted to the intended chrome/background layer and must not swallow normal content interaction.
- Corner-resize affordances should have a clear reachable hitbox inside the practical app bounds.
- App movement should use an explicit move affordance or clearly bounded drag region so operators can distinguish shell movement from content interaction.
- The shell move affordance and native window controls should be rendered as one top-right chrome cluster so frameless-shell layout changes do not split or relocate core window actions.
- Frameless maximize/fullscreen handling must keep the native desktop window bounds synchronized with the visible surface so no invisible blocked area sits over neighboring apps in side-by-side layouts.
- Dense archive panes should prefer panel-local scrolling/list behavior over clipped actions or invisible controls.
- Where a dense table cannot fit at practical widths, the panel scroll surface should stay local to that card/pane and action columns should remain visible without forcing the operator to guess where controls went.

## 7) Testing Strategy

- Unit tests for:
  - subtitle parsing/rendering
  - translation constraints (CPS/line breaks)
  - job state machine transitions
- Integration tests for:
  - ffprobe/ffmpeg wrappers
  - SQLite migrations
  - "golden file" subtitle outputs
