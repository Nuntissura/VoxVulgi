# Work Packet: WP-0257 - Subscription-refresh anti-bot / auth-block cascade hardening

## Status

IN_PROGRESS

## Owner

Claude (Opus 4.8)

## Operator Request Preserved

- 2026-06-17: "i triggered update all subscriptions but it failed in the jobs.queue. can you take a look what is going wrong. this is important this works. use /workflows to analise and harden this feature for possible other failure scenarios."

## Root Cause (verified from the live job DB on build 0.1.76)

"Update all" enqueued 255 `youtube_subscription_refresh_v1` jobs at once into the recurring lane. A burst of 255 channel enumerations (zero anti-bot pacing) tripped YouTube's anti-bot; ONE channel's saved-cookie rejection then cascaded into a fleet-wide outage:
- **Global block scope:** `youtube_auth_material_key` (jobs.rs ~3083) returns the GLOBAL cookie key first whenever a global cookie is configured, so all 255 subscriptions compute the same `auth_key`. One rejection wrote one `youtube_auth_block.json` row under that shared key; `ensure_youtube_auth_not_blocked` then fail-fast-failed all 255 remaining refreshes (276 FAILED, 234 succeeded).
- **Sticky, no TTL:** `active_youtube_auth_block` never checked `blocked_at_ms`; the block persisted until a manual Options re-test — a transient anti-bot blip became a permanent self-inflicted outage.
- **Recovery broken:** "Retry unresolved" re-trips the block; per-subscription failure backoff recorded during the cascade is not bypassed by force, so even a correct re-auth + "Update all" did nothing.
- **Misleading errors:** every cause surfaces as `EngineError::InstallFailed` ("model/tool install failed:").
- **Collateral:** `record_youtube_auth_block` also cancelled every queued YouTube `download_direct_url` sharing the key (the 6891 'canceled' rows).

## Research Basis

A 6-investigator + 1-synthesis read-only Workflow (`subscription-refresh-hardening`, run `wf_4a8044e5-e4d`, 7 agents / 713k subagent tokens / 166 tool-uses) mapped each failure dimension to exact functions/line numbers and produced the ranked plan below. Grounded entirely in `product/engine/src/jobs.rs` + `subscriptions.rs` reads plus the live failed-job evidence.

## Scope (ranked fixes; status marked)

- **#1 Corroboration + content-guard (DONE):** a single channel rejection no longer arms a global block — `register_youtube_auth_suspicion` requires >=3 DISTINCT subscriptions/URLs to reject under the same cookie key within a 15-min window before `record_youtube_auth_block` is called. A single rejection only fails its own job. Content/structural errors already do not match `is_youtube_saved_cookie_rejection`.
- **#2 TTL + backoff auto-heal (DONE):** `YoutubeAuthBlockState` gains `expires_at_ms` + `backoff_count` (serde-default for back-compat). `active_youtube_auth_block` auto-clears once `now >= expires_at_ms`; pre-WP-0257 sticky blocks (expires_at_ms==0) clear on the first check after upgrade. `record_youtube_auth_block` escalates the TTL 5m/15m/1h/6h. A refresh success clears accumulated suspicion.
- **#5 Recovery (DONE - engine half):** the refresh failure arm no longer records per-subscription backoff when the failure was the shared auth block (`is_youtube_auth_blocked_error`), so clearing the block instantly re-enables all subs. (Remaining: guard "Retry unresolved"/`retry_failed_jobs_for_batch` against an active block like `repair_batch` does — FOLLOW-UP.)
- **#3 Enumeration pacing (DONE - core):** runner now enforces a configurable **recurring-lane inter-dispatch cooldown** (default 45s) so subscription refreshes dispatch one every cooldown instead of bursting (single/localization lanes unaffected); `expand_yt_dlp_entries` adds a configurable **`--sleep-requests`** (default 1) on the enumeration. Operator-tunable via the new Options -> "Anti-bot pacing" card (`antibot_pacing_get`/`set` + clamped meta keys `antibot_*`). Remaining: per-subscription auth-key scope (jitter + enumeration rate-limit retry are refinements).
- **#4 Trickle (DONE - cap):** "Update all" (force path) enqueues at most a configurable `update_all_batch_size` (default 250) **most-overdue first** (`ORDER BY COALESCE(last_queued_at_ms,0) ASC`); the due-path (startup auto-sync) stays uncapped since the cooldown paces it. Remaining: the circuit-breaker (block -> recurring-lane PAUSE that holds rather than cancels queued public downloads).
- **#6 Failure labels (DONE - display):** the Jobs list now shows plain-language headlines instead of raw `model/tool install failed: ...` text. `classifyJobError` (JobsPage.tsx, display-only) maps the error string to a category + tone: auth ("YouTube blocked - re-authenticate"), content ("Members-only - skipped" / "no longer exists - skipped"), network ("rate-limited - ease pacing" / "Temporary network error - retry" / "Storage/NAS error"), tool (FFmpeg), interrupted. Raw error kept on hover + Copy button. Remaining (engine-side, deferred): a `FailureCategory` enum + `error_category` column so retry/skip *behavior* (not just the label) keys off the category, and transient auto-retry (HTTP 5xx / incomplete reads).
- **2026-07-16 reliability refinement (IN_PROGRESS):** live DB evidence shows 116/117 recent failures are saved-cookie/bot-check rejection while queued recurring work continues to accumulate. Implement the previously deferred block-to-lane-pause circuit breaker so active auth rejection holds queued YouTube recurring work, reduce the default update-all tranche 250 -> 25, randomize the existing enumeration cooldown, and enforce the yt-dlp-recommended 5-10 second sleep range for recurring child downloads. This refinement is paired with WP-0266 browser-session linking and WP-0264 status-headline refinement.

Out of scope: anything that deletes user library/subscriptions/playlists.

## Acceptance Criteria

- A single channel cookie-rejection does NOT write `youtube_auth_block.json`; 3 distinct rejections do (unit-tested).
- A pre-WP-0257 sticky block auto-clears on first check; a recorded block auto-clears after its TTL; TTL escalates on repeat (unit-tested).
- An auth-block refresh failure does NOT set per-sub `next_allowed_refresh_at_ms`.
- `cargo test -p voxvulgi_engine` green.
- Operator runtime: re-auth + "Update all" recovers; one bad channel no longer takes down the fleet (verified on the next build).

## Red-Team

- Genuinely dead cookie with <3 active subs: corroboration never trips -> falls back to existing per-sub consecutive-failure backoff (graceful degrade).
- TTL auto-clear lets a truly-dead cookie retry and re-trip: bounded by the escalating backoff cap + corroboration (one retry can't re-arm globally).
- Suspect counter staleness: cleared on a refresh success; window-expiry resets it; keyed by auth_key.
- Pre-WP-0257 block on disk: serde-default keeps it loadable; expires_at_ms==0 => treated expired => clears (resolves the operator's currently-stuck block on upgrade).
- yt-dlp wording drift (#6 follow-up) can break substring detection — pin a golden test to the live-evidence strings.

## Notes

- 2026-06-17: Workflow analysis complete; #1/#2/#5-engine implemented in jobs.rs with 3 new unit tests (single-rejection-no-block, old-block-auto-clear, backoff-escalates). #3/#4/#6 + retry-guard are the next tranche. Build pending after tests green.
