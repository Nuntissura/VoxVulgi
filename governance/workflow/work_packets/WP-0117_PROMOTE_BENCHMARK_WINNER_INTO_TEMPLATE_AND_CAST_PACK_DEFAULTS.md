# Work Packet: WP-0117 - Promote benchmark winner into template and cast-pack defaults

## Metadata
- ID: WP-0117
- Owner: Codex
- Status: DONE
- Created: 2026-03-08
- Target milestone: Voice backend modernization

## Intent

- What: Let operators promote a benchmark winner directly into reusable voice-template or cast-pack defaults, not only into the current item plan.
- Why: Once a backend/variant wins for a recurring show format, that decision should become reusable across later episodes without redoing the same manual promotion every time.

## Scope

In scope:

- Add reusable backend-default fields to voice-template and cast-pack workflows.
- Allow Localization Studio to promote a benchmark candidate into the selected template or cast pack.
- Reuse those defaults when applying the template/cast pack back to a new item.
- Keep the promotion explicit and operator-directed.

Out of scope:

- Automatic mutation of every existing template or cast pack.
- Automatic replacement of the global managed backend default.

## Acceptance criteria

- Operators can promote a benchmark winner into a selected reusable voice template and/or cast pack.
- Reusable template/cast-pack defaults persist and reload durably.
- Applying the reusable asset to a new item can also seed the item voice plan/backend preference from that saved default.
- Promotion remains explicit and reversible through normal template/cast-pack editing.

## Test / verification plan

- Rust tests for persistence and apply-time propagation of reusable backend defaults.
- Desktop build.
- Tauri/engine tests for promotion commands and template/cast-pack reload flows.

## Status updates

- 2026-03-08: Created from the research-driven operational backend tranche.
- 2026-03-08: Implemented durable backend-default metadata on reusable voice templates and cast packs, added direct benchmark-winner promotion from Localization Studio into the selected reusable asset, and let apply flows optionally seed the destination item voice plan from that saved default.
- 2026-03-08: Verified with `cargo test -q --manifest-path product/engine/Cargo.toml`, `cargo test -q --manifest-path product/desktop/src-tauri/Cargo.toml`, and `npm run build` in `product/desktop`; proof in `product/desktop/Build Target/tool_artifacts/wp_runs/WP-0117/20260308_161136/`.
