# Work Packet: WP-0160 - Legacy YouTube subscription continuity state decoupling

## Metadata
- ID: WP-0160
- Owner: Codex
- Status: IN_PROGRESS
- Created: 2026-03-25
- Target milestone: Post-0.1.6 smoke regression recovery

## Intent

- What: Decouple YouTube subscription continuity state from the physical output folder so migrated legacy/NAS-backed subscriptions keep dedupe and refresh continuity even when the default downloader root changes or the output folder is a legacy archive path.
- Why: Current imported legacy subscriptions already preserve `output_dir_override`, but the "already downloaded" archive file still rides inside the output folder. That makes continuity state depend on the NAS path instead of VoxVulgi-managed state and creates drift against the read-only legacy archive contract.

## Scope

In scope:

- Move YouTube subscription archive/dedupe tracking into VoxVulgi-managed app data rather than the subscription output folder.
- Automatically merge any existing legacy output-folder `voxvulgi_youtube_archive.txt` into the new app-managed archive location so older installs keep continuity.
- Keep per-subscription `output_dir_override` behavior intact so legacy subscriptions continue downloading into their mapped NAS folders.
- Surface continuity intent in the Library subscription UX so operators can see that folder targeting and tracking state are separate concerns.

Out of scope:

- Full root-profile switching or offline/online root failover orchestration.
- Reorganizing or rewriting the legacy media archive itself beyond bounded archive-state migration/merge.

## Acceptance criteria

- Imported legacy YouTube subscriptions continue refreshing against their existing mapped NAS folders even when the global download root differs.
- "Already downloaded" tracking survives independently of the output folder location.
- Existing NAS-side `voxvulgi_youtube_archive.txt` state is preserved by merge into the app-managed continuity state.
- Operator-facing UX makes it clear that legacy mapped folders remain the output target while tracking state lives in VoxVulgi-managed storage.

## Test / verification plan

- Focused engine tests for app-managed archive path resolution and merge-from-legacy-output behavior.
- Desktop build verification after Library UI continuity messaging is updated.
- Follow-on desktop app smoke on one migrated legacy subscription.

## Risks / open questions

- Older builds may already have written archive state into NAS folders, so migration must merge rather than replace.
- Root-profile switching remains a separate follow-on packet once continuity state is no longer path-bound.

## Status updates

- 2026-03-25: Created after confirming that imported legacy subscriptions already preserve NAS `output_dir_override`, but continuity tracking still writes/reads `voxvulgi_youtube_archive.txt` from the output folder instead of app-managed state.
- 2026-03-25: First implementation slice landed. YouTube subscription refresh and archive-append now resolve continuity state from VoxVulgi-managed app data, with one-time merge of any legacy NAS/output-folder `voxvulgi_youtube_archive.txt` into the app-managed archive path.
- 2026-03-25: Verification passed with `cargo check --manifest-path product/engine/Cargo.toml`, `cargo test --manifest-path product/engine/Cargo.toml ensure_archive_state_merges_legacy_output_archive_into_app_managed_path -- --nocapture`, `cargo test --manifest-path product/engine/Cargo.toml import_4kvdp_state_maps_to_root_and_seeds_archives -- --nocapture`, and `npm run build`. Remaining proof is app-boundary smoke on a migrated legacy subscription.
