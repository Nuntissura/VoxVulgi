# Work Packet: WP-0202 - Diarization pack dependency integrity and self-repair

## Metadata
- ID: WP-0202
- Owner: Codex
- Status: REVIEW
- Created: 2026-04-24
- Target milestone: Localization runtime reliability

## Intent

- What: Make the managed diarization pack detect incompatible Python dependencies, surface a truthful broken state in Diagnostics, and provide a reliable repair/reinstall path.
- Why: Operator smoke hit a live diarization failure where the managed venv contained `numba 0.64.0` alongside `llvmlite 0.46.0`, and current Diagnostics/install checks can still report the pack as effectively installed because they only validate a subset of imports.

## Scope

In scope:
- Pin a known-good dependency set for the shipped resemblyzer diarization path, including any required `numba`/`llvmlite` compatibility pair.
- Extend managed-pack validation beyond "package exists" checks so Diagnostics can distinguish `installed`, `loading`, and `broken`.
- Add explicit runtime warmup/import validation for the diarization path that exercises the actual dependencies used by `resemblyzer_partials_cluster_v1`.
- Provide a repair or forced-reinstall path that can replace an incompatible existing venv state.
- Keep offline-bundle prep and installer hydration aligned with the repaired dependency pins and validation rules.

Out of scope:
- BYO pyannote backend policy or token/model setup, which remains a separate optional path.
- Broad Python-environment redesign across all packs beyond what is required to make the managed diarization pack truthful and repairable.

## Acceptance criteria

- A broken managed diarization environment is surfaced as broken in Diagnostics instead of appearing healthy.
- Diagnostics or the installer-state pack surface shows enough detail for the operator to understand that repair/reinstall is required.
- Running the repair/reinstall path results in a compatible diarization environment on installer state.
- The Queen-sample diarization stage gets past Python startup and dependency import with the repaired pack.
- Offline-bundled installs hydrate the same validated dependency set instead of recreating the broken combination.

## Test / verification plan

- Reproduce the current incompatible `numba`/`llvmlite` state and confirm Diagnostics surfaces it as broken.
- Add focused validation coverage around diarization-pack status and install/repair behavior.
- Re-run pack install/repair on a managed venv and verify the diarization job advances past dependency import.
- Run `cargo check` and desktop `npm run build`.

## Risks / open questions

- The Python venv is shared with multiple managed packs, so package-version changes must be checked for cross-pack fallout.
- Some repair paths may require replacing or rebuilding a partially poisoned venv rather than incrementally installing packages in place.

## Status updates

- 2026-04-24: Created after operator smoke and live environment inspection showed a broken managed diarization venv (`numba 0.64.0`, `llvmlite 0.46.0`) while current pack validation still lacked full runtime-integrity checks.
- 2026-04-25: Implementation pass started. Live managed venv reproduced the broken state: `numba` import fails because installed `llvmlite` is below its runtime requirement, while existing Diagnostics validation still only checked a subset of packages.
- 2026-04-25: Moved to REVIEW. Added validated diarization pins, runtime warmup validation, Diagnostics broken/repair detail, and repair install flow. Live venv repaired to `numba 0.65.0`/`llvmlite 0.47.0`; VoiceEncoder validation and a Queen-media resemblyzer probe passed. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0202/2026-04-25_0315_wp0202_0204/summary.md`.
