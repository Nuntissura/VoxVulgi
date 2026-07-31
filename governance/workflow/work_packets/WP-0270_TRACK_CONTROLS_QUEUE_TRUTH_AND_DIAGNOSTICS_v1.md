# Work Packet: WP-0270 - Track controls, queue truth, and diagnostics

## Metadata

- ID: WP-0270
- Owner: Codex
- Status: DONE
- Created: 2026-07-22
- Target milestone: next managed desktop build
- Refinement: `WP-0270_TRACK_CONTROLS_QUEUE_TRUTH_AND_DIAGNOSTICS_v1_REFINEMENT.md`
- Predecessors reused/refined: `WP-0255`, `WP-0256`, `WP-0258`
- Dependencies: `WP-0268`, `WP-0269`

## Intent

- What: Make canonical product tracks, their real settings, and the shared YouTube gate visible and controllable in Jobs/Queue and headless diagnostics.
- Why: The current global `Concurrency` control writes a legacy value the scheduler no longer reads, while provider/track state remains hidden behind one queue projection.

## Scope

- In scope:
  - real per-track runtime settings contract;
  - canonical per-track counts and gate state;
  - no-card Jobs track strip/filter/labels and advanced controls;
  - track-bearing enqueue receipts and context summaries;
  - structured trace plus read-only headless bridge state;
  - frontend/engine/app-boundary performance and visual proof.
- Out of scope:
  - a new dashboard or one card per track;
  - an unsafe YouTube gate override;
  - deriving totals from loaded/filtered rows;
  - destructive queue or media cleanup.

## Existing systems reused

- `WP-0256` current-work-first Jobs layout, canonical totals, receipts, and failure classification.
- `WP-0258` bounded read-only overview/index patterns.
- `WP-0209` dump/console buffer, `WP-0171` headless bridge, and diagnostics JSONL trace.
- Existing Jobs toolbar/advanced disclosure and CSS; no new component framework.

## Acceptance criteria

- Every acceptance criterion and red-team control in the linked refinement passes.
- Every visible control is proven to change scheduler-consumed state.
- Canonical totals, bounded preview counts, and shared-gate state are visibly distinct.
- The UI and bridge agree on track/gate state under a representative large backlog.
- A proof bundle satisfying `governance/workflow/PROOF_STANDARD.md` exists before `DONE`.

## High-ROI additions

- Stable element IDs and a read-only bridge route improve repeatable human/model verification.
- Track-bearing enqueue receipts expose routing errors immediately.
- One indexed aggregate contract serves UI, diagnostics, tests, and future agents.

## Test / verification plan

- Engine settings transaction and restart-persistence tests for every track.
- Canonical grouped-count tests where preview/filter/page contains only a subset.
- Frontend contract tests for track labels, filters, receipts, errors, and gate explanation.
- Representative 55,000-row aggregate timing and bridge-timeout checks.
- Headless bridge navigate/snapshot/dump on Jobs and Video Archiver at normal and narrow widths.
- Visual inspection for readability, discoverability, no overlap, responsive layout, visible important state, coherent navigation, and no new cards.
- Managed desktop build only after `WP-0268` through `WP-0270` code/tests are complete, with version increment and changelog entry for all included WPs.

## Risks / open questions

- YouTube track budget wording must not imply unpaced same-tick starts: foreground and background transfers may overlap, but aggregate process starts still pass through the shared 5-10 second gate.
- If the installed app cannot be safely upgraded for exact bridge proof, the WP remains `IN_PROGRESS` until the real app boundary is available; build-only proof is insufficient.

## Status updates

- 2026-07-22: v1 refinement and contract created from verified Jobs UI/runtime mismatch and current queue-monitor patterns. No product code changed.
- 2026-07-22: Activated after WP-0269 implementation, adversarial repairs, and independent query-plan revalidation passed. WP-0268/WP-0269 final live app-boundary proof remains intentionally shared with this packet's managed build.
- 2026-07-22: `DONE` in installed v0.1.105. Headless bridge and inspected 800×600 snapshots prove six canonical tracks, zero active unclassified rows, scheduler-consumed budgets, explicit recurring direct-transfer scope, shared YouTube gate visibility, readable layout, and zero console errors. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0270/summary.md`.
