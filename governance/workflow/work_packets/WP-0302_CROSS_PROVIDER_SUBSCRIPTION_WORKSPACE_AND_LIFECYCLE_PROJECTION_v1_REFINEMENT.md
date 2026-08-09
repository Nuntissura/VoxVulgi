---
file_id: WP-0302-REFINEMENT-v1
file_kind: work-packet-refinement
updated_at: 2026-08-09
---

<topic id="operator-request-and-verified-state" status="active" version="v1" wp="WP-0302" updated_at="2026-08-09">

# Operator request

- Make the subscription module/window substantially less chaotic.
- Support Instagram and TikTok subscriptions with the same understandable behavior as YouTube while keeping provider-specific settings.

# Verified current state

- The inspected database contains 262 YouTube subscriptions and one Instagram subscription.
- The v0.1.126 Video Archiver artifact at 800x600 places active library state, workflow tabs, presets, then a large `Subscription groups (optional)` section before the YouTube subscription list.
- Existing subscription behavior is distributed among create/edit fields, groups, global/bulk queue controls, imported archive migration, source list, selected source, pending/downloaded videos, and activity.
- WP-0280 bounded the subscription master and selected video lists and proved canonical totals versus rendered windows. That behavior is required baseline and must not regress.
- Current provider schemas/lifecycle fields differ; a shared visual shell cannot assume every provider supports YouTube tabs, archive files, authentication, or pacing.
- Groups are useful secondary organization but are not the primary task and must not dominate the first viewport.

# Authority and dependencies

- Spec anchors: PRODUCT_SPEC 8.2; TECHNICAL_DESIGN 6.6.
- Preserve: WP-0220 multi-library/source layout, WP-0255/0280 Video Archiver behavior, WP-0281 memberships, WP-0282 source status, WP-0284 selected media lifecycle, WP-0279 headless audit.
- Dependencies: WP-0300 canonical metadata/title and WP-0301 module settings registry.
- Provider implementations WP-0303 and WP-0304 consume this workspace; they remain responsible for provider-specific runtime behavior.

# Scope edges

- In scope: provider-neutral bounded projection, reusable master-detail UI, clear command hierarchy, list/detail tabs, status/filter/group/search/sort, per-source settings, provider capability contract, stable IDs, responsive/accessibility proof.
- Non-goals: changing YouTube scheduler/dedupe rules, implementing Instagram/TikTok extraction, merging provider database tables without evidence, hiding canonical totals, or allowing bulk actions to operate only on rendered rows.

</topic>

<topic id="research-basis-and-selected-design" status="active" version="v1" wp="WP-0302" updated_at="2026-08-09">

# Sources checked

- Current Video Archiver React source, subscription commands/schemas, live counts, v0.1.126 screenshot/dump/proof, WP-0280, WP-0281, WP-0282, and WP-0284.
- WAI-ARIA tabs pattern and latency/keyboard guidance: `https://www.w3.org/WAI/ARIA/apg/patterns/tabs/`.
- WAI-ARIA grid/table interaction tradeoffs: `https://www.w3.org/WAI/ARIA/apg/patterns/grid/`.

# Provider-neutral projection

Each subscription summary must expose:

- stable provider + subscription ID,
- provider label/capabilities,
- display title from WP-0300 and source reference,
- active/pause state and durable source lifecycle (`normal`, `unavailable`, `deleted`, or provider-defined equivalent),
- last attempt, last success, classified last failure and required action,
- next eligible/scheduled check and interval,
- canonical queued/running/held/current activity totals,
- canonical discovered/downloaded/available/deleted totals where supported,
- group IDs/names, destination/library summary, session/capability health,
- observation timestamp and loaded-versus-canonical count semantics.

# Workspace layout

- One compact command row: Add subscription, search, provider/status/group filters, sort, Refresh due, Update all, and recurring pause/resume. Secondary/bulk actions use a labeled disclosure/menu.
- Bounded master list appears before optional group administration. Each row shows title, provider, state, last/next result, and active work without card chrome.
- Selected source detail uses accessible tabs: Overview, Media, Activity, Settings.
- Overview: source, status, destination, last/next refresh, current issue/action.
- Media: backend-filtered canonical items with bounded rendering, stable selection, and truthful loaded/total labels.
- Activity: current drain and bounded recent attempts; deep history remains in Jobs.
- Settings: per-subscription interval, output/library, provider capabilities, and active state. Global provider settings link to Options rather than duplicate.
- Group management moves to a compact drawer/dialog/disclosure reachable from the toolbar and source Settings; it never blocks list discovery.

# Canonical action rules

- Bulk refresh/update/pause acts on an explicit canonical backend predicate and returns targeted/eligible/skipped counts.
- Page selection is always labeled `Select loaded`; never imply unseen rows are selected.
- Provider/status/group/search filters execute in the backend before pagination and return exact matching totals.
- Mutating controls are not agent-safe. Read-only headless actions may navigate tabs, filters, disclosures, scroll, and load-more through explicit allowlisted semantics.
- Deleted/unavailable states preserve rows, memberships, media, metadata, and history per current lifecycle authority.

# Provider capability contract

- Capabilities declare supported media types, authentication/session test, refresh interval bounds, cursor/archive behavior, output/library selection, groups, bulk refresh, source status, manual retry, and per-source settings.
- Unsupported controls are absent with an explanatory capability state; they are not disabled clones of YouTube controls.
- Provider failures use the shared classified state type but retain provider-specific details/actions.

# Existing systems reused

- WP-0280 bounded list/render patterns and canonical totals, current subscription groups, source lifecycle, media membership, selected-item deletion/redownload, Jobs activity, module Options navigation, WP-0300 title resolver, headless audit/action bridge.

# Rejected options

- Add Instagram/TikTok as more sections below YouTube: recreates the chaotic long document.
- One universal provider form with every possible field: exposes irrelevant controls and fragile defaults.
- Provider tabs plus detail tabs plus global Quick/Advanced: creates three competing navigation axes.
- Move all subscription settings into Options: per-source interval/destination/status belongs with the selected source.
- Client-side filtering over loaded subscriptions: breaks canonical totals and bulk scope.

</topic>

<topic id="roi-red-team-microtasks-and-proof" status="active" version="v1" wp="WP-0302" updated_at="2026-08-09">

# High-ROI additions

- Provider-neutral projection: reuses canonical counts/lifecycle and makes Instagram/TikTok UI integration cheap without unifying incompatible runtime tables.
- One classified state/action component: reuses Jobs failure state and keeps operator recovery language consistent.
- Global-vs-per-source settings links: reuse WP-0301 and eliminate duplicated controls.
- Stable cross-provider IDs/test IDs: reuse WP-0279 and make future parallel model audits attributable.
- Saved filter/sort state per provider: cheap while filter state already exists and reduces repeated navigation effort.

# Risks, failure scenarios, controls, and verification

- Generic projection loses provider-specific lifecycle detail.
  - Control: common required fields plus typed provider capability/details payload.
  - Verify: YouTube, Instagram, TikTok fixtures with unsupported/different capabilities.
- Bulk action affects visible page only.
  - Control: backend predicate receipt with canonical targeted/eligible/skipped totals.
  - Verify: matches beyond first page and filtered/grouped sets.
- Optional group UI becomes undiscoverable after demotion.
  - Control: toolbar action, group filter, source Settings link, accessible label/help.
  - Verify: fresh-user headless/keyboard discovery path.
- List/detail state resets on panel changes.
  - Control: stable selected provider/subscription ID and bounded cache; reconcile deleted/filtered selections.
  - Verify: navigate away/back, restart, filter-out selected row, deleted/unavailable transitions.
- Too many nested tabs confuse navigation.
  - Control: provider is a filter, not another tab strip; only one selected-source detail tab set.
  - Verify: semantic inventory and 800x600 screenshot.
- Bounded Media list is mistaken for full selection.
  - Control: loaded/canonical labels and `Select loaded` language.
  - Verify: canonical total greater than loaded window.
- Shared component introduces broad regressions across providers.
  - Control: adapter contract tests and provider fixture matrix before replacing existing YouTube UI.
  - Verify: every current YouTube control/workflow mapping and exact 262-row live audit.

# Microtask plan

1. Inventory/map every existing YouTube and Instagram subscription control/action/state.
2. Define provider-neutral projection and capability types with backend contract tests.
3. Implement backend filtering/pagination/canonical action receipts.
4. Build master list/toolbar and selected Overview/Media/Activity/Settings panes without cards.
5. Migrate YouTube first and prove full behavioral parity against 262 live rows.
6. Add provider adapter slots consumed by WP-0303/WP-0304.
7. Add help/manual, accessibility, stable IDs, headless navigation, responsive proof, build, and summary.

# Acceptance and proof gates

- Every existing supported YouTube subscription workflow is mapped and remains reachable.
- Optional groups no longer precede the primary list in the normal flow.
- One bounded canonical projection drives provider/status/group/search/sort and truthful totals.
- Overview/Media/Activity/Settings expose required state and no global provider setting is duplicated per source.
- Bulk and selection semantics are proven against canonical sets beyond the first rendered page.
- YouTube 262-row live case, Instagram fixture/current source, and TikTok adapter fixture pass without mounting unbounded rows.
- Frontend/backend tests, keyboard/accessibility tests, TypeScript/build, governed version/changelog, 800x600 and wide headless audits/actions/snapshots/dumps, and proof `summary.md` pass.

</topic>
