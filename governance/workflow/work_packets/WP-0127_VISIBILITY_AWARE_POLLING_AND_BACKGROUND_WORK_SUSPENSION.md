# Work Packet: WP-0127 - Visibility-aware polling and background work suspension

## Metadata
- ID: WP-0127
- Owner: Codex
- Status: BACKLOG
- Created: 2026-03-08
- Target milestone: Audit remediation tranche

## Intent

- What: Make mounted-but-hidden pages and recurring timers visibility-aware, and consolidate job-status polling so background work scales gracefully under contention.
- Why: `WP-0121` found unnecessary background polling and always-on timers that keep competing with the foreground page.

## Scope

In scope:

- Active/visible page contracts across the desktop shell.
- Suspending page-local polling when hidden or minimized.
- Consolidated job-status polling/store or event-driven updates.
- Adaptive handling for app-level heartbeats and samplers.

Out of scope:

- Deep code-structure decomposition unrelated to polling/visibility.

## Acceptance criteria

- Hidden pages stop active polling/background refresh.
- Job status is no longer polled independently by each localization sub-flow.
- App-level recurring timers are visibility-aware or operator-configurable.

## Test / verification plan

- Desktop behavioral verification plus focused unit/contract tests where possible.
- Proof bundle documenting reduced polling paths and updated page lifecycle behavior.

## Status updates

- 2026-03-08: Created from `WP-0121` contention-tolerant performance findings.
