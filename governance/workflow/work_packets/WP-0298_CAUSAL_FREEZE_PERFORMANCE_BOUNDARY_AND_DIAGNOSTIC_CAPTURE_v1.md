---
file_id: WP-0298-v1
file_kind: work-packet
updated_at: 2026-08-09
---

<topic id="contract" status="backlog" version="v1" wp="WP-0298" updated_at="2026-08-09">

# Work Packet: WP-0298 — Causal freeze performance boundary and diagnostic capture

## Metadata

- ID: WP-0298
- Owner: —
- Status: BACKLOG
- Created: 2026-08-09
- Refinement: `WP-0298_CAUSAL_FREEZE_PERFORMANCE_BOUNDARY_AND_DIAGNOSTIC_CAPTURE_v1_REFINEMENT.md`
- Board: `../TASK_BOARD.md#wp-0298`
- Dependencies: WP-0221, WP-0242, WP-0250, WP-0278, WP-0280

## Intent

Make job startup and panel navigation responsive on the exact large local database/NAS environment, and make every remaining freeze attributable to a measured UI, bridge, database, storage, child-process, WebView2, or host-pressure phase.

## Base scope

- Implement the refinement's incident/span contract, bounded diagnostic storage, persistent media observations, activity/archive rollups, stale-request guards, Diagnostics capture controls, and `vvwatch` correlation.
- Reproduce and remediate the exact operator job-start and panel-switch cases using the current canonical database and configured NAS root.
- Preserve all library metadata, subscriptions, playlists, jobs, media, and third-party databases.

## Required implementation order

1. RED fixtures and incident/span schema.
2. Bounded trace writer and operator capture surface.
3. Persistent media observations and derived rollups.
4. Panel request cancellation/guards and phase timing.
5. `vvwatch`/WebView2 correlation.
6. Exact live reproduction, measured remediation, governed build, and proof.

## Acceptance and proof

- Every acceptance criterion, red-team control, and verification requirement in the refinement is normative.
- The packet cannot be `DONE` from synthetic tests, build-only proof, or missing Worker events.
- Exact current-case proof must state target, expected condition, observed condition, and capture/query/validator.
- `governance/workflow/PROOF_STANDARD.md` and `build_rules.md` apply.

</topic>

<topic id="status-updates" status="active" version="v1" wp="WP-0298" updated_at="2026-08-09">

# Status updates

- 2026-08-09: Created from direct source, live read-only database, trace, existing proof, and current external diagnostic research. No product code, user data, queue state, NAS media, or running process changed.

</topic>
