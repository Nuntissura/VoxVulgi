# Work Packet: WP-0183 - CosyVoice 2 Backend Integration

## Metadata
- ID: WP-0183
- Owner: Codex
- Status: DONE
- Created: 2026-04-08
- Target milestone: Voice Cloning Quality

## Intent

- What: Integrate CosyVoice 2 as a managed voice cloning backend that replaces the two-stage Kokoro+OpenVoice pipeline with single-pass zero-shot cloned TTS.
- Why: The current two-stage pipeline (Kokoro TTS → OpenVoice V2 tone color swap) loses prosody, emotion, and accent. CosyVoice 2 does TTS + voice cloning in one pass with native JA/KO/EN cross-lingual support. Apache 2.0 license.

## Scope

In scope:
- Add CosyVoice 2 Python package as a managed dependency (pip install).
- Create a voice backend adapter that accepts text + reference WAV and returns cloned speech WAV.
- Wire into the existing voice-preserving pipeline as an alternative to OpenVoice V2 + Kokoro.
- Add a starter recipe in Diagnostics for CosyVoice 2.
- Register as a selectable backend in the voice backend catalog.
- Benchmark comparison against the existing Kokoro + OpenVoice V2 pipeline.

Out of scope:
- Replacing the default managed backend without benchmark evidence. WP-0252 later supplied that evidence and authorizes a readiness-sensitive CosyVoice 2 default with OpenVoice V2 + Kokoro fallback.
- CosyVoice 3 (evaluate v2 first, upgrade later).
- Training custom models.

## Acceptance criteria
- Operator can select CosyVoice 2 as the voice backend for an item.
- Voice-preserving dub produces output using CosyVoice 2 zero-shot cloning.
- Benchmark report can compare CosyVoice 2 vs OpenVoice V2 + Kokoro.
- `cargo check` + `npm run build` pass.

## Research notes
- HuggingFace: FunAudioLLM/CosyVoice2-0.5B
- GitHub: github.com/FunAudioLLM/CosyVoice
- License: Apache 2.0
- Languages: JA/KO/EN/ZH + 6 more
- Model size: 0.5B params (~1-2 GB weights)

### 2026-08-14 implementation research basis

- Sources inspected: official `FunAudioLLM/CosyVoice` source, official `FunAudioLLM/CosyVoice2-0.5B` model card, ModelScope SDK `snapshot_download` API/source, and the installed `wetext==0.0.4` source used by the managed venv.
- Relevant field pattern: CosyVoice 2 performs zero-shot cross-lingual synthesis through `CosyVoice2(...).inference_cross_lingual(text, reference_wav, stream=False)`; the wetext frontend loads four TN FSTs after calling `modelscope.snapshot_download("pengzhendong/wetext")` during normalizer construction.
- Reuse selected: keep VoxVulgi's existing managed dub job, reference WAV contract, report schema, separation/mix/mux follow-ups, backend catalog, benchmark store, and offline bundle hydrator.
- Selected approach: bundle the exact wetext TN graph under the app-local CosyVoice backend, intercept only the known wetext ModelScope lookup before importing CosyVoice, and fail closed on every unexpected lookup. Readiness requires non-empty venv, model, wrapper, Matcha, and wetext files.
- Rejected: relying on `%USERPROFILE%/.cache/modelscope` (outside the app root and previously observed truncated), permitting first-run runtime downloads, or patching site-packages in place.
- Risks and mitigations: backend lineage drift is prevented by stamping the resolved backend into queued and runtime pipelines; OpenVoice and CosyVoice manifests use separate directories; unsupported explicit backend IDs fail closed; incomplete/zero-byte wetext assets fail readiness and wrapper startup.
- Validation plan: focused Rust selection/readiness/benchmark tests, frontend contracts, `cargo check`, `npm run build`, dead-proxy offline CosyVoice warmup/render, governed installer build, and hidden headless UI selection/audit proof.

## Completion notes

- Completed 2026-08-14 in governed desktop build v0.1.148.
- Proof bundle: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0183/20260814_204530Z/summary.md`.
- Exact headless app proof used a real item with subtitle tracks and exposed an enabled `Preferred managed voice backend` selector with both managed backends ready, CosyVoice 2 selected, and OpenVoice V2 rendered as fallback.
- The same proof run found and fixed a circular item-bootstrap effect dependency; final navigation produced one `item_outputs` and one `jobs_list_for_item` completion instead of hundreds.
- Benchmark evidence: CosyVoice 2 `0.7569` SECS versus OpenVoice V2 + Kokoro `0.4634` on the operator-content cross-lingual comparison (`+63.3%`).
