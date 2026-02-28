# Work Packet: WP-0062 - Subscription groups/tags + failure backoff (large subscription sets)

## Metadata
- ID: WP-0062
- Owner: Codex
- Status: BACKLOG
- Created: 2026-02-28
- Target milestone: Phase 1 (subscriptions hardening)

## Intent

- What: Add **subscription grouping** (folders/tags) and robust **failure backoff** so large subscription libraries can be managed safely.
- Why: With hundreds of subscriptions, one failing source should not destabilize the system; users need batch actions and organization tools.

## Scope

In scope:

- Data model:
  - subscription groups (name, created/updated),
  - membership mapping (subscription_id <-> group_id),
  - per-subscription failure state (last_error_at, consecutive_failures, next_allowed_refresh_at).
- Engine:
  - queue refresh for one group or all active,
  - exponential backoff on repeated failures,
  - per-subscription “pause” without deleting it.
- UI:
  - create/rename/delete groups (deleting a group must not delete subscriptions),
  - assign/unassign subscriptions to groups,
  - filter subscription list by group.

Out of scope:

- Provider-level auth improvements (separate WP).
- Advanced scheduling rules beyond backoff + interval gating.

## Acceptance criteria

- Users can organize subscriptions into groups and queue refresh per group.
- Repeated failures back off automatically and do not spam the queue.
- No subscription list is deleted as a side-effect of group operations.

## Test / verification plan

- `cargo test` in `product/engine`
- `cargo test` in `product/desktop/src-tauri`
- `npm -C product/desktop run build`
- Manual smoke:
  - create group, assign subs, queue group, observe backoff after forced failures.

## Status updates

- 2026-02-28: Created.

