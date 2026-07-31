---
file_id: WP-0284-v1
file_kind: work-packet
updated_at: 2026-07-29
---

<topic id="contract" status="done" version="v1" wp="WP-0284" updated_at="2026-07-29">

# WP-0284 — Subscription video selection, deletion, and manual redownload

- Owner: Codex
- Status: DONE
- Refinement: `WP-0284_SUBSCRIPTION_VIDEO_SELECTION_DELETE_AND_MANUAL_REDOWNLOAD_v1_REFINEMENT.md`
- Dependencies: `WP-0273`, `WP-0281`, `WP-0282`, `WP-0283`
- Task-board row: `WP-0284`

# Intent

Give the operator safe, explicit single/bulk control over canonical subscription video files from Video Archiver and Media Library without allowing automatic work to reverse the deletion.

# Scope

- Stable-ID checkbox selection and screen-appropriate selection toolbars.
- Recoverable Recycle Bin and explicit permanent filesystem deletion.
- Preserved library, identity, membership, subscription, playlist, and job metadata.
- Durable operator-deleted lifecycle state distinct from missing/unreachable media.
- Exact selected-item manual redownload authorization and dispatch enforcement.
- Dedicated Deleted projections plus normal-list suppression.
- Per-item receipts, focused tests, governed build, and quiet headless visual proof.

# Acceptance criteria

- All refinement verification requirements pass.
- Single and bulk actions submit exact canonical item IDs, never inferred folders or rendered counts.
- Successful deletion removes the selected physical file and preserves its canonical metadata and memberships.
- Failed or unreachable deletion is reported per item and is never falsely labeled completed.
- Operator-deleted and delete-pending items cannot be enqueued or executed by subscription refresh, retry, retry-all, batch repair, redownload-all, or any other generic path.
- Only the exact job created by an explicit selected-item manual-redownload command may rematerialize a deleted item.
- The tombstone clears only after the authorized job imports a present replacement file.
- Video Archiver derives subscription videos from canonical membership and separates Available, Pending, and Deleted.
- Media Library provides Available, Deleted, and All lifecycle filtering and keeps deleted rows after available rows in All.
- UI uses existing cohesive workspace/action-row/list patterns and adds no dashboard card.
- Headless audit cannot invoke delete or redownload mutations.
- Proof bundle includes `summary.md` and satisfies `governance/workflow/PROOF_STANDARD.md`.

</topic>

<topic id="status-updates" status="complete" version="v1" wp="WP-0284" updated_at="2026-07-29">

# Status updates

- 2026-07-29: Created from the operator request after tracing canonical identity, subscription membership, missing-media repair, generic retry, and dispatch paths. Implementation started with the queue left paused.
- 2026-07-29: Completed in desktop v0.1.132. Focused lifecycle, exact-authorization, dispatch, membership, partial-failure, unreachable-storage, and Windows Recycle Bin tests passed; 102 frontend contracts passed; the governed installer built; both requested screens passed quiet headless semantic and visual inspection. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0284/summary.md`.

</topic>
