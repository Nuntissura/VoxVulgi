# Work Packet: WP-0247 - Archiver session refresh and job context recovery

## Metadata

- ID: WP-0247
- Owner: Codex
- Status: DONE
- Created: 2026-06-01
- Target milestone: Desktop archive/operator usability

## Operator Request Preserved

- "i currently have two batches 46/46 and 29/29 that have failed items and will not progress."
- "i was wondering if old batches that are still ongoing or waiting because of a failure do take new cookies that get inserted options global authentication and session section over or do they keep using stale session cookies? if so they should update with new cookies."
- "session cookies in youtube single, playlist subscription and in options should all be the same (i even prefer it to be only shown in options so no confusion can exist)."
- "in the video archiver youtube single subtab i want at the bottom a list of all single videos downloadeded. latest at the top or reversed + fuzy search."
- "in media library i want a checkmark option for single videos, legacy."
- "bundled jobs when collapsed open do not tell the name or link of the video. this is a major oversight, i have never a clue what failed."
- "eveery job should come with a link or source or a path to the original file or filename."
- "you can use sub agents to read and write, you must also visually debug and use the front end navigation to test. update paperwork before you start."

## Intent

- What:
  1. Make Options global YouTube cookies the canonical source for YouTube archiver execution so old queued/waiting jobs do not keep using stale job-scoped cookie sidecars when fresh global cookies exist.
  2. Remove or demote duplicated YouTube cookie entry points from the Video Archiver YouTube single and playlist/subscription surfaces so the operator has one obvious place to update sessions.
  3. Make Jobs and bundled/collapsed batch rows identify the failed source: URL, subscription source, local path, output path, or item file name.
  4. Add a YouTube single-video downloaded list at the bottom of the YouTube single subtab, newest-first by default with reverse sort and fuzzy search.
  5. Add a Media Library checkbox filter for single-video/legacy-loose items.
- Why:
  - Stuck legacy batches are not diagnosable when rows hide the source URL and can also fail again if stale job-level auth sidecars override new global cookies.
  - Auth UI spread across YouTube single, subscriptions, and Options makes it unclear which cookie will be used.

## Scope

In scope:
- `product/engine/src/jobs.rs`
  - Resolve YouTube auth in `download_direct_url` and `youtube_subscription_refresh_v1` with current Options global cookies first, then fall back to job-scoped or inline legacy cookies only when no global cookies exist.
  - Preserve existing secret redaction behavior: cookie material must not enter `params_json` or UI-visible rows.
  - Add focused Rust tests proving a queued job with a stale per-job cookie uses the newer global cookie when global auth is present.
- `product/desktop/src/pages/LibraryPage.tsx`
  - Stop showing a per-batch YouTube session/cookie textarea and browser-cookie picker in the YouTube single subtab.
  - Stop showing per-subscription YouTube session/cookie controls in the playlist/subscription tab; save flows should not create fresh per-subscription cookie sidecars.
  - Add the downloaded single-video list to the bottom of the YouTube single tab.
  - Add a checkbox filter in Media Library for single-video/legacy loose-file rows.
- `product/desktop/src/pages/JobsPage.tsx`
  - Fix `download_direct_url` context extraction to read the persisted `url` field as well as any legacy `urls` array.
  - Improve collapsed batch summaries so source labels remain visible even when individual jobs are collapsed.
- Focused frontend contract tests for single-video classification/fuzzy search and job context extraction.
- App-boundary verification through the headless agent bridge: navigate to Video Archiver, Jobs, and Media Library; capture snapshots/dumps and inspect for readable source/context surfaces.

Out of scope:
- Deleting existing user libraries, media files, playlists, subscription lists, or third-party exports.
- Editing existing old job rows in the operator database by script.
- Broad queue scheduler redesign or forced auto-retry of the operator's current failed batches.
- Instagram auth unification; this packet targets YouTube session confusion from the request.
- New card-style UI. Changes must fit existing panels/sections and respect `build_rules.md`.

## Research Basis

### Repo sources checked
- `governance/workflow/TASK_BOARD.md`: related prior packets include WP-0162, WP-0170, WP-0193, and WP-0220.
- `governance/workflow/work_packets/WP-0162_GLOBAL_AUTHENTICATION_AND_SESSIONS.md`: established the global YouTube auth intent, but the file status is stale versus the task-board DONE row.
- `governance/workflow/work_packets/WP-0193_JOBS_OPERATOR_CONTEXT_AND_DIRECT_OUTPUT_NAVIGATION.md`: already tracks Jobs context gaps and is still IN_PROGRESS.
- `product/engine/src/jobs.rs`:
  - `DownloadDirectUrlParams` persists one `url` field, not an array.
  - `download_direct_url` currently prefers `read_job_cookie_secret(paths, job_id)` before global auth.
  - `youtube_subscription_refresh_v1` currently prefers job cookie secret before global auth.
  - `retry_job` re-enqueues identical params but does not copy job secret sidecars, so failed-job retries already tend to fall back to global auth; queued old jobs can still be stale if their sidecar remains.
- `product/desktop/src/pages/JobsPage.tsx`:
  - The context builder looks for `params.urls` for `download_direct_url`, which does not match persisted `DownloadDirectUrlParams.url`.
  - Collapsed group summaries depend on those contexts, so direct-video batches can show weak or unknown labels even with useful source URLs in `params_json`.
- `product/desktop/src/pages/LibraryPage.tsx`:
  - YouTube single and subscription UI still expose session/cookie controls.
  - Media Library already derives container metadata including `single_file`, and already has search/sort/source/type controls.

### Selected approach
- Make global YouTube auth execution-time canonical without migrating the DB: each YouTube job resolves current global cookies first, then falls back to legacy job/per-subscription cookies only if Options has no usable cookies.
- Keep old sidecar cleanup on job execution/cancel, but do not run destructive cleanup against user media or subscription data.
- Use small frontend helper functions with contract tests for job context and single-video filtering rather than burying all behavior inside React render logic.

### Rejected options
- Immediate DB migration/backfill of old job auth state. Rejected because execution-time precedence fixes the stale-cookie risk without touching old operator job records.
- Keeping all three cookie entry points synchronized in UI. Rejected because it preserves the confusion; Options should be the only obvious YouTube session surface.
- Adding a separate backend table for single-video history. Rejected for this slice because existing Library rows already contain enough source/path/title/time data to render a useful list. Final implementation adds a dedicated read-only backend query for all YouTube video candidates, then applies the existing single-video/legacy classifier in the frontend.

## High-ROI Additions

- Add source/path labels to Jobs batch summaries while fixing direct-job context.
  - Why high ROI: the same context helper powers collapsed and expanded rows.
  - Gap closed: failed rows no longer require raw log spelunking to identify the video.
  - Reuses: existing `params_json`, item lookup, subscription lookup, and `jobContexts`.
  - Validation: frontend contract test plus Jobs snapshot.
- Add fuzzy search to the single-video list using a local helper instead of a dependency.
  - Why high ROI: avoids package churn and works on title, URL, and path.
  - Gap closed: large single-download history becomes navigable.
  - Reuses: existing loaded `library_list` rows and Media Library item actions.
  - Validation: frontend contract test for typo/subsequence matches.
- Add a Media Library single/legacy checkbox using existing container inference.
  - Why high ROI: no schema change, directly addresses legacy loose-file confusion.
  - Gap closed: single videos stop being buried under subscriptions/playlists/folders.
  - Reuses: `deriveLibraryContainerMeta`, current filters, sort, and virtualization-ish scroll.
  - Validation: frontend contract test plus Media Library snapshot.

## Risks, Failure Scenarios, and Mitigations

- Risk: A valid per-subscription cookie should be used for a subscription when global cookies are absent.
  - Scenario: operator has no global cookie but an older subscription sidecar exists.
  - Mitigation: keep legacy fallback when global auth is empty or invalid.
  - Verification: Rust test covers global-present precedence; existing subscription tests cover sidecar persistence/fallback.
- Risk: Hidden per-subscription cookies continue to exist and confuse future debugging.
  - Scenario: old sidecar remains on disk but does not affect execution while global exists.
  - Mitigation: UI no longer creates new per-subscription sidecars; job execution removes job-scoped sidecars after use.
  - Verification: inspect params/secrets behavior and document residual sidecar fallback.
- Risk: Single-video classification accidentally includes subscription videos.
  - Scenario: YouTube subscription downloads are also `url_direct` rows with YouTube URLs.
  - Mitigation: classify as single only when provider/source is YouTube and derived container kind is `single_file`.
  - Verification: contract tests for single-file vs subscription-container paths.
- Risk: Collapsed batch summaries become noisy for 29-46 item batches.
  - Scenario: rendering every URL in a collapsed row overwhelms the table.
  - Mitigation: summarize first few distinct labels and append a count of additional targets.
  - Verification: contract test for capped summaries and visual snapshot.
- Risk: UI changes add more card chrome against current build rules.
  - Scenario: new history/filter surfaces become additional cards.
  - Mitigation: embed controls/lists inside existing page sections without adding new `.card` wrappers.
  - Verification: inspect DOM/snapshot and CSS diff.

## Acceptance Criteria

- Queued/waiting YouTube jobs with stale job-scoped cookies use fresh Options global cookies when those cookies are configured.
- YouTube single and playlist/subscription UI no longer ask for separate session cookies; Options is the visible YouTube session owner.
- Jobs collapsed group rows and expanded job rows show a source label/detail for direct-video jobs, subscription refreshes, image batches, imports, and item-backed jobs.
- Video Archiver YouTube single subtab shows a bottom downloaded-single-videos list, newest-first by default, reversible, with fuzzy search.
- Media Library has a checkbox filter for single-video/legacy loose-file rows.
- No user library/subscription/media/export data is deleted or overwritten.
- Automated Rust/frontend tests pass for touched behavior.
- Headless frontend navigation and visual debugger evidence is captured for Video Archiver, Jobs, and Media Library.

## Verification Plan

- Run focused Rust tests for YouTube auth resolution and existing cookie normalization.
- Run frontend contract tests for job context and single-video helpers.
- Run `pnpm -C product/desktop test:contracts` or `npm run test:contracts` from `product/desktop`.
- Run `cargo test` for the engine if focused tests are stable; otherwise record focused command and failure details.
- Start or connect to the desktop app bridge; navigate through `/agent/navigate` to:
  - `video_ingest`
  - `jobs`
  - `media_library`
- Capture `/agent/snapshot` and `/agent/dump` for each page and inspect the PNGs for readable non-overlapping UI and source/context visibility.

## Status Updates

- 2026-06-01: Created before product edits per operator request. Initial repo evidence shows stale job cookie sidecars can override global auth for old queued YouTube jobs, and Jobs direct-download context is reading `urls` while persisted jobs use `url`.
- 2026-06-01: Implemented execution-time global YouTube auth precedence, removed duplicate YouTube cookie/session UI from Video Archiver YouTube panes, added shared archiver runtime helpers/tests, fixed Jobs direct URL context summaries, added YouTube single history search/order list, and added Media Library `Single videos / legacy` filtering.
- 2026-06-01: Verification recorded under `product/desktop/build_target/tool_artifacts/wp_runs/WP-0247/2026-06-01_archiver_session_job_context/summary.md`. Focused frontend/Rust tests and `npm run build` pass. App-boundary snapshots/dumps captured through the bridge for Video Archiver, Jobs, and Media Library. WP is in REVIEW pending operator review/release-build decision.
- 2026-06-01: Closed audit gap in the YouTube single history list by replacing the 80-row display cap with a dedicated read-only backend query for all YouTube video candidates. Built managed desktop target `0.1.55` with WP-0247 changelog entry and captured release-boundary snapshots. Installer artifacts exist; installed app update is not run because this non-elevated session cannot safely perform a per-machine silent update without possible UAC/foreground interaction.
- 2026-06-01: Verified there were no active jobs, closed the old installed-app process, and launched the fixed `0.1.55` release executable as the live app process (PID `111220`, bridge port `49378`, safe mode off). Captured live real-data bridge snapshots for Video Archiver history, the old `29/29` failed Jobs batch context, and Media Library single/legacy filtering. Persistent shortcut/start-menu update still requires the `0.1.55` installer to be run with elevation/operator approval.
- 2026-08-15: Promoted to DONE after current-state reconciliation. Focused frontend tests passed 9/9, the global-auth-precedence Rust regression passed, and hidden packaged v0.1.153 inspection confirmed the downloaded-single history, Media Library single-video filter semantic control, and Jobs source/batch context remain present. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0247/20260815_board_reconciliation_v0_1_153/summary.md`.
