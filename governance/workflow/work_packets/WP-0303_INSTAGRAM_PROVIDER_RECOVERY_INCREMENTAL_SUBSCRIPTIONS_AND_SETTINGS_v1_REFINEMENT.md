---
file_id: WP-0303-REFINEMENT-v1
file_kind: work-packet-refinement
updated_at: 2026-08-09
---

<topic id="operator-request-and-verified-state" status="active" version="v1" wp="WP-0303" updated_at="2026-08-09">

# Operator request

- Restore Instagram Archiver, which has not been working lately.
- Make Instagram subscriptions behave like YouTube subscriptions where behavior is applicable, with Instagram-specific settings and recovery state.

# Verified current state

- The current database contains one active Instagram subscription (`paty.adler`) at a 180-minute interval.
- Current/recent Instagram profile jobs repeatedly fail with `[instagram:user] ... Unable to extract data` and do not produce canonical media/library context.
- Existing `instagram_subscription` fields include ID/title/source/folder/output/cookie/active/refresh interval/last queued, but no durable last-success, classified last-error, provider version, cursor, next-retry, or hold fields.
- Current jobs use Firefox browser cookies and the pinned yt-dlp `2026.03.17` runtime.
- The current taskboard marks WP-0263 Instagram parity `DONE`, while the current exact runtime case fails. This packet is a regression/recovery contract; it preserves useful WP-0263 behavior but does not accept its old completion as current runtime proof.
- yt-dlp `2026.07.04` contains an Instagram extractor rework and invalid-cookie detection, so updating and retesting the exact profile is the minimum first remediation.
- Instaloader 4.15.3 includes profile-resolution and post-metadata fixes and supports incremental archive behavior through `--fast-update`/`--latest-stamps`, resumable/session use, and browser cookie input. It is a candidate, not an automatic replacement before comparative proof.

# Authority and dependencies

- Spec anchors: PRODUCT_SPEC 8.2; TECHNICAL_DESIGN 6.6.
- Preserve/extend: WP-0191 failure investigation, WP-0263 Instagram parity, WP-0269 independent provider tracks.
- Dependencies: WP-0299 secure runtime/capability epoch, WP-0300 metadata/title, WP-0301 settings registry, WP-0302 subscription workspace.

# Scope edges

- In scope: exact runtime recovery, provider adapter, latest yt-dlp test, proof-gated Instaloader fallback/selection, schema lifecycle/cursor, session health, bounded recurring policy, per-provider settings, canonical identity/dedupe/metadata, shared workspace and Media Library integration.
- Non-goals: scraping through browser UI, CAPTCHA/challenge automation, deleting/recreating the existing subscription, copying YouTube-specific tabs/limits blindly, or declaring success from extractor help/version output without an exact media/profile result.

</topic>

<topic id="research-basis-and-selected-design" status="active" version="v1" wp="WP-0303" updated_at="2026-08-09">

# Sources checked

- Current VoxVulgi Instagram source/schema/jobs/logs/live row, WP-0191, WP-0263, trace/runtime settings, and current pinned manifest.
- yt-dlp `2026.07.04` release, including Instagram rework: `https://github.com/yt-dlp/yt-dlp/releases/tag/2026.07.04`.
- yt-dlp Instagram source/current extractor implementation: `https://github.com/yt-dlp/yt-dlp/blob/master/yt_dlp/extractor/instagram.py`.
- Historical exact `instagram:user Unable to extract data` issue class: `https://github.com/yt-dlp/yt-dlp/issues/8851`.
- Instaloader 4.15.3 release: `https://github.com/instaloader/instaloader/releases/tag/v4.15.3`.
- Instaloader incremental/cookie options: `https://instaloader.github.io/cli-options.html`.
- Instaloader basic/session usage: `https://instaloader.github.io/basic-usage.html`.

# Provider selection gate

1. Update/pin yt-dlp through WP-0299 and run the exact failing profile plus exact public single post/reel probes with captured version/args/outcome.
2. If updated yt-dlp passes profile enumeration and incremental repeat behavior, keep it as selected provider and retain Instaloader as evaluated fallback only.
3. If updated yt-dlp profile enumeration fails or is not incrementally reliable, package/pin Instaloader plus dependencies and compare exact profile enumeration, canonical IDs/metadata, second-run dedupe, session behavior, rate/challenge behavior, offline runtime availability, and output mapping.
4. Select provider per operation if evidence supports it: profile/subscription enumeration may use Instaloader while exact post/reel transfer uses yt-dlp. One canonical Instagram media ID prevents duplicates across adapters.
5. Persist selected provider and version/capability epoch; adapter changes do not discard cursors/history or re-download canonical present media.

# Provider adapter contract

- Classify exact post, reel, story, and profile URLs.
- Return canonical Instagram media/profile IDs and canonical source URLs.
- Enumerate bounded chronological items with cursor/checkpoint/archive evidence.
- Map WP-0300 title/uploader/publish/thumbnail metadata.
- Test session/cookie health without starting an uncontrolled profile download.
- Classify authentication, challenge/checkpoint, rate limit, unavailable/private, extractor regression, network, storage, and local-tool failure.
- Produce command/provider/version/effective-policy receipts linked to WP-0298 incidents.

# Schema/lifecycle additions

- Add or normalize last attempt, last success, last failure classification/message hash, provider/adapter/version/epoch, cursor/checkpoint, next eligible retry, hold reason, consecutive failure/success counts, and last canonical discovery count.
- Preserve current subscription ID/source/folder/output/interval/active state and all job/media history.
- A challenge/auth hold requires session remediation; repeated automatic retries may not continue indefinitely.
- Recurring Instagram uses its independent lane, default concurrency one, bounded page/item cap, randomized spacing, and provider-specific cooldown derived from evidence.

# Operator surfaces

- Options → Instagram Archiver: selected provider/status, global session source, exact session test, default interval, concurrency/pacing bounds, media-type defaults, output/library, and advanced provider details.
- Instagram Archiver single flow: URL input, target/output summary, session state, enqueue receipt, current/recent result.
- Shared subscription workspace: Add Instagram profile, last/next status, Media/Activity/Settings, posts/reels/stories capabilities, interval/output, manual refresh, hold/recovery action.
- Diagnostics: provider versions/capabilities, last classified failures, session test receipt, cursor/checkpoint, and exact command receipt.

# Existing systems reused

- Independent Instagram job lane, browser-cookie/session configuration, canonical identities/memberships/lineage, WP-0300 metadata, WP-0301 settings, WP-0302 workspace, Media Library query, diagnostics/headless bridge.

# Rejected options

- Keep retrying current yt-dlp without upgrading: exact repeated failures already disprove it.
- Immediately replace all Instagram work with Instaloader: current yt-dlp has a newer rework that must be tested, and single versus profile capability may differ.
- Use folder/file names as dedupe: loses provider identity and breaks adapter switching.
- Treat cookies present as authenticated: stored/browser cookie availability does not prove the provider accepts the session.
- Reuse YouTube policy thresholds: Instagram has different challenge/rate/session behavior.

</topic>

<topic id="roi-red-team-microtasks-and-proof" status="active" version="v1" wp="WP-0303" updated_at="2026-08-09">

# High-ROI additions

- Provider adapter/operation selection: reuses canonical identity and insulates the product from the next extractor regression.
- Durable cursor/last-success/error state: reuses subscription projection and stops invisible repeated failures.
- Exact session health test: reuses Options/Diagnostics receipts and separates stored credentials from accepted authentication.
- Incremental second-run proof: reuses archive/canonical IDs and prevents subscription redownload storms.
- Provider capability epoch: reuses WP-0299 and keeps historical evidence meaningful after tool updates.

# Risks, failure scenarios, controls, and verification

- Latest yt-dlp fixes one profile but fails another media type.
  - Control: operation-specific capability matrix and exact post/reel/profile fixtures.
  - Verify: each supported type plus unsupported state.
- Instaloader also receives 401/403/429/challenge changes.
  - Control: adapter fallback, session health, classified hold, bounded probe, pinned updates.
  - Verify: recorded fixtures and controlled failure paths.
- Adapter switch duplicates media.
  - Control: canonical Instagram media ID and present/active preflight before enqueue/import.
  - Verify: same item discovered by both adapters and repeated profile refresh.
- Stories expire and cursor semantics differ.
  - Control: declare capability/retention behavior; failure/unavailable is not a profile deletion signal.
  - Verify: expired/empty story result and profile posts continuity.
- Existing subscription state is lost during schema change.
  - Control: additive migration, backup/reopen tests, stable ID/source/path/interval.
  - Verify: migrated current row and fixtures with prior jobs/media.
- Browser cookie read hangs or steals focus.
  - Control: existing quiet Firefox path, bounded command, no browser launch for normal recurring execution.
  - Verify: Firefox available/locked/missing and manual session input paths.
- UI says working from a successful version probe only.
  - Control: readiness requires exact provider operation result and persisted capability receipt.
  - Verify: version-ready/extractor-failed distinction.

# Microtask plan

1. Add exact current failure fixture/receipt and schema migration tests.
2. Update yt-dlp via WP-0299 and run exact profile/post/reel comparison.
3. Implement provider adapter boundary and yt-dlp adapter.
4. If gate requires, package/pin Instaloader adapter and run comparison; record selected per-operation provider.
5. Implement durable lifecycle/cursor/backoff/hold and recurring lane integration.
6. Wire metadata/identity/dedupe/library and shared workspace/settings/diagnostics.
7. Prove exact profile first refresh, second incremental refresh, single post/reel, session failure/recovery, restart resume, build, and UI.

# Acceptance and proof gates

- The exact currently failing profile advances from repeated extractor failure to a successful bounded enumeration/download, or the packet reports a still-external exact blocker and cannot be `DONE`.
- Single post/reel and recurring profile operations have explicit selected-provider capability receipts.
- Second refresh creates no duplicate canonical media and does not redownload present items.
- Existing Instagram subscription/config/jobs/media are preserved through migration.
- Auth/challenge/rate/extractor/content/network/storage/tool outcomes are distinct and produce bounded recovery behavior.
- Instagram settings, subscription workspace, Jobs titles, and Media Library integration pass exact app-boundary proof.
- Packaged offline dependency availability, tests, TypeScript/build, governed version/changelog, headless audit/snapshots/dumps, and proof `summary.md` pass.

</topic>
