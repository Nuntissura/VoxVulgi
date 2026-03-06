# Work Packet: WP-0078 - Multi-reference cloning

## Metadata
- ID: WP-0078
- Owner: Codex
- Status: BACKLOG
- Created: 2026-03-06
- Target milestone: Phase 2 quality hardening

## Intent

- What: Allow each reusable template speaker to own multiple clean reference clips instead of a single clip.
- Why: Voice stability and similarity usually improve when the clone path can draw from several reference examples rather than one short sample.

## Scope

In scope:

- Support 3-10 reference clips per template speaker.
- Preserve clip order, labels, and quality notes.
- Let operators add, remove, reorder, and reveal individual references.
- Keep a safe single-reference fallback path for older templates.

Out of scope:

- Automatic external dataset ingestion.
- Retraining custom models from large corpora.

## Acceptance criteria

- Existing single-reference templates continue to work unchanged.
- New templates can store multiple references per speaker in app-managed storage.
- Voice-preserving jobs can consume a multi-reference speaker profile.

## Test / verification plan

- DB migration tests.
- Engine tests for persistence and backward compatibility.
- Manual smoke comparing single-reference vs multi-reference results.

## Status updates

- 2026-03-06: Created from the voice-cloning quality backlog after reusable template support landed.
