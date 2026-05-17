# Work Packet: WP-0226 - Comprehensive read-only UI read sweep

## Status

IN_PROGRESS

## Base Scope

- Apply WP-0224's read-only DB connection pattern to every panel-mount engine read so no panel switch can stall behind the job runner's write queue.
- Instrument the remaining Jobs page Tauri commands so any residual slow path is visible in the next freeze trace.

## Operator Request Preserved

- "can we not do this for all panels? because its the switching that breaks something or freezes the app."

## Research Basis

- v0.1.23 freeze report (`freeze_report_1779046348811.json`) confirmed the WP-0224 fix worked for the 5 rewired commands:
  - `youtube_subscriptions_list`: 7-15 ms (was 3 000-3 500 ms)
  - `youtube_subscription_groups_list`: 8 ms (was 5 542 ms)
  - `library_list`: 4 ms after the first call (was 13 829 ms)
  - 20 `worker_alive` rows present (CORS fix worked)
  - 0 `freeze_detected` rows over the whole session
- Operator reported "freeze when going into jobs/queue" but the trace showed no `command_slow` for any Jobs page command — because none of the Jobs commands (`jobs_list`, `jobs_list_for_item`, `jobs_queue_control_get`, `jobs_runtime_settings_get`) were instrumented and none used `db::open_readonly`. The Jobs page mount inherited the v0.1.22 contention pattern even though Library / Video Archiver no longer did.
- Engine audit (this session) of pure-read functions confirmed they call only SELECTs with no INSERT/UPDATE/DELETE in their bodies or helpers, making them safe to rewire to `db::open_readonly`:
  - `jobs::list_jobs` (jobs.rs:2955), `jobs::get_job` (jobs.rs:3004), `jobs::list_jobs_for_item` (jobs.rs:3068), `jobs::get_queue_control` (jobs.rs:3128), `jobs::get_runtime_settings` (jobs.rs:3136)
  - `library::get_item_by_id` (library.rs:228)
  - `subtitle_tracks::list_tracks` (subtitle_tracks.rs:21), `subtitle_tracks::get_track` (subtitle_tracks.rs:60), `subtitle_tracks::load_document` (subtitle_tracks.rs:100; calls `get_track` so inherits the fix)
  - `video_libraries::get_video_library_by_id` (video_libraries.rs:66)

### Selected approach

- Replace `db::open(paths)? + db::migrate(&conn)?` with `db::open_readonly(paths)?` in each of the audited pure-read functions.
- Add `InvokeTimer` to the five Jobs / Library Tauri commands not previously instrumented: `library_get`, `jobs_list`, `jobs_list_for_item`, `jobs_queue_control_get`, `jobs_runtime_settings_get`.
- Skip writer instrumentation (deferred to v0.1.25 if needed): the read-only sweep removes UI dependency on writers entirely. The writer instrumentation would only diagnose, not fix.
- Skip `video_libraries::list_video_libraries` rewire: it calls `ensure_default_video_library_conn` which writes when no default exists. Splitting it cleanly is a structural change worth its own WP; for now the function stays on `db::open`.

### Rejected options

- Rewiring every `list_*` / `get_*` function across voice_backend_adapters, voice_cast_packs, voice_plans, voice_templates, voice_library at once: lower-frequency panel-mount paths; defer until trace evidence shows they matter.
- Adding a project lint to enforce `db::open_readonly` for `list_*` functions: useful but a separate hygiene WP.

## High-ROI Additions

- The read-only pattern now covers Library, Video Archiver, Instagram Archiver, Localization workspace, Jobs, Subtitle Editor, and any single-item lookup (`*_get_by_id`). Eight panel-mount paths previously susceptible to writer-lock stalls.
- Instrumenting the Jobs commands means the next freeze report will name the slow command instead of leaving the Jobs path blind.
- The pattern is now established across four engine files, so future read paths copy the pattern cheaply.

## Reused Systems

- `db::open_readonly` from WP-0224 (no engine changes needed).
- `InvokeTimer` from WP-0221 (no Rust changes needed).
- Existing freeze-report tooling for verification.

## Gaps Closed

- The Jobs page (and Subtitle Editor + Localization deep-dive) no longer block on the job-runner write queue.
- The next freeze trace can identify any remaining slow Tauri command on Jobs page, not just on Library/Video Archiver.

## Risks And Hardening

- Risk: a rewired function unexpectedly writes through a helper not visible in the function body.
  - Remediation: SQLite returns `attempt to write a readonly database` at runtime — loud failure rather than silent regression. Each rewire is one line; the obvious fix.
- Risk: schema migration races a UI read on a brand-new install before `db::ensure_schema` finishes.
  - Remediation: per WP-0224 reasoning, `ensure_schema` runs in startup phase before UI is interactive. The earliest UI read fires after that.

## Red-Team

- Failure scenario: a future agent adds a new pure-read function and uses `db::open` because they did not see the WP-0224/WP-0226 pattern.
  - Control: the rewired functions all carry an inline `// WP-0226:` comment showing the pattern. Authority surfaces in `CLAUDE.md` / `AGENTS.md` should mention this rule in a follow-up.

## Acceptance Criteria

- A v0.1.24 freeze report shows `command_completed` rows for `jobs_list`, `jobs_list_for_item`, `jobs_queue_control_get`, `jobs_runtime_settings_get`, and `library_get` with elapsed_ms consistently below 500 ms.
- Engine and tauri Rust builds succeed.
- Existing engine tests pass.

## Verification

- `cargo build --release` in `product/engine` and `product/desktop/src-tauri`.
- Desktop build via `governance/scripts/build_desktop_target.ps1`.
- Post-install: operator captures freeze report via `vvfreeze.cmd`; agent confirms Jobs page commands are now fast and the perceived Jobs freeze is gone.

## Status Updates

- 2026-05-17: Created from operator request "can we not do this for all panels?" after v0.1.23 freeze report confirmed Library / Video Archiver fix but exposed Jobs page as the next contention site. Ten engine read functions rewired to `db::open_readonly`. Five Jobs/Library Tauri commands instrumented with `InvokeTimer`. Writer instrumentation explicitly deferred to v0.1.25.
