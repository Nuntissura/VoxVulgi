# Work Packet: WP-0177 - Glossary and Custom Term Mapping

## Metadata
- ID: WP-0177
- Owner: Codex
- Status: REVIEW
- Created: 2026-04-08
- Target milestone: Translation Quality

## Intent

- What: Add a glossary system that lets operators define custom term mappings applied during translation.
- Why: The spec (Section 4.3) requires "glossary (custom term mappings)" for translation quality. Without this, names, places, and domain terms are translated inconsistently or incorrectly across segments and items.

## Scope

In scope:
- Glossary data model: term pairs (source → target) with optional context/notes.
- Glossary CRUD UI in Localization Studio (add, edit, delete terms).
- Per-item glossary that can be loaded/saved.
- Global glossary that applies to all items as a base.
- Glossary terms highlighted in the subtitle editor when they appear in source text.
- Engine: pass glossary terms to the translation pipeline as context/instructions.
- Import/export glossary as CSV or JSON.

Out of scope:
- Automatic glossary extraction from existing translations.
- Glossary sharing across users/machines.
- Integration with external terminology databases.

## Acceptance criteria
- Operators can add term pairs (e.g. "東京" → "Tokyo", "先生" → "Sensei").
- Glossary terms are passed to the translation engine.
- Terms are visually highlighted in the subtitle editor.
- Glossary can be exported and imported.
- `cargo check` + `npm run build` pass.
