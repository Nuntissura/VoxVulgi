# Work Packet: WP-0005 — Diagnostics + log retention policy

## Metadata
- ID: WP-0005
- Owner: Codex
- Status: DONE
- Created: 2026-02-19
- Target milestone: Phase 1 (MVP)

## Intent

- What: Define a concrete logging/retention/redaction policy and diagnostics bundle contents.
- Why: The reverse-built reference app demonstrates how logs can explode in size; we want predictable storage and safe sharing.

## Scope

In scope:

- Define:
  - log format (structured JSON/JSONL)
  - redaction rules (tokens, cookies, full URLs with IDs, PII)
  - rotation limits (size, total cap, age)
  - crash/trace data policy (opt-in; what is stored; how long)
- Define diagnostics bundle format and default redactions.
- Define storage layout and cleanup semantics (cache vs library vs derived artifacts).

Out of scope:

- Building the diagnostics UI (tracked in WP-0013).

## Acceptance criteria

- `governance/spec/TECHNICAL_DESIGN.md` includes a concrete "Diagnostics & Observability" policy section with:
  - default caps
  - redaction rules
  - bundle contents
  - user-facing disclosure requirements

## Implementation notes

- Default to local-first privacy:
  - no telemetry by default
  - any outbound calls must be explicit and visible
- Provide an "export diagnostics" bundle that is safe to share by default.

## Test / verification plan

- Desk review the policy against `MODEL_BEHAVIOR.md` privacy requirements.

## Risks / open questions

- How to handle user-provided media paths (can leak PII) in diagnostics exports.
- How to balance debug usefulness vs privacy.

## Status updates

- 2026-02-22: Reconciled with implementation.
  - Policy is documented in `governance/spec/TECHNICAL_DESIGN.md` under "Diagnostics & Observability" (caps, redaction rules, bundle contents, disclosure).
  - Diagnostics UI + tooling is implemented in WP-0013.
