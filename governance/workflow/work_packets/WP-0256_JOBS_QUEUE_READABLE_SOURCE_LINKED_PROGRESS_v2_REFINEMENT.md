---
file_id: WP-0256-REFINEMENT-v2
file_kind: work-packet-refinement
updated_at: 2026-07-16
---

<topic id="operator-request" status="active" version="v2" wp="WP-0256" updated_at="2026-07-16">

# Operator request

- Jobs/Queue has become bloated and does not show the information the operator is looking for at the time.
- Rebuild the operator surface so current work leads, failures requiring action are separate, and old completed work is history rather than the default page.
- Preserve useful recovery, retry, log, output, source, and lineage actions behind focused rows/details.
- Do not introduce cards.

</topic>

<topic id="selected-ux" status="active" version="v2" wp="WP-0256" updated_at="2026-07-16">

# Selected UX

- Primary views: `Now`, `Needs attention`, and `History`.
- Default to `Now`; show canonical total counts from the backend on the view selectors.
- Compact queue toolbar: refresh, pause/resume, cancel active. Put concurrency, cleanup, and developer test controls under one advanced disclosure.
- Replace the channel/subscription pseudo-card grid with one compact source filter/list.
- Lead every row with the target/source and human state. Keep IDs, raw types, timestamps, raw errors, and lineage in the expandable detail path.
- A successful Video Archiver enqueue message includes the returned job ID(s), making the handoff independently traceable.

# Acceptance criteria

- The default Jobs view shows queued/running work only and makes the newest single-video attempt visible.
- `Needs attention` shows failed/canceled attempts with the shared plain-language requirement classifier.
- `History` is explicit and does not crowd current work.
- Loading, refresh error, empty current work, and truly empty job history have different messages.
- No card wrapper or button-card grid is introduced; touched Jobs card count is reduced.
- Retry, cancel, logs, details, outputs/artifacts, and batch recovery remain reachable.
- Visual bridge snapshots prove readable layout, discoverable controls, coherent navigation, no overlap, responsive table behavior, and visible important state.

</topic>

<topic id="red-team" status="active" version="v2" wp="WP-0256" updated_at="2026-07-16">

# Red team

- Risk: `Needs attention` repeats historical failures that later succeeded. Control: preserve latest-attempt and batch-health labels; do not call raw failed-row counts unresolved videos.
- Risk: source filtering uses only loaded rows. Control: label it as a preview filter; canonical totals remain backend-provided and separate.
- Risk: advanced controls become undiscoverable. Control: keep queue state and the advanced disclosure visible in the toolbar, with destructive cleanup clearly labeled.
- Failure scenario: a user queues a URL and navigates immediately. Control: the returned receipt exposes job IDs and the default `Now` view sorts newest active work first.

</topic>
