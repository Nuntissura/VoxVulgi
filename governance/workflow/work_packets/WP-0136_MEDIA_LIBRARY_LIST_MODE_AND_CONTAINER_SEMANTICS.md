# Work Packet: WP-0136 - Media Library list mode and container semantics

## Metadata
- ID: WP-0136
- Owner: Codex
- Status: DONE
- Created: 2026-03-09
- Target milestone: Installer smoke remediation tranche

## Intent

- What: Replace the current archive/library presentation with a clearer list-centric mode and explicit container semantics for subscriptions, playlists, folders, and single items.
- Why: Installer smoke shows that card layout still feels wrong for large archives and the current grouping model does not explain what is a folder, playlist, subscription, or single file.

## Scope

In scope:

- Expansive list mode for Media Library and archive-heavy views.
- Explicit labels, badges, or columns for container type and relationship.
- Width/layout fixes so important actions do not disappear in narrow panels.
- Panel-local scrolling behavior for dense archive tables/lists where appropriate.

Out of scope:

- A full redesign of every page in the app.

## Acceptance criteria

- Operators can browse archive content in a list-first mode suited to large libraries.
- Container semantics are obvious without guessing.
- Critical actions remain visible at practical window widths.

## Test / verification plan

- Desktop build and app-boundary verification with large migrated library state.
- Proof bundle with before/after operator flows and representative screenshots or path summaries.

## Status updates

- 2026-03-09: Created from installer smoke findings `ST-012`, `ST-014`, and `ST-015`.
- 2026-03-09: Implemented a list-first Media Library view with explicit provider/container semantics, preserved a secondary card view, and kept dense archive browsing inside a panel-local scrolling surface. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0136/20260309_093904/`.
