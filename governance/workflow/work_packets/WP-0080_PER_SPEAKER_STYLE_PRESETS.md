# Work Packet: WP-0080 - Per-speaker style presets

## Metadata
- ID: WP-0080
- Owner: Codex
- Status: DONE
- Created: 2026-03-06
- Target milestone: Phase 2 dubbing controls

## Intent

- What: Add reusable per-speaker style presets such as neutral, documentary narrator, game show energy, soft, and authoritative.
- Why: Voice identity is only part of dubbing quality. Operators also need quick control over delivery style without manually tuning every item.

## Scope

In scope:

- Add preset selection per speaker/template role.
- Persist style defaults inside templates and cast packs.
- Apply style hints to preview and voice-preserving jobs where supported.

Out of scope:

- Provider-specific deep prompt engineering surfaces for every backend.
- Style transfer from copyrighted reference performances.

## Acceptance criteria

- Operators can choose a style preset per speaker.
- Style presets survive template reuse and batch application.
- Unsupported backends degrade gracefully to neutral delivery.

## Test / verification plan

- Engine serialization tests.
- Desktop build.
- Manual preview smoke across at least two styles on the same speaker.

## Status updates

- 2026-03-06: Created to separate delivery-style control from raw voice identity selection.
- 2026-03-06: Implemented reusable style presets across item/template/cast-pack speaker settings and fed soft style-aware delivery hints into TTS preparation; verified via engine `cargo test`, desktop Tauri `cargo test`, desktop `npm run build`, and proof bundle `product/desktop/build_target/tool_artifacts/wp_runs/WP-0080/20260306_172806/`.
