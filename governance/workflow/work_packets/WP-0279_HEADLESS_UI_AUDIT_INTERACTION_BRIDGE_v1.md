# Work Packet: WP-0279 - Headless UI audit interaction bridge

## Metadata

- ID: WP-0279
- Owner: Codex
- Status: DONE
- Created: 2026-07-26
- Refinement: `WP-0279_HEADLESS_UI_AUDIT_INTERACTION_BRIDGE_v1_REFINEMENT.md`

## Intent

Let a no-context model inspect and safely navigate every UI state needed for evidence-backed VoxVulgi layout review without stealing focus or mutating operator data.

## Scope

- Bounded semantic UI inventory.
- Allowlisted structural/read-only interactions.
- Headless-only bridge gate and structured receipts.
- Existing diagnostics trace integration.
- App-boundary proof on Video Archiver and Jobs/Queue while `vvwatch` runs.

## Acceptance criteria

- All refinement acceptance criteria pass.
- The operator's foreground app and unrelated processes remain untouched.
- Build and proof satisfy `build_rules.md` and `PROOF_STANDARD.md`.

## Status updates

- 2026-07-26: Contract and research-backed refinement created after confirming the existing bridge could navigate only top-level pages and could not exercise tabs, disclosures, filters, rows, or scrolling.
- 2026-07-26: Live multi-instance audit exposed a bridge-discovery ownership bug: an older foreground instance exiting removed the newer headless instance's marker. Cleanup is now PID-owned so one instance cannot remove another instance's discovery files.
- 2026-07-26: Live trace exposed a headless safety defect: startup auto-sync queued 257 recurring checks. Headless setup now skips the runner, both subscription auto-sync lanes, offline hydration/seeding, fallback-media relocation, and watcher-supervisor startup.
- 2026-07-26: Video Archiver and Jobs structural controls now expose semantic selection/expanded state so the bridge can select subscription rows and expand job groups without allowlisting generic or mutating buttons.
- 2026-07-26: App-boundary proof corrected accessible-name precedence: visible button/option text now precedes `title` fallback, so receipts identify `Expand (113)` and subscription titles instead of generic tooltips or empty names.
- 2026-07-26: DONE on governed desktop v0.1.116. Final headless trace proves the job runner and offline hydration were skipped and no startup auto-sync fired; named subscription selection and Jobs group expansion passed; the 90-second external watcher reported no unresponsive, bridge-failure, DB-timeout, or incomplete-command sample. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0279/final_proof/summary.md`.
