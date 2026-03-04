# Work Packet: WP-0073 - Governance installer mode policy pointers

## Metadata
- ID: WP-0073
- Owner: Codex
- Status: DONE
- Created: 2026-03-04
- Target milestone: Stabilization sprint (governance alignment)

## Intent

- What: Add concise installer mode policy references to root governance/agent behavior docs.
- Why: Keep operator and agent guidance aligned with shipped NSIS UX labels without duplicating full spec content.

## Scope

In scope:

- Update `PROJECT_CODEX.md` with a short installer mode policy section and canonical spec links.
- Update `AGENTS.md` with explicit guidance to preserve installer mode labels and app-data behavior wording.
- Update `MODEL_BEHAVIOR.md` with a one-line reference to canonical installer policy in spec docs.
- Track this work in roadmap/task board.

Out of scope:

- Changing installer implementation, templates, or labels.
- Changing uninstall data retention behavior.

## Acceptance criteria

- Root docs explicitly reference the three maintenance labels: Update/Repair, Full reinstall, Uninstall.
- Canonical source remains in spec docs and is referenced by path.
- No conflicting wording across governance/agent docs.

## Test / verification plan

- Manual read-through of updated files for wording consistency.
- Search audit for three mode labels across root governance docs.

## Status updates

- 2026-03-04: Created.
- 2026-03-04: Updated root governance docs to reference canonical installer mode policy and preserve consistent labels.
