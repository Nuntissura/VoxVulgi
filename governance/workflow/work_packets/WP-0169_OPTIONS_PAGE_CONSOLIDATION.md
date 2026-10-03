# Work Packet: WP-0169 - Options Page Consolidation

## Metadata
- ID: WP-0169
- Owner: Codex
- Status: DONE
- Created: 2026-04-08
- Target milestone: UX Polish

## Intent

- What: Consolidate the Options page from 5 separate cards (base root + 4 feature roots) into a compact table view, and improve the YouTube auth UX.
- Why: Each feature root card shows Status/Effective/Default/Override with identical layout, creating visual repetition. The spec says "feature panes should show the resolved effective path but should not own or duplicate the root-configuration card" (Section 4.1). The YouTube cookie textarea has no guidance for non-technical users.

## Scope

In scope:
- Replace the 4 feature-root cards with a single table:
  | Feature | Effective path | Status | Override |
  - Each row has a "Change" button for the override.
- Keep the base storage root card as the primary configuration point.
- Improve YouTube auth UI:
  - Use the operator-approved current YouTube sign-in wizard: explicit browser choice, three-step sign-in/verification guidance, and advanced manual cookie fallback (WP-0266/WP-0267; replacement approved 2026-10-03).
  - Add placeholder text explaining cookie format
  - Add brief help text or link explaining how to export cookies from a browser
- Show free disk space next to the effective storage path.

Out of scope:
- Adding new configuration options.
- Changing storage root backend logic.

## Acceptance criteria
- Feature roots are shown in a single table instead of 4 separate cards.
- YouTube auth uses the current WP-0266/WP-0267 browser-choice and three-step sign-in wizard with help/recovery guidance and advanced manual cookie fallback, formally replacing the original three radio buttons under the operator decision of 2026-10-03.
- Free disk space is visible next to storage paths.
- `npm run build` passes.

## Test / verification plan
- Visual snapshot of Options page showing consolidated layout.
- Verify override changes persist after page switch.

## Status reconciliation (2026-09-30)

- 2026-09-30 status reconciliation: NEEDS_VALIDATION. WP-0301 explicitly preserves Options consolidation and has packaged proof, but no WP-0169 proof bundle or complete original-criterion mapping exists. Remaining: Reconcile original feature-root table, authentication guidance/selection, free-disk-space display and override persistence against current Options; record exact missing behavior or qualifying successor proof. Original requirements and dated history are retained; no product/runtime verification was rerun in this status-only pass.

## Operator-authorized authentication replacement (2026-10-03)

- Exact operator decision: "Use the current wizard as the formal replacement".
- This decision replaces only WP-0169's original authentication radio controls/acceptance with the current approved WP-0266/WP-0267 browser-choice, three-step sign-in/verification/recovery wizard and advanced manual cookie fallback. Root consolidation, free-space display, persistence and build requirements remain unchanged.
- Preserved original scope: Add radio buttons: "No authentication" / "Use browser profile cookies" / "Paste exported cookies".
- Preserved original acceptance: YouTube auth has radio-button selection with help text.
- Original intent is retained: make authentication selection and instructions understandable. Final packaged proof must inspect browser choice, sign-in guidance and manual fallback; the historical radio layout is no longer the current acceptance requirement.

## Final proof reconciliation (2026-10-03)

- DONE: Final 9f8a137 packaged proof passed: five-root single table, real native capacity independently compared, audited reset persisted across page switch and owned restart, approved current wizard/manual fallback inspected. Root reviewed summary/images; current acceptance complete. Historical radio criterion and dated replacement preserved.
- Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0169/20261003_final_9f8a137/summary.md`; final automated/build observation: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0331/20261003/summary.md`.
