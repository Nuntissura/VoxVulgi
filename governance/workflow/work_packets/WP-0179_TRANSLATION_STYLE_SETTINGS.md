# Work Packet: WP-0179 - Translation Style Settings

## Metadata
- ID: WP-0179
- Owner: Codex
- Status: BACKLOG
- Created: 2026-04-08
- Target milestone: Translation Quality

## Intent

- What: Add translation style controls (formal/informal, honorific handling, punctuation rules) to the translation pipeline.
- Why: The spec (Section 4.3) requires "style settings (formal/informal, honorific handling, punctuation rules)." Korean and Japanese have complex honorific systems that affect English translation tone. Without style controls, translations default to a single tone regardless of content type.

## Scope

In scope:
- Style selector in the Track card's translation section: Formal / Informal / Neutral / Custom.
- Honorific handling toggle: Preserve (keep -san, -sensei, etc.) / Translate (convert to English equivalents) / Drop.
- Per-item style that persists with the item.
- Engine: pass style parameters to the translation pipeline as system prompts or post-processing rules.

Out of scope:
- Per-segment style overrides.
- Style learning from existing translations.

## Acceptance criteria
- Operators can select translation style before running Translate.
- Honorific handling option is available.
- Style choice affects translation output.
- `cargo check` + `npm run build` pass.
