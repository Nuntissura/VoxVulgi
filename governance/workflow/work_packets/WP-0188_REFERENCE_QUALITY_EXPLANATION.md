# Work Packet: WP-0188 - Reference Quality Score Explanation

## Metadata
- ID: WP-0188
- Owner: Codex
- Status: DONE
- Created: 2026-04-08
- Target milestone: Voice Cloning UX

## Intent

- What: Show per-factor breakdown of reference quality scores so operators understand WHY one reference ranked higher than another.
- Why: The curation system scores references on 7 factors (duration, level, silence, clipping, noise, issues, pitch) but only shows a single composite score. Operators can't improve their references without knowing what's weak.

## Scope

In scope:
- In the voice plan speaker cards, show a mini breakdown when reference score is available:
  - Duration: OK (6.0s) / Too short (1.2s) / Too long (25s)
  - Audio level: OK / Too quiet / Clipping detected
  - Background noise: Low / Moderate / High
  - Pitch consistency: Matches speaker median / Diverges significantly
- Add a "Reference quality tips" help panel to the voice plan section.
- Color-code each factor: green (good), yellow (marginal), red (poor).

Out of scope:
- Automatic reference improvement.
- Re-running curation from the UI (already exists).

## Acceptance criteria
- Each speaker's reference shows a multi-factor quality breakdown.
- Weak factors are highlighted with improvement suggestions.
- `npm run build` passes.

## Final verification

- Focused clone UX tests: 9 passed, 0 failed, including all seven curation factors and unavailable-pitch guidance.
- Frontend production build passed.
- Governed desktop build produced packaged v0.1.153.
- Hidden packaged-app proof rendered all seven factors from real `candidate_bundle_v1.wav` curation (score 93.3), including the corrected unavailable-pitch suggestion. The screenshot was visually inspected and the paired dump recorded zero console entries.
- Proof bundle: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0188/20260815-0205_v0_1_153/summary.md`.
