# Work Packet: WP-0098 - Legacy downloader library reconciliation

## Metadata
- ID: WP-0098
- Owner: Codex
- Status: DONE
- Created: 2026-03-07
- Target milestone: Workflow and archive UX hardening

## Intent

- What: Reconcile large existing downloader-managed folder trees, playlists, and subscription layouts into VoxVulgi without flattening or destructive moves.
- Why: Operators may already have extensive video libraries on NAS or local disks, and VoxVulgi needs a migration and indexing path that respects those existing structures.

## Scope

In scope:

- Inspect and index an existing archive root produced by an older downloader workflow.
- Preserve playlist, channel, subscription, and folder structure as much as possible in VoxVulgi's library views and dedupe/index state.
- Add a non-destructive reconciliation flow for large existing roots on local disks or NAS-backed storage.
- Define how old downloader repo/app-data metadata maps onto VoxVulgi folder maps, subscription groups, and library containers.

Out of scope:

- Modifying or deleting the legacy downloader's data.
- Forced relocation of existing media into VoxVulgi-owned folders.

## Acceptance criteria

- VoxVulgi can point at a large existing archive root and index it without flattening the folder tree.
- Playlist/subscription/channel structure from the legacy downloader remains visible or mappable inside VoxVulgi.
- Reconciliation is non-destructive and backup-first.
- Governance and proof capture the exact legacy inputs used for validation.

## Test / verification plan

- Manual smoke against representative legacy downloader repo/app-data/output structures supplied by the operator.
- Focused engine and desktop verification once the mapping design is implemented.

## Risks / open questions

- This WP depends on inspecting the older downloader's repo/app-data/output structure to finalize the reconciliation model.

## Status updates

- 2026-03-07: Created from operator feedback on NAS-backed archive reuse and preserving old downloader playlist/subscription folder intent.
- 2026-03-07: Implemented a read-only, bounded reconciliation path with operator-supplied legacy root/install inputs, local JSON analysis reports, and offloaded import/analysis commands so large NAS roots do not block the UI thread.
- 2026-03-07: Deepened the reconciliation model against the operator's real 4KVDP/NAS inputs: VoxVulgi now auto-detects the old 4KVDP app-state SQLite, correlates managed subscription/playlist directories with the selected root, distinguishes unmatched manual folders and loose root files, and offers direct SQLite-based import with seeded yt-dlp archive history while remaining read-only toward the NAS.
