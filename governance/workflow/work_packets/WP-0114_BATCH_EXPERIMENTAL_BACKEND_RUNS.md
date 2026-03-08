# Work Packet: WP-0114 - Batch experimental backend runs

## Metadata
- ID: WP-0114
- Owner: Codex
- Status: DONE
- Created: 2026-03-08
- Target milestone: Voice backend modernization

## Intent

- What: Extend experimental backend execution from one item at a time into bounded batch runs across selected item sets.
- Why: Research backends only become operationally useful when operators can compare them over a representative episode set instead of clicking one item at a time.

## Scope

In scope:

- Add a batch queue flow for experimental backend render runs across multiple selected items.
- Reuse existing item-set selection and track-selection logic where possible.
- Allow one or more configured backends to be queued against the same bounded item set.
- Preserve explicit batch IDs, per-item warnings, and operator-visible job summaries.

Out of scope:

- Silent auto-selection of a new global managed default.
- Unbounded full-library backend sweeps without explicit operator selection.

## Acceptance criteria

- Operators can queue experimental backend runs across a selected item set from Localization Studio.
- The batch flow preserves per-item track selection, backend identity, and variant labels in queued jobs and artifacts.
- Skips/failures are explicit when an item lacks a usable subtitle track or the adapter is not ready.
- The batch flow reuses existing batch/job transparency instead of hiding work behind inline UI loops.

## Test / verification plan

- Rust tests for batch request normalization and per-item job queueing.
- Tauri command tests where applicable.
- Desktop build.

## Status updates

- 2026-03-08: Created from the research-driven operational backend tranche.
- 2026-03-08: Implemented bounded experimental backend batch queueing, Localization Studio backend-matrix controls, and proof under `product/desktop/Build Target/tool_artifacts/wp_runs/WP-0114/20260308_161900/`.
