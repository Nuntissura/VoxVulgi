# Work Packet: WP-0220 - Multi-library video archive and archiver IA

## Status

IN_PROGRESS

## Base Scope

- Treat legacy 4KVDP imports, new YouTube singles, new playlists, and new subscriptions as one canonical video archive system.
- Preserve the old 4K-style folder shape for YouTube libraries: library root -> subscription/playlist/container folder -> media files, without adding a `youtube` folder layer under the chosen library root.
- Add multiple video libraries that can exist in parallel, including unavailable NAS libraries that should not block the app from starting.
- Make the active video library control new manual YouTube downloads only.
- Bind saved YouTube playlists/subscriptions to their own library or explicit output folder so later active-library changes do not silently move older recurring targets.
- Keep imported legacy 4KVDP subscription/playlists and their archive state working as first-class saved subscriptions/playlists.
- Move legacy import/recovery controls toward Options/advanced recovery surfaces after the canonical library path is in place.
- Improve the bottom media list so singles, playlists, and subscriptions can be searched, grouped, separated, and sorted predictably.

## Operator Request Preserved

- "New videos and the videos that get downloaded based on the legacy playlists and subscriptions are considered as one."
- "Move the legacy part of the video downloader to the settings/options menu or depreciate it even" only if old videos are still recognized and old playlists/subscriptions still work.
- "Keep the app running while the legacy folders/default playlists and subscriptions are not available."
- "Multiple libraries that can work in parallel."
- NAS library can hold Kpop/Porn legacy archives; local disk library can hold active work, research material, samples, and similar active collections.
- New folder structure should mirror the old 4K app structure, not `youtube/<channel>/video file`.
- Top controls should expose session cookies/browser choice and active library.
- Tabs should split non-YouTube Video Archiver, YouTube single, YouTube playlist/subscription, and playlist/subscription management.
- Bottom panel should support search, flat/grouped modes, singles placement, date/name sort, and ascending/descending sort.

## Research Basis

- Jellyfin Libraries documentation (`https://jellyfin.org/docs/general/server/libraries/`) models media organization as user-managed libraries with folder paths, including manual path entry when visual selection is insufficient.
- Plex Creating Libraries documentation (`https://support.plex.tv/articles/200288926-creating-libraries/`) uses explicit library creation and source-folder selection rather than one mutable global media root.
- yt-dlp README (`https://github.com/yt-dlp/yt-dlp/blob/master/README.md`) documents output templates, cookies, download archives, and subtitle options; VoxVulgi should keep using those primitives rather than inventing a downloader state machine.
- Existing VoxVulgi WP-0160 and WP-0195 already decouple managed subscription continuity state from legacy output folders; this packet extends that design instead of replacing it.

## High-ROI Additions

- Add library availability status so missing NAS roots are visible but non-fatal.
- Add subscription-to-library binding now, because it prevents future accidental downloads into the wrong disk after the operator switches active libraries.
- Reuse existing app-managed yt-dlp archive state so old downloaded items still suppress re-downloads even when the output folder is external.
- Add sort direction and singles placement to the existing bottom media list, because that directly addresses the "recent singles are buried" complaint with small UI/code cost.
- Add a move-to-library command for saved subscriptions/playlists later, reusing the same output-dir resolution code.

## Reused Systems

- `youtube_subscription.output_dir_override` for legacy and explicit pinned targets.
- App-managed YouTube archive files under `library/subscriptions/youtube/<subscription_id>/voxvulgi_youtube_archive.txt`.
- Existing 4KVDP state import and archive seeding.
- Existing `feature_storage_roots.json` and download root status UI as a bridge while the new video-library registry becomes canonical.
- Existing media list grouping/filtering from WP-0170.

## Gaps Closed

- New recurring YouTube targets no longer depend on a mutable global active folder after they are saved.
- New YouTube single downloads can target the selected active video library without requiring a per-batch output override.
- Legacy/NAS folders can be unavailable at startup without making the whole app unusable.
- The visible folder model stops implying that "legacy" and "new" are different archive worlds.

## Risks And Hardening

- Risk: changing default output resolution can move downloads unexpectedly.
  - Remediation: preserve `output_dir_override` as strongest precedence, bind new subscriptions to a library, and show the effective folder in management rows.
- Risk: a NAS library may be offline during refresh.
  - Remediation: list it as missing and fail only the job that needs that library, with a direct path message.
- Risk: large NAS scans may freeze the desktop UI.
  - Remediation: keep scans in blocking worker threads, retain depth/file limits, and avoid first-open full-library hydration.
- Risk: imported 4KVDP state could be treated as a one-time legacy silo.
  - Remediation: keep imported rows in the same subscription table and preserve app-managed archive state as the canonical duplicate guard.
- Risk: UI declutter could hide recovery tools too soon.
  - Remediation: first make the canonical paths work, then move legacy tools into an Options/advanced recovery surface.

## Acceptance Criteria

- A video-library registry exists with list, add/load, remove-without-content-delete, set-active, and availability status.
- At least one default video library is bootstrapped without requiring the folder to be online.
- Manual YouTube downloads without an override use the selected active video library.
- New saved YouTube subscriptions can be bound to the selected library, and refresh uses that library even after the active library changes.
- Explicit output overrides and imported legacy output folders keep taking precedence over library selection.
- Subscription output paths use `library_root/<folder_map>` and do not insert `youtube` or `subscriptions` under the selected library root.
- Media list includes sort direction and a singles placement/grouping control.
- Legacy import/recovery is available from Options -> Advanced Recovery, not the Video Archiver main surface.
- Browser-cookie fallback requires an explicit browser choice instead of silently defaulting to Chrome.
- Video Archiver exposes separate tabs for YouTube single downloads, YouTube playlist/subscription management, and website/non-YouTube batch downloads.

## Verification

- Add Rust tests for video-library registry behavior, subscription output precedence, and the no-extra-`youtube` path shape.
- Run `cargo test` in `product/engine`.
- Run `npm run build` in `product/desktop`.
- For UI behavior, use the headless agent bridge snapshot/dump path if the app is running; otherwise record that app-boundary visual proof is pending.

## Status Updates

- 2026-05-15: Created packet from operator request; first implementation slice starts with engine library registry, subscription output binding, and bottom media-list sorting controls.
- 2026-05-15: Implemented first slice: video-library schema/API, active-library routing for direct YouTube jobs, subscription library binding, old 4K-style `library_root/<folder>` subscription output folders, and media-list direction/singles placement controls. Verified with `cargo test` in `product/engine` and `npm run build` in `product/desktop`. No running headless app bridge was available, so UI snapshot proof is pending.
- 2026-05-15: Implemented remaining WP-0220 surface work: moved legacy recovery/import controls to Options advanced recovery, split Video Archiver into YouTube single / YouTube playlist-subscription / website tabs, and replaced hardcoded Chrome cookie fallback with explicit Chrome/Firefox/Opera/Edge browser selection for direct jobs and saved recurring targets. Added schema persistence for YouTube and Instagram browser-cookie sources. Verified with `cargo test --manifest-path product/engine/Cargo.toml`, `npm run build` in `product/desktop`, and `git diff --check`.
- 2026-05-17: Operator reported a regression — single-video YouTube downloads still landed in `<library_root>/youtube/<channel>/<file>` despite acceptance criterion line 80. Root cause: `build_yt_dlp_output_template` (`product/engine/src/jobs.rs:12589`) defaulted the path template to `"%(extractor)s/%(channel)s"`. Subscriptions had been fixed to pass `"."` explicitly (jobs.rs:2804-2808) in commit 5b267de, but single-video downloads received no override and hit the default. Fix in v0.1.19: change the default to `"%(channel,uploader|misc)s"` so single-video downloads land in `<library_root>/<channel>/<file>` directly. Fallback to `uploader` covers Instagram; `misc` is the final fallback so the path never contains the literal "NA". Subscription tests at `subscriptions.rs:3147` and `:3202` still pass because the subscription path still explicitly sets `output_path_template = "."`.
