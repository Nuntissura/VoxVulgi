---
file_id: WP-0306-REFINEMENT-v1
file_kind: work-packet-refinement
updated_at: 2026-08-10
---

<topic id="operator-request-and-verified-state" status="active" version="v1" wp="WP-0306" updated_at="2026-08-10">

# Operator request

- Treat `Z:\Video\4K Video\4K Video 21-08-2025` as the intended directly connected NAS archive root after the NAS was removed from the router and wired to the PC.
- Stop producing new MP4 video downloads or managed video artifacts. New single downloads, subscription downloads, provider downloads, direct-HTTP video imports, previews, and video exports must finalize as MKV.
- Put selected video, audio, and subtitle tracks inside the MKV instead of leaving routine SRT/VTT companion files.
- Keep every already-downloaded MP4 recognizable and usable. Historical MP4 conversion, movement, deletion, or cleanup is explicitly deferred to a later cleaning pass.
- Make the policy durable in repository authority so later models do not reintroduce MP4 output assumptions.

# Verified current state

- The exact intended `Z:` path was tested read-only on 2026-08-09 and was not visible to that agent session. `Win32_LogicalDisk`, `Get-SmbMapping`, and `net use` exposed no `Z:` volume or SMB mapping. At that observation the path was `UNVERIFIED`, and no active storage config or database path was changed to it.
- On 2026-08-10, the direct Ethernet link exposed Synology peer `169.254.99.26`; `MIR.local` resolved to that APIPA peer, TCP/445 was reachable, and the existing canonical `\\MIR\home\Video\4K Video\4K Video 21-08-2025` path was readable. Windows `Z:` was mapped to that verified `\\MIR\home` share with `persistent:yes`, after which the exact intended `Z:\Video\4K Video\4K Video 21-08-2025` target passed a fresh read-only container check and `net use` reported that new connections will be remembered. Read-only `fsutil file queryfileid` returned the same directory identity `0x000000000000026a` for the UNC source and `Z:` target, proving this is an intentional logical-root migration over the same media tree rather than a byte move. Current target reachability over the direct link is proven; restoration after a real sign-out/reboot remains unverified, and the guarded root-rebind mutation has not run.
- Machine-local `feature_storage_roots.json` and the active `video_library` row still reference the former `\\?\UNC\MIR\home\Video\4K Video\4K Video 21-08-2025` root.
- Inspection found 250 of 262 YouTube subscription output overrides on the former root, 11 empty overrides, one non-device-prefix UNC variant, and approximately 140,791 library paths under the former root. These are canonical metadata/path populations, not permission for a blind text rewrite.
- The current saved default archive preset constrains formats to MP4/M4A. Engine code also unconditionally appends `--merge-output-format mp4` and `--remux-video mp4`; direct-HTTP video stages use `download.mp4`; current subtitle handling writes/converts sidecars without `--embed-subs`.
- Localization muxing already uses MKV and embeds mapped streams under WP-0289, but stale comments, export selection, UI defaults/placeholders, README text, and archive execution paths still encode MP4 assumptions.
- Library and subscription scanners already recognize both `.mp4` and `.mkv`. That compatibility is required and must be strengthened, not removed.

# Authority and dependencies

- Repo authority: `AGENTS.md` and `CLAUDE.md` section `Managed Video Container Policy (MKV-Only New Outputs)`.
- Spec anchors: PRODUCT_SPEC default archive/output policy, Localization export requirements, 8.2; TECHNICAL_DESIGN mux-preview notes, 6.6, and Video Archive runtime design.
- Existing contracts reused: WP-0289 MKV localization muxing; WP-0298 storage/performance observations; WP-0299 secure downloader runtime/effective command receipts.
- Dependencies: WP-0289, WP-0298, WP-0299. The engine MKV boundary may be implemented independently of a live `Z:` path. The root rebind may not execute until the exact target proof gate passes.

# Scope edges

- In scope: engine-enforced MKV finalization, embedded subtitle/audio/video tracks, preset/UI/docs migration, direct-HTTP remux, output validation, legacy MP4 recognition tests, configurable root alias/rebind support, exact-path preflight, dry-run/backup/receipt, and execution-boundary destination resolution.
- Non-goals: converting or deleting existing MP4, renaming historical files, moving media bytes, reorganizing folders, changing source videos, cleaning duplicates, hardcoding `Z:` into product code, or accepting a missing/unverified target.

</topic>

<topic id="research-basis-and-selected-design" status="active" version="v1" wp="WP-0306" updated_at="2026-08-09">

# Sources checked

- yt-dlp option contract for `--merge-output-format`, `--remux-video`, `--embed-subs`, subtitle selection, and sidecar behavior: `https://github.com/yt-dlp/yt-dlp/blob/master/README.md`.
- FFmpeg stream selection/mapping and mux behavior: `https://ffmpeg.org/ffmpeg.html`.
- FFmpeg Matroska muxer metadata behavior: `https://ffmpeg.org/ffmpeg-formats.html`.
- Current VoxVulgi config defaults, downloader command builder, direct-HTTP stage, localization mux/export selection, library/subscription extension recognition, UI presets/copy, machine-local storage config, and active SQLite path populations.

# Selected MKV output design

- Define one engine-owned managed-video output policy whose final container is `mkv`. UI state, saved presets, legacy queued arguments, provider defaults, filenames, or direct source extensions cannot override it.
- Saved format preference selects source quality/streams and must not constrain sources to MP4/M4A. The execution builder appends the canonical MKV merge/remux arguments after sanitizing conflicting container arguments.
- Routine video archive jobs select configured subtitle languages/formats and use embedded subtitle output. Temporary downloader/FFmpeg subtitle files are job-scoped staging only and are removed after a verified successful MKV; explicit subtitle-only jobs remain allowed to retain SRT/VTT.
- Treat subtitle absence truthfully: an MKV with video/audio may succeed when no selected subtitle exists, but the job receipt must say `no_selected_subtitle_available`; it may not falsely claim an embedded subtitle.
- Direct-HTTP video downloads stage under a neutral temporary filename, then FFmpeg stream-copy/remux with explicit stream mapping to an MKV. Remux/validation failure leaves recoverable job staging and a failed/attention job; it must never import the MP4 staging file as a successful new managed output.
- Final validation uses ffprobe (or an equally authoritative bundled probe) to require Matroska container, at least one video stream, expected audio when the source exposes audio, and every selected/downloaded subtitle stream with source-supplied language/title metadata where available.
- Localization and export-bundle selection prefer/produce MKV for new work while continuing to enumerate historical MP4 artifacts as legacy inputs or prior outputs.

# Selected direct-NAS root design

- Keep the target path machine-local. Repository product code stores no `Z:` literal and discovers all roots through existing storage configuration.
- Add a machine-local root-rebind contract with `from_root`, `to_root`, verification timestamp, bounded identity evidence, dry-run counts, config/database backup reference, and status. It must support Windows device-prefix and ordinary UNC spellings through canonical path normalization.
- A rebind first verifies that the exact target exists and is reachable, then compares bounded stable relative-path/media identity samples between old metadata and the new root. A drive letter alone is never identity proof.
- Before mutation, create and independently reopen a SQLite/config backup. Produce a dry-run enumerating exact affected canonical surfaces: feature root, active video-library root, subscription overrides, queued/output destination parameters, and legacy library paths.
- After proof, atomically update the active feature/library root plus exact descendant subscription overrides so every new write targets the new root. Execution-boundary destination resolution also maps legacy queued destinations through the verified alias.
- Keep historical `library_item.media_path` values intact for the later cleaning pass. A verified root alias resolves those paths for availability, open, reveal, playback, preflight, dedupe, and identity. This avoids a risky 140k-row rewrite while preventing false missing-media results.
- Root aliases are one-to-one, bounded, inspectable, cycle-rejected, and disabled when the target is unavailable. Ambiguous or partial prefix matches fail closed.

# Existing systems reused

- Storage-root configuration and active library rows; library identity/dedupe and availability flows; subscription output overrides; job parameter parsing; WP-0289 FFmpeg MKV mapping/probing; yt-dlp launch builder; Diagnostics receipts; build/proof/headless bridge.

# Rejected options

- Hardcode `Z:` into source or repo spec: violates portability and breaks if Windows remaps the directly attached NAS.
- Blindly rewrite all old path strings: does not prove target identity, risks metadata loss, and conflicts with the deferred cleanup pass.
- Keep new downloads as MP4 and convert later: creates new forbidden artifacts and loses the single execution-boundary guarantee.
- Rename `.mp4` to `.mkv`: does not change the container and corrupts format truth.
- Rely only on saved preset migration: old queues/custom presets could still request MP4.
- Always require subtitles for job success: many sources legitimately expose no selected subtitle; truth must come from source capability and the output probe.
- Retain routine SRT/VTT beside every MKV: contradicts the embedded-track operator workflow and duplicates deliverables.

</topic>

<topic id="roi-red-team-microtasks-and-proof" status="active" version="v1" wp="WP-0306" updated_at="2026-08-10">

# High-ROI additions

- Central container-policy constant and argument sanitizer: reuses the common command builder, closes legacy-preset/queued-job gaps, and makes Instagram/TikTok implementation cheaper.
- ffprobe output receipt: reuses the bundled probe, prevents extension-only false success, and supplies diagnostics/adversarial proof.
- Root-alias resolver: reuses existing metadata instead of mass rewriting it, keeps historical MP4 reachable, and makes future drive-letter or mount changes recoverable.
- Dry-run plus backup receipt: reuses current config/SQLite surfaces, protects irreplaceable subscription/library metadata, and gives no-context models exact mutation scope.
- Path/container compatibility matrix tests: cheaply prevents later models from interpreting MKV-only output as MP4 input removal.

# Gaps closed against current behavior

- Removes engine, preset, UI, direct-HTTP, comment, README, and export-selection paths that can create or prefer a new MP4 video.
- Replaces subtitle sidecar-as-default behavior with verified embedded tracks.
- Prevents old queued presets from bypassing the policy.
- Separates new-output policy from historical-input recognition and from the future cleanup pass.
- Prevents a NAS topology change from becoming an unverified 140k-row metadata rewrite.

# Risks, failure scenarios, controls, and verification

- MKV filename exists but contains the wrong container or missing streams.
  - Control: probe the completed file before import/success; persist format/stream receipt.
  - Verify: video-only, video+audio, multi-audio, subtitle-present, subtitle-absent, and malformed-output fixtures.
- Conflicting saved/queued yt-dlp arguments still produce MP4.
  - Control: sanitize container/output-extension flags and append engine policy last.
  - Verify: default, custom legacy preset, retried old job, and provider-specific command-capture tests assert no MP4 finalizer.
- Direct-HTTP remux fails because a codec/stream cannot be represented as assumed.
  - Control: Matroska stream-copy with mapped supported streams, bounded fallback only when governed, visible failure, retained job staging, no MP4 import.
  - Verify: representative MP4/WebM inputs, multiple streams, bad file, interrupted process, and retry.
- Subtitle download succeeds but embedding fails or metadata is lost.
  - Control: job-scoped staging retained on failure, explicit language/title mapping, post-mux probe, delete sidecar only after proof.
  - Verify: Unicode subtitle metadata, multiple languages, auto/manual captions, embedding failure, and subtitle-only export.
- Legacy MP4 becomes invisible, duplicated, or queued for automatic conversion.
  - Control: keep extension readers and canonical identity format-neutral; prohibit cleanup/conversion in this packet.
  - Verify: MP4 import, scan, open, play, reveal, availability, dedupe, retry lineage, subscription membership, migration, and Media Library search.
- `Z:` is absent, stale, or points to a different filesystem.
  - Control: exact live preflight and bounded identity samples; no mutation while unverified.
  - Verify: missing drive, unreachable target, wrong media tree, correct target, and reconnect cases.
- Prefix migration corrupts unrelated paths or double-maps aliases.
  - Control: canonical path-component matching, exact dry-run populations, one transaction, backup, alias cycle/ambiguity rejection.
  - Verify: device UNC, normal UNC, similar prefix, mixed case, separator, queued destination, and already-rebound cases.
- Root update makes historical rows look missing.
  - Control: preserve rows and resolve through the verified alias in every availability/open/dedupe/preflight path.
  - Verify: bounded sample plus aggregate availability comparison before/after; no full render-time filesystem scan.
- Machine-local config rollback and DB state diverge.
  - Control: transaction/receipt state machine, independently reopen backup, idempotent retry/rollback, startup reconciliation.
  - Verify: interruption at each phase and repeated application.

# Microtask plan

1. Add engine container policy, conflict sanitizer, format-neutral source preference, and command-capture tests.
2. Implement yt-dlp embedded-subtitle behavior and post-download MKV stream/container validation.
3. Replace direct-HTTP MP4 finalization with staged stream-copy MKV remux and recoverable failure behavior.
4. Update localization/export selection, defaults, UI copy/placeholders, README/help, and all production MP4-output assumptions while preserving legacy readers/fixtures.
5. Add an exhaustive historical-MP4 compatibility matrix across library, subscriptions, Jobs, dedupe, availability, and actions.
6. Implement machine-local root alias/rebind schema, canonical path resolver, diagnostics, dry-run, backup, and interruption-safe receipt.
7. When and only when the exact `Z:` target is live and identity-proven, apply the root rebind; verify every new destination plus bounded historical-media access without moving bytes.
8. Run adversarial code review, remediate findings, headless UI/diagnostics proof, governed desktop build, changelog, and proof summary.

# Acceptance and proof gates

- Command-capture tests prove every new managed video path finalizes as `.mkv` and no engine/UI/default/custom/queued/provider/direct-HTTP path can finalize a new `.mp4`.
- Probed representative outputs are Matroska with expected video/audio and selected/available embedded subtitle streams; successful routine video jobs leave no user-facing SRT/VTT sidecar.
- Existing MP4 files remain recognized and usable across the complete compatibility matrix and are not converted, moved, deleted, or redownloaded by this packet.
- The exact `Z:` path passed a fresh read-only reachability check on 2026-08-10. The rebind remains blocked until the remediated guarded workflow independently proves bounded target identity, exact dry-run scope, verified backups, and a fresh adversarial PASS. If any remaining proof stays false, MKV implementation may complete but the root-rebind acceptance surface remains not passed.
- A successful rebind includes independently verified backup, exact dry-run counts, atomic config/canonical-root/override change, alias receipt, execution-boundary mapping, and before/after access proof. Historical path rows remain preserved.
- No repository product/authority file hardcodes the operator's drive letter as a runtime default.
- Relevant automated tests, app-boundary command/output proof, headless UI audit/snapshots, governed build/version/changelog, adversarial review receipt, and `summary.md` satisfy PROOF_STANDARD.

</topic>
