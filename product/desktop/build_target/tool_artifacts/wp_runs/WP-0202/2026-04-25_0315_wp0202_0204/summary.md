# WP-0202 Proof Summary

Status: REVIEW

Scope verified:
- Managed diarization pins now include the `librosa` runtime chain plus `numba==0.65.0` and `llvmlite==0.47.0`.
- Diagnostics reports `installed`, `broken`, or `not_installed`, exposes repair-required detail, package versions, and runtime validation errors.
- The repair path first targets the binary `numba`/`llvmlite` pair, then installs the complete pinned set and requires VoiceEncoder runtime validation.

Live app-data evidence:
- Reproduced the broken managed venv before repair: `numba 0.64.0` with `llvmlite 0.46.0` failed during import.
- Repaired the managed venv to `resemblyzer=0.1.4`, `librosa=0.11.0`, `numba=0.65.0`, `llvmlite=0.47.0`, `numpy=1.26.4`, `scikit-learn=1.8.0`, `webrtcvad=2.0.10`, `soundfile=0.13.1`.
- Runtime validation loaded `VoiceEncoder` successfully.
- Queen media proof clip ran the baseline resemblyzer diarization startup and partial embedding path, producing `queen_probe_diarization.json` with 9 partial slices and 2 provisional speaker labels.

Visual debugger snapshots:
- `governance/snapshots/WP-0202/diagnostics_tools_absolute_15000_1777080113975.png`
- `governance/snapshots/WP-0202/diagnostics_diarization_versions_15300_1777080182146.png`
- `governance/snapshots/WP-0202/diagnostics_diarization_versions_15450_1777080189456.png`

Verification commands:
- `cargo test diarization_runtime_validation --lib`
- `cargo test manifest_parses --lib`
- `cargo check` from `product/engine`
- `cargo check` from `product/desktop/src-tauri`
- `npm.cmd run build` from `product/desktop`

Notes:
- A broad forced reinstall attempt against the live venv timed out; the targeted binary pair repair succeeded and is now part of the product install/repair path.
- The manual live repair still emits the existing `webrtcvad.py` `pkg_resources` warning. The product repair path still runs the existing vendor patch after dependency installation.
