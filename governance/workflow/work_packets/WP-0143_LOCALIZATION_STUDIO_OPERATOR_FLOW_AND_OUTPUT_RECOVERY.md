# Work Packet: WP-0143 - Localization Studio operator flow and output recovery

## Metadata
- ID: WP-0143
- Owner: Codex
- Status: IN_PROGRESS
- Created: 2026-03-12
- Target milestone: Post-0.1.6 smoke regression recovery

## Intent

- What: Restore Localization Studio as a real operator workflow by making start/progress/output handling explicit and by repairing the missing output/artifact visibility that still makes the feature feel broken in practice.
- Why: The latest smoke still reports no visible localization library, no obvious source/artifact/export actions, no visible output video path, and no confirmed dubbed MP4 result in normal operator use.

## Scope

In scope:

- Audit the current Localization Studio ingest-to-output flow on installer state.
- Add explicit operator control over when a newly configured localization run starts, or otherwise make the auto-start contract unmistakable.
- Add per-item progress visibility for newly queued localization items.
- Repair or expose the source video, subtitle outputs, dubbed audio, working artifacts, and exported video outputs inside Localization Studio.
- Ensure localized video outputs remain MP4 where the shipped path claims MP4 output.
- Fix the current localization root/path visibility gap between Options and Localization Studio.

Out of scope:

- New voice-cloning backend research features unrelated to the currently broken operator path.

## Acceptance criteria

- A normal in-app Localization Studio flow produces a visible non-silent English-dubbed MP4 on installer state.
- Operators can find and open the source video, subtitle files, dubbed audio, artifact folder, and exported video from Localization Studio itself.
- The start/progress contract is understandable enough that operators can set options before work begins without guessing.
- Localization root/path information is visible and consistent with Options.

## Test / verification plan

- Installer-state app-boundary verification using a real local source file.
- Focused engine/Tauri tests for repaired artifact/output discovery seams.
- Proof bundle with resulting output paths, screenshots or UI-state notes, and verification commands.

## Status updates

- 2026-03-12: Created from smoke findings `ST-007`, `ST-010`, `ST-029`, `ST-030`, `ST-031`, `ST-032`, `ST-033`, and `ST-034`.
- 2026-03-12: Added an explicit `Start / continue localization run` surface in Localization Studio, stage-level run visibility, recent-item handoff from the Localization home surface, and visible localization root / deliverables-folder context in both the home surface and the current-item editor.
- 2026-03-12: Tightened the staged run contract so the orchestrator now treats speaker labels and speaker/reference readiness as real checkpoints instead of silently skipping them.
- 2026-03-12: Runtime research now confirms the remaining first-dub gap is speaker-reference acquisition; follow-on implementation is queued under `WP-0152` so this packet can focus on current-item handoff, visible progress, and output discoverability instead of another speculative backend swap.
- 2026-03-12: `WP-0152` is now complete, so the remaining closeout here is the operator-facing handoff/progress/output visibility layer rather than the speaker-reference bridge itself.
- 2026-03-12: Localization Studio home now keeps recent imports visible as active localization items with per-item stage/output summaries, resilient import-to-item matching, and direct open/reveal actions for source media, working folders, and ready preview MP4 outputs. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0143/20260312_043759/`.
- 2026-03-12: Added direct home-surface entry points into the current item editor for run controls, outputs library, and advanced tools, plus editor-side section landing so operators no longer have to guess that these surfaces exist below the home card. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0143/20260312_052059/`.
- 2026-03-22: Fresh operator screenshot feedback shows the home surface still reads as a mostly empty ingest pane rather than the main Localization workspace. The remaining scope is now explicit: promote current-item continuation, workflow/readiness framing, recent localization items, and deliverable visibility on the first surface so Localization Studio visibly represents the shipped source-to-output workflow before an item editor is opened.
- 2026-03-22: Reworked the Localization home surface into a first-screen dashboard: added a main-workflow hero with live counts, a current-item continuation card, a lightweight import/setup card, a staged workflow explainer, an outputs handoff card, and recent localization-item cards with direct open/run/output/advanced actions. Verified with `npm run build`; live operator smoke still pending.
- 2026-03-22: Remaining first-screen hierarchy work is now split into `WP-0153` and `WP-0154` so the contract for dashboard-first presentation and shell-status demotion is explicit instead of staying implicit inside this broader recovery packet.
- 2026-03-22: `WP-0154` is now implemented in code: the home screen has a `Now / Next / Last Output` orientation layer and generic recovery/startup cards are demoted into compact shell-status chrome. Verified with `npm run build`; live operator smoke is still the remaining closeout gate.
