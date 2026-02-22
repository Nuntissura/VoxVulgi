# Work Packet: WP-0003 - downloader design

## Metadata
- ID: WP-0003
- Owner: Codex
- Status: DONE
- Created: 2026-02-19
- Target milestone: Phase 1 (MVP)

## Intent

- What: Define a downloading scope, provider boundaries, and UX requirements.
- Why: Downloading is a high-risk surface. We need a clear spec before implementation.

## Scope

In scope:

- Define allowed ingestion modes:
  - local import (always allowed)

Out of scope:

- Implementing any specific provider.

## Acceptance criteria

- `governance/spec/` has a short design note (or an added section in the existing technical design) that:
  - lists supported ingestion modes and exclusions
  - defines the provider interface at a high level
  - defines provenance schema fields
  - defines required UX confirmations and diagnostics visibility

## Implementation notes

- Prefer a "provider registry" where the app ships with curated provider list initially.
- Default MVP can be "local import only" if specific providers are uncertain.

## Test / verification plan

- Desk review the doc against the project's guardrails in `PROJECT_CODEX.md`.

## Risks / open questions

- Which services/providers are acceptable for an MVP (and in which jurisdictions)?
- How to handle user authentication flows without persisting sensitive cookies/tokens.

## Status updates

- 2026-02-21:
  - Backfilled downloader design into `governance/spec/TECHNICAL_DESIGN.md` (provider interface, provenance schema, and UX/safety requirements).
  - Follow-up hardening work tracked in `governance/workflow/work_packets/WP-0017_PHASE1_DOWNLOADER_PRIVACY_HARDENING.md`.


