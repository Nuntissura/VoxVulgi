# Work Packet: WP-0204 - Localization terminal truth and deliverable visibility

## Metadata
- ID: WP-0204
- Owner: Codex
- Status: BACKLOG
- Created: 2026-04-24
- Target milestone: Localization operator usability

## Intent

- What: Make Localization and Jobs describe terminal outcomes truthfully so operators can tell the difference between "imported only", "captions ready", "translation ready", "failed before dub", "preview ready", and "export ready".
- Why: Operator smoke reported that Jobs showed work as finished, while Localization Library stayed empty and no file appeared in the expected output folder, leaving no clear explanation of whether anything useful had actually been produced or where it should live.

## Scope

In scope:
- Define truthful run-outcome states for the current staged Localization pipeline.
- Surface those states in Localization home, current-item context, and Jobs summaries.
- Make the source file path, AppData working-artifact path, and final deliverable path explicit so operators know what should exist at each stage.
- Avoid implying full success when the run has only produced partial artifacts or has stopped before preview/export generation.
- Improve direct-open/reveal actions around source media, working files, and deliverables based on what actually exists.

Out of scope:
- New deliverable formats or major export-feature expansion.
- Replacing the existing staged pipeline with a different workflow model.

## Acceptance criteria

- Operators can tell immediately whether a run has only imported media, produced captions/translation, failed at diarization, or created a preview/export deliverable.
- Localization Library and Jobs only advertise outputs that really exist.
- The expected locations for source media, working artifacts, and final deliverables are visible and understandable from the app.
- Partial runs no longer create a misleading "finished but no output" impression.

## Test / verification plan

- Reproduce the current partial-run scenario where jobs finish without a usable preview/export deliverable.
- Verify Localization home, current item, and Jobs all describe the same truthful terminal state.
- Add focused verification around output-path and artifact-availability actions.
- Run `cargo check` and desktop `npm run build`.

## Risks / open questions

- This may require a small run-summary contract rather than relying only on individual job rows.
- Product wording must stay clear about the difference between source-path reuse and copied/generated outputs.

## Status updates

- 2026-04-24: Created after operator smoke reported "finished" localization work with no visible Localization Library item, no expected output file, and no clear explanation of whether the source file had been copied, whether captions existed, or whether a preview/export had actually been produced.
