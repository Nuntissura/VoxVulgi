# Work Packet: WP-0179 - Translation Style Settings

## Metadata
- ID: WP-0179
- Owner: Codex
- Status: REVIEW
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

## Research basis (2026-08-14)

- whisper.cpp exposes `initial_prompt` and `carry_initial_prompt`, with the decoder enforcing a bounded prompt context. OpenAI Whisper likewise accepts an initial prompt and conditions later windows on prior text. This is the installed offline backend's available control surface for style and honorific instructions.
- DeepL's current translation API independently models formality (`default`, more formal, or less formal), custom translation instructions, context, glossary selection, and formatting preservation. That supports treating tone, honorific behavior, punctuation, and terminology as one queue-time translation request rather than unrelated browser-only preferences.
- Selected approach: a versioned per-item compound setting, queue-time snapshots, a shared bounded Whisper prompt, and narrow deterministic cleanup only where the requested rule is unambiguous. Rejected: global-only browser storage, because it does not persist with the item or survive another client; unrestricted post-processing, because English title choice and contraction rewriting are context-sensitive; and a new online translator, because the default pipeline must remain offline.
- Risks and mitigations: long custom instructions could displace honorific/glossary guidance, so honorific guidance is ordered first and the whole prompt is character-bounded; item switches could save the prior item's values, so controls remain disabled until the requested item loads and writes capture the item ID; queued jobs could drift after later edits, so all translation producers serialize the effective setting; unsafe custom text and paths are bounded and validated by the engine.
- Primary sources checked: `https://github.com/ggml-org/whisper.cpp/blob/master/examples/cli/cli.cpp`, `https://github.com/ggml-org/whisper.cpp/blob/master/src/whisper.cpp`, `https://github.com/openai/whisper/blob/main/whisper/transcribe.py`, and `https://developers.deepl.com/api-reference/translate/request-translation`.

## Implementation status (2026-08-14)

- Product code implemented: per-item atomic persistence, Neutral/Formal/Informal/Custom UI, custom instruction entry, Preserve/Translate/Drop honorific control, queue-time snapshots in direct and automatic translation producers, execution fallback for legacy queued jobs, bounded prompt composition, deterministic punctuation behavior, and safe hyphenated-honorific removal.
- Verification passed: `npm run build`; `cargo check --locked -j 1 --manifest-path product/desktop/src-tauri/Cargo.toml`; targeted engine translation tests (`10 passed`, including per-item persistence, path traversal, control-character rejection, prompt bounding, punctuation differences, and honorific removal).
- Remaining before `DONE`: governed desktop build plus headless packaged UI inspection under `PROOF_STANDARD.md`; an audio-backed translation comparison to confirm the selected prompt changes the installed Whisper model's output beyond deterministic cleanup.
