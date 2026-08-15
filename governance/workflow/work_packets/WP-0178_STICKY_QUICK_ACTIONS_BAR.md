# Work Packet: WP-0178 - Sticky Quick-Actions Bar

## Metadata
- ID: WP-0178
- Owner: Codex
- Status: DONE
- Created: 2026-04-08
- Target milestone: UX Polish

## Intent

- What: Add a persistent bottom bar to Localization Studio that keeps the most common actions always visible regardless of scroll position.
- Why: The 3 most-used actions (Run, Export, Open Outputs) are in different cards that require scrolling to find. A sticky bar means zero scroll to perform the primary workflow.

## Scope

In scope:
- Sticky bottom bar visible when an item is open in the editor.
- Bar contains: Start/continue run button, Export button, Open outputs button, current item title.
- Bar shows current run status (idle/running/stage name) as a compact indicator.
- Bar should be visually distinct (slight elevation/shadow) and not overlap content.
- Bar hides when no item is loaded.

Out of scope:
- Customizable bar contents.
- Floating/draggable bar position.

## Acceptance criteria
- Bar is visible at the bottom of the editor view at all scroll positions.
- Run, Export, and Open Outputs work from the bar.
- Bar shows current run status.
- `npm run build` passes.

## Status updates

- 2026-08-15: DONE on packaged v0.1.169. The bottom bar remained visible and non-overlapping across top, Track, advanced, backend, benchmark, batch, A/B, QC, cleanup, and artifact scroll positions; the semantic audit exposed current item/status plus enabled Start, Export, and Open outputs controls. Focused handler/CSS contract passed 3/3. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0178/20260815-181110-v0_1_169/summary.md`.
