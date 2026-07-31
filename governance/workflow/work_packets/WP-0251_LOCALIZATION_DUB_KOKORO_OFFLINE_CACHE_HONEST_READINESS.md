# Work Packet: WP-0251 - Localization dub Kokoro offline-cache honest readiness

## Status

IN_PROGRESS

## Owner

Claude (Opus 4.8)

## Operator Request Preserved

- "we have been working for a while now on the localization studio, this still does not work as you can see in the jobs panel of the latest haerin test video … for now this never has worked a single time, not even close to producing an audio file or subs."
- Goal of the studio: dub Korean/Japanese vlogs to English + English subtitles, preserving speaker voice timbre/tonality and keeping background sounds. Korean is the priority language. Must be self-contained ("install VoxVulgi and everything is ready"), with swappable models.

## Intent

- What: Fix the defect that caused every `dub_voice_preserving_v1` job to fail before producing any audio/subtitles, and make the readiness gate honest so the failure class cannot silently recur.
- Why: The dub job's first stage (Kokoro base TTS) loads `hexgrad/Kokoro-82M` from the app-local Hugging Face cache (`HF_HOME=<appdata>/cache/huggingface`, `HF_HUB_OFFLINE=1`) at job time. That cache was empty; Kokoro-82M only ever landed in the default *user* cache. A stale `.warmup_ok` marker made the readiness gate report "installed", so re-provisioning into the app cache was skipped. The first synth call raised `OfflineModeIsEnabled`/`LocalEntryNotFoundError`, the job exited 1, and the background-separation + mix + mux + subtitle follow-ups never ran.

## Evidence (canonical, runtime-proven)

- Canonical job row (`db/app.sqlite`, read-only) for job `5b648db6-8636-4181-b8db-4764ac8dc968` (Haerin item `285097bf-...`), type `dub_voice_preserving_v1`, status `failed`:
  - `error: model/tool install failed: voice-preserving TTS script failed (code=Some(1)): … huggingface_hub.errors.OfflineModeIsEnabled: Cannot reach https://huggingface.co/hexgrad/Kokoro-82M/resolve/main/config.json: offline mode is enabled … LocalEntryNotFoundError`
  - Failing frame: `kokoro/model.py … config = hf_hub_download(repo_id, filename='config.json')` from `KPipeline(lang_code='a')`.
- On disk at diagnosis time: `<appdata>/cache/huggingface` did not exist; `C:\Users\Ilja Smets\.cache\huggingface\hub\models--hexgrad--Kokoro-82M` was present; `<appdata>/tools/python/models/kokoro/.warmup_ok` (3 bytes) existed with no model bytes beside it.
- Runtime fix proof: running the exact generated render script `tts_voice_preserving_v1.py` with the existing request against a populated cache produced `voice_clone_outcome: "clone_preserved"`, `segments_converted_ok: 1`, `seg_0000.wav` (148 KB; base 162 KB → converted 148 KB, i.e. timbre transfer real). The whole default downstream pipeline (Spleeter separation → mix → mux → subtitles) is verified GREEN by the WP-0251 investigation workflow.

## Scope

In scope:
- `product/engine/src/tools.rs`: add `kokoro_app_cache_ready(paths)` that mirrors the huggingface_hub offline resolver — read `cache/huggingface/hub/models--hexgrad--Kokoro-82M/refs/main` → commit sha → require `snapshots/<sha>/config.json` + a model `*.pth` (`kokoro-v1_0.pth`) + the default voice `voices/af_heart.pt` (using `is_file()` so a dangling snapshot symlink with a missing blob reads as not-ready).
- Wire that check into both readiness gates: `tts_neural_local_v1_pack_status` (`warmup_ready`) and `tts_voice_preserving_local_v1_pack_status` (`kokoro_warmup_ready`) so a stale marker alone can no longer satisfy `installed`.
- Harden the warmup writer in `install_tts_neural_local_v1_pack`: refuse to write `.warmup_ok` unless `kokoro_app_cache_ready` is true (fail loudly instead of shipping a false-ready state).
- Update the two user-facing status strings to say the model is missing from the app-local cache and to run Install/Repair.
- Add a regression unit test (`kokoro_app_cache_ready_requires_snapshot_files_not_just_marker`).
- Operational (live machine, non-code): populate `<appdata>/cache/huggingface` with the Kokoro-82M snapshot so the operator's running build can complete a dub immediately by retrying the job.

Out of scope (tracked in WP-0252):
- ASR quality upgrade (whisper large-v3) — the current tiny ASR produces garbled Korean.
- Voice-clone quality migration (CosyVoice 2 isolated venv).
- Demucs torchaudio ABI break (latent; Spleeter is the default backend and is GREEN).
- Silent OpenVoice fallback surfacing (a clone-intent job can "succeed" with a generic English voice).
- Offline `payload.zip` hydration verification + Kokoro snapshot pinning in `offline_bundle_prep` (hardening follow-ups, see Notes).
- Desktop semantic version bump / installer build (gated on completing WP-0252 so the build ships the full quality stack at once).

## Research Basis

### Sources checked
- `product/engine/src/jobs.rs` dub path 8770-9620 (env at 9362-9389: `HF_HOME`, `HUGGINGFACE_HUB_CACHE`, `HF_HUB_OFFLINE=1`, `TRANSFORMERS_OFFLINE=1`); generated Kokoro+OpenVoice script 9080-9339; follow-up chain `queue_post_voice_preserving_dub_followup` (5660) → separation → `MixDubPreviewV1` → `MuxDubPreviewV1`.
- `product/engine/src/tools.rs` 2806-3162 (`kokoro_warmup_probe_path`, `tts_neural_local_v1_pack_status`, warmup writer, `tts_voice_preserving_local_v1_pack_status`); `run_python_checked` 3518-3611 (sets `HF_HOME`/`HUGGINGFACE_HUB_CACHE` to the app cache — same as the job).
- Live read-only DB probe + filesystem probes (above). 5-agent investigation workflow (field research + engine map + provisioning verify): confirmed "a hardened fix must verify the model files exist in the offline cache the job will read, not trust a marker file", and that hugingface_hub 1.5.0 resolves `main` via `refs/main` → sha → `snapshots/<sha>/<file>` with no blob requirement.

### Selected approach
- A filesystem check that mirrors the exact offline resolution path (refs/main → sha → required files). Chosen over (a) trusting the marker (the bug) and (b) spawning an offline python load probe on every status poll (too slow for a frequently-polled UI gate). The warmup writer guard closes the "marker written without model present" path.

### Rejected options
- Spawn an offline `KPipeline` load probe inside the status function — rejected for the status path (status is polled often; a torch+kokoro load per poll is too heavy). Reserved as a stronger warmup-time verification (WP-0252 hardening).
- Move Kokoro to an explicit `local_dir` like OpenVoice and point `KPipeline` at it — larger change; deferred to the snapshot-pinning hardening follow-up.

### Risks and mitigations
- Risk: false-negative if a valid install uses a different weight filename. Mitigation: the check accepts `kokoro-v1_0.pth` or any `*.pth` in the snapshot; a false-negative only triggers a (network) re-provision, never a broken dub.
- Risk: partial offline-payload hydration drops `refs/main`. Mitigation: the check requires `refs/main` and the sha-named snapshot, so partial hydration reads as not-ready (the correct, self-healing outcome). Full payload-hydration verification is a WP-0252 hardening item.

### Validation plan
- `cargo test -p voxvulgi_engine` (the new gate test).
- Runtime: render the exact Haerin request against the populated app cache and confirm a non-silent cloned segment + report (done).
- Operator-relayed: retry the failed Haerin dub in the running app and confirm `dub_preview/mux_dub_preview_v1.mp4` + EN subtitle track are produced.

## Acceptance Criteria

- `kokoro_app_cache_ready` exists and is required by both `tts_neural_local_v1_pack_status.installed` and `tts_voice_preserving_local_v1_pack_status.installed`.
- The warmup writer refuses to write `.warmup_ok` when the app cache lacks an offline-resolvable Kokoro snapshot.
- `cargo test -p voxvulgi_engine` passes, including `kokoro_app_cache_ready_requires_snapshot_files_not_just_marker`.
- A `dub_voice_preserving_v1` run on the Haerin item produces a non-silent dub segment with `voice_clone_outcome="clone_preserved"` (render-level proof obtained; full-job proof pending operator retry or smoke run).
- Proof bundle under `product/desktop/build_target/tool_artifacts/wp_runs/WP-0251/<timestamp>/summary.md`.

## Red-Team

- Failure scenario: an unrelated app re-poisons the user cache and the live app cache is cleared. Control: the gate now reads the app cache only; if cleared, `installed=false` → Install/Repair re-provisions into the app cache.
- Failure scenario: huggingface_hub changes its cache layout in a future pin. Control: the check is co-located with the pinned hf_hub version; a layout change would flip the gate to not-ready (self-healing re-provision) rather than silently fail at job time.
- Failure scenario: the warmup runs but writes into a different cache (older bug shape). Control: the warmup writer guard fails loudly instead of marking ready.

## Notes

- 2026-06-15: Diagnosis + durable fix implemented in `product/engine/src/tools.rs`; engine `cargo check` green; unit test `kokoro_app_cache_ready_requires_snapshot_files_not_just_marker` passes. Live app cache populated with Kokoro-82M so the operator's running build can complete a dub on retry.
- Hardening follow-ups identified by the investigation (carried into WP-0252): offline-resolve verification at warmup time and at `payload.zip` hydration (`desktop/src-tauri/src/lib.rs` `extract_payload_zip_best_effort`); pin + sha-verify the Kokoro snapshot like OpenVoice in `pinned_dependency_manifest` + `offline_bundle_prep`; capture stdout (not only stderr) in `run_python_checked` for warmup diagnostics.
- WP remains IN_PROGRESS pending: (1) full-job runtime proof (operator retry or smoke), (2) desktop rebuild that ships this fix together with WP-0252.
