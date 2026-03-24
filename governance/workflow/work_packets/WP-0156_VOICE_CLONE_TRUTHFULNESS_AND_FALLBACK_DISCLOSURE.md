# Work Packet: WP-0156 - Voice clone truthfulness and fallback disclosure

## Metadata
- ID: WP-0156
- Owner: Codex
- Status: BACKLOG
- Created: 2026-03-24
- Target milestone: Educational-core voice-clone recovery

## Intent

- What: Make the runtime and operator surfaces truthful about whether a dubbed result was actually voice-preserved, partially converted, or plain TTS fallback.
- Why: Current runtime behavior can still copy base Kokoro TTS into the final dubbed artifact path when conversion fails, which means the app can appear successful while not delivering the cloned-voice outcome the product claims.

## Scope

In scope:

- Define explicit clone-success, partial-conversion, and fallback states for voice-preserving runs.
- Wire the report/manifest/job/UI surfaces so those states are visible to operators.
- Remove misleading "voice-preserving success" presentation when conversion did not actually occur.
- Decide and implement the correct failure-vs-fallback policy for the managed voice-preserving path.

Out of scope:

- Redesigning reusable voice asset management.
- Replacing the managed backend family.

## Acceptance criteria

- Localization Studio, artifact metadata, and reports clearly distinguish real converted output from plain TTS fallback.
- A voice-preserving run that falls back no longer looks identical to a successful cloned-voice result.
- The chosen fallback policy is explicit in spec/design and reflected in runtime behavior.

## Test / verification plan

- Focused engine/Tauri tests for report and manifest truthfulness.
- Desktop UI verification that clone/fallback state is visible on the current-item workflow.
- Proof bundle showing at least one true-conversion case and one fallback/error case.

## Risks / open questions

- Failing every fallback case may reduce resilience on weak machines; allowing fallback without strong labeling damages product truthfulness.
- Existing proof and benchmark surfaces may need coordinated updates so they consume the new truth-state correctly.

## Status updates

- 2026-03-24: Created from inspection findings that the current educational-core path can overstate cloned-voice success when conversion fails.
