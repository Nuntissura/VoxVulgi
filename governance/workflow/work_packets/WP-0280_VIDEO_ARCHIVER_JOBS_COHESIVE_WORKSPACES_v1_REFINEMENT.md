---
file_id: WP-0280-REFINEMENT-v1
file_kind: work-packet-refinement
updated_at: 2026-07-27
---

<topic id="operator-request" status="active" version="v1" wp="WP-0280" updated_at="2026-07-27">

# Operator request

- Visually inspect every element and navigate every feature of Video Archiver and Jobs/Queue.
- Use or extend VoxVulgi's own quiet inspection tools; correlate the review with internal Diagnostics and `vvwatch`.
- Create one work packet for all findings, update the taskboard, and implement the complete result autonomously.
- Simplify both pages into a cohesive layout without losing existing workflows, source identity, canonical queue truth, or imported/current library unification.
- Treat the operator machine as heavily loaded; do not close, focus, or reconfigure anything the agent did not start.
- Use Firefox only if authenticated browser testing becomes necessary.

</topic>

<topic id="spec-anchors" status="active" version="v1" wp="WP-0280" updated_at="2026-07-27">

# Spec anchors

- `governance/spec/PRODUCT_SPEC.md` — Video Archiver workflow selector, subscription master-detail, Jobs current-work hierarchy, canonical filtering, bounded rendering, and no visible imported/current distinction.
- `governance/spec/TECHNICAL_DESIGN.md` — canonical overview/track projection, bounded render-window contract, panel-local scrolling, granular refresh, diagnostics, and headless audit routes.
- `build_rules.md` — no new cards; quiet app-boundary visual/navigation proof; semantic-versioned governed desktop builds.
- `governance/workflow/PROOF_STANDARD.md` — build-only proof is insufficient for operator-heavy UI work.
- `WP-0255` — shipped Video Archiver behavior and unresolved simplification requirements carried forward.
- `WP-0256` — shipped Jobs readability and canonical `Now`/`Needs attention`/`History` baseline; preserve, do not supersede.
- `WP-0258` — shipped bounded Jobs read path and unresolved contention/render-performance requirements carried forward.
- `WP-0278` — panel-switch and external-watcher hardening to reuse.
- `WP-0279` — semantic headless audit/action bridge and the audit evidence that defines this packet.

</topic>

<topic id="inspection-findings" status="verified" version="v1" wp="WP-0280" updated_at="2026-07-27">

# Verified inspection findings

Evidence source: packaged desktop v0.1.116, headless semantic inventory/actions, snapshots/dumps, internal trace, and `vvwatch`; summary at `product/desktop/build_target/tool_artifacts/wp_runs/WP-0279/final_proof/summary.md`.

## Shared shell

- At the inspected 800x600 viewport, shell chrome consumes about 187 pixels, roughly 31% of the available height, before page work begins.
- This amplifies every page-local height problem. A compact-shell pass is useful but must preserve the frameless move and window controls.

## Video Archiver

- Quick/Advanced competes with the real source tabs and produces contradictory state: Subscriptions are already available in Quick while explanatory copy still says Advanced is required.
- The effective workflow selectors are currently `YouTube single`, `YouTube playlist/subscription`, and `Website`; their wording and surrounding fields are not cohesive.
- Active-library destination, library administration, source selector, presets, recurring controls, migration, history, source list, and source detail all compete in one long document.
- Presets can begin roughly 6,200 pixels below completed history, so a setup control is physically separated from the workflow it affects.
- The subscription view mixes create/edit forms, bulk queue controls, imported-archive migration, list selection, and selected-source inspection.
- The selected subscription's pending and downloaded video arrays render without a window or page bound. Large sources create extreme document height and DOM volume.
- Website mode still contains YouTube-specific labels.

## Jobs/Queue

- The unexpanded `Now` document measured about 48,574 pixels; `History` about 55,644 pixels.
- Expanding a 113-member batch produced about 99,783 pixels and 1,834 semantic elements because every child row mounted into the document.
- The page has two primary tab systems (`Now`/`Needs attention`/`History` plus eight source-track tabs) and an always-visible per-track status wall before the work table.
- Tables measured roughly 972–1,598 pixels wide in an 800-pixel viewport, separating source context from row actions.
- Queue controls, source scope, scheduler health, shared YouTube gate, filters, cleanup, developer tools, work rows, and batch detail do not form one clear hierarchy.

## Diagnostics and contention

- The final 90-second v0.1.116 watcher run recorded 39 samples with no not-responding sample, bridge failure, DB timeout, path timeout, or incomplete command.
- Slow reads still occurred under load: `subscription_download_activity` reached about 5.5 seconds, `item_outputs_many` about 2.4 seconds, and multiple Jobs reads exceeded one second.
- The current evidence therefore does not prove a WebView hard freeze; it proves expensive page construction and slow read overlap that can make navigation and actions feel frozen on a loaded host.

</topic>

<topic id="research-basis" status="verified" version="v1" wp="WP-0280" updated_at="2026-07-27">

# Research basis

## Current sources checked

- TanStack Virtual introduction and repository: headless windowing renders only the visible subset of large lists/tables while retaining markup control. Sources: https://tanstack.com/virtual/v3/docs/introduction and https://github.com/TanStack/virtual
- W3C ARIA Authoring Practices tabs pattern: one tab list represents one layered content selector; selected state and panel association must be explicit. Source: https://www.w3.org/WAI/ARIA/apg/patterns/tabs/
- W3C ARIA Authoring Practices listbox pattern: dynamically loaded/windowed sets must expose set size and position, and an option should not contain nested interactive controls. Source: https://www.w3.org/WAI/ARIA/apg/patterns/listbox/
- W3C table pattern: interactive widgets inside a table each add a tab stop, reinforcing the cost of dense action-heavy tables. Source: https://www.w3.org/WAI/ARIA/apg/patterns/table/
- qBittorrent WebUI API: bounded filters and revision-based partial synchronization separate live transfer changes from full-history hydration. Source: https://github.com/qbittorrent/qBittorrent/wiki/WebUI-API-%28qBittorrent-4.1%29
- Hugging Face Jobs: structured current status, inspect, logs, metrics, list, and cancel are distinct operations rather than one giant always-expanded document. Sources: https://huggingface.co/docs/huggingface_hub/package_reference/jobs and https://huggingface.co/docs/hub/jobs
- GitHub Actions current-jobs documentation: active and queued work are presented as explicit operational sets. Source: https://docs.github.com/en/actions/how-tos/using-github-hosted-runners/viewing-your-current-jobs
- Blueprint virtualization RFC: paging/incremental loading and virtualization are distinct; either can bound DOM work, while unbounded infinite append does not. Source: https://github.com/palantir/blueprint/wiki/Blueprint-Virtualization
- SQLite EXPLAIN QUERY PLAN and query-planner documentation: `SCAN` identifies a full-table/index scan while `SEARCH` visits an indexed subset; measured plans must drive separation of secondary full scans from primary navigation queries. Sources: https://sqlite.org/eqp.html and https://www.sqlite.org/queryplanner.html
- The WP-0258 research pass also checked Gradio, ComfyUI, Civitai, Reddit, X/Twitter, SQLite official planner/WAL material, issues, and implementation discussions. No stronger directly reusable Civitai/Reddit/X implementation was found than the official and source-level contracts above.

## Reuse opportunities

- Keep the canonical `jobs_overview` selected-track query and canonical totals; change only the presentation and rendered window.
- Keep `Now`, `Needs attention`, and `History`, job group expansion, retry lineage, source snapshots, and existing actions from WP-0256/WP-0271.
- Keep the Video Archiver master-detail manager, activity projection, library manager, presets, source groups, and imported-archive reconciliation.
- Keep the WP-0274 granular refresh cadences, WP-0278 trace/watcher signals, and WP-0279 headless semantic audit/actions.
- Use native `<select>`, `<details>`, stable IDs, array slicing, and existing CSS before adding dependencies.

## Selected approach

- Give Video Archiver one primary source selector: `Single videos`, `Subscriptions`, `Other websites`; remove its competing Quick/Advanced selector while retaining that mode on unrelated archive pages.
- Turn destination/library state into a compact strip and keep library administration disclosed.
- Move presets and workflow-specific options next to the workflow they affect.
- Keep subscription list/detail, but give selected-source pending/downloaded lists a fixed initial render window, local scroll, explicit `shown of total`, and fixed-size `Load more`.
- Give Jobs one compact command row, one `Now`/`Needs attention`/`History` selector, one source filter, and one secondary scheduler-health disclosure.
- Give the Jobs table a panel-local scroll surface. Expanded batch members receive their own explicit render window and `Load more`.
- Preserve canonical backend filtering/counts; rendered rows are never used as proof of the full set.
- Improve narrow-width readability through responsive column reduction and detail paths rather than introducing another horizontal navigation layer.
- Continue tracing panel/action durations and run `vvwatch` during app-boundary verification.
- Keep the canonical Single videos count/page on its indexed lineage path and load the exact full-library unclassified-legacy count through an independent read-only command so a cold diagnostic scan cannot delay page navigation.

## Rejected options

- Add a virtualizer dependency immediately: field-aligned, but it changes the offline dependency payload and accessibility/measurement surface. Reject until simple bounded windows are measured and proven insufficient.
- Pure CSS max-height only: shortens the document but still mounts thousands of nodes and keeps rerender cost.
- Replace canonical tables with decorative cards: violates `build_rules.md` and increases visual fragmentation.
- Merge Jobs history into current work: regresses the current-work-first contract and increases hot-path data.
- Hide rows without a count or expansion path: violates canonical-set truth and makes audit/recovery unreliable.
- Change scheduler/query semantics as part of the layout pass: unnecessary for the first closure unit and risks job correctness.

</topic>

<topic id="scope-and-phases" status="active" version="v1" wp="WP-0280" updated_at="2026-07-27">

# Scope and phased implementation

## Phase A — Video Archiver hierarchy and bounded source detail

- Remove Video Archiver's Quick/Advanced switch and contradictory help.
- Rename and semantically connect the three source tabs/panels.
- Compact destination/library state and keep administrative actions secondary.
- Put presets/options in contextual disclosures before the history/list they affect.
- Correct source-specific copy in Other websites.
- Window pending/downloaded selected-subscription rows with truthful totals and `Load more`.

## Phase B — Jobs command hierarchy and source filtering

- Keep primary queue state, Pause/Resume, and Refresh in one compact command row.
- Preserve `Now`, `Needs attention`, and `History`.
- Replace the eight-item source tab rail with one labeled native source filter backed by the same canonical selector.
- Combine per-track totals and shared YouTube gate into one scheduler-health disclosure.
- Keep advanced filters, cleanup, and developer tools secondary and correctly labeled.

## Phase C — Jobs local scrolling and bounded expansion

- Use a panel-local work-list scroll surface.
- Render a fixed initial child-row window for expanded batches and expose `shown of total` plus fixed-step `Load more`.
- Reset/trim windows when view, track, filter, or group state changes.
- Preserve collapsed-group title/source/progress/actions and canonical batch health.
- Reduce narrow-width table columns while retaining full context through row detail.

## Phase D — shared height/responsiveness and diagnostics

- Compact only the shell spacing that can be changed without weakening move/window controls.
- Avoid whole-page loading state on bounded refreshes.
- Trace render/action/read duration and overlap for the touched surfaces.
- Expand `vvwatch` only if current samples cannot distinguish layout/rerender pressure from command wait, DB contention, NAS delay, or WebView/compositor failure.

## Non-goals

- No destructive media, library, subscription, playlist, imported database, or NAS cleanup.
- No job-engine, canonical identity, dedupe, retry-lineage, or scheduler rewrite.
- No changes to Instagram/Image Quick/Advanced behavior unless a shared component change requires an equivalent regression fix.
- No credentials or browser session work unless an exact runtime flow requires it; Firefox is the only permitted browser in that case.
- No new cards.

</topic>

<topic id="roi-risks-and-verification" status="active" version="v1" wp="WP-0280" updated_at="2026-07-27">

# ROI additions, gaps, risks, and verification

## High-ROI additions

- Add truthful `shown of total` metadata to every bounded list. This is cheap because totals already exist, closes the rendered-vs-canonical ambiguity, and supports humans plus headless agents. Verify at initial and expanded windows.
- Reset list windows on source/filter changes. This prevents stale large DOM windows and confusing carry-over, reusing existing selected-source/view state. Verify across all tabs and source filters.
- Preserve stable row IDs and audit semantics. This reuses WP-0279, makes later regressions cheaper to inspect, and prevents brittle coordinate automation. Verify through `/agent/ui_audit` and `/agent/ui_action`.
- Record render-window size and interaction duration in existing traces when slow. This reuses Diagnostics, helps distinguish data waits from DOM work, and makes future freeze diagnosis cheaper. Verify trace payload bounds and redaction.
- Make local scroll ownership visible and keyboard-accessible. This closes dead-wheel/unreachable-action behavior without adding surfaces. Verify wheel/keyboard semantics and screenshots at 800x600 and a wider viewport.
- Keep the implementation compatible with a future true virtualizer. This reduces rework if measured row volume still exceeds the bounded-window design. Verify that row rendering is isolated and keyed.

## Risks and failure scenarios

- Risk: bounded rendering is mistaken for bounded backend state.
  - Control: always show canonical total separately; all bulk actions and filters continue to call canonical backend commands.
  - Verification: compare visible counts with backend totals in state dump and exact query.
- Risk: `Load more` grows without bound after repeated use.
  - Control: panel-local scroll plus fixed increments; reset on selection/filter/view change; trace high rendered counts.
  - Verification: repeated expansion, navigation away/back, and memory/semantic-element counts.
- Risk: removing Quick/Advanced hides existing preset, recurring, migration, or import controls.
  - Control: map every existing control to a named source workflow or secondary disclosure before deletion.
  - Verification: pre/post semantic inventory diff and safe-navigation receipts.
- Risk: replacing source tabs changes backend query scope.
  - Control: retain the existing canonical track value/type and setter; only replace its presentation.
  - Verification: inspect bridge dump and backend overview receipt for every source value.
- Risk: scheduler health becomes undiscoverable.
  - Control: disclosure summary includes active/held/error signal; full canonical detail opens in place.
  - Verification: snapshots for healthy, paused, held-provider, and error states where fixtures permit.
- Risk: compact shell breaks frameless drag/move/window controls.
  - Control: do not change control identity or drag-region ownership; test move/control hit areas through existing shell tests and headless geometry.
- Risk: responsive column hiding removes recovery context.
  - Control: only hide duplicated secondary columns at narrow width; full source/path/IDs remain in details.
  - Verification: 800x600 and wide snapshots plus semantic inventory.
- Risk: slow reads remain and the layout merely masks them.
  - Control: correlate app trace command timings with out-of-process watcher samples before and after; retain prior data on refresh errors.
  - Verification: representative headless run with internal trace and `vvwatch`.
- Risk: a secondary legacy/unclassified diagnostic scan delays the canonical Single videos page under host or storage contention.
  - Control: split the exact scan into an independent read-only command, expose loading/unavailable state, and keep canonical paging usable while it runs.
  - Verification: trace both command IDs on the live database and prove the canonical history command completes independently of the secondary count.
- Risk: a headless run starts or mutates operator jobs.
  - Control: retain WP-0279 headless mutation gates and runner/startup-sync skips.
  - Verification: startup trace and canonical queue counts before/after.

## Acceptance criteria

- Video Archiver has one primary workflow selector and no Video-Archiver Quick/Advanced toggle.
- Every pre-change Video Archiver workflow/control remains reachable in its correct workflow or a labeled secondary disclosure.
- Selected subscription pending/downloaded lists initially mount a bounded number of rows, show truthful canonical totals, load more deterministically, and reset on subscription change.
- Jobs keeps `Now`, `Needs attention`, and `History`, but uses one source filter and one scheduler-health disclosure instead of an eight-tab rail plus always-expanded track wall.
- Expanded Jobs groups initially mount a bounded number of child rows, show `shown of total`, load more deterministically, and never redefine canonical group/batch counts.
- At 800x600 and a wider viewport, both pages keep primary controls, source identity, state, progress, and actions readable without document-scale horizontal navigation.
- Semantic inventory confirms no existing safe workflow was orphaned and large-state element/document-height counts are materially reduced from the v0.1.116 baseline.
- Frontend tests, TypeScript/build checks, relevant Rust tests, governed desktop build, semantic-version bump, changelog entry, headless snapshots/dumps/actions, internal Diagnostics trace, and concurrent `vvwatch` satisfy the proof standard.
- No operator media, subscriptions, playlists, library metadata, imported app database, or unrelated process is modified or deleted.

</topic>
