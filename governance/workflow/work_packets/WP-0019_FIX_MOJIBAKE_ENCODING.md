# Work Packet: WP-0019 - Fix mojibake encoding artifacts

## Metadata
- ID: WP-0019
- Owner: Codex
- Status: DONE
- Created: 2026-02-22
- Target milestone: Phase 1 (repo hygiene)

## Intent

- What: Replace mojibake sequences (for example `â€”`, `â€œ...â€`, `â†’`, `â€“`) with readable equivalents.
- Why: Improve repo readability and prevent confusing characters in docs/UI.

## Scope

In scope:

- Fix mojibake occurrences in governance docs and UI strings.
- Keep Rust source ASCII where possible by using Unicode escapes in string literals when matching non-ASCII markers.

Out of scope:

- Repo-wide punctuation normalization in all docs (beyond the mojibake instances).
- Editing third-party/vendor code.

## Acceptance criteria

- No remaining mojibake sequences in:
  - `MODEL_BEHAVIOR.md`
  - `governance/spec/TECHNICAL_DESIGN.md`
  - `governance/workflow/work_packets/WP-0003_DOWNLOADER_COMPLIANCE_DESIGN.md`
  - `product/desktop/src/pages/SubtitleEditorPage.tsx`
  - `product/engine/src/image_batch.rs`
- `product/engine/src/image_batch.rs` still matches the intended pagination markers.

## Status updates

- 2026-02-22:
  - Cleaned mojibake punctuation sequences in docs/UI and updated Rust markers to use Unicode escapes.
