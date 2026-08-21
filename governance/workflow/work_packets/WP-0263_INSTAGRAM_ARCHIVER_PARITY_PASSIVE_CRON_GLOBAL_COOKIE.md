# Work Packet: WP-0263 - Instagram Archiver parity (single + passive subscription cron + global cookie)

## Status

REOPENED (2026-08-21; further investigation, remediation, and proof-gap closure deferred to a new operator session)

## Owner

Claude (Opus 4.8)

## Operator Request Preserved

- 2026-07-01: "i also want to address my instagram archiver, it should work like the video archiver: i can download a single channel, i can subscribe to a channel (cron job every few hours) this must be very passive because of instagram/meta strict anti bot policy. instagram cookie pasted in options and used for everything instagram related. add it to the list for next release."

## Intent

Bring the Instagram Archiver to feature parity with the Video Archiver: a clear **single-profile download**, **saved subscriptions** that auto-refresh on a **passive cron** (every few hours, extra-conservative for Meta's strict anti-bot), and **one Instagram login pasted in Options** that is used for every Instagram operation. Reuse the existing Instagram + subscription-lane + global-auth infrastructure; do not build a parallel system.

## Current State (verified by read-only grep)

- `instagram_subscription` DB table (db.rs:424) + `instagram_subscriptions.rs` module: `list/upsert/delete`, `queue_instagram_subscription` (one), `queue_all_active_instagram_subscriptions`, `is_subscription_due` (interval gating exists), per-subscription cookie secret via `paths.instagram_subscription_cookie_secret_path(id)`, `last_queued_at_ms`.
- Instagram downloads go through generic `download_direct_url` jobs after `expand_instagram_profile_media_targets` / post expansion (jobs.rs:13315, 15259). There is **no dedicated instagram_subscription_refresh job type** and (per WP-0254 analysis) Instagram jobs are **not** stamped into the conservative `recurring` lane the way YouTube subscription refreshes are.
- Instagram UI lives in `LibraryPage.tsx` (mode=instagram_archive) behind Advanced mode; a recurring Instagram panel exists but is a second-class citizen vs. the YouTube subscription surface (WP-0255).
- **Cookie is per-subscription / per-batch today**, NOT a single global Instagram login in Options (contrast: YouTube has a global auth in Options via WP-0162).

## Scope (parity, three pillars)

### 2a - Single Instagram download (like "YouTube single")
- A clear single-profile/single-post download surface in the Instagram Archiver (mirror the Video Archiver's "single" tab UX), using the existing profile/post expansion. Uses the global Instagram cookie (2c). One-off, runs in the `single` lane.

### 2b - Passive subscription cron (Meta-strict)
- Stamp Instagram subscription refresh/downloads into the **recurring lane** (WP-0254) so they never starve single downloads and run one-at-a-time.
- Add Instagram subscriptions to the **startup auto-check + due-based auto-download** path (WP-0254 2d), gated by safe-mode + config, respecting each subscription's interval (default measured in **hours**, per WP-0255).
- **Extra-conservative anti-bot pacing** beyond YouTube (Meta is stricter): a separate, larger recurring inter-dispatch cooldown for Instagram, jitter, low per-run item caps, and honor Instagram's existing failure-backoff. Reuse the WP-0257 pacing framework with Instagram-specific (more passive) defaults + an Options control. NEVER a burst; one profile at a time, long gaps.
- Interval expressed/edited in hours (parity with WP-0255).

### 2c - Global Instagram cookie in Options (used everywhere)
- Add a global Instagram auth in Options mirroring the YouTube global auth (WP-0162): paste the Instagram cookie/session once; store it as the app-global Instagram auth material with the same secret-handling.
- Every Instagram operation (single, subscription refresh, batch) resolves auth in precedence: explicit per-job/per-subscription cookie (if set) -> **global Instagram cookie from Options** -> browser-cookie fallback. Retire the need to paste a cookie per Instagram subscription (keep per-sub override optional).
- A "Test saved Instagram cookies" preflight in Options (mirror the YouTube preflight), and an Instagram auth-block/backoff guard modeled on WP-0257 (corroboration + TTL) so one rejected profile doesn't cascade the whole Instagram fleet.

### 2d - UI parity (Instagram subscription manager)
- Give the Instagram Archiver the same master-detail subscription manager + status strip + honest progress + plain-language copy the Video Archiver got (WP-0255/0260), reusing those components/patterns rather than the current second-class panel.

Out of scope: Instagram Stories/Reels-specific pipelines beyond profile/post media already supported; any change to YouTube behavior.

## Research Basis

- Grep of `product/engine/src/{instagram_subscriptions.rs, jobs.rs, db.rs}` + `paths.rs` confirmed the existing Instagram subscription table/module, per-sub cookie path, due-gating, and profile/post expansion; and confirmed the absence of a recurring-lane/auto-sync/global-cookie integration.
- Build on: WP-0162 (global YouTube auth in Options), WP-0254 (lanes + startup auto-sync + resume), WP-0257 (anti-bot pacing + auth-block corroboration/TTL), WP-0255/0260 (subscription manager UI + plain copy).
- Before implementation: research current Instagram/instaloader/yt-dlp anti-bot field practice for profile enumeration cadence (Meta rate-limits and challenge flows are stricter than YouTube) to set the passive defaults.

## Acceptance Criteria

- Single Instagram profile/post download works from a clear single surface using the global Options cookie.
- Instagram subscriptions auto-refresh on a passive cron (recurring lane, staggered, hours interval), one profile at a time with long gaps; startup auto-check enqueues only due subscriptions; operator-disablable.
- A single Instagram cookie in Options is used for all Instagram operations (with optional per-sub override); a Test button validates it; one rejection does not cascade the fleet.
- Instagram Archiver UI matches the Video Archiver manager (master-detail + status + progress + plain copy).
- No user Instagram subscription/library data deleted; `cargo test` green.

## Red-Team

- **Meta anti-bot is stricter than YouTube** — an aggressive cron gets the account challenged/blocked. Control: Instagram-specific passive pacing (long cooldown + jitter + low caps + backoff), one-at-a-time, opt-out, and a corroboration+TTL auth-block guard (WP-0257) tuned conservative.
- Global cookie rejection cascading all Instagram subs (the WP-0257 problem, Instagram flavor). Control: corroboration threshold + TTL auto-heal + don't pin per-sub backoff on a shared-auth failure.
- Session cookie expiry / challenge flows silently failing. Control: surface a clear "re-authenticate Instagram in Options" state, not a silent stall.
- Reusing YouTube pacing defaults would be too aggressive for Meta. Control: separate Instagram pacing keys with more passive defaults.

## Notes

- 2026-07-01: authored on operator request "add it to the list for next release"; BACKLOG. Distinct from the in-flight 0.1.82 overhaul (WP-0258/0259/0260/0261/0262). Scheduling to be decided by the operator after the 0.1.82 items land.

## Reopened 2026-08-21

- Further investigation and remediation are required before this packet can return to `DONE`.
- Close the gaps between the implemented Instagram surfaces and the packet's exact real-provider acceptance criteria, including reliable single/profile execution, recurring behavior, authentication recovery, and proof against the operator's real workflow.
- Reconcile this packet, WP-0303, the taskboard, and their proof bundles from current product/runtime evidence; unit tests, contract tests, and build success do not replace exact Instagram runtime proof.
- Continue this work in a new operator session. No product-code remediation was authorized or performed during this reopening.
