# WP-0170 Summary

Status: DONE
Date: 2026-08-14

## Outcome
- Media Library title search and Source/Sort controls are present in the installed desktop surface and drive the canonical engine query rather than filtering only a rendered page.
- Search, Source, Sort field, and Sort direction persist in namespaced localStorage and were restored in a fresh hidden v0.1.138 runtime.

## Verification
- Governed v0.1.138 build completed successfully.
- Directly inspected `runtime_state_1786712263595.png`: Media Library displayed the title search plus Source and Sort controls with active values.
- Paired runtime dump recorded `media_search=도리`, `media_source_filter=youtube`, `media_sort_by=date`, and `media_sort_direction=desc` on the Media Library page.
- Inspected `LibraryPage.tsx`: initial state reads those keys, effects persist them, and both initial and paginated calls pass search/source/sort to `library_query`.
- Focused canonical-set regression passed: 1 passed, 0 failed. It proves Source and title-search filtering occur before pagination and exercises date/title sorting.
- `cargo check --manifest-path product/engine/Cargo.toml` passed with warnings only in 31.95 seconds using one build job.
- The required desktop frontend build passed inside the governed v0.1.138 target build.

## Evidence
- `evidence.json`
- `governance/snapshots/WP-0209_build_0_1_138/runtime_state_1786712263595.png`
- `governance/snapshots/WP-0209_build_0_1_138/runtime_state_1786712263626.dump.json`
- `product/desktop/build_target/logs/wp_0170_filter_test.stdout.log`
- `product/desktop/build_target/logs/wp_0170_filter_test.stderr.log`
- `product/desktop/build_target/logs/wp_0170_cargo_check.stderr.log`
- `product/desktop/build_target/logs/build_desktop_target_20260814-143555_0_1_138.log`
- `product/desktop/src/pages/LibraryPage.tsx`
- `product/engine/src/library.rs`

## Notes
- The broader packet scope mentions Status and Date-range selectors, but its explicit acceptance requires title search, at least Source and Sort, persistence, and the two build gates. Completion is based on that authoritative acceptance surface.
