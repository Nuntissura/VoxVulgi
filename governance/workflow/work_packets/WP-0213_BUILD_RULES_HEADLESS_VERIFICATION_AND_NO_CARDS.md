# Work Packet: WP-0213 - Build rules, headless verification, and no cards

## Status

REVIEW

## Owner

Codex

## Scope

- Create a canonical `build_rules.md` at the repo root.
- Require build/UI verification to use visual inspection plus backend or frontend navigation/interaction evidence.
- Require routine verification to avoid popping up the app window or hijacking the operator keyboard or mouse.
- Add the no-new-cards UI rule.
- Link the new rules from the Codex-facing repo instructions.

## Out of Scope

- Redesigning existing UI surfaces.
- Changing build scripts or release packaging behavior.
- Changing the Headless Agent Bridge implementation.

## Acceptance

- `build_rules.md` exists with the requested headless verification and no-cards rules.
- `PROJECT_CODEX.md` links to `build_rules.md`.
- Agent-facing notes point to the new build rules.

## Notes

- 2026-05-12: Created after operator direction to make all build inspection headless/non-invasive and stop adding card-based UI.

