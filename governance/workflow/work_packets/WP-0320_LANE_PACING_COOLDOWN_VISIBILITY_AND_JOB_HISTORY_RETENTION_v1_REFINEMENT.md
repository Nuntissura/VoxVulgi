---
file_id: WP-0320-REFINEMENT
file_kind: refinement
updated_at: 2026-09-23
---

<topic id="operator-request" status="active" version="v1" wp="WP-0320" updated_at="2026-09-23">

# Operator request (2026-09-23)

Keep SQLite; close the database-engine chapter. Fix all findings of the live investigation. Almost everything should have two lanes — single item vs recurring subscription — with separate Options values; image archive and Localization Studio are single-lane. The current YouTube subscription default is too aggressive; "Safest" was set and still got blocked; even when blocked it must reattempt after a cooldown (length to be decided from evidence). Evaluate deleting all old jobs so the queue starts fresh while subscriptions survive. Use cheaper subagents; one bundled build/test run.

</topic>

<topic id="evidence" status="active" version="v1" wp="WP-0320" updated_at="2026-09-23">

# Evidence (v0.1.204, live)

- Diagnostics renders `null` tool state as "not installed" (`DiagnosticsPage.tsx:3613,3634,3646,3678,3724`); `diagnostics.tools-core` flights took 56.5 s and 83.7 s; version probes spawn ~7 children with a 3600 s timeout (`engine/src/tools.rs:18-22`).
- `database_locked` rows at 11:36:52/11:37:02/11:37:31 were dropped (`trace_row_too_large`, 264 KB > 256 KB) because the event is on the process-snapshot list (`lib.rs:5112-5120`).
- 985 `media_path_probe_completed` rows 10:53–12:03, avg 878 ms, max 11.0 s, all `present` (`library.rs:299`).
- `job` table: 339,092 rows; 254,348 canceled + 39,238 failed `download_direct_url`; 14,309 queued downloads; 2,706 queued refreshes. `jobs_overview` counts scale with terminal rows (`jobs.rs:5465-5485`).
- Protection state: `download` mode `cooldown` entered 07:07:59 after three `corroborated_rate_limited` transitions (06:46, 06:53, 07:07); `next_eligible 13:07:59` (6 h dwell). Preset on disk = "Safest" (fragments 1, sleep 8, requests 6) since 2026-09-22 10:54.
- YouTube pacing is global across `youtube_single`/`youtube_recurring` (`jobs.rs:20837-20855`); Instagram/TikTok already have per-track policies (`config.rs:828-870`).
- Dedupe never reads terminal job rows (`library.rs:1149`, `jobs.rs:13652-13710`); FK `ON DELETE RESTRICT` only on localization tables (`db.rs:1866-1868,1911`); `flush_jobs_cache` deletes files before rows (`jobs.rs:7160-7217`).
- Failure label "Network problem" came from regex `/timed out|timeout/` (`failureStates.ts:192`) on "managed_yt_dlp timed out after 900s".

</topic>

<topic id="research-basis" status="active" version="v1" wp="WP-0320" updated_at="2026-09-23">

# Research basis

Sources checked (2026-09-23): yt-dlp issue tracker (#14921 rate-limit handling; #7143/#13770 HTTP 429), yt-dlp.net 429 guide, DEV Community yt-dlp 2026 guide, Tube Archivist settings docs, Pinchflat wiki. Patterns found: YouTube rate-limit blocks documented as "up to an hour"; probe no more than once per 10 minutes; channel archiving guidance `--sleep-requests 2 --sleep-interval 5 --max-sleep-interval 15 --limit-rate 2M`; Tube Archivist requires ≥10 s between requests with ±50 % jitter. Reuse: existing `ProviderTransferPolicy`/`policy_for_track` shape for Instagram/TikTok; existing `DownloaderPolicyMode` ladder and canary exit; existing `delete_terminal_jobs_by_ids`. Rejected: per-lane cooldown state (block is per identity); fixed 6 h dwell (no field basis); database-engine change (WP-0315 `reject`). Selected: lane floors over the preset; escalating dwell 1 h → cap 6 h; backup-first purge reusing existing delete path with corrected ordering. Risks and mitigations in the red-team topic. Validation: bundled engine/desktop tests plus one headless app-boundary check.

</topic>

<topic id="red-team" status="active" version="v1" wp="WP-0320" updated_at="2026-09-23">

# Red team

- Purge deletes a localization job → FK RESTRICT aborts the transaction. Control: exclude localization job types by allow-list; test.
- Purge deletes files before rows and the transaction rolls back → orphaned rows. Control: rows first, files after commit; test.
- Lane floor below preset makes recurring faster than intended. Control: floor semantics (max sleep, min fragments); test both lanes.
- Cached tool status hides a fresh install. Control: cache key includes executable mtime; Refresh forces.
- Cached NAS `present` hides a deletion for ≤60 s. Control: TTL 60 s, invalidation on job completion; acceptable per operator.
- Trace snapshot capping removes attribution. Control: keep `cmd`, `message`, `busy_attribution`; cap child list to 25 entries; test row < 256 KB.
- Retention default deletes history the operator wanted. Control: default 30 days, 0 disables, Options field, purge always backs up first.

</topic>

<topic id="microtasks" status="active" version="v1" wp="WP-0320" updated_at="2026-09-23">

# Microtask plan

- MT-1 (engine A): lane policies + effective profile floor + cooldown escalation + gate snapshot fields + purge/retention commands + tests.
- MT-2 (engine B): bounded/cached tool probes + `force` + trace snapshot cap + NAS observation cache + tests.
- MT-3 (frontend): Diagnostics "checking…", Video Archiver gate strip + Return to baseline, failure reclassification + last error, Options lane/cooldown/retention fields, Jobs purge control + tests.
- MT-4: bundled build/test, headless app-boundary check, proof bundle, spec sync (PRODUCT_SPEC 8.x lanes/retention, TECHNICAL_DESIGN cooldown), task board.

</topic>
