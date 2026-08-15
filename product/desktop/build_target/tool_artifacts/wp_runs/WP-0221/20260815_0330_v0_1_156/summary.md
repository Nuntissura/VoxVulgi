---
file_id: WP-0221-PROOF-20260815-0330
file_kind: proof-summary
updated_at: 2026-08-15
wp: WP-0221
app_version: 0.1.156
outcome: PASS
---

<topic id="outcome" status="pass" version="v1" wp="WP-0221" updated_at="2026-08-15">

# Outcome

WP-0221 passes its detector, reporting, UI, build, and repo-trigger acceptance surfaces in governed desktop v0.1.156. A real packaged Worker detected and recovered from a bounded 750 ms WebView main-thread block, while the existing OS heartbeat independently emitted a controlled skew row.

</topic>

<topic id="scope-delivered" status="complete" version="v1" wp="WP-0221" updated_at="2026-08-15">

# Scope actually delivered

- Corrected the Worker heartbeat so one ping remains outstanding until answered; its age can now cross the freeze threshold.
- Enriched real freeze ingress with instrumented backend `last_invoke`, invoke age/completion state, and in-flight invoke count.
- Added a one-shot skew self-test request consumed by the existing dedicated heartbeat thread.
- Added one bounded `Run detector self-test` safe action inside the existing Freeze events subsection; it deliberately blocks only the WebView main thread for 750 ms and adds no card.
- Added a source contract preventing ping-timestamp overwrite regression and requiring the durable self-test surfaces.
- Preserved the existing Worker liveness heartbeat, bridge endpoint, trace files, freeze-report bundle, Diagnostics view, and `vvfreeze.cmd` trigger.

</topic>

<topic id="verification" status="pass" version="v1" wp="WP-0221" updated_at="2026-08-15" ingestable="true">

# Verification commands and scenarios

```text
node --import tsx --test tests/freezeContainmentContract.test.ts
Result: 19 passed, 0 failed.

npm run build
Result: PASS; tsc and Vite production build succeeded; Worker emitted as a bundled .js asset.

cargo check --manifest-path product/desktop/src-tauri/Cargo.toml
Result: PASS.

cargo test --manifest-path product/desktop/src-tauri/Cargo.toml -- --test-threads=1
Result: 45 passed, 0 failed.

governance/scripts/build_desktop_target.ps1 -WorkPackets WP-0221 -NoArchiveCurrent -SkipWarmupGate -SkipWarmupGateReason <recorded reason>
Result: PASS; 0.1.155 -> 0.1.156; release compile 6m30s; NSIS bundle produced.

Packaged scenario:
1. Launch product/desktop/build_target/Current/release/desktop.exe --agent-headless.
2. Confirm /agent/state reports app_version=0.1.156 and agent_headless=true.
3. Navigate to Diagnostics through /agent/navigate.
4. Run /agent/ui_audit and select only the row with test_id=diagnostics-freeze-self-test and safe_actions containing click.
5. Invoke /agent/ui_action for that audit ID.
6. Capture /agent/freeze_dump and filter rows at or after bridge started_at_ms=1786757189179.
7. Scroll to the Freeze events subsection through allowlisted semantic actions and capture screenshots.
8. Run vvfreeze.cmd and verify it prints both report paths.
```

Observed packaged rows:

- `freeze_detected`: `gap_ms=329`, `current_page=diagnostics`, `last_invoke.cmd=diagnostics_freeze_self_test_arm`, `last_invoke.age_ms=524`, `in_flight_invoke_count=0`.
- `freeze_recovered`: `total_freeze_ms=671`, same invoke context, `last_invoke.age_ms=866`.
- `event_loop_skew`: `target_interval_ms=250`, `actual_interval_ms=1000`, `skew_ms=750`, `self_test=true`.
- `worker_alive`: two fresh rows about 30 seconds apart.
- The self-test arm command emitted both `command_started` and `command_completed`; the packaged trace also contained real `command_slow` rows.
- The semantic UI action returned in 8 ms, before the bounded WebView block completed, so bridge control did not depend on the frozen main thread.

</topic>

<topic id="artifacts" status="complete" version="v1" wp="WP-0221" updated_at="2026-08-15">

# Referenced artifacts

- Build log: `product/desktop/build_target/logs/build_desktop_target_20260815-031905_0_1_156.log`
- Installer: `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.156_x64-setup.exe`
- Self-test freeze report: `C:/Users/Ilja Smets/AppData/Roaming/com.voxvulgi.voxvulgi/diagnostics/traces/freeze_reports/freeze_report_1786757250421.json`
- Repo-trigger report: `C:/Users/Ilja Smets/AppData/Roaming/com.voxvulgi.voxvulgi/diagnostics/traces/freeze_reports/freeze_report_1786757359535.json`
- Notice snapshot: `governance/snapshots/WP-0221/v0_1_156_freeze_self_test_1786757271254.png`
- Subsection snapshot: `governance/snapshots/WP-0221/v0_1_156_freeze_events_rows_1786757310721.png`
- Three-row visual proof: `governance/snapshots/WP-0221/v0_1_156_freeze_detect_recover_rows_1786757345855.png`
- Structured receipt: `evidence.json` beside this summary.

</topic>

<topic id="caveats" status="noted" version="v1" wp="WP-0221" updated_at="2026-08-15">

# Caveats and non-blocking gaps

- An initial parallel Tauri-suite run exposed two pre-existing shared-global-state test races. Both failing tests passed individually, and the complete suite passed 45/45 with one test thread. Product/runtime evidence was unaffected.
- The payload warmup gate was skipped because no Python, model, resolver, dependency, or offline-payload inputs changed; the verified 5.74 GB payload was reused.
- The self-test is intentionally explicit and bounded. It never runs automatically.

</topic>
