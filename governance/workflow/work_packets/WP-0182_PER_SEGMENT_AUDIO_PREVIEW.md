# Work Packet: WP-0182 - Per-Segment Audio Preview

## Metadata
- ID: WP-0182
- Owner: Codex
- Status: BACKLOG
- Created: 2026-04-08
- Target milestone: UX Polish

## Intent

- What: Add a play button on each subtitle segment that plays the dubbed audio for just that segment.
- Why: Currently operators must play the full video/mix and seek to the right timestamp to hear a specific segment. Per-segment preview enables rapid QC — click, listen, fix, repeat.

## Scope

In scope:
- Play button on each segment row in the subtitle editor.
- Plays the TTS/dub audio for that segment's time range from the mix or per-segment WAV artifacts.
- Visual playback indicator on the active segment.
- Stop on click or when playback finishes.
- Falls back gracefully if no dubbed audio exists for that segment.

Out of scope:
- Waveform visualization (separate WP).
- Editing audio from the segment view.

## Acceptance criteria
- Each segment row has a play button.
- Clicking play starts audio for that segment only.
- Playback stops at segment end or on re-click.
- Works with existing TTS/dub artifacts.
- `npm run build` passes.
