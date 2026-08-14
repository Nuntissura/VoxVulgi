---
file_id: wp-0177-proof-v0-1-149
file_kind: proof_summary
updated_at: 2026-08-15
---

<topic id="outcome" status="done" version="0.1.149" wp="WP-0177" updated_at="2026-08-15">

# WP-0177 Glossary and Custom Term Mapping

Status: DONE

VoxVulgi ships versioned global and per-item term mappings with per-item override precedence, optional context/notes, atomic persistence, JSON/CSV import/export, source-text highlighting, queue-time effective-glossary snapshots, and bounded Whisper prompt plumbing. Global and item CRUD are available in Localization Studio.

</topic>

<topic id="verification" status="passed" version="0.1.149" wp="WP-0177" updated_at="2026-08-15">

## Verification

- Existing focused engine verification passed seven tests, including global/item persistence, effective precedence, prompt plumbing, invalid item traversal, and invalid term boundaries.
- `cargo check --locked -j 1 --manifest-path product/desktop/src-tauri/Cargo.toml` and `npm run build` passed before the governed build; the same source shipped in governed v0.1.149.
- The exact packaged item began with zero global, item, and effective terms.
- Trusted UI input added `뭐 → what-wp0177-probe` with context and notes. A native `glossary_get` reread returned the exact entry.
- The Captions segment view rendered a visible `<mark>` over `뭐` with title `Glossary: what-wp0177-probe`; the screenshot was visually inspected.
- Packaged native JSON export wrote one complete entry. Trusted UI removal restored the empty baseline. Packaged native import then restored the same full entry and the UI loaded it after a real WebView reload.
- A final trusted UI removal restored the exact empty global/item/effective baseline; the matching temporary export was hash-verified and deleted. The final dump has zero console errors.

</topic>

<topic id="evidence" status="verified" version="0.1.149" wp="WP-0177" updated_at="2026-08-15">

## Evidence

- `evidence.json` in this directory.
- `governance/snapshots/WP-0177/glossary_highlight_v149_1786746019554.png`.
- `governance/snapshots/WP-0177/final_restored_v149_1786746139726.dump.json`.

The inspected screenshot shows the real Korean source term highlighted in yellow in the v0.1.149 subtitle editor, with timing, speaker, translation, and quick-action context visible and no overlap.

</topic>
