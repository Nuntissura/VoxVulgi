# Work Packet: WP-0290 — Fill the Empty Speakers and Dub Stages (v1)

> **v1 rewrite 2026-08-05.** The first draft of this packet proposed adding Single-speaker /
> Multi-speaker tabs on the premise that the Localization Studio was "a 10,686-line page with nine
> flat sections and no tabs". **That premise was false and the tab proposal is withdrawn.** Review
> (subagent, 2026-08-05) and direct verification found a workspace shell with a stage rail already
> shipped. Corrected scope below.

## Metadata
- ID: WP-0290
- Owner: assistant (implementation) + operator (UX acceptance)
- Status: BACKLOG
- Created: 2026-08-05 (rewritten same day after review)
- Depends on: WP-0289 MT-11 (MKV artifact) for Outputs/App wording
- Blocks: WP-0291 (mounts in `speakers`), WP-0292 (roster mounts in `dub`), WP-0293 (segment table mounts in `dub`), WP-0294 (mixer mounts in `mix`)
- Target milestone: Localization core recovery

## Verified current state (2026-08-05)
- `SubtitleEditorPage.tsx` L1137-1146 defines `WORKSPACE_STAGES`: **8 stages** —
  `captions, translate, speakers, voice_plan, dub, mix, mux, files` — rendered as a left rail inside
  a `loc-workspace` shell. `App.css` L2207-2217 shows only the card whose `data-stage` matches the
  selected stage, and L2222-2228 already strips card chrome inside the workspace.
- **Two of the eight stages render nothing.** Counting `data-stage` attributes in the page:
  `captions` 2, `files` 2, `mux` 2, `mix` 1, `translate` 1, `voice_plan` 1 — and **zero** for
  `speakers` and `dub`. Selecting either shows only a dashed hint (L6494-6510) reading
  *"A dedicated Speakers surface (chips per speaker, diarization preview) is planned for a follow-up
  WP"* and *"A dedicated Dub surface is planned for a follow-up WP."*
- Speaker assignment is **already a `<select>`** (L10729-10740) populated from `speakersInTrack`,
  with a text input only for the `__new__` case. Merge is two `<select>`s (L10773-10795).
- The page contains **23 mojibake (double-encoded UTF-8) strings**, including in the built-in help
  map: L1009, L1104, L1239, and visible UI at L7185, L7310, L7945, L10734 (`New speakerÃ¢â‚¬Â¦`),
  L10776, L10783, L10789, L11043, L11057.

## Intent
- What: Fill the two empty rail stages (`speakers`, `dub`) so the Studio has no dead stage, and
  establish the mount points the other four packets need. **No new navigation axis is added.**
- Why: A user walking the rail top-to-bottom hits two apologies out of eight steps. Single- vs
  multi-speaker is a property of the **data**, not a mode the user should have to choose: with one
  speaker the roster collapses to one row and the segment table has one colour. Adding a 2-way tab
  axis on top of 8 stages would create 16 states for a non-technical persona to model — strictly
  worse than today.

## Scope
- In scope:
  - Mount a `data-stage="speakers"` section (`id="loc-speakers"`) and a `data-stage="dub"` section
    (`id="loc-dub"`), replacing the two hint blocks; both render correctly for 1 or N speakers.
  - Reserve the mixer mount under `data-stage="mix"` for WP-0294.
  - Move mix controls (`mixDuckingStrength`, `mixLoudnessTargetLufs`) out of "Advanced audio/video"
    inside the **Captions** card (L7410-7450) — they are mix controls filed under captions today —
    into the Mix stage.
  - Promote batch dubbing out of the `<details>` "Advanced tools" block at L9168 (which physically
    sits inside the `captions`-stage card while its help map entry claims `dub`, L1162/L1170).
  - **Reduce** card count per `build_rules.md` L45: remove the now-dead `card` class from sections
    inside the workspace, whose chrome `App.css` already neutralizes.
  - Active hardware-tier chip (GPU vs CPU-only) in the workspace header strip, per PRODUCT_SPEC 8.1.9.
  - Honest "not analysed yet" state for an item with no speaker keys.
  - Fix the 23 mojibake strings, including the three in the built-in help map (GLOBAL-BUILD-002..011
    requires a no-context model to be able to read that manual).
  - Fix stale "preview MP4 deliverable" wording in `App.tsx` L391 against WP-0289 MT-11.
- Out of scope:
  - Single/Multi tabs — withdrawn, see header note.
  - The timeline, roster, sync table and mixer contents themselves (WP-0291/0292/0293/0294).
  - Rewriting `SubtitleEditorPage.tsx`.

## Acceptance criteria
1. No rail stage renders an apology or an empty pane; all 8 stages show real content.
2. `speakers` and `dub` sections exist with stable `loc-*` ids and correct `data-stage` values.
3. Card count on the Studio surface is **lower** than before the change (build_rules.md L45), not merely unchanged.
4. Zero mojibake strings remain in `SubtitleEditorPage.tsx`.
5. The active hardware tier is stated in operator language in the header strip.
6. An item with no speaker keys shows "not analysed yet — run Detect speakers", not a silent default.
7. `/agent/ui_audit` in `--agent-headless` reports every stage reachable with accessible names and zero controls missing names.
8. `tsc --noEmit` exit 0; desktop contract tests pass.

## Test / verification plan
- App-boundary: headless bridge walk of all 8 stages, `/agent/snapshot` + `/agent/dump` per stage; every snapshot inspected (GLOBAL-INSPECT), checking readability, no overlapping text, visible state.
- Contract test: every `WORKSPACE_STAGES` id has at least one matching `data-stage` section.
- Encoding: a check asserting no `Ã¢â‚¬` sequences remain.

## Risks / open questions
- **Merge surface**: 11,119-line file (10,686 non-blank). Mitigation: add sections and move by reference; do not rewrite.
- **Card removal changes layout subtly**: mitigated by the fact that `App.css` L2222-2228 already neutralizes card chrome inside the workspace, so removal should be visually inert — but must be snapshot-verified, not assumed.
- Open: does `voice_plan` remain a separate stage once `speakers` and `dub` are real, or does it fold into them? Deferred until 0291/0292 land, then re-check for redundancy.

## Status updates
- 2026-08-05: WP created from operator request for single/multi-speaker tabs.
- 2026-08-05: **Rewritten after review.** Tab proposal withdrawn — the premise ("no tab shell") was factually wrong; a stage rail already ships. Repointed at the real defect the review surfaced: `speakers` and `dub` are empty stages whose own hint text says a follow-up WP is expected. Absorbed review gaps G1, G3, G5, G6, G9, G22, G24.
