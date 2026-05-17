# Work Packet: WP-0224 - Agent bridge CORS and read-only UI DB connections

## Status

IN_PROGRESS

## Base Scope

- Unblock the WP-0221 freeze-detector Worker so its `fetch()` to the agent-bridge HTTP server actually succeeds and `worker_alive` / `freeze_detected` rows reach `diagnostics_trace.jsonl`.
- Isolate UI list reads from job-runner write contention so page navigations stop stalling for tens of seconds while a writer holds the SQLite write lock.

## Operator Request Preserved

- "when switching to instagram archiver app has a major freeze more then 20 seconds and ongoing. also in diagnostics it says \"database locked\""
- Operator approved a four-part v0.1.23 plan; Parts C (writer instrumentation) and D (phase2 reset) deferred to v0.1.24 to ship the freeze fix faster.

## Research Basis

- Evidence from `freeze_report_1779038188297.json` (v0.1.22):
  - `freeze_detector_install_attempted` and `freeze_detector_install_succeeded` rows present, so the Worker is being constructed.
  - 29 `main_thread_alive` rows but **zero** `worker_alive` rows over the same window.
  - 29 vs. 0 means the Worker is alive but its outbound `fetch()` is silently failing — the Worker code never reaches a state where `worker_alive` would persist.
  - Confirmed by reading `product/desktop/src/lib/freezeDetector.worker.ts`: every `fetch()` to `/agent/freeze_event` is wrapped in `.catch(() => {})` (best-effort, never raises), so a CORS rejection at the browser layer leaves no trace.
- Evidence from the same report on DB contention:
  - `library_list` elapsed 13 829 ms during the operator's "20+ second freeze" on Instagram Archiver.
  - `youtube_subscription_groups_list` 5 542 ms, `instagram_subscriptions_queue_all_active` 4 710 ms, `video_libraries_list` 2 370 ms.
  - Diagnostics UI showed "database locked" — SQLite returns that string when the 10 s `busy_timeout` is exceeded, so some writer held the write lock for longer than 10 s.
- CORS / browser security model: a Worker spawned from the `http://tauri.localhost` origin issuing `fetch()` with `Content-Type: application/json` to `http://127.0.0.1:<port>` triggers a CORS preflight (OPTIONS request). The current `handle_agent_request` route table in `product/desktop/src-tauri/src/lib.rs:126-135` returns 404 for OPTIONS and never sets `Access-Control-Allow-Origin` on POST responses, so the browser blocks the actual request silently. The main thread's `invoke()` path uses Tauri IPC, not browser `fetch()`, so it is not subject to CORS — which is why `main_thread_alive` succeeds while `worker_alive` does not.
- SQLite WAL: WAL mode allows concurrent readers while a writer is active. A `SQLITE_OPEN_READ_ONLY` connection does not contend on the write lock and is unaffected by the writer holding it. WP-0223 already set `synchronous=NORMAL` (good); WP-0224 adds the read-only flag for the list query paths so they bypass the queue entirely instead of merely paying less per writer cycle.

### Selected approach

1. **CORS / OPTIONS**: in `handle_agent_request`, treat any `OPTIONS *` as `204 No Content`. Add `Access-Control-Allow-Origin: *`, `Access-Control-Allow-Methods: GET, POST, OPTIONS`, `Access-Control-Allow-Headers: Content-Type`, `Access-Control-Max-Age: 86400` to every response header set. The bridge listens only on `127.0.0.1`, so `*` is safe.
2. **Read-only UI connections**: add `db::open_readonly` using `SQLITE_OPEN_READ_ONLY | SQLITE_OPEN_FULL_MUTEX`, no `journal_mode` / `synchronous` writes (a RO connection cannot mutate PRAGMA on the file). Rewire the five UI list functions:
   - `library::list_items`
   - `library::list_localization_workspace_items`
   - `instagram_subscriptions::list_instagram_subscriptions`
   - `subscriptions::list_youtube_subscriptions`
   - `subscriptions::list_youtube_subscription_groups`
3. Skip `video_libraries::list_video_libraries` for this slice because it calls `ensure_default_video_library_conn` which requires a writer. That call is harmless under contention (the default already exists after the first call) and rewiring it cleanly is a structural change for v0.1.24.

### Rejected options

- Restricting CORS to `http://tauri.localhost` exactly: more correct but brittle if Tauri rewrites the origin string between versions. `*` is fine on a localhost-only listener.
- Caching list results in the frontend with a short TTL: hides the underlying issue, makes invalidation a new failure surface, and does not help any future read path.
- Connection pool: per-call open is not the dominant cost (10-50 ms vs. 13 829 ms observed). Can revisit if needed.

## High-ROI Additions

- Once CORS works, every future Worker → bridge call works too. The freeze-event ingress, the freeze-dump endpoint, and any future Worker-driven probe inherit the fix.
- Read-only connection pattern is reusable for any future UI list command without further design work — the helper exists; the call site just uses it.

## Reused Systems

- Existing `handle_agent_request` HTTP server in `product/desktop/src-tauri/src/lib.rs:82-138`.
- Existing `db::open` / `db::migrate` / `paths.db_dir()` lifecycle in `product/engine/src/db.rs`.
- Existing `OpenFlags` from `rusqlite`.
- Existing freeze-report tooling from WP-0221 to verify the fix lands by capturing before/after `worker_alive` and `command_slow` rows.

## Gaps Closed

- The Worker is no longer blind to its own success — operators and agents can verify Worker liveness by reading the trace for `worker_alive` rows.
- UI page navigations (Library, Video Archiver, Instagram Archiver) no longer block behind the job runner's write lock. The structural fix removes the dependency entirely rather than reducing the variance.

## Risks And Hardening

- Risk: a UI list command unexpectedly requires write (e.g., a future hydrate that lazily seeds a default row).
  - Remediation: `db::open_readonly` returns a connection that will error explicitly on any write attempt, so the failure is loud, not silent. The rewired call site is the obvious place to fix.
- Risk: schema not yet migrated when a read-only call fires.
  - Remediation: `db::ensure_schema` runs in the startup phase (`product/desktop/src-tauri/src/lib.rs:7093-7095`) before the UI is interactive. The read-only call cannot run earlier than that. The comment on `open_readonly` documents this constraint.
- Risk: opening `Access-Control-Allow-Origin: *` invites any origin to call the bridge.
  - Remediation: the bridge listens only on `127.0.0.1` (`spawn_agent_bridge`, lib.rs:39). External origins cannot connect; the `*` only applies to requests that already reach the loopback listener.

## Red-Team

- Failure scenario: a future agent adds a new UI list function and uses `db::open` instead of `db::open_readonly`, reintroducing the contention.
  - Control: the helper exists alongside `open` and is documented; the WP and the inline comment on each rewired site make the pattern obvious. Long-term, a project-local lint rule could detect `db::open(` inside `list_*` functions.
- Failure scenario: SQLite reports "attempt to write a readonly database" if a rewired function still tries to write somewhere downstream (e.g., `ensure_default_video_library_conn`).
  - Control: only the five list functions known to be pure reads were rewired. `list_video_libraries` was explicitly excluded for this slice for exactly this reason.

## Acceptance Criteria

- A v0.1.23 freeze report shows at least one `worker_alive` row in the trace within the first 60 s after install.
- A v0.1.23 freeze report after a Media Library / Video Archiver page mount under operator load shows `library_list`, `youtube_subscriptions_list`, etc. elapsed_ms consistently below 500 ms.
- `cargo build --release` succeeds in `product/engine` and `product/desktop/src-tauri`.
- Existing engine tests pass without modification.

## Verification

- `cargo test --manifest-path product/engine/Cargo.toml`.
- Desktop build via `governance/scripts/build_desktop_target.ps1`.
- Post-install: operator captures freeze report via `vvfreeze.cmd`; agent reads the report and confirms `worker_alive` presence and list-command timings.

## Status Updates

- 2026-05-17: Created from operator approval after v0.1.22 freeze report disambiguated the Worker dead vs. UI-thread-block hypothesis (Worker is alive, fetch is silently blocked by CORS). Parts A (CORS) and B (read-only UI connections) shipped in v0.1.23. Parts C (writer instrumentation) and D (phase2 interrupted-pack reset) deferred to v0.1.24 to keep the freeze-fix turnaround short.
