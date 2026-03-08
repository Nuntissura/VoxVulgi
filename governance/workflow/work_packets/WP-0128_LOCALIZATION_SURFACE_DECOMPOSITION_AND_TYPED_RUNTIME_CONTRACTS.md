# Work Packet: WP-0128 - Localization surface decomposition and typed runtime contracts

## Metadata
- ID: WP-0128
- Owner: Codex
- Status: BACKLOG
- Created: 2026-03-08
- Target milestone: Audit remediation tranche

## Intent

- What: Break down oversized localization/runtime modules and replace brittle stringly-typed bridge contracts with explicit typed runtime metadata.
- Why: `WP-0119` found monolithic page/runtime modules, inline Python payload debt, and reverse-engineered artifact identity handling.

## Scope

In scope:

- `SubtitleEditorPage.tsx` decomposition into domain panels/hooks.
- Tauri bridge domain split and handler composition cleanup.
- Typed artifact/backend identity metadata from Rust to UI.
- Externalization of large inline script/template payloads where they are part of runtime contracts.

Out of scope:

- Every future feature addition in Localization Studio.

## Acceptance criteria

- Major localization/operator surfaces are decomposed into smaller reviewable modules.
- Artifact/backend identity is passed as typed metadata instead of reconstructed from filenames alone.
- Large inline runtime payloads are moved into maintainable assets/templates where appropriate.

## Test / verification plan

- Desktop build/test plus focused engine/bridge tests.
- Proof bundle with structural slices and contract updates.

## Status updates

- 2026-03-08: Created from `WP-0119` architecture and contract findings.
