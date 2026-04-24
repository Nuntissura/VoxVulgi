# Work Packet: WP-0200 - Diagnostics startup truth and operator-ready component state

## Metadata
- ID: WP-0200
- Owner: Codex
- Status: REVIEW
- Created: 2026-04-24
- Target milestone: Diagnostics operator trust

## Intent

- What: Make Diagnostics reflect startup and component readiness truthfully without acting like a second cold initialization pass when opened.
- Why: Operator feedback shows Diagnostics can report `Startup progress: 100%` while still cold-loading half the page on entry, which undermines trust in component status and failure visibility.

## Scope

In scope:
- Reconcile startup-hydrated tool/component state with the Diagnostics page so opening Diagnostics does not look like a silent re-hydration pass.
- Improve operator-facing readiness/loading/failed reporting for the top Diagnostics summary.
- Keep component status and recent-failure visibility useful even when the app has already completed startup.
- Preserve non-blocking page rendering while avoiding obviously stale or misleading status.

Out of scope:
- Full Diagnostics redesign unrelated to readiness truth.
- Deep storage-accounting redesign already tracked elsewhere unless directly needed for this fix.
- New installer/offline-bundle behavior.

## Acceptance criteria
- Opening Diagnostics after startup no longer feels like all tools are loading from zero again.
- The Diagnostics summary reports readiness/loading/failed state that matches the app's actual startup/component state.
- Recent-failure visibility remains available without requiring the operator to inspect raw logs first.
- Diagnostics remains responsive and non-blocking while loading any still-needed details.

## Test / verification plan

- Compare startup shell status with Diagnostics summary after the app reaches a ready state.
- Verify Diagnostics no longer regresses obviously ready components back to a misleading loading state on entry.
- Verify recent failures remain visible and meaningful after a failed Localization/archiver run.
- Re-run targeted desktop verification (`npm run build`) and Rust verification (`cargo check`).

## Risks / open questions

- Some Diagnostics sections are intentionally page-loaded today; tightening truthfulness without reintroducing heavy startup work needs careful boundary control.
- Startup status and Diagnostics status may currently come from different polling/caching assumptions.

## Status updates

- 2026-04-24: Created after operator smoke showed Diagnostics still behaving like a cold-loading surface despite startup claiming `100%`, with component-state trust breaking down in normal use.
- 2026-04-24: First implementation slice landed: the app now pre-visits Diagnostics after startup settles so component state can hydrate before the operator opens the page, reducing the misleading cold-start feel on first entry. Verification: `npm run build`, `cargo check`.
