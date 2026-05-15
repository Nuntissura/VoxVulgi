# Work Packet: WP-0195 - Archive output naming and target-root clarity

## Metadata
- ID: WP-0195
- Owner: Codex
- Status: IN_PROGRESS
- Created: 2026-04-23
- Target milestone: Desktop archive/operator usability

## Intent

- What: Make archive output names and effective target roots predictable across imported NAS-backed subscriptions, new VoxVulgi subscriptions, one-shot YouTube downloads, and Instagram archive runs.
- Why: Operator smoke showed that old and new archive targets do not feel like one coherent system, and current folder/file names do not reliably preserve meaningful playlist/channel/profile/video context.

## Scope

In scope:
- Clarify effective target roots before queueing and after completion.
- Improve default archiver output naming so operator-meaningful source/container names survive more often.
- Make imported legacy overrides vs VoxVulgi-managed roots visible in the UI.
- Cover one-shot and recurring archive flows where practical.

Out of scope:
- Destructive migration or rewriting of legacy archives already stored on NAS.
- A full templating language overhaul beyond what is needed for clearer default behavior.

## Acceptance criteria
- Operators can see where a queued archive run will land before starting it.
- Successful jobs and saved subscriptions expose roots/paths that make old-vs-new targeting obvious.
- Archive folder/file names preserve meaningful source/container context more consistently than the current behavior.
- Focused verification covers at least one legacy-root case and one VoxVulgi-managed-root case.

## Test / verification plan

- Inspect current one-shot and subscription target-root resolution and output naming behavior.
- Add focused desktop verification for legacy override and managed-root cases.
- Re-run desktop build verification after the changes.

## Status updates

- 2026-04-23: Created from smoke feedback that imported 4KVDP NAS targets and newer VoxVulgi outputs still feel like separate systems and that archive outputs are difficult to identify by name alone.
- 2026-04-23: Implemented a first clarity pass in `LibraryPage`: recurring YouTube and Instagram subscriptions now show target mode plus effective target path, the editor copy explicitly distinguishes managed-root behavior from pinned legacy/NAS overrides, Instagram one-shot vs saved-subscription behavior is called out, and the Media Library viewport now scales more with window height. `npm run build` passed.
- 2026-04-24: Additional operator feedback clarified the remaining gap: the issue is not only root visibility but also name preservation and mental-model clarity. Playlist/channel/profile/video names still need to survive more reliably into folder/file names, and one-shot Instagram Archiver versus recurring Instagram subscriptions still needs clearer output and retention semantics when both live against the same NAS-backed archive.
- 2026-05-15: Operator clarified that the legacy 4KVDP folder structure should be the foundation for new and migrated YouTube subscriptions/playlists/single-video archive behavior, with lightweight JSON/list backup kept for recovery and redownload. Investigation found subscription child downloads were resolving the subscription output folder and then applying the general preset path template `{provider}/{channel}`, producing unwanted nested paths such as `<legacy-folder>/youtube/<channel>/<file>`. First engine slice changes subscription child jobs to use the subscription folder itself as the container path while preserving the filename template; focused regression test `subscription_download_jobs_use_subscription_folder_as_container` passes.
- 2026-05-15: Operator decision: YouTube media should use the NAS as the canonical media root because the legacy downloaded files already live there and the location is already correct. Future YouTube archive work should treat local/app-managed storage as state, cache, backup, and diagnostics only; new YouTube downloads should land in the NAS-backed legacy-shaped structure unless the operator explicitly chooses another root.
- 2026-05-15: Speed/auth/subtitle first slice implemented after local job-log investigation and current yt-dlp reference checks. yt-dlp docs confirm `-N/--concurrent-fragments`, `--throttled-rate`, and file-access retry controls; yt-dlp YouTube wiki confirms cookies can still be rejected when the session is rotated/unusable and that YouTube request-rate limits are a known failure mode. Engine downloads now add `-N 4`, `--throttled-rate 100K`, and `--file-access-retries 10`; subtitle-enabled archive jobs now write sidecar subtitles and convert them to `.srt` instead of dropping subtitles as a speed workaround; YouTube auth errors now distinguish missing cookies, rejected saved cookies, and Chromium DPAPI browser-cookie failures. Options now exposes a saved-cookie preflight command that runs yt-dlp metadata-only against a YouTube URL before queueing long subscription batches. Focused engine tests plus `cargo check` and `npm run build` passed.
