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
