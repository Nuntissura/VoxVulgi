---
file_id: WP-0281-REFINEMENT-v1
file_kind: work-packet-refinement
updated_at: 2026-07-27
---

<topic id="operator-request-and-live-basis" status="completed" version="v1" wp="WP-0281" owner="Codex" summary="Cross-source subscription dedupe is effective for the live NAS-backed archive before subscription recovery." updated_at="2026-07-27">

# Operator request

- Stop a playlist from downloading a video that is already covered by a channel page, `/videos`, or `/shorts` subscription.
- Apply the new behavior before resolving current subscription attention/error/not-found rows.
- Preserve NAS media, subscriptions, playlists, library metadata, and subtitle files; cleanup planning follows only after subscription recovery.

# Verified live basis

- The local app database is schema v28. The Video Archiver library root is the configured NAS UNC path.
- `media_source_association` has 53,225 subscription associations, including 773 IDs shared by playlist and feed sources, but `media_source_membership` has zero rows.
- The queue currently has 946 queued subscription refreshes and 104,040 queued direct-download jobs. At the observed snapshot no subscription refresh was running.

# Scope edges

- In scope: additive membership backfill, refresh-cohort source priority, canonical-claim regression tests, packaged desktop verification, and read-only attention diagnosis.
- Out of scope: moving, deleting, renaming, hashing, or reorganizing NAS files; subtitle relocation; deleting subscriptions or playlists; mutation of third-party databases.

</topic>

<topic id="research-basis-and-selection" status="completed" version="v1" wp="WP-0281" owner="Codex" summary="Canonical IDs and atomic claims were used rather than folder ownership or archive-file-only suppression." updated_at="2026-07-27">

# Sources checked

- yt-dlp documents `--download-archive` as a success-only record for avoiding repeat downloads: <https://github.com/yt-dlp/yt-dlp/wiki/FAQ>.
- yt-dlp’s README documents archive-write semantics and structured playlist extraction: <https://github.com/yt-dlp/yt-dlp/blob/master/README.md>.
- yt-dlp issue discussion documents that channel-page enumeration can surface duplicates and needs context-aware handling: <https://github.com/yt-dlp/yt-dlp/issues/5555>.
- The existing project canonical-claim implementation, source-association data, and WP-0275/WP-0276 contracts were inspected.

# Selected approach

- Backfill durable `media_source_membership` rows from existing VoxVulgi-owned associations with the subscription row as the source-kind authority.
- Within a single refresh cohort, dispatch feed pages before playlists. Feed enumeration claims a canonical video ID first; a later playlist enumeration records its membership and sees the ID as active/present.
- Do not skip solely because a historical membership exists. The existing atomic canonical claim remains the physical-copy gate so missing media can still be repaired from a playlist.

# Rejected options

- Folder-name or path deduplication: it cannot prove video identity and can split subtitles from media.
- Sharing only per-subscription archive text files: it has no source priority and cannot safely distinguish a retained physical item from stale history.
- Canceling or rewriting the live job backlog as part of this change: it broadens risk; canonical queue reconciliation remains a separately attributable operation.

</topic>

<topic id="roi-red-team-and-verification" status="completed" version="v1" wp="WP-0281" owner="Codex" summary="Additive backfill closed the live-data gap while priority reduces wasted NAS transfers." updated_at="2026-07-27">

# High-ROI additions

- Backfill existing associations rather than waiting for future refreshes. This makes the historical 773 playlist/feed overlaps observable immediately and reuses data already captured by the engine.
- Keep playlist associations even when a feed wins the physical claim. That lowers future cleanup and source-grouping work because the relationship is retained without another NAS copy.
- Apply priority at cohort dispatch, not by globally reordering all subscriptions. This reuses existing batch IDs and prevents a steady stream of feed refreshes from starving playlists.

# Risks, plausible failures, and controls

- A historical feed membership is stale while the file is missing. Control: membership never suppresses a missing/ready canonical preflight; only active/present canonical state skips a download.
- An old queued cohort starts in playlist order. Control: scheduler priority applies when it selects queued refresh rows, not only when creating new rows.
- Backfill is large enough to contend with the job runner. Control: one indexed additive `INSERT OR IGNORE ... SELECT` migration, no NAS reads, no job rewrites, and focused migration proof.
- Playlist starvation behind continuously refreshed feeds. Control: order sources only inside the same queued refresh cohort; cohort age remains the first ordering key.
- Subtitles become detached by cleanup. Control: this packet performs no filesystem mutation; the later cleanup plan must retain subtitle paths as explicit inventory evidence.

# Acceptance and verification

- Schema migration backfills known association rows with correct playlist, videos, shorts, and channel source kinds and is idempotent.
- A same-cohort `/videos` refresh is selected before an earlier-created playlist refresh; cohort ordering remains stable across cohorts.
- Canonical present/active/missing semantics remain unchanged and prevent duplicate physical download jobs.
- Focused engine tests, desktop build, and headless bridge proof pass. The packaged app reports the upgraded schema and membership count without changing operator media.

</topic>
