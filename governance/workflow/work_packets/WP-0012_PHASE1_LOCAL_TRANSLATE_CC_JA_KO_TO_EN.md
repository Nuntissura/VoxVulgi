# Work Packet: WP-0012 - Phase 1: Local translate CC (JA/KO -> EN)

## Metadata
- ID: WP-0012
- Owner: Codex
- Status: DONE
- Created: 2026-02-19
- Target milestone: Phase 1 (MVP)

## Intent

- What: Translate captions on-device from Japanese/Korean to English with readability constraints.
- Why: Translation is the bridge from source captions to dubbed output and study-friendly subtitles.

## Scope

In scope:

- Implement a translation job that:
  - takes subtitle JSON segments as input
  - runs local translation (runtime + model per WP-0007)
  - supports a glossary (term overrides)
  - enforces QC constraints (line length, CPS)
  - outputs EN subtitle JSON + SRT/VTT

Out of scope:

- Cloud translation default behavior.
- Full translation memory system (beyond glossary).

## Acceptance criteria

- Translation job produces EN subtitle artifacts and a bilingual view is possible in the editor UI (even if minimal).
- Glossary overrides are applied deterministically.

## Implementation notes

- Keep timing stable by default; translation should not change segment boundaries unless explicitly requested.
- Phase 1 approach: use **Whisper.cpp translate mode** on the item's audio and **align output back onto the source segment windows** (stable timings, segment count preserved).
- Glossary: `config/glossary.json` (string->string map, applied deterministically longest-key-first).
- QC: wrap lines and emit warnings (CPS/line count) into job artifacts.

## Test / verification plan

- Translate a short JA clip and KO clip; verify:
  - QC constraints are enforced (or warnings are shown)
  - glossary substitutions occur

## Risks / open questions

- How to handle honorifics/style consistently across a series (settings UX).

## Status updates

- 2026-02-19: Started implementation (engine `translate_local` job + minimal bilingual editor view).
- 2026-02-19: Implemented `translate_local` (Whisper.cpp translate + alignment), glossary + QC warnings report, Tauri command + editor UI; build verified (`cargo test`, `npm run build`).
