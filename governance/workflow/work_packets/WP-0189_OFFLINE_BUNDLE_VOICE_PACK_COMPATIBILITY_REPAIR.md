# Work Packet: WP-0189 - Offline bundle voice-pack compatibility repair

## Metadata
- ID: WP-0189
- Owner: Codex
- Status: DONE
- Created: 2026-04-19
- Target milestone: Desktop Packaging Reliability

## Intent

- What: Repair the managed offline-bundle prep path so the voice-preserving pack installs cleanly during desktop installer payload assembly.
- Why: `build_desktop_target.ps1` currently fails while preparing the bundled offline payload because the voice-preserving pack downgrades `huggingface_hub` to a version that makes `kokoro` fail to import, which causes the repo's own readiness check to mark the pack as not installed.

## Scope

In scope:
- Fix the pinned Python dependency set used by `tts_voice_preserving_local_v1` so it remains compatible with the existing Kokoro + Transformers stack during offline payload prep.
- Re-verify offline bundle preparation on a clean stage directory.
- Re-run the managed desktop target build so installer and executable artifacts are produced with the repaired payload.
- Capture a proof bundle with the exact verification commands and resulting artifact/log paths.

Out of scope:
- Broader voice-backend refactors.
- New backend integrations or benchmark policy changes.
- Manual operator smoke beyond the installer/offline-prep build boundary.

## Acceptance criteria
- `voxvulgi_offline_bundle_prep` succeeds on a clean staged app-data root.
- The voice-preserving pack status passes after install during offline bundle prep.
- `governance/scripts/build_desktop_target.ps1` completes successfully and emits the managed installer/exe outputs.
- A proof bundle exists under `product/desktop/build_target/tool_artifacts/wp_runs/WP-0189/` with `summary.md`.

## Notes

- 2026-04-19: Root cause confirmed in the managed offline-bundle path. `tts_voice_preserving_local_v1` pinned `huggingface_hub==1.4.1`, which downgraded the shared venv enough to break `kokoro` imports through `transformers` during the post-install status probe.
- 2026-04-19: Repaired the pin to `huggingface_hub==1.5.0`, reran the managed desktop target build, and produced version `0.1.8` with refreshed offline payload plus new installer/executable artifacts.
- 2026-04-19: Proof bundle captured under `product/desktop/build_target/tool_artifacts/wp_runs/WP-0189/20260419_193510/`.
