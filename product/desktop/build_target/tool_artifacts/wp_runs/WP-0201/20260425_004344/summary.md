# WP-0201 Summary

Status: REVIEW
Date: 2026-04-25

## Outcome
- Implemented empty-transcript fail-fast behavior for `asr_local` and `translate_local`.
- ASR now records Whisper raw/usable segment diagnostics and fails before writing `source.json` or inserting a subtitle track when no usable segments exist.
- Translation now rejects empty source tracks, records raw Whisper translation counts plus aligned usable counts, and fails before writing `en.json` or inserting a translated track when no usable English segments exist.
- Localization continuation now refuses stale empty source/translated tracks and returns a blocked stage summary without queueing diarization, dub, mix, or export follow-ups.
- Reconciled WP metadata for `WP-0198`, `WP-0199`, and `WP-0200` to match their Task Board `REVIEW` status.

## Verification
- `cargo test whisper_json_to --lib` from `product/engine` - passed.
- `cargo test empty --lib` from `product/engine` - passed.
- `cargo test subtitle_document_segment_stats_counts_usable_text_only --lib` from `product/engine` - passed.
- `cargo check` from `product/engine` - passed with existing warnings.
- `npm run build` from `product/desktop` - blocked by local PowerShell `npm.ps1` execution policy.
- `& 'C:\Program Files\nodejs\npm.cmd' run build` from `product/desktop` - passed.
- `cargo check` from `product/desktop/src-tauri` - passed with existing warnings.

## Evidence
- `evidence.json`

## Notes
- This is not marked `DONE` because the failed Queen-sample installer-state smoke and live operator surface verification still need to be rerun.
- The implementation deliberately avoids changing Whisper model quality, VAD behavior, or diarization-pack repair; those remain out of scope for `WP-0201`.
