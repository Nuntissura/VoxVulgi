---
file_id: WP-0306-v1
file_kind: work-packet
updated_at: 2026-08-14
---

<topic id="contract" status="done" version="v1" wp="WP-0306" updated_at="2026-08-14">

# Work Packet: WP-0306 — MKV-only new video outputs, embedded tracks, and direct-NAS root rebind

## Metadata

- ID: WP-0306
- Owner: agent-wp0306
- Status: DONE
- Created: 2026-08-09
- Refinement: `WP-0306_MKV_ONLY_NEW_VIDEO_OUTPUTS_EMBEDDED_TRACKS_AND_DIRECT_NAS_ROOT_REBIND_v1_REFINEMENT.md`
- Board: `../TASK_BOARD.md#wp-0306`
- Dependencies: WP-0289, WP-0298, WP-0299

## Intent

Make MKV with embedded selected tracks the non-bypassable final container for every new managed video while preserving all historical MP4 behavior, and safely rebind new archive writes to the operator's directly attached NAS only after the exact target is proven live and identical.

## Base scope

- Implement the complete refinement: engine container policy, preset/queued-argument sanitization, embedded subtitles, direct-HTTP remux, output probing, UI/docs/default migration, historical-MP4 compatibility, root alias/resolver, dry-run/backup/receipt, and guarded active-root/override rebind.
- Preserve every existing media file, library row, subscription, membership, job, output, source identity, and operator setting except the explicitly proven active-root/descendant override mutation.
- Do not perform the deferred historical MP4 cleaning/conversion pass.

## Required implementation order

1. Central MKV execution boundary and command tests.
2. Embedded subtitles and authoritative output probe.
3. Direct-HTTP/local managed remux path.
4. Defaults/UI/docs/export selection and historical-MP4 compatibility tests.
5. Root alias/rebind dry-run, backup, resolver, receipts, and failure recovery.
6. Fresh exact-target proof, then and only then machine-local rebind execution.
7. Adversarial review, runtime/headless proof, governed build, and status synchronization.

## Acceptance and proof

- The refinement is the normative implementation, ROI, red-team, risk, hardening, and verification contract.
- MKV enforcement and historical MP4 compatibility must not be held back solely because the machine-local `Z:` proof gate is false.
- The rebind portion must remain explicitly blocked—not guessed or proxied—until the exact target/path identity gate passes.
- No status may be `DONE` until every new-output path, embedded-track probe, legacy-MP4 compatibility path, root-rebind safety path, adversarial review, UI/app-boundary proof, and build proof required by the refinement passes.

</topic>

<topic id="status-updates" status="active" version="v1" wp="WP-0306" updated_at="2026-08-09">

# Status updates

- 2026-08-09: Created after source/spec/config/SQLite inspection plus current official yt-dlp and FFmpeg documentation review. The exact intended `Z:` target was not visible, so no machine-local root or database state was changed. No historical media was modified.
- 2026-08-09: Implementable MKV/legacy-MP4 scope started in the first dependency-safe parallel wave. Direct-NAS mutation remains proof-gated because the exact `Z:` target is not visible; independent adversarial review is required.
- 2026-08-14: DONE. Engine-forced MKV/embedded-track and historical-MP4 compatibility gates passed; the exact `Z:` target and prior UNC root freshly proved the same file ID; guarded receipt `root-rebind-163d2da1-968d-47e3-a695-baa42fd94240` applied 7,840 exact mutations with verified backups while preserving 143,756 historical paths; packaged v0.1.137 Diagnostics/Media Library and status-command proof passed; independent adversarial review passed. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0306/20260814_final/summary.md`.

</topic>
