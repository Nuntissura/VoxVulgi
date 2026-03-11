# Work Packet: WP-0116 - Backend starter recipes and adapter templates

## Metadata
- ID: WP-0116
- Owner: Codex
- Status: DONE
- Created: 2026-03-08
- Target milestone: Voice backend modernization

## Intent

- What: Add backend-specific starter recipes for strong OSS candidates such as CosyVoice, Seed-VC, and XTTS-style adapters.
- Why: Operators need concrete starting configurations and command templates, not just a blank BYO adapter form.

## Scope

In scope:

- Add richer starter-recipe metadata for known experimental backends.
- Include suggested probe/render command templates, placeholder usage, and operator notes.
- Let Diagnostics apply a starter recipe into the adapter draft/config instead of retyping commands from scratch.
- Keep these recipes explicit, editable, and local-only.

Out of scope:

- Bundling heavyweight experimental runtimes into the default installer.
- Silent runtime installs or opaque one-click environment mutation.

## Acceptance criteria

- Diagnostics exposes backend-specific starter recipes for at least CosyVoice, Seed-VC, and XTTS-style adapters.
- Operators can apply a starter recipe into the current adapter config/draft and then edit it further.
- Recipe notes explain the expected layout/placeholders clearly enough to bootstrap a local checkout.
- Existing BYO adapter transparency and local-only constraints remain intact.

## Test / verification plan

- Rust tests for recipe lookup and application/merge behavior.
- Desktop build.
- Tauri/UI smoke via build coverage for recipe listing/application wiring.

## Status updates

- 2026-03-08: Created from the research-driven operational backend tranche.
- 2026-03-08: Implemented typed starter recipes for known OSS backends, Diagnostics recipe-apply controls, and proof under `product/desktop/build_target/tool_artifacts/wp_runs/WP-0116/20260308_171300/`.
