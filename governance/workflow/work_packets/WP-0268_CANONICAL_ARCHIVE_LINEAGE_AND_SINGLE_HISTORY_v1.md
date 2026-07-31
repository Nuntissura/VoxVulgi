# Work Packet: WP-0268 - Canonical archive lineage and single-video history

## Metadata

- ID: WP-0268
- Owner: Codex
- Status: DONE
- Created: 2026-07-22
- Target milestone: next managed desktop build
- Refinement: `WP-0268_CANONICAL_ARCHIVE_LINEAGE_AND_SINGLE_HISTORY_v1_REFINEMENT.md`
- Corrective predecessor: `WP-0247_ARCHIVER_SESSION_AND_JOB_CONTEXT_RECOVERY.md`

## Intent

- What: Replace the leaking downloaded-single-video projection with durable canonical ingest lineage and a backend single-only history contract.
- Why: The current broad YouTube query plus frontend path/URL inference misclassifies mapped subscription outputs as one-off singles and cannot remain authoritative after job cleanup.

## Scope

- In scope:
  - additive provenance-lineage schema and indexes;
  - lineage write at successful download/library handoff;
  - bounded, resumable, evidence-only historical backfill;
  - backend canonical single-history page and totals;
  - existing single-history UI and Media Library single-only filter switched to canonical results;
  - focused migration/query/frontend tests and exact live-data proof.
- Out of scope:
  - deleting, moving, or reimporting media;
  - deleting or rewriting subscriptions, playlists, third-party imports, or job history;
  - full scheduler replacement, owned by `WP-0269`;
  - general Media Library redesign.

## Existing systems reused

- `job.item_id`, `job.batch_id`, structured `params_json`, and retry lineage.
- `ingest_provenance` as the durable item-origin record.
- Bounded read-only list patterns and canonical totals from `WP-0256`/`WP-0258`.
- Existing single-history table, search, paging, thumbnail, open, and reveal actions.

## Acceptance criteria

- Every acceptance criterion and red-team control in the linked refinement passes.
- The single-video UI contains only canonically identified one-off single-video downloads.
- Unknown historical items remain preserved and visibly unclassified outside the canonical single list.
- The exact mapped-NAS leak and known one-off successes are both runtime-verified.
- A proof bundle satisfying `governance/workflow/PROOF_STANDARD.md` exists before `DONE`.

## High-ROI additions

- One explicit service/origin/work-track model is shared with later scheduler and observability packets.
- Diagnostic lineage fields make projection decisions reproducible by no-context models.
- Backfill progress/receipt prevents silent partial-history confusion on large libraries.

## Test / verification plan

- Migration tests from schema v22, including repeat migration and copied representative DB timing.
- Seeded classification tests for watch, shorts, live, playlist, channel, mapped subscription, Instagram, other service, retry, and unknown rows.
- Frontend contract tests proving no path/URL heuristic is used for single history.
- Read-only query-plan and paging checks with totals distinct from returned rows.
- Exact live read verification against the inspected newest-200 dataset after a timestamped DB backup.
- Headless bridge navigation, snapshot, and dump of Video Archiver single history.

## Risks / open questions

- The only accepted fallback for ambiguous legacy data is `unclassified`; operator-facing wording must make that preservation choice clear.
- If transactional item/provenance insertion requires a larger library API change, keep the repair operation idempotent and prove crash recovery.

## Status updates

- 2026-07-22: v1 refinement and contract created from repo, installed-app, live-DB, and current field research. No product code changed.
- 2026-07-22: Activated after the Work Packet, Task Board, Product Spec, and Technical Design gates were completed in the operator-required order.
- 2026-07-22: Implementation complete and focused engine, desktop compile, and frontend contract checks pass. Status remains `IN_PROGRESS` until exact live-data and headless app-boundary proof is captured in the shared managed build.
- 2026-07-22: `DONE` in installed v0.1.105. Live schema-v25 proof found 44 canonical singles, 1,349 preserved unclassified items, 6,113 mapped subscription items excluded, and zero subscription-linked rows in single history. The inspected headless snapshot shows the same 44-item up-to-date canonical projection with no console errors. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0268/summary.md`.
