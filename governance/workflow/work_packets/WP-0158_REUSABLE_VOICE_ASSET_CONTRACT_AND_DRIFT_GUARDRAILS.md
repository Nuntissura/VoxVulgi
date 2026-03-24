# Work Packet: WP-0158 - Reusable voice asset contract and drift guardrails

## Metadata
- ID: WP-0158
- Owner: Codex
- Status: BACKLOG
- Created: 2026-03-24
- Target milestone: Educational-core voice-clone recovery

## Intent

- What: Reduce future drift across templates, cast packs, memory profiles, and character profiles by tightening their shared apply/persistence/runtime contract and adding more explicit regression guardrails.
- Why: The repo has multiple reusable-voice asset layers with overlapping responsibilities. Without a sharper shared contract, the educational-core path can keep degrading while each layer still looks "implemented" in isolation.

## Scope

In scope:

- Audit and tighten the shared contract for reusable voice asset metadata, copied references, apply semantics, and optional voice-plan seeding.
- Add or extend targeted regression tests for common reusable-asset behaviors.
- Reduce duplicated or divergent logic where drift is already visible.
- Keep the remediation bounded to reusable-asset contract seams rather than general frontend cleanup.

Out of scope:

- Broad redesign of all voice-cloning UX.
- New backend families or benchmark features unrelated to reusable-asset drift.

## Acceptance criteria

- Templates, cast packs, memory, and character profiles have an explicit shared contract for copied references, apply behavior, and optional plan seeding.
- Regression tests cover the shared behaviors that should not drift apart.
- The remaining differences between asset classes are intentional and documented rather than accidental.

## Test / verification plan

- Focused engine/Tauri tests for reusable-asset persistence and apply behavior.
- Adversarial regression review of the shared contract points.
- Proof bundle summarizing the tightened contract and covered drift risks.

## Risks / open questions

- Over-refactoring this layer could slow operator-facing remediation if the work becomes too architectural.
- Some divergence may be legitimate; the packet must avoid flattening truly different asset classes into one confused abstraction.

## Status updates

- 2026-03-24: Created to keep the reusable-voice basics remediation from turning into another surface-only fix while internal asset contracts continue to drift.
