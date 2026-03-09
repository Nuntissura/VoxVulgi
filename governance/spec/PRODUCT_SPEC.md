# VoxVulgi — Product Spec (Rebuild; Cross-Platform; Local-First)

Date: 2026-03-09  
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
- Shared storage-root behavior:
  - the operator configures persistent roots from a global Options surface rather than pane-local blocks,
  - roots may be set per feature/export class (for example Video Archiver, Instagram Archiver, Image Archive, and Localization exports),
  - feature panes should show the resolved effective path but should not own or duplicate the root-configuration card,
  - configured roots must persist across startup, updates, and window switches,
  - selecting a valid root should create expected app-managed folders when absent and index or hydrate existing folders when already present.
- Add subscription export/import:
  - export all subscriptions to JSON (portable backup/migration file),
  - import from JSON with merge-by-URL behavior (upsert existing, add missing),
  - no subscription deletion on import unless explicitly requested by the user.
- Video workflow split:
  - Localization Studio should include a lightweight video-ingest block for local import/refresh and ASR language selection (`auto` plus explicit language choices),
  - the separate archive window should focus on URL ingest, presets/templates, subscription groups, and subscriptions,
  - archive windows should not duplicate Localization Studio ingest controls or global storage-root configuration blocks.
- Authenticated archive-session support:
  - login-required YouTube and Instagram workflows must support explicit operator-provided session material,
  - accepted operator inputs should include raw cookie headers, Netscape cookie files, browser-export JSON cookie blobs, and explicit cookie-file paths,
  - authenticated-session inputs must be reusable across one-shot batches and saved subscriptions where the operator chooses,
  - browser-profile cookie fallback must remain explicit, optional, and clearly disclosed when used.
- Instagram archive additions:
  - support saved recurring Instagram archive targets with an interval-based refresh model,
  - show the last 10 archived pictures/stories/reels with uncropped thumbnail framing.
- Add an in-app **image archive batch** mode for blogs/forums:
  - accepts multiple start URLs in one submission,
  - crawls pagination + post/thread links,
  - skips likely profile/avatar images,
  - prefers full-size image URLs over thumbnail variants,
  - writes a manifest for audit/review.
- Planned image-archive expansion:
  - add Pinterest board/folder crawl support with batch URL intake.
- Auto-extract metadata (duration, codecs, resolution) + generate thumbnails.
- Existing-library reconciliation:
  - allow non-destructive indexing of large existing downloader-managed or NAS-backed archive roots,
  - preserve playlist/channel/subscription folder structure where possible instead of flattening existing trees.
- Performance stance (large libraries):
  - thumbnails should be stored on disk (cache) and lazy-loaded (no giant DB BLOB storage),
  - Library list/grid should be virtualized to stay responsive with very large libraries.
- Library list with:
  - search (title/tags/text),
  - filters (language, status, date, source),
  - collections/playlists,
  - grouped browsing by source container such as playlist/subscription/folder,
  - media-type filters such as video and image,
  - a list-first mode suited to very large archives,
  - cards remain available as a secondary view, but list view is the default for large archives,
  - rows should surface provider, container type, container label, source reference, codecs, and file path without forcing the operator to open a detail view,
  - explicit container semantics so operators can tell whether a row/group represents a playlist, subscription, folder, or single imported file.
- Default archive/output media policy:
  - video workflows should prefer MP4 by default wherever the local toolchain can merge or remux cleanly,
  - image workflows should prefer JPEG defaults where the provider/toolchain offers multiple encodings without destructive tradeoffs.
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

### 4.3 Translate CC (JA/KO -> EN) v1

- One-click translation pass producing:
  - translated subtitles (EN),
  - optional bilingual view (source + EN).
- Quality features:
  - glossary (custom term mappings),
  - style settings (formal/informal, honorific handling, punctuation rules),
  - line-length and CPS (characters-per-second) constraints.

### 4.4 Diagnostics (must-have)

- "Diagnostics" page:
  - versions of major components (app, ffmpeg, models),
  - model inventory (what's installed, where it's stored, and how much space it uses),
  - storage usage breakdown (library, cache, logs),
  - last job errors with copy/export.
- Startup and performance diagnostics:
  - show a meaningful startup progress bar or phase list while heavyweight background initialization is in flight,
  - show numeric progress or percentages where the app can derive them,
  - provide an obvious shell-level loading-details surface that operators can open while the app is still usable, including current percentage and per-phase state,
  - when a feature is temporarily blocked because dependencies are still hydrating, the UI should explain that state explicitly near the action and surface the current loading progress,
  - capture deterministic local traces for startup phases, pane activation, resource usage, and major failures,
  - explain tool state in operator terms such as bundled, hydrated, installed, loaded, and ready,
  - suspend recurring pane-local polling and heartbeats when the page or app is not active so the UI degrades gracefully under heavy external CPU load.
- Diagnostics state export:
  - diagnostics should be able to export a coherent local snapshot of current app state, including roots, tool/model state, queue health, and major feature readiness,
  - the snapshot export should include both structured JSON and an operator-readable Markdown summary,
  - the snapshot should be readable both by operators and by support/LLM analysis workflows.
- Supply-chain and reproducibility requirements:
  - bundled dependency inputs must be tracked in a pinned manifest rather than scattered mutable constants,
  - mutable unpinned fallback installs must be disabled by default and only run through an explicit local operator/developer opt-in,
  - offline bundle hydration must verify payload size/hash when the bundle manifest provides them,
  - third-party source patches used by bundled packs must live in tested maintainable helpers rather than large inline patch scripts.
- Log rotation and retention (cap by size + age).
- Diagnostics must surface the derived-artifact retention policy so operators can distinguish working files, durable reports, and durable deliverables.
- Cache/history cleanup must be split from output-folder deletion, and custom or external output folders must require a separate explicit opt-in before deletion.
- Local config, override, and secret writes must be crash-safe and atomic rather than direct in-place truncation writes.
- "Export diagnostics bundle" (logs + job metadata + redacted config).
- Recovery UX:
  - a **Safe Mode** startup path to open the app without auto-refresh or heavy background work (so users can export/manage data even when providers regress).

## 5) Phase 2 (Voice-preserving dubbing MVP)

### 5.1 Multi-speaker segmentation

- Speaker diarization (label Speaker 1/2/3...).
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
- Voice backend strategy should now be a first-class operator-visible layer:
  - the shipped default remains the managed OpenVoice V2 + Kokoro path until benchmark evidence supports a change,
  - Diagnostics and Localization Studio should expose a research-backed backend catalog covering managed and experimental candidates,
  - the app should distinguish backend families such as two-stage TTS + VC, direct zero-shot TTS, and conversion-only pipelines,
  - the app should support explicit local BYO backend adapters for stronger experimental OSS candidates without silently installing them.

### 5.2 Background preservation

- Separate vocals vs background (best-effort source separation).
- Generate English speech per segment and mix back with background.
- If a separated background stem is unavailable or separation fails, `Mix dub` should degrade gracefully by mixing against the source-media audio with explicit operator-visible fallback messaging instead of dead-ending the workflow.
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
- Cleanup and review integrity requirements:
  - when a speaker has multiple reference clips, cleanup must let the operator choose which reference to process,
  - applying a cleaned reference must preserve the broader reference set unless the operator explicitly narrows it,
  - cleanup history must remain separated per real speaker key and not collide because of sanitized labels alone.

### 5.3 Export

- Export:
  - dubbed audio track (WAV/AAC),
  - muxed video with new audio track (MP4/MKV),
  - subtitles as sidecar or burned-in.
- Localization output discoverability:
  - Localization Studio should expose a dedicated outputs browser or library view that groups source media, working artifacts, and exported deliverables for the current item,
  - operators should be able to open or reveal source video, working artifact folders, dubbed outputs, subtitle exports, and export folders from one obvious surface.
- Planned export/review additions:
  - A/B preview variants before committing to a final voice choice,
  - batch dubbing across item sets or seasons,
  - export stems (speech only, background only, final mix) and alternate dubbed versions.
- Backend-comparison additions:
  - Localization Studio should include a benchmark lab that ranks current outputs and variants for an item,
  - benchmark reports should explain timing fit, coverage, reference health, silence/clipping/noise issues, and similarity proxies,
  - backend selection changes should be evidence-driven through durable report artifacts instead of implicit replacement of the shipped default,
  - benchmark and recommendation outcomes should be promotable into an explicit item-scoped voice plan instead of living only as transient UI state,
  - operators should be able to run configured experimental BYO backends into standard VoxVulgi manifests for real side-by-side comparison,
  - reference bundles should be rankable and promotable so the app helps choose the best subset/order of multi-reference clips,
  - experimental backend runs should also support bounded item-set batches so one backend family can be evaluated across a representative series sample,
  - benchmark reports should keep durable compare history and exportable leaderboard snapshots instead of only the latest in-place report,
  - Diagnostics should provide backend-specific starter recipes for known OSS stacks rather than only blank BYO adapter forms,
  - benchmark winners should be promotable into reusable template and cast-pack defaults, not only the current item voice plan.

Current implementation status:

- Localization Studio surfaces batch dubbing, A/B speaker previews, export stems/alternates visibility, voice memory, character libraries, and reference cleanup controls.
- Localization Studio now also generates goal-aware voice benchmark reports, stores them as durable JSON/Markdown artifacts, and surfaces the top-ranked candidates with explainable metric breakdowns.
- Diagnostics now exposes a local-only BYO backend registry where operators can save, probe, and remove experimental backend adapters without bundling or auto-installing those stacks.
- Current implementation now also includes bounded batch experimental backend runs across one selected item set.
- Current implementation now also includes immutable benchmark compare history and leaderboard export artifacts for the current item/track/goal.
- Current implementation now also includes backend-specific starter recipes so Diagnostics can prefill stronger BYO adapter drafts for known OSS stacks.
- Current implementation now also lets operators promote benchmark winners directly into the selected reusable voice template or cast pack and optionally seed later item voice plans from those saved defaults during apply.
- Export packs include speech stems and alternate dubbed variants when available.
- Artifact-browser actions must remain variant-aware:
  - rerun, status, and log links for A/B/alternate artifacts must target the matching variant/track/container instead of falling back to the base artifact state,
  - unsupported artifact rows must not expose misleading rerun actions.
- Batch dubbing item selection must scale across the full library and must not silently truncate selected item sets.

## 6) Phase 3 (Power Features)

- "Smart tags" v2:
  - named entities (people/places/orgs),
  - topic clustering,
  - "find similar clips" via embeddings.
- Content-aware workflows:
  - batch processing rules ("auto-translate all new JA videos"),
  - watch folders,
  - scheduled tasks.
- Collaboration:
  - shared glossary,
  - subtitle review comments,
  - export reports.
- Advanced dubbing library features:
  - richer evaluation/QC heuristics,
  - stronger subtitle-aware prosody controls and future expressive hinting,
  - deeper reuse/reporting workflows on top of the now-implemented memory/character libraries,
  - a backend-catalog and recommendation system for voice cloning and dubbing,
  - explicit BYO adapter support for experimental local backends,
  - benchmark-driven promotion of future managed backends,
  - item-scoped backend plans that persist operator decisions,
  - ranked reference-bundle curation and promotion,
  - experimental backend execution against real subtitle tracks via explicit local adapters,
  - multi-item experimental render matrices for representative episode sets,
  - durable benchmark history and leaderboard export artifacts,
  - backend-specific starter-recipe workflows for known OSS adapter families,
  - reusable template/cast-pack backend defaults informed by benchmark promotion.

## 7) UX Principles

- Fast: UI never blocks on AI jobs (always queued with progress).
- Fast: queueing URL/subscription downloads must return quickly; heavy URL expansion/extraction runs in worker jobs, not on the UI thread.
- Transparent: show what data is stored and where; easy cleanup.
- Editable: every AI output is reviewable and editable.
- Offline by default: no background network egress. Windows "full" installers bundle required local tools/models for Phase 1+2 and bootstrap them into app-data on first launch, so the core pipeline can run fully offline without manual pack installs.
- Safe defaults: no voice cloning by default.
- Voice and dubbing controls remain operator-directed; VoxVulgi should not add content-judgment or censorship workflows as part of these features.
- Discoverable: operator-critical controls must be visible in the workflow where they are needed rather than buried behind long scroll chains or hidden state gates.
- Localization Studio should surface a workflow/readiness summary that makes current track readiness, runtime readiness, and the main backend/benchmark/QC/artifact sections obvious before the operator starts deeper dubbing steps.
- Ergonomic: dense archive/workflow panes should provide clear scrolling behavior and an explicit app-move affordance that does not conflict with text selection or scrollbar use.

## 8) Key UX Screens

- **Library**: grid/list, filters, collections, "import" CTA.
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
- **Diagnostics** should also surface a voice-backend catalog, backend readiness, and recommendation reasoning instead of only package versions.

### 8.0.1 Current top-level windows (implemented 2026-03-03)

- **Localization Studio**: first/default window, focused on subtitles + dubbing workflow.
- **Localization Studio** also keeps a lightweight ingest block in-context for local import and ASR language selection, even when the editor is already open.
- **Video Archiver**: local import + URL batch ingest + presets/templates + YouTube subscriptions/groups + legacy archive reconciliation.
- **Instagram Archiver**: dedicated Instagram batch ingest workflow plus recurring archive targets.
- **Image Archive**: dedicated crawler-based image archive ingest workflow.
- **Media Library**: renamed from ambiguous "Items"; browse imported media and hand off to Localization Studio.
- **Jobs/Queue**: execution state + retry/cancel + logs and output reveal.
- **Diagnostics**: non-blocking, section-by-section loading with explicit readiness states, recent local trace rows, and startup/tool-lifecycle visibility.
- Localization Studio artifact rows must receive typed runtime metadata from the bridge for rerun/status matching, rather than reconstructing artifact identity from filenames in the UI.

### 8.0.2 Workspace hardening state (implemented 2026-03-07)

- **Localization Studio** now includes the lightweight ingest block for local import/refresh plus ASR-language selection because this is the primary operator workflow.
- **Video Archiver** is the dedicated home for URL ingest, presets/templates, subscription groups, YouTube subscriptions, and legacy archive reconciliation.
- **Legacy archive reconciliation** now distinguishes 4KVDP-managed subscription/playlist containers from unmatched manual folders and loose root files, using the old 4KVDP app-state SQLite when available to preserve folder mapping and resume/dedupe state without touching the legacy archive.
- **Instagram Archiver** is the dedicated home for direct Instagram archive runs plus recurring archive targets.
- **Options** is the discoverable home for shared storage-root configuration and related global path behavior.

## 8.1) Stabilization priorities for commercial readiness (2026-03-03)

### 8.1.1 Operator goals and needs

- Localization Studio (Dub/CC) must be the primary, first-visible workspace.
- Localization Studio should include the first ingest step needed to move directly into ASR and dubbing work.
- Navigation must be split into clear top-level windows for ingest/archive/localization work.
- Window switching, Diagnostics entry, and startup must stay responsive (no visible freezing).
- Jobs "Open outputs" actions must reliably open paths without ACL errors.
- Shared download/export roots must persist without temporary "missing folder" states.
- Operators should be able to reveal files or parent folders anywhere the app creates an output or artifact.
- Localization exports must be easy to find, with a predictable default folder map and direct open/reveal actions for both source files and generated deliverables.
- Generic cache/history cleanup must never silently remove Localization Studio deliverables, benchmark/report history, or custom output folders.
- Default preview/download video outputs should be MP4 wherever the local toolchain can merge/remux cleanly.
- Default archive image outputs should prefer JPEG where practical.
- UX must be fast enough for daily production use before any commercial release.

### 8.1.2 Required top-level window model

- Localization Studio (Dub/CC) - default first-run window and main feature surface.
- Localization Studio ingest block - local import/refresh + ASR language selection in-context.
- Video Archiver - local ingest + URL ingest + presets/templates + playlist/subscription/folder-map flows.
- Instagram Archiver - dedicated archive workflow with recurring archive targets.
- Image Archive - dedicated archive workflow.
- Options - shared storage roots and global path behavior.
- Jobs/Queue - execution visibility + controls.
- Diagnostics - health/recovery tooling with non-blocking load.
- "Items" must be either clearly defined and renamed or merged into a clearer workspace label.

### 8.1.3 Performance and responsiveness budgets (target)

- Startup: app shell becomes interactable before heavyweight background initialization completes.
- Startup instrumentation: boot timeline markers must identify slow phases in logs/diagnostics.
- Startup UX: show visible loading progress instead of opaque background work.
- Window switching: no multi-second freezes during normal navigation.
- Diagnostics entry: render shell immediately and load sections incrementally with explicit readiness states.
- Heavy external CPU load: the app should degrade gracefully when other software is consuming CPU; long-running work must stay bounded, queued, resumable, and visibly in-progress rather than freezing the shell.
- Contention tolerance: avoid synchronous recompute/refetch/remount loops on pane changes, and keep operator-facing actions responsive even when background jobs or third-party tools are busy.

### 8.1.4 Reliability requirement: shared storage roots

- Global download/export root selection must be stable across startup, updates, and pane switches.
- Choosing a valid existing root must hydrate or index expected folder structure instead of flashing a missing-folder error.

### 8.1.5 Reliability requirement: output path opening

- Queue/Library/Diagnostics open-path actions must work for valid output/artifact paths.
- Blocked/invalid paths must return actionable errors with copy-path fallback.

### 8.1.6 Desktop shell ergonomics requirement

- Corner resizing must have an obvious reachable hitbox.
- Dragging the app should use an explicit shell move affordance or tightly bounded chrome handle so text selection and scrollbars still work inside content areas.
- Dense per-panel tables should keep their own scroll surface and should keep critical actions visible when horizontal scrolling is required.

### 8.1.7 Installer and uninstall clarity requirement

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
