---
file_id: WP-0224-PROOF-20260815-0310
file_kind: proof-summary
updated_at: 2026-08-15
wp: WP-0224
app_version: 0.1.155
outcome: PASS
---

<topic id="outcome" status="pass" version="v1" wp="WP-0224" updated_at="2026-08-15">

# Outcome

WP-0224 passes its current acceptance gates in governed desktop v0.1.155. The freeze-detector Worker posts liveness successfully, and every measured packaged list command remained below 500 ms against the operator's live 144,082-row library database.

</topic>

<topic id="scope-delivered" status="complete" version="v1" wp="WP-0224" updated_at="2026-08-15">

# Scope actually delivered

- Preserved the existing localhost bridge CORS/OPTIONS and read-only UI-connection implementation.
- Removed redundant directory creation from `db::open_readonly`; premature reads now fail without creating an uninitialized app root.
- Replaced the common available-library parameterized `OR`/`CASE` query with a fixed, allowlisted predicate/order shape so SQLite uses `idx_library_item_file_status_created`.
- Added regressions for read-only initialization behavior and the exact available-list query plan.
- Produced the governed v0.1.155 desktop executable and NSIS installer with the already-verified offline payload.

</topic>

<topic id="verification" status="pass" version="v1" wp="WP-0224" updated_at="2026-08-15" ingestable="true">

# Verification commands and scenarios

```text
node --import tsx --test tests/dbStartupContentionContract.test.ts tests/freezeContainmentContract.test.ts
Result: 24 passed, 0 failed.

cargo test --manifest-path product/engine/Cargo.toml readonly_open_does_not_create_uninitialized_app_dirs -- --nocapture
Result: 1 passed, 0 failed.

cargo test --manifest-path product/engine/Cargo.toml available_library_list_uses_file_status_created_index -- --nocapture
Result: 1 passed, 0 failed.

cargo test --manifest-path product/engine/Cargo.toml
Result: 538 passed, 4 ignored, 0 failed; auxiliary targets and doc tests passed.

governance/scripts/build_desktop_target.ps1 -WorkPackets WP-0224 -NoArchiveCurrent -SkipWarmupGate -SkipWarmupGateReason <recorded reason>
Result: PASS; 0.1.154 -> 0.1.155; release compile 9m43s; NSIS bundle produced.

Packaged scenario:
1. Launch product/desktop/build_target/Current/release/desktop.exe --agent-headless.
2. Confirm GET /agent/state reports agent_headless=true and app_version=0.1.155.
3. Alternate Jobs/Queue and Instagram Archiver six times.
4. Wait more than 30 seconds and capture /agent/freeze_dump.
5. Filter trace rows at or after bridge started_at_ms=1786755012516.
6. Capture and inspect the Instagram Archiver snapshot.
```

Packaged observations:

- Page-navigation wall times: 152, 10, 3, 2, 2, 2 ms.
- `library_list`: 31, 110, 19, 30 ms.
- `youtube_subscriptions_list`: 16, 23, 13 ms.
- `worker_alive`: 2 rows, approximately 30 seconds apart.
- `main_thread_alive`: 2 rows.
- App state: `agent_headless=true`, `app_version=0.1.155`, `current_page=instagram_archive`.

</topic>

<topic id="artifacts" status="complete" version="v1" wp="WP-0224" updated_at="2026-08-15">

# Referenced artifacts

- Build log: `product/desktop/build_target/logs/build_desktop_target_20260815-023938_0_1_155.log`
- Installer: `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.155_x64-setup.exe`
- Freeze report: `C:/Users/Ilja Smets/AppData/Roaming/com.voxvulgi.voxvulgi/diagnostics/traces/freeze_reports/freeze_report_1786755063248.json`
- Snapshot: `governance/snapshots/WP-0224/v0_1_155_instagram_archive_1786755097574.png`
- State dump: `governance/snapshots/WP-0224/v0_1_155_instagram_archive_1786755097638.dump.json`
- Structured receipt: `evidence.json` beside this summary.

</topic>

<topic id="caveats" status="noted" version="v1" wp="WP-0224" updated_at="2026-08-15">

# Caveats and non-blocking gaps

- The payload warmup gate was skipped because WP-0224 changed only SQLite read behavior; no Python, model, resolver, dependency, or offline-payload inputs changed. The existing verified 5.74 GB payload was reused.
- The full suite initially exposed a stale compile error in the completed WP-0268 diagnostic example. That probe was updated to use the current dedicated unclassified-count API, after which the complete suite passed.
- WP-0221's deliberate freeze/skew capture remains a separate acceptance surface and is not claimed by this proof.

</topic>
