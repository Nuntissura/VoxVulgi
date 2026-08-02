# Work Packet: WP-0287 — Localization Stack Revision Research and Landscape Refresh (v1)

## Metadata
- ID: WP-0287
- Owner: operator + assistant (research); operator (stack decision)
- Status: DONE (governance-only; proof bundle at `product/desktop/build_target/tool_artifacts/wp_runs/WP-0287/20260801_research_complete/summary.md`)
- Created: 2026-07-31
- Target milestone: Localization core recovery (pre-implementation research gate)

## Intent
- What: Refresh the 2026-02 voice/dubbing tooling landscape (`governance/spec/VOICE_DUBBING_TOOLING_LANDSCAPE_2026.md`, WP-0020) against the mid-2026 field, stage by stage, under the new batteries-included installer constraint, and produce a recommended (possibly revised) localization pipeline stack for operator decision.
- Why: Operator decision 2026-07-31 (PRODUCT_SPEC 8.1.8): the public installer must bundle ALL models and dependencies, first run must work fully offline, and the audience is non-technical language students / culture enthusiasts. The existing landscape doc predates the WP-0252 quality findings (Kokoro+OpenVoice judged quality-fatal for the core promise) and predates five months of field movement. Repo policy requires research-first before any stack/pipeline revision. Installer size is explicitly NOT a constraint (operator decision 2026-07-31).

## Operator constraints (binding filters for every candidate)
1. Local-first, runs on Windows desktop (macOS later); no cloud/service backends for the default path.
2. Bundleable: code AND weights must be redistributable (Apache-2.0 / MIT / BSD / CC-BY-commercial-ok). Gated, non-commercial, or unclear-license weights are BYO-only and cannot be the shipped default.
3. Batteries-included: the candidate must be installable from bundled wheels + bundled weights with zero first-run network access (`pip --no-index`, populated HF cache, `HF_HUB_OFFLINE=1` safe).
4. Non-technical users: no manual model placement, tokens, or terminal steps for the default path.
5. Size is not a constraint; quality wins over payload size.
6. Hardware honesty: record VRAM/CPU requirements per candidate; the default pipeline must state a minimum-hardware contract and what degrades on CPU-only machines.
7. Repo guardrails: no watermarking/anti-abuse-instrumented backends in the default path (existing guardrail; e.g. Chatterbox PerTh watermark was classed Avoid in the 2026-02 doc).
8. Language focus: JA/KO -> EN quality is the primary quality axis.

## Scope
- In scope:
  - Stage-by-stage 2026-07 field refresh with primary sources:
    1. ASR for JA/KO (whisper.cpp large-v3 baseline vs current field),
    2. Subtitle translation JA/KO -> EN (current whisper-translate path vs dedicated local MT / local LLM translation),
    3. Speaker diarization (resemblyzer baseline vs current field),
    4. Source separation (Spleeter baseline vs Demucs and current field),
    5. Voice-preserving TTS / voice conversion (Kokoro+OpenVoice V2 default and CosyVoice2 candidate vs current field),
    6. Field implementations of complete OSS dubbing pipelines (how comparable projects wire these stages in real code).
  - License verification (code license AND weights license separately) for every candidate.
  - Packaging verdict per candidate: Bundle / BYO / Avoid under the batteries-included constraint.
  - A recommended default pipeline + runner-up per stage, with explicit rejected options and reasons.
  - Refreshed landscape document under `governance/spec/` superseding the 2026-02 doc.
- Out of scope:
  - Any implementation, dependency changes, venv changes, or builds (separate WPs after operator decision).
  - Benchmarking runs on local hardware (follow-up WP once candidates are selected).
  - Cloud/service backends.

## Acceptance criteria
- A refreshed landscape doc exists in `governance/spec/` with: sources checked, per-stage candidate tables (licenses, hardware, packaging verdict), recommended stack, runner-ups, rejected options with reasons, risks, mitigations, and validation plan — explicit enough for a no-context model to understand why the chosen approach is field-aligned (GLOBAL-RESEARCH-059).
- The 2026-02 landscape doc is marked superseded and points to the refresh.
- Task board row for WP-0287 exists and reflects status.
- The recommended stack is presented to the operator as a decision package; no implementation starts inside this WP.

## Implementation notes
- Research executed via parallel web-research agents (GitHub, Hugging Face, papers, issue trackers, release notes, community reports), synthesized by the session assistant; all load-bearing claims must carry primary-source URLs.
- Claims that cannot be verified online during the session must be labeled UNVERIFIED rather than asserted.

## Test / verification plan
- Verification for this WP is documentary: the refreshed doc satisfies the acceptance criteria above and every recommended candidate has verified license + packaging evidence linked.
- Runtime validation of the selected stack belongs to the follow-up implementation WP(s).

## Risks / open questions
- Model licenses change between releases; verdicts must cite the exact model-card revision consulted.
- JA/KO -> EN quality claims from the field may not transfer to the operator's real content; final selection needs a local benchmark WP before full adoption.
- CPU-only users: the quality-first stack may effectively require a GPU; the minimum-hardware contract needs an operator decision.
- Some strong candidates may have redistributable code but gated/NC weights; they can only enter as BYO lanes, not defaults.

## Status updates
- 2026-07-31: WP created; spec decisions recorded in PRODUCT_SPEC 1/3/8.1.8 and TECHNICAL_DESIGN 2.1; research fan-out started (6 lanes).
- 2026-08-01: 5 of 6 lanes complete and synthesized into `governance/spec/LOCALIZATION_STACK_LANDSCAPE_2026_07.md` (ASR, translation, diarization, separation, field pipelines all final; voice-cloning TTS topic and recommended-stack topic marked draft pending the final lane report). Old 2026-02 landscape doc marked SUPERSEDED with pointer. Operator size-not-a-constraint decision added to PRODUCT_SPEC 8.1.8.
- 2026-08-01: All 6 lanes complete. Voice-cloning topic finalized (architecture verdict: direct cross-lingual zero-shot TTS, retire TTS+VC cascade; default Fun-CosyVoice3-0.5B, runner-up GPT-SoVITS v2ProPlus; Chatterbox/VibeVoice/NeuTTS excluded on watermark guardrail). Recommended-stack decision package finalized in the landscape doc with retirements, payload composition, runtime consequences, and 4 open operator decisions. Status -> REVIEW; no implementation performed (per WP scope).
- 2026-08-01: Operator resolved all 4 decisions — (1) benchmark first and record other methods as backups, (2) minimum-hardware contract agreed as proposed, (3) ungated CC-BY-4.0 pyannote community-1 mirror accepted, (4) closure unit folded into the first implementation WP. Recorded: PRODUCT_SPEC 8.1.9 (minimum-hardware contract and degradation tiers); landscape recommended-stack decisions marked RESOLVED; diarization runner-up marked ACCEPTED with SHA/pin/attribution conditions. Follow-ups cut: WP-0288 (benchmark + backup registry) and WP-0289 (revised-stack vertical slice with folded closure unit) plus WP-0289 refinement incl. 10-item red team. Proof bundle written; status -> DONE.
