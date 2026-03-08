# Voice Cloning Research Corpus (2026-03-08)

This folder captures the March 8, 2026 research refresh for VoxVulgi's voice-cloning and voice-preserving dubbing program.

Contents:

- `VOICE_CLONING_LANDSCAPE_20260308.md`
  - deep research synthesis across papers, vendors, and OSS repos
- `VOICE_CLONING_IMPLEMENTATION_STRATEGY_20260308.md`
  - recommended implementation strategy for VoxVulgi
- `voice_cloning_candidates_20260308.json`
  - machine-readable candidate matrix for future tooling or audits

This corpus is governance material. It informs:

- `WP-0107` research transfer and spec sync
- `WP-0108` backend catalog and recommendation surface
- `WP-0109` benchmark lab and comparison reports
- `WP-0110` experimental BYO backend adapters

Current strategic conclusion:

- keep the shipped OpenVoice V2 + Kokoro path as the default managed backend
- add a first-class backend catalog and recommendation layer
- add a benchmark lab that ranks the actual outputs VoxVulgi generates
- add explicit BYO adapter support for stronger OSS backends before attempting to ship a second managed backend
