# Work Packet: WP-0057 - Phase 1: YouTube subscriptions UI + export/import portability

## Metadata
- ID: WP-0057
- Owner: Codex
- Status: DONE
- Created: 2026-02-25
- Target milestone: Phase 1 (downloader UX)

## Intent

- What: Add Library UI for managing YouTube subscriptions and export/import actions.
- Why: Users need a first-class workflow similar to 4K-style subscriptions, including portable backup/restore files.

## Scope

In scope:

- Library page controls to:
  - add/edit/delete subscriptions,
  - queue selected subscription and queue all active subscriptions,
  - export subscriptions to JSON,
  - import subscriptions from JSON.
- Tauri command surface for subscription CRUD + queue + export/import.
- UI persistence requirements:
  - loaded subscriptions remain visible after pane switches/window focus changes by reloading from DB.

Out of scope:

- New standalone subscription page outside Library.
- Scheduled automatic polling.

## Acceptance criteria

- Subscriptions list survives pane switching and app restart.
- Exported JSON can be imported back into a clean profile and recreate subscriptions/mappings.
- Queue actions from subscription UI produce download jobs visible in Jobs page.

## Test / verification plan

- `npm -C product/desktop run build`
- `cargo test` in `product/desktop/src-tauri`
- Manual smoke:
  - create 2 subscriptions with different folder maps,
  - switch panes and return; entries remain loaded,
  - export JSON and import it back.

## Status updates

- 2026-02-25: Created.
- 2026-02-25: Added Library UI for subscription CRUD, queue-one/all, and JSON export/import; wired new Tauri commands.
- 2026-02-25: Verified with `cargo test` in `product/desktop/src-tauri` and `npm -C product/desktop run build`.
