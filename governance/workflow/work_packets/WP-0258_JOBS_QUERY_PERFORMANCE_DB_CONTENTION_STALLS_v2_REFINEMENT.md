---
file_id: WP-0258-REFINEMENT-v2
file_kind: work-packet-refinement
updated_at: 2026-07-16
---

<topic id="operator-request" status="active" version="v2" wp="WP-0258" updated_at="2026-07-16">

# Operator request

- Exact failure: submitting `https://youtu.be/dNUkrrqmwug?si=eiqBo7PBu5gDkzk8` from YouTube single reports `Queued 1 download job`, but Jobs/Queue shows `No jobs yet`.
- Stable requirement: a successful single-video enqueue must create and immediately surface a canonical attempt; Jobs must show the operator's current work without waiting on unrelated history hydration.
- The existing durable job store, retry lineage, media, subscriptions, playlists, and library metadata must be preserved.

</topic>

<topic id="spec-anchors" status="active" version="v2" wp="WP-0258" updated_at="2026-07-16">

# Spec anchors

- `governance/spec/PRODUCT_SPEC.md` UX Principles: URL queueing returns quickly and does not block the UI.
- `governance/spec/PRODUCT_SPEC.md` Jobs/Queue: current execution state, retry/cancel, logs/output navigation, canonical batch truth, and latest-attempt-first display.
- `governance/spec/TECHNICAL_DESIGN.md` downloader: `job` is the canonical durable attempt store; visible rows are projections and may not define backend truth.
- `build_rules.md`: no new cards; touched UI must reduce competing surfaces and must be verified through the real app boundary.

</topic>

<topic id="research-basis" status="active" version="v2" wp="WP-0258" updated_at="2026-07-16">

# Research basis

## Repo and runtime evidence

- Installed desktop `0.1.91` was inspected through the headless bridge on 2026-07-16.
- Video Archiver visibly reported one queued job while Jobs/Queue rendered an empty table.
- The live canonical database is local at `%APPDATA%/com.voxvulgi.voxvulgi/db/app.sqlite`; it is approximately 379 MB with a roughly 56 MB WAL during the incident.
- `jobs_list_live_snapshot` opens one read connection, selects thousands of rows, opens a second connection through `list_jobs`, and then calls `hydrate_job_target_titles`.
- `hydrate_job_target_titles` performs one `library_item` lookup and a possible provenance lookup for each distinct direct-download URL, including rows that already have a persisted title. The lookup predicates are not indexed in schema v21.
- An independent read-only exact-ID query and the installed app's `jobs_list_live` both failed to complete during the inspection window. The bridge and WebView stayed responsive, which separates a slow Jobs projection from an app-wide UI-thread freeze.

## Current field patterns checked

- SQLite query planner documentation: table scans are proportional to table size; indexed lookup and search-plus-sort indexes are the intended remedy. Source: https://www.sqlite.org/queryplanner.html
- SQLite `EXPLAIN QUERY PLAN` documentation: query-plan inspection is the supported way to prove index use. Source: https://www.sqlite.org/eqp.html
- SQLite WAL documentation: WAL improves reader/writer concurrency but does not make expensive projections cheap. Source: https://www.sqlite.org/wal.html
- qBittorrent WebUI API: active-state filters, bounded list parameters, structured progress, and revision-based partial synchronization (`rid`) keep the transfer view current without rebuilding all history. Source: https://github.com/qbittorrent/qBittorrent/wiki/WebUI-API-%28qBittorrent-4.1%29
- Hugging Face Jobs: the default list shows running/scheduling work, while `--all` is explicit; status filters, inspect, logs, and metrics are separate paths. Source: https://huggingface.co/docs/hub/jobs-manage
- Gradio client queue contract: queue rank, size, ETA, progress units, state, and final result are structured updates rather than inferred UI text. Source: https://github.com/gradio-app/gradio/blob/main/client/python/gradio_client/utils.py
- ComfyUI queue/history contract and issue discussion: enqueue returns a prompt/job identifier, live progress is separate from history, and the identifier is the tracking handle. Source: https://github.com/Comfy-Org/ComfyUI/issues/6607
- Civitai's open repository, Reddit, and X searches were checked for adjacent queue-monitor patterns; no stronger directly reusable primary implementation evidence was found than the sources above.

## Selected approach

- Preserve the canonical `job` table and execution engine.
- Replace the initial Jobs read with one bounded, requested-view overview query on one read-only connection; polling `Now` must not also fetch attention and history rows. Compute canonical totals with indexed equality counts and defer canonical batch-detail aggregation until a batch is explicitly expanded.
- Return canonical status counts separately from bounded row previews so the UI never presents loaded-row counts as full backend state.
- Remove live per-row database title hydration from the overview path. Use persisted `target_title`, item batches, and URL/video-ID fallback; missing historical titles remain an explicit repair path.
- Add the missing lookup indexes for the slower inspection/search paths and prove them with focused migration/query tests.

## Rejected options

- Replacing the execution engine or deleting/rebuilding job history: high data and lineage risk; unnecessary for the proven read-projection defect.
- Increasing busy timeouts alone: hides the cost and makes the blank screen last longer.
- Loading every queued/history row: contradicts bounded, current-work-first field patterns and repeats the existing failure.
- Client-only filtering of a huge snapshot: preserves the expensive backend read and misstates visible-subset counts.

</topic>

<topic id="scope-and-acceptance" status="active" version="v2" wp="WP-0258" updated_at="2026-07-16">

# Scope

- Add a bounded Jobs overview contract with canonical status totals and newest active/attention/recent rows.
- Use one read-only connection and no nested `list_jobs` call.
- Do not invoke title hydration on the overview path.
- Add indexes supporting title fallback inspection by `library_item.source_uri` and `ingest_provenance.source_url`.
- Keep legacy list/search/detail APIs compatible, but make title hydration skip already-titled rows.

# Non-goals

- No destructive data migration.
- No reset or deletion of job history, library metadata, subscriptions, playlists, or media.
- No scheduler/lane rewrite unless exact runtime proof shows the new single attempt was not dispatched after the read path is fixed.

# Acceptance criteria

- Jobs initial overview returns within a bounded target on a representative large database and does not execute per-URL title queries.
- Canonical queued/running/succeeded/failed/canceled totals are distinct from returned preview lengths.
- A newly enqueued single-video job appears in the current-work preview by its returned job ID or exact video ID.
- Empty, loading, and failed-query states are distinct; query failure may not render as `No jobs yet`.
- Existing retry lineage and canonical batch-health tests remain green.

</topic>

<topic id="red-team" status="active" version="v2" wp="WP-0258" updated_at="2026-07-16">

# Red team

- Risk: a bounded preview hides an older active job. Control: order active rows newest-first, return canonical totals, and retain explicit search/detail paths.
- Risk: removing live hydration loses historical titles. Control: render persisted title first, URL/video ID fallback second, and keep explicit title repair/backfill.
- Risk: index creation delays first startup on a large database. Control: only add two targeted indexes, test migration against a copied fixture, and expose build/runtime timing evidence.
- Risk: a successful enqueue receipt can still lie if persistence fails. Control: construct the receipt only from returned persisted `JobRow` values and show the job IDs.
- Failure scenario: overview command errors or times out. Control: retain previous rows, show an explicit refresh error, and never replace them with an empty-state claim.

</topic>
