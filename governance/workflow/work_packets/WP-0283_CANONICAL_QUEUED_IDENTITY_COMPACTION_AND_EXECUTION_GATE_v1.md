---
file_id: WP-0283-v1
file_kind: work-packet
updated_at: 2026-07-27
---

<topic id="contract" status="in-progress" version="v1" wp="WP-0283" updated_at="2026-07-27">

# WP-0283 — Canonical queued identity compaction and execution gate

- Owner: Codex
- Status: IN_PROGRESS
- Refinement: `WP-0283_CANONICAL_QUEUED_IDENTITY_COMPACTION_AND_EXECUTION_GATE_v1_REFINEMENT.md`
- Dependencies: `WP-0273`, `WP-0275`, `WP-0276`, `WP-0281`
- Task-board row: `WP-0283`

# Intent

Correct the unproven queued-versus-queued gap left by WP-0276 and prevent legacy/stale queued YouTube attempts from crossing the downloader execution boundary.

# Scope

- Full canonical queued YouTube identity reconciliation with dry-run and atomic apply.
- One deterministic active-owner/source-priority/newest keeper per non-present identity and no keeper for present media.
- Source association, subscription membership, batch, retry, and attempt-history preservation.
- Execution-boundary identity ownership and present-media gate before network/`yt-dlp`.
- Live paused-queue backup, apply receipt, and invariant audit.

# Acceptance criteria

- All refinement verification requirements pass.
- Reconciliation scans the full canonical queued set irrespective of UI/filter/page state.
- Every identifiable non-present identity has at most one queued/running keeper after apply.
- Every present identity has zero queued/running download jobs after apply.
- All source memberships and job/batch history survive compaction.
- A stale non-owner or already-present direct YouTube job cannot transition to running or start a downloader process.
- Missing and unreachable media remain distinct; neither is silently treated as present.
- Live apply uses a verified online SQLite backup and keeps the queue paused.
- Proof bundle includes `summary.md` and satisfies `governance/workflow/PROOF_STANDARD.md`.

</topic>

<topic id="status-updates" status="active" version="v1" wp="WP-0283" updated_at="2026-07-27">

# Status updates

- 2026-07-27: Created after repo and live-library inspection proved WP-0276 canceled only queued jobs linked to already-present media and did not compact queued-versus-queued canonical identities. Queue remains paused.

</topic>
