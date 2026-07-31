---
file_id: WP-0271-REFINEMENT-v1
file_kind: work-packet-refinement
updated_at: 2026-07-22
---

<topic id="operator-request" status="active" version="v1" wp="WP-0271" updated_at="2026-07-22">

# Operator request

- Jobs/Queue must have separate subtabs for YouTube single downloads, subscriptions, Instagram, other video, Image Archive, and Localization.
- A subtab must represent the complete canonical track rather than merely filtering the rows already loaded on screen.
- Subscription jobs must carry their channel, playlist, or page name in queued, running, failed, and completed states.

</topic>

<topic id="verified-current-state" status="active" version="v1" wp="WP-0271" updated_at="2026-07-22">

# Verified current state

- `JobsPage.tsx` has a `selectedTrack` dropdown that filters `jobs_overview`'s bounded loaded preview; its own copy explicitly says canonical totals are unchanged.
- The engine already persists six product tracks and exposes canonical per-track totals through `jobs_track_runtime_get`.
- Subscription context is assembled from multiple optional lookups, so rows whose lookup path is absent lose the human source name.
- The canonical entities are job attempts, source subscriptions/pages, batches, outputs, and bounded UI projections; a visible filtered row set is not canonical track state.

</topic>

<topic id="research-and-selection" status="active" version="v1" wp="WP-0271" updated_at="2026-07-22">

# Research basis

- Hugging Face Jobs documents server-side status/label filters and separate inspect/log paths: https://huggingface.co/docs/hub/jobs-manage
- qBittorrent's WebUI API uses backend category/filter parameters over canonical transfer state rather than grouping the current rendered slice: https://github.com/qbittorrent/qBittorrent/wiki/WebUI-API-%28qBittorrent-4.1%29
- SQLite partial indexes cover a selected row subset and can improve both query and write cost when the query predicate matches: https://www.sqlite.org/partialindex.html
- GitHub, Hugging Face, Reddit, Civitai, and X/social searches were checked for adjacent queue/category patterns; no result justified replacing VoxVulgi's current canonical overview and no-card layout.

# Selected approach

- Extend the bounded Jobs overview contract with a canonical `track` selector and matching indexed query.
- Replace the loaded-preview dropdown with accessible subtabs whose active state drives the backend query.
- Persist a source display snapshot on job creation so never-started and failed jobs do not depend on a later subscription lookup; retain source IDs for live canonical linking.
- Reuse the existing track strip, batch rows, detail reveal, failure classifier, and bridge state.

# Rejected options

- Client-only filtering: it can hide matching canonical jobs that fell outside the bounded response.
- One card per lane: violates the existing no-new-card rule and wastes vertical space.
- Deriving subscription names from URLs at render time: URLs are not reliable human labels and can change.

</topic>

<topic id="scope-acceptance-red-team" status="active" version="v1" wp="WP-0271" updated_at="2026-07-22">

# Base scope and gaps closed

- Add canonical track filtering to Jobs queries, tabs, search, empty/loading/error copy, bridge inspection, and tests.
- Persist and render source display name/type/URL snapshots for every subscription child job.
- Keep all-track canonical counts visible while clearly labeling the selected track's bounded rows.

# High-ROI additions

- Stable tab IDs make headless and parallel-model navigation deterministic using existing bridge dumps.
- Persisted source snapshots reuse enqueue-time data and close metadata gaps on never-started, failed, retried, and deleted-source paths.
- URL/type in detail gives repair context without cluttering the primary row.

# Risks, failures, and controls

- A tab could still query only the current preview. Control: pass track to SQL and prove a match outside the unfiltered first page is returned.
- Backfilled jobs may lack source names. Control: join canonical subscriptions where possible and show an explicit `Source unavailable` state, never a guessed label.
- Track switches could race. Control: request generation guards and per-tab loading state prevent stale responses replacing the active tab.
- Added indexes could slow writes. Control: use a narrow/partial index and compare representative query plans and enqueue timing.

# Acceptance

- Every lane has a discoverable subtab and the selected tab is sent to the backend.
- Canonical totals never derive from rendered rows; selected-track result counts are explicitly bounded.
- Subscription rows show the source channel/playlist/page name across all job states and retries whenever it was known at enqueue time.
- Headless snapshot/dump and backend tests prove tab truth, keyboard/readability behavior, and no new cards.

</topic>
