# Work Packet: WP-0097 - Shared storage root options and path hydration

## Metadata
- ID: WP-0097
- Owner: Codex
- Status: DONE
- Created: 2026-03-07
- Target milestone: Workflow and archive UX hardening

## Intent

- What: Centralize the download/export root in an Options surface and make path hydration deterministic across startup, updates, and window switches.
- Why: Temporary "missing folder" states and pane-local folder controls make the app feel unreliable, especially when operators already maintain large archive roots on other disks or NAS locations.

## Scope

In scope:

- Move the shared download/export root controls into an Options window or equivalent global settings surface.
- Persist the chosen root across startup, updates, and window switches.
- When a root is selected, create expected app-managed subfolders if they do not already exist.
- If expected folders already exist, index or hydrate them instead of behaving like the root is missing.
- Remove transient "download folder missing" flicker caused by window switching or delayed config hydration.

Out of scope:

- Deep legacy-library reconciliation beyond initial root hydration.
- Provider-specific download behavior changes.

## Acceptance criteria

- The selected shared root remains available immediately after startup and when switching windows.
- Choosing an existing root no longer produces a short-lived missing-folder error when the folder is valid.
- Expected app-managed folders are created on first use when absent.
- Existing folders under a chosen valid root are indexed or recognized without forcing the operator to re-create structure manually.
- Folder configuration is discoverable from a single global settings surface instead of duplicated pane-local blocks.

## Test / verification plan

- Desktop build.
- Manual UI smoke across startup and several window switches with both fresh and pre-populated roots.

## Status updates

- 2026-03-07: Created from operator feedback on missing-folder flicker, path persistence, and the need for a single global storage-root control surface.
- 2026-03-07: Implemented shared download/export root management in a new Options window, replaced pane-local root ownership with a shared frontend hydration store, and updated Library/Localization to consume the same status source. Verified with `npm run build`, `cargo test -q` in `product/desktop/src-tauri`, and `cargo test -q` in `product/engine`. Proof: `product/desktop/Build Target/tool_artifacts/wp_runs/WP-0097/20260307_033244/summary.md`.
