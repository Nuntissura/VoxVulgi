# Work Packet: WP-0042 - ROI-15: Model/packs integrity (hash-verified downloads + pinned versions)

## Metadata
- ID: WP-0042
- Owner: Codex
- Status: DONE
- Created: 2026-02-22
- Target milestone: Phase 2 (voice-preserving dubbing)

## Intent

- What: Ensure Python packs and model downloads are reproducible and integrity-checked via pinned versions and hash verification.
- Why: Future commercial distribution and reliable support require deterministic installs and tamper-evident downloads.

## Scope

In scope:

- Engine:
  - Define per-pack pinned versions (lockfiles) rather than "latest".
  - Support hash-verified installs where practical:
    - pip `--require-hashes` from a pinned requirements set,
    - model downloads with SHA256 verification.
  - Persist a local manifest of installed pack versions and model hashes.
- Desktop:
  - Diagnostics shows:
    - installed versions,
    - integrity status (best-effort),
    - the manifest location.

Out of scope:

- Automatic updates.
- Telemetry.
- Consent gates or anti-abuse controls.

## Acceptance criteria

- Installing a pack uses pinned versions (not "latest") and produces deterministic results (best-effort).
- Model downloads (if any) are hash-verified before use.
- A provenance/integrity manifest is persisted and viewable.

## Test / verification plan

- Install packs on two machines and verify versions match and manifest hashes match (best-effort).

## Risks / open questions

- Some Python ecosystems do not reliably support fully hashed lockfiles across platforms without careful wheel selection.

## Status updates

- 2026-02-22: Created.
- 2026-02-23: Added pack/model integrity manifest generation + Diagnostics UI surfacing; pinned installs and hash-verified model downloads where applicable; verified via build + tests.
