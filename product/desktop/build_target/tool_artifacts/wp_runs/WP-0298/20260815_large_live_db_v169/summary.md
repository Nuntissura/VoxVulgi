---
file_id: WP-0298-PROOF-20260815-LARGE-LIVE-DB-V169
file_kind: proof-summary
updated_at: 2026-08-23
---

<topic id="outcome" status="invalidated-for-closure" version="0.1.169" wp="WP-0298" updated_at="2026-08-23">

# WP-0298 exact large-database panel boundary

Status: HISTORICAL_TIMING_ONLY — INVALIDATED FOR CURRENT-PROFILE/NON-MUTATION CLOSURE

This run was invalidated for WP-0298, WP-0314, and WP-0315 closure on 2026-08-23. It launched an agent-controlled headless process against canonical operator app data before the owned-disposable-base requirement was recognized. Preserve the measurements below only as historical timing and observer-interference evidence. They do not prove safe or non-mutating current-profile panel behavior and cannot satisfy any current-profile, exact-baseline, migration-decision, or closure gate. Fresh panel evidence must attach passively to an already operator-started process with agent-observation-only behavior; any agent-started headless proof must use a preflighted owned absolute `VOXVULGI_AGENT_HEADLESS_BASE_DIR`.

The exact governed v0.1.169 executable was launched hidden with `--agent-headless` and BelowNormal priority against the canonical operator database. The database was 1,066,110,976 bytes. Headless state proved `agent_headless=true` and `app_version=0.1.169`; the job runner, startup subscription sync, hydration, relocation, and watcher supervisor were therefore disabled.

A clean Options -> Media Library switch completed without a `freeze_detected` event. The panel committed in 85 ms with 964 controls and 162 table rows mounted; one 68 ms frontend long task was recorded. The asynchronous `library_query` completed in 3,105 ms and did not block the initial panel commit.

</topic>

<topic id="verification" status="historical-only" version="0.1.169" wp="WP-0298" updated_at="2026-08-23">

## Verification

- Exact executable SHA-256: `D176AC9525F301575C93B59D6D66FB5AC080261C931519423277673B5F79E38F`.
- Exact canonical database: `%APPDATA%/com.voxvulgi.voxvulgi/db/app.sqlite`, 1,066,110,976 bytes.
- Clean trace window started at Unix ms `1786814989193`.
- `panel_switch_rendered`: 85 ms, 964 controls, 162 rows, 800x600.
- `frontend_long_task`: 68 ms.
- `library_query`: 3,105 ms in `db_open_prepare_step_map` plus serialization.
- Clean trace window: zero `freeze_detected`, one `panel_switch_rendered`, one `frontend_long_task`.
- A separate screenshot-capture run produced two Worker freezes (1,091 ms and 339 ms) while `library_query` remained in flight. Because the clean switch did not freeze and the freezes began during html2canvas proof capture, these events are classified as proof-tool self-interference, not evidence that plain panel navigation froze.
- Canonical self-contained report: `%APPDATA%/com.voxvulgi.voxvulgi/diagnostics/traces/freeze_reports/freeze_report_1786815031962.json` (1,000 trace rows).

</topic>

<topic id="artifacts-and-remaining" status="historical-only" version="0.1.169" wp="WP-0298" updated_at="2026-08-23">

## Artifacts

- Snapshot: `governance/snapshots/WP-0298/large_library_panel_v169_1786814925791.png`, SHA-256 `A0CDBD8C2D247D212A9866CB365076B10D944F7B35F1D5F1C8BB74BEAB34574C`.
- Paired dump: `governance/snapshots/WP-0298/large_library_panel_v169_1786814925828.dump.json`, SHA-256 `DD0150F987E37434F622DE987AE41615B360297BBBD7A83508012C6F22C2FE6C`; v0.1.169, Media Library, zero console entries.
- Visual inspection: the 800x600 Media Library header and controls were readable and non-overlapping.

## Remaining closure gates

- Re-run the exact current-profile panel observation only by attaching to an already operator-started process without agent navigation, clicks, mutation, or process control. The historical run above cannot close this gate.
- Build the operation-specific diagnostics span remediation already committed after the foreign compiler load clears.
- Re-probe the packaged remediation and prove distinct download/enumeration request/span IDs.
- Exercise and measure the exact operator job-start case without starting unrelated work.
- Reconcile the 3.1-7.7 second large-database query cost with the packet's response-time target and remediate if the normative threshold is exceeded.

</topic>
