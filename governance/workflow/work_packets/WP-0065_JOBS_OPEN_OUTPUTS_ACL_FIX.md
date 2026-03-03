# Work Packet: WP-0065 - Jobs "Open outputs" ACL fix and reliable folder opening

## Metadata
- ID: WP-0065
- Owner: Codex
- Status: DONE
- Created: 2026-03-03
- Target milestone: Stabilization sprint (usability + reliability)

## Intent

- What: Fix the Jobs Queue "Open outputs" action so it reliably opens output folders/files without ACL errors.
- Why: Current error (`Command plugin:opener|open_path not allowed by ACL`) breaks a core post-job workflow and undermines trust.

## Scope

In scope:

- Audit all UI actions that trigger "open output/artifact path" behavior.
- Align Tauri opener/shell permissions with expected command usage.
- Add safe path checks and explicit error messages when a path is blocked.
- Ensure parity across Jobs, Library artifact links, and Diagnostics links.

Out of scope:

- Broad permissions expansion beyond output/artifact/relevant app-data paths.
- New file manager features beyond "open/reveal/copy path".

## Acceptance criteria

- Clicking "Open outputs" from Jobs succeeds for valid output paths on Windows.
- No ACL-denied error appears for supported open actions.
- Blocked/invalid paths show actionable feedback and a copy-path fallback.

## Test / verification plan

- Manual smoke: completed job -> "Open outputs" from Queue and item detail.
- Negative test: intentionally blocked path returns explicit warning.
- `cargo test` in `product/desktop/src-tauri` and relevant engine tests.
- `npm run build` in `product/desktop`.

## Status updates

- 2026-03-03: Created.
- 2026-03-03: Implemented opener ACL fix (`opener:allow-open-path`), centralized path-open fallback (`open_path` -> `reveal_item_in_dir`), and explicit copy-path fallback messaging across Jobs/Library/Subtitle Editor/Diagnostics. Verified with desktop build + tauri cargo check.
