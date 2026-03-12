# Work Packet: WP-0150 - Installer-state localization runtime and proof hardening

## Metadata
- ID: WP-0150
- Owner: Codex
- Status: DONE
- Created: 2026-03-12
- Target milestone: Post-0.1.6 localization-first recovery

## Intent

- What: Add a proof-driven installer-state localization verification path so Localization Studio is not considered recovered until a real installer build produces a visible dubbed MP4 with visible source/artifact/export actions.
- Why: Previous sub-step and harness proofs were not enough. The core product still failed in real operator use.

## Scope

In scope:

- Add focused runtime verification for the installer-state localization chain.
- Capture exact stage outputs and failure points for one representative localization run.
- Require proof from the actual shipped app path or an installer-state equivalent, not only isolated backend sub-steps.
- Backstop `WP-0143` and `WP-0145` with stronger proof expectations.

Out of scope:

- Broad archive/download work not directly needed for localization proof.

## Acceptance criteria

- The localization recovery tranche has a concrete installer-state proof path that records:
  - source media,
  - selected subtitle/translation track,
  - generated speech artifact,
  - mixed dub artifact,
  - muxed MP4 artifact,
  - visible output paths.
- Follow-on localization packets cannot be closed without evidence from that proof path.

## Test / verification plan

- Focused proof bundle under `product/desktop/build_target/tool_artifacts/wp_runs/WP-0150/`.
- Re-run targeted automated checks for touched seams plus installer-state verification notes.

## Status updates

- 2026-03-12: Created after smoke feedback showed that sub-step proofs were insufficient for the core localization workflow.
- 2026-03-12: Completed. Added `product/engine/examples/wp0150_localization_run_smoke.rs` and captured a staged installer-state-equivalent proof under `product/desktop/build_target/tool_artifacts/wp_runs/WP-0150/20260312_023121/`, showing first-run ASR->Translate->Diarize, an explicit speaker/reference checkpoint, and second-run Dub->Mix->Mux->QC after speaker references were assigned.
