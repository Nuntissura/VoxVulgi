# Work Packet: WP-0289 — Revised Stack: First Dubbed Deliverable Through the Installed App (v1)

## Metadata
- ID: WP-0289
- Owner: assistant (implementation) + operator (install + acceptance run)
- Status: BACKLOG
- Created: 2026-08-01
- Depends on: WP-0287 (research basis), WP-0288 (frozen per-stage defaults + backup registry)
- Refinement: `WP-0289_REVISED_STACK_FIRST_DUBBED_DELIVERABLE_THROUGH_INSTALLED_APP_v1_REFINEMENT.md`
- Target milestone: Localization core recovery

## Intent
- What: Implement the benchmark-frozen localization stack as a vertical slice and produce the first dubbed deliverable ever created through the *installed* VoxVulgi app, offline, from a batteries-included installer.
- Why: Operator decision 2026-08-01 — fold the never-closed core closure unit into the first implementation WP of the new stack, because the revised stack replaces every component implicated in the historical failures (whisper-translate hallucination, resemblyzer diarization, Spleeter deadlock, Kokoro/OpenVoice cache miss and quality-fatal cloning).

## THE CLOSURE UNIT (this WP is not done without it)
> One dubbed MP4 of the Haerin clip, produced through the installed VoxVulgi app on a machine with the network blocked, landing in the configured localization export root, with a truthful clone-status label, and reproducible a second time on the Miyeon multi-speaker clip or an explicit visible failure explaining exactly which speaker/stage blocked it.

Historical baseline this must beat (measured 2026-07-31 on the operator machine): 129,077 derived item folders, 4 items with subtitles, 1 item with a single 3.7-second TTS segment, **0 mixes, 0 muxes, 0 exported deliverables**.

## Scope
- In scope:
  - Integrate the WP-0288-frozen defaults for: translation (llama.cpp `llama-server` child process replacing whisper-translate mode), diarization (sherpa-onnx replacing resemblyzer), separation (RoFormer via python-audio-separator replacing Spleeter), voice cloning (direct cross-lingual zero-shot TTS replacing the Kokoro+OpenVoice cascade), and ASR routing if WP-0288 confirms the JA lane.
  - Batteries-included installer payload per PRODUCT_SPEC 8.1.8: all model weights, the full enumerated model graph per backend (from WP-0288's packaging gate), bundled Python wheelhouse with `pip --no-index --require-hashes`, populated Hugging Face cache so `HF_HUB_OFFLINE=1` resolves with zero misses.
  - Runtime tier detection and operator-visible tier statement per PRODUCT_SPEC 8.1.9 (full-quality GPU tier vs CPU-only tier), with CPU-tier counterparts present in the payload.
  - Readiness truth: "ready" must mean verified bytes on disk; retire any marker-file-based readiness that can lie (the `.warmup_ok` class of bug).
  - Timing-fit chain from the field evidence: pre-TTS chunk planning, gap borrowing, LLM shorten-on-overflow, TTS speed parameter, rubberband-first stretch with atempo fallback, explicit `overflow_trimmed` status.
  - The `VoicePlanBlocked`-with-empty-queue silent-stall path must terminate visibly (carried from WP-0262 fix 2b).
  - Retire from the default path: whisper-translate mode, resemblyzer, Spleeter, OpenVoice V2 cascade. Keep them selectable/BYO where licensing allows, per the backup registry.
- Out of scope:
  - Advanced surfaces already built and currently unreachable or secondary (benchmark lab, cast packs, character libraries, BYO adapter expansion, batch dubbing) — frozen until the closure unit is proven.
  - The stage-card/CSS reachability defects and other frontend divergences catalogued in the 2026-07-31 calibration; tracked separately so they do not expand this packet.
  - Non-default backends beyond wiring them as selectable options from the registry.

## Acceptance criteria
1. **Closure unit achieved**: the dubbed MP4 exists in the localization export root, produced through the installed app with the network blocked; its path, byte size, and duration are recorded, and the operator confirms the dub is audible and the voice is recognizably the source speaker's.
2. Clone-truth metadata is correct end to end: run-level outcome (`clone_preserved` / `partial_fallback` / `fallback_only` / `standard_tts_only`) matches what actually happened, and the surfaced label matches the manifest.
3. Multi-speaker second case: the Miyeon clip either completes, or fails **visibly** with a terminal state naming the blocking speaker and stage — never a silent stall.
4. Offline first run: on a clean Windows machine (or clean VM/profile), install and complete import -> captions -> translate -> dub -> export with no network access and zero downloads.
5. Tier behavior: on a CPU-only configuration the app selects the CPU tier, states it in operator language, and still completes the workflow.
6. No stage in the default path performs a runtime model download; Diagnostics reports every default backend as ready from verified on-disk bytes.
7. Proof bundle under `product/desktop/build_target/tool_artifacts/wp_runs/WP-0289/<timestamp>/` with `summary.md`, the exact operator flow exercised, job IDs, artifact paths, and the produced deliverable referenced (PROOF_STANDARD 3.4 — this is a manual/UI-heavy packet; build-only verification is explicitly insufficient).
8. Desktop semantic version incremented and `governance/release/BUILD_CHANGELOG.md` entry appended with WP IDs and a **real commit hash** (the 0.1.68-0.1.133 entries all carry the stale `3fd938c`; this packet must not repeat that).

## Microtask plan (execution units; each independently completable)
- MT-01: llama.cpp `llama-server` supervised child process + translation stage replacing whisper-translate mode (batch + rolling context + glossary + CPS constraints).
- MT-02: sherpa-onnx diarization backend with exact/auto/range speaker-count intent preserved; overlap frames excluded from clone-reference extraction.
- MT-03: python-audio-separator separation backend (single-process) replacing the Spleeter job path; Spleeter demoted to optional/legacy.
- MT-04: cloning TTS backend integration (WP-0288 winner) with the reference min-length/quality gate before synthesis.
- MT-05: timing-fit chain (chunk planning, gap borrowing, shorten-on-overflow, capped stretch, explicit overflow status).
- MT-06: readiness truth rework — byte-verified pack status; retire marker-file readiness; Diagnostics reflects it.
- MT-07: batteries-included payload build (wheelhouse + weights + HF cache) and offline hydration verification.
- MT-08: tier detection + operator-visible tier statement (PRODUCT_SPEC 8.1.9).
- MT-09: visible terminal state for `VoicePlanBlocked` with empty queue.
- MT-10: closure-unit acceptance run + proof bundle + build/changelog with real commit hash.
- MT-11 (added 2026-08-05 by operator decision): **Localization export artifact is MKV only.**
  The Localization Studio deliverable must carry the dubbed audio, the original audio, and the
  subtitle tracks as separately named, selectable tracks. MP4 is dropped for this artifact
  because it silently discards per-track `title` metadata (verified: titles written into an MP4
  do not survive the mux, so players fall back to "Track 1 / Track 2") and is limited to
  `mov_text` subtitles. **The Video Archiver is explicitly out of scope and keeps exporting
  MP4**: single and subscription downloads take their container from `format_preference`
  (`config.rs`) through the yt-dlp path and never reach `MuxDubPreviewV1`.
  Also folds in three defects found in the same code path:
  keep-original-audio defaulted to `false` while every auto-pipeline call site passed `None`
  (so the second track was unreachable in practice); subtitles were never embedded at all
  (SRT/VTT written only as sidecars); and `-shortest` truncated the output to the shortest
  input stream (observed: a 7.15 s clip cut to 6.25 s by a 6.32 s subtitle stream), which
  violates the field lesson "preserve trailing video after the last dubbed segment".
  Legacy `mux_dub_preview_v1.mp4` files from older builds stay playable; the app simply never
  produces a new one.

## Test / verification plan
- Per-microtask: focused automated checks on the touched boundary (engine `cargo test`, desktop build, contract tests).
- App-boundary: headless agent bridge (`--agent-headless`) for state/dump/snapshot evidence of the Localization Studio surfaces after each stage completes.
- Acceptance: the closure-unit run performed on the installed app by the operator, network blocked, with `vvwatch` sampling in parallel to capture responsiveness and any freeze/DB/bridge failures during the run.
- Regression guard: the historical failure signatures must be explicitly checked — no `OfflineModeIsEnabled`/`LocalEntryNotFoundError` in job logs, no job pinned at a frozen progress value, no "success" job without a deliverable.

## Risks / open questions
- Largest packet in the localization program; the microtask plan is the mitigation and must be respected — no attempting the whole slice in one pass.
- Python surface shrinks from five packs to two, but the two that remain (separation, cloning) are the heaviest; their bundled-wheel installs must be hash-locked exactly as today.
- Payload size lands around ~40 GB (operator has accepted size is not a constraint); build times and payload-refresh policy per `build_rules.md` need respecting so routine builds reuse a verified payload.
- Windows packaging is undocumented upstream for most cloning candidates; if the WP-0288 winner fails the Windows gate, fall to the next registry entry rather than improvising.
- Scope creep into the advanced surfaces is the historical failure pattern of this feature area; the out-of-scope list is binding.

## Status updates
- 2026-08-01: WP created; closure unit folded in per operator decision.
- 2026-08-05: **MT-11 implemented** (engine + frontend), ahead of MT-01..MT-10, on operator
  instruction ("drop mp4 for localization studio entirely as export artifact, but make sure
  video archiver, single or subscriptions still export mp4").
  - `jobs.rs` `MuxDubPreviewV1`: container forced to MKV (a non-MKV `output_container` request is
    honoured as MKV and logged as `mux_dub_preview_container_forced_mkv`);
    `keep_original_audio` now defaults to `true`; newest translated + source subtitle tracks are
    exported to SRT and embedded as named, language-tagged subtitle tracks; audio tracks carry
    `title` ("English (AI dub - cloned voice)" / "Original audio") with the dub as default
    disposition; `-shortest` removed with the reason recorded inline.
  - `SubtitleEditorPage.tsx`: the "Dub container" MP4/MKV/Auto selector is removed (no longer an
    operator choice) and replaced with a plain-language statement; `getPreferredMuxExportExt()`
    returns `mkv`; the persisted `export_dub_container` preference is now a constant. The preview
    dropdown still plays a legacy `.mp4` artifact when one exists on disk.
  - Video Archiver untouched and verified independent: its MP4 comes from `format_preference`
    (`config.rs:196`) via the yt-dlp download path, which never reaches this job.
  - Verification: `cargo check` clean; `npx tsc --noEmit` exit 0; desktop contract tests
    **104/104 pass**; engine `cargo test --lib` **281 passed / 3 failed**, and all 3 failures were
    reproduced on unmodified HEAD with the changes stashed — 2 `media_cleanup` tests fail
    identically at HEAD (pre-existing, WP-0277 queue-pause precondition) and the
    `job_tracks_runtime_snapshot_aggregate` perf test passes in isolation both at HEAD and with
    these changes applied (it only failed inside the full run while CosyVoice/Spleeter benchmark
    jobs were saturating the CPU). No regression attributable to MT-11.
  - NOT yet done for MT-11: no desktop build produced, so the running installed app (0.1.133)
    still contains the old MP4 mux. Needs a build + version bump + changelog entry before the
    operator can see this in the app.
