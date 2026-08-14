---
file_id: WP-0285-v1
file_kind: work-packet
updated_at: 2026-07-30
---

<topic id="contract" status="in-progress" version="v1" wp="WP-0285" updated_at="2026-07-29">

# WP-0285 — NAS deleted-space reclamation incident and evidence handoff

- Owner: Codex
- Status: BLOCKED — OPERATOR REVIEW
- Created: 2026-07-29
- Dependencies: `WP-0277`
- Task-board row: `WP-0285`
- Refinement: Not required for this v1 incident handoff because it authorizes no product-code or product-spec change.

# Intent

Recover the NAS volume capacity that should have become free after already-completed file removals,
without deleting any additional active files, the live `Recovery` container, unrelated history,
metadata, or recovery points. Preserve a complete no-context handoff so the next agent continues from
the evidence already gathered instead of repeating the same inspections, rescans, monitors, or
incorrect Recovery-root proposal.

# Primary closure unit

The incident closes only when exact volume-level before/after evidence proves where the missing blocks
were retained and shows the corresponding capacity reclaimed, reconciled category by category against
the operator's minimum expected recovery of `2.4 TB`.

If evidence proves that part of the expectation was a same-volume move rather than a deletion, the
packet must state that amount separately and reclaim it only through an exact operator-approved
permanent deletion target. Effort, queue activity, CPU, I/O, a `Running` label, SMB disappearance,
logical folder size, or a package uninstall does not count as reclamation.

# Current authorization boundary

This packet is a draft handoff for operator inspection and additions. It is not destructive-action
approval.

- Begin or continue read-only.
- Do not delete `\\MIR\home\Recovery`; it is a live preservation-only container.
- Do not delete any live child of `Recovery`.
- Do not infer that the historical deleted target was the `Recovery` root.
- Do not delete snapshots, recycle-bin contents, version history, quarantine contents, package data,
  files, databases, or metadata without a new exact action-time confirmation naming the target and
  scope.
- Do not restart the NAS, run data scrubbing, resume VoxVulgi jobs, or enable a recurring scan.
- Do not install, uninstall, disable, or re-enable a package or Team Folder without new approval.
- Do not bypass immutable/locked snapshot protections or DSM security boundaries.
- Do not launch foreground terminals, steal focus, close browser windows/tabs, close Codex, or close
  unrelated terminals/apps.
- Do not restore, replace, merge, or otherwise modify the backed-up Chrome recovery/live session
  state without a later explicit operator instruction.
- Do not use this WP as authorization to remove DSM or replace the NAS operating system.

# In scope

- Reconcile the already-removed VoxVulgi cleanup categories and the already-deleted approximately
  2 TB child folder that used to be inside the live `Recovery` container.
- Identify the exact current block owner: live same-volume quarantine, recycle bin, Btrfs snapshot,
  clone/reflink, Synology Drive/package data, open handle, LUN/backup/package allocation, or another
  evidenced layer.
- Produce an exact target proposal before each destructive action.
- Execute only separately approved targets.
- Verify reclamation using exact free bytes at the volume boundary and DSM Storage Manager.
- Preserve all active files and all unrelated recovery/history data.

# Out of scope

- Deleting or renaming `Recovery`.
- Deleting active media, subscriptions, playlists, library metadata, databases, or unrelated files.
- Removing DSM, replacing the NAS operating system, bypassing security, or unsupported database
  surgery.
- Repeating the completed Synology Drive rescan.
- Reinstalling Synology Drive Server merely to inspect the same stale catalog.
- Recurring free-space polling with no state transition.
- Data scrubbing.
- NAS restart without a later explicit approval.
- Chrome session restoration. The session backup and intentional purge of Chrome recovery/active
  sessions are complete; the later restore into both recovery state and live state remains a separate
  outstanding task and must not be silently claimed complete through this NAS packet.

# Acceptance criteria

- The live `Recovery` container remains present and no active child is deleted.
- Every reclaimed byte is attributed to a named retention/live-data layer and exact approved action.
- Exact free bytes are captured immediately before action and after the volume has stabilized.
- Reclaimed bytes are compared against:
  - `.part` files: `212.862604241 GB`
  - 1,826 exact duplicate videos: `256.561005896 GB`
  - redundant artifact quarantine: `123.181662082 GB`
  - already-deleted child inside `Recovery`: approximately `2 TB`
  - operator's minimum aggregate target: `2.4 TB`
- Same-volume quarantine moves are not counted as deleted or reclaimed.
- No CPU, I/O, worker, queue, indexing, or status-label activity is reported as reclamation.
- VoxVulgi remains paused with zero running jobs.
- No data scrubbing or unapproved NAS restart occurs.
- The final report lists total, used, exact available bytes, bytes reclaimed, actions taken,
  remaining retention layers, unresolved discrepancy, and preservation checks.
- Completion satisfies `governance/workflow/PROOF_STANDARD.md`; otherwise status remains
  `IN_PROGRESS` or becomes `BLOCKED` with the exact external blocker.

</topic>

<topic id="relayed-message-verbatim" status="source-record" version="v1" wp="WP-0285" updated_at="2026-07-29">

# Relayed message from the previous-session assistant — pasted verbatim by the operator

The following fenced block is a historical source record. It is intentionally reproduced verbatim,
including wording, spelling, punctuation, indentation, claims, and requested workflow. Later operator
corrections and current evidence are recorded outside the block and take precedence where they
conflict.

```text
 Use Computer Use to inspect my already-open Firefox session. Firefox has an authenticated Synology DSM Control Panel session for NAS `MIR`.

  Problem

  Large file deletions are visible at the SMB/filesystem level, but the NAS volume has not regained the expected free space.

  Verified storage baseline:

  - Total NAS volume: 61,399,107,633,152 bytes
    - 61.399 TB decimal
    - 55.842 TiB binary
  - Free: 77,278,429,184 bytes
    - 77.278 GB decimal
    - 71.971 GiB binary
  - Approximately 99.8741% used

  Recent VoxVulgi cleanup removed visible files totalling 592.605272219 GB:

  - `.part` files: 212.862604241 GB
  - 1,826 exact duplicate videos: 256.561005896 GB
  - Redundant artifact quarantine: 123.181662082 GB

  There was also an older deletion of more than 2 TB from:

  `\\MIR\home\Recovery`

  Those files disappeared from the share, but the volume capacity was never reclaimed.

  The likely storage-retention layers to inspect are:

  - Synology shared-folder recycle bins
  - User home recycle bins under the `homes` shared folder
  - Btrfs snapshots
  - Synology Drive version history
  - Space reclamation status and schedule

  Do not assume which layer is responsible. Verify everything in DSM.

  Goal

  Identify exactly where the deleted blocks are retained, recover the expected storage safely, and confirm the result using DSM Storage Manager’s volume capacity—not folder sizes or SMB disappearance.

  Operating constraints

  1. Use Computer Use with the existing Firefox/DSM session.
  2. Begin read-only.
  3. Do not scan or calculate folder sizes.
  4. Do not run data scrubbing. Data scrubbing is not a space-reclamation operation.
  5. Do not resume VoxVulgi downloads or jobs. Its queue must remain paused with zero running jobs.
  6. Do not delete active media, subscription lists, playlists, or VoxVulgi library metadata.
  7. Do not empty unrelated recycle bins without first reporting their exact scope.
  8. Do not delete snapshots without first reporting:
     - shared folder
     - snapshot dates
     - number of snapshots
     - retention policy
     - locked or immutable status
     - replication status
     - estimated reclaimable space, if DSM can calculate it
  9. Do not restart the NAS unless reclamation remains stuck after retention has been removed and I explicitly approve a restart.
  10. Before any destructive DSM action, stop and give me an exact proposed-action summary and wait for my approval.

  Phase 1 — Record the DSM storage baseline

  Open Storage Manager and record:

  - NAS model
  - DSM version
  - storage pool name and status
  - volume name and status
  - filesystem type
  - total capacity
  - used capacity
  - available capacity
  - allocation or usage categories shown by DSM
  - whether the volume is currently reclaiming space
  - whether optimization, repair, expansion, scrubbing, or another storage task is running
  - warnings or critical-volume messages

  Take a screenshot of the relevant Storage Manager view.

  Do not use folder sizes as evidence.

  Phase 2 — Inspect shared-folder recycle bins

  Open:

  Control Panel → Shared Folder

  Inspect the `homes` shared folder first because `\\MIR\home` is a user-home path and DSM manages user home recycle bins through the `homes` share.

  Record:

  - whether Recycle Bin is enabled for `homes`
  - whether access is restricted to administrators
  - whether automatic emptying is configured
  - any retention period
  - whether DSM offers “Empty Recycle Bin”
  - whether that action covers every user’s home recycle bin
  - whether DSM reports the bin’s size or item count

  Also inspect the shared folder corresponding to `Recovery`, if it is separate from `homes`.

  Do not empty anything yet. Report the exact scope and any size/count visible.

  Phase 3 — Inspect snapshots

  Open Snapshot Replication, if installed.

  Inspect snapshots for:

  - `homes`
  - any separate shared folder containing `Recovery`
  - the shared folder containing the VoxVulgi video library

  For each applicable shared folder, record:

  - snapshot count
  - oldest and newest snapshot dates
  - retention schedule and policy
  - whether snapshots are locked
  - whether snapshots are immutable
  - replication relationships
  - snapshot-reserved space
  - estimated snapshot space
  - estimated space reclaimable by deleting specific snapshots, if DSM provides a calculator

  Determine whether snapshots predate the large deletions and could still reference those deleted blocks.

  Do not delete snapshots yet.

  Phase 4 — Inspect Synology Drive versioning

  If Synology Drive Server is installed, open Synology Drive Admin Console.

  Inspect Team Folder/versioning settings for:

  - `homes`
  - `Recovery`, if applicable
  - the VoxVulgi media share

  Record:

  - whether versioning is enabled
  - maximum version count
  - rotation policy
  - database or version-storage usage
  - whether deleted-file versions are retained
  - cleanup or database-reclamation options

  Do not remove versions yet.

  Phase 5 — Inspect space reclamation

  Open:

  Storage Manager → Storage → Global Settings → Space Reclamation Schedule

  Record:

  - whether space reclamation is enabled
  - its current daily schedule
  - whether reclamation is actively running
  - whether DSM reports pending or reclaimable space
  - any task status, progress, or error

  Because the volume is critically full, a temporary 24-hours-per-day reclamation schedule is acceptable after recycle-bin/snapshot/version retention is handled. Do not change the schedule until you report
  its current state.

  Phase 6 — Produce an evidence-based diagnosis

  Before making changes, report a compact table:

  | Retention layer | Enabled/present | Scope | Size/count | Likely retaining deleted blocks | Proposed action |
  |---|---|---|---|---|---|

  Also report:

  - current DSM free space
  - estimated recoverable space
  - exact recycle bins proposed for emptying
  - exact snapshots proposed for deletion
  - exact Drive versioning cleanup proposed
  - whether 24/7 space reclamation should be enabled temporarily
  - risks of each action

  Then wait for my explicit approval before destructive actions.

  Phase 7 — Execute only approved cleanup

  After approval:

  1. Reconfirm the exact selected targets.
  2. Empty only the approved recycle bins.
  3. Delete only the approved snapshots.
  4. Clean only the approved Synology Drive version history.
  5. Enable temporary continuous space reclamation if approved.
  6. Do not start data scrubbing.
  7. Do not restart the NAS.
  8. Record the time and each action taken.
  9. Capture screenshots before and after each significant change.

  Risks and mitigations

  - Risk: Emptying `homes` may delete recoverable files belonging to other home directories.
    Mitigation: Report the action’s full DSM scope before approval. Do not imply it only affects the VoxVulgi path.

  - Risk: Snapshot deletion may destroy unrelated historical recovery points.
    Mitigation: List exact snapshot identities, dates, folder scope, retention state, replication state, and estimated benefit before deletion.

  - Risk: Immutable or replicated snapshots may not be removable.
    Mitigation: Do not bypass protection. Report the blocking policy precisely.

  - Risk: Drive version cleanup may affect unrelated user documents.
    Mitigation: Identify the exact Team Folder and version scope before cleanup.

  - Risk: Reclamation may generate heavy disk I/O.
    Mitigation: Temporarily allow continuous reclamation because free space is critically low, monitor NAS health, then recommend an off-peak schedule after recovery.

  - Risk: DSM’s displayed free capacity may update slowly.
    Mitigation: Record timestamps and monitor Storage Manager rather than repeatedly deleting more data.

  - Risk: Reclamation cannot free blocks that are still referenced.
    Mitigation: Remove approved recycle-bin, snapshot, and version retention first; then let reclamation run.

  - Risk: Performing data scrubbing now adds heavy I/O without reclaiming space.
    Mitigation: Do not run it. Consider it later, after capacity is healthy.

  Verification and completion criteria

  The task is complete only when:

  - DSM Storage Manager shows materially increased available volume capacity.
  - The recovered amount is compared with the 592.605 GB recent cleanup and the historical `Recovery` deletion of more than 2 TB.
  - No unapproved recycle bins, snapshots, Drive versions, media files, or metadata were removed.
  - VoxVulgi remains paused with zero running jobs.
  - No data scrubbing was started.
  - A final before/after report includes:
    - total capacity
    - used capacity
    - available capacity
    - bytes reclaimed
    - actions performed
    - retention layers still present
    - reclamation status
    - remaining risks or blockers

  If no retention layer explains the missing capacity, stop destructive work and inspect DSM Storage Analyzer, package data usage, LUN/iSCSI allocation, Synology Drive databases, Active Backup storage, and
  volume/pool allocation—but still do not perform folder-size scans or delete anything without reporting the exact target first.
```

</topic>

<topic id="later-operator-corrections" status="authoritative" version="v1" wp="WP-0285" updated_at="2026-07-29">

# Later operator corrections that override ambiguous or conflicting relay wording

1. The operator did not authorize deletion of `Recovery`.
2. `Recovery` is a live container. The historical approximately 2 TB deletion was a child folder
   inside `Recovery`, deleted earlier by the operator. The child's exact name/path has not been
   durably recovered in this packet.
3. Everything intended for the historical deletion was already removed at the SMB-visible layer.
   The requested task is to release retained blocks, not to repeat the file deletion.
4. Do not keep checking or monitoring unchanged free space. The prior recurring checks ran for many
   hours without advancing the closure unit.
5. Do not describe indexing, queue movement, process activity, CPU, disk I/O, or Btrfs workers as
   reclamation progress. Only increased exact free bytes or a decreased retaining allocation counts.
6. Do not launch foreground terminal windows or take over the operator's PC while working.
7. Do not close or reset Chrome, Firefox, Codex, existing terminal sessions, or unrelated apps.
8. The operator wants a no-context work packet that includes the attempts, findings, subagent
   research, forgotten constraints, failures, and room for added operator notes.
9. The original retention cause had already been identified: a version-saving feature was active and
   retained deleted-file versions, preventing the deleted blocks from being reclaimed.
10. That version-saving feature was turned off.
11. Recycle bins were also turned on and their contents were deleted several times.
12. Turning off version saving and repeatedly deleting recycle-bin contents produced no noticeable
    free-capacity increase.
13. Deleting Synology Drive Server later did not solve the missing-space problem; the measured
    action-window gain was only `1,578,983,424` bytes (`1.579 GB` decimal).
14. Across the whole session, DSM's displayed free capacity rose from approximately `70 GB` to
    approximately `132.5 GB`. Because no other app, model, or project was managing deletions on the
    NAS and other workloads were more likely to add data, the operator considers delayed Synology
    reclamation the more likely explanation for that net gain.
15. The operator recalled that the WP-0277 quarantine data was later deleted and suspected that this
    added more unreclaimed disk space. The durable purge receipts and ledgers now verify that both the
    duplicate and cleanup-artifact quarantines were permanently deleted.
16. A backup of the operator's multi-window, thousands-of-tabs Chrome state was created. Chrome
    recovery and active sessions were then intentionally purged so one controlled Chrome window can
    be launched for authenticated Synology work. Later, the backup must be restored into both Chrome
    recovery state and live state. Until the operator directs those steps, do not launch, close,
    reset, restore, merge, or otherwise alter Chrome session state.
17. During backup creation, the operator requested removal of duplicate YouTube tabs from the cleaned
    Chrome recovery copy. This browser-tab cleanup is unrelated to deleting YouTube media files from
    the NAS. Preserve both the original session material and the cleaned recovery image.

</topic>

<topic id="evidence-ledger" status="current-task-record" version="v1" wp="WP-0285" updated_at="2026-07-29">

# Evidence ledger

Evidence classes:

- `RELAYED`: present in the verbatim operator-relayed source, not independently reproduced this turn.
- `SESSION-OBSERVED`: recorded from the current incident's DSM/SMB interaction.
- `REPO-VERIFIED`: opened directly from the current repository.
- `SUBAGENT-REPORTED`: returned by a named prior subagent.
- `UNVERIFIED`: a remaining claim or hypothesis that must not drive deletion.

| Evidence ID | Class | Observation | Meaning / limit |
|---|---|---|---|
| E-0285-001 | RELAYED | Total volume `61,399,107,633,152` bytes; historical free baseline `77,278,429,184` bytes. | Starting reference only; the timestamp was not retained in a durable incident log. |
| E-0285-002 | RELAYED | Expected recent VV cleanup: `212.862604241 GB` `.part`, `256.561005896 GB` exact duplicates, `123.181662082 GB` redundant artifact quarantine; total `592.605272219 GB`. | These are logical category totals. They are not proof of physical-volume reclamation. |
| E-0285-003 | OPERATOR CORRECTION | Approximately 2 TB was deleted from a child inside the live `Recovery` container; the container itself was not the target. | All future action must preserve `Recovery`; the exact deleted child identity remains a required evidence gap. |
| E-0285-004 | SESSION-OBSERVED | `\\MIR\home\#recycle`, `\\MIR\homes\iljasmets\#recycle`, and `\\MIR\homes\#recycle` exposed only `desktop.ini` during inspection. | No approximately 2 TB retained target was found in those exposed recycle-bin paths. This does not prove every DSM-internal bin/reference is absent. |
| E-0285-005 | SESSION-OBSERVED | Snapshot Replication was not installed, port `5566` was closed, and no `#snapshot` path was exposed. | This is not proof that no Btrfs snapshot/subvolume references exist. Snapshot inventory remains unresolved. |
| E-0285-006 | SESSION-OBSERVED | Synology Drive Server `4.0.3-27892` reported an implausible/stale `File Versions` allocation of `55.7 TB` and database allocation of `24.4 GB`; My Drive was enabled with versions set to None; `homes` was not enabled as a Team Folder. | The Drive allocation display was not trustworthy enough to prove a 55.7 TB retained payload. |
| E-0285-007 | SESSION-OBSERVED | Drive Version Explorer at newest/current time, view role `iljasmets`, with deleted files shown, displayed top-level `Recovery` as a normal current folder rather than a deleted-history row. | Selecting or permanently deleting the `Recovery` root would have targeted the live container and was therefore forbidden. |
| E-0285-008 | SESSION-OBSERVED | The official one-shot Drive rescan completed with queue `0`; DSM no longer showed Drive indexing; Version Explorer still showed current `Recovery`; no material free-space increase followed. | The rescan branch is exhausted. Do not rerun it without new contradictory evidence. |
| E-0285-009 | SESSION-OBSERVED | DSM Resource Monitor's open-file view showed Plex database/log activity but no open Recovery/VV target. | No relevant open handle was seen in that sample; this is not a historical proof for every deleted inode. |
| E-0285-010 | SESSION-OBSERVED | Synology Drive's recycle-bin purge/status was run and monitored through unchanged-capacity intervals. | Repeating the same purge monitor is not a remedy and produced no proven material reclaim. |
| E-0285-011 | SESSION-OBSERVED | Synology Office and Synology Drive Server were uninstalled through Package Center with package-data removal selected; Package Center then showed `Install` for both. | The supported Synology Drive/Office package layer was removed. Reinstalling it is not justified merely to repeat inspection. |
| E-0285-012 | SESSION-OBSERVED | Exact free bytes at package-removal action start: `140,613,001,216`. Stable final exact free bytes across two readings: `142,191,984,640`. | Measured net action-window increase: `1,578,983,424` bytes (`1.579 GB` decimal), far below the `2.4 TB` target. |
| E-0285-013 | SESSION-OBSERVED | Final DSM display: Volume 1 `55.7 TB | 132.4 GB free`. Live `Recovery`, `Projects`, and `Video` paths remained present. | Package removal preserved sampled live roots but did not reclaim the expected capacity. |
| E-0285-014 | SESSION-OBSERVED | DSM File Change Log exposed paths under `/home/Video/vv_quarantine/WP-0277/cleanup_artifacts/...`, including `yt_dlp_format_fragment` and `temporary_download` entries. | This proved that cleanup artifacts had first been moved into a same-volume quarantine. Later purge evidence in E-0285-023 through E-0285-025 supersedes the earlier assumption that these paths remained live. |
| E-0285-015 | REPO-VERIFIED | `WP-0277` defines quarantine as a recoverable move and says permanent deletion is a separate explicit action. | The design itself does not reclaim space at quarantine time. |
| E-0285-016 | REPO-VERIFIED | `product/desktop/build_target/tool_artifacts/wp_runs/WP-0277/summary.md` says no operator NAS inventory, move, quarantine, rollback, or deletion was run during that packaged proof. | The later operator-data actions were separate manual cleanup operations, now accounted for by E-0285-023 through E-0285-025 rather than the packaged WP-0277 proof bundle. |
| E-0285-017 | SUBAGENT-REPORTED | The `firefox_dsm_session` subagent identified the supported one-shot Drive rescan and warned that disabling/re-enabling a Team Folder/My Drive destroys all Drive history and breaks client sync tasks. | The safe one-shot was completed; the destructive fallback was not approved or used. |
| E-0285-018 | SUBAGENT-REPORTED | Drive research identified Version Explorer's current-time `Show deleted files` and selected-row `More -> Delete permanently` path as the supported item-scoped route. | The required deleted child row never appeared; the live `Recovery` row was not an acceptable target. |
| E-0285-019 | SUBAGENT-REPORTED | Package research identified Package Center uninstall with deletion of package-owned items as the supported Drive-data removal route and warned that plain uninstall may preserve data. | The package-data removal path was subsequently used and reclaimed only `1,578,983,424` bytes in the measured action window. |
| E-0285-020 | UNVERIFIED | One or more Btrfs snapshots, hidden subvolumes, clones/reflinks, or another allocation layer may still reference blocks from the already-deleted child. | This is the highest-value unresolved evidence gap; it is not authorization to delete any snapshot or subvolume. |
| E-0285-021 | OPERATOR CORRECTION | A version-saving feature was active and was the already-identified reason deleted space was retained. It was turned off. Recycle bins were turned on and emptied several times. Neither change produced a noticeable free-space increase. | Do not restart discovery as though version retention was never found or these setting/bin actions were never performed. The remaining question is why the already-retained blocks were not released after those actions. |
| E-0285-022 | OPERATOR OBSERVATION + EXACT SESSION RECORD | DSM free capacity rose from approximately `70 GB` displayed at session start to approximately `132.5 GB` displayed later. The exact recorded baseline/final values were `77,278,429,184` and `142,191,984,640` bytes, a net increase of `64,913,555,456` bytes (`64.914 GB` decimal / `60.454 GiB`). | Some volume-level reclamation occurred across the session. The final package-removal action explains only `1,578,983,424` bytes of that increase; the remaining session gain cannot yet be assigned to one exact operation. The operator reports no competing deletion manager and notes other workloads were more likely to consume space, making delayed Synology reclamation a reasonable inference rather than direct causal proof. |
| E-0285-023 | REPO-VERIFIED DURABLE RECEIPT | SHA-256-verified `wp0277_duplicate_purge_receipt_20260729.json` records `status: complete`, `remaining_manifest_paths: 0`, and permanent deletion of 1,826 quarantined duplicates totaling `256,561,005,896` logical bytes. | The duplicate quarantine is not a current live allocation. The receipt proves logical deletion, not that the deleted files exclusively owned the same number of physical Btrfs bytes. |
| E-0285-024 | REPO-VERIFIED DURABLE RECEIPT | SHA-256-verified `wp0277_artifact_purge_receipt_20260729.json` records `status: complete`, `remaining_manifest_paths: 0`, and permanent deletion of 435 quarantined cleanup artifacts totaling `123,181,662,082` logical bytes. | The cleanup-artifact quarantine is not a current live allocation. The receipt proves logical deletion, not that the deleted files exclusively owned the same number of physical Btrfs bytes. |
| E-0285-025 | REPO-VERIFIED DURABLE LEDGER | Read-only queries of both purge ledgers found 1,826 duplicate rows and 435 artifact rows, every row marked `deleted`, zero error rows, and deleted-byte sums exactly matching the receipts. Combined permanent quarantine deletion was `379,742,667,978` bytes (`379.742667978 GB` / `353.663 GiB`). | This closes the live-quarantine provenance gap. Together with the separately recorded `.part` deletion of `212,862,604,241` bytes, the three recent cleanup categories total exactly `592,605,272,219` bytes. The remaining task is to identify the layer still referencing or withholding those deleted blocks, not to delete quarantine contents again. |
| E-0285-026 | OPERATOR CORRECTION | The operator created a backup of the prior multi-window, thousands-of-tabs Chrome state, then intentionally purged Chrome recovery and active sessions to permit one controlled Chrome window for Synology access. Restoration into both recovery and live state is planned later. | The backup/purge phases are complete; the controlled launch and later dual-state restoration are distinct operator-directed steps. This WP does not authorize changing Chrome state on its own. |
| E-0285-027 | LOCAL-FILESYSTEM VERIFIED | Backup folder `C:\Users\Ilja Smets\.codex\chrome_session_backups\2026-07-29T143606+0200_preclose` exists. The post-close manifest records 26 windows and 8,152 tabs. Its matching duplicate manifest records 1,196 duplicate YouTube tabs across 830 groups, and the cleaned session image is present; `8,152 - 1,196 = 6,956` cleaned tabs. Original session material remains under `Sessions`, `postclose\Sessions`, and `profile_session_hold`. | The backup and cleaned recovery copy are real, separate artifacts. Do not overwrite either. The exact restore source/sequence must be selected and verified at restoration time rather than inferred from filenames. |
| E-0285-028 | CURRENT OFFICIAL RESEARCH | Synology's 2026 article for the exact symptom says Btrfs deletions depend on background space reclamation, an insufficient schedule can delay release, a prolonged non-release should be followed by a NAS restart, and snapshots or file clones can still retain capacity. | This makes a bounded reclamation-schedule check and one separately approved controlled restart vendor-supported branches. It does not authorize a restart in this packet. |
| E-0285-029 | CURRENT OFFICIAL RESEARCH | Upstream Btrfs defines qgroup `referenced` as reachable extents including shared data and `exclusive` as extents freed when all subvolumes in that qgroup are deleted. Synology Fast Clone and snapshots share physical blocks. | Logical file totals, qgroup `referenced`, and Drive logical-size displays must not be treated as byte-for-byte physical reclaim estimates. Exact exclusive/shared-block evidence is required. |
| E-0285-030 | CURRENT OFFICIAL RESEARCH | Synology Storage Manager Usage Details separates Shared Folder, LUN/VMM, Synology Drive, Snapshot, and Others; Synology's official SSH storage diagnostic names hidden owners including `@sharesnap`, `@iSCSI`, `@ActiveBackup`, and `@synologydrive`. | Usage Details is the supported first attribution surface. The official broad `du` command remains outside this WP's no-folder-size-scan boundary unless the operator grants a specific waiver. |
| E-0285-031 | CURRENT TECHNICAL RESEARCH | Read-only Btrfs inventory can distinguish filesystem accounting, snapshots/subvolumes, deleted subvolumes awaiting cleanup, existing qgroup accounting, and open-unlinked files. Raw balance, quota enable/rescan, filesystem repair, cache clearing, subvolume deletion, and manual reclaim commands are state-changing or unsupported remedies on this appliance. | The recommended next action is a bounded read-only DSM/SSH evidence capture. Any incompatible or unavailable Synology command is recorded as unavailable; no replacement Btrfs tool is installed. |
| E-0285-032 | PLAN-A PREFLIGHT VERIFIED | Chrome `150.0.7871.187` was running. The ChatGPT Chrome Extension `1.2.27221.15725_0` was installed and enabled in the selected `Default` profile. The native-host manifest, registry path, host name, and allowed extension origin all validated. Two attempts to acquire the Chrome control binding and read its required documentation timed out at 30 and 60 seconds. | The Chrome installation surfaces are present, but the live control channel is not attached. Per the Chrome-control recovery contract, opening a Chrome window for the selected profile requires explicit operator approval. No browser window, tab, recovery file, or session state was changed. |
| E-0285-033 | PLAN-A PREFLIGHT VERIFIED | At `2026-07-30T00:00:04.3684256+02:00`, `MIR` resolved to `192.168.0.253`. The configured SSH destination was port 22, and the NAS actively refused the TCP connection. | SSH is not accepting connections on port 22. Plan A cannot collect Btrfs evidence until the operator separately approves enabling SSH or approves another exact read-only DSM execution surface. No password prompt, login, configuration change, or NAS command occurred. |
| E-0285-034 | PLAN-A DSM VERIFIED | DSM Info Center identified model `DS1825+` running `DSM 7.3.2-86009 Update 4`. Storage Manager identified `Storage Pool 1` as `JBOD (Without data protection)`, `58.2 TB` total and fully allocated, with all four drives shown Healthy. Volume 1 is Btrfs, `55.8 TB` total, `55.7 TB` used, `99%`, and `132.7 GB` available in the captured DSM views. Data Scrubbing status was Ready and “Never performed yet”; no scrub was started. | This is the current supported DSM storage baseline. The pool has no redundancy and no unallocated pool capacity, which increases the preservation requirement for every later action. DSM's rounded figures are not exact-byte evidence. |
| E-0285-035 | PLAN-A DSM VERIFIED | Volume 1 Usage Details reconciled the `55.8 TB` volume as `Shared Folder: 29 TB`, `Synology Drive: 26.7 TB`, `Others: 3.9 GB`, and `Available capacity: 132.7 GB`. No Snapshot or LUN/VMM category appeared. The dialog states that its calculation is based on disk space taken up. | Synology Drive is now the dominant proven physical allocation category and is large enough to explain the operator's missing-space target. The missing Snapshot category materially weakens snapshots as the principal owner, though only Btrfs inventory could prove that no hidden snapshot/subvolume exists. |
| E-0285-036 | PLAN-A DSM VERIFIED | Following the Synology Drive link from Usage Details opened Package Center at the Synology Drive Server page, where the action was `Install`, not Open/Run. | Synology Drive Server remains uninstalled while Storage Manager still attributes `26.7 TB` of disk usage to Synology Drive. This is direct evidence of a retained/orphaned Drive-class allocation after package removal; reinstall was not performed. |
| E-0285-037 | PLAN-A DSM VERIFIED | Storage Manager's Space Reclamation Schedule time grid showed every hour of every day blue (`Run space reclamation`), i.e. a continuous 24/7 schedule. The dialog was closed with Cancel and no schedule change was saved. | Insufficient scheduled reclamation time is ruled out as the current explanation. Continuous scheduling has already been in place; more schedule waiting/toggling is not a new remedy. |
| E-0285-038 | PLAN-A SSH-LIFECYCLE VERIFIED | With operator approval, SSH was temporarily enabled on port 22 and DSM confirmed `Changes applied`. TCP/22 then accepted connections. The NAS presented RSA fingerprint `SHA256:Hg2yxdHQ8fvjosUy73vNAtsuJWr1GLPrtnBgi+ADgRA` and ED25519 fingerprint `SHA256:eZYmGZjS4NYTE07FBOwhfTQot+N3CZI5rolnZ+r8N4U`. Non-interactive login as administrator account `iljasmets` was rejected with `Permission denied (publickey,password)` before any NAS command ran. | The Btrfs command baseline was not executed because no authorized local public key was configured and no password was inspected, requested in chat, or transmitted. The DSM-only ownership result remains valid; raw subvolume/qgroup/open-file evidence remains unavailable. |
| E-0285-039 | PLAN-A SSH-LIFECYCLE VERIFIED | After the rejected authentication attempt, SSH was unchecked and applied in DSM. DSM again confirmed `Changes applied`. A post-disable TCP/22 check at `2026-07-30T01:27:03.0917591+02:00` was actively refused. | Temporary SSH was fully returned to its original disabled state. No SSH-dependent service had existed beforehand because the port was originally closed. |
| E-0285-040 | PLAN-B PREFLIGHT VERIFIED | Immediately before the approved restart, Storage Manager showed Volume 1 at `55.7 TB` used and `132.7 GB` free. The preflight timestamp was `2026-07-30T01:39:49.8591202+02:00`. Storage Manager showed no task schedules; all four drives were Healthy; Data Scrubbing was Ready and “Never performed yet”; no repair, expansion, scrub, or package update was active or started. | The single restart began from a bounded, idle, non-degraded storage state. The pool remained JBOD without data protection, so no additional maintenance action was combined with the restart. |
| E-0285-041 | PLAN-B RESTART VERIFIED | DSM Restart was confirmed once at `2026-07-30T01:40:26.121+02:00`. DSM displayed “Synology NAS is restarting.” Local TCP checks then observed ports 443 and 5001 closed at approximately `01:42:07+02:00` and both open again at `2026-07-30T01:42:38.890+02:00`. DSM subsequently returned to its login screen and accepted the operator's normal sign-in. | This proves an actual down/up restart transition rather than a UI refresh. No second restart was performed. |
| E-0285-042 | PLAN-B STABILIZATION VERIFIED | After startup, DSM free capacity advanced monotonically from `1.7 TB` through repeated rounded display steps until the Volume 1 pair remained unchanged across consecutive checks at `45.3 TB` used and `10.5 TB` free. A newly opened Usage Details dialog then confirmed `81%` used and `10.5 TB` available. | The restart produced a material displayed-capacity recovery of approximately `10.37 TB` relative to the `132.7 GB` pre-restart DSM baseline. This is a rounded DSM comparison, not an exact-byte subtraction. The restart must not be repeated as a reclaim trial. |
| E-0285-043 | PLAN-B OWNERSHIP VERIFIED | Stabilized Usage Details reconciled Volume 1 as `Shared Folder: 29 TB`, `Synology Drive: 16.3 TB`, `Others: 3.3 GB`, `Available capacity: 10.5 TB`, `Total: 55.8 TB`. The pre-restart comparison was `29 TB`, `26.7 TB`, `3.9 GB`, and `132.7 GB`, respectively. | Shared Folder stayed unchanged while the Synology Drive category fell by `10.4 TB` and available capacity rose by the corresponding rounded magnitude. The released owner category is therefore Synology Drive, not live shared-folder data. |
| E-0285-044 | PLAN-B POST-HEALTH VERIFIED | At the stabilized reading, all four drives still showed Healthy. Storage Pool 1 remained `58.2 TB` JBOD and fully allocated; Volume 1 remained Btrfs `55.8 TB`. Data Scrubbing remained Ready and never performed. The displayed Warning text was specifically the low-available-space threshold suggestion, not drive degradation, repair, or filesystem failure. | The recovery completed without a concurrent scrub, balance, repair, rescan, package action, or disk-health event. The action-window attribution remains one controlled restart plus DSM's own post-start background cleanup. |
| E-0285-045 | PLAN-B EXACT-BYTE LIMITATION | DSM exposed rounded TB/GB figures only. A read-only Windows `GetDiskFreeSpaceEx` probe against `\\MIR\home` timed out after 10 seconds, so no exact-byte post-restart value was obtained. The final DSM login expired immediately after the stabilized Usage Details capture. | Do not relabel the approximately `10.37 TB` displayed gain as exact bytes. The prior exact session values remain valid historical evidence, but they are not the immediate Plan B action-window baseline/final pair. |
| E-0285-046 | PLAN-B PRESERVATION BOUNDARY | No NAS file, folder, snapshot, package, schedule, scrub, balance, repair, or rescan action was executed during Plan B. Shared Folder usage remained exactly `29 TB` in both pre- and post-restart Usage Details. A named post-restart File Station existence check for `Recovery`, `Projects`, and `Video` was attempted only after the final capacity capture, but DSM returned “Your login is invalid. Please sign in again” before File Station opened. | Unchanged shared-folder allocation and the absence of file actions support preservation, but the exact named paths were not post-verified in this action window. Do not claim that spot check as completed. Subscriptions, playlists, library metadata, and paused queue were not modified. |

# Capacity timeline

| Point | Exact free bytes | Change from immediately prior comparable point | Proof use |
|---|---:|---:|---|
| Historical relayed baseline | `77,278,429,184` | n/a | Context only; do not attribute later changes without timestamps/actions. |
| Verified post-rescan reference | `155,719,671,808` | n/a | Incident reference recorded before the final package-removal action; concurrent volume activity makes cross-window attribution unsafe. |
| Package-removal action start | `140,613,001,216` | n/a | Exact action baseline. |
| Stable post-removal final | `142,191,984,640` | `+1,578,983,424` | Only proven reclaim in the final package-removal action window. |
| Whole-session exact baseline to stable final | `77,278,429,184` to `142,191,984,640` | `+64,913,555,456` | Proves approximately `60.454 GiB` net free-capacity recovery across the session, but not which individual operation caused it. |

The fall from the post-rescan reference to the package-action baseline must not be called consumed,
lost, or reclaimed without a matching timestamped allocation record. It proves the volume was changing
concurrently and that causal comparisons require action-bounded measurements.

</topic>

<topic id="attempt-history" status="closed-branches" version="v1" wp="WP-0285" updated_at="2026-07-29">

# What was tried and what each attempt proved

| Attempt | Result | Branch status |
|---|---|---|
| DSM Storage Manager / shared-folder / Drive / Resource Monitor inspection | Established volume-level target, current Recovery state, exposed recycle-bin state, Drive's stale allocation, and sampled open files. | Do not redo wholesale. Reopen only a view needed for a new exact evidence gap or before/after proof. |
| Exposed SMB recycle-bin inspection | No deleted target found; only `desktop.ini` appeared at the checked paths. | Exhausted unless a different exact DSM-owned bin is identified. |
| Snapshot Replication presence check and `#snapshot` exposure check | Package absent; no exposed snapshots. | Inconclusive, not negative proof. Continue with a real Btrfs snapshot/subvolume inventory. |
| Drive Admin `Empty Recycle Bins` operation and monitoring | No material exact-free-byte change was proven. | Exhausted; do not repeat. |
| Version-saving feature disabled | The feature that had retained deleted-file versions was turned off, but no noticeable free-capacity increase followed. | Completed. Do not merely toggle the feature again. Determine why the already-retained versions/blocks were not purged. |
| Recycle bins enabled and emptied several times | No noticeable free-capacity increase followed. | Exhausted; do not repeat without a newly identified exact non-empty bin. |
| Drive `synotifyd` one-shot full-view rescan | Queue reached `0`; indexing ended; Recovery stayed current; no target row and no material reclaim. | Exhausted; do not rerun. |
| Version Explorer inspection with newest/current time and deleted files shown | Live Recovery root was visible, not the deleted child. | Exhausted until a deleted child row or new Drive evidence appears. Never select the root as a substitute. |
| Continuous capacity/queue/CPU/I/O monitoring | Ran for hours without advancing exact free bytes or Drive allocation. | Stop. Recurring monitoring is prohibited unless a later approved action has an actual asynchronous reclaim phase. |
| Supported Synology Drive Server + Synology Office uninstall with package-data removal | Both packages removed; `1,578,983,424` bytes gained in the action window; expected terabytes remained missing. | Completed. This materially weakens Drive package data as the principal holder. |
| Live-root preservation checks | `Recovery`, `Projects`, and `Video` still existed after package removal. | Repeat only after an approved destructive action as a preservation gate. |
| DSM File Change Log inspection | Revealed `vv_quarantine/WP-0277/cleanup_artifacts` paths that had existed after the quarantine move. | Historical lead resolved by the later purge receipts and ledgers. Do not treat those paths as still live. |
| WP-0277 duplicate-quarantine purge | Signed receipt and ledger prove 1,826 files totaling `256,561,005,896` logical bytes were permanently deleted with zero ledger errors and zero remaining manifest paths. | Completed. Reconcile the logical total against exclusive physical extents and volume free space; do not assume a byte-for-byte reclaim or attempt another quarantine deletion. |
| WP-0277 cleanup-artifact-quarantine purge | Signed receipt and ledger prove 435 files totaling `123,181,662,082` logical bytes were permanently deleted with zero ledger errors and zero remaining manifest paths. | Completed. Reconcile the logical total against exclusive physical extents and volume free space; do not assume a byte-for-byte reclaim or attempt another quarantine deletion. |

# Commands already used

The supported one-shot Drive command reported by the `firefox_dsm_session` subagent and executed
through DSM Task Scheduler was:

```sh
sqlite3 --json /var/packages/SynologyDrive/etc/repo/user-db.sqlite 'select view_id from user_table' | jq .[].view_id | xargs -I {} /var/packages/SynologyDrive/target/bin/cloud-control synotifyd-rescan --view_id {} --path /
```

The status command was:

```sh
/var/packages/SynologyDrive/target/bin/cloud-control synotifyd-status | grep -E 'Queue:|Total events in queue files'
```

Recorded completion was `Total events in queue files: 0`. These commands must not be rerun merely
because a new agent lacks chat history.

</topic>

<topic id="current-diagnosis" status="evidence-based-plan-b-success" version="v1" wp="WP-0285" updated_at="2026-07-30">

# Evidence-based current diagnosis

## Proven conclusions

1. SMB-visible disappearance was not sufficient proof of physical reclamation.
2. The live `Recovery` root is not the deleted target and must not be deleted.
3. A version-saving feature had been active and was the already-identified reason deleted-file blocks
   were retained.
4. Turning that feature off did not release the already-retained space.
5. Enabling recycle bins and repeatedly deleting their contents did not produce a noticeable
   free-capacity increase.
6. The completed Drive rescan did not expose the historical deleted child in Version Explorer.
7. Removing Synology Drive Server and Synology Office package data reclaimed only
   `1,578,983,424` bytes in the measured action window. The Drive package layer therefore does not
   provide the missing approximately 2.4 TB remedy.
8. Across the entire session, exact free capacity increased by `64,913,555,456` bytes
   (`60.454 GiB`). Some reclamation therefore did occur; it was not enough to satisfy the target and
   cannot yet be attributed to one exact operation.
9. WP-0277 quarantine was initially a move-based recovery mechanism, but later separately recorded
   purge actions permanently deleted both quarantines.
10. Durable receipts and ledgers prove permanent deletion of `379,742,667,978` quarantine bytes:
    `256,561,005,896` bytes across 1,826 duplicates and `123,181,662,082` bytes across 435 cleanup
    artifacts. All 2,261 ledger rows are `deleted`, with zero error rows and zero remaining manifest
    paths.
11. Together with the separately recorded `.part` deletion of `212,862,604,241` bytes, the three
    recent cleanup categories total exactly `592,605,272,219` logical bytes. The quarantine is no
    longer a live path owner or a deletion target.
12. The `592,605,272,219`-byte logical total is not proof that the files exclusively owned the same
    number of physical Btrfs bytes. Snapshots, fast clones/reflinks, hardlinks, compression, sparse
    allocation, and open-unlinked handles can make the physical reclaim smaller or delay it. The
    physical shortfall remains unquantified until shared/exclusive ownership is measured.
13. Current DSM Usage Details attributes `26.7 TB` of the `55.8 TB` volume to Synology Drive, even
    though Synology Drive Server remains uninstalled.
14. The `26.7 TB` Drive allocation is large enough to explain the entire operator-reported missing
    capacity. It is the current leading and DSM-proven owner category.
15. DSM's Space Reclamation Schedule already runs 24/7. Insufficient schedule time is not the
    remaining blocker.
16. DSM Usage Details showed no Snapshot or LUN/VMM category. Those layers are no longer co-leading
    DSM-visible hypotheses, although SSH authentication blocked a conclusive raw Btrfs inventory.
17. One controlled restart caused Volume 1's rounded free-capacity display to stabilize at
    `10.5 TB`, versus `132.7 GB` immediately beforehand, while used space fell from `55.7 TB` to
    `45.3 TB`.
18. Across the same action window, Shared Folder remained `29 TB`, Synology Drive fell from
    `26.7 TB` to `16.3 TB`, and Others changed only from `3.9 GB` to `3.3 GB`. Synology Drive is
    therefore the proven released owner category.
19. The approximately `10.37 TB` displayed free-capacity increase and `10.4 TB` Drive-category
    decrease are rounded DSM figures, not exact-byte measurements.
20. The restart and DSM's own background cleanup restored substantially more capacity than the
    operator's approximately 2.4 TB target. The immediate disk-space crisis is resolved without a
    second deletion, reinstall, rescan, scrub, balance, or raw filesystem action.

## Established original cause and unresolved current blocker

The original retention cause was the activated Synology Drive version-saving feature. Plan A now
shows that the current retained owner category is also Synology Drive: `26.7 TB` remains allocated to
Drive after versioning was disabled, recycle bins were emptied repeatedly, the rescan completed, and
Synology Drive/Office were removed with package-data deletion selected.

Plan B proved that a large part of the blocker was a Drive-class allocation whose cleanup was stalled
or not completed until a full DSM restart. The released approximately `10.4 TB` was larger than the
recent `592,605,272,219` logical-byte cleanup total, so the restart also released older historical
Drive retention, not only the newest quarantine and `.part` deletions.

The immediate capacity blocker is resolved. A residual `16.3 TB` remains assigned to Synology Drive
while Drive Server is uninstalled. That remainder is still an unexplained owner category, but it is
not permission to delete or reinstall anything now that `10.5 TB` is available. Use Synology Support
only if the operator wants the residual Drive allocation identified or if free space begins falling
without matching live-file growth.

Raw Btrfs snapshots, deleted subvolumes, qgroups, clones/reflinks, and open-unlinked handles could
still be the lower-level mechanism inside that Drive allocation. They were not enumerated because
SSH accepted connections but no configured local key authenticated. That lower-level limitation
does not erase the supported DSM owner attribution.

# Retention/ownership matrix

| Layer | Current evidence | Can explain missing bytes? | Next proof | Destructive action now? |
|---|---|---|---|---|
| Synology Drive physical allocation | Pre-restart Usage Details attributed `26.7 TB` to Synology Drive while Drive Server was uninstalled. One restart reduced it to a stabilized `16.3 TB` and increased available capacity to `10.5 TB`; Shared Folder stayed `29 TB`. | `PROVEN RELEASED OWNER CATEGORY`; approximately `10.4 TB` of rounded Drive allocation was released. A residual `16.3 TB` remains. | No immediate action. Use Synology Support only if the operator wants the residual owner identified or unexplained capacity loss recurs. Raw Btrfs inventory remains useful only through a secure supported access path. | No. Do not repeat the restart, reinstall Drive, perform database surgery, or delete raw Btrfs objects. |
| Purged WP-0277 quarantines | Signed receipts and ledgers prove 2,261 files totaling `379,742,667,978` logical bytes were permanently deleted; zero remaining manifest paths and zero ledger errors. | They are not a current live path owner. Their exclusive physical allocation is unknown, so the receipt total cannot be assumed to equal reclaimable capacity. | Reconcile logical totals against targeted shared/exclusive extent evidence and volume-level capacity. | No. The purge is complete; do not delete the quarantine again. |
| Btrfs snapshots/subvolumes | Raw inventory blocked by SSH authentication; DSM Usage Details showed no Snapshot category. | `UNRESOLVED LOWER-LEVEL MECHANISM`, but materially weakened as a separate principal owner. | Inventory only after a secure SSH key/credential path or through Synology Support. | No. Exact snapshots + benefit + new approval required. |
| Btrfs clones/reflinks | Not inspected. | Possible; Synology notes file clones as a reason less space may release than expected. | Read-only shared-block/clone evidence tied to the known deleted categories. | No. |
| Synology Drive package data | Package removal itself gained only 1.579 GB. The later single restart reduced Drive allocation from `26.7 TB` to `16.3 TB`. | `PROVEN RESTART-RELEASED CATEGORY` with a residual unexplained `16.3 TB`. | Preserve the successful state. Support is the remaining supported attribution path if further explanation is required. | No reinstall, repeated restart, or DB surgery. |
| Exposed recycle bins | Checked paths contained no target. | Unlikely for the checked paths. | Revisit only with a newly identified exact DSM-owned bin. | No. |
| Open deleted handles | No relevant handle in sampled DSM view. | Possible historically but unsupported now. | One bounded NAS-native deleted-open-file check if available; no recurring process polling. | No. |
| Space reclamation schedule | DSM time grid is blue for every hour of every day: 24/7. A single restart then released approximately `10.4 TB` of rounded Drive allocation. | `PROVEN NOT A SCHEDULE-WINDOW BLOCKER`; the missing state transition was the restart, not more schedule time. | Do not repeat or toggle. Preserve the current schedule and recovered state. | No schedule churn. |
| LUN/Active Backup/other packages | Usage Details showed no LUN/VMM category and only `3.9 GB` Others. | `PROVEN NOT PRINCIPAL DSM-VISIBLE OWNER`. | Revisit only if Support or raw Btrfs evidence contradicts Usage Details. | No. |

</topic>

<topic id="subagent-and-research-handoff" status="reference" version="v1" wp="WP-0285" updated_at="2026-07-29">

# Prior subagent discoveries

## `firefox_dsm_session`

- Identified the supported one-shot Drive rescan command and bounded status command recorded in the
  attempt history.
- Warned that disabling/re-enabling the Team Folder removes all Drive history and breaks sync tasks.
- Warned that My Drive fallback would also require disabling User Home.
- That fallback was not approved and must not be treated as a routine next step.

## `drive_reclaim_research`

- Identified Version Explorer's supported flow: current/newest time, enable `Show deleted files`,
  select only the exact deleted row, then `More -> Delete permanently`.
- Recommended rescan when Drive and File Station are inconsistent.
- Found that if the root remains current after the queue reaches zero, undocumented Drive database
  manipulation should not be used.
- Official references:
  - https://kb.synology.com/en-uk/DSM/help/SynologyDrive/drive_admin_console?version=7
  - https://kb.synology.com/en-sg/DSM/tutorial/inconsistent_file_list_between_Drive_File_Station
- Community comparison case, non-authoritative:
  - https://community.synology.com/enu/forum/1/post/132659

## `synology_retention_api`

- Identified Package Center uninstall with deletion of package-owned items as the supported package
  cleanup path; plain uninstall may retain package data.
- Reported package-owned scope can include Synology Office files/settings, sharing settings,
  stars/labels, version history, and Drive recycle-bin files.
- Warned that Synology Office is a dependency/related package.
- Found an internal API but classified it as undocumented and unsuitable for use.
- Official references:
  - https://kb.synology.com/en-eu/DSM/tutorial/How_to_manage_storage_in_Drive
  - https://kb.synology.com/en-global/DSM/tutorial/I_cant_find_the_storage_usage_after_updating_to_DSM_7_what_should_I_do
  - https://kb.synology.com/en-eu/DSM/help/SynologyDrive/drive_backup_and_restoration
  - https://kb.synology.com/en-sg/DSM/tutorial/Drive_Client_tasks_auto_restored_after_reinstalling_package

## `vv_cleanup_evidence`

- The subagent was interrupted before it delivered a verified finding.
- Do not invent or attribute conclusions to it.

# Current official snapshot research

- Synology's Snapshot Replication documentation states that a snapshot points to data blocks and that
  deleting a file does not free blocks while snapshots still reference them.
- Synology's snapshot calculator can estimate blocks preserved by shared-folder snapshots for a
  selected time range.
- Synology warns that less space than expected after snapshot deletion can be caused by other
  snapshots, recycle bins, file clones, or ongoing space reclamation.
- Official references:
  - https://kb.synology.com/en-uk/DSM/tutorial/How_can_I_free_up_snapshot_space_consumption
  - https://kb.synology.com/en-us/DSM/tutorial/Quick_Start_Snapshot_Replication
  - https://kb.synology.com/en-me/DSM/help/SnapshotReplication/snapshots

</topic>

<topic id="assistant-failure-record" status="operator-review" version="v1" wp="WP-0285" updated_at="2026-07-29">

# Assistant failure and forgotten-constraint record

This section is not a substitute for reclamation. It exists so the next agent does not repeat the
same failures and so the operator can add/correct the record before execution resumes.

1. The assistant repeatedly performed broad discovery and unchanged-state checks after the same
   branches were already exhausted.
2. The assistant failed to preserve the distinction between the live `Recovery` container and the
   already-deleted child inside it.
3. The assistant proposed a Recovery-root version-history deletion without authorization. The
   proposal was retracted, and no Recovery-root delete was performed.
4. The assistant repeatedly reopened the Drive rescan, queue monitor, recycle-bin status, and
   Recovery-root Version Explorer branches instead of carrying forward their results.
5. Earlier reports treated CPU, I/O, `Running`, indexing, or worker activity as if they suggested
   progress. Those are diagnostics, not reclaimed bytes.
6. A recurring monitor remained active after it had become a no-op and caused repeated checks for
   hours.
7. The operator reported that monitoring launched terminal/process windows and stole foreground
   focus. That violated the quiet-background requirement.
8. Attempts to stop/update the stale automation failed twice with
   `No handler registered for tool: automation_update`; the failed control path was not resolved
   promptly.
9. The assistant did not preserve a durable evidence ledger early enough, causing later automatic
   continuations to repeat discovery and lose the current conclusion.
10. The assistant focused on Synology Drive despite the absence of measured capacity progress and
    reached the supported package-removal step only after prolonged repetition.
11. The package removal was validly scoped and measured, but its `1.579 GB` result disproved it as
    the main remedy. Continuing to present Drive activity as the likely terabyte-scale solution after
    that measurement would be false.
12. The assistant did not immediately follow the strongest new evidence: live same-volume
    `vv_quarantine/WP-0277/cleanup_artifacts` paths.
13. The assistant did not reconcile the contradiction between the committed WP-0277 proof ("no
    operator NAS mutation") and the later live quarantine paths before describing the cleanup
    history.
14. The earlier record that the entire Chrome workflow remained unfinished was too broad. The backup
    and intentional purge of recovery/active sessions were completed. A controlled single-window
    Chrome launch for Synology work and the later restore into both recovery and live state remain
    operator-directed steps and must not be represented as completed before they occur.
15. The assistant repeatedly forgot the already-identified original cause: version saving had been
    enabled and retained the deleted-file versions.
16. The assistant repeatedly forgot that version saving had already been turned off.
17. The assistant repeatedly forgot that recycle bins had been enabled and their contents had already
    been deleted several times without a noticeable capacity result.
18. Because those facts were forgotten, later work incorrectly returned to discovery and rescanning
    as though the retention cause and attempted setting/bin remedies had never been established.
19. The assistant over-weighted the `1.579 GB` package-removal action window and initially failed to
    credit the larger whole-session net increase from approximately `70 GB` displayed free to
    approximately `132.5 GB`. That increase proves some reclamation occurred even though its exact
    triggering operation remains unattributed.

# Required behavioral controls for the next agent

- Read this packet first and cite the Evidence ID that justifies each repeated inspection.
- If no new evidence would change the next action, do not repeat the inspection.
- Report `NO DIRECT PROGRESS` when exact free bytes and retaining allocation have not changed.
- Never substitute activity metrics for capacity.
- Preserve the operator's wording and later corrections.
- Keep terminal/DSM work quiet and bounded.
- Before any destructive click/command, present: exact target, why it owns the blocks, estimated
  reclaim, unrelated data in scope, reversibility, preservation checks, and exact before/after proof.
- Wait for a new exact approval at action time.

# Operator additions and corrections — pending

The operator requested space to add what the assistant kept forgetting and did wrong. Add those notes
below without rewriting or compressing the operator's wording:

1. Added 2026-07-29, preserved verbatim:

```text
deleting the drive server did not solve anything, correct?

i think you kept forgetting the cause of why the space did not get reclaimed even though you kept rescanning over and over.

there was a feature activated that saved versions, this prevented reclaiming the space that got deleted.
we did turn this feature off, we also turned on recycke bins and tried to delete its contents a few times. this had no noticeable effect

all these things you forgot
```

2. Added 2026-07-29, preserved verbatim:

```text
we started the session with arround 70gb free space now 132.5 so something did work. because there is no other app or model or project managing the NAS, it is rather more likely other models and project would add rather then free up space.
```

3. Added 2026-07-29, preserved verbatim:

```text
i think we deleted the querantine data, this added more unreclamed disk space.
```

4. Added 2026-07-29, preserved verbatim:

```text
because you can have access to my browser through chrome, but i had multiple windows and thousands of tabs open, we did create a back up. we purged the chrome recovery and active sessions so we can launch a single chrome windows to work in and have access to synology software. we will later reload the back up into the recovery state and also in the live state.
```

5. Added 2026-07-29, preserved verbatim:

```text
while making the back up i asked to clean up and remove duplicate youtube videos
```

Context correction: in the Chrome backup workflow, “duplicate youtube videos” means duplicate
YouTube browser tabs in the cleaned recovery copy. It does not mean NAS media-file deletion.

6. Added 2026-07-29, preserved verbatim:

```text
ok now you have the complete picture, the goal and focus is my disk space reclaimation. research what is going wrong and what we can do about it. you can use sub agents and come up with different plans after researchihng it online through different lenses. then we will append the wp once more for state recovery purpose
```

</topic>

<topic id="research-basis-2026-07-29" status="current" version="v1" wp="WP-0285" ingestable="true" updated_at="2026-07-29">

# Current research basis — 2026-07-29

Three independent read-only research lenses were used:

- `synology_official`: current Synology-supported diagnosis, DSM surfaces, and remediation boundary;
- `btrfs_internals`: upstream Btrfs extent-sharing, subvolume cleanup, qgroup, open-file, and allocation
  semantics;
- `remediation_lenses`: ranked DSM-only, bounded-SSH, vendor-escalation, and recovery plans.

No NAS, browser, package, Chrome session, or repository state other than this WP was changed during
the research.

## Main finding

The incident is now two questions, not one:

1. How many physical bytes were exclusively owned by the deleted files and should therefore become
   reusable?
2. Which current reference, open handle, pending cleaner operation, or hidden allocation prevents
   those exclusive bytes from being reusable now?

The purge receipts prove that `592,605,272,219` logical bytes were permanently removed across the
three recent cleanup categories. They do not prove an equal physical shortfall on Btrfs. A snapshot,
Fast Clone/reflink, hardlink, compressed or sparse extent, or open-unlinked file can make logical
bytes and exclusive physical bytes differ. The older approximately 2 TB child deletion is also an
estimate rather than an exact exclusive-extent measurement.

This correction does not dismiss the incident. The volume remained approximately 99.8% used, the
original version-saving retention was real, and only `64,913,555,456` net free bytes returned across
the whole session. It changes the proof requirement: determine exclusive physical ownership before
calling the entire logical total physically missing.

## Evidence-based hypothesis ranking

| Rank | Candidate | Current assessment | Discriminating proof |
|---:|---|---|---|
| 1 | Snapshot/subvolume or Fast Clone/reflink references | Highest-value unresolved branch. Synology explicitly lists snapshots and clones for this symptom; prior package/`#snapshot` checks were inconclusive. | DSM Snapshot Usage Details plus Btrfs subvolume/snapshot inventory; targeted shared/exclusive evidence. |
| 2 | Pending or stalled Btrfs background deletion/reclamation | Strong fit for the slow partial return. Synology documents asynchronous reclamation and recommends restart after prolonged non-release. | Reclamation schedule/state, deleted-subvolume inventory, Btrfs usage, then one approved restart with exact before/after bytes. |
| 3 | Service holding deleted files open | Plausible; one DSM sample did not conclusively exclude it. | One bounded `lsof +L1`-class check, subject to installed-tool compatibility; otherwise the supported controlled restart test. |
| 4 | LUN/VMM, Active Backup, package, or `Others` allocation | Unresolved fallback owner. | Storage Manager Usage Details and exact DSM owner view. |
| 5 | Qgroup/reporting or filesystem accounting defect | Possible if normal owners do not reconcile. A stale qgroup can corrupt estimates but does not normally hold extents by itself. | Existing quota/qgroup state, Btrfs versus `df` versus DSM comparison, logs, Synology Support. |
| 6 | Data/metadata chunk imbalance | Lower priority. Allocated-minus-used chunks are normally reusable and not automatically lost space. | Btrfs filesystem/device usage only after references are assessed. |

The following are not current remedies: another Drive rescan, another generic recycle-bin purge,
Drive reinstall, repeated passive monitoring, TRIM/discard, data scrubbing, defragmentation, generic
balance, quota enable/rescan, free-space-cache clearing, direct subvolume deletion, or
`btrfs check --repair`.

## Primary sources checked

- Synology, “Storage space is not freed up after deleting files or data. What should I do?”:
  https://kb.synology.com/en-ph/DSM/tutorial/Storage_space_does_not_increase_after_deleting_files
- Synology, “Set a Space Reclamation Schedule”:
  https://kb.synology.com/en-global/DSM/help/DSM/StorageManager/storage_pool_space_reclamation
- Synology, “Storage Manager shows my storage usage is higher than expected”:
  https://kb.synology.com/en-uk/DSM/tutorial/How_do_I_check_storage_usage
- Synology, “How to manage storage in Synology Drive”:
  https://kb.synology.com/en-eu/DSM/tutorial/How_to_manage_storage_in_Drive
- Synology, snapshot space consumption:
  https://kb.synology.com/en-ca/DSM/tutorial/How_can_I_free_up_snapshot_space_consumption
- Synology, Snapshot Replication snapshot management:
  https://kb.synology.com/en-us/DSM/help/SnapshotReplication/snapshots?version=7
- Synology, File Fast Clone:
  https://kb.synology.com/en-global/PAS/help/PAS/AdminCenter/file_service_advanced_introduction?version=1_0
- Btrfs, filesystem usage and targeted `filesystem du` semantics:
  https://btrfs.readthedocs.io/en/stable/btrfs-filesystem.html
- Btrfs, subvolume and deleted-subvolume semantics:
  https://btrfs.readthedocs.io/en/latest/btrfs-subvolume.html
- Btrfs, qgroup referenced/exclusive semantics:
  https://btrfs.readthedocs.io/en/latest/Qgroups.html
- Btrfs, reflink semantics:
  https://btrfs.readthedocs.io/en/stable/Reflink.html
- Linux `unlink(2)` and `lsof(8)` open-deleted-file semantics:
  https://man7.org/linux/man-pages/man2/unlink.2.html
  and https://man7.org/linux/man-pages/man8/lsof.8.html

</topic>

<topic id="ranked-reclamation-plans-2026-07-29" status="pending-operator-selection" version="v1" wp="WP-0285" ingestable="true" updated_at="2026-07-29">

# Ranked reclamation plans — operator decision required

## Plan A — bounded read-only ownership proof — recommended first

Why first: it has the highest information value, deletes nothing, and can distinguish extant
snapshots, deleted subvolumes still awaiting cleanup, existing qgroup accounting, Btrfs allocation,
and open-unlinked files without repeating the exhausted Drive workflow.

Approval boundary:

- one bounded DSM and SSH evidence session;
- separate approval if SSH must be enabled;
- no broad `du`, no package installation, no sync/flush, no rescan, no service stop, and no NAS
  restart;
- capture output locally rather than writing evidence onto the critically full NAS;
- verify the actual volume mount instead of assuming `/volume1`;
- check each installed command's help/version before using optional flags; an unavailable command or
  flag is recorded as unavailable and is not replaced by installing tools.

DSM evidence:

1. Capture one exact Storage Manager baseline: total, used, free bytes, filesystem, pool/volume
   health, running storage operations, and warnings.
2. Capture Usage Details categories: Shared Folder, LUN/VMM, Synology Drive, Snapshot, and Others.
   If Usage Details is disabled, stop for approval before enabling it because Synology documents a
   temporary performance cost.
3. Capture the current Space Reclamation Schedule and task state once. Do not toggle it or recur.

Proposed SSH baseline, with compatibility checks and the verified mount substituted for
`<volume-mount>`:

```sh
date -Iseconds
uname -a
cat /etc.defaults/VERSION
btrfs --version
findmnt -T <volume-mount> -o TARGET,SOURCE,FSTYPE,OPTIONS
df -B1 <volume-mount>
btrfs filesystem usage -b <volume-mount>
btrfs filesystem df -b <volume-mount>
btrfs device usage -b <volume-mount>
btrfs filesystem show --raw <volume-mount>
btrfs subvolume list -a -p -u -q <volume-mount>
btrfs subvolume list -s <volume-mount>
btrfs subvolume list -d <volume-mount>
btrfs quota status <volume-mount>
btrfs qgroup show --raw -pcre <volume-mount>
btrfs balance status <volume-mount>
btrfs scrub status <volume-mount>
btrfs device stats <volume-mount>
lsof +aL1 <volume-mount>
```

Interpretation gates:

- snapshot rows pre-dating deletion establish a reference candidate, not an exact reclaim estimate;
- qgroup `referenced` values must not be summed; use `exclusive` only when the qgroup state is
  consistent and still confirm with Synology's snapshot calculator;
- `subvolume list -d` or `<under deletion>` means stop experiments and take the exact root ID to
  Synology Support;
- a material open-unlinked file identifies a service-holder branch;
- allocated-minus-used Btrfs chunks are not automatically missing space;
- no snapshots, deleted subvolumes, qgroups, or open-unlinked files shifts the case toward clones,
  LUN/VMM, package `Others`, or a filesystem-accounting defect.

Do not add `--sync`, `-z`, quota enable/rescan, `filesystem sync`, `subvolume sync`, balance,
defragment, scrub, repair, cache clearing, delete, or raw reclaim commands to this baseline.

## Plan B — supported controlled restart test

Why: Synology's current exact-symptom article explicitly recommends restarting after prolonged
non-release. Restart terminates services holding deleted files and restarts background work. It is a
better-supported test than killing processes or running raw Btrfs maintenance.

Prerequisites and gates:

1. Separate operator approval for a service-disruptive restart.
2. Confirm pool and volume are Healthy and no repair, expansion, package update, backup, or other
   critical operation is active.
3. Preserve the live `Recovery` container and irreplaceable VV subscriptions, playlists, library
   metadata, and paused queue.
4. Capture an exact timestamped free-byte baseline immediately before restart.
5. Restart once in a maintenance window; capture one stabilized post-start exact-byte value and
   Usage Details state. Do not create another recurring monitor.

Decision:

- material increase: an open handle or stalled background service was involved; attribute the exact
  action-window gain and do not repeat the restart;
- no material increase: persistent references/allocation remain; move to Plan A evidence or Plan D
  Support, not another restart.

## Plan C — snapshot-specific supported cleanup

Enter only if Plan A or DSM Usage Details proves material Snapshot allocation.

1. Use the compatible official Snapshot Replication package only after separate install approval, or
   ask Synology Support to inventory existing roots.
2. Record exact shared folder, snapshot/root ID, timestamps, count, oldest/newest, retention policy,
   lock/immutable state, replication state, and whether each snapshot predates the deletion.
3. Use Synology's snapshot calculator over at least seven days to estimate preserved blocks.
4. Present a new target-specific deletion proposal. Do not delete during diagnosis.
5. Delete only separately approved snapshot IDs through DSM. Reclaim may be asynchronous and smaller
   than the calculator estimate if other snapshots or clones still share the extents.

Snapshot deletion is irreversible loss of a recovery point and can cover unrelated files in the same
shared-folder snapshot. Package absence and hidden `#snapshot` exposure remain invalid negative
proofs.

## Plan D — Synology Support and capacity-recovery branch

Use immediately if Plan A shows a deleted root, inconsistent accounting, transaction/Btrfs errors,
or no supported owner; also use if the one approved restart produces no release.

Ticket bundle:

- NAS model, DSM build, exact pool/volume state, filesystem, and Usage Details;
- exact historical and current free-byte readings;
- signed logical-deletion receipts totaling `592,605,272,219` bytes, explicitly labelled logical;
- the approximate 2 TB deleted child inside the live `Recovery` container, with `Recovery` explicitly
  protected;
- history of versioning disabled, bins emptied, rescan completed, and Drive/Office removed;
- the `1,578,983,424`-byte package-removal action gain and `64,913,555,456`-byte whole-session net
  gain;
- all Plan A outputs, exact root/qgroup IDs, and screenshots;
- a request to identify the exact root, qgroup, open orphan, clone, package, LUN, or accounting owner
  and provide a supported cleanup with expected bytes and rollback.

Enable remote support only after explicit operator approval and a specific Synology request. Review
every proposed root command. Do not accept a generic Drive reinstall/rescan, `rm`, scrub, full
balance, filesystem repair, or volume rebuild without an exact diagnosed owner and recovery plan.

If the critically full volume prevents safe diagnosis or repair, a separately researched capacity
expansion or copy-first migration to another volume may provide breathing room. Volume
removal/recreation is the final destructive branch and requires verified independent backups,
copy/checksum/application verification, exact live-data preservation, and explicit operator
approval.

## Recommended operator choice

Choose Plan A first. Choose Plan B first only if the operator prefers Synology's simplest supported
test and accepts one maintenance-window restart before SSH evidence. Plans C and D are conditional on
what A/B prove.

</topic>

<topic id="plan-a-execution-2026-07-30" status="dsm-complete-ssh-auth-blocked" version="v1" wp="WP-0285" updated_at="2026-07-30">

# Plan A execution record — 2026-07-30

## Authorization carried forward

The operator selected Plan A, approved opening the required Chrome recovery window, approved
temporarily enabling SSH, and requested that the results be appended to this WP. No deletion,
restart, package install, rescan, folder-size scan, schedule change, scrub, balance, or raw Btrfs
command was authorized or performed.

## Completed preflight

1. The Chrome-control skill was read and followed.
2. Chrome was verified running.
3. The selected Chrome profile has the ChatGPT Chrome Extension installed and enabled.
4. The native messaging host manifest and registry integration validated.
5. Browser-control attachment timed out twice before tab discovery or DSM interaction.
6. After operator approval, one Chrome window was opened for the selected profile and the control
   channel recovered successfully.
7. `MIR` resolved to `192.168.0.253`.
8. TCP port 22 was initially actively refused.

## DSM ownership results

1. Model: `DS1825+`; version: `DSM 7.3.2-86009 Update 4`.
2. Storage Pool 1: `JBOD (Without data protection)`, `58.2 TB`, fully allocated, four drives Healthy.
3. Volume 1: Btrfs, `55.8 TB` total, `55.7 TB` used, `99%`, `132.7 GB` available.
4. Usage Details:
   - Shared Folder: `29 TB`
   - Synology Drive: `26.7 TB`
   - Others: `3.9 GB`
   - Available: `132.7 GB`
   - no Snapshot category
   - no LUN/VMM category
5. Package Center shows Synology Drive Server as uninstalled (`Install`) while Usage Details assigns
   `26.7 TB` of physical disk use to Synology Drive.
6. Space Reclamation Schedule is already blue for all 168 weekly hours: 24/7.
7. Data Scrubbing was Ready and had never been performed; it was not started.

## SSH lifecycle and evidence limit

1. SSH was enabled on port 22 only after approval; DSM confirmed `Changes applied`.
2. TCP/22 accepted connections and host fingerprints were recorded.
3. Non-interactive authentication as `iljasmets` failed with
   `Permission denied (publickey,password)` before any NAS command ran.
4. No saved password, cookie, local storage, Chrome session file, or credential store was inspected.
5. SSH was disabled again in DSM; DSM confirmed `Changes applied`.
6. The post-disable port check was actively refused, restoring the original closed state.
7. Because no key authenticated, the proposed Btrfs filesystem/subvolume/qgroup/open-file commands
   were not executed. This remains a lower-level evidence limitation, not a reason to disregard the
   supported DSM ownership result.

## Direct progress status

Plan A completed the supported DSM attribution stage and proved the dominant current owner category:
`26.7 TB` remains assigned to Synology Drive after Drive Server was uninstalled. This is enough to
move snapshots, LUN/VMM, generic Others, live quarantine, and reclamation-window insufficiency out of
the leading position.

Plan A did not obtain raw Btrfs object IDs because SSH authentication lacked an authorized key.
The next supported state-transition choice is Plan B's one controlled restart, separately approved,
because Synology recommends it after prolonged non-release to terminate service-held deleted objects
and restart background cleanup. If the Drive allocation remains after restart, move directly to Plan
D Synology Support with the `26.7 TB` orphaned-Drive evidence. Do not reinstall Drive or attempt raw
filesystem cleanup as a trial.

</topic>

<topic id="plan-b-execution-2026-07-30" status="complete-material-reclaim" version="v1" wp="WP-0285" updated_at="2026-07-30">

# Plan B execution — one controlled restart

## Approved closure unit

Perform one DSM restart from a verified idle storage state, allow DSM's own post-start cleanup to
finish, capture one stabilized Storage Manager and Usage Details result, and do not combine the
action window with any other reclamation operation.

## Pre-restart gate

1. At `2026-07-30T01:39:49.8591202+02:00`, Volume 1 showed `55.7 TB` used and
   `132.7 GB` available.
2. Storage Pool 1 remained `58.2 TB` JBOD without data protection.
3. All four drives showed Healthy.
4. Task Schedule showed no scheduled tasks.
5. Data Scrubbing was Ready and “Never performed yet.”
6. No repair, expansion, package update, backup, scrub, rescan, or other critical storage operation
   was active or started.

## Restart receipt

1. DSM Restart was confirmed once at `2026-07-30T01:40:26.121+02:00`.
2. DSM displayed “Synology NAS is restarting.”
3. Ports 443 and 5001 were both observed closed at approximately `01:42:07+02:00`.
4. Both ports were open again at `2026-07-30T01:42:38.890+02:00`.
5. DSM returned to its sign-in surface and the operator completed normal authentication.
6. No second restart was performed.

## Stabilized result

After startup, DSM showed a continuous monotonic release. The rounded Volume 1 used/free pair
eventually remained unchanged across consecutive checks, after which a newly opened Usage Details
dialog reported:

| DSM category | Before restart | Stabilized after restart | Rounded change |
|---|---:|---:|---:|
| Shared Folder | `29 TB` | `29 TB` | no displayed change |
| Synology Drive | `26.7 TB` | `16.3 TB` | `-10.4 TB` |
| Others | `3.9 GB` | `3.3 GB` | `-0.6 GB` |
| Available capacity | `132.7 GB` | `10.5 TB` | approximately `+10.37 TB` |
| Volume used | `55.7 TB` | `45.3 TB` | `-10.4 TB` |
| Volume total | `55.8 TB` | `55.8 TB` | no change |

All values in this table are DSM's rounded display values. The Windows exact-byte UNC probe timed
out, so this action window does not have an exact-byte final measurement.

## Verdict

Plan B succeeded materially. Synology Drive is the proven released owner category because its
allocation fell by the same rounded magnitude that available capacity rose, while Shared Folder
remained unchanged.

The original version-saving feature remains the proven retention cause. The restart proves that
historical Drive-class cleanup was stalled or incomplete until the service/filesystem state
transition. The lower-level mechanism—Drive versions, deleted subvolumes, shared Btrfs references,
or a stalled service—is still unresolved because raw Btrfs inventory was not available.

The immediate capacity target is exceeded: `10.5 TB` is available after stabilization. Do not repeat
the restart, reinstall Drive, rerun the rescan, toggle the reclamation schedule, scrub, balance, or
delete another target as a trial.

## Residual state and preservation boundary

1. `16.3 TB` remains attributed to Synology Drive while Drive Server is uninstalled.
2. This residual category is not a deletion authorization. Synology Support is the supported next
   attribution path only if the operator wants the remainder explained or unexplained capacity loss
   recurs.
3. All four drives remained Healthy; the Storage Manager Warning text was only the volume
   low-capacity threshold warning.
4. No file, folder, snapshot, package, schedule, scrub, balance, repair, or rescan action occurred
   during Plan B.
5. Shared Folder remained `29 TB`, but DSM authentication expired immediately after the final
   measurement. The exact `Recovery`, `Projects`, and `Video` File Station spot check therefore
   remains not post-verified and must not be represented as completed.
6. Chrome recovery/session backup artifacts were not opened, moved, restored, or modified.

</topic>

<topic id="next-agent-execution-contract" status="pending-operator-review" version="v1" wp="WP-0285" updated_at="2026-07-29">

# Next-agent execution contract

## Startup

1. Run `vvstart.cmd` and follow the repo acknowledgment contract.
2. Read this entire WP, `WP-0277_RECOVERABLE_NAS_DUPLICATE_CLEANUP_v1.md`,
   `WP-0277_RECOVERABLE_NAS_DUPLICATE_CLEANUP_v1_REFINEMENT.md`, and
   `product/desktop/build_target/tool_artifacts/wp_runs/WP-0277/summary.md`.
3. Treat the verbatim relay as historical source material and the later operator corrections as the
   current authority.
4. Confirm the operator has finished adding notes before resuming NAS action.
5. Treat E-0285-026 as the Chrome-state contract: do not alter Chrome until the operator directs the
   controlled single-window launch, and do not perform the later recovery/live restoration until that
   separate step is explicitly requested.
6. Use E-0285-027 as the backup artifact inventory. Preserve the original session material and the
   cleaned 6,956-tab recovery image; do not choose or overwrite a restore source without an explicit
   restoration instruction and a read-only preflight.

## Direct work sequence

### Step 1 — Carry forward the proven quarantine purge

- Use E-0285-023 through E-0285-025 as the canonical purge evidence.
- Treat 1,826 duplicate files (`256,561,005,896` bytes) and 435 cleanup artifacts
  (`123,181,662,082` bytes) as permanently deleted, with zero remaining manifest paths.
- Treat the combined `379,742,667,978` bytes as verified logical deletion, not as an equal guaranteed
  physical reclaim.
- Together with `.part` deletions, retain the exact recent-cleanup expectation of
  `592,605,272,219` logical bytes and measure exclusive/shared physical ownership before calculating
  the physical shortfall.
- Do not search for a live quarantine to delete, do not repeat the purge, and do not use the empty
  schema-v28 cleanup tables as evidence that the manual purge did not occur.

### Step 2 — Execute approved Plan A evidence capture

- Map `\\MIR\home\Recovery` to its DSM shared folder (`homes` plus the operator's home path) and map
  the VV media root to its shared folder.
- Use an authenticated, read-only DSM/SSH/Task-Scheduler path that does not steal focus.
- Follow `ranked-reclamation-plans-2026-07-29`. If Snapshot Replication remains absent, do not treat
  absence as proof and do not install it without approval. Use the bounded read-only Btrfs inventory
  or ask Synology Support for an official inventory.
- For every relevant shared folder/subvolume, record:
  - exact identity/path,
  - snapshot/subvolume count,
  - oldest/newest dates,
  - retention policy,
  - locked/immutable state,
  - replication state,
  - whether it predates the deletion,
  - DSM snapshot-calculator estimate when available.
- Preserve all snapshots until the operator approves exact snapshot IDs.
- If no snapshot exists, record the exact command/UI evidence rather than the earlier package-presence
  proxy.

### Step 3 — Attribute the remaining allocation

- Produce one canonical table covering the purged quarantine category, snapshots, clones/reflinks, recycle bins,
  open-deleted files, package data, Active Backup, LUN/iSCSI, and other DSM allocations.
- Each row must say `PROVEN OWNER`, `PROVEN NOT OWNER`, or `UNRESOLVED`, with evidence IDs and bytes.
- Do not run a broad folder-size scan.

### Step 4 — Present one exact action proposal

Before any deletion/removal, report:

- exact target IDs and paths;
- live versus deleted state;
- total logical bytes and estimated physical reclaim;
- whether unrelated files/history are included;
- lock/immutable/replication state;
- rollback/recovery implications;
- exact baseline free bytes;
- post-action preservation checks;
- expected stabilization/reclamation behavior.

Wait for the operator's new exact approval.

### Step 5 — Execute only the approved target and verify

- Reconfirm the target immediately before execution.
- Record timestamp, DSM/log receipt, and action result.
- Record exact free bytes after the action and after stabilization.
- Verify `Recovery`, `Projects`, `Video`, subscriptions, playlists, library metadata, and paused queue
  remain intact.
- Stop if the action releases materially less than its estimate; do not compensate by deleting
  adjacent data.

## Anti-loop gate

The completed Drive rescan, Drive queue monitoring, generic Storage Manager refreshes, recycle-bin
inspection, and current-Recovery Version Explorer check may be repeated only if a named new artifact
or state transition makes the prior result stale. A new agent's lack of memory is not such a state
transition.

</topic>

<topic id="status-updates" status="in-progress" version="v1" wp="WP-0285" updated_at="2026-07-29">

# Status updates

- 2026-07-29: WP created for operator inspection and additions. The previous assistant's relay is
  preserved verbatim, later operator corrections are separated, prior actions and exact capacity
  measurements are recorded, repeated branches are closed, and the next direct proof path starts
  with live WP-0277 quarantine provenance plus a conclusive Btrfs snapshot/reference inventory.
- 2026-07-29: No NAS action, file deletion, package change, rescan, recurring monitor, or free-space
  check was performed while authoring this packet.
- 2026-07-29: Operator review correction added: version saving was the already-identified original
  retention cause; it was turned off; recycle bins were enabled and emptied several times without
  noticeable capacity recovery; deleting Drive Server later also failed as the remedy. The operator's
  correction is preserved verbatim in the additions section.
- 2026-07-29: Operator corrected the capacity interpretation. Exact session records show free capacity
  increased by `64,913,555,456` bytes (`60.454 GiB`) from the original baseline to the stable final
  reading. The WP now credits real whole-session reclamation while keeping its exact triggering
  operation unattributed.
- 2026-07-29: Operator recalled that the quarantine data had been deleted. SHA-256-verified purge
  receipts and read-only ledger checks confirmed permanent deletion of 1,826 duplicates
  (`256,561,005,896` bytes) and 435 cleanup artifacts (`123,181,662,082` bytes), for a combined
  `379,742,667,978` bytes with zero ledger errors and zero remaining manifest paths. The prior
  live-quarantine diagnosis and deletion-proposal branch are closed; these bytes now count as
  deleted-but-unreclaimed volume expectation.
- 2026-07-29: Operator corrected the Chrome workflow state. The multi-window/thousands-of-tabs
  session backup and intentional recovery/active-session purge are complete. A controlled
  single-window Chrome launch for Synology access and the later restore into both recovery and live
  state remain separate operator-directed steps. No Chrome action was performed while recording this
  correction.
- 2026-07-29: Local filesystem verification found the exact Chrome backup folder and artifacts.
  Post-close evidence records 26 windows and 8,152 tabs; 1,196 duplicate YouTube tabs across 830
  groups; and a cleaned recovery image corresponding to 26 windows and 6,956 tabs. Original session
  material remains present separately. No Chrome session file was opened in Chrome, moved, restored,
  or modified during verification.
- 2026-07-29: Three read-only research lenses reviewed current Synology-supported remediation,
  upstream Btrfs accounting, and alternative recovery plans. The WP now corrects logical deletion
  totals versus exclusive physical reclaim, records the exact-symptom Synology restart guidance,
  ranks the remaining hypotheses, and preserves four gated plans. Plan A, a bounded read-only
  DSM/SSH ownership capture, is recommended first. No NAS, browser, package, Chrome, rescan,
  monitoring, restart, or destructive action was performed.
- 2026-07-30: The operator selected Plan A. Bounded preflight verified that Chrome, the enabled
  ChatGPT Chrome Extension, and its native-host integration are installed, but browser-control
  attachment timed out twice before DSM interaction. `MIR` resolved to `192.168.0.253`, and TCP/22
  was initially actively refused. After the operator approved both recovery actions, Chrome control
  connected and DSM Usage Details proved that `26.7 TB` remains assigned to Synology Drive while
  Drive Server is uninstalled. Shared Folder was `29 TB`, Others `3.9 GB`, available capacity
  `132.7 GB`, and no Snapshot or LUN/VMM category appeared. Reclamation is already scheduled 24/7.
  Temporary SSH accepted connections but no local key authenticated, so no NAS command ran; SSH was
  disabled again and TCP/22 returned to actively refused. Plan A's DSM attribution is complete; its
  lower-level Btrfs inventory remains unavailable without a secure authentication path. No data,
  snapshot, package, schedule, or recovery state was deleted or changed.
- 2026-07-30: The operator selected Plan B and approved one controlled DSM restart. The NAS was
  observed fully down and back up. DSM then released space monotonically until Volume 1 stabilized
  at `45.3 TB` used and `10.5 TB` free, versus `55.7 TB` used and `132.7 GB` free beforehand.
  Stabilized Usage Details changed Synology Drive from `26.7 TB` to `16.3 TB`, while Shared Folder
  remained `29 TB`; this proves Synology Drive as the released owner category. All four drives
  remained Healthy, and no second restart, rescan, package action, scrub, balance, repair, schedule
  change, or file deletion occurred. The exact-byte UNC probe timed out, so the approximately
  `10.37 TB` displayed gain is explicitly rounded. DSM authentication expired before the named
  `Recovery`/`Projects`/`Video` File Station spot check, which remains not post-verified.

</topic>
