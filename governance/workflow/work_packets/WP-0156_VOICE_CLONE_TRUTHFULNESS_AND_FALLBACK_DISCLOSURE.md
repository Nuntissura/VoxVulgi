# Work Packet: WP-0156 - Voice clone truthfulness and fallback disclosure

## Metadata
- ID: WP-0156
- Owner: Codex
- Status: NEEDS_VALIDATION
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
- 2026-03-24: Implementation started. First slice is adding explicit clone outcome state to the managed voice-preserving runtime, manifest/report metadata, and current operator-facing benchmark surfaces so fallback no longer reads as generic success.
- 2026-03-24: First implementation slice landed in code. Managed voice-preserving runs now emit explicit clone-intent and clone-outcome state per segment plus run-level clone outcome/counters in report + manifest metadata, and the benchmark cards now show clone preserved vs partial/plain-TTS fallback. Packet remains open for broader current-item/operator proof and follow-on surfaces.
- 2026-03-24: Follow-on current-item slice landed in code. `item_artifacts_list_v1` now exposes live clone-truth metadata from TTS manifests, and Localization Studio surfaces that truth directly in the item voice plan, localization run, and outputs cards so operators no longer need Benchmark Lab just to see whether the latest dub was actually cloned.

## Status reconciliation — 2026-09-30

- Current status: NEEDS_VALIDATION
- Manifest/report, benchmark, artifact and current-item clone-truth wiring landed. Remaining is genuine converted plus fallback/error output and UI evidence, including explicit fallback policy; historical wording about wiring in progress is superseded by later implementation notes.
- Historical requirements and proof remain preserved. Reconciled by WP-0326; no new runtime proof.


<topic id="installed-haerin-proof-20261005" status="partial-proof" wp="WP-0156" updated_at="2026-10-05">

## Installed Haerin proof reconciliation - 2026-10-05

- Status remains `NEEDS_VALIDATION`.
- The original CosyVoice report for job `da18f604-2531-4697-a865-d392e2cea496` records `clone_preserved`, one converted segment and zero fallback/standard-TTS segments. The published MKV English audio title is `English (AI dub - cloned voice)`, consistent with the successful report. This proves the successful report/artifact-label side only.
- Remaining: actual fallback/error outcome and operator-visible current-item clone/fallback labels for both cases. Diagnostics readiness screenshots do not prove Localization current-item outcome rendering. The explicit fallback policy is anchored in PRODUCT_SPEC clone-truth requirements and TECHNICAL_DESIGN voice-preserving outcome/report requirements; retain focused policy/runtime proof reconciliation.
- Inspected evidence: [installed workflow summary](../../../product/desktop/build_target/tool_artifacts/wp_runs/WP-0329/20261005/actual_localization_a0bb600/summary.md) and adjacent independent_validation.json, original voice report and canonical output records.
- Evidence is bound to installed source `a0bb600141fffcfb1f7274189917af11b17e562a`, version `0.1.205`, executable SHA-256 `a197ae258a379124b8797f00d0559ec5496c8c6fcbe1b840379de0e036faad11`, and qualified runtime `runtime_7ea19cb688095cd14dfbc84e`. Reuse only for the inspected scenario and unchanged asserted inputs; this is not exact-source proof for later production `d066`, not subjective speech/translation quality, and not clean blocked-network final ISO acceptance. The final offline gate and its one unused acceptance attempt remain open.

</topic>
