# Work Packet: WP-0217 - Diagnostics voice-pack installer stale-state truth

## Status

REVIEW

## Owner

Codex

## Scope

- Stop Diagnostics from reporting an old interrupted Voice cloning packages install as actively `Installing...`.
- Normalize `logs/install/phase2/latest.json` against the real job database before returning it to the frontend.
- Mark active-looking installer steps as `interrupted`, `failed`, `canceled`, or `stale` when the matching job is terminal or missing.
- Treat installer step status `done` as complete in the Diagnostics summary.

## Out of Scope

- Replacing the voice-preserving TTS/OpenVoice pack installer.
- Automatically downloading multi-GB voice packs from a localization run.
- Changing release installer payload policy.

## Acceptance

- An interrupted Phase 2 install no longer keeps the dashboard tile in `Installing...`.
- Diagnostics shows interrupted/stale install state as non-active, so the install action remains understandable.
- A completed Phase 2 state using `done` step statuses summarizes as installed.
- Regression coverage proves failed jobs normalize stale `running`/`queued` steps to `interrupted`.

## Notes

- 2026-05-13: Created after the live app showed `Voice packages: Installing...` from an April 27 interrupted installer while the job database had no active install job.
- 2026-05-13: Implemented backend state normalization plus frontend status helpers. Verification summary: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0217/20260513_173351/summary.md`.
