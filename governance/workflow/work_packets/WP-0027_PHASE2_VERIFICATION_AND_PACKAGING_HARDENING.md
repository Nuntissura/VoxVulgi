# Work Packet: WP-0027 - Phase 2: Verification + packaging hardening (Python packs)

## Metadata
- ID: WP-0027
- Owner: Codex
- Status: DONE
- Created: 2026-02-22
- Target milestone: Phase 2

## Intent

- What: Verify WP-0022..WP-0026 end-to-end on the target machines and harden installation UX for large Python packs (clear errors, progress, retry, and disk usage).
- Why: The Python pack approach is powerful but fragile; verification and UX hardening is required before calling these features "shippable".

## Scope

In scope:

- Run through installs and jobs:
  - Python toolchain setup
  - Spleeter separation install + job
  - Diarization install + job
  - TTS preview install + job
  - Mix job
  - Mux job
- Fix any immediate crashes/compile errors and improve error messages.
- Add clear UX notes in Diagnostics about disk/network impact (without adding consent mechanisms).

Out of scope:

- Changing the overall approach (portable Python bundling is separate).

## Acceptance criteria

- WP-0022..WP-0026 can be marked `DONE` with concrete verification steps recorded.

## Verification checklist (manual)

1. Diagnostics installs (explicit user action):
   - Setup Python toolchain
   - Install Spleeter
   - Install diarization pack
   - Install TTS preview pack
2. Per-item jobs (on a short local video):
   - Separate (Spleeter) -> expect:
     - `derived/items/<item_id>/separation/spleeter_2stems/vocals.wav`
     - `derived/items/<item_id>/separation/spleeter_2stems/background.wav`
   - ASR (local) -> expect a `subtitle_track` created and `derived/items/<item_id>/asr/*` artifacts
   - Diarize speakers (local) -> expect:
     - `derived/items/<item_id>/diarize/diarization.json`
     - a new subtitle track with `speaker` labels filled in
   - TTS preview (local) -> expect:
     - `derived/items/<item_id>/tts_preview/pyttsx3_v1/segments/*.wav`
     - `derived/items/<item_id>/tts_preview/pyttsx3_v1/manifest.json`
   - Mix dub -> expect:
     - `derived/items/<item_id>/dub_preview/mix_dub_preview_v1.wav`
   - Mux -> expect:
     - `derived/items/<item_id>/dub_preview/mux_dub_preview_v1.mp4`
3. Failure UX:
   - Trigger expected failures (run Mix before Separate, run TTS preview without installing pack) and confirm the errors tell the user what to do next.
4. Storage/footprint:
   - Check Diagnostics -> Storage reflects pack installs (Cache and/or Derived growth).

## Status updates

- 2026-02-22: Started; hardening Diagnostics UX for Phase 2 pack installs and preparing end-to-end smoke test.
- 2026-02-22: Added explicit note in diagnostics for pack footprint/explicit-install behavior.
- 2026-02-22: Build validation run:
  - `product/engine`: `cargo test --locked` initially failed due FFmpeg filter format string bug, now fixed and passes.
  - `product/desktop`: `npm run build` passes.
- 2026-02-22: Full re-run verification step:
  - `product/engine`: `cargo test --locked` -> 47 passed, 0 failed.
  - `product/desktop`: `npm run build` -> success.
  - Manual job-chain smoke test attempted on provided sample, but blocked before completion due Spleeter installer/runtime compatibility.
  - On this machine, Spleeter install failed with:
    - `python 3.14` selected by `install_python_toolchain` defaults.
    - `spleeter==2.4.0` unavailable for that runtime.
    - Fallback to `spleeter==2.1.0` failed during NumPy metadata generation (NumPy build dependency issues in this environment).
  - Impact: `separate_audio_spleeter` can’t be provisioned yet, so downstream `Mix/Dub->Mux` checks are still blocked until the separation backend installation path is made runtime-compatible.
- 2026-02-22: Mitigation implemented for WP-0027 blocker:
  - `product/engine/src/tools.rs` now prefers Python 3.11 (and 3.10/3.9/3.8 via `py -3.x`) on Windows before defaulting to system `python`.
  - Spleeter installer now tries version-adaptive package candidates (`spleeter==2.4.2`, then `spleeter`) and re-tries on failure instead of hard failing on one pinned version.
  - `cargo test --locked` passes after change (`47 passed, 0 failed`).
- 2026-02-22: Blocked pending dedicated pack dependency stability follow-up. Manual smoke chain still fails before completion due Spleeter install in current environment:
  - candidate install fails after resolver attempts, and TensorFlow-I/O/NumPy constraints are still incompatible with the resolved runtime.
  - Created follow-up packet `WP-0050` to make `install_spleeter_pack` resilient on Windows environments lacking compatible wheels by using safer install strategies and explicit operator diagnostics.
- 2026-02-22: `WP-0050` mitigation patch is implemented and compiling (`product/engine`: `cargo test --locked` passes, `product/desktop`: `npm run build` passes). Manual smoke remains unresolved on this machine until `WP-0050` is verified with a full pack install.
- 2026-02-22: Manual `cargo test --locked --test wp0027_smoke -- --ignored --nocapture` confirms current local blocker:
  - Python 3.11.9 selected in `install_python_toolchain` and `install_spleeter_pack` exits before attempt with runtime guidance.
  - `spleeter==2.4.2` is unavailable from this source in the environment (`pip index versions spleeter` reports `2.1.0` as available), so end-to-end WP-0027 cannot complete here.
- 2026-02-22: Follow-up `WP-0051` added for deterministic fallback options (non-2.4.x environments / alternate backend path) so verification can be completed on constrained machines.
- 2026-02-22: `WP-0051` implementation is in progress in `product/engine/src/tools.rs`:
  - Adds explicit `spleeter` fallback install with `--no-deps` and pinned dependency bootstrap.
  - Adds `tensorflow-io-gcs-filesystem==0.31.0` fallback for environments where `0.32.0` is unavailable.
  - Retained normal installer attempts first so compatible runtimes keep the fast standard path.
- 2026-02-22: WP-0027 completed after Spleeter runtime compatibility fix for Python job environment:
  - Added explicit `h2` dependency bootstrap for Spleeter invocation path so `httpx[http2]` requirements are satisfied when Spleeter is executed under the app venv.
  - `cargo test --locked --test wp0027_smoke -- --ignored --nocapture --exact wp_0027_phase2_smoke_chain_on_sample` now passes.
