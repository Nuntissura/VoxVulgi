---
file_id: WP-0286-REFINEMENT-v1
file_kind: work-packet-refinement
updated_at: 2026-07-31
---

<topic id="operator-request-and-verified-state" status="active" version="v1" wp="WP-0286" updated_at="2026-07-31">

# Operator request

- Reconcile the current library root `\\MIR\home\Video\4K Video\4K Video 21-08-2025` with VoxVulgi subscriptions and database state.
- Remove the product distinction between legacy-style and current downloads: imported and current media must behave as one equal pool.
- Detect folders or media that are no longer visible or attributable without losing subscriptions, playlists, library metadata, or NAS media.
- Reorganize only where evidence proves it is needed; do not preserve legacy wording or legacy product partitions.

# Verified current state

- The root contains 609 top-level directories and 134,251 indexed library items under that root.
- VoxVulgi stores 260 YouTube subscriptions; 257 are active. Twelve target folders are shared by multiple subscriptions and two configured targets are not present as physical directories.
- There are 363 physical folders that are not current subscription targets. Of these, 321 still contain indexed library rows. Forty-nine have no library rows, and a recursive read-only scan found no video files in those 49 folders.
- All 54,292 membership rows and all 10,180 lineage rows reference existing canonical source identities; there is no detached membership or lineage set.
- Seventy-one saved sources have membership items outside their current target folder. Some are intentional many-to-many overlap; some are old/new physical-layout splits. Folder position alone cannot safely decide ownership or authorize a move.
- There are 47,896 YouTube identities, of which 9,840 are currently linked to a library item and 38,056 are unlinked.
- The WP-0275 proof explicitly states that no live enrichment apply was run against operator data. The current gap is therefore unfinished exact-evidence enrichment, not proven mass unlinking by cleanup.
- The installed v0.1.132 Media Library fetched the newest 200 rows first and then applied source/type/search filters in React. With YouTube + Video + Available selected, all 200 loaded rows classified as local imports, so the UI displayed zero while matching rows existed elsewhere in the canonical store.

# Spec anchors

- `governance/spec/PRODUCT_SPEC.md`: unified imported/current library, canonical identity, many-to-many source membership, exact-evidence enrichment, and large-library truth.
- `governance/spec/TECHNICAL_DESIGN.md`: canonical `service + media_id`, imported evidence/checkpoints, backend lifecycle filtering, and storage folders as observations or destinations rather than ownership.
- `governance/workflow/PROOF_STANDARD.md`: focused engine/frontend tests, exact live-case proof, quiet app-boundary inspection, visual evidence, and proof summary.

# Scope edges

- In scope: canonical backend Media Library query and totals; exact imported identity application from a read-only 4KVDP database; verified VoxVulgi DB backup; read-only folder/source reconciliation report; optional metadata-only subscription target normalization when the effective path is unchanged.
- Non-goals: NAS file moves, folder renames, media deletion, empty-folder deletion, third-party database writes, mass subscription creation, folder-derived identity claims, speculative matching, queue resume, or modification of playlists/subscriptions beyond an evidence-preserving no-path-change normalization.

</topic>

<topic id="research-basis-and-selected-approach" status="active" version="v1" wp="WP-0286" updated_at="2026-07-31">

# Sources checked

- SQLite query planner: `https://www.sqlite.org/queryplanner.html`
- SQLite row-value scrolling guidance: `https://www.sqlite.org/rowvalue.html`
- qBittorrent WebUI API v5 torrent-list contract: `https://github.com/qbittorrent/qBittorrent/wiki/WebUI-API-(qBittorrent-5.0)`
- yt-dlp output-template and download-archive documentation: `https://github.com/yt-dlp/yt-dlp/blob/master/README.md`
- X Media Studio library/filter documentation: `https://help.x.com/en/using-x/media-studio-faqs`
- The current VoxVulgi schema, engine queries, React filter path, WP-0275/WP-0277 tooling/proof, live SQLite database, and live NAS root were inspected directly.
- GitHub, Hugging Face, Civitai, Reddit, and X were searched for adjacent implementations and operator failure reports. No external source overrides the current VoxVulgi canonical identity model.

# Relevant patterns

- Filtering and sorting belong in the same backend list request as pagination. qBittorrent exposes `filter`, `sort`, `reverse`, `limit`, and `offset` together rather than filtering a rendered page.
- SQLite can use indexes for filtering and ordering, and can avoid unnecessary full-result sorting when the requested order matches an index.
- Offset paging is retained for compatibility in this closure unit; SQLite keyset scrolling is a later optimization if measured offset cost becomes material.
- yt-dlp's canonical download archive records extractor IDs, and its default filename includes the media ID. VoxVulgi still requires structured exact evidence before identity binding and does not infer identity from a legacy folder name.
- Source identity and source membership are separate from physical storage. One media item may belong to many subscriptions/playlists while retaining one current physical file.

# Existing systems reused

- Schema-v26 canonical `media_source_identity`, schema-v27 `media_source_membership`, and `media_import_evidence`.
- WP-0275 read-only 4KVDP evidence reader, exact/ambiguous/unresolved classification, resumable checkpoint, and idempotent identity binding.
- WP-0268 durable download lineage and canonical single-video classification.
- WP-0284 backend lifecycle filtering and explicit loaded-row selection semantics.
- Existing read-only database connection, Tauri command boundary, diagnostics trace, headless agent bridge, and governed build script.

# Rejected options

- Increase the initial 200-row limit: still filters a slice and eventually fails as the library grows.
- Load all 134,000+ rows into React: creates avoidable memory, IPC, thumbnail, and UI work and still makes counts presentation-owned.
- Classify source from folder names: conflates physical placement with canonical identity and breaks many-to-many membership.
- Create subscriptions for all 363 unmatched folders: many are historical/manual containers and 49 contain no videos; this would invent ownership and recurring work.
- Move all 71 split-source folders into current targets: overlap and renamed source layouts make a bulk move unsafe without per-source evidence.
- Bind ambiguous/unresolved imported rows: risks assigning the wrong source ID and causing incorrect duplicate suppression.

# Selected approach

1. Add one typed `LibraryPage` response with `filtered_total` and bounded items.
2. Apply lifecycle, search, media type, resolved canonical service, canonical-single, and sort predicates in SQLite before `LIMIT`/`OFFSET`.
3. Resolve source service from durable download lineage first, then exact imported identity; leave unresolved imports local/unclassified.
4. Make React request the current filter/sort state and render the returned page without re-filtering it.
5. Back up the live VoxVulgi database and verify the backup before any metadata apply.
6. Dry-run the existing WP-0275 enrichment against the detected read-only 4KVDP database, compare counts, then apply only exact non-conflicting evidence through the existing resumable engine path.
7. Produce a machine-readable reconciliation report for the 71 split sources, unmatched physical folders, missing configured targets, and no-video directories. Do not move or delete media.
8. Normalize an `output_dir_override` only when the selected library binding resolves to the exact same effective path; otherwise report it for review.

</topic>

<topic id="roi-red-team-and-verification" status="active" version="v1" wp="WP-0286" updated_at="2026-07-31">

# High-ROI additions

- Return truthful loaded-versus-matching totals. This reuses the page query and prevents future zero/partial-list misdiagnosis.
- Return the resolved canonical service on each item. This reuses identity/lineage and keeps labels and filters consistent.
- Keep enrichment resumable and idempotent. This reuses WP-0275 and prevents partial-run rework on a large database.
- Pair the live apply with before/after identity and membership counts. This makes metadata changes auditable without touching NAS files.
- Preserve a no-move reconciliation report. This converts risky legacy-folder ambiguity into an inspectable next decision without creating another product partition.

# Risks, failure scenarios, and controls

- A source filter matches only the first loaded page. Control: focused engine test with matching rows outside the first page and a frontend contract that passes all predicates to the backend.
- A join duplicates one library item because it has multiple memberships or identities. Control: aggregate identity service per library item and verify stable unique item IDs/counts.
- A legacy imported item is mislabeled YouTube from a folder name. Control: service resolution accepts durable lineage or linked exact identity only.
- Enrichment writes to the 4KVDP database. Control: open it read-only and verify its size/mtime remain unchanged.
- The VoxVulgi DB apply is interrupted. Control: verified backup plus WP-0275 batch transactions/checkpoint and idempotent rerun.
- A current identity is already linked to a different item. Control: preserve as `conflict`; never overwrite automatically.
- A folder move breaks playlists/subscriptions or loses a file. Control: this work packet performs no NAS media move, rename, or deletion.
- UI counts drift during concurrent writes. Control: compute count and page from the same backend read transaction where practical and report the receipt timestamp/state through existing diagnostics.

# Verification

- RED/GREEN engine tests for full-set filtering before pagination, canonical imported identity service, media type, lifecycle, single lineage, search, sort, unique item rows, and exact totals.
- Frontend contract test proving `library_list` receives every canonical filter and that React does not filter/sort the loaded slice.
- TypeScript build, frontend contracts, focused Rust tests, and full relevant engine test suite.
- Live pre-apply backup verification, dry-run/apply summaries, source DB read-only proof, and before/after canonical counts.
- Governed desktop build with a semantic version increment and changelog entry.
- Quiet `--agent-headless` Media Library audit at 800x600: verify `agent_headless=true`, current built version, source/type/status controls, truthful loaded/matching text, nonempty exact live YouTube result, screenshot, dump, and no console errors.
- Proof bundle with `summary.md`; status remains REVIEW if exact live metadata apply or app-boundary proof cannot be completed.

# Microtask plan

1. Update specification, refinement, work packet, and task-board authority.
2. Implement and test the canonical backend Media Library page query.
3. Wire React to backend filtering/totals and add contracts.
4. Add a read-only reconciliation report command/script if no existing tool covers the exact live report.
5. Verify backup, dry-run, apply exact enrichment, and record before/after receipts.
6. Build, headless-audit, inspect visual/state evidence, and finalize proof.

</topic>
