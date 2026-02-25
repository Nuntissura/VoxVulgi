# App Audit (Reverse-Built): 4K Video Downloader+ (26.0.4.0286)

Date: 2026-02-19  
Method: static inspection of installed artifacts + inspection of runtime artifacts (logs/DB/registry). No decompilation.

## Scope

Install directory analyzed:

- `%ProgramFiles%\4KDownload\4kvideodownloaderplus`

Runtime artifacts analyzed (high-level; contents not extracted):

- `%LOCALAPPDATA%\4kdownload.com\4K Video Downloader+\` (DB, logs, cache, crashdb)
- `%APPDATA%\4kdownload.com\4K Video Downloader+\` (QtWebEngine profile, updater staging)
- Registry keys under `HKCU\Software\4kdownload.com` and `HKCU\Software\4K Video Downloader+`

## Executive Summary

Observed a Windows x64 desktop app built on Qt 6 (Qt Quick/QML + Qt WebEngine/Chromium) with an FFmpeg-based media pipeline and optional AI-related runtimes (ONNX Runtime + DirectML + a custom `lalalai_proc.dll`). The install footprint is large (~556 MB), consistent with bundling a Chromium engine plus media/AI stacks.

Top improvement opportunities (based on confirmed + observed evidence on this machine):

- Security maintenance: OpenSSL 1.1.1s is bundled (EOL); Chromium/QtWebEngine needs a tight patch cadence.
- Privacy/telemetry hygiene: telemetry to `sa.openmedia.co:8018` is confirmed in logs (over HTTP, not HTTPS); the app logs "external IP info".
- Runtime footprint control: local logs are extremely large (~24 GB total across 811 files) and appear to lack retention/rotation.
- Updater staging cleanup: updater caches full extracted app bundles and 7z archives under `%APPDATA%` (hundreds of MB per cached version).
- Automation: LGPL + OpenSSL licensing requires disciplined artifact management and source/notice workflows.

## Evidence Snapshot (Observed)

### Product + Build Info

- Product: `4K Video Downloader+`
- Company: `InterPromo GMBH`
- ProductVersion: `26.0`
- FileVersion: `26.0.4.0286`
- Main EXE: `4kvideodownloaderplus.exe` (~175 MB)
- Windows manifest requests: `asInvoker` (no admin)

### Code Signing

- Authenticode: `Valid`
- Subject: `CN=InterPromo GMBH, O=InterPromo GMBH, L=Baden-Baden, C=DE`
- Certificate validity (observed): `2026-02-13` -> `2026-02-16` (short-lived signing cert)

### Install Footprint

- Files: `1601`
- Total size: `~555.62 MB`

### Executables Observed

- `4kvideodownloaderplus.exe` (main app)
- `QtWebEngineProcess.exe` (Chromium/QtWebEngine subprocess)
- `crashpad_handler.exe` (crash handling)

## Technology Stack & Dependencies (Observed)

### UI + App Framework

- Qt 6.8.3 (`Qt6Core.dll`, `Qt6Gui.dll`, `Qt6Qml.dll`, `Qt6Quick.dll`, `Qt6Widgets.dll`, etc.)
- Qt Quick Controls styles include Fluent WinUI 3 assets (Qt-provided)

### Embedded Browser

- Qt WebEngine 6.8.3 (`Qt6WebEngineCore.dll`, `Qt6WebEngineWidgets.dll`, `Qt6WebEngineQuick.dll`)
- Chromium version string found in WebEngine core: `Chrome/122.0.6261.171`
- Multi-process model via `QtWebEngineProcess.exe`

### Media Processing

- FFmpeg libraries (by file naming): `avcodec-58.dll`, `avformat-58.dll`, `avutil-56.dll`, `swresample-3.dll`, `swscale-5.dll`, `postproc-55.dll`

### Crypto / TLS

- OpenSSL: `libssl-1_1-x64.dll` version resource shows `1.1.1s`
- Qt TLS plugins present: `qopensslbackend.dll`, `qschannelbackend.dll`, `qcertonlybackend.dll`

### AI / Acceleration (likely for "AI processing" features)

- ONNX Runtime: `onnxruntime_omnisale.dll` version `1.21.20250307.2.e0b66ca` (Microsoft)
- DirectML: `DirectML.dll` version `1.15.4+241025-1615.1.dml-1.15.fac7597` (Microsoft)
- Custom module: `lalalai_proc.dll` (no Windows version resource)

### Crash Reporting

- Crashpad (`crashpad_handler.exe`) present
- Crash reporter is configured to upload minidumps to Sentry (`https://o354938.ingest.sentry.io/.../minidump/`; key redacted) and attach the current `app_*.log` (confirmed via `crashpad_handler.exe` command line during an idle run).
- Local crash database exists under `%LOCALAPPDATA%\4kdownload.com\4K Video Downloader+\crashdb\` (minidumps + attachments)

## Network Egress Inventory

### Confirmed in runtime logs

First-party config/news/update:

- `https://dl.4kdownload.com/app/settings/4kvideodownloaderplus.crash.json`
- `https://dl.4kdownload.com/app/settings/4kvideodownloaderplus.settings.json`
- `https://dl.4kdownload.com/app/advertisement/videodownloaderplus.xml`
- `https://dl.4kdownload.com/app/news/videodownloaderplus.json`
- update feed: `https://dl.4kdownload.com/app/appcast/videodownloaderplus.xml`

Telemetry:

- `http://sa.openmedia.co:8018/collect` (event-style query string; includes persistent client id and metadata such as platform, update setting state, and license tier)
- `http://sa.openmedia.co:8018/getinfo` (response is logged and includes "external IP info")

Crash reporting (confirmed via crashpad process args):

- `https://o354938.ingest.sentry.io/.../minidump/` (minidump upload endpoint; key redacted)

### Strings-derived (present in binaries; not confirmed via logs reviewed)

- Google Analytics: `http://www.google-analytics.com/collect`

## Runtime Footprint (Confirmed on this machine)

- Local logs: 811 `app_*.log` files totaling ~24,179 MB in `%LOCALAPPDATA%\4kdownload.com\4K Video Downloader+\4K Video Downloader+\`
- Local DB: a large `<uuid>.sqlite` (~1.16 GB) plus `app.db` (WAL)
- Updater staging: `%APPDATA%\4kdownload.com\4K Video Downloader+\SoftwareUpdate\` contains:
  - 7z archives stored without extension
  - extracted full app bundles per GUID (hundreds of MB per cached version)

## Audit Findings & Recommendations

### Security

- Replace or upgrade OpenSSL 1.1.1: OpenSSL 1.1.1 is end-of-support; bundling it increases exposure surface and patch burden. Prefer:
  - defaulting to Windows SChannel (OS-managed updates), or
  - upgrading to OpenSSL 3.x and enforcing rapid patch adoption.
- Treat QtWebEngine/Chromium as a security dependency: Chromium 122 is not evergreen. If embedded browser is required, define a CVE-driven update cadence and emergency release process.
- Updater hardening: enforce signature/integrity verification for appcast-driven updates and staged bundles.
- Crash dumps as sensitive data: minidumps can contain user/environment data; ensure consent, redaction, and secure transport/storage.

### Privacy

- Telemetry governance: telemetry to `sa.openmedia.co:8018` is confirmed, and the `getinfo` response logs external IP info. Recommended:
  - clear first-run consent screen (region-aware),
  - granular toggles (analytics vs crash reporting),
  - an always-visible diagnostics page listing destinations and what is sent,
  - stop logging external IP in plaintext logs.
- Consider HTTPS for telemetry: `sa.openmedia.co:8018/collect` is HTTP in logs; evaluate risk and migrate to HTTPS if possible.
- LGPL/OpenSSL obligations: third-party notices exist (`thirdparty.txt`, LGPL texts). Add:
  - build-time license manifest generation,
  - an internal checklist for LGPL relinking requirements and OpenSSL notice requirements.

### Performance & Footprint

- Log retention/rotation: implement max size + max age policy; compress old logs; avoid multi-hundred-MB logs in normal operation.
- Updater cache cleanup: cap cached versions and remove stale staged bundles/archives.
- Lazy init: defer WebEngine and AI runtime initialization to reduce cold start (only when features are used).

## Suggested Improvement Backlog (Prioritized)

P0 (trust/security/privacy)

- Migrate telemetry transport to HTTPS (or remove/disable if not required).
- Stop logging external IP and other sensitive telemetry details in plaintext logs.
- Establish Chromium/QtWebEngine CVE patch cadence; define an emergency release path.
- Replace/upgrade OpenSSL 1.1.1 and define patch SLA.

P1 (footprint/reliability)

- Implement log rotation/retention + compression; add a "clear logs/cache" UX.
- Cap updater staging cache; implement cleanup on successful update.
- Improve observability: structured logs + redaction + export bundle.

P2 (engineering quality)

- SBOM generation for bundled DLLs + routine vulnerability scanning.
- Contract-test harness for extractors (YouTube and other fast-changing providers).
