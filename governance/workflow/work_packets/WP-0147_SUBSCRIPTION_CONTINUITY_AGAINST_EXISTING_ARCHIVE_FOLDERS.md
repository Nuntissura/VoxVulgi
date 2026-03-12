# Work Packet: WP-0147 - Subscription continuity against existing archive folders

## Metadata
- ID: WP-0147
- Owner: Codex
- Status: BACKLOG
- Created: 2026-03-12
- Target milestone: Post-0.1.6 smoke regression recovery

## Intent

- What: Preserve per-subscription archive continuity by reconciling existing mapped folders and queueing only missing items when subscription refresh is restored.
- Why: The operator has successfully migrated legacy subscriptions and NAS-backed output folders into VoxVulgi, and future refreshes must continue from that existing per-subscription state instead of starting over or ignoring already-downloaded media.

## Scope

In scope:

- Reconcile an existing subscription's mapped output folder with VoxVulgi's subscription refresh logic.
- Seed or refresh dedupe/index state from already-downloaded media inside the subscription folder before queueing new items where practical.
- Preserve per-subscription output-folder mappings already migrated from the legacy archive.
- Clarify operator behavior for subscription resume/continuation against existing folders.

Out of scope:

- Modifying, moving, renaming, or deleting files in legacy NAS-backed folders.
- Full archive reorganization.

## Acceptance criteria

- A subscription can continue using its current mapped folder instead of forcing a new folder path.
- Refresh logic reconciles already-downloaded items and queues only missing media where practical.
- Existing NAS-backed subscription folders remain read-only from the reconciliation side of the workflow.
- Operator-facing behavior makes it clear that VoxVulgi is continuing from prior archive state rather than starting a fresh archive.

## Test / verification plan

- Engine/library verification against a bounded existing-folder fixture or migrated live sample.
- Desktop smoke on one migrated subscription with an existing mapped folder.
- Proof bundle documenting the reconciliation path and resulting queue behavior.

## Status updates

- 2026-03-12: Created from smoke note attached to `ST-019`.
