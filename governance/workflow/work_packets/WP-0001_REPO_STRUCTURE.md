# Work Packet: WP-0001 — Repo structure: product + governance

## Metadata
- ID: WP-0001
- Owner: Codex
- Status: DONE
- Created: 2026-02-19

## Intent

Create the initial repo skeleton with:

- `product/` and `governance/` split
- governance workflow artifacts (task board, roadmap, work packets folder)
- operating documentation + AI model behavior rules

## Scope

In scope:

- Add folders and baseline Markdown artifacts.
- Move existing draft specs into `governance/spec/`.

Out of scope:

- Implementing downloader code.
- Implementing any AI pipeline code.

## Acceptance criteria

- `governance/workflow/TASK_BOARD.md` exists and lists WP-0001 as DONE.
- `governance/workflow/ROADMAP.md` exists.
- `governance/templates/WORK_PACKET_TEMPLATE.md` exists.
- `PROJECT_CODEX.md` exists and explains how to operate the repo.
- `MODEL_BEHAVIOR.md` exists and documents AI rules and safety boundaries.

## Notes

After completion:

- Set WP-0001 to DONE and create the next implementation WP (likely diagnostics + library skeleton).
