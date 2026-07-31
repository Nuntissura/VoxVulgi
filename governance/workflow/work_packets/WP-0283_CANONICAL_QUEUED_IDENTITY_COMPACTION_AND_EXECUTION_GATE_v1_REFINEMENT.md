---
file_id: WP-0283-REFINEMENT-v1
file_kind: work-packet-refinement
updated_at: 2026-07-27
---

<topic id="operator-request-and-spec-anchors" status="active" version="v1" wp="WP-0283" updated_at="2026-07-27">

# Operator request

- Keep the queue paused.
- Compact the full canonical queued set to one job per YouTube identity while retaining all source memberships.
- Add an execution-boundary identity gate so stale queued jobs cannot redownload files removed during cleanup.

# Spec anchors

- `governance/spec/PRODUCT_SPEC.md`: canonical archive ingress, full-set queue reconciliation, durable membership/attempt lineage, and execution-boundary suppression.
- `governance/spec/TECHNICAL_DESIGN.md`: unified-library reconciliation and deterministic identity ownership.
- `governance/workflow/PROOF_STANDARD.md`: focused tests, live canonical receipts, and proof summary.

# Scope edges

- In scope: queued `download_direct_url` YouTube jobs across all batches and tracks; canonical URL parsing; association/membership preservation; deterministic keeper selection; present-media suppression; atomic active-claim retargeting; execution-boundary revalidation.
- Non-goals: deleting job history, deleting media, resuming the queue, changing subscription lifecycle state, or resolving content-only duplicates without a canonical YouTube identity.
- Assumption: queue pause and zero running direct-download jobs are verified immediately before live apply.

</topic>

<topic id="research-basis-and-selected-approach" status="active" version="v1" wp="WP-0283" updated_at="2026-07-27">

# Sources checked

- SQLite transactions: `https://www.sqlite.org/lang_transaction.html`
- SQLite partial indexes: `https://www.sqlite.org/partialindex.html`
- SQLite online backup API: `https://www.sqlite.org/backup.html`
- SQLite busy timeout: `https://www.sqlite.org/c3ref/busy_timeout.html`
- Sidekiq Unique Jobs execution-lock lifecycle: `https://github.com/mhenrixon/sidekiq-unique-jobs`
- Sidekiq batch validity-at-execution guidance: `https://github.com/sidekiq/sidekiq/wiki/Batches/d9ba675338d1ecf2a02039b18bc8ba05fac60c26`
- RQ explicit job IDs: `https://github.com/rq/rq`
- Bee-Queue external-resource job IDs: `https://github.com/bee-queue/bee-queue/blob/master/README.md`

# Relevant patterns

- Use a canonical external-resource identity, not URL text, batch membership, folder, or rendered row identity.
- Enforce uniqueness both at enqueue/reconciliation time and again at execution because queued state can predate current guards.
- Apply status/ownership changes in one immediate write transaction; keep history rows instead of deleting them.
- Back up the live SQLite database through its online backup mechanism before a bulk state transition.

# Existing systems reused

- Schema-v26 `media_source_identity`, aliases, associations, and active claims.
- Schema-v27 many-to-many `media_source_membership`.
- Existing canonical YouTube URL parsing and bounded NAS present/missing/unreachable observation.
- Existing queue pause state, job status/history, tracks, batches, and retry lineage.
- Existing headless diagnostics and proof paths.

# Rejected options

- Raw URL grouping: rejects equivalent YouTube aliases and cannot satisfy canonical identity.
- Deleting duplicate job rows: loses attempt, batch, and operator audit history.
- Trusting the existing `active_job_id` alone: legacy queued jobs were created before identity claims were backfilled.
- Relying only on pre-enqueue claims: does not protect old queued jobs at dispatch.
- Holding a SQLite transaction open during NAS probing: risks long writer contention on a slow or unavailable UNC path.

# Selected approach

1. Enumerate the complete queued direct YouTube set in canonical order and canonicalize every target.
2. Preserve each job's source association and subscription membership idempotently.
3. Probe each linked canonical path outside the write transaction with bounded present/missing/unreachable semantics.
4. Retain a valid queued/running canonical owner. Without one, select a non-playlist channel-page, `/videos`, or `/shorts` job before a playlist job per WP-0281, then prefer the newest `created_at_ms`, then stable `id`.
5. Apply queued status changes and active-claim retargeting in one immediate transaction.
6. Revalidate canonical ownership immediately before dispatch; suppress stale/present jobs without launching network work.

</topic>

<topic id="roi-red-team-and-verification" status="active" version="v1" wp="WP-0283" updated_at="2026-07-27">

# Base scope

- Full-set dry-run and apply.
- One queued keeper per non-present YouTube identity.
- Zero queued keepers for present media.
- Execution-boundary identity gate.
- Durable receipt and rollback-ready live database backup.

# High-ROI additions

- Preserve association/membership rows while the queue is already being scanned; this closes source-context loss without another 104k-row pass.
- Emit deterministic counts and keeper IDs; this makes operator and parallel-agent verification cheap and attributable.
- Retain canceled jobs rather than deleting them; this reuses existing Jobs history and makes recovery possible.
- Test alias URLs, cross-batch/cross-track groups, legacy jobs without claims, and NAS-unreachable state; these fixtures prevent the most likely future regressions.

# Risks, failure scenarios, and controls

- A paged implementation compacts only one slice. Control: reconciliation owns its internal full-set scan and reports one full-set receipt; pagination may limit returned samples only.
- The selected keeper differs between dry-run and apply. Control: deterministic active-owner, source-priority, newest-creation, stable-ID ordering plus queue-paused precondition and one apply transaction.
- NAS timeout is mistaken for missing/present. Control: preserve the existing three-state bounded observation; unreachable never authorizes cancellation or redownload.
- Memberships are lost when duplicate jobs are canceled. Control: idempotently backfill association and membership before status mutation and assert all source IDs survive.
- Two stale jobs race at dispatch. Control: immediate transaction conditionally owns `active_job_id`; only the owner may transition to running.
- A crash interrupts bulk apply. Control: online backup before apply, atomic transaction, retained canceled rows, and post-apply invariant audit.
- A present media path disappears between probe and dispatch. Control: the execution gate performs a fresh bounded observation and applies explicit missing-media policy.

# Verification

- RED then GREEN focused engine tests for queued-vs-queued alias grouping, present suppression, deterministic active/source-priority/newest keeper selection, cross-batch/track membership preservation, full-set behavior beyond one page, stale execution suppression, and unreachable storage.
- Compile the engine and desktop command boundary.
- Verify live queue remains paused and no direct jobs are running before backup/apply.
- Produce and verify an online SQLite backup.
- Compare dry-run and apply receipts; audit invariant: at most one queued/running direct YouTube job per canonical identity.
- Verify all pre-apply source memberships and batch/job history rows remain present.
- Verify suppressed execution creates no downloader process and records the reason.

# Microtask plan

1. Extend the canonical reconciliation engine and receipts.
2. Add the execution-boundary gate before `queued -> running`.
3. Add focused regression fixtures and run them RED/GREEN.
4. Back up, dry-run, apply, and audit the live paused queue.
5. Build and run quiet headless proof under the existing proof standard.

</topic>
