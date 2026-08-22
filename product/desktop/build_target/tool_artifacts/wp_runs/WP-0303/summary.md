---
file_id: WP-0303-proof-summary-v1
file_kind: proof-summary
updated_at: 2026-08-22
---

<topic id="outcome" status="done" version="v1" wp="WP-0303" updated_at="2026-08-22">

# WP-0303 proof summary

## Outcome

`DONE`. Instagram now has a first-class single lane, bounded recurring profile lane, pinned provider runtime, durable lifecycle/cursor/hold state, canonical metadata and membership projection, independent settings, and packaged operator surfaces. The exact current `paty.adler` case advanced from yt-dlp profile-extractor failure to a successful bounded Instaloader enumeration and materialized archive.

## Scope delivered

- Pinned Instaloader 4.15.3 provides profile and post/reel structured resolution; resolved assets use the governed direct-HTTP transfer path and story URLs retain the pinned yt-dlp path.
- Instagram single and recurring work use separate persisted scheduler tracks and budgets.
- Subscription rows preserve stable IDs and history while adding canonical profile ID, provider/version/epoch, cursor, attempt/success/error, classified failure, hold/backoff, capability selection, and bounded item-count state.
- Discovery is persisted before suppression; canonical source identity, provider metadata, membership, library lineage, and materialization state remain attributable across refreshes and restart.
- Archive actions are non-destructive: they retain subscription rows, metadata, media, memberships, and job history.
- Options, Diagnostics, Jobs, Media Library, and the Instagram Archiver subscription workspace expose provider/session/capability and recovery state.

</topic>

<topic id="exact-runtime-proof" status="passed" version="v1" wp="WP-0303" updated_at="2026-08-22">

# Exact runtime proof

The ignored network acceptance test `exact_instagram_single_profile_second_refresh_and_restart` was run against a fresh isolated root with `VOXVULGI_IG_PROFILE=paty.adler` and the exact pinned tools. It exercised one single URL, a first bounded profile refresh, a second refresh, runner shutdown/restart, and a post-restart refresh.

Evidence root: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0303/runtime_20260822_v6/app_base`.

## Independent canonical re-read

- Profile: `paty.adler`; canonical profile ID: `304219882`; provider: Instaloader 4.15.3.
- Seven canonical Instagram identities, metadata rows, source memberships, provider-subscription items, materialized library rows, and lineage rows; zero orphan provider-metadata rows.
- One successful `instagram_single` transfer, six successful `instagram_recurring` transfers, and three successful recurring refresh jobs.
- Transfer count remained seven after the second refresh and after restart; no canonical item was queued again.
- The single video is an existing Matroska file with H.264/AAC. Six profile carousel assets are existing JPEG files with distinct `vv_asset` canonical identities.
- Reels returned HTTP 401 and Stories required login for the isolated unauthenticated session. The successful Posts capability was retained while the row persisted `last_failure_class=authentication` and `hold_reason=Instagram authentication must be reconnected in Options`; passive refresh skips the hold and manual recovery remains available.

</topic>

<topic id="packaged-ui-and-build" status="passed" version="v1" wp="WP-0303" updated_at="2026-08-22">

# Packaged UI and build proof

- Governed desktop build: v0.1.178.
- Executable: `product/desktop/build_target/Current/release/desktop.exe`.
- Installer: `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.178_x64-setup.exe`.
- Build log: `product/desktop/build_target/logs/build_desktop_target_20260822-223147_0_1_178.log`.
- The packaged executable ran with `--agent-headless`; `/agent/state` reported `agent_headless=true`, `app_version=0.1.178`, and `current_page=instagram_archive`.
- The semantic UI audit reported zero missing accessible names. The exact profile, canonical ID, saved checkpoint, actionable authentication hold, capability state, and non-destructive `Archive` action were visible.
- Snapshot: `governance/snapshots/WP-0303_packaged_v0_1_178/instagram_auth_hold_archive_1787431142317.png`.
- State dump: `governance/snapshots/WP-0303_packaged_v0_1_178/instagram_auth_hold_archive_1787431142327.dump.json`.
- Original-resolution inspection found readable controls and state, coherent navigation, no text overlap, and no hidden critical state.

</topic>

<topic id="verification" status="passed" version="v1" wp="WP-0303" updated_at="2026-08-22">

# Verification commands and results

- `cargo test --manifest-path product/engine/Cargo.toml --lib -- --test-threads=1` — 581 passed, 0 failed, 4 explicitly ignored.
- `cargo test --manifest-path product/desktop/src-tauri/Cargo.toml --lib -- --test-threads=1` — 46 passed, 0 failed after correcting two stale six-track assertions to the canonical nine-track shape.
- `npm run test:contracts` from `product/desktop` — 251 passed, 0 failed.
- Governed `governance/scripts/build_desktop_target.ps1` build — passed and produced v0.1.178.
- `governance/scripts/build_desktop_target.ps1 -ValidateOfflinePayloadOnly` — passed; 6,190,909,876-byte payload, bundle `offline_full_win64_20260822_201543`, pinned-manifest SHA-256 `06655594710E1805BB1D9354897DF3CE35F913EE709E613BE9A29BEAAEA46DF4`.
- `cargo fmt --manifest-path product/engine/Cargo.toml -- --check` — passed.
- `git diff --check` — no whitespace errors; Git emitted only working-tree line-ending warnings.

</topic>

<topic id="adversarial-review" status="passed" version="v1" wp="WP-0303" updated_at="2026-08-22">

# Adversarial review

## DIFF_ATTACK_SURFACES

Provider selection/install integrity, authentication/session replacement, canonical carousel identity, recurring replay, job-track isolation, direct-transfer finalization, additive schema migration, archive semantics, Tauri command wiring, Options/Diagnostics projection, and packaged UI wording.

## INDEPENDENT_CHECKS_RUN

Fresh exact network runtime, independent read-only SQLite reconciliation, serial engine suite, independent Tauri suite, frontend contract suite, governed build, independent offline-payload validation, headless semantic audit, state dump, and original-resolution screenshot inspection.

## COUNTERFACTUAL_CHECKS

Removing asset-index identity would collapse carousel assets; removing canonical suppression would increase transfer count on refresh two/restart; treating partial capability success as clean would erase the auth hold; treating archive as delete would remove the retained row/history. The executed tests and canonical re-read reject each counterfactual.

## BOUNDARY_PROBES

Single versus recurring tracks, first/second/restarted refresh, public Posts versus authenticated Reels/Stories, provider enumeration versus direct transfer, discovered/queued/materialized state, archive/reactivation boundary, paused queue, app bridge, packaged runtime, and offline payload.

## NEGATIVE_PATH_CHECKS

Paused queue produces no hidden jobs; archived subscriptions cannot enqueue; auth, challenge, rate, extractor, network, storage, content, and local-tool failures classify separately; repeated extractor failures become bounded holds; stale auth writes lose CAS races; provider install recovery rejects forged/ambiguous payloads.

## INDEPENDENT_FINDINGS

Five defects were found and fixed before closure: exited Windows provider processes could look live through retained handles; prepared-owner recovery could move an authenticated generation too early; restored provider lock bytes could acquire CRLF and fail their manifest hash; the packaged UI said `Delete` for non-destructive archive behavior; and two Tauri assertions still expected six scheduler tracks instead of nine.

## RESIDUAL_UNCERTAINTY

The isolated exact run had no accepted Instagram session, so current-profile Reels and Stories were verified through truthful authentication-hold behavior rather than successful authenticated downloads. Posts/profile enumeration, a single video, canonical replay suppression, restart recovery, and the operator remediation path were proven. A future run with an accepted browser session can add positive Reels/Stories evidence without changing the implementation contract. The Tauri formatter still reports pre-existing broad formatting drift in `src-tauri/src/lib.rs`; this is non-functional debt and the 46-test boundary suite passes.

</topic>
