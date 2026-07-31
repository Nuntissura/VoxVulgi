---
file_id: WP-0270-REFINEMENT-v1
file_kind: work-packet-refinement
updated_at: 2026-07-22
---

<topic id="operator-request" status="active" version="v1" wp="WP-0270" updated_at="2026-07-22">

# Operator request

- Single videos and subscription work must be visibly separate tracks rather than an opaque global queue.
- Operators must be able to tell that a large single batch is queued/running while subscriptions continue in the background.
- Instagram, other video services, and Localization Studio must have equally clear independent state.

# Stable workflow requirement

- Controls must change the settings the scheduler actually reads.
- Counts and track state come from the canonical job store, never from the currently rendered page.
- The shared YouTube start gate must be explained separately from independent track budgets so safe start staggering is not mistaken for starvation or total transfer serialization.

</topic>

<topic id="verified-current-state" status="active" version="v1" wp="WP-0270" updated_at="2026-07-22">

# Verified current state

- Jobs/Queue still renders one `Concurrency` input and calls `jobs_runtime_settings_set(maxConcurrency)`.
- The engine explicitly documents that legacy `jobs_max_concurrency` no longer drives scheduling; per-lane keys do. The visible control can therefore report success without changing runner behavior.
- The current Jobs overview already separates canonical status totals from bounded rows and has `Now`, `Needs attention`, and `History`; this packet must extend that contract rather than reintroduce full-history polling.
- Current frontend context derivation still labels every direct URL without a YouTube `subscription_id` as `Single video`, which is too broad for Instagram and other providers.
- The headless bridge exposes page/state/snapshot/dump routes but not canonical scheduler track/gate state.

</topic>

<topic id="spec-and-research-basis" status="active" version="v1" wp="WP-0270" updated_at="2026-07-22">

# Spec anchors

- `PRODUCT_SPEC` Jobs/Queue requirements demand current-work-first canonical totals, explicit failure/loading/empty states, lineage, and safe actions.
- `TECHNICAL_DESIGN` Jobs overview requirements prohibit rendered-subset counts and per-row hydration.
- `build_rules.md` requires no new cards, quiet headless verification, readable/discoverable controls, and real app-boundary proof.
- `WP-0255` already reserved per-lane Options UI; this packet replaces that stale three-lane/global-control concept with the canonical track contract from `WP-0269`.

# Research basis

- Hugging Face Jobs' current UI/CLI defaults to active work and requires explicit `--all`, filters by status/labels, and exposes inspect/logs/stats as separate paths: https://huggingface.co/docs/hub/jobs-manage
- qBittorrent's current WebUI API exposes bounded transfer lists, filters/categories, canonical state, and separate controls rather than deriving categories from visible paths: https://github.com/qbittorrent/qBittorrent/wiki/WebUI-API-%28qBittorrent-4.1%29
- Celery monitoring/control documentation separates queues, worker concurrency, and rate limits, reinforcing distinct labels for product-track budgets and shared-provider gates: https://docs.celeryq.dev/en/stable/userguide/monitoring.html, https://docs.celeryq.dev/en/stable/userguide/workers.html
- Existing VoxVulgi Jobs overview, failure classifier, trace JSONL, and agent bridge were inspected as the lowest-risk reusable implementation surfaces.
- GitHub, Hugging Face, Reddit, Civitai, and X searches found no stronger reason to replace the current no-card Jobs layout or canonical overview API.

# Selected approach

- Replace the ineffective global concurrency control with a runtime contract containing actual per-track budgets, pause/hold state, canonical queued/running totals, and shared provider-gate state.
- Render a compact track filter/status strip and advanced controls inside the existing Jobs toolbar/detail structure; add no card.
- Label every job from its persisted canonical track. Show `YouTube single`, `YouTube background`, `Instagram`, `Other video`, `Image Archive`, or `Localization` consistently across all job states and enqueue receipts.
- Explain that the two YouTube tracks are independently queued and may overlap in transfer execution, while every aggregate YouTube process start passes through one shared safe 5-10 second pacing/auth gate.
- Add a localhost-only read-only bridge route for canonical track/gate state and include stable element IDs in snapshots/dumps.
- Keep polling bounded with one aggregate SQL query and indexes introduced by `WP-0269`.

# Rejected options

- Keeping the global control with explanatory copy: it remains functionally false.
- Computing track totals by grouping loaded rows: repeats the visible-subset truth defect.
- Adding one card per track: violates the no-new-cards policy and makes a large-track screen harder to scan.
- Exposing raw anti-bot internals as the primary UI: safe defaults and plain-language status lead; technical values stay in advanced detail.

</topic>

<topic id="scope-and-acceptance" status="active" version="v1" wp="WP-0270" updated_at="2026-07-22">

# Base scope

- Add canonical engine runtime-state get/set contracts for real track budgets and read-only gate state.
- Add indexed canonical per-track queued/running/status totals to the existing bounded Jobs overview.
- Replace the global concurrency UI with truthful per-track advanced controls.
- Add track filter/labels/status and shared YouTube gate explanation without new cards.
- Include canonical track in enqueue receipts and frontend context summaries.
- Add structured trace events and a read-only bridge endpoint for no-context model inspection.

# High-ROI additions

- Stable `data-testid`/element IDs for each track and gate make parallel agent verification deterministic.
- Track state in enqueue receipts lets the operator prove routing immediately without waiting for Jobs polling.
- Explicit preview-vs-canonical wording prevents the prior count confusion from recurring in new filters.

# Gaps closed

- The UI can no longer claim a concurrency setting changed when it did not.
- Operators can distinguish queued background subscriptions from pasted single work.
- Provider gate holds are visible without being confused with a stalled track.
- Models can inspect scheduler truth without foreground input or fragile screen reading.

# Acceptance criteria

- Changing each exposed track budget changes the exact engine key read by the runner and survives restart.
- The Jobs strip reports canonical per-track totals independently of preview size, search, filter, and pagination.
- Enqueue receipts and every job state use the persisted track label; Instagram/other/image jobs are never called `Single video`.
- The shared YouTube gate shows ready/waiting/held/next-eligible state and explains safe process-start staggering while both independently runnable track queues remain visible.
- `/agent/jobs_tracks` or an equivalently explicit read-only bridge route returns the same canonical totals/settings/gate state as the UI.
- Headless snapshots/dumps prove readability, discoverability, coherent navigation, visible important state, responsive layout, and no new cards.

</topic>

<topic id="red-team" status="active" version="v1" wp="WP-0270" updated_at="2026-07-22">

# Red team

- Risk: per-track controls imply unpaced YouTube parallelism. Control: display active track budgets separately from the shared start/pacing gate; do not expose an unsafe bypass.
- Risk: aggregate polling adds DB contention over 55,000 queued rows. Control: one indexed grouped query, bounded cadence, trace timing, and representative p95 proof.
- Risk: filtering hides active work and appears empty. Control: canonical strip remains visible, filtered preview count is labeled separately, and empty/error/loading states remain distinct.
- Risk: a bridge endpoint blocks when the WebView is frozen. Control: use bounded read-only DB access on the bridge thread with a short timeout and explicit error response.
- Failure scenario: settings update partially succeeds. Control: validate all inputs, write in one transaction, return the reread canonical settings, and retain the prior UI state on error.
- Failure scenario: old jobs have unknown track. Control: display `Unclassified` with canonical count and repair/backfill state; never guess from a rendered path.

</topic>
