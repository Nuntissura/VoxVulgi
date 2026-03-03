# Work Packet: WP-0070 - "Items" window clarity, naming, and scope cleanup

## Metadata
- ID: WP-0070
- Owner: Codex
- Status: DONE
- Created: 2026-03-03
- Target milestone: Stabilization sprint (UX clarity)

## Intent

- What: Clarify the purpose of the current "Items" window, then rename and/or repurpose it to match user expectations.
- Why: Ambiguous navigation labels increase cognitive load and make workflows harder to discover.

## Scope

In scope:

- Inventory what features currently live under "Items".
- Define one-sentence purpose statement for that workspace.
- Rename window and supporting copy/tooltips to reflect real function.
- Update docs/spec terminology to match the shipped UX.

Out of scope:

- Major feature additions unrelated to terminology and discoverability.
- Rewriting ingestion or editor pipelines.

## Acceptance criteria

- The renamed window has a clear user-facing purpose and concise description.
- Navigation + docs use the same terminology.
- No broken links or route regressions from rename.

## Test / verification plan

- Manual smoke of navigation routes and deep links.
- Search/replace audit for stale "Items" labels in UI copy and docs.
- `npm run build` in `product/desktop`.

## Status updates

- 2026-03-03: Created.
- 2026-03-03: Replaced ambiguous `Items` window label with `Media Library`, updated workspace heading/copy to define its purpose, and aligned navigation terminology. Verified with desktop build.
