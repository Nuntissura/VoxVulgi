---
file_id: WP-0304-proof-summary-v1
file_kind: proof-summary
updated_at: 2026-08-22
---

<topic id="outcome" status="done" version="v1" wp="WP-0304" updated_at="2026-08-22">

# WP-0304 proof summary

## Outcome

`DONE`. TikTok now has first-class single-video and recurring profile workflows, provider-specific durable subscription state, canonical identity/metadata/membership/lineage, independent scheduler tracks and settings, bounded replay-safe discovery, and packaged operator surfaces.

## Scope delivered

- Single-video URLs and profile subscriptions have distinct commands, job types, tracks, defaults, budgets, pacing, and browser-session behavior.
- Stable TikTok video IDs dedupe across mutable handles and single/profile origins; canonical profile IDs are persisted separately from source URL snapshots.
- Discovery is recorded before queue suppression and moves through discovered, queued, and materialized state without losing membership evidence.
- TikTok provider settings include single/recurring transfer policy, browser source, API hostname, app info, device ID, output root, and independent job budgets.
- Failure classification distinguishes session/private, IP/rate, API/device/app, content, network, storage, tool, extractor, and unknown outcomes with bounded holds/backoff.
- Archive actions preserve subscription rows, media, metadata, memberships, and job history.
- TikTok Archiver, Options, Diagnostics, Jobs, and Media Library expose the new provider state and actions.

</topic>

<topic id="exact-runtime-proof" status="passed" version="v1" wp="WP-0304" updated_at="2026-08-22">

# Exact runtime proof

The ignored network acceptance test `exact_tiktok_single_profile_second_refresh_and_restart` ran against a fresh isolated root with the exact public `https://www.tiktok.com/@tiktok` profile and pinned yt-dlp 2026.07.04. It exercised a single-video download, first bounded profile refresh, second refresh, runner shutdown/restart, and post-restart refresh.

Evidence root: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0304/runtime_20260822_v4/app_base`.

## Independent canonical re-read

- Canonical profile ID: `MS4wLjABAAAAv7iSuuXDJGDvJkmH_vz1qkDZYo1apxgzaxdBSeIuPiM`; provider: yt-dlp 2026.07.04.
- Two canonical TikTok video identities, metadata rows, source memberships, provider-subscription items, materialized library rows, and lineage rows.
- Zero orphan metadata and zero noncanonical bare-ID metadata.
- One successful `tiktok_single` transfer, one successful `tiktok_recurring` transfer, and three successful recurring refresh jobs.
- The second and post-restart refreshes enumerated the same bounded canonical set and queued zero transfers; the transfer count remained two.
- Both materialized files exist, end in `.mkv`, and are recorded as Matroska H.264/AAC.
- The persisted profile is active at 180 minutes with cap two, a saved checkpoint, two successful canonical discoveries, and no hold/error.

</topic>

<topic id="packaged-ui-and-build" status="passed" version="v1" wp="WP-0304" updated_at="2026-08-22">

# Packaged UI and build proof

- Governed desktop build: v0.1.178.
- Executable: `product/desktop/build_target/Current/release/desktop.exe`.
- Installer: `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.178_x64-setup.exe`.
- Build log: `product/desktop/build_target/logs/build_desktop_target_20260822-223147_0_1_178.log`.
- The packaged executable ran with `--agent-headless`; `/agent/state` reported `agent_headless=true`, `app_version=0.1.178`, and `current_page=tiktok_archive`.
- The semantic audit reported zero missing accessible names. The exact profile, active interval, cap, provider/version, discovery count, saved checkpoint, and non-destructive `Archive` action were visible.
- Snapshot: `governance/snapshots/WP-0304_packaged_v0_1_178/tiktok_profile_subscription_1787431194427.png`.
- State dump: `governance/snapshots/WP-0304_packaged_v0_1_178/tiktok_profile_subscription_1787431194437.dump.json`.
- Original-resolution inspection found readable controls and state, coherent navigation, no text overlap, and no hidden critical state.

</topic>

<topic id="verification" status="passed" version="v1" wp="WP-0304" updated_at="2026-08-22">

# Verification commands and results

- `cargo test --manifest-path product/engine/Cargo.toml --lib -- --test-threads=1` — 581 passed, 0 failed, 4 explicitly ignored.
- `cargo test --manifest-path product/desktop/src-tauri/Cargo.toml --lib -- --test-threads=1` — 46 passed, 0 failed after correcting two stale six-track assertions to the canonical nine-track shape.
- `npm run test:contracts` from `product/desktop` — 251 passed, 0 failed.
- Governed `governance/scripts/build_desktop_target.ps1` build — passed and produced v0.1.178.
- `governance/scripts/build_desktop_target.ps1 -ValidateOfflinePayloadOnly` — passed; 6,190,909,876-byte payload, bundle `offline_full_win64_20260822_201543`, pinned-manifest SHA-256 `06655594710E1805BB1D9354897DF3CE35F913EE709E613BE9A29BEAAEA46DF4`.
- `cargo fmt --manifest-path product/engine/Cargo.toml -- --check` — passed.
- `git diff --check` — no whitespace errors; Git emitted only working-tree line-ending warnings.

</topic>

<topic id="adversarial-review" status="passed" version="v1" wp="WP-0304" updated_at="2026-08-22">

# Adversarial review

## DIFF_ATTACK_SURFACES

Canonical video/profile identity, mutable handle aliases, single/profile separation, discovery/checkpoint transaction order, duplicate suppression, operator-deleted suppression, independent track/settings persistence, yt-dlp effective options, MKV finalization, additive migration, Tauri wiring, and packaged UI state.

## INDEPENDENT_CHECKS_RUN

Fresh exact network runtime, independent read-only SQLite and filesystem reconciliation, serial engine suite, independent Tauri suite, frontend contracts, governed build, independent offline-payload validation, headless semantic audit, state dump, and original-resolution screenshot inspection.

## COUNTERFACTUAL_CHECKS

Using the mutable handle as identity would split renamed URLs; recording discovery after suppression would lose membership evidence; advancing only a UI cursor would not survive restart; reusing YouTube/Instagram tracks would change the nine-track snapshot; allowing MP4 finalization would contradict the observed files. Executed tests and canonical evidence reject each counterfactual.

## BOUNDARY_PROBES

Single versus profile URL acceptance, TikTok versus other-provider tracks and budgets, public profile enumeration, first/second/restarted refresh, discovery/queue/materialization, metadata/identity/lineage joins, archive/reactivation, provider API settings validation, app bridge, packaged runtime, and offline payload.

## NEGATIVE_PATH_CHECKS

Profile URLs are rejected by the single lane; archived subscriptions cannot enqueue; paused and held rows do not auto-queue; repeated canonical discovery stays idempotent; private/session, IP/rate, API/device/app, content, network, storage, tool, and extractor outcomes remain distinct; unsafe API host and pacing values are rejected.

## INDEPENDENT_FINDINGS

The joint provider review found and fixed provider-install liveness/recovery/hash defects that blocked the governed offline build, corrected the misleading `Delete` UI label to `Archive`, and corrected two Tauri diagnostics assertions that still expected six scheduler tracks rather than the canonical nine.

## RESIDUAL_UNCERTAINTY

TikTok is an external extractor-backed service and future upstream API drift can still require a provider/runtime update. Current exact single/profile behavior, replay, restart, metadata, MKV output, provider settings, failure taxonomy, and packaged operator surface are proven on yt-dlp 2026.07.04. The Tauri formatter still reports pre-existing broad formatting drift in `src-tauri/src/lib.rs`; this is non-functional debt and the 46-test boundary suite passes.

</topic>
