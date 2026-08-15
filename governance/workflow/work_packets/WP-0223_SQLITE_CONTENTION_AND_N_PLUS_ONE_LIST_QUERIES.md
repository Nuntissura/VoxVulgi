# Work Packet: WP-0223 - SQLite contention and N+1 list queries

## Status

DONE

## Base Scope

- Eliminate the multi-second stalls observed in UI list commands (`youtube_subscriptions_list`, `library_list`, `youtube_subscription_groups_list`, `video_libraries_list`) under any concurrent write from the job runner.
- Keep the existing single-file SQLite design — do not migrate the engine database to another backend.

## Operator Request Preserved

- "is postgresql better?" -> No; the slowness is caused by SQLite *misuse* in this codebase, not by SQLite itself. Operator approved a tuning-and-rewrite slice over a backend migration.

## Research Basis

- Sources checked: SQLite documentation `https://www.sqlite.org/pragma.html#pragma_synchronous` (sync=NORMAL is the recommended setting with WAL and is still crash-safe to a power loss within the last ~1 s of writes), SQLite WAL documentation `https://www.sqlite.org/wal.html` (checkpoint behavior; readers can stall during checkpoint when synchronous=FULL forces fsync per write), rusqlite docs.
- Evidence from this session's freeze traces (`freeze_report_*.json` produced by `vvfreeze.cmd` on v0.1.20):
  - `youtube_subscription_groups_list` elapsed 5542 ms in a Media Library page mount.
  - `library_list` elapsed 4581 ms in the same mount.
  - `youtube_subscriptions_list` elapsed 3462 ms and 3070 ms across two calls minutes apart.
  - `video_libraries_list` elapsed 2370 ms.
  - Variance from 207 ms to 5542 ms on the same command indicates lock contention, not query cost.
- Investigation (this session) at `product/engine/src/db.rs:40-56` and `product/engine/src/subscriptions.rs:233-267`:
  - WAL is on, busy_timeout is 10 s, `synchronous` is left at the SQLite default (`FULL`), so every job-runner UPDATE (~200-400 ms cadence per `jobs.rs:3901-3960`) does an fsync and triggers a WAL checkpoint that briefly blocks readers.
  - `list_youtube_subscriptions` executes 1 + 2N operations: one main SELECT, one `list_group_ids_for_subscription_conn` SELECT per row (`subscriptions.rs:2397-2405`), and one filesystem stat per row (`subscriptions.rs:2407-2421`). For 50 subscriptions that is 51 DB round-trips serialized; each one re-contests the lock when the job runner is writing.

### Selected approach

1. Set `PRAGMA synchronous=NORMAL` in `db::open` — safe with WAL and removes the per-write fsync cliff.
2. Rewrite `list_youtube_subscriptions` to a single SELECT with a correlated `GROUP_CONCAT` subquery that hydrates `group_ids` in one round-trip. Eliminates the N+1 SELECT pattern.
3. Leave `list_youtube_subscription_groups` alone for this slice (already a single query, no N+1). It will benefit from the PRAGMA change.
4. Leave the per-row filesystem `has_auth_session` stat alone for this slice (~50 ms total even at 50 subscriptions; not the smoking gun). Track as follow-up if it surfaces.

### Rejected options

- Migrating to Postgres: out of scope per operator and an order-of-magnitude larger lift for zero performance benefit on a local-first single-user desktop app. The N+1 pattern would still cost the same.
- Adding an in-process Rust query cache: hides the underlying bug, makes invalidation a new failure surface, does not address the job-runner write contention.
- Adding a connection pool: per-call `Connection::open` adds ~10-50 ms overhead; not the dominant cost. Can be revisited if the JOIN+sync=NORMAL combination still shows variance.

## High-ROI Additions

- `synchronous=NORMAL` benefits every list query, not just the subscription ones — `library_list`, `video_libraries_list`, and any future read path get the same lock-stall reduction with one line.
- The `GROUP_CONCAT` rewrite establishes the pattern that other N+1 hydrate functions in the same file (`hydrate_auth_session_flags`, equivalent Instagram code) can follow without further design.
- An agent reading this WP later sees both the symptom (freeze report rows) and the chain of evidence to the fix, so a future investigator does not redo the diagnostic walk.

## Reused Systems

- Existing `db::open` + `db::migrate` lifecycle in `product/engine/src/db.rs`.
- Existing `row_to_subscription` helper in `subscriptions.rs:2935` (used unchanged; one extra column read inline at the call site).
- Existing freeze-report tooling from WP-0221 to verify the fix lands by capturing before/after `command_slow` rows for the same commands.

## Gaps Closed

- Subscription list queries no longer scale 1+2N round-trips with subscription count.
- Job-runner UPDATEs no longer block UI reads behind a forced fsync.
- The "feels like a freeze" UX on page navigation (Media Library, Video Archiver) — which trace evidence confirms is just slow list queries, not a literal main-thread block — should drop from 3-5 s to sub-100 ms.

## Risks And Hardening

- Risk: `synchronous=NORMAL` weakens crash durability versus `FULL` — a power loss within the last ~1 s of writes can drop those transactions.
  - Remediation: documented in the SQLite reference linked above; the database is local user data, the loss window is small, and WAL guarantees the file is never corrupted. Acceptable trade.
- Risk: `GROUP_CONCAT` with newline separator could collide with a real id character.
  - Remediation: subscription and group IDs are UUIDs (hex + dashes only) — newline cannot appear inside an id, ruling out the collision. Asserted via inline comment in the rewritten function.
- Risk: the rewrite changes the order of `group_ids` in the returned row.
  - Remediation: the previous query sorted by `group_id ASC` server-side; the new code sorts the parsed list client-side with `Vec::sort` to preserve the same total order.

## Red-Team

- Failure scenario: a row has a corrupt `group_id` value that contains a newline because of an out-of-band insert.
  - Control: `Vec::sort` is order-only and total; even if a single id had a newline mid-string, the resulting Vec entry would be malformed but the function would still return without raising. The caller treats `group_ids` as opaque strings.
- Failure scenario: SQLite version too old to support `GROUP_CONCAT` with the current syntax.
  - Control: `rusqlite` 0.32 (per `Cargo.lock`) bundles a SQLite well above the introduction of `GROUP_CONCAT(expr, sep)`. No version gating needed.
- Failure scenario: another caller relies on the old N+1 hydrate path through `hydrate_group_ids` for behavior beyond `list_youtube_subscriptions`.
  - Control: `hydrate_group_ids` and `list_group_ids_for_subscription_conn` remain available for any single-row callers; only `list_youtube_subscriptions` is rewired. Verified by leaving the helpers in place.

## Acceptance Criteria

- `db::open` sets `PRAGMA synchronous=NORMAL`.
- `list_youtube_subscriptions` issues a single SELECT against the database (no per-row hydrate query), verifiable by reading the function body.
- `cargo build --release` succeeds in `product/engine` and `product/desktop/src-tauri`.
- Existing subscription tests in `product/engine/src/subscriptions.rs` pass without modification.
- After install of the build, a fresh `vvfreeze.cmd` capture under the same operator load shows `youtube_subscriptions_list` elapsed_ms consistently below 500 ms (target: below 100 ms when uncontended).

## Verification

- `cargo test --manifest-path product/engine/Cargo.toml`.
- Desktop build via `governance/scripts/build_desktop_target.ps1`.
- Post-install: capture freeze report via `vvfreeze.cmd` on the installed v0.1.21 build; compare the new `command_slow` (or `command_completed`) rows against the v0.1.20 traces. Land the comparison in the WP status updates.

## Status Updates

- 2026-05-17: Created from operator approval after the v0.1.20 freeze report confirmed the slowness pattern (zero `freeze_detected`/`worker_alive`, dozens of multi-second `command_slow` rows pointing at subscription/library list queries). Ships in v0.1.21 alongside WP-0221's Worker install fix.
- 2026-08-15: `DONE` after current-state reconciliation against governed v0.1.155/v0.1.156 proof. Source inspection confirms `db::open` still applies WAL plus `synchronous=NORMAL`, while `list_youtube_subscriptions` uses one read-only prepared SELECT with a correlated `GROUP_CONCAT` and no per-row group query. The full engine suite passed (538 passed, 4 ignored, 0 failed). Fresh packaged v0.1.155 trace rows recorded `youtube_subscriptions_list` at 16/23/13 ms, all below 500 ms and below the 100 ms target, while `library_list` also remained below the packet threshold after WP-0224's follow-up remediation. Governed v0.1.156 then passed frontend/Tauri checks and packaged successfully without changing the engine path. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0223/20260815_0340_current_v0_1_156/summary.md`.
