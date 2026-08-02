# Work Packet: WP-0288 — Localization Stack Local Benchmark and Backup Registry (v1)

## Metadata
- ID: WP-0288
- Owner: assistant (execution) + operator (listening/judgement calls on voice quality)
- Status: IN_PROGRESS
- Created: 2026-08-01
- Depends on: WP-0287 (research basis: `governance/spec/LOCALIZATION_STACK_LANDSCAPE_2026_07.md`)
- Blocks: WP-0289 (implementation freezes its defaults from this packet's results)
- Target milestone: Localization core recovery

## Intent
- What: Run the per-stage benchmark specs recorded in the 2026-07 landscape refresh against the operator's real content, freeze the per-stage default selection from measured evidence, and record every license-clean non-selected candidate as a durable ranked backup.
- Why: Operator decision 2026-08-01 — "benchmark and record other methods as backups." Research narrows the field but cannot settle JA/KO quality on real video audio; no stage except ASR has trustworthy public JA/KO evidence. A recorded backup chain means a later regression, license change, or hardware constraint resolves to a pre-vetted option instead of restarting research.

## Scope
- In scope:
  - Assemble a fixed benchmark corpus from real operator content (see Test material).
  - Per-stage A/B/C runs using the benchmark specs in each landscape topic (ASR, translation, diarization, separation, voice cloning).
  - The **packaging gate** for every candidate that could be bundled: install into a clean py3.11 venv from a local wheelhouse (`pip --no-index`), pre-populated Hugging Face cache, `HF_HUB_OFFLINE=1`, network blocked, run to first output; enumerate every asset the model graph touches.
  - Freeze per-stage defaults and write them into the landscape doc's recommended-stack topic.
  - Produce a machine-readable **backup registry** listing, per stage, a ranked fallback chain with license, hardware tier, and the measured reason it placed where it did.
- Out of scope:
  - Product-code integration of the winners (WP-0289).
  - Installer payload construction (WP-0289).
  - Any change to shipped defaults before results exist.

## Test material (fixed corpus, reused by WP-0289)
- Haerin clip (single speaker, short) — the historical test case; the only item that ever reached TTS.
- Miyeon clip (multi-speaker, chaotic environment/sounds) — the historical multi-speaker failure case.
- Additional real clips: at least 2 Korean and 2 Japanese, 3-10 minutes, covering music-bed speech, crowd noise, and overlapping speakers.
- Reference conditions for cloning: 3 s / 5 s / 10 s references, each in clean, BGM-bled, and post-separation variants.

## Candidates per stage (from WP-0287; all license-verified)
- ASR: whisper.cpp large-v3 (baseline), large-v3-turbo, Qwen3-ASR-1.7B, Qwen3-ASR-0.6B int8 (sherpa-onnx), Dolphin-small; word timestamps: Qwen3-ForcedAligner vs whisper.cpp.
- Translation: Qwen3.5-35B-A3B Q4, Qwen3.5-9B/4B Q4, Gemma 4 26B-A4B/E4B, Shisa-v2-unphi4-14b (JA), Kanana-1.5-8B / KORMo-10B (KO), Seed-X-7B; baseline = current whisper translate mode.
- Diarization: sherpa-onnx (pyannote segmentation-3.0 + CAM++), same with WeSpeaker ResNet34_LM, pyannote 4.0 + community-1 (ungated CC-BY-4.0 mirror, operator-accepted); baseline = resemblyzer.
- Separation: Kim Mel-Band RoFormer, Bandit-v2 `multi`, TIGER-DnR; baseline = Spleeter (and Demucs as BYO reference).
- Voice cloning: Fun-CosyVoice3-0.5B, GPT-SoVITS v2ProPlus, Qwen3-TTS-1.7B-Base, VoxCPM2, Confucius4-TTS, Step-Audio-EditX (if >=12 GB VRAM available); baseline = Kokoro + OpenVoice V2, to quantify the lift.

## Acceptance criteria
- A benchmark proof bundle exists under `product/desktop/build_target/tool_artifacts/wp_runs/WP-0288/<timestamp>/` with `summary.md` plus structured results (`evidence.json` or per-stage JSON) and the raw output artifacts referenced.
- Every stage has a measured winner with the metric table that justified it, per the metrics defined in the corresponding landscape topic.
- Every bundle-eligible candidate has a recorded packaging-gate result: PASS/FAIL for offline install + offline first output, plus the enumerated asset list of its model graph.
- A durable backup registry artifact exists (machine-readable, e.g. `governance/spec/localization_backend_registry_v1.json` or an equivalent path chosen at execution time) with, per stage: ranked entries carrying `id`, `role` (`default` | `backup` | `byo`), `code_license`, `weights_license`, `gating`, `hardware_tier`, `measured_notes`, `source_urls`.
- The landscape doc recommended-stack topic is updated with the frozen selections and a pointer to the registry.
- Operator-facing listening judgement is recorded for the cloning stage (A/B outcome), since similarity/naturalness cannot be settled by metrics alone.

## Implementation notes
- Reuse the existing job engine where practical so measurements reflect real runtime conditions rather than isolated scripts; where a candidate is not yet integrated, run it through a bounded harness and record that the measurement is harness-level, not app-level.
- Record hardware context (GPU model, VRAM, CPU, RAM) in the summary; every RTF/throughput number is meaningless without it.
- Metrics per stage are already specified in `LOCALIZATION_STACK_LANDSCAPE_2026_07.md` — do not invent new ones; if a metric proves impractical, record why and what replaced it.
- Cloning similarity uses ECAPA/WavLM speaker-embedding cosine between reference and English output; intelligibility uses whisper large-v3 WER on the generated audio.

## Test / verification plan
- Each stage run is reproducible from the recorded command list in the proof bundle.
- The packaging gate is verified with the network actually blocked, not merely with `HF_HUB_OFFLINE=1` set — the 2026-06-14 dub failure proves an offline flag against an unpopulated cache is the exact failure mode to catch.
- Baseline runs (current shipped components) are included so the lift is quantified rather than assumed.

## Risks / open questions
- Hardware ceiling: if the available GPU cannot host the heavy candidates (Step-Audio-EditX, MOSS-TTSD, 35B-A3B at speed), record those as untested rather than rejected, and mark them backup-eligible-pending-hardware.
- Corpus bias: 4-6 clips cannot represent all content; the registry must state that selections are corpus-scoped and revisitable.
- Listening judgement is subjective and operator-dependent; record the operator's verdict as data, not as an objective metric.
- Some candidates may fail the packaging gate outright (Windows undocumented for most Tier-A cloning challengers) — that is a valid and valuable result, not a blocked benchmark.

## Status updates
- 2026-08-01: WP created from operator decision "benchmark and record other methods as backups".
- 2026-08-02: **Baseline stage-reproduction run complete.** Proof:
  `product/desktop/build_target/tool_artifacts/wp_runs/WP-0288/20260802_stage_reproduction/summary.md`.
  - Hardware context recorded (RTX 3090 24 GB / 5950X 16C32T / 128 GB), but the cloning venv
    carries `torch 2.3.1+cpu` and `torch.cuda.is_available()` is `False` — **every number in this
    run is CPU-tier and the GPU is unused by any localization stage.**
  - Root cause of the never-produced deliverable identified from the canonical job store: exactly
    one localization job has ever existed (`dub_voice_preserving_v1`, `5b648db6`), which failed at
    5 % on 2026-06-14 20:11 with `OfflineModeIsEnabled` -> `LocalEntryNotFoundError` for
    `hexgrad/Kokoro-82M/config.json`. The app-local HF cache was populated 2026-06-15 00:51,
    4 h 40 m later, and no dub job has been queued since.
  - Shipped-component baseline measured by re-running the app's own scripts/venvs/models with the
    engine's exact environment: voice-preserving TTS `EXIT=0` (`clone_preserved` 1/1, 3.37 s real
    cloned audio, 87.6 s warm / 433 s cold); Spleeter separation `EXIT=0` (135 s);
    mix+mux `EXIT=0` -> 7.15 s dubbed MP4. **No shipped stage is broken on this machine.**
  - The revised-stack case is therefore narrowed to quality grounds (ASR model routing,
    whisper-translate hallucination + segment loss, cloning similarity), not "nothing works".
  - Still owed by this WP: per-stage candidate A/B/C runs, the packaging gate, frozen defaults,
    the backup registry artifact, and the operator listening verdict.
  - **Blocked input**: the corpus spec requires >= 2 KO and >= 2 JA clips at 3-10 min; only the
    Haerin (7.2 s) and Miyeon (174.7 s) clips exist. Operator supply or selection needed.
