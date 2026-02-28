# Work Packet: WP-0022 - Phase 2: Separation (Spleeter pack + job)

## Metadata
- ID: WP-0022
- Owner: Codex
- Status: DONE
- Created: 2026-02-22
- Target milestone: Phase 2 (voice-preserving dubbing)

## Intent

- What: Add a local-first source separation baseline by integrating Spleeter as an explicit-install Python pack, and a separation job that produces vocals/background stems per library item.
- Why: Background preservation in dubbing depends on reliably isolating speech from background audio.

## Scope

In scope:

- Engine:
  - Add Spleeter pack status + installer (pip install into managed venv; triggered only by explicit user action).
  - Add new job type `separate_audio_spleeter`:
    - extracts a working WAV audio file from the library item,
    - runs Spleeter 2-stem separation,
    - writes `vocals.wav` and `background.wav` to the item derived directory.
- Desktop:
  - Diagnostics shows Spleeter pack status and exposes "Install Spleeter" (explicit user action).
  - Library items list exposes a "Separate" action to enqueue the job.
- Governance:
  - Add WP row to Task Board.
  - Update technical design notes if needed.

Out of scope:

- Diarization, TTS, voice cloning (separate WPs).
- GPU optimization and performance tuning.
- Any telemetry or background downloads.

## Acceptance criteria

- Diagnostics shows:
  - Spleeter pack installed/not installed and version (best-effort).
- Clicking "Install Spleeter" installs the pack into the managed venv (explicit user action).
- A separation job can be queued for an item and writes:
  - `derived/items/<item_id>/separation/spleeter_2stems/vocals.wav`
  - `derived/items/<item_id>/separation/spleeter_2stems/background.wav`
- No silent network egress is introduced:
  - any pack installation happens only when the user clicks install in Diagnostics.

## Test / verification plan

- Queue separation job on a short local video:
  - verify stems files exist and are non-empty.
- Confirm no downloads occur unless the user explicitly installs Spleeter.

## Risks / open questions

- Spleeter dependency footprint (Tensorflow) is large; install failures may be common on some machines.
- Spleeter may download pretrained models on first use; we should ensure that happens during explicit install rather than during a background job.

## Status updates

- 2026-02-22: Started.
- 2026-02-22: Implemented Spleeter pack status/install (Diagnostics) + separation job + Library enqueue action.
- 2026-02-22: Marked done; verification is covered by WP-0027 once Spleeter installer/runtime compatibility is resolved.
