# Work Packet: WP-0203 - Localization import cancellation and duplicate queue containment

## Metadata
- ID: WP-0203
- Owner: Codex
- Status: REVIEW
- Created: 2026-04-24
- Target milestone: Localization operator reliability

## Intent

- What: Contain duplicate Localization imports so canceling or re-importing the same source file does not silently fan out into multiple indistinguishable work items or orphaned child jobs.
- Why: Operator testing accidentally started the same local video three times, canceled two imports, and still ended up with three localization pipelines running in parallel; the kept import also spent about thirty minutes in the import stage before downstream work began.

## Scope

In scope:
- Define and implement cancellation propagation so a canceled import cannot leave already-queued child work behind unexpectedly.
- Define same-source duplicate behavior for Localization intake, such as reuse, explicit duplicate confirmation, or clearly labeled independent copies.
- Surface import sub-stage progress and timing truth so long-running imports do not appear frozen with no explanation.
- Keep duplicate/canceled runs distinguishable in Localization and Jobs so operators can tell which item is the kept one.
- Audit the current import path for avoidable long stalls before item handoff or explicit stage start.

Out of scope:
- Global dedupe across the entire archive/media library outside the Localization-owned workspace contract.
- Broad queue-engine replacement unrelated to duplicate/cancel containment.

## Acceptance criteria

- Canceling duplicate local imports does not leave unexpected child localization stages running afterward.
- Importing the same file repeatedly does not silently create multiple indistinguishable active items.
- Long-running imports expose truthful stage/progress information instead of looking idle or frozen.
- Jobs and Localization clearly distinguish kept, canceled, and duplicate intake attempts.

## Test / verification plan

- Reproduce the current "import same file three times, cancel two" scenario and confirm only the kept intake remains active.
- Add focused verification around same-path duplicate intake behavior and import cancellation propagation.
- Capture import-stage timing or stage-label evidence for a local file import.
- Run `cargo check` and desktop `npm run build`.

## Risks / open questions

- Reusing an existing workspace item for a same-path import may be correct for most operators, but some workflows may still want an explicit "new copy" action.
- The long import delay may include more than one root cause, so the first slice may need to focus on truthful staging/containment before deeper performance work.

## Status updates

- 2026-04-24: Created after operator smoke showed duplicate local imports of the same file surviving cancellation and fanning out into parallel ASR/translate/diarize work, with the kept import taking roughly thirty minutes to exit the import stage.
- 2026-04-25: Implementation pass started. Scope is duplicate same-path Localization intake reuse, import batch/cancel containment, and clearer import stage progress.
- 2026-04-25: Moved to REVIEW. Same-path active Localization imports now reuse the existing active job; existing library media reselects the workspace item with completed reuse rows; import cancellation propagates to same-batch children. Queen sample smoke reused item `ab16785e-0fc4-4eba-9363-db81727a31db` with 0 active duplicate imports. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0203/2026-04-25_0315_wp0202_0204/summary.md`.
