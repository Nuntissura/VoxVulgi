# Work Packet: WP-0048 - Phase 2: Speaker -> TTS voice mapping UI (pyttsx3)

## Metadata
- ID: WP-0048
- Owner: Codex
- Status: DONE
- Created: 2026-02-22
- Target milestone: Phase 2 — Dubbing MVP

## Intent

- What: Add a simple per-item speaker settings store and UI to map diarized speakers to system TTS voices (pyttsx3), then use that mapping when rendering `tts_preview_pyttsx3_v1`.
- Why: Phase 2 requires multi-speaker English dubbing via selected TTS voices (no voice cloning). Without voice mapping, all speakers sound the same.

## Scope

In scope:

- Engine:
  - Add DB table for per-item speaker settings:
    - `speaker_key` (from subtitle segment `speaker`),
    - optional `display_name`,
    - optional `tts_voice_id` (pyttsx3 voice id).
  - Add APIs to list/upsert speaker settings for an item.
  - Add API to list available pyttsx3 system voices (id + name).
  - Update `tts_preview_pyttsx3_v1` job:
    - include `speaker` + resolved `tts_voice_id` per segment in the Python request payload,
    - set pyttsx3 voice per segment/group (best-effort),
    - include `tts_voice_id` per segment in the written manifest (best-effort).
- Desktop:
  - Subtitle editor:
    - load available voices (on demand),
    - show speakers present in the current track,
    - allow mapping each speaker to a voice,
    - persist mapping via engine APIs.
  - Display speaker name using `display_name` when set (fallback to `speaker_key`).

Out of scope:

- Neural TTS or voice cloning (separate WPs).
- Consent mechanisms or anti-abuse controls.
- Auto-detecting voices per speaker.

## Acceptance criteria

- A user can select a subtitle track with speaker labels and map each speaker to a TTS voice in the UI.
- The mapping is persisted per item and restored on reload.
- Running `TTS preview (local)` renders segments using the selected voice per speaker (best-effort).
- No silent network egress is introduced.

## Test / verification plan

- On an item with at least 2 speakers:
  - run diarization to produce speaker labels,
  - set two different voices for two speakers,
  - run TTS preview and spot-check that segments differ in voice (best-effort).

## Risks / open questions

- Voice availability differs per OS; voice ids are not portable across machines.
- pyttsx3 voice switching semantics may vary; we may need to group by voice and flush between groups for correctness.

## Status updates

- 2026-02-22: Started.
- 2026-02-22: Implemented `item_speaker` DB table + list/upsert APIs + pyttsx3 voice listing API + Subtitle Editor voice-mapping UI; updated `tts_preview_pyttsx3_v1` to apply per-speaker voice (best-effort).
- 2026-02-22: Marked done; verification is covered by WP-0027.
