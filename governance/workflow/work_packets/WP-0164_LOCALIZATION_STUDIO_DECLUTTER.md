# Work Packet: WP-0164 - Localization Studio Declutter

## Metadata
- ID: WP-0164
- Owner: Codex
- Status: REVIEW
- Created: 2026-04-08
- Target milestone: UX Polish

## Intent

- What: Reduce visual density and improve progressive disclosure on the Localization Studio first screen.
- Why: The Workflow Map card has 12+ navigation buttons in one row. Reusable Voice Basics has 7 form fields + 14 action buttons. Non-technical users cannot find the primary workflow path. The spec requires the first screen to be "a true operator dashboard" (Section 7).

## Scope

In scope:
- Group Workflow Map buttons into 3-4 collapsible categories (Captions, Voice/Dub, Quality/Review, Advanced) instead of one flat row.
- Simplify Reusable Voice Basics into a 3-step progress flow: Prepare voice samples → Save voice → Apply to item.
- Merge the 4 ref-related buttons (Generate/Reload/Use/Choose) into a single "Prepare voice samples" action with sub-options.
- Rename "dub truth" to "Clone status" in the operator-facing UI.
- Remove the "Advanced Tools" card that exists because controls were "easy to miss" — integrate critical ones into the workflow, keep the rest in section navigation.

Out of scope:
- Backend changes to voice clone pipeline.
- Adding new voice clone features.

## Acceptance criteria
- Workflow Map buttons are grouped into collapsible categories, not a single flat row.
- Reusable Voice Basics shows a clear step indicator (Prepare → Save → Apply).
- Ref-related actions are consolidated into fewer entry points.
- First screen is scannable in under 5 seconds for a non-technical operator.

## Test / verification plan
- Visual snapshot audit at 1400x900 and 800x600 showing grouped buttons and step indicator.
- `npm run build` passes.
