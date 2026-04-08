# Work Packet: WP-0166 - Terminology and Help Text Cleanup

## Metadata
- ID: WP-0166
- Owner: Codex
- Status: REVIEW
- Created: 2026-04-08
- Target milestone: UX Polish

## Intent

- What: Replace technical jargon with plain language across all pages and add help text where non-technical users need guidance.
- Why: The app uses developer/ML terminology that non-technical operators won't understand. Examples: "ASR lang", "refs", "voice memory profile", "flush cache", "hydration", "reconciliation", "dub truth". The spec requires the app to be "Discoverable" and that "operator-critical controls must be visible in the workflow where they are needed" (Section 7).

## Scope

In scope:
- Rename the following labels across the app:

| Current | Replacement |
|---------|------------|
| ASR lang | Source language |
| refs / reference candidates | Voice samples |
| voice memory profile | Saved voice |
| Flush cache/history | Clean up old jobs and logs |
| Enqueue dummy job | Run test job |
| Tool lifecycle model | Component status |
| Startup hydration | Initialization |
| Phase 2 packs (one-click) | Voice cloning packages (optional) |
| dub truth | Clone status |
| Reconciliation | Archive import |
| Bundled / Hydrated / Available | Included / Ready / Installed |

- Add placeholder text or inline help to:
  - YouTube cookie auth textarea (explain how to export cookies, link format)
  - Instagram auth session fields (which method to use)
  - Image archive crawl settings (what cross-domain and content links mean)
  - Download preset fields (what each option does)
- Add tooltips or (?) icons on Options page for "Override" vs "Using base root default".

Out of scope:
- Changing any backend behavior or data models (label-only changes).
- Adding a full help/documentation system.

## Acceptance criteria
- All listed jargon terms are replaced in the UI.
- YouTube cookie textarea has clear placeholder/help text.
- Archive crawl settings have inline explanations.
- `npm run build` passes.
- Localization keys updated where applicable.

## Test / verification plan
- Grep for old terms to confirm none remain in rendered UI text.
- Visual snapshot of Options, Jobs, Diagnostics pages showing new labels.
