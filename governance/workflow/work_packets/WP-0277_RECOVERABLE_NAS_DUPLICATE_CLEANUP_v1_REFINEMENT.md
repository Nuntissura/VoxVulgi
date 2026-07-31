---
file_id: WP-0277-REFINEMENT-v1
file_kind: work-packet-refinement
updated_at: 2026-07-26
---

<topic id="operator-request-research-and-selection" status="active" version="v1" wp="WP-0277" updated_at="2026-07-26">

# Operator request

- Retroactively clean duplicate videos from the NAS.
- Keep one canonical video and link all source subscriptions back to it.
- Reconcile physical files absent from the VV index with VV records whose paths are missing or
  zero-byte before hashing and quarantine.

# Research basis

- Czkawka stages exact duplicate work by size, partial prehash, then full hash: https://github.com/qarmin/czkawka/blob/master/instructions/Instruction.md
- jdupes uses size, a small leading comparison, full hash, then byte comparison: https://github.com/jbruchon/jdupes
- fclones separates grouping from removal, supports machine-readable output and dry-run, and stages prefix/suffix/full hashes: https://github.com/pkolaczk/fclones
- ffprobe provides structured stream/container metadata; FFmpeg frame hashes compare decoded content but are not physical-file identity proof: https://ffmpeg.org/ffprobe.html and https://ffmpeg.org/ffmpeg-formats.html

# Selected approach

- Inventory first, never mutate during discovery.
- Prefer canonical YouTube ID evidence; use size, bounded prefix/suffix, full digest, and optional byte comparison for unresolved exact duplicates.
- Treat different encodes as variants requiring review, not automatic duplicates.
- Propose a keeper using identity linkage, availability, integrity, quality, and path stability.
- Apply by moving non-keepers to quarantine, relinking memberships to the keeper, and writing a rollback manifest.

# Rejected options

- Hardlinks/symlinks as default: capability, backup, SMB, and operator-visibility risks.
- Automatic deletion after hash: insufficient recovery window.
- Perceptual similarity as delete proof: can collapse legitimate edits/versions.
- Unbounded parallel hashing: competes with active jobs and other heavy host workloads.

</topic>

<topic id="scope-roi-risks-and-acceptance" status="active" version="v1" wp="WP-0277" updated_at="2026-07-26">

# Base scope

- Resumable inventory runs and candidate groups.
- Physical-versus-library path reconciliation with one-to-one evidence gates, preservation of
  unresolved records, and indexing of unmatched physical media.
- Staged exact hashing and variant review metadata.
- Keeper proposal, reclaim estimate, membership impact, dry-run manifest.
- Quarantine apply and rollback; separate permanent-delete confirmation.
- Existing no-card library maintenance table/drawer UI and headless bridge support.

# High-ROI additions

- Persistent digest cache avoids re-reading multi-terabyte NAS content in later runs.
- Scan budgets that yield to active downloads and high host pressure reduce freezes and job interference.
- Pre-apply availability/integrity recheck prevents acting on stale candidates.
- Exportable JSON/Markdown manifests improve operator review and model handoff.
- Quarantine expiry remains informational until separately confirmed, preventing hidden deletion.

# Risks, failures, and controls

- NAS timeout could appear missing. Control: unreachable/slow is not absent; retry with bounded backoff.
- Filename or source-ID evidence may map one missing record to several physical variants. Control:
  apply only unique one-to-one mappings automatically; retain ambiguous candidates for the
  subsequent full-hash review and never overwrite a live canonical path.
- File may change after hashing. Control: record size/mtime/file identity and revalidate immediately before apply.
- Wrong keeper may reduce quality. Control: show codec/resolution/duration and require review for variants.
- Cross-volume move may partially copy. Control: copy-verify-then-remove source only inside approved apply; durable step state permits recovery.
- Crash mid-apply may split state. Control: per-action journal and idempotent resume/rollback.
- Membership relink may precede file safety. Control: quarantine verification first, short DB transaction second.
- A redundant `library_item` may still point at the source after that file moves to quarantine.
  Control: after verified movement, atomically relink identities, set the preserved redundant
  library row to the keeper path, and publish the action state; journal the source item/path so
  rollback restores both path truth and identity ownership.
- The filesystem move may succeed while the database transaction fails. Control: attempt a
  verified compensating move back to the original source before marking failure; retain
  `attention` with the durable action journal when compensation cannot complete.

# Verification and acceptance

- Exact duplicate, same-ID different-encode, hash collision simulation, changing-file, unreachable NAS, cross-volume failure, cancel/resume, crash/recovery, and rollback fixtures pass.
- Disposable apply/rollback proof asserts library rows never reference a moved-away source after a
  successful apply and return to their original path after rollback.
- No live operator media is mutated in automated proof.
- Inventory is read-only and apply is impossible without an explicit reviewed run.
- Path reconciliation dry-run and apply counts are reproducible from the same canonical root and
  database preimage; unmatched/ambiguous rows remain present and no library record is deleted.
- Every applied action is recoverable from the manifest.

</topic>
