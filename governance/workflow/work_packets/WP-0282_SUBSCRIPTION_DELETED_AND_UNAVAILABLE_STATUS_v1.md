---
file_id: WP-0282-WORK-PACKET-v1
file_kind: work-packet
updated_at: 2026-07-27
---

<topic id="contract" status="completed" version="v1" wp="WP-0282" owner="Codex" summary="Delivered manually controlled deleted and automatic recoverable unavailable subscription states." updated_at="2026-07-27">

# Work Packet: WP-0282 - Subscription deleted and unavailable status

## Dependencies and authority

- Refinement: `WP-0282_SUBSCRIPTION_DELETED_AND_UNAVAILABLE_STATUS_v1_REFINEMENT.md`
- Dependencies: WP-0264, WP-0279, WP-0281.
- Canonical requirements: `governance/spec/PRODUCT_SPEC.md`, `governance/spec/TECHNICAL_DESIGN.md`, `governance/workflow/PROOF_STANDARD.md`, and `build_rules.md`.

## Base scope

- Add durable `normal`, `unavailable`, and `deleted` source status with attribution.
- Make deleted manual-only and non-queueable without deleting any records/files.
- Set unavailable only from exact HTTP 404 refresh failures and show the required hosting-channel caveat.
- Expose operator UI and headless-assistant status controls.
- Mark Acerola, Fairy Ian, and Kpop Fap Cam deleted after packaged verification.

## Relevant files

- `product/engine/src/db.rs`
- `product/engine/src/subscriptions.rs`
- `product/engine/src/jobs.rs`
- `product/desktop/src-tauri/src/lib.rs`
- `product/desktop/src/pages/LibraryPage.tsx`
- `product/desktop/src/lib/failureStates.ts`

## Acceptance criteria

- Automated failures cannot write deleted.
- Deleted subscriptions cannot be queued or executed through any subscription refresh path.
- HTTP 404 writes unavailable; other failures do not; success clears unavailable but not deleted.
- UI actions preserve the row and dependent metadata and accurately explain 404 limits.
- Build, app-boundary, visual, and exact live-data proofs meet the refinement verification contract.

## Completion proof

- Proof bundle: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0282/20260727_v0_1_128/summary.md`
- Machine-readable evidence: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0282/20260727_v0_1_128/evidence.json`
- Governed desktop target: `v0.1.128`
- Live result: Acerola, Fairy Ian, and Kpop Fap Cam are `deleted`; all 260 subscription rows and their metadata/history remain.

</topic>
