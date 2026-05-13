# Work Packet: WP-0216 - Localization automatic voice-reference continuation

## Status

REVIEW

## Owner

Codex

## Scope

- Make the default voice-clone localization run use the existing source-reference candidate system automatically after diarization.
- When speakers are labeled but missing clone references, generate source-media voice samples for those speakers and attach them to the item speaker settings.
- Continue to the dub stage automatically when generated references satisfy the voice plan.
- Keep the operator checkpoint only when source-reference extraction fails or a speaker still lacks a usable reference.
- Improve run notes so the home/editor surfaces can report source-reference generation instead of implying speaker labeling is still running.

## Out of Scope

- Full voice-clone quality benchmarking.
- Replacing OpenVoice/Kokoro.
- Multi-label overlapping-speaker schema work.
- Removing advanced/manual voice asset surfaces.

## Acceptance

- A full localization run no longer stops at `voice_plan` just because generated source references have not been manually applied.
- Generated reference candidates are created from source media and applied to missing speakers before queuing dub.
- If generation fails, the run remains at `voice_plan` with concrete missing speaker and failure notes.
- Existing manual candidate generation/apply UI still works.
- Verification includes engine tests and headless/app-boundary inspection per `build_rules.md`.

## Notes

- 2026-05-13: Created after the running localization test finished diarization but paused at `voice_plan` for `S1`, contradicting the setup-first voice-clone service promise.
- 2026-05-13: Implemented automatic source-reference generation/apply before the voice-plan pause. Repaired the live test item by generating an `S1` source reference and queuing the dub continuation; that dub job then failed on the real next blocker: missing voice-preserving TTS/OpenVoice pack.
- 2026-05-13: Verification summary: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0216/20260513_143027/summary.md`.
