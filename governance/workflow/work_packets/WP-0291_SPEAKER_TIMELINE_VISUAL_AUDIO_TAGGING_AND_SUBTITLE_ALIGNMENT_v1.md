# Work Packet: WP-0291 — Speaker Timeline: Visual + Audio Tagging and Subtitle Alignment (v1)

## Metadata
- ID: WP-0291
- Owner: assistant (implementation) + operator (tagging acceptance)
- Status: BACKLOG
- Created: 2026-08-05
- Depends on: WP-0290 (mounts in the Multi-speaker workspace)
- Blocks: nothing hard; materially de-risks WP-0292 (a human-corrected speaker map is the best input an auto-dub can get)
- Target milestone: Localization core recovery

> **Corrected 2026-08-05 after review.** The first draft claimed the existing speaker tools were
> "text-entry only". **That was wrong**: assignment is already a `<select>` populated from
> `speakersInTrack` (L10729-10740, text input only for the `__new__` case) and merge is two
> `<select>`s (L10773-10795). The original acceptance criterion ("label speakers without typing a
> speaker key") would therefore have passed on shipped code. Scope re-grounded below on what is
> genuinely missing: **hearing the audio**, editing boundaries, and reference-budget truth.

## Intent
- What: A waveform + segment timeline where the operator can **see and hear** who is speaking, correct speaker labels directly on the audio, and fix subtitle timing against the waveform.
- Why: Operator request 2026-08-05 — "we need tools to manually tag speakers, so both subs and the voice cloning works better". What is actually missing today is not the dropdown but the **evidence**: you cannot hear a segment before labelling it, you cannot see where speaker boundaries fall, and you cannot tell whether a speaker has enough clean audio to be cloneable. Diarization on chaotic audio is the weakest link in the pipeline (resemblyzer, 2019 GE2E), so a manual correction surface is the highest-leverage quality lever available before any model change lands.

## Blocking prerequisite found by review (G2)
`playSegmentAudio` (L1329-1344) plays `outputs.mix_dub_preview_v1_wav_path` — the **dub mix**, not the
original audio (its own tooltip at L10998 says "Play dubbed audio for this segment"). So there is
currently **no way to hear the original speaker for a segment**, which is exactly what labelling
requires, and it must be true *before* a dub exists. Adding original-audio segment playback is a
prerequisite of this packet, not a nice-to-have.

## Why this improves cloning specifically
Clone reference clips are cut from subtitle-aligned, speaker-labelled segments (`voice_reference_candidates.rs`). A mislabelled or overlapping segment poisons a speaker's reference, and the reference is what the clone is built from. Correcting labels at the audio level therefore improves clone similarity directly, independent of which TTS backend is selected by WP-0288.

## Scope
- In scope:
  - Waveform rendering of the item audio (prefer the separated dialog/vocals stem when present, else source), with the subtitle segments overlaid as blocks on a time axis.
  - Per-speaker colour lanes; unlabelled/overlap segments visually distinct.
  - **Click a segment to play just that segment**; play a speaker's whole reference selection; loop a segment.
  - Assign a segment (or a multi-select of segments) to a speaker from the timeline; create a new speaker; rename a speaker; merge two speakers — all without typing a speaker key.
  - Split and merge segment boundaries, and nudge segment start/end against the waveform, writing through the existing subtitle versioning (`subtitle_tracks::save_new_version`, which already writes a new version rather than editing in place).
  - Mark a segment as **overlap / exclude from clone reference** so bad audio never reaches the reference builder.
  - Show, per speaker, the reference budget actually available against the real thresholds in `voice_reference_candidates.rs`: `MIN_TOTAL_DURATION_MS = 2500`, `MIN_CLIP_DURATION_MS = 900`, `MAX_CLIP_DURATION_MS = 6500`, `TARGET_DURATION_MS = 8000`, `MAX_CLIPS_PER_SPEAKER = 4` — with a clear "this speaker cannot be cloned yet, needs N more seconds" state.
- Out of scope:
  - Changing the diarization backend (WP-0289 MT-02).
  - Changing the auto-dub gating behavior (WP-0292).
  - Waveform editing of the audio itself (that is the mixer, WP-0294).

## Acceptance criteria
1. The operator can label every speaker in a multi-speaker clip **by listening to it** — select a
   segment, hear the original speaker, assign — and the labels persist to the subtitle track and
   `item_speaker`. (The reference clip used during development is the operator's Miyeon item,
   `ab16785e-0fc4-4eba-9363-db81727a31db`, 174.7 s; note this id is runtime APPDATA state and is
   **not resolvable from the repo**, so any no-context model must substitute its own multi-speaker
   item.)
2. Selecting a segment plays exactly that segment of the **original** audio (not the dub mix), and
   this works before any dub exists.
3. Per-speaker reference budget is displayed against the real constants above, and a speaker below `MIN_TOTAL_DURATION_MS` is visibly flagged as not-yet-cloneable with the shortfall stated in seconds.
4. Segments marked as overlap are excluded from that speaker's clone reference bundle.
5. Subtitle timing edits made on the timeline round-trip through subtitle versioning without destroying the prior version.
6. No operator media is modified; all edits produce new derived artifacts.
7. `tsc --noEmit` exit 0; contract tests cover reference-budget maths and overlap exclusion.

## Implementation notes
- Waveform: prefer a precomputed peaks file over decoding audio in the WebView; generate peaks once per item into `derived/items/<id>/waveform/` via the bundled ffmpeg and cache it. Do not add a new heavy frontend dependency without checking `build_rules.md` and the no-new-cards rule.
- Playback: reuse the existing in-app audio preview surface from WP-0034 rather than introducing a second player.
- Reference budget must read the constants from a single shared source; do not duplicate the numbers in the frontend where they can drift from `voice_reference_candidates.rs`.

## Test / verification plan
- Focused tests: reference-budget calculation per speaker, overlap exclusion, subtitle version round-trip.
- App-boundary: headless bridge audit of the timeline, snapshot inspected for readable lanes, no overlapping text, visible speaker state (`build_rules.md` GUI rules).
- Real-content check: run against the Miyeon clip and record how many speakers diarization proposed vs how many the operator ended up with — that delta is direct evidence for WP-0289 MT-02.

## Risks / open questions
- **Performance on long items**: a 175 s clip is fine; a 60-minute item with thousands of segments is not. Mitigation: virtualize the segment list and render the waveform from cached peaks at a fixed resolution; bound what is drawn.
- **Destructive edits**: timeline editing touches the operator's subtitle work. Mitigation: every write goes through existing versioning; never edit a track in place.
- **Scope creep into a full DAW**: the goal is labelling and alignment, not audio editing. The out-of-scope list is binding.
- **Re-diarize vs manual labels is UNSPECIFIED and is the biggest lost-work risk in this packet**
  (review G13). Diarization writes `diarization{suffix}.json` while manual speaker edits go through
  `subtitles_save_new_version` (`lib.rs` L8864-8869 -> `subtitle_tracks.rs` L118), which forks a
  **new track version**. Whether a re-run silently discards hand-labelling is UNVERIFIED and must be
  answered before implementation. Required behaviour: an explicit "keep my labels / re-detect anyway"
  choice; never a silent overwrite.
- Operator-visible **version history with revert** is required (review G14): versioning the user
  cannot see or roll back is not recovery, it is invisible disk growth.

## Resolved by review (was an open question)
**Speaker identity IS reusable across items and the backend already exists** — do not defer this.
`voice_templates_create_from_item` / `_apply_to_item` (TSX L4487, L4642),
`voice_cast_packs_create_from_template` / `_apply_to_item` (L4759, L4826), voice-plan default
promotion (L4677, L4861), and `cast_pack_id` already rides the batch request (L5333). For the
high-volume case (50 videos of one channel) re-labelling per item is the single most unbearable cost,
and the plumbing is built. **Save-cast / apply-cast is in scope for this packet.**

## Status updates
- 2026-08-05: WP created from operator request ("create the tools to visually and audio align and tag speakers and subtitles").
- 2026-08-05: **Corrected after review.** Removed the false "text-entry only" premise; added the
  original-audio playback prerequisite (G2); un-deferred cast reuse (G7); added the re-diarize
  lost-work question (G13) and version history (G14); marked the Miyeon id as runtime-only state.
