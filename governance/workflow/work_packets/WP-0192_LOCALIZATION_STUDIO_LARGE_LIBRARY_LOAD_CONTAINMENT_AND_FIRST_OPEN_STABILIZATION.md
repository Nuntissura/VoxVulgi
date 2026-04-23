# Work Packet: WP-0192 - Localization Studio large-library load containment and first-open stabilization

## Metadata
- ID: WP-0192
- Owner: Codex
- Status: IN_PROGRESS
- Created: 2026-04-23
- Target milestone: Desktop stability and operator usability

## Intent

- What: Bound the initial Localization Studio load path so opening the page does not try to hydrate the full library and every advanced data source up front.
- Why: Operator smoke on a six-figure library (`122k+` items) showed the Localization surface freezing, flickering, and becoming unusable on first open.

## Scope

In scope:
- Remove or defer full-library hydration from the first Localization Studio open path.
- Keep the current item usable without requiring the entire media library, benchmark history, reusable voice assets, and advanced backend surfaces to resolve first.
- Stage expensive reads so the page becomes interactive before secondary panels finish loading.
- Add focused verification against a large existing library.

Out of scope:
- A full Localization Studio redesign.
- New voice or benchmark features unrelated to load containment.
- Large data migrations or destructive cleanup of existing libraries.

## Acceptance criteria
- Opening Localization Studio no longer hard-freezes the app on large libraries.
- The current item, current track, and primary run actions become interactive before secondary advanced surfaces finish loading.
- Large-library reads are bounded or explicitly deferred instead of loading the full library on first open.
- Desktop build verification passes, plus focused app-boundary smoke on a large library.

## Test / verification plan

- Read the current Localization open path and identify the unbounded loads.
- Add focused verification for large-library behavior where practical.
- Re-run desktop build verification and targeted operator smoke on the affected page.

## Risks / open questions

- Some advanced panels currently assume whole-library data is already resident and may need small fallback states.
- Partial loading must not break existing current-item actions or track selection.

## Status updates

- 2026-04-23: Created after operator smoke showed Localization Studio freezing and flickering on first open against a `122k+` item library.
