# Work Packet: WP-0292 — Automatic Multi-Speaker Dubbing That Never Blocks (v1)

## Metadata
- ID: WP-0292
- Owner: assistant (implementation) + operator (acceptance on the Miyeon clip)
- Status: BACKLOG
- Created: 2026-08-05
- Depends on: WP-0290 (multi-speaker workspace surfaces the per-speaker state)
- Related: WP-0289 MT-02 (sherpa-onnx diarization), MT-09 (visible terminal state), WP-0291 (manual correction path)
- Target milestone: Localization core recovery

## Intent
- What: Make multi-speaker dubbing run automatically end to end, degrading **per speaker** instead of blocking the whole item, and telling the operator exactly which speaker needs what.
- Why: Operator request 2026-08-05 — "lets try an auto function". Today multi-speaker is all-or-nothing.

## The defect this fixes (verified in code 2026-08-05)

`jobs.rs::decide_localization_next_stage` (L2619-L2643) calls `missing_voice_plan_speakers`
(L2589-L2617). If **any** speaker on the translated track lacks a voice profile and is not routed
to `standard_tts`, it returns `VoicePlanBlocked`. The handler (L3026-L3089) then makes one automatic
attempt via `auto_apply_source_voice_references_for_missing_speakers`, and if any speaker is still
missing it returns with `queued_jobs: Vec::new()` and `voice_reference_blocked = true`.

**Consequence: one speaker with under 2.5 s of usable audio stops the dub for every other speaker in
the clip.** For a chaotic multi-speaker source — a game show, a variety clip, the Miyeon case — a
bit-part speaker with a two-second interjection is enough to block the entire deliverable. This is
the single largest reason multi-speaker "has never worked".

The reference threshold that decides this is `voice_reference_candidates.rs`:
`MIN_TOTAL_DURATION_MS = 2500`, `MIN_CLIP_DURATION_MS = 900`, `MAX_CLIPS_PER_SPEAKER = 4`.

## Scope
- In scope:
  - **Per-speaker degradation policy** replacing the item-wide block: each speaker independently
    resolves to `clone` (reference good), `standard_tts` (reference too short/poor — dub proceeds
    with a non-cloned voice), or `skip` (leave original audio for that speaker's segments).
    The item proceeds whenever at least one speaker is renderable.
  - **Truthful per-speaker labelling** end to end: the run-level outcome
    (`clone_preserved` / `partial_fallback` / `fallback_only` / `standard_tts_only`) must reflect
    the actual per-speaker mix, and the manifest must record why each speaker got its mode.
  - **Automatic reference selection quality gate**: prefer the separated dialog stem, exclude
    overlap-marked segments (WP-0291), reject clips below the min-length floor, and apply the
    field-standard fallback chain (per-speaker -> global -> reject) rather than accepting anything.
  - **Operator-visible speaker roster** in the Multi-speaker workspace: every speaker, its chosen
    mode, its reference seconds vs the 2.5 s floor, and the one action that would improve it.
  - **Visible terminal state** when nothing at all is renderable (closes WP-0289 MT-09 for the
    multi-speaker path): a terminal job/item state naming the blocking speaker and stage, never a
    silent stall with an empty queue.
  - Speaker-count intent (`exact` / `auto` / `range`) preserved exactly as today.
- Out of scope:
  - Replacing the diarization backend (WP-0289 MT-02) — this packet must work with resemblyzer today
    and improve automatically when sherpa-onnx lands.
  - The manual tagging UI (WP-0291).
  - Cloning-backend selection (WP-0288).

## Acceptance criteria
1. On a multi-speaker clip where one speaker has <2.5 s of usable audio, the dub **completes**:
   qualifying speakers are cloned, the short speaker is rendered via standard TTS or skipped per
   policy, and nothing silently stalls.
2. The Miyeon clip (`ab16785e-…`) either produces a dubbed MKV, or fails with a terminal state that
   names the blocking speaker and stage. A silent stall is a test failure.
3. The surfaced clone-status label matches the manifest, and the manifest records a per-speaker mode
   plus the reason for it.
4. No speaker is silently upgraded to "cloned" when it actually fell back — the historical
   `clone_preserved` claim must remain honest (the shipped renderer already refuses silent
   fallback; that property must survive).
5. Engine tests cover: all-speakers-good, one-speaker-short, all-speakers-short, zero speakers.
6. `cargo test` green for the touched boundary; no regression in the single-speaker path.

## Test / verification plan
- Focused engine tests on `missing_voice_plan_speakers` / the new per-speaker resolver, including the
  exact "one short speaker" case that blocks today.
- Real-content run on the Miyeon clip with the roster inspected in the Multi-speaker workspace.
- Regression: the Haerin single-speaker path must still produce `clone_preserved` 1/1.

## Risks / open questions
- **Quality vs completion tension**: auto-fallback to standard TTS trades voice fidelity for a
  finished deliverable. Mitigation: the mode is per speaker, visible, and overridable; the operator
  can always re-run a speaker as clone after adding references via WP-0291.
- **Mixed-voice output can sound worse than no output** if half the cast is cloned and half is
  generic. Mitigation: make the roster show the mix before the run, not after.
- **Threshold is a guess**: 2.5 s is the current constant and has never been validated against clone
  quality. Mitigation: WP-0288's short-reference degradation curve should set this number with
  evidence; until then treat 2.5 s as provisional and make it configurable.
- Open: default policy for a speaker that cannot be cloned — standard TTS, or leave original audio?
  Leaving the original is more honest for a bit-part, but mixes languages in one track. Proposed
  default: standard TTS, with "leave original" available per speaker. Operator decision wanted.

## Added by review 2026-08-05
- **Mount point**: this packet's roster fills the empty `data-stage="dub"` stage created by WP-0290.
  The stage's own shipped hint text already says a dedicated Dub surface "is planned for a follow-up
  WP" — this is that WP.
- **Reuse `failureStates.ts`, do not invent terminal-state prose** (G12). WP-0264 already ships a
  plain-language STATE + REQUIRED ACTION classifier with tone chips, consumed by `JobsPage.tsx` L17
  and `LibraryPage.tsx` L28 but **not** by `SubtitleEditorPage.tsx`. Extend `FailureKind` with
  localization kinds so a failed dub reads identically in Jobs, Library and Studio. This also
  satisfies WP-0289 MT-09 with consistent wording.
- **Extend the existing per-speaker render-mode control, do not add a second one** (review):
  `setSpeakerRenderMode` (L4408) with `{value:"standard_tts", label:"Standard TTS fallback"}` (L817)
  already exists inside Reusable Voice Basics. Two places to set the same field is a truth split.
- **Resume after a mid-run failure is missing** (G15). This packet handles "one speaker is short" but
  not "the run died at segment 40 of 120". At CPU RTF ~10 that difference is hours of lost work.
  Per-segment re-render (WP-0293) is the right primitive; this packet must state whether a partial
  dub resumes or restarts.
- **Provenance must be visible, not just recorded** (G21). AC3 requires the manifest to carry
  per-speaker mode and reason; add a requirement that the Dub surface *shows* which backends produced
  the output (`variant_label`, `tts_backend_id` already exist as pipeline options, jobs.rs L2203-2204).
  Without it the operator cannot explain why item 12 sounds worse than item 11 while WP-0288 is still
  swapping backends.
- **Time expectation before a long run** (G4): CosyVoice measured RTF 9.4-12.1 on the CPU tier, so a
  3-minute clip is ~30 minutes of synthesis. State an ETA before starting or a non-technical user
  concludes it hung.
- **AC4 needs a citation.** It asserts "the shipped renderer already refuses silent fallback" —
  UNVERIFIED as drafted. Cite the file/line that enforces it, or demote AC4 to a requirement to add
  that enforcement.
- **Blocking decision**: the standard-TTS-vs-leave-original default (below) sits inside the
  acceptance path; AC1 is not testable until the operator decides. Resolve before ACTIVE.

## Status updates
- 2026-08-05: WP created from operator request ("lets try an auto function") after verifying the
  all-or-nothing gate in `jobs.rs` L2589-L2643 and L3026-L3089.
- 2026-08-05: Review confirmed the defect claim line-for-line and rated this the strongest of the
  five packets. Added mount point, failure-state reuse, resume gap, visible provenance, ETA, and the
  AC4 citation requirement.
