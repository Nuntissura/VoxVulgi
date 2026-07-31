# Work Packet: WP-0262 - Localization Studio: why it never worked (root cause) + fix plan

## Status

IN_PROGRESS (evidence-based diagnosis complete; fixes are operator-rebuild-gated — see honesty note)

## Owner

Claude (Opus 4.8)

## Operator Request Preserved

- 2026-07-01: "can you also do a technical deepdive into the localization studio, this is the main feature of the app but never has worked... ever. not even subtitles, not single speaker, not multiple. haerin video is single speaker, miyeon video is multispeaker and more chaotic environment/sounds." + "so patch it if you can find improvements."

## Honesty note (read first) — why no blind dependency patch tonight

The primary live blocker (cause 2) is a Python-venv dependency stall. Fixing it means changing pinned package versions and REBUILDING the voice venvs, which cannot be validated without a build/install — and the operator instructed "do not build a new version" this session. Guessing a pin blind risks breaking the venv build and making things worse. So this WP delivers the verified diagnosis + a precise, actionable fix plan for the operator to execute with a rebuild + test, rather than an unvalidated dependency edit. The one already-correct-on-disk piece (large-v3 default, Kokoro cache gate from WP-0251/0252) ships in the 0.1.81 build made this session.

## Research Basis (verified against the operator's live machine)

Subagent deep-dive (121 tool-uses): traced the pipeline in `product/engine/src/{jobs.rs,tools.rs,paths.rs,asr.rs}` + `SubtitleEditorPage.tsx`, then verified against the canonical SQLite job store (read-only), the on-disk Haerin artifacts, and live bounded Python import probes of the voice venvs. Ground-truth item: Haerin `285097bf`.

### Pipeline (as built)
ASR (`asr_local`, native whisper.cpp FFI — NOT the Python venv) → Translate → Diarize (`diarize_local_v1`) → voice-plan gate → Dub (`dub_voice_preserving_v1`, runs the TTS backend) → Separation (spleeter/demucs) → Mix → Mux → QC/export. DB-driven state machine: `decide_localization_next_stage` (jobs.rs:2206) + `queue_localization_continuation_from_track` (jobs.rs:2508). "Start localization run" sends `output_mode:"dub"` (SubtitleEditorPage.tsx:4183), so the default always drives the full dub chain.

## Ranked root causes (with evidence)

1. **[PROVISIONING — fixed on disk, first shipped in 0.1.81] Historical Haerin dub failure: Kokoro absent from the app-local HF cache under offline mode.** Job `5b648db6` (`dub_voice_preserving_v1`, status `failed`) error: `huggingface_hub.errors.OfflineModeIsEnabled: Cannot reach .../hexgrad/Kokoro-82M/resolve/main/config.json ... LocalEntryNotFoundError`. The OpenVoice base stage `KPipeline(lang_code="a")` calls `hf_hub_download('hexgrad/Kokoro-82M', ...)` under `HF_HUB_OFFLINE=1` (jobs.rs:9059-9074) against a cache that was empty; a stale `.warmup_ok` made the gate lie "installed". WP-0251's `kokoro_app_cache_ready` (tools.rs:2848) fixed the gate; the app-local cache is now populated (offline `hf_hub_download` succeeds today).

2. **[CODE/PROVISIONING — the CURRENT live blocker] Both dub TTS backends' model-class import stalls for minutes.** Machine load ruled out (`import torch`=1.8s, `import transformers`=1.4s). But: OpenVoice/Kokoro `from kokoro.model import KModel` took **~18 minutes** (one unbounded run; 120s/95s bounded probes timed out) — kokoro 0.9.4 against the venv's **transformers 5.8.1 / torch 2.10.0** (major-version mismatch; kokoro 0.9.4 predates transformers 5.x). CosyVoice (the current DEFAULT, `default_dub_backend_id()`→"cosyvoice", jobs.rs:16986) `from cosyvoice.cli.cosyvoice import CosyVoice2` **times out >150s** even though torch (5s)/matcha (0.4s)/`cosyvoice` top-level (0.3s) are fast. A multi-minute import blows the job command timeout → looks like a hang → NO audio. This is why a fresh dub still cannot produce audio.

3. **[QUALITY] Subtitles are produced but garbled with whisper-tiny.** Three `subtitle_track` rows + files exist for Haerin (`asr/source.srt`, `translate/en.srt`) — so subtitles are NOT a hard break. But whisper-tiny yielded 2 short segments + a hallucinated translation ("I'm not sure if I can express it. I can do it."). WP-0252 makes `large-v3-q5_0` the default (`paths.rs:256 DEFAULT_ASR_MODEL_ID`, 1.03 GB model on disk) → a fresh run in 0.1.81 uses large-v3.

4. **[CODE/UX] Multi-speaker (Miyeon) silently stalls at the voice-plan gate.** No Miyeon job rows or logs exist anywhere. After diarize, the state machine hits `VoicePlanBlocked` (jobs.rs:2222) unless a usable voice reference is auto-extracted for EVERY speaker; chaotic multi-speaker audio fails reference extraction, so the run stalls at `voice_plan` and queues nothing (empty `queued_jobs`, notes-only) — silently. Diarization itself works (resemblyzer produced a valid Haerin track). So multi-speaker fails EARLIER and differently than single-speaker; if it got past this gate it would still hit cause 2.

5. **[GOVERNANCE/TRUTH] WP-0251/0252 were uncommitted and the "shipped 0.1.68/0.1.69" claim was false.** `git log` HEAD = `3fd938c "Build desktop 0.1.67 (WP-0250)"`. All WP-0251/0252 engine changes + the tauri.conf.json bump were uncommitted working-tree edits; no 0.1.68/0.1.69 commit or BUILD_CHANGELOG entry exists. The operator had been testing builds with NONE of the localization fixes. **The 0.1.81 build made this session (from the working tree) is the first build that actually contains WP-0251/0252** — but cause 2 still blocks dub, and the reused offline payload (2026-05-21) predates the large-v3/CosyVoice model additions, so those models may only be present because the running app provisioned them at runtime (verify the payload contains them before relying on offline).

## Why the three modes fail differently
- **Subtitles:** not a break — produced + surfaced; the issue is whisper-tiny QUALITY. Fix = large-v3 (already default, ships in 0.1.81).
- **Single-speaker dub:** historically Kokoro offline-cache miss (cause 1, fixed); currently the TTS model-class import stall (cause 2) — the real blocker to any audio.
- **Multi-speaker dub:** fails earlier at the voice-plan gate (cause 4), silently queuing nothing; cause 2 would block it even past that.

## Fix plan (scoped; each marked by who/how)

### 2a - Dub import stall (cause 2) [OPERATOR REBUILD + TEST; do not guess-pin blind]
- Kokoro venv: pin `transformers` to the last 4.x line compatible with kokoro 0.9.4 (kokoro 0.9.4 predates transformers 5.x) and re-pin torch to the matching supported line; rebuild the venv and confirm `from kokoro.model import KModel` imports in seconds. The exact pin must be chosen against kokoro 0.9.4's own metadata, then validated by a real import — not assumed.
- CosyVoice venv: add a bounded, instrumented warmup that actually imports `CosyVoice2` and logs where it stalls (likely a text-normalizer/onnx/model-graph step in `cli/cosyvoice.py`); make the render wrapper FAIL LOUDLY on a slow import instead of silently exceeding the job timeout. Files: `product/engine/resources/tooling/{requirements,constraints}.cosyvoice.txt`, `voxvulgi_cosyvoice_render.py`, `tools.rs` (`cosyvoice_pack_status` 2891), `jobs.rs` (dub gates 9242-9272).

### 2b - Multi-speaker voice-plan silent stall (cause 4) [CODE — jobs.rs, needs cargo validation]
- When `VoicePlanBlocked` yields an empty queue, raise a VISIBLE, actionable terminal job/item state ("Couldn't build a voice reference for one or more speakers — try single-speaker or provide a reference") instead of silently queuing nothing. Anchor: `decide_localization_next_stage` (jobs.rs:2206) + the VoicePlanBlocked arm (2222). Deferred from this session because it needs careful state-machine edits + a cargo build to validate, which the no-build constraint blocks; scoped here for the next build session.

### 2c - Subtitles-only usable path (cause 3) [FE — SubtitleEditorPage, low risk]
- Expose the subtitles-only `output_mode` prominently in the primary Localization UI so the operator can get usable subtitles (large-v3) WITHOUT the still-broken dub chain. Anchor: SubtitleEditorPage.tsx:4183 (`output_mode` send). High value: gives a working deliverable today while dub is repaired.

### 2d - Ship + verify [OPERATOR]
- Commit the working tree + build (0.1.81 already built this session has WP-0251/0252 + large-v3). Confirm the offline payload actually contains large-v3 + CosyVoice models, or refresh the payload. Re-test Haerin (subtitles should now be usable) and, after 2a, dub.

## Acceptance Criteria
- Diagnosis captured with evidence (done). Operator can act on a precise fix plan (done).
- After 2a: `from kokoro.model import KModel` and `from cosyvoice.cli.cosyvoice import CosyVoice2` import in seconds in their venvs (operator-verified).
- After 2b: a multi-speaker run that can't build references produces a visible terminal state, not silence.
- After 2c: subtitles-only produces usable large-v3 subtitles for Haerin without touching dub.
- No user data deleted; provenance preserved.

## Red-Team
- Blind transformers pin could break other main-venv models (whisper is native, but OpenVoice/others share the venv) — must be validated by a real import + a smoke of the other stages before shipping.
- Reused offline payload predates large-v3/CosyVoice models — offline dub/ASR could still fail if the payload lacks them; verify payload contents or refresh before claiming offline readiness.
- CosyVoice stall may be network/normalizer waiting under offline mode — the instrumented warmup must distinguish a slow import from an offline-fetch hang.

## Notes
- 2026-07-01: authored from the localization root-cause deep-dive during the overnight session. This WP is diagnosis + plan; the dependency repair is intentionally NOT blind-patched (unvalidatable without a build). Corrects the false "shipped 0.1.68/0.1.69" claim in WP-0252's notes (per proof-gated truthfulness) — those changes first ship in the 0.1.81 build made this session.
