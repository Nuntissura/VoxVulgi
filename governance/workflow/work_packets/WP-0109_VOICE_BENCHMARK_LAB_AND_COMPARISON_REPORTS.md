# Work Packet: WP-0109 - Voice benchmark lab and comparison reports

## Metadata
- ID: WP-0109
- Owner: Codex
- Status: BACKLOG
- Created: 2026-03-08
- Target milestone: Voice backend modernization

## Intent

- What: Add a benchmark lab that discovers current voice outputs and variants, computes comparable metrics, and emits ranked reports.
- Why: Backend changes and variant selection should be evidence-driven rather than subjective or memory-based.

## Scope

In scope:

- Discover available current-item voice artifacts and variants.
- Compute a stable comparison report using local metrics such as coverage, timing fit, silence/clipping/noise, and similarity proxies.
- Emit JSON and Markdown benchmark reports under item artifacts.
- Surface the benchmark summary in Localization Studio.

Out of scope:

- Cloud benchmarking.
- Multi-user annotation workflows.

## Acceptance criteria

- Operators can generate a ranked benchmark report for an item with existing voice outputs.
- The report is stored as a durable artifact and easy to open.
- Rankings are explainable via visible metric breakdowns.

## Test / verification plan

- Desktop build.
- Rust tests for ranking/report logic.
- Local report-generation smoke against sample artifacts.

## Status updates

- 2026-03-08: Created from the research transfer packet.
