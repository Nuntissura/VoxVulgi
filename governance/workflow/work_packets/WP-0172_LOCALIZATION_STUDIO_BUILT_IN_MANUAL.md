# Work Packet: WP-0172 - Localization Studio Built-in Manual

## Metadata
- ID: WP-0172
- Owner: Codex
- Status: REVIEW
- Created: 2026-04-08
- Target milestone: UX Polish

## Intent

- What: Add an in-context help system to Localization Studio so every section has a (?) button that explains what it does, when to use it, and what order to go in.
- Why: Localization Studio has 22 editor sections, 30+ job-type buttons, and 6 voice management systems. The only current guidance is a 5-bullet "What Happens Here" and an 8-step "First Dub Guide". Non-technical operators opening this for the first time are lost by section 5. The spec requires the speaker-reference checkpoint to be "survivable for first-run operators" and reusable voice basics to be "obvious before advanced reusable asset layers."

## Scope

In scope:
- Add a `SectionHelp` component that renders a (?) icon button on each section header. Clicking it toggles a help panel below the heading with:
  - **What this does** — one sentence
  - **When to use it** — context in the workflow
  - **Typical workflow** — numbered steps (2-4)
  - **Key concepts** — brief glossary of terms used in this section
- Write help content for all 22 editor sections plus the home screen cards.
- Add a persistent "Show all help" toggle (localStorage) that expands every help panel at once for learning mode.
- Use plain language per WP-0166 terminology (voice samples, saved voice, clone status, etc).

Out of scope:
- Interactive tutorials or walkthroughs.
- Video guides.
- Changing any existing functionality or layout.

## Acceptance criteria
- Every section heading in SubtitleEditorPage has a (?) button.
- Clicking (?) shows contextual help for that section.
- "Show all help" toggle works from any section.
- Help text uses plain language, not jargon.
- `npm run build` passes.

## Test / verification plan
- Visual snapshot with help panels expanded.
- Read through all help text for accuracy against spec.
