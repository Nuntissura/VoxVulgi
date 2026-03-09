# Work Packet: WP-0131 - Localization Studio end-to-end audit and repair

## Metadata
- ID: WP-0131
- Owner: Codex
- Status: DONE
- Created: 2026-03-09
- Target milestone: Installer smoke remediation tranche

## Intent

- What: Audit the full Localization Studio operator path from source ingest through ASR, translation, speaker setup, dubbing, mix, mux, outputs, and artifact review, then repair the broken operator path.
- Why: Real installer smoke shows that Localization Studio is still failing as a product workflow even though several helper surfaces and backend proofs exist.

## Scope

In scope:

- Item load, track load, ASR, translate, diarize, dub, mix, mux, QC, artifact refresh, and export-path discovery.
- Tool/model/runtime verification for the shipped Localization Studio path.
- Discoverability fixes for currently buried but shipped localization controls when they block real operator use.
- Targeted runtime and UI fixes required to produce a real dubbed MP4 from the installed app.

Out of scope:

- New voice-cloning research features unrelated to the currently broken operator path.

## Acceptance criteria

- A real in-app Localization Studio flow produces a non-silent English dubbed MP4 on installer state.
- Operators can find the required steps, outputs, and artifacts without hidden or misleading UI.
- Failures are explicit and actionable instead of leaving the operator without a usable product output.

## Test / verification plan

- App-boundary verification on installer state.
- Focused engine/Tauri tests for repaired runtime seams.
- Proof bundle with summary, executed operator flow, and resulting output paths.

## Status updates

- 2026-03-09: Created from installer smoke findings `ST-007`, `ST-008`, `ST-024`, `ST-027`, `ST-028`, and `ST-029`.
- 2026-03-09: Repaired Localization Studio to prefer translated English tracks for dubbing/benchmark actions, added a workflow/readiness map plus quick jumps to buried operator surfaces, and changed mix preview to fall back to source-media audio when separation is unavailable so preview generation does not hard-fail.
- 2026-03-09: Verified with engine and Tauri test/build passes plus a dedicated `wp0131_localization_smoke` proof run under `product/desktop/Build Target/tool_artifacts/wp_runs/WP-0131/20260309_061734/`, producing a non-silent dubbed MP4.
