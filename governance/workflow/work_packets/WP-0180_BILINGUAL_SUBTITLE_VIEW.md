# Work Packet: WP-0180 - Bilingual Subtitle View

## Metadata
- ID: WP-0180
- Owner: Codex
- Status: BACKLOG
- Created: 2026-04-08
- Target milestone: Translation Quality

## Intent

- What: Add a side-by-side bilingual view showing source language and English translation for each subtitle segment.
- Why: The spec (Section 4.3) requires "optional bilingual view (source + EN)." Language learners and QC reviewers need to see both versions simultaneously to verify translation accuracy.

## Scope

In scope:
- Toggle in the subtitle editor to enable bilingual mode.
- Each segment row shows source text (JA/KO) alongside English translation.
- Both columns are editable.
- Sync scrolling between source and translation.
- Visual alignment indicators when segment counts differ between tracks.

Out of scope:
- More than 2 languages simultaneously.
- Automatic alignment of mismatched segment counts.

## Acceptance criteria
- Bilingual toggle shows source + English side-by-side for each segment.
- Both columns are editable.
- View persists across page switches.
- `npm run build` passes.
