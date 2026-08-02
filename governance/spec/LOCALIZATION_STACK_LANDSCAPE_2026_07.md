---
file_id: localization-stack-landscape-2026-07
file_kind: research-landscape
updated_at: 2026-08-01
---

# VoxVulgi Localization Stack Landscape — 2026-07 Refresh (WP-0287)

Status: research basis for the localization stack revision decision. Supersedes `VOICE_DUBBING_TOOLING_LANDSCAPE_2026.md` (2026-02-22, WP-0020).
All license/gating claims were verified 2026-07-31 at primary sources (GitHub repos/releases, Hugging Face model cards and API, vendor pages) by parallel web-research agents; source URLs are cited inline. Claims that could not be verified online are marked UNVERIFIED.

<topic id="purpose-and-constraints" wp="WP-0287" status="final" summary="Why this refresh exists and the binding candidate filters" updated_at="2026-08-01">

## Purpose and binding constraints

Operator decisions of 2026-07-31 (PRODUCT_SPEC 1, 3, 8.1.8; TECHNICAL_DESIGN 2.1) reframe stack selection:

1. **Batteries-included**: the public installer bundles ALL models and dependencies; first run completes the full default localization workflow offline with zero downloads.
2. **Bundleable license filter**: code AND weights must be redistributable (Apache-2.0 / MIT / BSD / CC-BY-commercial). Gated, non-commercial, or unclear-license weights are BYO-only, never the shipped default.
3. **Non-technical primary persona**: no terminal, pip, tokens, or manual model placement on the default path. Open-source app aimed at language students and people enjoying other cultures.
4. **Size is not a constraint** (PC audience): do not trade model quality or completeness for payload size.
5. **Quality axis**: JA/KO -> EN on real-world video audio (music beds, noise, multiple speakers).
6. **Hardware honesty**: record VRAM/CPU needs; the default pipeline must state a minimum-hardware contract.
7. **Repo guardrail**: backends with built-in watermarking/anti-abuse instrumentation are excluded from the default path.

Why now: the 2026-02 landscape predates the WP-0252 finding that the shipped Kokoro + OpenVoice V2 default is quality-fatal for the core voice-preserving promise, and five months of rapid field movement.

</topic>

<topic id="asr-ja-ko" wp="WP-0287" status="final" summary="ASR refresh: keep whisper large-v3 for KO, add Qwen3-ASR-1.7B for JA, ForcedAligner for word timestamps" updated_at="2026-08-01">

## Stage 1 — ASR (JA/KO)

**Recommendation: per-language routing.** Keep whisper.cpp + `large-v3` as the KO default and universal fallback; add **Qwen3-ASR-1.7B** (Apache-2.0 code+weights, ungated) as the JA default; add **Qwen3-ForcedAligner-0.6B** (Apache-2.0, explicit JA+KO support) as the word-timestamp sidecar for both lanes. Runner-up if a second engine is too much surface: `large-v3-turbo` on the existing whisper.cpp engine (MIT, 4-6x faster, JA/KO within a hair of large-v3).

Key evidence:

- KO real-world (OpenKoASR leaderboard, KsponSpeech + AIHub, shared normalization): whisper-large-v3 avg CER 0.1062 — best verified; Qwen3-ASR-1.7B CER 0.1993 (~2x worse on KO). https://gt-kim.github.io/open-korean-automatic-speech-recognition/
- JA real-world TV/anime/variety (Neosophie 2026-02/2026-04 benchmarks): Qwen3-ASR-1.7B CER 0.140 vs whisper-turbo 0.184; second set 0.0823 vs whisper 0.1565. https://neosophie.com/en/blog/20260226-japanese-asr-benchmark ; https://neosophie.com/en/blog/20260427-qwen-finetuned-model
- Qwen3-ForcedAligner-0.6B: 11 languages incl. JA and KO, mean alignment error 42.9 ms vs WhisperX 133.2 ms; aligns arbitrary transcript+audio (works on whisper output too); ships in the same `qwen-asr` pip package. https://huggingface.co/Qwen/Qwen3-ForcedAligner-0.6B
- Qwen3-ASR packaging: `pip install qwen-asr` (Apache-2.0, torch+transformers), native transformers support since 2026-06-26, offline-safe from a local model dir. GPU (fp16/bf16) recommended; CPU fallback = whisper.cpp lane, or CrispASR GGUF Q4 (young project; quantization loss unmeasured). https://github.com/QwenLM/Qwen3-ASR

Rejected (verified reasons): kotoba-whisper (real-world TV CER 0.495, worst of 10); ReazonSpeech (JA-only, CER 0.329+ real-world); SenseVoiceSmall (custom non-listed weights license, stale, no independent JA/KO evidence); NVIDIA Parakeet/Canary multilingual (no JA/KO; the `parakeet-tdt_ctc-0.6b-ja` variant is JA-only clean-set, NeMo-heavy — BYO at most); Voxtral Realtime 4B (open model lacks documented timestamps, KO mediocre, 16 GB GPU); Cohere Transcribe (gated + no timestamps); Dolphin (right coverage/licensing, zero published JA/KO per-language numbers — benchmark-worthy BYO); Fun-ASR (repo declares timestamps unreliable); MOSS-Transcribe-Diarize (2026-07-09 release, CUDA-bound, JA/KO quality unpublished — strongest watch item: timestamps + speaker labels in one pass); Omnilingual ASR (40-s clip limit, no timestamps); Granite Speech 4.1 (no KO); Kyutai STT (no JA/KO).

Risks: KO benchmark divergence (Qwen's clean-set FLEURS ko 2.57 vs real-world 0.199 CER — never trust clean-set numbers); LLM-decoder hallucination on music-only stretches; long-form chunking drift in the qwen-asr toolkit vs whisper.cpp's mature pipeline; whisper.cpp release cadence (last tag v1.9.1, 2025-06-19).

</topic>

<topic id="translation-ja-ko-en" wp="WP-0287" status="final" summary="Translation refresh: replace whisper-translate with local instruct-LLM via embedded llama.cpp (Qwen3.5 MoE default)" updated_at="2026-08-01">

## Stage 2 — Translation (JA/KO -> EN)

**Architecture verdict: the current whisper.cpp translate-mode path is a confirmed dead end** (single-pass audio->EN, no glossary/context/register control, hallucination-prone — it hallucinated the Haerin test line). Replace with **local instruct-LLM translation via an embedded `llama-server` (llama.cpp) child process**, translating the transcribed source-language subtitle JSON in context batches. This is the converged field design (Subtitle Edit "llama.cpp advanced", llm-subtrans, VideoLingo, pyvideotrans — all batch + rolling context + glossary block in prompt).

**Recommendation:**

- Default (bundle): **Qwen3.5-35B-A3B-Instruct GGUF Q4_K_M** — Apache-2.0 code+weights; MoE with ~3B active params, CPU-viable on 32 GB RAM and fast on consumer GPUs; direct predecessor Qwen3-30B-A3B-Instruct-2507 holds the strongest verified open-weight JA->EN result (JP-TL-Bench rank #3, above GPT-4o: https://github.com/shisa-ai/jp-tl-bench). Ship Qwen3.5-9B/4B Q4 as low-spec presets.
- Runner-up (bundle): **Gemma 4 26B-A4B / E4B** — Apache-2.0 (license changed vs Gemma 3), day-one llama.cpp support, MT-proven lineage (Gemma 3 is the base of TranslateGemma and MiLMMT-46). Pick if the local benchmark shows a KO->EN edge.
- Optional bundled boosters (license-clean, per-language presets): Shisa-v2-unphi4-14b (MIT, JA specialist, JP-TL rank #6), Kanana-1.5-8B (Apache, Kakao KO), KORMo-10B (Apache, fully-open KO), Seed-X-7B (OpenMDW fully-permissive pure-MT fast path — community GGUF maturity is the risk).
- BYO tier (user drops a GGUF in the models folder, Subtitle Edit pattern): TranslateGemma 12B/27B (Gemma Terms, gated — the dedicated-MT quality leader), MiLMMT-46 (Gemma license), PLaMo-2-translate (revenue-capped license, Japan-government-adopted JA specialist), Tower+ (CC-BY-NC), Aya Expanse (CC-BY-NC), EXAONE (NC, KO).

Unshippable, verified: NLLB-200 weights remain CC-BY-NC-4.0; SeamlessM4T CC-BY-NC; Hunyuan-MT/HY-MT1.5 license territory **excludes the EU, UK, and South Korea** (untenable for a KO-translation app); Llama 4 (custom license + impractical footprint); Sugoi Toolkit (opaque Patreon-first distribution, JParaCrawl research-clause ancestry). MADLAD-400 (Apache) is quality-obsolete as default but acceptable as an emergency tiny-footprint fallback; OPUS-MT/M2M-100 obsolete.

Pipeline pattern (field consensus): whisper stays in **transcribe** mode -> scene-detect via timing gaps -> translate in batches of 10-30 segments with rolling summary + glossary block -> validate line-length/CPS -> constrained-rewrite retry on violations -> smaller batches on failure.

Risks: no public KO<->EN benchmark for open models (local benchmark must carry the KO verdict; boosters are the hedge); Qwen3.5/3.6-specific MT scores unpublished (benchmark old-vs-new before locking the shipped GGUF); LLM failure modes — instruction bleed-through, N-in/N-out drift, refusals on adult content (test explicitly; an uncensored preset may be needed), repetition loops on songs; Gemma 4 license file must be re-verified in the shipped weight repo at bundle time.

Runtime note: `llama-server` as a supervised localhost child process keeps the translation engine model-agnostic and makes the BYO tier free; same GGML family as the whisper.cpp competence already in the product. CTranslate2 (4.8.1, 2026-07-03, active) is only needed for the legacy seq2seq class and is not on the recommended path.

</topic>

<topic id="diarization" wp="WP-0287" status="final" summary="Diarization refresh: sherpa-onnx + pyannote segmentation-3.0 (MIT) + CAM++ embeddings (Apache) replaces resemblyzer" updated_at="2026-08-01">

## Stage 3 — Speaker diarization

**Recommendation: replace the resemblyzer baseline with a sherpa-onnx offline diarization stack** (Apache-2.0 runtime, prebuilt Windows binaries, CPU-first, zero-network from local files):

- Segmentation: `sherpa-onnx-pyannote-segmentation-3-0` ONNX — **MIT weights** (6.6 MB, overlap-aware powerset), already redistributed ungated by k2-fsa. https://huggingface.co/pyannote/segmentation-3.0 ; https://github.com/k2-fsa/sherpa-onnx
- Optional pre-filter: Silero VAD v6.2.1 (MIT, repo-hosted weights) to suppress music/noise phantom speakers. https://github.com/snakers4/silero-vad
- Embeddings: **3D-Speaker CAM++** `zh_en_16k-common_advanced` ONNX — Apache-2.0, 7.2M params, EER 0.65% VoxCeleb1-O, trained on 200k-speaker Mandarin corpus = best East-Asian positioning of any open embedding. Alternate A/B: WeSpeaker ResNet34_LM (CC-BY-4.0). https://huggingface.co/funasr/campplus
- Clustering: sherpa-onnx built-in (`num_clusters` = exact mode, `threshold` = auto); range mode needs a thin wrapper reusing the existing silhouette k-search.
- Cloning-purity lever: exclude overlap frames (powerset output) from per-speaker clone-reference audio.

Runner-up (**ACCEPTED by operator 2026-08-01**): pyannote.audio 4.0 + community-1 weights (CC-BY-4.0) from the ungated community mirror (https://huggingface.co/pyannote-community/speaker-diarization-community-1) — highest open quality (VBx clustering, native num/min/max speakers, exclusive-diarization mode) at PyTorch-stack cost. Bundling conditions: one-time SHA verification against the official gated repo, pinned revision recorded in the pinned dependency manifest, and in-app CC-BY attribution. BYO lane: official gated `pyannote/speaker-diarization-community-1` via HF token (unchanged); DiariZen (CC-BY-NC) as quality-ceiling research BYO.

Rejected (verified): NeMo Sortformer (hard 4-speaker cap, English-primary, v1 offline NC, NeMo Windows weight); Reverb diarization (non-production license); DiariZen as default (NC weights); FunASR (redundant — CAM++ obtainable directly); diart (streaming mismatch, gated deps); senko as dependency (auto-count only, runtime HF downloads — but it validates the CAM++ architecture with published DERs: VoxConverse 13.5%); SpeechBrain ECAPA (2021 model, superseded); resemblyzer (2019 GE2E — weakest link for cloning purity, retire).

Risks: JA/KO diarization performance unverified everywhere (nearest proxies are Mandarin sets — local benchmark carries the verdict); sherpa-onnx ONNX-vs-PyTorch drift (k2-fsa issue #1708 — A/B locally); CC-BY-4.0 attribution text required in-app; singing-voice phantom speakers (Silero pre-filter mitigates, needs testing).

</topic>

<topic id="separation" wp="WP-0287" status="final" summary="Separation refresh: retire Spleeter; Kim Mel-Band RoFormer (MIT) in python-audio-separator; Bandit-v2 as 3-stem option" updated_at="2026-08-01">

## Stage 4 — Source separation (dialog vs background)

**Recommendation: retire Spleeter** (frozen 2019 architecture, TF runtime with Python <3.12 cap, unresolved weights-license question deezer/spleeter#898, and the proven deadlock source in the job runner — WP-0246).

- Default (bundle): **Kim Mel-Band RoFormer vocal model** — weights **relicensed to MIT 2026-04-22** (verified on the HF card + discussion thread; Intel ships an OpenVINO conversion for Audacity), vocals SDR ~10.9-11 (MVSep multisong) vs htdemucs ~8.8 — run inside **python-audio-separator** (MIT, v0.44.5 2026-07-20, actively maintained, explicitly single-process/no-multiprocessing — eliminates the Spleeter deadlock class; pip wheels Python 3.10-3.14, CPU/CUDA; offline via pre-seeded model dir). Do NOT ship audio-separator's default viperx BS-RoFormer model (weights license unstated at host) — configure Kim's MIT model as default; other UVR models remain BYO drop-ins. https://huggingface.co/KimberleyJSN/melbandroformer ; https://github.com/nomadkaraoke/python-audio-separator
- Cinematic 3-stem option (bundle): **Bandit-v2 `multi` checkpoint** — code Apache-2.0, weights CC-BY-SA-4.0 on Zenodo (unmodified redistribution with attribution OK); the only verified redistributable model actually trained for dialog/music/effects on cinematic-style data, **Japanese in its DnR v3 training set** (Korean absent); run via ZFTurbo MSST (MIT, active 2026-07-27, single-process). Lightweight alternative: TIGER-DnR (Apache-2.0 weights, 1.4M params, CPU-viable, DnR speech SI-SDR 15.5 dB) — resolve its repo MIT-file-vs-Apache-badge conflict before bundling. https://github.com/kwatcharasupat/bandit-v2 ; https://zenodo.org/records/12701995 ; https://github.com/JusperLee/TIGER
- **Demucs: BYO only.** Maintainer statement verified (issue #327): "The model weights are not covered by the MIT license, and are provided only for scientific purposes"; repo archived 2025-01-01. Do not redistribute htdemucs weights in the installer.

Rejected: Open-Unmix (best weights NC + quality class below RoFormers); Mini-BS-RoFormer-V2 (CC-BY-NC); Banda (AGPL/dual-license); MVSEP-CDX23 (dormant, license holes); SAM Audio (license actually permits redistribution, but gated acquisition, ~15 GB, CUDA-only diffusion — revisit as optional GPU "pro" backend); SCNet (weights unstated).

Risks: Kim's MIT tag is 3 months old and rests on the author's HF tag — snapshot the model card + license tag + discussion as governance evidence now; vocal-model vs dialog domain gap (sung OST vocals may be extracted out of the background bed — the 3-stem models handle this by construction; A/B on speech-over-sung-OST cases); Korean out-of-distribution for all candidates; SDR numbers across benchmark families are not comparable — only the local benchmark decides.

</topic>

<topic id="voice-cloning-tts" wp="WP-0287" status="final" summary="Voice cloning refresh: retire TTS+VC cascade; CosyVoice3 default, GPT-SoVITS runner-up/CPU lane; Chatterbox excluded on watermark guardrail" updated_at="2026-08-01">

## Stage 5 — Voice-preserving TTS / voice cloning (core differentiator)

**Architecture verdict: direct cross-lingual zero-shot cloning TTS. Retire the TTS + voice-conversion cascade.** No significant 2025-2026 open dubbing pipeline uses TTS->VC; the field wires cloning TTS engines directly. The best open VC (Seed-VC, SECS 0.8676) is GPL-3.0 and its repo was archived 2025-11-21; OpenVoice V2 is dormant with the lowest measured similarity of compared systems (SECS 0.7547, Seed-VC EVAL) — consistent with the WP-0252 quality-fatal verdict. The cascade was the 2023-24 architecture; cross-lingual cloning TTS is the 2026 one.

**Recommendation:**

- Default (bundle): **Fun-CosyVoice3-0.5B-2512** (Alibaba FunAudioLLM) — Apache-2.0 code+weights (ungated), explicit multi-lingual/cross-lingual zero-shot cloning, JA/KO/EN among 9 languages, 0.5B consumer-VRAM class, streaming + instruct speed control, released 2025-12. Runs in the same FunAudioLLM/CosyVoice stack the product already vendored with a validated isolated-venv recipe (WP-0252: torch 2.3.1 pin set) — lowest integration risk, straight upgrade over the already-selected CosyVoice2. https://huggingface.co/FunAudioLLM/Fun-CosyVoice3-0.5B-2512
- Runner-up + CPU fallback (bundle): **GPT-SoVITS v2ProPlus** — the only candidate that is MIT code + MIT weights, JA/KO cross-lingual first-class (`ja`/`ko`/`auto` language codes in its API), officially Windows-packaged (integrated 7z, `go-webui.bat`), ~230M params with a real CPU story, active July 2026. Zero-shot similarity 0.737 (SeedTTS bench) may trail the LLM-TTS tier; optional 1-minute per-speaker fine-tune is a similarity power lane. https://github.com/RVC-Boss/GPT-SoVITS
- Bench challengers (must be in the local benchmark before the default is frozen): **Qwen3-TTS-1.7B-Base** (Apache, the only candidate with published ja-to-en 3.04 / ko-to-en 3.09 cross-lingual numbers; py3.12 + Windows undocumented; low arena naturalness Elo 915), **VoxCPM2** (Apache "free for commercial use", 30 langs incl. JA/KO, ~8 GB VRAM, RTF ~0.30 on RTX 4090; cross-lingual not explicitly benchmarked; the current-generation clone backend in YouDub/tachidubb/OmniVoice field pipelines), **Confucius4-TTS** (Apache, 14 langs incl. JA/KO, no-transcript cloning = best fit for noisy source-video refs, cleanest dependency set of the field — but 2 months old, maturity risk).
- Optional heavy quality packs (bundleable since size is not a constraint; >=12-16 GB GPUs): **Step-Audio-EditX 3B** (Apache, best open-weights arena Elo 1108, JA/KO) and **MOSS-TTSD 8B** (Apache, 20 langs, robust cross-lingual claim).
- BYO power lane: **XTTS-v2** via the maintained idiap fork (best-documented cross-language JA/KO cloning; CPML non-commercial), **OpenAudio S1-mini** (strongest JA data lineage; NC + gated), **Seed-VC** (GPL VC add-on), **F5-TTS** (NC; the field's best hard duration control via `--fix_duration`).
- Current stack disposition: **Kokoro-82M** stays only as a fixed-voice non-clone fallback (Apache, no cloning, no KO); **OpenVoice V2 retired from the default path**.

Guardrail exclusions (verified): **Chatterbox/Chatterbox Multilingual** — unconditional Perth watermarker in `tts.py`/`mtl_tts.py` with no off-switch; otherwise MIT+JA/KO would have made it the top default. **Microsoft VibeVoice** — audible baked-in AI disclaimer + watermark + research-only + EN/ZH. **NeuTTS Air** — Perth watermark default + no JA/KO.

Other rejections (verified, condensed): Fish/OpenAudio S1-mini (NC code AND CC-BY-NC-SA gated weights); IndexTTS-2 (custom bilibili license requiring downstream contractual binding — kills bundling; ZH/EN only; headline duration control disabled in the open release; IndexTTS-1.5 has an HF-tag-vs-license-file conflict resolving to NC); Higgs Audio v2/v3 (custom community license, 24 GB, no JA; v3 NC); MegaTTS3 (WavVAE encoder withheld — local cloning impossible); Spark-TTS (NC), OmniVoice-k2fsa (NC), LLaSA (NC), Zonos (no KO; ZONOS2 Linux-only), Dia (EN-only), Orpheus (gated, no JA), RVC (not zero-shot; checkpoint provenance ambiguity), FireRedTTS2 (Apache tag but vendor restricts cloning to academic research — fatal for a cloning-centric default). Watch items: IndexTTS-2.5 weights (adds JA, paper-only), X-Voice, Fish S2 beta license, ZONOS2 Windows.

Risks specific to this stage:
1. **License drift is the norm**: Fish, IndexTTS, Higgs, NeuTTS all tightened licenses within ~12 months. Pin exact HF revisions and archive LICENSE files + card revision hashes at bundle time. CosyVoice3/Confucius4/Qwen3-TTS/VoxCPM2 are tag-only (no license file in the weights repo) — snapshot the card.
2. **HF tag != effective license** (IndexTTS-1.5 precedent) — always read the license file in the weights repo.
3. **Cross-lingual claims are clean-studio evidence**; product references are 3-10 s BGM-bled drama/anime audio post-separation. Only the local benchmark decides.
4. **Hidden runtime downloads**: CosyVoice pulls campplus/speech-tokenizer assets; Confucius4 pulls facebook/w2v-bert-2.0 (MIT); Qwen3-TTS pulls tokenizer models. `HF_HUB_OFFLINE=1` hard-fails any missed asset — the bundled cache must enumerate the full model graph per candidate (this exact failure killed the only real dub run, 2026-06-14).
5. **Windows is undocumented for every Tier-A challenger except GPT-SoVITS**; CosyVoice pain is known and solved in-repo; Qwen3-TTS py3.12 and Confucius4 py3.10 may conflict with the py3.11 venv standard (UNVERIFIED).
6. **No permissive model has hard duration control** — subtitle fit rests on the translation-length + speed-param + capped time-stretch chain (see field-pipeline-patterns).

Benchmark spec (gate before adoption): candidates CosyVoice3, GPT-SoVITS v2ProPlus, Qwen3-TTS-1.7B/0.6B, VoxCPM2, Confucius4 (+ Step-Audio-EditX on >=12 GB), with Kokoro+OpenVoice as baseline to quantify the lift. Real operator content (JA and KO clips, multi-speaker, 3/5/10 s refs, clean vs BGM-bled vs post-separation). Metrics: speaker similarity (ECAPA/WavLM SECS ref-vs-EN-output), intelligibility (whisper large-v3 WER on output), naturalness (UTMOS/DNSMOS + operator A/B), short-ref degradation curve, speed/stretch artifact thresholds at 0.9-1.3x, per-segment RTF + batch throughput + VRAM peak on target GPU, CPU-only RTF (GPT-SoVITS insurance lane). Packaging gate per candidate: clean py3.11 venv from local wheelhouse (`pip --no-index`), pre-populated HF cache, `HF_HUB_OFFLINE=1`, network-blocked run to first audio; record every asset the model graph touches.

</topic>

<topic id="field-pipeline-patterns" wp="WP-0287" status="final" summary="How 12 OSS dubbing pipelines are actually built; transferable timing-fit and packaging techniques" updated_at="2026-08-01">

## Field evidence — how OSS dubbing pipelines are actually built (12 projects inspected)

Projects profiled with code-level inspection: VideoLingo (17.9k stars, Apache), pyvideotrans (18.5k, GPL-3.0, most active), KrillinAI/KlicStudio (10.6k, GPL-3.0, Go), SoniTranslate (Apache), Linly-Dubbing (Apache, dormant), YouDub-webui (5.2k, Apache), open-dubbing/Softcatala (Apache), Weeablind (no license), OmniVoice-Studio (9.4k, AGPL+commercial), dub-studio (Tauri 2 + Rust — closest architecture to VoxVulgi; fully-offline ~15 GB installer), tachidubb (MIT, MCP/agent-first), youtube-auto-dub (MIT).

Modal 2026 stack (dominance across the set): Whisper-family ASR (WhisperX for word timestamps, faster-whisper for speed; Parakeet/FunASR as second engines in the newest projects) -> **LLM translation via any OpenAI-compatible endpoint with context batching** (pure-MT survives only in fully-offline tiers) -> pyannote diarization (with its HF-gating friction driving the newest projects to token-free embedding clustering or Sortformer/CAM++ alternatives) -> Demucs or UVR/MDX-ONNX separation (default-on when cloning: vocal stem doubles as clone-reference source, instrumental stem is the mix bed) -> CosyVoice / VoxCPM2 / F5 / Higgs-class zero-shot clone TTS (GPT-SoVITS/XTTS are the 2024 generation) -> identical ffmpeg idiom (silence-padded concat placement -> `amix=inputs=2:duration=first` with the instrumental stem -> libx264/NVENC + AAC).

VoxVulgi already matches the modal stack shape and its pinned wheel-URL+sha256 lockfiles are stronger than anything observed in the field (whose #1 issue-tracker failure class is exactly model-download/dependency rot). The packaging winners ship prebuilt installers with models included — validating the batteries-included decision (dub-studio: portable zip + MSI, ~15 GB, fully offline).

Five transferable techniques (file-level evidence):

1. **Three-tier timing-fit with gap borrowing** (OmniVoice `backend/services/fit_planner.py`): need<=1.0 -> optional slow-fill to 0.85; need<=1.2 -> pure audio stretch ("imperceptible"); beyond -> geometric split audio_rate=min(sqrt(need),1.5) + per-segment video slow-down <=2.0; slot extends into following silence minus 0.05 s gap guard; residual becomes explicit `overflow_trimmed` status.
2. **LLM shorten-rewrite as the escape valve** when the speed cap is hit (KlicStudio `internal/service/dubbing/optimizer.go`: one-shot "rewrite to read naturally within X seconds" prompt, fall back to original on empty), plus the cheaper pre-emptive variant: character-budget-per-time-window at translation time (youtube-auto-dub `--target-cps 15`, measured over-compressed segments 86% -> 56%).
3. **Pre-TTS chunk planning** (VideoLingo `core/_8_2_dub_chunks.py`; hardened Go port in KlicStudio `dubbing/fit.go`): estimate translated-line duration, merge short adjacent subtitles (<=5) into dub chunks, allocate windows = duration + min(gap, tolerance); speed caps cluster tightly across all projects (accept ~1.2, hard max 1.3-1.5, slow-down floor 0.8-0.9).
4. **Clone-reference slicing from the separated vocal stem with a min-length fallback chain** (VideoLingo `core/_9_refer_audio.py`; YouDub `adapters/voxcpm.py`: 1200 ms floor -> per-speaker fallback -> global fallback, badcase retry x3 on duration-ratio >6) — reject bad references before synthesis, not after.
5. **Rubberband-first pitch-preserving stretch with atempo-chain fallback** (pyvideotrans `videotrans/task/_rate.py`): plain atempo above ~1.3 audibly degrades cloned speech (corroborated by Weeablind issue #35 — unlimited slow-down is also a defect); optional capped video `setpts` as the second lever.

Operational lessons from field issue trackers: verify ffmpeg presence explicitly; never rely on live HF downloads at runtime; CUDA-DLL check with a reporting (not crashing) CPU fallback; avoid gated pyannote as the only diarization path; preserve trailing video after the last dubbed segment.

</topic>

<topic id="recommended-stack" wp="WP-0287" status="final" summary="Recommended default pipeline, retirements, payload composition, and open operator decisions" updated_at="2026-08-01">

## Recommended default stack (decision package)

| Stage | Shipped today | Recommended default (bundle) | Runner-up / options | BYO lane |
|---|---|---|---|---|
| ASR | whisper.cpp large-v3-q5_0 (all langs) | Per-language routing: whisper.cpp large-v3 (KO + fallback) + **Qwen3-ASR-1.7B** (JA) + **Qwen3-ForcedAligner-0.6B** (word timestamps) | large-v3-turbo single-engine (zero new surface) | Dolphin, MOSS-Transcribe-Diarize |
| Translation | whisper.cpp translate mode (dead end) | **Embedded llama.cpp `llama-server` + Qwen3.5-35B-A3B-Instruct Q4 GGUF**, batch+context+glossary prompting; 9B/4B low-spec presets | Gemma 4 26B-A4B/E4B; boosters: Shisa-14B (JA), Kanana-8B / KORMo-10B (KO), Seed-X-7B | Any GGUF drop-in: TranslateGemma, MiLMMT-46, PLaMo-2, Tower+, Aya, EXAONE |
| Diarization | resemblyzer + sklearn (retire) | **sherpa-onnx: pyannote segmentation-3.0 (MIT) + CAM++ zh_en embeddings (Apache) + Silero VAD v6 pre-filter**; exact/auto/range preserved | pyannote.audio 4.0 + community-1 CC-BY-4.0 mirror (operator provenance decision) | Official gated pyannote community-1 (HF token); DiariZen (NC) |
| Separation | Spleeter (retire; deadlock history) | **Kim Mel-Band RoFormer (MIT weights) via python-audio-separator (MIT, single-process)** | **Bandit-v2 `multi`** cinematic 3-stem (Apache code / CC-BY-SA weights, JA in training set); TIGER-DnR lightweight | Demucs (weights science-only), UVR community models, SAM Audio |
| Voice cloning | Kokoro + OpenVoice V2 (retire cascade) | **Fun-CosyVoice3-0.5B** direct cross-lingual zero-shot cloning | **GPT-SoVITS v2ProPlus** (MIT/MIT, Windows-packaged, CPU lane); challengers Qwen3-TTS / VoxCPM2 / Confucius4; heavy packs Step-Audio-EditX / MOSS-TTSD | XTTS-v2 (idiap fork), OpenAudio S1-mini, Seed-VC, F5-TTS |
| Mix/mux | ffmpeg (keep) | Keep; adopt field timing-fit chain: pre-TTS chunk planning -> gap borrowing -> LLM shorten-on-overflow -> TTS speed param -> rubberband-first stretch (atempo fallback) -> explicit overflow status | — | — |

**Retirements from the default path**: whisper-translate mode, resemblyzer, Spleeter, the OpenVoice V2 conversion cascade. Kokoro survives only as a fixed-voice non-clone fallback.

**Payload composition (batteries-included, size not a constraint)**: whisper large-v3 (~3 GB full or q5 ~1 GB) + Qwen3-ASR-1.7B (~4 GB) + ForcedAligner (~1.5 GB) + Qwen3.5-35B-A3B Q4 GGUF (~20 GB) + small-preset GGUF (~5 GB) + diarization ONNX (<150 MB) + Mel-Band RoFormer ckpt (~700 MB class) + Bandit-v2 (~450 MB) + CosyVoice3 (~2-3 GB with asset graph) + GPT-SoVITS (~1 GB) + Python wheelhouses. Order-of-magnitude ~40 GB installer; per operator decision 2026-07-31 this is acceptable.

**Runtime consequences**: llama.cpp (GGML family, same competence as whisper.cpp) becomes a supervised child process; sherpa-onnx (or onnxruntime directly) enters the Rust engine; the Python surface SHRINKS to separation + cloning TTS (the two heaviest historical failure sources drop from five Python packs toward two), all installed from bundled wheels with hashed lockfiles per existing discipline.

**Operator decisions (RESOLVED 2026-08-01)**:

1. **Benchmark first, and record the other methods as backups.** Defaults are not frozen from research alone; WP-0288 runs the per-stage benchmark specs against real operator content and freezes the selection from measured evidence. Every non-selected candidate that passes the license filter is recorded as a durable ranked backup so a later regression, license change, or hardware constraint has a pre-vetted fallback instead of new research.
2. **Minimum-hardware contract: agreed as proposed.** Recorded normatively in PRODUCT_SPEC 8.1.9 — recommended tier is a consumer GPU (~8 GB VRAM class) for the full-quality stack; the CPU-only tier degrades to whisper.cpp + small GGUF translation preset + GPT-SoVITS CPU cloning + CPU separation, with the active tier detected and stated in-app rather than failing.
3. **Diarization runner-up provenance: the ungated CC-BY-4.0 community-1 mirror is accepted**, subject to one-time SHA verification against the official gated repo, a pinned revision, and in-app CC-BY attribution. It ships as the quality runner-up alongside the sherpa-onnx default.
4. **Closure unit folded in.** The never-closed core deliverable — one dubbed MP4 produced through the *installed* app — becomes the acceptance criterion of the first implementation WP (WP-0289) rather than a separate packet, because the revised stack replaces every component implicated in the historical failures.

</topic>

<topic id="risks-and-benchmark-plan" wp="WP-0287" status="final" summary="Cross-cutting risks and the local benchmark WP that must gate adoption" updated_at="2026-08-01">

## Cross-cutting risks and validation plan

1. **JA/KO evidence gap is systemic**: no stage has trustworthy public JA/KO-specific benchmarks except ASR. A local benchmark WP on the operator's real content (Haerin single-speaker, Miyeon multi-speaker chaotic, plus K-drama/anime/variety clips) must gate every adoption. Per-stage benchmark specs are recorded in each stage topic.
2. **License snapshots**: verdicts cite model-card revisions as of 2026-07-31; snapshot license files/tags into governance evidence at bundle time (especially Kim Mel-Band RoFormer's young MIT tag and Gemma 4's new Apache text).
3. **Attribution obligations**: CC-BY-4.0 components (pyannote segmentation, WeSpeaker, community-1, Bandit-v2 CC-BY-SA) require an in-app attribution surface.
4. **Hardware contract**: the quality-first stack (Qwen3-ASR GPU lane, LLM translation, clone TTS ~8 GB VRAM class) effectively wants a consumer GPU; CPU-only degradation paths exist per stage (whisper.cpp, small GGUF presets, CPU separation) but the minimum-hardware contract needs an operator decision.
5. **Adult-content refusals**: LLM translation must be tested against the operator's production scope; an uncensored model preset may be required.
6. **Migration risk**: stages 1-4 replace Python packs that have historically been the app's most fragile surface; the batteries-included wheel+weights payload (PRODUCT_SPEC 8.1.8) plus the existing hashed-lockfile discipline is the mitigation and must extend to every new pack.

</topic>
