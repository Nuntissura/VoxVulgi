---
file_id: WP-0273-REFINEMENT-v1
file_kind: work-packet-refinement
updated_at: 2026-07-22
---

<topic id="operator-request" status="active" version="v1" wp="WP-0273" updated_at="2026-07-22">

# Operator request

- Before any single or batch download, prevent duplicates already represented by the canonical library and physical media.
- If metadata exists but its file is missing, ask to relocate the file or approve a redownload.
- The same source video discovered by single and subscription workflows must use one canonical future file/library item.
- If a missing-file redownload URL fails, show the failing URL and let the operator replace it or explicitly remove the library record.

</topic>

<topic id="verified-current-state" status="active" version="v1" wp="WP-0273" updated_at="2026-07-22">

# Verified current state

- Schema v25 provides canonical `library_download_lineage` across service, origin, work track, source job, and source subscription.
- The single enqueue path does not currently preflight canonical source identity plus physical-path existence.
- yt-dlp `--download-archive` can suppress known IDs but cannot by itself distinguish an intentionally archived item from a database row whose NAS file disappeared.
- Irreplaceable subscriptions, playlists, library metadata, and physical media are protected by repo policy; no automatic metadata or media deletion is allowed.
- Canonical entities must remain distinct: normalized source identity, library item, source associations, job attempts, output paths, physical-file observation, and UI repair decisions.

</topic>

<topic id="research-and-selection" status="active" version="v1" wp="WP-0273" updated_at="2026-07-22">

# Research basis

- yt-dlp's official archive contract skips extractor IDs already recorded, providing a useful last-line duplicate guard but not a physical-file reconciliation workflow: https://github.com/yt-dlp/yt-dlp
- SQLite unique partial indexes can enforce identity only for rows that meet an explicit predicate, supporting additive migration without deleting ambiguous legacy data: https://www.sqlite.org/partialindex.html
- Current download-manager operator reports show that silently removing missing/failed history destroys recovery context; VoxVulgi therefore retains explicit repair state and requires confirmation.
- The existing VoxVulgi search, `library_item`, output, lineage, subscription, retry, and file-picker systems were inspected as the reusable source of truth.

# Selected approach

- Introduce a normalized canonical media identity (`service + extractor/media id`) and alias/source-association records without merging ambiguous legacy rows automatically.
- Add a batch preflight that classifies every submitted URL as `ready`, `active`, `present`, `missing`, or `invalid`, returning canonical evidence and preserving input order.
- Prevent enqueue for `active`/`present`; require explicit per-item or apply-to-selected decisions for missing files.
- Relocate verifies a selected file then atomically changes the canonical path; redownload preserves item/lineage and records repair state until a new output is proven.
- Subscription discovery consults the same identity: reuse present media, repair missing media, and create no second canonical file.
- Broken-source repair exposes the attempted URL and supports explicit replace-link/retry or metadata-only remove with typed/confirming action.

# Rejected options

- Title/path matching: unstable and collision-prone.
- Automatic merge/delete of legacy duplicates: violates preservation and risks data loss.
- Trusting database presence without a bounded file existence check: does not solve NAS drift.
- Deleting a library record when redownload fails: destroys recovery context.

</topic>

<topic id="scope-acceptance-red-team" status="active" version="v1" wp="WP-0273" updated_at="2026-07-22">

# Base scope and gaps closed

- Canonical source identity schema/backfill, batch preflight, duplicate prevention, and active-job suppression.
- Present/missing/unreachable distinction and relocate/redownload/replace-link/remove-record flows.
- One canonical media item/file across single and subscription ingress, with all associations retained.
- Large-batch review and bulk decisions that never bypass item-level evidence.

# High-ROI additions

- A dry-run preflight receipt is reusable by UI, bridge, tests, imports, and future archive sources.
- Persisted repair state makes crashes/restarts recoverable and gives models an attributable action trail.
- Source alias history preserves old and replacement URLs, making later link repair cheaper.

# Risks, failures, and controls

- NAS timeout could be misclassified as missing. Control: distinguish `present`, `missing`, and `storage_unreachable`; never offer destructive removal from an unreachable result.
- Concurrent single/subscription enqueue could race. Control: transactional unique identity claim plus active-job check.
- Relocate could select the wrong video. Control: require an existing regular file and compare known size/duration/hash where available; disclose mismatches before commit.
- Redownload could create a second item. Control: bind the repair job to the existing identity/item and attach output only after success.
- New URL might point to another video. Control: extract/normalize identity before replacing; require explicit reassociation on mismatch.
- Remove-record could erase valuable metadata. Control: explicit confirmation, metadata-only default, audit receipt, no media deletion.

# Acceptance

- Single, batch, and subscription ingress all consult one canonical preflight/claim path.
- Existing present media is not re-downloaded; an active identical job is not duplicated.
- Missing and unreachable storage are distinct and lead to safe actions.
- Relocate/redownload/relink/remove remain recoverable and preserve lineage according to the controls above.
- Exact duplicate, missing NAS, failed old URL, replacement URL, and concurrent-ingress tests pass.

</topic>
