# Work Packet: WP-0107 - Voice cloning research refresh and transfer

## Metadata
- ID: WP-0107
- Owner: Codex
- Status: DONE
- Created: 2026-03-08
- Target milestone: Voice backend modernization

## Intent

- What: Refresh VoxVulgi's voice-cloning knowledge base using current papers, vendor surfaces, and OSS repos, then transfer that knowledge into spec/governance.
- Why: The app now has a substantial voice-cloning feature surface, but its backend strategy is still anchored to one narrow implementation path and older research assumptions.

## Scope

In scope:

- Create a new tracked research corpus for voice cloning and voice-preserving dubbing.
- Summarize current technical families, vendor patterns, and strong OSS candidates.
- Convert the research into concrete product/governance follow-up work packets.
- Patch spec and roadmap so the new direction is explicit.

Out of scope:

- Product-code changes.
- Replacing the default backend in this WP.

## Acceptance criteria

- A tracked research folder exists with narrative documents and at least one machine-readable artifact.
- The research clearly recommends a VoxVulgi implementation sequence instead of remaining purely descriptive.
- Product spec, technical design, roadmap, and task board reflect the new backend-catalog, benchmark, and adapter strategy.

## Test / verification plan

- Governance document review.
- Git diff review for research/spec/taskboard/roadmap coverage.

## Status updates

- 2026-03-08: Created from operator request for a deeper paper/vendor/OSS voice-cloning research pass.
- 2026-03-08: Completed. Added research corpus under `governance/research/voice_cloning_20260308/` and synced the resulting strategy into spec/governance.
