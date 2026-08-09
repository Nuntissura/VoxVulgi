---
file_id: WP-0305-REFINEMENT-v1
file_kind: work-packet-refinement
updated_at: 2026-08-09
---

<topic id="operator-request-and-verified-state" status="active" version="v1" wp="WP-0305" updated_at="2026-08-09">

# Operator request

- Redo the media gallery to incorporate all modules with a minimal, clean GUI/UI.
- Add tabs, dropdown filters, better search, and a Favorites tab.
- Keep the surface useful for YouTube, Instagram, TikTok, Image Archive, Localization, and local/imported media.

# Verified current state

- The canonical product surface is named Media Library; imported and current media are already intended to form one library.
- The inspected live database contains 141,117 library items.
- WP-0286 moved filtering/search/sorting before pagination and returned truthful matching totals. This full-set contract is required baseline.
- Current filter controls cover search, source, media type, lifecycle, canonical single, sort, view, and grouping, but provider coverage is currently YouTube/Instagram/Local and no Favorites persistence was found in inspected product code.
- The v0.1.133 800x600 artifact shows substantial filter/action chrome, card-like bordered groupings, long path/title truncation, and ambiguous container labels. The current live UI was not running for a new snapshot, so current visual parity with that artifact remains to be rechecked.
- Render-time media availability can trigger NAS-backed preflight paths; WP-0298 persistent observations are required before the redesigned Library relies on availability filters at scale.
- Current search uses ordinary predicates. FTS5 is an appropriate candidate for 141,000+ multilingual items, but bundled SQLite capability and index synchronization must be proven before selection.

# Authority and dependencies

- Spec anchors: PRODUCT_SPEC 8.2; TECHNICAL_DESIGN 6.6.
- Preserve: WP-0170 filters, WP-0284 selection/lifecycle, WP-0286 canonical full-set query, WP-0279 headless audit.
- Dependencies: WP-0298 availability/performance boundary, WP-0300 canonical metadata/title, WP-0301 settings. Provider filter completion depends on WP-0303 and WP-0304; the layout/query may be built earlier with typed provider fixtures.

# Scope edges

- In scope: canonical tabs, compact dropdown toolbar, indexed multilingual search, favorites persistence, provider integration, saved views, list/grid/detail hierarchy, bounded rendering/thumbnails, backend counts/selection, accessible/stable IDs, visual/performance proof.
- Non-goals: renaming Media Library, splitting providers into separate physical libraries, moving/deleting media, inferring provider from folder names, reintroducing imported/current partitions, new dashboard cards, or cleaning existing MP4/other artifacts.

</topic>

<topic id="research-basis-and-selected-design" status="active" version="v1" wp="WP-0305" updated_at="2026-08-09">

# Sources checked

- Current `LibraryPage.tsx`, engine library query/schema, live library counts, WP-0170/WP-0284/WP-0286 contracts/proof, v0.1.133 screenshot/dump, and no-card/headless rules.
- SQLite FTS5, external-content synchronization/pitfalls, integrity/rebuild, Unicode/prefix/trigram options: `https://www.sqlite.org/fts5.html`.
- SQLite query planner: `https://www.sqlite.org/queryplanner.html`.
- SQLite row-value/keyset scrolling: `https://www.sqlite.org/rowvalue.html`.
- WAI-ARIA tabs and grid/table patterns: `https://www.w3.org/WAI/ARIA/apg/patterns/tabs/` and `https://www.w3.org/WAI/ARIA/apg/patterns/grid/`.

# Selected information architecture

- Preserve top-level page name `Media Library`.
- Primary tabs: All, Videos, Images, Audio, Favorites. Tabs represent media/favorite outcomes rather than providers, so new providers do not add another horizontal axis.
- One compact toolbar below tabs:
  - search,
  - provider dropdown: All, YouTube, Instagram, TikTok, Image Archive, Localization, Local/imported,
  - availability/lifecycle dropdown,
  - source/subscription/container dropdown or searchable selector,
  - date dropdown,
  - sort dropdown,
  - list/grid toggle,
  - saved-view control and clear filters.
- Toolbar collapses predictably at 800px; controls remain labeled and accessible without multi-row card chrome.
- Default list rows prioritize canonical title, provider/source, creator/channel, date/duration, availability, favorite, and primary action. Full path/provenance/IDs/activity live in a detail drawer.
- Grid view is optional, bounded/virtualized, thumbnail-lazy, and uses the same canonical query/selection as list view.

# Canonical query contract

- Backend request includes tab/media class, favorite, provider, lifecycle/availability, source/container, date range, search, sort, page size, and continuation.
- All predicates apply before pagination and exact matching total. Loaded count and canonical matching total remain distinct.
- Deterministic stable order includes item ID tie-breaker. Use keyset/row-value continuation where measurement shows offset cost; continuation token carries query version/order values.
- Bulk actions receive explicit stable IDs or a canonical backend predicate/receipt; UI never expands `Select loaded` into unseen rows.
- Provider is resolved only from canonical lineage/identity/metadata, never a folder-name guess.

# Search contract and selection gate

- Search fields: operator/canonical/file title, creator/channel, provider media ID, source URL/reference, tags, and documented codec/path fallback where retained.
- First prove bundled SQLite `ENABLE_FTS5`, migration/build time, index size, query latency, Unicode behavior, and write synchronization on a disposable copy.
- If selected, use an external-content/contentless design only with explicit triggers/hooks, integrity check, rebuild command, version, and interrupted migration recovery. Escape/construct FTS queries safely; user text is not raw FTS syntax unless an explicit advanced mode exists.
- If FTS5 is unavailable or fails measured requirements, implement a documented indexed fallback; do not silently load/filter all rows in React.
- Search result relevance may be one sort mode; date/title/creator/provider sorts remain deterministic.

# Favorites and saved views

- Add `library_favorite` keyed by library item ID with created/updated time and attribution; toggles are idempotent and additive.
- Favorite state survives present/missing/unreachable/operator-deleted/file-relocation transitions and imported/current provenance.
- Favorites tab is a canonical backend predicate before pagination.
- Saved views store only filter/sort/view definitions with stable versioned IDs; they do not snapshot or own media rows. Invalid retired provider/source references degrade visibly and remain editable/deletable.

# Existing systems reused

- WP-0286 canonical query/totals/service resolution, WP-0284 stable selection/lifecycle, WP-0298 availability observations, WP-0300 metadata/title, thumbnail cache, source membership/lineage, headless bridge, diagnostics traces, WP-0301 setting IDs.

# Rejected options

- Provider tabs plus media tabs: creates two competing tab axes and scales poorly.
- Cards as the default: violates no-card rule and reduces density for 141,000 items.
- Frontend search/filter over a loaded page: already disproven by WP-0286.
- Live NAS existence check per displayed item: repeats the measured freeze path.
- Favorites stored in localStorage: not canonical, not portable across views, and not available to backend filters/agents.
- Make paths primary row text: technical detail crowds out recognizable media identity.

</topic>

<topic id="roi-red-team-microtasks-and-proof" status="active" version="v1" wp="WP-0305" updated_at="2026-08-09">

# High-ROI additions

- FTS capability benchmark/rebuild path: reuses SQLite and makes future metadata/tag growth cheaper without committing blindly.
- Saved views: reuses query serialization and makes repetitive operator workflows one action.
- Detail drawer: reuses canonical row detail and removes path/provenance clutter from every row.
- Additive favorites: reuses item IDs and provides immediate cross-provider organization without file mutation.
- One query contract for list/grid/headless: prevents UI modes and agent views from drifting.
- Search/query receipts with filter version/count/continuation: reuse Diagnostics and make canonical-versus-rendered proof reproducible.

# Risks, failure scenarios, controls, and verification

- FTS index drifts from canonical metadata/library rows.
  - Control: transactional hooks/triggers, integrity counts, rebuild/version state, startup health without blocking navigation.
  - Verify: insert/update/delete/lifecycle/metadata repair, interrupted rebuild, and canonical count parity.
- FTS query syntax crashes on punctuation/quotes/Unicode.
  - Control: safe tokenizer/query builder and literal default search.
  - Verify: Korean/Japanese/emoji/quotes/hyphens/URLs and malicious syntax strings.
- Favorite disappears when file is missing/deleted.
  - Control: foreign key to preserved library item, not path; lifecycle-independent query.
  - Verify: present→missing→relocated→deleted→redownload transitions.
- Provider filter duplicates rows due many-to-many memberships.
  - Control: canonical item projection with aggregate/EXISTS predicates.
  - Verify: one item in multiple subscriptions/playlists/providers where valid and unique result IDs.
- Exact total query becomes slow.
  - Control: measured indexes/query plans, bounded cached totals only with explicit age, and no rendered-count substitution.
  - Verify: live 141,117-item p50/p95 and query-plan receipts for common combinations.
- Grid loads excessive thumbnails/memory.
  - Control: virtualized/bounded window, lazy thumbnails, cache budget, cancellation on scroll/filter.
  - Verify: large scroll, rapid filter changes, memory/DOM counts.
- Saved view references retired settings/provider.
  - Control: versioned schema/migration and visible partial-invalid state.
  - Verify: old/unknown field fixtures.
- Clean UI hides essential recovery actions.
  - Control: primary action in row; status-specific recovery and full provenance in detail drawer.
  - Verify: available/missing/unreachable/deleted/failed-download cases and semantic inventory diff.

# Microtask plan

1. Inventory every current filter/action/detail and capture a pre-change semantic/visual map.
2. Add favorite/saved-view/query types and migrations with RED tests.
3. Benchmark FTS5 capability/design on a disposable database copy; record selected/rejected approach.
4. Implement canonical backend query/search/count/keyset/receipt and integrity/rebuild path.
5. Build tabs/compact dropdown toolbar/list/detail; retain all current workflows.
6. Add bounded grid/lazy thumbnails and stable selection/favorite actions.
7. Integrate Instagram/TikTok/provider metadata and Options defaults.
8. Run live scale/query plan/performance, lifecycle/favorites, accessibility, headless visual/action, build, and proof.

# Acceptance and proof gates

- All/Videos/Images/Audio/Favorites tabs and all named dropdown/search/sort predicates execute on the canonical full set before pagination.
- Favorites are durable/additive and survive every lifecycle/path transition.
- Search covers all required metadata, preserves multilingual text, and has a proven index integrity/rebuild or measured fallback contract.
- List/grid/detail use one canonical query/selection model with exact loaded-versus-matching truth.
- Common live queries on the 141,117-item database meet packet-defined budgets established by pre-change measurement; no render-time bulk NAS probing occurs.
- Every pre-change workflow/action is preserved or explicitly governed as replaced; card count does not increase.
- Frontend/backend/migration/FTS/interruption/accessibility tests, TypeScript/build, governed version/changelog, 800x600 and wide headless audits/actions/snapshots/dumps, concurrent diagnostics/`vvwatch`, and proof `summary.md` pass.

</topic>
