# Work Packet: WP-0201 - Localization empty-transcript fail-fast and stage truth

## Metadata
- ID: WP-0201
- Owner: Codex
- Status: BACKLOG
- Created: 2026-04-24
- Target milestone: Localization runtime reliability

## Intent

- What: Stop Localization ASR and speech-to-English translation from recording successful stage completion when Whisper returns zero usable subtitle segments.
- Why: Operator smoke on the Queen sample produced `source.json` and `en.json` with empty `segments` arrays while `asr_local` and `translate_local` still succeeded, which makes Localization and Jobs look finished even though no usable captions or downstream deliverables exist.

## Scope

In scope:
- Treat zero-segment ASR output as a hard stage failure rather than a successful subtitle-track insert.
- Treat zero-segment translation output as a hard stage failure rather than a successful translated-track insert.
- Prevent downstream localization stages from queueing or continuing when the prerequisite source/translated track is empty.
- Record stage diagnostics that make the failure debuggable, including detected language, raw segment count, usable segment count, and media path.
- Expose a clear operator-facing failure reason so Localization home and Jobs do not imply that a usable run exists.

Out of scope:
- Improving Whisper model quality or changing the shipped ASR/translation backend family.
- Full VAD research or large prompt-engineering work for low-speech/noisy clips.
- Diarization Python-pack repair, which is tracked separately in `WP-0202`.

## Acceptance criteria

- A file that yields zero usable ASR segments fails `asr_local` with an explicit empty-transcript error instead of producing a persisted empty `source` track.
- `translate_local` does not insert a translated subtitle track when the translated output is empty.
- Localization does not queue diarization, dub, mix, or export stages off an empty prerequisite track.
- Operator-facing surfaces describe the run as failed or blocked at the real stage instead of implying completion.
- Logs contain enough detail to distinguish "no speech detected", "all segments filtered empty", and other zero-output paths.

## Test / verification plan

- Reproduce the current Queen-sample failure and confirm the stage now fails instead of writing empty subtitle tracks.
- Add focused Rust tests for the empty-document guardrails in ASR/translation stage handling.
- Verify the Jobs and Localization surfaces reflect the failure reason rather than a false successful state.
- Run `cargo check` and desktop `npm run build`.

## Risks / open questions

- Some legitimate inputs may be nearly silent or intentionally contain no speech, so the error copy must be precise rather than implying generic corruption.
- The right product behavior for operator-approved "captions optional" flows remains open; this packet is about truthful failure semantics for the current shipped Localization path.

## Status updates

- 2026-04-24: Created after installer-state smoke reproduced a path where `asr_local` and `translate_local` both succeeded, but `source.json` and `en.json` contained zero subtitle segments and no usable localization run could continue.
