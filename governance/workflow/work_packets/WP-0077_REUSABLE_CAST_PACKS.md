# Work Packet: WP-0077 - Reusable cast packs

## Metadata
- ID: WP-0077
- Owner: Codex
- Status: DONE
- Created: 2026-03-06
- Target milestone: Phase 2 usability and repeatability

## Intent

- What: Add reusable cast packs so operators can define recurring roles such as host, narrator, contestant, and guest once and apply that role map across a whole series.
- Why: Voice templates solve per-item reuse, but recurring shows still need a higher-level series cast abstraction so operators do not rebuild the same role mapping for every episode.

## Scope

In scope:

- Add a cast-pack entity above reusable voice templates.
- Support named role slots per pack (`host`, `narrator`, `panelist_a`, `guest`, and custom labels).
- Allow applying a cast pack to an item or series item set.
- Keep cast packs operator-editable and non-destructive.

Out of scope:

- Automatic speaker identity resolution without operator review.
- New cloud dubbing services.

## Acceptance criteria

- Operators can create, rename, delete, and apply cast packs.
- A cast pack can reference reusable voice templates or template speakers for each role.
- Applying a cast pack reduces per-item setup work without overwriting unrelated speaker settings silently.

## Test / verification plan

- Engine tests for cast-pack persistence and apply behavior.
- Desktop build.
- Manual smoke on a recurring-show sample set.

## Status updates

- 2026-03-06: Created from post-WP-0076 voice-cloning expansion backlog.
- 2026-03-06: Implemented cast-pack persistence plus Localization Studio create/rename/delete/apply flows above reusable templates; verified via engine `cargo test`, desktop Tauri `cargo test`, desktop `npm run build`, and proof bundle `product/desktop/Build Target/tool_artifacts/wp_runs/WP-0077/20260306_172806/`.
