# Work Packet: WP-0222 - Reveal media file exit code truthfulness

## Status

BLOCKED

## Base Scope

- Stop showing the false error toast `"Reveal media file failed: reveal path failed with exit code Some(1). Media path copied to clipboard."` after a Reveal action that actually succeeded.
- Keep the path-copy-to-clipboard fallback for the legitimate failure path (path missing, parent unreadable, etc.).

## Operator Request Preserved

- "'Reveal media file failed: reveal path failed with exit code Some(1). Media path copied to clipboard.' but it did open the folder after."

## Research Basis

- The reveal command runs `explorer.exe /select,<file>` on Windows (`product/desktop/src-tauri/src/lib.rs:2046-2055`).
- Microsoft's community support material documents the `explorer.exe /select,<path>` syntax, but no checked official Microsoft source documents exit code 1 as a success code. The exit-1/success behavior is established here by the operator's exact runtime report: Explorer opened and selected the target while the pre-fix command surfaced `Some(1)` as an error.
- The reveal helper routed through `run_shell_command` (lib.rs:2012-2021) which treated any non-zero exit as failure, so the toast lied while Explorer correctly opened.
- Folder-only reveals (`path.is_dir()` true, no `/select,`) do not exhibit the quirk and should keep strict success semantics so a real failure surfaces.

### Rejected options

- Switching to `tauri-plugin-opener`'s `reveal_item_in_dir`: more dependencies and behavior change beyond the bug surface. Local fix is one branch in one function.
- Running through PowerShell with `Start-Process`: heavier launch and changes process lineage; not worth it for a quirk fix.

### Selected approach

- In `shell_reveal_target`, special-case the Windows `/select,` branch to treat exit code 1 as success. Keep `run_shell_command` unchanged so it still treats exit-1 as failure in every other caller.

## Reused Systems

- `shell_reveal_target` and `shell_reveal_path` command surface (`product/desktop/src-tauri/src/lib.rs:2046-2121`).
- Frontend reveal helper at `product/desktop/src/lib/pathOpener.ts:30-36` and the toast logic at `product/desktop/src/pages/LibraryPage.tsx:1596-1610`.

## Gaps Closed

- The toast no longer claims failure on a successful reveal.
- The clipboard fallback continues to fire for genuine reveal failures (path missing, normalization failure), preserving the recovery path.

## Risks And Hardening

- Risk: a real reveal failure that happens to exit 1 from Explorer is now silenced.
  - Remediation: only the `/select,` call path bypasses the exit-1 check; folder-only reveals still fail loudly. Genuine failure modes (path missing, normalization failure, OS-level launch error) surface via `command.status().map_err(...)`, not via exit code.

## Red-Team

- Failure scenario: Explorer launch is blocked by Windows policy (e.g., shell replacement) and returns exit 1.
  - Control: exit-1 still occurs but the user does see Explorer fail to come to the foreground; this is observable and they can re-trigger. Acceptable for a corner case; the previous behavior was to also silently fail with a misleading toast.
- Failure scenario: `/select,` is silently ignored by a third-party file manager that registered as the default for the URI handler.
  - Control: out of scope; the command we run is `explorer.exe` directly, not the URI handler.

## Acceptance Criteria

- Reveal Media File on a Library item that exists on disk no longer surfaces an error toast on Windows when Explorer opens successfully.
- A reveal call on a path that does not exist still produces the error toast and copies the path to clipboard (LibraryPage existing behavior unchanged).
- `cargo build --release` in `product/desktop/src-tauri` succeeds.

## Verification

- `cargo build --release` clean (covered by the desktop build pipeline).
- Manual smoke after install of v0.1.19: open Library, click "Open folder" on any imported item, confirm Explorer opens and no error toast appears.

## Status Updates

- 2026-05-17: Created packet from operator report. Implemented in the same slice as WP-0221 freeze diagnostic and WP-0220 single-video subfolder follow-up. Ships in desktop v0.1.19.
- 2026-08-15: Added a pure regression seam and Rust test proving exit 1 is accepted only for file-selection reveals; folder reveals, other codes, and missing codes remain failures. Added frontend contracts proving the genuine-failure path still copies the media path before showing the error. Rust test passed 1/1 and frontend contract passed 2/2. The packet is `BLOCKED` only on its explicit installed-app manual smoke because triggering Explorer would open a foreground window during agent testing. Partial proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0222/20260815_0530_v0_1_160/summary.md`.
