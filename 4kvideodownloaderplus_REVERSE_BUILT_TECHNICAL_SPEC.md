# Reverse-Built Technical Spec: 4K Video Downloader+ (26.0.4.0286)

Date: 2026-02-19  
Scope: describes the instance installed on this machine, based on static + runtime artifacts.

## 0) Confidence Levels

- **Confirmed**: observed directly in runtime artifacts (logs, DB schema, files, registry, process list).
- **Observed**: measured from file metadata, PE imports, or embedded strings.
- **Inferred**: deduction from confirmed/observed evidence and common Qt/FFmpeg/WebEngine patterns.

No decompilation was performed. Limited runtime execution was used only to produce and inspect logs/artifacts.

## 1) Evidence Used (Confirmed)

### Install artifacts

- `C:\Program Files\4KDownload\4kvideodownloaderplus\` (EXE/DLLs/resources)

### Runtime artifacts (LocalAppData)

Base:

- `%LOCALAPPDATA%\4kdownload.com\4K Video Downloader+\`

Examples (logs created during this deeper pass):

- `%LOCALAPPDATA%\4kdownload.com\4K Video Downloader+\4K Video Downloader+\app_2026_02_19__04_02_06+0100.log`
- `%LOCALAPPDATA%\4kdownload.com\4K Video Downloader+\4K Video Downloader+\app_2026_02_19__04_10_49+0100.log`

### Runtime artifacts (RoamingAppData)

Base:

- `%APPDATA%\4kdownload.com\4K Video Downloader+\`

### Registry

- `HKCU\Software\4kdownload.com\4K Video Downloader+` (referenced in logs as "app settings path")
- `HKCU\Software\4kdownload.com\ApplicationDirectories\4K Video Downloader+\26.0.4.0286` (install directory)
- `HKCU\Software\4K Video Downloader+` (contains `UET_SDK_*` values)

## 2) Product Overview (Confirmed + Inferred)

4K Video Downloader+ is a Windows desktop application that:

- resolves metadata for supported services (notably YouTube and many others),
- manages a download queue and subscription-style downloads,
- post-processes media (remux/transcode) via FFmpeg,
- supports login/auth flows via an embedded browser (Qt WebEngine) when needed.

## 3) Platform & Runtime (Observed)

- OS: Windows x64
- App type: GUI (`Subsystem: Windows GUI`)
- Privileges: `asInvoker` (no admin requested in manifest)
- Primary framework: Qt 6.8.3
- Embedded browser engine: Qt WebEngine 6.8.3 (Chromium `122.0.6261.171`)

## 4) Persistence & On-Disk Layout (Confirmed)

### 4.1 LocalAppData layout

Base:

- `%LOCALAPPDATA%\4kdownload.com\4K Video Downloader+\`
  - `4K Video Downloader+\` (primary app data: DB + logs)
  - `cache\` (QML cache, shader/pipeline caches, WebEngine cache)
  - `crashdb\` (minidumps + attachments)
  - `QtWebEngine\` (present; older/alternate WebEngine location)

Primary app data directory:

- `%LOCALAPPDATA%\4kdownload.com\4K Video Downloader+\4K Video Downloader+\`
  - `app.db` (SQLite; WAL enabled)
    - schema objects (tables): `Cookies`, `News`, `Notifications`, `meta`
  - `<uuid>.sqlite` (SQLite; schema includes download + subscription domain tables)
    - notable tables: `download_item`, `media_info`, `video_info`, `audio_info`, `track_info`
    - content tables: `subtitle`, `subtitle_file`, `subtitle_status`, `thumbnails`, `temp_files`, `url_description`
    - subscription tables: `subscription_entries`, `subscription_state`, `subscription_timestamp`, plus `subscription_*_info` tables
  - many `app_YYYY_MM_DD__HH_MM_SS+ZZZZ.log` files
    - on this machine: 811 log files totaling ~24,179 MB; individual logs up to ~975 MB
  - large migration backups: `*.sqlite.migration.bak` (hundreds of MB each)

Cache directory highlights:

- `%LOCALAPPDATA%\4kdownload.com\4K Video Downloader+\cache\qmlcache\` (`*.qmlc`)
- `%LOCALAPPDATA%\4kdownload.com\4K Video Downloader+\cache\QtWebEngine\Default\Cache\...` (Chromium cache)
- shader/pipeline caches: `_qt_QGfxShaderBuilder_*`, `qtpipelinecache-*`

### 4.2 RoamingAppData layout

Base:

- `%APPDATA%\4kdownload.com\4K Video Downloader+\`
  - `QtWebEngine\Default\` (persistent Chromium profile data)
    - observed files include: `Cookies`, `History`, `TransportSecurity`, leveldb directories, etc.
  - `SoftwareUpdate\` (update staging)

Update staging details (confirmed from files):

- `%APPDATA%\4kdownload.com\4K Video Downloader+\SoftwareUpdate\`
  - stores 7z archives without extension (magic bytes `37 7A BC AF 27 1C`)
  - stores extracted app bundles under GUIDs, e.g.:
    - `<guid>\4kvideodownloaderplus\4kvideodownloaderplus.exe`
    - `<guid>\4kvideodownloaderplus\QtWebEngineProcess.exe` and supporting DLLs/resources
  - observed cached bundles include older versions (e.g. 25.3.4.0241 and 25.4.0.0248)

### 4.3 SQLite domain schema (Confirmed)

The primary domain DB (`<uuid>.sqlite`) appears to model a download queue, media variants (container/video/audio tracks), subtitles, thumbnails, temp files, and subscriptions.

Key tables and columns (confirmed via SQLite schema inspection; enum mappings not confirmed):

- `download_item`:
  - `id` (PK), `filename`, `state`, `position`, `timestampNs`
- `media_item_description` (per `download_item`):
  - `download_item_id`, `item_index`, `title`, `duration`, `publishing_timestamp`
  - `url_description` (per description): `service_name`, `url`, `handler_name`, `type`
  - `thumbnails` (per description): `content_type`, `data` (BLOB)
- `media_item_metadata` (per `download_item`):
  - `type`, `value`
- `media_info` (per `download_item`):
  - `container`
  - `video_info`: `codec`, `dimension`, `resolution`, `fps`, `bitrate`, `hdr`, `video_360`, `uid`
  - `audio_info`: `codec`, `bitrate`, `layout`, `uid`
    - `track_info` (per audio): `language_tag`, `track_id`, `title`, `type`
- subtitle domain (per `download_item`):
  - `subtitle`: `type`, `language`, `language_code`
  - `subtitle_file`: `filename`
  - `subtitle_status`: `status`
- temp files domain (per `download_item`):
  - `temp_files`: `filename`, `nodename`, `final_size`

Subscriptions (confirmed via schema and table naming):

- `downloader_subscription_info`:
  - `id` (PK), `type`, `dirname`, `parent_id`, `uuid`
- subscription tables keyed by `downloader_subscription_info_id`:
  - `subscription_entries` (`reference`, `status`)
  - `subscription_state` (`state`)
  - `subscription_timestamp` (`timestamp`)
  - `subscription_checks_count` (`count`)
  - `subscription_*_media_info` / `subscription_*_video_info` / `subscription_*_audio_info` / `subscription_*_track_info`
  - `subscription_subtitle`, `subscription_thumbnails`, `subscription_url_description`, `subscription_metadata`

Implication (inferred from schema): the app supports selection of specific audio tracks (`track_info`) and stores subtitle variants and statuses per item.

## 5) Process Model (Confirmed + Observed)

In a short idle run on 2026-02-19:

- `4kvideodownloaderplus.exe` (main GUI process)
- `crashpad_handler.exe` spawned as a child of the main process (crash capture)

Qt WebEngine:

- `QtWebEngineProcess.exe` exists in the install bundle and in update-staged bundles.
- A persistent WebEngine profile exists under `%APPDATA%\4kdownload.com\4K Video Downloader+\QtWebEngine\Default\`, confirming embedded web views were used on this machine (even if not spawned during the short idle run).

## 6) Networking, Remote Config, and Telemetry (Confirmed)

All items below are confirmed via application log output. URLs are shown without user-specific query parameters.

### 6.1 Startup remote fetches (first-party)

On startup the app initiates HTTP requests to:

- `https://dl.4kdownload.com/app/settings/4kvideodownloaderplus.crash.json`
- `https://dl.4kdownload.com/app/advertisement/videodownloaderplus.xml`
- `https://dl.4kdownload.com/app/settings/4kvideodownloaderplus.settings.json`
- `https://dl.4kdownload.com/app/news/videodownloaderplus.json`

Update feed is configured as:

- `https://dl.4kdownload.com/app/appcast/videodownloaderplus.xml`

### 6.2 Telemetry endpoint + event shape

The app sends telemetry to:

- `http://sa.openmedia.co:8018/collect` (event-style query string similar to GA Measurement Protocol)
- `http://sa.openmedia.co:8018/getinfo`

Confirmed telemetry content patterns (redacted):

- includes app name + version (`av=26.0.4.0286`)
- includes a persistent client id (`cid=<uuid>`)
- includes platform/OS version in event labels/properties
- includes update setting state (e.g., AutoUpdateEnabled)
- includes license state metadata (license tier is included; exact value is user-specific)
- includes request/result telemetry for remote config downloads (e.g., "ConfigDownloadResult" for the dl.4kdownload.com config URLs)

Observed event categories/actions in the startup logs include:

- `Application/Run`
- `Application/Platform`
- `Update/AutoUpdateEnabled`
- `License/State`

The `getinfo` response is logged and includes "external IP info" (value redacted).

### 6.3 Notable runtime warnings

Qt WebEngine locale search warning observed:

- searched for `en-US.pak` in:
  - `C:\Program Files\4KDownload\4kvideodownloaderplus\translations\qtwebengine_locales\`
  - `%USERPROFILE%\.4K Video Downloader+` (path searched even if not present)

## 7) Download/Extractor Engine (Confirmed + Inferred)

### 7.1 Graph-based execution model (confirmed by logs)

Logs show an internal "App Graph" model with worker graphs and nodes. Confirmed patterns include:

- graph URLs like `parsers/youtube/...`
- graph name `ChannelSubscriptionGraph` (and a subscription manager)
- background threads like `WorkerGraphManager::checkingThread`

### 7.2 YouTube behavior (confirmed by logs)

Observed request patterns (redacted):

- fetches YouTube channel pages: `https://www.youtube.com/channel/<channel_id>` and listing pages (`/videos`, `/shorts`, `/streams`)
- fetches watch pages: `https://www.youtube.com/watch?v=<video_id>`
- performs service-specific stream URL resolution (logs include `PlayerSigDecryptor`; implementation details intentionally omitted)

## 8) Media Pipeline (Observed + Confirmed)

Observed libraries indicate:

- demux/mux/transcode via FFmpeg DLLs (`avcodec-*`, `avformat-*`, `swscale-*`, etc.)
- audio I/O via PortAudio (`portaudio_x64.dll`)

Confirmed by runtime logs:

- a `ConversionQueue` exists and is configured for parallelism (max active count changed from 1 to 4).
- a downloader queue (`HttpDownloaderQueue`) is also configured for parallelism (max active count changed from 1 to 4).

## 9) Update Mechanism (Confirmed + Inferred)

Confirmed:

- update manager initializes with an appcast feed (`.../appcast/videodownloaderplus.xml`) at startup
- update staging exists under `%APPDATA%\4kdownload.com\4K Video Downloader+\SoftwareUpdate\`
- staged packages are 7z archives stored without file extension, plus extracted bundles by GUID

Inferred:

- user-space staging likely enables download/extract without elevated writes, then applies updates via an installer or copy step (exact apply/verification steps not confirmed from artifacts alone).

## 10) Crash Handling (Confirmed)

- Crash handler: `crashpad_handler.exe` is launched by the main process.
- Crash reporter configuration includes:
  - a Sentry minidump ingestion endpoint (`https://o354938.ingest.sentry.io/.../minidump/`; key redacted),
  - attaching the current `app_*.log` file to crash reports (confirmed via `crashpad_handler.exe` command line).
- Crash database: `%LOCALAPPDATA%\4kdownload.com\4K Video Downloader+\crashdb\`
  - minidumps stored under `reports\*.dmp`
  - attachments directory can include copies of `app_*.log`

## 11) Open Items / Not Confirmed Yet

- Whether telemetry is opt-in or opt-out by region (needs UI inspection).
- Whether GA (`google-analytics.com/collect`) is actively used (present in strings; not observed in reviewed logs).
- Whether Sentry minidump ingestion is actually exercised in normal use (crash reporter is configured for it; no crash was induced in this pass).
- Exact cookie/session strategy: how `app.db` cookies relate to QtWebEngine cookies (both stores exist).
- Exact update verification: signature/integrity checks and rollback behavior.
- AI model acquisition/storage: no `.onnx` files were present in the install directory; models may be embedded or downloaded later.

See `4kvideodownloaderplus_APP_AUDIT.md` for improvement recommendations and a prioritized backlog.
