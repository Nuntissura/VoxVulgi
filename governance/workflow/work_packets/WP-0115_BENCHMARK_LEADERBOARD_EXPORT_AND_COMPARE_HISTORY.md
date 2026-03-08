# Work Packet: WP-0115 - Benchmark leaderboard export and compare history

## Metadata
- ID: WP-0115
- Owner: Codex
- Status: DONE
- Created: 2026-03-08
- Target milestone: Voice backend modernization

## Intent

- What: Turn benchmark reports into a durable history with exportable leaderboard and compare outputs.
- Why: Backend decisions should be traceable over time, not overwritten by the latest single-item report.

## Scope

In scope:

- Preserve immutable benchmark snapshots alongside the latest stable report.
- Add compare-history discovery for prior runs on the same item/track/goal.
- Export leaderboard views in durable machine-readable and operator-readable formats.
- Surface the compare history and export actions in Localization Studio.

Out of scope:

- Network-based telemetry or cloud benchmarking services.
- Cross-user/shared benchmark synchronization.

## Acceptance criteria

- Generating a benchmark report keeps a durable snapshot history instead of only overwriting the latest report.
- Operators can review prior benchmark runs for the same track/goal and compare current vs earlier rankings.
- Leaderboard exports are written as durable artifacts and easy to open from the app.
- The ranking/export flow remains local-first and artifact-driven.

## Test / verification plan

- Rust tests for snapshot persistence, history discovery, and leaderboard export.
- Desktop build.
- Tauri/engine tests for history load/export commands.

## Status updates

- 2026-03-08: Created from the research-driven operational backend tranche.
- 2026-03-08: Implemented immutable benchmark snapshot history, leaderboard export artifacts, Localization Studio compare history, and proof under `product/desktop/Build Target/tool_artifacts/wp_runs/WP-0115/20260308_163900/`.
