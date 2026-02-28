# Work Packet: WP-0023 - Phase 2: Diarization baseline (pack + job)

## Metadata
- ID: WP-0023
- Owner: Codex
- Status: DONE
- Created: 2026-02-22
- Target milestone: Phase 2 (voice-preserving dubbing)

## Intent

- What: Add a local-first diarization baseline (who spoke when) using non-gated components, installed explicitly as a Python pack, and a job that produces a speaker-labeled subtitle track.
- Why: Voice-preserving dubbing needs multi-speaker segmentation so we can map speakers to voices and preserve identity per speaker.

## Scope

In scope:

- Engine:
  - Add diarization pack status + installer (explicit install; no background downloads).
  - Add new job type `diarize_local_v1`:
    - inputs: `item_id`, `source_track_id`,
    - extracts 16k mono WAV for analysis,
    - runs local diarization baseline (VAD + speaker embeddings + clustering),
    - outputs diarization JSON and a new subtitle track with `speaker` labels populated.
- Desktop:
  - Diagnostics shows diarization pack status and exposes an install action.
  - Subtitle editor exposes a "Diarize speakers" action for a selected source track.
- Governance:
  - Add WP row to Task Board.
  - Update technical design notes if needed.

Out of scope:

- Gated diarization (pyannote pipelines) as default.
- Perfect speaker counting; this is a baseline.
- Any telemetry.

## Acceptance criteria

- Diagnostics shows diarization pack installed/not installed and versions (best-effort).
- A diarization job can be queued and produces:
  - `derived/items/<item_id>/diarize/diarization.json` (speaker turns)
  - a new `subtitle_track` row pointing at a JSON doc that has `speaker` values.
- No silent network egress:
  - any package/model downloads occur only via explicit install actions in Diagnostics.

## Test / verification plan

- Run diarization on a short multi-speaker clip:
  - confirm speaker labels are written into the new subtitle track,
  - confirm subtitle editor can load and display the labeled track.

## Risks / open questions

- Python dependency footprint is large; installs may fail.
- Baseline clustering quality may be mediocre for overlapping speech and noisy backgrounds.

## Status updates

- 2026-02-22: Started.
- 2026-02-22: Implemented diarization pack status/install (Diagnostics) + `diarize_local_v1` job + subtitle editor enqueue/polling.
- 2026-02-22: Marked done; verification is covered by WP-0027.
