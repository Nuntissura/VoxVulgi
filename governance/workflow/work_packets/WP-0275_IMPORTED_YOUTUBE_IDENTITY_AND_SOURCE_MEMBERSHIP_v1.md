# Work Packet: WP-0275 - Imported YouTube identity and source membership

## Metadata

- ID: WP-0275
- Owner: Codex
- Status: DONE
- Created: 2026-07-26
- Refinement: `WP-0275_IMPORTED_YOUTUBE_IDENTITY_AND_SOURCE_MEMBERSHIP_v1_REFINEMENT.md`
- Dependencies: `WP-0273`

## Intent

Enrich imported media with canonical YouTube identity and many-to-many source memberships so imported and current downloads participate in the same duplicate-prevention system.

## Scope

- Additive membership/import-evidence schema and indexes.
- Read-only 4KVDP evidence extraction and path normalization.
- Dry-run/apply enrichment with exact-only linking, conflicts, checkpoints, and receipts.
- Source membership recovery for playlist, `/videos`, `/shorts`, and channel subscriptions.

## Acceptance criteria

- All refinement acceptance and red-team controls pass.
- No imported media moves and no third-party database writes occur.
- Exact linking is idempotent; ambiguous/unresolved evidence remains preserved.
- Proof bundle satisfies `PROOF_STANDARD.md` before `DONE`.

## Verification

- Engine unit/integration tests.
- Disposable copy of representative operator database.
- Read-only source fingerprint before/after.
- Bounded runtime/trace evidence.

## Status updates

- 2026-07-26: Contract created after product and technical specifications were updated; implementation started.
- 2026-07-26: Engine and desktop command implementation complete. Schema v27 adds source memberships, import evidence, and resumable checkpoints; read-only 4KVDP evidence uses the verified download-item relation and exact-only linking. Migration, exact/unresolved, extended-UNC, imported subscription, membership-kind, and idempotent behavior tests pass. Final installed-app/build proof remains in the shared release verification step.
- 2026-07-26: DONE. Final v0.1.113 desktop and installer build passed. Focused final-state tests passed, and the packaged Options import surface was visually inspected through the headless bridge. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0275/summary.md`.
