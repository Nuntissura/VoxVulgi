# VoxVulgi — Product Spec (Rebuild; Cross-Platform; Local-First)

Date: 2026-03-09  
Status: Draft (assumptions noted; intended as the starting point for implementation planning).

## 1) Framing / Constraints

- This is a **rebuild** (new codebase), inspired by the observed behavior of 4K Video Downloader+ on this machine. We do **not** reuse proprietary code or decompile binaries.
- Target platforms: cross-platform desktop (Windows + macOS first; Linux later if needed).
- Local-first: default to on-device ASR/translation/dubbing for privacy and offline use, with optional cloud providers only when the user opts in (and with clear disclosure of what data is sent).
- Offline-first out of the box (operator decision 2026-07-31): the public installer ships all default models and dependencies; core workflows (import, captions, translate, dub, export) must work without network access from first run, with zero downloads.
- Open-source direction (operator decision 2026-07-31): VoxVulgi is intended to ship as an open-source app aimed at language students and people enjoying content from other cultures.
- Primary persona (operator decision 2026-07-31): non-technical users. The default experience must require zero technical setup: no terminal, no Python/pip steps, no manual model downloads, no dependency repair.
- Batteries-included distribution (operator decision 2026-07-31): the public installer must bundle every model and dependency required by the complete default localization pipeline; bundled defaults are user-swappable later through in-app surfaces, but swapping is optional and never required (see 8.1.8).
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

- Primary: language students and learners who want accurate captions, translations, and dubbed audio.
- Primary: people enjoying content from other cultures who want to watch it localized without technical setup.
- Creators/editors who localize content (YouTube/TikTok clips, interviews, podcasts).
- Archivists who want a searchable library with tags/metadata.
- Assume non-technical users by default; all core workflows must be operable without command-line, Python, or dependency knowledge.

## 4) MVP Scope (Phase 1)

### 4.1 Library + Ingestion (core UX)

- Import local video/audio files.
- Drag-and-drop import: operators can drag media files onto the Localization Studio home screen to import; visual drop indicator during drag-over; supports multi-file batch; accepted formats include mp4, mkv, avi, mov, webm, mp3, wav, flac, ogg.
- Downloading: provider layer + batch URL ingest:
  - direct HTTP/HTTPS media URLs (strict schemes; best-effort),
  - YouTube (and many webpage video links) via `yt-dlp`, with a supported local JavaScript runtime available when current upstream extraction requires it,
  - Instagram batch ingest (posts/reels/stories/profiles) that expands into media targets (optional session cookie header for private content),
  - provenance captured per ingest (provider + source URL),
  - every successful downloaded library item also receives durable canonical lineage for service, origin kind, execution track, originating job/batch, and subscription when applicable; this lineage survives terminal-job cleanup,
  - `Downloaded single videos` is a backend-defined canonical projection of one-off single-video lineage, never a path/URL guess; subscription, playlist, channel, and unclassified imported outputs remain preserved but do not enter that projection.
- Add YouTube subscription management:
  - save persistent subscriptions (channel/playlist/video feed URLs),
  - define a folder map per subscription so each subscription writes into its own mapped folder,
  - treat a folder map as the preferred landing location for newly materialized files, not as an ownership boundary that justifies duplicate copies,
  - preserve the current mapped folder for an existing subscription and reconcile already-downloaded items where practical before queueing fresh downloads,
  - keep per-subscription "already downloaded" continuity state in VoxVulgi-managed storage rather than the physical output folder so imported/NAS overrides remain usable when the global root changes,
  - when one refresh cohort contains overlapping playlist and channel-page, `/videos`, or `/shorts` subscriptions, enumerate the non-playlist sources first so their canonical IDs claim the physical item before playlist discovery,
  - record every playlist/feed association, but only suppress a playlist download when the canonical item is already present or actively claimed; historical membership alone must never hide a missing or unavailable video,
  - backfill existing subscription-source associations into durable source memberships idempotently so upgrades use the same cross-source behavior as newly discovered items,
  - retain a durable subscription source status distinct from the Active/pause toggle: `normal`, automatically recoverable `unavailable`, or manually controlled `deleted`,
  - only an explicit operator or assistant action may set or clear `deleted`; search results, connection failures, authentication failures, extractor failures, and refresh outcomes must never set it,
  - a manually deleted subscription remains stored with its groups, source memberships, videos, library metadata, and job history, while all refresh queue entry points refuse it,
  - an exact HTTP 404 refresh result sets `unavailable`; its operator-facing explanation must state that the missing URL does not prove the hosting channel was deleted, because the URL may be renamed, private, restricted, temporarily unavailable, or undisclosed,
  - a later successful refresh clears `unavailable`, while unrelated network/auth/tool errors neither imply deletion nor overwrite the unavailable status,
  - select one or many canonical subscription videos by stable item ID in both Video Archiver subscription detail and Media Library; page-level selection must say `Select loaded` and never imply an unseen canonical set,
  - explicit file deletion defaults to the OS Recycle Bin, offers separately confirmed permanent deletion, preserves the library item plus identity/membership/subscription/playlist/job metadata, and records a durable operator-deleted lifecycle state distinct from missing or unreachable media,
  - subscription refresh, automatic repair, retry, retry-all, batch repair, redownload-all, and already-queued execution must refuse operator-deleted video items,
  - redownload of an operator-deleted video is available only as an explicit action on the exact selected deleted items; only the exact jobs created by that action are authorized, and the deleted state clears only after a replacement file imports successfully,
  - set a per-subscription refresh interval (minutes) that can be edited in the Library UI,
  - queue refresh for one subscription or all active subscriptions,
  - keep loaded subscriptions stable across pane switches and window focus changes.
- Download execution tracks:
  - operator-submitted YouTube work, background YouTube subscription work, Instagram, other video services, Image Archive, and Localization Studio use independent durable tracks so a backlog in one does not consume another track's worker budget,
  - a manually submitted YouTube batch remains foreground work even when one URL is a playlist/channel; its downloaded members keep their truthful `playlist`/`channel` origin rather than being presented as individual singles,
  - the foreground and background YouTube tracks may each make progress, but every aggregate YouTube process start passes through one shared randomized pacing/auth gate,
  - all YouTube single downloads use the same conservative effective download profile as subscription children: one concurrent fragment, the configured 5-10 second pre-download delay, and the same retry, throttling, browser-session, and auth-circuit behavior,
  - foreground YouTube starts lead when both tracks become eligible, while bounded alternating fairness keeps subscriptions draining in the background,
  - a YouTube account/rate hold leaves both YouTube tracks queued and actionable while Instagram, other video, Image Archive, and Localization Studio remain dispatchable.
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
  - Localization Studio should include a lightweight video-ingest block for local import/refresh and source language selection (`auto` plus explicit language choices),
  - the separate archive window should focus on URL ingest, presets/templates, subscription groups, and subscriptions,
  - archive windows should not duplicate Localization Studio ingest controls or global storage-root configuration blocks.
- Authenticated archive-session support:
  - login-required YouTube and Instagram workflows must support explicit operator-provided session material,
  - accepted operator inputs should include raw cookie headers, Netscape cookie files, browser-export JSON cookie blobs, and explicit cookie-file paths,
  - authenticated-session inputs must be reusable across one-shot batches and saved subscriptions where the operator chooses,
  - browser-profile cookie fallback must remain explicit, optional, and clearly disclosed when used,
  - YouTube Options must provide a goal-led supported-browser sign-in flow: open YouTube in the selected normal browser, let the user sign in there, then run an exact-source test; cookie terminology and manual export stay in an advanced fallback,
  - Google OAuth must not be presented as media-download authentication because yt-dlp requires cookies, and Google login must not run in an app-controlled embedded WebView,
  - YouTube connection state must distinguish configured-but-unverified, verified-ready, and reconnect-required; a rejected preflight or corroborated runtime rejection must remain visibly reconnect-required after restart until a new exact-source test passes,
  - a rejected global YouTube session must be shown as one actionable account state and must hold queued recurring YouTube work instead of multiplying the same account failure across the queue.
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
  - preserve playlist/channel/subscription folder structure where possible instead of flattening existing trees,
  - present imported and VoxVulgi-downloaded items as one unified library; import origin remains internal provenance for audit and rollback, not a user-facing `legacy` versus `new` distinction,
  - identify each YouTube video canonically by service plus extractor video ID and allow it to belong to any number of playlist, `/videos`, `/shorts`, channel-page, or direct-video sources without creating another physical media copy,
  - treat source subscriptions and playlists as library memberships and discovery inputs, not owners of separate physical copies; reuse an existing canonical file wherever it is already stored,
  - enrich imported 4K Video Downloader records from the third-party database in read-only mode using structured URL, download-item, subscription-entry, and exact-path evidence before filename or content heuristics,
  - auto-link imported identity only from unambiguous exact evidence and preserve ambiguous or unresolved records in an inspectable review state,
  - separate duplicate inventory from mutation and default to dry-run; progress candidates from canonical source-ID evidence to exact file-size and staged content-hash evidence,
  - allow decoded or perceptual similarity to assist review but never to authorize automatic deletion,
  - show the proposed keeper, all affected source memberships, reclaimable bytes, evidence strength, and exact filesystem action before apply,
  - apply cleanup through a recoverable quarantine and durable rollback manifest; permanent deletion is a separate operator-confirmed action,
  - reconcile physical-only media and missing/zero-byte VV paths before duplicate decisions:
    automatically relink only deterministic one-to-one evidence, index unmatched physical media,
    preserve unresolved/ambiguous records, and delete no metadata,
  - keep preserved library metadata path-true during cleanup: after quarantine, redundant records
    resolve to the verified keeper path in the same database handoff as canonical identity relinking,
    and rollback restores their original source path and identity ownership,
  - keep NAS inventory, hashing, and cleanup resumable, pauseable, bounded in concurrency, and observable without blocking navigation or foreground interaction.
- Performance stance (large libraries):
  - thumbnails should be stored on disk (cache) and lazy-loaded (no giant DB BLOB storage),
  - Library list/grid should be virtualized to stay responsive with very large libraries.
- Built-in headless UI audit:
  - a no-context model must be able to inventory visible page structure and interactive elements with accessible names, roles, states, bounds, and stable audit identifiers,
  - headless audit navigation must support top-level pages, scrolling, tabs, filters, and disclosures without foreground focus or keyboard/mouse simulation,
  - interaction is read-only by default: structural controls may be exercised, while queue starts, retries, cancellation, deletion, file operations, settings writes, and other mutations are refused unless a future explicit authority surface adds a separately gated workflow,
  - every audit action must return a structured receipt and write timing/outcome evidence into the existing diagnostics trace so internal Diagnostics and `vvwatch` can correlate UI behavior with freezes or slow commands,
  - audit interaction endpoints are available only when the app was launched with `--agent-headless`,
  - `--agent-headless` must not start the job runner, subscription auto-sync, offline-payload hydration, fallback-media relocation, or watcher supervisor; audit startup must not enqueue, resume, relocate, install, or otherwise mutate operator work.
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
- Current Media Library filter controls:
  - search, source, media-type, lifecycle, canonical-single, and sort controls are applied by the backend to the full canonical library set before pagination; the UI must never filter only the currently loaded slice,
  - the response reports the full matching total separately from the loaded row count so empty/partial rendered pages cannot be mistaken for empty backend state,
  - search matches title, file path, source reference, and codec metadata,
  - source filter (YouTube / Instagram / Local import / All),
  - media-type filter (Video / Image / Audio / Other / All),
  - sort selector (Date added / Title),
  - view mode (Archive list / Cards),
  - group mode (Container/folder / Flat list),
  - all filter state persists in localStorage across page switches.
- Default archive/output media policy:
  - every newly managed video download and newly muxed video deliverable must finalize as MKV; MP4 remains a supported input, playback, import, library, dedupe, and historical-media format but is not a permitted new managed video output,
  - MKV video deliverables contain the selected video and audio streams plus selected/available subtitle tracks with truthful language/title metadata; routine video downloads must not leave SRT/VTT sidecars as the user-facing deliverable after successful embedding,
  - explicit subtitle-only export remains allowed to create SRT/VTT when the operator requests subtitle files independently from a video deliverable,
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
  - time nudge and reflow,
  - undo/redo (Ctrl+Z / Ctrl+Shift+Z) for text, timing, and speaker changes with a 50-operation stack that resets on track switch.

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
- Diagnostics dashboard summary (top of page):
  - clickable status tiles: App version, Voice packages (installed/missing), FFmpeg (ready/missing), Storage (total MB), Recent failures (count),
  - each tile scrolls to the corresponding detail section below,
  - color coding: green (ready), yellow (action needed), red (error/missing).
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
- Localization Studio must let the operator choose diarization speaker-count intent before labeling speakers: automatic detection, exact count, or a min/max range. That intent should flow through both direct diarization and full localization runs.
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
  - the shipped default is readiness-sensitive: use managed CosyVoice 2 when its complete offline pack is byte-verified, otherwise fall back to managed OpenVoice V2 + Kokoro,
  - Diagnostics and Localization Studio should expose a research-backed backend catalog covering managed and experimental candidates,
  - the app should distinguish backend families such as two-stage TTS + VC, direct zero-shot TTS, and conversion-only pipelines,
  - the app should support explicit local BYO backend adapters for stronger experimental OSS candidates without silently installing them.

### 5.2 Background preservation

- Separate vocals vs background (best-effort source separation).
- Generate English speech per segment and mix back with background.
- If a separated background stem is unavailable or separation fails, `Mix dub` should degrade gracefully by mixing against the source-media audio with explicit operator-visible fallback messaging instead of dead-ending the workflow.
- The shipped Localization Studio path should remain a staged cascade, not a direct speech-to-speech black box:
  - source media,
  - subtitle/ASR track,
  - translated English track,
  - speaker/reference state,
  - target speech generation,
  - optional voice-preserving conversion or experimental backend render,
  - background-aware mix,
  - MKV mux/export with selected audio and subtitle tracks embedded; historical MP4 previews remain readable but are never produced by new localization runs,
  - explicit review of outputs.
- The educational-core reusable-voice promise should remain simple even as advanced surfaces grow:
  - capture a reusable voice from the current speaker setup,
  - apply that reusable voice to a later translated item,
  - run the dubbed preview,
  - verify whether the result was actually voice-preserved or plain TTS fallback.
- Localization Studio should expose that basic promise as one obvious operator lane before the deeper reusable-asset surfaces:
  - a dedicated `Reusable Voice Basics` surface should let the operator choose a speaker, capture or apply source references, save reusable voice memory, apply saved reusable voice, and continue the dubbed preview without detouring through cast packs, character profiles, benchmark-default setup, or backend research surfaces first.
- The explicit `Start / continue localization run` action should advance automatically until the next real operator checkpoint.
  - If speaker labels are missing, it should queue diarization rather than jumping straight into dubbing.
  - If speaker labels exist but clone references are missing, it should first generate and attach source-media voice samples for those speakers.
  - It should pause at the speaker/reference stage only when source-sample extraction fails or a speaker still lacks a usable clone reference.
  - Localization Studio should also provide an assisted bridge out of that checkpoint by letting operators generate candidate speaker-reference bundles from the current source media after diarization, review/apply them, and then continue the staged run without manual file hunting.
- Direct speech-to-speech research systems may inform future R&D and benchmark lanes, but they should not replace the default shipped operator path until they meet the same packaging, inspectability, and operator-control standard as the staged cascade.
- Keyboard shortcuts for common Localization Studio actions:
  - Ctrl+Z / Ctrl+Shift+Z — undo/redo subtitle edits,
  - Ctrl+Enter — start/continue localization run,
  - Ctrl+Shift+E — export selected outputs,
  - Ctrl+Shift+R — refresh readiness,
  - Ctrl+1 through Ctrl+8 — select Captions, Translate, Speakers, Voice plan, Dub, Mix, Combine A/V, or Files in the master-detail editor (WP-0211 refinement of WP-0173's original fixed-section mapping),
  - visible shortcut reference in the master-detail workflow rail; non-undo shortcuts are disabled when typing in form fields.
- Sticky quick-actions bar: persistent bottom bar visible when an item is open, showing item title, run status (idle/running), and Run/Export/Open Outputs buttons at all scroll positions.
- Batch-on-import defaults remain configurable through Options/Diagnostics and compatible import flows. Localization-owned intake does not apply them implicitly: import remains idle until the operator reviews source language and run settings, then presses the explicit start action (WP-0199 supersedes WP-0174 on this surface).
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
  - muxed MKV video with selected original/dubbed audio tracks and embedded subtitle tracks,
  - explicit subtitle-only SRT/VTT export when requested; routine video deliverables keep subtitles embedded instead of creating companion sidecars.
- Localization Studio must make the run contract explicit before work starts:
  - operators should be able to set options before the localization run begins,
  - the app should expose an explicit start action or an equally explicit pre-start review/confirm contract,
  - once started, each active item should expose stage-level progress rather than only a generic background queue presence.
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
- Localization Studio now also exposes a dedicated `Reusable Voice Basics` lane for choosing a speaker, capturing/applying references, saving reusable voice memory, applying saved reusable voice, and handing off directly into the localization run.
- Localization Studio now also generates goal-aware voice benchmark reports, stores them as durable JSON/Markdown artifacts, and surfaces the top-ranked candidates with explainable metric breakdowns.
- Localization Studio now also generates source-based speaker-reference candidate bundles after diarization and lets operators apply them directly into the current item voice plan as a bridge to a first real dubbed preview.
- Diagnostics now exposes a local-only BYO backend registry where operators can save, probe, and remove experimental backend adapters without bundling or auto-installing those stacks.
- Current implementation now also includes bounded batch experimental backend runs across one selected item set.
- Current implementation now also includes immutable benchmark compare history and leaderboard export artifacts for the current item/track/goal.
- Current implementation now also includes backend-specific starter recipes so Diagnostics can prefill stronger BYO adapter drafts for known OSS stacks.
- Current implementation exposes OpenVoice V2 + Kokoro and CosyVoice 2 as item-selectable managed dub backends, keeps their manifests separate for benchmark comparison, and resolves the managed default from verified pack readiness.
- Current implementation now also lets operators promote benchmark winners directly into the selected reusable voice template or cast pack and optionally seed later item voice plans from those saved defaults during apply.
- Export packs include speech stems and alternate dubbed variants when available.
- Voice-preserving runs must not be presented as successful cloned-voice results when conversion did not actually occur; clone-vs-fallback truth is part of the product contract, not an optional detail.
- Current implementation now surfaces clone-truth status directly in the main Localization Studio operator flow, including the item voice plan, localization run, outputs surfaces, and benchmark candidate cards, using explicit labels such as `clone preserved`, `partial fallback`, `plain TTS fallback`, and `standard TTS only`.
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
- Parallel and safe: independent product tracks must continue making progress under unrelated backlogs, while shared provider gates pace work that reaches the same upstream service.
- Transparent: show what data is stored and where; easy cleanup.
- Editable: every AI output is reviewable and editable.
- Offline by default: no background network egress. Windows "full" installers bundle required local tools/models for Phase 1+2 and bootstrap them into app-data on first launch, so the core pipeline can run fully offline without manual pack installs.
- Safe defaults: no voice cloning by default.
- Voice and dubbing controls remain operator-directed; VoxVulgi should not add content-judgment or censorship workflows as part of these features.
- Discoverable: operator-critical controls must be visible in the workflow where they are needed rather than buried behind long scroll chains or hidden state gates.
- Plain language: all operator-facing labels use non-technical terminology; "Source language" not "ASR lang", "Voice samples" not "refs", "Saved voice" not "voice memory profile", "Clone status" not "dub truth", "Clean up" not "Flush", "Component status" not "Tool lifecycle model".
- Localization Studio must surface missing or repair-required voice-cloning readiness in-context before a dub starts. Setup/repair is explicitly operator-triggered, shows a manifest-owned size/time estimate for slim/dev or damaged installs, supports session-scoped “Set up later” with an obvious return action, and never silently starts a pack install. Public offline-full installs should normally bypass this gate because required bytes are already bundled and verified.
- Localization Studio should surface a workflow/readiness summary that makes current track readiness, runtime readiness, and the main backend/benchmark/QC/artifact sections obvious before the operator starts deeper dubbing steps.
- Localization Studio should not require a confusing bounce through Media Library just to understand current source, active run, or output state after import; the current item handoff and its output path should remain obvious inside Localization Studio.
- Localization Studio should use its first screen as a true operator dashboard: current item, recommended next action, and latest preview/deliverable path should be understandable at a glance before the operator scrolls or opens another window.
- Localization Studio should make the speaker-reference checkpoint survivable for first-run operators:
  - when the run pauses for missing references, the app should surface a direct path to generate or apply reference candidates from the current media,
  - the operator should not need to leave the current item flow just to build a first voice-cloned dub preview.
- Localization Studio should also make reusable-voice basics obvious before advanced reusable asset layers:
  - operators should be able to save a reusable voice from one item and apply it to a later item without first reasoning about cast packs, memory profiles, character profiles, benchmark defaults, or backend research surfaces.
- Generic startup/recovery messaging should remain available without visually displacing Localization Studio as the main feature surface once the app is usable; compact shell-status affordances should be the default, with larger recovery/detail panels reserved for Safe Mode, active failure, or explicit operator request.
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
  - dedicated `Reusable Voice Basics` lane for speaker -> reference -> save/apply reusable voice -> continue dub,
  - reusable voice-template save/apply for recurring speaker setups,
  - translation side-by-side,
  - QC warnings (too fast, too long),
  - in-context help system: (?) button on every section heading that expands a help panel showing "What this does", "When to use it", "Steps", and "Key terms"; persistent "Show all help" toggle for learning mode.
- **Jobs/Queue**: current work first, with separate `Now`, `Needs attention`, and `History` views; retry and cancel stay primary while logs, outputs, IDs, raw types/errors, and lineage live in focused detail paths. Developer-only test controls stay behind an advanced disclosure, and "Clean up old jobs and logs" remains separate from media/library deletion. Jobs/Queue must be a trustworthy recovery and inspection surface for failed, retried, bundled, and batched downloads: it shows original title, URL, video ID, job ID, batch ID, source/output paths, retry lineage, attempt history, canonical batch health, persisted product track, and safe next actions. Every failed status leads with a classified plain-language reason and required action; raw technical detail remains directly discoverable. Batch, status, and per-track totals come from backend canonical summaries, not visible/rendered row counts. Loading, query failure, no current work, held-provider state, and no job history are distinct states.
- Jobs/Queue must use one compact command row for queue state and primary actions. Per-track scheduler health and the shared YouTube pacing/auth gate belong in one secondary disclosure instead of an always-expanded status wall. Advanced controls must write the settings the scheduler actually consumes.
- Jobs/Queue must expose one backend-selected source/track filter for All, YouTube singles, subscriptions, Instagram, other video, Image Archive, and Localization. The selected value filters the canonical bounded query, not only the rendered rows. Subscription job rows retain their enqueue-time channel/playlist/page display identity across queued, running, failed, retried, and completed states.
- Jobs/Queue tables and expanded batch members must use panel-local scrolling plus bounded incremental rendering or virtualization. Canonical totals remain visible and explicitly distinct from the loaded/rendered window; expanding a large batch must not mount every member at once.
- Batch retry and repair must start as bounded background operations and return an attributable receipt immediately. Jobs remains navigable while canonical dry-run/retry/repair work executes, exposes running/completed/failed state, prevents duplicate concurrent work for the same batch and mode, and refreshes canonical Jobs truth after completion.
- The Video Archiver Single Videos surface must combine canonical active batch members with canonical completed history: every submitted member has a queued/running/held/failed/downloaded state, stable job identity, and truthful numeric or labeled indeterminate progress. Progress ticks must not trigger full history/library refreshes.
- All archive ingress paths must use one canonical media identity per source video. Existing present or active media is not enqueued again. Missing media, unreachable storage, and invalid sources are distinct states; missing-media repair offers verified relocation or explicit redownload, while a failed old URL offers replace-link/retry or explicit metadata-only removal. Single and subscription discovery associate with the same canonical item/file and retain all lineage.
- Queue reconciliation must inspect the full canonical queued YouTube set, group work by service plus media ID across every batch and track, cancel all queued work for media already present, and otherwise retain one deterministic queued keeper while canceling—not deleting—redundant attempts. Every source membership, batch association, and attempt record remains durable.
- Immediately before a queued direct YouTube job can start network or `yt-dlp` work, execution must revalidate canonical present/active/missing/unreachable state. Stale work suppressed by this gate must not start a downloader process, and its terminal record must explain the canonical reason.
- Operator-deleted media is a canonical lifecycle tombstone, not repairable missing media. Normal Video Archiver and Media Library projections exclude it; dedicated Deleted projections keep it discoverable. An All projection orders available rows before deleted rows. Delete and redownload actions operate on explicit stable item IDs and return per-item receipts.
- Library maintenance must expose unified imported/current identity coverage, source memberships, duplicate candidates, keeper selection, quarantine state, and rollback through existing toolbar, drawer, table, and detail patterns without adding a new dashboard card.
- Progress UI must update through bounded active projections and stable keyed rows. Heavy history, archive statistics, and filesystem/storage checks run at separate cadences only for visible surfaces, with bounded NAS health states and no whole-page loading flash on progress ticks.
- **Diagnostics**: storage usage, logs export, version info, privacy settings, panel-switch latency, command overlap, database wait state, queue/identity-claim pressure, bounded NAS-stage timing, and host process-pressure evidence.
- **Diagnostics** should also surface a voice-backend catalog, backend readiness, and recommendation reasoning instead of only package versions.

### 8.0.1 Current top-level windows (implemented 2026-03-03)

- **Localization Studio**: first/default window, focused on subtitles + dubbing workflow.
- **Localization Studio** also keeps a lightweight ingest block in-context for local import and source language selection, even when the editor is already open.
- **Localization Studio** first-screen home should prioritize current-item continuation, recent localization items, workflow/readiness, outputs handoff, and advanced-tool entrypoints before import/setup utilities.
- **Video Archiver**: local import + URL batch ingest + presets/templates + YouTube subscriptions/groups + imported archive reconciliation. Its only primary workflow selector is `Single videos`, `Subscriptions`, and `Other websites`; it must not add a competing page-wide Quick/Advanced mode. Destination/library state stays compact and always visible, while library administration, presets, migration, and rare controls use contextual disclosures beside the workflow they affect. The subscription surface is a master-detail manager with bounded incremental rendering or virtualization for both the source list and selected-source video lists. Canonical totals remain visible and explicitly distinct from the rendered window. Its downloaded-single-video history is a canonical paged backend projection; mapped subscription outputs and unclassified older items cannot leak into it through path conventions. The canonical page must render independently of the secondary full-library unclassified-legacy count; that exact count loads in the background and reports unavailable state without blocking navigation.
- **Instagram Archiver**: dedicated Instagram batch ingest workflow plus recurring archive targets. Quick/Advanced toggle (Quick shows batch + recent media; Advanced adds subscriptions).
- **Image Archive**: dedicated crawler-based image archive ingest workflow. Quick/Advanced toggle (Quick shows URL + output; Advanced adds Pinterest crawler and crawl settings).
- **Media Library**: renamed from ambiguous "Items"; browse imported media and hand off to Localization Studio. Includes search, source filter, type filter, sort, view mode, and group-by controls.
- **Jobs/Queue**: execution state + retry/cancel + logs and output reveal. The default view is current queued/running work; failed work needing action and terminal history are separate explicit views. A compact queue command row leads the page. One secondary scheduler-health disclosure contains canonical foreground YouTube, background YouTube, Instagram, other-video, Image Archive, and Localization totals plus shared YouTube gate state. One compact source/track filter replaces a second horizontal tab strip. Developer tools and real track tuning stay behind an advanced disclosure; per-job actions are grouped into a primary action plus detail/more path. Large batches remain locally scrollable and incrementally rendered, collapsed batch rows still expose title/link/source context, and latest-attempt state leads the display while historical failures remain available in attempt history.
- **Diagnostics**: non-blocking, section-by-section loading with explicit readiness states, recent local trace rows, and startup/tool-lifecycle visibility.
- Localization Studio artifact rows must receive typed runtime metadata from the bridge for rerun/status matching, rather than reconstructing artifact identity from filenames in the UI.

### 8.0.2 Workspace hardening state (implemented 2026-03-07)

- **Localization Studio** now includes the lightweight ingest block for local import/refresh plus ASR-language selection because this is the primary operator workflow.
- **Video Archiver** is the dedicated home for URL ingest, presets/templates, subscription groups, YouTube subscriptions, and imported archive reconciliation.
- **Imported archive reconciliation** distinguishes 4KVDP-managed subscription/playlist containers from unmatched manual folders and loose root files, using the old 4KVDP app-state SQLite when available to preserve folder mapping while keeping ongoing subscription continuity state inside VoxVulgi-managed storage rather than the imported archive path.
- The Library subscription surface should treat mapped output folders and continuity tracking as separate concepts: output overrides decide where media lands, while dedupe / "already downloaded" state is app-managed and may be seeded or merged from imported archive files.
- **Instagram Archiver** is the dedicated home for direct Instagram archive runs plus recurring archive targets.
- **Options** is the discoverable home for shared storage-root configuration and related global path behavior. Feature roots consolidated into a single table (Feature/Path/Status/Actions) instead of 4 separate cards. YouTube auth improved with help text and clear button.
- Browser-cookie auth and recovery flows default to Firefox because that is the operator's current browser session. Automated credential validation and operator-environment testing use Firefox only and must not launch, inspect, or source credentials from another browser; other product-supported sources remain outside this environment's automated test path.
- **Localization Studio** Workflow Map buttons grouped into 4 categories (Captions & Translation, Voice & Dubbing, Quality & Review, Advanced) instead of a flat button row.
- **Built-in visual debugger**: deterministic snapshot tool that captures the active worksurface to `governance/snapshots/` for AI orchestrators; supports subfolder and label for organized captures; triggered by Ctrl+Shift+S hotkey or `window.__voxVulgiRequestSnapshot()`.
- **Headless agent bridge**: localhost-only HTTP server that lets AI agents navigate pages (`POST /agent/navigate`), capture snapshots (`POST /agent/snapshot`), read state (`GET /agent/state`), and check health (`GET /agent/health`) without stealing window focus; port written to `agent_bridge_port.txt` in app data on startup.

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
- Localization Studio should open as a setup-first workbench: select or drop a source file, choose subtitle and dub target outputs, confirm the Options-linked output folder, optionally include a source copy, then use one Start/Stop control with visible percentage progress.
- When English dubbing is selected, Localization Studio must treat voice-cloning runtime setup as part of the Start workflow. If the local voice-cloning packages are missing, Start queues the one-time setup in plain operator language and the run continues automatically after setup succeeds; operators should not have to discover or manually install voice packs from Diagnostics.
- Successful Localization outputs should be listed as thumbnail rows with direct actions for source file, output folder, subtitle location, dub file, and job/working folder.
- Localization deliverable filenames should keep the source stem and add target markers such as `.source.<ext>` and `.dub-en.mkv`; explicit subtitle-only exports may use `.sub-en.srt` or `.sub-en.vtt` when requested.
- Generic cache/history cleanup must never silently remove Localization Studio deliverables, benchmark/report history, or custom output folders.
- Default preview/download video outputs must be MKV. No new managed single, subscription, provider, direct-HTTP, preview, or export video artifact may finalize as MP4.
- Default archive image outputs should prefer JPEG where practical.
- UX must be fast enough for daily production use before any commercial release.

### 8.1.2 Required top-level window model

- Localization Studio (Dub/CC) - default first-run window and main feature surface.
- Localization Studio ingest block - local import/refresh + source language selection in-context.
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
- The explicit move affordance and the minimize/maximize/close controls must stay grouped together in the top-right shell chrome rather than drifting into the content area or opposite side of the window.
- Frameless maximize/fullscreen behavior must align the actual usable native window bounds with the visible app surface; no invisible window region should block interaction with adjacent visible applications.
- Dense per-panel tables should keep their own scroll surface and should keep critical actions visible when horizontal scrolling is required.

### 8.1.7 Installer and uninstall clarity requirement

- Setup/maintenance UI must clearly present:
  - `Update`,
  - `Reinstall (keep preferences and options)`,
  - `Full reinstall`,
  - `Uninstall (keep preferences and options)`,
  - `Full uninstall`.
- `Update` keeps the current installation in place and preserves preferences/options.
- `Reinstall (keep preferences and options)` uninstalls installed program files, then installs again while preserving preferences/options.
- `Full reinstall` removes installed program files plus preferences/options, then installs again.
- `Uninstall (keep preferences and options)` removes installed program files only.
- `Full uninstall` removes installed program files plus preferences/options.
- For existing installs, show a short explainer page before maintenance choice so operators see one-line outcomes for each mode and understand the keep-vs-full distinction.
- Installer and uninstall copy must explicitly indicate that preferences/options live under `%APPDATA%\com.voxvulgi.voxvulgi` and are only removed by the full actions.
- Every shipped desktop installer build must increment the desktop semantic version.

### 8.1.8 Batteries-included installer payload requirement (operator decision 2026-07-31)

- The public/default installer must include every model and dependency required by the complete default localization pipeline: ASR model(s), translation path, diarization pack, source-separation pack, TTS and voice-conversion models with their populated runtime caches (including the Hugging Face cache), portable Python with all pinned wheels, FFmpeg/ffprobe, and every other runtime tool the default path touches.
- First run must be able to complete the full default localization workflow (import -> captions -> translate -> dub -> export) fully offline with zero network downloads.
- Readiness surfaces must never depend on first-run network access for the default path; "ready" must mean the required bytes are already on disk and verified.
- Users may swap models and backends later through in-app surfaces (backend catalog, BYO adapters, model management); swapping is optional and never required for the default experience.
- Non-technical users are the primary persona: no terminal steps, no pip or dependency repair, and no manual model placement may ever be required for the default path.
- Slim or developer installers without the full payload may exist for development purposes only and must not be the public download default.
- Installer size is explicitly not a constraint (operator decision 2026-07-31): the audience is PC users who download videos and are assumed to have disk space for models; do not trade model quality or completeness for payload size.
- This supersedes any earlier treatment of bundled wheels or bundled model weights as optional additions; backlog items WP-0237 (bundle wheels in installer) and WP-0238 (bundled model weights) are elevated from optional to spec-required direction.

### 8.1.9 Minimum-hardware contract and degradation tiers (operator decision 2026-08-01)

- The app must define, detect, and state two runtime tiers for the localization pipeline rather than failing or silently degrading:
  - **Full-quality tier (recommended)**: consumer GPU in the ~8 GB VRAM class (CUDA) plus 32 GB system RAM. Runs the full default stack including GPU ASR, the default LLM translation preset, and neural voice cloning.
  - **CPU-only tier (supported fallback)**: whisper.cpp ASR, a small GGUF translation preset, CPU-capable voice cloning, and CPU separation. Reduced speed and quality are expected and must be stated plainly, not hidden.
- The active tier must be detected at runtime and surfaced in operator language on the Localization Studio and Diagnostics surfaces, including which stages are running in degraded mode and why.
- A missing or unusable GPU must never produce a hard failure on the default path; it must select the CPU tier and say so.
- Every bundled default must have a CPU-tier counterpart in the payload so the CPU tier is also fully offline and requires no downloads.
- Stage backends must be swappable per PRODUCT_SPEC 8.1.8; tier selection sets defaults only and never removes operator choice.

### 8.2 Archiver reliability, provider expansion, and library workspace requirements (operator decision 2026-08-09)

- Panel navigation and job startup must remain responsive against the operator's six-figure library/job history and NAS-backed media roots. UI entry must not synchronously wait for NAS file probes, archive-file recounts, queue-wide aggregation, or full-history title repair.
- Diagnostics must correlate a reported slowdown across UI interaction, frontend long task, Tauri/IPC queue wait, command phase, SQLite query/row work, NAS probe, child-process launch, WebView2 renderer, and host-resource pressure using one incident/span identifier.
- Internal traces must use bounded rotation and retention. Long-lived diagnostic learning belongs in compact aggregates; an unbounded JSONL trace is not an acceptable history store.
- Diagnostics must provide bounded normal and incident-capture modes, including operator actions to arm capture for the next job start or panel switch. `vvwatch` remains the sibling out-of-process evidence path and must correlate with the same incident identifiers where practical.
- The bundled downloader runtime must be current, hash-pinned, offline-bundled, and security-reviewed before authenticated provider work ships. Version/capability changes create a new observation epoch rather than silently mixing unlike behavior.
- YouTube protection must use a deterministic, inspectable controller over a preserved operator baseline. It may apply a temporary effective overlay for request spacing, download sleep/jitter, concurrency, and batch tranche size, but must not silently rewrite saved settings.
- YouTube controller transitions require corroborated provider-specific outcomes separated by a minimum time. Authentication, content-unavailable, network, storage, PO-token/capability, and rate-limit failures are distinct and must not drive the same remediation.
- Automatic protection must expose current mode, effective settings, transition reason, next retry/probe time, recent outcome, and a manual return-to-baseline action. Raw outcome retention is bounded; durable rollups and transition history may grow over time.
- YouTube PO-token provider capability is a separate readiness state from pacing. A missing/failed PO-token capability must not be misrepresented as a bandwidth or concurrency problem.
- Canonical provider metadata is keyed by service plus media ID and records remote title, uploader/channel, source URL, published time, thumbnail reference, provider/runtime version, provenance, and observation time independently from job attempts and filename-derived library titles.
- Display-title precedence is operator override, canonical remote title, imported/file title, then a stable provider/ID fallback. Missing, placeholder, or encoding-damaged titles may be repaired, but operator-authored titles must never be overwritten automatically.
- Structured downloader output must be parsed through an explicit UTF-8/JSON contract rather than delimiter parsing that can corrupt multilingual titles.
- Options must provide module-scoped subnavigation for General, Localization Studio, Video Archiver, Instagram Archiver, TikTok Archiver, Image Archive, Media Library, Jobs/Queue, and Diagnostics. Narrow layouts use a compact accessible selector/rail rather than overflowing horizontal tabs.
- One typed settings registry must identify each setting's owning module, type, validation, default, persisted value, effective value, temporary policy overlay, reset behavior, and diagnostic test path. Existing persisted keys remain compatible unless a governed migration says otherwise.
- YouTube, Instagram, and TikTok subscription managers must share one accessible, bounded master-detail workspace while preserving provider-specific capabilities. Search, status, group, sort, refresh, last result, next check, queued/running state, and hold reason stay visible without placing optional group management before the source list.
- Instagram Archiver must support working single-post/reel ingest and recurring profile archive behavior through a provider adapter. The app must persist last attempt, last success, classified failure, provider/runtime version, cursor/checkpoint, next retry, and hold reason; repeated upstream/auth failures must not loop indefinitely.
- TikTok Archiver must support single-video ingest and recurring profile/channel downloads with canonical TikTok IDs, durable incremental state, provider-specific session/settings, dedupe, job lineage, and the shared subscription workspace. YouTube pacing thresholds must not be copied to TikTok without provider evidence.
- Media Library remains the canonical product name and must integrate YouTube, Instagram, TikTok, Image Archive, Localization, and local/imported media without splitting imported and current items into different product libraries.
- Media Library primary tabs are All, Videos, Images, Audio, and Favorites. Provider, availability, lifecycle, source/subscription, date, and sort are compact dropdown filters applied by the backend to the canonical full set before pagination.
- Favorites are additive metadata keyed by canonical library item ID. Missing, unreachable, deleted, or moved media must not silently remove favorite state or source metadata.
- Media Library search covers canonical remote title, operator/file title, uploader/channel, provider ID, source URL/reference, and tags through an indexed full-set query. Search, counts, selection, and bulk actions must never operate only on the rendered page unless explicitly labeled `loaded`.
- Media Library is list-first with an optional bounded grid view, one compact toolbar, saved views where implemented, and a detail drawer for paths/provenance/activity. New card stacks are forbidden; large paths and technical provenance do not dominate the primary row.

## 9) Top 20 ROI backlog (next additions)

Current direction keeps baseline values intact; these are explicitly deferred/planned features.

Note (2026-04-08): WP-0161 through WP-0182 move several ROI items from backlog to implementation:
- ROI-12 (Batch processing rules): batch-on-import toggles surfaced in Localization Studio home (WP-0174).
- ROI-16 (Derived output browser): exists; sticky quick-actions bar improves discoverability (WP-0178).
- Undo/redo (WP-0175), drag-and-drop import (WP-0176), keyboard shortcuts (WP-0173), and in-context help system (WP-0172) now implemented.
- Remaining ROI items below are still deferred/planned.

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
