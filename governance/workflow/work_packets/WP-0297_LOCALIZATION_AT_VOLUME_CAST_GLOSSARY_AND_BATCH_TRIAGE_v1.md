# Work Packet: WP-0297 — Localization at Volume: Cast, Glossary Scope and Batch Triage (v1)

## Metadata
- ID: WP-0297
- Owner: assistant (implementation) + operator (acceptance)
- Status: BACKLOG
- Created: 2026-08-05 (from review gaps G8, G9, G10, G11)
- Depends on: WP-0290 (stage mounts), WP-0291 (cast write-back from the speaker surface)
- Target milestone: Localization core recovery

## Intent
- What: Make localizing **50 videos of one channel** bearable — reuse the cast, reuse the glossary,
  reuse mix settings, and triage the batch from one list instead of opening 50 editors.
- Why: Every one of WP-0290..0294 is scoped to a single item. The review's high-volume lens found
  that nothing carries across items, which is the dominant cost for the operator's actual library
  (122k items, subscription-driven).

## Scope

### 1. Cast reuse across items (G7/G8 backend already exists)
The plumbing is built and unused from the speaker surface: `voice_templates_create_from_item` /
`_apply_to_item` (`SubtitleEditorPage.tsx` L4487, L4642), `voice_cast_packs_create_from_template` /
`_apply_to_item` (L4759, L4826), voice-plan default promotion (L4677, L4861), and `cast_pack_id`
already rides the batch request (L5333). Surface **Save this cast** / **Use saved cast** where the
labelling happens (WP-0291's speaker surface), so a channel's recurring speakers are labelled once.

### 2. Series/channel-scoped glossary (G11)
The glossary is per item (L7213) with CSV import/export as the only sharing mechanism (help L1104),
so recurring names across a channel must be re-imported item by item. This directly degrades
translation quality at volume, because the landscape's `translation-ja-ko-en` topic makes the glossary
block a first-class part of the LLM translation prompt. Add a glossary scope above the item —
per channel/subscription or per named series — that merges into the item glossary at translate time,
with the item still able to override.

### 3. Reusable mix/timing preset (G8)
WP-0294 scopes presets per item. Add save-as-default and apply-to-batch. **Prerequisite**: the auto
pipeline currently discards mix and timing params entirely (`jobs.rs` L8101, L8334, L11977, L12278,
L12366, L12778, L21412), so a preset would have no effect until WP-0294 threads them through.

### 4. Batch dubbing surfaced + batch triage list (G9, G10)
`queueLocalizationBatch` / `jobs_enqueue_localization_batch_v1` (L5320-5339) already exists but is
buried in a `<details>` labelled "Advanced tools" at L9168 — physically inside the `captions`-stage
card, while its own help-map entries claim `dub` and `files` (L1162, L1170). Promote it, and add a
**batch triage table**: item, speakers cloned vs fell back, QC fails, dub status, open. Selection plus
one toolbar; a table, not per-item cards.

## Acceptance criteria
1. A cast saved from one item can be applied to another item of the same channel without re-labelling.
2. A glossary entry defined at channel/series scope reaches the translation prompt for every item in
   that scope, and an item-level entry still wins.
3. A saved mix/timing preset applies to an automatic batch run, verified in the job params.
4. Batch dubbing is reachable without opening an "Advanced tools" disclosure inside an unrelated stage.
5. The triage table shows, for a batch, which items fell back to standard TTS and which have QC fails,
   without opening each editor.
6. No new cards; `tsc --noEmit` exit 0.

## Risks / open questions
- **Cast mis-application** across items with different speakers would assign the wrong voice to the
  wrong person — worse than no reuse. Mitigation: apply by explicit operator action with a preview of
  the mapping, never automatically on import.
- Scope creep into a full "series" data model. Mitigation: scope keys off the existing subscription /
  channel identity already in the library rather than inventing a new entity.
- Open: should cast application be offered automatically when a new item arrives from a subscription
  whose cast is known? Powerful at volume, risky if wrong. Proposed: suggest, never auto-apply.

## Status updates
- 2026-08-05: Created from review gaps G8-G11 (high-volume lens) — none of WP-0290..0294 carried
  anything across items.
