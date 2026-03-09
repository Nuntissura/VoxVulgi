# Work Packet: WP-0135 - Diagnostics loading UX and full state snapshot

## Metadata
- ID: WP-0135
- Owner: Codex
- Status: BACKLOG
- Created: 2026-03-09
- Target milestone: Installer smoke remediation tranche

## Intent

- What: Improve diagnostics and startup/loading UX with numeric progress, feature-gating feedback, and a richer app-state snapshot for operator support and LLM analysis.
- Why: Current installer smoke confirms that diagnostics is clearer than before but still lacks the loading affordances and full-state export needed for fast support and deep inspection.

## Scope

In scope:

- Numeric startup/loading progress and clearer percentages alongside phase labels.
- Feature-level blocked/loading UX when dependencies are still hydrating.
- A richer diagnostics snapshot/export of current app state, tool/model state, roots, queues, and major feature health.
- Operator-readable and LLM-friendly snapshot structure.

Out of scope:

- Full remote telemetry.

## Acceptance criteria

- Startup and tool-loading surfaces show explicit percentage/progress where available.
- Disabled or blocked actions explain that dependencies are still loading and point to current progress.
- Diagnostics export includes a coherent local snapshot of app state rather than fragmented isolated reports only.

## Test / verification plan

- Desktop build, focused Tauri snapshot tests, and app-boundary verification.
- Proof bundle with snapshot examples and loading states.

## Status updates

- 2026-03-09: Created from installer smoke findings `ST-023` and the follow-up diagnostics-state request.
