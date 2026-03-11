# Work Packet: WP-0101 - Artifact and output open-action parity

## Metadata
- ID: WP-0101
- Owner: Codex
- Status: DONE
- Created: 2026-03-07
- Target milestone: Workflow and archive UX hardening

## Intent

- What: Make file and parent-folder open actions consistently available wherever VoxVulgi creates a deliverable or artifact.
- Why: Operators should not need to guess where outputs landed or which window exposes the only working reveal/open action.

## Scope

In scope:

- Standardize `Open file` and `Open parent folder` actions across library, jobs, localization, archiver, diagnostics, and artifact surfaces wherever a valid path exists.
- Reuse one shared open-path policy and error model across these surfaces.
- Keep path-open failures actionable with clear fallback or copy-path behavior.

Out of scope:

- Storage-root redesign beyond what is covered by `WP-0097`.
- New artifact classes unrelated to current outputs.

## Acceptance criteria

- Every user-facing artifact or deliverable surface exposes the appropriate open/reveal action when a valid path exists.
- Missing or invalid paths fail with the same actionable message pattern across windows.
- Operators no longer need to switch to a different window just to reveal a known output.

## Test / verification plan

- Desktop build.
- Manual smoke covering at least Library, Localization Studio, Jobs/Queue, and Diagnostics.

## Status updates

- 2026-03-07: Created from operator feedback requesting uniform open-file and open-folder actions for all produced artifacts and outputs.
- 2026-03-07: Implemented shared backend shell open/reveal commands and routed major output surfaces through them, including Library, Jobs, Diagnostics, and Localization Studio. Verified with `npm run build`, `cargo test -q` in `product/desktop/src-tauri`, and `cargo test -q` in `product/engine`. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0101/20260307_033244/summary.md`.
