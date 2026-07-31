---
file_id: WP-0280-v1
file_kind: work-packet
updated_at: 2026-07-27
---

<topic id="contract" status="done" version="v1" wp="WP-0280" updated_at="2026-07-27">

# Work Packet: WP-0280 — Video Archiver and Jobs cohesive workspaces

## Metadata

- ID: WP-0280
- Owner: Codex
- Status: DONE
- Created: 2026-07-27
- Refinement: `WP-0280_VIDEO_ARCHIVER_JOBS_COHESIVE_WORKSPACES_v1_REFINEMENT.md`
- Board: `../TASK_BOARD.md#wp-0280`
- Carries forward unresolved scope from: WP-0255 and WP-0258
- Preserves shipped baseline from: WP-0256, WP-0271, WP-0274, WP-0278, and WP-0279

## Intent

Turn Video Archiver and Jobs/Queue into two cohesive, bounded, responsive workspaces whose primary controls and current state remain immediately visible even with hundreds of subscriptions, very large playlists, and large job history.

## Base scope

- Implement every phase and acceptance criterion in the refinement.
- Preserve canonical job, subscription, source-membership, imported/current unification, dedupe, retry-lineage, and media-library behavior.
- Use existing no-card layout, canonical projections, diagnostics, headless audit/action bridge, and external watcher.
- Update this packet and the taskboard after each implemented phase.

## Required implementation order

1. Video Archiver hierarchy and bounded selected-source detail.
2. Jobs command hierarchy, canonical source filter, and scheduler-health disclosure.
3. Jobs panel-local scrolling, bounded batch expansion, and responsive row context.
4. Shared shell height/responsiveness and any evidence-required Diagnostics/`vvwatch` extension.
5. Governed build and complete app-boundary proof.

## Acceptance and proof

- The refinement acceptance criteria are the contract.
- Each phase requires focused automated checks plus a headless semantic inventory/action/snapshot/dump on the changed packaged surface.
- The final build requires concurrent internal Diagnostics and `vvwatch` evidence.
- The work packet is not `DONE` until `governance/workflow/PROOF_STANDARD.md` is satisfied.

</topic>

<topic id="status-updates" status="active" version="v1" wp="WP-0280" updated_at="2026-07-27">

# Status updates

- 2026-07-27: Created from the complete v0.1.116 Video Archiver, Jobs/Queue, Diagnostics, and `vvwatch` audit. Updated the product specification and technical design before finalizing this contract.
- 2026-07-27: WP-0255 and WP-0258 unresolved scope was preserved and consolidated here; their shipped behavior and proof remain historical inputs, while WP-0256 remains the required Jobs baseline.
- 2026-07-27: Phase A implementation slice landed in product code: Video Archiver no longer renders its competing Quick/Advanced selector; its semantic workflow tabs are `Single videos`, `Subscriptions`, and `Other websites`; source-specific copy/placeholders no longer describe Other websites as YouTube; selected-subscription pending/downloaded lists initially render 24 rows, use panel-local scrolling, expose `shown of loaded`, load in fixed 24-row steps, and reset on subscription change. Focused contracts (88/88) and the production frontend build pass. Packaged app-boundary proof remains required after the governed build.
- 2026-07-27: Phase B/C implementation slice landed: Jobs retains canonical `Now`/`Needs attention`/`History`, replaces the eight-button track rail with one backend-bound `Work source` selector, collapses canonical per-track budgets and the shared YouTube gate into one scheduler-health disclosure, gives the work table a sticky-header panel-local scroll surface, and limits expanded groups to 30 child attempts with truthful loaded counts and deterministic 30-row expansion/reset. Focused contracts (89/89) and the production frontend build pass. No scheduler, query, retry, or canonical-count semantics changed.
- 2026-07-27: Packaged audits of v0.1.117 through v0.1.119 drove two additional hierarchy fixes: the Jobs canonical preview now initially mounts 50 groups and expands in fixed 50-group steps, and the Video Archiver preset editor is a collapsed `Download presets` disclosure. The v0.1.118 Jobs audit fell from the pre-change approximately 48,574-pixel page to a 1,096-pixel content surface and from 2,046 mounted audit candidates to 227.
- 2026-07-27: Packaged v0.1.120 exposed that the locally scrolled subscription master still mounted all 260 rows. The master now initially mounts 50 subscriptions, reports `Showing N of M subscriptions`, expands in fixed 50-row steps, and resets when its group or attention filter changes. The packaged audit fell from 311 candidates to 102 while preserving the 260-subscription canonical total outside the render window.
- 2026-07-27: The headless bridge could inspect but intentionally refused generic pagination buttons. The existing explicit-safe contract was applied only to deterministic subscription-video, subscription-master, expanded-attempt, and group-preview `Load more` controls. Packaged v0.1.121 proved 50→100 subscription rows and 50→100 Jobs groups through `/agent/ui_action`; mutating generic controls remain refused.
- 2026-07-27: v0.1.121 is the current governed verification artifact. Contracts pass 89/89; production frontend and desktop/NSIS builds pass. Visual snapshots cover Video Archiver, Jobs `Now`, Jobs `History`, and the scrolled Jobs work surface. Concurrent `vvwatch` recorded 28 samples with zero not-responding, bridge-failure, DB-timeout, path-timeout, database-contention, incomplete-command, freeze, or event-loop-skew results. It recorded two skipped intervals under host load and a 2,627 ms peak `subscription_download_activity` read; that read remains a measured performance target, not a proven freeze.
- 2026-07-27: Proof bundle: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0280/20260727_v0_1_121/summary.md`. Packet remains `IN_PROGRESS` because the carried-forward canonical subscription-total wording, shared-shell height pass, exact p95/retry/repair/archive-stat contention scope, and their full proof gates are not yet complete.
- 2026-07-27: Phase D shell and count-truth slice shipped to governed v0.1.122. Safe Mode now sits with the non-drag window chrome instead of forcing a third narrow-screen header row; 800x600 Jobs and Video Archiver gained 60 pixels of usable content height (`client_height` 384→444, page heading y 201→143) without changing the move handle or window controls. Selected-subscription detail now separates bounded `loaded rows` from canonical `queued total` and `archived total`.
- 2026-07-27: v0.1.122 contracts pass 90/90 and desktop/NSIS builds pass. Packaged 800x600 snapshots show the complete compact shell without overlap on both pages. Concurrent `vvwatch` recorded 30/30 responsive samples, zero skipped intervals, zero bridge/DB/path/contention/incomplete/freeze/skew results, and a 1,922 ms peak `subscription_download_activity` read. Current proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0280/20260727_v0_1_122/summary.md`. Remaining packet work is the exact p95/retry/repair/archive-stat contention scope and proof carried from WP-0258.
- 2026-07-27: Governed v0.1.123 replaces `subscription_download_activity`'s all-history join with an active-drain CTE: only refresh batches with queued/running children enter the projection, while queued/running/succeeded/failed counts for those current drains remain intact. The live read-only query benchmark improved from 2,063.5 ms to 401.8 ms before implementation. A focused engine test proves terminal-only history is excluded while every current-drain status remains accurate.
- 2026-07-27: v0.1.123 contracts pass 91/91; the focused engine test passes. The full engine suite produced 267 passes and one 5.004 s timing-threshold miss under host load; the exact missed validator passed immediately on rerun at 4.71 s. Packaged `vvwatch` recorded three exact activity calls at 1,252, 994, and 545 ms versus v0.1.122's 1,922 and 1,824 ms, with zero freeze/contention/incomplete results. A candidate covering index was tested on a disposable live-DB copy but rejected: it required a 9.5-second migration and 15 MB permanent index for a smaller remaining margin. The 790,945,792-byte temporary copy was deleted; the live DB was read-only. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0280/20260727_v0_1_123/summary.md`.
- 2026-07-27: v0.1.124 implemented bounded attributable background receipts for canonical batch dry-run/retry/repair. The start command returns immediately, matching running batch/mode work is reused, completed receipts are retained for one hour and bounded below 128 entries, and Jobs remains navigable while polling receipt state. Contracts and the focused receipt-retention test pass.
- 2026-07-27: Packaged v0.1.124 visual inspection found a malformed native disclosure marker beside Download presets. v0.1.125 removed the added card chrome and replaced the browser marker with a deterministic `+`/`−` disclosure affordance. The same inspection found and corrected missing copy spacing shared by Single videos and Other websites.
- 2026-07-27: The v0.1.125 watcher caught one 18,712 ms `library_youtube_single_history` call while Worker heartbeats and UI actions remained responsive. Read-only live-DB measurement and `EXPLAIN QUERY PLAN` isolated the secondary unclassified-legacy full-table scan from the indexed canonical page. v0.1.126 moves that exact count to an independent read-only command: the cold packaged Single videos action completed in 8 ms, canonical history in 9 ms, and the independent exact count in 408 ms.
- 2026-07-27: Final v0.1.126 contracts pass 93/93; TypeScript/Vite, Rust check, focused receipt and lineage tests, desktop executable, and NSIS installer pass. Exact 800×600 screenshots/dumps for Single videos, Subscriptions, and Jobs have zero missing accessible names and no observed overlap or malformed marker. Final `vvwatch`: 22 samples, zero not-responding/bridge/DB/path/freeze/skew failures, one skipped interval under declared host load; final trace has no incomplete command. Jobs overview p95 is 56 ms across 23 calls; archive stats is 73 ms. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0280/20260727_v0_1_126/summary.md`. WP-0280 is `DONE`.
- 2026-07-27: Final artifact reconciliation found that the changelog helper recorded its default MSI target even when Tauri produced only NSIS. The v0.1.126 entry now lists only the existing installer, and future entries test target existence before recording a path. PowerShell parser validation passes.

</topic>
