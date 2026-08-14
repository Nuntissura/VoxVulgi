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

## Research basis (2026-08-14)

- whisper.cpp exposes a bounded `initial_prompt` and `carry_initial_prompt` at the decoder boundary; OpenAI Whisper documents initial prompts for custom vocabulary and proper nouns. This is the available offline integration point for terminology hints without replacing the installed translation backend.
- Google Cloud Translation, DeepL, and Azure Translator all model glossary customization as explicit source-to-target pairs supplied with the translation request. Their current documentation also supports the packet's CSV/JSON interchange and optional contextual metadata direction.
- Selected approach: versioned global and per-item documents, per-item override precedence, queue-time snapshots, source-relevant prompt filtering, and atomic writes. Rejected: post-translation-only source-string replacement, because JA/KO source terms are normally absent from English Whisper output; and a new online translation provider, because it would violate the offline-default product contract.
- Primary sources checked: `https://github.com/ggml-org/whisper.cpp/blob/master/examples/cli/cli.cpp`, `https://github.com/openai/whisper/blob/main/whisper/transcribe.py`, `https://docs.cloud.google.com/translate/docs/advanced/glossary`, `https://developers.deepl.com/api-reference/multilingual-glossaries/create-a-glossary`, and `https://learn.microsoft.com/en-us/azure/ai-services/translator/text-translation/how-to/use-dynamic-dictionary`.

## Implementation status (2026-08-14)

- Product code implemented: versioned/legacy-compatible data model, global and per-item CRUD, context/notes, JSON/CSV import/export, source highlighting, queue-time effective-glossary snapshots, and native Whisper prompt plumbing.
- Verification passed: `npm run build`; `cargo check --locked -j 1 --manifest-path product/desktop/src-tauri/Cargo.toml`; targeted engine tests (`7 passed`, including invalid-item-path and invalid-term negative paths).
- Remaining before `DONE`: governed desktop build plus headless packaged UI inspection under `PROOF_STANDARD.md`.
