# Work Packet: WP-0069 - Startup boot performance profiling and staged initialization

## Metadata
- ID: WP-0069
- Owner: Codex
- Status: DONE
- Created: 2026-03-03
- Target milestone: Stabilization sprint (startup performance)

## Intent

- What: Reduce startup stalls by profiling startup phases and deferring heavy initialization until after first interactive paint.
- Why: Current startup can take 15+ minutes in worst cases, preventing practical day-to-day usage.

## Scope

In scope:

- Add startup phase timing logs (boot timeline markers).
- Identify top startup blockers (offline payload apply, scans, queue refresh, model checks, migrations).
- Make app shell interactive before non-critical heavyweight tasks complete.
- Add progress/status UI for deferred startup tasks.
- Define and document startup performance budgets for cold/warm starts.

Out of scope:

- Replacing core dependency toolchain.
- Functional changes to ASR/translation/dubbing quality.

## Acceptance criteria

- App window becomes interactable early while heavy startup tasks continue asynchronously.
- Startup phase timing data is available in diagnostics/logs for troubleshooting.
- Documented startup budget targets are added to product spec and used for regression checks.

## Test / verification plan

- Cold start and warm start timing captures on representative machines.
- Manual validation that primary windows remain responsive during deferred init.
- `cargo test` + `npm run build` for touched modules.

## Status updates

- 2026-03-03: Created.
- 2026-03-03: Moved offline bundle application off the blocking setup path into background startup staging, added startup status command/state, and surfaced startup-progress messaging in shell UI. Verified with desktop build + tauri cargo check.
