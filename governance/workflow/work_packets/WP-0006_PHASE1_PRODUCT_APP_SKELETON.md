# Work Packet: WP-0006 — Phase 1: Product app skeleton

## Metadata
- ID: WP-0006
- Owner: Codex
- Status: DONE
- Created: 2026-02-19
- Target milestone: Phase 1 (MVP)

## Intent

- What: Create the initial cross-platform desktop app skeleton under `product/`.
- Why: Unblocks Phase 1 feature work (library, jobs, captions, translate, diagnostics) with a runnable baseline.

## Scope

In scope:

- Create an app skeleton that:
  - builds and runs locally (Windows baseline; macOS next)
  - has basic navigation (Library / Jobs / Diagnostics placeholders)
  - has a minimal Rust core "engine" crate/module boundary for the job runner + DB access
- Establish baseline configuration and build scripts (dev + release).

Out of scope:

- Implementing downloader providers.
- Implementing ASR/translation/dubbing logic (tracked in other WPs).

## Acceptance criteria

- `product/` contains a runnable desktop app skeleton.
- App starts and shows placeholder screens for Library, Jobs, Diagnostics.
- A minimal engine boundary exists (even if stubs) to avoid UI-thread coupling.

## Implementation notes

- Recommended stack: Tauri + Rust core + React UI (per technical design), unless Phase 0 decisions change it.

## Test / verification plan

- Build + run the desktop app locally.

## Risks / open questions

- Final stack is not locked (Qt/Electron alternatives exist); confirm before heavy implementation.

## Status updates

- 2026-02-19: Implemented Tauri v2 + React desktop skeleton in `product/desktop/`, added `product/engine/` boundary, and verified build via `npm run tauri build`.
