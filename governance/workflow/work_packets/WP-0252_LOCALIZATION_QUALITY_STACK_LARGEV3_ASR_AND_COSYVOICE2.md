# Work Packet: WP-0252 - Localization quality stack (whisper large-v3 ASR + CosyVoice 2 voice clone)

## Status

IN_PROGRESS

## Owner

Claude (Opus 4.8)

## Operator Request Preserved

- "the goal of the localization studio is to dub korean and japanese to english and create subtitles in english while keeping the speaker voice timbre/tonality ... perhaps the techniques and tools we use for localization studio is not the correct ones or the implementation is not good. we should research this topic again ... have other people attempted this? github? ... make sure good code applies, no vibe coding, no gaps, no scaffolds or mock ups ... address concerns, risks and harden against it."
- Operator decision (AskUserQuestion, 2026-06-15): **"Full quality stack now"** — implement whisper large-v3 ASR + CosyVoice 2 in one push.
- Self-contained ("install VoxVulgi and everything is ready"), swappable models/languages, Korean priority.

## Intent

- What: Raise localization quality from "produces output" (WP-0251) to genuinely usable Korean→English dubbing that preserves the speaker's timbre. Two changes: (1) ASR whisper-tiny → large-v3 (tiny produces garbled Korean); (2) voice clone Kokoro+OpenVoice (timbre-paint only) → CosyVoice 2 (zero-shot cross-lingual clone, offline-by-design).
- Why: Field research (5-agent + 4-agent workflows, GitHub code-level) found two independent quality-fatal defects beyond the WP-0251 cache bug, and that the repo already vendors CosyVoice with a recommendation engine. A Korean-authored field project (voice-pro) independently standardized on CosyVoice + Demucs + WhisperX.

## Scope

In scope (implemented):
- ASR swap: `manifest.json` adds `whispercpp-large-v3-q5_0` (default, ~1.08 GB, locally re-hashed/verified) + `whispercpp-large-v3` (optional, 3.1 GB). `paths.rs` adds `effective_asr_model_id()` + `config/asr_model_id.txt` override (default large-v3 q5_0). All 6 enqueue sites in `jobs.rs`, `models.rs` inventory + test, desktop `lib.rs` health gate, and both provisioning bins use the configured model. whisper.cpp FFI unchanged (loads any ggml; large-v3 alignment tables vendored).
- CosyVoice 2 backend: isolated venv (`tools/python/venv_cosyvoice`, torch 2.3.1 CPU stack), `CosyVoice2-0.5B` model (4.86 GB) under `voice_backends/cosyvoice/pretrained_models`, populated Matcha-TTS submodule. `paths.rs` helpers, `tools.rs` `cosyvoice_pack_status` honest gate + `cosyvoice_venv_python_path`, `jobs.rs` `run_cosyvoice_dub_render` + dub-job routing branch (reuses the existing separation→mix→mux→subtitle follow-up), and `default_dub_backend_id()` (config `dub_backend_id.txt`, defaults to CosyVoice when its pack is installed, else openvoice_v2).
- Render wrapper rewritten to the Kokoro request/report (`VoiceCloneReport`) contract with the audit's hardening: no silent-fallback (clone failures are real failures), multi-sentence chunk concatenation, loud failure on missing model / over-long reference, pass the reference PATH (the frontend load_wav()s it).

Out of scope (carried forward — see Remaining):
- Self-contained fresh-install provisioning of the CosyVoice pack: `install_voice_clone_cosyvoice_v1_pack` Rust function, hashed lockfile (`requirements.cosyvoice.txt` + the setuptools<80 / `--no-build-isolation` recipe validated below), offline-bundle capture of the second venv + model, wetext normalizer-asset pre-cache, and shipping the cosyvoice repo + render wrapper in the installer.
- Demucs torchaudio ABI break (latent; Spleeter is the default separation backend and is GREEN) — separate fix.
- GPU acceleration (CosyVoice CPU RTF ≈ 6.6; fine for short clips, slow for long vlogs).
- A UI backend/ASR-model selector (the config files already make both swappable for an operator/agent).

## Research Basis

### Sources checked (code-level)
- Field projects inspected at source level: SoniTranslate (XTTS-v2 + MDX-Net), open-dubbing (faster-whisper + NLLB + Demucs + explicit timing-fit — closest architectural twin), voice-pro (Korean author: CosyVoice + Demucs + WhisperX), Linly-Dubbing. CosyVoice/Fish-Speech vendored repos on disk; CosyVoice `cli/cosyvoice.py`, `cli/frontend.py` (load_wav contract), `cli/model.py`; FunAudioLLM/CosyVoice2-0.5B HF + ModelScope model cards; ggerganov/whisper.cpp HF tree (large-v3 q5_0/full git-lfs metadata).
- Engine: `jobs.rs` dub path + experimental-backend plumbing, `tools.rs` pack install/warmup + `run_python_checked` env, `models.rs`, `paths.rs`, `voice_backends.rs` (cosyvoice catalog + recommendation), `voice_backend_adapters.rs`, `bin/voxvulgi_offline_bundle_prep.rs`.

### Selected approach + why
- **ASR: whisper large-v3 (NOT turbo) q5_0 default.** Turbo drops the decoder 32→4 layers and degrades most on low-resource languages (KO/JA) — wrong place to economize. q5_0 (~1.08 GB) keeps near-full Korean quality at 1/3 the size; full v3 (3.1 GB) optional.
- **Clone: CosyVoice 2 in an isolated venv.** Offline-by-design (loads from a local `model_dir`, no HF-cache dependency — structurally avoids the WP-0251 Kokoro bug class). Genuine zero-shot cross-lingual clone (Korean reference + English text → English in the speaker's voice) vs Kokoro+OpenVoice timbre-paint. Routed through the existing dub job (not the brittle BYO-adapter path) so the proven separation/mix/mux/subtitle follow-up is reused unchanged.
- **Isolated venv** because CosyVoice pins torch==2.3.1 / transformers==4.51.3 / numpy<2 / onnxruntime==1.18.0, which cannot coexist with the main venv's torch 2.10.

### Validated install recipe (for the install fn / lockfile)
- Base: portable Python 3.11.9. `python -m venv venv_cosyvoice`; `pip install "setuptools<80" wheel` (modern setuptools dropped `pkg_resources`, which `openai-whisper==20231117`'s setup.py needs); `pip install --no-build-isolation -r requirements.cosyvoice.txt` (so openai-whisper builds against the venv's setuptools). Avoid `PIP_CONSTRAINT` with a spaced path (pip space-splits the value). Model via `huggingface_hub.snapshot_download("FunAudioLLM/CosyVoice2-0.5B")` (or ModelScope `iic/CosyVoice2-0.5B`) into `pretrained_models/CosyVoice2-0.5B`. Matcha-TTS: clone `shivammehta25/Matcha-TTS` into `third_party/Matcha-TTS` (empty by default). wetext fetches normalizer assets from ModelScope on first use — warm once online, then job runs need no model download.

### Rejected options
- whisper large-v3-turbo (Korean accuracy regression). XTTS-v2 / Fish-Speech (Fish wrapper is a scaffold with a NotImplementedError + requires a running HTTP server; XTTS quality below CosyVoice for cross-lingual KO). Wiring CosyVoice through the experimental BYO-adapter path (requires operator-saved adapter config + probe + a manifest schema the render script doesn't emit; routing through the dub job avoids all of it).

### Risks and mitigations
- CPU-only (torch 2.3.1+cpu, RTF ≈ 6.6): slow for long vlogs; the WP-0246 stall watchdog auto-fails only well past the command timeout, so healthy long jobs survive. GPU is a separate WP.
- wetext first-run network: job env no longer forces `MODELSCOPE_OFFLINE`; install warmup must pre-cache. HF/transformers stay offline (local snapshot).
- Fail-closed ASR hashes: large-v3 q5_0 size+sha256 re-hashed locally (1,081,140,203 / `d75795ec…`) before commit.
- Honest gate: `cosyvoice_pack_status` verifies the actual model weights + venv python + Matcha + render wrapper exist (WP-0251 lesson), not a marker.

### Validation plan
- `cargo check`/`test` engine (done, green incrementally).
- Standalone render proof on the Haerin item (done — see Notes).
- End-to-end: a CosyVoice dub run producing `mux_dub_preview_v1.mp4` + EN subtitle track (pending: operator run on the rebuilt app, or smoke once the install fn lands).

## Acceptance Criteria

- Default ASR is large-v3 q5_0, swappable via `config/asr_model_id.txt`; all enqueue + health-gate sites honor it; manifest hashes verified.
- A localization dub with the CosyVoice backend produces a non-silent cloned segment (`voice_clone_outcome="clone_preserved"`) and a `VoiceCloneReport` the dub job parses, then runs separation→mix→mux→subtitles to `mux_dub_preview_v1.mp4` + EN track.
- CosyVoice is the default dub backend when its pack is installed; `cosyvoice_pack_status` is honest.
- `cargo test -p voxvulgi_engine` passes.
- Proof bundle under `product/desktop/build_target/tool_artifacts/wp_runs/WP-0252/<timestamp>/summary.md`.

## Red-Team

- Failure: CosyVoice venv install fails on a fresh machine (setuptools/pkg_resources, spaced PIP_CONSTRAINT). Control: validated recipe above codified in the install fn + a hashed lockfile; honest gate reports not-installed so the dub fails loudly with guidance rather than silently.
- Failure: wetext can't fetch assets offline at job time. Control: install warmup pre-caches; HF/transformers offline only.
- Failure: operator on a low-RAM/CPU machine times out on a long vlog. Control: stall watchdog tolerances (WP-0246); document CPU latency; GPU WP later.
- Failure: large-v3 on CPU is much slower than tiny and looks like a hang. Control: provenance + `command_slow` trace rows; tiny retained as an instant offline fallback via the config.

## Notes

- 2026-06-15: ASR large-v3 engine changes complete; `cargo check` (lib+bins+examples) green. CosyVoice: isolated venv installed (torch 2.3.1+cpu, lightning, transformers 4.51.3, onnxruntime 1.18.0, wetext, etc.), model + Matcha provisioned, render wrapper rewritten + path-contract fixed. **Standalone render proof PASSED**: the Haerin Korean reference cloned into English ("I'm not sure if I can express it…"), `clone_preserved`, `used_voice_preserving=true`, 24 kHz / 2.64 s / 63,360 samples; report parses into the engine `VoiceCloneReport`. Dub-job routing + default-backend wiring added.
- Remaining before DONE: engine+desktop build shipping the stack; end-to-end mp4+subs proof; self-contained provisioning (install fn + lockfile + offline-bundle capture + wetext pre-cache + render-wrapper shipping). The validated install recipe is recorded above so a fresh no-context model can implement the provisioning.
- 2026-06-15 (cont.): Shipped in 0.1.68/0.1.69. Fresh-install self-containment now CLOSED: the ~3 MB CosyVoice runtime code (cosyvoice package + Matcha-TTS + prompt assets, pinned commit `ace7c47f41bbd303aa6bf1ea80e6f9fbd595cd40`) is vendored at `src-tauri/voice_backends_seed/cosyvoice` and bundled (`tauri.conf.json resources += voice_backends_seed/**/*`); `lib.rs::seed_cosyvoice_backend_if_missing` seeds it into app-data on first run (never clobbering an existing checkout), so `install_voice_clone_cosyvoice_v1_pack` (download venv + model on demand) has the code it needs. render wrapper + requirements remain engine-embedded (`include_str!`). Remaining: hashed lockfile for the cosyvoice venv (currently a pinned requirements file), wetext pre-cache during warmup (warmup currently allows the one-time fetch), and a clean-machine end-to-end install test.
