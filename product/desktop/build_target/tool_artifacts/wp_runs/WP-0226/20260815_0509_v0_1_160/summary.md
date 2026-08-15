---
file_id: WP-0226-PROOF-20260815-0509
file_kind: proof-summary
updated_at: 2026-08-15
wp: WP-0226
app_version: 0.1.160
outcome: PASS
---

<topic id="outcome" status="pass" version="v1" wp="WP-0226" updated_at="2026-08-15">

# Outcome

WP-0226 passes its current acceptance gates in governed desktop v0.1.160. A fresh packaged freeze report contains every literal Jobs/Library command with four or more measurements each; all measurements are 8-21 ms against the canonical 1.06 GB, 320,122-job database.

</topic>

<topic id="scope-delivered" status="complete" version="v1" wp="WP-0226" updated_at="2026-08-15">

# Scope actually delivered

- Preserved the packet's existing read-only connection sweep and Tauri command timers.
- Added schema v50 index `job(item_id, created_at_ms DESC)` after the exact 320,122-row query reproduced at 381.68-389.24 ms and planned as a full created-at index scan.
- Kept schema v50 compatible with intentional partial historical/repair databases that have no `job` table.
- Sequenced `jobs_list_for_item` before the remaining Subtitle Editor bootstrap fan-out after governed v0.1.157 exposed a real 604 ms cold invocation.
- Added a repeatable, headless-safe read-only Diagnostics timing check that invokes the five literal WP-0226 commands sequentially using a canonical library item.
- Produced governed v0.1.160 desktop and NSIS artifacts with the previously verified offline payload.

</topic>

<topic id="verification" status="pass" version="v1" wp="WP-0226" updated_at="2026-08-15" ingestable="true">

# Verification commands and observations

```text
cargo test --manifest-path product/engine/Cargo.toml
Result: exit 0; 544 library tests executed, including both v50 regressions and the two historical partial-database cases; auxiliary targets and doc tests passed.

node --import tsx --test tests/freezeContainmentContract.test.ts tests/localizationVoiceSetupContract.test.ts tests/agentUiAuditContract.test.ts
Result: 34 passed, 0 failed.

npm run build
Result: PASS (TypeScript + Vite production build).

governance/scripts/build_desktop_target.ps1 -WorkPackets WP-0226 -NoArchiveCurrent -SkipWarmupGate -SkipWarmupGateReason <recorded reason>
Final result: PASS; 0.1.159 -> 0.1.160; release compile 7m01s; NSIS bundle produced.

Independent canonical DB read:
- PRAGMA user_version = 50.
- idx_job_item_created exists as CREATE INDEX idx_job_item_created ON job(item_id, created_at_ms DESC).
- EXPLAIN QUERY PLAN = SEARCH job USING INDEX idx_job_item_created (item_id=?).
- Five exact query runs = 0.100, 0.051, 0.049, 0.046, 0.040 ms.

Fresh packaged cold editor proof in v0.1.158:
- jobs_list_for_item = 13 ms before library_get and item_outputs began.

Fresh packaged Diagnostics sweep in v0.1.160, captured by vvfreeze.cmd:
- jobs_list = 10, 21, 20, 17, 19 ms (max 21).
- jobs_list_for_item = 9, 9, 9, 9 ms.
- jobs_queue_control_get = 9, 8, 11, 8, 10 ms (max 11).
- jobs_runtime_settings_get = 9, 10, 10, 10 ms.
- library_get = 9, 8, 10, 9 ms.
- All five literal acceptance commands remain below 500 ms.
```

</topic>

<topic id="visual-proof" status="pass" version="v1" wp="WP-0226" updated_at="2026-08-15">

# Visual proof

Direct inspection of the two v0.1.160 screenshots confirmed:

- the completion notice is fully readable and identifies the tested item;
- the read-only timing control is discoverable inside the existing Recent failures card;
- the non-mutating explanation is visible;
- no overlap, clipping, extra card, or hidden important state is present.

</topic>

<topic id="artifacts" status="complete" version="v1" wp="WP-0226" updated_at="2026-08-15">

# Referenced artifacts

- Final build log: `product/desktop/build_target/logs/build_desktop_target_20260815-045827_0_1_160.log`
- Installer: `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.160_x64-setup.exe`
- Copied freeze report: `freeze_report_1786763342332.json` beside this summary.
- Completion screenshot: `governance/snapshots/WP-0226/v0_1_160_read_sweep_completed_top_1786763321974.png`
- Control screenshot: `governance/snapshots/WP-0226/v0_1_160_read_sweep_control_1786763324838.png`
- Structured receipt: `evidence.json` beside this summary.

</topic>

<topic id="caveats" status="noted" version="v1" wp="WP-0226" updated_at="2026-08-15">

# Caveats and non-blocking observations

- The payload warmup gate was skipped because WP-0226 changed only SQLite read behavior and frontend diagnostics/navigation; no Python, model, resolver, dependency, or offline-payload inputs changed. The verified 5.74 GB payload was reused.
- The full frontend contract sweep was 201/202: its sole failure is a pre-existing YouTube auth message-regex mismatch in unchanged `OptionsPage.tsx`, reproducible at HEAD and outside WP-0226. The three directly affected contract files pass 34/34, and the production frontend build passes.
- Governed v0.1.157's 604 ms cold editor failure and v0.1.159's no-item self-test refusal were retained in packet history and corrected before this PASS claim.

</topic>
