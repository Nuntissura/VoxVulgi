# Work Packet: WP-0205 - Localization home dashboard simplification

## Metadata
- ID: WP-0205
- Owner: Codex
- Status: IN_PROGRESS
- Created: 2026-04-25
- Target milestone: Localization operator usability
- Supersedes direction of: WP-0153, WP-0154

## Intent

- What: Remove the `Now / Next / Last Output` orientation grid from the Localization Studio home, keep the hero/summary/recent-items layer that actually answers "what should I do next", and trim the home polling cost so opening Localization Studio does not spike IPC.
- Why: Operator feedback is that the three orientation cards are clutter, not orientation. They duplicate state that is already shown in the hero summary tiles (`Workspace items / Runs active / Previews ready / Need next step`) and the recent-items list, while adding their own data fetches and re-render surface. WP-0154 explicitly added this layer and is still IN_PROGRESS pending operator validation - that validation has now come back negative, so the layer is being retired before WP-0154 closes.

## Scope

In scope:
- Delete the `loc-home-orientation-grid` block in `App.tsx` `LocalizationStudioHome` (the `Now`, `Next`, `Last Output` focus cards).
- Keep the hero card, summary tiles, recent-items list, drag-and-drop overlay, and pending-import notice intact - those are not what the user objected to.
- Reduce home polling cost: collapse the per-item double-IPC pattern (`item_outputs` + `jobs_list_for_item` per item) to one batched call where possible, or at minimum stop refreshing per-item statuses when nothing is running and no import is pending. The current loop fires every 2.5s with up to 24 IPC calls per tick, which is a real freeze contributor under heavy host CPU load.
- Update `governance/spec/PRODUCT_SPEC.md` and `governance/spec/TECHNICAL_DESIGN.md` localization-home contract to drop the three-card orientation requirement; move WP-0154 to superseded with a status-update note pointing here.

Out of scope:
- The Workflow panel restructure (covered by WP-0207).
- Editor-side `SubtitleEditorPage.tsx` decomposition (covered by WP-0208).
- Any change to the engine `item_outputs` / `jobs_list_for_item` contract beyond what is needed for the batched home call. If a batched command is added, it is purely additive.

## Acceptance criteria

- Localization Studio home no longer renders the `Now / Next / Last Output` cards.
- Hero summary, recent-items list, drag-and-drop import, and pending-import handoff still work end to end.
- Home loop no longer dispatches more than 1 IPC per visible item per refresh, and pauses entirely when no item is running and no import is pending.
- Spec/design wording for the localization home matches the new layout; WP-0154 row in `TASK_BOARD.md` is marked `SUPERSEDED` with a reference to WP-0205.

## Test / verification plan

- Manual: open Localization Studio with several recent items, confirm the orientation grid is gone and the hero/recent layer still surfaces current item, runs active, previews ready, needs next step.
- Manual: queue a localization import, confirm pending-import state and handoff notice still update.
- Manual: with one item running, confirm the home polling stays bounded (no per-tick burst) by inspecting Diagnostics trace timeline.
- `cargo check` (engine) and desktop `npm run build`.

## Risks / open questions

- Risk: removing the orientation cards must not break the `nextAction` logic that the rest of the home depends on - but `nextAction` was an internal computed value used only by the deleted block, so it can be removed alongside.
- Open: do we keep the `currentHomeItem` "Continue current item" affordance in the hero card, or do we lean only on the recent-items list? For this WP we keep the hero affordance; if it still feels cluttered we revisit in a follow-up.

## Status updates

- 2026-04-25: Created. Operator feedback retired the WP-0154 orientation-cards direction. Implementation pass started.
