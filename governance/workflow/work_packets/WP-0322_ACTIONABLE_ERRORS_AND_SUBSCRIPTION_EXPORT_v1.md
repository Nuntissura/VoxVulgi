---
file_id: WP-0322
file_kind: work_packet
updated_at: 2026-09-23
---

# Work Packet: WP-0322 — Actionable errors and automatic subscription export

## Metadata

- ID: WP-0322
- Owner: Claude
- Status: NEEDS_VALIDATION (installed 2026-09-24 01:49 build; operator validates through daily use — see status updates)
- Created: 2026-09-23
- Board: `../TASK_BOARD.md`

## Operator request (2026-09-23)

"make it in the user app data by default, in options make the export location selectable and add an export button. then take a look at all the errors, needs attention, stalled, network etc. i still do not understand what is going wrong and what the remedy is as a user, or what the user can do to fix it, except asking you to fix it. this is not good UI/UX"

## Evidence (live DB, read-only, 2026-09-23 ~23:40)

195 YouTube subscriptions carry a last error. Grouped: 58 `database runtime error: writer_admission_timeout` (app-internal contention, mostly during the S2 purge) — shown to the operator as "Network problem" because `failureStates.ts` matches `/timeout/`; 81 `yt-dlp … timed out after Ns` (YouTube not responding / throttling); 33 `pinned localhost PO provider payload failed integrity validation` / `selected download engine is unavailable` (YouTube helper broken at the time; fixed since, error stale until the next check clears it — success clears `last_error_message` at `subscriptions.rs:979-983`); ~12 `Playlists that require authentication…` / `does not have a videos tab` (wrong tab or needs sign-in); ~6 `playlist does not exist` / `channel was removed` / `account has been terminated` (source gone); 2 `youtube archive merge intent target or members are invalid` (internal). Failed jobs: 18 `writer_admission_timeout`, 9 `job stalled: no progress … watchdog`, 7 `managed MKV output is missing expected audio track metadata language=Some("ko")` (yt-dlp reports `ko`, the muxed file has no language tag, validation at `jobs.rs:30076-30104` rejects it — app defect), 3 `job canceled while running yt-dlp`, 1 `managed output is already being finalized`, plus localization/TTS failures. Instagram: `feedback_required` (Instagram checkpoint/rate limit).

## Research basis

Nielsen Norman Group, Error-Message Guidelines (updated Aug 2024) and Error-Message Scoring Rubric (Nov 2024): say what happened and why in human language, be precise, don't blame the user, offer a constructive fix with a high probability of success, prefer one-click fixes. Project addition: every problem states **who acts** (you vs the app) and, when the app acts, **what it will do and when**.

## Scope

A. Automatic subscription export
- Default folder `<app data>/exports/subscriptions` (resolved from `AppPaths`, disk-agnostic); operator-selectable folder in Options (Video Archiver module) with Browse, Reset to default, and **Export now**; daily automatic export from the runner tick (off-thread, never at startup, first run ≥ 1 h after start), keeping the newest 30 files matching the app's own pattern `subscriptions_<yyyymmdd_hhmmss>.json`. Export file = the existing importable YouTube export format (`export_youtube_subscriptions_json`), plus Instagram/TikTok subscriptions if export functions exist (else YouTube only, stated in UI). Last export time/path/count shown in Options. Tauri commands: `subscriptions_export_settings_get` → `{ dir, default_dir, is_default, keep, last_export_at_ms, last_export_path, last_export_count, last_error }`, `subscriptions_export_settings_set { dir: string | null }` (null = default), `subscriptions_export_now` → `{ path, count }`.

B. Engine root causes behind "errors" that are not the operator's
- B1 App-busy: a background subscription refresh or download that fails with `writer_admission_timeout`, `read_admission_timeout` or `database is locked` is requeued with a short delay (bounded retries) and must not set `last_error_message` or increment `consecutive_failures` on the subscription; after bounded retries it records an `app_busy` failure.
- B2 MKV audio language: when yt-dlp reported a language for a selected audio track and the muxed MKV stream has none, write the language (and title when reported) into the file with a stream-copy remux / property edit, then revalidate — never fail the download for a tag the app itself failed to write.
- B3 Expose on subscription rows (if not already): `last_error_at_ms`, `next_check_at_ms` (next scheduled refresh / backoff), `consecutive_failures`.

C. Actionable error catalogue (frontend, one source: `lib/failureStates.ts`)
Each class has: `kind`, short `label`, `whatHappened` (plain), `whoActs: "you" | "app"`, `appWillDo` (when app), `yourFix` (when you), and `actions[]` (buttons wired to existing commands). Classes (ordered; first match wins):
1. `app_busy` — writer/read admission timeout, database locked → app retries automatically; no action.
2. `youtube_blocked` — 429 / rate limit / "Sign in to confirm you're not a bot" (not cookie rejection) → app cooldown, next test time; actions Retry now, Return to normal, Slower pacing (opens Options pacing).
3. `sign_in_rejected` — cookies rejected / auth circuit → you; action Reconnect YouTube sign-in (Options).
4. `youtube_helper` — PO provider integrity/bootstrap, download engine unavailable → app repairs/rechecks; action Check again now; if persists Repair YouTube helper (existing install/repair command).
5. `youtube_not_responding` — yt-dlp timed out → app retries at next check; action Retry now; after ≥3 consecutive: suggest Slower pacing.
6. `wrong_link` — "does not have a videos tab", "Playlists that require authentication … webpage download" → you; actions Open on YouTube, Edit link (suggest /shorts, /streams, /videos), Connect sign-in.
7. `source_gone` — playlist does not exist, channel removed/does not exist, account terminated, 404 → you decide; actions Keep as archive (stop checking), Mark deleted; downloaded videos kept.
8. `members_only` — members-only/private → you; action Connect sign-in with an account that has access, or Keep as archive.
9. `stalled` — watchdog no progress → app retries; action Retry now.
10. `storage` — disk/permission → you; actions Open folder, Change folder.
11. `instagram_checkpoint` — `feedback_required`, challenge → you; open Instagram in your browser, complete any prompt, wait ~24 h; action Open Instagram, Retry later.
12. `internal` — merge intent invalid, managed output already finalizing, other app invariants → app; action Retry now; details copyable.
13. `unknown` — raw message shown with Copy details.
Every rendering shows `last failed <relative time>` and, when the app acts, `next automatic try <time>`. Status strip separates **Needs your action (N)** from **App is retrying (M)**; clicking filters the list. Applies to: Video Archiver subscription chips + detail, Instagram/TikTok archivers, Jobs "Needs attention", DownloadActivity rows. No new cards.

## Acceptance

- Classifier unit tests: every normalized message from the evidence section maps to the intended class (fixture list in the test).
- Live: after install, re-run the grouped inventory; the Video Archiver strip shows the split counts; the 58 app-busy subscriptions are no longer labelled "Network problem"; a snapshot of a subscription in each of `wrong_link`, `source_gone`, `youtube_not_responding` shows plain text + buttons.
- Export: Export now writes a file in the default folder; changing folder in Options persists; automatic export test (tempdir) keeps 30.
- B1/B2 engine tests; one bundled test/build run; version retained.

## Status updates

- 2026-09-23: Created from live evidence; engine (A-engine, B) and frontend (A-UI, C) dispatched in parallel.
- 2026-09-24: Implemented and installed (builds `20260924-005013`, `-014551`). Verified live: status strip reads "Needs your action: 22 / App is retrying: 170" with per-class chips (snapshot `governance/snapshots/WP-0322/status_strip_*.png`); "Network problem / Check your connection" no longer rendered; schema v59. Tests: engine 17 + regression 155/155 + follow-up 19/19; frontend 382/385 (3 pre-existing). Follow-ups in the same packet: who-acts colouring (app-handled grey "App retrying", operator action red "Your action needed"); `jobs_track_runtime_get` took 26–78 s because every identity-cache miss launched `yt-dlp --version` twice (measured 14–16 s per launch) — verified engine identity is now cached by executable size+mtime, measured afterwards 18 ms–0.9 s; export retries on busy database (was failing "database is locked").
- 2026-09-24: Status NEEDS_VALIDATION, reason: operator direction to stop cargo work on VoxVulgi (it slows the higher-priority Handshake build); not yet validated live — Export now after the retry fix, the explainer buttons on each error class, the daily automatic export (first run ≥1 h after start), and the MKV audio-language retag on a real download. The remaining intermittent "database is locked" on reads is tracked in WP-0323.
