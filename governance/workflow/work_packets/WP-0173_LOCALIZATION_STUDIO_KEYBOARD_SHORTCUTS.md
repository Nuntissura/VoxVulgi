# Work Packet: WP-0173 - Localization Studio Keyboard Shortcuts

## Metadata
- ID: WP-0173
- Owner: Codex
- Status: DONE
- Created: 2026-04-08
- Target milestone: UX Polish

## Intent

- What: Add keyboard shortcuts for the most common Localization Studio actions.
- Why: Operators perform dozens of clicks per item with no keyboard alternatives. Shortcuts reduce friction for daily production use and make the workflow feel professional.

## Scope

In scope:
- Ctrl+Enter — Start / continue localization run
- Ctrl+Shift+E — Export selected outputs
- Ctrl+Shift+R — Refresh readiness
- Ctrl+1 through Ctrl+5 — Jump to Track, Voice Basics, Run, Outputs, Artifacts
- Shortcuts only active when editor is visible and focus is not in an input/textarea/select
- Visible shortcut reference in the Workflow Map card (collapsible)

## Acceptance criteria
- All listed shortcuts work when the editor is active.
- Shortcuts do not fire when typing in text fields.
- Shortcut reference is visible in the UI.
- `npm run build` passes.

## Successor mapping note

- WP-0211's master-detail layout intentionally refines the original Ctrl+1 through Ctrl+5 fixed-section mapping to Ctrl+1 through Ctrl+8 stage selection: Captions, Translate, Speakers, Voice plan, Dub, Mix, Combine A/V, and Files. This is the current shipped mapping and preserves the packet's numbered-navigation intent against the current product structure.

## Implementation status (2026-08-15)

- Governed packaged v0.1.149 proof used real keyboard input to select Translate with Ctrl+2 and Files with Ctrl+8, proved Ctrl+3 is suppressed while a `<select>` owns focus, and proved Ctrl+Shift+R completes with the readiness-refreshed notice.
- Trusted pointer input opened the visible shortcut reference; the screenshot was visually inspected. Final source inspection confirms Ctrl+Enter and Ctrl+Shift+E dispatch to the current run/export handlers behind the same visible-editor and form-focus guards; those mutating actions were not fired against operator data during proof.
- Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0173/20260815-0010_v0_1_149/summary.md`.
