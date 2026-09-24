---
file_id: WP-0321
file_kind: work_packet
updated_at: 2026-09-23
---

# Work Packet: WP-0321 — Simplification pass

## Metadata

- ID: WP-0321
- Owner: Claude
- Status: DONE (2026-09-23; proof `product/desktop/build_target/tool_artifacts/wp_runs/WP-0321/summary.md`)
- Created: 2026-09-23
- Board: `../TASK_BOARD.md`
- Basis: 2026-09-23 read-only simplification audit (four areas: YouTube protection, startup/diagnostics, jobs/database, governance) plus live evidence on 0.1.204 (7 min startup hash, 152 s library scan, `read_admission_timeout`, 339,400 job rows, 11 full DB backups).
- Operator direction 2026-09-23: "do them all back to back, in your preferred order" — covers items S1–S8, including accepting the startup-verification trade-off (S3), the live purge (S2) and deleting old full-database backups beyond the newest three (S5).

## Intent

Remove mechanisms whose cost exceeds their value, keep the user outcomes. No new features.

## Scope (all operator-requested)

- S1 Replace the 152 s `count_youtube_single_unclassified` full scan with an indexed/bounded query; no read may hold a read slot for more than a few seconds on the hot paths.
- S2 Purge finished job history on the live database (backup-first) after the S1/S3/S5/S7 build is installed; add a bridge command `jobs.purge_terminal_history` (dry-run + execute, token + actor + operation id) so the agent can run it without native input.
- S3 Startup provider verification: persist a file-identity receipt (relative path, size, mtime) after a successful full hash; on launch, stat-compare only and skip the full SHA-256 walk when identical; full re-hash when the receipt is missing, any file differs, the install generation changes, or the last full hash is older than 7 days.
- S4 YouTube pacing/protection: one pacing policy per lane (sleep + jitter + request sleep + fragments) and two protection modes (normal, cooldown with doubling wait capped); remove the duplicate knob sets and the cautious/conservative/hold ladder. Design first (read-only), then implement.
- S5 Full-database backups: keep the newest three app-created backups automatically; delete older ones once on operator direction.
- S6 Job model: one row per video with a bounded attempts list, replacing one row per attempt. Design first (read-only), then implement with migration.
- S7 Diagnostics demand coordinator: replace with a small single-flight + generation guard.
- S8 Governance documents: shorten and de-duplicate (PROJECT_CODEX vs AGENTS, TECHNICAL_DESIGN vs PRODUCT_SPEC, task-board row length) without dropping any rule ID or requirement (preservation map required). CLAUDE.md/AGENTS.md mirroring stays (global rule).

Out of scope: database engine change; subscription, library, or archive data changes other than S2/S6 migrations defined in their designs.

## Acceptance

- S1: the replaced query answers in < 1 s on the live 1.2 GB database (read-only measurement).
- S2: bridge dry-run receipt equals the read-only preview (322,116 ± new rows); execute receipt shows backup path then deleted count; queued/running/localization rows unchanged (independent re-count).
- S3: second launch with unchanged files reaches `offline_bundle=ready` without a `provider_tree_verify` walk (trace), first launch/changed file still walks (test).
- S4/S6: design documents approved by being written into this packet's refinement; implementation passes focused tests; live gate and queue verified through the bridge.
- S5: backup directory holds the newest three plus the S2 pre-purge backup; deleted list recorded.
- S7: Diagnostics loads, refreshes and supersedes sections as before (existing contract tests pass).
- S8: preservation map lists every rule ID before/after with no losses.
- One bundled test/build run per phase.

## Status updates

- 2026-09-23: Created. Phase 1 (S1, S3, S5-rotation, S7, S2 bridge command) dispatched; S4 and S6 designs dispatched read-only in parallel.
- 2026-09-23: S5 one-off cleanup (app confirmed closed): kept the newest three full-database backups in `db/backups/` (`wp0300_pre_repair_20260814_0825`, `wp0286_pre_enrichment_20260731_062003`, `20260729_005525_pre_artifact_purge_evidence`); deleted seven older copies and their sidecars (18 files, 4.86 GB): `20260727_wp0277_post_path_pre_identity`, `20260727_wp0277_pre_artifact_quarantine`, `20260727_wp0277_pre_duplicate_quarantine`, `20260727_wp0277_pre_reconcile`, `20260727_wp0283_pre_compaction`, `20260729_003219_pre_remove_two_missing_records`, `app_before_subscription_url_groups_20260703_233519`. `db/subscription_status_backups/` left untouched (subscription data preservation policy).
- 2026-09-23: Phase-1 build (`build_desktop_target_20260923-175451_0_1_204`) installed by the operator; running pid 152252 from 18:01:58. Startup: `offline_bundle` ready at 18:02:32 (34 s after launch; receipt written 18:02:27). S2 bridge dry-run 18:03 reported 322,135 purgeable rows (read-only preview earlier: 322,116). Execute (op `wp0321-purge-execute-180315`) wrote backup `pre_purge_20260923_160324.sqlite` (1.16 GB) and deleted ~50k rows, then failed with `writer_admission_timeout`: the backup ran `VACUUM INTO` through the single writer lane (~90 s), starving other writers (subscription auto-sync also failed 18:03:29), and 5,000-row chunks then timed out. Survivors verified unchanged: queued 17,371→17,394 and running 1→4 (new work), non-purgeable jobs 3→3, subscriptions 315→315, library items 144,267→144,267. Fix (in phase-2 build): backup through a read-only context with `query_only` lifted for the one statement (verified with SQLite: backup succeeds, live writes remain blocked); chunk size 1,000 with up to 30 retries on writer admission timeout.
- 2026-09-23: S2 complete. Phase-2 build installed (pid 172780, 18:37:26): startup took the receipt fast path (`offline_bundle` ready 4 s after start); pacing migration receipt matched design (single {8,5,6,1}, recurring {10,5,6,1}; cooldown 3600/21600). Purge op `wp0321-purge-execute2-183814` progressed (backup `pre_purge_20260923_163818`, 12 s via read slot) until the operator reinstalled the same build at 18:56 (interrupt, safe per-chunk commits). Resumed op `wp0321-purge-execute3-185751`: `completed`, deleted 203,154 of 203,154 selected, 52 min, backup `pre_purge_20260923_165754.sqlite`. Independent re-count: job rows 339,510 → 17,649; queued 17,627 + running 9; non-purgeable 3 → 3; subscriptions 315 → 315; library items 144,267 → 144,267.
- 2026-09-23: Found during S2: the Video Archiver rendered a failed `youtube_subscriptions_list` (trace `database_locked` at 18:57:31, holder `library.rs` lineage backfill write) as "Saved subscriptions: 0" via `.catch(() => null)`; DB held all 315. Fixed in `LibraryPage.tsx` (load state, "could not load … retrying" text, up to 5 retries at 3 s, keeps last good list); ships with the S6 build.
- 2026-09-23: Found during S6 review: the new same-row download retry skipped `library::claim_download_source`, which would have allowed a retry to re-download operator-deleted media or duplicate present media (WP-0284 contract test caught it). Restored the claim with the pre-S6 outcomes; test `s6_retry_refuses_operator_deleted_and_present_media` added.
- 2026-09-23: S8 done. De-duplicated `PROJECT_CODEX.md` (110->100 lines; version/installer/diagnostics prose that restated `AGENTS.md`/`CLAUDE.md` IDs replaced with pointers) and `governance/spec/TECHNICAL_DESIGN.md` (874->861 lines; `VV-VERSION-001..003` and byte-identical `VV-0319-POLICY-001..009` blocks replaced with pointers to `PRODUCT_SPEC.md`, unique design-only detail kept). Added a one-line "How to read this board" note to `TASK_BOARD.md` (343->347 lines) after confirming no script parses it (`grep -rn TASK_BOARD` over `governance/scripts`, `offline-installer-runtime/scripts`, `product/**/*.{ts,rs,ps1}` = no hits); rows untouched. `build_rules.md`, `PRODUCT_SPEC.md`, `AGENTS.md`, `CLAUDE.md` left unchanged (already canonical / no restated duplicate found). Verified all 74 distinct rule IDs (`[VV-...]`, `[OPERATOR-AUTHORITY-...]`) found before edits still appear after, each as a definition in exactly one file (AGENTS.md/CLAUDE.md mirror pair excepted by design). `diff CLAUDE.md AGENTS.md` is empty. Full preservation map: `governance/workflow/work_packets/WP-0321_S8_PRESERVATION_MAP.md`.
