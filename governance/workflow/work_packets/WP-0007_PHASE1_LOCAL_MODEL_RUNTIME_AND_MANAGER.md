# Work Packet: WP-0007 — Phase 1: Local model runtime + model manager

## Metadata
- ID: WP-0007
- Owner: Codex
- Status: DONE
- Created: 2026-02-19
- Target milestone: Phase 1 (MVP)

## Intent

- What: Define and implement the local-first model runtime layer (ASR + translation first) plus model inventory/installation flows.
- Why: Local-first is a core stance; we need a consistent way to install, verify, store, and report model assets.

## Scope

In scope:

- Define a model manifest format (name, version, size, sha256, source, license).
- Define model storage locations and cache controls.
- Implement integrity-checked model acquisition (download-on-demand) OR bundled models (decision captured in this WP).
- Expose a model inventory surface for Diagnostics (what is installed, where, sizes, versions).

Out of scope:

- Voice-preserving models (gated R&D; Phase 3).
- Cloud provider integrations (only define interface hooks; no default cloud usage).

## Acceptance criteria

- A model manifest exists and is used by the app.
- The app can verify model assets by hash before use.
- Diagnostics can show installed model inventory and storage usage.

## Implementation notes

- Downloads (if used) must be:
  - user-visible (with consent)
  - resumable if feasible
  - verified (sha256/signature)
- Prefer keeping models per-language/task (JA ASR, KO ASR, JA->EN translate, KO->EN translate), even if they share a runtime.
- Decision (for this WP): ship a tiny bundled "demo model" artifact to prove the manifest + install + hash verification flow; real model sources/licensing will be decided in a later WP before adding large assets.

## Test / verification plan

- Install at least one model asset and verify:
  - hash validation works
  - inventory is updated
  - cache clear does not delete user library media

## Risks / open questions

- Model licensing and redistribution constraints.
- Hardware variability (CPU-only users vs GPU acceleration).

## Status updates

- 2026-02-19: Added a bundled model manifest + storage layout + sha256 verification in `product/engine/` and surfaced model inventory + install action in the Diagnostics UI.
