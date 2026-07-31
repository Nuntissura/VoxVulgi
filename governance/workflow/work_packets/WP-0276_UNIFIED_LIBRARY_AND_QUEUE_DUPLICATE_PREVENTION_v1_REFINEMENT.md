---
file_id: WP-0276-REFINEMENT-v1
file_kind: work-packet-refinement
updated_at: 2026-07-26
---

<topic id="operator-request-and-scope" status="active" version="v1" wp="WP-0276" updated_at="2026-07-26">

# Operator request

- Remove the product concept of old/new videos and playlists.
- Prevent duplicate downloads when direct, playlist, `/videos`, `/shorts`, and channel sources overlap.
- Use Firefox only for credential-backed testing.

# Base scope

- Use canonical imported/current identity for every enqueue and discovery path.
- Record source membership even when preflight returns `active` or `present`.
- Treat subscription folders as preferred destinations only when no physical item exists.
- Add full-canonical-set queue reconciliation with dry-run before apply.
- Remove remaining user-visible old/new and legacy classifications from affected library/archive surfaces.

</topic>

<topic id="reuse-roi-and-red-team" status="active" version="v1" wp="WP-0276" updated_at="2026-07-26">

# Existing systems reused

- Schema-v26 identity claims, aliases, associations, preflight, missing-media repair, job tracks, lineage, and bounded library queries.
- WP-0275 membership and import evidence.
- Firefox browser-cookie preflight and shared YouTube auth circuit.

# High-ROI additions

- A membership-preserving `present` receipt prevents source context loss at almost no additional download cost.
- A canonical queue reconciliation preview prevents already-queued duplicates after imported identity coverage increases.
- Coverage counters reveal which duplicates remain preventable by identity and which require content inventory.
- Stable backend filters remove UI path/title guessing and reduce future library feature rework.

# Risks, failures, and controls

- Reconciliation could target only visible jobs. Control: backend canonical query with independent totals and explicit selected IDs.
- A present item may be on unavailable NAS storage. Control: keep `present`, `missing`, and `storage_unreachable` distinct.
- Source membership write could race. Control: unique identity/source constraint and idempotent upsert.
- Removing user-visible distinctions could erase provenance. Control: retain internal origin/evidence, remove only product partitioning.
- Credential tests could touch another browser. Control: test contract pins `firefox`; assert no other source is launched or read.

# Verification and acceptance

- Exact overlapping-source fixtures create one canonical item and multiple memberships.
- Existing imported identity prevents direct and recurring enqueue.
- Queue preview and apply use the full canonical queued set and preserve repairable history.
- UI/bridge proof shows one library with source memberships and no old/new product bucket.
- Firefox exact-source auth check is the only browser-backed test.

</topic>
