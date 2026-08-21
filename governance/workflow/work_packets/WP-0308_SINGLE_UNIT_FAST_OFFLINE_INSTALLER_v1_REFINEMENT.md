---
file_id: WP-0308-refinement-v1
file_kind: refinement
updated_at: 2026-08-21
---

<topic id="operator-request-and-current-evidence" status="active" version="v1" wp="WP-0308" updated_at="2026-08-21">

# Operator request and current evidence

- Operator request: every future public installer must be one unit that a non-technical user can download and install, and installation must be materially faster.
- Exact observed v0.1.175 case: after at least 90 minutes, the Inno progress bar was near 10%. Live process samples showed the installer alive but usually at 0% CPU and only tens of KB/s of its own I/O. C: and D: showed disk queue depths between 11 and 44. Defender was idle during the samples.
- Current public output is not one unit: it is one setup executable plus four required `.bin` slices totalling 7,737,986,006 bytes.
- Current Inno `[Files]` entries recursively compile and install raw `tools/`, `models/`, Hugging Face cache, CosyVoice venv, and voice-backend trees. The visible failing/slow path was inside a Python `__pycache__` tree.
- Current source is a local D: hard disk and the destination is C: SSD. The verified immediate bottleneck was storage latency/contention; raw small-file handling amplifies that bottleneck.

# Spec anchors and scope edges

- Product anchors: `PRODUCT_SPEC.md` sections 8.1.7 through 8.1.9.
- Technical anchor: `TECHNICAL_DESIGN.md` section 2.1.
- Existing installer lineage: WP-0265. WP-0308 supersedes only WP-0265's public delivery shape and raw-file Inno payload layout; it preserves Inno 7 long-path support, the core NSIS maintenance flow, `shellexec` elevation, complete offline payload, update semantics, and user-data boundaries.
- In scope: one public ISO, obvious root installer entrypoint, external non-solid payload archives, native Inno archive extraction, archive/build manifests, always-on installer logging, truthful phase/progress copy, reproducible tool discovery, performance regression gates, and fixture/runtime verification.
- Non-goals: removing Python from the runtime, reducing model quality, creating a slim public installer, downloading dependencies during install, changing maintenance labels, removing user data, or running a destructive production install over the operator's live profile.

</topic>

<topic id="research-basis-and-selected-design" status="active" version="v1" wp="WP-0308" updated_at="2026-08-21">

# Sources checked

- Inno Setup 7 `[Files]` documentation: `external extractarchive` extracts an archive directly from distribution media without copying it first; `ExternalSize` can supply truthful uncompressed progress; solid archives are not recommended because extraction performance can degrade. `https://jrsoftware.org/ishelp/topic_filessection.htm`
- Inno Setup `ArchiveExtraction` documentation: `enhanced/nopassword` embeds the maintained 7-Zip extraction library, supports non-password `.7z`, uses normal dictionary-bounded memory, preserves file properties, and keeps extraction inside the installer. `https://jrsoftware.org/ishelp/topic_setup_archiveextraction.htm`
- Inno Setup 7 revision history: archive extraction runs on a secondary thread; Inno 7 adds extended-length path support throughout Setup/Uninstall. `https://jrsoftware.org/files/is7-whatsnew.htm`
- Inno Setup `SetupLogging` documentation: `SetupLogging=yes` creates a detailed Setup log for every run and is equivalent to always enabling `/LOG`. `https://jrsoftware.org/ishelp/topic_setup_setuplogging.htm`
- Inno Setup `[Run]` documentation: `logoutput` cannot be combined with `shellexec`, while `waituntilterminated` is required to wait for a shell-launched process. `https://jrsoftware.org/ishelp/topic_runsection.htm`
- Inno Setup support-function documentation: `ShellExec` exposes an error code only when launch fails, so a successful shell launch is not proof that the elevated child installed the expected version. `https://jrsoftware.org/ishelp/topic_isxfunc_shellexec.htm`
- Inno Setup install-order and event documentation: `[Run]` occurs after files and before the completed wizard; `BeforeInstall`/`AfterInstall` callbacks provide durable handoff boundaries. `https://jrsoftware.org/ishelp/topic_installorder.htm`, `https://jrsoftware.org/ishelp/topic_scriptevents.htm`
- 7-Zip official release/docs: the maintained command-line archiver creates `.7z` archives with LZMA/LZMA2 and is available as a standalone console tool; current checked release is 26.02. `https://www.7-zip.org/download.html`
- Microsoft Oscdimg documentation: Oscdimg creates ISO files and supports UDF; `-u2 -udfver102 -m` supports a UDF-only image without the default media-size ceiling. `https://learn.microsoft.com/en-us/windows-hardware/manufacture/desktop/oscdimg-command-line-options`
- Current repository installer definition, build driver, full payload policy, v0.1.175 artifact manifest, tests, build logs, WP-0265, and live installer process/disk evidence.

# Selected design

- The public release/update artifact is exactly one UDF ISO named `VoxVulgi_<version>_x64_offline_full.iso`.
- The ISO root contains one obvious user entrypoint named `Install_VoxVulgi.exe`, a short `README.txt`, a machine-readable payload manifest, and a `payload/` directory. Users never need a terminal, extraction tool, Python, pip, or manual model step.
- `Install_VoxVulgi.exe` remains an Inno Setup 7 wrapper around the core NSIS installer, preserving maintenance-mode ownership and the required `shellexec` UAC handoff.
- The payload is built into five non-solid `.7z` archives: default tools, default models, Hugging Face cache, CosyVoice venv, and voice backends. Inno reads them directly from `{src}\payload` through `external extractarchive`; the archives are not compiled into Inno `.bin` slices and are not copied to a temporary directory first.
- Inno uses `ArchiveExtraction=enhanced/nopassword`, `DiskSpanning=no`, and `SolidCompression=no`. `ExternalSize` values come from the generated payload manifest so the progress bar represents uncompressed work.
- The build requires a current 7-Zip CLI and Microsoft's Oscdimg, records their versions/paths in the build transcript, creates archives with fast LZMA2 and `-ms=off`, and emits an ISO only after every expected archive, manifest, core installer, and root entrypoint is present and non-empty.
- Installer logging is always enabled. The installer continuously checkpoints a latest log and retains a timestamped final log in `%APPDATA%\com.voxvulgi.voxvulgi\diagnostics\installer` on success, failure, or cancellation.
- Because the required elevated `shellexec` handoff cannot use Inno `logoutput` and does not expose a reliable child exit code, the wrapper logs handoff boundaries and independently verifies the postcondition through the uninstall registry, install location, main-binary existence, and binary file version. A mismatch raises a setup failure instead of presenting false success.
- Public release publication treats the ISO as the sole user download. Internal manifests/logs remain governed build evidence, not additional user-required parts.

# Rejected options

- Keep the five-file spanned set and merely rename it: does not create one download and does not remove raw small-file Inno processing.
- One giant Inno executable: Windows/Inno practical executable-size limits require spanning for this payload and recreate slow signed-executable startup behavior.
- ZIP that the user manually extracts: adds another large write pass and a technical/manual step.
- 7-Zip self-extracting executable above 4 GB: adds an outer extraction layer, uncertain large-PE behavior, and duplicated temporary I/O.
- Built-in Windows `tar.exe` as a runtime dependency: availability/format behavior varies by Windows version and compressed tar extraction is a two-stage operation in Inno.
- Solid 7z archives: Inno explicitly warns of degraded archive extraction depending on solid block size; retry/random access is also worse.
- Remove bundled Python or `.pyc` files in this packet: changes runtime/first-import behavior and does not satisfy the immediate delivery/install closure unit.

</topic>

<topic id="roi-red-team-and-proof" status="active" version="v1" wp="WP-0308" updated_at="2026-08-21">

# High-ROI additions and reused systems

- Always-on durable installer log: reuses the existing diagnostics root, closes the lost-screenshot/support gap, and lets models diagnose failures without operator transcription.
- Truthful named phases and uncompressed progress: reuses Inno's native archive progress and removes the misleading “several minutes” copy.
- Archive manifest with source file count/bytes, archive bytes/hash, relative destination, and tool versions: reuses current artifact manifest conventions and makes release inputs auditable.
- Performance contract test and representative Python-tree fixture: reuses the Node contract suite and prevents a future model from reintroducing raw recursive Inno entries, solid archives, or public `.bin` slices.
- ISO content verification before publication: reuses Oscdimg/7-Zip listing commands and makes the one-unit claim independently inspectable.
- Archive reuse by content fingerprint: reuses the existing offline-payload fingerprint policy so unchanged dependency archives do not rebuild during app-only releases.

# Risks, failure scenarios, controls, and verification

- ISO mounts but the obvious installer or an archive is missing.
  - Control: list the completed ISO and compare exact required paths/sizes to the manifest before publication.
  - Verify: fixture ISO content test plus full release ISO listing.
- Archive is corrupt or interrupted.
  - Control: 7z archive CRCs, pre-publication `7z t`, installer extraction failure with a durable log, and no false ready marker.
  - Verify: valid fixture, truncated archive, missing archive, and retry cases.
- Archive paths escape the intended dependency roots.
  - Control: build only from canonical roots, reject links/absolute/up-level archive entries, and list/audit archives before ISO creation.
  - Verify: traversal/link fixture rejection and long-path extraction fixture.
- Existing-install update overwrites irreplaceable data.
  - Control: archives target only `tools/`, `models/`, `cache/huggingface/`, and `voice_backends/`; database/config/subscriptions/library roots are absent from every archive and guarded by tests.
  - Verify: archive inventory forbidden-prefix test and isolated update-boundary fixture.
- App/yt-dlp/Python process locks payload files.
  - Control: preserve NSIS maintenance close behavior, the WP-0265 elevation handoff, and the new VoxVulgi-owned yt-dlp Job Object lifecycle; installer errors must name the blocked path and persist the log.
  - Verify: controlled locked-file fixture and normal closed-app path.
- Faster archive layout regresses compression/download size excessively.
  - Control: fast non-solid LZMA2, per-archive size reporting, and comparison with the prior full artifact.
  - Verify: record compression ratio and build/install durations; size is secondary to completeness and install speed.
- Build silently falls back to old spanned/raw layout.
  - Control: authority rules plus tests forbid public `.bin` slices, `DiskSpanning=yes`, and recursive raw dependency `Source` entries.
  - Verify: focused contract tests and ISO manifest schema.
- Elevated core installer launches but exits without updating VoxVulgi, leaving the old version running.
  - Control: after the waited shell handoff returns, require the expected uninstall-registry version, recorded install path/main binary, existing binary, and matching binary file version; raise a wrapper failure and preserve the detailed log on any mismatch.
  - Verify: focused logging contract, Inno compile, then clean-profile success plus a controlled mismatched/missing-core postcondition case before publication.
- Performance looks fast on a toy fixture but remains unacceptable on the full payload.
  - Control: two gates: representative Python-tree comparison must be at least 2x faster than legacy raw-file extraction, and the full clean-profile offline install target is at most 30 minutes on the documented reference local-SSD machine with default security settings.
  - Verify: timestamped benchmark receipt with hardware/source/destination/security state and full clean-profile install log. Until full proof exists, status remains IN_PROGRESS/BLOCKED, never DONE.

# Microtask plan

1. Update product/technical/repo/build authority for one-ISO delivery and fast external archives.
2. Refactor the Inno definition to external archive extraction, durable logging, truthful phases, and preserved NSIS elevation/update semantics.
3. Refactor the build driver to discover 7-Zip/Oscdimg, create/reuse/audit five non-solid archives, compile the wrapper, build one UDF ISO, and emit manifests.
4. Update contracts to forbid legacy public slices/raw dependency entries and verify ISO/archive/logging semantics.
5. Run input validation, Inno compile-only/fixture extraction, archive/ISO listing, corruption/long-path/user-data-boundary cases, and representative performance comparison without touching the live install.
6. After the current installation and disk contention end, build the next semantic version through the governed desktop build and run clean-profile/offline full-payload timing before claiming DONE.

# Acceptance gates

- One public ISO is the only user-required release/update download; opening it exposes `Install_VoxVulgi.exe` at the root.
- No public `.bin` slice is required and no raw recursive dependency tree is compiled into Inno.
- The complete default pipeline payload remains present and offline; maintenance labels/data preservation remain unchanged.
- Archive integrity, long paths, missing/corrupt media, locked files, interrupted extraction, log persistence, and user-data boundaries have direct tests.
- The durable installer log contains every named payload phase boundary, core handoff boundary, before/after installed state, expected and observed versions/paths, verification result/failure reason, and terminal outcome; no successful wrapper run may lack a passing core-version postcondition.
- Representative container extraction is at least 2x faster than the legacy raw-file fixture; full clean-profile/local-SSD install target is at most 30 minutes.
- Governed build/version/changelog/proof requirements pass before the artifact is called shipped or DONE.

</topic>
