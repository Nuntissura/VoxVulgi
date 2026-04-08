# Work Packet: WP-0186 - Per-Segment Clone Fallback Breakdown

## Metadata
- ID: WP-0186
- Owner: Codex
- Status: REVIEW
- Created: 2026-04-08
- Target milestone: Voice Cloning UX

## Intent

- What: Show per-segment clone/fallback status so operators can identify exactly which subtitle lines used voice cloning and which fell back to standard TTS.
- Why: "Partial fallback 8/3" tells operators something went wrong but not WHERE. They must listen to every segment to find the fallback lines. A per-segment breakdown enables targeted re-recording or manual fix.

## Scope

In scope:
- Parse the TTS manifest for per-segment voice_clone_outcome metadata.
- Add a "Clone status" column to the segment table showing: cloned / fallback / standard TTS per segment.
- Color-code segments: green (cloned), yellow (fallback), grey (standard TTS).
- Add filter to show only fallback segments for quick review.
- Show fallback reason per segment when available (missing profile, converter error, invalid output).

Out of scope:
- Automatic re-cloning of fallback segments.
- Changing the clone pipeline logic.

## Acceptance criteria
- Each segment row shows its clone outcome with color coding.
- Operators can filter to show only fallback segments.
- Fallback reasons are shown in plain language.
- `npm run build` passes.
