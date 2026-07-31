# Work Packet: WP-0255 - Video Archiver subscription manager redesign + honest per-subscription progress

## Status

SUPERSEDED by WP-0280

## Owner

Claude (Opus 4.8)

## Note on this file

The WP-0255 row has existed in `TASK_BOARD.md` (IN_PROGRESS, partially shipped in desktop 0.1.74-0.1.80) but the WP **file** was never created — the prior session tracked it inline in the board row. This file is authored now to (a) resolve that drift and (b) capture the new operator scope from 2026-06-30/07-01. Prior shipped work is preserved below; this session adds the subscription-state/redesign scope on top.

## Operator Request Preserved

- 2026-06-15 (original): "redo the whole video archiver, too many buttons/fields"; "no more mentioning of legacy (UI + code identifiers), keep provenance values".
- 2026-06-30/07-01 (this session): "i find the app unreadable, main friction is my playlists and subscriptions. i never know the state of them, when is the last time it gets updated, if i press update all, what is getting updated currently, how many videos are out of how many are being updated per subscription/playlist, how many videos does a subscription have."
- "the subscription list also scrolls horizontal ... that scroll bar is only at the bottom so i need to scroll all the way to the bottom to scroll horizontal. mouse wheel scrolling up and down on the list does not work only when the cursor is not on the list but at other parts of that window."
- "the buttons 'save subscriptions, clear editor, update all, stop, queue due active, import/export, migration (does not even have a real button function), refresh subscriptions' are unclear in what it actually does or even work."
- "do a visual inspection to find a better cleaner way to show my playlists and how to address my issues."
- 2026-07-01 (follow-up): "there is also some kind of cron job for every subscription/playlist this is set to 60min by default, can we change this in hours. i do not expect videos to be uploaded that frequent per subscription." → the per-subscription `refresh_interval_minutes` should be expressed and edited in **hours** with a sensible larger default.
- Operator decisions (this session, via AskUserQuestion): counts model = the operator's natural flow (already-downloaded · new found this refresh · downloaded-of-new-batch · total in playlist), cheap because the refresh already enumerates; layout = **master-detail manager with the slim all-subscriptions status overview kept on top**; scope = do this + WP-0256 together, then one build + visual verify.

## Prior Shipped State (preserve - do not regress)

Shipped cumulatively in desktop 0.1.74-0.1.80 under this WP id:
- Un-gated the YouTube playlist/subscription tab (fixed the blank-tab bug; shows in any view mode).
- Added `Update all now` + `Stop` buttons wired to the WP-0254 engine commands (`youtube_subscriptions_update_all`, `youtube_subscriptions_stop_recurring`).
- Collapsed library-management controls + rare subscription import/export buttons behind disclosures.
- Removed visible "legacy" wording from subscription labels/help (code identifiers + dead legacy-archive card removal still pending, tracked but NOT in this session's scope).

## Intent

Make the operator always able to answer, at a glance: which subscriptions exist, their per-subscription progress (downloaded / new-found / total), when each was last checked, what is updating right now, and what each control does — by replacing the unreadable 15-column horizontally-scrolling table with a master-detail manager fronted by an all-subscriptions status strip, and by backing it with honest persisted progress data. Build on the finished WP-0254 engine + WP-0257 anti-bot pacing; do not re-introduce cards (`build_rules.md` No More Cards).

## Scope (this session)

### 2a - Honest per-subscription progress data (engine: `db.rs`, `subscriptions.rs`, `jobs.rs`, `lib.rs`)
- Additive **v18** migration (explicit `PRAGMA user_version` stepping, per WP-0126/WP-0254 pattern; idempotent `ensure_column`): add to `youtube_subscription`:
  - `last_checked_at_ms INTEGER` — written on refresh **completion** (success and handled failure), distinct from `last_queued_at_ms`. This is the truthful "last updated/checked" the operator asked for.
  - `upstream_total` INTEGER — count of entries seen in the most recent `--flat-playlist` enumeration (the playlist/channel length). This is "Y" in "X of Y".
  - `last_new_found` INTEGER — count of new (non-archived) entries found at the last refresh.
  - `last_refresh_queued` INTEGER — count of child downloads enqueued at the last refresh (the "new batch" size).
- In the refresh job (`expand_yt_dlp_entries` / `youtube_subscription_refresh_*` in `jobs.rs` ~6700-6869): the enumeration already computes `entries`, `new_urls`, `skipped_archived`; persist `upstream_total = entries.len()`, `last_new_found = new_urls.len()`, `last_refresh_queued`, and stamp `last_checked_at_ms` on completion. Currently these are only logged (root cause confirmed: `youtube_subscription_refresh_done` log line, jobs.rs ~6832-6842).
- Raise the enumeration cap so big playlists report a true total: the current `max_items` default 200 (`--playlist-end`, jobs.rs ~6713) caps `entries`. For the **count** pass use an uncapped/high `--flat-playlist` tally (one cheap request per playlist; anti-bot risk is burst volume, already paced by WP-0257 recurring cooldown, NOT a single listing). Keep the download fan-out capped as today; only the count is uncapped.
- Surface the new fields on `YoutubeSubscriptionRow` + the `youtube_subscriptions_list` projection so the UI receives them without a second command. Keep the existing `archiveStats` (downloaded = yt-dlp archive id count) as the "X downloaded".
- Add a derived per-subscription pending/active count exposed for the UI: count of queued/running `download_direct_url` child jobs carrying this `subscription_id` (or reuse `active_youtube_subscription_refresh_ids`); used for "downloading N now".

### 2b - Subscription manager UI (frontend: `LibraryPage.tsx`, `App.css`)
- Replace the subscription `<table className="table-wrap table-wrap-wide table-wrap-sticky-actions">` region (LibraryPage.tsx ~4193-4307) with a **master-detail manager**:
  - **Top status strip** (no card): total subs · # updating now · # errored/backoff · last sync time. Honors the active group filter.
  - **Left list**: one row per subscription = title + type icon + a compact progress bar (`X / Y` downloaded with the new-found delta) + status pill (Idle / Downloading… / Error-backoff). Scannable; selectable; virtualization or capped render acceptable for 258+ rows but must not silently hide rows (log any cap per VV-SOT).
  - **Right detail pane**: full fields for the selected subscription (URL, target mode/path, folder map, preset, interval, last checked, last queued, backoff) + the per-row actions (Queue, Open folder, Edit, Delete, Seed continuity) moved here.
  - Eliminates the wide horizontal table entirely → kills the scroll bug by removing the over-wide table (see 2c).
- Editor form: keep, but the `Save subscription` / `Clear editor` toolbar drives the same editor (consider moving New/Edit into the detail pane). Preserve all existing handlers (all are wired; none dead).

### 2c - Scroll + wheel bug (frontend: `App.css`, `LibraryPage.tsx`)
- Root cause (confirmed in `App.css` 1152-1197): `.table-wrap` is `overflow:auto` with **no height cap** → its horizontal scrollbar sits at the bottom of all 258 rows (only reachable after scrolling the page down); `.table-wrap-wide table{min-width:1180px}` forces overflow; `.table-wrap` `overscroll-behavior:contain` blocks wheel scroll-chaining to `.content` when the wrap has no vertical overflow → dead wheel over the list.
- Primary fix: the master-detail layout (2b) removes the over-wide table so no horizontal scroll exists; the left list scrolls vertically and chains to the page normally.
- For any remaining `.table-wrap` tables elsewhere (Instagram subs, media): if a wide table must stay, give it a bounded `max-height` so the horizontal scrollbar is reachable without scrolling to the end, and drop/relax `overscroll-behavior:contain` so wheel chains to the page. Do not regress other surfaces.

### 2d - Button clarity (frontend: `LibraryPage.tsx`)
- Disambiguate the three near-identical verbs with clearer labels + inline one-line help: `Update all now` (force-queue ALL active now), `Queue due active` (queue only those past their interval), `Refresh subscriptions` (reload the list — queues nothing; consider renaming to "Reload list" or making it an auto/implicit refresh).
- Fix the real bug: with a group filter active, `Update all now` ignores the filter and force-updates ALL subscriptions while its label shows the group's count (LibraryPage.tsx:2278-2295 vs label using group-filtered `activeSubscriptionCount`). Either scope it to the group (add a group arg path) or relabel honestly to "Update ALL (ignores filter)".
- Keep `Import / export & migration` as a labeled disclosure (it is a real `<details>` with 5 working buttons) but make it obviously a section, not a dead button.

### 2e - Refresh interval in hours (frontend: `LibraryPage.tsx`; engine default: `subscriptions.rs`)
- Storage stays canonical in minutes (`refresh_interval_minutes`, no migration) to preserve the 258 existing rows; the UI expresses and edits the interval in **hours** (input/preset in hours, convert hours×60 → minutes on save; display minutes/60 as hours).
- Offer common presets (e.g., 1 h / 3 h / 6 h / 12 h / 24 h / 48 h / weekly) plus a free numeric hours field. Keep the existing clamp (engine MIN 5 min / MAX 10080 min = 7 days); UI minimum becomes 1 h.
- Change the **new-subscription default** from 60 min to a less-frequent default (operator: "i do not expect videos that frequent") — set to 12 h (720 min) for new subs; existing subs keep their stored value (shown as hours). Surface the chosen default to the operator; trivially changeable.
- The detail pane "Refresh: every N h" + the editor label read in hours, not minutes.

Out of scope (tracked, not this session): per-lane Options UI control, code-identifier "legacy" renames, dead legacy-archive card removal, Media-Library/Video-Archiver single-history view collapse.

## Research Basis

- Live app inspection (bridge, no focus steal): captured + visually inspected `video_ingest` subscription table, top toolbar, and `jobs` page on build 0.1.80 (`governance/snapshots/audit_ux_2026-06-30/`). Confirmed the 15-column table, full URL/target-path columns, `Idle`+raw-count opacity, and the dev "panel scrolls" hint.
- 6-agent read-only understand workflow `wf_30f244e6-69e` (4 structured results; scroll + governance agents re-derived by hand). Canonical findings:
  - Video Archiver = `LibraryPage.tsx` mode=`video_ingest`, gated `videoArchiverTab==='youtube_recurring'`; table 4193-4307 (15 columns); toolbar 4132-4184; groups 3884-3975.
  - Subscription data: downloaded = `youtube_subscriptions_archive_stats` (fs archive count, not a column); **`upstream_total` not stored anywhere** (only transient `expand_yt_dlp_entries`, capped 200, logged); only `last_queued_at_ms` exists (no completion timestamp); status derived (active_refresh_ids + backoff). Confirmed via `subscriptions.rs` struct 35-57, `db.rs` 373-392, `jobs.rs` 6700-6869.
  - All toolbar buttons wired to registered Tauri commands; the only behavioral bug is the group-filter scope of `Update all now`.
- Scroll root cause confirmed by hand in `App.css` 1152-1197 (.table-wrap no max-height + min-width:1180px + overscroll-behavior:contain).
- Builds on WP-0254 (lanes + update-all/stop engine commands, done), WP-0257 (anti-bot pacing/cooldown so per-playlist count fetch is safe), WP-0161 (Type/Downloaded/Status columns; detected-count was deferred — this WP delivers it).

## Acceptance Criteria

- v18 migration runs additively on the real DB (additive columns + idempotent ensure_column + user_version step; no row deletes). `cargo test -p voxvulgi_engine` green.
- A completed refresh writes `last_checked_at_ms`, `upstream_total`, `last_new_found`, `last_refresh_queued` to the subscription row (unit-tested where practical).
- The subscription manager shows, per subscription: downloaded X / total Y, new-found delta, status, last checked — and an all-subscriptions status strip on top.
- No horizontal scrollbar on the subscription surface; mouse wheel scrolls the list/page over the list region (visually verified via bridge snapshot on the new build).
- The three update/refresh verbs are unambiguous; `Update all now` no longer under-reports its scope under a group filter.
- The per-subscription refresh interval is shown and edited in hours; new subs default to a less-frequent interval (12 h); existing stored minute values render correctly as hours and round-trip on save without data loss.
- No user subscription/playlist/library data deleted or reset (User Data Preservation policy).
- `build_rules.md` No More Cards honored (status strip / master-detail / list, not new cards).

## Red-Team

- Per-playlist uncapped count fetch adds YouTube enumeration: bounded by WP-0257 recurring-lane cooldown (one source at a time) + `--flat-playlist` being a single cheap request; the count reuses the refresh's existing enumeration, no extra burst.
- v18 migration on 122k-row DB slow at startup: ADD COLUMN is metadata-only; no backfill UPDATE needed for the new nullable columns; idempotent.
- Master-detail render of 258+ subs janky: cap/virtualize the left list; never silently hide rows (log any cap).
- Removing the wide table could hide a field the operator relied on: every current column is preserved in the detail pane; nothing dropped.
- `last_checked_at_ms` written only on success would lie on persistent failure: write it on handled completion (success and recorded failure) so "last checked" is truthful even when the refresh errored.
- Counts drift (archive count includes seeded/imported ids): label "downloaded" honestly as archive-known; keep `upstream_total` separate so the bar can't claim more than upstream.

## Notes

- 2026-07-01: WP file authored (resolving the missing-file drift) + scope extended to subscription state visibility, master-detail redesign, scroll/wheel fix, and button clarity per the operator's 2026-06-30 request and AskUserQuestion decisions. Implementation + build + visual verification follow in this session, paired with WP-0256.
- 2026-07-27: Superseded by `WP-0280_VIDEO_ARCHIVER_JOBS_COHESIVE_WORKSPACES_v1.md`. All shipped behavior and proof above remain preserved; every unresolved Video Archiver simplification, progress, control-clarity, and render-bounding requirement is carried forward into the consolidated packet.
