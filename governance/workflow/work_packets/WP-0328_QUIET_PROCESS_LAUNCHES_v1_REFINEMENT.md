---
file_id: WP-0328-REFINEMENT
file_kind: refinement
updated_at: 2026-10-03
---

<topic id="quiet-process-launches" wp="WP-0328" status="IN_PROGRESS">

# Quiet process launches

Operator request: opening a single video must not flash a terminal; correct this throughout the app. Preserve intentional opening of the associated media player and file manager.

Spec anchors: PRODUCT_SPEC local-first background pipeline/tooling and TECHNICAL_DESIGN desktop shell integration; AGENTS GLOBAL-BUILD-QUIET and build_rules quiet verification. Existing engine `cmd::command` already applies Windows CREATE_NO_WINDOW; reuse that boundary rather than inventing a second launch framework.

Research: Microsoft https://learn.microsoft.com/en-us/windows/win32/procthread/process-creation-flags defines CREATE_NO_WINDOW for console children and states it is ignored for GUI applications or combined DETACHED_PROCESS/CREATE_NEW_CONSOLE. Rust https://doc.rust-lang.org/std/os/windows/process/trait.CommandExt.html exposes the creation flags. Python https://docs.python.org/3/library/subprocess.html exposes CREATE_NO_WINDOW. The existing cmd helper is suitable for background console children; retain intentional GUI launches and platform argv unchanged. Reject hiding associated GUI applications, broad process termination, and dependency replacement.

Inspected omissions: desktop shell_open_target's cmd/start; voice backend probe; offline proof ffprobe; generated voice-conversion Python FFmpeg; developer wheelhouse helper. Existing lifecycle/startup flags and deliberate GUI relaunches are exceptions, not unreviewed console work. Third-party vendored scripts are not shipped application launch paths and remain untouched.

Red team: hiding an app target would break Open; changing argv could break spaced/non-ASCII media paths; generated nested subprocesses can reopen consoles even when the parent is hidden. Controls: apply only console-helper creation policy, preserve argv, cover generated Python and test real Windows console absence. No foreign-process stops or user data mutation.

</topic>
