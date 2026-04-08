# Work Packet: WP-0168 - Jobs Page Cleanup

## Metadata
- ID: WP-0168
- Owner: Codex
- Status: REVIEW
- Created: 2026-04-08
- Target milestone: UX Polish

## Intent

- What: Clean up the Jobs/Queue page by removing developer-only controls from default view, consolidating per-row actions, and improving labels.
- Why: The page exposes "Enqueue dummy job" (developer-only), uses "flush" terminology, and each job row has 8+ action buttons. Non-technical users are intimidated and can't quickly find what they need.

## Scope

In scope:
- Move "Enqueue dummy job" / "Run test job" behind a developer toggle or to Diagnostics page.
- Rename "Flush cache/history" to "Clean up old jobs and logs" with explicit checkboxes showing what will be removed (terminal jobs, log files, work folders, cache entries).
- Consolidate per-job action buttons: keep primary actions visible (Cancel, Retry) and group secondary actions (Open log, Open outputs, Open artifacts, Export) into a "More..." dropdown or expandable row.
- Show item title alongside or instead of truncated job ID where an item association exists.
- Replace "sub" prefix on nested jobs with visual indentation (tree lines).
- Add a brief status explanation: "Paused — queued jobs will not start until resumed" when queue is paused.

Out of scope:
- Changing job execution logic or queue behavior.
- Backend changes.

## Acceptance criteria
- Default view does not show "Enqueue dummy job".
- Job rows show item title where available.
- Secondary actions are grouped, not a flat button row.
- "Clean up" flow clearly shows what will be removed before confirmation.
- `npm run build` passes.

## Test / verification plan
- Visual snapshot of Jobs page with jobs in various states.
- Verify developer toggle hides test controls.
