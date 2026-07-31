# Work Packet: WP-0266 - YouTube browser-session link and recurring auth guard

## Status

DONE

## Owner

Codex

## Operator Request Preserved

- 2026-07-16: "can we make the failed status also declare the reason, i also want a better way to link my google/youtube account. and find a less agressive download regime for playlists and subscriptions so it does not trigger anti bot"
- 2026-07-16: "currently everything fails but i do not know why, i assume it is because of log in or anti bot"

## Verified Runtime Evidence

- Canonical `%APPDATA%/com.voxvulgi.voxvulgi/db/app.sqlite` sample for the last 24 hours contained 117 failed jobs: 116 classified as YouTube authentication / bot-check rejection and one other failure; no sampled failure was HTTP 429.
- All 117 recent failures were `download_direct_url` attempts. The dominant exact errors say the saved YouTube cookies were supplied but rejected, or "Sign in to confirm you're not a bot".
- The latest canonical rows continued to add queued recurring work while the account state was rejected, amplifying one account-level problem into many doomed attempts.
- No YouTube anti-bot pacing override was stored, so the runtime used the existing defaults: 45 seconds between subscription checks, one second between enumeration requests, and 250 subscriptions per forced update-all tranche.
- The saved global download preset was aggressive for recurring work: four fragments with no pre-download or request sleep.
- Release-candidate runtime proof found a stale-auth precedence defect after Firefox preflight succeeded: older queued jobs still read their per-job cookie secret, re-armed the shared auth block, and prevented the exact reported URL from enqueueing. The corrected precedence is current global manual cookie, current global browser source, then legacy per-job material only when neither global source exists.

## Research Basis

Primary sources checked 2026-07-16:

- yt-dlp README authentication and sleep options: `--cookies-from-browser` is a supported first-class cookie source; `--sleep-requests` paces extraction requests; `--sleep-interval` plus `--max-sleep-interval` paces downloads.
  - https://github.com/yt-dlp/yt-dlp/blob/master/README.md
- yt-dlp FAQ: browser extraction is the easiest cookie path, manual Netscape cookie files remain supported, and exporting browser cookies can expose cookies for all sites if handled carelessly.
  - https://github.com/yt-dlp/yt-dlp/wiki/FAQ
- yt-dlp YouTube extractor guidance: YouTube OAuth no longer works with yt-dlp; cookies are required. It recommends roughly 5-10 seconds between downloads when the request limit is reached and warns that using an account for automated downloads has account-risk.
  - https://github.com/yt-dlp/yt-dlp/wiki/Extractors
- Google installed-app OAuth documentation: OAuth with PKCE is appropriate for YouTube Data API authorization, but it authorizes Google APIs and does not replace yt-dlp media-download cookies.
  - https://developers.google.com/youtube/v3/guides/auth/installed-apps

Selected approach:

- Do not present Google OAuth as download authentication. Add a guided, explicit browser-session connection that saves a supported browser source and tests that exact cookie source without unauthenticated fallback.
- Keep manual YouTube-only cookie import as a fallback for locked/unreadable browser stores and for sessions the operator intentionally exports.
- Apply a recurring-only conservative regime so one-off downloads keep their operator-selected preset: one recurring download at a time, 5-10 seconds before recurring downloads, smaller update-all tranches, and randomized spacing between enumeration dispatches.
- Reuse the existing corroborated auth-block state as a circuit breaker: hold queued YouTube recurring work while the active account state is rejected instead of claiming and failing each row.

Rejected options:

- A Google OAuth "Connect" button for downloads: misleading because yt-dlp no longer supports YouTube OAuth login.
- Exporting and persisting the full browser cookie jar automatically: yt-dlp documents that this can export cookies for all sites; the app should read the selected browser explicitly at execution instead.
- Retrying a failed browser-cookie preflight without cookies: a public video could succeed as a guest and falsely declare the account connected.
- Increasing recurring concurrency: contradicts the operator's anti-bot goal and the existing SQLite-contention evidence.

## Scope

In scope:

- Options guided browser selector and `Connect and test` flow for Firefox, Chrome, Edge, and Opera.
- Persist the explicit global browser-cookie source separately from manual saved cookie material and honor it at execution/retry time for YouTube single, playlist, and subscription work when no saved cookie is configured.
- Exact-cookie-source preflight with clear connected/rejected/unreadable state and timestamp; no guest fallback in this test.
- Recurring-only conservative pacing: randomized enumeration dispatch interval, smaller update-all default tranche, and 5-10 second per-download delay while keeping recurring concurrency at one.
- Auth circuit breaker that leaves queued YouTube recurring work queued while the active auth block is open; Instagram and one-off non-YouTube work remain independent.
- WP-0264 refinement: failed Jobs status leads with the classified reason and required action.

Out of scope:

- Google Data API OAuth and subscription inventory import.
- Account creation, CAPTCHA solving, proxy rotation, identity automation, or bypass tooling.
- Deleting or rewriting subscriptions, playlists, library metadata, job history, or downloaded media.

## Acceptance Criteria

- Options offers a supported browser choice and one explicit `Connect and test` action; a successful result proves the selected browser-cookie invocation succeeded, not a cookie-less retry.
- Manual cookie import remains available and functional as fallback.
- Current global browser source is used by old queued/retried YouTube jobs at execution time when no global saved-cookie material exists.
- A successful replacement browser connection supersedes stale per-job cookies on already queued single, playlist, and subscription work, so old rows cannot immediately re-arm the previous session's circuit.
- `Update all` defaults to at most 25 most-overdue subscriptions per invocation, subscription checks are serialized with randomized spacing, and recurring child downloads wait 5-10 seconds before each download.
- When the corroborated YouTube auth block is active, queued YouTube subscription refreshes and recurring YouTube downloads remain queued and do not generate repeated failed attempts; unrelated lanes/providers continue.
- Every failed Jobs row renders `Failed - <classified reason>` plus a required action; technical detail remains behind disclosure.
- Focused engine tests, Tauri compilation/tests, frontend type/build checks, and headless bridge visual verification pass.
- No user library/subscription/playlist data is deleted or overwritten.

## Red-Team

- Browser database locked or DPAPI decryption rejected: show a source-specific failure and direct the operator to close that browser or use manual YouTube-only cookie import; do not claim connection.
- Browser selected but not signed into YouTube: preflight fails and the global source remains configured but visibly rejected; recurring circuit holds once corroborated.
- Public test video succeeds as guest: prevented because preflight does not strip `--cookies-from-browser` and retry anonymously.
- Auth block expires while cookies remain bad: existing escalating TTL and corroboration bound retries; the circuit opens again only after distinct rejection evidence.
- A YouTube auth block starves Instagram recurring work: recurring candidate selection must skip held YouTube work and continue scanning for unrelated provider work.
- Smaller update-all tranche leaves more subscriptions for later: UI must state the tranche size and most-overdue ordering; repeated scheduled/due runs continue from canonical state without deleting or resetting subscriptions.

## Verification Plan

- Unit-test config normalization, browser-source auth keys, exact-source preflight argument construction, conservative pacing clamps/defaults, circuit filtering, and failed-reason classification helpers.
- Run focused engine and desktop checks on the final source.
- Build the canonical desktop target with a semantic version increment and changelog entry naming WP-0257, WP-0264, and WP-0266.
- Use the localhost bridge to navigate to Options and Jobs, capture paired snapshot/dump artifacts, and inspect both the browser connection flow and failed-reason status without foreground interaction.
- Test the exact reported URL `https://youtu.be/dNUkrrqmwug?si=eiqBo7PBu5gDkzk8` only after a browser/manual session preflight succeeds; otherwise report the auth blocker rather than generating another doomed attempt.

## Result

- Completed in desktop 0.1.99. The exact URL passes Firefox-session preflight, the stale per-job-cookie precedence defect found during runtime proof is fixed, recurring pacing/circuit behavior is active, and Jobs displays reason-led failure states including sign-in and watchdog-stall cases.
- Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0266/2026-07-16_youtube_auth_jobs_pacing_v0_1_99/summary.md`.
