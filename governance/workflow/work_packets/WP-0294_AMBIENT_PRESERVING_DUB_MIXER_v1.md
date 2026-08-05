# Work Packet: WP-0294 — Ambient-Preserving Dub Mixer (v1)

## Metadata
- ID: WP-0294
- Owner: assistant (implementation) + operator (listening acceptance)
- Status: BACKLOG
- Created: 2026-08-05
- Depends on: WP-0290 (mixer mounts in both workspaces)
- Related: WP-0287 separation topic (3-stem candidates), WP-0288 (separation benchmark freezes the backend), WP-0289 MT-03 (separation backend swap), WP-0032 (single-pass mixer, DONE)
- Target milestone: Localization core recovery

## Intent
- What: Keep the **ambience** of the source — audience noise, game-show stingers, room tone, sound effects, music beds — under the cloned dub, and give the operator real mixing control over the balance.
- Why: Operator request 2026-08-05 — "the haerin sample removed almost all background noise, i wanted to have natural speaker voice clone and remain the ambient audio, so sound fx from game shows still come through etc."

## Grounding evidence (measured 2026-08-05, Haerin clip)

| Signal | Mean level | Speaker similarity (CAM++ SECS vs reference) |
|---|---|---|
| Original audio | −28.6 dB | 0.9529 |
| Spleeter `vocals.wav` | **−28.5 dB** | 0.9351 |
| Spleeter `accompaniment.wav` | **−52.9 dB** | 0.1725 |

Spleeter put **essentially all energy into the vocals stem**; the background stem came out ~24 dB
below the source, i.e. near-silent. When that stem is used as the bed under the dub, the result is a
cloned voice over near-silence — exactly what the operator heard.

**Root cause: Spleeter `2stems` is a music-separation model** (vocals vs accompaniment), trained to
split singing from instruments. It has no concept of dialog vs effects, so on speech-led content it
sweeps speech *and* ambience into "vocals" and leaves an empty bed. This is a modelling mismatch, not
a mixing bug — no gain setting can recover ambience that the separator already discarded.

This independently corroborates the WP-0287 recommendation to retire Spleeter, and specifically
justifies bundling a **dialog/music/effects 3-stem** model rather than only a 2-stem vocal splitter.

## Current mixer state (corrected 2026-08-05 after review)

> The first draft claimed *"every call site passes `None`"* for `ducking_strength`. **That was
> wrong.** The corrected finding is sharper and worse:

- `ducking_strength` **is** threaded through `enqueue_mix_dub_preview_v1_with_options`
  (`jobs.rs` L2232-2252), which is reachable **only** from the manual "Mix dub" button
  (`SubtitleEditorPage.tsx` L3816-3829, which sends `duckingStrength`, `loudnessTargetLufs`,
  `timingFitEnabled/Min/Max`).
- **Every *automatic pipeline* enqueue passes `None`** — `jobs.rs` L8101, L8334, L11977, L12278,
  L12366, **L12778**, L21412 — and drops `timing_fit_enabled/min/max` and `loudness_target_lufs`
  too, falling back to `unwrap_or(0.6)` at L13830.
- **Consequence: the operator's mix settings apply only if they click the manual Mix button, and are
  silently discarded on the one-button run.** This is the same latent pattern found and fixed for
  `keep_original_audio` in WP-0289 MT-11, and it also blocks WP-0293 AC3.
- The Studio exposes exactly **one** mix control, `mixDuckingStrength` (L7427) — and it sits under
  "Advanced audio/video" **inside the Captions card** (L7410-7450), not under Mix. There is no
  ambience/background level control at all.
- The separation backend selector currently offers only `demucs` / `spleeter` (L3801-3807); adding a
  third option is UI work this packet must own.
- `mix_background_audio_source` (L8017) uses the separated background when present and otherwise
  falls back to source audio, so the plumbing for a bed exists; the bed **content** is the problem.

> **Split 2026-08-05 after review.** This packet originally bundled two different risk classes:
> mixer controls (buildable today) and 3-stem model adoption (gated on WP-0288's benchmark **and** an
> installer-payload change with a CC-BY-SA attribution obligation). The 3-stem bed has moved to
> **WP-0295**. This packet is now the buildable half, and its acceptance no longer depends on a model
> nobody has selected or on a test clip that does not exist.

## Scope
- In scope:
  - **Thread mix params through the automatic pipeline** (the prerequisite above) so operator
    settings survive the one-button run. Also unblocks WP-0293 AC3.
  - **Operator mixer controls**, with sane defaults and no jargon:
    - dub voice level
    - ambience / effects level (the control that is missing today)
    - music level (when the 3-stem model separates it)
    - ducking amount and release, wired to the existing `ducking_strength` param
    - optional keep-a-little-original-dialog blend for naturalness
  - **Wire the existing param through**: stop passing `None` at every call site so operator settings
    actually reach the mix job.
  - **Preview before commit**: audition the mix on a selected segment without re-running the whole
    dub.
  - **Truthful bed reporting**: state which stem source the mix actually used (3-stem dialog bed,
    2-stem accompaniment, or source-audio fallback) and warn when the bed is near-silent — a bed more
    than ~20 dB below source is a strong signal the separator failed, and the operator should be told
    rather than shipped silence.
  - Loudness target retained (`loudnorm I=-16:TP=-1.5:LRA=11` class) so output levels stay consistent.
  - **Relocate the existing mix controls** out of "Advanced audio/video" inside the Captions card
    (L7410-7450) into the Mix stage where they belong.
- Out of scope:
  - **3-stem model adoption — moved to WP-0295.**
  - Choosing the separation winner (WP-0288 benchmark decides).
  - Multi-track export mechanics (already done in WP-0289 MT-11).
  - A general-purpose multi-band audio editor.

## Acceptance criteria
1. Mix settings chosen by the operator reach the **automatic** pipeline run, verified in the job
   params — not only the manual "Mix dub" button.
2. The ambience level is operator-controllable and the setting reaches the mix job.
3. The mix report states which bed source was used (3-stem dialog bed / 2-stem accompaniment /
   source-audio fallback), and **flags a near-silent bed** — more than ~20 dB below source — instead
   of silently shipping silence.
4. The measured bed level is **displayed** as a level meter with the dB value and a red zone, not
   merely warned about in a log (review G20: the measurement is the trust artifact).
5. Ducking makes the dub intelligible over a loud bed without pumping artifacts.
6. Segment-level mix preview works without a full dub re-run.
7. Mix controls appear under the Mix stage, not inside the Captions card.
8. `cargo test` green on the touched boundary; `tsc --noEmit` exit 0.

## Test / verification plan
- Measured: bed level (mean dB) and bed speaker-similarity (SECS) before/after the separator change,
  using the same method as the 2026-08-05 baseline so the numbers are comparable.
- Real-content: a clip with strong ambience (crowd/SFX) in addition to the Haerin and Miyeon corpus —
  **this clip does not exist yet and is an operator input**, and it overlaps the WP-0288 corpus gap.
- Listening acceptance by the operator; this is the deciding test for "natural" and cannot be settled
  by metrics.

## Risks / open questions
- **Dialog bleed into the bed**: a 3-stem model that leaks the original Korean dialogue into the
  effects stem reintroduces the original language under the dub. Mitigation: measure bed SECS against
  the speaker reference (the 2026-08-05 method) and gate on it — a high-similarity bed means leakage.
- **CPU cost**: RoFormer/Bandit-class models are heavier than Spleeter. The machine's RTX 3090 is
  currently unused (`torch 2.3.1+cpu`); GPU enablement (WP-0289 MT-08) matters here too.
- **Payload growth**: adding a 3-stem model to the batteries-included installer. Accepted per
  PRODUCT_SPEC 8.1.8 (size is explicitly not a constraint), but the weights licence must be
  snapshotted at bundle time (Bandit-v2 is CC-BY-SA-4.0 and carries attribution obligations).
- **Too many knobs for a non-technical persona**: contradicts PRODUCT_SPEC 3. Mitigation: one
  plain-language preset selector ("keep the room sound" / "voice forward" / "custom") with the raw
  sliders behind Advanced disclosure, per WP-0260.
- Open: should the default preset preserve ambience aggressively or favour dialogue clarity? Operator
  decision; the request implies ambience-forward as the default.

## Status updates
- 2026-08-05: WP created from operator request ("perhaps also an audio mixer of some sort ... remain
  the ambient audio, so sound fx from game shows still come through"), grounded in measured evidence
  that Spleeter's background stem came out 24 dB below source on the Haerin clip.
- 2026-08-05: **Corrected and split after review.** Fixed the false "every call site passes `None`"
  claim (the real defect is that only the *manual* Mix button threads params; every automatic enqueue
  discards them, jobs.rs L8101/L8334/L11977/L12278/L12366/L12778/L21412 -> `unwrap_or(0.6)` at
  L13830). Moved 3-stem adoption to WP-0295 so this packet no longer depends on an unselected model
  or a test clip that does not exist. Added the bed level meter (G20) and the control-relocation
  requirement.
