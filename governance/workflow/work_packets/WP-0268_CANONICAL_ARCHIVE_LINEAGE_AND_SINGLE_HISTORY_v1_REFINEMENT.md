---
file_id: WP-0268-REFINEMENT-v1
file_kind: work-packet-refinement
updated_at: 2026-07-22
---

<topic id="operator-request" status="active" version="v1" wp="WP-0268" updated_at="2026-07-22">

# Operator request

- Subscription downloads must never appear in the downloaded-single-video preview.
- The single-video surface must keep supporting large one-off URL batches, primarily YouTube, while subscriptions continue in the background.
- Inspect the canonical source entities before changing the projection; do not treat a rendered preview or path convention as backend truth.

# Stable workflow requirement

- A row is a downloaded single video only when durable lineage identifies it as a one-off single-video ingest.
- Subscription, playlist, channel, Instagram, other-service, image-archive, and localization outputs remain available in their owning surfaces and the Media Library, but cannot enter the single-video projection by URL/path guessing.
- Older records that cannot be classified from durable evidence remain preserved and explicitly unclassified; they are not guessed into the single-video list.

</topic>

<topic id="verified-current-state" status="active" version="v1" wp="WP-0268" updated_at="2026-07-22">

# Verified current state

- The installed desktop `0.1.101` and its live schema-v22 database were inspected on 2026-07-22 through the headless bridge and read-only SQLite queries.
- `library_list_youtube_video_candidates` in `product/engine/src/library.rs` returns a broad page of all video-shaped YouTube library items.
- `filterYoutubeSingleVideoItems` in `product/desktop/src/lib/archiverRuntime.ts` then guesses single-vs-subscription state from URL and output-path/container heuristics.
- In the newest 200 returned candidates, 197 were canonically linked through `job.item_id` to subscription child jobs whose params contained `subscription_id` and whose lane was `recurring`.
- Those subscription outputs used mapped NAS destinations that did not contain a `subscriptions` path segment, while their per-video source URI was a normal YouTube watch/shorts URL. That combination defeats the frontend heuristic and causes the leak.
- Known later one-off single-video jobs exist and succeeded, so the corrective query must retain provable singles instead of hiding all YouTube downloads.
- `WP-0247` remains in review and introduced the broad candidate query; this packet is its focused corrective successor rather than a rewrite of unrelated recovery work.

</topic>

<topic id="spec-and-research-basis" status="active" version="v1" wp="WP-0268" updated_at="2026-07-22">

# Spec anchors

- `governance/spec/PRODUCT_SPEC.md` sections 4.1 and 8 require provenance, explicit container semantics, and trustworthy Jobs/Queue lineage.
- `governance/spec/TECHNICAL_DESIGN.md` sections 3 and 5.6 define `library_item`, `ingest_provenance`, canonical jobs, and downloaded-library handoff.
- The temporary technical-design allowance for frontend URI/path inference conflicts with the verified mapped-output failure and must be narrowed.
- `VV-SOT-001` through `VV-SOT-008` require canonical entity proof and exact-case verification.

# Research basis

- SQLite query-planner, expression-index, WAL, and transaction documentation was checked for additive indexed lookup and migration behavior: https://www.sqlite.org/queryplanner.html, https://www.sqlite.org/expridx.html, https://www.sqlite.org/wal.html, https://www.sqlite.org/lang_transaction.html
- qBittorrent's current WebUI contract was checked as an adjacent mature transfer UI: canonical transfer/category fields and bounded/filterable projections are kept separate from the rendered subset: https://github.com/qbittorrent/qBittorrent/wiki/WebUI-API-%28qBittorrent-4.1%29
- Hugging Face Jobs' current management contract was checked for durable IDs, explicit statuses, filters, inspection, and logs rather than filename-derived identity: https://huggingface.co/docs/hub/jobs-manage
- Current GitHub, Hugging Face, Reddit, X, and Civitai searches were checked for adjacent queue/lineage patterns. No stronger project-appropriate source superseded the repo's existing canonical `job.item_id` plus `ingest_provenance` relationship.

# Selected approach

- Add an additive `library_download_lineage` table keyed by `item_id`, with indexed `service`, `origin_kind`, `work_track`, `source_job_id`, optional `source_batch_id`, optional `source_subscription_id`, and item/lineage timestamps. `source_job_id` remains a durable string rather than a deleting foreign key because terminal job cleanup must not erase item origin.
- Write lineage at the successful download-to-library handoff from execution context, not by re-parsing the output path later.
- Backfill only from durable evidence: successful job-to-item links plus exact structured job params. Use deterministic precedence when retries point to the same item; leave ambiguous or unmatched historical rows unclassified.
- Replace the broad single-history command with a backend canonical single-only query. Return canonical totals separately from the bounded page.
- Keep unclassified historical items in the Media Library and expose an inline unclassified count/description in the existing single-history area without adding a card.
- Keep service, origin, and execution scheduling as separate dimensions. For example, a manually submitted YouTube playlist is `service=youtube`, `origin_kind=playlist`, and `work_track=youtube_single`: it receives foreground scheduling but its members do not masquerade as individual one-off singles.

# Rejected options

- More path/URL rules: mapped destinations and per-video watch URLs have already disproved this source of truth.
- Hiding every subscription-looking folder: deletes visibility for legitimate one-off downloads and still fails custom paths.
- Deleting/reimporting library rows or rebuilding subscriptions: unnecessary and violates data-preservation requirements.
- Depending only on live job rows at read time: job cleanup would later erase the classification; lineage must survive independently in provenance.

</topic>

<topic id="scope-and-acceptance" status="active" version="v1" wp="WP-0268" updated_at="2026-07-22">

# Base scope

- Add an additive schema migration and indexes for durable ingest lineage.
- Persist lineage for new successful URL downloads across single, recurring, Instagram, and other-service paths.
- Backfill classifiable historical rows in bounded, resumable batches without deleting or moving media.
- Add a bounded canonical single-video history query with canonical total and unclassified total.
- Remove single-history and Media Library `Single videos` filter dependence on `filterYoutubeSingleVideoItems`, `isSingleVideoLibraryItem`, and path-derived container classification.
- Preserve search, sort, paging, open, and reveal behavior in the existing no-new-card surface.

# High-ROI additions

- Reuse the same service/origin/work-track dimensions later used by `WP-0269` scheduling and `WP-0270` queue visibility, avoiding competing frontend guesses.
- Include source job/batch/subscription IDs in diagnostic output so humans and models can reproduce a projection decision cheaply.
- Add a migration/backfill progress receipt so a large live library never looks silently incomplete.

# Gaps closed

- Mapped NAS subscription outputs can no longer masquerade as one-off singles.
- Job cleanup no longer destroys the only available origin association.
- Visible row count is no longer presented as the canonical total.
- Uncertain legacy state becomes explicit rather than guessed.

# Acceptance criteria

- The exact inspected newest-200 case returns zero subscription-child items in the single-video result.
- The Media Library `Single videos` filter also excludes mapped subscription/playlist/channel and unknown rows from canonical single results.
- Known July one-off single-video jobs that have successful item links remain present.
- YouTube watch, shorts, live, playlist, channel, mapped-NAS subscription, retry, and unknown-legacy fixtures classify according to durable lineage, not path text.
- A successful new download writes provenance lineage before the UI can list it.
- Backfill is idempotent, bounded, resumable, and preserves every library item, subscription, playlist, job, and media path.
- Search/sort/paging and canonical totals are backend-defined and covered by focused tests.

</topic>

<topic id="red-team" status="active" version="v1" wp="WP-0268" updated_at="2026-07-22">

# Red team

- Risk: schema migration/backfill contends with the approximately 537 MB live database. Control: additive columns/indexes, short transactions, bounded batches with yields, a copied-database migration test, and a timestamped backup before exact live proof.
- Risk: retry attempts or duplicate jobs point to one item with conflicting params. Control: accept only unambiguous structured lineage, use a documented deterministic successful-attempt precedence, and otherwise retain `unclassified`.
- Risk: a legacy real single lacks a surviving job. Control: keep it in Media Library and report it as unclassified; never infer it into subscriptions or delete it.
- Risk: a new success inserts the item but crashes before lineage is written. Control: make item/provenance/lineage handoff transactional where practical and add a repair path for item-linked successful jobs.
- Failure scenario: frontend accidentally re-applies path heuristics after receiving canonical rows. Control: delete the single-history heuristic call path and assert the command contract in frontend tests.
- Failure scenario: canonical total and preview length are conflated. Control: return named `canonical_total`, `unclassified_total`, and `items` fields and test a paged fixture larger than the preview.

</topic>
