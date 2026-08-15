# Work Packet: WP-0192 - Localization Studio large-library load containment and first-open stabilization

## Metadata
- ID: WP-0192
- Owner: Codex
- Status: DONE
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
- 2026-04-23: Implemented the first containment pass in `SubtitleEditorPage`: the page no longer hydrates the full library on first open, advanced voice data shifts into a deferred background load, and batch-library selection becomes explicit via a manual `Load items` action. `npm run build` and `cargo check` passed.
- 2026-08-15: Closed against packaged v0.1.168 with a disposable database containing 122,001 `library_item` rows and 122,001 Localization workspace rows. The headless bridge became healthy in 1,268 ms and reported the exact editor item in 1,500 ms. The packaged editor rendered the current item, source track state, workflow stages, and enabled primary quick actions while the full workspace remained behind the explicit `Load items` action; `Select all listed` stayed disabled before that load. Focused desktop contracts passed 47/47. Evidence: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0192/20260815-114612-v0_1_168/summary.md`.
