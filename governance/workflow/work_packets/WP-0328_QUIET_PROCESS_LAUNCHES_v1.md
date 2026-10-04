---
file_id: WP-0328
file_kind: work_packet
updated_at: 2026-10-03
---

<topic id="contract" wp="WP-0328" status="DONE">

# WP-0328 â€” Quiet process launches

Status: DONE. Board: ../TASK_BOARD.md. Refinement: WP-0328_QUIET_PROCESS_LAUNCHES_v1_REFINEMENT.md.

Scope: shared Windows file opening, first-party background console launchers, generated FFmpeg/Git helper children, and tooling wheelhouse subprocesses. Preserve associated GUI applications, argv, process lifecycle ownership, cross-platform behavior and user data. No version/changelog changes.

Acceptance: no intermediate terminal for file/folder opening, probes, media conversion or dependency helpers. All first-party production Command/subprocess construction sites are audited, using the existing quiet helper or explicit reviewed flags. Intentional GUI opening remains functional. Windows focused test observes GetConsoleWindow()==0 in launched console helper and checks argument fidelity. Packaged app-boundary opening proof is required before DONE; a controlled opening may need an explicit foreground-test allowance.

Verification: root batches engine/desktop focused tests and one canonical-cache desktop build after all parallel inputs stabilize. Retain version 0.1.205. Record summary under build_target/tool_artifacts/wp_runs/WP-0328. Packet remains IN_PROGRESS until runtime proof; source audit and compilation alone do not prove operator opening behavior.

Microtasks: replace omitted quiet wrappers; cover nested Python subprocesses; source-audit intentional exceptions; run bundled tests; verify packaged associated-file opening and background console absence; synchronize board/status with proof.

</topic>

<topic id="installed-positive-opening-20261004" status="DONE" wp="WP-0328" updated_at="2026-10-04">

Installed source 762180ae52ff42746ea38c340fbb127b041369f7, version 0.1.205, native executable SHA256 6ca8c144164446adce2177efd2c07fffb5a8b81a1a3c9e2434408628e0b8828c. Silent Update preserved all 14 protected tables; independent closed backup reconciliation matched all 69 tables. Exact canonical item e794ce4f-eb14-49d9-a0da-126129c6e380, job 997c312e-40b5-4951-8727-b3a3214e2550 attempt 2, source https://www.youtube.com/watch?v=3Q61HdKKJeo was activated through its declared Open action at 1791131459762.

Evidence directory: product/desktop/build_target/tool_artifacts/wp_runs/WP-0332/20261004/checkpoint_shutdown_batch_connection_owner/associated_open_02. Native VLC PID228564 was independently observed as a new child of owned VoxVulgi PID224440 with the exact canonical filename argument. An initial process-title read selected its `VLC media player updates` dialog; later exact-PID window inventory and quiet PrintWindow capture prove the distinct visible main window titled `____3Q61HdKKJeo.mkv - VLC media player`, rendered video and playback03:03. The update dialog coexists; the pre-existing VLC PID253760 remains untouched. Continuous WinEvent/10ms window enumeration observed no new visible console during its 12-second observation; this bounded observation is not a claim about every transient process. The separate process observer failed after readiness due bare Boolean literals; its failed receipts are preserved and the helper is corrected. Attempt01 failed before activation because Windows denied WMI process-event subscription.

Final acceptance: independently opened extended quiet-child raw test on sourcec798a25f4e1a6f1d67ed8d5e04ee478afc267eb3 passes1, asserting GetConsoleWindowNULL and all six literal argv values. Retained `owning_quiet_tests/test_1.log` proves missing literal-path1PASS; earlier `test_0.log` proves console-only and supplies no argv proof. New rawlog/hash/receipt: `WP-0328/20261004/quiet_argv_c798a25.log` and `.json`. Installed762 production opener/quiet paths remain unchanged. Exact associated-video GUI and independent bounded window observation pass. All unchanged acceptance passes/statusDONE, with failed observers and transient-process limitations retained. Final evidence: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0328/20261004/summary.md` and `evidence.json`. No player/dialog stopped or dismissed, unrelated queue resumed, or version/changelog changed.

</topic>

<topic id="shell-parser-remediation" status="IN_PROGRESS" wp="WP-0328" updated_at="2026-10-03">

Independent hidden parser probes proved cmd/start interprets unspaced ampersands and expands percent-delimited environment names inside filenames. Replace that shared Windows open path with the existing pinned tauri_plugin_opener::open_path default-association API. Local opener2.5.3 enables open5.3.3 shellexecute-on-windows; its detached Windows implementation passes literal UTF-16 lpFile to ShellExecuteExW. Microsoft reference: https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shellexecuteexw. No new dependency or shell escaping layer. Missing special-character paths must fail before launching; actual associated-player GUI proof remains outstanding. Root interrupted only its own candidate build and descendants after the late finding; no foreign process was stopped.

</topic>

<topic id="actual-associated-file-opening-proof-review-20261003" status="IN_PROGRESS" wp="WP-0328" updated_at="2026-10-03">

## Findings and insights

- WP-0328-F-20261003-001: Final shared opener uses pinned tauri_plugin_opener::open_path/ShellExecuteExW literal paths; production background children use the quiet helper or reviewed Windows flags. Focused no-console/argument checks and missing special-character path refusal have proof. Evidence: product/desktop/build_target/tool_artifacts/wp_runs/WP-0328/20261003/summary.md.
- WP-0328-I-20261003-001: Source audit, missing-path refusal and console-helper tests do not prove actual associated-player opening. A GUI player intentionally opening is distinct from the unwanted intermediate terminal.

## Still to check

- WP-0328-C-20261003-001: Open the actual single-video file through the current packaged app action; prove the associated player opens and no intermediate console appears. Foreground-test permission remains pending; no permission is inferred from recording this check.
- WP-0328-C-20261003-002: Verify relevant file/folder paths and generated helper descendants remain quiet and preserve argument fidelity/lifecycle; bind final executable identity and preserve existing intentional GUI behavior. No broad process termination permitted. Status remains IN_PROGRESS.

</topic>

<topic id="controlled-associated-opening-authority-20261004" status="IN_PROGRESS" wp="WP-0328" updated_at="2026-10-04">

- WP-0328-A-20261004-001: The operator's AFK/full waiver authorizes completion and controlled associated-player GUI opening for the packaged proof. Keep native keyboard/mouse simulation out of this workflow; do not stop existing or foreign player processes.
- WP-0328-F-20261004-002: Installed source 9e8fc893 exposes the actual single-video history Open/Reveal handlers but lacks declared semantic actions. Its bridge capabilities/manual have no corresponding backend open command; generic ordinary buttons are not safe semantic activation targets.
- WP-0328-R-20261004-001: Add per-item `media.open.<item-id>` and `media.reveal.<item-id>` declarations with `reversible_state_change` to those existing history buttons, preserving handlers, busy behavior and layout. Reuse the established audited product-action mechanism and pinned associated-file opener; no raw IPC or generic selector bypass.
- WP-0328-C-20261004-003: Root builds and installs the candidate, then activates the exact canonical video's declared Open action through a fresh authenticated audit. An independently running process/window event observer must establish the associated GUI outcome and absence of intermediate consoles, preserving candidate/file identity and existing processes. Source annotation alone does not satisfy runtime acceptance. Status remains IN_PROGRESS.

</topic>
