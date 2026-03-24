# Work Packet: WP-0159 - Voice clone basics installer-state smoke and proof

## Metadata
- ID: WP-0159
- Owner: Codex
- Status: BACKLOG
- Created: 2026-03-24
- Target milestone: Educational-core voice-clone recovery

## Intent

- What: Add a proof-driven installer-state verification path for the reusable voice basics contract: capture voice, reuse later, dub translated output, and verify clone-vs-fallback truth.
- Why: The engine has unit coverage for templates, reusable profiles, and generated references, but the educational-core promise still needs one end-to-end operator proof path that shows later-item reuse and truthful cloned-voice results.

## Scope

In scope:

- Define an installer-state smoke that starts from a normal Localization Studio flow.
- Cover reusable-voice capture from one item or speaker setup.
- Apply that reusable asset to a later translated item.
- Run the dub path and verify whether conversion truly occurred.
- Capture proof artifacts and operator-facing notes.

Out of scope:

- Shipping new reusable-voice features beyond what the smoke requires.
- Declaring broader localization recovery complete on build-only evidence.

## Acceptance criteria

- A normal installer-state flow can capture a reusable voice, apply it to a later translated item, and produce a dubbed output with explicit clone/fallback truth.
- The proof bundle includes reusable asset state, the later-item apply path, the resulting dubbed artifacts, and the clone/fallback evidence.
- This packet becomes the closeout gate for the educational-core reusable voice basics path rather than relying on indirect unit tests alone.

## Test / verification plan

- Installer-state or installer-state-equivalent app-boundary smoke.
- Supporting engine/Tauri verification where the smoke uncovers seam regressions.
- Proof bundle under `product/desktop/build_target/tool_artifacts/wp_runs/WP-0159/`.

## Risks / open questions

- Manual proof is slower, but without it the reusable-voice educational core can regress while still passing local unit tests.
- A realistic multi-item sample set is required to validate "reuse later" instead of only same-item cloning.

## Status updates

- 2026-03-24: Created as the proof gate for reusable voice basics after inspection confirmed that local reusable-asset tests alone are not enough.
