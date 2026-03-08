# Work Packet: WP-0111 - Voice reference curation and bundle promotion

## Metadata
- ID: WP-0111
- Owner: Codex
- Status: BACKLOG
- Created: 2026-03-08
- Target milestone: Voice backend modernization

## Intent

- What: Add a reference-curation lab that scores current speaker reference clips, recommends stronger bundle order/compact sets, and lets operators promote the curated bundle into live speaker settings without manual guesswork.
- Why: The research shows that reference quality and bundle composition matter as much as backend choice. VoxVulgi already stores multiple references, but it does not yet help the operator decide which clips to trust most.

## Scope

In scope:

- Generate per-speaker reference-curation reports from existing reference clips.
- Rank references using local QC, duration, level, silence, clipping, noise, and internal consistency heuristics.
- Recommend a primary reference and a compact multi-reference bundle.
- Let Localization Studio apply the curated order or compact bundle to the active item speaker non-destructively and explicitly.

Out of scope:

- Auto-deleting weak references.
- Silent rewriting of saved library/template/profile references outside the operator's explicit action.

## Acceptance criteria

- Operators can generate and reload a reference-curation report for an item speaker.
- The report explains why references were ranked the way they were.
- Operators can promote the ranked order or recommended compact set into the current item speaker settings from the UI.
- The workflow preserves explicit operator control and does not delete source references.

## Test / verification plan

- Rust tests for scoring/order application.
- Desktop build.
- Tauri/engine tests for report generation and apply actions.

## Status updates

- 2026-03-08: Created from the voice-cloning research modernization tranche.
