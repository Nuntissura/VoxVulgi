# Work Packet: WP-0091 - Character libraries

## Metadata
- ID: WP-0091
- Owner: Codex
- Status: DONE
- Created: 2026-03-06
- Target milestone: Phase 3 advanced reusable voice assets

## Intent

- What: Add reusable character libraries for fictional narrator or educational persona voices, separate from real recurring speakers.
- Why: Some teaching workflows need stable narrator/guide voices that are not tied to one source speaker.

## Scope

In scope:

- Create named character voices with style/prosody defaults.
- Reuse character voices across projects independent of source-speaker identity.
- Apply character voices in hybrid workflows and alternates.

Out of scope:

- Marketplace/distribution platform for shared voices.
- Cloud-hosted asset synchronization.

## Acceptance criteria

- Operators can create and reuse character voices across items/projects.
- Character voices remain distinct from source-speaker memory/template records.
- Character voices integrate with batch dubbing and alternate exports.

## Test / verification plan

- Persistence tests.
- Desktop build.
- Manual smoke with a recurring narrator voice across unrelated items.

## Status updates

- 2026-03-06: Created to support reusable fictional or teaching-specific narrator personas.
- 2026-03-06: Completed. Added reusable `character` profiles in the app-managed voice library with copied refs, suggestions, apply/fork/delete flows, and Localization Studio controls. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0091/20260306_204301/summary.md`.
