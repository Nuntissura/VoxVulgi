# Work Packet: WP-0277 - Recoverable NAS duplicate cleanup

## Metadata

- ID: WP-0277
- Owner: Codex
- Status: REVIEW
- Created: 2026-07-26
- Refinement: `WP-0277_RECOVERABLE_NAS_DUPLICATE_CLEANUP_v1_REFINEMENT.md`
- Dependencies: `WP-0275`, `WP-0276`

## Intent

Reconcile physical and VV library-path truth, inventory exact and review-only duplicate candidates,
then safely consolidate reviewed groups through quarantine and rollback without deleting operator
media during automated work.

## Scope

- Bounded resumable inventory and staged exact hashing.
- Dry-run-first reconciliation of physical-only files and missing/zero-byte VV paths; deterministic
  unique matches relink preserved records, unmatched physical files are indexed without erasing
  unresolved metadata, and ambiguous matches remain review-only.
- Variant evidence and keeper proposals.
- Dry-run/apply manifests, quarantine, membership relink, atomic library-path convergence, and rollback.
- Library maintenance UI/bridge/diagnostics with no new cards.

## Acceptance criteria

- All refinement acceptance and controls pass.
- Inventory performs no filesystem mutation.
- Reconciliation deletes neither media nor library metadata, does not overwrite existing files, and
  applies only one-to-one evidence matches automatically.
- Automated tests use disposable fixtures only.
- A successful quarantine action leaves no `library_item.media_path` pointing at its moved source;
  preserved redundant rows resolve to the verified keeper path in the same database transaction as
  identity relinking and action-state publication.
- Rollback restores the source file, original redundant library path, and recorded identity links
  together; a failed database handoff compensates the filesystem move or remains explicitly
  recoverable as `attention`.
- Permanent deletion remains a separate explicit operator action.
- Proof bundle satisfies `PROOF_STANDARD.md` before `DONE`.

## Status updates

- 2026-07-26: Contract created; queued after unified identity and prevention work.
- 2026-07-26: Implemented schema v28 cleanup runs/files/groups/actions/cache, bounded resumable inventory, staged size/window/full SHA-256 comparison, exact-only review groups, explicit keeper approval, quarantine apply, identity relink, and rollback.
- 2026-07-26: Disposable fixture proof confirms inventory does not mutate media, quarantine cannot overlap an inventory root, exact apply removes no metadata, and rollback restores byte-identical files; frontend production build and Tauri compile pass. Advanced to REVIEW pending packaged-app proof.
- 2026-07-26: Final v0.1.113 build and focused final-state engine tests pass; the collapsed cleanup disclosure is visually clean and discoverable. Status remains REVIEW because the complete packaged UI approval, quarantine, and rollback flow has not been exercised against a disposable fixture. No operator NAS files were scanned, moved, or deleted. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0277/summary.md`.
