# WP-0204 Proof Summary

Status: REVIEW

Scope verified:
- `item_outputs` now returns source media, working directories, subtitle/translation availability, terminal state, terminal summary/detail, stage/progress/error, and deliverable existence/path.
- Localization home, Localization Library, and Jobs consume the shared terminal outcome instead of inferring success from finished job rows alone.
- Localization Library now shows run outcome, outcome detail, caption/translation state, source media path, latest source captions, latest English translation path, and deliverable folder/path truth.
- Jobs summaries include per-item outcome and deliverable path when one exists.

Live Queen-sample evidence:
- The repaired UI correctly reports the existing Queen item as `Failed before deliverable: Label speakers` rather than implying a complete preview/export.
- The Localization Library shows the resolved localization root and error detail for the diarization failure, making the missing deliverable explicit.

Visual debugger snapshots:
- `governance/snapshots/WP-0203_0204/localization_queen_absolute_1700_1777080267045.png`
- `governance/snapshots/WP-0203_0204/localization_queen_library_1850_1777080320698.png`
- `governance/snapshots/WP-0203_0204/localization_queen_library_2050_1777080335198.png`

Verification commands:
- `cargo check` from `product/desktop/src-tauri`
- `npm.cmd run build` from `product/desktop`
- `cargo check` from `product/engine`

Notes:
- The Queen item still carries the historical failed diarization job from the pre-repair smoke, so WP-0204 proof intentionally demonstrates truthful partial/failed terminal state rather than a completed deliverable.
