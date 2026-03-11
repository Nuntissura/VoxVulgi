# Work Packet: WP-0083 - Voice QC

## Metadata
- ID: WP-0083
- Owner: Codex
- Status: DONE
- Created: 2026-03-06
- Target milestone: Phase 2 reliability hardening

## Intent

- What: Add voice-specific QC that flags bad references, silence, clipping, noise, weak similarity, and likely mismatch issues before or after dubbing runs.
- Why: Operators need fast signals about why a clone setup or dub output is poor before spending time on a full render.

## Scope

In scope:

- Reference-clip QC.
- Dub-output QC for silent or degraded segments.
- Explainable warnings surfaced in Localization Studio.

Out of scope:

- Hard policy gating that blocks operators from proceeding.
- Demographic inference beyond coarse operator-facing quality heuristics.

## Acceptance criteria

- QC reports flag missing/silent/noisy reference clips and output issues.
- Operators can inspect warnings and proceed or revise inputs.
- QC results are exportable with job artifacts.

## Test / verification plan

- Audio-stat unit tests.
- Engine QC report tests.
- Manual smoke with intentionally poor clips.

## Status updates

- 2026-03-06: Created after repeated need for faster reference/output quality feedback.
- 2026-03-06: Completed. QC reports now include voice-reference and dubbed-output analysis (silence, clipping, noise, and weak-similarity heuristics) plus Localization Studio rendering. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0083/20260306_204301/summary.md`.
