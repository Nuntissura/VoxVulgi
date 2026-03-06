# Work Packet: WP-0075 - Voice-preserving dub audio regression

## Metadata
- ID: WP-0075
- Owner: Codex
- Status: DONE
- Created: 2026-03-06
- Target milestone: Stabilization sprint (localization reliability hardening)

## Intent

- What: Fix the regression where voice-preserving dubbing reports success but produces silent or missing spoken audio segments.
- Why: A real end-to-end smoke on a Korean clip showed subtitles were generated correctly but the dubbed output had no audible speaker voice, which breaks the primary classroom-use workflow.

## Scope

In scope:

- Diagnose the actual failure point in the voice-preserving dub chain.
- Fix the Kokoro/OpenVoice runtime path so the base speech stage produces real English speech audio on current pack versions.
- Stop treating silent placeholder outputs as success.
- Harden the manual smoke/example verification so future runs fail when no usable dubbed audio is produced.
- Produce fresh proof artifacts for the same sample-driven workflow under a dedicated WP-0075 artifact folder.

Out of scope:

- Replacing the voice-preserving model stack.
- New UI/UX work beyond the minimum needed to surface accurate failures.
- Cloud dubbing backends or any non-local fallback.

## Acceptance criteria

- `dub_voice_preserving_v1` produces real non-silent speech segments for a valid translated/diarized track when the pack is installed.
- When base speech generation fails, the job fails loudly instead of succeeding with silent placeholders.
- The manual smoke/example validation fails if the dubbed output contains no usable speech audio.
- Fresh smoke artifacts under `product/desktop/Build Target/tool_artifacts/wp_runs/WP-0075/` show a successful dubbed-audio run on the sample clip.

## Test / verification plan

- `cargo test` in `product/engine`
- Re-run `cargo run --example wp0029_smoke` against the same Korean sample clip used in the previous smoke.
- Inspect the generated voice-preserving report and at least one synthesized segment to confirm non-silent speaker audio.

## Status updates

- 2026-03-06: Created after manual smoke on `Test material\[4K] Queen is here 😍 Miyeon so cute 💕 (ENG SUB).mp4` exposed that `dub_voice_preserving_v1` succeeded while writing silent 0.2s WAV placeholders.
- 2026-03-06: Root cause narrowed to three issues in the voice-preserving path: the Kokoro base-TTS call did not supply a concrete fallback voice for current pack behavior, the embedded chunk parser did not understand current `KPipeline.Result` objects (`audio` / `output.audio`), and the exception path wrote silent placeholder WAVs that still let the job report success.
- 2026-03-06: Hardened the runtime and installer:
  - voice-preserving and neural Kokoro calls now use a deterministic fallback voice (`af_heart`),
  - chunk extraction now supports current Kokoro result objects and tensor audio payloads,
  - silent placeholder creation was removed,
  - `dub_voice_preserving_v1` now fails when zero usable output segments were generated,
  - the WP-0029 smoke example now checks the report and rejects silent first-segment output,
  - neural/voice-preserving pack install now requires a real Kokoro warmup into the app-managed offline cache before reporting installed.
- 2026-03-06: Verified with `cargo test` in `product/engine` and a clean-room successful `cargo run --example wp0029_smoke` on the Queen sample after deleting the smoke base directory (`tmp_smoke_wp0029`). Proof artifacts were written under `product/desktop/Build Target/tool_artifacts/wp_runs/WP-0075/manual_smoke_queen/`, including deliverables (`queen_dub_preview.mp4`, `queen_dub_preview.wav`, `queen_en.srt`, `queen_en.vtt`), `voice_preserving_manifest.json`, `tts_voice_preserving_report.json`, `voice_preserving_job_log.jsonl`, `ffprobe_mux_preview.json`, `audio_stats.json`, and `smoke_summary.md`.
