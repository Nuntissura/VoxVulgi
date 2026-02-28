# Work Packet: WP-0047 - ROI-20: Licensing/attribution report generator for installed packs/models

## Metadata
- ID: WP-0047
- Owner: Codex
- Status: DONE
- Created: 2026-02-22
- Target milestone: Phase 2 (voice-preserving dubbing)

## Intent

- What: Generate a licensing/attribution report for all installed packs and models.
- Why: If the project becomes freemium/commercial, we need a clear view of dependencies and attribution requirements.

## Scope

In scope:

- Engine:
  - Collect:
    - installed Python package list (best-effort),
    - known model/weights manifests and license info (when available),
    - relevant app-side licenses.
  - Output a report (markdown or JSON) into a known location.
- Desktop:
  - Diagnostics action: "Generate licensing report".
  - Show the report path and allow opening/revealing it.

Out of scope:

- Legal advice.
- Telemetry.
- Consent gates or anti-abuse controls.

## Acceptance criteria

- A licensing report can be generated and includes:
  - package names + versions (best-effort),
  - model/weights license metadata if present in manifests.

## Test / verification plan

- Install one or more packs, generate the report, verify it lists the expected dependencies.

## Risks / open questions

- Some packages do not expose license metadata cleanly; report must be best-effort and transparent about gaps.

## Status updates

- 2026-02-22: Created.
- 2026-02-23: Implemented licensing/attribution report generator + Diagnostics UI action to generate/reveal report; verified via build + tests.
