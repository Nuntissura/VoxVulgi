---
file_id: wp-0172-proof-v0-1-149
file_kind: proof_summary
updated_at: 2026-08-15
---

<topic id="outcome" status="done" version="0.1.149" wp="WP-0172" updated_at="2026-08-15">

# WP-0172 Localization Studio Built-in Manual

Status: DONE

The governed v0.1.149 desktop contains contextual help for all 22 current editor sections and all six Localization home topics. Each help surface supplies plain-language What, When, workflow steps, and concepts where the section needs a glossary. The shared Show all help toggle expands or collapses all rendered help and persists across reloads.

</topic>

<topic id="verification" status="passed" version="0.1.149" wp="WP-0172" updated_at="2026-08-15">

## Verification

- Source inspection counted 22 `SECTION_HELP` definitions, 22 matching editor `SectionHelp` controls, and six home help definitions.
- In the packaged hidden v0.1.149 WebView, Show all was initially enabled and all 22 editor help controls reported their expanded `Hide help` state.
- Trusted pointer input disabled Show all: the editor changed to 22 `Show help` controls, zero expanded controls, and localStorage recorded `voxvulgi.v1.loc.help_all=0`.
- A real WebView reload preserved the disabled state. Trusted pointer input restored the setting and localStorage recorded `1`.
- `npm run build` passed both before and inside the governed v0.1.149 build.

</topic>

<topic id="evidence" status="verified" version="0.1.149" wp="WP-0172" updated_at="2026-08-15">

## Evidence

- `evidence.json` in this directory.
- `governance/snapshots/WP-0172/show_all_help_v149_1786745314111.png`.
- `governance/snapshots/WP-0172_0173_0175_batch/final_v149_1786745652435.dump.json`.

The screenshot was opened and visually inspected. It shows v0.1.149, an expanded What/When/Typical workflow panel, the checked Show all help control, readable text, and no overlap at 800×600.

</topic>
