---
file_id: WP-0304-REFINEMENT-v1
file_kind: work-packet-refinement
updated_at: 2026-08-22
---

<topic id="operator-request-and-verified-state" status="active" version="v1" wp="WP-0304" updated_at="2026-08-09">

# Operator request

- Add a TikTok single-video downloader.
- Add a TikTok profile/channel subscription downloader with the same understandable recurring behavior as YouTube, while using TikTok-specific settings and limits.

# Verified current state

- VoxVulgi has no TikTok subscription table, dedicated execution track, module settings pane, Archiver flow, or Media Library provider filter.
- The current provider pipeline delegates many URLs to yt-dlp, but generic URL support is not equivalent to a governed TikTok single/subscription product workflow.
- Current yt-dlp source includes TikTok single-video and user/profile extractors and configurable `api_hostname`, `app_info`, and `device_id` extractor inputs.
- TikTok authentication, private content, IP blocks, app/device API behavior, and profile pagination are provider-specific; YouTube auth/pacing state cannot be reused as policy evidence.
- Existing canonical source identity, memberships, job lineage, provider tracks, browser/manual session inputs, shared subscription workspace plan, and Media Library can be extended instead of building a second archive database.

# Authority and dependencies

- Spec anchors: PRODUCT_SPEC 8.2; TECHNICAL_DESIGN 6.6.
- Dependencies: WP-0299 secure downloader/runtime outcome epoch, WP-0300 metadata/title, WP-0301 settings registry, WP-0302 subscription workspace.
- Preserve: canonical identity/membership/dedupe/lifecycle contracts from WP-0268, WP-0281, WP-0283, WP-0284, and WP-0286.

# Scope edges

- In scope: TikTok provider adapter, single and profile URL flows, canonical IDs/metadata, durable subscriptions/cursor/archive, independent track/pacing state, session/capability settings, Jobs/Media Library integration, manual/help/diagnostics, and exact packaged proof.
- Non-goals: browser UI automation, CAPTCHA solving, proxy/IP rotation, scraping without a provider adapter, copying YouTube thresholds, downloading comments/likes/followers as a primary feature, or treating a successful single-video extractor as proof that profile subscriptions work.

</topic>

<topic id="research-basis-and-selected-design" status="active" version="v1" wp="WP-0304" updated_at="2026-08-22">

# Sources checked

- Current VoxVulgi provider/job/config/schema/identity/membership/library/subscription source and relevant WPs.
- Current yt-dlp TikTok extractor source, including single/user extractors and provider arguments: `https://github.com/yt-dlp/yt-dlp/blob/master/yt_dlp/extractor/tiktok.py`.
- yt-dlp current release and option/plugin contracts: `https://github.com/yt-dlp/yt-dlp/releases/tag/2026.07.04` and `https://github.com/yt-dlp/yt-dlp/blob/master/README.md`.
- Current pinned-release source rechecked 2026-08-22: TikTok single and user/profile extractors remain separate; the extractor exposes provider-specific `api_hostname`, `app_info`, and `device_id` arguments and returns stable video/uploader/channel IDs. Current upstream issues continue to distinguish individual-video success from profile/API pagination failures, so separate capability receipts remain mandatory.

# Provider adapter contract

- Classify canonical TikTok single-video and profile/channel URLs, including supported short/share aliases after resolved canonical identity.
- Return canonical TikTok video ID and profile/user ID separately from mutable handle/URL aliases.
- Enumerate a bounded chronological profile page with cursor/archive/checkpoint and exact canonical item IDs.
- Map remote description/title, creator ID/name, published time, thumbnail, source URL, duration, and provider/runtime provenance into WP-0300 metadata.
- Test public/session capability without starting an uncontrolled profile archive.
- Classify success, login/private, IP/rate block, device/app/API capability, unavailable/deleted content, network, storage, and local-tool failures.
- Emit immutable provider/version/effective-policy/command receipts and WP-0298 incident linkage.

# Persistence and track contract

- Add `tiktok_subscription` or an approved provider-neutral subscription state with stable ID, profile ID, source URL/handle snapshots, title, folder/output/library, active/source status, interval, last queued/attempt/success, classified failure, provider epoch, cursor/checkpoint, next retry, hold, and counts.
- Add TikTok source membership using the existing many-to-many identity model.
- Add an independent canonical TikTok execution track/budget so YouTube or Instagram holds/backlogs do not consume it.
- Repeated discovery uses canonical preflight and successful archive/checkpoint; it never downloads a present/active/operator-deleted item automatically.
- TikTok raw adaptive outcomes may use WP-0299 persistence/controller framework, but its thresholds/state are separate from YouTube and start in a conservative evidence-gathering baseline.

# Single-video workflow

- TikTok Archiver or the provider-aware Video Archiver entry accepts one/many TikTok single URLs and returns ordered preflight/enqueue receipts.
- The row shows canonical title/creator/state/progress and preserves source URL/video ID/job/batch detail.
- Output/library selection uses current archiver storage contracts and no terminal/manual dependency steps.

# Profile subscription workflow

- Add profile URL/handle, resolve/test canonical profile identity, configure interval/output/library/media defaults, and create one stable subscription.
- Refresh enumerates bounded new items, records memberships before present/active suppression, queues only eligible missing candidates, and updates cursor/archive after canonical outcomes according to an explicit transaction contract.
- Second refresh with no new items is a successful no-op, not a failure.
- Shared WP-0302 workspace provides Overview, Media, Activity, and Settings with provider-specific capability/status.

# Operator surfaces

- Options → TikTok Archiver: provider/runtime capability, session input/health, default interval, baseline concurrency/pacing, output/library, and advanced API host/app/device settings with safe defaults and reset.
- Diagnostics: provider/runtime version, capability test, current track budget/hold, last classified errors, cursor/checkpoint, and command receipt.
- Media Library: TikTok provider filter/badge, canonical metadata/search, subscription memberships, favorites/lifecycle.

# Existing systems reused

- Secure yt-dlp payload, provider job wrapper, canonical source identity/membership/lineage, ordered preflight, active claim/dedupe, independent tracks, WP-0300 metadata, WP-0301 settings, WP-0302 workspace, Jobs, Diagnostics, Media Library.

# Rejected options

- Add TikTok URLs to generic Other websites only: no recurring identity, settings, lifecycle, or product proof.
- Reuse `youtube_subscription`: provider-specific schema and semantics would corrupt both modules.
- Use handle as permanent identity: handles/URLs may change; provider user ID is canonical when available.
- Advance cursor at enumeration time: a crash/failure could permanently skip discovered but unmaterialized items.
- Expose raw `device_id`/`app_info` as required normal setup: non-technical default must work from bundled provider defaults; advanced values are optional diagnostics/recovery.

</topic>

<topic id="roi-red-team-microtasks-and-proof" status="active" version="v1" wp="WP-0304" updated_at="2026-08-09">

# High-ROI additions

- Provider adapter and independent track: reuse current architecture, isolate future TikTok drift, and prevent cross-provider queue starvation.
- Canonical user/video IDs plus aliases: prevent duplicates and preserve subscriptions if handles change.
- Cursor/checkpoint transaction tied to canonical outcomes: prevents data loss during partial profile refresh.
- Shared outcome/epoch schema: reuse WP-0299 diagnostics without transferring YouTube policy assumptions.
- Immediate shared workspace/metadata/library integration: avoids a temporary isolated TikTok silo that would require later migration.

# Risks, failure scenarios, controls, and verification

- Single works but profile extractor is blocked.
  - Control: separate capability states/probes; subscription creation requires profile-capability proof.
  - Verify: independent exact single and profile tests.
- Handle changes create duplicate subscriptions.
  - Control: canonical profile/user ID uniqueness plus URL/handle aliases.
  - Verify: same profile through old/new/share URLs.
- Cursor advances before downloads succeed.
  - Control: explicit discovered/queued/materialized/checkpoint state and idempotent replay.
  - Verify: crash/failure between each state and restart.
- Private/session failure is treated as rate limiting.
  - Control: provider-specific classifier and hold action.
  - Verify: public/private/expired-session/IP-block fixtures.
- Generated/random device state makes runs irreproducible.
  - Control: command receipt records non-secret effective provider configuration and epoch; advanced stable device configuration is optional/persisted when set.
  - Verify: replay/capture tests without logging secrets.
- Profile refresh produces a duplicate already in a different subscription or single batch.
  - Control: canonical identity claim across all TikTok origins and durable memberships.
  - Verify: single-first/profile-later and overlapping profile/source fixtures.
- Large profile mounts unbounded UI/media.
  - Control: WP-0302 backend paging and render windows.
  - Verify: large fixture with canonical totals beyond loaded rows.

# Microtask plan

1. Add provider URL/identity/classifier fixtures and schema/track migrations.
2. Implement TikTok adapter and structured single/profile metadata output.
3. Implement single preflight/enqueue/import/lineage.
4. Implement durable profile subscription, cursor/checkpoint, dedupe, recurring scheduler/track.
5. Register Options/settings, shared subscription workspace, Jobs, Diagnostics, and Media Library contracts.
6. Add built-in manual/recovery and stable agent IDs.
7. Run exact single/profile/second-refresh/failure/restart tests, packaged offline capability, build, headless UI, and proof.

# Acceptance and proof gates

- An exact TikTok single downloads/imports with canonical ID, correct metadata/title, lineage, Jobs state, and Media Library row.
- An exact public profile subscription enumerates/queues bounded eligible items and a second refresh is duplicate-free.
- Cursor/checkpoint survives interruption without skipping or duplicating canonical items.
- TikTok track/settings/holds are independent from YouTube and Instagram.
- Session/private/IP/device/API/content/network/storage/tool outcomes are distinct and actionable.
- Existing libraries/subscriptions/jobs/media remain unchanged except additive TikTok state produced by explicit tests.
- Rust/frontend/migration/interruption tests, packaged offline tool proof, TypeScript/build, governed version/changelog, headless audits/actions/snapshots/dumps, and proof `summary.md` pass.

</topic>
