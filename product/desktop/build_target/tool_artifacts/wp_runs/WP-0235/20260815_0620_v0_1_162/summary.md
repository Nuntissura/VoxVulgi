# WP-0235 Summary

Status: IN_PROGRESS
Date: 2026-08-15

## Outcome

- Governed desktop v0.1.162 ships the Localization Studio voice-setup gate, manifest-owned size/time estimate, session-scoped `Set up later`/Escape path, compact deferred reminder, tracked setup/repair job projection, recovery guidance, and Advanced setup navigation.
- Added `VOXVULGI_AGENT_HEADLESS_BASE_DIR`, honored only with `--agent-headless`, so packaged UI verification can use a disposable database/config/trace root without reading or mutating operator app data.
- The clean isolated app showed one full voice-setup region, no deferred region, enabled `Set up now` and `Set up later` controls, the 3.0 GB / 5–15 minute manifest estimate, and zero missing accessible names.
- The audit-approved `Set up later` click produced the compact reminder; restarting the packaged app against the same isolated root restored the full setup region, proving session-only deferral.
- Existing runtime bundles continue to prove usable installed-pack localization on exact-one-speaker and three-speaker inputs.
- WP-0235 is not marked `DONE`: its work contract names WP-0236's per-pack repair primitive for the failure action, and WP-0236 remains open. WP-0230 owns the separately scoped truthful install-progress feed.

## Verification

- `node --import tsx --test tests/localizationVoiceSetupContract.test.ts` — 9/9 passed.
- `node --import tsx --test tests/agentUiAuditContract.test.ts tests/localizationVoiceSetupContract.test.ts` — 16/16 passed on the final source state.
- `cargo test --manifest-path product/engine/Cargo.toml phase2_setup_estimate_is_manifest_owned_and_coherent -- --nocapture` with `CARGO_BUILD_JOBS=1` and below-normal priority — 1/1 passed.
- `npm run build` — TypeScript and Vite production build passed.
- `governance/scripts/build_desktop_target.ps1 -NoArchiveCurrent -SkipWarmupGate ... -WorkPackets WP-0235` with `CARGO_BUILD_JOBS=1` and below-normal priority — governed v0.1.162 executable and NSIS installer built; verified 5.74 GB payload reused; changelog appended. The warmup gate was skipped because dependency pins/payload inputs were unchanged and the operator reported heavy host load.
- Packaged app launched from `product/desktop/build_target/Current/release/desktop.exe --agent-headless` with `VOXVULGI_AGENT_HEADLESS_BASE_DIR` set to the isolated proof root. Sidecar PID matched the launched PID; `/agent/state` reported `agent_headless=true` and `app_version=0.1.162`.
- Initial semantic audit: 37/37 candidates returned, zero missing accessible names; setup region, estimate, and controls present; `Set up later` was an allowlisted safe click and `Set up now` was not invoked.
- After the safe Later click: deferred status present, full setup absent, subtitles-available/dub-blocked copy present, reversible `Set up now` available, zero missing accessible names.
- After process restart using the same isolated base root: full setup regions `1`, deferred regions `0`, `Set up later` buttons `1`, zero missing accessible names.
- Quiet Win32 Escape messages posted to the hidden WebView did not reach React and are explicitly not counted as proof. Escape wiring remains covered by the source contract and shares the same `deferVoiceCloningSetup` function proven by the packaged Later-click scenario.

## Evidence

- `evidence.json`
- `governance/snapshots/WP-0235/voice_setup_initial_v0_1_162_1786767552584.png`
- `governance/snapshots/WP-0235/voice_setup_initial_v0_1_162_1786767552611.dump.json`
- `governance/snapshots/WP-0235/voice_setup_scrolled_v0_1_162_1786767571641.png`
- `governance/snapshots/WP-0235/voice_setup_actions_v0_1_162_1786767585703.png`
- `governance/snapshots/WP-0235/voice_setup_deferred_v0_1_162_1786767609152.png`
- `governance/snapshots/WP-0235/voice_setup_deferred_v0_1_162_1786767609162.dump.json`
- `governance/snapshots/WP-0235/voice_setup_deferred_visible_v0_1_162_1786767618522.png`
- `governance/snapshots/WP-0235/voice_setup_after_restart_v0_1_162_1786767665685.png`
- `governance/snapshots/WP-0235/voice_setup_after_restart_v0_1_162_1786767665697.dump.json`
- `product/desktop/build_target/logs/build_desktop_target_20260815-061149_0_1_162.log`
- `product/desktop/build_target/tool_artifacts/wp_runs/WP-0235/runtime_haerin_retry_20260521_180314/summary.md`
- `product/desktop/build_target/tool_artifacts/wp_runs/WP-0235/runtime_queen_20260521_183952/summary.md`

## Notes

- Visual inspection passed for the setup body, estimate/status, action row, and deferred reminder: readable text, discoverable controls, coherent alignment, and no overlap.
- No setup/download/repair job was queued during this proof.
- The first v0.1.161 isolated launch demonstrated that Windows/Tauri ignored `APPDATA` redirection for `app.path().app_data_dir()`. The headless-only override in v0.1.162 closes that safety gap without changing normal app launches.
