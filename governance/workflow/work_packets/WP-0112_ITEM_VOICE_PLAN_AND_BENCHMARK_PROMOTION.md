# Work Packet: WP-0112 - Item voice plan and benchmark promotion

## Metadata
- ID: WP-0112
- Owner: Codex
- Status: DONE
- Created: 2026-03-08
- Target milestone: Voice backend modernization

## Intent

- What: Add an item-scoped voice plan that stores the active backend strategy for a dubbing item and lets Localization Studio promote recommendation or benchmark outcomes into that durable plan.
- Why: Research-backed backend choice only helps if the app can persist the operator's decision and reuse it across the next experimental or production run.

## Scope

In scope:

- Add durable item-scoped voice plan storage.
- Store goal, preferred backend, fallback backend, selected candidate/variant, and operator notes.
- Let operators promote the current strategy recommendation or benchmark winner into the item voice plan.
- Surface the current voice plan in Localization Studio and use it as the default for subsequent experimental runs.

Out of scope:

- Global auto-promotion of a new managed default across the whole app.
- Silent backend switching for existing items.

## Acceptance criteria

- Each item can store and reload a durable voice plan.
- Localization Studio can promote recommendation/benchmark results into that plan explicitly.
- The active plan is visible and editable before queuing further voice runs.
- No existing managed backend path is replaced implicitly.

## Test / verification plan

- Rust tests for persistence and promotion logic.
- Desktop build.
- Tauri/engine tests for load/save/promote flows.

## Status updates

- 2026-03-08: Created from the voice-cloning research modernization tranche.
- 2026-03-08: Implemented durable item voice plans, recommendation/benchmark promotion actions, and Localization Studio plan editing; proof in `product/desktop/Build Target/tool_artifacts/wp_runs/WP-0112/20260308_141931/`.
