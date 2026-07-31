---
file_id: WP-0275-REFINEMENT-v1
file_kind: work-packet-refinement
updated_at: 2026-07-26
---

<topic id="operator-request" status="active" version="v1" wp="WP-0275" updated_at="2026-07-26">

# Operator request

- Make imported and newly downloaded videos one library without an old/new distinction.
- Understand subscriptions to playlist, `/videos`, and `/shorts` pages as overlapping sources.
- Prevent those overlapping sources from creating duplicate NAS files.
- Preserve all subscriptions, playlists, metadata, third-party data, and media.

</topic>

<topic id="verified-state-and-research" status="active" version="v1" wp="WP-0275" updated_at="2026-07-26">

# Verified current state

- The live VoxVulgi database contains 260 active YouTube subscriptions: 110 playlists, 61 `/videos` pages, 47 `/shorts` pages, and 42 channel/creator pages.
- The library contains 121,090 imported 4KVDP rows and 7,953 VoxVulgi-download rows. Imported rows currently have no canonical source identity, ingest provenance, or download lineage.
- The read-only 4KVDP database contains 55,062 distinct YouTube IDs in subscription entries; 7,083 occur in multiple sources.
- The verified per-download relation is `download_item -> media_item_description -> url_description`.
- Exact normalized path evidence currently resolves 38,152 imported library rows to one YouTube ID; 211 path mappings are ambiguous and 82,727 are unresolved.

# Research basis

- yt-dlp uses extractor IDs for download archives and exposes canonical `id` plus final path through supported output/print contracts: https://github.com/yt-dlp/yt-dlp/blob/master/README.md
- SQLite permits one writer and concurrent readers; enrichment must use short explicit transactions rather than one large startup write: https://www.sqlite.org/lang_transaction.html
- SQLite's Online Backup API supports consistent incremental snapshots for disposable proof and recovery: https://www.sqlite.org/backup.html

# Selected approach

- Copy structured evidence out of the read-only 4KVDP store.
- Normalize Windows extended UNC and ordinary UNC spelling before exact path comparison.
- Persist exact, ambiguous, and unresolved evidence in VoxVulgi-managed tables.
- Bind only exact single-candidate matches to canonical `service + media_id`.
- Persist every known source as a many-to-many membership, including playlist, `/videos`, `/shorts`, and channel page.
- Run bounded, resumable enrichment outside schema migration.

# Rejected options

- Assigning ownership to the `/videos` page: a source page is membership, not canonical ownership.
- Filename/title-only automatic linking: collision-prone and destructive when wrong.
- Updating the third-party database: violates its read-only role.
- Folding the enrichment into startup migration: unbounded write/load risk on the live database.

</topic>

<topic id="roi-red-team-and-acceptance" status="active" version="v1" wp="WP-0275" updated_at="2026-07-26">

# Base scope

- Add source membership and import evidence schema.
- Add read-only 4KVDP exact-evidence enrichment with dry-run and apply modes.
- Link exact imported items to canonical identity and all recoverable memberships.
- Expose structured progress, counts, conflicts, and resumable checkpoints.

# High-ROI additions

- Reuse schema-v26 identity, alias, association, lineage, archive-import, and repair systems so later queue and cleanup packets operate on one truth.
- Store evidence receipts so humans and models can inspect why a row was or was not linked.
- Return source-kind totals and unresolved reasons, making later review and UI work cheap.
- Use an incremental disposable database backup for realistic proof without writing to operator data.

# Risks, failures, and controls

- Path aliases could mislink files. Control: canonicalize spelling only, require one item and one media ID, and preserve ambiguity.
- Imported rows may already conflict with an identity linked to another file. Control: record conflict; never overwrite automatically.
- Database writes may worsen freezes. Control: bounded batches, short transactions, pause/cancel, progress receipts, no startup execution.
- Third-party schema variants may omit tables/columns. Control: schema introspection and explicit unsupported-state result.
- Source entry may exist without a download path. Control: membership remains identity-level evidence; it does not fabricate a library-item link.

# Verification

- Schema migration and idempotency tests.
- Extended-UNC normalization and exact/ambiguous/unresolved fixture tests.
- Read-only third-party source proof.
- Disposable backup run against representative live structure with before/after counts.
- Existing identity/preflight tests remain green.

# Acceptance

- Exact imported items gain canonical identity without file movement or third-party writes.
- Multiple source memberships attach to one identity without multiplying physical items.
- Ambiguous/unresolved/conflicting cases remain preserved and inspectable.
- Enrichment is resumable, idempotent, and bounded.

</topic>
