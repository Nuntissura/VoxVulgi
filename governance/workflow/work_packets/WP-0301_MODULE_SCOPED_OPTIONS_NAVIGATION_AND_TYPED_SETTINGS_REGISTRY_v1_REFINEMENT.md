---
file_id: WP-0301-REFINEMENT-v1
file_kind: work-packet-refinement
updated_at: 2026-08-09
---

<topic id="operator-request-and-verified-state" status="active" version="v1" wp="WP-0301" updated_at="2026-08-09">

# Operator request

- Add sub-tabs/settings navigation for every module.
- Give YouTube, Instagram, TikTok, Media Library, Jobs, Diagnostics, and the existing modules clear settings ownership without another chaotic long page.

# Verified current state

- `OptionsPage.tsx` is approximately 84 KB and renders one long document.
- Current sections are Readability, YouTube sign-in, Instagram sign-in, Import from 4K Video Downloader, Download speed vs. safety, subscription interval, save locations, and feature folders.
- WP-0169 consolidated feature roots but did not create module subnavigation or a typed settings registry.
- Existing persisted values and engine consumers are distributed across current config, localStorage, commands, and runtime settings; relocating controls must not silently create new keys or disconnected settings.
- Nine equal horizontal tabs do not fit the proven 800x600 app boundary. W3C guidance also warns that automatic tab activation is inappropriate when panel display has noticeable latency.

# Authority and dependencies

- Spec anchors: PRODUCT_SPEC 8.2; TECHNICAL_DESIGN 6.6.
- Preserve: WP-0169 Options consolidation, WP-0220 feature roots, WP-0266/WP-0267 authentication, build no-card rule, headless semantic audit.
- This packet is a product settings foundation; provider packets add their fields through its registry instead of adding new page-local blocks.

# Scope edges

- In scope: accessible module navigation, typed registry, existing-setting inventory/migration map, settings search, baseline/effective display support, module reset, validation/test receipts, responsive layout, stable IDs, manual/help updates.
- Non-goals: changing the meaning/default of existing settings, implementing provider behavior owned by WP-0299/WP-0303/WP-0304, moving repo governance into the product, adding cards, or duplicating settings in both page and module panes.

</topic>

<topic id="research-basis-and-selected-design" status="active" version="v1" wp="WP-0301" updated_at="2026-08-09">

# Sources checked

- Current `OptionsPage.tsx`, config/runtime-settings commands, current persistence keys, Options-related WPs, headless bridge semantics, and 800x600 packaged screenshots.
- WAI-ARIA tabs pattern and latency/keyboard guidance: `https://www.w3.org/WAI/ARIA/apg/patterns/tabs/`.
- WAI-ARIA role/state requirements: `https://www.w3.org/TR/wai-aria/`.
- Current VoxVulgi no-new-cards, responsive, quiet headless verification rules.

# Selected navigation

- Module destinations: General, Localization Studio, Video Archiver, Instagram Archiver, TikTok Archiver, Image Archive, Media Library, Jobs/Queue, Diagnostics.
- Wide layout: compact vertical module navigation rail plus one content pane.
- Narrow layout: labeled native select/combobox or compact overflow-safe module control; do not force nine horizontal tabs.
- Module selection is addressable through stable page state, survives page switches/restart where current conventions permit, and restores focus/scroll predictably.
- Settings search spans registry labels/help/keywords and returns module/section matches without mounting every module pane.

# Typed settings registry contract

- Stable setting ID and owning module.
- Current persistence key/source and backward-compatible aliases/migration.
- Type/schema, validation, default, allowable range/options, and secret/redaction class.
- Saved operator baseline and effective runtime value; optional temporary policy overlay/source.
- Restart requirement and current dirty/invalid state.
- Plain-language label/help, advanced flag, search keywords, and related setting IDs.
- Reset action scope and optional exact capability/test command returning a structured receipt.
- Product/test IDs for headless semantic audit.

# Existing-setting migration method

1. Inventory every current Options control, persistence key, writer, reader/consumer, default, validation, and restart behavior.
2. Map each control exactly once to a module and section; shared paths/readability belong to General unless a current product contract requires module ownership.
3. Add registry adapters around existing persistence first; do not rename keys merely to match UI organization.
4. Render one registry-owned control; remove the old duplicate only after round-trip parity tests pass.
5. Record unmapped readers/writers as blockers; do not declare migration complete from visual presence alone.

# UI rules

- One content pane, section headings and rows; no card-per-setting or card-per-module design.
- Primary save/apply behavior remains consistent with current persistence semantics; unsaved changes are visible.
- Advanced controls are discoverable but secondary.
- Effective adaptive overrides are shown read-only beside the saved baseline with source/reason, not written into the field.
- Module-level reset enumerates affected settings before confirmation and never deletes user libraries/subscriptions/media.
- Session/capability tests show running/success/failure/stale state and return a diagnostic receipt; they do not start downloads unless the owning packet explicitly defines an exact-source test.

# Existing systems reused

- Current Options controls/config commands, feature-root table, readability/auth/session flows, localStorage conventions, diagnostics trace, built-in help, and headless stable IDs/audit.

# Rejected options

- Nine horizontal tabs: overflow and discoverability failure at 800px.
- Keep the long page and add anchors: does not create ownership, search, validation, or lazy module mounting.
- Rewrite every persistence key: unnecessary migration risk and breaks old installs/queued runtime consumers.
- Duplicate a setting in General and a module: creates conflicting source of truth.
- Auto-save every field without respecting current semantics: can turn typing/validation into runtime mutations.

</topic>

<topic id="roi-red-team-microtasks-and-proof" status="active" version="v1" wp="WP-0301" updated_at="2026-08-09">

# High-ROI additions

- Settings search: reuses registry metadata and makes future provider additions cheap to find.
- Baseline/effective/source model: reuses WP-0299 adaptive overlay and prevents UI confusion.
- Structured capability-test receipts: reuse diagnostics and let no-context models verify settings without screen guessing.
- Stable setting IDs and dependency links: reduce future migrations and enable targeted headless audits.
- Module reset/dirty state: prevents accidental loss and clarifies operator decisions while the registry already knows scope.

# Risks, failure scenarios, controls, and verification

- Control moves but no longer persists to the engine consumer.
  - Control: inventory writer/reader and round-trip tests per setting.
  - Verify: save, restart, read engine effective state, and UI reload.
- Duplicate settings diverge.
  - Control: registry owns one rendering; old surface removed in same slice after parity.
  - Verify: semantic inventory and source search for duplicate labels/keys.
- Narrow layout hides modules or actions.
  - Control: native compact selector, local content scroll, sticky module label/save status.
  - Verify: 800x600 and wider snapshots/audits, keyboard-only navigation.
- Automatic tab activation triggers slow data loads.
  - Control: settings panes are local/lightweight; use explicit activation if a pane cannot meet latency target.
  - Verify: focus/activation timing and WAI-ARIA keyboard contract.
- Reset clears secrets/data beyond settings.
  - Control: registry reset allowlist and exact preview receipt; no broad config-directory deletion.
  - Verify: before/after files/keys and preservation of library/subscription counts.
- Adaptive effective value is mistaken for saved baseline.
  - Control: distinct labels, read-only overlay, reset affects baseline only unless policy action explicitly chosen.
  - Verify: WP-0299 overlay matrix.
- Registry becomes a second config source.
  - Control: registry describes/routes current canonical persistence; it does not cache independent values.
  - Verify: one source-of-truth assertion per setting.

# Microtask plan

1. Inventory existing settings, persistence keys, consumers, defaults, validation, and duplication.
2. Add registry types/schema, migration map, and focused round-trip tests.
3. Implement responsive module navigation, addressable selection, and settings search.
4. Migrate General, Video Archiver, Instagram, Image Archive, Media Library, Jobs, Diagnostics, and Localization existing controls one module at a time.
5. Add empty/disabled TikTok slots only when WP-0304 fields are implemented; do not ship a misleading ready pane.
6. Add dirty/reset/effective/capability receipt surfaces and built-in model manual updates.
7. Run semantic inventory diff, keyboard/responsive proof, restart persistence, build, and proof bundle.

# Acceptance and proof gates

- Every pre-change Options control is mapped exactly once or explicitly documented as retired/superseded by authority.
- Existing persistence keys and engine-effective behavior survive migration or pass an explicit tested migration.
- All nine module destinations are reachable without horizontal overflow; unavailable future settings are truthfully labeled.
- Search, keyboard navigation, focus, scroll restoration, dirty state, validation, reset preview, and capability receipts pass.
- No card count increase; the new surface reduces long-document mounting.
- Frontend/Rust/config tests, restart proof, TypeScript/build, governed version/changelog, 800x600 and wide headless audit/snapshots/dumps, and proof `summary.md` pass.

</topic>
