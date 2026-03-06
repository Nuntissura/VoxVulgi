# Work Packet: WP-0093 - Reference cleanup integrity for multi-reference voices

## Metadata
- ID: WP-0093
- Owner: Codex
- Status: IN_PROGRESS
- Created: 2026-03-06
- Target milestone: Phase 3 voice-workflow hardening

## Intent

- What: Make reference cleanup non-destructive and multi-reference aware.
- Why: Cleanup currently behaves like a single-reference shortcut, which can discard extra references, sever profile provenance, and collide speaker artifact folders when speaker keys sanitize to the same slug.

## Scope

In scope:

- Let operators choose which current reference clip to clean when multiple references exist.
- Preserve existing reference sets when applying a cleaned result unless the operator explicitly requests a narrower override.
- Keep speaker/profile provenance visible and avoid unnecessary clearing of reusable profile linkage.
- Store cleanup artifacts in collision-safe per-speaker folders while remaining backward-compatible with already-generated cleanup records.

Out of scope:

- Full spectral/audio-restoration tooling beyond current cleanup filters.
- Automatic background speaker isolation or diarization changes.

## Acceptance criteria

- Cleanup can target a chosen reference clip on multi-reference speakers.
- Applying a cleaned result no longer silently collapses the speaker to a single reference.
- Existing cleanup history remains readable after the storage key change.
- Distinct speaker keys do not share the same cleanup folder unless they are actually identical.

## Test / verification plan

- Engine tests for cleanup storage keying/backward compatibility.
- Desktop build.
- Manual multi-reference cleanup smoke notes captured in proof summary.

## Status updates

- 2026-03-06: Created from remediation review after finding destructive single-reference cleanup behavior and cleanup-folder key collisions.
