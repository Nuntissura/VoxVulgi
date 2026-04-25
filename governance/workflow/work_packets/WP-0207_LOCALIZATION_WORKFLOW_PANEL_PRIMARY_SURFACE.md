# Work Packet: WP-0207 - Localization workflow panel as the primary surface

## Metadata
- ID: WP-0207
- Owner: Codex
- Status: IN_PROGRESS
- Created: 2026-04-25
- Target milestone: Localization operator usability

## Intent

- What: Make the Workflow Panel the central work surface in Localization Studio. Today's `loc-workflow` jump-button grid and the separate `loc-run` card are merged into one stepper-style panel that owns the explicit run controls plus a per-stage row with an explainer, readiness indicator, and "Open controls" link to the existing section.
- Why: Operator feedback: "I do like the workflow panel, it is instructive. Move all the options and steps into this panel." The current arrangement splits the same information across the Workflow Map (readiness rows + jump buttons) and the Localization Run card (run buttons + a second stage table). Operators have to alternate between them, while the actual controls live in 11+ separate large sections further down.

## Scope (this WP - scaffold + run-controls migration)

In scope:
- Replace the current Workflow Map card and the separate Localization Run card with a single `Workflow Panel` card that:
  - hosts the explicit run controls (Start / continue localization run, Generate missing speaker refs, Queue QC, Queue export pack, current export root, run summary, clone-status banner) at the top, lifted verbatim from `loc-run`;
  - renders one row per pipeline stage (ASR, Translate -> EN, Speaker labels, Speaker / references, Dub, Mix, Mux), each with: a one-line explainer derived from `SECTION_HELP` for the relevant section, a readiness chip (`Ready` / `Needs attention` / `Running` / `Failed`) sourced from `localizationRunStages`, the existing `detail` text, and a single `Open controls` button that scrolls to the stage's existing section;
  - includes a collapsible `Advanced` expander surfacing the same advanced rows that exist today (Voice plan & reusable voices, Backend strategy, Benchmark lab, Batch & A/B, QC / artifacts) using the existing `advancedLocalizationRows` data, each with explainer, readiness, jump buttons;
  - keeps the keyboard shortcuts `<details>` block.
- Preserve the `id="loc-run"` anchor on the merged panel so existing `scrollToLocalizationSection("loc-run")` callers still work, and add a `loc-workflow` alias if the SectionHelp/scroll path requires it.
- Update `SECTION_HELP` for `loc-workflow` to reflect the merged surface.

Out of scope (explicitly deferred to follow-on WPs):
- Moving each stage's actual controls inline into its workflow row (e.g., embedding the Run ASR, Run Translate, Run Diarize, Run Mix, Run Mux buttons + their option pickers inside the stage rows). That is the inline-controls migration; it will land stage-by-stage as WP-0208 (Captions inline), WP-0209 (Voice plan inline), etc.
- Decomposition of `SubtitleEditorPage.tsx` into stage-scoped modules (covered separately).
- Any change to the engine / Tauri contract.

## Acceptance criteria

- Localization Studio shows one combined Workflow Panel (no separate Workflow Map and Localization Run cards).
- The panel exposes the explicit run controls + per-stage readiness rows + advanced expander + keyboard shortcuts in that order.
- Every stage row has an `Open controls` button that scrolls to its existing section.
- All existing `scrollToLocalizationSection("loc-run")` and `loc-workflow` paths still land on a valid anchor.
- Existing `localizationRunStages`, `localizationReadinessRows`, and `advancedLocalizationRows` data are reused (no duplication of stage truth).
- `cargo check` (engine) and desktop `npm run build` pass when run.

## Test / verification plan

- Manual: open Localization Studio with the Queen sample. Confirm the panel shows the run controls at top, the per-stage rows with explainer + chip + Open-controls link, and the advanced expander.
- Manual: click each stage's `Open controls` button and confirm it lands on the correct section.
- Manual: trigger Start / continue localization run from the new panel and confirm batch summary / stage states update in place.
- Manual: test Ctrl+Enter / Ctrl+3 keyboard shortcuts still resolve to the workflow surface.
- Capture before/after snapshots via the agent bridge under `governance/snapshots/WP-0207/`.

## Risks / open questions

- Risk: any consumer that depends on the old Workflow Map or Localization Run card being separate (e.g., layout assumptions, scroll offsets) will need adjustment. Mitigation: all `id` anchors are preserved; the merged panel keeps the same vertical position the old Workflow Map occupied.
- Risk: the explainer text duplicates `SECTION_HELP`. Mitigation: derive the workflow-row explainer from the existing entries instead of inventing new copy.
- Open: do we keep the inline stage table that was in `loc-run`, or rely entirely on the new per-stage rows? For this WP we drop the stage table; the per-stage rows replace it.

## Status updates

- 2026-04-25: Created. Implementation pass started in parallel with WP-0205/0206.
