# Work Packet: WP-0181 - Pipeline Presets

## Metadata
- ID: WP-0181
- Owner: Codex
- Status: REVIEW
- Created: 2026-04-08
- Target milestone: Automation

## Intent

- What: Add one-click pipeline presets that configure the entire localization workflow for common content types.
- Why: Setting up ASR language, batch-on-import toggles, translation style, and voice settings individually for each item is repetitive. Presets like "Japanese Anime" or "Korean Variety Show" configure everything in one click.

## Scope

In scope:
- Preset data model: name, ASR language, batch-on-import rules, translation style, default voice template/cast pack.
- 3 built-in presets: "Japanese Anime" (ja, auto-ASR+translate+diarize), "Korean Variety" (ko, auto-ASR+translate+diarize), "Quick Subtitles Only" (auto, ASR only).
- Preset selector on the Localization Studio home screen.
- Custom preset CRUD (save current settings as preset, edit, delete).
- Applying a preset configures all matching settings in one action.

Out of scope:
- Preset sharing/export.
- Per-segment preset overrides.

## Acceptance criteria
- Built-in presets are selectable from the home screen.
- Applying a preset sets ASR language, batch rules, and translation style.
- Custom presets can be saved and loaded.
- `cargo check` + `npm run build` pass.
