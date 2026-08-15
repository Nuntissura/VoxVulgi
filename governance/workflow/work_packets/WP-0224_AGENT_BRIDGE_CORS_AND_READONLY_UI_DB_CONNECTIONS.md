# Work Packet: WP-0224 - Agent bridge CORS and read-only UI DB connections

## Status

DONE

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
- 2026-08-15: Current v0.1.153 acceptance audit kept this packet open. Fresh hidden-app proof recorded two `worker_alive` and two `main_thread_alive` rows after successful Worker installation; YouTube/video-library reads completed in 17-69 ms and page-navigation requests in 3-43 ms. However, Instagram's legacy `library_list` cold call took 5,393 ms and three warm repeats took 668/670/697 ms, violating the packet's <500 ms list gate.
- 2026-08-15: Read-only canonical DB diagnosis isolated SQLite query work from connection setup. Against the 1.06 GB operator DB (144,082 library rows), the base 160-row query completed in 1.1 ms and the provider-title join in 2.4 ms using existing indexes. `db::open_readonly` nevertheless calls `AppPaths::ensure_dirs`, which performs sixteen `create_dir_all` operations on every read-only connection; `library_list` opens another read-only connection during title hydration, multiplying unrelated filesystem work under host I/O pressure.
- 2026-08-15: Current primary-source basis: SQLite's `sqlite3_open_v2` documentation states that `SQLITE_OPEN_READONLY` returns an error when the database does not already exist. Selected remediation is therefore to remove directory creation from `open_readonly`: startup/schema initialization remains the sole directory/database creator, while a premature read fails explicitly. Add a regression proving an uninitialized read-only open neither creates the app root nor a database, then re-run current hidden-app list timing. Source: `https://sqlite.org/c3ref/open.html`.
- 2026-08-15: Correction after v0.1.154 packaged proof: removing read-only directory creation passed its regression but did not remove the list delay (672-1,089 ms in the packaged app). A temporary Rust read-only phase probe reproduced 881-911 ms outside Tauri while measuring connection open at 0 ms, the indexed 160-row base query at 9-11 ms, and title hydration at 7-8 ms. The prior claim that repeated directory creation explained the measured latency is retracted.
- 2026-08-15: Exact production-query inspection found the causal boundary. `list_items_by_file_status` encodes all three lifecycle states in parameterized `OR` branches and uses a parameter-dependent `CASE` sort. On the 144,082-row live library, `EXPLAIN QUERY PLAN` reports `SCAN library_item` plus `USE TEMP B-TREE FOR ORDER BY`; the same query takes 269-286 ms even through direct Python SQLite, before Rust row decoding. Selected remediation: retain the existing allowlist, but generate one fixed predicate/order shape for each normalized status so the common `available` path can use `idx_library_item_file_status_created`. Add a query-plan regression and re-run the exact packaged timing gate.
- 2026-08-15: `DONE` in governed desktop v0.1.155. The exact engine path against the live 144,082-row database completed in 12-13 ms after the fixed-shape query change. The full engine suite passed (538 passed, 4 ignored, 0 failed), including the new read-only-open and query-plan regressions. Fresh packaged headless proof recorded `library_list` at 31/110/19/30 ms, `youtube_subscriptions_list` at 16/23/13 ms, two `worker_alive` rows about 30 seconds apart, and app version `0.1.155`; all list timings are below the 500 ms gate. Visual inspection of the packaged Instagram Archiver snapshot found the mounted surface readable with no overlap, blank shell, or frozen state. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0224/20260815_0310_v0_1_155/summary.md`.
