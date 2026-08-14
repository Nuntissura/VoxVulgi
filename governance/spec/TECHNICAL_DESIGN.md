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
- Localization Studio's primary home surface should be a setup-first workbench backed by the existing localization feature root from Options. The operator-visible setup owns source selection, subtitle/dub output target choices, a source-copy export checkbox, Start/Stop controls, and percentage progress.
- The localization run orchestrator owns voice-cloning runtime preflight. When a dub continuation reaches the voice-preserving stage and the managed OpenVoice/Kokoro pack is missing, the orchestrator queues the Phase 2 voice setup job with the original localization request embedded as a resume payload. The setup job re-enters the localization run after successful installation, so UI surfaces only need to show `voice_setup` as a normal queued stage rather than sending operators to Diagnostics.
- Localization exports should place user-facing files together under the resolved item output folder with language-marked filenames and an optional copied source media file.
- Explicit consent mechanisms must not be implemented.
- Anti-abuse controls must not be included.
- These features must not appear in development or the final product. Users must comply with all local regulations.

### 2.1 Windows "full" installer (bundled dependencies)

To minimize setup friction and support offline use, the Windows "full" installer bundles the core local toolchain for Phase 1 + Phase 2 (FFmpeg/ffprobe, yt-dlp, a supported JavaScript runtime for yt-dlp's current extractor path, whisper model(s), portable Python + venv, and Phase 2 packs/models).

Normative status (operator decision 2026-07-31; see PRODUCT_SPEC.md 8.1.8):

- The batteries-included payload is mandatory for public releases; the "full" installer is the public default, and slim payloads are development-only.
- The payload must contain ALL models and dependencies for the complete default localization pipeline: the default ASR model (currently `whispercpp-large-v3-q5_0`), the diarization pack, the source-separation pack, the TTS and voice-conversion models with their populated Hugging Face cache (so `HF_HUB_OFFLINE=1` resolution succeeds on first run with no cache misses), portable Python with all pinned wheels, and FFmpeg/ffprobe.
- Python pack installs for the default path must resolve from bundled wheels (`pip --no-index --find-links <bundled wheels> --require-hashes`); no PyPI network access may be required on first run.
- First run must be able to pass the complete default localization workflow offline with zero downloads; release verification must include an offline first-run check of that path.
- Bundled models/backends remain user-swappable later via in-app surfaces; swapping is optional and never required.

Implementation notes (desktop):

- The installer includes an `offline/` resource payload:
  - `offline/manifest.json`
  - `offline/payload.zip` (contains `tools/`, `models/`, and `cache/huggingface/`)
- On first run, the app extracts the payload into the user app-data dir and writes a marker (`config/offline_bundle_applied_v1.json`) so it only applies once per bundle id.
- Build policy: routine app builds and UI/backend verification should reuse an existing verified `src-tauri/offline/payload.zip` when bundled dependency inputs did not change. Payload refresh is required for explicit release/full-refresh builds, changed dependency inputs, missing/stale payloads, or operator-requested full dependency refreshes.
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
- `library_download_lineage`:
  - `item_id` (primary key), `service`, `origin_kind`, `work_track`, `source_job_id`, `source_batch_id`, `source_subscription_id`, `item_created_at_ms`, `created_at_ms`, `updated_at_ms`
  - `source_job_id` is durable text rather than a deleting foreign key because terminal-job cleanup must not erase library origin,
  - `service`, `origin_kind`, and `work_track` are independent dimensions: a manually submitted YouTube playlist can be foreground `youtube_single` work while retaining `origin_kind='playlist'`,
  - new downloaded items receive lineage during the successful job-to-library handoff; historical backfill accepts only exact structured job/item evidence and leaves missing or conflicting history `unclassified`,
  - indexes support canonical single-history paging (`service`, `origin_kind`, item time), source-job repair, subscription inspection, and work-track diagnostics.
- `media_source_identity`:
  - canonical key is `service + media_id`; URLs, job attempts, physical paths, and import generations are aliases or observations rather than identity,
  - one identity may reference one current physical library item while retaining historical observations needed for repair and rollback.
- `media_source_membership`:
  - `service`, `media_id`, `source_subscription_id`, `source_kind`, `source_url_snapshot`, `source_title_snapshot`, `evidence_kind`, `created_at_ms`, `updated_at_ms`,
  - `source_kind` distinguishes playlist, `/videos`, `/shorts`, channel page, direct video, and imported archive source without changing canonical media ownership,
  - membership uniqueness prevents the same identity/source pair from multiplying while preserving many memberships for one canonical video.
  - schema migration backfills pre-membership `media_source_association` rows by joining the current subscription source URL/title; the backfill is additive, idempotent, and never changes media, jobs, archive files, subtitles, or subscription configuration.
- `media_import_evidence`:
  - `library_item_id`, `service`, `media_id`, `evidence_kind`, `source_record_key`, `source_path_snapshot`, `source_url_snapshot`, `match_state`, `details_json`, `created_at_ms`,
  - `match_state` distinguishes exact, ambiguous, and unresolved evidence; only an exact single-candidate match may bind an imported item automatically,
  - third-party database and export sources remain read-only; evidence is copied into VoxVulgi-managed storage with its source record identifiers.
- `media_cleanup_run` / `media_cleanup_candidate` / `media_cleanup_action`:
  - persist resumable inventory stage, bounded progress, evidence strength, candidate members, proposed keeper, reclaimable bytes, review decision, quarantine action, and rollback state,
  - inventory and apply are distinct commands; permanent deletion is not an inventory or reconciliation side effect.
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
- Source grouping is implemented from canonical `media_source_membership` rows. Storage folders remain physical observations or preferred landing locations and do not become duplicate ownership containers.

- Current Media Library UX should remain list-first for large archives even before the normalized
  `library_container` tables exist. Presentation-only provider/folder hints may still be inferred
  when canonical lineage is absent, but frontend source URI or storage-path inference must never
  decide whether an item is a single, subscription, playlist, or channel download. Those origin
  labels come from `library_download_lineage`; older unclassifiable rows are labeled `Unknown` and
  remain available in Media Library without entering canonical single-video history.
- The main Media Library query is a backend canonical projection. Lifecycle, search, media type,
  canonical source identity, single-video lineage, and sort predicates execute before
  `LIMIT`/`OFFSET`, and the response returns `filtered_total` independently from the bounded page.
  Canonical source service prefers durable download lineage, then exact imported
  `media_source_identity`; unresolved imports remain local/unclassified rather than being guessed
  from a folder name. Frontend grouping is presentation-only and may operate on the returned page,
  but frontend filtering must not redefine the canonical matching set.

- Imported archive reconciliation reports are written as local JSON artifacts under the app-managed derived tree so large/NAS-backed archive analysis remains read-only and inspectable.

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
  - persist independent product tracks for `youtube_single`, `youtube_recurring`, `instagram`, `other_video`, `image_archive`, and `localization`, with indexed per-track queued fetches and real scheduler-consumed budgets,
  - classify and stamp a job at enqueue from job type plus structured service/origin context; preserve the older `lane` field during compatibility migration rather than rewriting or re-enqueueing canonical jobs,
  - use provider/resource gates in addition to track budgets: the two YouTube tracks retain one active direct-download slot each by default, but every aggregate YouTube process claim/start is separated by the shared randomized 5-10 second start gate,
  - choose foreground YouTube first when both tracks become eligible at one gate opening, then alternate continuously eligible tracks so recurring work keeps a bounded background share,
  - provider holds are scoped: the YouTube circuit leaves both YouTube tracks queued while unrelated service tracks remain dispatchable.

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
- Diarization jobs accept an explicit speaker-count request: `auto`, `exact`, or `range`. The request is serialized into direct diarization jobs and localization-run continuations, constrains baseline clustering, is passed to pyannote BYO as `num_speakers` / `min_speakers` / `max_speakers`, and is recorded in `derived/items/<item_id>/diarize/diarization_report*.json`.
- Subtitle JSON v1 still stores one speaker label per subtitle segment. Overlap ratios, label confidence, and multi-label ownership remain future schema work; pyannote exclusive diarization output can be used as the assignment source when available.

### 5.4 Translate CC (JA/KO -> EN)

Inputs:

- source subtitle JSON (segments + timing)

Phase 1 implementation (confirmed):

- Backend: `translate_local` job uses Whisper.cpp **translate mode** on extracted audio and then aligns output text back onto the source segment windows (stable timings, same segment count).
- Glossary: versioned JSON documents with `source`, `target`, optional `context`, and optional `notes`. `config/glossary.json` is the global base; `derived/items/<item_id>/glossary.json` contains per-item overrides. Legacy JSON string-to-string maps remain readable and migrate on the next save.
- Translation jobs snapshot the effective global-plus-item glossary when queued. Only terms present in the source subtitle document are serialized into a bounded Whisper initial prompt, which is carried across decode windows; deterministic longest-key-first replacement remains a compatibility fallback when source terms survive in the translated output.
- Glossary import/export supports UTF-8 CSV (`source,target,context,notes`) and versioned JSON. Writes use the engine atomic-persistence helper.
- Translation style is stored per item in `derived/items/<item_id>/translation_style.json` as a versioned compound setting: `neutral`, `formal`, `informal`, or `custom`, plus `preserve`, `translate`, or `drop` honorific handling. Translation jobs snapshot the setting when queued so later UI edits cannot change an in-flight job.
- The style and honorific instructions share the existing bounded, carried Whisper initial prompt with source-relevant glossary terms. Conservative deterministic cleanup makes formal/casual punctuation observably different and removes only explicitly hyphenated known honorific suffixes in `drop` mode; ambiguous honorific translation remains decoder-guided rather than applying unsafe name/title substitutions.
- Localization pipeline presets use three layers: immutable built-ins generated by the engine, a versioned atomic custom catalog at `config/localization_pipeline_presets.json`, and an applied full-definition snapshot at `derived/items/<item_id>/localization_pipeline_preset.json`. Applying a preset writes the existing global batch-on-import store and the existing per-item translation-style store rather than introducing parallel setting authorities. Optional default voice template/cast-pack IDs are matched once translated speaker labels exist; the item snapshot records that application attempt so retries do not repeatedly overwrite later operator voice choices.
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
- Per-segment audio preview reuses the current item mix WAV without generating another artifact: the editor seeks to the subtitle window, confirms `play()` before showing active playback, and stops by observed media time. Missing/invalid media stays non-destructive and visible; playback is cleared on error, pause, natural end, re-click, item change, and unmount.
- WebView playback of local source and derived media uses Tauri's asset protocol with an empty static scope. Before constructing an asset URL, the native boundary canonicalizes the requested file and dynamically allows that exact path only when it is either the canonical library source for the requested item or a canonical descendant of that item's derived-output root; missing files, invalid item IDs, traversal, and unrelated paths are rejected. Broad filesystem asset scopes are forbidden.
- Mux preview: `mux_dub_preview_v1` muxes selected original/dubbed audio plus available subtitle tracks with the original video into an MKV.
- User-facing exports are separated from working artifacts:
  - working artifacts remain under `derived/items/<item_id>/...`
  - exported deliverables default to `<download_root>/localization/en/<media-stem>/`
  - the separate dubbed audio track remains the working `mix_dub_preview_v1.wav`; the exported/muxed MKV embeds selected original/dubbed audio and subtitle tracks into video
- Localization Studio should auto-prefer the latest translated English track for dubbing, benchmarking, experimental backend runs, and A/B preview actions, and should surface a compact workflow/readiness map so operators can see track/runtime state before queueing jobs.
- The shipped localization path should stay stage-explicit and inspectable:
  - source import/select,
  - subtitle/ASR readiness,
  - translated-track readiness,
  - speaker/reference readiness,
  - generated speech artifacts,
  - voice-preserved or experimental-backend artifacts,
  - mix artifact,
  - muxed MKV artifact,
  - deliverable/export surface.
- The localization-run orchestrator should use one shared next-stage decision point instead of scattered ad hoc follow-on queues.
  - translated English tracks without speaker labels should continue to diarization first,
  - translated English tracks with missing cloned-speaker references should generate/apply source-reference candidates before stopping,
  - only speakers still missing usable references after source extraction should stop at the voice-plan checkpoint with explicit missing-speaker notes,
  - once the voice plan is ready, dubbing can continue through mix and mux.
- The voice-plan checkpoint should expose an assisted reference-acquisition lane:
  - use diarized subtitle spans and source media audio to build candidate per-speaker reference bundles,
  - keep those candidates in item-managed voice-reference storage,
  - require explicit operator apply before they become active references,
  - preserve any existing manual multi-reference state unless the operator chooses to replace it.
- The basic reusable-voice path should be expressible as one compressed operator lane built on top of the existing reusable asset layers:
  - capture reusable voice from the current item speaker,
  - save it into an app-managed reusable asset,
  - apply it to a later translated item,
  - continue the dubbed preview from that applied state.
- Localization Studio should expose that lane as a first-class surface (`Reusable Voice Basics`) rather than only as scattered advanced tools:
  - choose current speaker,
  - generate/apply a source-based reference or manual reference,
  - save reusable voice memory,
  - apply an existing reusable voice,
  - continue the localization run from that state.
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
- Current implementation now also includes item-scoped generated speaker-reference bundles:
  - candidate clips are extracted from diarized source-media spans,
  - the candidate bundle is stored under the current item's managed voice-reference area,
  - the operator can apply it as append-or-replace into the current speaker settings before continuing the staged localization run.
- Voice-preserving truthfulness requirement:
  - report and manifest state must distinguish real conversion, partial conversion, and plain TTS fallback,
  - operator surfaces must not present plain TTS fallback as if it were a successful cloned-voice result,
  - if fallback remains allowed for resilience, that fallback must be visible and reviewable end to end rather than hidden behind a generic "dub succeeded" state.
- Current runtime contract for truthfulness:
  - TTS manifests/reports should carry per-segment `voice_clone_intent` and `voice_clone_outcome` metadata,
  - run-level artifact metadata should carry `voice_clone_outcome` plus requested/converted/fallback/standard-TTS segment counters,
  - current run-level outcomes are `clone_preserved`, `partial_fallback`, `fallback_only`, and `standard_tts_only`,
  - the bridge/frontend should consume that metadata directly for the item voice plan, Localization Run, Outputs, and benchmark surfaces instead of inferring clone success from file existence alone.
- Voice-backend modernization strategy:
  - select managed CosyVoice 2 as the default only when its complete offline pack passes the byte-level readiness gate; otherwise use managed OpenVoice V2 + Kokoro,
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
- Current operator-surface sync requirements:
  - the main Localization Studio flow should keep reusable-voice basics visible before advanced reusable asset abstractions,
  - current-item/run/output surfaces should show clone-truth labels from runtime metadata rather than generic dub-success messaging,
  - benchmark cards may add detail, but clone-vs-fallback truth must already be visible on the main item path.
- Item handoff from import -> current localization item should be visible inside Localization Studio rather than hidden behind a separate Media Library navigation step.
- The Localization home surface should expose a compact first-screen orientation layer that makes the current item, recommended next action, and latest preview or deliverable path obvious without a second navigation hop.
- Non-blocking startup/recovery state should use compact shell-level status affordances when the app is otherwise usable; expanded cards or modal detail views should be reserved for Safe Mode, active startup failure, or explicit operator request for details.

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
- A supported JavaScript runtime is part of the downloader toolchain, not an optional afterthought:
  - prefer a bundled/pinned Deno runtime for installer-state reliability,
  - surface JS-runtime readiness in Diagnostics alongside yt-dlp,
  - when a runtime is available, prefer yt-dlp's documented default YouTube client strategy instead of forcing brittle custom `player_client` overrides.
- Default download presets must select the best compatible source streams without constraining selection to MP4, and every yt-dlp video execution must request MKV merge/remux so the final managed container is predictably MKV.
- yt-dlp video execution must embed selected/available subtitle tracks into the MKV and remove transient subtitle sidecars after successful embedding. Explicit subtitle-only export remains a separate workflow.
- Direct-HTTP video assets that arrive as MP4 or another supported container must be staged and remuxed with stream copy into MKV while preserving available video/audio/subtitle streams. A remux failure is a visible failed/attention result; the MP4 staging file must not be imported as a successful new managed output.
- Existing MP4 files remain fully recognized by library import, availability, canonical identity, dedupe, playback, reveal, migration, repair, and cleanup-inventory logic. The MKV output policy never authorizes conversion, deletion, or redownload of historical MP4 media.
- Output-container policy is enforced at the engine/execution boundary. A persisted legacy preset or queued job requesting MP4 cannot override MKV finalization; migration updates saved defaults and UI copy without invalidating historical MP4 rows.
- Machine-specific archive roots remain configuration, not source-code constants. A move from UNC to a directly attached/mapped path requires live path identity/reachability validation and machine-local config update; no drive letter is hardcoded into portable product code or repo authority.
- Image/archive providers should prefer JPEG defaults when multiple equivalent encodings are available and JPEG is the practical archive target; avoid surprising WebM-first or similarly unsuitable defaults.
- Instagram batch ingest expands instagram.com URLs (posts/reels/stories/profiles) into direct media asset URLs where possible, then downloads via `direct_http_v1` into `downloads/instagram/` by default (optional session cookie header for private content).
- Planned archive additions:
  - Pinterest board/folder crawl support should plug into the existing crawler-style image archive flow.
  - Instagram recurring archive targets should reuse the subscription/interval model already established for YouTube where practical.
- Downloaded media is imported into `library_item`, provenance is persisted in `ingest_provenance`, and downloads are grouped via `job.batch_id` for UI batching.
  - Successful downloads also persist `library_download_lineage` from execution context before the item is eligible for origin-specific projections. Job cleanup preserves this row.
  - Canonical downloaded-single-video queries select `service='youtube' AND origin_kind='single'`; the bounded canonical count/page never calls frontend path heuristics. The exact unclassified-legacy diagnostic is a separate read-only Tauri command because its evidence predicates require a full `library_item` scan; the frontend starts that count without awaiting it, preserves explicit loading/unavailable state, and never lets it delay canonical history navigation.
  - Historical lineage backfill runs outside the schema-migration transaction in bounded, resumable batches with a durable checkpoint/receipt; schema migration only creates the additive table/indexes so a large database does not incur an unbounded startup write.
  - Every job persists one canonical product `track`; enqueue receipts, Jobs labels, scheduler fetches, provenance lineage, and diagnostics consume that value rather than reclassifying in React.
  - Jobs/Queue recovery truth must be derived from backend canonical job/batch queries, not from the current page of rendered rows.
  - The initial Jobs projection must be bounded and current-work-first: return canonical status totals separately from the one requested `Now`, `Needs attention`, or `History` preview so inactive history is not fetched on every active poll and a rendered subset is never presented as the full store. Canonical totals must use indexed status counts, and collapsed overview rows must not fan out batch-detail aggregation calls; canonical batch detail loads only when the operator expands that batch.
  - Initial Jobs reads must use one read-only connection and indexed predicates. They must not perform per-row/per-URL library or provenance hydration; persisted `target_title`, batched item context, and URL/video-ID fallback are the display path, with historical title repair kept explicit.
  - Jobs overview returns canonical per-track queued/running totals and effective limits separately from the bounded requested preview. A rendered filter/page count is always labeled as preview-scoped.
  - Runtime track state includes configured/effective budgets, pause/hold reason, active counts, and shared provider-gate next-eligible state. Settings updates validate and persist in one transaction, then return the reread canonical state.
  - Successful enqueue UI receipts are constructed from the persisted `JobRow` values returned by the command and include job IDs so the operator can trace the attempt before any later list refresh.
  - Retry operations should persist lineage for new attempts (`retry_of_job_id` / `retry_replacement_job_id` or equivalent) and use best-effort historical inference only when explicit lineage is missing.
  - Canonical batch dry-run, retry, and repair execute behind an in-process background-task receipt (`request_id`, mode, batch query, state, timestamps, summary/error). The start command returns before batch inspection/re-enqueue work begins; a bounded status command reports completion. Concurrent starts for the same mode and batch query reuse the running receipt, completed receipts are pruned, and engine-level canonical scope, lineage, chunking, and idempotency remain authoritative.
  - Batch inspection APIs should return canonical health counts, latest-attempt state, retryable/unresolved counts, and attempt history for all matching batch rows.
  - Deleting queue/history rows must remain separate from deleting media, library metadata, subscriptions, playlists, or third-party exports.
  - YouTube auth uses the current global Options auth at execution/retry time for old queued/waiting/retried jobs. Cookie Editor `.js`/JSON exports, Netscape cookie text, cookie headers, and cookie-file paths are accepted inputs and normalized internally.
   - Browser-cookie auth source selection defaults to Firefox when browser cookies are enabled without an explicit saved source. In the operator environment, automated credential checks and runtime auth verification use Firefox only and must not launch, inspect, or source credentials from Chrome, Edge, Opera, or another browser. Other product-supported sources remain outside that automated test path. The global Options browser source is persisted separately from manual cookie material and is resolved at job execution time when no current global cookie is saved. A current global browser source supersedes stale per-job cookie secrets on already queued YouTube work so a successful replacement session cannot be undone by imported rows.
  - Browser-session setup is a three-step external-browser flow: launch YouTube in the explicitly selected supported browser on a user click, let the user complete Google/YouTube sign-in in that normal browser, then run exact-source preflight. Do not host Google sign-in in the Tauri WebView. Browser-session preflight must execute the selected `--cookies-from-browser` source without the normal anonymous fallback retry; otherwise a public URL can falsely report a connected account. Google OAuth is reserved for a future YouTube Data API integration and is not a yt-dlp download-auth path.
  - `YoutubeAuthConfig` persists `last_verified_at_ms` and `reconnect_required_at_ms` in addition to the browser/manual source. Saving a new source clears both timestamps; successful preflight sets verified and clears reconnect-required; rejected preflight or a corroborated runtime auth block clears verified and sets reconnect-required. The source remains stored so the matching auth circuit can hold recurring work. The app never deletes or signs out the user's real browser session.
  - Recurring YouTube work remains single-download concurrency, uses a 5-10 second pre-download delay, checks subscriptions one at a time with randomized inter-dispatch spacing, and defaults forced update-all to a bounded 25-subscription most-overdue tranche.
  - Foreground YouTube direct downloads use the same effective safety profile as recurring children: fragment concurrency 1, configured randomized 5-10 second pre-download sleep, current retry/throttled-rate/file-access settings, and current browser-session/auth resolution.
  - Foreground and background YouTube direct-download tracks have independent active slots so a long subscription transfer does not block one-off work, while a runner-owned shared start gate staggers all aggregate YouTube process starts and prevents same-tick bursts.
  - The existing corroborated/TTL-bound global YouTube auth block is a shared YouTube circuit breaker: queued YouTube refresh/download rows in both tracks remain queued while the matching account state is blocked, while Instagram, other-video, image-archive, and localization tracks remain dispatchable.
  - Failed Jobs rows render the shared classified state as the status headline (`Failed - <reason>`) with the required action adjacent and the raw engine error behind disclosure.
  - Privacy hardening: cookie headers are not persisted in `job.params_json` and browser-cookie usage is opt-in via explicit Library toggles.

Phase 1 extension status (2026-02-25):

- Added persistent YouTube subscriptions in SQLite (`youtube_subscription`) with a per-subscription folder map.
- Added per-subscription refresh interval (`refresh_interval_minutes`) so users can control how often each subscription should be refreshed.
- Queue-all-active honors interval gating by comparing `last_queued_at_ms` against each subscription's `refresh_interval_minutes`; users can still queue a specific subscription directly.
- Queueing a subscription expands its URL(s) through the existing provider pipeline and applies subscription-specific output mapping:
  - default mapped path: `downloads/video/subscriptions/<folder_map>/`
  - optional absolute output override per subscription (`output_dir_override`)
- For subscriptions that already point at an existing archive folder, refresh logic should reconcile already-downloaded items against that folder and seed/refresh dedupe state where practical before queueing new media.
- Per-subscription dedupe / "already downloaded" continuity state must live in VoxVulgi-managed app data and must not rely on the output folder remaining writable or stable. Imported output-folder archive files may be merged as migration input, but ongoing tracking state should be app-managed.
- Within one queued subscription-refresh cohort, channel-page, `/videos`, and `/shorts` sources are enumerated before playlists. Each candidate still takes the canonical present/active/missing preflight, so a healthy feed claims shared IDs first while a failed or unavailable feed cannot suppress a playlist recovery download.
- `youtube_subscription.source_status` is the durable lifecycle authority (`normal`, `unavailable`, `deleted`) and remains separate from the existing `active` pause toggle. Schema fields also retain the status-change timestamp and source.
- The only write path to `deleted` or from `deleted` to `normal` is the validated manual-status command, attributed as `operator` or `assistant`. It sets `active=0` when deleted, cancels queued/running refresh jobs without deleting them, and preserves the subscription row plus all media/source metadata.
- Refresh failure recording recognizes only explicit HTTP 404 status forms as `unavailable`; generic not-found wording, empty tabs, network errors, authentication errors, and tool failures remain ordinary failure classifications. Refresh success returns `unavailable` to `normal` but cannot change `deleted`.
- All subscription refresh enqueue and execution boundaries defensively reject `deleted`, including direct, group, bulk, scheduler, and already-queued execution paths. Existing child video downloads are not deleted or rewritten.
- The subscription manager replaces its destructive primary Delete action with `Mark deleted` / `Restore subscription`, renders durable Deleted and Unavailable states, and always explains that a 404 URL does not prove its hosting channel was deleted.
- Current managed-state layout is `library/subscriptions/youtube/<subscription_id>/voxvulgi_youtube_archive.txt` under the VoxVulgi app-data root; `output_dir_override` continues to control where downloaded media lands, but not where continuity state is persisted.
- Added JSON export/import for subscription portability:
  - export path is user selected in desktop UI,
  - import uses URL-keyed upsert (`source_url`) and keeps existing rows not present in the import file.
- Subscriptions are loaded from DB whenever the Library page mounts, so pane/window switches do not clear loaded subscription state.
- Imported archive reconciliation also supports the old 4KVDP app-state SQLite:
  - auto-detect the largest Local AppData 4KVDP SQLite store when available,
  - correlate stored `dirname` basenames against the selected imported root,
  - classify managed subscription/channel rows vs playlist rows, then separate those from unmatched manual folders and loose root files,
  - import managed rows directly into `youtube_subscription` plus `youtube_subscription_group` memberships,
  - seed VoxVulgi-managed subscription archive state from imported `subscription_entries`, with one-time merge from any older output-folder archive file when present, so refresh jobs inherit dedupe state without requiring ongoing NAS-side state writes.
  - ingest per-download URL identity evidence through the verified `download_item -> media_item_description -> url_description` relation, normalize UNC path spelling before exact comparison, and retain ambiguous/unresolved matches without mutating the third-party database,
  - bind each exact imported item to canonical source identity and all known source memberships before future subscription discovery or one-off enqueue can materialize another copy.
- Unified library and duplicate prevention:
  - imported and newly downloaded items use the same canonical identity, membership, lineage, and library queries; import generation remains provenance, not a product-library partition,
  - discovery from playlists, `/videos`, `/shorts`, channel pages, and direct URLs records membership before returning `active` or `present`, so duplicate suppression never loses source context,
  - a subscription folder map is a preferred destination only when no canonical physical item exists,
  - `library_item.file_status` is the durable per-physical-item lifecycle authority: `available`, internal `delete_pending`, or `operator_deleted`. It stores change time/source, delete method, and the latest exact authorized redownload job ID; lifecycle state is independent of filesystem present/missing/unreachable observation,
  - deletion accepts only explicit canonical item IDs, writes `delete_pending` before the filesystem handoff, uses the OS Recycle Bin by default or an explicitly selected permanent mode, preserves every identity/membership/history row, and returns a per-item receipt. Unreachable storage never becomes a successful deletion,
  - all preflight, enqueue, subscription discovery, retry, batch repair, and execution paths suppress `delete_pending` and `operator_deleted`. An explicit selected-item manual-redownload path may replace the stored authorization with its newly created exact job ID; no aggregate or generic retry path receives this authority,
  - successful authorized import clears lifecycle state and authorization only after the replacement path is present. Failed, canceled, stale, or generic jobs leave the tombstone intact,
  - Video Archiver subscription detail queries `media_source_membership -> media_source_identity -> library_item` rather than output-folder prefixes and returns bounded available/deleted projections. Media Library applies lifecycle filtering in the backend before pagination and orders deleted rows last only in the explicit All projection,
  - Media Library applies search, media-type, canonical source, canonical-single, lifecycle, and sort predicates to the full backend set before pagination; its page receipt returns the exact filtered total and each item exposes the resolved canonical service used by source filtering,
  - queued work reconciliation is dry-run first and targets the full canonical queued set, not a rendered, filtered, or paginated subset,
  - reconciliation canonicalizes every queued direct YouTube URL, records every job's source association and subscription membership, groups by `service + media_id` across batches and tracks, cancels every group member when canonical media is present, and otherwise retains the valid canonical active claimant when one exists; without one it prefers channel-page, `/videos`, or `/shorts` discovery over playlist discovery per the source-priority contract, then the newest `created_at_ms`, then stable `id`, while canceling redundant attempts,
  - one immediate SQLite write transaction applies the selected queued-job status changes and retargets `media_source_identity.active_job_id` to the keeper; canceled job, batch, retry, association, and membership rows are retained as history,
  - the reconciliation receipt distinguishes scanned, unidentifiable, present-suppressed, keeper, duplicate-canceled, missing, and unreachable counts and proves that the full canonical set—not only a requested page—was processed,
  - direct YouTube dispatch performs an execution-boundary identity gate before changing the job to `running` or launching network/`yt-dlp` work. The gate atomically admits the current keeper only when it still owns or can acquire the canonical active claim; present media or another active queued/running owner suppresses the stale job, while missing and unreachable storage remain distinct and follow the existing explicit-repair/storage policy.
- Recoverable NAS duplicate cleanup:
  - inventory compares normalized canonical filesystem paths with the full `library_item` set before
    hashing; path reconciliation applies only unique one-to-one basename/canonical-source evidence,
    indexes unmatched physical media, and retains missing or ambiguous metadata for repair,
  - inventory starts with structured identity evidence, then groups unresolved files by size, bounded prefix/suffix digest, full-file digest, and optional byte comparison,
  - ffprobe metadata and decoded-frame fingerprints may rank or explain variants but cannot authorize automatic deletion,
  - scans use bounded low concurrency, durable checkpoints, pause/cancel controls, incremental SQLite commits, and path/read timeouts,
  - apply moves non-keeper files into an operator-visible quarantine, records a rollback manifest with source and quarantine paths plus memberships, atomically changes every preserved redundant `library_item.media_path` to the verified keeper path with its identity relink, and never replaces canonical association with a hardlink or symlink by default,
  - rollback restores the quarantined file, original redundant library path, and recorded identity links together; a database-handoff failure must compensate the filesystem move when possible or retain an explicit recoverable `attention` action,
  - permanent deletion is a separate explicit confirmation after quarantine validation.

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
- Diagnostics and the localhost agent bridge should expose one bounded read-only track runtime snapshot containing canonical per-track totals, configured/effective budgets, held reasons, and shared YouTube gate state. The UI, bridge, trace, and tests must consume the same engine contract.
- Jobs overview accepts an optional canonical track selector and applies it in the indexed backend query before its row limit. Track totals remain a separate unfiltered canonical aggregate. Jobs persist enqueue-time source display snapshots so UI metadata does not depend on a later subscription lookup.
- The Jobs frontend exposes that canonical track selector through one compact source filter rather than a second horizontal tab rail. Queue controls remain visible; scheduler budgets, per-track totals, and shared YouTube gate detail share one secondary disclosure backed by the same canonical runtime snapshot.
- Large Jobs groups and Video Archiver subscription-video projections use explicit bounded render windows with stable keys and visible `shown of canonical total` truth. `Load more` expands the local window by a fixed bound; filters, counts, retry, update-all, and cleanup actions continue to target canonical backend sets rather than the rendered slice. Panel-local scroll containers prevent document-height and horizontal-table growth from becoming page-navigation or WebView layout work.
- Video-file selection is controlled by stable canonical item IDs. Header selection is labeled `Select loaded`, mixed selections expose eligible available/deleted counts, and destructive commands return exact per-item outcomes. Delete/redownload controls are never marked as agent-safe actions and remain unavailable to the read-only headless audit bridge.
- The first bounded-render implementation remains dependency-free to preserve routine offline-payload reuse. A later measured transition to a headless virtualizer is allowed only if it preserves stable accessibility position/set-size metadata, selection, scroll restoration, audit identifiers, and canonical-set action semantics.
- A bounded active-job projection keyed by stable job ID serves Jobs and Video Archiver progress. Frontends reconcile changed fields without replacing unrelated rows; completed history refreshes only on terminal transitions or explicit refresh. Persisted job state is canonical even if an event/listener is missed.
- Canonical source identity is `service + normalized extractor/media ID`, stored independently from URL aliases, job attempts, library items, source associations, output paths, and physical-file observations. Identity claim and active-job suppression are transactional. Imported ambiguity is preserved for review rather than auto-merged.
- Imported media identity enrichment consumes copied read-only evidence and records exact, ambiguous, and unresolved outcomes. Exact linkage must be idempotent and resumable; conflicts never overwrite the existing canonical item automatically.
- Source membership is many-to-many and survives `active`/`present` preflight suppression, queue cleanup, retry, quarantine, rollback, and terminal-job cleanup.
- Download preflight is ordered and batch-capable, returning `ready`, `active`, `present`, `missing`, `storage_unreachable`, or `invalid` with evidence. Relocation verifies before atomically updating the path; redownload binds to the existing identity/item and attaches a new output only after success; removal is explicit and metadata-only by default.
- Slow archive statistics, history, and storage probes are isolated from progress cadence and inactive pages. Network-path checks are bounded worker operations with present/missing/unreachable/slow outcomes. Trace rows record command, duration, row count, selected track, storage class, overlap skips, and cache age without exposing secrets.
- External watcher evidence must include requested versus actual sample cadence, bridge latency/failures, database probe waits, page-transition timing, in-flight/overlapping command summaries, queue and identity-claim pressure, NAS scan stage, and bounded host process pressure. Probes must remain read-only, low-priority, and skip rather than accumulate when the host is already delayed.
- Panel navigation must render from cached/lightweight state first and must not synchronously await archive statistics, full history, NAS traversal, browser credential extraction, or queue-wide aggregation. Background refreshes are cancelable or stale-result guarded when the active panel changes.
- Snapshot exports should emit both JSON and Markdown from the same captured state so support handoff and LLM analysis use the same underlying point-in-time record.
- The localhost agent bridge exposes bounded `POST /agent/ui_audit` and `POST /agent/ui_action` routes only in `agent_headless` mode:
  - `ui_audit` inventories the mounted `.content` subtree using semantic HTML/ARIA roles and accessible names plus visibility, enabled/selected/expanded state, current value metadata, viewport/scroll bounds, and generated per-mount audit IDs,
  - `ui_action` resolves exactly one current audit ID and permits only scroll, disclosure activation, semantic selection/expansion controls (`aria-pressed`, tabs, `role=option` with `aria-selected`, and `aria-expanded`), and controls explicitly marked `data-agent-safe-action`; arbitrary selectors, script evaluation, file selection, form submission, and mutating buttons are rejected,
  - frontend execution uses the existing Tauri event/completion channel rather than exposing arbitrary JavaScript evaluation over HTTP,
  - each request is serialized, bounded by timeout and element-count limits, emits `agent_ui_audit` or `agent_ui_action` diagnostics timing/outcome rows, and returns a JSON receipt that identifies the page and resulting state,
  - role/name/state inventory follows native semantics first and ARIA overrides where present; stable product `id`/`data-testid` remains preferable to generated audit IDs for durable automated checks,
  - headless setup skips all runtime background work that can mutate operator work or compete for resources: offline payload hydration, backend seeding, watcher-supervisor startup, NAS fallback resync, the job runner, and YouTube/Instagram startup auto-sync; diagnostics sampling and the localhost audit bridge remain active.

### 6.5 Desktop shell interaction rules

- Drag-region behavior should be restricted to the intended chrome/background layer and must not swallow normal content interaction.
- Corner-resize affordances should have a clear reachable hitbox inside the practical app bounds.
- App movement should use an explicit move affordance or clearly bounded drag region so operators can distinguish shell movement from content interaction.
- The shell move affordance and native window controls should be rendered as one top-right chrome cluster so frameless-shell layout changes do not split or relocate core window actions.
- Frameless maximize/fullscreen handling must keep the native desktop window bounds synchronized with the visible surface so no invisible blocked area sits over neighboring apps in side-by-side layouts.
- Dense archive panes should prefer panel-local scrolling/list behavior over clipped actions or invisible controls.
- Where a dense table cannot fit at practical widths, the panel scroll surface should stay local to that card/pane and action columns should remain visible without forcing the operator to guess where controls went.

### 6.6 Archiver reliability, adaptive-provider, and unified-library design contract

- Panel reads use read-only SQLite connections, bounded indexed projections, cancellation/stale-result guards, and explicit cache age. Opening a pane must not run migrations, acquire a write connection only to read, enumerate archive files, or synchronously probe every NAS media path.
- Physical media availability is a cached observation with `present`, `missing`, `unreachable`, `slow`, and observation-time evidence. A bounded reconciler refreshes observations independently from rendering; execution-boundary checks may force one exact fresh probe where product correctness requires it.
- Subscription archive totals and current job/subscription activity are maintained as indexed database state or event-updated rollups. UI polling reads the rollup and does not repeatedly derive it from all historical jobs or filesystem archive files.
- Performance traces carry `incident_id`, `span_id`, `parent_span_id`, page, interaction, command, job/batch/provider identity where applicable, queue-wait time, execution phase, row count, storage class, cache age, child PID, outcome, and bounded error classification. Secrets, full authenticated URLs, and cookie material remain redacted.
- Frontend diagnostics use supported `PerformanceObserver`/long-task and render/interaction timing where available. Windows deep capture is optional and operator-triggered through the existing Diagnostics/`vvwatch` workflow, using WebView2/ETW/WPR evidence without stealing focus or running indefinitely.
- `diagnostics_trace.jsonl` and external-watch artifacts rotate by bounded size/age. Incident capture may temporarily increase detail for a fixed duration; dropped/sampled event counts are explicit.
- Downloader execution stores an immutable request/effective-policy receipt. The receipt distinguishes the saved operator baseline from adaptive overlay and records downloader/runtime/plugin versions plus provider capability epoch.
- Adaptive outcomes are append-only and keyed by provider, operation class, auth/session identity fingerprint, source target, runtime epoch, error class, policy mode, and timing. Raw rows use retention/compaction; daily outcome rollups and policy transitions are durable.
- YouTube policy states are `normal`, `cautious`, `conservative`, `cooldown`, and `hold`. Transitions use corroboration, minimum spacing, dwell time, hysteresis, a one-target canary after cooldown, and slow recovery after a sustained success window.
- Rate-limit outcomes may reduce aggregate starts and add request/download delay. Authentication outcomes hold the affected auth identity. PO-token/capability outcomes require capability remediation. Content-unavailable, local storage, and generic network outcomes do not train the pacing controller.
- `limit_rate` is modeled as maximum transfer bandwidth; `throttled_rate` is modeled only as yt-dlp's slow-transfer detection threshold. UI labels, validation, effective receipts, and command arguments must preserve that distinction.
- Provider enumeration consumes structured JSON with explicit UTF-8. Canonical metadata upsert preserves raw provider values and normalized search/display fields; repair jobs are bounded, resumable, provenance-recorded, and never replace an operator override.
- Display-title resolution is one engine/shared-frontend contract consumed by Jobs, Video Archiver, provider subscription detail, and Media Library. Frontends must not independently reconstruct title precedence.
- The settings registry is a typed product-code module, not repo governance. It exposes module, stable setting ID, persistence key, schema/type, default, validation, restart requirement, secret/redaction class, baseline value, effective overlay, reset action, help, and optional test action.
- Options navigation is URL/local-state addressable, keyboard accessible, stable across restart/page switches, and represented through stable product IDs for headless audit. Moving a setting between panes does not change its persistence key without a migration.
- A provider adapter defines URL classification, canonical media ID, single-media expansion, subscription/profile enumeration, incremental cursor/archive behavior, authentication/session test, effective pacing inputs, error classification, and canonical metadata mapping.
- The shared subscription workspace queries one bounded provider-neutral projection while provider-specific tables/adapters remain allowed behind it. Bulk actions target canonical backend sets; `Select loaded` remains the only page-local selection label.
- Instagram provider selection starts with the current pinned yt-dlp profile/post capability tested against the exact failing target. A second adapter such as Instaloader may own profile/subscription enumeration when selected by proof; provider choice/version is persisted in lifecycle state and does not create duplicate canonical media.
- TikTok uses canonical video IDs, one provider-specific execution track/budget, incremental profile cursor/archive state, and provider-specific session/device/app capability. It reuses the shared identity, membership, job, settings, adaptive-outcome, and subscription-projection contracts.
- Media Library favorites use an additive table keyed by library item ID with creation/update attribution. Favorite state survives file lifecycle changes and participates in canonical backend filtering before pagination.
- Media Library search uses an inspected SQLite FTS5 design when available, with an explicit synchronization/integrity/rebuild contract; a measured indexed fallback is required if bundled SQLite lacks FTS5. Search results join back to canonical lifecycle/provider filters before pagination and exact totals.
- Media Library pagination uses stable deterministic ordering and keyset/row-value continuation where measured offset cost is material. Every page receipt reports query/filter version, exact matching total or explicitly bounded count mode, loaded count, continuation token, and observation time.
- List and grid renderers are bounded/virtualized, lazy-load thumbnails, preserve stable item IDs and accessibility position, and never derive canonical counts or bulk-action scope from mounted DOM rows.

## 7) Testing Strategy

- Unit tests for:
  - subtitle parsing/rendering
  - translation constraints (CPS/line breaks)
  - job state machine transitions
- Integration tests for:
  - ffprobe/ffmpeg wrappers
  - SQLite migrations
  - "golden file" subtitle outputs
