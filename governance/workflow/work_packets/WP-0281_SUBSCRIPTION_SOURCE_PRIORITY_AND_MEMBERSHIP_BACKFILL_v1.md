---
file_id: WP-0281-WORK-PACKET-v1
file_kind: work-packet
updated_at: 2026-07-27
---

<topic id="contract" status="done" version="v1" wp="WP-0281" owner="Codex" summary="Backfilled live subscription memberships and prioritized feed pages before playlists without mutating NAS data." updated_at="2026-07-27">

# Work Packet: WP-0281 - Subscription source priority and membership backfill

## Dependencies and authority

- Refinement: `WP-0281_SUBSCRIPTION_SOURCE_PRIORITY_AND_MEMBERSHIP_BACKFILL_v1_REFINEMENT.md`
- Dependencies: WP-0273, WP-0275, WP-0276.
- Canonical requirements: `governance/spec/PRODUCT_SPEC.md`, `governance/spec/TECHNICAL_DESIGN.md`, `governance/workflow/PROOF_STANDARD.md`, and `build_rules.md`.

## Base scope

- Add an idempotent schema-v29 membership backfill from existing subscription associations.
- Prioritize channel-page, `/videos`, and `/shorts` refresh jobs before playlists inside the same queued refresh cohort.
- Preserve the canonical present/active/missing claim gate and source-membership recording.
- Build and quietly verify the desktop artifact before using it for subscription recovery.

## Relevant files

- `product/engine/src/db.rs`
- `product/engine/src/subscriptions.rs`
- `product/engine/src/jobs.rs`
- `product/engine/src/library.rs`
- `product/engine/src/paths.rs`

## Constraints and non-goals

- No deletion, move, rename, hash scan, archive cleanup, subscription deletion, playlist deletion, third-party database write, or NAS mutation.
- No use of folder names as identity proof.
- Existing queued jobs retain their canonical identity claims; this packet does not bulk-cancel or recreate them.

## Expected behavior

- Feed sources enumerate first within a cohort, so a common video receives one canonical claim before a playlist sees it.
- A playlist still records its source membership when the video is present or active.
- A feed membership that has no present/active canonical item never blocks a playlist from downloading or repairing the video.

## Acceptance criteria

- All refinement controls and verification requirements pass.
- The live association-to-membership gap is corrected by an additive idempotent migration.
- No operator media, subtitle, library, subscription, playlist, or third-party record is modified by the behavior migration.
- Desktop artifact version and changelog follow the build policy; proof bundle meets `PROOF_STANDARD.md`.

## Verification steps

- Run focused database, membership, and refresh-selection tests.
- Build the governed desktop target.
- Launch only the produced artifact with `--agent-headless`; verify state, source membership projection, and non-mutating UI audit/snapshot.
- Record proof under `product/desktop/build_target/tool_artifacts/wp_runs/WP-0281/`.

## Status updates

- 2026-07-27: Started from a read-only live-state audit. Verified 53,225 associations and zero membership rows; no operator data changed.
- 2026-07-27: Completed in v0.1.127. Schema v29 backfilled 53,224 current association rows into memberships and the focused migration/scheduler tests, governed desktop build, and headless Video Archiver proof passed. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0281/20260727_v0_1_127/summary.md`.

</topic>
