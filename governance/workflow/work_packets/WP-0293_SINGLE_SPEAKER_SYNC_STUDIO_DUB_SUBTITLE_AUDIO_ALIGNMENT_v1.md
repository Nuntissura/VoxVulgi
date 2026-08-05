# Work Packet: WP-0293 — Single-Speaker Sync Studio: Dub / Subtitle / Audio Alignment (v1)

## Metadata
- ID: WP-0293
- Owner: assistant (implementation) + operator (listening acceptance)
- Status: BACKLOG
- Created: 2026-08-05
- Depends on: WP-0290 (mounts in the Single-speaker workspace)
- Related: WP-0289 MT-05 (timing-fit chain), WP-0035 (dub timing-fit tools, DONE), WP-0034 (in-app A/B preview, DONE)
- Target milestone: Localization core recovery

## Intent
- What: A focused surface for the common case — one speaker — where the operator can align the cloned dub audio, the subtitles, and the original audio against each other, and fix the places where they drift.
- Why: Operator request 2026-08-05 — "i also want an audio/voiceclone/sub sync tools for the single speaker". The dub is generated per segment and placed at the subtitle's `start_ms`; when the synthesized line is longer or shorter than the original delivery, lip/beat sync drifts and nothing in the current UI shows that drift or lets the operator correct it segment by segment.

## Grounding evidence (2026-08-05)
On the Haerin clip, segment 0 spans 0-3720 ms in the subtitle track, while the synthesized clone came
out **3.37 s** (Kokoro+OpenVoice) and **2.64 s** (CosyVoice2) for the same line. That is a 350-1080 ms
gap between the subtitle slot and the actual dub audio, per segment, with no surface that shows it.
Segment 1 (3720-6320 ms) had an **empty** translated string, so it produced no audio at all and left a
silent hole — again with nothing in the UI indicating why.

## Scope
- In scope:
  - A per-segment sync view: original audio waveform, the generated dub audio, and the subtitle slot
    on one shared time axis, with the **delta between subtitle slot length and dub audio length**
    shown numerically and visually per segment.
  - Segment-level actions: nudge start/end, re-time the subtitle to the audio, re-render just this
    segment, and adjust speed within the safe band.
  - A/B playback: original vs dub for the selected segment, and dub-in-context with the background
    bed, reusing the WP-0034 preview surface.
  - Explicit statuses per segment: `fits`, `stretched`, `compressed`, `overflow_trimmed`,
    `empty_text` (the segment-1 case above), `not_rendered`.
  - Surface the existing timing-fit controls (WP-0035) here instead of leaving them buried, and
    respect the field-standard caps recorded in the landscape doc: accept ~1.2x, hard max 1.3-1.5x,
    slow-down floor 0.8-0.9x, rubberband-first stretch with atempo fallback.
  - A whole-item drift indicator so the operator can see accumulating misalignment at a glance.
- Out of scope:
  - Multi-speaker work (WP-0291/0292).
  - The mixer and ambience (WP-0294).
  - Replacing the translation stage (WP-0289 MT-01) — but `empty_text` must be **visible** here,
    since that defect is currently invisible until you inspect the JSON by hand.

## Acceptance criteria
1. For the Haerin clip, every segment shows its subtitle slot length, its rendered dub length, and
   the delta; segment 1's empty translated text is visibly flagged rather than silently absent.
2. The operator can re-render a single segment without re-running the whole dub.
3. Speed/stretch adjustments are bounded by the documented caps and the applied factor is displayed.
4. A/B playback works for original vs dub per segment.
5. Changes write through subtitle versioning; no in-place destruction of the operator's edits.
6. `tsc --noEmit` exit 0; contract tests cover the delta calculation and the status taxonomy.

## Test / verification plan
- Focused tests: slot-vs-audio delta maths, status classification (including `empty_text` and
  `overflow_trimmed`), stretch-cap enforcement.
- Real-content: Haerin clip end to end, with the operator confirming the dub lands on the beat.
- App-boundary: headless snapshot of the sync view inspected for readability and no overlapping text.

## Risks / open questions
- **Per-segment re-render cost**: CosyVoice measured RTF ~9.4-12.1 on CPU (2.64 s of audio took
  ~29-33 s). A single-segment re-render is therefore tens of seconds, not instant. Mitigation: make
  it explicitly asynchronous with progress, and prioritise GPU enablement (WP-0289 MT-08) — the
  RTX 3090 is currently unused because the cloning venv carries `torch 2.3.1+cpu`.
- **Over-correction**: unlimited stretch degrades cloned speech audibly (field evidence: plain
  atempo above ~1.3x). Mitigation: enforce caps and show the applied factor.
- **Scope overlap with WP-0291**: both draw waveforms. Mitigation: build one shared waveform/peaks
  component in whichever packet lands first; the second reuses it.
- Open: should re-render use the frozen WP-0288 backend or allow a per-segment backend override for
  experimentation? Proposed: frozen default, with override behind Advanced disclosure.

## Added by review 2026-08-05

**Hard prerequisite — the auto pipeline discards timing-fit params.** `timing_fit_enabled`,
`timing_fit_min_factor`, `timing_fit_max_factor` and `loudness_target_lufs` are passed as `None` at
every *automatic* enqueue site: `jobs.rs` L8101, L8334, L11977, L12278, L12366, **L12778**, L21412,
falling back to `unwrap_or(0.6)` at L13830. They are threaded only through
`enqueue_mix_dub_preview_v1_with_options` (L2232-2252), reachable solely from the manual "Mix dub"
button (TSX L3816-3829). **AC3 would therefore display a stretch factor the default run never
applied.** Threading these params through the auto pipeline is a prerequisite of this packet — and it
is the same fix WP-0294 needs, so it should be done once, in one packet, not twice.

**Reconcile the status taxonomy with the shipped QC report** (G18). QC already ships severity + kind
+ segment + jump + reveal (L10331-10515) and already advertises "timing mismatches, silent clips,
noisy references, clipping, and weak clone similarity" (L10334-10335). Introducing a parallel
`fits/stretched/compressed/overflow_trimmed/empty_text` taxonomy creates two truth surfaces for one
fact, which teaches the operator to trust neither. Extend the QC kinds instead.

**`empty_text` is not single-speaker-specific** (G16). Multi-speaker items have the identical defect
— an empty translated string yields a silent hole — with no surface at all today. Make it a QC issue
kind that applies to every item, not a status local to this view.

**Design for the real interaction cost.** At CPU RTF ~9.4-12.1, a single-segment re-render is tens of
seconds. AC2 must be specified as an asynchronous queued action with visible progress, not a button
that appears to hang; AC4's A/B playback implies a fluidity the CPU tier cannot deliver, so it must
play already-rendered audio only.

**Grounding numbers are runtime artifacts, not repo state.** The segment timings and the 3.37 s /
2.64 s clone durations cited above come from APPDATA derived artifacts and the WP-0288 proof bundle;
they are **not verifiable from the repo** by a no-context model. Treat them as recorded measurements
with their proof-bundle path, not as facts a reader can re-derive from source.

## Status updates
- 2026-08-05: WP created from operator request ("audio/voiceclone/sub sync tools for the single speaker").
- 2026-08-05: Review added the timing-fit plumbing prerequisite (which also blocks WP-0294), QC
  taxonomy reconciliation, `empty_text` generalisation, async re-render design, and flagged the
  grounding numbers as runtime-only.
