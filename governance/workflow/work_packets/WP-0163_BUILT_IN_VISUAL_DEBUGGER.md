# Work Packet: WP-0163 - Built-in Visual Debugger for Agents

## Metadata
- ID: WP-0163
- Owner: Codex
- Status: BACKLOG
- Created: 2026-04-08
- Target milestone: Tooling

## Intent

- What: Implements a deterministic application-layer visual snapshot tool that can capture the active worksurface to disk.
- Why: AI Orchestrators/Coders need visual context (e.g., testing alignment, checking state) without manual operator copy-pasting, accelerating UI debugging sequences.

## Scope

In scope:
- Add a Tauri command (e.g. `admin_capture_surface`) that leverages frontend canvas export or Tauri window APIs to save an image.
- Target path defaults to `governance/snapshots/`.
- Document usage in `PROJECT_CODEX.md` and `AGENTS.md`.

## Acceptance criteria
- An agent invoking the snapshot tool can retrieve an image file and use it as multimodal context for evaluation.

## Test / verification plan
- Verify agent-invoked shell access to the command outputs an actual `.png` or `.jpg`.

## Status updates

- 2026-04-08: Created from operator evaluation session.
