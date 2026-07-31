---
file_id: WP-0272-REFINEMENT-v1
file_kind: work-packet-refinement
updated_at: 2026-07-22
---

<topic id="operator-request" status="active" version="v1" wp="WP-0272" updated_at="2026-07-22">

# Operator request

- The Video Archiver's Single Videos subtab must list every submitted batch member as queued, active with a progress bar, failed, or downloaded.
- Progress bars throughout the app must feel lively and update as elements, without visibly refreshing the whole GUI.

</topic>

<topic id="verified-current-state" status="active" version="v1" wp="WP-0272" updated_at="2026-07-22">

# Verified current state

- The Single Videos surface reads `library_youtube_single_history`, which contains canonical completed library items but no queued/running job members.
- `JobsPage` displays persisted numeric progress and preserves equal job rows, but polling still invokes broad snapshots whose measured calls reached multiple seconds under load.
- Tauri commands already persist job progress; the missing product surface is a bounded active projection and targeted frontend reconciliation, not invented progress.
- Canonical entities are submitted batch members, job attempts, terminal archive lineage, outputs, and UI projections; the completed library list cannot represent active work.

</topic>

<topic id="research-and-selection" status="active" version="v1" wp="WP-0272" updated_at="2026-07-22">

# Research basis

- Tauri v2 recommends channels for fast ordered streaming such as download progress and documents correct listener cleanup: https://v2.tauri.app/develop/calling-frontend/
- React documents stable external-store snapshots and targeted subscriptions through `useSyncExternalStore`, while recommending ordinary state where it remains sufficient: https://react.dev/reference/react/useSyncExternalStore
- SQLite partial indexes are suited to the small active subset of a much larger job table: https://www.sqlite.org/partialindex.html
- yt-dlp's official README exposes real download progress and download archives but an archive alone does not verify whether a physical file still exists: https://github.com/yt-dlp/yt-dlp

# Selected approach

- Add one bounded canonical active/recent projection per track/batch and reconcile changed rows by stable job ID.
- Poll the small persisted projection at a lively bounded cadence; refresh heavy history only when a member crosses a terminal boundary.
- Use CSS width transitions and truthful indeterminate stage animation where no numeric percentage exists; never synthesize fake completion percentages.
- Reuse batch IDs, persisted jobs, progress parser, lineage, existing tables, and polling activity gates.

# Rejected options

- Full library/history refresh on every progress tick: measured DB/disk contention makes this the opposite of the requested behavior.
- Frontend-only submitted rows: they disappear after restart and cannot prove canonical queue state.
- A new global state framework: unnecessary rework for stable-ID reconciliation.

</topic>

<topic id="scope-acceptance-red-team" status="active" version="v1" wp="WP-0272" updated_at="2026-07-22">

# Base scope and gaps closed

- Persist/return submitted single batch membership and show every member in the Single Videos list.
- Add queued/running/held/failed/succeeded presentation, numeric or indeterminate progress, and batch counts.
- Split lightweight progress polling from heavy history/library reads on Jobs and Video Archiver.

# High-ROI additions

- A shared `LiveJobProgressRow` contract serves Jobs, Video Archiver, bridge dumps, and later tracks.
- Stable row components and motion-reduction support improve responsiveness and accessibility with little extra code.
- Terminal-transition refresh avoids stale completed history without constant NAS/library work.

# Risks, failures, and controls

- Event loss or page remount could lose progress. Control: persisted DB projection remains canonical; streaming/polling is only an accelerator.
- Large batches could render thousands of DOM rows. Control: bounded recent/active projection, pagination/virtual-style windowing, and canonical batch counts.
- Fast polling could amplify DB locks. Control: partial index, no per-row hydration, self-rescheduling non-overlap, page visibility gates, and trace timing.
- Animated bars could imply false progress. Control: numeric width only from persisted progress; indeterminate styling is labeled by stage.

# Acceptance

- Every member of a submitted batch appears immediately and remains recoverable after restart.
- Numeric progress changes without replacing unrelated rows or reloading completed history.
- Jobs and Single Videos show consistent state for the same job ID.
- Representative backlog timing, frontend render tests, and installed-app headless proof pass.

</topic>
