# Work Packet: WP-0248 - Jobs old batch retry, delete, and title context

## Metadata

- ID: WP-0248
- Owner: Codex
- Status: REVIEW
- Created: 2026-06-02
- Target milestone: Desktop archive/operator usability

## Operator Request Preserved

- "i still can not download a past failed batch ID 42a89117 and ID 2de9cc9c, what is going wrong?"
- "there is also no option to delete single failed jobs (single or part of a batch)"
- "the URL now shows but not the video title for yourube videos in the jobs/queue panel."
- Follow-up: "i still have a lot of 'failed: model/tool install failed ... ERROR: [youtube] ... Sign in to confirm you're not a bot ... VoxVulgi saved YouTube cookies were supplied, but YouTube rejected them ... run the auth preflight before queueing another subscription batch.'"
- Follow-up 2026-06-03: Jobs/Queue must become a trustworthy recovery and inspection surface for failed, retried, bundled, and batched YouTube downloads.
- Follow-up 2026-06-03: The operator must see what every job is, its true current state, what failed, what retry attempt replaced it, what can be deleted, and what action is safe next.
- Follow-up 2026-06-03: The current Cookie Editor export file at `D:\Projects\Image_sourcing\lora_avatar_test_0007\cookie.js` is an accepted YouTube auth input format; do not require the operator to reformat it.
- Follow-up 2026-06-03: The operator's current default browser is Firefox; browser-cookie auth paths should default to Firefox while keeping Chrome and Edge supported. Existing Opera support may remain.
- Follow-up 2026-06-04: The old batch must show correct canonical video status: number of unique videos, downloaded videos, unresolved videos, running/queued videos, and historical failed attempts separately. Pressing `Retry failed batch` must not requeue failed attempts for a video target that already has a successful output.

## Root Cause Evidence

- `42a89117` is not failing to retry now. Read-only DB inspection shows 20 fresh `queued` retry rows in batch `42a89117-db6e-4202-970a-fe62a01b2dbe`.
- The live Jobs page and DB `meta` row show `jobs_queue_paused=1`, so those queued retries cannot start until the queue is resumed.
- `2de9cc9c` has no fresh retry rows. Read-only DB inspection shows 281 historical rows: 176 `failed`, 105 `succeeded`, all old `download_direct_url` rows.
- The current Jobs page loads only the latest 80 jobs, so old batches like `2de9cc9c` are not reliably reachable for inspection/retry through the normal queue view.
- The latest `2de9cc9c` failures show `yt-dlp.exe failed ... HTTP Error 403: Forbidden`; this may be resolved by fresh Options cookies after retry, but the UI must first expose the old batch.
- Failed/queued YouTube direct-download jobs usually have `item_id=null`, so the existing item-context fetch cannot show titles. Existing `library_item` rows often contain titles for the same `source_uri`, so title lookup can be cached locally without network calls.
- Follow-up live inspection after the operator reported `42a89117` still failing shows the installed `v0.1.57` app running from `C:\Program Files\VoxVulgi\desktop.exe`, queue unpaused, and active `yt-dlp` children.
- Latest completed `42a89117` failures are YouTube auth rejections: "Sign in to confirm you're not a bot" and "saved YouTube cookies were supplied, but YouTube rejected them."
- The current `42a89117` retry wave has two running jobs and 38 queued rows; the two running rows reached final MP4 output on the NAS path, so the current first two retries got past authentication.
- Final `42a89117` live DB inspection after the retry wave completed shows `87` succeeded, `88` failed, `20` canceled, and no queued/running rows.
- The latest `42a89117` failed rows all still use the current YouTube direct-download settings (`use_browser_cookies=true`, `browser_cookie_source=firefox`) and fail at YouTube authentication with "Sign in to confirm you're not a bot"; this points to YouTube rejecting the current session/cookie material for the remaining URLs, not old per-job stale cookies.
- Follow-up operator evidence shows many visible failed rows still carry the same YouTube cookie/session rejection. The app currently explains the rejection per row but still leaves the operator with many failed rows and does not contain future retry/batch attempts after the same global YouTube auth material is known-bad.
- The running command includes `--write-subs --write-auto-subs --convert-subs srt` without `--sub-langs`, so `subtitle_mode=auto` can keep jobs busy after media output while fetching/converting many YouTube subtitle sidecars.
- Repeated batch retry can enqueue duplicate active rows for the same direct URL, so a batch retry wave can run the same large YouTube video twice in parallel and block later unique URLs.
- 2026-06-03 live read-only DB inspection confirms the visible `Retry failed (57)` count is not canonical batch state:
  - Batch `42a89117-db6e-4202-970a-fe62a01b2dbe` has `213` rows: `100` succeeded, `93` failed, `20` canceled, and `0` queued/running. Canonical retryable rows: `113`.
  - Batch `2de9cc9c-5c19-4801-bdbd-8321d3b0e3b4` has `281` rows: `105` succeeded, `176` failed, and `0` queued/running. Canonical retryable rows: `176`.
- Current frontend `retryGroup` derives retry scope from the rendered `group.jobs` array and calls `jobs_retry` sequentially, so it can only affect loaded/visible rows and can stop the whole group retry on the first row-level error.
- Backend `retry_job` is a single-row primitive. It preserves batch ID and suppresses duplicate active direct-download targets, but there is no canonical backend primitive for retrying every failed/canceled row in a batch or search scope.

## Scope

In scope:
- Add an old-job search/load path in Jobs/Queue so an operator can find a batch or job by ID prefix, URL, title-derived cached context, or error text beyond the latest page.
- Make queue-paused state explicit after retrying jobs or batches, especially when retry enqueues work that will not start until `Resume all` is clicked.
- Add a safe per-job delete action for failed/canceled terminal jobs, including jobs inside a batch.
- Add cached title context for YouTube direct-download Jobs rows by resolving `params_json.url` against existing `library_item.source_uri` / `ingest_provenance.source_url` records.
- Cap default auto-subtitle download languages for YouTube archive jobs to the project's useful localization languages instead of requesting every available auto-subtitle language.
- Prevent retrying a failed/canceled direct-download row when an active queued/running row for the same canonical direct URL already exists, including within old batches.
- Add containment for repeated YouTube auth rejection so one known-bad current cookie/session state does not create many additional failed rows.
- Add an operator cleanup path for failed/canceled rows matching the current Jobs search/filter so bulk historical auth-rejection noise can be removed without deleting media, library rows, subscriptions, or playlists.
- Add a backend whole-batch retry command that resolves the canonical batch by full ID or unique prefix, selects every failed/canceled row from the database, retries each row with per-row error accumulation, and returns a summary instead of aborting the whole operation on the first failure.
- Wire the Jobs/Queue batch retry button to the canonical backend command whenever a group has a batch ID; keep single-row retry for unbatched jobs.
- Make the batch retry notice state the canonical retryable count, queued count, reused-active count, failed-retry count, and first error when any row could not be retried.
- Add persisted retry lineage for new retries so original failed jobs link to retry jobs and retry jobs link back to originals.
- Add best-effort legacy lineage inference from batch ID, job type, canonical direct URL / YouTube video ID, item ID, and timestamps where explicit retry columns are absent.
- Add canonical batch health/detail summaries from the backend: total, succeeded, queued, running, failed, canceled, blocked, unknown, retryable, latest-attempt state, missing title, and no-output counts.
- Add Job Detail / Attempt History data that exposes attempts, errors, lineage, outputs, source metadata, IDs, source path, filename, output path, and bundle or batch membership.
- Add Jobs filters for failed, blocked by YouTube auth, retried, unretried, succeeded on retry, missing title, and no output, backed by canonical row fields and best-effort derived metadata.
- Add fuzzy search across title, URL, YouTube video ID, batch ID, job ID, filename, source path, and output path.
- Add retry dry-run summary before mass retry, including queued, reused active, blocked, skipped, succeeded, failed-to-enqueue, and unresolved counts.
- Add auth preflight gating before mass retry or repair so known-bad YouTube auth blocks retry waves instead of creating repeated identical failures.
- Accept the operator's Cookie Editor `cookie.js` export as-is for global YouTube auth; normalize it internally and redact it in proof artifacts.
- Make Firefox the default browser-cookie source when browser cookies are enabled without an explicit saved source, while keeping Chrome and Edge as supported alternatives and preserving existing Opera support.
- Make batch rows and batch retry operate on canonical video targets first: succeeded/downloaded targets must lead the status display and must be skipped by `Retry failed batch` even when old failed attempt rows remain.
- Add YouTube title backfill for historical failed rows once current auth is valid.
- Add export of failed or unresolved items as CSV, JSON, and plain URL list.
- Add Repair Batch action: dedupe by canonical video/source, skip successes, retry unresolved items, link attempts, and report final unresolved items.
- Make 80+ item batches scrollable and inspectable through a bounded UI viewport whose totals come from backend canonical summaries rather than loaded/rendered rows.
- Add copy actions for URL, video ID, job ID, batch ID, output path, and error.
- Add focused frontend/backend tests where practical.
- Capture headless visual evidence for:
  - searching/loading `2de9cc9c`
  - `42a89117` queued while paused
  - delete button on a single failed job
  - YouTube title context in the Jobs target column
  - canonical batch health for an 80+ item batch
  - Job Detail / Attempt History lineage
  - retry dry-run and auth-gated blocked state

Out of scope:
- Deleting user media, library rows, subscription lists, playlists, or third-party exports.
- Automatically resuming the global queue without operator action.
- Retrying all historical failed batches automatically.
- Network metadata fetching just to display titles in Jobs/Queue.
- Manual mutation of the operator's live media/library DB to mark currently running rows complete.

## Risks, Failure Scenarios, and Mitigations

- Risk: Search by old batch ID returns too many rows and hurts Jobs page responsiveness.
  - Scenario: broad query over `params_json` scans thousands of rows.
  - Mitigation: cap search results, keep exact ID/batch prefix matching cheap, and only run search when the operator enters a query.
  - Verification: search `2de9cc9c` on the live DB through the bridge and confirm the UI remains responsive.
- Risk: Single-job delete removes active work or useful succeeded history unexpectedly.
  - Scenario: operator clicks delete on a queued/running/succeeded row.
  - Mitigation: expose delete only for failed/canceled rows, and backend enforces status before deleting.
  - Verification: backend test rejects queued/running delete and accepts failed/canceled delete.
- Risk: Title lookup shows the wrong title for duplicated or reused URLs.
  - Scenario: multiple library rows share one YouTube URL.
  - Mitigation: choose the newest matching library item title and still show the source URL underneath.
  - Verification: frontend/helper test for title + URL display.
- Risk: Queue-paused retry behavior remains confusing.
  - Scenario: retry succeeds but nothing starts.
  - Mitigation: retry notices mention paused state and target rows show that queued work is waiting for resume.
  - Verification: visual snapshot with queue paused and queued retry rows.
- Risk: YouTube media finishes but job appears stuck because uncapped auto-subtitle fetching runs after the MP4 is written.
  - Scenario: `yt-dlp` finishes media merge on a large NAS output path, then spends unbounded time fetching/converting every available auto-subtitle language.
  - Mitigation: add `--sub-langs` for default auto/manual subtitle modes, scoped to English/Japanese/Korean localization use.
  - Verification: unit test asserts auto subtitle args include a language cap; live command inspection no longer shows uncapped subtitle fetches after rebuild.
- Risk: Retrying an old batch repeatedly creates duplicate active direct-download jobs for the same URL.
  - Scenario: operator clicks Retry failed again while a previous retry wave is queued/running; two workers download the same multi-GB video simultaneously.
  - Mitigation: backend retry refuses duplicate active direct-download retries for the same canonical URL and returns the already-active job row.
  - Verification: backend test seeds a failed row plus an active row for the same URL and verifies retry returns the active row instead of inserting another duplicate.
- Risk: Known-bad YouTube auth keeps creating rows that all fail the same way.
  - Scenario: current global cookies are rejected, then old batch retry or subscription queue creates dozens of identical failed jobs.
  - Mitigation: detect saved-cookie YouTube auth rejection as a reusable auth-blocked condition and block/reuse future YouTube retries until auth changes or preflight passes.
  - Verification: focused backend test for auth-blocked retry containment and live Jobs verification that the operator sees a single actionable state instead of more rows.
- Risk: Bulk cleanup deletes useful job history.
  - Scenario: operator wants to remove repeated auth failures but accidentally deletes succeeded/running queue history.
  - Mitigation: bulk delete only terminal failed/canceled rows matching the explicit search/filter and confirm that media/library/subscription data is untouched.
  - Verification: focused backend test for failed/canceled-only deletion plus visual Jobs snapshot.
- Risk: Persisted retry lineage creates misleading history for legacy rows without explicit columns.
  - Scenario: an old failed row and a later queued row share a source URL but were not produced by Retry.
  - Mitigation: label inferred links as inferred, prefer explicit lineage for new rows, and include source URL/video ID/timestamps in attempt history.
  - Verification: backend tests cover explicit lineage and legacy inference separately.
- Risk: UI filters hide canonical unresolved work.
  - Scenario: an operator filters by visible failed rows and assumes that is the whole batch.
  - Mitigation: batch health uses backend canonical totals and every filtered view states loaded row count separately from canonical batch count.
  - Verification: frontend test checks displayed canonical totals differ from rendered-row totals when a batch summary is present.
- Risk: failed attempt counts are mistaken for failed video counts after a target already downloaded.
  - Scenario: an old batch shows dozens of failed rows even though each unique video has at least one successful output, so the operator keeps retrying and creates fresh auth failures for already-downloaded videos.
  - Mitigation: batch rows must lead with canonical video health (`videos`, `downloaded`, `unresolved`, `queued/running`) and show failed/canceled rows only as historical attempt counts; batch retry must skip canonical targets with any successful attempt.
  - Verification: backend test for retry skipping succeeded canonical targets after later failed attempts, and frontend contract test for target-health wording.
- Risk: title backfill mutates live library or media rows while trying to repair job context.
  - Scenario: historical failed jobs get title context and accidentally overwrite library metadata.
  - Mitigation: backfill writes only job metadata/lineage context or cached job title fields; library/media/subscription tables remain read-only for this action.
  - Verification: backend test asserts library rows are unchanged after title backfill.
- Risk: the Cookie Editor export is rejected because it uses a `.js` extension even though its content is valid JSON.
  - Scenario: the operator supplies `D:\Projects\Image_sourcing\lora_avatar_test_0007\cookie.js` and the app asks for another format.
  - Mitigation: treat `.js` cookie-file paths as valid inputs, parse the JSON array shape internally, and document the accepted path style in Options.
  - Verification: sanitized backend test accepts a `cookie.js` file path and normalizes it to Netscape cookie text.
- Risk: browser-cookie auth silently uses a browser profile different from the operator's real default session.
  - Scenario: the app falls back to Chrome while the operator is logged into YouTube in Firefox, so retries fail or appear inconsistent.
  - Mitigation: make Firefox the default browser-cookie source and keep Chrome/Edge selectable alternatives for operators whose valid YouTube session lives there.
  - Verification: backend tests assert default Firefox plus Chrome/Edge normalization; frontend contract tests assert the browser selector exposes Firefox, Chrome, and Edge.

## Acceptance Criteria

- Searching `2de9cc9c` in Jobs/Queue loads the old batch rows even though they are older than the default latest-80 view.
- `42a89117` clearly communicates whether retried jobs are queued, blocked by a paused queue, reused because an active retry already exists, or still running.
- A failed/canceled single job inside a batch has a delete action, and deleting one row does not delete the full batch or media/library data.
- YouTube direct-download job target context shows a cached video title when a matching library item exists, with the URL still visible.
- Default YouTube archive auto-subtitle fetching is bounded to English/Japanese/Korean language patterns instead of requesting every available subtitle language.
- Retrying a failed/canceled direct-download row while the same target already has queued/running work reuses the active row instead of inserting a duplicate.
- Repeated YouTube saved-cookie rejection is contained so retrying/batching with known-bad auth does not create many more identical failed rows.
- Jobs/Queue provides a bounded cleanup path for failed/canceled rows matching the operator's current search/filter.
- Batch-level `Retry failed` does not use the visible row count as its retry scope; it retries the canonical failed/canceled rows for the full matching batch in the backend.
- Batch-level retry continues after individual row retry failures and reports partial success/failure counts instead of silently stopping after the first error.
- Jobs rows show original video title, URL, video ID, job ID, batch ID, source path, filename, output path, and bundle membership wherever available.
- Failed rows show historical failure labels when a newer retry attempt is the current truth.
- Retry rows show the original failed job/video when explicit lineage is available, and legacy rows show inferred lineage only when confidence is sufficient.
- Job Detail / Attempt History shows all attempts, errors, lineage, outputs, and source metadata for the selected job or batch member.
- Batch rows show backend canonical health summaries independent of rendered/loaded row count.
- Batch rows distinguish canonical video counts from attempt-row counts; failed attempts do not imply failed videos when the target has a successful output.
- `Retry failed batch` retries only unresolved canonical targets and does not requeue already-downloaded targets with historical failed attempts.
- Filters and fuzzy search cover failed, auth-blocked, retried, unretried, succeeded-on-retry, missing-title, no-output, title, URL, video ID, batch ID, job ID, filename, and output path.
- Mass retry and Repair Batch expose a dry-run summary before mutation and are blocked by known-bad YouTube auth.
- Cookie Editor `cookie.js` file paths are accepted as global YouTube auth input without operator-side reformatting.
- Browser-cookie auth defaults to Firefox when no explicit source is saved, while Chrome and Edge remain supported browser-cookie sources.
- Historical YouTube title backfill can run after valid auth and does not modify media/library/subscription data.
- Failed/unresolved items can be exported as CSV, JSON, and plain URL list.
- 80+ item batches remain scrollable and inspectable, and collapsed batch rows still expose title/link/source context.
- Focused tests pass.
- Headless bridge visual verification is captured and recorded in the proof bundle.

## Verification Plan

- Run focused frontend contract tests for Jobs context rendering/search/delete affordance helpers.
- Run focused Rust tests for job search, terminal single-job delete behavior, canonical whole-batch retry behavior, retry dry-run, repair batch, explicit lineage, legacy inference, duplicate direct-download retry suppression, Cookie Editor `cookie.js` auth normalization, and yt-dlp archive option construction.
- Run focused Rust tests for YouTube auth-rejection containment and terminal bulk cleanup.
- Run focused Rust tests for title backfill and export formats without touching library/media/subscription rows.
- Run focused Rust and frontend contract tests for Firefox browser-cookie default behavior and Chrome/Edge support.
- Run focused Rust and frontend contract tests for canonical batch target-health display and retry skipping already-downloaded targets with historical failed attempts.
- Run focused frontend tests for detail/history rendering, canonical batch health, filters, copy/export controls, scrollable large batches, and historical-vs-current retry labels.
- Run `npm run build` from `product/desktop`.
- Use the app bridge to navigate to Jobs, search `2de9cc9c`, inspect `42a89117`, and capture `/agent/snapshot` plus `/agent/dump`.

## Status Updates

- 2026-06-02: Created before product edits. Root cause found from read-only live DB and bridge snapshot: `42a89117` retry enqueued 20 jobs but queue is paused; `2de9cc9c` remains historical failed rows outside the latest page load; title and delete affordances are missing UI/backend features.
- 2026-06-02: Implemented Jobs search over old rows, cached YouTube title hydration, terminal-only per-job delete, paused-queue retry/row cues, and target text wrapping. Focused frontend/Rust tests pass. Built managed desktop target `0.1.57` with WP-0248 changelog entry and captured live bridge snapshots/dumps. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0248/2026-06-02_jobs_old_batch_retry_delete_title_context/summary.md`.
- 2026-06-02: Operator reports `42a89117` is still failing. Reopened WP-0248 to inspect the live running build, bridge state, queue pause state, latest `42a89117` rows, and latest downloader errors before any further product edits.
- 2026-06-02: Live `42a89117` evidence refined: queue is unpaused and current retry execution uses the current configured YouTube auth path. Some retry rows got past authentication and succeeded, but the completed retry wave ended at `87` succeeded, `88` failed, `20` canceled, with the latest failures all showing YouTube "not a bot" cookie/session rejection. The app-side remaining problems were duplicate retry waves for the same direct URL and uncapped YouTube auto-subtitle sidecar fetching. Implemented direct-download active retry reuse, capped default subtitle languages, and updated Jobs retry notices. Focused Rust, frontend, and `cargo check` verification passed before managed build.
- 2026-06-02: Managed desktop build `0.1.58` completed with WP-0248 changelog entry, pack warmup gate `ok`, offline payload reused, NSIS/MSI artifacts generated, and built `desktop.exe` launched as the live app process for bridge verification. Captured `v0.1.58` Jobs bridge snapshot/dump. Proof updated: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0248/2026-06-02_jobs_old_batch_retry_delete_title_context/summary.md`.
- 2026-06-02: Operator reports many remaining visible failed rows with the YouTube saved-cookie rejection. Reopened WP-0248 again to add auth-failure containment and a safe terminal-row cleanup path before any further retry/batch work creates more identical failures.
- 2026-06-02: Implemented persistent YouTube auth-block containment for rejected saved/browser auth material. Direct retries, direct batch enqueue, subscription enqueue, subscription expansion, preflight, and direct download execution now share the same block/clear behavior; saving new Options auth clears the stale block and successful preflight clears the matching block. Added bounded Jobs search cleanup for failed/canceled rows only. Focused Rust tests, frontend contract tests, `npm run build`, and Tauri `cargo check` passed.
- 2026-06-02: Managed desktop build `0.1.59` completed with WP-0248 changelog entry, pack warmup gate `ok`, offline payload reused, NSIS/MSI artifacts generated, and built `desktop.exe` launched as the live app process for bridge verification. Captured `v0.1.59` Jobs bridge snapshots/dump. Proof updated: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0248/2026-06-02_jobs_old_batch_retry_delete_title_context/summary.md`.
- 2026-06-02: Operator asks why the existing Options cookie export handling does not filter out only the correct information. Reopened WP-0248 to add YouTube-specific cookie normalization on global Options save, preserving valid YouTube-domain cookies while dropping unrelated domains and expired persistent records before preflight/download use.
- 2026-06-03: Implemented YouTube-only Options auth normalization. Global auth save now accepts JSON, Netscape text, cookie headers, or cookie-file paths; stores normalized YouTube-domain Netscape cookie text; rejects exports without non-expired YouTube cookies; and no longer frontend-rejects valid Netscape text with JSON-only parsing. Focused cookie/auth tests, `npm run build`, and Tauri `cargo check` passed. Managed build `0.1.60` completed with WP-0248 changelog entry and bridge visual verification captured with cookie text redacted in proof artifacts.
- 2026-06-03: Operator reports Jobs/Queue `Retry failed (57)` still does not restart all failed attempts in the batch. Reopened WP-0248 because local inspection confirms the current UI retries only failed/canceled rows loaded into the current page/search result and aborts the retry loop on the first per-row error; live DB evidence shows batch `42a89117-db6e-4202-970a-fe62a01b2dbe` has `93` failed rows, so a visible-row retry count like `57` is not a whole-batch retry.
- 2026-06-03: Updated canonical evidence before product edits. Live DB read-only counts: `42a89117-db6e-4202-970a-fe62a01b2dbe` has `113` failed/canceled retryable rows and no active rows; `2de9cc9c-5c19-4801-bdbd-8321d3b0e3b4` has `176` failed retryable rows and no active rows. Planned fix is a backend whole-batch retry command with per-row error accumulation, then Jobs/Queue wiring to use that command for batch groups.
- 2026-06-03: Implemented canonical backend batch retry and Jobs/Queue wiring. `Retry failed batch` now calls `jobs_retry_batch_failed`, which resolves the canonical batch, retries every failed/canceled DB row, reuses active duplicate direct-download targets, and reports partial row errors instead of aborting at the first failure. Focused Rust tests, `npm run build`, `cargo check`, managed build `0.1.61`, and bridge Jobs snapshot/dump completed; full contract suite still has 3 pre-existing unrelated failures noted in proof.
- 2026-06-03: Scope expanded from old-batch retry/delete/title recovery into the full trustworthy Jobs/Queue recovery surface. Research basis recorded: yt-dlp FAQ and YouTube extractor wiki for cookie/auth fragility and metadata/title behavior, Tauri async command guidance for off-UI-thread DB work, and rusqlite query/transaction docs for canonical backend summaries. Product edits must add persisted retry lineage, canonical batch detail/health, detail/history, filters/search/export/backfill/repair, dry-run/auth gating, large-batch inspection, and first-class Cookie Editor `cookie.js` auth input without operator-side reformatting.
- 2026-06-03: Implemented the expanded recovery surface and moved WP-0248 to REVIEW. Managed desktop build `0.1.64` completed with WP-0248 changelog entry, final bridge Jobs snapshots/dumps captured, proof bundle written at `product/desktop/build_target/tool_artifacts/wp_runs/WP-0248/2026-06-03_jobs_trustworthy_recovery_surface/summary.md`, and frontend contract tests pass. Live `cookie.js` auth material is structurally accepted but YouTube rejects it on the new default preflight URL, so broad unresolved retry/download remains auth-blocked by design rather than queued into repeated identical failures.
- 2026-06-03: Operator clarified that Firefox is the current default browser and Chrome/Edge must still be supported. Reopened WP-0248 narrowly to align browser-cookie defaults and auth-source selection with that operator environment before any future login/import wizard work.
- 2026-06-03: Implemented Firefox browser-cookie source default while preserving Chrome, Edge, and Opera support. Backend browser-cookie source normalization defaults missing source to Firefox; yt-dlp expansion no longer hardcodes Chrome; Instagram browser-cookie selectors initialize blank legacy state to Firefox and persist Firefox for enabled rows. Verification passed: focused Rust source test, full engine unit tests, frontend contract tests, desktop web build, Tauri `cargo check`, managed desktop build `0.1.65`, and bridge snapshot/dump. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0248/2026-06-03_browser_source_default_firefox/summary.md`.
- 2026-06-04: Operator reports the old batch still does not visibly show correct downloaded-vs-failed status. Live read-only DB inspection of `42a89117-db6e-4202-970a-fe62a01b2dbe` shows the core issue: `55` canonical video URLs all have at least one success, but historical attempt rows still include `96` failed and `23` canceled rows, so the UI and retry controls must make canonical target truth dominant and keep failed attempts in history.
- 2026-06-04: Implemented the canonical-target follow-up. Backend batch retry now groups by canonical target and skips any target with a successful attempt, even when later failed/canceled historical attempts remain. Jobs batch rows now show video target health first (`55 videos: 55 downloaded / 0 queued or running / 0 unresolved`) and attempt history separately (`232 attempts: 113 succeeded / 96 failed / 23 canceled / 99 auth-blocked`), with the batch retry action labeled `Retry unresolved` and disabled/no-op when unresolved videos are `0`. Live DB/file proof confirms all `55` succeeded targets have linked library items and existing media files. Verification passed: frontend contracts, focused Rust regression, full engine unit suite, desktop web build, Tauri `cargo check`, managed build `0.1.66`, and bridge visual proof against the old batch. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0248/2026-06-04_batch_target_health_status/summary.md`.
