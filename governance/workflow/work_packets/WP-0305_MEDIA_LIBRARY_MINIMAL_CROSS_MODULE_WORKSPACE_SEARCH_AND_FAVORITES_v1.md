---
file_id: WP-0305-v1
file_kind: work-packet
updated_at: 2026-08-09
---

<topic id="contract" status="backlog" version="v1" wp="WP-0305" updated_at="2026-08-09">

# Work Packet: WP-0305 — Media Library minimal cross-module workspace, search, and favorites

## Metadata

- ID: WP-0305
- Owner: —
- Status: BACKLOG
- Created: 2026-08-09
- Refinement: `WP-0305_MEDIA_LIBRARY_MINIMAL_CROSS_MODULE_WORKSPACE_SEARCH_AND_FAVORITES_v1_REFINEMENT.md`
- Board: `../TASK_BOARD.md#wp-0305`
- Dependencies: WP-0170, WP-0284, WP-0286, WP-0298, WP-0300, WP-0301, WP-0303, WP-0304

## Intent

Turn Media Library into a minimal, clean, scalable cross-module workspace with canonical tabs/dropdown filters, multilingual indexed search, durable favorites, saved views, and bounded list/grid/detail rendering.

## Base scope

- Implement the complete canonical query, search-selection gate, favorites, saved views, UI hierarchy, provider integration, risk controls, and proof requirements in the refinement.
- Preserve the Media Library name, imported/current unification, all metadata, lifecycle, selections, and recovery workflows.

## Required implementation order

1. Existing surface map and migrations/tests.
2. FTS/fallback benchmark and governed selection.
3. Canonical query/count/search/favorites/saved views.
4. Tabs/toolbar/list/detail/grid implementation.
5. Provider integration, live-scale performance, visual/accessibility, build, and proof.

## Acceptance and proof

- The refinement is normative.
- No rendered-page count/filter or live per-row NAS probe may replace canonical backend state.
- FTS5 is a candidate requiring proof, not a pre-approved implementation regardless of bundled capability/performance.

</topic>

<topic id="status-updates" status="active" version="v1" wp="WP-0305" updated_at="2026-08-09">

# Status updates

- 2026-08-09: Created from current source/live-count inspection, WP-0286 canonical query proof, v0.1.133 visual evidence, and current SQLite/WAI primary documentation. No product code, library metadata, or media changed.

</topic>
