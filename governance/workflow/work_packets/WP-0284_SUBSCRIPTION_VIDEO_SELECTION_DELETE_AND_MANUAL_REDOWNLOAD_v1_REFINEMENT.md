---
file_id: WP-0284-REFINEMENT-v1
file_kind: work-packet-refinement
updated_at: 2026-07-29
---

<topic id="operator-request-and-spec-anchors" status="active" version="v1" wp="WP-0284" updated_at="2026-07-29">

# Operator request

- Select one or many subscription videos with checkboxes.
- Delete selected video files from disk while retaining a durable deleted state that prevents automatic redownload.
- Keep deleted videos out of normal working lists and provide an explicit way to redownload selected deleted videos.
- Never let a redownload-all, subscription refresh, batch repair, retry-all, or other automatic job redownload an operator-deleted video.
- Expose the workflow in both Video Archiver subscription detail and Media Library, with screen-appropriate behavior.

# Spec anchors

- `governance/spec/PRODUCT_SPEC.md`: canonical media identity, subscription membership, explicit missing-media repair, bounded canonical-set actions, and cross-screen library behavior.
- `governance/spec/TECHNICAL_DESIGN.md`: `service + media_id` identity, `library_item`, source memberships, preflight, dispatch-time identity gate, and bounded UI projections.
- `governance/workflow/PROOF_STANDARD.md`: focused engine/frontend tests, real app-boundary inspection, visual evidence, and proof summary.

# Scope edges

- In scope: canonical YouTube library items; explicit stable-ID selection; recoverable Recycle Bin deletion and explicit permanent deletion; durable per-item lifecycle state; exact-job manual-redownload authorization; Video Archiver subscription detail; Media Library; partial-failure receipts.
- Non-goals: deleting library metadata, identities, memberships, subscriptions, playlists, job history, or third-party records; automatically deleting duplicate files; making headless audit controls authorize destructive clicks; resuming the paused queue owned by WP-0283.
- Existing user subscriptions, playlists, video metadata, and third-party stores remain preserved.

</topic>

<topic id="research-basis-and-selected-approach" status="active" version="v1" wp="WP-0284" updated_at="2026-07-29">

# Sources checked

- W3C ARIA Authoring Practices grid pattern: `https://www.w3.org/WAI/ARIA/apg/patterns/grid/`
- W3C ARIA Authoring Practices checkbox pattern: `https://www.w3.org/WAI/ARIA/apg/patterns/checkbox/`
- MUI Data Grid row-selection contract: `https://mui.com/x/react-data-grid/row-selection/`
- yt-dlp download-archive behavior: `https://github.com/yt-dlp/yt-dlp/blob/master/README.md`
- Rust `trash` crate API and platform behavior: `https://docs.rs/trash/latest/trash/`
- Microsoft `IFileOperation` deletion/recycle semantics: `https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nn-shobjidl_core-ifileoperation`
- Microsoft OneDrive delete/restore workflow: `https://support.microsoft.com/en-us/office/delete-files-or-folders-in-onedrive-21fe345a-e488-4fa7-932b-f053c1bebe8a`
- Amazon S3 delete-marker model: `https://docs.aws.amazon.com/AmazonS3/latest/userguide/DeletingObjectVersions.html`
- Google Drive trash/restore behavior: `https://support.google.com/drive/answer/2375102`
- Radarr issue discussions and Sonarr operator reports were checked for deletion/redownload failure modes; they are treated as non-authoritative field reports, not implementation authority.
- Hugging Face and Civitai were searched; no directly applicable canonical-media lifecycle implementation was found.

# Relevant patterns

- Checkbox selection is the established explicit multi-row model. The header control applies only to the described loaded set, and selected state is controlled by stable IDs.
- Destructive actions need a persistent selection toolbar, an exact count, and a confirmation that distinguishes recoverable trash from permanent deletion.
- A durable tombstone must be separate from physical-file absence. Otherwise a background downloader interprets operator intent as repairable missing media.
- Enqueue-time suppression is insufficient because legacy, retry, and already-queued jobs can cross the boundary later. Dispatch must verify the tombstone and an exact authorized job ID.
- Deleted records remain discoverable through a dedicated Deleted projection and restore/redownload action rather than being silently mixed into or lost at the bottom of a paginated list.

# Existing systems reused

- Schema-v26 canonical source identity and active claim.
- Schema-v27 many-to-many source membership.
- WP-0273 present/missing/unreachable observation and canonical import repair.
- WP-0281 subscription membership backfill.
- WP-0283 execution-boundary identity gate.
- Existing bounded subscription and Media Library projections, action rows, Tauri command boundary, diagnostics trace, and headless visual debugger.

# Rejected options

- Treat deleted media as ordinary `missing`: automatic subscription refresh and retry paths are allowed to repair missing items.
- Only write the yt-dlp archive: that file is subscription-scoped, does not cover all ingress/retry paths, and cannot authorize one exact manual job.
- Drop deleted rows to the absolute bottom only: paginated/filtered screens may make them undiscoverable. Use normal/deleted projections and keep deleted rows last only in an explicit All view.
- Select all canonical rows from a loaded page: this risks applying an action to unseen rows. The control says `Select loaded` and submits exact item IDs.
- Delete metadata with the file: destroys the source identity needed to prevent redownload and removes recovery context.
- Expose destructive actions through the generic headless safe-action bridge: violates its read-only mutation boundary.

# Selected approach

1. Add a durable library-item lifecycle state: `available`, internal `delete_pending`, or `operator_deleted`, with change timestamp/source, delete method, and optional exact authorized redownload job ID.
2. Delete by explicit item IDs. Move to the OS Recycle Bin by default; permanent deletion is a separate choice. Preserve all metadata and memberships and return a per-item receipt.
3. Keep `operator_deleted` distinct from present/missing/unreachable. Preflight, all enqueue paths, generic retries, subscription refresh, batch repair, and dispatch suppress it.
4. An explicit manual-redownload command creates one new job per selected deleted item and stores that exact job ID as the only authorization. The dispatch gate admits only that job.
5. Clear the tombstone only after the authorized job successfully imports the replacement file. Failure leaves the item deleted and requires another explicit selection-scoped redownload.
6. In Video Archiver, show Pending, Downloaded, and Deleted sections for the selected subscription with one selection toolbar. In Media Library, add Available/Deleted/All status filtering, checkbox selection, and the same actions.

</topic>

<topic id="roi-red-team-and-verification" status="active" version="v1" wp="WP-0284" updated_at="2026-07-29">

# Base scope

- Exact single/bulk selection in both requested screens.
- Recoverable/permanent file deletion with preserved metadata.
- Durable suppression across every automatic and aggregate redownload path.
- Exact selected-item manual redownload and successful-import restoration.

# High-ROI additions

- Use canonical subscription membership rather than output-folder prefix for the subscription detail list; this reuses the identity graph and keeps moved/shared videos attributable.
- Return partial receipts instead of an all-or-nothing toast; this prevents successful deletions from being hidden when one NAS item fails.
- Preserve `delete_pending` during filesystem handoff; this prevents an interrupted deletion from reopening an automatic-redownload window.
- Make `Select loaded` explicit and expose loaded-versus-total truth; this reuses bounded projections and prevents pagination mistakes.
- Add a dedicated Deleted projection; this makes recovery cheap without polluting normal browsing.

# Risks, failure scenarios, and controls

- NAS storage is unreachable and is mistaken for a missing file. Control: bounded three-state observation; unreachable fails the item and does not mark it deleted.
- App crashes between filesystem deletion and DB finalization. Control: write `delete_pending` before the file operation, suppress it like deleted, and reconcile the final state from exact path evidence.
- Recycle Bin is unavailable on a network filesystem. Control: report the exact item failure and allow the operator to select explicit permanent deletion; never silently fall back.
- A general retry creates a new job after manual deletion. Control: deleted claims require the stored exact authorization job ID at enqueue and dispatch.
- A redownload-all discovers the URL through another subscription membership. Control: lifecycle authority lives on the canonical library item, not the membership or URL.
- A stale authorized job remains when the user repeats manual redownload. Control: an active exact authorization is returned unchanged rather than replaced by a job that was never enqueued; terminal creation/download failure releases it, and the next explicit command may then create a new exact authorized job.
- Mixed selection contains available and deleted rows. Control: each action states and affects only its eligible subset and reports skipped IDs.
- A bulk UI action targets only visible rows while claiming all. Control: submit exact selected stable IDs and label page-level selection `Select loaded`.

# Verification

- RED then GREEN schema/migration and engine tests for delete lifecycle, metadata/membership preservation, unreachable storage, partial receipt, generic-claim suppression, generic retry suppression, exact authorized claim, stale authorization, dispatch gate, failed redownload, and successful-import restoration.
- Frontend contract tests for stable-ID checkboxes, `Select loaded`, exact command payloads, Available/Deleted projections, confirmation wording, and absence of destructive `data-agent-safe-action`.
- Run engine tests, desktop frontend tests, TypeScript build, Rust checks, and the governed desktop target build.
- Launch the built executable with `--agent-headless`; verify `agent_headless=true` and the built version.
- Navigate to Video Archiver and Media Library, audit semantics, inspect snapshots and dumps at the minimum supported 800x600 viewport, and verify selection/action/status controls without mutating operator data.
- Produce a proof bundle with `summary.md`; do not mark DONE without the proof-standard evidence.

# Microtask plan

1. Add schema and canonical lifecycle engine APIs.
2. Gate every enqueue/retry/dispatch path and bind manual redownload to exact job IDs.
3. Replace folder-derived subscription video projection with canonical membership projection.
4. Add Tauri commands and both screen-specific selection workflows.
5. Add and run focused regression tests.
6. Build, headless-audit, visually inspect, and record proof.

</topic>
