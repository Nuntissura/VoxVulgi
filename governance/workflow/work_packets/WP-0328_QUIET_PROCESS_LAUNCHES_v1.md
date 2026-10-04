---
file_id: WP-0328
file_kind: work_packet
updated_at: 2026-10-03
---

<topic id="contract" wp="WP-0328" status="IN_PROGRESS">

# WP-0328 — Quiet process launches

Status: IN_PROGRESS. Board: ../TASK_BOARD.md. Refinement: WP-0328_QUIET_PROCESS_LAUNCHES_v1_REFINEMENT.md.

Scope: shared Windows file opening, first-party background console launchers, generated FFmpeg/Git helper children, and tooling wheelhouse subprocesses. Preserve associated GUI applications, argv, process lifecycle ownership, cross-platform behavior and user data. No version/changelog changes.

Acceptance: no intermediate terminal for file/folder opening, probes, media conversion or dependency helpers. All first-party production Command/subprocess construction sites are audited, using the existing quiet helper or explicit reviewed flags. Intentional GUI opening remains functional. Windows focused test observes GetConsoleWindow()==0 in launched console helper and checks argument fidelity. Packaged app-boundary opening proof is required before DONE; a controlled opening may need an explicit foreground-test allowance.

Verification: root batches engine/desktop focused tests and one canonical-cache desktop build after all parallel inputs stabilize. Retain version 0.1.205. Record summary under build_target/tool_artifacts/wp_runs/WP-0328. Packet remains IN_PROGRESS until runtime proof; source audit and compilation alone do not prove operator opening behavior.

Microtasks: replace omitted quiet wrappers; cover nested Python subprocesses; source-audit intentional exceptions; run bundled tests; verify packaged associated-file opening and background console absence; synchronize board/status with proof.

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