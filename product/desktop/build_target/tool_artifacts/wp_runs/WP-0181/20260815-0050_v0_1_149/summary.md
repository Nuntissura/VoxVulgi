---
file_id: wp-0181-proof-v0-1-149
file_kind: proof_summary
updated_at: 2026-08-15
---

<topic id="outcome" status="done" version="0.1.149" wp="WP-0181" updated_at="2026-08-15">

# WP-0181 Pipeline Presets

Status: DONE

VoxVulgi v0.1.149 ships the packet-defined Japanese Anime, Korean Variety, and Quick Subtitles Only built-ins plus atomic custom preset CRUD. Applying a preset configures source language, global batch-on-import rules, per-item translation style and honorific handling, and stores a full item snapshot. Selected voice-template or cast-pack defaults remain pending until speaker labels exist and then apply once.

</topic>

<topic id="verification" status="passed" version="0.1.149" wp="WP-0181" updated_at="2026-08-15">

## Automated verification

- `cargo test --manifest-path product/engine/Cargo.toml --lib jobs::tests::pipeline_preset_voice_template_waits_for_speakers_then_applies_once -- --exact --nocapture` — 1 passed, 0 failed, 539 filtered out.
- The focused test uses a temporary `AppPaths` root. It proves the item snapshot starts pending, an input without speaker labels does not apply or mark defaults, a later speaker label applies the configured voice template, the mapped voice settings persist, the applied flag persists, and a second invocation is a no-op.
- Source-final gates already recorded in the packet: `npm run build`; `cargo check --locked -j 1 --manifest-path product/desktop/src-tauri/Cargo.toml`; and 3 targeted configuration tests covering exact built-ins, custom CRUD/item snapshots, immutable built-ins, traversal/control-character rejection, and custom-instruction validation.
- `git diff --check` passed on the final WP-0181 change set.

## Governed build

- Governed v0.1.149 includes WP-0181 in `governance/release/BUILD_CHANGELOG.md`.
- Installer: `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.149_x64-setup.exe`.
- Build log: `product/desktop/build_target/logs/build_desktop_target_20260814-234737_0_1_149.log`.

## Packaged hidden-WebView scenario

1. Launched the governed `desktop.exe --agent-headless` at below-normal priority. `/agent/state` returned `agent_headless=true`, `app_version=0.1.149`, `current_page=localization`, and no selected editor item.
2. Ran the semantic UI audit at 800x600. The Pipeline preset selector exposed all three exact built-ins and the audit reported zero unnamed interactive controls.
3. Used only allowlisted headless UI actions to scroll the selector into view and open `Save or edit a custom preset`.
4. Re-audited after opening. The rendered editor exposed Preset name, Translation tone, Honorifics, Default voice template, Default voice cast pack, Save as new, Update custom preset, and Delete custom preset.
5. Captured two screenshots and a structured dump. Both screenshots were opened and visually inspected; fields and controls are readable, aligned, unclipped within their scrolled viewport, and free of overlap. The dump contains zero console entries.
6. No mutating preset action was executed against live app data. After stopping only the exact proof PID, the previously absent preset catalog, batch-rule file, and item snapshot remained absent; the existing item translation-style file remained 80 bytes with SHA-256 `647B3C44B758684D2D07361862E89F8FCEA45B304007DFB239D0B6762E9D683A`.

</topic>

<topic id="evidence" status="verified" version="0.1.149" wp="WP-0181" updated_at="2026-08-15">

## Evidence

- Structured receipt: `evidence.json` in this directory.
- Focused test output: `deferred_voice_test.log` in this directory.
- Selector screenshot: `governance/snapshots/WP-0181/pipeline_presets_v149_1786747528549.png`.
- Custom editor screenshot: `governance/snapshots/WP-0181/pipeline_preset_editor_v149_1786747556821.png`.
- State dump: `governance/snapshots/WP-0181/pipeline_presets_v149_1786747528558.dump.json`.

</topic>

<topic id="caveats" status="none-blocking" version="0.1.149" wp="WP-0181" updated_at="2026-08-15">

## Caveats and residual risk

- The packaged UI scenario was intentionally read-only because the desktop app has no test-only native app-data override and the operator's live preferences must not be mutated. Custom persistence is proven at the native engine boundary in temporary roots; the packaged scenario proves the actual v0.1.149 selector/editor surface and safe interaction path.
- Voice-template matching is covered directly. Cast-pack defaults use the parallel deferred branch and existing cast-pack application API, but this closure did not execute a second full cast-pack fixture because the packet's remaining risk was the shared defer/apply-once state transition.

</topic>
