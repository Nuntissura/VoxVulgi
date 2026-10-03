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
