---
file_id: WP-0286-v1
file_kind: work-packet
updated_at: 2026-07-31
---

<topic id="contract" status="done" version="v1" wp="WP-0286" updated_at="2026-07-31">

# WP-0286 — Canonical Media Library unification and live imported-identity enrichment

- Owner: Codex
- Status: DONE
- Refinement: `WP-0286_CANONICAL_MEDIA_LIBRARY_UNIFICATION_AND_LIVE_IMPORTED_IDENTITY_ENRICHMENT_v1_REFINEMENT.md`
- Dependencies: `WP-0268`, `WP-0275`, `WP-0276`, `WP-0281`, `WP-0284`
- Task-board row: `WP-0286`

# Intent

Make imported and current media one truthful Media Library pool, complete the previously unrun exact imported-identity apply, and expose legacy folder/source splits without risking operator media.

# Scope

- Full-set backend Media Library filtering, search, sorting, pagination, and matching totals.
- Canonical source resolution from durable lineage or exact imported identity.
- Verified live VoxVulgi DB backup and exact-only resumable imported-identity enrichment.
- Read-only folder/subscription reconciliation report.
- No NAS media move, rename, deletion, speculative link, or mass subscription creation.

# Acceptance criteria

- Media Library source/type/search/single/lifecycle/sort predicates execute before pagination.
- A filter can return matching rows located outside the newest unfiltered page, with an exact `filtered_total`.
- Returned library rows are unique and expose the canonical service used by source filtering.
- Unresolved imports are not guessed from folders or filenames.
- React requests the canonical query and does not redefine the matching set from loaded rows.
- A verified backup exists before live metadata apply.
- The 4KVDP source database is opened read-only and is unchanged by dry-run/apply.
- Only exact non-conflicting evidence is linked; ambiguous, unresolved, and conflicting records remain preserved.
- Before/after identity, membership, evidence, checkpoint, and library counts are recorded.
- The reconciliation report distinguishes canonical memberships, current target folders, physical folders, missing targets, split locations, and no-video directories.
- No NAS media file or directory is moved, renamed, or deleted.
- Governed build and quiet installed-artifact headless proof pass with an inspected screenshot/dump and a proof-standard `summary.md`.

</topic>

<topic id="status-updates" status="done" version="v1" wp="WP-0286" updated_at="2026-07-31">

# Status updates

- 2026-07-31: Contract created from direct live DB/NAS inspection. The exact installed v0.1.132 defect was reproduced: backend returned the newest 200 rows, React filtered that slice to zero, and canonical matching rows elsewhere in SQLite were invisible. No NAS media mutation was performed.
- 2026-07-31: DONE in desktop v0.1.133. The live database was backed up and enriched with 37,631 additional exact YouTube links while library, membership, and lineage counts remained stable. The governed six-pack warmup gate, release build, and quiet headless app proof passed. The exact YouTube + Video + Available flow rendered NAS rows and reported 49,942 matching items. No NAS file or directory was moved, renamed, or deleted. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0286/summary.md`.

</topic>
