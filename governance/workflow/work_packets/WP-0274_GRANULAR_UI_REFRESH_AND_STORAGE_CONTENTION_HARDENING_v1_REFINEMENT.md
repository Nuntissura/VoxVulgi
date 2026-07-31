---
file_id: WP-0274-REFINEMENT-v1
file_kind: work-packet-refinement
updated_at: 2026-07-22
---

<topic id="operator-request" status="active" version="v1" wp="WP-0274" updated_at="2026-07-22">

# Operator request

- Progress elements should update fluidly without an apparent full-GUI refresh.
- Investigate and reduce intermittent app freezes, including the operator's suspicion that the large NAS library is being loaded.

</topic>

<topic id="verified-current-state" status="active" version="v1" wp="WP-0274" updated_at="2026-07-22">

# Verified current evidence

- The latest report and raw trace contain regular `worker_alive` events but no `freeze_detected`, `freeze_recovered`, or `event_loop_skew` events in the inspected window.
- Measured slow commands include `youtube_subscriptions_archive_stats` at 21.6 s, `jobs_overview`/`jobs_track_runtime_get` around 8.5 s maximum, subscription activity around 4.8 s, and library/history calls around 2.7 s.
- This evidence does not prove a NAS-only cause; it proves DB/disk command contention and broad refresh cost. NAS reachability/latency remains a separate hypothesis to test.
- Existing activity hooks prevent overlapping polls, but multiple broad page-level calls can still contend and cause large component updates.

</topic>

<topic id="research-and-selection" status="active" version="v1" wp="WP-0274" updated_at="2026-07-22">

# Research basis

- SQLite WAL permits readers and a writer to overlap, but long-lived readers can prevent checkpoint completion and grow/block later work: https://sqlite.org/wal.html
- SQLite partial indexes reduce the indexed row set and can improve active-queue reads and write cost when predicates match: https://www.sqlite.org/partialindex.html
- Tauri distinguishes low-volume events from ordered streaming channels and requires listener cleanup in SPA components: https://v2.tauri.app/develop/calling-frontend/
- React documents stable snapshots/subscriptions and memoized calculations as tools for avoiding unrelated rerenders: https://react.dev/reference/react/useSyncExternalStore and https://react.dev/reference/react/useMemo
- VoxVulgi trace, freeze-report, external-watch, polling, command timing, and page activation paths were inspected before selecting changes.

# Selected approach

- Separate fast active-progress projections from slow canonical counts/history/archive statistics and schedule each only while its page/surface is visible.
- Cache or incrementally maintain expensive archive stats; move filesystem checks off UI command paths and bound NAS probes with explicit unreachable state.
- Preserve stable row/object identity and isolate progress rows so a percentage update does not replace the full page tree.
- Add command/query/row-count/storage-root trace detail plus headless performance state; use the sibling external watch for exact freeze reproduction.
- Retain WAL and short read transactions; measure before adding checkpoints or cache complexity.

# Rejected options

- Declaring the NAS the root cause from current evidence: unproven.
- Raising timeouts: hides contention and makes freezes longer.
- Aggressive global polling for visual smoothness: increases the measured failure mode.
- Disabling subscriptions while foreground work runs: violates independent-track behavior.

</topic>

<topic id="scope-acceptance-red-team" status="active" version="v1" wp="WP-0274" updated_at="2026-07-22">

# Base scope and gaps closed

- Inventory visible-page polling and remove inactive/heavy duplicate refreshes.
- Add lightweight active projections, stable reconciliation, bounded storage probes, cached/incremental archive stats, and trace evidence.
- Use the existing freeze detector, bridge dump, and `vvwatch.cmd` for real installed-app verification.

# High-ROI additions

- A diagnostics polling table exposes cadence, last duration, rows, skipped overlaps, and errors for human/model debugging.
- A storage-root health cache distinguishes NAS unreachable/slow/missing from database contention and can support dedup repair safely.
- Shared projection contracts from WP-0272 avoid building a second progress system.

# Risks, failures, and controls

- Caches can become stale. Control: versioned invalidation on terminal/library mutations, visible last-updated time, and manual refresh.
- Faster progress may starve writes. Control: indexed bounded queries, one in flight, jitter/backoff on busy, and measured budgets.
- Network probes may hang OS threads. Control: bounded worker-thread probes, never synchronous metadata recursion, explicit timeout state.
- Component memoization may hide updates. Control: immutable keyed reconciliation and transition tests for every mutable field.
- No freeze may reproduce during validation. Control: report runtime evidence honestly, retain before/after command timings, and keep WP open unless required installed-app proof passes.

# Acceptance

- Progress updates use bounded lightweight calls and preserve unrelated row/component identity.
- Expensive stats/history/library calls do not run at progress cadence or while their surface is inactive.
- Storage unreachable/slow and DB busy are diagnosable as distinct states.
- Installed-app traces show materially lower command frequency/duration under the representative backlog, and headless visual proof shows responsive live bars without whole-page loading flashes.

</topic>
