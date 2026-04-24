# Work Packet: WP-0193 - Jobs operator context and direct output navigation

## Metadata
- ID: WP-0193
- Owner: Codex
- Status: IN_PROGRESS
- Created: 2026-04-23
- Target milestone: Desktop archive/operator usability

## Intent

- What: Make Jobs rows explain what each job corresponds to and provide direct navigation to the relevant output, artifact, log, and source context.
- Why: Operator smoke showed that failed jobs are hard to interpret because many rows expose only raw job type, IDs, and error text, especially when archiver failures happen before an item is created.

## Scope

In scope:
- Surface human-readable media/source context for Jobs rows when available.
- Improve failed archiver rows that currently have no `item_id` so the operator can still tell which URL/subscription/profile caused the failure.
- Keep direct access to logs, artifacts, and output folders easy to find.
- Make success/failure navigation paths more obvious from the Jobs page itself.

Out of scope:
- Replacing the Jobs page with a different workflow model.
- Broad queue-engine redesign or backend scheduler changes.

## Acceptance criteria
- Operators can identify what a recent job corresponds to without opening raw logs first.
- Failed archiver rows with no `item_id` still expose useful source context.
- Direct output/log/artifact navigation remains available and becomes easier to discover.
- Focused desktop verification covers failed and successful job rows.

## Test / verification plan

- Inspect how Jobs rows are currently populated for succeeded vs failed archiver jobs.
- Add focused desktop verification for failed one-shot archiver rows and succeeded output rows.
- Re-run desktop build verification after the UI changes.

## Status updates

- 2026-04-23: Created after smoke feedback that Jobs is useful in principle but still does not tell the operator what failed rows correspond to or where completed outputs live.
- 2026-04-23: Implemented a first operator-context pass in `JobsPage`: rows now resolve item titles/source URLs where possible, direct URL/import/subscription jobs show human-readable targets, batches summarize those targets, and non-item jobs expose direct target-root/source actions. `npm run build` passed.
- 2026-04-24: Follow-on operator smoke says the page is still not sufficient for real troubleshooting when a queue is busy: rows still need clearer playlist/video/profile naming, more obvious current-stage meaning, and direct file/folder navigation for completed outputs so Jobs can serve as the operational command surface instead of only a raw trace list.
