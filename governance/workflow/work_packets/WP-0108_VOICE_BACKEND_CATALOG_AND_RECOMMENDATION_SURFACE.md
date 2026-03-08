# Work Packet: WP-0108 - Voice backend catalog and recommendation surface

## Metadata
- ID: WP-0108
- Owner: Codex
- Status: DONE
- Created: 2026-03-08
- Target milestone: Voice backend modernization

## Intent

- What: Add a first-class catalog of managed and experimental voice backends plus recommendation logic in Diagnostics and Localization Studio.
- Why: VoxVulgi already exposes rich voice controls, but operators still cannot see backend families, strengths, install modes, or research-backed recommendations in-product.

## Scope

In scope:

- Add a built-in backend descriptor catalog for managed and experimental candidates.
- Surface backend family, install mode, license posture, languages, GPU needs, and strengths/risks.
- Add recommendation logic based on available references, performance tier, and dubbing goals.
- Show the resulting catalog and recommendation in Diagnostics and Localization Studio.

Out of scope:

- Running experimental backends.
- Replacing the default OpenVoice path.

## Acceptance criteria

- Diagnostics exposes a backend catalog beyond simple version strings.
- Localization Studio shows a recommendation with explicit reasoning.
- The shipped default path remains stable and clearly identified.

## Test / verification plan

- Desktop build.
- Rust test coverage for catalog/recommendation logic.
- Manual UI spot-check for catalog rendering.

## Status updates

- 2026-03-08: Created from the research transfer packet.
- 2026-03-08: Implemented and verified. Added engine-side backend descriptors and recommendation logic, plus Diagnostics and Localization Studio surfaces for the resulting catalog/recommendation.
