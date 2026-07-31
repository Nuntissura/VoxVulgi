# Work Packet: WP-0273 - Canonical source dedup and missing-media repair

## Metadata

- ID: WP-0273
- Owner: Codex
- Status: DONE
- Created: 2026-07-22
- Refinement: `WP-0273_CANONICAL_SOURCE_DEDUP_AND_MISSING_MEDIA_REPAIR_v1_REFINEMENT.md`
- Dependencies: `WP-0268`, `WP-0272`

## Intent

Guarantee one canonical media item/file per source video while making missing-file and broken-link repair explicit, safe, and batch-capable.

## Scope

- Normalized source identity and association schema with preservation-first backfill.
- Canonical ordered batch preflight and transactional enqueue claim.
- Present/active prevention; missing/unreachable distinction.
- Relocate, redownload, replace-link, retry, and explicit metadata-only removal.
- Shared single/subscription behavior, receipts, diagnostics, and tests.

## Acceptance criteria

- All refinement criteria and red-team controls pass.
- No test path silently deletes or overwrites library metadata or media.
- One source identity cannot produce concurrent duplicate canonical downloads.
- Proof bundle satisfies `PROOF_STANDARD.md` before `DONE`.

## Verification

- Migration/backfill ambiguity tests and transaction-race tests.
- Exact single, large batch, subscription overlap, missing/unreachable NAS, relocation, broken URL, relink, and removal tests.
- Installed-app headless repair dialogs plus DB/filesystem before/after evidence using disposable fixtures only.

## Status updates

- 2026-07-22: Preservation-first contract created before product-code edits; queued behind live activity work.
- 2026-07-23: DONE in desktop v0.1.107. Schema v26 identity/association state, ordered preflight, atomic claims, relocate/redownload/relink/metadata-only removal, and existing-item reuse passed disposable lifecycle tests and frontend contracts. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0273/summary.md`.
