# Work Packet: WP-0199 - Localization explicit start contract and run-progress surface

## Metadata
- ID: WP-0199
- Owner: Codex
- Status: IN_PROGRESS
- Created: 2026-04-24
- Target milestone: Localization operator usability

## Intent

- What: Make Localization Studio start behavior explicit, operator-controlled, and legible before jobs begin.
- Why: Current operator feedback is that Localization feels like it auto-starts before settings are understood, while progress/failure truth is mostly visible only in Jobs/Queue.

## Scope

In scope:
- Reshape Localization home/start surfaces so import/setup and run-start are clearly separated.
- Add or reinforce one distinct primary start action after the operator can review source language and run settings.
- Demote or remove implicit batch-on-import behavior from the Localization-owned intake path where it conflicts with explicit start expectations.
- Surface stage-level run progress and the latest failure state from Localization Studio itself.

Out of scope:
- Redesigning the deep editor sections unrelated to start-state clarity.
- Replacing Jobs/Queue as the durable execution log.
- Changing archive-side batch-on-import semantics outside Localization-owned intake.

## Acceptance criteria
- Importing from Localization Studio does not feel like hidden work has already started before the operator reviews the run.
- The operator can see what will happen next before pressing a clear start action.
- Localization Studio exposes stage-level progress or failure truth for the current item without requiring the operator to live in Jobs/Queue.
- The start surface is materially less cluttered and easier to read as a first-run operator entrypoint.

## Test / verification plan

- Inspect the live Localization home/start surface before and after the change using the agent snapshot flow.
- Verify a newly imported item remains idle until the explicit start action is triggered.
- Verify run-stage progress/failure state updates on the Localization surface while jobs are active.
- Re-run targeted desktop verification (`npm run build`) and Rust verification (`cargo check`).

## Risks / open questions

- Existing batch-on-import settings are global and currently leak into Localization-owned intake.
- Some operators may still want optional auto-run later, so the first slice should prioritize explicitness over configurability creep.

## Status updates

- 2026-04-24: Created after operator smoke and live inspection showed Localization starts are hard to reason about, with implicit follow-on jobs and no trustworthy run-progress story on the main surface.
