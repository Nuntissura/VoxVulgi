# Work Packet: WP-0211 - Localization editor master-detail layout

## Metadata
- ID: WP-0211
- Owner: Codex
- Status: IN_PROGRESS
- Created: 2026-04-26
- Target milestone: Localization operator usability
- Builds on: WP-0207, WP-0208

## Intent

- What: Replace the long vertical stack of 13 bordered cards in the localization editor with one cohesive master-detail panel: a fixed left rail listing the staged workflow, and a right pane that renders only the selected stage's content. Strip the inner card chrome so the page reads as one panel, not a deck of cards.
- Why: Operator feedback is direct: "I feel there should only be a single panel, I am not a fan of the card system we have." The card system gives equal visual weight to everything from the 12 cm Workflow panel to a tiny Glossary, so nothing reads as primary and the page feels like a junk drawer. A master-detail layout enforces single-mention by construction (each stage owns its content) and matches the operator's mental model (one item, one staged workflow, one place to act).

## Scope (this WP - shell + stage routing + first cleanup)

In scope:
- New top-level editor shell: a thin item header strip + a two-column body. Left rail (~320 px) lists the eight stages (Captions, Translate, Speakers, Voice plan, Dub, Mix, Mux, Files). Right pane renders only the selected stage's content.
- Stage selection state with a smart default (first stage that is not `Done` for the current item; falls back to Captions).
- Migrate every existing top-level editor card under a stage:
  - **Captions** -> Track-and-segment-editor surface (track switcher, segment table, ASR controls)
  - **Translate** -> Translate controls + Glossary
  - **Speakers** -> Diarize controls + speaker tooling that today lives in Reusable Voice Basics' diarization-related slice
  - **Voice plan** -> Reusable Voice Basics + Reusable Tools content + voice plan grid + speaker references
  - **Dub** -> Dub run + Backend strategy + Benchmark lab + Experimental backends + A/B preview
  - **Mix** -> Mix dub run + Mix settings + Separation + Vocals cleanup + TTS preview variants
  - **Mux** -> Mux run + container/bilingual + Outputs export + QC report
  - **Files** -> Localization Library (Source / Working / Deliverables tabs) + Artifacts table
- Remove pure-redundancy cards: First Dub Guide, Reusable Tools (the jump-button block), Advanced Tools (loc-advanced index that duplicates the Workflow expander).
- Strip the heavy `.card` chrome (border + radius + padding) from migrated sections; use typography + subtle dividers for hierarchy instead.
- Preserve all existing scroll-anchor IDs (`loc-track`, `loc-voice-basics`, `loc-run`, `loc-outputs`, `loc-artifacts`, `loc-library`, etc.) so existing programmatic jumps from `App.tsx` (compact-home buttons, recent-item card actions, agent bridge `/agent/navigate?section_id=...`) still resolve. They scroll the right pane to the matching stage rather than to a separate card.

Out of scope (deferred):
- Same single-panel treatment for `LocalizationStudioHome`. Will land in WP-0212 so this WP stays scoped to the editor.
- Behavioural changes inside any stage's content. Buttons, handlers, and state stay identical to today; this WP is layout/relocation only.
- Decomposition of `SubtitleEditorPage.tsx` into per-stage modules. The 11k-line component remains; the inside is reorganized but not split.
- Snapshot save-path fix for the installed build (V2 from WP-0210 audit) - lands separately as WP-0214.

## Acceptance criteria

- The localization editor renders as one outer panel: item header strip + two-column body. No inner `<div className="card">` borders inside the body except the optional sub-section dividers.
- Exactly one stage's content is visible in the right pane at any time. Clicking a stage in the left rail switches it.
- The left rail shows an item header at the top and eight stage rows, each with: stage index, title, status chip (`Done` / `Running` / `Failed` / `Needs attention`).
- Default stage on item open is the first non-`Done` stage; falls back to Captions when everything is `Done`.
- All existing scroll anchors still resolve to a visible region inside the right pane (when the bridge requests `section_id=loc-track`, the editor selects the matching stage and the anchor is in view).
- Pure-redundancy cards (First Dub Guide, Reusable Tools, Advanced Tools) are gone from the rendered output.
- `cargo check` and desktop `npm run build` pass.

## Test / verification plan

- Capture a baseline snapshot via `/agent/snapshot` of the Queen item under `governance/snapshots/WP-0211/baseline_*` before the diff lands (already captured under `WP-0211_visual_audit/`).
- After the diff lands and a 0.1.13 build, capture `governance/snapshots/WP-0211/after_*` for the same item with `selectedStage` set to each of the eight stages, plus a paired `/agent/dump` for each. Verify mounted_section_ids changes correctly and only one stage content surface is mounted at a time.
- Manual: drive Start/Continue localization run, confirm each stage row's chip transitions and the right pane reflects current state.
- Manual: trigger an `/agent/navigate` with a `section_id` for an old anchor (e.g. `loc-voice-basics`) and confirm the editor selects the Voice plan stage.

## Risks / open questions

- Risk: the existing render tree is interleaved (some controls share state across the loc-track and loc-voice-basics regions). Mitigation: the first slice of this WP wraps existing sections in stage conditionals without splitting their content; later slices simplify each stage's content.
- Risk: scroll memory across stages. Mitigation: each stage's right-pane scroll position resets on switch by design; if operators want to keep position, follow-up.
- Open: keyboard shortcut Ctrl+1..5 currently jumps to fixed sections. After this WP it should select stages instead. Will rebind: 1=Captions, 2=Translate, 3=Speakers, 4=Voice plan, 5=Dub, 6=Mix, 7=Mux, 8=Files.

## Status updates

- 2026-04-26: Created. Implementation pass started.
