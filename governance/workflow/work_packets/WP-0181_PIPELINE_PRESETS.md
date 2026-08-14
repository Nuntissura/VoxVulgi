# Work Packet: WP-0181 - Pipeline Presets

## Metadata
- ID: WP-0181
- Owner: Codex
- Status: DONE
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

## Research basis (2026-08-14)

- HandBrake separates immutable built-in presets from user-created custom presets and lets operators save current settings into a reusable preset. Its official preset design also treats source-dependent audio and subtitle selection as rules rather than fixed output tracks. Adobe Media Encoder follows the same built-in-plus-custom pattern with explicit edit, save, and delete operations.
- Selected approach: generate the three packet-defined built-ins in code; persist only custom definitions in one versioned, atomically written catalog; snapshot the selected definition into the current item; apply global batch rules and per-item translation style through their existing authoritative stores; and defer voice-template/cast-pack matching until translated speaker labels exist. Rejected: browser-only preset storage, because jobs and other clients could not consume it; embedding the preset ID alone in jobs, because editing/deleting a custom preset could change an item already in flight; and preset export/import, because the packet explicitly excludes sharing.
- Risks and mitigations: built-ins and custom IDs are separated and engine-enforced; item IDs and user text are bounded and validated; custom style requires an actual instruction; catalog and item files use atomic writes; applying to an item stores a full snapshot so later custom edits/deletion do not mutate that item; voice defaults record a one-time application flag so retries do not repeatedly overwrite the operator's voice plan; applying a preset broadcasts the per-item style to the mounted editor so its debounced persistence cannot restore stale controls.
- Validation plan: engine CRUD/immutability/traversal tests, frontend production build, full desktop `cargo check`, packaged headless semantic UI audit, and item-level persistence/voice-default inspection through the installed app boundary.
- Primary sources checked: `https://handbrake.fr/docs/en/latest/advanced/custom-presets.html`, `https://handbrake.fr/docs/en/latest/technical/official-presets.html`, and `https://helpx.adobe.com/media-encoder/using/custom-encoding-presets.html`.

## Implementation status (2026-08-14)

- Product code implemented: three immutable built-ins; versioned atomic custom catalog; custom create/update/delete; active Localization Studio selector; global batch-rule and per-item ASR/translation-style application; per-item full preset snapshots; and deferred one-time voice template/cast-pack auto-matching after translated speaker labels exist.
- Verification passed: `npm run build`; `cargo check --locked -j 1 --manifest-path product/desktop/src-tauri/Cargo.toml`; and targeted engine tests (`3 passed`, covering the exact built-ins, atomic custom CRUD/item snapshots, built-in immutability, item traversal rejection, control-character rejection, and the required custom instruction boundary).
- Completion proof: governed v0.1.149 packaged headless UI audit and visually inspected screenshots prove the three built-ins and the complete custom editor surface. A focused engine regression proves item snapshot persistence and that voice-template defaults remain pending without speakers, apply after a speaker appears, persist the one-time flag, and do not apply again. Proof bundle: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0181/20260815-0050_v0_1_149/`.
