# Work Packet: WP-0050 - Phase 2: Spleeter install resilience on Windows

## Metadata
- ID: WP-0050
- Owner: Codex
- Status: DONE
- Created: 2026-02-22
- Target milestone: Phase 2 — Verification hardening

## Intent
- What: Remove the remaining blocker where Spleeter install fails in `install_spleeter_pack` due unresolved Python pack compatibility.
- Why: WP-0027 depends on this to complete end-to-end verification for `separate_audio_spleeter` and downstream phase 2 jobs.

## Scope
- In scope:
  - Extend Spleeter installer to try deterministic, safer install strategies:
    - `--only-binary=:all:` when wheels are available
    - `--only-binary=:none:` for source builds only when explicitly allowed by user
    - Python runtime fallback to known-good interpreters (3.11 -> 3.10 -> 3.9 -> 3.8).
  - Add preflight compatibility checks for pip/numpy/nuitka-related blockers and surface clear operator guidance.
  - Add a short diagnostics note with manual fallback steps and a non-blocking retry path.
- Out of scope:
  - Replacing Spleeter with a new separation backend.
  - Changing default install UX consent behavior.

## Acceptance criteria
- A user can install and run `separate_audio_spleeter` on a supported Windows environment without manual Python package editing.
- If installation still fails, diagnostics show the exact blocker and a concrete next action.
- `WP-0027` smoke steps can continue through Separate -> Diarize -> TTS -> Mix -> Mux on the same machine.

## Implementation notes
- Keep changes in `product/engine/src/tools.rs` under `install_spleeter_pack` and helpers.
- Avoid touching unrelated job logic.
- Preserve strict defaults: explicit install action only, no silent background egress, no telemetry.

## Test / verification plan
- `cargo test --locked` should remain green.
- Add/extend a targeted manual smoke command path to confirm pack install and separation produce `derived/items/<id>/separation/spleeter_2stems/vocals.wav`.
- Verify operator-facing install failure text includes suggested manual fallback.

## Risks / open questions
- True wheel availability can vary by Python patch version and platform.
- Source builds on some machines may still require Visual C++ / MSVC toolchains.

## Status updates
- 2026-02-22: Opened from WP-0027 blocker after unresolved Python pack compatibility on Windows.
- 2026-02-22: Implementing resilient install strategy in `product/engine/src/tools.rs`:
  - pip/setuptools/wheel bootstrap attempt before package install,
  - wheel-first Spleeter installation attempt and build-isolation fallbacks,
  - improved retry visibility and clearer manual troubleshooting path.
- 2026-02-22: Code path is in place and validated for compile/test pass (`product/engine`: `cargo test --locked`).
- 2026-02-22: Added install-failure parsing in `install_spleeter_pack` to return explicit remediation for known blockers (especially `tensorflow-io-gcs-filesystem==0.32.0` on unsupported Python combos), plus build-access/surface hints.
- 2026-02-22: Manual WP-0027 smoke still fails on Python 3.11.9 due hard dependency resolution blockers; added Python 3.11+ fast-fail guidance to direct users to Python 3.9/3.10 override and updated messaging for Poetry-core / numpy metadata failures.
- 2026-02-22: Next required step is a full manual `WP-0027` smoke run on the target pack sample to verify separation/mix/mux chain completion.
- 2026-02-22: Current machine-level probe confirms `pip index versions spleeter` returns only `2.1.0` (with Python 3.11). WP-0027 remains blocked until alternate compatibility or source packaging path is added in `WP-0051`.
- 2026-02-22: Completed fallback implementation:
  - Added fallback pass that installs `spleeter` with `--no-deps` when normal strategies fail.
  - Added deterministic dependency bootstrap with a pinned set and `tensorflow-io-gcs-filesystem==0.31.0` fallback when `0.32.0` cannot be resolved.
  - Kept normal candidate attempts intact for compatible environments; removed the hard Python >=11 fail-fast guard.
