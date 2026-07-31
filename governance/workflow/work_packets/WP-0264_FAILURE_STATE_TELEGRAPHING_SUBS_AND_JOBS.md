# Work Packet: WP-0264 - Failure-state telegraphing (subscription panel + Jobs/Queue)

## Status

IN_PROGRESS (operator-requested 2026-07-01; for the next build)

## Owner

Claude (Opus 4.8)

## Operator Request Preserved

- 2026-07-01: "when youtube is rate limiting or it needs a sign in/cookie it should show the different states + requirements both in the subscription panel as in jobs/queue. so i know what to do. it is now very unclear and badly telegraphed. patch/address this now for the next future build."

## Intent

When a subscription refresh or a job fails, classify the failure into a clear STATE and a plain REQUIRED ACTION, and surface it in BOTH the subscription panel and Jobs/Queue — so the operator immediately knows what (if anything) to do. Today the app only shows a raw jargon error (or nothing on the subscription), so a rate-limit, an expired cookie, and a genuinely dead channel all look identical.

## Evidence / real taxonomy (from this machine's live DB, 2026-07-01)

Current 45 "need attention" subscriptions map to these causes (latest error per sub):
- 23 `database is locked` (contention; internal, auto-retries) — fixed by WP-0258.
- 16 `Unable to download API page: HTTP Error` (rate-limit / sign-in needed on a channel).
- 3 `model/tool install failed: ...` (tool/install hiccup).
- 2 `This channel does not have a videos tab` (dead/empty channel; e.g. @iroha_plume824, @Mokaron_1008 — never succeeded in 116 days).
- 1 `io error: file is being used by another process` (transient contention).

## Design

Single classifier, two surfaces:

### Classifier (frontend, DRY) - `product/desktop/src/lib/failureStates.ts`
`classifyFailure(errorText: string | null): { kind, label, requirement, tone }` where `tone` in `'info'|'warn'|'error'|'action'`. Rules (first match wins):
ORDER MATTERS - evaluate most-specific first. The HTTP status code is decisive (learned from live data: "Unable to download API page: HTTP Error 404" is a renamed/gone handle, NOT a rate-limit; @mirai42322 videos+shorts are 404, never worked in 116 days):
- `auth_required` <- /auth is blocked|cookies were rejected|Sign in to confirm|HTTP Error 403|403: Forbidden|login required|--cookies/i -> label "Sign-in needed", requirement "Refresh your YouTube sign-in in Options, then Test saved cookies", tone `action`.
- `channel_not_found` <- /HTTP Error 404|404: Not Found|does not have a videos tab|channel does not exist|This channel does not/i -> "Channel/handle not found", "The @handle was renamed or is wrong - update the URL (or use the channel-ID `/channel/UC...` URL, which never changes), or remove it", tone `action`.
- `rate_limited` <- /HTTP Error 429|Too Many Requests|rate.?limit/i -> "YouTube is rate-limiting", "Retries automatically - no action needed", `warn`.
- `members_only` <- /members-only|join this channel|Private video|is private/i -> "Members-only / private", "Needs an account with access, or remove it", `warn`.
- `busy` <- /database is locked|being used by another process|io error/i -> "Busy (temporary)", "Retries automatically", `info`.
- `network` <- /timed out|connection|network|getaddrinfo|Temporary failure/i -> "Network problem", "Check your connection; retries automatically", `warn`.
- `unknown` <- else (incl. a bare "Unable to download API page" with no status code) -> "Error", "See details below", `error`.

Live-data reality check (2026-07-01, current 45): 23 busy (db-lock, fixed), 16 channel_not_found (HTTP 404 renamed handles), 2 channel_not_found (no videos tab), 3 other/tool, 1 busy (file) - i.e. ~18 are stale/renamed handles needing a URL update, NOT rate-limits or sign-in.

### Persistence (engine) - so the subscription panel can classify without a per-poll job join
- Schema v21: add `last_error_message TEXT` to `youtube_subscription`.
- `record_subscription_refresh_failure` stores the (truncated) raw error into `last_error_message`; `record_subscription_refresh_success` CLEARS it (set NULL) so a recovered sub shows no state.
- Return `last_error_message` on `YoutubeSubscriptionRow` + in the list query + the group query.

### Subscription panel (LibraryPage)
- Per-subscription (list row + detail): when `consecutive_failures>0` and `last_error_message`, render a state chip (`label`, tone-colored) + a one-line requirement. `action`-tone chips (sign-in) get a shortcut/link to Options.
- Status strip aggregate: replace the bare "N need attention" with a per-kind breakdown, e.g. "3 sign-in · 16 rate-limited · 2 broken · 24 busy".

### Jobs/Queue (JobsPage)
- Failed job rows show the classified state chip + requirement instead of the raw jargon error; keep the raw error behind the existing "Show technical details" expander.
- 2026-07-16 refinement: the status headline itself must declare the reason as `Failed - <classified reason>`; a bare `failed` label followed by a secondary chip is insufficient for fast scanning. Extend the classifier for observed missing-output, interrupted-transfer, storage, and tool failures so unknown is a genuine fallback.

## Acceptance Criteria

- A failed subscription shows, in both the subscription panel and Jobs, a plain state + required action derived from its error (sign-in vs rate-limit vs dead-channel vs busy are visibly distinct).
- `record_subscription_refresh_success` clears the state so recovered subs are clean.
- No change to what counts as a failure, retry lineage, or batch-health truth; `cargo test` green; FE `tsc` clean.
- No user data deleted.

## Red-Team

- Misclassification (a real dead-channel shown as "rate-limited") sends the operator down the wrong path. Control: order rules most-specific-first; keep the raw error one expander away; treat `unknown` as "see details", never invent a requirement.
- The classifier drifting from real YouTube/yt-dlp wording over time. Control: patterns live in one file (failureStates.ts) with the evidence taxonomy above; extend as new errors appear.
- Telling the operator to re-auth for a transient rate-limit would cause needless cookie churn (and re-auth is the WP-0257 cascade risk). Control: rate_limited/busy tone = warn/info with "retries automatically", NOT an action.

## Notes

- 2026-07-01: authored + implemented on operator request; for the next build after the in-flight 0.1.82. Builds on WP-0255 (subscription manager), WP-0256 (jobs readability), WP-0261 (activity), WP-0257 (auth-block), WP-0258 (db-lock now fixed).
- 2026-07-16 runtime refinement: an exact-URL historical attempt exposed `job stalled: no progress ... (watchdog backstop)` rendering as generic `Failed - Error`. The shared classifier now emits `Failed - Stalled` with a retry-once / inspect-log-and-network-or-destination action.
