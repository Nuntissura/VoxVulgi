# Work Packet: WP-0010 — Phase 1: Local ASR (JA/KO) -> captions

## Metadata
- ID: WP-0010
- Owner:
- Status: DONE
- Created: 2026-02-19
- Target milestone: Phase 1 (MVP)

## Intent

- What: Generate captions on-device for Japanese/Korean media with timestamps and export formats.
- Why: Captions are the foundation for translation and dubbing workflows.

## Scope

In scope:

- Implement an ASR job that:
  - extracts audio to a canonical format
  - runs local ASR (runtime + model per WP-0007)
  - produces a subtitle JSON representation
  - exports SRT and VTT

Out of scope:

- Speaker diarization (Phase 2).
- Any cloud ASR default behavior.

## Acceptance criteria

- Running ASR on a library item produces:
  - `source.json` + `source.srt` + `source.vtt` artifacts
  - job logs and a clear error message on failure

## Implementation notes

- Prioritize:
  - stable timestamps
  - readable segmentation for learners
  - reasonable speed on CPU-only machines (where feasible)

## Test / verification plan

- Run ASR on a short JA clip and a short KO clip; verify outputs open in the editor and round-trip to SRT/VTT.

## Risks / open questions

- Trade-offs between accuracy vs speed on low-spec machines.

## Status updates

- 2026-02-19:
  - Implemented `asr_local` job type:
    - extracts 16kHz mono WAV via FFmpeg
    - runs local Whisper.cpp (compiled into the Rust engine; no cloud)
    - writes `source.json`, `source.srt`, `source.vtt` to `derived/items/<item_id>/asr/`
    - inserts a `subtitle_track` row pointing at `source.json`
  - Added model entry `whispercpp-tiny` (downloadable + integrity-checked via Diagnostics -> Models).
