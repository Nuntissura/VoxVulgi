# Work Packet: WP-0076 - Reusable voice template library

## Metadata
- ID: WP-0076
- Owner: Codex
- Status: DONE
- Created: 2026-03-06
- Target milestone: Phase 2 (voice-preserving dubbing usability hardening)

## Intent

- What: Add a reusable voice-template library so operators can save per-speaker voice mappings/reference clips from one item and apply them to later items with explicit speaker-slot mapping.
- Why: Current voice settings are item-local only. For recurring shows, hosts, and panel formats, operators need to reuse known voice-clone setups across episodes without rebuilding every speaker mapping by hand.

## Scope

In scope:

- Add an app-managed reusable voice-template store in app data for named templates and copied speaker reference clips.
- Save the current item's speaker settings into a reusable template.
- Apply a saved template to another item through explicit current-speaker -> template-speaker mapping.
- Keep existing TTS preview / neural TTS / voice-preserving jobs working by resolving back to the same item speaker settings they already consume.
- Add Localization Studio UI for listing, saving, reviewing, applying, deleting, and revealing voice templates.
- Extend guidance text so operators understand how to reuse saved templates for recurring series.

Out of scope:

- Automatic speaker-identity matching across items.
- Training new voice models or replacing the current Kokoro/OpenVoice stack.
- Cloud dubbing providers or any network-dependent fallback.
- External JSON import/export of templates (can be a follow-up WP if needed).

## Acceptance criteria

- An operator can save a named voice template from an item that already has per-speaker display names / voice ids / voice reference clips set.
- Template reference clips are copied into an app-managed template folder, so later reuse does not depend on the original picked file still existing in its original location.
- An operator can apply a saved template to another item by explicitly mapping current speakers to template speakers.
- After apply, the existing preview/dub jobs consume the applied settings without any new manual file picking.
- Localization Studio makes the template workflow discoverable enough to create a first reusable voice setup without external docs.

## Implementation notes

- Prefer keeping job request formats stable by writing applied template values back into `item_speaker`.
- Store reusable template metadata in SQLite and reference clip assets under app data.
- Preserve item-local overrides: template apply should update selected speakers only, not silently wipe unrelated speaker settings.

## Test / verification plan

- `cargo test` in `product/engine`
- `cargo test` in `product/desktop/src-tauri`
- `npm -C product/desktop run build`
- Add engine tests for template persistence and apply behavior.
- Manual smoke:
  - save a template from an item with configured speaker clips,
  - apply it to another item,
  - confirm the applied item can queue voice-preserving dub without re-picking speaker clips.

## Risks / open questions

- Speaker keys are item-local and diarization labels are not identity-stable, so explicit mapping UI is required.
- Reference clips should be copied, not merely referenced, or template reuse will break when the original clip path disappears.

## Status updates

- 2026-03-06: Created for reusable voice-clone/template reuse after WP-0074/WP-0075 stabilized the localization output and voice-preserving runtime baseline.
- 2026-03-06: Implemented app-managed reusable voice-template storage in SQLite/app data, added template create/get/list/apply/delete Tauri commands, and extended Localization Studio with save/apply/delete/reveal controls plus explicit current-speaker -> template-speaker mapping.
- 2026-03-06: Verified with `cargo test` in `product/engine`, `cargo test` in `product/desktop/src-tauri`, and `npm -C product/desktop run build`. Proof artifacts were written under `product/desktop/build_target/tool_artifacts/wp_runs/WP-0076/20260306_132316/`, including `summary.md`, `engine_cargo_test.log`, `tauri_cargo_test.log`, and `desktop_build.log`.
