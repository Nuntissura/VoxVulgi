# Work Packet: WP-0259 - Remove legacy wording, unify old/new, relocate 4KVDP import to Settings

## Status

IN_PROGRESS (authored + grounded by audit; implementation this session, NO build)

## Owner

Claude (Opus 4.8)

## Operator Request Preserved

- 2026-07-01: "the concept of grouped subscriptions is ok, this was done because of the legacy subscriptions. but i want to remove all legacy wording out of the app because for me the new and old are the same. they should be handled the same, if i export the complete database it should not know what is legacy and what is new. the only thing that perhaps needs to be under settings is importing my old playlists and subscription (or any other from the 4K downloader app)."

## Intent

Make old (4K Video Downloader-imported) and new (VoxVulgi-downloaded) subscriptions/videos indistinguishable to the user: no "legacy" wording anywhere in the UI, no legacy/new distinction in exports, and the ONLY 4KVDP-specific surface is a clearly-labeled "Import from 4K Video Downloader" tool under Options. Keep the general grouping feature (operator likes it). Preserve all data — this is a wording/relocation change, never a data reset.

## Research Basis

Read-only legacy audit (subagent, 32 tool-uses) across `product/desktop/src` + `product/engine/src`. Verified findings:
1. The three `LEGACY_4KVDP_GROUP_*` groups are created ONLY inside the explicit, user-triggered `import_youtube_subscriptions_4kvdp_state()` (`subscriptions.rs:1764-1768`), NOT on startup/auto-sync. No auto-creation to stop.
2. The WP-0249 library metadata export does NOT serialize `origin` (`library.rs:14-28` struct has no origin field; `library_item_from_row` reads cols 0-12). Exports already carry no legacy/new distinction for library items.
3. The ONLY export vector revealing legacy vs new is subscription `group_ids` -> group NAMES ("Legacy 4KVDP*"). Renaming the groups neutralizes the export.
4. The big LibraryPage "Legacy archive import" card is DEAD CODE, gated `{false && showVideoIngest && advancedMode ...}` at `LibraryPage.tsx:3306`, spans ~3306-3583. Safe to delete.
5. Options already hosts the full 4KVDP importer ("Advanced Recovery" card, `OptionsPage.tsx:666-764`) wiring the same four Tauri commands. "Move to Settings" = rename that card + remove the LibraryPage duplicates.
6. Live duplicate 4KVDP controls remain in LibraryPage's "Import / export & migration" `<details>` (`LibraryPage.tsx:4256-4277`): "Import 4KVDP exports" + "Import existing downloads" — remove (functionality lives in Options).

## Scope

### 2a - Neutralize the auto-created group names (engine: `subscriptions.rs`) + one-time in-place rename
- Rename the display constants `subscriptions.rs:34-36`: `LEGACY_4KVDP_GROUP_ALL/SUBSCRIPTIONS/PLAYLISTS` values `"Legacy 4KVDP*"` -> neutral `"Imported"`, `"Imported subscriptions"`, `"Imported playlists"`. Future imports get neutral names (get-or-create-by-name at `2085-2107`, so old groups are untouched by new imports).
- Because the operator EXPLICITLY asked to remove all legacy wording *from the app* and these names render in the UI (`groupNameById`), add an idempotent, surgical, in-place normalization that renames ONLY groups whose name exactly equals the three old legacy constants -> the neutral equivalents. Rename (not delete); preserves group id + all `youtube_subscription_group_member` rows. Skip if a neutral-named group already exists (merge-by-leaving to avoid unique-name collisions). Runs once at startup/migrate; idempotent. Never touches user-named groups.
- Data safety: NO deletes; keep the `origin` DB column + its values (internal provenance, already invisible to UI/export).

### 2b - Remove legacy UI on LibraryPage (frontend)
- Delete the dead "Legacy archive import" card (`LibraryPage.tsx:3306-3583`) + its now-dead handlers/state/types (`analyzeLegacyArchiveRoot`, `import4kvdp*`, `importExistingDownloadsFromLegacyRoot`, `openLegacyAnalysisReport`, `chooseLegacyArchiveRoot`, `legacyArchive*` state 853-875 + persist effects; types `LegacyArchive*`). Keep the shared localStorage keys writable by Options (do not rename keys).
- Remove the two live duplicate buttons in the `<details>` block (`4256-4277`): "Import 4KVDP exports", "Import existing downloads". Keep Export JSON / Import JSON / Scan folder + seed continuity.
- Fix filter label `LibraryPage.tsx:5130` `Single videos / legacy` -> `Single videos`.

### 2c - Relabel the Options importer as "Import from 4K Video Downloader" (frontend: `OptionsPage.tsx`)
- Rename per the audit table: h2 `Advanced Recovery` -> `Import from 4K Video Downloader`; the description; `Legacy archive root` -> `4K Video Downloader library folder`; `4KVDP app/state folder` -> `4K Video Downloader app folder (optional)`; buttons `Import 4KVDP app state` -> `Import subscriptions & state`, `Import 4KVDP export` -> `Import exported subscriptions`; dialog titles + error strings (`469,499,512,520,535,547`).
- Keep the Tauri command names + localStorage keys unchanged (renaming breaks the invoke contract / orphans saved paths).

### 2d - Export neutralization
- No separate export-code change required: once 2a renames the groups, both the subscription export (`subscriptions.rs:1398`) and the library bundle resolve `group_ids` to neutral names, revealing nothing legacy-specific. Library items already clean (finding #2). (Do NOT strip `group_ids` from the export — that would drop the grouping the operator likes on round-trip.)

Out of scope: renaming internal Rust/React code identifiers + Tauri command names + localStorage keys (invisible to users; renaming risks orphaning data/contracts — audit finding (ii)); the unrelated non-4KVDP "legacy" strings (audio-mix fallback, python lockfile, diagnostics-trace compat, subtitle anchor remap); `App.tsx:2376` "Legacy global auto-processing defaults" (localization batch wording, not 4KVDP — optional, defer).

## Acceptance Criteria

- No "legacy" or "4KVDP"/"4K Video Downloader+"-as-jargon wording renders in the app UI outside the single Options "Import from 4K Video Downloader" section; that section reads in plain language.
- Existing operator groups + memberships + subscriptions + library rows are intact (no deletes); the 3 auto-legacy groups render with neutral names after the idempotent rename.
- A fresh subscription/library export contains no legacy-vs-new distinction.
- `cargo test -p voxvulgi_engine` green; FE `tsc` clean; contract tests unaffected. NOT built (operator builds next).

## Red-Team

- In-place group rename is a data edit: mitigate by exact-name match to the 3 app-created constants only, rename-not-delete, membership-preserving, idempotent, skip-on-collision; document prominently for operator review; easy to revert (rename back).
- Removing LibraryPage legacy handlers could break a shared localStorage write: keys are shared with Options which retains them; removing LibraryPage's duplicate writes is safe (Options owns the feature).
- Deleting the dead card could remove a still-referenced symbol: it's `{false &&}`-gated (never renders); verify no other live reference before deleting each handler.
- Neutral group name "Imported subscriptions" could collide with an existing user group of that name: skip-on-collision in the normalization; get-or-create-by-name for new imports.

## Notes

- 2026-07-01: authored from the legacy audit as part of the operator's overnight autonomous UX overhaul. Implementation batched with WP-0260/0261 to minimize passes over the large files; validated with tsc + cargo, NOT built.
