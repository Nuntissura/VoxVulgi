# Work Packet: WP-0133 - Options-managed per-feature storage roots

## Metadata
- ID: WP-0133
- Owner: Codex
- Status: DONE
- Created: 2026-03-09
- Target milestone: Installer smoke remediation tranche

## Intent

- What: Move durable root-path ownership fully into Options and let operators configure distinct persistent roots per feature/export class while keeping all feature panes in sync.
- Why: Installer smoke confirms that the shared-root card is still duplicated in feature panes, per-pane folder overrides can drift from the displayed root, and the current model is too coarse for real operator libraries.

## Scope

In scope:

- Persistent roots for Video Archiver, Instagram Archiver, Image Archive, Media Library imports where needed, and Localization Studio exports.
- Options as the single durable configuration surface for these roots.
- Feature panes showing resolved effective paths only, without owning the configuration card.
- Migration from the current single shared root without destructive moves.

Out of scope:

- Moving or rewriting existing operator media on disk.

## Acceptance criteria

- Options is the only place where durable feature roots are configured.
- Feature panes display effective paths but no longer duplicate the configuration card.
- Persistent roots remain in sync across restart, pane switches, and updates.

## Test / verification plan

- Focused desktop build and Tauri tests around config read/write and path hydration.
- App-boundary verification across pane switches and restart.
- Proof bundle with resolved-path examples per feature.

## Status updates

- 2026-03-09: Created from installer smoke findings `ST-003`, `ST-007`, and `ST-009`.
- 2026-03-09: Implemented per-feature root overrides for Video Archiver, Instagram Archiver, Image Archive, and Localization Studio exports, all managed from Options with the old base root retained as migration-safe fallback.
- 2026-03-09: Verified with `cargo test -q --manifest-path product\engine\Cargo.toml`, `cargo test -q --manifest-path product\desktop\src-tauri\Cargo.toml`, and `npm -C product\desktop run build`; proof in `product/desktop/Build Target/tool_artifacts/wp_runs/WP-0133/20260309_055158/`.
